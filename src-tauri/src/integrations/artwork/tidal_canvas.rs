//! Tidal's video cover — square 1280x1280 looping clips.
//!
//! Port of `lib/core/artwork/tidal_canvas.dart`.
//! Read through the public search endpoint Tidal's embeddable player uses
//! (no account needed). The track search resolves the album object, which
//! carries a `videoCover` UUID expandable into a fixed URL on Tidal's CDN.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::{
    canvas_artists_match, canvas_get_json, canvas_titles_match, AnimatedArtwork, CanvasSource,
    CANVAS_USER_AGENT,
};

const SEARCH_URL: &str = "https://api.tidal.com/v1/search";
const EMBED_TOKEN: &str = "vNVdglQOjFJJGG2U";

static COUNTRY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z]{2}$").unwrap());

pub struct TidalCanvas;

impl TidalCanvas {
    pub fn country_code() -> String {
        if let Ok(locale) = std::env::var("LANG").or_else(|_| std::env::var("LC_ALL")) {
            let parts: Vec<&str> = locale.split(['-', '_', '.']).collect();
            if parts.len() >= 2 {
                let country = parts[1].trim().to_uppercase();
                if country.len() == 2 && COUNTRY_RE.is_match(&country) {
                    return country;
                }
            }
        }
        "US".to_string()
    }

    pub async fn search(
        client: &reqwest::Client,
        title: &str,
        artist: &str,
        album: Option<&str>,
    ) -> Option<AnimatedArtwork> {
        let query = match album.filter(|a| !a.trim().is_empty()) {
            Some(alb) => format!("{alb} {artist} {title}"),
            None => format!("{artist} {title}"),
        };
        let country = Self::country_code();
        let mut headers = HashMap::new();
        headers.insert("X-Tidal-Token".to_string(), EMBED_TOKEN.to_string());
        headers.insert("User-Agent".to_string(), CANVAS_USER_AGENT.to_string());

        let root = canvas_get_json(
            client,
            SEARCH_URL,
            &headers,
            &[
                ("query", &query),
                ("limit", "10"),
                ("types", "TRACKS"),
                ("countryCode", &country),
            ],
        )
        .await?;

        let items = root
            .get("tracks")
            .and_then(Value::as_object)
            .and_then(|t| t.get("items"))
            .and_then(Value::as_array)?;

        for item in items {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let Some(track_title) = obj
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };

            let artists = Self::extract_artists(obj.get("artists"));
            if !Self::is_match(track_title, &artists, title, artist) {
                continue;
            }

            let album_obj = obj.get("album").and_then(Value::as_object);
            let Some(video_cover) = album_obj
                .and_then(|a| a.get("videoCover"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let Some(video_url) = Self::cover_url(video_cover) else {
                continue;
            };

            let joined_artists = artists.join(", ");
            return Some(AnimatedArtwork {
                url: video_url,
                url_tall: String::new(),
                fallback_url: None,
                title: Some(track_title.to_string()),
                artist: if joined_artists.is_empty() {
                    None
                } else {
                    Some(joined_artists)
                },
                album: album_obj
                    .and_then(|a| a.get("title"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                source: CanvasSource::Tidal,
            });
        }
        None
    }

    pub async fn search_album(
        client: &reqwest::Client,
        album: &str,
        artist: &str,
    ) -> Option<AnimatedArtwork> {
        let query = format!("{album} {artist}");
        let country = Self::country_code();
        let mut headers = HashMap::new();
        headers.insert("X-Tidal-Token".to_string(), EMBED_TOKEN.to_string());
        headers.insert("User-Agent".to_string(), CANVAS_USER_AGENT.to_string());

        let root = canvas_get_json(
            client,
            SEARCH_URL,
            &headers,
            &[
                ("query", &query),
                ("limit", "10"),
                ("types", "ALBUMS"),
                ("countryCode", &country),
            ],
        )
        .await?;

        let items = root
            .get("albums")
            .and_then(Value::as_object)
            .and_then(|a| a.get("items"))
            .and_then(Value::as_array)?;

        for item in items {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let Some(record_title) = obj
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };

            let artists = Self::extract_artists(obj.get("artists"));
            if !Self::is_match(record_title, &artists, album, artist) {
                continue;
            }

            let Some(video_cover) = obj
                .get("videoCover")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let Some(video_url) = Self::cover_url(video_cover) else {
                continue;
            };

            let joined_artists = artists.join(", ");
            return Some(AnimatedArtwork {
                url: video_url,
                url_tall: String::new(),
                fallback_url: None,
                title: None,
                artist: if joined_artists.is_empty() {
                    None
                } else {
                    Some(joined_artists)
                },
                album: Some(record_title.to_string()),
                source: CanvasSource::Tidal,
            });
        }
        None
    }

    fn extract_artists(raw: Option<&Value>) -> Vec<String> {
        let Some(arr) = raw.and_then(Value::as_array) else {
            return Vec::new();
        };
        arr.iter()
            .filter_map(|a| a.as_object()?.get("name")?.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn is_match(
        got_name: &str,
        got_artists: &[String],
        want_name: &str,
        want_artist: &str,
    ) -> bool {
        if !canvas_titles_match(want_name, got_name) {
            return false;
        }
        let joined_credited: String = got_artists
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if joined_credited.is_empty() {
            return false;
        }
        canvas_artists_match(want_artist, &joined_credited, Some(want_name), Some(got_name))
    }

    pub fn cover_url(id: &str) -> Option<String> {
        let parts: Vec<&str> = id.split('-').collect();
        if parts.len() != 5 || parts.iter().any(|p| p.is_empty()) {
            return None;
        }
        Some(format!(
            "https://resources.tidal.com/videos/{}/1280x1280.mp4",
            parts.join("/")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_video_cover_uuid() {
        assert_eq!(
            TidalCanvas::cover_url("11111111-2222-3333-4444-555555555555"),
            Some("https://resources.tidal.com/videos/11111111/2222/3333/4444/555555555555/1280x1280.mp4".to_string())
        );
        assert_eq!(TidalCanvas::cover_url("invalid-id"), None);
    }
}
