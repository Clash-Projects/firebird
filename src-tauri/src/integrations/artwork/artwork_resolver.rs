//! Central artwork pipeline — URL normalization, sizing, deduplication,
//! and disk-cache resolution (pure backend logic; no Flutter widget layer).
//!
//! Port of `lib/core/artwork/artwork_resolver.dart` + `artwork_cache.dart`
//! bucket ladder.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtworkKind {
    Track,
    Album,
    Artist,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkRequest {
    pub kind: ArtworkKind,
    pub candidates: Vec<String>,
    pub label: String,
    pub target_px: f64,
    pub video_id: String,
}

impl Default for ArtworkRequest {
    fn default() -> Self {
        Self {
            kind: ArtworkKind::Track,
            candidates: Vec::new(),
            label: String::new(),
            target_px: 300.0,
            video_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedArtwork {
    pub urls: Vec<String>,
    pub is_fallback: bool,
}

/// Shared size-ladder from `ArtworkCache` so every layout width snaps to a
/// canonical pixel edge.
pub const ARTWORK_LADDER: &[u32] = &[64, 128, 256, 384, 512, 640, 768, 1024];

pub fn artwork_bucket(needed: f64) -> Option<u32> {
    if needed <= 0.0 || needed.is_nan() {
        return None;
    }
    let n = needed.ceil() as u32;
    for &step in ARTWORK_LADDER {
        if n <= step {
            return Some(step);
        }
    }
    ARTWORK_LADDER.last().copied()
}

const BAD_TOKENS: &[&str] = &["", "null", "undefined", "n/a", "na", "none"];

const TLS_HOSTS: &[&str] = &[
    "ytimg.com",
    "googleusercontent.com",
    "ggpht.com",
    "last.fm",
    "lastfm",
    "deezer.com",
    "dzcdn.net",
    "spotifycdn.com",
    "scdn.co",
    "mzstatic.com",
    "apple.com",
    "i.scdn.co",
    "qobuz.com",
    "static.qobuz.com",
];

static WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static YT_VIDEO_ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\w-]{11}$").unwrap());
static MZSTATIC_SIZE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+x\d+bb").unwrap());
static QOBUZ_SIZE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"_\d+\.jpg$").unwrap());
static GUSER_PARAM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"=w\d+(-h\d+)?(-c)?(-p)?(-l\d+)?(-rj)?").unwrap());
static GUSER_PATH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/w\d+(-h\d+)?/").unwrap());

pub struct ArtworkResolver;

impl ArtworkResolver {
    /// Track hierarchy: track -> album -> related -> generated fallback.
    pub fn track(
        track_art: &str,
        album_art: &str,
        related_art: &str,
        label: &str,
        target_px: f64,
    ) -> ArtworkRequest {
        ArtworkRequest {
            kind: ArtworkKind::Track,
            candidates: vec![
                track_art.to_string(),
                album_art.to_string(),
                related_art.to_string(),
            ],
            label: label.to_string(),
            target_px,
            video_id: String::new(),
        }
    }

    /// Album hierarchy: album -> representative track -> generated.
    pub fn album(
        album_art: &str,
        representative_art: &str,
        label: &str,
        target_px: f64,
    ) -> ArtworkRequest {
        ArtworkRequest {
            kind: ArtworkKind::Album,
            candidates: vec![album_art.to_string(), representative_art.to_string()],
            label: label.to_string(),
            target_px,
            video_id: String::new(),
        }
    }

    /// Artist hierarchy: real image -> associated -> generated.
    pub fn artist(
        artist_image: &str,
        associated_art: &str,
        label: &str,
        target_px: f64,
    ) -> ArtworkRequest {
        ArtworkRequest {
            kind: ArtworkKind::Artist,
            candidates: vec![artist_image.to_string(), associated_art.to_string()],
            label: label.to_string(),
            target_px,
            video_id: String::new(),
        }
    }

    pub fn resolve(request: &ArtworkRequest) -> ResolvedArtwork {
        let mut primary = Vec::new();
        let mut primary_seen = HashSet::new();
        let mut fallback = Vec::new();
        let mut fallback_seen = HashSet::new();

        let push_unique = |vec: &mut Vec<String>, seen: &mut HashSet<String>, val: String| {
            if seen.insert(val.clone()) {
                vec.push(val);
            }
        };

        for raw in &request.candidates {
            let Some(n) = Self::normalize(raw) else {
                continue;
            };
            let Ok(parsed) = url::Url::parse(&n) else {
                continue;
            };
            let host = parsed.host_str().unwrap_or("");
            let is_youtube = ["ytimg.com", "googleusercontent.com", "ggpht.com"]
                .iter()
                .any(|d| host == *d || host.ends_with(&format!(".{d}")));

            let sized_url = Self::sized(&n, request.target_px);
            if is_youtube {
                push_unique(&mut fallback, &mut fallback_seen, sized_url);
                push_unique(&mut fallback, &mut fallback_seen, n.clone());
            } else {
                push_unique(&mut primary, &mut primary_seen, sized_url);
                push_unique(&mut primary, &mut primary_seen, n.clone());
            }

            if host == "i.ytimg.com" || host.ends_with(".ytimg.com") {
                if let Some(segs) = parsed.path_segments() {
                    let parts: Vec<&str> = segs.collect();
                    if parts.len() == 3 {
                        let id = parts[1];
                        if YT_VIDEO_ID_RE.is_match(id) {
                            push_unique(
                                &mut fallback,
                                &mut fallback_seen,
                                format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"),
                            );
                        }
                    }
                }
            }
        }

        let id = request.video_id.trim();
        if YT_VIDEO_ID_RE.is_match(id) {
            push_unique(
                &mut fallback,
                &mut fallback_seen,
                format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"),
            );
            push_unique(
                &mut fallback,
                &mut fallback_seen,
                format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"),
            );
            if request.target_px >= 400.0 {
                push_unique(
                    &mut fallback,
                    &mut fallback_seen,
                    format!("https://i.ytimg.com/vi/{id}/maxresdefault.jpg"),
                );
            }
        }

        let mut urls = Vec::with_capacity(primary.len() + fallback.len());
        let mut all_seen = HashSet::new();
        for u in primary.into_iter().chain(fallback.into_iter()) {
            if all_seen.insert(u.clone()) {
                urls.push(u);
            }
        }
        let is_fallback = urls.is_empty();
        ResolvedArtwork { urls, is_fallback }
    }

    /// Normalize without destroying CDN signatures. Returns `None` when the
    /// URL is unusable so callers advance the chain instead of fetching it.
    pub fn normalize(raw: &str) -> Option<String> {
        let mut s = WHITESPACE_RE.replace_all(raw.trim(), "").to_string();
        if s.is_empty() {
            return None;
        }
        let lower = s.to_lowercase();
        if BAD_TOKENS.contains(&lower.as_str()) {
            return None;
        }
        if lower.contains("2a96cbd8")
            || lower.contains("default_album")
            || lower.contains("noimage")
        {
            return None;
        }
        if s.starts_with("//") {
            s = format!("https:{s}");
        }
        let mut uri = match url::Url::parse(&s) {
            Ok(u) => u,
            Err(url::ParseError::RelativeUrlWithoutBase) => url::Url::parse(&format!("https://{s}")).ok()?,
            Err(_) => return None,
        };
        if uri.scheme() != "http" && uri.scheme() != "https" {
            return None;
        }
        let host = uri.host_str()?.to_lowercase();
        if host.is_empty() {
            return None;
        }
        if uri.scheme() == "http" && TLS_HOSTS.iter().any(|h| host.contains(h)) {
            let _ = uri.set_scheme("https");
        }
        let out = uri.to_string();
        if out.len() < 12 || !out.contains('.') {
            return None;
        }
        Some(out)
    }

    /// Pick an appropriate rendition for recognized patterns only.
    pub fn sized(url: &str, target_px: f64) -> String {
        let px = artwork_bucket(target_px.clamp(64.0, 1024.0)).unwrap_or(64);
        if url.contains("mzstatic.com") || url.contains("apple.com") {
            let mut next = MZSTATIC_SIZE_RE
                .replace_all(url, &format!("{px}x{px}bb"))
                .to_string();
            next = next
                .replace("{w}", &px.to_string())
                .replace("{h}", &px.to_string())
                .replace("{f}", "jpg");
            return next;
        }
        if url.contains("qobuz.com") && px >= 400 {
            return QOBUZ_SIZE_RE.replace(url, "_600.jpg").to_string();
        }
        if url.contains("googleusercontent.com") || url.contains("ggpht.com") {
            let next = GUSER_PARAM_RE
                .replace_all(url, &format!("=w{px}-h{px}-l90-rj"))
                .to_string();
            if next != url {
                return next;
            }
            let next2 = GUSER_PATH_RE
                .replace_all(url, &format!("/w{px}-h{px}/"))
                .to_string();
            if next2 != url {
                return next2;
            }
        }
        if url.contains("ytimg.com") && px < 300 && url.contains("hqdefault") {
            return url.replace("hqdefault", "mqdefault");
        }
        url.to_string()
    }

    /// Disk cache path for a normalized+sized artwork URL under the app support dir.
    pub fn disk_cache_path(cache_dir: &std::path::Path, url: &str) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        let hex_key = hex::encode(hasher.finalize());
        cache_dir.join(format!("{hex_key}.img"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_filters_bad_and_upgrades_tls() {
        assert_eq!(ArtworkResolver::normalize("null"), None);
        assert_eq!(ArtworkResolver::normalize("   "), None);
        assert_eq!(
            ArtworkResolver::normalize("http://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg"),
            Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg".to_string())
        );
        assert_eq!(
            ArtworkResolver::normalize("//is1-ssl.mzstatic.com/image/thumb/100x100bb.jpg"),
            Some("https://is1-ssl.mzstatic.com/image/thumb/100x100bb.jpg".to_string())
        );
    }

    #[test]
    fn sized_snaps_to_bucket() {
        assert_eq!(
            ArtworkResolver::sized(
                "https://is1-ssl.mzstatic.com/image/thumb/100x100bb.jpg",
                300.0
            ),
            "https://is1-ssl.mzstatic.com/image/thumb/384x384bb.jpg"
        );
        assert_eq!(
            ArtworkResolver::sized("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg", 160.0),
            "https://i.ytimg.com/vi/dQw4w9WgXcQ/mqdefault.jpg"
        );
    }

    #[test]
    fn resolve_prioritizes_official_over_youtube() {
        let req = ArtworkRequest {
            kind: ArtworkKind::Track,
            candidates: vec![
                "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg".to_string(),
                "https://is1-ssl.mzstatic.com/image/thumb/100x100bb.jpg".to_string(),
            ],
            label: "test".to_string(),
            target_px: 300.0,
            video_id: "dQw4w9WgXcQ".to_string(),
        };
        let resolved = ArtworkResolver::resolve(&req);
        assert!(!resolved.is_fallback);
        assert!(resolved.urls[0].contains("mzstatic.com"));
    }
}
