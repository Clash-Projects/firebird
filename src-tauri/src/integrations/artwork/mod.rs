pub mod animated_artwork_service;
pub mod animated_artwork_session;
pub mod apple_music_canvas;
pub mod artwork_resolver;
pub mod community_canvas;
pub mod official_artwork_service;
pub mod tidal_canvas;

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

pub const CANVAS_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Which provider a motion artwork clip came from.
///
/// Priority order: `Apple` > `Tidal` > `Community`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CanvasSource {
    Apple,
    Tidal,
    Community,
}

/// Looping motion artwork clip (Apple Music editorialVideo HLS/fMP4,
/// Tidal 1280x1280 video cover MP4, or Community Canvas manifest MP4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimatedArtwork {
    pub url: String,
    pub url_tall: String,
    pub fallback_url: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub source: CanvasSource,
}

impl AnimatedArtwork {
    pub fn has_url(&self) -> bool {
        !self.url.is_empty()
    }

    /// Whether this clip really belongs to the track we asked about.
    pub fn matches(&self, want_title: &str, want_artist: &str, want_album: Option<&str>) -> bool {
        if !self.artist.as_deref().is_some_and(|artist| {
            confirmed_artist_match(want_artist, artist)
        }) {
            return false;
        }

        let album = want_album.filter(|album| !album.trim().is_empty());
        if album.is_some_and(|album| {
            !self.album.as_deref().is_some_and(|candidate| {
                confirmed_album_match(album, candidate)
            })
        }) {
            return false;
        }

        match self.title.as_deref().filter(|title| !title.trim().is_empty()) {
            Some(title) if !want_title.trim().is_empty() => confirmed_track_match(want_title, title),
            Some(_) => album.is_some(),
            None => album.is_some() && self.source != CanvasSource::Community,
        }
    }
}

static TRACK_CREDIT_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*(?:\((?:feat\.? |ft\.? |featuring |from )[^)]*\)|\[(?:feat\.? |ft\.? |featuring |from )[^\]]*\]|\s+-\s+(?:feat\.? |ft\.? |featuring ).*)$").unwrap()
});
static RELEASE_TYPE_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s+-\s+(?:single|ep)\s*$").unwrap());
static METADATA_PUNCTUATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}\s]").unwrap());

fn normalize_confirmed_metadata(value: &str) -> String {
    let letters: String = value
        .nfd()
        .filter(|ch| !unicode_normalization::char::is_combining_mark(*ch))
        .flat_map(char::to_lowercase)
        .collect();
    METADATA_PUNCTUATION.replace_all(&letters, " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn confirmed_track_match(expected: &str, candidate: &str) -> bool {
    let expected = normalize_confirmed_metadata(&TRACK_CREDIT_SUFFIX.replace(expected.trim(), ""));
    !expected.is_empty() && expected == normalize_confirmed_metadata(&TRACK_CREDIT_SUFFIX.replace(candidate.trim(), ""))
}

pub(super) fn confirmed_album_match(expected: &str, candidate: &str) -> bool {
    let expected = normalize_confirmed_metadata(&RELEASE_TYPE_SUFFIX.replace(expected, ""));
    !expected.is_empty() && expected == normalize_confirmed_metadata(&RELEASE_TYPE_SUFFIX.replace(candidate, ""))
}

pub(super) fn confirmed_artist_match(expected: &str, candidate: &str) -> bool {
    let expected = normalize_confirmed_metadata(&primary_canvas_artist(expected));
    !expected.is_empty() && expected == normalize_confirmed_metadata(&primary_canvas_artist(candidate))
}

static NON_ALNUM_SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9\s]").unwrap());
static MULTI_SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static SUBTITLE_BRACKETS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*(?:\([^)]*\)|\[[^\]]*\])\s*").unwrap());
static TRAILING_DASH_TAG: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\s+-\s+(?:single|ep|deluxe|remaster(?:ed)?|explicit|bonus.*|feat\.?.*|ft\.?.*)$",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static ARTIST_SEPARATORS: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"(?:\s*,\s*|\s*&\s*|\s+×\s+|\s+x\s+|\bfeat\.?\b|\bft\.?\b|\bfeaturing\b|\bwith\b)",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static CANVAS_NOISE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\((?:from|official|lyrical|video|audio)[^)]*\)|\[[^\]]*\]|\b(?:official (?:video|audio|music video)|lyrical|full song|4k video)\b",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});

/// Normalizes case, accents and punctuation between platforms.
pub fn normalize_canvas_match(raw: &str) -> String {
    let stripped_accents: String = raw
        .nfd()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .collect();
    let lower = stripped_accents.to_lowercase();
    let alnum = NON_ALNUM_SPACE.replace_all(&lower, " ");
    MULTI_SPACE.replace_all(&alnum, " ").trim().to_string()
}

/// Strips parenthetical/bracketed subtitles such as `(feat. Daft Punk)`.
pub fn strip_canvas_subtitles(raw: &str) -> String {
    let s1 = SUBTITLE_BRACKETS.replace_all(raw, " ");
    let s2 = TRAILING_DASH_TAG.replace_all(&s1, " ");
    let s3 = MULTI_SPACE.replace_all(&s2, " ");
    let trimmed = s3.trim();
    if trimmed.is_empty() {
        raw.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Compares two track or album titles across catalog conventions.
pub fn canvas_titles_match(want_title: &str, got_title: &str) -> bool {
    let n_want = normalize_canvas_match(want_title);
    let n_got = normalize_canvas_match(got_title);
    if n_want.is_empty() || n_got.is_empty() {
        return false;
    }
    if n_want == n_got || n_got.contains(&n_want) || n_want.contains(&n_got) {
        return true;
    }
    let s_want = normalize_canvas_match(&strip_canvas_subtitles(want_title));
    let s_got = normalize_canvas_match(&strip_canvas_subtitles(got_title));
    if !s_want.is_empty() && !s_got.is_empty() {
        if s_want == s_got || s_got.contains(&s_want) || s_want.contains(&s_got) {
            return true;
        }
    }
    false
}

/// Extracts the primary artist name.
pub fn primary_canvas_artist(raw: &str) -> String {
    for part in ARTIST_SEPARATORS.split(raw) {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    raw.trim().to_string()
}

/// Strips YouTube/video noise markers before querying canvas providers.
pub fn clean_canvas_query(raw: &str) -> String {
    let s1 = CANVAS_NOISE.replace_all(raw, " ");
    let first = s1.split(" | ").next().unwrap_or("");
    let s2 = MULTI_SPACE.replace_all(first, " ");
    let trimmed = s2.trim();
    if trimmed.is_empty() {
        raw.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Plain GET returning response text, or `None` on non-2xx or failure.
pub async fn canvas_get_text(
    client: &reqwest::Client,
    url: &str,
    headers: &HashMap<String, String>,
    query_params: &[(&str, &str)],
) -> Option<String> {
    let mut req = client.get(url).header("User-Agent", CANVAS_USER_AGENT);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if !query_params.is_empty() {
        req = req.query(query_params);
    }
    let res = req.send().await.ok()?;
    if !res.status().is_success() {
        return None;
    }
    res.text().await.ok()
}

/// Plain GET returning a decoded JSON object map, or `None` on non-2xx/failure.
pub async fn canvas_get_json(
    client: &reqwest::Client,
    url: &str,
    headers: &HashMap<String, String>,
    query_params: &[(&str, &str)],
) -> Option<serde_json::Map<String, Value>> {
    let text = canvas_get_text(client, url, headers, query_params).await?;
    match serde_json::from_str::<Value>(&text).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}
