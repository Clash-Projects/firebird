//! Resolves official studio album artwork and album metadata from high-authority
//! music catalog CDNs (iTunes / Apple Music store catalog + Deezer).
//!
//! Ported from `lib/core/artwork/official_artwork_service.dart`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{Mutex, Semaphore};

use crate::models::artist_parser::{primary_artist_name, split_artist_credits};
use crate::storage::app_database::AppDatabase;

use super::{confirmed_album_match, confirmed_artist_match, confirmed_track_match, known_cover_album};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialArtworkResult {
    pub artwork_url: String,
    pub album_title: String,
    pub artist: String,
    pub title: String,
}

impl OfficialArtworkResult {
    pub fn new(
        artwork_url: impl Into<String>,
        album_title: impl Into<String>,
        artist: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            artwork_url: artwork_url.into(),
            album_title: album_title.into(),
            artist: artist.into(),
            title: title.into(),
        }
    }
}

const MAX_PARALLEL: usize = 3;

const OFFICIAL_CDNS: &[&str] = &[
    "mzstatic.com",
    "apple.com",
    "qobuz.com",
    "scdn.co",
    "spotifycdn.com",
    "deezer.com",
    "dzcdn.net",
    "tidal.com",
    "resources.tidal.com",
];

const TIDAL_SEARCH_URL: &str = "https://api.tidal.com/v1/search";
const TIDAL_EMBED_TOKEN: &str = "vNVdglQOjFJJGG2U";
const QOBUZ_APP_ID: &str = "950096963";

const YOUTUBE_CDNS: &[&str] = &["ytimg.com", "googleusercontent.com", "ggpht.com"];

const ARTIST_STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "of", "feat", "ft", "with", "x",
];

static APOSTROPHES_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"['’ʼ`´]").unwrap());
static DASH_NOISE_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"\s*-\s*(?:official|video|audio|remaster|remastered|lyrics?).*$")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static PAREN_NOISE_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\s*\([^)]*(?:feat|ft\.?|featuring|official|video|audio|remaster|visualizer|lyrics?|explicit|clean)[^)]*\)",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static BRACKET_NOISE_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\s*\[[^\]]*(?:feat|ft\.?|featuring|official|video|audio|remaster|visualizer|lyrics?|explicit|clean)[^\]]*\]",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static PREFIX_ARTIST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[\w&.\- ]+\s*-\s+").unwrap());
static DOLLAR_DIGIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$(\d)").unwrap());
static NON_ALNUM_UNI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}]+").unwrap());
static MULTI_WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static SIZE_BB_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+x\d+bb").unwrap());
static FEAT_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"[(\[]\s*(?:feat|ft\.?|featuring)\b")
        .case_insensitive(true)
        .build()
        .unwrap()
});

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Default)]
struct ServiceState {
    cache: HashMap<String, OfficialArtworkResult>,
    cache_order: Vec<String>,
    fallback_cache: HashMap<String, OfficialArtworkResult>,
    fallback_order: Vec<String>,
    itunes_403_streak: u32,
    itunes_skip_until_ms: i64,
    spotify_token: Option<(String, i64)>,
    spotify_skip_until_ms: i64,
    qobuz_skip_until_ms: i64,
}

impl ServiceState {
    fn insert_cache(&mut self, key: String, val: OfficialArtworkResult) {
        self.cache_order.retain(|k| k != &key);
        self.cache.insert(key.clone(), val);
        self.cache_order.push(key);
        if self.cache_order.len() > 500 {
            let oldest = self.cache_order.remove(0);
            self.cache.remove(&oldest);
        }
    }

    fn insert_fallback(&mut self, key: String, val: OfficialArtworkResult) {
        self.fallback_order.retain(|k| k != &key);
        self.fallback_cache.insert(key.clone(), val);
        self.fallback_order.push(key);
        if self.fallback_order.len() > 500 {
            let oldest = self.fallback_order.remove(0);
            self.fallback_cache.remove(&oldest);
        }
    }
}

pub struct OfficialArtworkService {
    http: reqwest::Client,
    db: Mutex<Option<Arc<AppDatabase>>>,
    state: Mutex<ServiceState>,
    semaphore: Semaphore,
    inflight: Mutex<HashMap<String, Arc<tokio::sync::Notify>>>,
}

impl OfficialArtworkService {
    pub fn new(http: Option<reqwest::Client>, db: Option<Arc<AppDatabase>>) -> Self {
        Self {
            http: http.unwrap_or_else(Self::create_artwork_client),
            db: Mutex::new(db),
            state: Mutex::new(ServiceState::default()),
            semaphore: Semaphore::new(MAX_PARALLEL),
            inflight: Mutex::new(HashMap::new()),
        }
    }

    pub fn create_artwork_client() -> reqwest::Client {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
            ),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("*/*"),
        );
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            reqwest::header::HeaderValue::from_static("en-US,en;q=0.9"),
        );
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(8))
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    }

    pub async fn set_db(&self, db: Arc<AppDatabase>) {
        *self.db.lock().await = Some(db);
    }

    fn track_cache_key(artist: &str, title: &str) -> String {
        format!(
            "t5|{}|{}",
            Self::normalize_for_search(artist),
            Self::normalize_for_search(title)
        )
    }

    fn album_cache_key(artist: &str, album: &str) -> String {
        format!(
            "album2|{}|{}",
            Self::normalize_for_search(artist),
            Self::normalize_for_search(album)
        )
    }

    pub fn is_official_artwork(url: &str) -> bool {
        if url.is_empty() {
            return false;
        }
        let Ok(uri) = url::Url::parse(url) else {
            return false;
        };
        let Some(host) = uri.host_str().map(str::to_lowercase) else {
            return false;
        };
        if YOUTUBE_CDNS
            .iter()
            .any(|d| host == *d || host.ends_with(&format!(".{d}")))
        {
            return false;
        }
        OFFICIAL_CDNS
            .iter()
            .any(|d| host == *d || host.ends_with(&format!(".{d}")))
    }

    pub fn is_official_artist_photo(url: &str) -> bool {
        if url.is_empty() {
            return false;
        }
        let Ok(uri) = url::Url::parse(url) else {
            return false;
        };
        let Some(host) = uri.host_str().map(str::to_lowercase) else {
            return false;
        };
        let path = uri.path().to_lowercase();
        let deezer = host == "deezer.com"
            || host.ends_with(".deezer.com")
            || host == "dzcdn.net"
            || host.ends_with(".dzcdn.net");
        if !deezer {
            return false;
        }
        path.contains("/images/artist/") || path.contains("/artist/")
    }

    pub fn primary_artist(artist: &str) -> String {
        primary_artist_name(artist)
    }

    pub fn split_credits(artist: &str) -> Vec<String> {
        split_artist_credits(artist)
    }

    pub fn normalize_for_search(text: &str) -> String {
        let mut s = text.to_lowercase();
        s = APOSTROPHES_RE.replace_all(&s, "").to_string();
        s = DASH_NOISE_RE.replace_all(&s, "").to_string();
        s = PAREN_NOISE_RE.replace_all(&s, "").to_string();
        s = BRACKET_NOISE_RE.replace_all(&s, "").to_string();
        s = PREFIX_ARTIST_RE.replace_all(&s, "").to_string();
        s = DOLLAR_DIGIT_RE.replace_all(&s, "$1").to_string();
        s = s.replace('$', "s");
        s = NON_ALNUM_UNI_RE.replace_all(&s, " ").to_string();
        MULTI_WS_RE.replace_all(&s, " ").trim().to_string()
    }

    fn artists_aliased(a: &str, b: &str) -> bool {
        if a.is_empty() || b.is_empty() {
            return false;
        }
        if a == b {
            return true;
        }
        const GROUPS: &[&[&str]] = &[
            &["madvillain", "mf doom", "mfdoom", "doom", "metal fingers"],
            &["jay z", "jayz", "jigga", "shawn carter"],
            &["kanye west", "ye", "yeezy"],
            &["the weeknd", "weeknd"],
        ];
        GROUPS
            .iter()
            .any(|g| g.contains(&a) && g.contains(&b))
    }

    fn is_deezer_blank(url: &str) -> bool {
        url.contains("/artist//") || url.contains("d41d8cd98f00b204e9800998ecf8427e")
    }

    pub async fn remember_track(
        &self,
        title: &str,
        artist: &str,
        artwork_url: &str,
        album: &str,
    ) {
        if !Self::is_official_artwork(artwork_url) || Self::normalize_for_search(title).is_empty() {
            return;
        }
        let key = Self::track_cache_key(artist, title);
        let result = OfficialArtworkResult::new(artwork_url, album, artist, title);
        self.state.lock().await.insert_cache(key.clone(), result);
        if let Some(ref db) = *self.db.lock().await {
            db.save_artwork_entry(&key, artwork_url, "remember");
        }
    }

    pub async fn peek_track(&self, title: &str, artist: &str) -> Option<OfficialArtworkResult> {
        if Self::normalize_for_search(title).is_empty() {
            return None;
        }
        let key = Self::track_cache_key(artist, title);
        if let Some(res) = self.state.lock().await.cache.get(&key).cloned() {
            return Some(res);
        }
        if let Some(ref db) = *self.db.lock().await {
            if let Some(row) = db.load_artwork_entry(&key) {
                if !row.url.is_empty() {
                    let res = OfficialArtworkResult::new(row.url, "", "", "");
                    self.state.lock().await.insert_cache(key, res.clone());
                    return Some(res);
                }
            }
        }
        None
    }

    pub async fn peek_display_track(
        &self,
        title: &str,
        artist: &str,
    ) -> Option<OfficialArtworkResult> {
        if let Some(official) = self.peek_track(title, artist).await {
            return Some(official);
        }
        let key = Self::track_cache_key(artist, title);
        self.state.lock().await.fallback_cache.get(&key).cloned()
    }

    pub async fn peek_album(&self, album: &str, artist: &str) -> Option<OfficialArtworkResult> {
        if Self::normalize_for_search(album).is_empty() {
            return None;
        }
        let key = Self::album_cache_key(artist, album);
        if let Some(res) = self.state.lock().await.cache.get(&key).cloned() {
            return Some(res);
        }
        if let Some(ref db) = *self.db.lock().await {
            if let Some(row) = db.load_artwork_entry(&key) {
                if !row.url.is_empty() {
                    let res = OfficialArtworkResult::new(row.url, "", "", "");
                    self.state.lock().await.insert_cache(key, res.clone());
                    return Some(res);
                }
            }
        }
        None
    }

    pub async fn peek_artist(&self, artist: &str) -> Option<OfficialArtworkResult> {
        let clean = Self::normalize_for_search(&primary_artist_name(artist));
        if clean.is_empty() {
            return None;
        }
        let key = format!("artist4|{clean}");
        if let Some(res) = self.state.lock().await.cache.get(&key).cloned() {
            return Some(res);
        }
        if let Some(ref db) = *self.db.lock().await {
            if let Some(row) = db.load_artwork_entry(&key) {
                if !row.url.is_empty() && !Self::is_deezer_blank(&row.url) {
                    let res = OfficialArtworkResult::new(row.url, "", "", "");
                    self.state.lock().await.insert_cache(key, res.clone());
                    return Some(res);
                }
            }
        }
        None
    }

    async fn cached_resolve<F, Fut>(
        &self,
        cache_key: String,
        provider: &str,
        resolve: F,
    ) -> Option<OfficialArtworkResult>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Option<OfficialArtworkResult>>,
    {
        if let Some(res) = self.state.lock().await.cache.get(&cache_key).cloned() {
            return Some(res);
        }

        if let Some(ref db) = *self.db.lock().await {
            if let Some(row) = db.load_artwork_entry(&cache_key) {
                if !row.url.is_empty()
                    && !Self::is_deezer_blank(&row.url)
                    && (Self::is_official_artwork(&row.url)
                        || row.url.contains("googleusercontent.com")
                        || row.url.contains("ggpht.com"))
                {
                    let res = OfficialArtworkResult::new(row.url, "", "", "");
                    self.state
                        .lock()
                        .await
                        .insert_cache(cache_key.clone(), res.clone());
                    return Some(res);
                }
            }
        }

        // Dedup concurrent requests for the same key
        loop {
            let maybe_notify = {
                let mut map = self.inflight.lock().await;
                if let Some(n) = map.get(&cache_key) {
                    Some(Arc::clone(n).notified_owned())
                } else {
                    map.insert(cache_key.clone(), Arc::new(tokio::sync::Notify::new()));
                    None
                }
            };
            if let Some(wait) = maybe_notify {
                wait.await;
                if let Some(res) = self.state.lock().await.cache.get(&cache_key).cloned() {
                    return Some(res);
                }
                return None;
            }
            break;
        }

        let _permit = self.semaphore.acquire().await.ok();
        let result = resolve().await;

        let valid = result.filter(|r| Self::is_official_artwork(&r.artwork_url));
        if let Some(ref res) = valid {
            self.state
                .lock()
                .await
                .insert_cache(cache_key.clone(), res.clone());
            if let Some(ref db) = *self.db.lock().await {
                if !res.artwork_url.is_empty() {
                    db.save_artwork_entry(&cache_key, &res.artwork_url, provider);
                }
            }
        }

        if let Some(notify) = self.inflight.lock().await.remove(&cache_key) {
            notify.notify_waiters();
        }
        valid
    }

    pub async fn resolve_official_artwork(
        &self,
        title: &str,
        artist: &str,
        album: &str,
    ) -> Option<OfficialArtworkResult> {
        let clean_title = Self::normalize_for_search(title);
        let clean_artist = Self::normalize_for_search(artist);
        if clean_title.is_empty() {
            return None;
        }
        let key = Self::track_cache_key(artist, title);
        self.cached_resolve(key, "catalog", || async {
            self.do_resolve(&clean_title, &clean_artist, title, artist, album)
                .await
        })
        .await
    }

    pub async fn resolve_verified_cover(
        &self,
        title: &str,
        artist: &str,
        album: &str,
    ) -> Option<OfficialArtworkResult> {
        let album = known_cover_album(title, album).unwrap_or_default();
        let clean_title = Self::normalize_for_search(title);
        let clean_artist = Self::normalize_for_search(artist);
        if clean_title.is_empty() || clean_artist.is_empty() {
            return None;
        }
        let key = format!("verified-cover|{}", serde_json::json!([artist, album, title]));
        self.cached_resolve(key, "catalog", || async {
            if !album.is_empty() {
                let cover = self.do_resolve_album(
                    &Self::normalize_for_search(album), &clean_artist, album, artist,
                ).await;
                if let Some(cover) = cover.filter(|cover| {
                    confirmed_album_match(album, &cover.album_title)
                        && confirmed_artist_match(artist, &cover.artist)
                }) {
                    return Some(cover);
                }
            }
            self.do_resolve(&clean_title, &clean_artist, title, artist, album)
                .await
                .filter(|cover| {
                    confirmed_track_match(title, &cover.title)
                        && confirmed_artist_match(artist, &cover.artist)
                        && (album.is_empty() || confirmed_album_match(album, &cover.album_title))
                })
        }).await
    }

    pub async fn remember_fallback_track(
        &self,
        title: &str,
        artist: &str,
        url: &str,
    ) -> Option<OfficialArtworkResult> {
        if url.is_empty() {
            return None;
        }
        let key = Self::track_cache_key(artist, title);
        let res = OfficialArtworkResult::new(url, "", artist, title);
        self.state
            .lock()
            .await
            .insert_fallback(key.clone(), res.clone());
        if let Some(ref db) = *self.db.lock().await {
            db.save_artwork_entry(&key, url, "fallback");
        }
        Some(res)
    }

    pub async fn resolve_album_artwork(
        &self,
        album: &str,
        artist: &str,
    ) -> Option<OfficialArtworkResult> {
        let clean_album = Self::normalize_for_search(album);
        let clean_artist = Self::normalize_for_search(artist);
        if clean_album.is_empty() {
            return None;
        }
        let key = Self::album_cache_key(artist, album);
        self.cached_resolve(key, "catalog", || async {
            self.do_resolve_album(&clean_album, &clean_artist, album, artist)
                .await
        })
        .await
    }

    pub async fn resolve_artist_artwork(&self, artist: &str) -> Option<OfficialArtworkResult> {
        let primary = primary_artist_name(artist);
        let clean = Self::normalize_for_search(&primary);
        if clean.is_empty() {
            return None;
        }
        let key = format!("artist4|{clean}");
        self.cached_resolve(key, "deezer", || async {
            self.do_resolve_artist(&clean, &primary).await
        })
        .await
    }

    async fn itunes_search(&self, term: &str, entity: &str, limit: &str) -> Vec<Value> {
        {
            let st = self.state.lock().await;
            if now_ms() < st.itunes_skip_until_ms {
                return Vec::new();
            }
        }
        let Ok(res) = self
            .http
            .get("https://itunes.apple.com/search")
            .query(&[
                ("term", term),
                ("entity", entity),
                ("limit", limit),
                ("media", "music"),
                ("country", "us"),
            ])
            .send()
            .await
        else {
            return Vec::new();
        };

        let code = res.status().as_u16();
        if code == 403 || code == 429 {
            let mut st = self.state.lock().await;
            st.itunes_403_streak += 1;
            if st.itunes_403_streak >= 2 {
                st.itunes_skip_until_ms = now_ms() + 15 * 60 * 1000;
            }
            return Vec::new();
        }
        if code != 200 {
            return Vec::new();
        }
        self.state.lock().await.itunes_403_streak = 0;

        let Ok(text) = res.text().await else {
            return Vec::new();
        };
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) else {
            return Vec::new();
        };
        map.get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    async fn deezer_get(&self, path: &str, query: &[(&str, &str)]) -> Option<Value> {
        let url = format!("https://api.deezer.com{path}");
        for attempt in 0..3u64 {
            if let Ok(res) = self.http.get(&url).query(query).send().await {
                let status = res.status().as_u16();
                let raw: Option<Value> = res
                    .text()
                    .await
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok());
                let limited = status == 429
                    || raw
                        .as_ref()
                        .and_then(Value::as_object)
                        .and_then(|m| m.get("error"))
                        .is_some();
                if !limited {
                    return if status == 200 { raw } else { None };
                }
            }
            tokio::time::sleep(Duration::from_millis(900 * (attempt + 1))).await;
        }
        None
    }

    async fn deezer_track_hits(&self, query: &str) -> Vec<Value> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let Some(raw) = self.deezer_get("/search", &[("q", q), ("limit", "15")]).await else {
            return Vec::new();
        };
        let Some(items) = raw
            .as_object()
            .and_then(|m| m.get("data"))
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };

        items
            .iter()
            .filter_map(|item| {
                let obj = item.as_object()?;
                let artist = obj.get("artist").and_then(Value::as_object);
                let album = obj.get("album").and_then(Value::as_object);
                let cover = album
                    .and_then(|a| {
                        a.get("cover_xl")
                            .or_else(|| a.get("cover_big"))
                            .or_else(|| a.get("cover"))
                    })
                    .and_then(Value::as_str)
                    .unwrap_or("");
                Some(json!({
                    "trackName": obj.get("title").and_then(Value::as_str).unwrap_or(""),
                    "artistName": artist.and_then(|a| a.get("name")).and_then(Value::as_str).unwrap_or(""),
                    "collectionName": album.and_then(|a| a.get("title")).and_then(Value::as_str).unwrap_or(""),
                    "artworkUrl100": cover,
                    "releaseDate": album.and_then(|a| a.get("release_date")).and_then(Value::as_str).unwrap_or(""),
                }))
            })
            .collect()
    }

    fn tidal_cover_url(uuid: &str, dim: u32) -> Option<String> {
        let parts: Vec<&str> = uuid.trim().split('-').collect();
        if parts.len() != 5 || parts.iter().any(|p| p.is_empty()) {
            return None;
        }
        Some(format!(
            "https://resources.tidal.com/images/{}/{dim}x{dim}.jpg",
            parts.join("/")
        ))
    }

    async fn tidal_search(&self, query: &str, types: &str) -> Option<Value> {
        let res = self
            .http
            .get(TIDAL_SEARCH_URL)
            .header("X-Tidal-Token", TIDAL_EMBED_TOKEN)
            .query(&[
                ("query", query),
                ("limit", "10"),
                ("types", types),
                ("countryCode", "US"),
            ])
            .send()
            .await
            .ok()?;
        if !res.status().is_success() {
            return None;
        }
        res.json::<Value>().await.ok()
    }

    async fn tidal_track_hits(&self, query: &str) -> Vec<Value> {
        let Some(root) = self.tidal_search(query, "TRACKS").await else {
            return Vec::new();
        };
        let Some(items) = root
            .get("tracks")
            .and_then(|t| t.get("items"))
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| {
                let obj = item.as_object()?;
                let album = obj.get("album").and_then(Value::as_object);
                let uuid = album.and_then(|a| a.get("cover")).and_then(Value::as_str)?;
                let cover = Self::tidal_cover_url(uuid, 1280)?;
                let artist_name = obj
                    .get("artists")
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.first())
                    .and_then(|a| a.get("name"))
                    .and_then(Value::as_str)
                    .or_else(|| {
                        obj.get("artist")
                            .and_then(|a| a.get("name"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or("");
                Some(json!({
                    "trackName": obj.get("title").and_then(Value::as_str).unwrap_or(""),
                    "artistName": artist_name,
                    "collectionName": album.and_then(|a| a.get("title")).and_then(Value::as_str).unwrap_or(""),
                    "artworkUrl100": cover,
                    "releaseDate": obj.get("streamStartDate").and_then(Value::as_str).unwrap_or(""),
                }))
            })
            .collect()
    }

    async fn tidal_album_hits(&self, query: &str) -> Vec<Value> {
        let Some(root) = self.tidal_search(query, "ALBUMS").await else {
            return Vec::new();
        };
        let Some(items) = root
            .get("albums")
            .and_then(|t| t.get("items"))
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| {
                let obj = item.as_object()?;
                let uuid = obj.get("cover").and_then(Value::as_str)?;
                let cover = Self::tidal_cover_url(uuid, 1280)?;
                let artist_name = obj
                    .get("artists")
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.first())
                    .and_then(|a| a.get("name"))
                    .and_then(Value::as_str)
                    .or_else(|| {
                        obj.get("artist")
                            .and_then(|a| a.get("name"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or("");
                Some(json!({
                    "collectionName": obj.get("title").and_then(Value::as_str).unwrap_or(""),
                    "artistName": artist_name,
                    "artworkUrl100": cover,
                }))
            })
            .collect()
    }

    async fn spotify_access_token(&self) -> Option<String> {
        {
            let st = self.state.lock().await;
            if now_ms() < st.spotify_skip_until_ms {
                return None;
            }
            if let Some((ref tok, exp)) = st.spotify_token {
                if now_ms() + 15_000 < exp {
                    return Some(tok.clone());
                }
            }
        }
        let res = self
            .http
            .get("https://open.spotify.com/get_access_token?reason=transport&productType=web_player")
            .send()
            .await
            .ok()?;
        if !res.status().is_success() {
            self.state.lock().await.spotify_skip_until_ms = now_ms() + 10 * 60 * 1000;
            return None;
        }
        let body: Value = res.json().await.ok()?;
        let token = body
            .get("accessToken")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())?
            .to_string();
        let exp = body
            .get("accessTokenExpirationTimestampMs")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| now_ms() + 20 * 60 * 1000);
        self.state.lock().await.spotify_token = Some((token.clone(), exp));
        Some(token)
    }

    async fn spotify_search(&self, query: &str, kind: &str) -> Option<Value> {
        let token = self.spotify_access_token().await?;
        let res = self
            .http
            .get("https://api.spotify.com/v1/search")
            .bearer_auth(token)
            .query(&[("q", query), ("type", kind), ("limit", "10")])
            .send()
            .await
            .ok()?;
        if res.status().as_u16() == 401 || res.status().as_u16() == 429 {
            let mut st = self.state.lock().await;
            st.spotify_token = None;
            st.spotify_skip_until_ms = now_ms() + 5 * 60 * 1000;
            return None;
        }
        if !res.status().is_success() {
            return None;
        }
        res.json::<Value>().await.ok()
    }

    async fn spotify_track_hits(&self, query: &str) -> Vec<Value> {
        let Some(root) = self.spotify_search(query, "track").await else {
            return Vec::new();
        };
        let Some(items) = root
            .get("tracks")
            .and_then(|t| t.get("items"))
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| {
                let obj = item.as_object()?;
                let album = obj.get("album").and_then(Value::as_object)?;
                let cover = album
                    .get("images")
                    .and_then(Value::as_array)
                    .and_then(|imgs| imgs.first())
                    .and_then(|i| i.get("url"))
                    .and_then(Value::as_str)?;
                let artist_name = obj
                    .get("artists")
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.first())
                    .and_then(|a| a.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                Some(json!({
                    "trackName": obj.get("name").and_then(Value::as_str).unwrap_or(""),
                    "artistName": artist_name,
                    "collectionName": album.get("name").and_then(Value::as_str).unwrap_or(""),
                    "artworkUrl100": cover,
                    "releaseDate": album.get("release_date").and_then(Value::as_str).unwrap_or(""),
                }))
            })
            .collect()
    }

    async fn spotify_album_hits(&self, query: &str) -> Vec<Value> {
        let Some(root) = self.spotify_search(query, "album").await else {
            return Vec::new();
        };
        let Some(items) = root
            .get("albums")
            .and_then(|t| t.get("items"))
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| {
                let obj = item.as_object()?;
                let cover = obj
                    .get("images")
                    .and_then(Value::as_array)
                    .and_then(|imgs| imgs.first())
                    .and_then(|i| i.get("url"))
                    .and_then(Value::as_str)?;
                let artist_name = obj
                    .get("artists")
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.first())
                    .and_then(|a| a.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                Some(json!({
                    "collectionName": obj.get("name").and_then(Value::as_str).unwrap_or(""),
                    "artistName": artist_name,
                    "artworkUrl100": cover,
                }))
            })
            .collect()
    }

    async fn qobuz_search(&self, endpoint: &str, query: &str) -> Option<Value> {
        if now_ms() < self.state.lock().await.qobuz_skip_until_ms {
            return None;
        }
        let url = format!("https://www.qobuz.com/api.json/0.2/{endpoint}");
        let res = self
            .http
            .get(&url)
            .query(&[("query", query), ("limit", "10"), ("app_id", QOBUZ_APP_ID)])
            .send()
            .await
            .ok()?;
        let code = res.status().as_u16();
        if code == 400 || code == 401 || code == 403 || code == 429 {
            self.state.lock().await.qobuz_skip_until_ms = now_ms() + 15 * 60 * 1000;
            return None;
        }
        if !res.status().is_success() {
            return None;
        }
        res.json::<Value>().await.ok()
    }

    async fn qobuz_track_hits(&self, query: &str) -> Vec<Value> {
        let Some(root) = self.qobuz_search("track/search", query).await else {
            return Vec::new();
        };
        let Some(items) = root
            .get("tracks")
            .and_then(|t| t.get("items"))
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| {
                let obj = item.as_object()?;
                let album = obj.get("album").and_then(Value::as_object)?;
                let img = album.get("image").and_then(Value::as_object)?;
                let cover = img
                    .get("large")
                    .or_else(|| img.get("small"))
                    .and_then(Value::as_str)?;
                let artist_name = obj
                    .get("performer")
                    .and_then(|p| p.get("name"))
                    .and_then(Value::as_str)
                    .or_else(|| {
                        album
                            .get("artist")
                            .and_then(|a| a.get("name"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or("");
                Some(json!({
                    "trackName": obj.get("title").and_then(Value::as_str).unwrap_or(""),
                    "artistName": artist_name,
                    "collectionName": album.get("title").and_then(Value::as_str).unwrap_or(""),
                    "artworkUrl100": cover,
                    "releaseDate": album.get("release_date_original").and_then(Value::as_str).unwrap_or(""),
                }))
            })
            .collect()
    }

    async fn qobuz_album_hits(&self, query: &str) -> Vec<Value> {
        let Some(root) = self.qobuz_search("album/search", query).await else {
            return Vec::new();
        };
        let Some(items) = root
            .get("albums")
            .and_then(|t| t.get("items"))
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| {
                let obj = item.as_object()?;
                let img = obj.get("image").and_then(Value::as_object)?;
                let cover = img
                    .get("large")
                    .or_else(|| img.get("small"))
                    .and_then(Value::as_str)?;
                let artist_name = obj
                    .get("artist")
                    .and_then(|a| a.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                Some(json!({
                    "collectionName": obj.get("title").and_then(Value::as_str).unwrap_or(""),
                    "artistName": artist_name,
                    "artworkUrl100": cover,
                }))
            })
            .collect()
    }

    async fn deezer_photo_is_stock(&self, url: &str) -> bool {
        let no_redir = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(6))
            .build()
            .unwrap_or_else(|_| self.http.clone());
        if let Ok(res) = no_redir.head(url).send().await {
            if let Some(loc) = res.headers().get("location").and_then(|v| v.to_str().ok()) {
                return Self::is_deezer_blank(loc);
            }
        }
        false
    }

    async fn do_resolve_artist(&self, clean: &str, raw: &str) -> Option<OfficialArtworkResult> {
        let raw_primary = primary_artist_name(raw);
        let search_name = if raw_primary.is_empty() {
            raw
        } else {
            &raw_primary
        };
        let clean_search_name = Self::normalize_for_search(search_name);

        let raw_data = self
            .deezer_get("/search/artist", &[("q", search_name), ("limit", "10")])
            .await?;
        let items = raw_data
            .as_object()?
            .get("data")?
            .as_array()?;
        if items.is_empty() {
            return None;
        }

        let mut best: Option<&serde_json::Map<String, Value>> = None;
        let mut best_fans = -1i64;
        let mut most_popular: Option<&serde_json::Map<String, Value>> = None;
        let mut most_fans = -1i64;

        for item in items {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let fans = obj.get("nb_fan").and_then(Value::as_i64).unwrap_or(0);
            if fans > most_fans {
                most_fans = fans;
                most_popular = Some(obj);
            }
            let name = Self::normalize_for_search(
                obj.get("name").and_then(Value::as_str).unwrap_or(""),
            );
            if name.is_empty() {
                continue;
            }
            if (name == clean || name == clean_search_name) && fans > best_fans {
                best_fans = fans;
                best = Some(obj);
            }
        }

        let chosen = best?;
        let pic = ["picture_xl", "picture_big", "picture_medium"]
            .iter()
            .filter_map(|k| chosen.get(*k).and_then(Value::as_str))
            .find(|s| !s.is_empty())?;
        if Self::is_deezer_blank(pic) || self.deezer_photo_is_stock(pic).await {
            return None;
        }
        Some(OfficialArtworkResult::new(
            pic,
            "",
            chosen
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(raw),
            "",
        ))
    }

    fn match_album(items: &[Value], clean_album: &str, clean_artist: &str) -> Option<(i32, Value)> {
        let mut best: Option<Value> = None;
        let mut best_score = -999;
        for item in items {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let collection = Self::normalize_for_search(
                obj.get("collectionName")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            );
            let artist_name = Self::normalize_for_search(
                obj.get("artistName")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            );
            if collection.is_empty() {
                continue;
            }
            let title_score = Self::artwork_title_score(&collection, clean_album);
            if title_score < 100 {
                continue;
            }
            let mut score = title_score;
            if !clean_artist.is_empty() {
                let artist_score = Self::artwork_artist_score(&artist_name, clean_artist);
                if artist_score < 95 {
                    continue;
                }
                score += artist_score;
            }
            if artist_name.contains("karaoke")
                || artist_name.contains("tribute")
                || artist_name.contains("cover band")
                || collection.contains("instrumental")
                || collection.contains("karaoke")
            {
                score -= 80;
            }
            if score > best_score {
                best_score = score;
                best = Some(item.clone());
            }
        }
        best.filter(|_| best_score >= 60).map(|b| (best_score, b))
    }

    fn match_album_via_songs(
        items: &[Value],
        clean_album: &str,
        clean_artist: &str,
    ) -> Option<(i32, Value)> {
        let mut best: Option<Value> = None;
        let mut best_score = -999;
        for item in items {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let collection = Self::normalize_for_search(
                obj.get("collectionName")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            );
            let artist_name = Self::normalize_for_search(
                obj.get("artistName")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            );
            if collection.is_empty() {
                continue;
            }
            let title_score = Self::artwork_title_score(&collection, clean_album);
            if title_score < 100 {
                continue;
            }
            let artist_score = Self::artwork_artist_score(&artist_name, clean_artist);
            if artist_score < 95 {
                continue;
            }
            let mut score = title_score + artist_score;
            let track_name = Self::normalize_for_search(
                obj.get("trackName").and_then(Value::as_str).unwrap_or(""),
            );
            let noise = format!("{track_name} {collection}");
            if noise.contains("instrumental")
                || noise.contains("karaoke")
                || noise.contains("tribute")
            {
                score -= 80;
            }
            if score > best_score {
                best_score = score;
                best = Some(item.clone());
            }
        }
        best.filter(|_| best_score >= 150).map(|b| (best_score, b))
    }

    fn album_result_from(
        item: &Value,
        raw_album: &str,
        raw_artist: &str,
    ) -> Option<OfficialArtworkResult> {
        let obj = item.as_object()?;
        let raw_art = obj
            .get("artworkUrl100")
            .and_then(Value::as_str)
            .unwrap_or("");
        if raw_art.is_empty() {
            return None;
        }
        Some(OfficialArtworkResult::new(
            SIZE_BB_RE.replace_all(raw_art, "1400x1400bb"),
            obj.get("collectionName")
                .and_then(Value::as_str)
                .unwrap_or(raw_album),
            obj.get("artistName")
                .and_then(Value::as_str)
                .unwrap_or(raw_artist),
            "",
        ))
    }

    async fn do_resolve_album(
        &self,
        clean_album: &str,
        clean_artist: &str,
        raw_album: &str,
        raw_artist: &str,
    ) -> Option<OfficialArtworkResult> {
        let mut hits: Vec<(i32, Value)> = Vec::new();
        let mut terms = Vec::new();
        if !clean_artist.is_empty() {
            terms.push(format!("{clean_artist} {clean_album}"));
        }
        if !terms.iter().any(|t| t == clean_album) {
            terms.push(clean_album.to_string());
        }
        for term in terms {
            let items = self.itunes_search(&term, "album", "25").await;
            if let Some(hit) = Self::match_album(&items, clean_album, clean_artist) {
                hits.push(hit);
            }
        }
        if !clean_artist.is_empty() {
            let items = self
                .itunes_search(&format!("{clean_artist} {clean_album}"), "song", "25")
                .await;
            if let Some(hit) = Self::match_album_via_songs(&items, clean_album, clean_artist) {
                hits.push(hit);
            }
        }

        if !hits.is_empty() {
            hits.sort_by(|a, b| b.0.cmp(&a.0));
            if let Some(res) = Self::album_result_from(&hits[0].1, raw_album, raw_artist) {
                return Some(res);
            }
        }

        let album_q = if clean_artist.is_empty() {
            clean_album.to_string()
        } else {
            format!("{clean_artist} {clean_album}")
        };
        let (t_hits, s_hits, q_hits) = tokio::join!(
            self.tidal_album_hits(&album_q),
            self.spotify_album_hits(&album_q),
            self.qobuz_album_hits(&album_q),
        );
        let mut extra_mapped: Vec<Value> = Vec::new();
        extra_mapped.extend(t_hits);
        extra_mapped.extend(s_hits);
        extra_mapped.extend(q_hits);
        if let Some(hit) = Self::match_album(&extra_mapped, clean_album, clean_artist) {
            if let Some(res) = Self::album_result_from(&hit.1, raw_album, raw_artist) {
                return Some(res);
            }
        }

        // Deezer album catalog fallback
        let q = if clean_artist.is_empty() {
            clean_album.to_string()
        } else {
            format!("{clean_artist} {clean_album}")
        };
        if let Some(res) = self
            .http
            .get("https://api.deezer.com/search/album")
            .query(&[("q", q.as_str()), ("limit", "10")])
            .send()
            .await
            .ok()
        {
            if res.status().is_success() {
                if let Ok(text) = res.text().await {
                    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) {
                        if let Some(items) = map.get("data").and_then(Value::as_array) {
                            let mapped: Vec<Value> = items
                                .iter()
                                .filter_map(|item| {
                                    let obj = item.as_object()?;
                                    let artist = obj.get("artist").and_then(Value::as_object);
                                    let cover = obj
                                        .get("cover_xl")
                                        .or_else(|| obj.get("cover_big"))
                                        .or_else(|| obj.get("cover"))
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    Some(json!({
                                        "collectionName": obj.get("title").and_then(Value::as_str).unwrap_or(""),
                                        "artistName": artist.and_then(|a| a.get("name")).and_then(Value::as_str).unwrap_or(""),
                                        "artworkUrl100": cover,
                                    }))
                                })
                                .collect();
                            if let Some(hit) = Self::match_album(&mapped, clean_album, clean_artist)
                            {
                                if let Some(res) =
                                    Self::album_result_from(&hit.1, raw_album, raw_artist)
                                {
                                    return Some(res);
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    async fn do_resolve(
        &self,
        clean_title: &str,
        clean_artist: &str,
        raw_title: &str,
        raw_artist: &str,
        raw_album: &str,
    ) -> Option<OfficialArtworkResult> {
        let raw_primary = primary_artist_name(raw_artist);
        let clean_primary = Self::normalize_for_search(&raw_primary);

        let track_q = if clean_artist.is_empty() {
            clean_title.to_string()
        } else {
            format!("{clean_artist} {clean_title}")
        };

        let (c1, c2, c3, d1, d2, d3, t1, s1, q1) = tokio::join!(
            async {
                if !clean_artist.is_empty() {
                    self.itunes_search(&format!("{clean_artist} {clean_title}"), "song", "25")
                        .await
                } else {
                    Vec::new()
                }
            },
            async {
                if !clean_primary.is_empty() && clean_primary != clean_artist {
                    self.itunes_search(&format!("{clean_primary} {clean_title}"), "song", "25")
                        .await
                } else {
                    Vec::new()
                }
            },
            self.itunes_search(clean_title, "song", "25"),
            async {
                if !raw_artist.is_empty() {
                    self.deezer_track_hits(&format!("{raw_artist} {raw_title}"))
                        .await
                } else {
                    Vec::new()
                }
            },
            async {
                if !raw_primary.is_empty() && raw_primary != raw_artist {
                    self.deezer_track_hits(&format!("{raw_primary} {raw_title}"))
                        .await
                } else {
                    Vec::new()
                }
            },
            self.deezer_track_hits(raw_title),
            self.tidal_track_hits(&track_q),
            self.spotify_track_hits(&track_q),
            self.qobuz_track_hits(&track_q),
        );

        let mut all_items = Vec::new();
        all_items.extend(c1);
        all_items.extend(c2);
        all_items.extend(c3);
        all_items.extend(d1);
        all_items.extend(d2);
        all_items.extend(d3);
        all_items.extend(t1);
        all_items.extend(s1);
        all_items.extend(q1);

        let mut found = Self::find_best_match(&all_items, clean_title, clean_artist);
        if found.is_none() && !clean_primary.is_empty() && clean_primary != clean_artist {
            found = Self::find_best_match(&all_items, clean_title, &clean_primary);
        }

        if let Some(ref m) = found {
            if let Some(obj) = m.as_object() {
                let raw_art = obj
                    .get("artworkUrl100")
                    .or_else(|| obj.get("artworkUrl60"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !raw_art.is_empty() {
                    let high_res =
                        if raw_art.contains("mzstatic.com") || raw_art.contains("apple.com") {
                            SIZE_BB_RE.replace_all(raw_art, "1400x1400bb").to_string()
                        } else {
                            raw_art.to_string()
                        };
                    return Some(OfficialArtworkResult::new(
                        high_res,
                        obj.get("collectionName")
                            .and_then(Value::as_str)
                            .unwrap_or(raw_album),
                        obj.get("artistName")
                            .and_then(Value::as_str)
                            .unwrap_or(raw_artist),
                        obj.get("trackName")
                            .and_then(Value::as_str)
                            .unwrap_or(raw_title),
                    ));
                }
            }
        }

        if !raw_album.is_empty() && !clean_artist.is_empty() {
            if let Some(album_match) = self
                .do_resolve_album(
                    &Self::normalize_for_search(raw_album),
                    clean_artist,
                    raw_album,
                    raw_artist,
                )
                .await
            {
                return Some(OfficialArtworkResult::new(
                    album_match.artwork_url,
                    album_match.album_title,
                    album_match.artist,
                    raw_title,
                ));
            }
        }

        None
    }

    fn name_tokens(s: &str) -> HashSet<String> {
        s.split(' ')
            .filter(|t| t.len() > 1 && !ARTIST_STOPWORDS.contains(t))
            .map(str::to_string)
            .collect()
    }

    pub fn artwork_title_score(candidate: &str, target: &str) -> i32 {
        let a = Self::normalize_for_search(candidate);
        let b = Self::normalize_for_search(target);
        if a.is_empty() || b.is_empty() {
            return 0;
        }
        if a == b {
            return 100;
        }
        0
    }

    pub fn artwork_artist_score(candidate: &str, target: &str) -> i32 {
        let a = Self::normalize_for_search(candidate);
        let b = Self::normalize_for_search(target);
        if a.is_empty() || b.is_empty() {
            return 0;
        }
        if a == b || Self::artists_aliased(&a, &b) {
            return 100;
        }
        for part in split_artist_credits(candidate) {
            let norm = Self::normalize_for_search(&part);
            if !norm.is_empty() && (norm == b || Self::artists_aliased(&norm, &b)) {
                return 95;
            }
        }
        0
    }

    fn is_near_typo(a: &str, b: &str) -> bool {
        let ca: Vec<char> = a.chars().collect();
        let cb: Vec<char> = b.chars().collect();
        let la = ca.len();
        let lb = cb.len();
        if la < 6 || lb < 6 || la.abs_diff(lb) > 1 {
            return false;
        }
        if la == lb {
            let mut diffs = 0usize;
            let mut first = 0usize;
            for i in 0..la {
                if ca[i] == cb[i] {
                    continue;
                }
                diffs += 1;
                if diffs == 1 {
                    first = i;
                } else if diffs == 2 && first == i - 1 && ca[first] == cb[i] && ca[i] == cb[first] {
                    return true;
                } else {
                    return false;
                }
            }
            return diffs <= 1;
        }
        let (longer, shorter) = if la > lb { (&ca, &cb) } else { (&cb, &ca) };
        let (mut i, mut j, mut skipped) = (0usize, 0usize, 0usize);
        while i < longer.len() && j < shorter.len() {
            if longer[i] == shorter[j] {
                i += 1;
                j += 1;
            } else {
                skipped += 1;
                if skipped > 1 {
                    return false;
                }
                i += 1;
            }
        }
        true
    }

    pub fn pick_best_track_artwork(
        items: &[Value],
        title: &str,
        artist: &str,
    ) -> Option<Value> {
        Self::find_best_match(
            items,
            &Self::normalize_for_search(title),
            &Self::normalize_for_search(artist),
        )
    }

    fn find_best_match(
        items: &[Value],
        target_title: &str,
        target_artist: &str,
    ) -> Option<Value> {
        if items.is_empty() || target_title.is_empty() || target_artist.is_empty() {
            return None;
        }

        let mut accepted: Vec<(i32, Option<String>, Value)> = Vec::new();
        let mut exact: Vec<(i32, Option<String>, Value)> = Vec::new();

        for item in items {
            let Some(obj) = item.as_object() else {
                continue;
            };
            let raw_artist_name = obj
                .get("artistName")
                .and_then(Value::as_str)
                .unwrap_or("");
            let track_name = obj.get("trackName").and_then(Value::as_str).unwrap_or("");
            let collection = Self::normalize_for_search(
                obj.get("collectionName")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            );
            let collection_artist = Self::normalize_for_search(
                obj.get("collectionArtistName")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            );

            let title_score = Self::artwork_title_score(track_name, target_title);
            if title_score < 100 {
                continue;
            }
            let artist_score = Self::artwork_artist_score(raw_artist_name, target_artist);
            if artist_score < 95 {
                continue;
            }

            let mut score = title_score + artist_score;
            let noise = format!("{} {collection}", Self::normalize_for_search(track_name));
            if !target_title.contains("instrumental")
                && (noise.contains("instrumental")
                    || noise.contains("karaoke")
                    || noise.contains("tribute")
                    || noise.contains("cover version")
                    || noise.contains("made famous"))
            {
                score -= 80;
            }

            let looks_like_compilation = collection_artist == "various artists"
                || collection.contains("greatest hits")
                || collection.contains("best of")
                || collection.contains("essential")
                || collection.contains("anthology")
                || collection.contains("various");
            if looks_like_compilation {
                score -= 60;
            }
            if collection.ends_with(" single") {
                score -= 15;
            }
            if FEAT_TAG_RE.is_match(track_name) {
                score -= 5;
            }

            let released = obj
                .get("releaseDate")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            if score >= 170 {
                accepted.push((score, released, item.clone()));
            } else if title_score == 100 && artist_score == 100 {
                exact.push((score, released, item.clone()));
            }
        }

        let sort_candidates = |list: &mut Vec<(i32, Option<String>, Value)>| {
            list.sort_by(|a, b| {
                let s = b.0.cmp(&a.0);
                if s != std::cmp::Ordering::Equal {
                    return s;
                }
                match (&a.1, &b.1) {
                    (Some(ad), Some(bd)) => ad.cmp(bd),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            });
        };

        if !accepted.is_empty() {
            sort_candidates(&mut accepted);
            return Some(accepted.remove(0).2);
        }
        if !exact.is_empty() {
            sort_candidates(&mut exact);
            return Some(exact.remove(0).2);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_and_score_prevent_impostors() {
        assert_eq!(
            OfficialArtworkService::normalize_for_search("Can't Tell Me Nothing (Explicit)"),
            "cant tell me nothing"
        );
        assert_eq!(
            OfficialArtworkService::artwork_title_score("Cinderella", "Cider"),
            0
        );
        assert_eq!(
            OfficialArtworkService::artwork_artist_score("MF DOOM", "Madvillain"),
            100
        );
    }

    #[test]
    fn pick_best_prefers_studio_over_greatest_hits() {
        let items = vec![
            json!({
                "trackName": "Starboy",
                "artistName": "The Weeknd",
                "collectionName": "The Highlights (Greatest Hits)",
                "releaseDate": "2021-02-05T00:00:00Z",
                "artworkUrl100": "https://is1-ssl.mzstatic.com/hits/100x100bb.jpg"
            }),
            json!({
                "trackName": "Starboy",
                "artistName": "The Weeknd",
                "collectionName": "Starboy",
                "releaseDate": "2016-11-25T00:00:00Z",
                "artworkUrl100": "https://is1-ssl.mzstatic.com/studio/100x100bb.jpg"
            }),
        ];
        let best = OfficialArtworkService::pick_best_track_artwork(&items, "Starboy", "The Weeknd")
            .unwrap();
        assert_eq!(best["collectionName"], "Starboy");
    }
}
