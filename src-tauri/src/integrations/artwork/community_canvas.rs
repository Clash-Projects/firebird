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
    canvas_artists_match, canvas_get_json, canvas_titles_match, normalize_canvas_match,
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
        let want_title = normalize_canvas_match(title);
        let want_artist = normalize_canvas_match(artist);
        let want_album = album.map(normalize_canvas_match);
        if want_title.is_empty() || want_artist.is_empty() {
            return None;
        }

        // Pass 1: match song title + artist + album (when album is provided).
        for entry in index {
            let listed = normalize_canvas_match(&entry.album);
            let title_ok = canvas_titles_match(title, &entry.song);
            let artist_ok =
                canvas_artists_match(artist, &entry.artist, Some(title), Some(&entry.song));
            let album_ok = listed.is_empty()
                || want_album.as_deref().map(str::is_empty).unwrap_or(true)
                || album
                    .map(|a| canvas_titles_match(a, &entry.album))
                    .unwrap_or(false);
            if title_ok && artist_ok && album_ok {
                return Some(AnimatedArtwork {
                    url: entry.url.clone(),
                    url_tall: String::new(),
                    fallback_url: None,
                    title: Some(entry.song.clone()),
                    artist: Some(entry.artist.clone()),
                    album: if entry.album.trim().is_empty() {
                        None
                    } else {
                        Some(entry.album.clone())
                    },
                    source: CanvasSource::Community,
                });
            }
        }

        // Pass 2: if noisy album metadata rejected a valid song+artist match.
        if want_album.as_deref().map(|s| !s.is_empty()).unwrap_or(false) {
            for entry in index {
                let title_ok = canvas_titles_match(title, &entry.song);
                let artist_ok =
                    canvas_artists_match(artist, &entry.artist, Some(title), Some(&entry.song));
                if title_ok && artist_ok {
                    return Some(AnimatedArtwork {
                        url: entry.url.clone(),
                        url_tall: String::new(),
                        fallback_url: None,
                        title: Some(entry.song.clone()),
                        artist: Some(entry.artist.clone()),
                        album: if entry.album.trim().is_empty() {
                            None
                        } else {
                            Some(entry.album.clone())
                        },
                        source: CanvasSource::Community,
                    });
                }
            }
        }

        // Pass 3: album-level fallback.
        if let Some(alb) = album.filter(|a| !normalize_canvas_match(a).is_empty()) {
            for entry in index {
                if entry.album.trim().is_empty() {
                    continue;
                }
                let album_ok = canvas_titles_match(alb, &entry.album);
                let artist_ok =
                    canvas_artists_match(artist, &entry.artist, Some(title), Some(&entry.song));
                if album_ok && artist_ok {
                    return Some(AnimatedArtwork {
                        url: entry.url.clone(),
                        url_tall: String::new(),
                        fallback_url: None,
                        title: None,
                        artist: Some(entry.artist.clone()),
                        album: Some(entry.album.clone()),
                        source: CanvasSource::Community,
                    });
                }
            }
        }

        None
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
    fn matches_passes_and_album_fallback() {
        let index = vec![CommunityEntry {
            song: "Blinding Lights".to_string(),
            artist: "The Weeknd".to_string(),
            album: "After Hours".to_string(),
            url: "https://example.com/after_hours.mp4".to_string(),
        }];

        // Pass 1: exact
        let m1 = CommunityCanvas::match_from_index(
            &index,
            "Blinding Lights",
            "The Weeknd",
            Some("After Hours"),
        );
        assert!(m1.is_some());
        assert_eq!(m1.unwrap().source, CanvasSource::Community);

        // Pass 3: album-level fallback
        let m3 = CommunityCanvas::match_from_index(
            &index,
            "Save Your Tears",
            "The Weeknd",
            Some("After Hours"),
        );
        assert!(m3.is_some());
        assert_eq!(m3.unwrap().title, None);
    }
}
