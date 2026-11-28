use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::{LazyLock, Mutex};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::{DateTime, Utc};
use flate2::read::ZlibDecoder;
use hmac::{Hmac, Mac};
use regex::Regex;
use reqwest::header::{ACCEPT, USER_AGENT};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use url::Url;

use crate::models::lyrics_models::{
    is_lyric_credit_line, lyrics_artists_match_strict, lyrics_decode_entities,
    lyrics_for_search_artist, lyrics_for_search_title, lyrics_plausible_duration,
    lyrics_same_version, lyrics_titles_match_strict, parse_enhanced_lrc, parse_lrc, parse_ttml,
    strip_leading_artist_prefix, strip_lyric_credit_lines, LyricLine, LyricSyllable, LyricsResult,
};

type HmacSha256 = Hmac<Sha256>;

const LASTWAVE_UA: &str =
    "LastWave-Desktop/1.0 (https://github.com/clash-projects/lastwave-desktop)";
const KUGOU_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";
const GENIUS_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)";

static KARAOKE_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[(\d{1,8}),(\d{1,8})](.*)$").expect("karaoke line regex"));
static KARAOKE_WORD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\((\d{1,8}),(\d{1,8})(?:,\d{1,8})?\)([^()]*)").expect("karaoke word regex")
});
static KARAOKE_TIME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\(\d{1,8},\d{1,8}(?:,\d{1,8})?\)").expect("karaoke time regex")
});

static KRC_OFFSET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\[offset:\s*([+-]?\d+)\s*]").expect("krc offset"));
static KRC_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[(\d+),(\d+)](.*)$").expect("krc line"));
static KRC_SYLLABLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<(\d+),(\d+),\d+>([^<]*)").expect("krc syllable"));
static KRC_CREDIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^\s*(?:lyrics\s*by|written\s*by|composed\s*by|produced\s*by|arranged\s*by|recorded\s*(?:at|by)|mixed\s*by|remixed\s*by|mixing\s*(?:assistant|engineer)?|mastered\s*by|mastering|drums|guitar|bass|keyboards|strings|vocals?|作\s*词|作\s*曲|编\s*曲|制\s*作(?:人)?|演\s*唱|原\s*唱|录\s*音|混\s*音|母\s*带|吉\s*他|贝\s*斯|鼓)\s*[:：]",
    )
    .expect("krc credit")
});
static KRC_META_TI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\[ti:\s*(.*?)\s*\]$").expect("krc ti"));
static KRC_META_AL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\[al:\s*(.*?)\s*\]$").expect("krc al"));
static DJ_MIX_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[\(\[]\s*(?:mixed|dj\s+mix)\s*[\)\]]").expect("dj mix"));
static SHORT_TRACK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(intro|outro|interlude|skit|short)\b").expect("short track")
});
static KUGOU_FEAT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s*(?:feat\.?|ft\.?)\s+.*$").expect("kugou feat"));

static MEGALOBIZ_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)href=["'](/lrc/maker/download/[^"']+)["']"#).expect("megalobiz link")
});
static MEGALOBIZ_BODY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)id=["']lrc_[^"']*_details["'][^>]*>(.*?)</span>"#).expect("megalobiz body")
});
static GENIUS_CONTAINER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<div[^>]*data-lyrics-container=["']true["'][^>]*>(.*?)</div>"#)
        .expect("genius container")
});
static BR_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").expect("br tag"));
static HTML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").expect("html tag"));

static MXM_TOKEN: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsProviderId {
    Auto,
    Lastwave,
    LrcRed,
    AppleMusic,
    BetterLyrics,
    BetterLyricsPortato,
    Kugou,
    BiniLyrics,
    Musixmatch,
    Lrclib,
    LyricsPlus,
    Unison,
    YoutubeCaptions,
    YoutubeMusic,
    Megalobiz,
    Genius,
}

impl LyricsProviderId {
    pub const ALL: &'static [Self] = &[
        Self::Auto,
        Self::Lastwave,
        Self::LrcRed,
        Self::AppleMusic,
        Self::BetterLyrics,
        Self::BetterLyricsPortato,
        Self::Kugou,
        Self::BiniLyrics,
        Self::Musixmatch,
        Self::Lrclib,
        Self::LyricsPlus,
        Self::Unison,
        Self::YoutubeCaptions,
        Self::YoutubeMusic,
        Self::Megalobiz,
        Self::Genius,
    ];

    pub const fn id(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Lastwave => "lastwave",
            Self::LrcRed => "lrc_red",
            Self::AppleMusic => "apple_music",
            Self::BetterLyrics => "better_lyrics",
            Self::BetterLyricsPortato => "better_lyrics_portato",
            Self::Kugou => "kugou",
            Self::BiniLyrics => "bini_lyrics",
            Self::Musixmatch => "musixmatch",
            Self::Lrclib => "lrclib",
            Self::LyricsPlus => "lyrics_plus",
            Self::Unison => "unison",
            Self::YoutubeCaptions => "youtube_captions",
            Self::YoutubeMusic => "youtube_music",
            Self::Megalobiz => "megalobiz",
            Self::Genius => "genius",
        }
    }

    pub const fn title(&self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Lastwave => "LastWave",
            Self::LrcRed => "lrc.red",
            Self::AppleMusic => "Apple Music",
            Self::BetterLyrics => "BetterLyrics",
            Self::BetterLyricsPortato => "QQ Karaoke",
            Self::Kugou => "Kugou",
            Self::BiniLyrics => "Syllable-Sync",
            Self::Musixmatch => "Catalog",
            Self::Lrclib => "LRCLIB",
            Self::LyricsPlus => "LyricsPlus",
            Self::Unison => "Unison",
            Self::YoutubeCaptions => "YouTube Captions",
            Self::YoutubeMusic => "YouTube Music",
            Self::Megalobiz => "Megalobiz",
            Self::Genius => "Genius",
        }
    }

    pub const fn subtitle(&self) -> &'static str {
        match self {
            Self::Auto => "Fastest word-sync wins, LRCLIB fallback",
            Self::Lastwave => "Syllable & line-synced Apple lyrics first",
            Self::LrcRed => "ISRC-indexed syllable TTML & LRC first",
            Self::AppleMusic => "Syllable-synced Apple Music lyrics first",
            Self::BetterLyrics => "Word-synced lyrics first",
            Self::BetterLyricsPortato => "QQ Music karaoke word timings first",
            Self::Kugou => "KRC word-synced lyrics first",
            Self::BiniLyrics => "Recording-matched syllable lyrics first",
            Self::Musixmatch => "Largest catalogue line-sync first",
            Self::Lrclib => "Line-synced community lyrics first",
            Self::LyricsPlus => "Syllable-synced on community mirrors first",
            Self::Unison => "Community-submitted synced lyrics first",
            Self::YoutubeCaptions => "Timed captions from the playing video first",
            Self::YoutubeMusic => "Plain lyrics from the video's Lyrics tab first",
            Self::Megalobiz => "Community line-synced LRC first",
            Self::Genius => "Plain-text web catalogue, tried last",
        }
    }

    /// True for providers that can return syllable/word timing and take
    /// part in the word-sync race with a preferred head start.
    pub const fn is_word_provider(&self) -> bool {
        !matches!(
            self,
            Self::Auto
                | Self::Lrclib
                | Self::Musixmatch
                | Self::Megalobiz
                | Self::Genius
                | Self::YoutubeMusic
                | Self::YoutubeCaptions
        )
    }

    pub fn from_id(id: Option<&str>) -> Self {
        let Some(id) = id else {
            return Self::Auto;
        };
        Self::ALL
            .iter()
            .copied()
            .find(|p| p.id() == id)
            .unwrap_or(Self::Auto)
    }
}

/// Map a result `source` label back to its provider id.
pub fn lyrics_source_to_provider_id(source: &str) -> Option<&'static str> {
    let s = source.to_ascii_lowercase();
    if s.starts_with("lastwave") {
        return Some(LyricsProviderId::Lastwave.id());
    }
    if s.starts_with("lrc.red") {
        return Some(LyricsProviderId::LrcRed.id());
    }
    if s.starts_with("apple music") || s.contains("apple") {
        return Some(LyricsProviderId::AppleMusic.id());
    }
    if s.starts_with("qq karaoke") || s.starts_with("portato") {
        return Some(LyricsProviderId::BetterLyricsPortato.id());
    }
    if s.starts_with("betterlyrics") {
        return Some(LyricsProviderId::BetterLyrics.id());
    }
    if s.starts_with("kugou") {
        return Some(LyricsProviderId::Kugou.id());
    }
    if s.starts_with("syllable-sync") || s.starts_with("bini") {
        return Some(LyricsProviderId::BiniLyrics.id());
    }
    if s.starts_with("catalog") || s.starts_with("musixmatch") {
        return Some(LyricsProviderId::Musixmatch.id());
    }
    if s.contains("lrclib") {
        return Some(LyricsProviderId::Lrclib.id());
    }
    if s.starts_with("lyricsplus") {
        return Some(LyricsProviderId::LyricsPlus.id());
    }
    if s.starts_with("unison") {
        return Some(LyricsProviderId::Unison.id());
    }
    if s.starts_with("youtube captions") {
        return Some(LyricsProviderId::YoutubeCaptions.id());
    }
    if s.starts_with("youtube music") {
        return Some(LyricsProviderId::YoutubeMusic.id());
    }
    if s.starts_with("megalobiz") {
        return Some(LyricsProviderId::Megalobiz.id());
    }
    if s.starts_with("genius") {
        return Some(LyricsProviderId::Genius.id());
    }
    None
}

pub fn next_lyrics_selection(
    picked_id: Option<&str>,
    current_id: Option<&str>,
    current_excludes: &HashSet<String>,
) -> (Option<String>, HashSet<String>) {
    let Some(picked) = picked_id else {
        return (None, HashSet::new());
    };
    if picked == LyricsProviderId::Auto.id() {
        return (None, HashSet::new());
    }
    let mut excludes = current_excludes.clone();
    if let Some(cur) = current_id {
        if cur != picked {
            excludes.insert(cur.to_string());
        }
    }
    (Some(picked.to_string()), excludes)
}

pub fn retry_lyrics_excluding_current(
    current_id: Option<&str>,
    excludes: &HashSet<String>,
) -> HashSet<String> {
    let mut next = excludes.clone();
    if let Some(cur) = current_id {
        next.insert(cur.to_string());
    }
    next
}

// -- BetterLyrics (lyrics-api.boidu.dev) --------------------------------------

const BETTER_BASES: &[&str] = &[
    "https://lyrics-api.boidu.dev/getLyrics",
    "https://lyrics-api.boidu.dev/ttml/getLyrics",
    "https://lyrics-api.boidu.dev/qq/getLyrics",
];

const BETTER_ENVELOPE_KEYS: &[&str] = &[
    "ttml",
    "ttmlContent",
    "lyrics",
    "lrc",
    "content",
    "text",
    "plainLyrics",
    "syncedLyrics",
    "line",
    "lines",
    "lyric",
    "data",
    "result",
    "response",
];

pub async fn fetch_better_lyrics(
    client: &Client,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_seconds: Option<i64>,
) -> Option<LyricsResult> {
    if title.trim().is_empty() || artist.trim().is_empty() {
        return None;
    }
    let mut attempts = vec![(title.to_string(), artist.to_string())];
    let cleaned = (lyrics_for_search_title(title), lyrics_for_search_artist(artist));
    if cleaned.0 != title || cleaned.1 != artist {
        attempts.push(cleaned);
    }

    for (att_title, att_artist) in attempts {
        if att_title.trim().is_empty() || att_artist.trim().is_empty() {
            continue;
        }
        if let Some((lines, word_synced)) = fetch_better_attempt(
            client,
            &att_title,
            &att_artist,
            album,
            duration_seconds,
        )
        .await
        {
            if lyrics_plausible_duration(&lines, duration_seconds) {
                let plain_lyrics = lines
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                return Some(LyricsResult {
                    lines,
                    is_synced: true,
                    is_word_synced: word_synced,
                    plain_lyrics,
                    is_instrumental: false,
                    source: if word_synced {
                        "BetterLyrics (Word-Sync)".to_string()
                    } else {
                        "BetterLyrics (Line-Sync)".to_string()
                    },
                });
            }
        }
    }
    None
}

async fn fetch_better_attempt(
    client: &Client,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_seconds: Option<i64>,
) -> Option<(Vec<LyricLine>, bool)> {
    for base in BETTER_BASES {
        let mut qp = vec![
            ("s", title.trim().to_string()),
            ("a", artist.trim().to_string()),
        ];
        if let Some(d) = duration_seconds.filter(|&d| d > 0) {
            qp.push(("d", d.to_string()));
        }
        if let Some(al) = album.map(str::trim).filter(|s| !s.is_empty()) {
            qp.push(("al", al.to_string()));
        }
        let Ok(resp) = client
            .get(*base)
            .query(&qp)
            .header(ACCEPT, "application/json")
            .send()
            .await
        else {
            continue;
        };
        let Ok(body) = resp.text().await else {
            continue;
        };
        if body.is_empty() {
            continue;
        }
        if let Some(parsed) = parse_better_document(&body) {
            if !parsed.is_empty() {
                let payload = unwrap_better_payload(&body).unwrap_or(body);
                return Some((parsed, better_has_word_timing(&payload)));
            }
        }
    }
    None
}

pub fn better_has_word_timing(payload: &str) -> bool {
    let lower = payload.to_ascii_lowercase();
    if lower.contains("<tt")
        || lower.contains("<p")
        || lower.contains("http://www.w3.org/ns/ttml")
    {
        let ttml = parse_ttml(payload);
        if !ttml.is_empty() {
            return ttml.iter().any(|l| l.has_syllables());
        }
    }
    if !parse_better_karaoke(payload).is_empty() {
        return true;
    }
    if !parse_enhanced_lrc(payload).is_empty() {
        return true;
    }
    false
}

pub fn parse_better_document(raw: &str) -> Option<Vec<LyricLine>> {
    let payload = unwrap_better_payload(raw)?;
    if payload.is_empty() {
        return None;
    }
    let lower = payload.to_ascii_lowercase();
    if lower.contains("<tt")
        || lower.contains("<p")
        || lower.contains("http://www.w3.org/ns/ttml")
    {
        let ttml = parse_ttml(&payload);
        if !ttml.is_empty() {
            return Some(ttml);
        }
    }
    let karaoke = parse_better_karaoke(&payload);
    if !karaoke.is_empty() {
        return Some(karaoke);
    }
    let enhanced = parse_enhanced_lrc(&payload);
    if !enhanced.is_empty() {
        return Some(enhanced);
    }
    let lrc = parse_lrc(&payload);
    if !lrc.is_empty() {
        return Some(lrc);
    }
    None
}

fn unwrap_better_payload(raw: &str) -> Option<String> {
    let trimmed = raw.replace('\u{FEFF}', "").trim().to_string();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return Some(trimmed);
    }
    let Ok(element) = serde_json::from_str::<Value>(&trimmed) else {
        return Some(trimmed);
    };
    let found = extract_better_content(&element)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(found.unwrap_or(trimmed))
}

fn extract_better_content(element: &Value) -> Option<String> {
    match element {
        Value::String(raw) => {
            let text = raw.trim();
            if text.is_empty() {
                return None;
            }
            if (text.starts_with('{') || text.starts_with('['))
                && serde_json::from_str::<Value>(text).is_ok()
            {
                if let Ok(nested) = serde_json::from_str::<Value>(text) {
                    return extract_better_content(&nested).or_else(|| Some(text.to_string()));
                }
            }
            Some(text.to_string())
        }
        Value::Array(arr) => {
            let texts: Vec<String> = arr
                .iter()
                .filter_map(extract_better_content)
                .filter(|s| !s.is_empty())
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        Value::Object(map) => {
            if map.get("isError").and_then(Value::as_bool) == Some(true)
                || map.get("isError").and_then(Value::as_str) == Some("true")
                || map.get("ok").and_then(Value::as_bool) == Some(false)
                || map.get("ok").and_then(Value::as_str) == Some("false")
            {
                return None;
            }
            for key in BETTER_ENVELOPE_KEYS {
                if let Some(val) = map.get(*key) {
                    if let Some(found) = extract_better_content(val) {
                        if !found.is_empty() {
                            return Some(found);
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// QQ karaoke rows: `[lineStart,lineDur](wStart,wDur[,?])word ...`.
pub fn parse_better_karaoke(raw: &str) -> Vec<LyricLine> {
    if !raw.contains('[') || !raw.contains('(') {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for source in raw.split('\n') {
        let Some(caps) = KARAOKE_LINE_RE.captures(source.trim()) else {
            continue;
        };
        let Some(line_start) = caps.get(1).and_then(|m| m.as_str().parse::<i64>().ok()) else {
            continue;
        };
        let line_duration = caps
            .get(2)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0);
        let body = caps.get(3).map_or("", |m| m.as_str());

        let mut words = Vec::new();
        for wcap in KARAOKE_WORD_RE.captures_iter(body) {
            let text = lyrics_decode_entities(wcap.get(3).map_or("", |m| m.as_str()))
                .trim()
                .to_string();
            if text.is_empty() {
                continue;
            }
            let Some(start_ms) = wcap.get(1).and_then(|m| m.as_str().parse::<i64>().ok()) else {
                continue;
            };
            let dur_ms = wcap
                .get(2)
                .and_then(|m| m.as_str().parse::<i64>().ok())
                .unwrap_or(0);
            words.push(LyricSyllable {
                time_ms: start_ms,
                duration_ms: dur_ms.clamp(0, 1_i64 << 31),
                text,
                is_background: false,
            });
        }
        if words.is_empty() {
            continue;
        }
        let cleaned_body = KARAOKE_TIME_RE.replace_all(body, "");
        let text = lyrics_decode_entities(&cleaned_body).trim().to_string();
        if text.is_empty() {
            continue;
        }
        let first_word_ms = words[0].time_ms;
        rows.push(LyricLine {
            time_ms: line_start.min(first_word_ms),
            duration_ms: line_duration.clamp(0, 1_i64 << 31),
            text,
            syllables: words,
            transliteration: String::new(),
            agent: None,
        });
    }
    rows.sort_by_key(|l| l.time_ms);
    strip_lyric_credit_lines(
        &LyricsResult {
            lines: rows,
            is_synced: true,
            is_word_synced: true,
            ..Default::default()
        },
        "",
        "",
    )
    .lines
}

// -- Lrc.Red (lrc.red/api/v1, Bini-compatible) -------------------------------

const LRC_RED_BASE: &str = "https://lrc.red/api/v1";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LrcRedHit {
    pub track_name: Option<String>,
    pub artist_name: Option<String>,
    pub album_name: Option<String>,
    pub duration: Option<i64>,
    pub isrc: Option<String>,
    pub timing_type: Option<String>,
    #[serde(rename = "lyricsUrl")]
    pub lyrics_url: Option<String>,
}

pub fn score_lrc_red_hit(
    hit: &LrcRedHit,
    title: &str,
    artist: &str,
    duration_seconds: Option<i64>,
) -> i32 {
    let mut score = 0;
    let cand_title = lyrics_for_search_title(hit.track_name.as_deref().unwrap_or(""));
    let req_title = lyrics_for_search_title(title);
    if cand_title.eq_ignore_ascii_case(&req_title) {
        score += 3;
    } else if lyrics_titles_match_strict(&cand_title, &req_title) {
        score += 1;
    }

    let cand_artist = lyrics_for_search_artist(hit.artist_name.as_deref().unwrap_or(""));
    let req_artist = lyrics_for_search_artist(artist);
    if !cand_artist.is_empty()
        && !req_artist.is_empty()
        && (cand_artist.eq_ignore_ascii_case(&req_artist)
            || lyrics_artists_match_strict(&cand_artist, &req_artist))
    {
        score += 2;
    }

    let hit_secs = hit.duration.unwrap_or(0);
    if let Some(dur) = duration_seconds.filter(|&d| d > 0) {
        if hit_secs > 0 {
            let delta = (hit_secs - dur).abs();
            if delta <= 3 {
                score += 3;
            } else if delta <= 10 {
                score += 1;
            }
        }
    }
    score
}

fn lrc_red_duration_delta(hit: &LrcRedHit, duration_seconds: Option<i64>) -> i64 {
    let hit_secs = hit.duration.unwrap_or(0);
    let Some(dur) = duration_seconds.filter(|&d| d > 0) else {
        return 0;
    };
    if hit_secs <= 0 {
        return 0;
    }
    (hit_secs - dur).abs()
}

pub fn select_lrc_red_best(
    hits: &[LrcRedHit],
    title: &str,
    artist: &str,
    duration_seconds: Option<i64>,
) -> Option<LrcRedHit> {
    if hits.is_empty() {
        return None;
    }
    let mut scored: Vec<(LrcRedHit, i32)> = Vec::new();
    for hit in hits {
        if !lyrics_same_version(title, hit.track_name.as_deref().unwrap_or("")) {
            continue;
        }
        let score = score_lrc_red_hit(hit, title, artist, duration_seconds);
        if score >= 5 {
            scored.push((hit.clone(), score));
        }
    }
    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| {
                let a_word = if a
                    .0
                    .timing_type
                    .as_deref()
                    .is_some_and(|t| t.eq_ignore_ascii_case("word"))
                {
                    0
                } else {
                    1
                };
                let b_word = if b
                    .0
                    .timing_type
                    .as_deref()
                    .is_some_and(|t| t.eq_ignore_ascii_case("word"))
                {
                    0
                } else {
                    1
                };
                a_word.cmp(&b_word)
            })
            .then_with(|| {
                lrc_red_duration_delta(&a.0, duration_seconds)
                    .cmp(&lrc_red_duration_delta(&b.0, duration_seconds))
            })
    });
    scored.into_iter().next().map(|(h, _)| h)
}

async fn query_lrc_red(client: &Client, params: &[(&str, String)]) -> Vec<LrcRedHit> {
    let Ok(resp) = client
        .get(LRC_RED_BASE)
        .query(params)
        .header(ACCEPT, "application/json")
        .send()
        .await
    else {
        return Vec::new();
    };
    let Ok(val) = resp.json::<Value>().await else {
        return Vec::new();
    };
    let Some(results) = val.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    results
        .iter()
        .filter_map(|item| serde_json::from_value::<LrcRedHit>(item.clone()).ok())
        .collect()
}

pub async fn identify_lrc_red(
    client: &Client,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_seconds: Option<i64>,
    isrc: Option<&str>,
) -> Option<LrcRedHit> {
    if let Some(code) = isrc.map(str::trim).filter(|s| !s.is_empty()) {
        let mut hits = query_lrc_red(client, &[("isrc", code.to_string())]).await;
        hits.sort_by_key(|h| {
            if h.timing_type
                .as_deref()
                .is_some_and(|t| t.eq_ignore_ascii_case("word"))
            {
                0
            } else {
                1
            }
        });
        if let Some(first) = hits.into_iter().next() {
            return Some(first);
        }
    }
    if title.trim().is_empty() {
        return None;
    }
    let mut shaped = vec![
        ("track", title.trim().to_string()),
        ("artist", artist.trim().to_string()),
    ];
    if let Some(al) = album.map(str::trim).filter(|s| !s.is_empty()) {
        shaped.push(("album", al.to_string()));
    }
    if let Some(dur) = duration_seconds.filter(|&d| d > 0) {
        shaped.push(("duration", dur.to_string()));
    }
    let hits = query_lrc_red(client, &shaped).await;
    if let Some(best) = select_lrc_red_best(&hits, title, artist, duration_seconds) {
        return Some(best);
    }
    let q = if artist.trim().is_empty() {
        title.trim().to_string()
    } else {
        format!("{} - {}", artist.trim(), title.trim())
    };
    let fallback_hits = query_lrc_red(client, &[("q", q)]).await;
    select_lrc_red_best(&fallback_hits, title, artist, duration_seconds)
}

pub async fn fetch_lrc_red_lines(client: &Client, hit: &LrcRedHit) -> Option<Vec<LyricLine>> {
    let url = hit.lyrics_url.as_deref()?.trim();
    if url.is_empty() {
        return None;
    }
    let resp = client
        .get(url)
        .header(ACCEPT, "application/xml, text/xml, */*")
        .send()
        .await
        .ok()?;
    let ttml = resp.text().await.ok()?;
    if ttml.trim().is_empty() {
        return None;
    }
    let lines = parse_ttml(&ttml);
    if lines.is_empty() {
        None
    } else {
        Some(lines)
    }
}

pub async fn fetch_lrc_red(
    client: &Client,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_seconds: Option<i64>,
    isrc: Option<&str>,
) -> Option<LyricsResult> {
    let hit = identify_lrc_red(client, title, artist, album, duration_seconds, isrc).await?;
    let lines = fetch_lrc_red_lines(client, &hit).await?;
    if !lyrics_plausible_duration(&lines, duration_seconds) {
        return None;
    }
    let word_synced = hit
        .timing_type
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case("word"));
    let plain_lyrics = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Some(LyricsResult {
        lines,
        is_synced: true,
        is_word_synced: word_synced,
        plain_lyrics,
        is_instrumental: false,
        source: if word_synced {
            "Lrc.Red (Word-Sync)".to_string()
        } else {
            "Lrc.Red (Line-Sync)".to_string()
        },
    })
}

// -- Kugou KRC (lyrics.kugou.com) ---------------------------------------------

const KUGOU_KEY: [u8; 16] = [
    0x40, 0x47, 0x61, 0x77, 0x5e, 0x32, 0x74, 0x47, 0x51, 0x36, 0x31, 0x2d, 0xce, 0xd2, 0x6e,
    0x69,
];

fn is_kugou_credit_line(text: &str, is_first_line: bool) -> bool {
    if KRC_CREDIT_RE.is_match(text) {
        return true;
    }
    if is_lyric_credit_line(text, if is_first_line { 0 } else { 1 }, 10, "", "") {
        return true;
    }
    is_first_line && (text.contains(" - ") || text.contains(" – ") || text.contains(" — "))
}

/// Decrypt a KRC `content` payload: Base64 -> drop 4 magic bytes -> XOR -> zlib inflate.
pub fn decrypt_kugou_krc(base64_content: &str) -> Option<String> {
    let enc = BASE64_STANDARD.decode(base64_content.trim()).ok()?;
    if enc.len() <= 4 {
        return None;
    }
    let xored: Vec<u8> = enc[4..]
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ KUGOU_KEY[i % KUGOU_KEY.len()])
        .collect();
    let mut decoder = ZlibDecoder::new(xored.as_slice());
    let mut out = String::new();
    decoder.read_to_string(&mut out).ok()?;
    Some(out)
}

/// Parse decrypted KRC text into word-timed lines (including embedded `[language:...]` romaji/pinyin).
pub fn parse_kugou_krc(krc_text: &str) -> Vec<LyricLine> {
    static KRC_LANG_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\[language:\s*([A-Za-z0-9+/=]+)\s*]").expect("krc lang"));

    let mut phonetic_rows: Vec<String> = Vec::new();
    for raw_line in krc_text.split('\n') {
        let trimmed = raw_line.trim();
        if let Some(lcaps) = KRC_LANG_RE.captures(trimmed) {
            if let Ok(decoded) = BASE64_STANDARD.decode(lcaps[1].trim()) {
                if let Ok(lang_json) = serde_json::from_slice::<Value>(&decoded) {
                    if let Some(contents) = lang_json.get("content").and_then(Value::as_array) {
                        if let Some(phonetic_entry) = contents
                            .iter()
                            .find(|c| c.get("type").and_then(Value::as_i64) == Some(0))
                        {
                            if let Some(lyric_content) =
                                phonetic_entry.get("lyricContent").and_then(Value::as_array)
                            {
                                phonetic_rows = lyric_content
                                    .iter()
                                    .map(|row| {
                                        row.as_array()
                                            .map(|arr| {
                                                arr.iter()
                                                    .filter_map(Value::as_str)
                                                    .map(str::trim)
                                                    .filter(|s| !s.is_empty())
                                                    .collect::<Vec<_>>()
                                                    .join(" ")
                                            })
                                            .unwrap_or_default()
                                    })
                                    .collect();
                            }
                        }
                    }
                }
            }
            break;
        }
    }

    let mut lines = Vec::new();
    let mut global_offset_ms = 0i64;
    let mut is_first_raw_line = true;
    let mut raw_lyric_idx = 0usize;

    for raw_line in krc_text.split('\n') {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || !trimmed.starts_with('[') {
            continue;
        }
        if let Some(ocaps) = KRC_OFFSET_RE.captures(trimmed) {
            global_offset_ms = ocaps
                .get(1)
                .and_then(|m| m.as_str().parse::<i64>().ok())
                .unwrap_or(0);
            continue;
        }
        let Some(caps) = KRC_LINE_RE.captures(trimmed) else {
            continue;
        };
        let cur_lyric_idx = raw_lyric_idx;
        raw_lyric_idx += 1;

        let Some(raw_start) = caps.get(1).and_then(|m| m.as_str().parse::<i64>().ok()) else {
            continue;
        };
        let line_duration_ms = caps
            .get(2)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0);
        let syllables_content = caps.get(3).map_or("", |m| m.as_str());
        let line_start_ms = (raw_start + global_offset_ms).max(0);

        let mut syllables: Vec<LyricSyllable> = Vec::new();
        let mut buf = String::new();
        for scaps in KRC_SYLLABLE_RE.captures_iter(syllables_content) {
            let offset_ms = scaps
                .get(1)
                .and_then(|m| m.as_str().parse::<i64>().ok())
                .unwrap_or(0);
            let dur_ms = scaps
                .get(2)
                .and_then(|m| m.as_str().parse::<i64>().ok())
                .unwrap_or(0);
            let syl_text = scaps.get(3).map_or("", |m| m.as_str());
            if syl_text.trim().is_empty() {
                if let Some(last) = syllables.last_mut() {
                    if !last.text.ends_with(' ') {
                        last.text.push(' ');
                    }
                }
                buf.push_str(syl_text);
                continue;
            }
            syllables.push(LyricSyllable {
                time_ms: (line_start_ms + offset_ms).max(0),
                duration_ms: dur_ms,
                text: syl_text.to_string(),
                is_background: false,
            });
            buf.push_str(syl_text);
        }

        let full_line_text = buf.trim().to_string();
        if !full_line_text.is_empty() && !syllables.is_empty() {
            let is_credit = is_kugou_credit_line(&full_line_text, is_first_raw_line);
            is_first_raw_line = false;
            if is_credit {
                continue;
            }
            let transliteration = phonetic_rows
                .get(cur_lyric_idx)
                .cloned()
                .unwrap_or_default();
            lines.push(LyricLine {
                time_ms: line_start_ms,
                duration_ms: line_duration_ms,
                text: full_line_text,
                syllables,
                transliteration,
                agent: None,
            });
        }
    }
    lines.sort_by_key(|l| l.time_ms);
    lines
}

fn kugou_clean_artist(raw: &str) -> String {
    let no_feat = KUGOU_FEAT_RE.replace_all(raw, "");
    let trimmed = no_feat.trim();
    lyrics_for_search_artist(if trimmed.is_empty() { raw } else { trimmed })
}

fn kugou_duration_delta(cand: &Value, duration_seconds: Option<i64>) -> i64 {
    let cand_ms = cand.get("duration").and_then(Value::as_i64).unwrap_or(0);
    let Some(dur) = duration_seconds.filter(|&d| d > 0) else {
        return 0;
    };
    if cand_ms <= 0 {
        return 0;
    }
    (cand_ms - dur * 1000).abs()
}

pub fn kugou_krc_matches_track(krc_text: &str, title: &str) -> bool {
    if title.trim().is_empty() || DJ_MIX_TAG_RE.is_match(title) {
        return true;
    }
    for raw in krc_text.split('\n') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(caps) = KRC_META_TI_RE.captures(trimmed) {
            let krc_title = caps.get(1).map_or("", |m| m.as_str()).trim();
            if DJ_MIX_TAG_RE.is_match(krc_title) {
                return false;
            }
            continue;
        }
        if let Some(caps) = KRC_META_AL_RE.captures(trimmed) {
            let krc_album = caps.get(1).map_or("", |m| m.as_str()).trim();
            if DJ_MIX_TAG_RE.is_match(krc_album) {
                return false;
            }
            continue;
        }
        if KRC_LINE_RE.is_match(trimmed) {
            break;
        }
    }
    true
}

fn artist_matches_loose(candidate: &str, request: &str) -> bool {
    if candidate.is_empty() || request.is_empty() {
        return false;
    }
    if candidate.eq_ignore_ascii_case(request) {
        return true;
    }
    lyrics_artists_match_strict(candidate, request)
}

pub async fn fetch_kugou(
    client: &Client,
    title: &str,
    artist: &str,
    duration_seconds: Option<i64>,
) -> Option<LyricsResult> {
    if title.trim().is_empty() || artist.trim().is_empty() {
        return None;
    }
    let cleaned_title = lyrics_for_search_title(title);
    let cleaned_artist = kugou_clean_artist(artist);
    let mut search_title = strip_leading_artist_prefix(&cleaned_title);
    if search_title.is_empty() {
        search_title = cleaned_title.clone();
    }
    let has_known_duration = duration_seconds.is_some_and(|d| d > 0);
    let is_short_track_query = SHORT_TRACK_RE.is_match(title);

    let search_res = client
        .get("https://lyrics.kugou.com/search")
        .query(&[
            ("ver", "1".to_string()),
            ("man", "yes".to_string()),
            ("client", "pc".to_string()),
            ("keyword", format!("{cleaned_artist} - {search_title}")),
            (
                "duration",
                duration_seconds
                    .filter(|&d| d > 0)
                    .map(|d| (d * 1000).to_string())
                    .unwrap_or_default(),
            ),
            ("hash", String::new()),
        ])
        .header(USER_AGENT, KUGOU_UA)
        .send()
        .await
        .ok()?
        .json::<Value>()
        .await
        .ok()?;

    let candidates = search_res.get("candidates").and_then(Value::as_array)?;
    if candidates.is_empty() {
        return None;
    }

    let mut matched: Vec<Value> = candidates
        .iter()
        .filter(|cand| {
            let song = cand
                .get("song")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !lyrics_same_version(title, song) {
                return false;
            }
            let cand_dur_ms = cand.get("duration").and_then(Value::as_i64).unwrap_or(0);
            if !has_known_duration && !is_short_track_query && cand_dur_ms > 0 && cand_dur_ms < 55000
            {
                return false;
            }
            let cand_singer = kugou_clean_artist(
                cand.get("singer")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            let cand_song = lyrics_for_search_title(song);
            if !artist_matches_loose(&cand_singer, &cleaned_artist) {
                return false;
            }
            lyrics_titles_match_strict(&cand_song, &cleaned_title)
                || lyrics_titles_match_strict(&cand_song, &search_title)
        })
        .cloned()
        .collect();

    matched.sort_by_key(|c| kugou_duration_delta(c, duration_seconds));
    if matched.is_empty() {
        return None;
    }

    let viable: Vec<Value> = if has_known_duration {
        matched
            .into_iter()
            .filter(|c| kugou_duration_delta(c, duration_seconds) <= 30_000)
            .collect()
    } else {
        matched
    };

    for candidate in viable.into_iter().take(3) {
        let id = candidate
            .get("id")
            .map(val_to_str)
            .unwrap_or_default();
        let accesskey = candidate
            .get("accesskey")
            .map(val_to_str)
            .unwrap_or_default();
        let Ok(dl_resp) = client
            .get("https://lyrics.kugou.com/download")
            .query(&[
                ("ver", "1"),
                ("client", "pc"),
                ("id", &id),
                ("accesskey", &accesskey),
                ("fmt", "krc"),
                ("charset", "utf8"),
            ])
            .header(USER_AGENT, KUGOU_UA)
            .send()
            .await
        else {
            continue;
        };
        let Ok(dl_json) = dl_resp.json::<Value>().await else {
            continue;
        };
        let Some(raw_base64) = dl_json
            .get("content")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(krc_text) = decrypt_kugou_krc(raw_base64) else {
            continue;
        };
        if !kugou_krc_matches_track(&krc_text, title) {
            continue;
        }
        let lines = parse_kugou_krc(&krc_text);
        if lines.is_empty() || !lyrics_plausible_duration(&lines, duration_seconds) {
            continue;
        }
        let word_synced = lines.iter().any(|l| l.has_syllables());
        let plain_lyrics = lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        return Some(LyricsResult {
            lines,
            is_synced: true,
            is_word_synced: word_synced,
            plain_lyrics,
            is_instrumental: false,
            source: "Kugou KRC (Word-Sync)".to_string(),
        });
    }
    None
}

// -- Musixmatch (apic.musixmatch.com) ----------------------------------------

const MXM_BASE: &str = "https://apic.musixmatch.com/ws/1.1";
const MXM_APP_ID: &str = "web-desktop-app-v1.0";
const MXM_SIGNING_SECRET: &str = "RJDefUswhwjkZDeM";

pub fn musixmatch_sign(url: &str, now: Option<DateTime<Utc>>) -> String {
    let date = now.unwrap_or_else(Utc::now);
    let stamp = date.format("%Y%m%d").to_string();
    let mut mac =
        HmacSha256::new_from_slice(MXM_SIGNING_SECRET.as_bytes()).expect("hmac accepts any key");
    mac.update(format!("{url}{stamp}").as_bytes());
    let signature = BASE64_STANDARD.encode(mac.finalize().into_bytes());
    let encoded = form_encode_component(&signature);
    format!("{url}&signature={encoded}&signature_protocol=sha256")
}

pub fn musixmatch_score(
    track_name: &str,
    artist_name: &str,
    track_length: Option<i64>,
    title: &str,
    artist: &str,
    seconds: i64,
) -> f64 {
    let mut score = 0.0;
    let name = track_name.trim().to_ascii_lowercase();
    let target_title = title.trim().to_ascii_lowercase();
    if name == target_title {
        score += 80.0;
    } else if name.contains(&target_title) || target_title.contains(&name) {
        score += 40.0;
    }
    if artist_name
        .trim()
        .to_ascii_lowercase()
        .contains(&artist.trim().to_ascii_lowercase())
    {
        score += 40.0;
    }
    if let Some(len) = track_length {
        if seconds > 0 {
            let diff = (len - seconds).abs();
            if diff <= 2 {
                score += 30.0;
            } else if diff <= 5 {
                score += 15.0;
            } else if diff <= 10 {
                score += 5.0;
            } else {
                score -= 20.0;
            }
        }
    }
    score
}

pub fn musixmatch_subtitle_to_lrc(subtitle_body: &str) -> String {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(subtitle_body) else {
        return String::new();
    };
    let mut out = String::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let text = obj.get("text").map(val_to_str).unwrap_or_default();
        if text.trim().is_empty() {
            continue;
        }
        let total = obj
            .get("time")
            .and_then(|t| t.get("total"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let total_ms = (total * 1000.0) as i64;
        let minutes = total_ms / 60_000;
        let seconds = (total_ms / 1000) % 60;
        let millis = total_ms % 1000;
        out.push_str(&format!("[{minutes:02}:{seconds:02}.{millis:03}]{text}\n"));
    }
    out.trim().to_string()
}

async fn mxm_get(client: &Client, url: &str) -> Option<String> {
    client
        .get(url)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()
}

fn mxm_unauthorized(body: &str) -> bool {
    let Ok(val) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    let status = val
        .get("message")
        .and_then(|m| m.get("header"))
        .and_then(|h| h.get("status_code"))
        .and_then(Value::as_i64);
    matches!(status, Some(401 | 402))
}

async fn mxm_token_for(client: &Client) -> Option<String> {
    if let Ok(guard) = MXM_TOKEN.lock() {
        if let Some(ref tok) = *guard {
            return Some(tok.clone());
        }
    }
    let url = musixmatch_sign(&format!("{MXM_BASE}/token.get?app_id={MXM_APP_ID}"), None);
    let body = mxm_get(client, &url).await?;
    let val: Value = serde_json::from_str(&body).ok()?;
    let token = val
        .get("message")?
        .get("body")?
        .get("user_token")?
        .as_str()?
        .to_string();
    if token.is_empty() {
        return None;
    }
    if let Ok(mut guard) = MXM_TOKEN.lock() {
        *guard = Some(token.clone());
    }
    Some(token)
}

async fn mxm_signed_get(client: &Client, unsigned_url: &str) -> Option<String> {
    let token = mxm_token_for(client).await?;
    let first = mxm_get(
        client,
        &musixmatch_sign(&format!("{unsigned_url}&usertoken={token}"), None),
    )
    .await;
    if let Some(ref body) = first {
        if !mxm_unauthorized(body) {
            return first;
        }
    }
    if let Ok(mut guard) = MXM_TOKEN.lock() {
        *guard = None;
    }
    let fresh = mxm_token_for(client).await?;
    mxm_get(
        client,
        &musixmatch_sign(&format!("{unsigned_url}&usertoken={fresh}"), None),
    )
    .await
}

pub async fn fetch_musixmatch(
    client: &Client,
    title: &str,
    artist: &str,
    duration_seconds: Option<i64>,
) -> Option<LyricsResult> {
    if title.trim().is_empty() || artist.trim().is_empty() {
        return None;
    }
    let seconds = duration_seconds.unwrap_or(0);
    let query = format!(
        "{MXM_BASE}/track.search?app_id={MXM_APP_ID}&q_track={}&q_artist={}&f_has_lyrics=1&s_track_rating=desc&quorum_factor=1&page_size=10&page=1",
        form_encode_component(title),
        form_encode_component(artist),
    );
    let body = mxm_signed_get(client, &query).await?;
    let val: Value = serde_json::from_str(&body).ok()?;
    let tracks = val
        .get("message")?
        .get("body")?
        .get("track_list")?
        .as_array()?;
    if tracks.is_empty() {
        return None;
    }

    let mut best: Option<&Value> = None;
    let mut best_score = 0.0f64;
    for wrapper in tracks {
        let Some(track) = wrapper.get("track") else {
            continue;
        };
        let track_name = track
            .get("track_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let artist_name = track
            .get("artist_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !lyrics_same_version(title, track_name) {
            continue;
        }
        if !artist.trim().is_empty()
            && !artist_name
                .to_ascii_lowercase()
                .contains(&artist.trim().to_ascii_lowercase())
        {
            continue;
        }
        let score = musixmatch_score(
            track_name,
            artist_name,
            track.get("track_length").and_then(Value::as_i64),
            title,
            artist,
            seconds,
        );
        if score < 80.0 || score <= best_score {
            continue;
        }
        best_score = score;
        best = Some(track);
    }

    let best = best?;
    if best.get("has_subtitles").and_then(Value::as_i64) != Some(1) {
        return None;
    }
    let track_id = best.get("track_id").map(val_to_str)?;
    let sub_url = format!(
        "{MXM_BASE}/track.subtitle.get?app_id={MXM_APP_ID}&track_id={}&subtitle_format=mxm",
        form_encode_component(&track_id)
    );
    let sub_body = mxm_signed_get(client, &sub_url).await?;
    let sub_val: Value = serde_json::from_str(&sub_body).ok()?;
    let subtitle = sub_val
        .get("message")?
        .get("body")?
        .get("subtitle")?
        .get("subtitle_body")?
        .as_str()?;
    if subtitle.is_empty() {
        return None;
    }
    let lrc = musixmatch_subtitle_to_lrc(subtitle);
    if lrc.is_empty() {
        return None;
    }
    let lines = parse_lrc(&lrc);
    if lines.is_empty() || !lyrics_plausible_duration(&lines, duration_seconds) {
        return None;
    }
    let plain_lyrics = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Some(LyricsResult {
        lines,
        is_synced: true,
        is_word_synced: false,
        plain_lyrics,
        is_instrumental: false,
        source: "Catalog (Line-Sync)".to_string(),
    })
}

// -- QQ Karaoke / Portato -----------------------------------------------------

pub async fn fetch_portato_lyrics(
    client: &Client,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_seconds: Option<i64>,
) -> Option<LyricsResult> {
    if title.trim().is_empty() || artist.trim().is_empty() {
        return None;
    }
    let attempts = [
        (title.to_string(), artist.to_string()),
        (lyrics_for_search_title(title), lyrics_for_search_artist(artist)),
    ];
    for (att_title, att_artist) in attempts {
        if att_title.trim().is_empty() || att_artist.trim().is_empty() {
            continue;
        }
        let mut qp = vec![
            ("s", att_title.trim().to_string()),
            ("a", att_artist.trim().to_string()),
        ];
        if let Some(d) = duration_seconds.filter(|&d| d > 0) {
            qp.push(("d", d.to_string()));
        }
        if let Some(al) = album.map(str::trim).filter(|s| !s.is_empty()) {
            qp.push(("al", al.to_string()));
        }
        let Ok(resp) = client
            .get("https://lyrics-api.boidu.dev/qq/getLyrics")
            .query(&qp)
            .header(ACCEPT, "application/json")
            .send()
            .await
        else {
            continue;
        };
        let Ok(body) = resp.text().await else {
            continue;
        };
        if body.is_empty() {
            continue;
        }
        if let Some(parsed) = parse_better_document(&body) {
            if !parsed.is_empty() && lyrics_plausible_duration(&parsed, duration_seconds) {
                let payload = unwrap_better_payload(&body).unwrap_or(body);
                let word_synced = better_has_word_timing(&payload);
                let plain_lyrics = parsed
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                return Some(LyricsResult {
                    lines: parsed,
                    is_synced: true,
                    is_word_synced: word_synced,
                    plain_lyrics,
                    is_instrumental: false,
                    source: if word_synced {
                        "QQ Karaoke (Word-Sync)".to_string()
                    } else {
                        "QQ Karaoke (Line-Sync)".to_string()
                    },
                });
            }
        }
    }
    None
}

// -- Syllable-Sync (Bini / lyrics-api.binimum.org) ----------------------------

pub async fn fetch_bini_lyrics(
    client: &Client,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_seconds: Option<i64>,
    isrc: Option<&str>,
) -> Option<LyricsResult> {
    if title.trim().is_empty() {
        return None;
    }

    async fn query_bini(client: &Client, qp: &[(&str, String)]) -> Vec<Value> {
        let Ok(resp) = client
            .get("https://lyrics-api.binimum.org/")
            .query(qp)
            .header(USER_AGENT, LASTWAVE_UA)
            .header(ACCEPT, "application/json")
            .send()
            .await
        else {
            return Vec::new();
        };
        let Ok(val) = resp.json::<Value>().await else {
            return Vec::new();
        };
        val.get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    let mut hits = Vec::new();
    if let Some(code) = isrc.map(str::trim).filter(|s| !s.is_empty()) {
        hits = query_bini(client, &[("isrc", code.to_string())]).await;
    }
    if hits.is_empty() {
        let mut shaped = vec![
            ("track", title.trim().to_string()),
            ("artist", artist.trim().to_string()),
        ];
        if let Some(al) = album.map(str::trim).filter(|s| !s.is_empty()) {
            shaped.push(("album", al.to_string()));
        }
        if let Some(d) = duration_seconds.filter(|&d| d > 0) {
            shaped.push(("duration", d.to_string()));
        }
        hits = query_bini(client, &shaped).await;
        if hits.is_empty() {
            let q = if artist.trim().is_empty() {
                title.trim().to_string()
            } else {
                format!("{} - {}", artist.trim(), title.trim())
            };
            hits = query_bini(client, &[("q", q)]).await;
        }
    }
    if hits.is_empty() {
        return None;
    }

    let mut best: Option<&Value> = None;
    let mut best_score = -1;
    for hit in &hits {
        let track_name = hit
            .get("track_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let artist_name = hit
            .get("artist_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !lyrics_same_version(title, track_name) {
            continue;
        }
        let mut score = 0;
        let clean_t = lyrics_for_search_title(track_name);
        let req_t = lyrics_for_search_title(title);
        if clean_t.eq_ignore_ascii_case(&req_t) {
            score += 3;
        } else if lyrics_titles_match_strict(&clean_t, &req_t) {
            score += 1;
        }
        let clean_a = lyrics_for_search_artist(artist_name);
        let req_a = lyrics_for_search_artist(artist);
        if lyrics_artists_match_strict(&clean_a, &req_a) {
            score += 2;
        }
        if let (Some(req_d), Some(dur)) = (
            duration_seconds.filter(|&d| d > 0),
            hit.get("duration").and_then(Value::as_i64).filter(|&d| d > 0),
        ) {
            let diff = (dur - req_d).abs();
            if diff <= 3 {
                score += 3;
            } else if diff <= 10 {
                score += 1;
            }
        }
        if hit
            .get("timing_type")
            .and_then(Value::as_str)
            .is_some_and(|t| t.eq_ignore_ascii_case("word"))
        {
            score += 1;
        }
        if score >= 5 && score > best_score {
            best_score = score;
            best = Some(hit);
        }
    }

    let lyrics_url = best?
        .get("lyricsUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let xml = client
        .get(lyrics_url)
        .header(USER_AGENT, LASTWAVE_UA)
        .header(ACCEPT, "application/xml, text/xml, */*")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    if xml.is_empty() {
        return None;
    }
    let lines = parse_ttml(&xml);
    if !lines.is_empty() && lyrics_plausible_duration(&lines, duration_seconds) {
        let word_synced = lines.iter().any(|l| l.has_syllables());
        let plain_lyrics = lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        return Some(LyricsResult {
            lines,
            is_synced: true,
            is_word_synced: word_synced,
            plain_lyrics,
            is_instrumental: false,
            source: "Syllable-Sync".to_string(),
        });
    }
    None
}

// -- LyricsPlus (multi-mirror aggregation) ------------------------------------

const LYRICS_PLUS_MIRRORS: &[&str] = &[
    "https://lyricsplus.prjktla.my.id",
    "https://lyricsplus.atomix.one",
    "https://lyricsplus.binimum.org",
    "https://lyricsplus.prjktla.workers.dev",
    "https://lyricsplus-seven.vercel.app",
    "https://lyrics-plus-backend.vercel.app",
];

fn merge_syllables(raw_syllables: &[Value]) -> Vec<LyricSyllable> {
    let mut words = Vec::new();
    let mut buf = String::new();
    let mut start = 0i64;
    let mut end = 0i64;

    let flush = |words: &mut Vec<LyricSyllable>, buf: &mut String, start: i64, end: i64| {
        if !buf.is_empty() {
            words.push(LyricSyllable {
                time_ms: start,
                duration_ms: if end > start { end - start } else { 0 },
                text: std::mem::take(buf),
                is_background: false,
            });
        }
    };

    for s in raw_syllables {
        let raw = s.get("text").and_then(Value::as_str).unwrap_or("");
        let content = raw.trim();
        if content.is_empty() {
            flush(&mut words, &mut buf, start, end);
            continue;
        }
        let Some(time) = s.get("time").and_then(Value::as_i64) else {
            continue;
        };
        let duration = s.get("duration").and_then(Value::as_i64).unwrap_or(0);
        let joins = !buf.is_empty() && !raw.starts_with(char::is_whitespace);
        if !joins {
            flush(&mut words, &mut buf, start, end);
        }
        if buf.is_empty() {
            start = time;
            end = time;
        }
        buf.push_str(content);
        let word_end = time + duration;
        if word_end > end {
            end = word_end;
        }
        if raw.ends_with(' ') || raw.ends_with('\t') {
            flush(&mut words, &mut buf, start, end);
        }
    }
    flush(&mut words, &mut buf, start, end);
    words
}

pub async fn fetch_lyrics_plus(
    client: &Client,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_seconds: Option<i64>,
    isrc: Option<&str>,
) -> Option<LyricsResult> {
    if title.trim().is_empty() || artist.trim().is_empty() {
        return None;
    }

    for host in LYRICS_PLUS_MIRRORS {
        let mut qp = vec![
            ("title", title.trim().to_string()),
            ("artist", artist.trim().to_string()),
        ];
        if let Some(d) = duration_seconds.filter(|&d| d > 0) {
            qp.push(("duration", d.to_string()));
        }
        if let Some(al) = album.map(str::trim).filter(|s| !s.is_empty()) {
            qp.push(("album", al.to_string()));
        }
        if let Some(code) = isrc.map(str::trim).filter(|s| !s.is_empty()) {
            qp.push(("isrc", code.to_string()));
        }
        let Ok(resp) = client
            .get(format!("{host}/v2/lyrics/get"))
            .query(&qp)
            .header(USER_AGENT, LASTWAVE_UA)
            .header(ACCEPT, "application/json")
            .send()
            .await
        else {
            continue;
        };
        let Ok(data) = resp.json::<Value>().await else {
            continue;
        };
        let Some(raw_lines) = data.get("lyrics").and_then(Value::as_array) else {
            continue;
        };
        let mut lines: Vec<LyricLine> = Vec::new();
        for item in raw_lines {
            let Some(time) = item.get("time").and_then(Value::as_i64) else {
                continue;
            };
            let duration = item.get("duration").and_then(Value::as_i64).unwrap_or(0);
            let words = item
                .get("syllabus")
                .and_then(Value::as_array)
                .map(|arr| merge_syllables(arr))
                .unwrap_or_default();
            let raw_item_text = item
                .get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            let transliteration = item
                .get("romanizedText")
                .or_else(|| item.get("transliteration"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    let syls = item.get("syllabus").and_then(Value::as_array)?;
                    let joined: String = syls
                        .iter()
                        .filter_map(|s| s.get("romanizedText").and_then(Value::as_str))
                        .collect();
                    let t = joined.trim().to_string();
                    if t.is_empty() { None } else { Some(t) }
                })
                .unwrap_or_default();
            let agent = if item.get("oppositeTurn").and_then(Value::as_bool) == Some(true) {
                Some("v2".to_string())
            } else {
                item.get("element")
                    .and_then(|e| e.get("songPart").or_else(|| e.get("singer")))
                    .and_then(Value::as_str)
                    .filter(|s| s.eq_ignore_ascii_case("v2"))
                    .map(|_| "v2".to_string())
            };
            if !raw_item_text.is_empty()
                && is_lyric_credit_line(raw_item_text, lines.len(), raw_lines.len(), title, artist)
            {
                continue;
            }
            if !words.is_empty() {
                let joined_words = words
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if is_lyric_credit_line(&joined_words, lines.len(), raw_lines.len(), title, artist)
                {
                    continue;
                }
                let first_ms = words[0].time_ms;
                lines.push(LyricLine {
                    time_ms: time.min(first_ms),
                    duration_ms: duration.max(0),
                    text: joined_words,
                    syllables: words,
                    transliteration,
                    agent,
                });
            } else if !raw_item_text.is_empty() {
                lines.push(LyricLine {
                    time_ms: time,
                    duration_ms: duration.max(0),
                    text: raw_item_text.to_string(),
                    syllables: Vec::new(),
                    transliteration,
                    agent,
                });
            }
        }
        lines.sort_by_key(|l| l.time_ms);
        if !lines.is_empty() && lyrics_plausible_duration(&lines, duration_seconds) {
            let word_synced = lines.iter().any(|l| l.has_syllables());
            let plain_lyrics = lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            return Some(LyricsResult {
                lines,
                is_synced: true,
                is_word_synced: word_synced,
                plain_lyrics,
                is_instrumental: false,
                source: if word_synced {
                    "LyricsPlus (Word-Sync)".to_string()
                } else {
                    "LyricsPlus (Line-Sync)".to_string()
                },
            });
        }
    }
    None
}

// -- Unison (unison.boidu.dev) ------------------------------------------------

pub async fn fetch_unison_lyrics(
    client: &Client,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_seconds: Option<i64>,
) -> Option<LyricsResult> {
    if title.trim().is_empty() || artist.trim().is_empty() {
        return None;
    }
    let mut qp = vec![
        ("song", title.trim().to_string()),
        ("artist", artist.trim().to_string()),
    ];
    if let Some(al) = album.map(str::trim).filter(|s| !s.is_empty()) {
        qp.push(("album", al.to_string()));
    }
    if let Some(d) = duration_seconds.filter(|&d| d > 0) {
        qp.push(("duration", d.to_string()));
    }
    let data = client
        .get("https://unison.boidu.dev/lyrics")
        .query(&qp)
        .header(USER_AGENT, LASTWAVE_UA)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .ok()?
        .json::<Value>()
        .await
        .ok()?;
    if data.get("success").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let entry = data.get("data")?.as_object()?;
    let lyrics_text = entry.get("lyrics").and_then(Value::as_str).unwrap_or("");
    if lyrics_text.trim().is_empty() {
        return None;
    }
    let format = entry
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let sync_type = entry
        .get("syncType")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();

    let lines = if format == "ttml" {
        parse_ttml(lyrics_text)
    } else if sync_type == "plain" {
        Vec::new()
    } else {
        parse_lrc(lyrics_text)
    };
    if lines.is_empty() || !lyrics_plausible_duration(&lines, duration_seconds) {
        return None;
    }
    let word_synced = if format == "ttml" {
        lines.iter().any(|l| l.has_syllables())
    } else {
        sync_type == "word"
            || sync_type == "syllable"
            || !parse_enhanced_lrc(lyrics_text).is_empty()
    };
    let clean_lines: Vec<LyricLine> = if word_synced {
        lines
    } else {
        lines
            .into_iter()
            .map(|l| LyricLine {
                time_ms: l.time_ms,
                duration_ms: l.duration_ms,
                text: l.text,
                syllables: Vec::new(),
                transliteration: l.transliteration,
                agent: l.agent,
            })
            .collect()
    };
    let plain_lyrics = clean_lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Some(LyricsResult {
        lines: clean_lines,
        is_synced: true,
        is_word_synced: word_synced,
        plain_lyrics,
        is_instrumental: false,
        source: if word_synced {
            "Unison (Word-Sync)".to_string()
        } else {
            "Unison (Line-Sync)".to_string()
        },
    })
}

// -- Megalobiz (megalobiz.com) ------------------------------------------------

pub async fn fetch_megalobiz_lyrics(
    client: &Client,
    title: &str,
    artist: &str,
    duration_seconds: Option<i64>,
) -> Option<LyricsResult> {
    if title.trim().is_empty() {
        return None;
    }
    let qry = format!("{artist} {title}").trim().to_string();
    let html = client
        .get("https://www.megalobiz.com/searchall")
        .query(&[("qry", qry)])
        .header(USER_AGENT, LASTWAVE_UA)
        .header(
            ACCEPT,
            "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8",
        )
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let path = MEGALOBIZ_LINK_RE
        .captures(&html)?
        .get(1)?
        .as_str()
        .replace("&amp;", "&");
    let page_html = client
        .get(format!("https://www.megalobiz.com{path}"))
        .header(USER_AGENT, LASTWAVE_UA)
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let raw_lrc = MEGALOBIZ_BODY_RE.captures(&page_html)?.get(1)?.as_str();
    let with_newlines = BR_TAG_RE.replace_all(raw_lrc, "\n");
    let stripped = HTML_TAG_RE.replace_all(&with_newlines, "");
    let unescaped = unescape_html(&stripped);
    let lines = parse_lrc(&unescaped);
    if !lines.is_empty() && lyrics_plausible_duration(&lines, duration_seconds) {
        let plain_lyrics = lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        return Some(LyricsResult {
            lines,
            is_synced: true,
            is_word_synced: false,
            plain_lyrics,
            is_instrumental: false,
            source: "Megalobiz (Line-Sync)".to_string(),
        });
    }
    None
}

// -- Genius (genius.com) ------------------------------------------------------

pub async fn fetch_genius_lyrics(
    client: &Client,
    title: &str,
    artist: &str,
) -> Option<LyricsResult> {
    if title.trim().is_empty() {
        return None;
    }
    let query = if artist.trim().is_empty() {
        title.trim().to_string()
    } else {
        format!("{} {}", artist.trim(), title.trim())
    };
    let data = client
        .get("https://genius.com/api/search/multi")
        .query(&[("q", query)])
        .header(USER_AGENT, GENIUS_UA)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .ok()?
        .json::<Value>()
        .await
        .ok()?;
    let sections = data
        .get("response")?
        .get("sections")?
        .as_array()?;
    let song_section = sections
        .iter()
        .find(|s| s.get("type").and_then(Value::as_str) == Some("song"))?;
    let hits = song_section.get("hits")?.as_array()?;
    let song_url = hits
        .iter()
        .filter_map(|h| h.get("result")?.get("url")?.as_str())
        .find(|u| !u.is_empty())?;
    let html = client
        .get(song_url)
        .header(USER_AGENT, GENIUS_UA)
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let mut chunks = Vec::new();
    for caps in GENIUS_CONTAINER_RE.captures_iter(&html) {
        let raw = caps.get(1).map_or("", |m| m.as_str());
        let with_nl = BR_TAG_RE.replace_all(raw, "\n");
        let stripped = HTML_TAG_RE.replace_all(&with_nl, "");
        let unescaped = unescape_html(&stripped);
        let trimmed = unescaped.trim();
        if !trimmed.is_empty() {
            chunks.push(trimmed.to_string());
        }
    }
    let text = chunks.join("\n\n");
    if text.trim().is_empty() {
        return None;
    }
    let lines: Vec<LyricLine> = text
        .split('\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| LyricLine {
            time_ms: 0,
            text: l.to_string(),
            ..Default::default()
        })
        .collect();
    Some(LyricsResult {
        lines,
        is_synced: false,
        is_word_synced: false,
        plain_lyrics: text,
        is_instrumental: false,
        source: "Genius (Plain)".to_string(),
    })
}

// -- LastWave (lyrics-api.boidu.dev/lastwave) ---------------------------------

pub async fn fetch_lastwave_lyrics(
    client: &Client,
    title: &str,
    artist: &str,
    album: Option<&str>,
    duration_seconds: Option<i64>,
    isrc: Option<&str>,
) -> Option<LyricsResult> {
    let mut qp = vec![("title", title.trim().to_string())];
    if !artist.trim().is_empty() {
        qp.push(("artist", artist.trim().to_string()));
    }
    if let Some(al) = album.map(str::trim).filter(|s| !s.is_empty()) {
        qp.push(("album", al.to_string()));
    }
    if let Some(d) = duration_seconds.filter(|&d| d > 0) {
        qp.push(("duration", d.to_string()));
    }
    if let Some(code) = isrc.map(str::trim).filter(|s| !s.is_empty()) {
        qp.push(("isrc", code.to_string()));
    }
    let body = client
        .get("https://lyrics-api.boidu.dev/lastwave/lyrics")
        .query(&qp)
        .header(USER_AGENT, LASTWAVE_UA)
        .header(ACCEPT, "application/json, text/plain")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    if body.trim().is_empty() {
        return None;
    }
    let payload = unwrap_better_payload(&body).unwrap_or_else(|| body.trim().to_string());
    let lines = parse_better_document(&payload)?;
    if lines.is_empty() || !lyrics_plausible_duration(&lines, duration_seconds) {
        return None;
    }
    let word_synced = better_has_word_timing(&payload);
    let plain_lyrics = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Some(LyricsResult {
        lines,
        is_synced: true,
        is_word_synced: word_synced,
        plain_lyrics,
        is_instrumental: false,
        source: if word_synced {
            "LastWave (Word-Sync)".to_string()
        } else {
            "LastWave (Line-Sync)".to_string()
        },
    })
}

fn unescape_html(raw: &str) -> String {
    raw.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

fn val_to_str(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn form_encode_component(raw: &str) -> String {
    let mut url = Url::parse("https://example.invalid/").expect("valid base");
    url.query_pairs_mut().append_pair("v", raw);
    let q = url.query().unwrap_or("");
    q.strip_prefix("v=").unwrap_or(q).to_string()
}

/// Placeholder helper used only to keep `HashMap` imported cleanly if callers
/// inspect provider metadata by id.
pub fn provider_map() -> HashMap<&'static str, LyricsProviderId> {
    LyricsProviderId::ALL
        .iter()
        .map(|&p| (p.id(), p))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use chrono::TimeZone;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    #[test]
    fn provider_id_and_selection_helpers() {
        assert_eq!(LyricsProviderId::from_id(Some("kugou")), LyricsProviderId::Kugou);
        assert_eq!(LyricsProviderId::from_id(Some("missing")), LyricsProviderId::Auto);
        assert!(LyricsProviderId::Kugou.is_word_provider());
        assert!(!LyricsProviderId::Lrclib.is_word_provider());
        assert_eq!(
            lyrics_source_to_provider_id("BetterLyrics (Word-Sync)"),
            Some("better_lyrics")
        );

        let cur_ex = HashSet::new();
        let (ovr, ex) = next_lyrics_selection(Some("kugou"), Some("lrclib"), &cur_ex);
        assert_eq!(ovr.as_deref(), Some("kugou"));
        assert!(ex.contains("lrclib"));
    }

    #[test]
    fn parse_better_karaoke_and_envelope() {
        let wrapped = r#"{"ttml":"[1000,2000](1000,500)Hello (1500,500)world"}"#;
        let lines = parse_better_document(wrapped).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello world");
        assert_eq!(lines[0].syllables.len(), 2);
        assert!(better_has_word_timing(
            "[1000,2000](1000,500)Hello (1500,500)world"
        ));
    }

    #[test]
    fn kugou_decrypt_and_parse_krc() {
        let krc_plain = "[offset:0]\n[1000,2000]<0,500,0>Hel<500,500,0>lo";
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(krc_plain.as_bytes()).unwrap();
        let compressed = enc.finish().unwrap();

        let mut raw = vec![b'k', b'r', b'c', b'1'];
        for (i, b) in compressed.iter().enumerate() {
            raw.push(b ^ KUGOU_KEY[i % KUGOU_KEY.len()]);
        }
        let b64 = STANDARD.encode(raw);
        let decrypted = decrypt_kugou_krc(&b64).unwrap();
        assert_eq!(decrypted, krc_plain);

        let lines = parse_kugou_krc(&decrypted);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello");
        assert_eq!(lines[0].syllables.len(), 2);
        assert!(kugou_krc_matches_track("[ti:Song]\n[1000,2000]hi", "Song"));
        assert!(!kugou_krc_matches_track(
            "[ti:Song (Mixed)]\n[1000,2000]hi",
            "Song"
        ));
    }

    #[test]
    fn musixmatch_sign_and_subtitle_to_lrc() {
        let dt = Utc.with_ymd_and_hms(2026, 10, 6, 12, 0, 0).unwrap();
        let signed = musixmatch_sign("https://apic.musixmatch.com/ws/1.1/token.get?app_id=x", Some(dt));
        assert!(signed.contains("&signature="));
        assert!(signed.ends_with("&signature_protocol=sha256"));

        let lrc = musixmatch_subtitle_to_lrc(
            r#"[{"text":"Line one","time":{"total":12.345}},{"text":"  ","time":{"total":15.0}}]"#,
        );
        assert_eq!(lrc, "[00:12.345]Line one");
    }

    #[test]
    fn lrc_red_selection_prefers_exact_word_match() {
        let hits = vec![
            LrcRedHit {
                track_name: Some("Song".to_string()),
                artist_name: Some("Artist".to_string()),
                duration: Some(200),
                timing_type: Some("line".to_string()),
                ..Default::default()
            },
            LrcRedHit {
                track_name: Some("Song".to_string()),
                artist_name: Some("Artist".to_string()),
                duration: Some(200),
                timing_type: Some("word".to_string()),
                ..Default::default()
            },
        ];
        let best = select_lrc_red_best(&hits, "Song", "Artist", Some(200)).unwrap();
        assert_eq!(best.timing_type.as_deref(), Some("word"));
    }
}
