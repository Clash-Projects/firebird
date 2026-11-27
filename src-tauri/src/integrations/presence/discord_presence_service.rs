use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::models::player_state::PlayerSnapshot;
use crate::models::stream_models::PlayableTrack;
use crate::storage::prefs::Prefs;

use super::discord_ipc::DiscordIpc;

/// Application ID from the Discord Developer Portal (public by design).
pub const DISCORD_APPLICATION_ID: &str = "1552563991426105414";

/// Brand mark for the card's small badge (and the fallback large image).
pub const DISCORD_LOGO_ASSET: &str =
    "https://raw.githubusercontent.com/Clash-Projects/LastWave-Desktop/main/lastwave-logo.png";

const REPO_URL: &str = "https://github.com/Clash-Projects/LastWave-Desktop";
const MAX_FIELD: usize = 120;
const PUSH_INTERVAL: Duration = Duration::from_secs(15);
const CLEAR_DELAY: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Default)]
struct InnerState {
    ipc: Option<DiscordIpc>,
    disposed: bool,
    connecting: bool,
    last_attempt: Option<Instant>,
    last_key: String,
    last_push: Option<Instant>,
    last_snap: Option<PlayerSnapshot>,
    clear_task: Option<JoinHandle<()>>,
    heartbeat_task: Option<JoinHandle<()>>,
}

/// Discord Rich Presence ("Listening to LastWave") over local IPC.
///
/// Pushes on track/play-state change (throttled), silent when Discord is closed,
/// pipe missing, or rate-limited. Clears activity after a short grace period
/// when playback is not running so elapsed timestamps never linger on Discord.
pub struct DiscordPresenceService {
    prefs: Arc<Prefs>,
    state: Mutex<InnerState>,
}

impl DiscordPresenceService {
    pub fn new(prefs: Arc<Prefs>) -> Arc<Self> {
        Arc::new(Self {
            prefs,
            state: Mutex::new(InnerState::default()),
        })
    }

    /// Starts the 15-second self-healing heartbeat loop using the provided
    /// snapshot reader.
    pub async fn start_heartbeat<F>(self: &Arc<Self>, read_snapshot: F)
    where
        F: Fn() -> Option<PlayerSnapshot> + Send + Sync + 'static,
    {
        let reader: Arc<dyn Fn() -> Option<PlayerSnapshot> + Send + Sync> =
            Arc::new(read_snapshot);
        let svc = Arc::clone(self);
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(PUSH_INTERVAL);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if svc.is_disposed().await {
                    break;
                }
                if let Some(snap) = reader() {
                    svc.on_snapshot(snap).await;
                }
            }
        });
        let mut st = self.state.lock().await;
        if st.disposed {
            handle.abort();
            return;
        }
        if let Some(prev) = st.heartbeat_task.replace(handle) {
            prev.abort();
        }
    }

    pub async fn is_disposed(&self) -> bool {
        self.state.lock().await.disposed
    }

    fn configured() -> bool {
        !DISCORD_APPLICATION_ID.is_empty()
            && DISCORD_APPLICATION_ID != "YOUR_DISCORD_APPLICATION_ID"
    }

    fn enabled(&self) -> bool {
        self.prefs.discord_rich_presence()
    }

    pub async fn on_snapshot(self: &Arc<Self>, snap: PlayerSnapshot) {
        {
            let mut st = self.state.lock().await;
            if st.disposed {
                return;
            }
            st.last_snap = Some(snap);
        }
        self.evaluate().await;
    }

    async fn evaluate(self: &Arc<Self>) {
        let snap = {
            let st = self.state.lock().await;
            if st.disposed {
                return;
            }
            st.last_snap.clone()
        };

        if !self.enabled() || !Self::configured() || !DiscordIpc::is_supported() {
            self.clear_quiet().await;
            return;
        }

        let Some(snap) = snap else {
            return;
        };
        let Some(track) = snap.current.as_ref() else {
            self.clear_quiet().await;
            return;
        };
        if track.title.trim().is_empty() {
            self.clear_quiet().await;
            return;
        }

        if !snap.is_playing {
            self.schedule_clear().await;
            return;
        }

        let key = format!(
            "{}|playing={}|dur={}|br={}",
            track.queue_key(),
            snap.is_playing,
            snap.duration().as_secs(),
            snap.bitrate_kbps
        );
        let now = Instant::now();

        {
            let mut st = self.state.lock().await;
            if let Some(task) = st.clear_task.take() {
                task.abort();
            }
            if key == st.last_key
                && st
                    .last_push
                    .is_some_and(|last| now.duration_since(last) < PUSH_INTERVAL)
            {
                return;
            }
        }

        if !self.ensure_connected().await {
            return;
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let activity = build_activity_payload(track, &snap, now_ms);

        let mut st = self.state.lock().await;
        if st.disposed {
            return;
        }
        let Some(ipc) = st.ipc.as_mut() else {
            return;
        };
        if !ipc.set_activity(activity).await {
            ipc.close();
            st.ipc = None;
            st.last_attempt = Some(Instant::now());
            return;
        }
        st.last_key = key;
        st.last_push = Some(now);
    }

    async fn ensure_connected(&self) -> bool {
        {
            let mut st = self.state.lock().await;
            if st.disposed {
                return false;
            }
            if st.ipc.as_ref().is_some_and(DiscordIpc::is_open) {
                return true;
            }
            if st.connecting {
                return false;
            }
            if st.ipc.is_some()
                && st
                    .last_attempt
                    .is_some_and(|last| Instant::now().duration_since(last) < RETRY_INTERVAL)
            {
                return false;
            }
            st.connecting = true;
            st.last_attempt = Some(Instant::now());
            if let Some(mut old) = st.ipc.take() {
                old.close();
            }
        }

        let fresh = DiscordIpc::connect(DISCORD_APPLICATION_ID).await;

        let mut st = self.state.lock().await;
        st.connecting = false;
        if st.disposed {
            if let Some(mut ipc) = fresh {
                ipc.close();
            }
            return false;
        }
        match fresh {
            Some(ipc) => {
                st.ipc = Some(ipc);
                true
            }
            None => false,
        }
    }

    async fn schedule_clear(self: &Arc<Self>) {
        let mut st = self.state.lock().await;
        if st.disposed || st.clear_task.is_some() || st.last_key.is_empty() {
            return;
        }
        let svc = Arc::clone(self);
        st.clear_task = Some(tokio::spawn(async move {
            tokio::time::sleep(CLEAR_DELAY).await;
            {
                let mut inner = svc.state.lock().await;
                inner.clear_task = None;
            }
            svc.clear_quiet().await;
        }));
    }

    async fn clear_quiet(&self) {
        let mut st = self.state.lock().await;
        let had = !st.last_key.is_empty();
        st.last_key.clear();
        if !had {
            return;
        }
        if let Some(ipc) = st.ipc.as_mut() {
            let _ = ipc.clear_activity().await;
        }
    }

    /// Best-effort clear + release (app quit). Never panics.
    pub async fn shutdown(&self) {
        let (heartbeat, clear_task, had_key, mut ipc) = {
            let mut st = self.state.lock().await;
            if st.disposed {
                return;
            }
            st.disposed = true;
            let hb = st.heartbeat_task.take();
            let ct = st.clear_task.take();
            let had = !st.last_key.is_empty();
            st.last_key.clear();
            let ipc = st.ipc.take();
            (hb, ct, had, ipc)
        };

        if let Some(task) = heartbeat {
            task.abort();
        }
        if let Some(task) = clear_task {
            task.abort();
        }
        if let Some(ref mut client) = ipc {
            if had_key {
                let _ = client.clear_activity().await;
            }
            client.close();
        }
    }
}

/// Activity payload for `SET_ACTIVITY` (`type = 2`, listening).
pub fn build_activity_payload(
    track: &PlayableTrack,
    snap: &PlayerSnapshot,
    now_ms: i64,
) -> Value {
    let title = clip_field(&track.title);
    let artist = if track.artist.trim().is_empty() {
        if track.album.trim().is_empty() {
            "LastWave".to_string()
        } else {
            clip_field(&track.album)
        }
    } else {
        clip_field(&track.artist)
    };

    let mut obj = Map::new();
    obj.insert("type".to_string(), json!(2));
    obj.insert("name".to_string(), json!("LastWave"));
    obj.insert("details".to_string(), json!(title));
    obj.insert("state".to_string(), json!(artist));
    obj.insert("status_display_type".to_string(), json!(2));

    if snap.duration_ms > 0 {
        let pos_ms = snap.position_ms.min(snap.duration_ms) as i64;
        let dur_ms = snap.duration_ms as i64;
        let end_ms = now_ms + (dur_ms - pos_ms);
        let start_ms = end_ms - dur_ms;
        obj.insert(
            "timestamps".to_string(),
            json!({
                "start": start_ms / 1000,
                "end": end_ms / 1000,
            }),
        );
    }

    let quality = quality_line(snap);
    let art = track.artwork_url.trim();
    let art_is_url = art.starts_with("http://") || art.starts_with("https://");
    let large_image = if art_is_url { art } else { DISCORD_LOGO_ASSET };
    let large_text = if track.album.trim().is_empty() {
        title.clone()
    } else {
        clip_field(&track.album)
    };
    let small_text = clip_field(&if quality.is_empty() {
        "LastWave".to_string()
    } else {
        format!("LastWave · {quality}")
    });

    obj.insert(
        "assets".to_string(),
        json!({
            "large_image": large_image,
            "large_text": large_text,
            "small_image": DISCORD_LOGO_ASSET,
            "small_text": small_text,
        }),
    );

    let vid = track.video_id.trim();
    let listen_url = if vid.is_empty() {
        format!("{REPO_URL}/releases")
    } else {
        format!("https://www.youtube.com/watch?v={vid}")
    };
    obj.insert(
        "buttons".to_string(),
        json!([
            {
                "label": "Listen On LastWave",
                "url": listen_url,
            },
            {
                "label": "Get LastWave",
                "url": REPO_URL,
            }
        ]),
    );
    obj.insert("instance".to_string(), json!(true));

    Value::Object(obj)
}

/// `"Hi-Res Lossless · 24-bit · 96 kHz · Stereo"`. Only known parts are
/// included — never fabricated.
pub fn quality_line(snap: &PlayerSnapshot) -> String {
    let mut parts: Vec<String> = Vec::new();
    let stream = snap.stream.as_ref();
    let out = snap.output_format.as_ref();

    let depth = match out {
        Some(o) if o.bit_depth > 0 => o.bit_depth,
        _ => stream.map(|s| s.bit_depth).unwrap_or(0),
    };
    let rate_khz = match out {
        Some(o) if o.sample_rate_hz > 0 => o.sample_rate_hz as f64 / 1000.0,
        _ => stream.map(|s| s.sampling_rate_khz).unwrap_or(0.0),
    };

    if stream.is_some_and(|s| s.is_lossless) {
        parts.push(
            if rate_khz > 48.0 || depth > 16 {
                "Hi-Res Lossless"
            } else {
                "Lossless"
            }
            .to_string(),
        );
    }
    if depth > 0 {
        parts.push(format!("{depth}-bit"));
    }
    if rate_khz > 0.0 {
        parts.push(rate_label(rate_khz));
    }
    let ch = out.map(|o| o.channels).unwrap_or(0);
    if ch == 1 {
        parts.push("Mono".to_string());
    } else if ch == 2 {
        parts.push("Stereo".to_string());
    } else if ch > 2 {
        parts.push(format!("{ch}-ch"));
    }
    parts.join(" · ")
}

fn rate_label(khz: f64) -> String {
    if (khz - khz.round()).abs() < 0.05 {
        format!("{} kHz", khz.round() as i64)
    } else {
        format!("{khz:.1} kHz")
    }
}

fn clip_field(s: &str) -> String {
    let t = s.trim();
    if t.chars().count() > MAX_FIELD {
        let head: String = t.chars().take(MAX_FIELD - 1).collect();
        format!("{head}...")
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::pcm_format::PcmFormat;
    use crate::models::stream_models::ResolvedStream;

    #[test]
    fn activity_payload_and_quality_line() {
        let mut track = PlayableTrack::new("Everything In Its Right Place", "Radiohead");
        track.album = "Kid A".to_string();
        track.video_id = "NUnXHudSSww".to_string();
        track.artwork_url = "https://example.com/cover.jpg".to_string();

        let snap = PlayerSnapshot {
            current: Some(track.clone()),
            is_playing: true,
            position_ms: 30_000,
            duration_ms: 251_000,
            stream: Some(ResolvedStream {
                is_lossless: true,
                bit_depth: 24,
                sampling_rate_khz: 96.0,
                ..Default::default()
            }),
            output_format: Some(PcmFormat::new(96000, 24, 2)),
            ..Default::default()
        };

        assert_eq!(
            quality_line(&snap),
            "Hi-Res Lossless · 24-bit · 96 kHz · Stereo"
        );
        let payload = build_activity_payload(&track, &snap, 1_700_000_000_000);
        assert_eq!(payload["type"], 2);
        assert_eq!(payload["details"], "Everything In Its Right Place");
        assert_eq!(payload["state"], "Radiohead");
        assert_eq!(payload["assets"]["large_image"], "https://example.com/cover.jpg");
        assert_eq!(
            payload["buttons"][0]["url"],
            "https://www.youtube.com/watch?v=NUnXHudSSww"
        );
        assert_eq!(payload["timestamps"]["start"], 1_700_000_000 - 30);
        assert_eq!(payload["timestamps"]["end"], 1_700_000_000 + 221);
    }

    #[tokio::test]
    async fn shutdown_marks_disposed_cleanly() {
        let prefs = Arc::new(Prefs::in_memory());
        let svc = DiscordPresenceService::new(prefs);
        assert!(!svc.is_disposed().await);
        svc.shutdown().await;
        assert!(svc.is_disposed().await);
        // Second shutdown is idempotent.
        svc.shutdown().await;
        assert!(svc.is_disposed().await);
    }
}
