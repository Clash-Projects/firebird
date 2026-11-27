//! A community-curated `song + artist -> looping video` index (ViMusic manifest).
//!
//! Port of `lib/core/artwork/community_canvas.dart`.
//! Covers back catalog and independent tracks that major streaming services
//! lack. Held in memory for `TTL_MS` to avoid re-fetching per track.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::sync::Mutex;

use super::{
    canvas_get_json, confirmed_album_match, confirmed_artist_match, confirmed_track_match,
    AnimatedArtwork, CanvasSource, CANVAS_USER_AGENT,
};

const MANIFEST_URL: &str = "https://vivimusicanvas.mkmdevilmi.workers.dev/canvas.json";
const TTL_MS: i64 = 30 * 60 * 1000;

#[derive(Debug, Clone)]
pub struct CommunityEntry {
    pub song: String,
    pub artist: String,
    pub album: String,
    pub url: String,
}

#[derive(Default)]
struct ManifestState {
    entries: Vec<CommunityEntry>,
    fetched_at_ms: i64,
}

static MANIFEST_STATE: LazyLock<Mutex<ManifestState>> =
    LazyLock::new(|| Mutex::new(ManifestState::default()));
static MANIFEST_INFLIGHT: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub struct CommunityCanvas;

impl CommunityCanvas {
    /// Clears the cached manifest (used by unit tests).
    pub async fn reset_for_testing() {
        let mut guard = MANIFEST_STATE.lock().await;
        *guard = ManifestState::default();
    }

    pub async fn search(
        client: &reqwest::Client,
        title: &str,
        artist: &str,
        album: Option<&str>,
    ) -> Option<AnimatedArtwork> {
        let index = Self::manifest(client).await;
        if index.is_empty() {
            return None;
        }
        Self::match_from_index(&index, title, artist, album)
    }

    /// Synchronous check against the already-loaded manifest (0 network calls).
    pub async fn peek_match(
        title: &str,
        artist: &str,
        album: Option<&str>,
    ) -> Option<AnimatedArtwork> {
        let guard = MANIFEST_STATE.lock().await;
        if guard.entries.is_empty() {
            return None;
        }
        Self::match_from_index(&guard.entries, title, artist, album)
    }

    pub fn match_from_index(
        index: &[CommunityEntry],
        title: &str,
        artist: &str,
        album: Option<&str>,
    ) -> Option<AnimatedArtwork> {
        index.iter().find_map(|entry| {
            if !confirmed_track_match(title, &entry.song)
                || !confirmed_artist_match(artist, &entry.artist)
                || album.is_some_and(|album| !confirmed_album_match(album, &entry.album))
            {
                return None;
            }
            Some(AnimatedArtwork {
                url: entry.url.clone(),
                url_tall: String::new(),
                fallback_url: None,
                title: Some(entry.song.clone()),
                artist: Some(entry.artist.clone()),
                album: (!entry.album.trim().is_empty()).then(|| entry.album.clone()),
                source: CanvasSource::Community,
            })
        })
    }

    async fn manifest(client: &reqwest::Client) -> Vec<CommunityEntry> {
        let now = now_ms();
        {
            let guard = MANIFEST_STATE.lock().await;
            if !guard.entries.is_empty() && now - guard.fetched_at_ms < TTL_MS {
                return guard.entries.clone();
            }
        }
        let _inflight = MANIFEST_INFLIGHT.lock().await;
        Self::fetch_manifest(client).await
    }

    async fn fetch_manifest(client: &reqwest::Client) -> Vec<CommunityEntry> {
        let now = now_ms();
        {
            let guard = MANIFEST_STATE.lock().await;
            if !guard.entries.is_empty() && now - guard.fetched_at_ms < TTL_MS {
                return guard.entries.clone();
            }
        }

        let mut headers = HashMap::new();
        headers.insert("User-Agent".to_string(), CANVAS_USER_AGENT.to_string());
        let root = canvas_get_json(client, MANIFEST_URL, &headers, &[]).await;

        let mut parsed = Vec::new();
        if let Some(map) = root {
            if let Some(items) = map.get("items").and_then(Value::as_array) {
                for item in items {
                    let Some(obj) = item.as_object() else {
                        continue;
                    };
                    let song = obj
                        .get("song")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or("");
                    let artist = obj
                        .get("artist")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or("");
                    let album = obj
                        .get("album")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or("");
                    let url = obj
                        .get("url")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or("");
                    if song.is_empty() || artist.is_empty() || url.is_empty() {
                        continue;
                    }
                    parsed.push(CommunityEntry {
                        song: song.to_string(),
                        artist: artist.to_string(),
                        album: album.to_string(),
                        url: url.to_string(),
                    });
                }
            }
        }

        let mut guard = MANIFEST_STATE.lock().await;
        if !parsed.is_empty() {
            guard.entries = parsed;
        }
        guard.fetched_at_ms = now;
        guard.entries.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_the_requested_song_and_release() {
        let index = vec![CommunityEntry {
            song: "Blinding Lights".to_string(),
            artist: "The Weeknd".to_string(),
            album: "After Hours".to_string(),
            url: "https://example.com/after_hours.mp4".to_string(),
        }];

        let matched = CommunityCanvas::match_from_index(
            &index,
            "Blinding Lights",
            "The Weeknd",
            Some("After Hours"),
        );
        assert_eq!(matched.unwrap().source, CanvasSource::Community);

        let other_song = CommunityCanvas::match_from_index(
            &index,
            "Save Your Tears",
            "The Weeknd",
            Some("After Hours"),
        );
        assert!(other_song.is_none());
        assert!(CommunityCanvas::match_from_index(
            &index,
            "Blinding Lights",
            "The Weeknd",
            Some("After Hours (Deluxe)"),
        ).is_none());
    }
}
