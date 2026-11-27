//! Multi-source lookup + memory/SQLite/disk cache for animated album artwork.
//!
//! Ported from `lib/core/artwork/animated_artwork_service.dart`.
//! Waterfall priority order:
//! 1. Apple Music (`AppleMusicCanvas` + proxy fallback)
//! 2. Tidal (`TidalCanvas` 1280x1280 video covers)
//! 3. Community (`CommunityCanvas` ViMusic manifest)

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, Semaphore};

use crate::storage::app_database::AppDatabase;

use super::apple_music_canvas::AppleMusicCanvas;
use super::community_canvas::CommunityCanvas;
use super::official_artwork_service::OfficialArtworkService;
use super::tidal_canvas::TidalCanvas;
use super::{
    clean_canvas_query, primary_canvas_artist, strip_canvas_subtitles, AnimatedArtwork,
    CanvasSource, CANVAS_USER_AGENT,
};

pub const ANIMATED_ARTWORK_BASE_URL: &str = "https://artwork.m8tec.top";
const ANIM_HIT: &str = "anim";
const ANIM_TIDAL_HIT: &str = "anim-tidal";
const ANIM_COMMUNITY_HIT: &str = "anim-community";
const ANIM_MISS: &str = "anim-v2-miss";
const HIT_TTL_MS: i64 = 30 * 24 * 3600 * 1000;
const MISS_TTL_MS: i64 = 12 * 3600 * 1000;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimatedArtworkQuery {
    pub artist: String,
    pub album: String,
    pub title: String,
}

impl AnimatedArtworkQuery {
    pub fn new(artist: impl Into<String>, album: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            artist: artist.into(),
            album: album.into(),
            title: title.into(),
        }
    }

    pub fn cache_key(&self) -> String {
        animated_artwork_cache_key(&self.artist, &self.album, &self.title)
    }
}

pub fn animated_artwork_cache_key(artist: &str, album: &str, title: &str) -> String {
    let cleaned_album = clean_canvas_query(&strip_canvas_subtitles(album));
    let b = OfficialArtworkService::normalize_for_search(if cleaned_album.is_empty() {
        album
    } else {
        &cleaned_album
    });
    if !b.is_empty() {
        let primary = primary_canvas_artist(&clean_canvas_query(artist));
        let a_primary = OfficialArtworkService::normalize_for_search(if primary.is_empty() {
            artist
        } else {
            &primary
        });
        return format!("anim6|{a_primary}|{b}");
    }
    let a = OfficialArtworkService::normalize_for_search(artist);
    format!("anim6|{a}||{}", OfficialArtworkService::normalize_for_search(title))
}

pub fn headers_for_animated_artwork_url(url: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("User-Agent".to_string(), CANVAS_USER_AGENT.to_string());
    map.insert("Accept".to_string(), "*/*".to_string());
    if url.to_lowercase().contains("apple.com") {
        map.insert("Referer".to_string(), "https://music.apple.com/".to_string());
        map.insert("Origin".to_string(), "https://music.apple.com".to_string());
    }
    map
}

/// Persistent disk cache for `.mp4` animated cover loops.
pub struct CanvasDiskCache {
    inflight: Mutex<HashMap<String, Arc<tokio::sync::Notify>>>,
}

static CANVAS_DISK_CACHE: LazyLock<CanvasDiskCache> = LazyLock::new(|| CanvasDiskCache {
    inflight: Mutex::new(HashMap::new()),
});

impl CanvasDiskCache {
    const MAX_CACHE_BYTES: u64 = 150 * 1024 * 1024;
    const MIN_VALID_BYTES: u64 = 1024;

    pub fn instance() -> &'static Self {
        &CANVAS_DISK_CACHE
    }

    fn cache_dir() -> PathBuf {
        let dir = std::env::temp_dir().join("lastwave_canvas_cache");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    pub fn is_cacheable_mp4(url: &str) -> bool {
        let lower = url.trim().to_lowercase();
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            return false;
        }
        let path = lower
            .split('?')
            .next()
            .unwrap_or("")
            .split('#')
            .next()
            .unwrap_or("");
        path.ends_with(".mp4") || path.ends_with(".m4v") || path.ends_with(".mov")
    }

    fn hash_url(url: &str) -> String {
        let mut hash: u64 = 0xcbf29ce484222325;
        let prime: u64 = 0x100000001b3;
        for &byte in url.trim().as_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(prime);
        }
        format!("{hash:016x}")
    }

    fn file_for(url: &str) -> PathBuf {
        Self::cache_dir().join(format!("canvas_{}.mp4", Self::hash_url(url)))
    }

    pub fn peek_file_path(&self, url: &str) -> Option<String> {
        if !Self::is_cacheable_mp4(url) {
            return None;
        }
        let path = Self::file_for(url);
        let meta = std::fs::metadata(&path).ok()?;
        if meta.len() >= Self::MIN_VALID_BYTES {
            Some(path.to_string_lossy().into_owned())
        } else {
            None
        }
    }

    pub async fn warm(&self, client: &reqwest::Client, url: &str) -> Option<String> {
        if !Self::is_cacheable_mp4(url) {
            return None;
        }
        if let Some(existing) = self.peek_file_path(url) {
            return Some(existing);
        }

        loop {
            let maybe_notify = {
                let mut map = self.inflight.lock().await;
                if let Some(n) = map.get(url) {
                    Some(Arc::clone(n))
                } else {
                    map.insert(url.to_string(), Arc::new(tokio::sync::Notify::new()));
                    None
                }
            };
            if let Some(notify) = maybe_notify {
                notify.notified().await;
                return self.peek_file_path(url);
            }
            break;
        }

        let res = self.download(client, url).await;
        if let Some(notify) = self.inflight.lock().await.remove(url) {
            notify.notify_waiters();
        }
        res
    }

    async fn download(&self, client: &reqwest::Client, url: &str) -> Option<String> {
        let target = Self::file_for(url);
        if let Some(existing) = self.peek_file_path(url) {
            return Some(existing);
        }
        let tmp = PathBuf::from(format!("{}.tmp", target.to_string_lossy()));
        let mut req = client.get(url).timeout(Duration::from_secs(30));
        for (k, v) in headers_for_animated_artwork_url(url) {
            req = req.header(k, v);
        }
        let res = match req.send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return None,
        };
        let bytes = match res.bytes().await {
            Ok(b) if (b.len() as u64) >= Self::MIN_VALID_BYTES => b,
            _ => return None,
        };
        if std::fs::write(&tmp, &bytes).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
        if target.exists() {
            let _ = std::fs::remove_file(&target);
        }
        if std::fs::rename(&tmp, &target).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
        Self::evict_if_needed();
        Some(target.to_string_lossy().into_owned())
    }

    fn evict_if_needed() {
        let dir = Self::cache_dir();
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            return;
        };
        let now = SystemTime::now();
        let mut total_bytes: u64 = 0;
        let mut mp4s: Vec<(PathBuf, u64, SystemTime)> = Vec::new();

        for entry in read_dir.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
                if let Ok(age) = now.duration_since(modified) {
                    if age > Duration::from_secs(3600) {
                        let _ = std::fs::remove_file(&path);
                    }
                }
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("mp4") {
                total_bytes += meta.len();
                mp4s.push((path, meta.len(), modified));
            }
        }

        if total_bytes <= Self::MAX_CACHE_BYTES {
            return;
        }
        mp4s.sort_by_key(|e| e.2);
        for (path, size, _) in mp4s {
            if total_bytes <= Self::MAX_CACHE_BYTES {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total_bytes = total_bytes.saturating_sub(size);
            }
        }
    }
}

static RES_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"RESOLUTION=(\d+)x(\d+)").unwrap());
static MAP_URI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"#EXT-X-MAP:URI="([^"]+)""#).unwrap());
static MP4_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\.mp4(?:[?#]|$)").unwrap());

/// Picks a mid-size H.264 `STREAM-INF` rendition.
pub fn pick_animated_artwork_stream(playlist: &str, master_url: &str) -> Option<String> {
    let lines: Vec<&str> = playlist.lines().collect();
    let mut best_url = String::new();
    let mut best_score = i32::MIN;
    for i in 0..lines.len() {
        let line = lines[i];
        if !line.starts_with("#EXT-X-STREAM-INF:") {
            continue;
        }
        let (w, h) = RES_RE
            .captures(line)
            .map(|c| {
                let w = c.get(1).and_then(|m| m.as_str().parse::<i32>().ok()).unwrap_or(0);
                let h = c.get(2).and_then(|m| m.as_str().parse::<i32>().ok()).unwrap_or(0);
                (w, h)
            })
            .unwrap_or((0, 0));
        let size = w.max(h);
        if i + 1 >= lines.len() {
            continue;
        }
        let next = lines[i + 1].trim();
        if next.is_empty() || next.starts_with('#') {
            continue;
        }
        let mut score = 0i32;
        if line.contains("avc1") {
            score += 2000;
        }
        if line.contains("hvc1") || line.contains("hev1") {
            score -= 400;
        }
        score -= (size - 640).abs();
        if size >= 1080 {
            score -= 800;
        }
        if size >= 1920 {
            score -= 2000;
        }
        if score > best_score {
            best_score = score;
            best_url = next.to_string();
        }
    }
    if best_url.is_empty() {
        return None;
    }
    if master_url.is_empty() {
        return Some(best_url);
    }
    url::Url::parse(master_url)
        .ok()
        .and_then(|base| base.join(&best_url).ok())
        .map(|u| u.to_string())
        .or(Some(best_url))
}

/// Apple motion-art variants are one fMP4 addressed by HLS byteranges.
pub fn pick_animated_artwork_file(variant_playlist: &str, variant_url: &str) -> Option<String> {
    if !variant_playlist.contains("#EXT-X-MAP:") {
        return None;
    }
    let mut uri = MAP_URI_RE
        .captures(variant_playlist)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();
    if uri.is_empty() {
        for line in variant_playlist.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                uri = trimmed.to_string();
                break;
            }
        }
    }
    if uri.is_empty() {
        return None;
    }
    url::Url::parse(variant_url)
        .ok()
        .and_then(|base| base.join(&uri).ok())
        .map(|u| u.to_string())
        .or(Some(uri))
}

pub fn parse_animated_artwork_payload(json: &Value) -> Option<AnimatedArtwork> {
    let map = json.as_object()?;
    let url = map.get("url").and_then(Value::as_str).unwrap_or("");
    if url.is_empty() || !url.contains(".m3u8") {
        return None;
    }
    Some(AnimatedArtwork {
        url: url.to_string(),
        url_tall: map
            .get("url_tall")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        fallback_url: None,
        title: None,
        artist: None,
        album: None,
        source: CanvasSource::Apple,
    })
}

pub type AlbumLookupFn = Arc<
    dyn Fn(String, String) -> Pin<Box<dyn Future<Output = String> + Send>> + Send + Sync,
>;

#[derive(Default)]
struct MemoryCache {
    map: HashMap<String, Option<AnimatedArtwork>>,
    order: Vec<String>,
    track_album_keys: HashMap<String, String>,
}

impl MemoryCache {
    fn store(&mut self, key: String, val: Option<AnimatedArtwork>) {
        self.order.retain(|k| k != &key);
        self.map.insert(key.clone(), val);
        self.order.push(key);
        if self.order.len() > 256 {
            let oldest = self.order.remove(0);
            self.map.remove(&oldest);
        }
    }
}

pub struct AnimatedArtworkService {
    http: reqwest::Client,
    db: Option<Arc<AppDatabase>>,
    official: Option<Arc<OfficialArtworkService>>,
    album_lookup: Option<AlbumLookupFn>,
    memory: Mutex<MemoryCache>,
    semaphore: Semaphore,
    inflight: Mutex<HashMap<String, Arc<tokio::sync::Notify>>>,
}

impl AnimatedArtworkService {
    pub fn new(
        http: reqwest::Client,
        db: Option<Arc<AppDatabase>>,
        official: Option<Arc<OfficialArtworkService>>,
        album_lookup: Option<AlbumLookupFn>,
    ) -> Self {
        Self {
            http,
            db,
            official,
            album_lookup,
            memory: Mutex::new(MemoryCache::default()),
            semaphore: Semaphore::new(2),
            inflight: Mutex::new(HashMap::new()),
        }
    }

    fn track_key_id(artist: &str, title: &str) -> String {
        let p = OfficialArtworkService::normalize_for_search(&primary_canvas_artist(
            &clean_canvas_query(artist),
        ));
        let t = OfficialArtworkService::normalize_for_search(&strip_canvas_subtitles(
            &clean_canvas_query(title),
        ));
        format!("{p}|{t}")
    }

    async fn fallback_album_key(&self, query: &AnimatedArtworkQuery) -> Option<String> {
        if !query.album.trim().is_empty() {
            return None;
        }
        let id = Self::track_key_id(&query.artist, &query.title);
        if let Some(mapped) = self.memory.lock().await.track_album_keys.get(&id).cloned() {
            if !mapped.is_empty() {
                return Some(mapped);
            }
        }
        if let Some(ref official) = self.official {
            if let Some(peeked) = official
                .peek_track(
                    &clean_canvas_query(&query.title),
                    &clean_canvas_query(&query.artist),
                )
                .await
            {
                let alb = peeked.album_title.trim();
                if !alb.is_empty() {
                    return Some(animated_artwork_cache_key(
                        &query.artist,
                        alb,
                        &query.title,
                    ));
                }
            }
        }
        None
    }

    pub async fn peek(&self, query: &AnimatedArtworkQuery) -> Option<AnimatedArtwork> {
        let key = query.cache_key();
        if let Some(Some(hit)) = self.memory.lock().await.map.get(&key).cloned() {
            return Some(hit);
        }
        if let Some(alt_key) = self.fallback_album_key(query).await {
            let mut mem = self.memory.lock().await;
            if let Some(Some(via_album)) = mem.map.get(&alt_key).cloned() {
                mem.store(key, Some(via_album.clone()));
                return Some(via_album);
            }
        }
        self.memory.lock().await.map.get(&key).cloned().flatten()
    }

    pub async fn lookup(&self, query: &AnimatedArtworkQuery) -> Option<AnimatedArtwork> {
        if OfficialArtworkService::normalize_for_search(&query.artist).is_empty()
            && OfficialArtworkService::normalize_for_search(&query.title).is_empty()
        {
            return None;
        }
        let key = query.cache_key();

        {
            let mem = self.memory.lock().await;
            if let Some(cached) = mem.map.get(&key) {
                if let Some(hit) = cached {
                    return Some(hit.clone());
                }
                drop(mem);
                if !query.title.trim().is_empty() {
                    let alb = clean_canvas_query(&query.album);
                    if let Some(community_hit) = CommunityCanvas::peek_match(
                        &clean_canvas_query(&query.title),
                        &clean_canvas_query(&query.artist),
                        if alb.is_empty() { None } else { Some(&alb) },
                    )
                    .await
                    {
                        return Some(community_hit);
                    }
                }
                return None;
            }
        }

        if let Some(alt_key) = self.fallback_album_key(query).await {
            if let Some(Some(hit)) = self.memory.lock().await.map.get(&alt_key).cloned() {
                self.memory.lock().await.store(key, Some(hit.clone()));
                return Some(hit);
            }
            if let Some((Some(hit), true)) = self.read_disk(&alt_key) {
                let mut mem = self.memory.lock().await;
                mem.store(alt_key, Some(hit.clone()));
                mem.store(key, Some(hit.clone()));
                return Some(hit);
            }
        }

        if let Some((maybe_hit, fresh)) = self.read_disk(&key) {
            self.memory.lock().await.store(key.clone(), maybe_hit.clone());
            if let Some(ref hit) = maybe_hit {
                let http = self.http.clone();
                let url = hit.url.clone();
                tokio::spawn(async move {
                    let _ = CanvasDiskCache::instance().warm(&http, &url).await;
                });
            }
            if fresh {
                return maybe_hit;
            }
        }

        // Dedup concurrent lookups for the same key
        loop {
            let maybe_notify = {
                let mut map = self.inflight.lock().await;
                if let Some(n) = map.get(&key) {
                    Some(Arc::clone(n))
                } else {
                    map.insert(key.clone(), Arc::new(tokio::sync::Notify::new()));
                    None
                }
            };
            if let Some(notify) = maybe_notify {
                notify.notified().await;
                return self.memory.lock().await.map.get(&key).cloned().flatten();
            }
            break;
        }

        let _permit = self.semaphore.acquire().await.ok();
        let res = self.fetch_search(query, &key).await;

        if let Some(notify) = self.inflight.lock().await.remove(&key) {
            notify.notify_waiters();
        }
        res
    }

    fn read_disk(&self, key: &str) -> Option<(Option<AnimatedArtwork>, bool)> {
        let row = self.db.as_ref()?.load_artwork_record(key)?;
        let age = now_ms() - row.timestamp_millis;
        if row.provider == ANIM_MISS {
            return Some((None, age < MISS_TTL_MS));
        }
        let source = match row.provider.as_str() {
            ANIM_HIT => CanvasSource::Apple,
            ANIM_TIDAL_HIT => CanvasSource::Tidal,
            ANIM_COMMUNITY_HIT => CanvasSource::Community,
            _ => return None,
        };
        if row.url.contains(".m3u8") || row.url.contains(".mp4") || row.url.starts_with("http") {
            return Some((
                Some(AnimatedArtwork {
                    url: row.url,
                    url_tall: String::new(),
                    fallback_url: None,
                    title: None,
                    artist: None,
                    album: None,
                    source,
                }),
                age < HIT_TTL_MS,
            ));
        }
        None
    }

    async fn fetch_search(
        &self,
        query: &AnimatedArtworkQuery,
        key: &str,
    ) -> Option<AnimatedArtwork> {
        let artist = clean_canvas_query(query.artist.trim());
        let title = clean_canvas_query(query.title.trim());
        let raw_album = query.album.trim();
        let mut album = if raw_album.is_empty() {
            String::new()
        } else {
            clean_canvas_query(raw_album)
        };

        if album.is_empty() && (!artist.is_empty() || !title.is_empty()) {
            album = self.album_from_official(&artist, &title).await;
        }
        if album.is_empty() {
            if let Some(ref lookup) = self.album_lookup {
                if let Ok(via_ytm) =
                    tokio::time::timeout(Duration::from_secs(8), lookup(title.clone(), artist.clone()))
                        .await
                {
                    if !via_ytm.trim().is_empty() {
                        album = clean_canvas_query(via_ytm.trim());
                    }
                }
            }
        }

        if raw_album.is_empty() && !album.is_empty() {
            let album_key = animated_artwork_cache_key(&artist, &album, &title);
            self.memory
                .lock()
                .await
                .track_album_keys
                .insert(Self::track_key_id(&artist, &title), album_key.clone());
            if let Some(Some(cached)) = self.memory.lock().await.map.get(&album_key).cloned() {
                self.store(key, Some(&cached)).await;
                return Some(cached);
            }
            if let Some((Some(cached), true)) = self.read_disk(&album_key) {
                self.memory
                    .lock()
                    .await
                    .store(album_key, Some(cached.clone()));
                self.store(key, Some(&cached)).await;
                return Some(cached);
            }
        }

        // Waterfall: 1. Apple Music > 2. Tidal > 3. Community
        let album_opt = if album.is_empty() {
            None
        } else {
            Some(album.as_str())
        };

        let mut found: Option<AnimatedArtwork> = None;
        for candidate in [
            self.lookup_apple_music(&artist, &album, &title).await,
            self.lookup_tidal(&artist, &album, &title).await,
            self.lookup_community(&artist, &album, &title).await,
        ] {
            let Some(art) = candidate else {
                continue;
            };
            if !art.has_url() {
                continue;
            }
            let matches_with_album = art.matches(&title, &artist, album_opt);
            let matches_song =
                !title.is_empty() && art.title.is_some() && art.matches(&title, &artist, None);
            if matches_with_album || matches_song {
                found = Some(art);
                break;
            }
        }

        let Some(found) = found else {
            self.store(key, None).await;
            return None;
        };

        let playable = self.resolve_playable_if_needed(&found.url).await;
        let fallback_playable = match &found.fallback_url {
            Some(f) if !f.is_empty() => Some(self.resolve_playable_if_needed(f).await),
            _ => None,
        };
        let effective_album = if !album.is_empty() {
            album.clone()
        } else {
            found.album.as_deref().unwrap_or("").trim().to_string()
        };
        let resolved = AnimatedArtwork {
            url: playable,
            url_tall: found.url_tall,
            fallback_url: fallback_playable,
            title: found.title,
            artist: found.artist,
            album: if effective_album.is_empty() {
                found.album
            } else {
                Some(effective_album.clone())
            },
            source: found.source,
        };
        self.store(key, Some(&resolved)).await;
        if !effective_album.is_empty() {
            let album_key = animated_artwork_cache_key(&artist, &effective_album, &title);
            self.memory
                .lock()
                .await
                .track_album_keys
                .insert(Self::track_key_id(&artist, &title), album_key.clone());
            if album_key != key {
                self.store(&album_key, Some(&resolved)).await;
            }
        }
        let http = self.http.clone();
        let url = resolved.url.clone();
        tokio::spawn(async move {
            let _ = CanvasDiskCache::instance().warm(&http, &url).await;
        });
        Some(resolved)
    }

    async fn lookup_apple_music(
        &self,
        artist: &str,
        album: &str,
        title: &str,
    ) -> Option<AnimatedArtwork> {
        let mut found = None;
        if !title.is_empty() && !artist.is_empty() {
            found = AppleMusicCanvas::search(
                &self.http,
                title,
                artist,
                if album.is_empty() { None } else { Some(album) },
            )
            .await;
            if found.is_none() && !album.is_empty() {
                found = AppleMusicCanvas::search(&self.http, title, artist, None).await;
            }
            if found.is_none() && !album.is_empty() {
                found = AppleMusicCanvas::search_album(&self.http, album, artist).await;
            }
        } else if !album.is_empty() && !artist.is_empty() {
            found = AppleMusicCanvas::search_album(&self.http, album, artist).await;
        }
        if found.is_some() {
            return found;
        }

        let primary = primary_canvas_artist(artist);
        if !album.is_empty() && !title.is_empty() {
            found = self.request_search(artist, album, title).await;
            if found.is_none() && primary != artist {
                found = self.request_search(&primary, album, title).await;
            }
        }
        if found.is_none() && !album.is_empty() {
            found = self.request_search(artist, album, "").await;
            if found.is_none() && primary != artist {
                found = self.request_search(&primary, album, "").await;
            }
        }
        if found.is_none() {
            let q_artist = if primary.is_empty() { artist } else { &primary };
            let q_album = if album.is_empty() { title } else { album };
            found = self.lookup_via_itunes(q_artist, q_album).await;
        }
        found
    }

    async fn lookup_tidal(
        &self,
        artist: &str,
        album: &str,
        title: &str,
    ) -> Option<AnimatedArtwork> {
        if artist.is_empty() {
            return None;
        }
        if !title.is_empty() {
            let mut found = TidalCanvas::search(
                &self.http,
                title,
                artist,
                if album.is_empty() { None } else { Some(album) },
            )
            .await;
            if found.is_none() && !album.is_empty() {
                found = TidalCanvas::search(&self.http, title, artist, None).await;
            }
            if found.is_none() && !album.is_empty() {
                found = TidalCanvas::search_album(&self.http, album, artist).await;
            }
            found
        } else if !album.is_empty() {
            TidalCanvas::search_album(&self.http, album, artist).await
        } else {
            None
        }
    }

    async fn lookup_community(
        &self,
        artist: &str,
        album: &str,
        title: &str,
    ) -> Option<AnimatedArtwork> {
        let search_title = if title.is_empty() { album } else { title };
        if search_title.is_empty() || artist.is_empty() {
            return None;
        }
        let mut found = CommunityCanvas::search(
            &self.http,
            search_title,
            artist,
            if album.is_empty() { None } else { Some(album) },
        )
        .await;
        if found.is_none() && !album.is_empty() && !title.is_empty() {
            found = CommunityCanvas::search(&self.http, title, artist, None).await;
        }
        found
    }

    async fn album_from_official(&self, artist: &str, title: &str) -> String {
        let Some(ref official) = self.official else {
            return String::new();
        };
        if let Some(peeked) = official.peek_track(title, artist).await {
            if !peeked.album_title.trim().is_empty() {
                return peeked.album_title.trim().to_string();
            }
        }
        official
            .resolve_official_artwork(title, artist, "")
            .await
            .map(|r| r.album_title.trim().to_string())
            .unwrap_or_default()
    }

    async fn request_search(&self, artist: &str, album: &str, title: &str) -> Option<AnimatedArtwork> {
        let mut params = vec![("artist", artist), ("album", album)];
        if !title.is_empty() {
            params.push(("title", title));
        }
        self.request("/api/v1/artwork/search", &params).await
    }

    async fn lookup_via_itunes(&self, artist: &str, album: &str) -> Option<AnimatedArtwork> {
        if artist.is_empty() || album.is_empty() {
            return None;
        }
        let term = format!("{artist} {album}");
        let res = self
            .http
            .get("https://itunes.apple.com/search")
            .query(&[("term", term.as_str()), ("entity", "album"), ("limit", "5")])
            .send()
            .await
            .ok()?;
        let val: Value = res.json().await.ok()?;
        let results = val.get("results").and_then(Value::as_array)?;
        for item in results {
            let raw = item
                .get("collectionViewUrl")
                .and_then(Value::as_str)
                .unwrap_or("");
            if raw.is_empty() {
                continue;
            }
            let clean_url = raw.split('?').next().unwrap_or("");
            if let Some(art) = self
                .request("/api/v1/artwork/url", &[("url", clean_url)])
                .await
            {
                return Some(art);
            }
        }
        None
    }

    async fn request(&self, path: &str, query: &[(&str, &str)]) -> Option<AnimatedArtwork> {
        let url = format!("{ANIMATED_ARTWORK_BASE_URL}{path}");
        let res = self.http.get(&url).query(query).send().await.ok()?;
        if res.status().as_u16() != 200 {
            return None;
        }
        let val: Value = res.json().await.ok()?;
        parse_animated_artwork_payload(&val)
    }

    async fn resolve_playable_if_needed(&self, url: &str) -> String {
        if url.contains(".m3u8") {
            self.resolve_playable_url(url).await
        } else {
            url.to_string()
        }
    }

    async fn resolve_playable_url(&self, master_url: &str) -> String {
        let Ok(master) = self.get_plain(master_url).await else {
            return master_url.to_string();
        };
        if let Some(direct) = pick_animated_artwork_file(&master, master_url) {
            if MP4_URL_RE.is_match(&direct) {
                return direct;
            }
        }
        let stream = pick_animated_artwork_stream(&master, master_url);
        let variant_url = stream.as_deref().unwrap_or(master_url);
        if let Some(ref s) = stream {
            if s != master_url {
                if let Ok(variant) = self.get_plain(s).await {
                    if let Some(file) = pick_animated_artwork_file(&variant, s) {
                        if MP4_URL_RE.is_match(&file) {
                            return file;
                        }
                    }
                }
            }
        }
        if variant_url.contains(".m3u8") || variant_url.contains(".mp4") {
            variant_url.to_string()
        } else {
            master_url.to_string()
        }
    }

    async fn get_plain(&self, url: &str) -> Result<String, ()> {
        let mut req = self.http.get(url);
        for (k, v) in headers_for_animated_artwork_url(url) {
            req = req.header(k, v);
        }
        let res = req.send().await.map_err(|_| ())?;
        if !res.status().is_success() {
            return Err(());
        }
        res.text().await.map_err(|_| ())
    }

    async fn store(&self, key: &str, art: Option<&AnimatedArtwork>) {
        self.memory
            .lock()
            .await
            .store(key.to_string(), art.cloned());
        let provider = match art.map(|a| a.source) {
            None => ANIM_MISS,
            Some(CanvasSource::Apple) => ANIM_HIT,
            Some(CanvasSource::Tidal) => ANIM_TIDAL_HIT,
            Some(CanvasSource::Community) => ANIM_COMMUNITY_HIT,
        };
        if let Some(ref db) = self.db {
            db.save_artwork_entry(key, art.map(|a| a.url.as_str()).unwrap_or(""), provider);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_h264_stream_and_byterange_file() {
        let master = "\
#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=5000000,CODECS=\"hvc1.1.6.L120.90\",RESOLUTION=1920x1920
4k_hevc.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1200000,CODECS=\"avc1.64001f\",RESOLUTION=720x720
mid_h264.m3u8
";
        assert_eq!(
            pick_animated_artwork_stream(master, "https://cdn.apple.com/master.m3u8"),
            Some("https://cdn.apple.com/mid_h264.m3u8".to_string())
        );

        let variant = "\
#EXTM3U
#EXT-X-MAP:URI=\"clip.mp4\"
#EXTINF:4.0,
#EXT-X-BYTERANGE:1000@0
clip.mp4
";
        assert_eq!(
            pick_animated_artwork_file(variant, "https://cdn.apple.com/sub/mid.m3u8"),
            Some("https://cdn.apple.com/sub/clip.mp4".to_string())
        );
    }

    #[test]
    fn cache_key_uses_album_when_present() {
        assert_eq!(
            animated_artwork_cache_key("The Weeknd", "After Hours (Deluxe)", "Blinding Lights"),
            "anim6|the weeknd|after hours"
        );
        assert_eq!(
            animated_artwork_cache_key("The Weeknd", "", "Blinding Lights"),
            "anim6|the weeknd||blinding lights"
        );
    }
}
