//! Track/artist/album/user models for scrobbler data (Last.fm, ListenBrainz, Libre.fm).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeTrack {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub timestamp_millis: Option<i64>,
    #[serde(default)]
    pub play_count: i64,
}

impl HomeTrack {
    pub fn key(&self) -> String {
        format!("{}|{}", self.name.to_lowercase(), self.artist.to_lowercase())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeStats {
    #[serde(default)]
    pub scrobbles: i64,
    #[serde(default)]
    pub artist_count: i64,
    #[serde(default)]
    pub album_count: i64,
    #[serde(default)]
    pub track_count: i64,
    #[serde(default)]
    pub avatar_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopArtistItem {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub play_count: i64,
    #[serde(default)]
    pub artwork_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopAlbumItem {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub play_count: i64,
    #[serde(default)]
    pub artwork_url: String,
}

/// Shared Last.fm 2.0 / GNU FM JSON payload parser used by both
/// `HomeRepository` (Last.fm) and `LibreFmScrobbler` (Libre.fm).
pub struct LastFmResponseParser;

impl LastFmResponseParser {
    pub fn as_map(v: Option<&Value>) -> Option<&Map<String, Value>> {
        v.and_then(Value::as_object)
    }

    pub fn as_list(v: Option<&Value>) -> Vec<&Map<String, Value>> {
        match v {
            Some(Value::Array(arr)) => arr.iter().filter_map(Value::as_object).collect(),
            Some(Value::Object(obj)) => vec![obj],
            _ => Vec::new(),
        }
    }

    fn val_to_string(v: Option<&Value>) -> String {
        match v {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::Bool(b)) => b.to_string(),
            Some(other) => other.to_string(),
        }
    }

    fn parse_i64(v: Option<&Value>) -> Option<i64> {
        match v {
            Some(Value::Number(n)) => n.as_i64(),
            Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
            _ => None,
        }
    }

    pub fn best_image(images: Option<&Value>) -> String {
        let mut fallback = String::new();
        for img in Self::as_list(images) {
            let url = Self::val_to_string(img.get("#text"));
            if url.is_empty()
                || url.contains("2a96cbd8")
                || url.contains("default_album")
                || url.contains("noimage")
            {
                continue;
            }
            fallback = url.clone();
            let size = Self::val_to_string(img.get("size"));
            if size == "extralarge" || size == "large" {
                return url;
            }
        }
        fallback
    }

    pub fn parse_recent_tracks(json: &Value) -> Vec<HomeTrack> {
        let track_val = Self::as_map(Some(json))
            .and_then(|m| m.get("recenttracks"))
            .and_then(|v| Self::as_map(Some(v)))
            .and_then(|m| m.get("track"));
        let mut out = Vec::new();
        for t in Self::as_list(track_val) {
            let is_now_playing = Self::as_map(t.get("@attr"))
                .and_then(|attr| attr.get("nowplaying"))
                .is_some_and(|v| Self::val_to_string(Some(v)) == "true");
            if is_now_playing {
                continue;
            }
            let artist = match t.get("artist") {
                Some(Value::Object(m)) => m
                    .get("#text")
                    .or_else(|| m.get("name"))
                    .map(|v| Self::val_to_string(Some(v)))
                    .unwrap_or_default(),
                other => Self::val_to_string(other),
            };
            let ts_sec = Self::as_map(t.get("date"))
                .and_then(|d| d.get("uts"))
                .and_then(|u| Self::parse_i64(Some(u)));
            let name = Self::val_to_string(t.get("name"));
            if name.is_empty() {
                continue;
            }
            out.push(HomeTrack {
                name,
                artist,
                artwork_url: Self::best_image(t.get("image")),
                timestamp_millis: ts_sec.map(|s| s * 1000),
                play_count: 0,
            });
        }
        out
    }

    pub fn parse_total_attr(root: &Value, key: &str) -> i64 {
        Self::as_map(Some(root))
            .and_then(|m| m.get(key))
            .and_then(|v| Self::as_map(Some(v)))
            .and_then(|m| m.get("@attr"))
            .and_then(|v| Self::as_map(Some(v)))
            .and_then(|attr| Self::parse_i64(attr.get("total")))
            .unwrap_or(0)
    }

    pub fn parse_stats(
        user_info_json: &Value,
        top_tracks_json: Option<&Value>,
        top_artists_json: Option<&Value>,
        top_albums_json: Option<&Value>,
    ) -> HomeStats {
        let info = Self::as_map(Some(user_info_json)).and_then(|m| m.get("user"));
        let info_map = Self::as_map(info);
        HomeStats {
            scrobbles: info_map
                .and_then(|m| Self::parse_i64(m.get("playcount")))
                .unwrap_or(0),
            track_count: top_tracks_json
                .map(|j| Self::parse_total_attr(j, "toptracks"))
                .unwrap_or(0),
            artist_count: top_artists_json
                .map(|j| Self::parse_total_attr(j, "topartists"))
                .unwrap_or(0),
            album_count: top_albums_json
                .map(|j| Self::parse_total_attr(j, "topalbums"))
                .unwrap_or(0),
            avatar_url: info_map
                .map(|m| Self::best_image(m.get("image")))
                .unwrap_or_default(),
        }
    }

    pub fn parse_period_play_count(json: &Value) -> i64 {
        Self::parse_total_attr(json, "recenttracks")
    }

    pub fn parse_top_tracks(json: &Value) -> Vec<HomeTrack> {
        let track_val = Self::as_map(Some(json))
            .and_then(|m| m.get("toptracks"))
            .and_then(|v| Self::as_map(Some(v)))
            .and_then(|m| m.get("track"));
        Self::as_list(track_val)
            .into_iter()
            .filter_map(|t| {
                let name = Self::val_to_string(t.get("name"));
                if name.is_empty() {
                    return None;
                }
                let artist = match t.get("artist") {
                    Some(Value::Object(m)) => Self::val_to_string(m.get("name")),
                    other => Self::val_to_string(other),
                };
                Some(HomeTrack {
                    name,
                    artist,
                    artwork_url: Self::best_image(t.get("image")),
                    timestamp_millis: None,
                    play_count: Self::parse_i64(t.get("playcount")).unwrap_or(0),
                })
            })
            .collect()
    }

    pub fn parse_top_artists(json: &Value) -> Vec<TopArtistItem> {
        let artist_val = Self::as_map(Some(json))
            .and_then(|m| m.get("topartists"))
            .and_then(|v| Self::as_map(Some(v)))
            .and_then(|m| m.get("artist"));
        Self::as_list(artist_val)
            .into_iter()
            .filter_map(|a| {
                let name = Self::val_to_string(a.get("name"));
                if name.is_empty() {
                    return None;
                }
                Some(TopArtistItem {
                    name,
                    play_count: Self::parse_i64(a.get("playcount")).unwrap_or(0),
                    artwork_url: Self::best_image(a.get("image")),
                })
            })
            .collect()
    }

    pub fn parse_top_albums(json: &Value) -> Vec<TopAlbumItem> {
        let album_val = Self::as_map(Some(json))
            .and_then(|m| m.get("topalbums"))
            .and_then(|v| Self::as_map(Some(v)))
            .and_then(|m| m.get("album"));
        Self::as_list(album_val)
            .into_iter()
            .filter_map(|a| {
                let name = Self::val_to_string(a.get("name"));
                if name.is_empty() {
                    return None;
                }
                let artist = match a.get("artist") {
                    Some(Value::Object(m)) => Self::val_to_string(m.get("name")),
                    other => Self::val_to_string(other),
                };
                Some(TopAlbumItem {
                    name,
                    artist,
                    play_count: Self::parse_i64(a.get("playcount")).unwrap_or(0),
                    artwork_url: Self::best_image(a.get("image")),
                })
            })
            .collect()
    }
}
