//! WebSocket client for the Clash Projects Jam server.
//!
//! Port of `lib/features/jam/jam_client.dart`.
//! A failed connection sets `JamPhase::Error` and does not touch playback.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Local, TimeZone, Timelike};
use futures_util::{SinkExt, StreamExt};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_tungstenite::tungstenite::Message;

use crate::storage::prefs::Prefs;

pub const JAM_DEFAULT_SERVER: &str = "wss://jam.clashgram.org/ws";
pub const JAM_MODE_COLLABORATIVE: &str = "collaborative";
pub const JAM_MODE_HOST_ONLY: &str = "host_only";

/// "3:42:10 PM" for a wall-clock millisecond timestamp.
pub fn jam_clock(ms: i64) -> String {
    let Some(dt) = Local.timestamp_millis_opt(ms).single() else {
        return String::new();
    };
    let hour24 = dt.hour();
    let h = if hour24 % 12 == 0 { 12 } else { hour24 % 12 };
    let am_pm = if hour24 < 12 { "AM" } else { "PM" };
    format!("{}:{:02}:{:02} {}", h, dt.minute(), dt.second(), am_pm)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JamStation {
    pub code: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
}

/// Always-active stations. The server provisions these and never closes them.
pub const JAM_STATIONS: &[JamStation] = &[
    JamStation {
        code: "INDIA",
        title: "India",
        blurb: "Always-on room for India",
    },
    JamStation {
        code: "WORLD",
        title: "International",
        blurb: "Always-on room for everyone else",
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JamPerson {
    pub user_id: String,
    pub name: String,
    /// True for the main host and the co-host.
    pub is_host: bool,
    pub is_co_host: bool,
    /// Joined from the browser listen page (no app).
    pub is_web: bool,
}

/// WebRTC signaling relayed by the server between the host and one
/// browser guest. `data["kind"]` is hello / offer / answer / bye.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JamRtcSignal {
    pub from: String,
    pub data: Value,
}

/// Snapshot of the message a chat line replies to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JamReply {
    pub id: String,
    pub user_name: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JamChatLine {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub is_host: bool,
    pub text: String,
    pub timestamp: i64,
    pub reply_to: Option<JamReply>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JamTrack {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub artwork_url: Option<String>,
    pub duration_ms: Option<i64>,
}

impl JamTrack {
    pub fn from_value(v: &Value) -> Option<Self> {
        let map = v.as_object()?;
        let id = val_to_string(map.get("id"));
        let title = val_to_string(map.get("title"));
        let artist = val_to_string(map.get("artist"));
        let album = map.get("album").and_then(|x| {
            if x.is_null() {
                None
            } else {
                Some(val_to_string(Some(x)))
            }
        });
        let artwork_url = map.get("artwork_url").and_then(|x| {
            if x.is_null() {
                None
            } else {
                Some(val_to_string(Some(x)))
            }
        });
        let duration_ms = map.get("duration_ms").and_then(|x| {
            x.as_i64()
                .or_else(|| x.as_f64().map(|f| f as i64))
                .or_else(|| x.as_str().and_then(|s| s.parse::<i64>().ok()))
        });
        Some(Self {
            id,
            title,
            artist,
            album,
            artwork_url,
            duration_ms,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JamRemote {
    Play {
        position_ms: i64,
        server_time_ms: i64,
        from_user_id: Option<String>,
    },
    Pause {
        position_ms: i64,
        from_user_id: Option<String>,
    },
    Seek {
        position_ms: i64,
        from_user_id: Option<String>,
    },
    Track {
        track: JamTrack,
        is_playing: bool,
        position_ms: i64,
        from_user_id: Option<String>,
    },
    Full {
        track: Option<JamTrack>,
        is_playing: bool,
        position_ms: i64,
        server_time_ms: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum JamPhase {
    #[default]
    Disconnected,
    Connecting,
    Active,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JamSnapshot {
    pub phase: JamPhase,
    pub error: String,
    pub room_code: String,
    pub user_id: String,
    pub display_name: String,
    pub is_host: bool,
    pub host_id: String,
    pub co_host_id: String,
    pub permanent: bool,
    /// Always-on rooms are 24/7 radio stations driven by the server.
    pub radio: bool,
    /// Station on auto radio (true) or run by a station admin (false).
    pub station_auto: bool,
    pub room_mode: String,
    pub people: Vec<JamPerson>,
    pub chat: Vec<JamChatLine>,
    pub track: Option<JamTrack>,
    pub is_playing: bool,
    /// The admin's upcoming queue, shared read-only with the room.
    pub queue: Vec<JamTrack>,
    /// Friendly title of an always-on room ("India"); empty otherwise.
    pub room_name: String,
    /// Wall-clock ms until which pause/skip are locked for you; 0 when free.
    pub cooldown_until_ms: i64,
    /// Transient message from the server (rate limit, host rules).
    pub notice: String,
    /// Wall-clock ms when the playback cooldown ends; 0 when none.
    pub retry_at_ms: i64,
    /// The main host allows browser guests (listen page) in this room.
    pub web_guests: bool,
}

impl Default for JamSnapshot {
    fn default() -> Self {
        Self {
            phase: JamPhase::Disconnected,
            error: String::new(),
            room_code: String::new(),
            user_id: String::new(),
            display_name: String::new(),
            is_host: false,
            host_id: String::new(),
            co_host_id: String::new(),
            permanent: false,
            radio: false,
            station_auto: true,
            room_mode: JAM_MODE_COLLABORATIVE.to_string(),
            people: Vec::new(),
            chat: Vec::new(),
            track: None,
            is_playing: false,
            queue: Vec::new(),
            room_name: String::new(),
            cooldown_until_ms: 0,
            notice: String::new(),
            retry_at_ms: 0,
            web_guests: false,
        }
    }
}

impl JamSnapshot {
    /// True only for the main host, who may give and remove co-hosts.
    pub fn is_main_host(&self) -> bool {
        !self.user_id.is_empty() && self.user_id == self.host_id
    }

    /// A listener on a live station: no playback control at all.
    pub fn is_station_listener(&self) -> bool {
        self.phase == JamPhase::Active && self.radio && !self.is_host
    }
}

fn is_default_name(name: &str) -> bool {
    matches!(
        name.trim().to_lowercase().as_str(),
        "" | "listener" | "host" | "guest" | "user" | "anonymous"
    )
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

static CODE_SPLIT_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[&#\s]").unwrap());

pub struct JamController {
    prefs: Arc<Prefs>,
    http: reqwest::Client,
    state: Arc<RwLock<JamSnapshot>>,
    remote_tx: broadcast::Sender<JamRemote>,
    rtc_tx: broadcast::Sender<JamRtcSignal>,
    snapshot_tx: broadcast::Sender<JamSnapshot>,
    generation: Arc<AtomicU64>,
    outbound_tx: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
    signal_queue: Arc<Mutex<Vec<String>>>,
    signal_pump_running: Arc<Mutex<bool>>,
    pending_web_guests: Arc<Mutex<bool>>,
}

impl JamController {
    pub fn new(prefs: Arc<Prefs>, http: reqwest::Client) -> Self {
        let (remote_tx, _) = broadcast::channel(64);
        let (rtc_tx, _) = broadcast::channel(64);
        let (snapshot_tx, _) = broadcast::channel(64);
        Self {
            prefs,
            http,
            state: Arc::new(RwLock::new(JamSnapshot::default())),
            remote_tx,
            rtc_tx,
            snapshot_tx,
            generation: Arc::new(AtomicU64::new(0)),
            outbound_tx: Arc::new(Mutex::new(None)),
            signal_queue: Arc::new(Mutex::new(Vec::new())),
            signal_pump_running: Arc::new(Mutex::new(false)),
            pending_web_guests: Arc::new(Mutex::new(false)),
        }
    }

    pub fn subscribe_snapshot(&self) -> broadcast::Receiver<JamSnapshot> {
        self.snapshot_tx.subscribe()
    }

    pub async fn snapshot(&self) -> JamSnapshot {
        self.state.read().await.clone()
    }

    pub fn subscribe_remote(&self) -> broadcast::Receiver<JamRemote> {
        self.remote_tx.subscribe()
    }

    pub fn subscribe_rtc(&self) -> broadcast::Receiver<JamRtcSignal> {
        self.rtc_tx.subscribe()
    }

    pub fn server_url(&self) -> String {
        let raw = self.prefs.jam_server_url();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            JAM_DEFAULT_SERVER.to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Shareable https link; opens the web invite page.
    pub fn invite_link(&self, code: &str) -> String {
        let Ok(u) = url::Url::parse(&self.server_url()) else {
            return format!("https://jam.clashgram.org/?code={code}");
        };
        let scheme = if u.scheme() == "wss" { "https" } else { "http" };
        let host = u.host_str().unwrap_or("jam.clashgram.org");
        match u.port() {
            Some(port) => format!("{scheme}://{host}:{port}/?code={code}"),
            None => format!("{scheme}://{host}/?code={code}"),
        }
    }

    /// Accepts a bare code or a pasted invite link (`...?code=ABC123`).
    pub fn parse_code(raw: &str) -> String {
        let text = raw.trim();
        let lower = text.to_lowercase();
        let tail = if let Some(at) = lower.find("code=") {
            &text[at + 5..]
        } else {
            text
        };
        CODE_SPLIT_REGEX
            .split(tail)
            .next()
            .unwrap_or("")
            .trim()
            .to_uppercase()
    }

    /// Main host only: allow or stop browser guests.
    pub async fn set_web_guests(&self, enabled: bool) {
        self.send("set_web_guests", Some(json!({ "enabled": enabled })))
            .await;
    }

    /// Relays WebRTC signaling to one browser guest. Paced at 5/s: the server
    /// drops (and eventually disconnects) clients above 10 messages/s.
    pub async fn send_rtc_signal(&self, to: &str, data: Value) {
        let msg = json!({
            "type": "rtc_signal",
            "payload": { "to": to, "data": data },
        })
        .to_string();
        {
            let mut q = self.signal_queue.lock().await;
            q.push(msg);
        }
        let mut running = self.signal_pump_running.lock().await;
        if *running {
            return;
        }
        *running = true;
        drop(running);

        let queue = Arc::clone(&self.signal_queue);
        let pump_flag = Arc::clone(&self.signal_pump_running);
        let tx_holder = Arc::clone(&self.outbound_tx);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(200)).await;
                let next = {
                    let mut q = queue.lock().await;
                    if q.is_empty() {
                        None
                    } else {
                        Some(q.remove(0))
                    }
                };
                match next {
                    Some(text) => {
                        if let Some(ref tx) = *tx_holder.lock().await {
                            let _ = tx.send(text);
                        }
                    }
                    None => {
                        let mut flag = pump_flag.lock().await;
                        *flag = false;
                        break;
                    }
                }
            }
        });
    }

    pub async fn create_room(
        &self,
        display_name: &str,
        collaborative: bool,
        browser_guests: bool,
    ) {
        let name = if display_name.trim().is_empty() {
            "Listener"
        } else {
            display_name.trim()
        };
        *self.pending_web_guests.lock().await = browser_guests;
        let open_msg = json!({
            "type": "create_room",
            "payload": {
                "name": name,
                "room_mode": if collaborative { JAM_MODE_COLLABORATIVE } else { JAM_MODE_HOST_ONLY },
            }
        })
        .to_string();
        self.connect(open_msg, None).await;
    }

    pub async fn join_room(&self, room_code: &str, display_name: &str) {
        let code = Self::parse_code(room_code);
        let name = if display_name.trim().is_empty() {
            "Listener"
        } else {
            display_name.trim()
        };
        if code.is_empty() {
            let mut st = self.state.write().await;
            st.phase = JamPhase::Error;
            st.error = "Room code is required".to_string();
            let _ = self.snapshot_tx.send(st.clone());
            return;
        }
        if JAM_STATIONS.iter().any(|s| s.code == code) && is_default_name(display_name) {
            let mut st = self.state.write().await;
            st.phase = JamPhase::Error;
            st.error = "Pick your own display name to join this public room".to_string();
            let _ = self.snapshot_tx.send(st.clone());
            return;
        }
        let open_msg = json!({
            "type": "join_room",
            "payload": {
                "room_code": code,
                "name": name,
            }
        })
        .to_string();
        self.connect(open_msg, Some(code)).await;
    }

    pub async fn leave(&self) {
        self.send("leave_room", None).await;
        self.close(true).await;
    }

    pub async fn send_chat(&self, text: &str, reply_to_id: Option<&str>) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        let mut payload = serde_json::Map::new();
        payload.insert("text".to_string(), json!(trimmed));
        if let Some(reply) = reply_to_id.filter(|s| !s.is_empty()) {
            payload.insert("reply_to_id".to_string(), json!(reply));
        }
        self.send("send_chat", Some(Value::Object(payload))).await;
    }

    pub async fn grant_host(&self, user_id: &str) {
        self.send("grant_host", Some(json!({ "user_id": user_id })))
            .await;
    }

    pub async fn transfer_admin(&self, user_id: &str) {
        self.send("transfer_host", Some(json!({ "user_id": user_id })))
            .await;
    }

    pub async fn revoke_host(&self, user_id: &str) {
        self.send("revoke_host", Some(json!({ "user_id": user_id })))
            .await;
    }

    pub async fn broadcast_queue(&self, queue: Vec<JamTrack>) {
        if self.state.read().await.phase != JamPhase::Active {
            return;
        }
        let shared: Vec<JamTrack> = queue.into_iter().take(5).collect();
        self.send("queue_update", Some(json!({ "queue": shared })))
            .await;
        let mut st = self.state.write().await;
        st.queue = shared;
        let _ = self.snapshot_tx.send(st.clone());
    }

    /// Live listener counts for the always-on rooms (`{code: count}`).
    pub async fn fetch_room_counts(&self) -> HashMap<String, i64> {
        let Ok(mut u) = url::Url::parse(&self.server_url()) else {
            return HashMap::new();
        };
        let scheme = if u.scheme() == "wss" { "https" } else { "http" };
        let _ = u.set_scheme(scheme);
        u.set_path("/rooms");
        u.set_query(None);

        let Ok(res) = self
            .http
            .get(u.as_str())
            .timeout(Duration::from_secs(5))
            .send()
            .await
        else {
            return HashMap::new();
        };
        if !res.status().is_success() {
            return HashMap::new();
        }
        let Ok(text) = res.text().await else {
            return HashMap::new();
        };
        let Ok(Value::Object(decoded)) = serde_json::from_str::<Value>(&text) else {
            return HashMap::new();
        };
        let Some(rooms) = decoded.get("rooms").and_then(Value::as_array) else {
            return HashMap::new();
        };
        let mut counts = HashMap::new();
        for r in rooms {
            if let Some(obj) = r.as_object() {
                let code = val_to_string(obj.get("code"));
                let participants = val_to_i64(obj.get("participants"));
                if !code.is_empty() {
                    counts.insert(code, participants);
                }
            }
        }
        counts
    }

    pub async fn clear_notice(&self) {
        let mut st = self.state.write().await;
        if st.notice.is_empty() && st.retry_at_ms == 0 {
            return;
        }
        st.notice.clear();
        st.retry_at_ms = 0;
        let _ = self.snapshot_tx.send(st.clone());
    }

    pub async fn broadcast_play(&self, position_ms: i64) {
        self.action(json!({
            "action": "play",
            "position_ms": position_ms,
            "client_time_ms": now_ms(),
            "is_playing": true,
        }))
        .await;
    }

    pub async fn broadcast_pause(&self, position_ms: i64) {
        self.action(json!({
            "action": "pause",
            "position_ms": position_ms,
            "client_time_ms": now_ms(),
            "is_playing": false,
        }))
        .await;
    }

    pub async fn broadcast_seek(&self, position_ms: i64) {
        self.action(json!({
            "action": "seek",
            "position_ms": position_ms,
            "client_time_ms": now_ms(),
        }))
        .await;
    }

    pub async fn broadcast_track(&self, track: &JamTrack, position_ms: i64, is_playing: bool) {
        self.action(json!({
            "action": "change_track",
            "track": track,
            "position_ms": position_ms,
            "client_time_ms": now_ms(),
            "is_playing": is_playing,
        }))
        .await;
    }

    async fn action(&self, payload: Value) {
        if self.state.read().await.phase != JamPhase::Active {
            return;
        }
        self.send("playback_action", Some(payload)).await;
    }

    async fn connect(&self, open_msg: String, code: Option<String>) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.close(false).await;
        let snap_tx_connect = self.snapshot_tx.clone();
        {
            let mut st = self.state.write().await;
            *st = JamSnapshot {
                phase: JamPhase::Connecting,
                room_code: code.unwrap_or_default(),
                ..Default::default()
            };
            let _ = snap_tx_connect.send(st.clone());
        }

        let url = self.server_url();
        let ws_res = tokio::time::timeout(
            Duration::from_secs(12),
            tokio_tungstenite::connect_async(&url),
        )
        .await;

        let Ok(Ok((ws_stream, _))) = ws_res else {
            if self.generation.load(Ordering::SeqCst) == generation {
                let err = match ws_res {
                    Err(_) => "Connection timed out".to_string(),
                    Ok(Err(e)) => e.to_string(),
                    Ok(Ok(_)) => unreachable!(),
                };
                let mut st = self.state.write().await;
                *st = JamSnapshot {
                    phase: JamPhase::Error,
                    error: err,
                    ..Default::default()
                };
                let _ = snap_tx_connect.send(st.clone());
            }
            return;
        };

        if self.generation.load(Ordering::SeqCst) != generation {
            return;
        }

        let (mut write, mut read) = ws_stream.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let _ = tx.send(open_msg);
        *self.outbound_tx.lock().await = Some(tx.clone());

        let gen_ref = Arc::clone(&self.generation);
        let state_ref = Arc::clone(&self.state);
        let remote_tx = self.remote_tx.clone();
        let rtc_tx = self.rtc_tx.clone();
        let snapshot_tx = self.snapshot_tx.clone();
        let pending_wg = Arc::clone(&self.pending_web_guests);
        let out_tx_clone = tx.clone();

        // Writer + 25s ping task
        tokio::spawn(async move {
            let mut ping_interval = tokio::time::interval(Duration::from_secs(25));
            ping_interval.tick().await;
            loop {
                tokio::select! {
                    maybe_msg = rx.recv() => {
                        let Some(msg) = maybe_msg else { break; };
                        if write.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                    _ = ping_interval.tick() => {
                        let ping = json!({ "type": "ping", "payload": Value::Null }).to_string();
                        if write.send(Message::Text(ping.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = write.close().await;
        });

        // Reader task
        tokio::spawn(async move {
            while let Some(msg_res) = read.next().await {
                if gen_ref.load(Ordering::SeqCst) != generation {
                    break;
                }
                match msg_res {
                    Ok(Message::Text(text)) => {
                        handle_message(
                            &text,
                            &state_ref,
                            &remote_tx,
                            &rtc_tx,
                            &pending_wg,
                            &out_tx_clone,
                        )
                        .await;
                        let snap = state_ref.read().await.clone();
                        let _ = snapshot_tx.send(snap);
                    }
                    Ok(Message::Close(_)) => break,
                    Err(e) => {
                        if gen_ref.load(Ordering::SeqCst) == generation {
                            let mut st = state_ref.write().await;
                            st.phase = JamPhase::Error;
                            st.error = e.to_string();
                            let snap = st.clone();
                            drop(st);
                            let _ = snapshot_tx.send(snap);
                        }
                        break;
                    }
                    _ => {}
                }
            }
            if gen_ref.load(Ordering::SeqCst) == generation {
                let mut st = state_ref.write().await;
                if st.phase != JamPhase::Error {
                    *st = JamSnapshot::default();
                }
                let snap = st.clone();
                drop(st);
                let _ = snapshot_tx.send(snap);
            }
        });
    }

    async fn send(&self, msg_type: &str, payload: Option<Value>) {
        if let Some(ref tx) = *self.outbound_tx.lock().await {
            let text = json!({
                "type": msg_type,
                "payload": payload.unwrap_or(Value::Null),
            })
            .to_string();
            let _ = tx.send(text);
        }
    }

    async fn close(&self, clear: bool) {
        self.signal_queue.lock().await.clear();
        *self.outbound_tx.lock().await = None;
        if clear {
            let mut st = self.state.write().await;
            *st = JamSnapshot::default();
            let _ = self.snapshot_tx.send(st.clone());
        }
    }
}

async fn handle_message(
    text: &str,
    state_ref: &Arc<RwLock<JamSnapshot>>,
    remote_tx: &broadcast::Sender<JamRemote>,
    rtc_tx: &broadcast::Sender<JamRtcSignal>,
    pending_web_guests: &Arc<Mutex<bool>>,
    outbound_tx: &mpsc::UnboundedSender<String>,
) {
    let Ok(Value::Object(decoded)) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let msg_type = val_to_string(decoded.get("type"));
    let empty_map = serde_json::Map::new();
    let map = decoded
        .get("payload")
        .and_then(Value::as_object)
        .unwrap_or(&empty_map);

    match msg_type.as_str() {
        "room_created" => {
            let user_id = val_to_string(map.get("user_id"));
            let raw_name = val_to_string(map.get("name"));
            let name = if raw_name.is_empty() {
                "Host".to_string()
            } else {
                raw_name
            };
            let room_mode = {
                let rm = val_to_string(map.get("room_mode"));
                if rm.is_empty() {
                    JAM_MODE_COLLABORATIVE.to_string()
                } else {
                    rm
                }
            };
            {
                let mut st = state_ref.write().await;
                *st = JamSnapshot {
                    phase: JamPhase::Active,
                    room_code: val_to_string(map.get("room_code")),
                    user_id: user_id.clone(),
                    display_name: name.clone(),
                    is_host: true,
                    host_id: user_id.clone(),
                    room_mode,
                    people: vec![JamPerson {
                        user_id,
                        name,
                        is_host: true,
                        is_co_host: false,
                        is_web: false,
                    }],
                    ..Default::default()
                };
            }
            let mut pwg = pending_web_guests.lock().await;
            if *pwg {
                *pwg = false;
                let msg = json!({
                    "type": "set_web_guests",
                    "payload": { "enabled": true },
                })
                .to_string();
                let _ = outbound_tx.send(msg);
            }
        }
        "sync_state" => {
            let playback = map
                .get("playback")
                .and_then(Value::as_object)
                .unwrap_or(&empty_map);
            let track = playback.get("current_track").and_then(JamTrack::from_value);
            let is_playing = playback.get("is_playing").and_then(Value::as_bool) == Some(true);
            let room_mode = {
                let rm = val_to_string(map.get("room_mode"));
                if rm.is_empty() {
                    JAM_MODE_COLLABORATIVE.to_string()
                } else {
                    rm
                }
            };
            {
                let mut st = state_ref.write().await;
                *st = JamSnapshot {
                    phase: JamPhase::Active,
                    room_code: val_to_string(map.get("room_code")),
                    user_id: val_to_string(map.get("user_id")),
                    display_name: val_to_string(map.get("name")),
                    is_host: map.get("is_host").and_then(Value::as_bool) == Some(true),
                    host_id: val_to_string(map.get("host_id")),
                    co_host_id: val_to_string(map.get("cohost_id")),
                    permanent: map.get("permanent").and_then(Value::as_bool) == Some(true),
                    radio: map.get("radio").and_then(Value::as_bool) == Some(true),
                    station_auto: map.get("station_auto").and_then(Value::as_bool) != Some(false),
                    room_name: val_to_string(map.get("room_name")),
                    room_mode,
                    people: parse_people(map.get("participants")),
                    chat: parse_chat(map.get("chat_history")),
                    track: track.clone(),
                    is_playing,
                    queue: parse_tracks(playback.get("queue")),
                    web_guests: map.get("web_guests").and_then(Value::as_bool) == Some(true),
                    ..Default::default()
                };
            }
            let _ = remote_tx.send(JamRemote::Full {
                track,
                is_playing,
                position_ms: val_to_i64(playback.get("position_ms")),
                server_time_ms: val_to_i64(playback.get("server_time_ms")),
            });
        }
        "user_joined" => {
            let mut st = state_ref.write().await;
            if st.phase != JamPhase::Active {
                return;
            }
            let person = JamPerson {
                user_id: val_to_string(map.get("user_id")),
                name: val_to_string(map.get("name")),
                is_host: map.get("is_host").and_then(Value::as_bool) == Some(true),
                is_co_host: false,
                is_web: map.get("is_web").and_then(Value::as_bool) == Some(true),
            };
            st.people.retain(|p| p.user_id != person.user_id);
            st.people.push(person);
        }
        "user_left" => {
            let id = val_to_string(map.get("user_id"));
            {
                let mut st = state_ref.write().await;
                if st.phase != JamPhase::Active {
                    return;
                }
                st.people.retain(|p| p.user_id != id);
            }
            let _ = rtc_tx.send(JamRtcSignal {
                from: id,
                data: json!({ "kind": "bye" }),
            });
        }
        "web_guests_changed" => {
            let mut st = state_ref.write().await;
            if st.phase != JamPhase::Active {
                return;
            }
            st.web_guests = map.get("enabled").and_then(Value::as_bool) == Some(true);
        }
        "rtc_signal" => {
            if let Some(data) = map.get("data").filter(|v| v.is_object()) {
                let _ = rtc_tx.send(JamRtcSignal {
                    from: val_to_string(map.get("from")),
                    data: data.clone(),
                });
            }
        }
        "host_changed" => {
            let mut st = state_ref.write().await;
            if st.phase != JamPhase::Active {
                return;
            }
            let h1 = val_to_string(map.get("host_id"));
            let h2 = val_to_string(map.get("new_host_id"));
            let h3 = val_to_string(map.get("user_id"));
            let host_id = if !h1.is_empty() {
                h1
            } else if !h2.is_empty() {
                h2
            } else {
                h3
            };
            let cohost_id = val_to_string(map.get("cohost_id"));
            apply_roles(&mut st, host_id, cohost_id);
        }
        "station_mode" => {
            let mut st = state_ref.write().await;
            if st.phase != JamPhase::Active {
                return;
            }
            st.station_auto = map.get("auto").and_then(Value::as_bool) == Some(true);
        }
        "cooldown" => {
            start_cooldown(state_ref, val_to_i64(map.get("retry_at_ms"))).await;
        }
        "queue_updated" => {
            let mut st = state_ref.write().await;
            if st.phase != JamPhase::Active {
                return;
            }
            st.queue = parse_tracks(map.get("queue"));
        }
        "cohost_changed" => {
            let mut st = state_ref.write().await;
            if st.phase != JamPhase::Active {
                return;
            }
            let host_id = map
                .get("host_id")
                .map(|v| val_to_string(Some(v)))
                .unwrap_or_else(|| st.host_id.clone());
            let cohost_id = val_to_string(map.get("cohost_id"));
            apply_roles(&mut st, host_id, cohost_id);
        }
        "room_mode_changed" => {
            let mut st = state_ref.write().await;
            let rm = val_to_string(map.get("room_mode"));
            if !rm.is_empty() {
                st.room_mode = rm;
            }
        }
        "chat_message" => {
            let mut st = state_ref.write().await;
            st.chat.push(parse_chat_line(map));
            if st.chat.len() > 50 {
                let excess = st.chat.len() - 50;
                st.chat.drain(0..excess);
            }
        }
        "sync_playback" => {
            let mut st = state_ref.write().await;
            let track = map
                .get("track")
                .and_then(JamTrack::from_value)
                .or_else(|| st.track.clone());
            let playing = map.get("is_playing").and_then(Value::as_bool) == Some(true);
            st.is_playing = playing;
            st.track = track.clone();
            drop(st);

            let from = map.get("from_user_id").and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    Some(val_to_string(Some(v)))
                }
            });
            let position = val_to_i64(map.get("position_ms"));
            match val_to_string(map.get("action")).as_str() {
                "play" => {
                    let _ = remote_tx.send(JamRemote::Play {
                        position_ms: position,
                        server_time_ms: val_to_i64(map.get("server_time_ms")),
                        from_user_id: from,
                    });
                }
                "pause" => {
                    let _ = remote_tx.send(JamRemote::Pause {
                        position_ms: position,
                        from_user_id: from,
                    });
                }
                "seek" => {
                    let _ = remote_tx.send(JamRemote::Seek {
                        position_ms: position,
                        from_user_id: from,
                    });
                }
                "change_track" => {
                    if let Some(t) = track.filter(|t| !t.id.is_empty()) {
                        let _ = remote_tx.send(JamRemote::Track {
                            track: t,
                            is_playing: playing,
                            position_ms: position,
                            from_user_id: from,
                        });
                    }
                }
                _ => {}
            }
        }
        "error" => {
            let mut st = state_ref.write().await;
            let message = val_to_string(map.get("message"));
            if st.phase == JamPhase::Active {
                if val_to_string(map.get("code")) == "rate_limited" {
                    let retry_at = val_to_i64(map.get("retry_at_ms"));
                    let after = val_to_i64(map.get("retry_after_ms"));
                    let until = if retry_at > 0 {
                        retry_at
                    } else {
                        now_ms() + after
                    };
                    st.notice = message;
                    st.retry_at_ms = until;
                    drop(st);
                    start_cooldown(state_ref, until).await;
                } else if !message.is_empty() {
                    st.notice = message;
                    st.retry_at_ms = 0;
                }
                return;
            }
            st.phase = JamPhase::Error;
            st.error = if message.is_empty() {
                "Unknown server error".to_string()
            } else {
                message
            };
        }
        "pong" | _ => {}
    }
}

async fn start_cooldown(state_ref: &Arc<RwLock<JamSnapshot>>, until_ms: i64) {
    let wait = until_ms - now_ms();
    if wait <= 0 {
        return;
    }
    {
        let mut st = state_ref.write().await;
        st.cooldown_until_ms = until_ms;
    }
    let state_clone = Arc::clone(state_ref);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis((wait + 100) as u64)).await;
        let mut st = state_clone.write().await;
        if st.cooldown_until_ms == until_ms {
            st.cooldown_until_ms = 0;
        }
    });
}

fn apply_roles(st: &mut JamSnapshot, host_id: String, co_host_id: String) {
    st.host_id = host_id.clone();
    st.co_host_id = co_host_id.clone();
    st.is_host = st.user_id == host_id || st.user_id == co_host_id;
    for person in &mut st.people {
        person.is_host = person.user_id == host_id || person.user_id == co_host_id;
        person.is_co_host = !co_host_id.is_empty() && person.user_id == co_host_id;
    }
}

fn parse_people(raw: Option<&Value>) -> Vec<JamPerson> {
    let Some(arr) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let map = item.as_object()?;
            Some(JamPerson {
                user_id: val_to_string(map.get("user_id")),
                name: val_to_string(map.get("name")),
                is_host: map.get("is_host").and_then(Value::as_bool) == Some(true),
                is_co_host: map.get("is_cohost").and_then(Value::as_bool) == Some(true),
                is_web: map.get("is_web").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect()
}

fn parse_tracks(raw: Option<&Value>) -> Vec<JamTrack> {
    let Some(arr) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(JamTrack::from_value).collect()
}

fn parse_chat(raw: Option<&Value>) -> Vec<JamChatLine> {
    let Some(arr) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| item.as_object().map(parse_chat_line))
        .collect()
}

fn parse_chat_line(map: &serde_json::Map<String, Value>) -> JamChatLine {
    let reply_to = map.get("reply_to").and_then(Value::as_object).map(|r| JamReply {
        id: val_to_string(r.get("id")),
        user_name: val_to_string(r.get("user_name")),
        text: val_to_string(r.get("text")),
    });
    JamChatLine {
        id: val_to_string(map.get("id")),
        user_id: val_to_string(map.get("user_id")),
        user_name: val_to_string(map.get("user_name")),
        is_host: map.get("is_host").and_then(Value::as_bool) == Some(true),
        text: val_to_string(map.get("text")),
        timestamp: val_to_i64(map.get("timestamp")),
        reply_to,
    }
}

fn val_to_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn val_to_i64(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_code_extracts_bare_and_url() {
        assert_eq!(JamController::parse_code("  abc123 "), "ABC123");
        assert_eq!(
            JamController::parse_code("https://jam.clashgram.org/?code=xyz789&foo=1"),
            "XYZ789"
        );
        assert_eq!(JamController::parse_code("india"), "INDIA");
    }

    #[tokio::test]
    async fn handle_room_created_and_chat() {
        let state = Arc::new(RwLock::new(JamSnapshot::default()));
        let (remote_tx, _) = broadcast::channel(8);
        let (rtc_tx, _) = broadcast::channel(8);
        let pending = Arc::new(Mutex::new(false));
        let (tx, _rx) = mpsc::unbounded_channel();

        let created = json!({
            "type": "room_created",
            "payload": {
                "user_id": "u1",
                "name": "Alice",
                "room_code": "ROOM1",
                "room_mode": "collaborative"
            }
        })
        .to_string();
        handle_message(&created, &state, &remote_tx, &rtc_tx, &pending, &tx).await;
        {
            let st = state.read().await;
            assert_eq!(st.phase, JamPhase::Active);
            assert_eq!(st.room_code, "ROOM1");
            assert!(st.is_main_host());
        }

        let chat = json!({
            "type": "chat_message",
            "payload": {
                "id": "m1",
                "user_id": "u1",
                "user_name": "Alice",
                "is_host": true,
                "text": "hello",
                "timestamp": 1000
            }
        })
        .to_string();
        handle_message(&chat, &state, &remote_tx, &rtc_tx, &pending, &tx).await;
        let st = state.read().await;
        assert_eq!(st.chat.len(), 1);
        assert_eq!(st.chat[0].text, "hello");
    }
}
