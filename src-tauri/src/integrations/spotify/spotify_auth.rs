//! Spotify session: the `sp_dc` cookie plus the tokens minted from it.
//!
//! Port of `lib/features/spotify/spotify_auth.dart`. Both sign-in routes
//! (system WebView and pasted cookie) end in `connect_with_sp_dc`; storing a
//! cookie is NOT the same as having a session, so every connect exchanges
//! it for a token before reporting success.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::Sha1;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::storage::diagnostics_log::DiagnosticsLog;
use crate::storage::prefs::Prefs;
use crate::storage::secure_store::SecureStore;

use super::spotify_web_player::{SpotifySecret, SpotifyWebPlayer, USER_AGENT};

type HmacSha1 = Hmac<Sha1>;

/// Outcome of exchanging the stored `sp_dc` cookie for a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpotifySessionCheck {
    /// The cookie is a signed-in session.
    Ok,
    /// No cookie stored.
    Missing,
    /// Spotify answered, but with a guest session or a refusal: the cookie
    /// is expired / from an ended session and must be replaced.
    Refused,
    /// The exchange could not be attempted (offline, player unreadable).
    Unreachable,
}

const ORIGIN: &str = "https://open.spotify.com";
const APP_VERSION: &str = "1.2.62.476.g2ad6e7f3";
const CLIENT_ID: &str = "d8a5ed958d274c2e8ee717e6a4b0971d";
const CLIENT_TOKEN_URL: &str = "https://clienttoken.spotify.com/v1/clienttoken";
const SERVER_TIME_URL: &str = "https://open.spotify.com/api/server-time";
const SESSION_TOKEN_URL: &str = "https://open.spotify.com/api/token";

const SECRETS_DICT_URLS: &[&str] = &[
    "https://raw.githubusercontent.com/xyloflake/spot-secrets-go/refs/heads/main/secrets/secretDict.json",
    "https://cdn.jsdelivr.net/gh/xyloflake/spot-secrets-go@main/secrets/secretDict.json",
];

fn fallback_secrets() -> Vec<SpotifySecret> {
    let from_codes = |codes: &[u8], version: i32| -> SpotifySecret {
        let s: String = codes.iter().map(|&c| c as char).collect();
        SpotifySecret::new(s, version)
    };
    vec![
        from_codes(
            &[
                44, 55, 47, 42, 70, 40, 34, 114, 76, 74, 50, 111, 120, 97, 75, 76, 94, 102, 43,
                69, 49, 120, 118, 80, 64, 78,
            ],
            61,
        ),
        from_codes(
            &[
                79, 109, 69, 123, 90, 65, 46, 74, 94, 34, 58, 48, 70, 71, 92, 85, 122, 63, 91, 64,
                87, 87,
            ],
            60,
        ),
        from_codes(
            &[
                123, 105, 79, 70, 110, 59, 52, 125, 60, 49, 80, 70, 89, 75, 80, 86, 63, 53, 123,
                37, 117, 49, 52, 93, 77, 62, 47, 86, 48, 104, 68, 72,
            ],
            59,
        ),
    ]
}

const MAX_SECRET_ATTEMPTS: usize = 4;
const CLIENT_TOKEN_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";
const MIN_PERSONAL_TOKEN_LENGTH: usize = 100;
const GRACE_MS: i64 = 60 * 1000;

static SPDC_KV_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"sp_dc\s*=\s*([^;\s]+)").unwrap());
static SPDC_ROW_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)sp_dc\s+(\S+)").unwrap());
static INVISIBLE_WS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\s\u{200B}-\u{200D}\u{FEFF}]").unwrap());

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
struct TotpPair {
    otp_client: String,
    otp_server: String,
    version: i32,
}

#[derive(Default)]
struct AuthState {
    spdc: Option<String>,
    spdc_loaded: bool,
    personal: Option<String>,
    personal_exp: i64,
    client: Option<String>,
    client_exp: i64,
    connected: bool,
}

pub struct SpotifyAuth {
    store: Arc<SecureStore>,
    prefs: Arc<Prefs>,
    http: reqwest::Client,
    state: Mutex<AuthState>,
    personal_lock: Mutex<()>,
    client_lock: Mutex<()>,
}

impl SpotifyAuth {
    pub fn new(store: Arc<SecureStore>, prefs: Arc<Prefs>, http: reqwest::Client) -> Self {
        Self {
            store,
            prefs,
            http,
            state: Mutex::new(AuthState::default()),
            personal_lock: Mutex::new(()),
            client_lock: Mutex::new(()),
        }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Whether a cookie is stored (and was not found to be dead).
    pub async fn connected(&self) -> bool {
        self.state.lock().await.connected
    }

    /// Loads the stored cookie. Call once at startup.
    pub async fn load(&self) {
        let spdc = self.store.read_spotify_sp_dc();
        let mut guard = self.state.lock().await;
        guard.connected = spdc.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
        guard.spdc = spdc;
        guard.spdc_loaded = true;
    }

    pub async fn spdc(&self) -> Option<String> {
        let loaded = self.state.lock().await.spdc_loaded;
        if !loaded {
            self.load().await;
        }
        let guard = self.state.lock().await;
        guard.spdc.clone().filter(|s| !s.is_empty())
    }

    /// Accepts `sp_dc=…; …`, a whole Cookie header, the bare value, or a row
    /// copied from the browser's cookie table (`sp_dc`, TAB, `AQ…`, TAB, domain …).
    pub fn clean_sp_dc(raw: &str) -> String {
        let mut v = raw.trim().to_string();
        if let Some(caps) = SPDC_KV_REGEX.captures(&v) {
            if let Some(m) = caps.get(1) {
                v = m.as_str().to_string();
            }
        } else if let Some(caps) = SPDC_ROW_REGEX.captures(&v) {
            if let Some(m) = caps.get(1) {
                v = m.as_str().to_string();
            }
        }
        v = v.replace(['"', '\''], "").trim().to_string();
        v = INVISIBLE_WS_REGEX.replace_all(&v, "").to_string();
        if v.contains('%') {
            if let Ok(decoded) = urlencoding_decode(&v) {
                v = decoded;
            }
        }
        v
    }

    /// Stores `raw` and verifies it can be exchanged for a real session.
    /// A cookie Spotify refuses is dropped again, so a dead session never
    /// looks like a connected one.
    pub async fn connect_with_sp_dc(&self, raw: &str) -> SpotifySessionCheck {
        let clean = Self::clean_sp_dc(raw);
        if clean.is_empty() {
            return SpotifySessionCheck::Missing;
        }
        let previous = {
            let mut guard = self.state.lock().await;
            let prev = guard.spdc.clone();
            guard.spdc = Some(clean.clone());
            guard.spdc_loaded = true;
            guard.personal = None;
            guard.personal_exp = 0;
            prev
        };

        let result = self.verify().await;
        if result == SpotifySessionCheck::Ok {
            self.store.write_spotify_sp_dc(Some(&clean));
            let mut guard = self.state.lock().await;
            guard.connected = true;
        } else {
            let mut guard = self.state.lock().await;
            guard.connected = previous.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
            guard.spdc = previous;
            guard.personal = None;
            guard.personal_exp = 0;
        }
        result
    }

    pub async fn disconnect(&self) {
        {
            let mut guard = self.state.lock().await;
            guard.spdc = None;
            guard.personal = None;
            guard.personal_exp = 0;
            guard.client = None;
            guard.client_exp = 0;
            guard.connected = false;
        }
        self.store.write_spotify_sp_dc(None);
    }

    /// Exchanges the cookie for a token; reports what Spotify said.
    pub async fn verify(&self) -> SpotifySessionCheck {
        let Some(cookie) = self.spdc().await else {
            return SpotifySessionCheck::Missing;
        };

        let server_seconds = self.server_seconds(&cookie).await;
        let mut saw_answer = false;
        let mut tried = 0usize;
        let mut seen = HashSet::new();

        let mut try_secret = |secret: &SpotifySecret| -> bool {
            seen.insert(format!("{}:{}", secret.version, secret.value))
        };

        let mut candidates: Vec<SpotifySecret> = Vec::new();
        if let Some(rem) = Self::decode_stored_secret(&self.prefs.spotify_otp_secret()) {
            if try_secret(&rem) {
                candidates.push(rem);
            }
        }
        for s in SpotifyWebPlayer::player_secrets(&self.http).await {
            if try_secret(&s) {
                candidates.push(s);
            }
        }
        if candidates.len() < MAX_SECRET_ATTEMPTS {
            for url in SECRETS_DICT_URLS {
                for s in self.read_secret_dict(url).await {
                    if try_secret(&s) {
                        candidates.push(s);
                    }
                }
                if candidates.len() >= MAX_SECRET_ATTEMPTS {
                    break;
                }
            }
        }
        if candidates.len() < MAX_SECRET_ATTEMPTS {
            for s in fallback_secrets() {
                if try_secret(&s) {
                    candidates.push(s);
                }
            }
        }

        for secret in candidates {
            if tried >= MAX_SECRET_ATTEMPTS {
                break;
            }
            tried += 1;
            let totp = Self::totp_for(&secret, server_seconds);
            for reason in ["transport", "init"] {
                let Some(res) = self.request_personal(&cookie, &totp, reason).await else {
                    continue;
                };
                saw_answer = true;
                let anon = res.get("isAnonymous").and_then(Value::as_bool);
                let token_len = res
                    .get("accessToken")
                    .and_then(Value::as_str)
                    .map(str::len)
                    .unwrap_or(0);
                DiagnosticsLog::info(&format!(
                    "spotify auth reason={reason} totpVer={} anonymous={anon:?} tokenLen={token_len}",
                    secret.version
                ));
                if Self::usable(&res) {
                    self.store_token(&res).await;
                    self.remember_secret(&secret);
                    return SpotifySessionCheck::Ok;
                }
            }
        }

        if tried == 0 {
            DiagnosticsLog::info("spotify auth stage=otp-secret ok=false");
        }
        if saw_answer {
            SpotifySessionCheck::Refused
        } else {
            SpotifySessionCheck::Unreachable
        }
    }

    async fn store_token(&self, res: &serde_json::Map<String, Value>) {
        let token = res
            .get("accessToken")
            .and_then(Value::as_str)
            .map(str::to_string);
        let exp = res
            .get("accessTokenExpirationTimestampMs")
            .and_then(Value::as_i64)
            .unwrap_or_else(|| now_ms() + 3600 * 1000);
        let mut guard = self.state.lock().await;
        guard.personal = token;
        guard.personal_exp = exp;
    }

    fn usable(res: &serde_json::Map<String, Value>) -> bool {
        let Some(token) = res.get("accessToken").and_then(Value::as_str) else {
            return false;
        };
        let is_anon = res.get("isAnonymous").and_then(Value::as_bool) == Some(true);
        !is_anon && token.len() >= MIN_PERSONAL_TOKEN_LENGTH
    }

    /// Active web-player access token for the signed-in account, or `None`.
    pub async fn personal_token(&self) -> Option<String> {
        {
            let guard = self.state.lock().await;
            if let Some(ref p) = guard.personal {
                if !p.is_empty() && now_ms() < guard.personal_exp - GRACE_MS {
                    return Some(p.clone());
                }
            }
        }
        let _inflight = self.personal_lock.lock().await;
        {
            let guard = self.state.lock().await;
            if let Some(ref p) = guard.personal {
                if !p.is_empty() && now_ms() < guard.personal_exp - GRACE_MS {
                    return Some(p.clone());
                }
            }
        }
        self.refresh_personal().await
    }

    async fn refresh_personal(&self) -> Option<String> {
        self.spdc().await?;
        let result = self.verify().await;
        if result == SpotifySessionCheck::Refused {
            let mut guard = self.state.lock().await;
            guard.connected = false;
            return None;
        }
        if result == SpotifySessionCheck::Ok {
            self.state.lock().await.personal.clone()
        } else {
            None
        }
    }

    /// Client token (identifies the app), cached until near expiry.
    pub async fn client_token(&self) -> Option<String> {
        {
            let guard = self.state.lock().await;
            if let Some(ref c) = guard.client {
                if !c.is_empty() && now_ms() < guard.client_exp - GRACE_MS {
                    return Some(c.clone());
                }
            }
        }
        let _inflight = self.client_lock.lock().await;
        {
            let guard = self.state.lock().await;
            if let Some(ref c) = guard.client {
                if !c.is_empty() && now_ms() < guard.client_exp - GRACE_MS {
                    return Some(c.clone());
                }
            }
        }
        self.refresh_client().await
    }

    /// Per-install anonymous device id (also the web player's `sp_t`).
    pub fn device_id(&self) -> String {
        let existing = self.prefs.spotify_device_id();
        if !existing.is_empty() {
            return existing;
        }
        let id = Uuid::new_v4().to_string();
        self.prefs.set_spotify_device_id(&id);
        id
    }

    async fn refresh_client(&self) -> Option<String> {
        let device_id = self.device_id();
        let payload = json!({
            "client_data": {
                "client_version": APP_VERSION,
                "client_id": CLIENT_ID,
                "js_sdk_data": {
                    "device_brand": "Apple",
                    "device_model": "unknown",
                    "os": "macos",
                    "os_version": "10.15.7",
                    "device_id": device_id,
                    "device_type": "computer",
                }
            }
        });

        if let Ok(res) = self
            .http
            .post(CLIENT_TOKEN_URL)
            .header("User-Agent", CLIENT_TOKEN_UA)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(payload.to_string())
            .send()
            .await
        {
            if res.status().is_success() {
                if let Ok(text) = res.text().await {
                    if let Ok(Value::Object(body)) = serde_json::from_str::<Value>(&text) {
                        if let Some(granted) = body.get("granted_token").and_then(Value::as_object) {
                            if let Some(token) = granted
                                .get("token")
                                .and_then(Value::as_str)
                                .filter(|s| !s.is_empty())
                            {
                                let ttl = granted
                                    .get("expires_after_seconds")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(7200);
                                let mut guard = self.state.lock().await;
                                guard.client = Some(token.to_string());
                                guard.client_exp = now_ms() + (ttl * 1000);
                                return Some(token.to_string());
                            }
                        }
                    }
                }
            }
        }
        self.state.lock().await.client.clone()
    }

    // -- one-time password --------------------------------------------------

    async fn server_seconds(&self, cookie: &str) -> i64 {
        let headers = Self::web_headers(cookie);
        let mut req = self.http.get(SERVER_TIME_URL);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }

        match req.send().await {
            Ok(res) => {
                let date_hdr = res
                    .headers()
                    .get("date")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                if res.status().is_success() {
                    if let Ok(text) = res.text().await {
                        if let Ok(Value::Object(body)) = serde_json::from_str::<Value>(&text) {
                            if let Some(t) = body.get("serverTime").and_then(Value::as_i64) {
                                return t;
                            }
                        }
                    }
                }
                if let Some(date) = date_hdr {
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(&date) {
                        return dt.timestamp();
                    }
                }
            }
            Err(_) => {
                if let Some(fallback) =
                    SpotifyWebPlayer::curl_get(SERVER_TIME_URL, &headers, 15).await
                {
                    if (200..300).contains(&fallback.status) {
                        if let Ok(Value::Object(body)) =
                            serde_json::from_str::<Value>(&fallback.body)
                        {
                            if let Some(t) = body.get("serverTime").and_then(Value::as_i64) {
                                return t;
                            }
                        }
                    }
                }
            }
        }
        now_ms() / 1000
    }

    fn totp_for(secret: &SpotifySecret, server_seconds: i64) -> TotpPair {
        // The secret's characters, each xor'd with ((index % 33) + 9), are
        // written out as decimal text; that text is the HMAC key.
        let mut key_text = String::new();
        for (i, b) in secret.value.bytes().enumerate() {
            let xored = b ^ (((i % 33) + 9) as u8);
            key_text.push_str(&xored.to_string());
        }
        let key = key_text.as_bytes();
        let client_seconds = now_ms() / 1000;
        TotpPair {
            otp_client: Self::generate_otp(key, client_seconds),
            otp_server: Self::generate_otp(key, server_seconds),
            version: secret.version,
        }
    }

    /// RFC 4226/6238 HMAC-SHA1 one-time password, 6 digits, 30 s step.
    pub fn generate_otp(key: &[u8], timestamp_seconds: i64) -> String {
        let counter = (timestamp_seconds / 30) as u64;
        let bytes = counter.to_be_bytes();
        let mut mac = HmacSha1::new_from_slice(key).expect("HMAC can take key of any size");
        mac.update(&bytes);
        let result = mac.finalize().into_bytes();
        let offset = (result[result.len() - 1] & 0x0f) as usize;
        let binary = (((result[offset] & 0x7f) as u32) << 24)
            | ((result[offset + 1] as u32) << 16)
            | ((result[offset + 2] as u32) << 8)
            | (result[offset + 3] as u32);
        format!("{:06}", binary % 1_000_000)
    }

    async fn read_secret_dict(&self, url: &str) -> Vec<SpotifySecret> {
        if let Ok(res) = self
            .http
            .get(url)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
        {
            if res.status().is_success() {
                if let Ok(text) = res.text().await {
                    return Self::parse_secret_dict(&text);
                }
            }
        }
        let empty = HashMap::new();
        if let Some(fallback) = SpotifyWebPlayer::curl_get(url, &empty, 15).await {
            if (200..300).contains(&fallback.status) {
                return Self::parse_secret_dict(&fallback.body);
            }
        }
        let host = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default();
        DiagnosticsLog::info(&format!("spotify auth secretDict unreachable host={host}"));
        Vec::new()
    }

    /// `{"<version>": [char codes], …}` -> secrets, newest first.
    pub fn parse_secret_dict(body: &str) -> Vec<SpotifySecret> {
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(body) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (k, v) in map {
            let Ok(version) = k.parse::<i32>() else {
                continue;
            };
            let Some(arr) = v.as_array() else {
                continue;
            };
            if arr.is_empty() {
                continue;
            }
            let s: String = arr
                .iter()
                .filter_map(|n| n.as_u64().and_then(|c| char::from_u32(c as u32)))
                .collect();
            out.push(SpotifySecret::new(s, version));
        }
        out.sort_by(|a, b| b.version.cmp(&a.version));
        out
    }

    pub fn decode_stored_secret(raw: &str) -> Option<SpotifySecret> {
        if raw.is_empty() {
            return None;
        }
        let Value::Object(map) = serde_json::from_str::<Value>(raw).ok()? else {
            return None;
        };
        let v = map.get("v")?.as_i64()? as i32;
        let c = map.get("c")?.as_array()?;
        if c.is_empty() {
            return None;
        }
        let s: String = c
            .iter()
            .filter_map(|n| n.as_u64().and_then(|code| char::from_u32(code as u32)))
            .collect();
        Some(SpotifySecret::new(s, v))
    }

    fn remember_secret(&self, s: &SpotifySecret) {
        let codes: Vec<u32> = s.value.chars().map(|c| c as u32).collect();
        let encoded = json!({ "v": s.version, "c": codes }).to_string();
        self.prefs.set_spotify_otp_secret(&encoded);
    }

    fn web_headers(cookie: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("User-Agent".to_string(), USER_AGENT.to_string());
        map.insert("Accept".to_string(), "application/json".to_string());
        map.insert("Cookie".to_string(), format!("sp_dc={cookie}"));
        map.insert("App-platform".to_string(), "WebPlayer".to_string());
        map.insert("Spotify-App-Version".to_string(), APP_VERSION.to_string());
        map.insert("Origin".to_string(), ORIGIN.to_string());
        map.insert("Referer".to_string(), format!("{ORIGIN}/"));
        map
    }

    async fn request_personal(
        &self,
        cookie: &str,
        totp: &TotpPair,
        reason: &str,
    ) -> Option<serde_json::Map<String, Value>> {
        let url = format!(
            "{SESSION_TOKEN_URL}?reason={reason}&productType=web-player\
            &totp={}&totpServer={}&totpVer={}",
            totp.otp_client, totp.otp_server, totp.version
        );
        let headers = Self::web_headers(cookie);
        let mut req = self.http.get(&url);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }

        match req.send().await {
            Ok(res) => {
                if !res.status().is_success() {
                    DiagnosticsLog::info(&format!(
                        "spotify auth reason={reason} http={}",
                        res.status().as_u16()
                    ));
                    return None;
                }
                let text = res.text().await.unwrap_or_default();
                return match serde_json::from_str::<Value>(&text) {
                    Ok(Value::Object(m)) => Some(m),
                    _ => None,
                };
            }
            Err(e) => {
                DiagnosticsLog::info(&format!("spotify auth reason={reason} err={e}"));
            }
        }

        let fallback = SpotifyWebPlayer::curl_get(&url, &headers, 15).await?;
        if !(200..300).contains(&fallback.status) {
            DiagnosticsLog::info(&format!(
                "spotify auth reason={reason} via=curl http={}",
                fallback.status
            ));
            return None;
        }
        match serde_json::from_str::<Value>(&fallback.body) {
            Ok(Value::Object(m)) => Some(m),
            Err(e) => {
                DiagnosticsLog::info(&format!(
                    "spotify auth reason={reason} via=curl err={e}"
                ));
                None
            }
            _ => None,
        }
    }
}

fn urlencoding_decode(input: &str) -> Result<String, ()> {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex_str = std::str::from_utf8(&bytes[i + 1..i + 3]).map_err(|_| ())?;
            let val = u8::from_str_radix(hex_str, 16).map_err(|_| ())?;
            out.push(val);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_sp_dc_handles_all_formats() {
        assert_eq!(SpotifyAuth::clean_sp_dc("  AQB123  "), "AQB123");
        assert_eq!(
            SpotifyAuth::clean_sp_dc("sp_t=abc; sp_dc=AQB_xyz; Path=/"),
            "AQB_xyz"
        );
        assert_eq!(
            SpotifyAuth::clean_sp_dc("sp_dc\tAQB_tab\t.spotify.com"),
            "AQB_tab"
        );
        assert_eq!(
            SpotifyAuth::clean_sp_dc("\"AQB%2B123\u{200B}\""),
            "AQB+123"
        );
    }

    #[test]
    fn generate_otp_rfc6238_vector() {
        // RFC 6238 SHA1 test vector: key "12345678901234567890" at t=59 -> 94287082 -> 287082
        let key = b"12345678901234567890";
        assert_eq!(SpotifyAuth::generate_otp(key, 59), "287082");
    }

    #[test]
    fn parse_and_decode_secrets() {
        let json = r#"{"59":[65,66],"61":[67,68]}"#;
        let parsed = SpotifyAuth::parse_secret_dict(json);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], SpotifySecret::new("CD", 61));
        assert_eq!(parsed[1], SpotifySecret::new("AB", 59));

        let stored = r#"{"v":61,"c":[67,68]}"#;
        assert_eq!(
            SpotifyAuth::decode_stored_secret(stored),
            Some(SpotifySecret::new("CD", 61))
        );
    }
}
