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
        let title_ok = match &self.title {
            None => true,
            Some(_) if want_title.trim().is_empty() => true,
            Some(t) => canvas_titles_match(want_title, t),
        };

        let artist_ok = match &self.artist {
            None => true,
            Some(_) if want_artist.trim().is_empty() => true,
            Some(a) => canvas_artists_match(
                want_artist,
                a,
                Some(want_title),
                self.title.as_deref(),
            ),
        };

        let n_album = self
            .album
            .as_deref()
            .map(normalize_canvas_match)
            .unwrap_or_default();
        let n_want_album = want_album.map(normalize_canvas_match).unwrap_or_default();
        let s_album = self
            .album
            .as_deref()
            .map(|a| normalize_canvas_match(&strip_canvas_subtitles(a)))
            .unwrap_or_default();
        let s_want_album = want_album
            .map(|a| normalize_canvas_match(&strip_canvas_subtitles(a)))
            .unwrap_or_default();

        let album_ok = n_album.is_empty()
            || n_want_album.is_empty()
            || n_album == n_want_album
            || n_album.contains(&n_want_album)
            || n_want_album.contains(&n_album)
            || (!s_album.is_empty()
                && !s_want_album.is_empty()
                && (s_album == s_want_album
                    || s_album.contains(&s_want_album)
                    || s_want_album.contains(&s_album)));

        title_ok && artist_ok && album_ok
    }
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

/// Compares credited artists across platforms.
pub fn canvas_artists_match(
    want_artist: &str,
    got_artist: &str,
    want_title: Option<&str>,
    got_title: Option<&str>,
) -> bool {
    let wanted = split_canvas_artists(want_artist);
    let credited = split_canvas_artists(got_artist);
    if wanted.is_empty() || credited.is_empty() {
        let n_want = normalize_canvas_match(want_artist);
        let n_got = normalize_canvas_match(got_artist);
        if n_want.is_empty() || n_got.is_empty() {
            return false;
        }
        return n_want == n_got || n_got.contains(&n_want) || n_want.contains(&n_got);
    }

    let n_got_title = got_title.map(normalize_canvas_match).unwrap_or_default();
    let n_want_title = want_title.map(normalize_canvas_match).unwrap_or_default();

    let token_matches_list = |token: &str, pool: &[String], extra_title: &str| -> bool {
        if pool
            .iter()
            .any(|it| it == token || it.contains(token) || token.contains(it.as_str()))
        {
            return true;
        }
        !extra_title.is_empty() && extra_title.contains(token)
    };

    if wanted
        .iter()
        .all(|w| token_matches_list(w, &credited, &n_got_title))
    {
        return true;
    }
    credited
        .iter()
        .all(|c| token_matches_list(c, &wanted, &n_want_title))
}

/// Splits credited artists across varying separators.
pub fn split_canvas_artists(raw: &str) -> Vec<String> {
    ARTIST_SEPARATORS
        .split(raw)
        .map(normalize_canvas_match)
        .filter(|s| !s.is_empty())
        .collect()
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
