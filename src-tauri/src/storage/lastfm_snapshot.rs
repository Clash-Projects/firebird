//! Disk snapshots for Last.fm pages (profile stats, top tracks, recent
//! history, friends list) stored in the `lastfm` group of `snapshot_cache`.
//!
//! 1:1 port of `features/lastfm/lastfm_snapshot.dart`.

use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::models::lastfm_home_models::{HomeStats, HomeTrack};
use crate::storage::home_snapshot::SnapshotCodec;

/// Cache group for Last.fm snapshots in `snapshot_cache`.
pub const CACHE_GROUP_LASTFM: &str = "lastfm";

/// Last.fm data is cheap to re-fetch and changes by the minute, so a copy
/// older than 14 days is not worth painting.
pub const LASTFM_CACHE_MAX_AGE: Duration = Duration::from_secs(14 * 24 * 60 * 60);

const VERSION: i64 = 1;

/// Key for one Last.fm view. `owner` is the signed-in user and `viewing` the
/// friend being browsed (`None` = own profile). `None` when nobody is signed in.
pub fn last_fm_cache_key(
    what: &str,
    owner: &str,
    viewing: Option<&str>,
    extra: &str,
) -> Option<String> {
    if owner.is_empty() {
        return None;
    }
    let subject = match viewing {
        Some(v) if !v.is_empty() => v,
        _ => owner,
    };
    let mut key = format!(
        "lf_{what}_v1|{}|{}",
        owner.to_lowercase(),
        subject.to_lowercase()
    );
    if !extra.is_empty() {
        key.push('|');
        key.push_str(extra);
    }
    Some(key)
}

fn str_field(m: &Map<String, Value>, key: &str) -> String {
    match m.get(key) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

fn decode_versioned_list<T>(raw: &str, mut read: impl FnMut(&Map<String, Value>) -> T) -> Option<Vec<T>> {
    let val: Value = serde_json::from_str(raw).ok()?;
    let obj = val.as_object()?;
    if obj.get("v").and_then(Value::as_i64) != Some(VERSION) {
        return None;
    }
    let items = match obj.get("items") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_object)
            .map(&mut read)
            .collect(),
        _ => Vec::new(),
    };
    Some(items)
}

/// Snapshot codec for `Vec<HomeTrack>`.
#[derive(Debug, Clone, Copy, Default)]
pub struct HomeTracksSnapshotCodec;

impl SnapshotCodec<Vec<HomeTrack>> for HomeTracksSnapshotCodec {
    fn encode(&self, list: &Vec<HomeTrack>) -> String {
        let items: Vec<Value> = list
            .iter()
            .map(|t| {
                let mut m = Map::new();
                m.insert("n".to_string(), json!(t.name));
                m.insert("a".to_string(), json!(t.artist));
                m.insert("art".to_string(), json!(t.artwork_url));
                if let Some(ts) = t.timestamp_millis {
                    m.insert("t".to_string(), json!(ts));
                }
                m.insert("pc".to_string(), json!(t.play_count));
                Value::Object(m)
            })
            .collect();
        json!({ "v": VERSION, "items": items }).to_string()
    }

    fn decode(&self, raw: &str) -> Option<Vec<HomeTrack>> {
        decode_versioned_list(raw, |e| HomeTrack {
            name: str_field(e, "n"),
            artist: str_field(e, "a"),
            artwork_url: str_field(e, "art"),
            timestamp_millis: e.get("t").and_then(Value::as_i64),
            play_count: e.get("pc").and_then(Value::as_i64).unwrap_or(0),
        })
    }

    fn is_empty(&self, value: &Vec<HomeTrack>) -> bool {
        value.is_empty()
    }
}

/// Snapshot codec for `HomeStats`.
#[derive(Debug, Clone, Copy, Default)]
pub struct HomeStatsSnapshotCodec;

impl SnapshotCodec<HomeStats> for HomeStatsSnapshotCodec {
    fn encode(&self, s: &HomeStats) -> String {
        json!({
            "v": VERSION,
            "s": s.scrobbles,
            "ar": s.artist_count,
            "al": s.album_count,
            "tr": s.track_count,
            "av": s.avatar_url,
        })
        .to_string()
    }

    fn decode(&self, raw: &str) -> Option<HomeStats> {
        let val: Value = serde_json::from_str(raw).ok()?;
        let obj = val.as_object()?;
        if obj.get("v").and_then(Value::as_i64) != Some(VERSION) {
            return None;
        }
        Some(HomeStats {
            scrobbles: obj.get("s").and_then(Value::as_i64).unwrap_or(0),
            artist_count: obj.get("ar").and_then(Value::as_i64).unwrap_or(0),
            album_count: obj.get("al").and_then(Value::as_i64).unwrap_or(0),
            track_count: obj.get("tr").and_then(Value::as_i64).unwrap_or(0),
            avatar_url: str_field(obj, "av"),
        })
    }

    /// An all-zero payload is a failed or private profile, not data to keep.
    fn is_empty(&self, s: &HomeStats) -> bool {
        s.scrobbles == 0 && s.artist_count == 0 && s.track_count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_formatting_matches_dart() {
        assert_eq!(last_fm_cache_key("recent", "", None, ""), None);
        assert_eq!(
            last_fm_cache_key("recent", "Alice", None, ""),
            Some("lf_recent_v1|alice|alice".to_string())
        );
        assert_eq!(
            last_fm_cache_key("top_tracks", "Alice", Some("Bob"), "7day"),
            Some("lf_top_tracks_v1|alice|bob|7day".to_string())
        );
        assert_eq!(
            last_fm_cache_key("stats", "Alice", Some(""), ""),
            Some("lf_stats_v1|alice|alice".to_string())
        );
    }

    #[test]
    fn home_tracks_and_stats_codecs_roundtrip() {
        let tracks = vec![
            HomeTrack {
                name: "Track 1".to_string(),
                artist: "Artist 1".to_string(),
                artwork_url: "https://img/1.jpg".to_string(),
                timestamp_millis: Some(1_700_000_000_000),
                play_count: 12,
            },
            HomeTrack {
                name: "Track 2".to_string(),
                artist: "Artist 2".to_string(),
                artwork_url: String::new(),
                timestamp_millis: None,
                play_count: 0,
            },
        ];
        let tc = HomeTracksSnapshotCodec;
        let raw_tracks = tc.encode(&tracks);
        assert!(!tc.is_empty(&tracks));
        assert_eq!(tc.decode(&raw_tracks), Some(tracks));
        assert_eq!(tc.decode("{\"v\":2,\"items\":[]}"), None);

        let stats = HomeStats {
            scrobbles: 1000,
            artist_count: 50,
            album_count: 120,
            track_count: 300,
            avatar_url: "https://img/av.jpg".to_string(),
        };
        let sc = HomeStatsSnapshotCodec;
        let raw_stats = sc.encode(&stats);
        assert!(!sc.is_empty(&stats));
        assert_eq!(sc.decode(&raw_stats), Some(stats));
        assert!(sc.is_empty(&HomeStats::default()));
    }
}
