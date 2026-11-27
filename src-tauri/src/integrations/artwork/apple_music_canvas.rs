//! Apple Music's motion artwork — animated sleeves exposed on the catalog API
//! as `editorialVideo`.
//!
//! Port of `lib/core/artwork/apple_music_canvas.dart`.
//! Scrapes an anonymous guest JWT from Apple Music Web Player bundles, queries
//! the AMP catalog search with `extend=editorialVideo`, ranks and validates
//! results against artist/title/album, and returns square/portrait HLS video
//! streams.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use regex::Regex;
use serde_json::Value;
use tokio::sync::Mutex;

use super::{
    canvas_artists_match, canvas_get_text, canvas_titles_match, normalize_canvas_match,
    primary_canvas_artist, AnimatedArtwork, CanvasSource, CANVAS_USER_AGENT,
};

const AMP_BASE: &str = "https://amp-api.music.apple.com/v1/catalog";
const WEB_PLAYER: &str = "https://music.apple.com/us/browse";
const MIN_SCORE: i32 = 12;
const TOKEN_RETRY_MS: i64 = 30 * 60 * 1000;

const EDITION_WORDS: &[&str] = &[
    "deluxe",
    "expanded",
    "remastered",
    "remix",
    "version",
    "edit",
    "mix",
    "bonus",
];

const COMPILATION_MARKERS: &[&str] = &[
    "playlist",
    "set list",
    "essentials",
    "dj mix",
    "mixed",
    "apple music",
    "today's hits",
    "session",
];

#[derive(Default)]
struct TokenState {
    cached_token: Option<String>,
    expires_at_ms: i64,
    retry_after_ms: i64,
    rejected: HashSet<String>,
}

static TOKEN_STATE: LazyLock<Mutex<TokenState>> =
    LazyLock::new(|| Mutex::new(TokenState::default()));
static TOKEN_INFLIGHT: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

static STOREFRONT_COUNTRY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z]{2}$").unwrap());
static DIGITS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());
static SCRIPT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/assets/index(?:-legacy)?[~-][A-Za-z0-9_-]+\.js").unwrap());
static JWT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"ey[A-Za-z0-9_-]+\.ey[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+").unwrap());
static EXP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""exp"\s*:\s*(\d+)"#).unwrap());

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub struct AppleMusicCanvas;

impl AppleMusicCanvas {
    pub fn storefront() -> String {
        if let Ok(locale) = std::env::var("LANG").or_else(|_| std::env::var("LC_ALL")) {
            let parts: Vec<&str> = locale.split(['-', '_', '.']).collect();
            if parts.len() >= 2 {
                let country = parts[1].trim().to_lowercase();
                if country.len() == 2 && STOREFRONT_COUNTRY_RE.is_match(&country) {
                    return country;
                }
            }
        }
        "us".to_string()
    }

    /// Clears cached JWT state (used by unit tests).
    pub async fn reset_for_testing() {
        let mut guard = TOKEN_STATE.lock().await;
        *guard = TokenState::default();
    }

    pub async fn search(
        client: &reqwest::Client,
        title: &str,
        artist: &str,
        album: Option<&str>,
    ) -> Option<AnimatedArtwork> {
        let bearer = Self::token(client).await?;
        if bearer.is_empty() {
            return None;
        }

        let primary_artist = primary_canvas_artist(artist);
        let query_artist = if primary_artist.is_empty() {
            artist.to_string()
        } else {
            primary_artist
        };
        let mut term = String::new();
        if !title.to_lowercase().contains(&query_artist.to_lowercase()) {
            term.push_str(&query_artist);
            term.push(' ');
        }
        term.push_str(title);
        if let Some(alb) = album {
            if !alb.trim().is_empty() && !title.to_lowercase().contains(&alb.to_lowercase()) {
                term.push(' ');
                term.push_str(alb);
            }
        }

        let url = format!("{AMP_BASE}/{}/search", Self::storefront());
        let root = Self::get_json(
            client,
            &url,
            &bearer,
            &[
                ("term", &term),
                ("types", "songs"),
                ("limit", "10"),
                ("extend", "editorialVideo"),
                ("include", "albums"),
            ],
        )
        .await?;

        let hits = root
            .get("results")
            .and_then(Value::as_object)
            .and_then(|r| r.get("songs"))
            .and_then(Value::as_object)
            .and_then(|s| s.get("data"))
            .and_then(Value::as_array)?;

        let mut ranked: Vec<(i32, &serde_json::Map<String, Value>)> = Vec::new();
        for hit in hits {
            let Some(song) = hit.as_object() else {
                continue;
            };
            if let Some(s) = Self::score(song, title, artist, album, false) {
                ranked.push((s, song));
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0));

        for (score, song) in ranked {
            if score < MIN_SCORE {
                break;
            }
            let Some(attributes) = song.get("attributes").and_then(Value::as_object) else {
                continue;
            };
            let song_name = attributes.get("name").and_then(Value::as_str).map(str::to_string);
            let song_artist = attributes
                .get("artistName")
                .and_then(Value::as_str)
                .map(str::to_string);
            let album_name = attributes
                .get("albumName")
                .and_then(Value::as_str)
                .map(str::to_string);

            if let Some(video) = attributes.get("editorialVideo").and_then(Value::as_object) {
                if let Some((primary, alternate, tall)) = Self::motion_urls(video) {
                    return Some(AnimatedArtwork {
                        url: primary,
                        fallback_url: alternate,
                        url_tall: tall.unwrap_or_default(),
                        title: song_name,
                        artist: song_artist,
                        album: album_name,
                        source: CanvasSource::Apple,
                    });
                }
            }

            let Some(id) = Self::album_id(song) else {
                continue;
            };
            if let Some(from_album) = Self::fetch_album(
                client,
                &id,
                &bearer,
                song_name.as_deref(),
                song_artist.as_deref(),
            )
            .await
            {
                return Some(from_album);
            }
        }
        None
    }

    pub async fn search_album(
        client: &reqwest::Client,
        album: &str,
        artist: &str,
    ) -> Option<AnimatedArtwork> {
        let bearer = Self::token(client).await?;
        if bearer.is_empty() {
            return None;
        }

        let primary_artist = primary_canvas_artist(artist);
        let query_artist = if primary_artist.is_empty() {
            artist.to_string()
        } else {
            primary_artist
        };
        let term = if album.to_lowercase().contains(&query_artist.to_lowercase()) {
            album.to_string()
        } else {
            format!("{query_artist} {album}")
        };

        let url = format!("{AMP_BASE}/{}/search", Self::storefront());
        let root = Self::get_json(
            client,
            &url,
            &bearer,
            &[
                ("term", &term),
                ("types", "albums"),
                ("limit", "10"),
                ("extend", "editorialVideo"),
            ],
        )
        .await?;

        let hits = root
            .get("results")
            .and_then(Value::as_object)
            .and_then(|r| r.get("albums"))
            .and_then(Value::as_object)
            .and_then(|a| a.get("data"))
            .and_then(Value::as_array)?;

        let mut ranked: Vec<(i32, &serde_json::Map<String, Value>)> = Vec::new();
        for hit in hits {
            let Some(record) = hit.as_object() else {
                continue;
            };
            if let Some(s) = Self::score(record, album, artist, Some(album), true) {
                ranked.push((s, record));
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0));

        for (score, record) in ranked {
            if score < MIN_SCORE {
                break;
            }
            let Some(attributes) = record.get("attributes").and_then(Value::as_object) else {
                continue;
            };
            let name = attributes.get("name").and_then(Value::as_str);
            if let Some(n) = name {
                if Self::is_compilation(n) {
                    continue;
                }
            }
            let Some(video) = attributes.get("editorialVideo").and_then(Value::as_object) else {
                continue;
            };
            let Some((primary, alternate, tall)) = Self::motion_urls(video) else {
                continue;
            };
            return Some(AnimatedArtwork {
                url: primary,
                fallback_url: alternate,
                url_tall: tall.unwrap_or_default(),
                title: None,
                artist: attributes
                    .get("artistName")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                album: name.map(str::to_string),
                source: CanvasSource::Apple,
            });
        }
        None
    }

    fn score(
        song: &serde_json::Map<String, Value>,
        title: &str,
        artist: &str,
        album: Option<&str>,
        album_is_self: bool,
    ) -> Option<i32> {
        let attributes = song.get("attributes")?.as_object()?;
        let hit_name = attributes.get("name").and_then(Value::as_str).unwrap_or("");
        let hit_artist = attributes
            .get("artistName")
            .and_then(Value::as_str)
            .unwrap_or("");
        let hit_album = if album_is_self {
            hit_name
        } else {
            attributes
                .get("albumName")
                .and_then(Value::as_str)
                .unwrap_or("")
        };

        if Self::is_compilation(hit_name) || Self::is_compilation(hit_album) {
            return None;
        }
        if !canvas_artists_match(artist, hit_artist, Some(title), Some(hit_name)) {
            return None;
        }

        let mut score = 10;
        let want_title = normalize_canvas_match(title);
        let hit_title = normalize_canvas_match(hit_name);
        if hit_title == want_title {
            score += 15;
        } else if canvas_titles_match(title, hit_name) {
            score += 7;
        } else {
            score -= 10;
        }

        if let Some(alb) = album.filter(|a| !a.trim().is_empty()) {
            if !hit_album.trim().is_empty() {
                let want_album = normalize_canvas_match(alb);
                let got_album = normalize_canvas_match(hit_album);
                if got_album == want_album {
                    score += 20;
                } else if canvas_titles_match(alb, hit_album) {
                    score += 10;
                }
            }
        }

        let lower_title = title.to_lowercase();
        let lower_hit = hit_name.to_lowercase();
        for word in EDITION_WORDS {
            let in_wanted = lower_title.contains(word);
            let in_hit = lower_hit.contains(word);
            if in_wanted && in_hit {
                score += 5;
            } else if in_hit {
                score -= 3;
            }
        }

        Some(score)
    }

    fn is_compilation(name: &str) -> bool {
        let lower = name.to_lowercase();
        COMPILATION_MARKERS.iter().any(|m| lower.contains(m))
    }

    fn album_id(song: &serde_json::Map<String, Value>) -> Option<String> {
        if let Some(data) = song
            .get("relationships")
            .and_then(Value::as_object)
            .and_then(|r| r.get("albums"))
            .and_then(Value::as_object)
            .and_then(|a| a.get("data"))
            .and_then(Value::as_array)
        {
            if let Some(first) = data.first().and_then(Value::as_object) {
                if let Some(id) = first.get("id").and_then(Value::as_str) {
                    if !id.is_empty() && !id.starts_with("pl.") {
                        return Some(id.to_string());
                    }
                }
            }
        }

        let url = song
            .get("attributes")
            .and_then(Value::as_object)
            .and_then(|a| a.get("url"))
            .and_then(Value::as_str)?;
        let idx = url.find("/album/")?;
        let after_album = &url[idx + "/album/".len()..];
        let before_query = after_album.split('?').next().unwrap_or("");
        let last_segment = before_query.split('/').next_back().unwrap_or("").trim();
        if !last_segment.is_empty() && DIGITS_RE.is_match(last_segment) {
            return Some(last_segment.to_string());
        }
        None
    }

    async fn fetch_album(
        client: &reqwest::Client,
        album_id: &str,
        bearer: &str,
        song_title: Option<&str>,
        song_artist: Option<&str>,
    ) -> Option<AnimatedArtwork> {
        let url = format!("{AMP_BASE}/{}/albums/{album_id}", Self::storefront());
        let root = Self::get_json(client, &url, bearer, &[("extend", "editorialVideo")]).await?;
        let data = root.get("data").and_then(Value::as_array)?;
        let album = data.first()?.as_object()?;
        let attributes = album.get("attributes")?.as_object()?;
        let album_name = attributes.get("name").and_then(Value::as_str).unwrap_or("");
        if Self::is_compilation(album_name) {
            return None;
        }
        let video = attributes.get("editorialVideo")?.as_object()?;
        let (primary, alternate, tall) = Self::motion_urls(video)?;

        Some(AnimatedArtwork {
            url: primary,
            fallback_url: alternate,
            url_tall: tall.unwrap_or_default(),
            title: song_title.map(str::to_string),
            artist: song_artist.map(str::to_string).or_else(|| {
                attributes
                    .get("artistName")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
            album: Some(album_name.to_string()),
            source: CanvasSource::Apple,
        })
    }

    fn motion_urls(
        video: &serde_json::Map<String, Value>,
    ) -> Option<(String, Option<String>, Option<String>)> {
        let link = |key: &str| -> Option<String> {
            let asset = video.get(key)?.as_object()?;
            let raw = asset
                .get("video")
                .or_else(|| asset.get("videoUrl"))
                .or_else(|| asset.get("hlsUrl"))
                .or_else(|| asset.get("url"))
                .and_then(Value::as_str)?
                .trim();
            if raw.is_empty() {
                None
            } else {
                Some(raw.to_string())
            }
        };

        let square = link("motionDetailSquare").or_else(|| link("motionSquareVideo1x1"));
        let raw = link("motionDetailRaw");
        let tall = link("motionDetailTall").or_else(|| link("motionTallVideo3x4"));
        let primary = square
            .clone()
            .or_else(|| raw.clone())
            .or_else(|| tall.clone())?;
        let alternate = [square, raw, tall.clone()]
            .into_iter()
            .flatten()
            .find(|c| c != &primary);
        Some((primary, alternate, tall))
    }

    async fn token(client: &reqwest::Client) -> Option<String> {
        let now = now_ms();
        {
            let guard = TOKEN_STATE.lock().await;
            if let Some(ref cached) = guard.cached_token {
                if now < guard.expires_at_ms - 60_000 {
                    return Some(cached.clone());
                }
            }
            if now < guard.retry_after_ms {
                return None;
            }
        }
        let _inflight = TOKEN_INFLIGHT.lock().await;
        Self::scrape_token(client).await
    }

    async fn scrape_token(client: &reqwest::Client) -> Option<String> {
        let now = now_ms();
        {
            let guard = TOKEN_STATE.lock().await;
            if let Some(ref cached) = guard.cached_token {
                if now < guard.expires_at_ms - 60_000 {
                    return Some(cached.clone());
                }
            }
            if now < guard.retry_after_ms {
                return None;
            }
        }

        let headers = HashMap::new();
        let html = canvas_get_text(client, WEB_PLAYER, &headers, &[]).await;
        let scripts: Vec<String> = match html {
            Some(ref body) => {
                let mut seen = HashSet::new();
                SCRIPT_RE
                    .find_iter(body)
                    .map(|m| m.as_str().to_string())
                    .filter(|s| seen.insert(s.clone()))
                    .collect()
            }
            None => Vec::new(),
        };

        for path in scripts {
            let url = format!("https://music.apple.com{path}");
            let Some(script) = canvas_get_text(client, &url, &headers, &[]).await else {
                continue;
            };
            if script.is_empty() {
                continue;
            }

            let mut seen = HashSet::new();
            let mut candidates: Vec<(String, i64)> = Vec::new();
            let rejected_snapshot = TOKEN_STATE.lock().await.rejected.clone();

            for m in JWT_RE.find_iter(&script) {
                let jwt = m.as_str().to_string();
                if !seen.insert(jwt.clone()) || rejected_snapshot.contains(&jwt) {
                    continue;
                }
                if let Some(exp) = Self::expiry(&jwt) {
                    if exp > now {
                        candidates.push((jwt, exp));
                    }
                }
            }
            if candidates.is_empty() {
                continue;
            }

            let chosen = candidates
                .iter()
                .find(|(jwt, _)| Self::is_web_player_token(jwt))
                .cloned()
                .unwrap_or_else(|| candidates[0].clone());

            let mut guard = TOKEN_STATE.lock().await;
            guard.cached_token = Some(chosen.0.clone());
            guard.expires_at_ms = chosen.1;
            return Some(chosen.0);
        }

        let mut guard = TOKEN_STATE.lock().await;
        guard.retry_after_ms = now + TOKEN_RETRY_MS;
        None
    }

    async fn get_json(
        client: &reqwest::Client,
        url: &str,
        bearer: &str,
        query_params: &[(&str, &str)],
    ) -> Option<serde_json::Map<String, Value>> {
        let res = client
            .get(url)
            .query(query_params)
            .header("Authorization", format!("Bearer {bearer}"))
            .header("Origin", "https://music.apple.com")
            .header("Referer", "https://music.apple.com/")
            .header("User-Agent", CANVAS_USER_AGENT)
            .send()
            .await
            .ok()?;

        let status = res.status().as_u16();
        if status == 401 {
            let mut guard = TOKEN_STATE.lock().await;
            guard.rejected.insert(bearer.to_string());
            if guard.cached_token.as_deref() == Some(bearer) {
                guard.cached_token = None;
                guard.expires_at_ms = 0;
            }
            return None;
        }
        if !(200..300).contains(&status) {
            return None;
        }
        let text = res.text().await.ok()?;
        match serde_json::from_str::<Value>(&text).ok()? {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    fn is_web_player_token(jwt: &str) -> bool {
        let parts: Vec<&str> = jwt.split('.').collect();
        if parts.len() < 2 {
            return false;
        }
        let Ok(h_bytes) = URL_SAFE_NO_PAD.decode(parts[0].trim_end_matches('=')) else {
            return false;
        };
        let Ok(p_bytes) = URL_SAFE_NO_PAD.decode(parts[1].trim_end_matches('=')) else {
            return false;
        };
        let header = String::from_utf8_lossy(&h_bytes);
        let payload = String::from_utf8_lossy(&p_bytes);
        header.contains("WebPlayKid") || payload.contains("AMPWebPlay")
    }

    fn expiry(jwt: &str) -> Option<i64> {
        let parts: Vec<&str> = jwt.split('.').collect();
        if parts.len() < 2 {
            return None;
        }
        let p_bytes = URL_SAFE_NO_PAD
            .decode(parts[1].trim_end_matches('='))
            .ok()?;
        let payload = String::from_utf8_lossy(&p_bytes);
        let seconds: i64 = EXP_RE
            .captures(&payload)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse().ok())?;
        Some(seconds * 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_helpers_decode_exp_and_webplayer() {
        let header = URL_SAFE_NO_PAD.encode(r#"{"kid":"WebPlayKid"}"#);
        let payload = URL_SAFE_NO_PAD.encode(r#"{"iss":"AMPWebPlay","exp":1800000000}"#);
        let jwt = format!("{header}.{payload}.sig");
        assert!(AppleMusicCanvas::is_web_player_token(&jwt));
        assert_eq!(AppleMusicCanvas::expiry(&jwt), Some(1_800_000_000_000));
    }

    #[test]
    fn motion_urls_prefers_square() {
        let v: serde_json::Map<String, Value> = serde_json::from_str(
            r#"{
                "motionDetailSquare":{"video":"https://example.com/sq.m3u8"},
                "motionDetailTall":{"video":"https://example.com/tall.m3u8"}
            }"#,
        )
        .unwrap();
        let (p, alt, tall) = AppleMusicCanvas::motion_urls(&v).unwrap();
        assert_eq!(p, "https://example.com/sq.m3u8");
        assert_eq!(alt.as_deref(), Some("https://example.com/tall.m3u8"));
        assert_eq!(tall.as_deref(), Some("https://example.com/tall.m3u8"));
    }
}
