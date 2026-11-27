//! What Spotify's web player is built on, read out of its own script.
//!
//! Port of `lib/features/spotify/spotify_web_player.dart`. Two things live in
//! the player's bundle and both move whenever Spotify rebuilds it, so a copy
//! of them goes stale and every request made with the copy starts being
//! refused (which looks exactly like "signed out" or "no canvas"):
//!
//! 1. the **persisted-query hashes** — every GraphQL operation is named
//!    by a hash that changes when the operation is rebuilt;
//! 2. the **one-time-password secret** (+ version) the token exchange is
//!    signed with.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::storage::diagnostics_log::DiagnosticsLog;

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36";

const PLAYER_PAGE: &str = "https://open.spotify.com/";
const CACHE_TTL: Duration = Duration::from_secs(6 * 3600);
const MAX_SCRIPTS: usize = 4;
const MAX_HASH_SCRIPTS: usize = 24;
const MAX_SECRET_SCRIPTS: usize = 24;

/// The operation that asks for a track's canvas.
pub const CANVAS_OPERATION: &str = "canvas";
/// The operation that searches for tracks (was `searchTracks`).
pub const SEARCH_OPERATION: &str = "assistedCurationSearch";
pub const LIBRARY_OPERATION: &str = "libraryV3";
/// The web player's Home feed query.
pub const HOME_OPERATION: &str = "home";
pub const FETCH_PLAYLIST_OPERATION: &str = "fetchPlaylist";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifySecret {
    pub value: String,
    pub version: i32,
}

impl SpotifySecret {
    pub fn new(value: impl Into<String>, version: i32) -> Self {
        Self {
            value: value.into(),
            version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurlResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Default)]
struct PlayerCache {
    script: Option<String>,
    read_at: Option<Instant>,
    hashes: HashMap<String, String>,
    secrets: Option<Vec<SpotifySecret>>,
    secrets_at: Option<Instant>,
    last_page_unreadable: bool,
}

#[derive(Default)]
struct ProxyCache {
    checked: bool,
    win_proxy: Option<String>,
}

static CACHE: LazyLock<Mutex<PlayerCache>> = LazyLock::new(|| Mutex::new(PlayerCache::default()));
static PROXY_CACHE: LazyLock<StdMutex<ProxyCache>> =
    LazyLock::new(|| StdMutex::new(ProxyCache::default()));

static BUNDLE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"https://open\.spotifycdn\.com/cdn/build/web-player/[A-Za-z0-9._~-]+\.(?:js|mjs)",
    )
    .unwrap()
});

static OPERATION_NAMED_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"name:\s*["'](\w+)["']\s*,\s*operation:\s*["']query["']\s*,\s*sha256Hash:\s*["']([0-9a-f]{64})["']"#,
    )
    .unwrap()
});

static OPERATION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"["'](\w+)["']\s*,\s*["']query["']\s*,\s*["']([0-9a-f]{64})["']"#).unwrap()
});

static SECRET_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"secret:\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")\s*,\s*version:\s*(\d+)"#)
        .unwrap()
});

pub struct SpotifyWebPlayer;

impl SpotifyWebPlayer {
    /// `operation`'s hash from whichever player bundle declares it. `script`
    /// only keeps the first matching bundle, and route chunks such as Home
    /// declare their queries elsewhere. Cached for the app session.
    pub async fn find_hash(client: &reqwest::Client, operation: &str) -> Option<String> {
        {
            let guard = CACHE.lock().await;
            if let Some(cached) = guard.hashes.get(operation) {
                return Some(cached.clone());
            }
        }

        let main = Self::script(client, false).await;
        let mut hash = main
            .as_deref()
            .and_then(|s| Self::query_hash(s, operation));

        if hash.is_none() {
            if let Some(page) = Self::get_text(client, PLAYER_PAGE).await {
                for bundle in Self::scripts(&page).into_iter().take(MAX_HASH_SCRIPTS) {
                    let Some(body) = Self::get_text(client, &bundle).await else {
                        continue;
                    };
                    if let Some(found) = Self::query_hash(&body, operation) {
                        hash = Some(found);
                        break;
                    }
                }
            }
        }

        DiagnosticsLog::info(&format!(
            "spotify player op={operation} hash={}",
            hash.is_some()
        ));
        if let Some(ref h) = hash {
            let mut guard = CACHE.lock().await;
            guard.hashes.insert(operation.to_string(), h.clone());
        }
        hash
    }

    /// The one-time-password secrets the player carries, newest first.
    pub async fn player_secrets(client: &reqwest::Client) -> Vec<SpotifySecret> {
        {
            let guard = CACHE.lock().await;
            if let (Some(cached), Some(at)) = (&guard.secrets, guard.secrets_at) {
                if !cached.is_empty() && at.elapsed() < CACHE_TTL {
                    return cached.clone();
                }
            }
        }
        Self::read_secrets(client).await
    }

    async fn read_secrets(client: &reqwest::Client) -> Vec<SpotifySecret> {
        let main = Self::script(client, false).await;
        if let Some(ref m) = main {
            let found = Self::secrets(m);
            if !found.is_empty() {
                return Self::keep_secrets(found).await;
            }
        } else {
            let guard = CACHE.lock().await;
            if guard.last_page_unreadable {
                return Vec::new();
            }
        }

        let Some(page) = Self::get_text(client, PLAYER_PAGE).await else {
            return Vec::new();
        };
        for bundle in Self::scripts(&page).into_iter().take(MAX_SECRET_SCRIPTS) {
            let Some(body) = Self::get_text(client, &bundle).await else {
                continue;
            };
            let found = Self::secrets(&body);
            if !found.is_empty() {
                return Self::keep_secrets(found).await;
            }
        }
        DiagnosticsLog::info("spotify player secrets=none");
        Vec::new()
    }

    async fn keep_secrets(found: Vec<SpotifySecret>) -> Vec<SpotifySecret> {
        let mut guard = CACHE.lock().await;
        guard.secrets = Some(found.clone());
        guard.secrets_at = Some(Instant::now());
        found
    }

    /// The player's script, or `None` when it could not be read. Cached for a
    /// few hours: the player changes rarely and the bundle is megabytes.
    pub async fn script(client: &reqwest::Client, force: bool) -> Option<String> {
        if !force {
            let guard = CACHE.lock().await;
            if let (Some(ref s), Some(at)) = (&guard.script, guard.read_at) {
                if at.elapsed() < CACHE_TTL {
                    return Some(s.clone());
                }
            }
        }
        Self::read_script(client).await
    }

    async fn read_script(client: &reqwest::Client) -> Option<String> {
        let Some(page) = Self::get_text(client, PLAYER_PAGE).await else {
            let mut guard = CACHE.lock().await;
            guard.last_page_unreadable = true;
            DiagnosticsLog::info("spotify player page=unreadable");
            return None;
        };
        {
            let mut guard = CACHE.lock().await;
            guard.last_page_unreadable = false;
        }
        let bundles = Self::scripts(&page);
        DiagnosticsLog::info(&format!("spotify player bundles={}", bundles.len()));

        for bundle in bundles.into_iter().take(MAX_SCRIPTS) {
            let Some(body) = Self::get_text(client, &bundle).await else {
                continue;
            };
            let has_canvas = Self::query_hash(&body, CANVAS_OPERATION).is_some();
            let secrets_len = Self::secrets(&body).len();
            if has_canvas || secrets_len > 0 {
                let mut guard = CACHE.lock().await;
                guard.script = Some(body.clone());
                guard.read_at = Some(Instant::now());
                DiagnosticsLog::info(&format!(
                    "spotify player script ok secrets={secrets_len} canvasHash={has_canvas}"
                ));
                return Some(body);
            }
        }
        DiagnosticsLog::info("spotify player script=no-hashes-or-secrets");
        None
    }

    async fn get_text(client: &reqwest::Client, url: &str) -> Option<String> {
        let is_page = url == PLAYER_PAGE;
        let accept = if is_page {
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
        } else {
            "*/*"
        };
        let mut headers = HashMap::new();
        headers.insert("User-Agent".to_string(), USER_AGENT.to_string());
        headers.insert("Referer".to_string(), PLAYER_PAGE.to_string());
        headers.insert("Accept".to_string(), accept.to_string());

        let req = client
            .get(url)
            .header("User-Agent", USER_AGENT)
            .header("Referer", PLAYER_PAGE)
            .header("Accept", accept)
            .timeout(Duration::from_secs(40));

        match req.send().await {
            Ok(res) if res.status().is_success() => {
                if let Ok(text) = res.text().await {
                    return Some(text);
                }
            }
            Ok(res) => {
                let host = url::Url::parse(url)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_string))
                    .unwrap_or_default();
                DiagnosticsLog::info(&format!(
                    "spotify player get failed host={host} http={}",
                    res.status().as_u16()
                ));
            }
            Err(e) => {
                let host = url::Url::parse(url)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_string))
                    .unwrap_or_default();
                DiagnosticsLog::info(&format!(
                    "spotify player get failed host={host} err={e}"
                ));
            }
        }

        let fallback = Self::curl_get(url, &headers, 30).await;
        if let Some(fb) = fallback {
            if (200..300).contains(&fb.status) {
                return Some(fb.body);
            }
        }
        None
    }

    /// Reads the Windows WinINET system proxy once, or `None` when disabled.
    async fn windows_system_proxy() -> Option<String> {
        if !cfg!(windows) {
            return None;
        }
        {
            let guard = PROXY_CACHE.lock().ok()?;
            if guard.checked {
                return guard.win_proxy.clone();
            }
        }

        let mut detected: Option<String> = None;
        let enabled_out = tokio::time::timeout(
            Duration::from_secs(3),
            Command::new("reg")
                .args([
                    "query",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                    "/v",
                    "ProxyEnable",
                ])
                .output(),
        )
        .await;

        if let Ok(Ok(enabled)) = enabled_out {
            let stdout = String::from_utf8_lossy(&enabled.stdout);
            let enable_re = Regex::new(r"ProxyEnable\s+REG_DWORD\s+0x1\b").unwrap();
            if enabled.status.success() && enable_re.is_match(&stdout) {
                let server_out = tokio::time::timeout(
                    Duration::from_secs(3),
                    Command::new("reg")
                        .args([
                            "query",
                            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                            "/v",
                            "ProxyServer",
                        ])
                        .output(),
                )
                .await;
                if let Ok(Ok(server)) = server_out {
                    if server.status.success() {
                        let s_out = String::from_utf8_lossy(&server.stdout);
                        let srv_re = Regex::new(r"ProxyServer\s+REG_SZ\s+(.+)").unwrap();
                        if let Some(caps) = srv_re.captures(&s_out) {
                            let raw = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                            if !raw.is_empty() {
                                let mut picked = raw.to_string();
                                if raw.contains('=') {
                                    let mut parts = HashMap::new();
                                    for entry in raw.split(';') {
                                        if let Some(eq) = entry.find('=') {
                                            parts.insert(
                                                entry[..eq].trim().to_lowercase(),
                                                entry[eq + 1..].trim().to_string(),
                                            );
                                        }
                                    }
                                    picked = parts
                                        .remove("https")
                                        .or_else(|| parts.remove("http"))
                                        .or_else(|| parts.remove("socks"))
                                        .unwrap_or_default();
                                }
                                if !picked.is_empty() {
                                    detected = Some(if picked.contains("://") {
                                        picked
                                    } else {
                                        format!("http://{picked}")
                                    });
                                    DiagnosticsLog::info("spotify net win-proxy=detected");
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Ok(mut guard) = PROXY_CACHE.lock() {
            guard.checked = true;
            guard.win_proxy = detected.clone();
        }
        detected
    }

    /// Fallback HTTP GET via Windows' built-in `curl.exe` (Schannel TLS,
    /// Windows certificate store, IPv4 fallback, and Windows system proxy).
    /// URL and headers are passed via stdin (`-K -`) so cookies/tokens never
    /// appear in process arguments.
    pub async fn curl_get(
        url: &str,
        headers: &HashMap<String, String>,
        timeout_seconds: u64,
    ) -> Option<CurlResponse> {
        if !cfg!(windows) {
            return None;
        }
        let proxy = Self::windows_system_proxy().await;
        for ipv4 in [false, true] {
            if let Some(res) = Self::run_curl(
                url,
                headers,
                timeout_seconds,
                proxy.as_deref(),
                ipv4,
            )
            .await
            {
                return Some(res);
            }
        }
        None
    }

    fn curl_escape(v: &str) -> String {
        v.replace('\\', "\\\\").replace('"', "\\\"")
    }

    async fn run_curl(
        url: &str,
        headers: &HashMap<String, String>,
        timeout_seconds: u64,
        proxy: Option<&str>,
        ipv4: bool,
    ) -> Option<CurlResponse> {
        let mut args = vec![
            "-sS".to_string(),
            "-L".to_string(),
            "--max-time".to_string(),
            timeout_seconds.to_string(),
        ];
        if ipv4 {
            args.push("-4".to_string());
        }
        if let Some(p) = proxy {
            args.push("--proxy".to_string());
            args.push(p.to_string());
        }
        args.extend([
            "-w".to_string(),
            "\n%{http_code}".to_string(),
            "-K".to_string(),
            "-".to_string(),
        ]);

        let mut child = Command::new("curl.exe")
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .ok()?;

        let mut cfg = format!("url = \"{}\"\n", Self::curl_escape(url));
        for (k, v) in headers {
            cfg.push_str(&format!(
                "header = \"{}\"\n",
                Self::curl_escape(&format!("{k}: {v}"))
            ));
        }

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(cfg.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }

        let output = tokio::time::timeout(
            Duration::from_secs(timeout_seconds + 5),
            child.wait_with_output(),
        )
        .await
        .ok()?
        .ok()?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            let mut brief = err.trim().lines().next().unwrap_or("").to_string();
            if let Some(q) = brief.find('?') {
                brief.truncate(q);
            }
            if brief.len() > 120 {
                brief.truncate(120);
            }
            DiagnosticsLog::info(&format!(
                "spotify curl exit={:?} ipv4={ipv4} err={brief}",
                output.status.code()
            ));
            return None;
        }

        let out = String::from_utf8_lossy(&output.stdout);
        let split = out.rfind('\n')?;
        let status = out[split + 1..].trim().parse::<u16>().ok()?;
        Some(CurlResponse {
            status,
            body: out[..split].to_string(),
        })
    }

    /// The player's script bundles, as linked from the player's page.
    pub fn scripts(html: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for m in BUNDLE_REGEX.find_iter(html) {
            let s = m.as_str().to_string();
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
        out
    }

    /// An operation's hash, as the player declares it:
    /// `"<name>","query","<64 hex>"` or `name:"x",operation:"query",sha256Hash:"…"`.
    pub fn query_hash(script: &str, operation: &str) -> Option<String> {
        for caps in OPERATION_REGEX.captures_iter(script) {
            if caps.get(1).map(|m| m.as_str()) == Some(operation) {
                return caps.get(2).map(|m| m.as_str().to_string());
            }
        }
        for caps in OPERATION_NAMED_REGEX.captures_iter(script) {
            if caps.get(1).map(|m| m.as_str()) == Some(operation) {
                return caps.get(2).map(|m| m.as_str().to_string());
            }
        }
        None
    }

    /// The one-time-password secrets the player carries, newest first.
    pub fn secrets(script: &str) -> Vec<SpotifySecret> {
        let mut out = Vec::new();
        for caps in SECRET_REGEX.captures_iter(script) {
            let raw = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map(|m| m.as_str())
                .unwrap_or("");
            let version = caps
                .get(3)
                .and_then(|m| m.as_str().parse::<i32>().ok());
            if raw.is_empty() {
                continue;
            }
            if let Some(ver) = version {
                out.push(SpotifySecret::new(Self::unescape(raw), ver));
            }
        }
        out.sort_by(|a, b| b.version.cmp(&a.version));
        out
    }

    fn unescape(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    match next {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        other => out.push(other),
                    }
                    continue;
                }
            }
            out.push(c);
        }
        out
    }

    /// Test hook: forget what was read.
    pub async fn reset_for_test() {
        let mut guard = CACHE.lock().await;
        *guard = PlayerCache::default();
        if let Ok(mut pguard) = PROXY_CACHE.lock() {
            *pguard = ProxyCache::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_scripts_and_hashes() {
        let html = r#"
        <script src="https://open.spotifycdn.com/cdn/build/web-player/web-player.abc123.js"></script>
        <script src="https://open.spotifycdn.com/cdn/build/web-player/web-player.abc123.js"></script>
        <script src="https://open.spotifycdn.com/cdn/build/web-player/chunk.def456.mjs"></script>
        "#;
        let scripts = SpotifyWebPlayer::scripts(html);
        assert_eq!(scripts.len(), 2);

        let bundle = r#"
        e.exports=["canvas","query","0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"];
        const x={name:"home",operation:"query",sha256Hash:"fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"};
        "#;
        assert_eq!(
            SpotifyWebPlayer::query_hash(bundle, CANVAS_OPERATION).as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(
            SpotifyWebPlayer::query_hash(bundle, HOME_OPERATION).as_deref(),
            Some("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210")
        );
    }

    #[test]
    fn extracts_and_sorts_secrets() {
        let script = r#"const a={secret:'abc\'d',version:58};const b={secret:"xyz",version:61};"#;
        let secrets = SpotifyWebPlayer::secrets(script);
        assert_eq!(secrets.len(), 2);
        assert_eq!(secrets[0], SpotifySecret::new("xyz", 61));
        assert_eq!(secrets[1], SpotifySecret::new("abc'd", 58));
    }
}
