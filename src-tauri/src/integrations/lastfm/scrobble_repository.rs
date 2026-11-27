//! Last.fm scrobbling repository.
//!
//! Ported from `lib/features/lastfm/scrobble_repository.dart`:
//! - `track.updateNowPlaying` (deduped) + `track.scrobble`
//! - `track.love` / `track.unlove`
//! - write cooldown + offline queue (max 200) flushed on next scrobble

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::extractor::auth_crypto::AuthCrypto;
use crate::integrations::lastfm::lastfm_api::LastFmApiService;
use crate::integrations::scrobblers::extra_scrobblers::{ExtraScrobblers, Listen};
use crate::storage::prefs::Prefs;

/// Scrobble result classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScrobbleOutcome {
    Success,
    Queued,
    NoSession,
    Failed,
}

#[derive(Debug, Clone)]
struct QueuedScrobble {
    artist: String,
    track: String,
    album: String,
    timestamp: String,
}

struct ScrobbleState {
    last_now_playing_key: String,
    last_write: Option<Instant>,
    offline_queue: VecDeque<QueuedScrobble>,
}

impl Default for ScrobbleState {
    fn default() -> Self {
        Self {
            last_now_playing_key: String::new(),
            last_write: None,
            offline_queue: VecDeque::new(),
        }
    }
}

pub struct ScrobbleRepository {
    api: Arc<LastFmApiService>,
    prefs: Arc<Prefs>,
    extras: Option<Arc<ExtraScrobblers>>,
    state: Mutex<ScrobbleState>,
}

impl ScrobbleRepository {
    pub fn new(
        api: Arc<LastFmApiService>,
        prefs: Arc<Prefs>,
        extras: Option<Arc<ExtraScrobblers>>,
    ) -> Self {
        Self {
            api,
            prefs,
            extras,
            state: Mutex::new(ScrobbleState::default()),
        }
    }

    fn can_write(&self) -> bool {
        self.prefs.is_last_fm_configured() && !self.prefs.session_key().is_empty()
    }

    fn signed(&self, method: &str, extra: &[(&str, &str)]) -> BTreeMap<String, String> {
        let mut params = BTreeMap::new();
        params.insert("method".to_string(), method.to_string());
        params.insert("api_key".to_string(), self.prefs.last_fm_api_key());
        params.insert("sk".to_string(), self.prefs.session_key());
        for &(k, v) in extra {
            if !v.is_empty() {
                params.insert(k.to_string(), v.to_string());
            }
        }
        let sig = AuthCrypto::api_sig(
            params.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            &self.prefs.last_fm_api_secret(),
        );
        params.insert("api_sig".to_string(), sig);
        params.insert("format".to_string(), "json".to_string());
        params
    }

    async fn pace_writes(&self) {
        let wait = {
            let mut st = self.state.lock().await;
            let now = Instant::now();
            let wait = st
                .last_write
                .and_then(|last| Duration::from_millis(700).checked_sub(now.duration_since(last)));
            if wait.is_none() {
                st.last_write = Some(now);
            }
            wait
        };
        if let Some(d) = wait {
            tokio::time::sleep(d).await;
            let mut st = self.state.lock().await;
            st.last_write = Some(Instant::now());
        }
    }

    pub async fn update_now_playing(
        &self,
        artist: &str,
        track: &str,
        album: &str,
    ) -> ScrobbleOutcome {
        if self.prefs.scrobble_now_playing() {
            if let Some(ref ex) = self.extras {
                ex.now_playing(Listen::new(artist, track, album, 0));
            }
        }
        if !self.prefs.scrobbler_enabled()
            || !self.prefs.scrobble_now_playing()
            || !self.can_write()
        {
            return ScrobbleOutcome::NoSession;
        }
        let key = format!("{}|{}", artist.to_lowercase(), track.to_lowercase());
        {
            let mut st = self.state.lock().await;
            if key == st.last_now_playing_key {
                return ScrobbleOutcome::Success;
            }
            st.last_now_playing_key = key;
        }

        self.pace_writes().await;
        let fields = self.signed(
            "track.updateNowPlaying",
            &[("artist", artist), ("track", track), ("album", album)],
        );
        match self.api.post(&fields).await {
            Ok(json) if json.get("error").is_none() => ScrobbleOutcome::Success,
            _ => ScrobbleOutcome::Failed,
        }
    }

    /// Scrobble a fully-played track. `timestamp_sec` = playback start (UTC).
    pub async fn scrobble(
        &self,
        artist: &str,
        track: &str,
        album: &str,
        timestamp_sec: i64,
    ) -> ScrobbleOutcome {
        if let Some(ref ex) = self.extras {
            ex.submit(Listen::new(artist, track, album, timestamp_sec));
        }
        if !self.prefs.scrobbler_enabled() {
            return ScrobbleOutcome::NoSession;
        }
        let ts_str = timestamp_sec.to_string();
        let entry = QueuedScrobble {
            artist: artist.to_string(),
            track: track.to_string(),
            album: album.to_string(),
            timestamp: ts_str.clone(),
        };
        if !self.can_write() {
            self.enqueue(entry).await;
            return ScrobbleOutcome::Queued;
        }
        self.flush_queue().await;

        self.pace_writes().await;
        let fields = self.signed(
            "track.scrobble",
            &[
                ("artist[0]", artist),
                ("track[0]", track),
                ("timestamp[0]", &ts_str),
                ("album[0]", album),
            ],
        );
        match self.api.post(&fields).await {
            Ok(json) => {
                if let Some(err) = json.get("error") {
                    let code = err.as_i64().map(|n| n as i32);
                    if matches!(code, Some(29 | 11 | 16)) {
                        self.enqueue(entry).await;
                        return ScrobbleOutcome::Queued;
                    }
                    return ScrobbleOutcome::Failed;
                }
                ScrobbleOutcome::Success
            }
            Err(_) => {
                self.enqueue(entry).await;
                ScrobbleOutcome::Queued
            }
        }
    }

    pub async fn set_loved(&self, artist: &str, track: &str, loved: bool) -> bool {
        if let Some(ref ex) = self.extras {
            ex.love_track(artist, track, loved);
        }
        if !self.can_write() {
            return self
                .extras
                .as_ref()
                .map(|ex| ex.services().iter().any(|s| s.connected()))
                .unwrap_or(false);
        }
        self.pace_writes().await;
        let method = if loved { "track.love" } else { "track.unlove" };
        let fields = self.signed(method, &[("artist", artist), ("track", track)]);
        match self.api.post(&fields).await {
            Ok(Value::Object(m)) => !m.contains_key("error"),
            _ => false,
        }
    }

    async fn enqueue(&self, entry: QueuedScrobble) {
        let mut st = self.state.lock().await;
        if st.offline_queue.len() >= 200 {
            st.offline_queue.pop_front();
        }
        st.offline_queue.push_back(entry);
    }

    async fn flush_queue(&self) {
        if !self.can_write() {
            return;
        }
        let pending: Vec<QueuedScrobble> = {
            let mut st = self.state.lock().await;
            if st.offline_queue.is_empty() {
                return;
            }
            st.offline_queue.drain(..).collect()
        };

        for e in pending {
            self.pace_writes().await;
            let fields = self.signed(
                "track.scrobble",
                &[
                    ("artist[0]", &e.artist),
                    ("track[0]", &e.track),
                    ("timestamp[0]", &e.timestamp),
                    ("album[0]", &e.album),
                ],
            );
            if self.api.post(&fields).await.is_err() {
                self.enqueue(e).await;
                break;
            }
        }
    }

    pub async fn queued_count(&self) -> usize {
        self.state.lock().await.offline_queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::lastfm::rate_guard::LastFmRateGuard;

    #[tokio::test]
    async fn queues_when_offline_or_no_session_and_caps_at_200() {
        let prefs = Arc::new(Prefs::in_memory());
        prefs.set_scrobbler(Some(true), None, None);
        let api = Arc::new(LastFmApiService::new(Arc::new(LastFmRateGuard::new())));
        let repo = ScrobbleRepository::new(api, prefs, None);

        for i in 0..205 {
            let out = repo
                .scrobble("Artist", &format!("Track {i}"), "Album", 1_700_000_000 + i)
                .await;
            assert_eq!(out, ScrobbleOutcome::Queued);
        }
        assert_eq!(repo.queued_count().await, 200);
    }
}
