use std::collections::{HashMap, HashSet};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use chrono::Utc;
use futures_util::StreamExt;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::models::lyrics_models::LyricsResult;
use crate::models::stream_models::{AudioQualityTiers, ResolvedStream};

use super::app_database::AppDatabase;
use super::diagnostics_log::DiagnosticsLog;
use super::folder_layout::{download_subpath, DownloadFolderLayout};
use super::local_audio_tags::LocalAudioTags;
use super::media_tagger::{DownloadTags, MediaTagger};
use super::prefs::Prefs;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

fn invalid_filename_chars_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"[\\/:*?"<>|]"#).expect("valid filename regex"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStatus {
    #[default]
    Queued,
    Downloading,
    Done,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCancelled;

impl std::fmt::Display for DownloadCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Download cancelled")
    }
}

impl std::error::Error for DownloadCancelled {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadEntry {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub status: DownloadStatus,
    pub progress: f64,
    pub badge: String,
    /// Tagging outcome: `full`, `text`, `raw:<reason>`, or `""`.
    pub tag_note: String,
    pub file_path: Option<String>,
    pub error: Option<String>,
    pub artwork_url: String,
}

impl Default for DownloadEntry {
    fn default() -> Self {
        Self {
            key: String::new(),
            title: String::new(),
            artist: String::new(),
            status: DownloadStatus::Queued,
            progress: 0.0,
            badge: String::new(),
            tag_note: String::new(),
            file_path: None,
            error: None,
            artwork_url: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DownloadTrackRequest {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork_url: String,
    pub year: String,
    pub video_id: String,
}

/// Async stream/metadata/lyrics resolver injected by higher layers (`extractor/`).
pub trait DownloadStreamResolver: Send + Sync {
    fn is_lossless_configured(&self) -> bool;

    fn resolve_lossless_stream<'a>(
        &'a self,
        title: &'a str,
        artist: &'a str,
        album: &'a str,
        preferred_quality: i32,
    ) -> BoxFuture<'a, Option<ResolvedStream>>;

    fn match_best_video_id<'a>(
        &'a self,
        title: &'a str,
        artist: &'a str,
    ) -> BoxFuture<'a, Option<String>>;

    fn match_best_effort_video_id<'a>(
        &'a self,
        title: &'a str,
        artist: &'a str,
    ) -> BoxFuture<'a, Option<String>>;

    fn resolve_youtube_download_stream<'a>(
        &'a self,
        video_id: &'a str,
    ) -> BoxFuture<'a, Option<ResolvedStream>>;

    fn resolve_lyrics<'a>(
        &'a self,
        title: &'a str,
        artist: &'a str,
        album: &'a str,
    ) -> BoxFuture<'a, LyricsResult>;

    fn resolve_artwork_url<'a>(
        &'a self,
        title: &'a str,
        artist: &'a str,
    ) -> BoxFuture<'a, Option<String>>;
}

/// No-op resolver useful in unit tests and offline checks.
#[derive(Debug, Default)]
pub struct NoopDownloadStreamResolver;

impl DownloadStreamResolver for NoopDownloadStreamResolver {
    fn is_lossless_configured(&self) -> bool {
        false
    }

    fn resolve_lossless_stream<'a>(
        &'a self,
        _title: &'a str,
        _artist: &'a str,
        _album: &'a str,
        _preferred_quality: i32,
    ) -> BoxFuture<'a, Option<ResolvedStream>> {
        Box::pin(async { None })
    }

    fn match_best_video_id<'a>(
        &'a self,
        _title: &'a str,
        _artist: &'a str,
    ) -> BoxFuture<'a, Option<String>> {
        Box::pin(async { None })
    }

    fn match_best_effort_video_id<'a>(
        &'a self,
        _title: &'a str,
        _artist: &'a str,
    ) -> BoxFuture<'a, Option<String>> {
        Box::pin(async { None })
    }

    fn resolve_youtube_download_stream<'a>(
        &'a self,
        _video_id: &'a str,
    ) -> BoxFuture<'a, Option<ResolvedStream>> {
        Box::pin(async { None })
    }

    fn resolve_lyrics<'a>(
        &'a self,
        _title: &'a str,
        _artist: &'a str,
        _album: &'a str,
    ) -> BoxFuture<'a, LyricsResult> {
        Box::pin(async { LyricsResult::empty() })
    }

    fn resolve_artwork_url<'a>(
        &'a self,
        _title: &'a str,
        _artist: &'a str,
    ) -> BoxFuture<'a, Option<String>> {
        Box::pin(async { None })
    }
}

/// Offline download manager.
pub struct DownloadManager {
    db: Arc<AppDatabase>,
    prefs: Arc<Prefs>,
    client: reqwest::Client,
    app_support_dir: PathBuf,
    state: RwLock<Vec<DownloadEntry>>,
    active: Mutex<HashSet<String>>,
    cancel_generation: Mutex<HashMap<String, u64>>,
    cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
    muted: Mutex<HashSet<String>>,
}

impl DownloadManager {
    pub fn new(
        db: Arc<AppDatabase>,
        prefs: Arc<Prefs>,
        app_support_dir: impl Into<PathBuf>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let mgr = Self {
            db,
            prefs,
            client,
            app_support_dir: app_support_dir.into(),
            state: RwLock::new(Vec::new()),
            active: Mutex::new(HashSet::new()),
            cancel_generation: Mutex::new(HashMap::new()),
            cancel_flags: Mutex::new(HashMap::new()),
            muted: Mutex::new(HashSet::new()),
        };
        mgr.load_existing();
        mgr
    }

    pub fn key_of(title: &str, artist: &str) -> String {
        format!(
            "{}|{}",
            artist.to_lowercase().trim(),
            title.to_lowercase().trim()
        )
    }

    pub fn all(&self) -> Vec<DownloadEntry> {
        self.state.read().map(|g| g.clone()).unwrap_or_default()
    }

    fn load_existing(&self) {
        let entries: Vec<DownloadEntry> = self.db.with_conn(|conn| {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT track_key, title, artist, file_path, format_badge, tag_status, artwork_url \
                 FROM downloaded_tracks ORDER BY downloaded_at_millis DESC;",
            ) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok(DownloadEntry {
                        key: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                        title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        artist: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        status: DownloadStatus::Done,
                        progress: 1.0,
                        badge: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        tag_note: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        file_path: r.get::<_, Option<String>>(3)?,
                        error: None,
                        artwork_url: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                    })
                }) {
                    return rows.filter_map(Result::ok).collect();
                }
            }

            if let Ok(mut stmt) = conn.prepare(
                "SELECT track_key, title, artist, file_path, format_badge \
                 FROM downloaded_tracks ORDER BY downloaded_at_millis DESC;",
            ) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok(DownloadEntry {
                        key: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                        title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        artist: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        status: DownloadStatus::Done,
                        progress: 1.0,
                        badge: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        tag_note: String::new(),
                        file_path: r.get::<_, Option<String>>(3)?,
                        error: None,
                        artwork_url: String::new(),
                    })
                }) {
                    return rows.filter_map(Result::ok).collect();
                }
            }
            Vec::new()
        });

        if let Ok(mut m) = self.muted.lock() {
            for e in &entries {
                m.insert(e.key.clone());
            }
        }
        if let Ok(mut s) = self.state.write() {
            *s = entries;
        }
    }

    fn drop_in_progress(&self, key: &str) {
        if let Ok(mut s) = self.state.write() {
            s.retain(|e| e.key != key || e.status == DownloadStatus::Done);
        }
    }

    pub fn cancel_download(&self, key: &str) {
        if let Ok(mut gens) = self.cancel_generation.lock() {
            let next = gens.get(key).copied().unwrap_or(0) + 1;
            gens.insert(key.to_string(), next);
        }
        if let Ok(mut flags) = self.cancel_flags.lock() {
            if let Some(flag) = flags.remove(key) {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }

    pub fn cancel_active(&self) {
        let keys: Vec<String> = self
            .active
            .lock()
            .map(|a| a.iter().cloned().collect())
            .unwrap_or_default();
        for key in keys {
            self.cancel_download(&key);
        }
    }

    pub fn should_announce(&self, key: &str) -> bool {
        self.muted
            .lock()
            .map(|m| !m.contains(key))
            .unwrap_or(true)
    }

    pub fn mute_announcement(&self, key: &str) {
        if let Ok(mut m) = self.muted.lock() {
            m.insert(key.to_string());
        }
    }

    pub fn is_downloaded(&self, title: &str, artist: &str) -> bool {
        let key = Self::key_of(title, artist);
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT id FROM downloaded_tracks WHERE track_key = ?;",
                [&key],
                |r| r.get::<_, i64>(0),
            )
            .is_ok()
        })
    }

    pub fn local_path_for(&self, title: &str, artist: &str) -> Option<String> {
        let key = Self::key_of(title, artist);
        let path: Option<String> = self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT file_path FROM downloaded_tracks WHERE track_key = ?;",
                [&key],
                |r| r.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten()
        });
        let p = path?;
        if p.is_empty() || !Path::new(&p).exists() {
            return None;
        }
        Some(p)
    }

    fn music_dir(&self) -> std::io::Result<PathBuf> {
        let custom = self.prefs.download_dir();
        let custom = custom.trim();
        if !custom.is_empty() {
            let dir = PathBuf::from(custom);
            if fs::create_dir_all(&dir).is_ok() {
                return Ok(dir);
            }
        }
        if let Some(music) = dirs::audio_dir() {
            let dir = music.join("LastWave");
            if fs::create_dir_all(&dir).is_ok() {
                return Ok(dir);
            }
        }
        if let Some(downloads) = dirs::download_dir() {
            if let Some(parent) = downloads.parent() {
                let candidate = parent.join("Music").join("LastWave");
                if fs::create_dir_all(&candidate).is_ok() {
                    return Ok(candidate);
                }
            }
            let candidate = downloads.join("LastWave");
            if fs::create_dir_all(&candidate).is_ok() {
                return Ok(candidate);
            }
        }
        let base = dirs::document_dir().unwrap_or_else(|| self.app_support_dir.clone());
        let dir = base.join("LastWave");
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn sanitize(s: &str) -> String {
        invalid_filename_chars_regex()
            .replace_all(s, "_")
            .trim()
            .to_string()
    }

    fn layout_file(
        &self,
        root: &Path,
        artist: &str,
        album: &str,
        year: &str,
        filename: &str,
    ) -> std::io::Result<PathBuf> {
        let structure = DownloadFolderLayout::from_id(Some(&self.prefs.download_structure()));
        let sub = download_subpath(structure, artist, album, None, year);
        let dir = if sub.is_empty() {
            root.to_path_buf()
        } else {
            let mut d = root.to_path_buf();
            for seg in sub.split('/') {
                d.push(seg);
            }
            d
        };
        fs::create_dir_all(&dir)?;
        Ok(dir.join(filename))
    }

    fn move_file(from: &Path, to: &Path) -> bool {
        if fs::rename(from, to).is_ok() {
            return true;
        }
        if fs::copy(from, to).is_err() {
            return false;
        }
        if fs::remove_file(from).is_ok() {
            true
        } else {
            let _ = fs::remove_file(to);
            false
        }
    }

    /// Moves tracked downloads into the current folder layout. Returns `(moved, skipped)`.
    pub fn reorganize(&self) -> (usize, usize) {
        let Ok(root) = self.music_dir() else {
            return (0, 0);
        };
        let rows: Vec<(String, String, String, String, String)> = self.db.with_conn(|conn| {
            let Ok(mut stmt) = conn.prepare(
                "SELECT track_key, artist, album, file_path, lrc_file_path FROM downloaded_tracks;",
            ) else {
                return Vec::new();
            };
            let Ok(mapped) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                ))
            }) else {
                return Vec::new();
            };
            mapped.filter_map(Result::ok).collect()
        });

        let mut moved = 0usize;
        let mut skipped = 0usize;
        for (key, artist, album, current_path, old_lrc) in rows {
            let current = PathBuf::from(&current_path);
            if current_path.is_empty() || !current.exists() {
                skipped += 1;
                continue;
            }
            let mut year = LocalAudioTags::read(&current, "").year;
            if year.is_empty() {
                year = LocalAudioTags::year_in(&album);
            }
            let Some(filename) = current.file_name().and_then(|n| n.to_str()) else {
                skipped += 1;
                continue;
            };
            let Ok(dest) = self.layout_file(&root, &artist, &album, &year, filename) else {
                skipped += 1;
                continue;
            };
            if dest == current || dest.exists() {
                skipped += 1;
                continue;
            }
            if !Self::move_file(&current, &dest) {
                skipped += 1;
                continue;
            }
            let dest_str = dest.to_string_lossy().into_owned();
            let mut new_lrc = old_lrc.clone();
            if !new_lrc.is_empty() && Path::new(&new_lrc).exists() {
                let lrc_dest = PathBuf::from(format!("{dest_str}.lrc"));
                if !lrc_dest.exists() && Self::move_file(Path::new(&new_lrc), &lrc_dest) {
                    new_lrc = lrc_dest.to_string_lossy().into_owned();
                }
            }
            self.db.with_conn(|conn| {
                let _ = conn.execute(
                    "UPDATE downloaded_tracks SET file_path = ?, lrc_file_path = ? WHERE track_key = ?;",
                    params![dest_str, new_lrc, key],
                );
            });
            if let Ok(mut s) = self.state.write() {
                for entry in s.iter_mut() {
                    if entry.key == key {
                        entry.file_path = Some(dest_str.clone());
                    }
                }
            }
            moved += 1;
        }
        (moved, skipped)
    }

    fn upsert(&self, entry: DownloadEntry) {
        if let Ok(mut s) = self.state.write() {
            s.retain(|e| e.key != entry.key);
            s.insert(0, entry);
        }
    }

    fn is_cancelled(&self, key: &str, generation: u64, flag: &AtomicBool) -> bool {
        if flag.load(Ordering::SeqCst) {
            return true;
        }
        self.cancel_generation
            .lock()
            .map(|g| g.get(key).copied().unwrap_or(0) != generation)
            .unwrap_or(false)
    }

    async fn fetch_audio_bytes(
        &self,
        stream: &ResolvedStream,
        key: &str,
        title: &str,
        artist: &str,
        badge: &str,
        generation: u64,
        flag: &AtomicBool,
    ) -> Result<Vec<u8>, String> {
        if self.is_cancelled(key, generation, flag) {
            return Err("Download cancelled".to_string());
        }
        if !stream.url.starts_with("http") {
            let bytes = tokio::fs::read(&stream.url)
                .await
                .map_err(|e| e.to_string())?;
            if self.is_cancelled(key, generation, flag) {
                return Err("Download cancelled".to_string());
            }
            return Ok(bytes);
        }

        let mut headers = HeaderMap::new();
        for (k, v) in &stream.request_headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(v),
            ) {
                headers.insert(name, val);
            }
        }
        let resp = self
            .client
            .get(&stream.url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let total = resp.content_length().unwrap_or(0);
        let mut received = 0u64;
        let mut buf = Vec::new();
        let mut body = resp.bytes_stream();
        while let Some(chunk) = body.next().await {
            if self.is_cancelled(key, generation, flag) {
                return Err("Download cancelled".to_string());
            }
            let chunk = chunk.map_err(|e| e.to_string())?;
            received += chunk.len() as u64;
            buf.extend_from_slice(&chunk);
            if total > 0 {
                self.upsert(DownloadEntry {
                    key: key.to_string(),
                    title: title.to_string(),
                    artist: artist.to_string(),
                    status: DownloadStatus::Downloading,
                    progress: (received as f64 / total as f64).min(1.0),
                    badge: badge.to_string(),
                    ..Default::default()
                });
            }
        }
        Ok(buf)
    }

    async fn fetch_cover_bytes(
        &self,
        resolver: &dyn DownloadStreamResolver,
        title: &str,
        artist: &str,
        artwork_url: &str,
    ) -> Option<(Vec<u8>, String)> {
        let mut url = artwork_url.trim().to_string();
        if url.is_empty() {
            if let Ok(Some(resolved)) = tokio::time::timeout(
                Duration::from_secs(8),
                resolver.resolve_artwork_url(title, artist),
            )
            .await
            {
                url = resolved;
            }
        }
        if url.is_empty() {
            return None;
        }
        let resp = tokio::time::timeout(
            Duration::from_secs(15),
            self.client.get(&url).send(),
        )
        .await
        .ok()?
        .ok()?;
        let bytes = tokio::time::timeout(Duration::from_secs(15), resp.bytes())
            .await
            .ok()?
            .ok()?
            .to_vec();
        if bytes.len() < 512 || bytes.len() > 12 * 1024 * 1024 {
            return None;
        }
        let mime = MediaTagger::sniff_image_mime(&bytes)?;
        Some((bytes, mime.to_string()))
    }

    async fn match_video_id(
        resolver: &dyn DownloadStreamResolver,
        title: &str,
        artist: &str,
    ) -> Option<String> {
        if let Some(strict) = resolver.match_best_video_id(title, artist).await {
            return Some(strict);
        }
        resolver.match_best_effort_video_id(title, artist).await
    }

    pub async fn download_track(
        &self,
        resolver: &dyn DownloadStreamResolver,
        req: DownloadTrackRequest,
    ) {
        let key = Self::key_of(&req.title, &req.artist);
        let generation = self
            .cancel_generation
            .lock()
            .map(|g| g.get(&key).copied().unwrap_or(0))
            .unwrap_or(0);
        let cancel_flag = Arc::new(AtomicBool::new(false));
        if self.is_cancelled(&key, generation, &cancel_flag) {
            return;
        }
        {
            let mut active = self.active.lock().expect("active mutex");
            if active.contains(&key) || self.is_downloaded(&req.title, &req.artist) {
                return;
            }
            active.insert(key.clone());
        }
        if let Ok(mut flags) = self.cancel_flags.lock() {
            flags.insert(key.clone(), Arc::clone(&cancel_flag));
        }
        if let Ok(mut m) = self.muted.lock() {
            m.remove(&key);
        }
        self.upsert(DownloadEntry {
            key: key.clone(),
            title: req.title.clone(),
            artist: req.artist.clone(),
            status: DownloadStatus::Downloading,
            ..Default::default()
        });

        let mut written_path: Option<PathBuf> = None;
        let mut committed = false;

        let outcome: Result<(), String> = async {
            if self.is_cancelled(&key, generation, &cancel_flag) {
                return Err("Download cancelled".to_string());
            }

            let mut stream: Option<ResolvedStream> = None;
            let mut badge = String::new();
            let mut is_lossless = false;
            let mut from_addon = false;
            let mut yt_codec_hint = String::new();

            let dl_quality = self.prefs.download_quality() as i32;
            if self.prefs.prefer_lossless()
                && dl_quality != AudioQualityTiers::YOUTUBE_ONLY
                && resolver.is_lossless_configured()
            {
                if let Ok(Some(s)) = tokio::time::timeout(
                    Duration::from_secs(30),
                    resolver.resolve_lossless_stream(
                        &req.title,
                        &req.artist,
                        &req.album,
                        dl_quality,
                    ),
                )
                .await
                {
                    is_lossless = s.is_lossless;
                    from_addon = true;
                    badge = s.quality_badge();
                    stream = Some(s);
                }
            }

            if stream.is_none() {
                let id = if !req.video_id.is_empty() {
                    Some(req.video_id.clone())
                } else {
                    Self::match_video_id(resolver, &req.title, &req.artist).await
                };
                let Some(video_id) = id else {
                    DiagnosticsLog::info(&format!(
                        "download \"{}\" - \"{}\" stage=match ok=false",
                        req.title, req.artist
                    ));
                    return Err("No playable source found".to_string());
                };
                let Some(yt) = resolver.resolve_youtube_download_stream(&video_id).await else {
                    DiagnosticsLog::info(&format!(
                        "download \"{}\" - \"{}\" stage=stream video={video_id} ok=false",
                        req.title, req.artist
                    ));
                    return Err("Stream unavailable".to_string());
                };
                yt_codec_hint = yt.audio_codec.to_uppercase();
                stream = Some(yt);
            }

            let stream = stream.expect("stream present");
            if self.is_cancelled(&key, generation, &cancel_flag) {
                return Err("Download cancelled".to_string());
            }

            let audio_bytes = self
                .fetch_audio_bytes(
                    &stream,
                    &key,
                    &req.title,
                    &req.artist,
                    &badge,
                    generation,
                    &cancel_flag,
                )
                .await?;
            if self.is_cancelled(&key, generation, &cancel_flag) {
                return Err("Download cancelled".to_string());
            }
            if audio_bytes.is_empty() {
                return Err("Empty audio payload".to_string());
            }

            let (lyrics_res, cover) = tokio::join!(
                tokio::time::timeout(
                    Duration::from_secs(20),
                    resolver.resolve_lyrics(&req.title, &req.artist, &req.album),
                ),
                self.fetch_cover_bytes(resolver, &req.title, &req.artist, &req.artwork_url),
            );
            let lyrics = lyrics_res.unwrap_or_else(|_| LyricsResult::empty());

            let lrc_content = if self.prefs.download_lyrics()
                && lyrics.is_synced
                && !lyrics.lines.is_empty()
            {
                let mut buf = String::new();
                for line in &lyrics.lines {
                    let m = line.time_ms / 60000;
                    let s = (line.time_ms % 60000) / 1000;
                    let cs = (line.time_ms % 1000) / 10;
                    buf.push_str(&format!("[{m:02}:{s:02}.{cs:02}]{}\n", line.text));
                }
                Some(buf)
            } else {
                None
            };

            let lyrics_text = if !lyrics.lines.is_empty() {
                lyrics
                    .lines
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_string()
            } else {
                lyrics.plain_lyrics.trim().to_string()
            };

            let (cover_bytes, cover_mime) = match cover {
                Some((b, m)) => (Some(b), m),
                None => (None, String::new()),
            };
            let tags = DownloadTags {
                title: req.title.clone(),
                artist: req.artist.clone(),
                album: req.album.clone(),
                lyrics: lyrics_text,
                cover_bytes,
                cover_mime,
            };

            let container = MediaTagger::detect_container(&audio_bytes);
            let mut final_ext = "webm";
            let mut out_bytes = audio_bytes.clone();
            let tag_status: String;

            let tag_res = match container {
                "webm" => {
                    final_ext = "opus";
                    MediaTagger::remux_webm_opus_to_ogg(&audio_bytes, &tags)
                }
                "mp4" => {
                    if MediaTagger::is_flac_in_mp4(&audio_bytes) {
                        match MediaTagger::remux_flac_in_mp4_to_flac(&audio_bytes, &tags) {
                            Ok(fl) => {
                                final_ext = "flac";
                                Ok(fl)
                            }
                            Err(_) => {
                                final_ext = "m4a";
                                MediaTagger::tag_m4a(&audio_bytes, &tags)
                            }
                        }
                    } else {
                        final_ext = "m4a";
                        MediaTagger::tag_m4a(&audio_bytes, &tags)
                    }
                }
                "flac" => {
                    final_ext = "flac";
                    MediaTagger::tag_flac(&audio_bytes, &tags)
                }
                "mp3" => {
                    final_ext = "mp3";
                    MediaTagger::tag_mp3(&audio_bytes, &tags)
                }
                _ => Err(super::media_tagger::TaggerSkip::new("unknown container")),
            };

            match tag_res {
                Ok(tagged) => {
                    out_bytes = tagged;
                    tag_status = if tags.has_cover() {
                        "full".to_string()
                    } else {
                        "text".to_string()
                    };
                }
                Err(skip) => {
                    final_ext = match container {
                        "mp4" => "m4a",
                        "flac" => "flac",
                        "mp3" => "mp3",
                        _ => "webm",
                    };
                    tag_status = format!("raw:{}", skip.reason);
                }
            }

            if !from_addon {
                badge = if container == "webm" {
                    "OPUS".to_string()
                } else if !yt_codec_hint.is_empty() {
                    yt_codec_hint
                } else {
                    "AUDIO".to_string()
                };
            }

            if self.is_cancelled(&key, generation, &cancel_flag) {
                return Err("Download cancelled".to_string());
            }

            let mut folder_year = req.year.clone();
            if folder_year.is_empty() && !out_bytes.is_empty() {
                let head_len = out_bytes.len().min(262_144);
                folder_year = LocalAudioTags::from_head(&out_bytes[..head_len]).year;
            }
            if folder_year.is_empty() {
                folder_year = LocalAudioTags::year_in(&req.album);
            }

            let dir = self.music_dir().map_err(|e| e.to_string())?;
            let filename = format!(
                "{}.{final_ext}",
                Self::sanitize(&format!("{} - {}", req.artist, req.title))
            );
            let file = self
                .layout_file(&dir, &req.artist, &req.album, &folder_year, &filename)
                .map_err(|e| e.to_string())?;
            fs::write(&file, &out_bytes).map_err(|e| e.to_string())?;
            written_path = Some(file.clone());
            if self.is_cancelled(&key, generation, &cancel_flag) {
                return Err("Download cancelled".to_string());
            }

            let file_str = file.to_string_lossy().into_owned();
            let mut lrc_path = String::new();
            if let Some(lrc_body) = lrc_content {
                let lrc = PathBuf::from(format!("{file_str}.lrc"));
                if fs::write(&lrc, lrc_body).is_ok() {
                    lrc_path = lrc.to_string_lossy().into_owned();
                }
            }

            let size_bytes = fs::metadata(&file).map(|m| m.len() as i64).unwrap_or(0);
            let now = Utc::now().timestamp_millis();
            self.db.with_conn(|conn| {
                let v7_res = conn.execute(
                    "INSERT INTO downloaded_tracks(\
                     track_key, title, artist, album, artwork_url, file_path, \
                     file_size_bytes, format_badge, tag_status, is_lossless, \
                     has_lyrics, lrc_file_path, downloaded_at_millis) \
                     VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                     ON CONFLICT(track_key) DO UPDATE SET file_path = excluded.file_path, \
                     file_size_bytes = excluded.file_size_bytes, format_badge = excluded.format_badge, \
                     tag_status = excluded.tag_status, \
                     lrc_file_path = excluded.lrc_file_path, downloaded_at_millis = excluded.downloaded_at_millis;",
                    params![
                        key,
                        req.title,
                        req.artist,
                        req.album,
                        req.artwork_url,
                        file_str,
                        size_bytes,
                        badge,
                        tag_status,
                        if is_lossless { 1 } else { 0 },
                        if lrc_path.is_empty() { 0 } else { 1 },
                        lrc_path,
                        now,
                    ],
                );
                if v7_res.is_err() {
                    let _ = conn.execute(
                        "INSERT INTO downloaded_tracks(\
                         track_key, title, artist, album, artwork_url, file_path, \
                         file_size_bytes, format_badge, is_lossless, \
                         has_lyrics, lrc_file_path, downloaded_at_millis) \
                         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                         ON CONFLICT(track_key) DO UPDATE SET file_path = excluded.file_path, \
                         file_size_bytes = excluded.file_size_bytes, format_badge = excluded.format_badge, \
                         lrc_file_path = excluded.lrc_file_path, downloaded_at_millis = excluded.downloaded_at_millis;",
                        params![
                            key,
                            req.title,
                            req.artist,
                            req.album,
                            req.artwork_url,
                            file_str,
                            size_bytes,
                            badge,
                            if is_lossless { 1 } else { 0 },
                            if lrc_path.is_empty() { 0 } else { 1 },
                            lrc_path,
                            now,
                        ],
                    );
                }
            });

            self.upsert(DownloadEntry {
                key: key.clone(),
                title: req.title.clone(),
                artist: req.artist.clone(),
                status: DownloadStatus::Done,
                progress: 1.0,
                badge,
                tag_note: tag_status,
                file_path: Some(file_str),
                error: None,
                artwork_url: req.artwork_url.clone(),
            });
            committed = true;
            Ok(())
        }
        .await;

        if let Err(err) = outcome {
            if self.is_cancelled(&key, generation, &cancel_flag) || err == "Download cancelled" {
                if let Some(p) = written_path {
                    if !committed {
                        let _ = fs::remove_file(&p);
                        let _ = fs::remove_file(format!("{}.lrc", p.to_string_lossy()));
                    }
                }
                self.drop_in_progress(&key);
            } else {
                self.upsert(DownloadEntry {
                    key: key.clone(),
                    title: req.title,
                    artist: req.artist,
                    status: DownloadStatus::Error,
                    error: Some(err),
                    ..Default::default()
                });
            }
        }

        if let Ok(mut active) = self.active.lock() {
            active.remove(&key);
        }
        if let Ok(mut flags) = self.cancel_flags.lock() {
            flags.remove(&key);
        }
    }

    pub fn delete(&self, key: &str) {
        let file_path = self
            .all()
            .into_iter()
            .find(|e| e.key == key)
            .and_then(|e| e.file_path);
        if let Some(p) = file_path {
            let _ = fs::remove_file(&p);
            let _ = fs::remove_file(format!("{p}.lrc"));
        }
        self.db.with_conn(|conn| {
            let _ = conn.execute("DELETE FROM downloaded_tracks WHERE track_key = ?;", [key]);
        });
        if let Ok(mut s) = self.state.write() {
            s.retain(|e| e.key != key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeLocalResolver {
        source_path: String,
    }

    impl DownloadStreamResolver for FakeLocalResolver {
        fn is_lossless_configured(&self) -> bool {
            true
        }

        fn resolve_lossless_stream<'a>(
            &'a self,
            _title: &'a str,
            _artist: &'a str,
            _album: &'a str,
            _preferred_quality: i32,
        ) -> BoxFuture<'a, Option<ResolvedStream>> {
            let url = self.source_path.clone();
            Box::pin(async move {
                Some(ResolvedStream {
                    url,
                    is_lossless: true,
                    bit_depth: 16,
                    sampling_rate_khz: 44.1,
                    audio_codec: "FLAC".to_string(),
                    ..Default::default()
                })
            })
        }

        fn match_best_video_id<'a>(
            &'a self,
            _title: &'a str,
            _artist: &'a str,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async { None })
        }

        fn match_best_effort_video_id<'a>(
            &'a self,
            _title: &'a str,
            _artist: &'a str,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async { None })
        }

        fn resolve_youtube_download_stream<'a>(
            &'a self,
            _video_id: &'a str,
        ) -> BoxFuture<'a, Option<ResolvedStream>> {
            Box::pin(async { None })
        }

        fn resolve_lyrics<'a>(
            &'a self,
            _title: &'a str,
            _artist: &'a str,
            _album: &'a str,
        ) -> BoxFuture<'a, LyricsResult> {
            Box::pin(async { LyricsResult::empty() })
        }

        fn resolve_artwork_url<'a>(
            &'a self,
            _title: &'a str,
            _artist: &'a str,
        ) -> BoxFuture<'a, Option<String>> {
            Box::pin(async { None })
        }
    }

    #[tokio::test]
    async fn downloads_local_flac_tags_and_persists_to_sqlite() {
        let dir = std::env::temp_dir().join(format!(
            "lastwave_dl_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let src_flac = dir.join("input.flac");
        let mut raw_flac = Vec::new();
        raw_flac.extend_from_slice(b"fLaC");
        raw_flac.push(0x80);
        raw_flac.extend_from_slice(&[0, 0, 34]);
        raw_flac.extend_from_slice(&[0x12; 34]);
        raw_flac.extend_from_slice(&[0xFF, 0xF8, 0x01, 0x02]);
        fs::write(&src_flac, &raw_flac).unwrap();

        let dl_dir = dir.join("downloads");
        let db = Arc::new(AppDatabase::in_memory());
        let prefs = Arc::new(Prefs::in_memory());
        prefs.set_download_dir(&dl_dir.to_string_lossy());
        prefs.set_download_structure("artist_album");

        let mgr = DownloadManager::new(Arc::clone(&db), Arc::clone(&prefs), dir.join("support"));
        let resolver = FakeLocalResolver {
            source_path: src_flac.to_string_lossy().into_owned(),
        };

        mgr.download_track(
            &resolver,
            DownloadTrackRequest {
                title: "Around the World".to_string(),
                artist: "Daft Punk".to_string(),
                album: "Homework".to_string(),
                ..Default::default()
            },
        )
        .await;

        assert!(mgr.is_downloaded("Around the World", "Daft Punk"));
        let out_path = mgr
            .local_path_for("Around the World", "Daft Punk")
            .expect("downloaded path");
        assert!(out_path.ends_with(".flac"));
        assert!(Path::new(&out_path).exists());

        mgr.delete(&DownloadManager::key_of("Around the World", "Daft Punk"));
        assert!(!mgr.is_downloaded("Around the World", "Daft Punk"));
        assert!(!Path::new(&out_path).exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
