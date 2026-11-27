//! Lyrics orchestrator with Apple Music word-by-word, multi-provider wave
//! racing, LRCLIB fallback, and memory + SQLite caching.
//!
//! Ported from `lib/features/lyrics/lyrics_repository.dart`.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use futures_util::stream::{FuturesUnordered, StreamExt};
use regex::{Regex, RegexBuilder};
use serde_json::Value;
use tokio::sync::Mutex;
use unicode_normalization::UnicodeNormalization;

use crate::extractor::http_client::HttpClientFactory;
use crate::extractor::innertube_api::InnerTubeMusicApi;
use crate::models::lyrics_models::{
    expand_packed_lyric_lines, lyrics_body_length, lyrics_plausible_duration, lyrics_same_version,
    normalize_karaoke_timings, parse_enhanced_lrc, parse_lrc, parse_lyric_timestamp_ms,
    strip_leading_artist_prefix, strip_lyric_credit_lines, LyricLine, LyricSyllable, LyricsResult,
};
use crate::storage::app_database::AppDatabase;
use crate::storage::lyrics_snapshot::{
    decode_lyrics, encode_lyrics, LYRICS_CACHE_KEY_PREFIX, LYRICS_CACHE_MAX_AGE,
};

use super::lyrics_providers::{
    fetch_better_lyrics, fetch_bini_lyrics, fetch_genius_lyrics, fetch_kugou,
    fetch_lastwave_lyrics, fetch_lrc_red, fetch_lyrics_plus, fetch_megalobiz_lyrics,
    fetch_musixmatch, fetch_portato_lyrics, fetch_unison_lyrics, LyricsProviderId,
};

const DESKTOP_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

static CLEAN_PAREN_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\s*\([^)]*(?:official|video|audio|remaster|feat\b|ft\.?|featuring\b|live|version|edit|visualizer|lyrics?|hd|hq|4k|1080p|mv|m/v|full\s*song|full\s*video|full\s*audio|explicit|clean|deluxe|bonus|with|prod)[^)]*\)",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static CLEAN_BRACKET_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\s*\[[^\]]*(?:official|video|audio|remaster|feat\b|ft\.?|featuring\b|live|version|edit|visualizer|lyrics?|hd|hq|4k|1080p|mv|m/v|full\s*song|full\s*video|full\s*audio|explicit|clean|deluxe|bonus|with|prod)[^\]]*\]",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static CLEAN_DASH_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\s*[-–—|/:]\s*(?:official|video|audio|remaster|live|remastered|lyrics?|hd|hq|4k|1080p|mv|m/v|deluxe|bonus|visualizer|music\s*video|lyric\s*video|full\s*(?:song|video|audio)|prod).*$",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static CLEAN_FEAT_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\s+(?:feat\.?|ft\.?|featuring)\b.*$")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static CLEAN_TRAILING_PUNCT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\s\-–—|/:]+$").unwrap());

static ARTIST_TOPIC_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\s*-\s*Topic$")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static ARTIST_PARENS_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\s*[\(\[]\s*(?:feat\.?|ft\.?|featuring\b|with\b|prod\.?|produced by\b)[^\)\]]*[\)\]]",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static ARTIST_FEAT_TAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\s*(?:feat\.?|ft\.?|featuring\b|with\b).*$")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static ARTIST_ENTITY_SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\s*[,/;&+]\s*|\s+(?:feat\.?|ft\.?|featuring|with|vs\.?|x)\s+")
        .case_insensitive(true)
        .build()
        .unwrap()
});

static NORM_APOSTROPHES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\u{2018}\u{2019}\u{201A}\u{201B}`']").unwrap());
static NORM_QUOTES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[\u{201C}\u{201D}\u{201E}\u{201F}"]"#).unwrap());
static NORM_DASHES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\u{2013}\u{2014}\u{2212}-]").unwrap());
static DOLLAR_DIGIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$(\d)").unwrap());
static BRACKETS_ANY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]").unwrap());
static FEAT_DOT_TAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:feat\.?|ft\.?|featuring)\b.*$").unwrap());
static EMPTY_PARENS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(\s*\)").unwrap());
static TRAILING_OPEN_PAREN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\(\[]\s*$").unwrap());
static NON_WORD_PAREN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\w\s()&]").unwrap());
static MULTI_WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static PAREN_OR_BRACKET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*[\(\[].*?[\)\]]").unwrap());
static PACKAGING_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\b(official|video|audio|remaster(?:ed)?|feat|ft|featuring|with|prod|live|version|edit|visualizer|lyrics?|hd|hq|4k|mv|deluxe|bonus|clean|explicit|album|single|original)\b",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static NON_WORD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\w]").unwrap());
static GENERIC_TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(intro|outro|interlude|untitled|instrumental)$").unwrap());

fn strip_combining_marks(s: &str) -> String {
    s.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect()
}

pub fn clean_song_title(title: &str) -> String {
    let mut t = CLEAN_PAREN_RE.replace_all(title, "").into_owned();
    t = CLEAN_BRACKET_RE.replace_all(&t, "").into_owned();
    t = CLEAN_DASH_RE.replace_all(&t, "").into_owned();
    t = CLEAN_FEAT_TAIL_RE.replace_all(&t, "").into_owned();
    let trimmed = CLEAN_TRAILING_PUNCT_RE.replace_all(&t, "").trim().to_string();
    if trimmed.is_empty() {
        title.to_string()
    } else {
        trimmed
    }
}

pub fn clean_song_artist(artist: &str) -> String {
    let mut a = ARTIST_TOPIC_RE.replace_all(artist, "").into_owned();
    a = ARTIST_PARENS_RE.replace_all(&a, "").into_owned();
    a = ARTIST_FEAT_TAIL_RE.replace_all(&a, "").into_owned();
    let trimmed = a.trim();
    if trimmed.is_empty() {
        artist.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn normalize_lyrics_title(s: &str) -> String {
    let mut t = NORM_APOSTROPHES_RE.replace_all(s, "").into_owned();
    t = NORM_QUOTES_RE.replace_all(&t, " ").into_owned();
    t = NORM_DASHES_RE.replace_all(&t, " ").into_owned();
    t = strip_combining_marks(&t.to_lowercase());
    t = DOLLAR_DIGIT_RE.replace_all(&t, "$1").into_owned();
    t = t.replace('$', "s");
    t = BRACKETS_ANY_RE.replace_all(&t, " ").into_owned();
    t = FEAT_DOT_TAIL_RE.replace_all(&t, " ").into_owned();
    t = EMPTY_PARENS_RE.replace_all(&t, " ").into_owned();
    t = TRAILING_OPEN_PAREN_RE.replace_all(&t, " ").into_owned();
    t = NON_WORD_PAREN_RE.replace_all(&t, " ").into_owned();
    MULTI_WS_RE.replace_all(&t, " ").trim().to_string()
}

fn lyrics_core_title(normalized: &str) -> String {
    let s = PAREN_OR_BRACKET_RE.replace_all(normalized, " ");
    MULTI_WS_RE.replace_all(&s, " ").trim().to_string()
}

pub fn lyrics_titles_match(query: &str, candidate: &str) -> bool {
    if !lyrics_same_version(query, candidate) {
        return false;
    }

    let clean_q = clean_song_title(query);
    let clean_c = clean_song_title(candidate);
    let mut q = normalize_lyrics_title(&clean_q);
    let mut c = normalize_lyrics_title(&clean_c);
    if q.is_empty() || c.is_empty() {
        return false;
    }
    if q == c {
        return true;
    }

    let mut q_core = lyrics_core_title(&q);
    let mut c_core = lyrics_core_title(&c);
    if q_core.is_empty() || c_core.is_empty() || q_core != c_core {
        let q_strip = normalize_lyrics_title(&strip_leading_artist_prefix(&clean_q));
        let c_strip = normalize_lyrics_title(&strip_leading_artist_prefix(&clean_c));
        if q_strip != q || c_strip != c {
            let q_strip_core = lyrics_core_title(&q_strip);
            let c_strip_core = lyrics_core_title(&c_strip);
            if !q_strip_core.is_empty() && q_strip_core == c_strip_core {
                q = q_strip;
                c = c_strip;
                q_core = q_strip_core;
                c_core = c_strip_core;
            } else {
                return false;
            }
        } else {
            return false;
        }
    }

    let strip_packaging = |s: &str| -> String {
        let low = s.to_lowercase();
        let v = PACKAGING_RE.replace_all(&low, " ");
        NON_WORD_RE.replace_all(&v, "").trim().to_string()
    };

    let q_extra = strip_packaging(&q.replace(&q_core, ""));
    let c_extra = strip_packaging(&c.replace(&c_core, ""));
    if !q_extra.is_empty() && c_extra.is_empty() {
        return false;
    }
    if q_extra.is_empty() && !c_extra.is_empty() {
        return false;
    }
    if !q_extra.is_empty() && !c_extra.is_empty() && q_extra != c_extra {
        return false;
    }
    true
}

fn normalize_lyrics_artist(s: &str) -> String {
    let mut v = NORM_APOSTROPHES_RE.replace_all(s, "").into_owned();
    v = NORM_QUOTES_RE.replace_all(&v, " ").into_owned();
    v = NORM_DASHES_RE.replace_all(&v, " ").into_owned();
    v = strip_combining_marks(&v.to_lowercase());
    v = DOLLAR_DIGIT_RE.replace_all(&v, "$1").into_owned();
    v.replace('$', "s")
}

fn extract_artist_entities(raw_artist: &str) -> Vec<String> {
    let cleaned = clean_song_artist(raw_artist);
    let normalized = normalize_lyrics_artist(&cleaned).trim().to_string();
    let mut entities = Vec::new();
    let mut seen = HashSet::new();
    if !normalized.is_empty() && seen.insert(normalized.clone()) {
        entities.push(normalized.clone());
    }
    for p in ARTIST_ENTITY_SPLIT_RE.split(&normalized) {
        let t = p.trim();
        if !t.is_empty() && seen.insert(t.to_string()) {
            entities.push(t.to_string());
        }
    }
    entities
}

pub fn lyrics_artists_match(query: &str, candidate: &str) -> bool {
    let q = normalize_lyrics_artist(&clean_song_artist(query))
        .trim()
        .to_string();
    let c = normalize_lyrics_artist(&clean_song_artist(candidate))
        .trim()
        .to_string();
    if q.is_empty() || c.is_empty() {
        return false;
    }
    if q == c {
        return true;
    }

    let q_entities = extract_artist_entities(query);
    let c_entities = extract_artist_entities(candidate);
    for qe in &q_entities {
        if qe.len() < 2 {
            continue;
        }
        for ce in &c_entities {
            if ce.len() >= 2 && qe == ce {
                return true;
            }
        }
    }

    let (shorter, longer) = if q.len() <= c.len() { (&q, &c) } else { (&c, &q) };
    if shorter.len() >= 2 {
        let escaped = regex::escape(shorter);
        let pattern = format!(
            r"(^|(?:\s*[,&/;+]\s*|\s+(?:with|feat\.?|ft\.?|featuring|vs\.?|x)\s+)){escaped}($|(?:\s*[,&/;+]\s*|\s+(?:with|feat\.?|ft\.?|featuring|vs\.?|x)\s+))"
        );
        if let Ok(re) = RegexBuilder::new(&pattern).case_insensitive(true).build() {
            if re.is_match(longer) {
                return true;
            }
        }
    }
    false
}

pub fn lyrics_is_alternate_recording(title: &str, album: &str) -> bool {
    let blob = format!("{} {}", title.to_lowercase(), album.to_lowercase());
    const ALT: &[&str] = &[
        "instrumental",
        "karaoke",
        "a cappella",
        "acapella",
        "minus one",
        "backing track",
    ];
    ALT.iter().any(|w| blob.contains(w))
}

pub fn lyrics_duration_plausible(query_seconds: Option<i64>, candidate_seconds: Option<f64>) -> bool {
    let Some(q) = query_seconds.filter(|&s| s > 0) else {
        return true;
    };
    let Some(c_raw) = candidate_seconds else {
        return true;
    };
    let candidate = c_raw.round() as i64;
    if candidate <= 0 {
        return true;
    }
    let diff = (candidate - q).abs();
    if diff <= 25 {
        return true;
    }
    diff <= 45 && (diff as f64 / q as f64 <= 0.18)
}

pub fn lyrics_unique_line_count(result: &LyricsResult) -> usize {
    let mut seen = HashSet::new();
    for line in &result.lines {
        let n = normalize_lyrics_title(&line.text);
        if !n.is_empty() {
            seen.insert(n);
        }
    }
    if seen.is_empty() {
        for line in result.plain_lyrics.lines() {
            let n = normalize_lyrics_title(line);
            if !n.is_empty() {
                seen.insert(n);
            }
        }
    }
    seen.len()
}

fn lyrics_non_empty_line_count(result: &LyricsResult) -> usize {
    let from_lines = result
        .lines
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .count();
    if from_lines > 0 {
        return from_lines;
    }
    result
        .plain_lyrics
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

pub fn lyrics_looks_like_thin_loop(result: &LyricsResult) -> bool {
    if result.is_instrumental {
        return false;
    }
    let unique = lyrics_unique_line_count(result);
    let total = lyrics_non_empty_line_count(result);
    if total < 10 {
        return false;
    }
    unique <= 8 && (unique as f64 / total as f64) <= 0.45
}

pub fn lyrics_looks_like_placeholder(result: &LyricsResult) -> bool {
    let mut texts: Vec<String> = result
        .lines
        .iter()
        .map(|l| l.text.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if texts.is_empty() {
        texts = result
            .plain_lyrics
            .lines()
            .map(|l| l.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
    }
    if texts.is_empty() {
        return false;
    }
    texts.iter().all(|t| {
        matches!(
            t.as_str(),
            "instrumental" | "[instrumental]" | "(instrumental)" | "♪" | "♫"
        )
    })
}

pub fn lyrics_mentions_title(title: &str, result: &LyricsResult) -> bool {
    let core = lyrics_core_title(&normalize_lyrics_title(&clean_song_title(title)));
    if core.len() < 6 || GENERIC_TITLE_RE.is_match(&core) {
        return false;
    }
    let joined_lines: String = result
        .lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let blob = normalize_lyrics_title(&format!("{joined_lines} {}", result.plain_lyrics));
    blob.contains(&core)
}

pub fn lyrics_for_display_mode(result: &LyricsResult, word_by_word: bool) -> LyricsResult {
    if word_by_word || !result.is_word_synced {
        return result.clone();
    }
    LyricsResult {
        lines: result
            .lines
            .iter()
            .map(|line| LyricLine {
                time_ms: line.time_ms,
                duration_ms: line.duration_ms,
                text: line.text.clone(),
                syllables: Vec::new(),
                transliteration: line.transliteration.clone(),
                agent: line.agent.clone(),
            })
            .collect(),
        is_synced: result.is_synced,
        is_word_synced: false,
        plain_lyrics: result.plain_lyrics.clone(),
        is_instrumental: result.is_instrumental,
        source: result.source.clone(),
    }
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Testability seam for overriding provider dispatch in unit tests.
pub trait LyricsProviderFetcher: Send + Sync {
    fn fetch_from_provider<'a>(
        &'a self,
        provider_id: &'a str,
        title: &'a str,
        artist: &'a str,
        album: &'a str,
        duration_seconds: Option<i64>,
        video_id: Option<&'a str>,
    ) -> BoxFuture<'a, Option<LyricsResult>>;
}

const MAX_CACHE_ENTRIES: usize = 50;

const RACE_WAVES: &[&[&str]] = &[
    &["lastwave", "apple_music", "lrclib"],
    &["lrc_red", "better_lyrics", "lyrics_plus", "unison"],
    &["better_lyrics_portato", "bini_lyrics", "kugou"],
];

#[derive(Default)]
struct MemoryLru {
    map: HashMap<String, LyricsResult>,
    order: Vec<String>,
}

impl MemoryLru {
    fn touch(&mut self, key: &str) -> Option<LyricsResult> {
        let hit = self.map.get(key)?.clone();
        self.order.retain(|k| k != key);
        self.order.push(key.to_string());
        Some(hit)
    }

    fn insert(&mut self, key: String, value: LyricsResult) {
        self.order.retain(|k| k != &key);
        self.map.insert(key.clone(), value);
        self.order.push(key);
        while self.order.len() > MAX_CACHE_ENTRIES {
            let oldest = self.order.remove(0);
            self.map.remove(&oldest);
        }
    }
}

pub struct LyricsRepository {
    http: reqwest::Client,
    tube: Option<Arc<InnerTubeMusicApi>>,
    db: Option<Arc<AppDatabase>>,
    preferred_head_start: Duration,
    race_total: Duration,
    wave_gap: Duration,
    cache: Mutex<MemoryLru>,
    custom_fetcher: Option<Arc<dyn LyricsProviderFetcher>>,
}

impl LyricsRepository {
    pub fn new(
        http: Option<reqwest::Client>,
        tube: Option<Arc<InnerTubeMusicApi>>,
        db: Option<Arc<AppDatabase>>,
    ) -> Self {
        Self {
            http: http.unwrap_or_else(|| {
                HttpClientFactory::create().unwrap_or_else(|_| reqwest::Client::new())
            }),
            tube,
            db,
            preferred_head_start: Duration::from_secs(4),
            race_total: Duration::from_secs(12),
            wave_gap: Duration::from_millis(1500),
            cache: Mutex::new(MemoryLru::default()),
            custom_fetcher: None,
        }
    }

    pub fn with_tuning(
        mut self,
        preferred_head_start: Duration,
        race_total: Duration,
        wave_gap: Duration,
        custom_fetcher: Option<Arc<dyn LyricsProviderFetcher>>,
    ) -> Self {
        self.preferred_head_start = preferred_head_start;
        self.race_total = race_total;
        self.wave_gap = wave_gap;
        self.custom_fetcher = custom_fetcher;
        self
    }

    fn key(title: &str, artist: &str, album: &str) -> String {
        format!(
            "{}|{}|{}",
            title.to_lowercase(),
            artist.to_lowercase(),
            album.to_lowercase()
        )
    }

    fn read_disk(&self, key: &str) -> Option<LyricsResult> {
        let db = self.db.as_ref()?;
        let row = db.cache_get(&format!("{LYRICS_CACHE_KEY_PREFIX}{key}"), None)?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        if now_ms - row.saved_at_ms > LYRICS_CACHE_MAX_AGE.as_millis() as i64 {
            return None;
        }
        decode_lyrics(&row.value)
    }

    fn write_disk(&self, key: &str, value: &LyricsResult) {
        if let Some(ref db) = self.db {
            db.cache_put(
                &format!("{LYRICS_CACHE_KEY_PREFIX}{key}"),
                "lyrics",
                &encode_lyrics(value),
                None,
            );
        }
    }

    async fn store(&self, key: &str, value: &LyricsResult, query_title: &str) {
        if value.is_empty() {
            return;
        }
        let existing = {
            let mem = self.cache.lock().await;
            mem.map.get(key).cloned()
        }
        .or_else(|| self.read_disk(key));

        if let Some(ex) = existing.filter(|e| !e.is_empty()) {
            if Self::is_better_candidate(&ex, value, query_title) {
                return;
            }
        }
        self.cache.lock().await.insert(key.to_string(), value.clone());
        self.write_disk(key, value);
    }

    async fn lookup(&self, key: &str) -> Option<LyricsResult> {
        if let Some(hit) = self.cache.lock().await.touch(key) {
            return Some(hit);
        }
        if let Some(disk_hit) = self.read_disk(key) {
            self.cache.lock().await.insert(key.to_string(), disk_hit.clone());
            return Some(disk_hit);
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn get_lyrics(
        &self,
        title: &str,
        artist: &str,
        album: &str,
        duration_seconds: Option<i64>,
        force_refresh: bool,
        word_by_word: bool,
        video_id: Option<&str>,
        preferred_provider_id: &str,
        exclude_provider_ids: &HashSet<String>,
    ) -> LyricsResult {
        let preferred = LyricsProviderId::from_id(Some(preferred_provider_id));
        let mut sorted_excludes: Vec<&str> =
            exclude_provider_ids.iter().map(String::as_str).collect();
        sorted_excludes.sort_unstable();
        let key = format!(
            "{}|{}|{}|{}|{}",
            Self::key(title, artist, album),
            duration_seconds.unwrap_or(0),
            word_by_word,
            preferred.id(),
            sorted_excludes.join(",")
        );

        if !force_refresh {
            if let Some(raw_cached) = self.lookup(&key).await {
                let cached = strip_lyric_credit_lines(&raw_cached, title, artist);
                if !word_by_word || cached.is_word_synced || cached.is_instrumental {
                    return lyrics_for_display_mode(&cached, word_by_word);
                }
            }
        }

        let mut line_fallback: Option<LyricsResult> = None;
        let wants_alt = lyrics_is_alternate_recording(title, album);

        let normalize_res = |res: Option<LyricsResult>| -> Option<LyricsResult> {
            let r = res?;
            if r.is_empty() {
                return None;
            }
            Some(normalize_karaoke_timings(&r, title, artist))
        };

        let plausible = |res: &LyricsResult| -> bool {
            res.is_instrumental || lyrics_plausible_duration(&res.lines, duration_seconds)
        };

        let mut lrclib_attempted = false;
        let mut preferred_fallback: Option<LyricsResult> = None;

        if preferred != LyricsProviderId::Auto
            && !preferred.is_word_provider()
            && !exclude_provider_ids.contains(preferred.id())
        {
            if preferred == LyricsProviderId::Lrclib {
                lrclib_attempted = true;
            }
            if let Some(first) = normalize_res(
                self.attempt_provider(
                    preferred.id(),
                    title,
                    artist,
                    album,
                    duration_seconds,
                    video_id,
                    exclude_provider_ids,
                )
                .await,
            ) {
                if first.is_word_synced
                    || first.is_synced
                    || (first.is_instrumental && wants_alt)
                    || !word_by_word
                {
                    self.store(&key, &first, title).await;
                    return lyrics_for_display_mode(&first, word_by_word);
                }
                if plausible(&first) {
                    preferred_fallback = Some(first.clone());
                    line_fallback = Some(first);
                }
            }
        }

        if preferred.is_word_provider() && !exclude_provider_ids.contains(preferred.id()) {
            let single = tokio::time::timeout(
                self.preferred_head_start,
                self.fetch_from_provider(
                    preferred.id(),
                    title,
                    artist,
                    album,
                    duration_seconds,
                    video_id,
                ),
            )
            .await
            .ok()
            .flatten();

            if let Some(s) = normalize_res(single).filter(|s| plausible(s)) {
                if (s.is_word_synced && word_by_word)
                    || (s.is_synced && !word_by_word)
                    || (s.is_instrumental && wants_alt)
                {
                    self.store(&key, &s, title).await;
                    return lyrics_for_display_mode(&s, word_by_word);
                }
                preferred_fallback = Some(s.clone());
                line_fallback = Some(s);
            }
        }

        // Launch provider waves with bounded race deadline
        let mut futs = FuturesUnordered::new();
        for (wave_idx, wave) in RACE_WAVES.iter().enumerate() {
            for &id in *wave {
                if id == "lrclib" && lrclib_attempted {
                    continue;
                }
                if exclude_provider_ids.contains(id) {
                    continue;
                }
                let delay = self.wave_gap * wave_idx as u32;
                let fut = async move {
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
                    self.attempt_provider(
                        id,
                        title,
                        artist,
                        album,
                        duration_seconds,
                        video_id,
                        exclude_provider_ids,
                    )
                    .await
                };
                futs.push(fut);
            }
        }
        lrclib_attempted = true;

        let mut race_word: Option<LyricsResult> = None;
        let race_deadline = tokio::time::sleep(self.race_total);
        tokio::pin!(race_deadline);

        loop {
            tokio::select! {
                _ = &mut race_deadline => break,
                next = futs.next() => {
                    let Some(raw) = next else { break; };
                    let Some(result) = normalize_res(raw) else { continue; };
                    if word_by_word {
                        if result.is_word_synced
                            && plausible(&result)
                            && !lyrics_looks_like_placeholder(&result)
                            && !lyrics_looks_like_thin_loop(&result)
                        {
                            race_word = Some(result);
                            break;
                        }
                        if !plausible(&result) || (result.is_instrumental && !wants_alt) {
                            continue;
                        }
                        let should_replace = line_fallback
                            .as_ref()
                            .map(|cur| Self::is_better_candidate(&result, cur, title))
                            .unwrap_or(true);
                        if should_replace {
                            line_fallback = Some(result);
                        }
                    } else {
                        if !plausible(&result) || (result.is_instrumental && !wants_alt) {
                            continue;
                        }
                        let should_replace = line_fallback
                            .as_ref()
                            .map(|cur| Self::is_better_candidate(&result, cur, title))
                            .unwrap_or(true);
                        if should_replace {
                            line_fallback = Some(result.clone());
                        }
                        if result.is_synced {
                            break;
                        }
                    }
                }
            }
        }

        if let Some(won) = race_word {
            self.store(&key, &won, title).await;
            return lyrics_for_display_mode(&won, word_by_word);
        }

        if let Some(pref) = preferred_fallback {
            self.store(&key, &pref, title).await;
            return lyrics_for_display_mode(&pref, word_by_word);
        }

        if let Some(ref settled) = line_fallback {
            if settled.is_synced {
                self.store(&key, settled, title).await;
                return lyrics_for_display_mode(settled, word_by_word);
            }
        }

        for extra_id in ["musixmatch", "megalobiz"] {
            if let Some(res) = normalize_res(
                self.attempt_provider(
                    extra_id,
                    title,
                    artist,
                    album,
                    duration_seconds,
                    video_id,
                    exclude_provider_ids,
                )
                .await,
            ) {
                if res.is_synced && plausible(&res) {
                    self.store(&key, &res, title).await;
                    return lyrics_for_display_mode(&res, word_by_word);
                }
            }
        }

        if video_id.map(|v| !v.is_empty()).unwrap_or(false) {
            if let Some(cap) = normalize_res(
                self.attempt_provider(
                    "youtube_captions",
                    title,
                    artist,
                    album,
                    duration_seconds,
                    video_id,
                    exclude_provider_ids,
                )
                .await,
            ) {
                if cap.is_synced && plausible(&cap) {
                    self.store(&key, &cap, title).await;
                    return lyrics_for_display_mode(&cap, word_by_word);
                }
            }
        }

        if !lrclib_attempted {
            if let Some(lrc) = normalize_res(
                self.attempt_provider(
                    "lrclib",
                    title,
                    artist,
                    album,
                    duration_seconds,
                    video_id,
                    exclude_provider_ids,
                )
                .await,
            ) {
                if lrc.is_synced && plausible(&lrc) {
                    self.store(&key, &lrc, title).await;
                    return lyrics_for_display_mode(&lrc, word_by_word);
                }
            }
        }

        if let Some(settled) = line_fallback.filter(|s| !s.is_empty()) {
            self.store(&key, &settled, title).await;
            return lyrics_for_display_mode(&settled, word_by_word);
        }

        if video_id.map(|v| !v.is_empty()).unwrap_or(false) {
            if let Some(ytm) = normalize_res(
                self.attempt_provider(
                    "youtube_music",
                    title,
                    artist,
                    album,
                    duration_seconds,
                    video_id,
                    exclude_provider_ids,
                )
                .await,
            ) {
                if plausible(&ytm) {
                    self.store(&key, &ytm, title).await;
                    return lyrics_for_display_mode(&ytm, word_by_word);
                }
            }
        }

        if let Some(gn) = normalize_res(
            self.attempt_provider(
                "genius",
                title,
                artist,
                album,
                duration_seconds,
                video_id,
                exclude_provider_ids,
            )
            .await,
        ) {
            if plausible(&gn) {
                self.store(&key, &gn, title).await;
                return lyrics_for_display_mode(&gn, word_by_word);
            }
        }

        LyricsResult::empty()
    }

    #[allow(clippy::too_many_arguments)]
    async fn attempt_provider(
        &self,
        provider_id: &str,
        title: &str,
        artist: &str,
        album: &str,
        duration_seconds: Option<i64>,
        video_id: Option<&str>,
        excludes: &HashSet<String>,
    ) -> Option<LyricsResult> {
        if excludes.contains(provider_id) {
            return None;
        }
        let raw = tokio::time::timeout(
            Duration::from_secs(10),
            self.fetch_from_provider(provider_id, title, artist, album, duration_seconds, video_id),
        )
        .await
        .ok()
        .flatten()?;
        if raw.is_empty() {
            return None;
        }
        Some(normalize_karaoke_timings(&raw, title, artist))
    }

    pub async fn fetch_from_provider(
        &self,
        provider_id: &str,
        title: &str,
        artist: &str,
        album: &str,
        duration_seconds: Option<i64>,
        video_id: Option<&str>,
    ) -> Option<LyricsResult> {
        if let Some(ref fetcher) = self.custom_fetcher {
            return fetcher
                .fetch_from_provider(provider_id, title, artist, album, duration_seconds, video_id)
                .await;
        }
        let alb_opt = if album.is_empty() { None } else { Some(album) };
        match provider_id {
            "lastwave" => {
                fetch_lastwave_lyrics(&self.http, title, artist, alb_opt, duration_seconds, None)
                    .await
            }
            "lrc_red" => {
                fetch_lrc_red(&self.http, title, artist, alb_opt, duration_seconds, None).await
            }
            "apple_music" => {
                self.fetch_apple_word_by_word(title, artist, duration_seconds)
                    .await
            }
            "better_lyrics" => {
                fetch_better_lyrics(&self.http, title, artist, alb_opt, duration_seconds).await
            }
            "better_lyrics_portato" => {
                fetch_portato_lyrics(&self.http, title, artist, alb_opt, duration_seconds).await
            }
            "bini_lyrics" => {
                fetch_bini_lyrics(&self.http, title, artist, alb_opt, duration_seconds, None).await
            }
            "lyrics_plus" => {
                fetch_lyrics_plus(&self.http, title, artist, alb_opt, duration_seconds, None).await
            }
            "unison" => {
                fetch_unison_lyrics(&self.http, title, artist, alb_opt, duration_seconds).await
            }
            "kugou" => fetch_kugou(&self.http, title, artist, duration_seconds).await,
            "youtube_captions" => {
                let vid = video_id.filter(|v| !v.is_empty())?;
                let tube = self.tube.as_ref()?;
                let cues = tube.fetch_transcript_cues(vid).await?;
                if cues.is_empty() {
                    return None;
                }
                let lines: Vec<LyricLine> = cues
                    .into_iter()
                    .map(|c| LyricLine {
                        time_ms: c.time_ms,
                        duration_ms: 0,
                        text: c.text,
                        syllables: Vec::new(),
                        transliteration: String::new(),
                        agent: None,
                    })
                    .collect();
                if !lyrics_plausible_duration(&lines, duration_seconds) {
                    return None;
                }
                let plain = lines
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(LyricsResult {
                    lines,
                    is_synced: true,
                    is_word_synced: false,
                    plain_lyrics: plain,
                    is_instrumental: false,
                    source: "YouTube Captions".to_string(),
                })
            }
            "youtube_music" => {
                let vid = video_id.filter(|v| !v.is_empty())?;
                let tube = self.tube.as_ref()?;
                let text = tube.fetch_lyrics_tab_text(vid).await?;
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return None;
                }
                let lines: Vec<LyricLine> = trimmed
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(|l| LyricLine {
                        time_ms: 0,
                        duration_ms: 0,
                        text: l.to_string(),
                        syllables: Vec::new(),
                        transliteration: String::new(),
                        agent: None,
                    })
                    .collect();
                Some(LyricsResult {
                    lines,
                    is_synced: false,
                    is_word_synced: false,
                    plain_lyrics: trimmed.to_string(),
                    is_instrumental: false,
                    source: "YouTube Music".to_string(),
                })
            }
            "musixmatch" => fetch_musixmatch(&self.http, title, artist, duration_seconds).await,
            "megalobiz" => {
                fetch_megalobiz_lyrics(&self.http, title, artist, duration_seconds).await
            }
            "genius" => fetch_genius_lyrics(&self.http, title, artist).await,
            "lrclib" => {
                self.fetch_lrclib(title, artist, album, duration_seconds)
                    .await
            }
            _ => None,
        }
    }

    pub fn is_better_candidate(
        new_res: &LyricsResult,
        current: &LyricsResult,
        query_title: &str,
    ) -> bool {
        if new_res.is_word_synced && !current.is_word_synced {
            return true;
        }
        if !new_res.is_word_synced && current.is_word_synced {
            return false;
        }

        let new_ph = lyrics_looks_like_placeholder(new_res);
        let cur_ph = lyrics_looks_like_placeholder(current);
        if !new_ph && cur_ph {
            return true;
        }
        if new_ph && !cur_ph {
            return false;
        }

        if new_res.is_synced && !current.is_synced {
            return true;
        }
        if !new_res.is_synced && current.is_synced {
            return false;
        }

        let new_thin = lyrics_looks_like_thin_loop(new_res);
        let cur_thin = lyrics_looks_like_thin_loop(current);
        if !new_thin && cur_thin {
            return true;
        }
        if new_thin && !cur_thin {
            return false;
        }

        if !query_title.trim().is_empty() {
            let new_m = lyrics_mentions_title(query_title, new_res);
            let cur_m = lyrics_mentions_title(query_title, current);
            if new_m && !cur_m {
                return true;
            }
            if !new_m && cur_m {
                return false;
            }
        }

        let new_len = lyrics_body_length(new_res);
        let cur_len = lyrics_body_length(current);
        if new_len >= ((cur_len as f64 * 1.25).round() as usize)
            && new_len.saturating_sub(cur_len) >= 120
        {
            return true;
        }
        if cur_len >= ((new_len as f64 * 1.25).round() as usize)
            && cur_len.saturating_sub(new_len) >= 120
        {
            return false;
        }

        let new_unique = lyrics_unique_line_count(new_res);
        let cur_unique = lyrics_unique_line_count(current);
        if new_unique >= ((cur_unique as f64 * 1.5).round() as usize)
            && new_unique.saturating_sub(cur_unique) >= 4
        {
            return true;
        }
        if cur_unique >= ((new_unique as f64 * 1.5).round() as usize)
            && cur_unique.saturating_sub(new_unique) >= 4
        {
            return false;
        }

        let new_curated = new_res.source.to_lowercase().contains("apple");
        let cur_curated = current.source.to_lowercase().contains("apple");
        if new_curated && !cur_curated {
            return true;
        }
        if !new_curated && cur_curated {
            return false;
        }

        new_res.lines.len() >= ((current.lines.len() as f64 * 1.3).round() as usize)
    }

    async fn fetch_lrclib(
        &self,
        title: &str,
        artist: &str,
        album: &str,
        duration_seconds: Option<i64>,
    ) -> Option<LyricsResult> {
        let clean_t = clean_song_title(title);
        let clean_a = clean_song_artist(artist);
        let mut records: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();

        let add_record = |data: &serde_json::Map<String, Value>,
                          records: &mut HashMap<String, serde_json::Map<String, Value>>| {
            let rec_title = data
                .get("trackName")
                .or_else(|| data.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let rec_artist = data
                .get("artistName")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !lyrics_titles_match(&clean_t, rec_title) && !lyrics_titles_match(title, rec_title) {
                return;
            }
            if !lyrics_artists_match(&clean_a, rec_artist)
                && !lyrics_artists_match(artist, rec_artist)
            {
                return;
            }
            if !lyrics_duration_plausible(
                duration_seconds,
                data.get("duration").and_then(Value::as_f64),
            ) {
                return;
            }
            let rec_album = data.get("albumName").and_then(Value::as_str).unwrap_or("");
            if !clean_t.to_lowercase().contains("instrumental")
                && lyrics_is_alternate_recording(rec_title, rec_album)
            {
                return;
            }
            let id = data
                .get("id")
                .map(|v| v.to_string())
                .unwrap_or_else(|| {
                    format!(
                        "{}|{}|{}",
                        rec_title.to_lowercase(),
                        rec_artist.to_lowercase(),
                        rec_album
                    )
                });
            records.entry(id).or_insert_with(|| data.clone());
        };

        let mut attempts = vec![
            vec![
                ("track_name", clean_t.clone()),
                ("artist_name", clean_a.clone()),
                ("album_name", album.to_string()),
            ],
            vec![
                ("track_name", clean_t.clone()),
                ("artist_name", clean_a.clone()),
            ],
        ];
        if clean_t != title || clean_a != artist {
            attempts.push(vec![
                ("track_name", title.to_string()),
                ("artist_name", artist.to_string()),
                ("album_name", album.to_string()),
            ]);
            attempts.push(vec![
                ("track_name", title.to_string()),
                ("artist_name", artist.to_string()),
            ]);
        }
        if clean_a.contains('$') {
            let replaced = clean_a.replace('$', "s");
            attempts.push(vec![
                ("track_name", clean_t.clone()),
                ("artist_name", replaced.clone()),
                ("album_name", album.to_string()),
            ]);
            attempts.push(vec![
                ("track_name", clean_t.clone()),
                ("artist_name", replaced),
            ]);
        }

        for params in attempts {
            let mut qp: Vec<(&str, String)> = params
                .into_iter()
                .filter(|(_, v)| !v.trim().is_empty())
                .collect();
            if let Some(d) = duration_seconds.filter(|&s| s > 0) {
                qp.push(("duration", d.to_string()));
            }
            if let Ok(res) = self
                .http
                .get("https://lrclib.net/api/get")
                .query(&qp)
                .header("User-Agent", DESKTOP_UA)
                .send()
                .await
            {
                if res.status().as_u16() == 200 {
                    if let Ok(Value::Object(map)) = res.json::<Value>().await {
                        add_record(&map, &mut records);
                    }
                }
            }
        }

        for hit in self.lrclib_search_hits(&clean_t, &clean_a, duration_seconds).await {
            add_record(&hit, &mut records);
        }
        if clean_t != title || clean_a != artist {
            for hit in self.lrclib_search_hits(title, artist, duration_seconds).await {
                add_record(&hit, &mut records);
            }
        }
        if records.is_empty() {
            if let Some(primary_a) = extract_artist_entities(&clean_a).into_iter().next() {
                if primary_a != clean_a {
                    for hit in self
                        .lrclib_search_hits(&clean_t, &primary_a, duration_seconds)
                        .await
                    {
                        add_record(&hit, &mut records);
                    }
                }
            }
        }

        let mut best: Option<LyricsResult> = None;
        for record in records.values() {
            let Some(parsed) = Self::lrclib_record_to_result(record, &clean_t) else {
                continue;
            };
            if parsed.is_empty() {
                continue;
            }
            let replace = best
                .as_ref()
                .map(|b| Self::is_better_candidate(&parsed, b, &clean_t))
                .unwrap_or(true);
            if replace {
                best = Some(parsed);
            }
        }
        best
    }

    fn lrclib_record_to_result(
        record: &serde_json::Map<String, Value>,
        query_title: &str,
    ) -> Option<LyricsResult> {
        let record_name = record
            .get("trackName")
            .or_else(|| record.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let is_inst = record.get("instrumental").and_then(Value::as_bool) == Some(true)
            || record_name.to_lowercase().contains("(instrumental)")
            || record_name.to_lowercase().contains("[instrumental]");
        if is_inst {
            if !query_title.to_lowercase().contains("instrumental") {
                return None;
            }
            return Some(LyricsResult {
                is_instrumental: true,
                source: "lrclib".to_string(),
                ..LyricsResult::empty()
            });
        }

        let synced = record
            .get("syncedLyrics")
            .and_then(Value::as_str)
            .unwrap_or("");
        let plain = record
            .get("plainLyrics")
            .and_then(Value::as_str)
            .unwrap_or("");

        let mut parsed: Option<LyricsResult> = None;
        if !synced.is_empty() {
            let lines = parse_lrc(synced);
            if !lines.is_empty() {
                let word_synced = !parse_enhanced_lrc(synced).is_empty();
                parsed = Some(LyricsResult {
                    lines,
                    is_synced: true,
                    is_word_synced: word_synced,
                    plain_lyrics: plain.to_string(),
                    is_instrumental: false,
                    source: "lrclib".to_string(),
                });
            }
        }
        if parsed.is_none() && !plain.is_empty() {
            let lines: Vec<LyricLine> = plain
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(|l| LyricLine {
                    time_ms: 0,
                    duration_ms: 0,
                    text: l.to_string(),
                    syllables: Vec::new(),
                    transliteration: String::new(),
                    agent: None,
                })
                .collect();
            parsed = Some(LyricsResult {
                lines,
                is_synced: false,
                is_word_synced: false,
                plain_lyrics: plain.to_string(),
                is_instrumental: false,
                source: "lrclib".to_string(),
            });
        }

        let res = parsed.filter(|p| !p.is_empty())?;
        if lyrics_looks_like_placeholder(&res)
            && !query_title.to_lowercase().contains("instrumental")
        {
            return None;
        }
        if !res.is_synced && !res.is_word_synced {
            let unpacked = expand_packed_lyric_lines(&res.lines);
            if unpacked.len() > res.lines.len() {
                return Some(LyricsResult {
                    lines: unpacked,
                    ..res
                });
            }
        }
        Some(res)
    }

    async fn lrclib_search_hits(
        &self,
        title: &str,
        artist: &str,
        duration_seconds: Option<i64>,
    ) -> Vec<serde_json::Map<String, Value>> {
        let q = format!("{artist} {title}");
        let Ok(res) = self
            .http
            .get("https://lrclib.net/api/search")
            .query(&[("q", q.as_str())])
            .header("User-Agent", DESKTOP_UA)
            .send()
            .await
        else {
            return Vec::new();
        };
        let Ok(Value::Array(list)) = res.json::<Value>().await else {
            return Vec::new();
        };
        let wants_alt = lyrics_is_alternate_recording(title, "");
        let mut hits = Vec::new();
        for item in list {
            let Some(map) = item.as_object() else {
                continue;
            };
            let rec_title = map
                .get("trackName")
                .or_else(|| map.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let rec_artist = map.get("artistName").and_then(Value::as_str).unwrap_or("");
            let rec_album = map.get("albumName").and_then(Value::as_str).unwrap_or("");
            if !lyrics_titles_match(title, rec_title) {
                continue;
            }
            if !lyrics_artists_match(artist, rec_artist) {
                continue;
            }
            if !wants_alt && lyrics_is_alternate_recording(rec_title, rec_album) {
                continue;
            }
            if !lyrics_duration_plausible(
                duration_seconds,
                map.get("duration").and_then(Value::as_f64),
            ) {
                continue;
            }
            hits.push(map.clone());
        }
        hits
    }

    async fn fetch_apple_word_by_word(
        &self,
        title: &str,
        artist: &str,
        duration_seconds: Option<i64>,
    ) -> Option<LyricsResult> {
        let clean_t = clean_song_title(title);
        let clean_a = clean_song_artist(artist);
        let mut track_ids = self
            .resolve_apple_track_ids(&clean_t, &clean_a, duration_seconds)
            .await;
        if track_ids.is_empty() && (clean_t != title || clean_a != artist) {
            track_ids = self
                .resolve_apple_track_ids(title, artist, duration_seconds)
                .await;
        }
        let mut fallback: Option<LyricsResult> = None;
        for track_id in track_ids.into_iter().take(3) {
            let Ok(res) = self
                .http
                .get("https://lyrics.paxsenix.org/apple-music/lyrics")
                .query(&[("id", track_id.as_str())])
                .header("User-Agent", DESKTOP_UA)
                .header("Accept", "application/json")
                .send()
                .await
            else {
                continue;
            };
            let Ok(Value::Object(map)) = res.json::<Value>().await else {
                continue;
            };
            let Some(parsed) = Self::parse_apple_word_by_word(&map) else {
                continue;
            };
            if parsed.is_empty() {
                continue;
            }
            if lyrics_looks_like_placeholder(&parsed) || lyrics_looks_like_thin_loop(&parsed) {
                if fallback.is_none() {
                    fallback = Some(parsed);
                }
                continue;
            }
            return Some(parsed);
        }
        fallback
    }

    async fn resolve_apple_track_ids(
        &self,
        title: &str,
        artist: &str,
        duration_seconds: Option<i64>,
    ) -> Vec<String> {
        let term = format!("{artist} {title}");
        let res = self
            .http
            .get("https://itunes.apple.com/search")
            .query(&[
                ("term", term.as_str()),
                ("media", "music"),
                ("entity", "song"),
                ("limit", "10"),
            ])
            .header("User-Agent", DESKTOP_UA)
            .send()
            .await
            .ok();

        let mut items: Vec<Value> = match res {
            Some(r) if r.status().is_success() => {
                let text = r.text().await.unwrap_or_default();
                serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v.get("results").and_then(Value::as_array).cloned())
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };

        if items.is_empty() {
            if let Some(primary) = extract_artist_entities(artist).first().cloned() {
                if primary != artist {
                    let term2 = format!("{primary} {title}");
                    if let Ok(r2) = self
                        .http
                        .get("https://itunes.apple.com/search")
                        .query(&[
                            ("term", term2.as_str()),
                            ("media", "music"),
                            ("entity", "song"),
                            ("limit", "10"),
                        ])
                        .header("User-Agent", DESKTOP_UA)
                        .send()
                        .await
                    {
                        if let Ok(text2) = r2.text().await {
                            items = serde_json::from_str::<Value>(&text2)
                                .ok()
                                .and_then(|v| v.get("results").and_then(Value::as_array).cloned())
                                .unwrap_or_default();
                        }
                    }
                }
            }
        }

        if items.is_empty() {
            return Vec::new();
        }
        let wants_alt = lyrics_is_alternate_recording(title, "");
        let target_ms = duration_seconds.unwrap_or(0) * 1000;
        let mut scored: Vec<(String, i32)> = Vec::new();

        for item in items {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let id = obj
                .get("trackId")
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            if id.is_empty() {
                continue;
            }
            let raw_title = obj.get("trackName").and_then(Value::as_str).unwrap_or("");
            let raw_artist = obj.get("artistName").and_then(Value::as_str).unwrap_or("");
            let album = obj
                .get("collectionName")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !wants_alt && lyrics_is_alternate_recording(raw_title, album) {
                continue;
            }
            if !lyrics_titles_match(title, raw_title) || !lyrics_artists_match(artist, raw_artist) {
                continue;
            }
            let mut score = 10;
            let ms = obj
                .get("trackTimeMillis")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if target_ms > 0 && ms > 0 {
                let diff = (ms - target_ms).abs();
                if diff > 25000 && (diff as f64 / target_ms as f64) > 0.25 {
                    continue;
                }
                if diff <= 4000 {
                    score += 3;
                } else if diff <= 10000 {
                    score += 1;
                }
            }
            scored.push((id, score));
        }

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(id, _)| id).collect()
    }

    pub fn parse_apple_word_by_word(
        json: &serde_json::Map<String, Value>,
    ) -> Option<LyricsResult> {
        let content = json.get("content")?.as_array()?;
        if content.is_empty() {
            return None;
        }
        let kind = json
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();

        struct ParsedLine {
            start: i64,
            duration: i64,
            text: String,
            syllables: Vec<LyricSyllable>,
        }

        let mut parsed = Vec::new();
        let mut saw_multi_word = false;

        for item in content {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let start = obj
                .get("timestamp")
                .map(parse_lyric_timestamp_ms)
                .unwrap_or(0);
            let end = obj.get("endtime").map(parse_lyric_timestamp_ms).unwrap_or(0);
            let parsed_dur = obj
                .get("duration")
                .map(parse_lyric_timestamp_ms)
                .unwrap_or(0);
            let duration = if parsed_dur > 0 {
                parsed_dur
            } else {
                (end - start).max(0)
            };
            let is_bg = obj.get("background").and_then(Value::as_bool) == Some(true);
            let Some(words) = obj.get("text").and_then(Value::as_array) else {
                continue;
            };
            let word_objs: Vec<&serde_json::Map<String, Value>> =
                words.iter().filter_map(Value::as_object).collect();
            if word_objs.is_empty() {
                continue;
            }
            if word_objs.len() > 1 {
                saw_multi_word = true;
            }

            let mut syllables = Vec::new();
            let mut buf = String::new();
            for (i, w) in word_objs.iter().enumerate() {
                let wt = w.get("text").and_then(Value::as_str).unwrap_or("");
                if wt.is_empty() {
                    continue;
                }
                let ws = w
                    .get("timestamp")
                    .map(parse_lyric_timestamp_ms)
                    .unwrap_or(start);
                let wd = w.get("duration").map(parse_lyric_timestamp_ms).unwrap_or(0);
                let is_part = w.get("part").and_then(Value::as_bool) == Some(true);
                syllables.push(LyricSyllable {
                    time_ms: ws,
                    duration_ms: wd,
                    text: wt.to_string(),
                    is_background: is_bg,
                });
                buf.push_str(wt);
                if !is_part && i < word_objs.len() - 1 {
                    buf.push(' ');
                }
            }
            let text = buf.trim().to_string();
            if text.is_empty() {
                continue;
            }
            parsed.push(ParsedLine {
                start,
                duration,
                text,
                syllables,
            });
        }

        let timed = parsed.iter().any(|p| p.start > 0 || p.duration > 0);
        let word_synced = timed
            && (kind == "syllable"
                || kind == "word"
                || saw_multi_word
                || parsed.iter().any(|p| p.syllables.len() > 1));

        let raw_lines: Vec<LyricLine> = parsed
            .into_iter()
            .map(|p| LyricLine {
                time_ms: p.start,
                duration_ms: p.duration,
                text: p.text,
                syllables: if word_synced { p.syllables } else { Vec::new() },
                transliteration: String::new(),
                agent: None,
            })
            .collect();

        let lines = if timed {
            let mut indexed: Vec<(usize, LyricLine)> = raw_lines.into_iter().enumerate().collect();
            indexed.sort_by(|a, b| a.1.time_ms.cmp(&b.1.time_ms).then(a.0.cmp(&b.0)));
            indexed.into_iter().map(|(_, l)| l).collect()
        } else {
            expand_packed_lyric_lines(&raw_lines)
        };

        Some(LyricsResult {
            lines,
            is_synced: timed,
            is_word_synced: word_synced,
            plain_lyrics: json
                .get("plain")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            is_instrumental: false,
            source: "Apple Music".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_artist_matching_prevent_false_positives() {
        assert!(lyrics_titles_match("All Caps [Official Audio]", "ALL CAPS"));
        assert!(!lyrics_titles_match("All", "All Caps"));
        assert!(lyrics_artists_match(
            "The Weeknd feat. Daft Punk",
            "The Weeknd"
        ));
    }

    #[test]
    fn apple_word_by_word_parser_preserves_syllables() {
        let payload: serde_json::Map<String, Value> = serde_json::from_str(
            r#"{
                "type": "Syllable",
                "content": [
                    {
                        "timestamp": 1000,
                        "endtime": 2500,
                        "text": [
                            {"text": "con", "timestamp": 1000, "duration": 300, "part": true},
                            {"text": "versation", "timestamp": 1300, "duration": 500, "part": false},
                            {"text": "now", "timestamp": 1800, "duration": 700, "part": false}
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();
        let res = LyricsRepository::parse_apple_word_by_word(&payload).unwrap();
        assert!(res.is_synced);
        assert!(res.is_word_synced);
        assert_eq!(res.lines[0].text, "conversation now");
        assert_eq!(res.lines[0].syllables.len(), 3);
    }
}
