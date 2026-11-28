use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

use regex::Regex;
use reqwest::header::{ACCEPT, USER_AGENT};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::extractor::http_client::HttpClientFactory;
use crate::storage::prefs::LYRICS_TRANSLATION_OPTIONS;

const TRANSLATE_ENDPOINT: &str = "https://translate.googleapis.com/translate_a/single";
const MAX_BATCH_CHARS: usize = 3500;

static MARKER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("\u{E000}[^\u{E001}]*\u{E001}").expect("valid marker regex"));

/// One translate request for the lines of the track that is open.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsTranslationKey {
    pub track_key: String,
    pub target: String,
    pub fingerprint: String,
}

/// Sub-lines aligned with `lines`. `None` when a line should not be drawn
/// (empty, or identical to the original).
pub struct LyricsTranslator {
    client: Client,
    memory: Mutex<HashMap<String, Vec<Option<String>>>>,
    roman_memory: Mutex<HashMap<String, Vec<Option<String>>>>,
}

impl Default for LyricsTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsTranslator {
    pub fn new() -> Self {
        let client = HttpClientFactory::create().unwrap_or_else(|_| Client::new());
        Self {
            client,
            memory: Mutex::new(HashMap::new()),
            roman_memory: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            memory: Mutex::new(HashMap::new()),
            roman_memory: Mutex::new(HashMap::new()),
        }
    }

    /// Phonetic romanisation (`dt=rm`) for lines containing non-Latin scripts
    /// (Devanagari/Indic, CJK, Japanese Kana/Kanji, Hangul, Cyrillic, Arabic, Thai, Greek).
    pub async fn romanize(&self, track_key: &str, lines: &[String]) -> Vec<Option<String>> {
        if lines.is_empty() {
            return Vec::new();
        }
        let cache_key = format!("rom|{track_key}|{}", lines_fingerprint_hash(lines));
        {
            let guard = self.roman_memory.lock().await;
            if let Some(cached) = guard.get(&cache_key) {
                if cached.len() == lines.len() {
                    return cached.clone();
                }
            }
        }

        let slots: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let t = l.trim();
                if !t.is_empty() && needs_romanization(t) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        let mut out = vec![None; lines.len()];
        if slots.is_empty() {
            let mut guard = self.roman_memory.lock().await;
            guard.insert(cache_key, out.clone());
            return out;
        }

        let mut start = 0;
        while start < slots.len() {
            let mut end = start;
            let mut length = 0usize;
            while end < slots.len() {
                let added = lines[slots[end]].chars().count() + 4;
                if end > start && length + added > MAX_BATCH_CHARS {
                    break;
                }
                length += added;
                end += 1;
            }
            let slice = &slots[start..end];
            let batch_inputs: Vec<&str> = slice.iter().map(|&idx| lines[idx].as_str()).collect();

            // Fast path: newline-joined `dt=rm` preserves `\n` line boundaries in ~120ms.
            let mut resolved = self.romanize_batch(&batch_inputs).await;
            if resolved.as_ref().is_none_or(|v| v.len() != slice.len()) {
                // Deterministic fallback: individual line requests in small concurrent chunks.
                let mut per_line = Vec::with_capacity(slice.len());
                for &slot_idx in slice {
                    let single = self
                        .romanize_single(lines[slot_idx].trim())
                        .await
                        .unwrap_or_default();
                    per_line.push(single);
                }
                resolved = Some(per_line);
            }

            if let Some(romanized) = resolved {
                for (i, &slot_idx) in slice.iter().enumerate() {
                    if let Some(raw) = romanized.get(i) {
                        let text = raw.trim();
                        let original = lines[slot_idx].trim();
                        if !text.is_empty() && !text.eq_ignore_ascii_case(original) {
                            out[slot_idx] = Some(text.to_string());
                        }
                    }
                }
            }
            start = end;
        }

        let mut guard = self.roman_memory.lock().await;
        guard.insert(cache_key, out.clone());
        out
    }

    async fn romanize_batch(&self, texts: &[&str]) -> Option<Vec<String>> {
        let joined_input = texts
            .iter()
            .map(|s| s.replace('\n', " ").trim().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let raw = self.fetch_dt_rm(&joined_input).await?;
        let parts: Vec<String> = raw.split('\n').map(|s| s.trim().to_string()).collect();
        if parts.len() == texts.len() {
            Some(parts)
        } else {
            None
        }
    }

    async fn romanize_single(&self, text: &str) -> Option<String> {
        if text.is_empty() {
            return None;
        }
        let raw = self.fetch_dt_rm(text).await?;
        let clean = raw
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if clean.is_empty() {
            None
        } else {
            Some(clean)
        }
    }

    async fn fetch_dt_rm(&self, query: &str) -> Option<String> {
        let resp = self
            .client
            .post(TRANSLATE_ENDPOINT)
            .query(&[
                ("client", "gtx"),
                ("sl", "auto"),
                ("tl", "en"),
                ("dt", "t"),
                ("dt", "rm"),
            ])
            .header(USER_AGENT, "LastWave/1.0")
            .header(ACCEPT, "application/json")
            .form(&[("q", query)])
            .send()
            .await
            .ok()?;

        let body = resp.text().await.ok()?;
        parse_romanize_response(&body)
    }

    pub async fn translate(
        &self,
        track_key: &str,
        target: &str,
        lines: &[String],
    ) -> Vec<Option<String>> {
        if target == "off" || target.is_empty() || lines.is_empty() {
            return vec![None; lines.len()];
        }

        let cache_key = format!("{track_key}|{target}|{}", lines_fingerprint_hash(lines));
        {
            let guard = self.memory.lock().await;
            if let Some(cached) = guard.get(&cache_key) {
                if cached.len() == lines.len() {
                    return cached.clone();
                }
            }
        }

        let slots: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| if l.trim().is_empty() { None } else { Some(i) })
            .collect();
        let mut out = vec![None; lines.len()];
        if slots.is_empty() {
            return out;
        }

        let mut start = 0;
        while start < slots.len() {
            let mut end = start;
            let mut length = 0usize;
            while end < slots.len() {
                let added = lines[slots[end]].chars().count() + 8;
                if end > start && length + added > MAX_BATCH_CHARS {
                    break;
                }
                length += added;
                end += 1;
            }
            let slice = &slots[start..end];
            let batch_inputs: Vec<&str> = slice.iter().map(|&idx| lines[idx].as_str()).collect();
            let Some(translated) = self.batch(&batch_inputs, target).await else {
                return out;
            };
            if translated.len() != slice.len() {
                return out;
            }
            for (i, &slot_idx) in slice.iter().enumerate() {
                let text = translated[i].trim();
                let original = lines[slot_idx].trim();
                if text.is_empty() || text == original {
                    continue;
                }
                out[slot_idx] = Some(text.to_string());
            }
            start = end;
        }

        let mut guard = self.memory.lock().await;
        guard.insert(cache_key, out.clone());
        out
    }

    async fn batch(&self, texts: &[&str], target: &str) -> Option<Vec<String>> {
        let payload = build_batch_payload(texts);
        let form = [
            ("client", "dict-chrome-ex"),
            ("sl", "auto"),
            ("tl", target),
            ("dt", "t"),
            ("q", payload.as_str()),
        ];

        let resp = self
            .client
            .post(TRANSLATE_ENDPOINT)
            .header(USER_AGENT, "LastWave/1.0")
            .header(ACCEPT, "application/json")
            .form(&form)
            .send()
            .await
            .ok()?;

        let body = resp.text().await.ok()?;
        parse_translate_response(&body, texts.len())
    }
}

fn lines_fingerprint_hash(lines: &[String]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            "\n".hash(&mut hasher);
        }
        line.hash(&mut hasher);
    }
    hasher.finish()
}

pub fn build_batch_payload(texts: &[&str]) -> String {
    let mut payload = String::new();
    for (i, text) in texts.iter().enumerate() {
        if i > 0 {
            payload.push('\n');
            payload.push_str(&format!("\u{E000}{i:04}\u{E001}"));
            payload.push('\n');
        }
        payload.push_str(text);
    }
    payload
}

pub fn parse_translate_response(body: &str, expected_count: usize) -> Option<Vec<String>> {
    if body.is_empty() {
        return None;
    }
    let root: Value = serde_json::from_str(body).ok()?;
    let segments = root.as_array()?.first()?.as_array()?;
    let mut joined = String::new();
    for seg in segments {
        if let Some(text) = seg.as_array().and_then(|a| a.first()).and_then(Value::as_str) {
            joined.push_str(text);
        }
    }
    let parts: Vec<String> = MARKER_RE
        .split(&joined)
        .map(|s| s.trim().to_string())
        .collect();
    if parts.len() != expected_count || parts.iter().any(|s| s.is_empty()) {
        return None;
    }
    Some(parts)
}

/// True when `text` contains non-Latin script characters (Devanagari/Indic, CJK,
/// Japanese Hiragana/Katakana, Korean Hangul, Cyrillic, Arabic/Hebrew, Thai, Greek, etc.).
pub fn needs_romanization(text: &str) -> bool {
    text.chars().any(|ch| {
        let u = ch as u32;
        (0x0370..=0x03FF).contains(&u) // Greek
            || (0x0400..=0x052F).contains(&u) // Cyrillic
            || (0x0530..=0x08FF).contains(&u) // Armenian, Hebrew, Arabic, Syriac
            || (0x0900..=0x0DFF).contains(&u) // Devanagari, Bengali, Gurmukhi, Gujarati, Tamil, Telugu, Kannada, Malayalam
            || (0x0E00..=0x109F).contains(&u) // Thai, Lao, Tibetan, Myanmar
            || (0x10A0..=0x10FF).contains(&u) // Georgian
            || (0x1100..=0x11FF).contains(&u) // Hangul Jamo
            || (0x1780..=0x17FF).contains(&u) // Khmer
            || (0x3040..=0x30FF).contains(&u) // Hiragana & Katakana
            || (0x3400..=0x9FFF).contains(&u) // CJK Unified Ideographs
            || (0xAC00..=0xD7AF).contains(&u) // Korean Hangul Syllables
            || (0xF900..=0xFAFF).contains(&u) // CJK Compatibility Ideographs
    })
}

/// Extracts source phonetic transliteration (`dt=rm`) from Google `translate_a/single`.
pub fn parse_romanize_response(body: &str) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let root: Value = serde_json::from_str(body).ok()?;
    let segments = root.as_array()?.first()?.as_array()?;
    let mut romaji = String::new();
    for seg in segments {
        let Some(arr) = seg.as_array() else {
            continue;
        };
        // Source-text transliteration lives at index 3 (or index 2 on compact responses)
        // in the trailing metadata tuple where `arr[0]` is null.
        if arr.first().is_some_and(Value::is_null) {
            if let Some(src_rm) = arr
                .get(3)
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .or_else(|| {
                    arr.get(2)
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                })
            {
                romaji.push_str(src_rm);
            }
        }
    }
    if romaji.trim().is_empty() {
        None
    } else {
        Some(romaji)
    }
}

/// Builds the provider key from the open track. `None` when translation is off.
pub fn lyrics_translation_key(
    track_key: &str,
    target: &str,
    lines: &[String],
) -> Option<LyricsTranslationKey> {
    if target == "off"
        || !LYRICS_TRANSLATION_OPTIONS
            .iter()
            .any(|(code, _)| *code == target)
    {
        return None;
    }
    Some(LyricsTranslationKey {
        track_key: track_key.to_string(),
        target: target.to_string(),
        fingerprint: lines.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_payload_and_response_parsing() {
        let payload = build_batch_payload(&["Hello world", "Good night"]);
        assert!(payload.contains("\u{E000}0001\u{E001}"));

        let fake_json = format!(
            r#"[[["Hola mundo\n\uE0000001\uE001\nBuenas noches", "src", null, null, 1]]]"#
        );
        let parts = parse_translate_response(&fake_json, 2).unwrap();
        assert_eq!(parts, vec!["Hola mundo", "Buenas noches"]);

        assert!(parse_translate_response(&fake_json, 3).is_none());
        assert!(parse_translate_response("", 1).is_none());
    }

    #[test]
    fn translation_key_validates_target_option() {
        let lines = vec!["Line 1".to_string()];
        assert!(lyrics_translation_key("k", "off", &lines).is_none());
        assert!(lyrics_translation_key("k", "xx", &lines).is_none());
        let key = lyrics_translation_key("k", "es", &lines).unwrap();
        assert_eq!(key.target, "es");
        assert_eq!(key.fingerprint, "Line 1");
    }
}
