//! External playlist link support: detect the provider behind a pasted
//! string and pull out its playlist id. Public links only - no account,
//! no API keys, no OAuth anywhere in this feature.
//!
//! Ported from `lib/features/library/playlist_link.dart`.

use std::collections::{HashSet, VecDeque};
use std::sync::LazyLock;
use std::time::Duration;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Supported external playlist providers for the paste-a-link importer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaylistLinkSource {
    Youtube,
    Spotify,
    AppleMusic,
}

impl PlaylistLinkSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Youtube => "YouTube",
            Self::Spotify => "Spotify",
            Self::AppleMusic => "Apple Music",
        }
    }
}

static SPOTIFY_ID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"spotify\.com/(?:[a-z]{2,4}(?:-[a-z]{2})?/)?playlist/([A-Za-z0-9]+)").unwrap()
});
static SPOTIFY_URI_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"spotify:playlist:([A-Za-z0-9]+)").unwrap());
static APPLE_ID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"music\.apple\.com/[a-z]{2}(?:-[a-z]{2})?/playlist/(?:[^/]+/)?(pl\.[A-Za-z0-9]+)")
        .unwrap()
});

/// Best-effort provider detection from a pasted string.
pub fn detect_playlist_link(raw: &str) -> Option<PlaylistLinkSource> {
    let value = raw.trim().to_lowercase();
    if value.is_empty() {
        return None;
    }
    if value.contains("spotify.com")
        || value.contains("spotify.link")
        || value.starts_with("spotify:")
    {
        return Some(PlaylistLinkSource::Spotify);
    }
    if value.contains("music.apple.com") {
        return Some(PlaylistLinkSource::AppleMusic);
    }
    if value.contains("list=")
        || value.contains("playlist/")
        || value.contains("youtu.be")
        || value.contains("youtube.com")
        || value.contains("music.youtube.com")
    {
        return Some(PlaylistLinkSource::Youtube);
    }
    None
}

fn non_empty(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Provider-specific playlist id, or `None` when the string is not a
/// playlist link for `source`.
pub fn extract_playlist_id(raw: &str, source: PlaylistLinkSource) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    match source {
        PlaylistLinkSource::Spotify => SPOTIFY_ID_REGEX
            .captures(value)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .or_else(|| {
                SPOTIFY_URI_REGEX
                    .captures(value)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
            }),
        PlaylistLinkSource::AppleMusic => APPLE_ID_REGEX
            .captures(value)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string()),
        PlaylistLinkSource::Youtube => {
            if let Some(idx) = value.find("list=") {
                let after = &value[idx + "list=".len()..];
                let end = after.find(['&', '#']).unwrap_or(after.len());
                return non_empty(&after[..end]);
            }
            if let Some(idx) = value.find("playlist/") {
                let after = &value[idx + "playlist/".len()..];
                let end = after.find(['?', '/']).unwrap_or(after.len());
                return non_empty(&after[..end]);
            }
            non_empty(value)
        }
    }
}

/// One row read off a provider page, before it is matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistLinkRow {
    pub title: String,
    pub artist: String,
}

impl PlaylistLinkRow {
    pub fn new(title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            artist: artist.into(),
        }
    }
}

/// A parsed provider playlist - the preview payload shown before importing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistLinkPage {
    pub source: PlaylistLinkSource,
    pub title: String,
    pub rows: Vec<PlaylistLinkRow>,
}

static NEXT_DATA_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r#"<script[^>]*\bid=["']__NEXT_DATA__["'][^>]*>(.*?)</script>"#)
        .dot_matches_new_line(true)
        .build()
        .unwrap()
});

const SPOTIFY_TRACK_URI: &str = "spotify:track:";
const SPOTIFY_PLAYLIST_URI: &str = "spotify:playlist:";

/// Spotify embed parser: `__NEXT_DATA__` JSON in, rows out. Pure, never
/// fails - unreadable input yields empty rows.
pub fn parse_spotify_embed(html: &str, fallback_title: Option<&str>) -> PlaylistLinkPage {
    let fallback = fallback_title.unwrap_or("Spotify Playlist");
    let Some(payload) = NEXT_DATA_REGEX
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .filter(|s| !s.trim().is_empty())
    else {
        return PlaylistLinkPage {
            source: PlaylistLinkSource::Spotify,
            title: fallback.to_string(),
            rows: Vec::new(),
        };
    };

    let Ok(root) = serde_json::from_str::<Value>(&unescape_embed_html(payload)) else {
        return PlaylistLinkPage {
            source: PlaylistLinkSource::Spotify,
            title: fallback.to_string(),
            rows: Vec::new(),
        };
    };

    let mut title: Option<String> = None;
    let mut rows = Vec::new();
    let mut seen_uris = HashSet::new();
    let mut pending = VecDeque::new();
    pending.push_back(&root);

    while let Some(current) = pending.pop_front() {
        match current {
            Value::Object(map) => {
                if let (Some(Value::String(uri)), Some(Value::String(name))) =
                    (map.get("uri"), map.get("title"))
                {
                    if uri.starts_with(SPOTIFY_PLAYLIST_URI) {
                        if title.is_none() {
                            title = non_empty(&clean_spotify(name));
                        }
                    } else if uri.starts_with(SPOTIFY_TRACK_URI) {
                        let track_title = clean_spotify(name);
                        if !track_title.is_empty() && seen_uris.insert(uri.clone()) {
                            let artist = map
                                .get("subtitle")
                                .and_then(|v| match v {
                                    Value::String(s) => Some(s.as_str()),
                                    _ => None,
                                })
                                .map(clean_spotify)
                                .unwrap_or_default();
                            rows.push(PlaylistLinkRow::new(track_title, artist));
                        }
                    }
                }
                for val in map.values() {
                    pending.push_back(val);
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    pending.push_back(val);
                }
            }
            _ => {}
        }
    }

    let resolved_title = title
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| fallback.to_string());

    PlaylistLinkPage {
        source: PlaylistLinkSource::Spotify,
        title: resolved_title,
        rows,
    }
}

/// Collapses non-breaking spaces (Spotify pads artist lists with them).
fn clean_spotify(raw: &str) -> String {
    raw.replace('\u{00a0}', " ").trim().to_string()
}

/// The embed JSON is HTML-escaped inside the script tag (`&amp;` last).
fn unescape_embed_html(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

// -- Apple Music ------------------------------------------------------------
// Tier 1 (`serialized-server-data`, carries artists), then tier 2
// (`schema:music-playlist` ld+json, names only) as a fallback.

static SERVER_DATA_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r#"<script[^>]*\bid\s*=\s*["']?serialized-server-data["']?[^>]*>(.*?)</script>"#,
    )
    .dot_matches_new_line(true)
    .case_insensitive(true)
    .build()
    .unwrap()
});

static SCHEMA_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r#"<script[^>]*\bid\s*=\s*["']?schema:music-playlist["']?[^>]*>(.*?)</script>"#,
    )
    .dot_matches_new_line(true)
    .case_insensitive(true)
    .build()
    .unwrap()
});

static TITLE_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"<title[^>]*>(.*?)</title>")
        .dot_matches_new_line(true)
        .case_insensitive(true)
        .build()
        .unwrap()
});

static BIDI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("[\u{200E}\u{200F}\u{202A}-\u{202E}\u{2066}-\u{2069}\u{FEFF}]").unwrap()
});

static APPLE_SUFFIX_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\s*[-\u{2013}\u{2014}]\s*(?:Playlist\s*[-\u{2013}\u{2014}]\s*)?Apple\s*Music\s*$",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});

static WHITESPACE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

const APPLE_DEFAULT_TITLE: &str = "Apple Music Playlist";
const MAX_JSON_DEPTH: usize = 256;

/// Apple page parser: tier 1 rows, else tier 2. Pure, never fails.
pub fn parse_apple_page(html: &str) -> PlaylistLinkPage {
    PlaylistLinkPage {
        source: PlaylistLinkSource::AppleMusic,
        title: parse_apple_title(html),
        rows: parse_apple_rows(html),
    }
}

fn parse_apple_rows(html: &str) -> Vec<PlaylistLinkRow> {
    let server_rows = parse_apple_server_rows(html);
    if !server_rows.is_empty() {
        return server_rows;
    }
    parse_apple_ld_rows(html)
}

fn parse_apple_server_rows(html: &str) -> Vec<PlaylistLinkRow> {
    let Some(content) = script_content(&SERVER_DATA_REGEX, html) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };

    let mut rows = Vec::new();
    let mut seen = HashSet::new();

    fn collect(
        element: &Value,
        depth: usize,
        rows: &mut Vec<PlaylistLinkRow>,
        seen: &mut HashSet<String>,
    ) {
        if depth > MAX_JSON_DEPTH {
            return;
        }
        match element {
            Value::Object(map) => {
                if let (Some(Value::String(title)), Some(Value::String(artist))) =
                    (map.get("title"), map.get("artistName"))
                {
                    let trimmed_title = title.trim();
                    if !trimmed_title.is_empty() {
                        let row = PlaylistLinkRow::new(trimmed_title, artist.trim());
                        let key = format!(
                            "{}|{}",
                            row.title.to_lowercase(),
                            row.artist.to_lowercase()
                        );
                        if seen.insert(key) {
                            rows.push(row);
                        }
                    }
                }
                for val in map.values() {
                    collect(val, depth + 1, rows, seen);
                }
            }
            Value::Array(arr) => {
                for val in arr {
                    collect(val, depth + 1, rows, seen);
                }
            }
            _ => {}
        }
    }

    collect(&root, 0, &mut rows, &mut seen);
    rows
}

fn parse_apple_ld_rows(html: &str) -> Vec<PlaylistLinkRow> {
    let Some(content) = script_content(&SCHEMA_REGEX, html) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };
    let Some(tracks) = find_array(&root, "track", 0) else {
        return Vec::new();
    };

    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for entry in tracks {
        let title = entry
            .as_object()
            .and_then(|m| m.get("name"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or("");
        if title.is_empty() {
            continue;
        }
        if seen.insert(title.to_lowercase()) {
            rows.push(PlaylistLinkRow::new(title, ""));
        }
    }
    rows
}

fn parse_apple_title(html: &str) -> String {
    if let Some(schema_content) = script_content(&SCHEMA_REGEX, html) {
        if let Ok(root) = serde_json::from_str::<Value>(schema_content) {
            if let Some(name) = find_name(&root) {
                let cleaned = clean_apple_title(&name);
                if !cleaned.is_empty() {
                    return cleaned;
                }
            }
        }
    }
    if let Some(tag) = TITLE_TAG_REGEX
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
    {
        let cleaned = clean_apple_title(&decode_apple_entities(tag));
        if !cleaned.is_empty() {
            return cleaned;
        }
    }
    APPLE_DEFAULT_TITLE.to_string()
}

fn find_name(element: &Value) -> Option<String> {
    match element {
        Value::Object(map) => {
            if let Some(Value::String(name)) = map.get("name") {
                return Some(name.clone());
            }
            for val in map.values() {
                if let Some(found) = find_name(val) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => {
            for item in arr {
                if let Some(found) = find_name(item) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn find_array<'a>(element: &'a Value, key: &str, depth: usize) -> Option<&'a Vec<Value>> {
    if depth > MAX_JSON_DEPTH {
        return None;
    }
    match element {
        Value::Object(map) => {
            if let Some(Value::Array(direct)) = map.get(key) {
                return Some(direct);
            }
            for val in map.values() {
                if let Some(found) = find_array(val, key, depth + 1) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => {
            for item in arr {
                if let Some(found) = find_array(item, key, depth + 1) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn script_content<'a>(pattern: &Regex, html: &'a str) -> Option<&'a str> {
    let content = pattern
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim())?;
    if content.is_empty() {
        None
    } else {
        Some(content)
    }
}

fn decode_apple_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

fn clean_apple_title(value: &str) -> String {
    let no_bidi = BIDI_REGEX.replace_all(value, "");
    let collapsed = WHITESPACE_REGEX.replace_all(&no_bidi, " ");
    let trimmed = collapsed.trim();
    let no_suffix = APPLE_SUFFIX_REGEX.replace(trimmed, "");
    let result = no_suffix.trim();
    if result.is_empty() {
        APPLE_DEFAULT_TITLE.to_string()
    } else {
        result.to_string()
    }
}

// -- Fetchers ----------------------------------------------------------------
// Keyless page downloads. Bad links fail fast with Format error (no HTTP);
// transport failures propagate as FetchFailed.

const SPOTIFY_EMBED_BASE: &str = "https://open.spotify.com/embed/playlist/";
const APPLE_FALLBACK_BASE: &str = "https://music.apple.com/us/playlist/";
const DESKTOP_WINDOWS_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const DESKTOP_MAC_UA: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, thiserror::Error)]
pub enum PlaylistLinkError {
    #[error("{0}")]
    InvalidFormat(String),
    #[error("{0}")]
    FetchFailed(String),
}

/// Mix/radio ids are never browsable playlists (explicit guard so a
/// regex change elsewhere cannot silently start importing mixes).
fn is_mix_id(raw: &str) -> bool {
    raw.trim().to_uppercase().starts_with("RD")
}

/// Downloads a public Spotify playlist page and parses its track list.
/// `spotify.link` short URLs are followed to their canonical playlist
/// (reqwest follows the HTTP redirect; the id is re-read from the final URI).
pub async fn fetch_spotify_playlist(
    client: &reqwest::Client,
    url_or_id: &str,
) -> Result<PlaylistLinkPage, PlaylistLinkError> {
    if is_mix_id(url_or_id) {
        return Err(PlaylistLinkError::InvalidFormat(
            "Mixes and radio cannot be imported. Paste a playlist link instead.".to_string(),
        ));
    }
    let mut id = extract_playlist_id(url_or_id, PlaylistLinkSource::Spotify);
    if id.is_none() && url_or_id.trim().to_lowercase().contains("spotify.link") {
        id = expand_short_link(client, url_or_id.trim()).await;
    }
    let Some(id) = id.filter(|s| !s.is_empty()) else {
        return Err(PlaylistLinkError::InvalidFormat(
            "That does not look like a Spotify playlist link.".to_string(),
        ));
    };

    let url = format!("{SPOTIFY_EMBED_BASE}{id}");
    let res = tokio::time::timeout(
        FETCH_TIMEOUT,
        client
            .get(&url)
            .header("User-Agent", DESKTOP_WINDOWS_UA)
            .header("Accept", "text/html")
            .send(),
    )
    .await
    .map_err(|_| {
        PlaylistLinkError::FetchFailed(
            "Could not load that Spotify playlist. Check the link is public and try again."
                .to_string(),
        )
    })?
    .map_err(|_| {
        PlaylistLinkError::FetchFailed(
            "Could not load that Spotify playlist. Check the link is public and try again."
                .to_string(),
        )
    })?;

    let status = res.status().as_u16();
    let body = res.text().await.unwrap_or_default();
    if status != 200 || body.trim().is_empty() {
        return Err(PlaylistLinkError::FetchFailed(format!(
            "Spotify playlist page returned HTTP {status}."
        )));
    }

    Ok(parse_spotify_embed(&body, None))
}

/// Follows a short link to its canonical playlist id (`None` when the
/// target is not a playlist link).
async fn expand_short_link(client: &reqwest::Client, raw: &str) -> Option<String> {
    let mut target = raw.trim().to_string();
    if !target.to_lowercase().starts_with("http") {
        target = format!("https://{target}");
    }
    let res = tokio::time::timeout(
        FETCH_TIMEOUT,
        client
            .get(&target)
            .header("User-Agent", DESKTOP_WINDOWS_UA)
            .send(),
    )
    .await
    .ok()?
    .ok()?;
    let final_url = res.url().as_str();
    extract_playlist_id(final_url, PlaylistLinkSource::Spotify)
}

/// Downloads a public Apple Music playlist page and parses it.
/// Accepts a full URL, a scheme-less host path, an id-bearing path, or
/// a bare `pl.<id>` (fetched from the `us` storefront by id redirect).
pub async fn fetch_apple_playlist(
    client: &reqwest::Client,
    url_or_id: &str,
) -> Result<PlaylistLinkPage, PlaylistLinkError> {
    if is_mix_id(url_or_id) {
        return Err(PlaylistLinkError::InvalidFormat(
            "Mixes and radio cannot be imported. Paste a playlist link instead.".to_string(),
        ));
    }
    let input = url_or_id.trim();
    let lower = input.to_lowercase();
    let is_url = lower.starts_with("http://") || lower.starts_with("https://");
    let raw = if !is_url && lower.contains("music.apple.com") {
        format!("https://{input}")
    } else {
        input.to_string()
    };
    let id = extract_playlist_id(&raw, PlaylistLinkSource::AppleMusic);
    let bare_id = if lower.starts_with("pl.") {
        Some(input)
    } else {
        None
    };

    let target = if is_url {
        input.to_string()
    } else if let Some(id) = id {
        format!("{APPLE_FALLBACK_BASE}{id}")
    } else if let Some(bare) = bare_id {
        format!("{APPLE_FALLBACK_BASE}{bare}")
    } else {
        return Err(PlaylistLinkError::InvalidFormat(
            "That does not look like an Apple Music playlist link.".to_string(),
        ));
    };

    let mut body: Option<String> = None;
    if let Ok(Ok(res)) = tokio::time::timeout(
        FETCH_TIMEOUT,
        client
            .get(&target)
            .header("User-Agent", DESKTOP_MAC_UA)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", "en-US,en;q=0.9")
            .send(),
    )
    .await
    {
        if res.status().is_success() {
            body = res.text().await.ok();
        }
    }

    let Some(body) = body.filter(|b| !b.trim().is_empty()) else {
        return Err(PlaylistLinkError::FetchFailed(
            "Could not load that Apple Music playlist. Check the link and try again.".to_string(),
        ));
    };

    Ok(parse_apple_page(&body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_providers() {
        assert_eq!(
            detect_playlist_link("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M"),
            Some(PlaylistLinkSource::Spotify)
        );
        assert_eq!(
            detect_playlist_link("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"),
            Some(PlaylistLinkSource::Spotify)
        );
        assert_eq!(
            detect_playlist_link(
                "https://music.apple.com/us/playlist/todays-hits/pl.f4d106fed2bd41149aaacabb233eb5eb"
            ),
            Some(PlaylistLinkSource::AppleMusic)
        );
        assert_eq!(
            detect_playlist_link("https://music.youtube.com/playlist?list=PL12345"),
            Some(PlaylistLinkSource::Youtube)
        );
        assert_eq!(detect_playlist_link("   "), None);
    }

    #[test]
    fn extract_ids() {
        assert_eq!(
            extract_playlist_id(
                "https://open.spotify.com/intl-en/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc",
                PlaylistLinkSource::Spotify
            ),
            Some("37i9dQZF1DXcBWIGoYBM5M".to_string())
        );
        assert_eq!(
            extract_playlist_id(
                "spotify:playlist:abc123XYZ",
                PlaylistLinkSource::Spotify
            ),
            Some("abc123XYZ".to_string())
        );
        assert_eq!(
            extract_playlist_id(
                "https://music.apple.com/gb/album/chill/12345",
                PlaylistLinkSource::AppleMusic
            ),
            None
        );
        assert_eq!(
            extract_playlist_id(
                "https://music.apple.com/gb/playlist/chill/pl.12345abc",
                PlaylistLinkSource::AppleMusic
            ),
            Some("pl.12345abc".to_string())
        );
        assert_eq!(
            extract_playlist_id(
                "https://www.youtube.com/watch?v=abc&list=PLxyz123#t=10",
                PlaylistLinkSource::Youtube
            ),
            Some("PLxyz123".to_string())
        );
    }

    #[test]
    fn parse_spotify_embed_extracts_title_and_tracks() {
        let html = r#"
        <html><head></head><body>
        <script id="__NEXT_DATA__" type="application/json">
        {"props":{"pageProps":{"state":{"data":{"entity":{
            "uri":"spotify:playlist:123",
            "title":"Late Night &amp; Chill",
            "trackList":[
                {"uri":"spotify:track:t1","title":"Song One","subtitle":"Artist\u00a0A"},
                {"uri":"spotify:track:t1","title":"Song One","subtitle":"Artist A"},
                {"uri":"spotify:track:t2","title":"Song Two","subtitle":"Artist B"}
            ]
        }}}}}}
        </script></body></html>"#;

        let page = parse_spotify_embed(html, None);
        assert_eq!(page.source, PlaylistLinkSource::Spotify);
        assert_eq!(page.title, "Late Night & Chill");
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[0], PlaylistLinkRow::new("Song One", "Artist A"));
        assert_eq!(page.rows[1], PlaylistLinkRow::new("Song Two", "Artist B"));
    }

    #[test]
    fn parse_apple_page_uses_server_data_and_cleans_title() {
        let html = r#"
        <html>
        <head><title>\u200ELo-Fi Beats - Playlist - Apple Music</title></head>
        <body>
        <script id="serialized-server-data" type="application/json">
        {"data":[{"title":"Midnight","artistName":"Nujabes"},{"title":"Midnight","artistName":"Nujabes"},{"title":"Feather","artistName":"Nujabes"}]}
        </script>
        </body></html>"#;

        let page = parse_apple_page(html);
        assert_eq!(page.source, PlaylistLinkSource::AppleMusic);
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[0], PlaylistLinkRow::new("Midnight", "Nujabes"));
        assert_eq!(page.rows[1], PlaylistLinkRow::new("Feather", "Nujabes"));
    }

    #[test]
    fn parse_apple_page_falls_back_to_ld_json() {
        let html = r#"
        <html>
        <head><title>Ignored Title</title></head>
        <body>
        <script id="schema:music-playlist" type="application/ld+json">
        {"name":"Morning Coffee — Apple Music","track":[{"name":"Track A"},{"name":"Track A"},{"name":"Track B"}]}
        </script>
        </body></html>"#;

        let page = parse_apple_page(html);
        assert_eq!(page.title, "Morning Coffee");
        assert_eq!(
            page.rows,
            vec![
                PlaylistLinkRow::new("Track A", ""),
                PlaylistLinkRow::new("Track B", ""),
            ]
        );
    }
}
