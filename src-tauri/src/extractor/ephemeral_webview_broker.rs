use std::collections::HashSet;
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde_json::json;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex as AsyncMutex};

use super::challenge_parser::{
    comma_separated_bytes_to_base64, parse_create_challenge, parse_integrity_token,
    string_to_js_uint8_array,
};
use super::webview_profile::webview_user_data_folder;
use super::yt_auth_store::YtAuthStore;
use super::yt_webview_guard::{YtWebviewGuard, BOTGUARD_TITLE};

const CREATE_URL: &str = "https://jnn-pa.googleapis.com/$rpc/google.internal.waa.v1.Waa/Create";
const GENERATE_IT_URL: &str =
    "https://jnn-pa.googleapis.com/$rpc/google.internal.waa.v1.Waa/GenerateIT";
const REQUEST_KEY: &str = "O43z0dpjhgX20SCx4KAo";
const JNN_API_KEY: &str = "AIzaSyDyT5W0Jh49F30Pqqtyfdf7pDLFKLJoAnw";
const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(15);
const PO_TOKEN_HTML: &str = include_str!("../../assets/po_token.html");

/// Result of an ephemeral BotGuard PO-Token minting burst.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EphemeralTokenResult {
    pub player_token: String,
    pub session_token: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Error)]
pub enum EphemeralBrokerError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("challenge parse failed: {0}")]
    Challenge(#[from] super::challenge_parser::ChallengeParseError),

    #[error("webview operation failed: {0}")]
    Webview(String),

    #[error("ephemeral BotGuard burst timed out after 15s")]
    WatchdogTimeout,

    #[error("operation cancelled")]
    Cancelled,
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Abstraction over a short-lived WebView session capable of loading a URL and
/// evaluating JavaScript expressions (backed by Tauri WebView2/WebKit in production
/// and mockable in unit tests).
pub trait BotGuardWebviewSession: Send + Sync {
    fn launch(&self, url: &str) -> BoxFuture<'_, Result<(), String>>;
    fn eval_js(&self, script: &str) -> BoxFuture<'_, Result<Option<String>, String>>;
    fn hide(&self) -> BoxFuture<'_, ()>;
    fn close(&self) -> BoxFuture<'_, ()>;
}

/// Factory creating ephemeral [`BotGuardWebviewSession`] instances.
pub trait BotGuardWebviewHost: Send + Sync {
    fn create_session(&self) -> BoxFuture<'_, Result<Arc<dyn BotGuardWebviewSession>, String>>;
}

/// Ephemeral Tauri WebView2 / WebKit host using an invisible off-screen window
/// (`LastWave BotGuard`) and a local loopback callback bridge for JS return values.
pub struct TauriBotGuardHost {
    app: AppHandle,
    #[cfg(target_os = "linux")]
    linux_reusable: AsyncMutex<Option<Arc<TauriBotGuardSession>>>,
}

impl TauriBotGuardHost {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            #[cfg(target_os = "linux")]
            linux_reusable: AsyncMutex::new(None),
        }
    }
}

impl BotGuardWebviewHost for TauriBotGuardHost {
    fn create_session(&self) -> BoxFuture<'_, Result<Arc<dyn BotGuardWebviewSession>, String>> {
        Box::pin(async move {
            #[cfg(target_os = "linux")]
            {
                let mut guard = self.linux_reusable.lock().await;
                if let Some(existing) = guard.as_ref() {
                    let _ = YtWebviewGuard::hide_botguard();
                    return Ok(Arc::clone(existing) as Arc<dyn BotGuardWebviewSession>);
                }
                let created = Arc::new(TauriBotGuardSession::spawn(&self.app).await?);
                *guard = Some(Arc::clone(&created));
                return Ok(created as Arc<dyn BotGuardWebviewSession>);
            }

            #[cfg(not(target_os = "linux"))]
            {
                let created = Arc::new(TauriBotGuardSession::spawn(&self.app).await?);
                Ok(created as Arc<dyn BotGuardWebviewSession>)
            }
        })
    }
}

/// Live Tauri [`WebviewWindow`] session paired with a loopback callback port for `eval_js`.
pub struct TauriBotGuardSession {
    window: WebviewWindow,
    callback_port: u16,
    pending_evals: Arc<Mutex<std::collections::HashMap<u64, oneshot::Sender<Option<String>>>>>,
    next_eval_id: std::sync::atomic::AtomicU64,
    shutdown: Arc<AtomicBool>,
    _hook_guard: Option<super::yt_webview_guard::BotGuardHookGuard>,
}

impl TauriBotGuardSession {
    async fn spawn(app: &AppHandle) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("bind loopback bridge: {e}"))?;
        let callback_port = listener
            .local_addr()
            .map_err(|e| format!("read loopback addr: {e}"))?
            .port();

        let pending_evals: Arc<
            Mutex<std::collections::HashMap<u64, oneshot::Sender<Option<String>>>>,
        > = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let pending_clone = Arc::clone(&pending_evals);
        let shutdown_clone = Arc::clone(&shutdown);
        tokio::spawn(async move {
            run_loopback_bridge(listener, pending_clone, shutdown_clone).await;
        });

        let hook_guard = YtWebviewGuard::install_botguard_hook();
        let label = format!(
            "botguard_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        if let Some(existing) = app.get_webview_window(&label) {
            let _ = existing.destroy();
        }

        let initial_url = WebviewUrl::External(
            url::Url::parse("about:blank").map_err(|e| format!("invalid about:blank: {e}"))?,
        );
        let mut builder = WebviewWindowBuilder::new(app, &label, initial_url)
            .title(BOTGUARD_TITLE)
            .visible(false)
            .skip_taskbar(true)
            .decorations(false)
            .inner_size(2.0, 2.0)
            .position(-32000.0, -32000.0);

        if let Some(folder) = webview_user_data_folder("botguard") {
            builder = builder.data_directory(folder);
        }

        let window = builder
            .build()
            .map_err(|e| format!("create BotGuard webview window: {e}"))?;
        let _ = window.hide();
        let _ = YtWebviewGuard::hide_botguard();

        Ok(Self {
            window,
            callback_port,
            pending_evals,
            next_eval_id: std::sync::atomic::AtomicU64::new(1),
            shutdown,
            _hook_guard: hook_guard,
        })
    }
}

impl BotGuardWebviewSession for TauriBotGuardSession {
    fn launch(&self, url: &str) -> BoxFuture<'_, Result<(), String>> {
        let target = url.to_string();
        Box::pin(async move {
            let parsed = url::Url::parse(&target).map_err(|e| format!("invalid url: {e}"))?;
            self.window
                .navigate(parsed)
                .map_err(|e| format!("navigate failed: {e}"))?;
            let _ = YtWebviewGuard::hide_botguard();
            Ok(())
        })
    }

    fn eval_js(&self, script: &str) -> BoxFuture<'_, Result<Option<String>, String>> {
        let expr = script.to_string();
        Box::pin(async move {
            let eval_id = self
                .next_eval_id
                .fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = oneshot::channel();
            if let Ok(mut map) = self.pending_evals.lock() {
                map.insert(eval_id, tx);
            }

            let wrapped = format!(
                "(function(){{var __v=null;try{{__v=eval({expr_json});}}catch(_e){{__v=null;}}\
                 var __payload=(__v===undefined||__v===null)?null:String(__v);\
                 fetch('http://127.0.0.1:{port}/cb?id={eval_id}',{{\
                   method:'POST',headers:{{'Content-Type':'text/plain'}},\
                   body:JSON.stringify({{value:__payload}})\
                 }}).catch(function(){{}});}})();",
                expr_json = serde_json::to_string(&expr).unwrap_or_else(|_| "\"\"".to_string()),
                port = self.callback_port,
            );
            self.window
                .eval(&wrapped)
                .map_err(|e| format!("eval failed: {e}"))?;

            match tokio::time::timeout(Duration::from_secs(2), rx).await {
                Ok(Ok(val)) => Ok(val),
                _ => {
                    if let Ok(mut map) = self.pending_evals.lock() {
                        map.remove(&eval_id);
                    }
                    Ok(None)
                }
            }
        })
    }

    fn hide(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _ = self.window.hide();
            let _ = YtWebviewGuard::hide_botguard();
        })
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            if cfg!(target_os = "linux") {
                let _ = self.window.hide();
                let _ = YtWebviewGuard::hide_botguard();
            } else {
                self.shutdown.store(true, Ordering::Release);
                let _ = YtWebviewGuard::hide_botguard();
                let _ = self.window.destroy();
            }
        })
    }
}

async fn run_loopback_bridge(
    listener: TcpListener,
    pending: Arc<Mutex<std::collections::HashMap<u64, oneshot::Sender<Option<String>>>>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) {
        let Ok(accept_res) = tokio::time::timeout(Duration::from_millis(250), listener.accept()).await
        else {
            continue;
        };
        let Ok((mut stream, _)) = accept_res else {
            break;
        };
        let pending_ref = Arc::clone(&pending);
        tokio::spawn(async move {
            let mut raw = Vec::with_capacity(4096);
            let mut tmp = [0u8; 4096];
            let mut header_end: Option<usize> = None;
            let mut content_len: usize = 0;

            loop {
                let Ok(Ok(n)) = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut tmp)).await else {
                    break;
                };
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&tmp[..n]);
                if header_end.is_none() {
                    if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        let end = pos + 4;
                        header_end = Some(end);
                        let hdr = String::from_utf8_lossy(&raw[..pos]);
                        for line in hdr.lines() {
                            if let Some((k, v)) = line.split_once(':') {
                                if k.trim().eq_ignore_ascii_case("content-length") {
                                    content_len = v.trim().parse::<usize>().unwrap_or(0);
                                }
                            }
                        }
                    }
                }
                if let Some(end) = header_end {
                    if raw.len() >= end + content_len {
                        break;
                    }
                }
                if raw.len() >= 262_144 {
                    break;
                }
            }

            let req = String::from_utf8_lossy(&raw);
            let first_line = req.lines().next().unwrap_or("");
            let is_post = first_line.starts_with("POST ");
            let eval_id = first_line
                .split_whitespace()
                .nth(1)
                .and_then(|path| path.split("id=").nth(1))
                .and_then(|rest| rest.split('&').next())
                .and_then(|s| s.parse::<u64>().ok());

            if is_post {
                if let Some(id) = eval_id {
                    let body = header_end
                        .and_then(|end| raw.get(end..))
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .unwrap_or_default();
                    let value = serde_json::from_str::<serde_json::Value>(body.trim())
                        .ok()
                        .and_then(|v| v.get("value").and_then(|x| x.as_str()).map(str::to_owned));
                    if let Ok(mut map) = pending_ref.lock() {
                        if let Some(tx) = map.remove(&id) {
                            let _ = tx.send(value);
                        }
                    }
                }
            }

            let resp = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
            let _ = stream.write_all(resp.as_bytes()).await;
        });
    }
}

/// Ephemeral broker for system WebView2 / WebKit BotGuard operations.
///
/// Enforces single-flight execution, runs BotGuard handshake in an off-screen
/// WebView burst, tears down the window immediately on capture, and attaches a
/// 15-second watchdog timeout.
pub struct EphemeralWebViewBroker {
    client: Client,
    auth_store: Option<Arc<YtAuthStore>>,
    host: Mutex<Option<Arc<dyn BotGuardWebviewHost>>>,
    enabled: AtomicBool,
    single_flight: AsyncMutex<()>,
    failed_tracks: Mutex<HashSet<String>>,
}

impl EphemeralWebViewBroker {
    pub fn new(
        client: Client,
        auth_store: Option<Arc<YtAuthStore>>,
        host: Option<Arc<dyn BotGuardWebviewHost>>,
    ) -> Self {
        Self {
            client,
            auth_store,
            host: Mutex::new(host),
            enabled: AtomicBool::new(true),
            single_flight: AsyncMutex::new(()),
            failed_tracks: Mutex::new(HashSet::new()),
        }
    }

    pub fn set_host(&self, host: Arc<dyn BotGuardWebviewHost>) {
        if let Ok(mut guard) = self.host.lock() {
            *guard = Some(host);
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }

    /// Patches `assets/po_token.html` with `window.__bgLoaded=true;` and writes it to
    /// a temporary directory, returning `(dir_path, file_uri)`.
    pub fn write_temp_botguard_html() -> Result<(PathBuf, String), EphemeralBrokerError> {
        let patched = PO_TOKEN_HTML.replacen(
            "</script>",
            "\nwindow.__bgLoaded=true;</script>",
            1,
        );
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "lastwave_bg_ephemeral_{}_{}",
            std::process::id(),
            stamp
        ));
        fs::create_dir_all(&dir).map_err(|e| EphemeralBrokerError::Webview(e.to_string()))?;
        let file = dir.join("po_token.html");
        fs::write(&file, patched).map_err(|e| EphemeralBrokerError::Webview(e.to_string()))?;
        let uri = url::Url::from_file_path(&file)
            .map_err(|_| EphemeralBrokerError::Webview("failed to convert temp file path to URI".to_string()))?
            .to_string();
        Ok((dir, uri))
    }

    /// Single-flight BotGuard token minting burst with 15-second watchdog and immediate teardown.
    pub async fn mint_token(
        &self,
        video_id: &str,
        session_id: &str,
        force_refresh: bool,
    ) -> Option<EphemeralTokenResult> {
        if !self.is_enabled() {
            return None;
        }

        if !force_refresh {
            if let Some(store) = &self.auth_store {
                let current = store.current();
                if !current.is_po_token_stale() && !current.po_token.is_empty() {
                    let fallback_exp = now_unix_ms() + 3_600_000;
                    return Some(EphemeralTokenResult {
                        player_token: current.po_token.clone(),
                        session_token: current.po_token,
                        expires_at_ms: current.po_token_expires_at.unwrap_or(fallback_exp),
                    });
                }
            }
            if let Ok(failed) = self.failed_tracks.lock() {
                if failed.contains(video_id) {
                    return None;
                }
            }
        }

        let _flight = self.single_flight.lock().await;

        // Re-check cache after acquiring single-flight lock in case another caller just populated it.
        if !force_refresh {
            if let Some(store) = &self.auth_store {
                let current = store.current();
                if !current.is_po_token_stale() && !current.po_token.is_empty() {
                    let fallback_exp = now_unix_ms() + 3_600_000;
                    return Some(EphemeralTokenResult {
                        player_token: current.po_token.clone(),
                        session_token: current.po_token,
                        expires_at_ms: current.po_token_expires_at.unwrap_or(fallback_exp),
                    });
                }
            }
        }

        let Some(host) = self.host.lock().ok().and_then(|g| g.clone()) else {
            return None;
        };

        let cancelled = Arc::new(AtomicBool::new(false));
        let active_session: Arc<AsyncMutex<Option<Arc<dyn BotGuardWebviewSession>>>> =
            Arc::new(AsyncMutex::new(None));

        let burst_fut = self.run_ephemeral_burst(
            host,
            video_id.to_string(),
            session_id.to_string(),
            Arc::clone(&cancelled),
            Arc::clone(&active_session),
        );

        let result = match tokio::time::timeout(WATCHDOG_TIMEOUT, burst_fut).await {
            Ok(Ok(res)) => {
                crate::storage::diagnostics_log::DiagnosticsLog::info(&format!(
                    "botguard mint_ok video_id={video_id} len={}",
                    res.player_token.len()
                ));
                Some(res)
            }
            Ok(Err(e)) => {
                crate::storage::diagnostics_log::DiagnosticsLog::info(&format!(
                    "botguard mint_err video_id={video_id} err={e}"
                ));
                None
            }
            Err(_) => {
                crate::storage::diagnostics_log::DiagnosticsLog::info(&format!(
                    "botguard mint_timeout video_id={video_id}"
                ));
                cancelled.store(true, Ordering::Release);
                let stuck = active_session.lock().await.take();
                if let Some(session) = stuck {
                    session.close().await;
                }
                None
            }
        };

        if let Some(ref token) = result {
            if let Ok(mut failed) = self.failed_tracks.lock() {
                failed.remove(video_id);
            }
            if let Some(store) = &self.auth_store {
                store.update_po_token(token.player_token.clone(), token.expires_at_ms);
            }
        } else if let Ok(mut failed) = self.failed_tracks.lock() {
            failed.insert(video_id.to_string());
        }

        result
    }

    async fn run_ephemeral_burst(
        &self,
        host: Arc<dyn BotGuardWebviewHost>,
        video_id: String,
        session_id: String,
        cancelled: Arc<AtomicBool>,
        active_session: Arc<AsyncMutex<Option<Arc<dyn BotGuardWebviewSession>>>>,
    ) -> Result<EphemeralTokenResult, EphemeralBrokerError> {
        struct TempDirCleanup(Option<PathBuf>);
        impl Drop for TempDirCleanup {
            fn drop(&mut self) {
                if let Some(dir) = self.0.take() {
                    let _ = fs::remove_dir_all(dir);
                }
            }
        }

        let (temp_dir, uri) = Self::write_temp_botguard_html()?;
        let _cleanup = TempDirCleanup(Some(temp_dir));

        if cancelled.load(Ordering::Acquire) {
            return Err(EphemeralBrokerError::Cancelled);
        }

        let session = host
            .create_session()
            .await
            .map_err(EphemeralBrokerError::Webview)?;
        {
            let mut slot = active_session.lock().await;
            *slot = Some(Arc::clone(&session));
        }

        let outcome = self
            .execute_botguard_handshake(&session, &uri, &video_id, &session_id, &cancelled)
            .await;

        {
            let mut slot = active_session.lock().await;
            *slot = None;
        }
        session.close().await;
        outcome
    }

    async fn execute_botguard_handshake(
        &self,
        session: &Arc<dyn BotGuardWebviewSession>,
        uri: &str,
        video_id: &str,
        session_id: &str,
        cancelled: &AtomicBool,
    ) -> Result<EphemeralTokenResult, EphemeralBrokerError> {
        session.hide().await;
        let already_loaded = eval_str(session, "window.__bgLoaded")
            .await
            .as_deref()
            == Some("true");
        if !already_loaded {
            session
                .launch(uri)
                .await
                .map_err(EphemeralBrokerError::Webview)?;
            session.hide().await;
            let loaded = poll_until(Duration::from_secs(5), cancelled, || async {
                eval_str(session, "window.__bgLoaded").await.as_deref() == Some("true")
            })
            .await;
            session.hide().await;
            if !loaded {
                return Err(EphemeralBrokerError::Webview(
                    "ephemeral bg page failed to load".to_string(),
                ));
            }
        }

        let challenge_raw = self.post_json_array(CREATE_URL, &[REQUEST_KEY]).await?;
        let challenge_json = parse_create_challenge(&challenge_raw)?;

        let run_js = format!(
            "window.__bgOut=null;window.__bgErr=null;\
             try{{var data={challenge_json};\
             runBotGuard(data).then(function(r){{\
             window.__bgW=r.webPoSignalOutput;\
             window.__bgB=(typeof r.botguardResponse===\"string\")\
             ?r.botguardResponse:JSON.stringify(r.botguardResponse);\
             }},function(e){{window.__bgErr=String((e&&e.stack)||e);}});\
             }}catch(e){{window.__bgErr=String((e&&e.stack)||e);}}"
        );
        let _ = session.eval_js(&run_js).await;

        let botguard_response = poll_string(
            session,
            "window.__bgB",
            "window.__bgErr",
            Duration::from_secs(5),
            cancelled,
        )
        .await
        .ok_or_else(|| EphemeralBrokerError::Webview("runBotGuard failed".to_string()))?;

        let integrity_raw = self
            .post_json_array(GENERATE_IT_URL, &[REQUEST_KEY, &botguard_response])
            .await?;
        let (token_u8, lifetime_sec) = parse_integrity_token(&integrity_raw)?;
        let ttl_sec = (lifetime_sec - 300).clamp(60, 1 << 30);
        let expires_at_ms = now_unix_ms() + ttl_sec * 1000;

        let minter_js = format!(
            "window.__bgM=0;window.__bgErr=null;\
             try{{createPoTokenMinter(window.__bgW,{token_u8})\
             .then(function(){{window.__bgM=1;}},\
             function(e){{window.__bgErr=String((e&&e.stack)||e);}});\
             }}catch(e){{window.__bgErr=String((e&&e.stack)||e);}}"
        );
        let _ = session.eval_js(&minter_js).await;

        let minter_ok = poll_until(Duration::from_secs(5), cancelled, || async {
            eval_str(session, "window.__bgM").await.as_deref() == Some("1")
        })
        .await;
        if !minter_ok {
            return Err(EphemeralBrokerError::Webview(
                "minter failed to initialize".to_string(),
            ));
        }

        let session_token = mint_identifier(session, session_id, cancelled)
            .await
            .ok_or_else(|| EphemeralBrokerError::Webview("failed to mint session token".to_string()))?;
        let player_token = mint_identifier(session, video_id, cancelled)
            .await
            .ok_or_else(|| EphemeralBrokerError::Webview("failed to mint player token".to_string()))?;

        Ok(EphemeralTokenResult {
            player_token,
            session_token,
            expires_at_ms,
        })
    }

    async fn post_json_array(
        &self,
        url: &str,
        body: &[&str],
    ) -> Result<String, EphemeralBrokerError> {
        let payload = json!(body).to_string();
        let res = self
            .client
            .post(url)
            .header("Content-Type", "application/json+protobuf")
            .header("X-Goog-Api-Key", JNN_API_KEY)
            .header("Origin", "https://www.youtube.com")
            .header("Referer", "https://www.youtube.com/")
            .body(payload)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            return Err(EphemeralBrokerError::Webview(format!(
                "HTTP {} from {url}: {}",
                status.as_u16(),
                text.chars().take(160).collect::<String>()
            )));
        }
        if text.is_empty() {
            return Err(EphemeralBrokerError::Webview(format!("empty response from {url}")));
        }
        Ok(text)
    }
}

async fn mint_identifier(
    session: &Arc<dyn BotGuardWebviewSession>,
    identifier: &str,
    cancelled: &AtomicBool,
) -> Option<String> {
    let u8_js = string_to_js_uint8_array(identifier);
    let script = format!(
        "window.__bgMint=null;window.__bgErr=null;\
         try{{obtainPoToken({u8_js})\
         .then(function(u){{window.__bgMint=Array.from(u).join(\",\");}},\
         function(e){{window.__bgErr=String((e&&e.stack)||e);}});\
         }}catch(e){{window.__bgErr=String((e&&e.stack)||e);}}"
    );
    let _ = session.eval_js(&script).await;
    let csv = poll_string(
        session,
        "window.__bgMint",
        "window.__bgErr",
        Duration::from_secs(5),
        cancelled,
    )
    .await?;
    if csv.is_empty() {
        return None;
    }
    comma_separated_bytes_to_base64(&csv).ok()
}

async fn eval_str(session: &Arc<dyn BotGuardWebviewSession>, expr: &str) -> Option<String> {
    let raw = session.eval_js(expr).await.ok().flatten()?;
    let trimmed = raw.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        if let Ok(decoded) = serde_json::from_str::<String>(trimmed) {
            return Some(decoded);
        }
    }
    Some(trimmed.to_string())
}

async fn poll_until<F, Fut>(timeout: Duration, cancelled: &AtomicBool, mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && !cancelled.load(Ordering::Acquire) {
        if check().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

async fn poll_string(
    session: &Arc<dyn BotGuardWebviewSession>,
    result_expr: &str,
    error_expr: &str,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Option<String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline && !cancelled.load(Ordering::Acquire) {
        if let Some(err) = eval_str(session, error_expr).await {
            if !err.is_empty() && err != "null" {
                return None;
            }
        }
        if let Some(val) = eval_str(session, result_expr).await {
            if !val.is_empty() && val != "null" {
                return Some(val);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::http_client::HttpClientFactory;
    use crate::storage::secure_store::SecureStore;

    #[test]
    fn write_temp_botguard_html_patches_and_cleans_up() {
        let (dir, uri) =
            EphemeralWebViewBroker::write_temp_botguard_html().expect("write temp html");
        assert!(uri.starts_with("file://"));
        let file = dir.join("po_token.html");
        let content = fs::read_to_string(&file).expect("read patched html");
        assert!(content.contains("window.__bgLoaded=true;</script>"));
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn mint_token_uses_cached_non_stale_store_entry() {
        let store = Arc::new(YtAuthStore::new(Arc::new(SecureStore::in_memory())));
        let future_exp = now_unix_ms() + 3_600_000;
        store.update_po_token("cached_po_tok".to_string(), future_exp);

        let client = HttpClientFactory::create().expect("client");
        let broker = EphemeralWebViewBroker::new(client, Some(store), None);
        let res = broker
            .mint_token("vid123", "sess1", false)
            .await
            .expect("cached token");
        assert_eq!(res.player_token, "cached_po_tok");
        assert_eq!(res.session_token, "cached_po_tok");
    }
}
