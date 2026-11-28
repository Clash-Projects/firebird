//! Lyrics models and parsers (LRC, Enhanced LRC, TTML, stanza assembly,
//! karaoke timing normalization, credit stripping, and matching helpers).
//! Ported from LastWave-native `data/lyrics/LyricsRepository.kt`.

use std::collections::HashSet;
use std::sync::OnceLock;

use quick_xml::{events::Event, Reader};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

fn diacritics_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\p{M}+").expect("valid diacritics regex"))
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricSyllable {
    #[serde(default)]
    pub time_ms: i64,
    #[serde(default)]
    pub duration_ms: i64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub is_background: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    #[serde(default)]
    pub time_ms: i64,
    #[serde(default)]
    pub duration_ms: i64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub syllables: Vec<LyricSyllable>,
    #[serde(default)]
    pub transliteration: String,
    #[serde(default)]
    pub agent: Option<String>,
}

impl LyricLine {
    pub fn has_syllables(&self) -> bool {
        !self.syllables.is_empty()
    }

    pub fn is_rtl(&self) -> bool {
        is_rtl_text(&self.text)
    }
}

/// Vocal role for spatial staging in desktop widescreen layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VocalRole {
    /// Vocalist 1 / Main singer (Left aligned, max 72% width).
    #[default]
    LeadLeft,
    /// Vocalist 2 / Duet singer (Right aligned, max 72% width).
    DuetRight,
    /// Both vocalists / Solo chorus (Center aligned).
    UnisonCenter,
    /// Accompaniment / harmony (Nested under lead line, 0.65x font).
    BackgroundSubline,
}

/// A line with its nested accompaniment background vocals.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLineGroup {
    pub lead_line: LyricLine,
    #[serde(default)]
    pub background_sublines: Vec<LyricLine>,
    #[serde(default)]
    pub role: VocalRole,
}

impl LyricLineGroup {
    pub fn time_ms(&self) -> i64 {
        self.lead_line.time_ms
    }

    pub fn duration_ms(&self) -> i64 {
        self.lead_line.duration_ms
    }
}

/// A cohesive stanza (verse, chorus, bridge) with musical boundary timing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricStanza {
    #[serde(default)]
    pub time_ms: i64,
    #[serde(default)]
    pub duration_ms: i64,
    #[serde(default)]
    pub groups: Vec<LyricLineGroup>,
    #[serde(default)]
    pub is_chorus: bool,
}

/// Group flat lyric lines into musical stanzas and assign spatial vocal roles.
pub fn assemble_stanzas(raw_lines: &[LyricLine]) -> Vec<LyricStanza> {
    if raw_lines.is_empty() {
        return Vec::new();
    }
    let mut stanzas = Vec::new();
    let mut current_groups: Vec<LyricLineGroup> = Vec::new();
    let mut stanza_start_ms = raw_lines[0].time_ms;

    for (i, line) in raw_lines.iter().enumerate() {
        let prev_line = if i > 0 { Some(&raw_lines[i - 1]) } else { None };

        // Detect stanza break: pause >= 2500ms or explicit credit/section change
        let is_stanza_break = prev_line.is_some_and(|prev| {
            line.time_ms - (prev.time_ms + prev.duration_ms.max(1000)) >= 2500
        });

        if is_stanza_break && !current_groups.is_empty() {
            let last = current_groups.last().unwrap();
            let stanza_end_ms = last.time_ms() + last.duration_ms();
            stanzas.push(LyricStanza {
                time_ms: stanza_start_ms,
                duration_ms: (stanza_end_ms - stanza_start_ms).max(0),
                groups: current_groups,
                is_chorus: false,
            });
            current_groups = Vec::new();
            stanza_start_ms = line.time_ms;
        }

        // Determine vocal role
        let mut role = VocalRole::LeadLeft;
        let text = line.text.trim();
        let is_bg = (!line.syllables.is_empty() && line.syllables.iter().all(|s| s.is_background))
            || (text.starts_with('(') && text.ends_with(')'));

        if is_bg && !current_groups.is_empty() {
            // Nest as background subline attached to previous lead line.
            let mut last_group = current_groups.pop().unwrap();
            last_group.background_sublines.push(line.clone());
            current_groups.push(last_group);
            continue;
        }

        // Detect duet opposing alignment (TTML agents or singer markers)
        if line.agent.as_deref() == Some("v2")
            || text.starts_with("Vocalist 2:")
            || text.starts_with("Singer 2:")
            || text.contains("[v2]")
        {
            role = VocalRole::DuetRight;
        } else if line.agent.as_deref().is_some_and(|a| a.contains("both"))
            || text.starts_with("Both:")
            || text.starts_with("Chorus:")
            || text.contains("[all]")
        {
            role = VocalRole::UnisonCenter;
        }

        current_groups.push(LyricLineGroup {
            lead_line: line.clone(),
            background_sublines: Vec::new(),
            role,
        });
    }

    if !current_groups.is_empty() {
        let last = current_groups.last().unwrap();
        let stanza_end_ms = last.time_ms() + last.duration_ms();
        stanzas.push(LyricStanza {
            time_ms: stanza_start_ms,
            duration_ms: (stanza_end_ms - stanza_start_ms).max(0),
            groups: current_groups,
            is_chorus: false,
        });
    }

    stanzas
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsResult {
    #[serde(default)]
    pub lines: Vec<LyricLine>,
    #[serde(default)]
    pub is_synced: bool,
    #[serde(default)]
    pub is_word_synced: bool,
    #[serde(default)]
    pub plain_lyrics: String,
    #[serde(default)]
    pub is_instrumental: bool,
    #[serde(default)]
    pub source: String,
}

impl LyricsResult {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.plain_lyrics.is_empty() && !self.is_instrumental
    }
}

/// Synthesize proportional word syllables across a line's duration
/// for smooth Apple Music karaoke wipe on line-synced lyrics.
pub fn interpolate_line_syllables(
    text: &str,
    start_time_ms: i64,
    duration_ms: i64,
) -> Vec<LyricSyllable> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

    let total_chars: usize = words.iter().map(|w| w.chars().count()).sum();
    if total_chars == 0 {
        return Vec::new();
    }

    let safe_duration = if duration_ms > 0 {
        duration_ms
    } else {
        1500.max((words.len() as i64) * 350)
    };
    let mut syllables = Vec::with_capacity(words.len());
    let mut current_ms = start_time_ms;

    for (i, word) in words.iter().enumerate() {
        let is_last = i == words.len() - 1;
        let word_chars = word.chars().count() as f64;
        let frac = (word_chars / total_chars as f64) * safe_duration as f64;
        let word_dur = 120.max(frac.round() as i64);
        syllables.push(LyricSyllable {
            time_ms: current_ms,
            duration_ms: word_dur,
            text: if is_last {
                (*word).to_string()
            } else {
                format!("{word} ")
            },
            is_background: false,
        });
        current_ms += word_dur;
    }

    syllables
}

/// Parse a lyric cue that may be milliseconds, seconds, or a fractional
/// second (e.g. `12.45`). Fractional values under 1000 are seconds.
pub fn parse_lyric_timestamp_ms(raw: &Value) -> i64 {
    let n = match raw {
        Value::Null => return 0,
        Value::Number(num) => num.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        _ => return 0,
    };
    parse_lyric_timestamp_ms_f64(n)
}

pub fn parse_lyric_timestamp_ms_f64(n: f64) -> i64 {
    if !n.is_finite() || n <= 0.0 {
        return 0;
    }
    if n < 1000.0 && n != n.round() {
        return (n * 1000.0).round() as i64;
    }
    n.round() as i64
}

/// True when every cue shares the same timestamp (including Apple
/// `itunes:timing="None"` transcripts where every line is 0). Follow
/// scroll must not treat that as "already on the last line".
pub fn lyrics_are_untimed(lines: &[LyricLine]) -> bool {
    if lines.len() < 2 {
        return false;
    }
    lines.first().map(|l| l.time_ms) == lines.last().map(|l| l.time_ms)
}

pub fn lyrics_body_length(result: &LyricsResult) -> usize {
    let n: usize = result.lines.iter().map(|l| l.text.trim().chars().count()).sum();
    if n > 0 {
        return n;
    }
    result
        .plain_lyrics
        .chars()
        .filter(|c| !c.is_whitespace())
        .count()
}

/// Apple unsynced TTML often packs two sung phrases into one `<p>`.
/// Split once at the midpoint so the second phrase is not clipped.
pub fn expand_packed_lyric_line(text: &str, max_len: usize) -> Vec<String> {
    let t = text.trim();
    if t.is_empty() {
        return Vec::new();
    }
    if t.len() <= max_len {
        return vec![t.to_string()];
    }
    let mid = t.len() / 2;
    let floor_35 = ((t.len() as f64) * 0.35).floor() as usize;

    let mut at = t[..t.floor_char_boundary(mid)].rfind(' ');
    if at.is_none_or(|idx| idx < floor_35) {
        let start = t.ceil_char_boundary(mid);
        at = t[start..].find(' ').map(|rel| start + rel);
    }

    let Some(at) = at else {
        return vec![t.to_string()];
    };
    if at == 0 || at >= t.len() - 1 {
        return vec![t.to_string()];
    }
    let left = t[..at].trim();
    let right = t[at + 1..].trim();
    if left.len() < 16 || right.len() < 16 {
        return vec![t.to_string()];
    }
    vec![left.to_string(), right.to_string()]
}

pub fn expand_packed_lyric_lines(lines: &[LyricLine]) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for line in lines {
        let parts = expand_packed_lyric_line(&line.text, 48);
        if parts.is_empty() {
            continue;
        }
        if parts.len() == 1 {
            out.push(line.clone());
            continue;
        }
        for part in parts {
            out.push(LyricLine {
                time_ms: line.time_ms,
                duration_ms: line.duration_ms,
                text: part,
                syllables: Vec::new(),
                transliteration: line.transliteration.clone(),
                agent: None,
            });
        }
    }
    out
}

/// Index of the line currently being sung, or `-1` if playback is
/// still before the first cue.
pub fn active_lyric_line_index(lines: &[LyricLine], position_ms: i64) -> isize {
    if lines.is_empty() {
        return -1;
    }
    if position_ms < lines[0].time_ms {
        return -1;
    }
    if lyrics_are_untimed(lines) {
        return -1;
    }

    if lines.len() > 1 && lines[0].time_ms == lines[1].time_ms {
        let next_distinct_time = lines[2..]
            .iter()
            .find(|l| l.time_ms != lines[0].time_ms)
            .map(|l| l.time_ms);
        if let Some(next_t) = next_distinct_time {
            if position_ms < next_t {
                return 0;
            }
        }
    }

    let mut low: isize = 0;
    let mut high: isize = lines.len() as isize - 1;
    let mut result: isize = 0;

    while low <= high {
        let mid = (low + high) >> 1;
        if lines[mid as usize].time_ms <= position_ms {
            result = mid;
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }
    result
}

pub fn calculate_syllable_progress(syl: &LyricSyllable, pos_ms: i64) -> f64 {
    let start_ms = syl.time_ms;
    let dur_ms = syl.duration_ms.max(1);
    let end_ms = start_ms + dur_ms;
    if pos_ms <= start_ms {
        return 0.0;
    }
    if pos_ms >= end_ms {
        return 1.0;
    }
    (pos_ms - start_ms) as f64 / dur_ms as f64
}

/// Viewport alignment for follow-scroll. Opening lines stay at the
/// top; only later lines pin to the target anchor.
pub fn lyric_follow_alignment(
    active_index: isize,
    compact: bool,
    target_anchor: Option<f64>,
) -> f64 {
    if active_index <= 0 {
        return 0.0;
    }
    if active_index == 1 {
        return if compact { 0.12 } else { 0.14 };
    }
    target_anchor.unwrap_or(if compact { 0.28 } else { 0.34 })
}

/// Scale cues that were clearly authored in seconds (or microseconds)
/// into milliseconds so a 3-minute song is not treated as already
/// finished after the first second of playback.
pub fn ensure_millisecond_timestamps(result: &LyricsResult) -> LyricsResult {
    if !result.is_synced || result.lines.len() < 3 {
        return result.clone();
    }
    let mut times: Vec<i64> = result.lines.iter().map(|l| l.time_ms).collect();
    times.sort_unstable();
    let first = times[0];
    let last = *times.last().unwrap();
    let span = last - first;
    if last <= 0 {
        return result.clone();
    }

    let scale: fn(i64) -> i64 = if last <= 720 && span <= 720 {
        |v| v * 1000
    } else if last >= 10 * 60 * 1000 * 1000 {
        |v| (v as f64 / 1000.0).round() as i64
    } else {
        return result.clone();
    };

    LyricsResult {
        lines: result
            .lines
            .iter()
            .map(|line| LyricLine {
                time_ms: scale(line.time_ms),
                duration_ms: scale(line.duration_ms),
                text: line.text.clone(),
                syllables: line
                    .syllables
                    .iter()
                    .map(|s| LyricSyllable {
                        time_ms: scale(s.time_ms),
                        duration_ms: scale(s.duration_ms),
                        text: s.text.clone(),
                        is_background: s.is_background,
                    })
                    .collect(),
                transliteration: line.transliteration.clone(),
                agent: line.agent.clone(),
            })
            .collect(),
        is_synced: result.is_synced,
        is_word_synced: result.is_word_synced,
        plain_lyrics: result.plain_lyrics.clone(),
        is_instrumental: result.is_instrumental,
        source: result.source.clone(),
    }
}

fn labeled_credit_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(?:lyrics?(?:\s*by)?|lyricist(?:\(s\))?|written\s*by|writer(?:\(s\))?|song\s*writer(?:\(s\))?|composed\s*by|composer(?:\(s\))?|composition|music(?:\s*by)?|produced\s*by|producer(?:\(s\))?|co-?producer(?:\(s\))?|executive\s*producer(?:\(s\))?|arranged\s*by|arranger(?:\(s\))?|arrangement|performed\s*by|performer(?:\(s\))?|artist(?:\(s\))?|singer(?:\(s\))?|vocals?(?:\s*by)?|backing\s*vocals?|background\s*vocals?|lead\s*vocals?|feat(?:uring|\.)?|ft\.?|mixed\s*by|remixed\s*by|mixing(?:\s*(?:by|engineer|assistant|studio))?|mastered\s*by|mastering(?:\s*(?:by|engineer|studio))?|recorded\s*(?:at|by|in)|recording(?:\s*(?:engineer|studio))?|engineered\s*by|engineer(?:\(s\))?|audio\s*engineer|drums?|guitar(?:s|ist)?|acoustic\s*guitar|electric\s*guitar|bass(?:\s*guitar)?|keyboards?|piano|synthesizer|synth|strings?|percussion|horns?|brass|programming|programmed\s*by|published\s*by|publisher|record\s*label|copyright|phonographic\s*copyright|licensed\s*(?:to|by)|courtesy\s*of|transcribed\s*by|synced\s*by|synchronized\s*by|timed\s*by|lrc\s*(?:by|maker)|edited\s*by|translated\s*by|作\s*词|作\s*曲|词\s*曲|编\s*曲|制\s*作(?:人|团队)?|プロデューサー|演\s*唱|原\s*唱|主\s*唱|合\s*唱|和\s*声|和\s*音|混\s*音(?:师|室|工程师)?|母\s*带(?:工程|处理|工程师)?|录\s*音(?:师|室|棚|工程师)?|吉\s*他|木\s*吉\s*他|电\s*吉\s*他|贝\s*斯|鼓|架\s*子\s*鼓|键\s*盘|钢\s*琴|弦\s*乐(?:编写)?|配\s*唱(?:制作人)?|人\s*声(?:编辑)?|统\s*筹|监\s*制|出\s*品(?:人)?|发\s*行|曲\s*绘|题\s*字|文\s*案|策\s*划|鸣\s*谢|翻\s*唱|后\s*期|宣\s*传|推\s*广|版\s*权|词|曲|编|唱|OP|SP)\s*[:：]",
        )
        .expect("valid labeled_credit regex")
    })
}

fn cjk_space_credit_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^\s*(?:作\s*词|作\s*曲|词\s*曲|编\s*曲|制\s*作\s*人|演\s*唱|原\s*唱|混\s*音(?:师)?|母\s*带(?:工程师)?|录\s*音(?:师)?|配\s*唱\s*制\s*作\s*人|弦\s*乐\s*编\s*写|和\s*声\s*编\s*写|出\s*品\s*人?)\s+\S",
        )
        .expect("valid cjk_space_credit regex")
    })
}

fn phrase_credit_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(?:(?:lyrics?\s+and\s+music|music\s+and\s+lyrics?)\s+by\s+\S|(?:lyrics?|written|composed|produced|co-?produced|arranged|performed|mixed|remixed|mastered|recorded|engineered|published|programmed|transcribed|synced|synchronized|timed)\s+by\s+\S|recorded\s+(?:at|in)\s+\S|courtesy\s+of\s+\S|copyright\s+(?:©|\(c\)|\d{4})|[℗©]\s*\d{4})",
        )
        .expect("valid phrase_credit regex")
    })
}

const NON_CREDIT_SINGLE_WORDS: &[&str] = &[
    "oh", "ooh", "ah", "uh", "uhh", "yeah", "yea", "yep", "nah", "hey", "na", "la", "da", "ba",
    "sha", "mm", "hmm", "woah", "whoa", "ayy", "ay", "yo", "no", "yes", "go", "he", "she", "him",
    "her", "his", "hers", "they", "them", "their", "me", "you", "we", "us", "it", "on", "in", "up",
    "down", "left", "right", "day", "night", "love", "hate", "life", "death", "true", "false",
    "black", "white", "one", "two", "three", "four",
];

const CREDIT_NAME_PARTICLES: &[&str] = &[
    "de", "del", "della", "da", "di", "du", "dos", "das", "van", "von", "der", "den", "het", "ten",
    "ter", "bin", "ibn", "ibin", "al", "el", "la", "le", "los", "las", "mc", "mac", "st", "st.",
    "jr", "jr.", "sr", "sr.", "ii", "iii", "iv", "v", "and", "&", "x", "feat", "feat.", "ft",
    "ft.", "the", "of",
];

fn looks_like_credit_name_segment(raw_segment: &str) -> bool {
    let segment = raw_segment.trim();
    if segment.is_empty() || segment.chars().count() > 50 {
        return false;
    }
    if segment
        .chars()
        .any(|c| matches!(c, '?' | '!' | ';' | '"' | '(' | ')' | '[' | ']'))
    {
        return false;
    }
    if segment.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    if segment.chars().any(|c| {
        ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3040}'..='\u{30FF}').contains(&c)
            || ('\u{AC00}'..='\u{D7AF}').contains(&c)
    }) {
        let len = segment.chars().count();
        return (2..=12).contains(&len);
    }

    let words: Vec<&str> = segment.split_whitespace().collect();
    if words.is_empty() || words.len() > 6 {
        return false;
    }
    if words.len() == 1 && NON_CREDIT_SINGLE_WORDS.contains(&words[0].to_lowercase().as_str()) {
        return false;
    }

    let mut saw_capitalized = false;
    for w in words {
        let clean = w.trim_matches(|c| c == '.' || c == ',');
        if clean.is_empty() {
            continue;
        }
        let lower = clean.to_lowercase();
        if CREDIT_NAME_PARTICLES.contains(&lower.as_str()) {
            continue;
        }
        if NON_CREDIT_SINGLE_WORDS.contains(&lower.as_str()) {
            return false;
        }
        let first = clean.chars().next().unwrap();
        if first.is_ascii_uppercase()
            || ('\u{00C0}'..='\u{024F}').contains(&first)
            || first == '$'
        {
            saw_capitalized = true;
        } else {
            return false;
        }
    }

    saw_capitalized
}

fn is_slash_separated_credit_list(
    text: &str,
    index: usize,
    total_lines: usize,
    artist: &str,
) -> bool {
    if !text.contains('/') {
        return false;
    }
    let segments: Vec<&str> = text
        .split('/')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if segments.len() < 2 {
        return false;
    }
    let unique_lower: HashSet<String> = segments.iter().map(|s| s.to_lowercase()).collect();
    if unique_lower.len() < 2 {
        return false;
    }

    let name_count = segments
        .iter()
        .filter(|s| looks_like_credit_name_segment(s))
        .count();
    let multi_word_count = segments
        .iter()
        .filter(|s| s.split_whitespace().count() >= 2)
        .count();

    if segments.len() >= 3 {
        let all_or_most_are_names = name_count == segments.len()
            || (segments.len() >= 4 && (name_count as f64 / segments.len() as f64) >= 0.8);
        if all_or_most_are_names
            && (multi_word_count >= 1 || index < 8 || index + 4 >= total_lines)
        {
            return true;
        }
    } else if segments.len() == 2 && (index < 5 || index + 3 >= total_lines) && name_count == 2 {
        if multi_word_count == 2 {
            return true;
        }
        if !artist.trim().is_empty()
            && lyrics_artists_match_strict(segments[0], artist)
            && lyrics_artists_match_strict(segments[1], artist)
        {
            return true;
        }
    }
    false
}

/// True when `text` is a lyric metadata/credit row (songwriter/composer/producer
/// label, slash-separated songwriter list, or opening `Title - Artist` banner).
pub fn is_lyric_credit_line(
    text: &str,
    index: usize,
    total_lines: usize,
    title: &str,
    artist: &str,
) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    if labeled_credit_regex().is_match(trimmed) {
        return true;
    }
    if cjk_space_credit_regex().is_match(trimmed) {
        return true;
    }

    if (index < 6 || index + 4 >= total_lines) && phrase_credit_regex().is_match(trimmed) {
        return true;
    }

    if is_slash_separated_credit_list(trimmed, index, total_lines, artist) {
        return true;
    }

    // Opening header lines (`Title - Artist`, `Artist - Title`, or multi-artist billing).
    if index < 3 {
        let req_title = title.trim();
        let req_artist = artist.trim();
        if !req_title.is_empty() && !req_artist.is_empty() {
            static DASH_SPLIT: OnceLock<Regex> = OnceLock::new();
            let dash_re = DASH_SPLIT
                .get_or_init(|| Regex::new(r"\s+(?:[-–—:|])\s+").expect("valid dash_split regex"));
            let dash_parts: Vec<&str> = dash_re.split(trimmed).collect();
            if dash_parts.len() == 2 {
                let left = dash_parts[0].trim();
                let right = dash_parts[1].trim();
                if (lyrics_titles_match_strict(left, req_title)
                    && lyrics_artists_match_strict(right, req_artist))
                    || (lyrics_artists_match_strict(left, req_artist)
                        && lyrics_titles_match_strict(right, req_title))
                {
                    return true;
                }
            }
            static MULTI_ARTIST_MARKER: OnceLock<Regex> = OnceLock::new();
            let marker_re = MULTI_ARTIST_MARKER.get_or_init(|| {
                Regex::new(r"(?i)[,&/]|(?:\b(?:feat|ft|featuring|with)\b)")
                    .expect("valid multi_artist_marker regex")
            });
            if index == 0
                && !trimmed.contains('!')
                && !trimmed.contains('?')
                && marker_re.is_match(trimmed)
                && !lyrics_titles_match_strict(trimmed, req_title)
            {
                let line_entities = extract_artist_entities(trimmed);
                let artist_entities = extract_artist_entities(req_artist);
                if line_entities.len() >= 2
                    && !artist_entities.is_empty()
                    && line_entities.iter().all(|e| artist_entities.contains(e))
                {
                    return true;
                }
            }
        }
    }

    false
}

fn lyric_line_is_credit(
    line: &LyricLine,
    index: usize,
    total_lines: usize,
    title: &str,
    artist: &str,
) -> bool {
    if is_lyric_credit_line(&line.text, index, total_lines, title, artist) {
        return true;
    }
    if !line.syllables.is_empty() {
        let syl_text: String = line
            .syllables
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let syl_trimmed = syl_text.trim();
        if !syl_trimmed.is_empty()
            && syl_trimmed != line.text
            && is_lyric_credit_line(syl_trimmed, index, total_lines, title, artist)
        {
            return true;
        }
    }
    false
}

/// Remove credit/songwriter/artist metadata lines from `result`.
/// Never strips all lines if doing so would leave a non-empty result empty.
pub fn strip_lyric_credit_lines(result: &LyricsResult, title: &str, artist: &str) -> LyricsResult {
    if result.lines.is_empty() && result.plain_lyrics.is_empty() {
        return result.clone();
    }

    let mut lines = result.lines.clone();
    let mut changed = false;
    if !lines.is_empty() {
        let total = lines.len();
        let mut filtered = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let effective_index = if filtered.is_empty() { 0 } else { i };
            if lyric_line_is_credit(line, effective_index, total, title, artist) {
                changed = true;
                continue;
            }
            filtered.push(line.clone());
        }
        if !filtered.is_empty() {
            lines = filtered;
        } else {
            changed = false;
        }
    }

    let mut plain = result.plain_lyrics.clone();
    if !plain.is_empty() {
        let raw_plain: Vec<&str> = plain.split('\n').collect();
        let total_plain = raw_plain.len();
        let mut filtered_plain = Vec::new();
        let mut plain_changed = false;
        for (i, raw_line) in raw_plain.iter().enumerate() {
            let l = raw_line.trim();
            if l.is_empty() {
                continue;
            }
            let effective_index = if filtered_plain.is_empty() { 0 } else { i };
            if is_lyric_credit_line(l, effective_index, total_plain, title, artist) {
                plain_changed = true;
                continue;
            }
            filtered_plain.push(*raw_line);
        }
        if plain_changed && !filtered_plain.is_empty() {
            plain = filtered_plain.join("\n");
            changed = true;
        }
    }

    if !changed {
        return result.clone();
    }

    LyricsResult {
        lines,
        is_synced: result.is_synced,
        is_word_synced: result.is_word_synced,
        plain_lyrics: plain,
        is_instrumental: result.is_instrumental,
        source: result.source.clone(),
    }
}

/// Fill missing karaoke durations and preserve a timed syllable that carries
/// into the next line.
pub fn normalize_karaoke_timings(
    result: &LyricsResult,
    title: &str,
    artist: &str,
) -> LyricsResult {
    let mut res = ensure_millisecond_timestamps(result);
    if res.lines.is_empty() && !res.plain_lyrics.is_empty() {
        let raw_lines: Vec<LyricLine> = res
            .plain_lyrics
            .split('\n')
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
        res.lines = raw_lines;
    }
    res = strip_lyric_credit_lines(&res, title, artist);
    if !res.is_synced || res.lines.is_empty() {
        return res;
    }

    let mut src = res.lines.clone();
    src.sort_by_key(|l| l.time_ms);
    let mut lines = Vec::with_capacity(src.len());

    for (i, line) in src.iter().enumerate() {
        let next_start = src.get(i + 1).map(|l| l.time_ms);
        let mut duration = line.duration_ms;
        if let Some(next) = next_start {
            let gap = next - line.time_ms;
            if gap <= 0 {
                duration = 80;
            } else if duration <= 0 || duration > gap {
                duration = gap;
            }
        } else if duration <= 0 {
            duration = 4000;
        }
        if res.is_word_synced {
            if let Some(last) = line
                .syllables
                .iter()
                .max_by_key(|s| (s.time_ms, s.duration_ms))
                .filter(|s| s.time_ms >= line.time_ms && s.duration_ms > 0)
            {
                let timed_duration = last
                    .time_ms
                    .saturating_sub(line.time_ms)
                    .saturating_add(last.duration_ms);
                duration = duration.max(timed_duration);
            }
        }
        duration = duration.clamp(80, 30000);
        let syllables = if line.has_syllables() {
            normalize_syllables(&line.syllables, line.time_ms, duration)
        } else {
            interpolate_line_syllables(&line.text, line.time_ms, duration)
        };
        lines.push(LyricLine {
            time_ms: line.time_ms,
            duration_ms: duration,
            text: line.text.clone(),
            syllables,
            transliteration: line.transliteration.clone(),
            agent: line.agent.clone(),
        });
    }

    LyricsResult {
        lines,
        is_synced: res.is_synced,
        is_word_synced: res.is_word_synced,
        plain_lyrics: res.plain_lyrics,
        is_instrumental: res.is_instrumental,
        source: res.source,
    }
}

fn normalize_syllables(
    raw: &[LyricSyllable],
    line_start_ms: i64,
    line_duration_ms: i64,
) -> Vec<LyricSyllable> {
    // Preserve lyric text order when a backing vocal overlaps the lead.
    raw.chunk_by(|a, b| a.is_background == b.is_background)
        .flat_map(|group| normalize_vocal_syllables(group, line_start_ms, line_duration_ms))
        .collect()
}

fn normalize_vocal_syllables(
    raw: &[LyricSyllable],
    line_start_ms: i64,
    line_duration_ms: i64,
) -> Vec<LyricSyllable> {
    let mut sorted = raw.to_vec();
    sorted.sort_by_key(|s| s.time_ms);
    let line_end = line_start_ms + line_duration_ms;
    let mut out = Vec::with_capacity(sorted.len());

    for (i, s) in sorted.iter().enumerate() {
        let next_start = sorted.get(i + 1).map_or(line_end, |next| next.time_ms);
        let mut start = s.time_ms.max(line_start_ms);
        if start >= next_start {
            start = line_start_ms.max(next_start - 40);
        }
        let gap = 40.max(next_start - start);
        const MIN_SUNG_MS: i64 = 80;
        let provided = s.duration_ms;
        let dur = if provided >= MIN_SUNG_MS && provided <= gap {
            provided
        } else {
            gap
        };
        out.push(LyricSyllable {
            time_ms: start,
            duration_ms: dur,
            text: s.text.clone(),
            is_background: s.is_background,
        });
    }
    out
}

/// RTL detection for Arabic/Hebrew lyrics rendering.
pub fn is_rtl_text(text: &str) -> bool {
    text.chars().any(|ch| {
        let rune = ch as u32;
        (0x0590..=0x08FF).contains(&rune)
            || (0xFB00..=0xFDFF).contains(&rune)
            || (0xFE70..=0xFEFF).contains(&rune)
    })
}

fn word_stamp_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<(\d{1,3}):(\d{2})[.:](\d{2,3})>").expect("valid word stamp"))
}

fn word_stamp_to_ms(min_raw: &str, sec_raw: &str, frac_raw: &str) -> i64 {
    let min = min_raw.parse::<i64>().unwrap_or(0);
    let sec = sec_raw.parse::<i64>().unwrap_or(0);
    let frac_ms = match frac_raw.len() {
        1 => frac_raw.parse::<i64>().unwrap_or(0) * 100,
        2 => frac_raw.parse::<i64>().unwrap_or(0) * 10,
        3 => frac_raw.parse::<i64>().unwrap_or(0),
        _ => 0,
    };
    min * 60000 + sec * 1000 + frac_ms
}

fn parse_word_runs(body: &str, line_start_ms: i64, line_end_ms: Option<i64>) -> Vec<LyricSyllable> {
    let re = word_stamp_regex();
    let marks: Vec<regex::Captures<'_>> = re.captures_iter(body).collect();
    if marks.is_empty() {
        return Vec::new();
    }
    let mut words = Vec::new();
    const MAX_MS: i64 = 1_i64 << 31;

    // 1. Capture leading text before the first timestamp tag (Word 1)
    let first_whole = marks[0].get(0).unwrap();
    let raw_leading = &body[..first_whole.start()];
    let leading_text = raw_leading.trim();
    if !leading_text.is_empty() && (raw_leading.ends_with(' ') || raw_leading.ends_with('\t')) {
        let first_mark_ms = word_stamp_to_ms(
            marks[0].get(1).unwrap().as_str(),
            marks[0].get(2).unwrap().as_str(),
            marks[0].get(3).unwrap().as_str(),
        );
        let start_ms = line_start_ms.min(first_mark_ms);
        let dur_ms = (first_mark_ms - start_ms).max(0);
        words.push(LyricSyllable {
            time_ms: start_ms.clamp(0, MAX_MS),
            duration_ms: (if dur_ms > 0 { dur_ms } else { 500 }).clamp(0, MAX_MS),
            text: leading_text.to_string(),
            is_background: false,
        });
    }

    // 2. Iterate through matched word tags
    for i in 0..marks.len() {
        let whole = marks[i].get(0).unwrap();
        let until = if i + 1 < marks.len() {
            marks[i + 1].get(0).unwrap().start()
        } else {
            body.len()
        };
        let text = body[whole.end()..until].trim();
        if text.is_empty() {
            continue;
        }
        let start_ms = word_stamp_to_ms(
            marks[i].get(1).unwrap().as_str(),
            marks[i].get(2).unwrap().as_str(),
            marks[i].get(3).unwrap().as_str(),
        );
        let end_ms = if i + 1 < marks.len() {
            word_stamp_to_ms(
                marks[i + 1].get(1).unwrap().as_str(),
                marks[i + 1].get(2).unwrap().as_str(),
                marks[i + 1].get(3).unwrap().as_str(),
            )
        } else {
            line_end_ms.unwrap_or(start_ms + 800)
        };
        words.push(LyricSyllable {
            time_ms: start_ms.clamp(0, MAX_MS),
            duration_ms: (end_ms - start_ms).clamp(0, MAX_MS),
            text: text.to_string(),
            is_background: false,
        });
    }

    words
}

/// Parse standard LRC into [`LyricLine`]s with line durations and
/// word-by-word syllable interpolation.
///
/// Enhanced LRC carries inline word stamps (`<m:ss.xx>word`).
pub fn parse_lrc(lrc: &str) -> Vec<LyricLine> {
    let mut text = lrc.to_string();
    if text.contains(r"\n") && !text.contains('\n') {
        text = text.replace(r"\r\n", "\n").replace(r"\n", "\n");
    }
    text = text.replace("\r\n", "\n").replace('\r', "\n");

    static TIMESTAMP_RE: OnceLock<Regex> = OnceLock::new();
    let timestamp_re = TIMESTAMP_RE.get_or_init(|| {
        Regex::new(r"\[\s*(?:(\d{1,2}):)?(\d{1,3}):(\d{2})(?:[.:](\d{1,4}))?\s*\]")
            .expect("valid lrc timestamp regex")
    });
    static OFFSET_RE: OnceLock<Regex> = OnceLock::new();
    let offset_re = OFFSET_RE.get_or_init(|| {
        Regex::new(r"(?i)\[offset:\s*([+-]?\d+)\]").expect("valid lrc offset regex")
    });

    const MAX_MS: i64 = 1_i64 << 31;
    let mut offset: i64 = 0;
    let mut raw_entries: Vec<(i64, String)> = Vec::new();

    for raw in text.split('\n') {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(offset_match) = offset_re.captures(line) {
            offset = offset_match
                .get(1)
                .and_then(|m| m.as_str().parse::<i64>().ok())
                .unwrap_or(0);
            continue;
        }
        let matches: Vec<regex::Captures<'_>> = timestamp_re.captures_iter(line).collect();
        if matches.is_empty() {
            continue;
        }
        let body = timestamp_re.replace_all(line, "").into_owned();
        for m in matches {
            let frac_raw = m.get(4).map_or("", |g| g.as_str());
            let frac_ms: i64 = match frac_raw.len() {
                1 => frac_raw.parse::<i64>().unwrap_or(0) * 100,
                2 => frac_raw.parse::<i64>().unwrap_or(0) * 10,
                3 => frac_raw.parse::<i64>().unwrap_or(0),
                n if n >= 4 => frac_raw[..3].parse::<i64>().unwrap_or(0),
                _ => 0,
            };

            let total_ms = if let Some(g1) = m.get(1) {
                if !frac_raw.is_empty() {
                    let hour = g1.as_str().parse::<i64>().unwrap_or(0);
                    let min = m
                        .get(2)
                        .and_then(|g| g.as_str().parse::<i64>().ok())
                        .unwrap_or(0);
                    let sec = m
                        .get(3)
                        .and_then(|g| g.as_str().parse::<i64>().ok())
                        .unwrap_or(0);
                    (hour * 3_600_000 + min * 60_000 + sec * 1000 + frac_ms + offset)
                        .clamp(0, MAX_MS)
                } else {
                    let min = g1.as_str().parse::<i64>().unwrap_or(0);
                    let sec = m
                        .get(2)
                        .and_then(|g| g.as_str().parse::<i64>().ok())
                        .unwrap_or(0);
                    let raw_frac = m.get(3).map_or("", |g| g.as_str());
                    let mut f_ms = raw_frac.parse::<i64>().unwrap_or(0);
                    if raw_frac.len() == 1 {
                        f_ms *= 100;
                    } else if raw_frac.len() == 2 {
                        f_ms *= 10;
                    } else if raw_frac.len() >= 4 {
                        f_ms = raw_frac[..3].parse::<i64>().unwrap_or(0);
                    }
                    (min * 60_000 + sec * 1000 + f_ms + offset).clamp(0, MAX_MS)
                }
            } else {
                let min = m
                    .get(2)
                    .and_then(|g| g.as_str().parse::<i64>().ok())
                    .unwrap_or(0);
                let sec = m
                    .get(3)
                    .and_then(|g| g.as_str().parse::<i64>().ok())
                    .unwrap_or(0);
                (min * 60_000 + sec * 1000 + frac_ms + offset).clamp(0, MAX_MS)
            };

            raw_entries.push((total_ms, body.clone()));
        }
    }

    raw_entries.sort_by_key(|e| e.0);

    let ws_re = word_stamp_regex();
    let mut result = Vec::new();
    for i in 0..raw_entries.len() {
        let (time_ms, ref body) = raw_entries[i];
        let next_time = raw_entries.get(i + 1).map(|e| e.0);
        let text = ws_re.replace_all(body, "");
        let text = text.trim();
        let words = parse_word_runs(body, time_ms, next_time);
        if !words.is_empty() {
            let line_start = time_ms.min(words[0].time_ms);
            let calc_dur = match next_time {
                Some(nt) => (nt - line_start).clamp(80, 20000),
                None => 4000,
            };
            let joined: String = words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let chosen_text = if !text.is_empty() && text.len() >= joined.len() {
                text.to_string()
            } else {
                joined
            };
            result.push(LyricLine {
                time_ms: line_start,
                duration_ms: calc_dur,
                text: chosen_text,
                syllables: words,
                transliteration: String::new(),
                agent: None,
            });
            continue;
        }
        if text.is_empty() {
            continue;
        }
        let calc_dur = match next_time {
            Some(nt) => (nt - time_ms).clamp(80, 20000),
            None => 4000,
        };
        result.push(LyricLine {
            time_ms,
            duration_ms: calc_dur,
            text: text.to_string(),
            syllables: interpolate_line_syllables(text, time_ms, calc_dur),
            transliteration: String::new(),
            agent: None,
        });
    }

    result
}

/// Parse enhanced LRC (inline `<m:ss.xx>` word stamps) only: returns
/// empty when the source carries no word stamps at all.
pub fn parse_enhanced_lrc(lrc: &str) -> Vec<LyricLine> {
    if !word_stamp_regex().is_match(lrc) {
        return Vec::new();
    }
    parse_lrc(lrc)
}

const VERSION_KEYWORDS: &[(&str, &str)] = &[
    ("live", "live"),
    ("concert", "live"),
    ("session", "live"),
    ("unplugged", "unplugged"),
    ("acoustic", "acoustic"),
    ("piano", "acoustic"),
    ("stripped", "acoustic"),
    ("remix", "remix"),
    ("mixed", "remix"),
    ("dj mix", "remix"),
    ("mix", "remix"),
    ("club mix", "remix"),
    ("extended", "remix"),
    ("vip", "remix"),
    ("cover", "cover"),
    ("tribute", "cover"),
    ("karaoke", "karaoke"),
    ("instrumental", "instrumental"),
    ("slowed", "slowed"),
    ("sped up", "sped"),
    ("speed up", "sped"),
    ("spedup", "sped"),
    ("sped", "sped"),
    ("nightcore", "sped"),
    ("demo", "demo"),
    ("lullaby", "lullaby"),
    ("8d", "8d"),
    ("interlude", "interlude"),
    ("intro", "intro"),
    ("outro", "outro"),
    ("skit", "skit"),
    ("acapella", "acapella"),
    ("a cappella", "acapella"),
    ("orchestral", "orchestral"),
    ("orchestra", "orchestral"),
    ("symphonic", "orchestral"),
    ("re-recorded", "rerecorded"),
    ("rerecorded", "rerecorded"),
];

fn version_tags(raw_title: &str) -> HashSet<&'static str> {
    static BRACKET_RE: OnceLock<Regex> = OnceLock::new();
    static TRAILING_RE: OnceLock<Regex> = OnceLock::new();
    static TRAILING_WORD_RE: OnceLock<Regex> = OnceLock::new();

    let bracket_re =
        BRACKET_RE.get_or_init(|| Regex::new(r"[\(\[](.*?)[\)\]]").expect("valid bracket regex"));
    let trailing_re = TRAILING_RE.get_or_init(|| {
        Regex::new(r"\s*[-–—:]\s*([^-–—:\(\[]+)\s*$").expect("valid trailing regex")
    });
    let trailing_word_re = TRAILING_WORD_RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(remix|acoustic|live|cover|karaoke|instrumental|slowed|nightcore|extended|unplugged|vip)\s*$",
        )
        .expect("valid trailing word regex")
    });

    let mut segments: Vec<&str> = Vec::new();
    for caps in bracket_re.captures_iter(raw_title) {
        segments.push(caps.get(1).map_or("", |m| m.as_str()));
    }
    if let Some(caps) = trailing_re.captures(raw_title) {
        segments.push(caps.get(1).map_or("", |m| m.as_str()));
    }
    if let Some(caps) = trailing_word_re.captures(raw_title) {
        segments.push(caps.get(1).map_or("", |m| m.as_str()));
    }

    let mut tags = HashSet::new();
    for segment in segments {
        let lower = format!(" {} ", segment.to_lowercase());
        for &(keyword, tag) in VERSION_KEYWORDS {
            if lower.contains(keyword) {
                tags.insert(tag);
            }
        }
    }
    tags
}

/// True only when both titles describe the same recording version —
/// a live/remix/cover tag on one side but not the other rejects.
pub fn lyrics_same_version(request_title: &str, candidate_title: &str) -> bool {
    version_tags(request_title) == version_tags(candidate_title)
}

/// Removes a leading `Artist - ` / `Artist – ` / `Artist: ` segment.
pub fn strip_leading_artist_prefix(raw: &str) -> String {
    static PREFIX_RE: OnceLock<Regex> = OnceLock::new();
    let re = PREFIX_RE
        .get_or_init(|| Regex::new(r"^(\s*.+?\s*[-–—:]\s+)(\S.*)$").expect("valid prefix regex"));
    let stripped = if let Some(caps) = re.captures(raw) {
        let tail_start = caps.get(2).unwrap().start();
        raw[tail_start..].trim()
    } else {
        raw.trim()
    };
    if stripped.chars().count() >= 2 {
        stripped.to_string()
    } else {
        raw.trim().to_string()
    }
}

fn normalize_lyrics_symbol(s: &str) -> String {
    static APOSTROPHE_RE: OnceLock<Regex> = OnceLock::new();
    static QUOTE_RE: OnceLock<Regex> = OnceLock::new();
    static DASH_RE: OnceLock<Regex> = OnceLock::new();
    static DOLLAR_DIGIT_RE: OnceLock<Regex> = OnceLock::new();

    let apos = APOSTROPHE_RE
        .get_or_init(|| Regex::new("[\u{2018}\u{2019}\u{201A}\u{201B}`']").expect("apos"));
    let quote =
        QUOTE_RE.get_or_init(|| Regex::new("[\u{201C}\u{201D}\u{201E}\u{201F}\"]").expect("quote"));
    let dash = DASH_RE.get_or_init(|| Regex::new("[\u{2013}\u{2014}\u{2212}-]").expect("dash"));
    let dollar_digit = DOLLAR_DIGIT_RE.get_or_init(|| Regex::new(r"\$(\d)").expect("dollar_digit"));

    let v = apos.replace_all(s, "");
    let v = quote.replace_all(&v, " ");
    let v = dash.replace_all(&v, " ");
    let nfd: String = v.to_lowercase().nfd().collect();
    let v = diacritics_regex().replace_all(&nfd, "");
    let v = dollar_digit.replace_all(&v, "$1");
    v.replace('$', "s")
}

fn extract_artist_entities(raw_artist: &str) -> HashSet<String> {
    static TOPIC_RE: OnceLock<Regex> = OnceLock::new();
    static FEAT_BRACKET_RE: OnceLock<Regex> = OnceLock::new();
    static PART_SPLIT_RE: OnceLock<Regex> = OnceLock::new();

    let topic_re = TOPIC_RE.get_or_init(|| Regex::new(r"(?i)\s*-\s*Topic$").expect("topic"));
    let feat_re = FEAT_BRACKET_RE.get_or_init(|| {
        Regex::new(
            r"(?i)\s*[\(\[]\s*(?:feat\.?|ft\.?|featuring\b|with\b|prod\.?|produced by\b)[^\)\]]*[\)\]]",
        )
        .expect("feat_bracket")
    });
    let split_re = PART_SPLIT_RE.get_or_init(|| {
        Regex::new(r"(?i)\s*[,/;&+]\s*|\s+(?:feat\.?|ft\.?|featuring|with|vs\.?|x)\s+")
            .expect("part_split")
    });

    let stripped = topic_re.replace_all(raw_artist, "");
    let stripped = stripped.trim();
    let cleaned = feat_re.replace_all(stripped, "");
    let normalized = normalize_lyrics_symbol(&cleaned);
    let normalized = normalized.trim();

    let mut entities = HashSet::new();
    if !normalized.is_empty() {
        entities.insert(normalized.to_string());
    }
    for p in split_re.split(normalized) {
        let t = p.trim();
        if !t.is_empty() {
            entities.insert(t.to_string());
        }
    }
    entities
}

fn token_overlap(a: &str, b: &str, floor: f64) -> bool {
    let na = normalize_lyrics_symbol(a);
    let nb = normalize_lyrics_symbol(b);
    let a_tokens: HashSet<&str> = na.split_whitespace().collect();
    let b_tokens: HashSet<&str> = nb.split_whitespace().collect();
    let union = a_tokens.union(&b_tokens).count();
    if union == 0 {
        return false;
    }
    let inter = a_tokens.intersection(&b_tokens).count();
    (inter as f64 / union as f64) >= floor
}

/// Title match that also tolerates dirty community titles carrying an
/// `Artist - Title` prefix. Enforces version agreement, length gate,
/// and high token overlap.
pub fn lyrics_titles_match_strict(candidate_title: &str, request_title: &str) -> bool {
    if !lyrics_same_version(request_title, candidate_title) {
        return false;
    }

    let cands = [
        candidate_title.to_string(),
        strip_leading_artist_prefix(candidate_title),
    ];
    let reqs = [
        request_title.to_string(),
        strip_leading_artist_prefix(request_title),
    ];

    for cand in &cands {
        for req in &reqs {
            let ca = normalize_lyrics_symbol(cand);
            let ca = ca.trim();
            let cb = normalize_lyrics_symbol(req);
            let cb = cb.trim();
            if ca.is_empty() || cb.is_empty() {
                continue;
            }
            if ca == cb {
                return true;
            }
            if ca.contains(cb) || cb.contains(ca) {
                let ca_len = ca.chars().count();
                let cb_len = cb.chars().count();
                let ratio = ca_len.min(cb_len) as f64 / ca_len.max(cb_len) as f64;
                if ratio >= 0.7 {
                    return true;
                }
            }
            if token_overlap(ca, cb, 0.6) {
                return true;
            }
        }
    }
    false
}

/// Artist agreement for search-result filtering. Delimiter-aware entity
/// matching handles multi-artist billings (commas, feat, &, x) while
/// rejecting short-name theft.
pub fn lyrics_artists_match_strict(candidate_artist: &str, request_artist: &str) -> bool {
    let ca_owned = normalize_lyrics_symbol(candidate_artist);
    let ca = ca_owned.trim();
    let ra_owned = normalize_lyrics_symbol(request_artist);
    let ra = ra_owned.trim();
    if ca.is_empty() || ra.is_empty() {
        return false;
    }
    if ca == ra {
        return true;
    }

    let c_entities = extract_artist_entities(candidate_artist);
    let r_entities = extract_artist_entities(request_artist);

    for ce in &c_entities {
        if ce.chars().count() < 2 {
            continue;
        }
        for re in &r_entities {
            if re.chars().count() < 2 {
                continue;
            }
            if ce == re {
                return true;
            }
        }
    }

    let (shorter, longer) = if ca.chars().count() <= ra.chars().count() {
        (ca, ra)
    } else {
        (ra, ca)
    };
    if ca.contains(ra) || ra.contains(ca) {
        if shorter.chars().count() < 4 {
            return token_overlap(ca, ra, 0.6);
        }
        let pattern = format!(
            r"(?i)(^|(?:\s*[,&/;+]\s*|\s+(?:with|feat\.?|ft\.?|featuring|vs\.?|x)\s+)){}(?:$|(?:\s*[,&/;+]\s*|\s+(?:with|feat\.?|ft\.?|featuring|vs\.?|x)\s+))",
            regex::escape(shorter)
        );
        if let Ok(re) = Regex::new(&pattern) {
            if re.is_match(longer) {
                return true;
            }
        }
        let ca_len = ca.chars().count();
        let ra_len = ra.chars().count();
        let ratio = ca_len.min(ra_len) as f64 / ca_len.max(ra_len) as f64;
        if ratio >= 0.6 {
            return true;
        }
    }
    token_overlap(ca, ra, 0.6)
}

/// Search-form title: strip credits/packaging only. Version markers
/// (remix, live, acoustic ...) name a different recording and survive so
/// [`lyrics_same_version`] can reject the wrong cut.
pub fn lyrics_for_search_title(raw: &str) -> String {
    static SEARCH_WS: OnceLock<Regex> = OnceLock::new();
    static SEARCH_CREDITS: OnceLock<[Regex; 4]> = OnceLock::new();
    static TRAILING_DASH_COMMA: OnceLock<Regex> = OnceLock::new();

    let ws = SEARCH_WS.get_or_init(|| Regex::new(r"\s+").expect("ws"));
    let credits = SEARCH_CREDITS.get_or_init(|| {
        [
            Regex::new(r"(?i)\s*[(\[]\s*(feat|ft|featuring|with)\b[^)\]]*[)\]]").expect("c0"),
            Regex::new(r"(?i)\s+(feat|ft|featuring)\.?\s+.*$").expect("c1"),
            Regex::new(
                r"(?i)\s*[(\[]\s*(official\s*)?(music\s*)?(video|audio|visuali[sz]er|lyrics?\s*video|lyrics?|m/?v|hd|hq|4k|full\s*song)\s*[)\]]",
            )
            .expect("c2"),
            Regex::new(r"(?i)\s*[(\[]\s*official\s*[)\]]").expect("c3"),
        ]
    });
    let trailing_punct =
        TRAILING_DASH_COMMA.get_or_init(|| Regex::new(r"[,–—-]+$").expect("trailing_punct"));

    let mut name = raw.to_string();
    for pattern in credits {
        name = pattern.replace_all(&name, " ").into_owned();
    }
    let collapsed = ws.replace_all(&name, " ");
    let trimmed = collapsed.trim();
    let no_trail = trailing_punct.replace_all(trimmed, "");
    let final_trimmed = no_trail.trim();
    if final_trimmed.is_empty() {
        raw.trim().to_string()
    } else {
        final_trimmed.to_string()
    }
}

/// Search-form artist: strip the YouTube `- Topic` suffix.
pub fn lyrics_for_search_artist(raw: &str) -> String {
    static TOPIC_RE: OnceLock<Regex> = OnceLock::new();
    let re = TOPIC_RE.get_or_init(|| Regex::new(r"\s*-\s*Topic$").expect("topic"));
    let stripped = re.replace_all(raw, "");
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        raw.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Decode HTML/XML entities (`&#x..;`, `&#..;`, named — `&amp;` last).
pub fn lyrics_decode_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    static HEX_RE: OnceLock<Regex> = OnceLock::new();
    static DEC_RE: OnceLock<Regex> = OnceLock::new();

    let hex_re = HEX_RE.get_or_init(|| Regex::new(r"&#x([0-9a-fA-F]+);").expect("hex"));
    let dec_re = DEC_RE.get_or_init(|| Regex::new(r"&#(\d+);").expect("dec"));

    let out = hex_re.replace_all(raw, |caps: &regex::Captures<'_>| {
        u32::from_str_radix(&caps[1], 16)
            .ok()
            .and_then(char::from_u32)
            .map_or_else(|| caps[0].to_string(), |c| c.to_string())
    });
    let out = dec_re.replace_all(&out, |caps: &regex::Captures<'_>| {
        caps[1]
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map_or_else(|| caps[0].to_string(), |c| c.to_string())
    });
    out.replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Duration plausibility for providers whose responses carry no
/// candidate identity: the lyric timeline must roughly fit the track.
pub fn lyrics_plausible_duration(lines: &[LyricLine], duration_seconds: Option<i64>) -> bool {
    let Some(dur_s) = duration_seconds else {
        return true;
    };
    if dur_s <= 0 || lines.len() < 3 {
        return true;
    }
    let expected_ms = dur_s * 1000;
    if expected_ms < 60_000 {
        return true;
    }
    let mut end_ms: i64 = 0;
    for line in lines {
        let mut syl_end = 0;
        for s in &line.syllables {
            syl_end = syl_end.max(s.time_ms + s.duration_ms);
        }
        end_ms = end_ms.max((line.time_ms + line.duration_ms).max(syl_end));
    }
    if end_ms <= 0 {
        return true;
    }
    if end_ms > expected_ms + 45_000 {
        return false;
    }
    if (end_ms as f64) < (expected_ms as f64) * 0.45 && expected_ms - end_ms > 90_000 {
        return false;
    }
    true
}

/// Parse TTML (`<p begin end>` lines, `<span begin end>` words, plus
/// `<itunes:transliteration>` and `<span ttm:role="x-roman">` romanisations).
pub fn parse_ttml(ttml: &str) -> Vec<LyricLine> {
    static P_TAG: OnceLock<Regex> = OnceLock::new();
    static BEGIN_ATTR: OnceLock<Regex> = OnceLock::new();
    static END_ATTR: OnceLock<Regex> = OnceLock::new();
    static AGENT_ATTR: OnceLock<Regex> = OnceLock::new();
    static KEY_ATTR: OnceLock<Regex> = OnceLock::new();
    static TRANSLIT_BLOCK: OnceLock<Regex> = OnceLock::new();
    static TRANSLIT_TEXT: OnceLock<Regex> = OnceLock::new();
    static XML_TAGS: OnceLock<Regex> = OnceLock::new();

    let p_tag = P_TAG.get_or_init(|| {
        Regex::new(r"(?is)<(?:[a-zA-Z0-9_-]+:)?p\b([^>]*?)>(.*?)</(?:[a-zA-Z0-9_-]+:)?p>")
            .expect("p_tag")
    });
    let begin_attr = BEGIN_ATTR.get_or_init(|| {
        Regex::new(r#"(?i)(?:begin|start|startTime|from)=["']([^"']+)["']"#).expect("begin_attr")
    });
    let end_attr = END_ATTR.get_or_init(|| {
        Regex::new(r#"(?i)(?:end|endTime|to)=["']([^"']+)["']"#).expect("end_attr")
    });
    let agent_attr =
        AGENT_ATTR.get_or_init(|| Regex::new(r#"(?i)agent=["']([^"']+)["']"#).expect("agent_attr"));
    let key_attr =
        KEY_ATTR.get_or_init(|| Regex::new(r#"(?i)\bkey=["']([^"']+)["']"#).expect("key_attr"));
    let translit_block = TRANSLIT_BLOCK.get_or_init(|| {
        Regex::new(r"(?is)<(?:[a-zA-Z0-9_-]+:)?transliteration\b[^>]*>(.*?)</(?:[a-zA-Z0-9_-]+:)?transliteration>")
            .expect("translit_block")
    });
    let translit_text = TRANSLIT_TEXT.get_or_init(|| {
        Regex::new(r#"(?is)<(?:[a-zA-Z0-9_-]+:)?text\b[^>]*?\bfor=["']([^"']+)["'][^>]*>(.*?)</(?:[a-zA-Z0-9_-]+:)?text>"#)
            .expect("translit_text")
    });
    let xml_tags = XML_TAGS.get_or_init(|| Regex::new(r"<[^>]+>").expect("xml_tags"));

    let mut raw = ttml.to_string();
    if raw.contains(r"\n") && !raw.contains('\n') {
        raw = raw.replace(r"\r\n", "\n").replace(r"\n", "\n");
    }

    // Extract Apple `<itunes:transliteration><text for="L1">...</text></itunes:transliteration>`
    let mut translit_by_key = std::collections::HashMap::<String, String>::new();
    for block in translit_block.captures_iter(&raw) {
        for item in translit_text.captures_iter(&block[1]) {
            let k = item[1].trim().to_string();
            let plain = unescape_xml(&xml_tags.replace_all(&item[2], ""))
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !k.is_empty() && !plain.is_empty() {
                translit_by_key.entry(k).or_insert(plain);
            }
        }
    }

    // Strip `<head>...</head>` so `<text>` inside metadata is never matched as `<p>`
    static HEAD_TAG: OnceLock<Regex> = OnceLock::new();
    let head_tag = HEAD_TAG.get_or_init(|| {
        Regex::new(r"(?is)<(?:[a-zA-Z0-9_-]+:)?head\b[^>]*>.*?</(?:[a-zA-Z0-9_-]+:)?head>")
            .expect("head_tag")
    });
    let body_raw = head_tag.replace_all(&raw, "");

    let mut lines = Vec::new();
    for p in p_tag.captures_iter(&body_raw) {
        let attrs = &p[1];
        let start = begin_attr
            .captures(attrs)
            .map_or(0, |c| parse_ttml_time(&c[1]));
        let end = end_attr
            .captures(attrs)
            .map_or(0, |c| parse_ttml_time(&c[1]));
        let agent = agent_attr.captures(attrs).map(|c| c[1].to_string());
        let line_key = key_attr.captures(attrs).map(|c| c[1].trim().to_string());
        let Some((text, syllables, inline_roman)) =
            parse_ttml_paragraph(&p[2], begin_attr, end_attr)
        else {
            continue;
        };

        if text.is_empty() {
            continue;
        }
        let transliteration = if !inline_roman.is_empty() {
            inline_roman
        } else if let Some(ref k) = line_key {
            translit_by_key.get(k).cloned().unwrap_or_default()
        } else {
            String::new()
        };
        let duration = ttml_duration(start, end);
        lines.push(LyricLine {
            time_ms: start,
            duration_ms: if syllables.is_empty() && duration == 0 {
                4000
            } else {
                duration
            },
            text,
            syllables,
            transliteration,
            agent,
        });
    }

    lines.sort_by_key(|l| l.time_ms);
    lines
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct TtmlSpan {
    timing: Option<usize>,
    is_background: bool,
    is_roman: bool,
    is_translation: bool,
}

fn parse_ttml_paragraph(
    inner: &str,
    begin_attr: &Regex,
    end_attr: &Regex,
) -> Option<(String, Vec<LyricSyllable>, String)> {
    let mut reader = Reader::from_str(inner);
    let mut stack = Vec::<TtmlSpan>::new();
    let mut timings = Vec::new();
    let mut runs = Vec::<(TtmlSpan, String)>::new();
    let mut roman_parts = Vec::<String>::new();

    loop {
        let current = stack.last().copied().unwrap_or_default();
        let text = match reader.read_event().ok()? {
            Event::Start(element) => {
                let mut span = current;
                if element.local_name().as_ref().eq_ignore_ascii_case(b"span") {
                    for attr in element.attributes().flatten() {
                        if attr.key.local_name().as_ref() == b"role" {
                            let v = attr.value.as_ref();
                            if v == b"x-bg" {
                                span.is_background = true;
                            } else if v == b"x-roman" {
                                span.is_roman = true;
                            } else if v == b"x-translation" {
                                span.is_translation = true;
                            }
                        }
                    }
                    if !span.is_roman && !span.is_translation {
                        let attrs = String::from_utf8_lossy(element.attributes_raw());
                        if let (Some(begin), Some(end)) =
                            (begin_attr.captures(&attrs), end_attr.captures(&attrs))
                        {
                            span.timing = Some(timings.len());
                            timings.push((parse_ttml_time(&begin[1]), parse_ttml_time(&end[1])));
                        }
                    }
                }
                stack.push(span);
                continue;
            }
            Event::End(_) => {
                stack.pop();
                continue;
            }
            Event::Text(text) => text
                .unescape()
                .unwrap_or_else(|_| unescape_xml(&String::from_utf8_lossy(&text)).into())
                .into_owned(),
            Event::CData(text) => String::from_utf8_lossy(&text).into_owned(),
            Event::Empty(element) if element.local_name().as_ref() == b"br" => " ".into(),
            Event::Eof => break,
            _ => continue,
        };
        if current.is_translation {
            continue;
        }
        if current.is_roman {
            roman_parts.push(text);
            continue;
        }
        if let Some((_, previous_text)) = runs.last_mut().filter(|(span, _)| *span == current) {
            previous_text.push_str(&text);
        } else {
            runs.push((current, text));
        }
    }

    static WHITESPACE: OnceLock<Regex> = OnceLock::new();
    let whitespace = WHITESPACE.get_or_init(|| Regex::new(r"\s+").expect("whitespace"));
    let has_explicit_gaps = runs.windows(3).any(|w| {
        w[0].0.timing.is_some()
            && w[1].0.timing.is_none()
            && w[1].1.contains(char::is_whitespace)
            && w[2].0.timing.is_some()
    });
    // Some providers omit spaces between whole words. Backing vocal groups
    // can contain adjacent syllables within one word, so keep their spacing.
    let add_implicit_gaps = !has_explicit_gaps && runs.iter().all(|(span, _)| !span.is_background);
    let mut syllables = Vec::<LyricSyllable>::new();
    let mut pending = String::new();
    for (span, text) in runs {
        let Some(timing) = span.timing else {
            pending.push_str(&text);
            continue;
        };
        if let Some(previous) = syllables.last() {
            let needs_gap = previous.is_background != span.is_background || add_implicit_gaps;
            if needs_gap
                && pending.is_empty()
                && !previous.text.ends_with(char::is_whitespace)
                && !text.starts_with(char::is_whitespace)
            {
                pending.push(' ');
            }
        }
        pending.push_str(&text);
        let (start, end) = timings[timing];
        syllables.push(LyricSyllable {
            time_ms: start,
            duration_ms: ttml_duration(start, end),
            text: std::mem::take(&mut pending),
            is_background: span.is_background,
        });
    }

    let text = if let Some(last) = syllables.last_mut() {
        last.text.push_str(&pending);
        syllables
            .iter_mut()
            .map(|syllable| {
                syllable.text = whitespace.replace_all(&syllable.text, " ").into_owned();
                syllable.text.as_str()
            })
            .collect::<String>()
    } else {
        pending
    };
    let roman_joined = whitespace
        .replace_all(roman_parts.join("").trim(), " ")
        .into_owned();
    Some((
        whitespace.replace_all(text.trim(), " ").into_owned(),
        syllables,
        roman_joined,
    ))
}

fn ttml_duration(start: i64, end: i64) -> i64 {
    end.saturating_sub(start).clamp(0, 1_i64 << 31)
}
fn parse_ttml_time(s: &str) -> i64 {
    let mut str_val = s.trim();
    if str_val.is_empty() {
        return 0;
    }
    let stripped_ms;
    if let Some(val) = str_val.strip_suffix("ms") {
        let val = val.trim();
        if !val.contains(':') {
            return val
                .parse::<f64>()
                .map_or(0, |v| v.round() as i64);
        }
        stripped_ms = val;
        str_val = stripped_ms;
    } else if let Some(val) = str_val.strip_suffix('s') {
        str_val = val.trim();
    }

    if str_val.contains(':') {
        let mut total = 0.0_f64;
        let mut mult = 1.0_f64;
        for part in str_val.split(':').rev() {
            let Ok(n) = part.parse::<f64>() else {
                return 0;
            };
            total += n * mult;
            mult *= 60.0;
        }
        return (total * 1000.0).round() as i64;
    }
    str_val
        .parse::<f64>()
        .map_or(0, |v| (v * 1000.0).round() as i64)
}

fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

/// True for scripts whose letters join or stack (Devanagari and the other
/// Indic scripts, Thai, Lao, Khmer, Myanmar).
pub fn is_joined_script(text: &str) -> bool {
    text.chars().any(|ch| {
        let rune = ch as u32;
        (0x0900..=0x0DFF).contains(&rune)
            || (0x0E00..=0x0EFF).contains(&rune)
            || (0x1000..=0x109F).contains(&rune)
            || (0x1780..=0x17FF).contains(&rune)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_karaoke_keeps_a_held_word_past_the_next_line_start() {
        let result = LyricsResult {
            is_synced: true,
            is_word_synced: true,
            lines: vec![
                LyricLine {
                    time_ms: 1000,
                    duration_ms: 1000,
                    text: "Hold on".into(),
                    syllables: vec![
                        LyricSyllable {
                            time_ms: 1000,
                            duration_ms: 600,
                            text: "Hold ".into(),
                            ..Default::default()
                        },
                        LyricSyllable {
                            time_ms: 1600,
                            duration_ms: 900,
                            text: "on".into(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                LyricLine {
                    time_ms: 2000,
                    duration_ms: 800,
                    text: "Here we go".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let normalized = normalize_karaoke_timings(&result, "Song", "Artist");
        assert_eq!(normalized.lines[0].duration_ms, 1500);
        assert_eq!(normalized.lines[0].syllables, result.lines[0].syllables);
        assert_eq!(normalized.lines[1].time_ms, 2000);
    }

    #[test]
    fn parse_lrc_handles_offset_multi_tag_and_enhanced_stamps() {
        let lrc = "[offset: 500]\n[00:10.00]Hello world\n[00:12.50][00:25.00]Chorus line\n[00:15.00]<00:15.00>Word <00:15.50>sync";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].time_ms, 10500);
        assert_eq!(lines[0].text, "Hello world");
        assert!(!lines[0].syllables.is_empty());
        assert_eq!(lines[1].time_ms, 13000);
        assert_eq!(lines[2].time_ms, 15000);
        assert_eq!(lines[2].syllables.len(), 2);
        assert_eq!(lines[3].time_ms, 25500);
        assert!(parse_enhanced_lrc("[00:10.00]Plain only").is_empty());
    }

    #[test]
    fn active_line_and_untimed_detection() {
        let untimed = vec![
            LyricLine {
                time_ms: 0,
                text: "Line 1".to_string(),
                ..Default::default()
            },
            LyricLine {
                time_ms: 0,
                text: "Line 2".to_string(),
                ..Default::default()
            },
        ];
        assert!(lyrics_are_untimed(&untimed));
        assert_eq!(active_lyric_line_index(&untimed, 1000), -1);

        let timed = vec![
            LyricLine {
                time_ms: 1000,
                text: "A".to_string(),
                ..Default::default()
            },
            LyricLine {
                time_ms: 3000,
                text: "B".to_string(),
                ..Default::default()
            },
            LyricLine {
                time_ms: 6000,
                text: "C".to_string(),
                ..Default::default()
            },
        ];
        assert_eq!(active_lyric_line_index(&timed, 500), -1);
        assert_eq!(active_lyric_line_index(&timed, 3000), 1);
        assert_eq!(active_lyric_line_index(&timed, 9000), 2);
    }

    #[test]
    fn strip_credits_and_version_matching() {
        assert!(lyrics_same_version("Hello", "Hello (Official Video)"));
        assert!(!lyrics_same_version("Hello", "Hello (Live)"));
        assert!(lyrics_titles_match_strict("Artist - Hello", "Hello"));
        assert!(lyrics_artists_match_strict("Daft Punk feat. Pharrell", "Daft Punk"));

        let res = LyricsResult {
            lines: vec![
                LyricLine {
                    time_ms: 0,
                    text: "Lyrics by: Someone".to_string(),
                    ..Default::default()
                },
                LyricLine {
                    time_ms: 2000,
                    text: "First real sung line".to_string(),
                    ..Default::default()
                },
            ],
            is_synced: true,
            ..Default::default()
        };
        let stripped = strip_lyric_credit_lines(&res, "Song", "Artist");
        assert_eq!(stripped.lines.len(), 1);
        assert_eq!(stripped.lines[0].text, "First real sung line");
    }

    #[test]
    fn parse_ttml_and_assemble_stanzas() {
        let ttml = r#"
        <tt><body><div>
          <p begin="00:01.000" end="00:03.000" ttm:agent="v1">
            <span begin="00:01.000" end="00:02.000">Hello</span>
            <span begin="00:02.000" end="00:03.000">world</span>
          </p>
          <p begin="00:03.200" end="00:04.000">(Ooh yeah)</p>
          <p begin="00:09.000" end="00:11.000" ttm:agent="v2">Duet line</p>
        </div></body></tt>"#;
        let lines = parse_ttml(ttml);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "Hello world");
        assert_eq!(lines[0].syllables.len(), 2);

        let stanzas = assemble_stanzas(&lines);
        assert_eq!(stanzas.len(), 2);
        assert_eq!(stanzas[0].groups[0].background_sublines.len(), 1);
        assert_eq!(stanzas[1].groups[0].role, VocalRole::DuetRight);
    }

    #[test]
    fn ttml_preserves_nested_backing_vocals_and_overlapping_timing() {
        let ttml = r#"<p begin="1000ms" end="4500ms"><span begin="1000ms" end="2000ms">Lead</span> <span begin="2000ms" end="3200ms">line</span><span ttm:role='x-bg'><span begin="1800ms" end="2200ms">(ba</span><span begin="2200ms" end="2600ms">ck</span> <span begin="2600ms" end="4500ms">up)</span></span></p>"#;
        let parsed = parse_ttml(ttml);
        assert_eq!(parsed[0].text, "Lead line (back up)");
        assert_eq!(parsed[0].syllables.len(), 5);

        let normalized = normalize_karaoke_timings(
            &LyricsResult {
                lines: parsed,
                is_synced: true,
                is_word_synced: true,
                ..Default::default()
            },
            "Example title",
            "Example artist",
        );
        let words = &normalized.lines[0].syllables;
        assert_eq!(words[1].text.trim(), "line");
        assert_eq!(words[1].duration_ms, 1200);
        assert_eq!(words[2].text.trim(), "(ba");
        assert_eq!(words[2].time_ms, 1800);
        assert!(words[2..].iter().all(|word| word.is_background));
        assert_eq!(words[4].text.trim(), "up)");
    }

    #[test]
    fn ttml_preserves_punctuation_outside_timed_spans() {
        let ttml = r#"<p begin="1000ms" end="4000ms"><span begin="1000ms" end="2000ms">Lead &amp; echo</span> &#40;<span begin="2000ms" end="4000ms">again</span>)</p>"#;
        let lines = parse_ttml(ttml);
        assert_eq!(lines[0].text, "Lead & echo (again)");
        assert_eq!(lines[0].syllables[1].text.trim(), "(again)");
    }

    #[test]
    fn ttml_extracts_itunes_transliteration_and_inline_roman_and_preserves_agent() {
        let ttml = r#"<tt><head><metadata><itunes:transliteration><text for="L1">konnichiwa sekai</text></itunes:transliteration></metadata></head><body><div><p begin="1000ms" end="3000ms" itunes:key="L1" ttm:agent="v2"><span begin="1000ms" end="2000ms">こんにちは</span><span begin="2000ms" end="3000ms">世界</span></p><p begin="3500ms" end="5000ms"><span begin="3500ms" end="5000ms">ありがとう</span><span ttm:role="x-roman">arigatou</span></p></div></body></tt>"#;
        let parsed = parse_ttml(ttml);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].text, "こんにちは 世界");
        assert_eq!(parsed[0].transliteration, "konnichiwa sekai");
        assert_eq!(parsed[0].agent.as_deref(), Some("v2"));
        assert_eq!(parsed[1].text, "ありがとう");
        assert_eq!(parsed[1].transliteration, "arigatou");

        let normalized = normalize_karaoke_timings(
            &LyricsResult {
                lines: parsed,
                is_synced: true,
                is_word_synced: true,
                ..Default::default()
            },
            "Song",
            "Artist",
        );
        assert_eq!(normalized.lines[0].agent.as_deref(), Some("v2"));
        assert_eq!(normalized.lines[0].transliteration, "konnichiwa sekai");
        assert_eq!(normalized.lines[1].transliteration, "arigatou");
    }
}
