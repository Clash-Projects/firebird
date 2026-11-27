//! Host side of Jam "Browser Guest Mode".
//!
//! Ported from `lib/features/jam/jam_browser_stream.dart`.
//! Browser guests get the YouTube version (M4A AAC, ~128 kbps) of the
//! room's current and next song, whatever the host itself plays. The host
//! fetches each file once in bounded 512 KB HTTP ranges and sends it over a
//! WebRTC data channel per guest.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{Mutex, Notify, RwLock};

use super::jam_client::{JamController, JamPhase, JamRtcSignal, JamSnapshot};

/// Above this many browser guests the host's upload starts to matter.
pub const JAM_BROWSER_GUEST_WARN_AT: usize = 8;
pub const MAX_GUESTS: usize = 30;
pub const CHUNK_SIZE: usize = 16 * 1024;
pub const RANGE_SIZE: usize = 512 * 1024;
pub const MAX_BUFFERED: usize = 1024 * 1024;
pub const ICE_SERVERS: &[&str] = &[
    "stun:stun.l.google.com:19302",
    "stun:stun.cloudflare.com:3478",
];

static VIDEO_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{11}$").unwrap());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JamBrowserState {
    pub on: bool,
    /// Browser guests with a live peer-to-peer connection.
    pub connected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadStreamRef {
    pub url: String,
    pub mime_type: String,
    pub request_headers: HashMap<String, String>,
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Narrow seam for resolving a YouTube download stream without coupling
/// `integrations::jam` to `extractor::innertube_api`.
pub trait DownloadStreamResolver: Send + Sync {
    fn resolve_download_stream<'a>(&'a self, video_id: &'a str) -> BoxFuture<'a, Option<DownloadStreamRef>>;
}

/// Narrow seam for a WebRTC guest data channel.
pub trait GuestDataChannel: Send + Sync {
    fn send_text<'a>(&'a self, text: String) -> BoxFuture<'a, Result<(), String>>;
    fn send_binary<'a>(&'a self, bytes: Vec<u8>) -> BoxFuture<'a, Result<(), String>>;
    fn buffered_amount<'a>(&'a self) -> BoxFuture<'a, usize>;
    fn is_connected(&self) -> bool;
    fn close<'a>(&'a self) -> BoxFuture<'a, ()>;
}

/// A song being fetched in ranges, shared by every guest that wants it.
pub struct SongFile {
    pub chunks: Mutex<Vec<Vec<u8>>>,
    pub size: Mutex<usize>,
    pub mime: Mutex<String>,
    pub done: Mutex<bool>,
    pub failed: Mutex<bool>,
    pub changed: Notify,
}

impl SongFile {
    fn new() -> Self {
        Self {
            chunks: Mutex::new(Vec::new()),
            size: Mutex::new(0),
            mime: Mutex::new("audio/mp4".to_string()),
            done: Mutex::new(false),
            failed: Mutex::new(false),
            changed: Notify::new(),
        }
    }
}

pub struct JamBrowserStream {
    jam: Arc<JamController>,
    resolver: Arc<dyn DownloadStreamResolver>,
    http: reqwest::Client,
    state: RwLock<JamBrowserState>,
    guests: Mutex<HashMap<String, Arc<dyn GuestDataChannel>>>,
    files: Mutex<HashMap<String, Arc<SongFile>>>,
    file_order: Mutex<Vec<String>>,
}

impl JamBrowserStream {
    pub fn new(
        jam: Arc<JamController>,
        resolver: Arc<dyn DownloadStreamResolver>,
        http: reqwest::Client,
    ) -> Self {
        Self {
            jam,
            resolver,
            http,
            state: RwLock::new(JamBrowserState::default()),
            guests: Mutex::new(HashMap::new()),
            files: Mutex::new(HashMap::new()),
            file_order: Mutex::new(Vec::new()),
        }
    }

    pub async fn state(&self) -> JamBrowserState {
        *self.state.read().await
    }

    pub async fn on_snapshot(self: &Arc<Self>, next: &JamSnapshot) {
        let allowed = next.phase == JamPhase::Active
            && next.is_main_host()
            && next.web_guests
            && !next.radio
            && !next.permanent;
        if !allowed {
            if self.state.read().await.on {
                self.stop().await;
            }
            return;
        }
        {
            let mut st = self.state.write().await;
            if !st.on {
                st.on = true;
            }
        }
        if !self.guests.lock().await.is_empty() {
            let mut ids = Vec::new();
            if let Some(ref t) = next.track {
                ids.push(t.id.as_str());
            }
            if let Some(q0) = next.queue.first() {
                ids.push(q0.id.as_str());
            }
            for id in ids {
                if Self::is_allowed_in_snapshot(next, id) {
                    let _ = self.get_or_fetch_song(id).await;
                }
            }
        }
    }

    pub async fn stop(&self) {
        *self.state.write().await = JamBrowserState::default();
        let all_ids: Vec<String> = self.guests.lock().await.keys().cloned().collect();
        for id in all_ids {
            self.close_guest(&id, true).await;
        }
        self.files.lock().await.clear();
        self.file_order.lock().await.clear();
    }

    pub async fn report(&self) {
        if !self.state.read().await.on {
            return;
        }
        let connected = self
            .guests
            .lock()
            .await
            .values()
            .filter(|g| g.is_connected())
            .count();
        *self.state.write().await = JamBrowserState {
            on: true,
            connected,
        };
    }

    pub async fn register_guest(&self, id: String, channel: Arc<dyn GuestDataChannel>) -> bool {
        let mut guests = self.guests.lock().await;
        if guests.len() >= MAX_GUESTS {
            drop(guests);
            self.jam
                .send_rtc_signal(&id, json!({ "kind": "bye", "reason": "busy" }))
                .await;
            return false;
        }
        guests.insert(id, channel);
        drop(guests);
        self.report().await;
        true
    }

    pub async fn on_signal(&self, s: &JamRtcSignal) {
        let kind = s.data.get("kind").and_then(Value::as_str).unwrap_or("");
        if kind == "bye" {
            self.close_guest(&s.from, false).await;
        }
    }

    pub async fn close_guest(&self, id: &str, bye: bool) {
        let removed = self.guests.lock().await.remove(id);
        let Some(g) = removed else {
            return;
        };
        if bye {
            self.jam
                .send_rtc_signal(id, json!({ "kind": "bye" }))
                .await;
        }
        g.close().await;
        self.report().await;
    }

    pub fn is_allowed_in_snapshot(jam: &JamSnapshot, song: &str) -> bool {
        if !VIDEO_ID_RE.is_match(song) {
            return false;
        }
        jam.track.as_ref().map(|t| t.id.as_str()) == Some(song)
            || jam.queue.iter().any(|t| t.id == song)
    }

    pub async fn is_allowed(&self, song: &str) -> bool {
        let snap = self.jam.snapshot().await;
        Self::is_allowed_in_snapshot(&snap, song)
    }

    pub async fn handle_guest_message(
        self: &Arc<Self>,
        id: &str,
        channel: &Arc<dyn GuestDataChannel>,
        text: &str,
    ) {
        let Ok(Value::Object(m)) = serde_json::from_str::<Value>(text) else {
            return;
        };
        if m.get("kind").and_then(Value::as_str) != Some("get") {
            return;
        }
        let song = m.get("id").and_then(Value::as_str).unwrap_or("");
        if song.is_empty() {
            return;
        }
        self.send_song(id, channel, song).await;
    }

    async fn send_song(
        self: &Arc<Self>,
        id: &str,
        channel: &Arc<dyn GuestDataChannel>,
        song: &str,
    ) {
        let send_json = |v: Value| channel.send_text(v.to_string());

        if !VIDEO_ID_RE.is_match(song) {
            let _ = send_json(json!({ "kind": "missing", "id": song, "reason": "no_youtube" })).await;
            return;
        }
        if !self.is_allowed(song).await {
            let _ = send_json(json!({ "kind": "missing", "id": song, "reason": "not_allowed" })).await;
            return;
        }

        let file = self.get_or_fetch_song(song).await;
        loop {
            let size = *file.size.lock().await;
            let failed = *file.failed.lock().await;
            if size > 0 || failed {
                break;
            }
            file.changed.notified().await;
        }

        if *file.failed.lock().await && *file.size.lock().await == 0 {
            let _ = send_json(json!({ "kind": "missing", "id": song })).await;
            return;
        }

        let size = *file.size.lock().await;
        let mime = file.mime.lock().await.clone();
        if send_json(json!({
            "kind": "file",
            "id": song,
            "size": size,
            "mime": mime,
        }))
        .await
        .is_err()
        {
            return;
        }

        let mut i = 0usize;
        loop {
            loop {
                let len = file.chunks.lock().await.len();
                let done = *file.done.lock().await;
                let failed = *file.failed.lock().await;
                if i < len || done || failed {
                    break;
                }
                file.changed.notified().await;
            }
            if !self.guests.lock().await.contains_key(id) || !self.is_allowed(song).await {
                return;
            }
            let maybe_bytes = file.chunks.lock().await.get(i).cloned();
            let Some(bytes) = maybe_bytes else {
                break;
            };
            for slice in bytes.chunks(CHUNK_SIZE) {
                while channel.buffered_amount().await > MAX_BUFFERED {
                    if !self.guests.lock().await.contains_key(id) {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(40)).await;
                }
                if channel.send_binary(slice.to_vec()).await.is_err() {
                    return;
                }
            }
            i += 1;
        }

        let done = *file.done.lock().await;
        let _ = send_json(json!({
            "kind": if done { "end" } else { "missing" },
            "id": song,
        }))
        .await;
    }

    async fn get_or_fetch_song(self: &Arc<Self>, song: &str) -> Arc<SongFile> {
        {
            let files = self.files.lock().await;
            if let Some(hit) = files.get(song) {
                if !*hit.failed.lock().await {
                    return Arc::clone(hit);
                }
            }
        }

        let file = Arc::new(SongFile::new());
        {
            let mut files = self.files.lock().await;
            let mut order = self.file_order.lock().await;
            order.retain(|k| k != song);
            if order.len() >= 4 {
                let oldest = order.remove(0);
                files.remove(&oldest);
            }
            files.insert(song.to_string(), Arc::clone(&file));
            order.push(song.to_string());
        }

        let this = Arc::clone(self);
        let song_id = song.to_string();
        let file_ref = Arc::clone(&file);
        tokio::spawn(async move {
            this.fetch_ranges(&song_id, &file_ref).await;
        });
        file
    }

    async fn fetch_ranges(&self, song: &str, file: &SongFile) {
        let stream_res = tokio::time::timeout(
            Duration::from_secs(25),
            self.resolver.resolve_download_stream(song),
        )
        .await;

        let Ok(Some(stream)) = stream_res else {
            *file.failed.lock().await = true;
            file.changed.notify_waiters();
            return;
        };
        if !stream.url.starts_with("http") {
            *file.failed.lock().await = true;
            file.changed.notify_waiters();
            return;
        }
        *file.mime.lock().await = stream.mime_type.clone();

        let mut at: usize = 0;
        let mut total: usize = 1;
        while at < total {
            let evicted = !self.files.lock().await.contains_key(song);
            if evicted {
                *file.failed.lock().await = true;
                file.changed.notify_waiters();
                return;
            }

            let mut response = None;
            for _attempt in 0..3 {
                let mut req = self.http.get(&stream.url);
                for (k, v) in &stream.request_headers {
                    req = req.header(k.as_str(), v.as_str());
                }
                req = req.header("Range", format!("bytes={at}-{}", at + RANGE_SIZE - 1));
                if let Ok(res) = req.send().await {
                    response = Some(res);
                    break;
                }
            }
            let Some(res) = response else {
                *file.failed.lock().await = true;
                file.changed.notify_waiters();
                return;
            };

            let status = res.status().as_u16();
            let content_range = res
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let Ok(data) = res.bytes().await else {
                *file.failed.lock().await = true;
                file.changed.notify_waiters();
                return;
            };
            if data.is_empty() {
                *file.failed.lock().await = true;
                file.changed.notify_waiters();
                return;
            }

            total = if status == 206 {
                content_range
                    .rsplit('/')
                    .next()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(at + data.len())
            } else {
                data.len()
            };

            {
                let mut sz = file.size.lock().await;
                if *sz == 0 {
                    *sz = total;
                }
            }
            file.chunks.lock().await.push(data.to_vec());
            at += data.len();
            file.changed.notify_waiters();
            if status != 206 {
                break;
            }
        }

        *file.done.lock().await = true;
        file.changed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::jam::jam_client::JamTrack;

    #[test]
    fn allowed_song_checks_video_id_and_room_state() {
        let mut snap = JamSnapshot::default();
        snap.track = Some(JamTrack {
            id: "dQw4w9WgXcQ".to_string(),
            title: "Song".to_string(),
            artist: "Artist".to_string(),
            album: None,
            artwork_url: None,
            duration_ms: None,
        });
        snap.queue = vec![JamTrack {
            id: "abc12345678".to_string(),
            title: "Next".to_string(),
            artist: "Artist".to_string(),
            album: None,
            artwork_url: None,
            duration_ms: None,
        }];
        assert!(JamBrowserStream::is_allowed_in_snapshot(&snap, "dQw4w9WgXcQ"));
        assert!(JamBrowserStream::is_allowed_in_snapshot(&snap, "abc12345678"));
        assert!(!JamBrowserStream::is_allowed_in_snapshot(&snap, "not-a-vid-id"));
        assert!(!JamBrowserStream::is_allowed_in_snapshot(&snap, "zzz12345678"));
    }
}
