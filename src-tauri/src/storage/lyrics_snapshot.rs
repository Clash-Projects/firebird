//! Compact JSON serialization for `LyricsResult`, including karaoke
//! syllable trees, for the on-disk lyrics cache (group `lyrics` in `snapshot_cache`).
//!
//! Short keys on purpose: a word-synced song is a few hundred syllables and
//! the cache keeps hundreds of songs.

use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::models::lyrics_models::{LyricLine, LyricSyllable, LyricsResult};

const VERSION: i64 = 1;

/// Cache-key prefix; bump `VERSION` and this together when the shape changes.
pub const LYRICS_CACHE_KEY_PREFIX: &str = "lyr1|";

/// How long a stored result is trusted before the race runs again (90 days).
pub const LYRICS_CACHE_MAX_AGE: Duration = Duration::from_secs(90 * 24 * 60 * 60);

pub fn encode_lyrics(r: &LyricsResult) -> String {
    let lines: Vec<Value> = r
        .lines
        .iter()
        .map(|l| {
            let mut obj = Map::new();
            obj.insert("t".to_string(), json!(l.time_ms));
            obj.insert("d".to_string(), json!(l.duration_ms));
            obj.insert("x".to_string(), json!(l.text));
            if !l.transliteration.is_empty() {
                obj.insert("tr".to_string(), json!(l.transliteration));
            }
            if !l.syllables.is_empty() {
                let syls: Vec<Value> = l
                    .syllables
                    .iter()
                    .map(|s| {
                        let mut so = Map::new();
                        so.insert("t".to_string(), json!(s.time_ms));
                        so.insert("d".to_string(), json!(s.duration_ms));
                        so.insert("x".to_string(), json!(s.text));
                        if s.is_background {
                            so.insert("b".to_string(), json!(1));
                        }
                        Value::Object(so)
                    })
                    .collect();
                obj.insert("s".to_string(), Value::Array(syls));
            }
            Value::Object(obj)
        })
        .collect();

    json!({
        "v": VERSION,
        "sy": r.is_synced,
        "ws": r.is_word_synced,
        "in": r.is_instrumental,
        "pl": r.plain_lyrics,
        "src": r.source,
        "ln": lines,
    })
    .to_string()
}

/// `None` when the payload is unreadable, from another version, or holds no
/// lyrics (an empty result is never worth serving from cache).
pub fn decode_lyrics(raw: &str) -> Option<LyricsResult> {
    let parsed: Value = serde_json::from_str(raw).ok()?;
    let map = parsed.as_object()?;
    if map.get("v")?.as_i64()? != VERSION {
        return None;
    }

    let mut lines = Vec::new();
    if let Some(arr) = map.get("ln").and_then(Value::as_array) {
        for item in arr {
            let Some(l) = item.as_object() else {
                continue;
            };
            let mut syllables = Vec::new();
            if let Some(s_arr) = l.get("s").and_then(Value::as_array) {
                for s_item in s_arr {
                    let Some(s) = s_item.as_object() else {
                        continue;
                    };
                    syllables.push(LyricSyllable {
                        time_ms: s.get("t").and_then(Value::as_i64).unwrap_or(0),
                        duration_ms: s.get("d").and_then(Value::as_i64).unwrap_or(0),
                        text: value_to_string(s.get("x")),
                        is_background: s.get("b").and_then(Value::as_i64) == Some(1),
                    });
                }
            }
            lines.push(LyricLine {
                time_ms: l.get("t").and_then(Value::as_i64).unwrap_or(0),
                duration_ms: l.get("d").and_then(Value::as_i64).unwrap_or(0),
                text: value_to_string(l.get("x")),
                syllables,
                transliteration: value_to_string(l.get("tr")),
                agent: None,
            });
        }
    }

    let result = LyricsResult {
        lines,
        is_synced: map.get("sy").and_then(Value::as_bool).unwrap_or(false),
        is_word_synced: map.get("ws").and_then(Value::as_bool).unwrap_or(false),
        plain_lyrics: value_to_string(map.get("pl")),
        is_instrumental: map.get("in").and_then(Value::as_bool).unwrap_or(false),
        source: value_to_string(map.get("src")),
    };

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn value_to_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip_and_empty_rejection() {
        let r = LyricsResult {
            lines: vec![LyricLine {
                time_ms: 1200,
                duration_ms: 2500,
                text: "Hello world".to_string(),
                transliteration: "Hel-lo".to_string(),
                syllables: vec![
                    LyricSyllable {
                        time_ms: 1200,
                        duration_ms: 500,
                        text: "Hello ".to_string(),
                        is_background: false,
                    },
                    LyricSyllable {
                        time_ms: 1700,
                        duration_ms: 700,
                        text: "(ooh)".to_string(),
                        is_background: true,
                    },
                ],
                agent: None,
            }],
            is_synced: true,
            is_word_synced: true,
            plain_lyrics: "Hello world".to_string(),
            is_instrumental: false,
            source: "apple".to_string(),
        };

        let encoded = encode_lyrics(&r);
        let decoded = decode_lyrics(&encoded).expect("valid payload");
        assert_eq!(decoded, r);

        let empty_json = encode_lyrics(&LyricsResult::empty());
        assert_eq!(decode_lyrics(&empty_json), None);
        assert_eq!(decode_lyrics("{\"v\":2,\"pl\":\"hi\"}"), None);
        assert_eq!(decode_lyrics("not json"), None);
    }
}
