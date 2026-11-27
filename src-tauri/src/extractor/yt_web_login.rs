use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use url::Url;

use super::webview_profile::{reset_webview_profile, webview_user_data_folder};
use super::yt_webview_guard::{YtWebviewGuard, YT_SIGN_IN_TITLE};

pub const LOGIN_URL: &str = "https://music.youtube.com/";
pub const LOGOUT_URL: &str = "https://accounts.google.com/Logout";
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);
pub const DEFAULT_LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const SILENT_REFRESH_TIMEOUT: Duration = Duration::from_secs(75);
pub const WINDOW_TITLE: &str = YT_SIGN_IN_TITLE;

const MAC_CHROME_SUFFIX: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/605.1.15";

const MAC_COMPAT_SHIM: &str = r#"
(function () {
  try {
    var ua = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) '
      + 'AppleWebKit/537.36 (KHTML, like Gecko) '
      + 'Chrome/131.0.0.0 Safari/537.36';
    Object.defineProperty(navigator, 'userAgent', { get: function () { return ua; } });
    if (!navigator.userAgentData) {
      var brands = [
        { brand: 'Chromium', version: '131' },
        { brand: 'Google Chrome', version: '131' },
        { brand: 'Not_A Brand', version: '24' }
      ];
      Object.defineProperty(navigator, 'userAgentData', {
        get: function () {
          return {
            brands: brands,
            mobile: false,
            platform: 'macOS',
            getHighEntropyValues: function () {
              return Promise.resolve({
                brands: brands,
                mobile: false,
                platform: 'macOS',
                platformVersion: '14.0.0',
                architecture: 'arm',
                bitness: '64',
                uaFullVersion: '131.0.0.0'
              });
            }
          };
        }
      });
    }
    if (!window.chrome) { window.chrome = { runtime: {} }; }
    Object.defineProperty(navigator, 'webdriver', { get: function () { return undefined; } });
  } catch (e) {}
})();
"#;

const SAPISID_NAMES: &[&str] = &[
    "__Secure-3PAPISID",
    "__Secure-1PAPISID",
    "SAPISID",
    "APISID",
];
const PSID_NAMES: &[&str] = &["__Secure-3PSID", "__Secure-1PSID", "SID"];
const FRESH_NAMES: &[&str] = &["__Secure-3PSIDTS", "__Secure-1PSIDTS"];
const LOGIN_NAMES: &[&str] = &["LOGIN_INFO"];

/// Why the last [`YtWebLogin::sign_in`] attempt produced no session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum YtSignInFailure {
    #[default]
    None,
    Unavailable,
    StartFailed,
    WindowClosed,
    Timeout,
    BlankPage,
    BrowserBlocked,
    NoSession,
}

/// A cookie as read from the system WebView jar.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct YtJarCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YtPageState {
    pub url: Option<String>,
    pub signed_in: Option<bool>,
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Abstract handle over an interactive login WebView window (backed by Tauri
/// `WebviewWindow` in production and mockable in unit tests).
pub trait LoginWebviewSession: Send + Sync {
    fn launch(&self, url: &str, trigger_url_request: bool) -> BoxFuture<'_, Result<(), String>>;
    fn eval_js(&self, script: &str) -> BoxFuture<'_, Result<Option<String>, String>>;
    fn read_cookies(&self) -> BoxFuture<'_, Option<Vec<YtJarCookie>>>;
    fn set_visible(&self, visible: bool) -> BoxFuture<'_, Result<(), String>>;
    fn bring_to_foreground(&self) -> BoxFuture<'_, Result<(), String>>;
    fn unpark_centered(&self) -> BoxFuture<'_, Result<(), String>>;
    fn is_closed(&self) -> bool;
}

/// Factory managing the single reusable login WebView window.
pub trait LoginWebviewHost: Send + Sync {
    fn is_available(&self) -> BoxFuture<'_, bool>;
    fn create_window(
        &self,
        show: bool,
    ) -> BoxFuture<'_, Result<Arc<dyn LoginWebviewSession>, String>>;
}

// Production Tauri Webview Implementation

pub struct TauriLoginWebviewHost {
    app: AppHandle,
}

impl TauriLoginWebviewHost {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl LoginWebviewHost for TauriLoginWebviewHost {
    fn is_available(&self) -> BoxFuture<'_, bool> {
        Box::pin(async { true })
    }

    fn create_window(
        &self,
        show: bool,
    ) -> BoxFuture<'_, Result<Arc<dyn LoginWebviewSession>, String>> {
        Box::pin(async move {
            let session = TauriLoginWebviewSession::spawn(&self.app, show).await?;
            Ok(Arc::new(session) as Arc<dyn LoginWebviewSession>)
        })
    }
}

pub struct TauriLoginWebviewSession {
    window: WebviewWindow,
    callback_port: u16,
    pending_evals: Arc<Mutex<std::collections::HashMap<u64, oneshot::Sender<Option<String>>>>>,
    next_eval_id: AtomicU64,
    closed: Arc<AtomicBool>,
    parked: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
}

impl TauriLoginWebviewSession {
    async fn spawn(app: &AppHandle, show: bool) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("bind login loopback bridge: {e}"))?;
        let callback_port = listener
            .local_addr()
            .map_err(|e| format!("read login loopback addr: {e}"))?
            .port();

        let pending_evals: Arc<
            Mutex<std::collections::HashMap<u64, oneshot::Sender<Option<String>>>>,
        > = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let closed = Arc::new(AtomicBool::new(false));

        let p_clone = Arc::clone(&pending_evals);
        let s_clone = Arc::clone(&shutdown);
        tokio::spawn(async move {
            run_login_loopback(listener, p_clone, s_clone).await;
        });

        let label = "yt_sign_in_webview";
        if let Some(existing) = app.get_webview_window(label) {
            let _ = existing.destroy();
        }

        let park = !show && cfg!(target_os = "windows");
        let initial = WebviewUrl::External(
            Url::parse("about:blank").map_err(|e| format!("invalid url: {e}"))?,
        );
        let mut builder = WebviewWindowBuilder::new(app, label, initial)
            .title(WINDOW_TITLE)
            .inner_size(480.0, 760.0)
            .visible(show && !park);

        if park {
            builder = builder.position(-32000.0, -32000.0);
        } else {
            builder = builder.center();
        }

        if let Some(folder) = webview_user_data_folder("auth") {
            builder = builder.data_directory(folder);
        }

        #[cfg(target_os = "macos")]
        {
            builder = builder
                .initialization_script(MAC_COMPAT_SHIM)
                .user_agent(MAC_CHROME_SUFFIX);
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (MAC_COMPAT_SHIM, MAC_CHROME_SUFFIX);
        }

        let window = builder
            .build()
            .map_err(|e| format!("create sign-in webview: {e}"))?;

        let closed_clone = Arc::clone(&closed);
        let win_clone = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Hide, never destroy (upstream Linux GTK/EGL segfault protection).
                api.prevent_close();
                closed_clone.store(true, Ordering::Release);
                if cfg!(target_os = "linux") {
                    let _ = YtWebviewGuard::hide();
                } else {
                    let _ = win_clone.hide();
                }
            } else if let WindowEvent::Destroyed = event {
                closed_clone.store(true, Ordering::Release);
            }
        });

        if !show {
            if cfg!(target_os = "linux") {
                let _ = YtWebviewGuard::hide();
            } else {
                let _ = window.hide();
            }
        } else if cfg!(target_os = "linux") {
            let _ = YtWebviewGuard::show();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }

        Ok(Self {
            window,
            callback_port,
            pending_evals,
            next_eval_id: AtomicU64::new(1),
            closed,
            parked: Arc::new(AtomicBool::new(park)),
            shutdown,
        })
    }
}

impl Drop for TauriLoginWebviewSession {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

impl LoginWebviewSession for TauriLoginWebviewSession {
    fn launch(&self, url: &str, _trigger_url_request: bool) -> BoxFuture<'_, Result<(), String>> {
        let target = url.to_string();
        Box::pin(async move {
            let parsed = Url::parse(&target).map_err(|e| format!("invalid url: {e}"))?;
            self.closed.store(false, Ordering::Release);
            self.window
                .navigate(parsed)
                .map_err(|e| format!("navigate failed: {e}"))
        })
    }

    fn eval_js(&self, script: &str) -> BoxFuture<'_, Result<Option<String>, String>> {
        let expr = script.to_string();
        Box::pin(async move {
            let eval_id = self.next_eval_id.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = oneshot::channel();
            if let Ok(mut map) = self.pending_evals.lock() {
                map.insert(eval_id, tx);
            }
            let expr_json = serde_json::to_string(&expr).unwrap_or_else(|_| "\"\"".to_string());
            let wrapped = format!(
                "(function(){{var __v=null;try{{__v=eval({expr_json});}}catch(_e){{__v=null;}}\
                 var __payload=(__v===undefined||__v===null)?null:String(__v);\
                 fetch('http://127.0.0.1:{port}/cb?id={eval_id}',{{\
                   method:'POST',headers:{{'Content-Type':'application/json'}},\
                   body:JSON.stringify({{value:__payload}})\
                 }}).catch(function(){{}});}})();",
                port = self.callback_port,
            );

            if let Err(e) = self.window.eval(&wrapped) {
                if let Ok(mut map) = self.pending_evals.lock() {
                    map.remove(&eval_id);
                }
                return Err(format!("eval failed: {e}"));
            }

            match tokio::time::timeout(Duration::from_secs(5), rx).await {
                Ok(Ok(val)) => Ok(val),
                Ok(Err(_)) => Ok(None),
                Err(_) => {
                    if let Ok(mut map) = self.pending_evals.lock() {
                        map.remove(&eval_id);
                    }
                    Err("eval_js timeout".to_string())
                }
            }
        })
    }

    fn read_cookies(&self) -> BoxFuture<'_, Option<Vec<YtJarCookie>>> {
        Box::pin(async move {
            let raw = self.window.cookies().ok()?;
            let mut out = Vec::with_capacity(raw.len());
            for c in raw {
                out.push(YtJarCookie {
                    name: YtWebLogin::norm_name(c.name()),
                    value: YtWebLogin::norm_value(c.value()),
                    domain: YtWebLogin::norm_name(c.domain().unwrap_or("")),
                });
            }
            Some(out)
        })
    }

    fn set_visible(&self, visible: bool) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            if cfg!(target_os = "linux") {
                if visible {
                    YtWebviewGuard::show();
                } else {
                    YtWebviewGuard::hide();
                }
                return Ok(());
            }
            if visible {
                self.window.show().map_err(|e| e.to_string())
            } else {
                self.window.hide().map_err(|e| e.to_string())
            }
        })
    }

    fn bring_to_foreground(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            self.window.set_focus().map_err(|e| e.to_string())
        })
    }

    fn unpark_centered(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            if self.parked.swap(false, Ordering::AcqRel) {
                let _ = self.window.set_size(LogicalSize::new(480.0, 760.0));
                let _ = self.window.center();
                let _ = self.window.set_position(LogicalPosition::new(100.0, 80.0));
                let _ = self.window.center();
            }
            Ok(())
        })
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

async fn run_login_loopback(
    listener: TcpListener,
    pending: Arc<Mutex<std::collections::HashMap<u64, oneshot::Sender<Option<String>>>>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) {
        let Ok((mut stream, _)) = listener.accept().await else {
            break;
        };
        let p_clone = Arc::clone(&pending);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            let Ok(n) = stream.read(&mut buf).await else {
                return;
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let first_line = req.lines().next().unwrap_or("");
            let path = first_line.split_whitespace().nth(1).unwrap_or("");
            let eval_id = path
                .split("id=")
                .nth(1)
                .and_then(|s| s.split('&').next())
                .and_then(|s| s.parse::<u64>().ok());

            if let Some(id) = eval_id {
                let value = req
                    .split("\r\n\r\n")
                    .nth(1)
                    .or_else(|| req.split("\n\n").nth(1))
                    .and_then(|body| serde_json::from_str::<serde_json::Value>(body.trim()).ok())
                    .and_then(|v| {
                        v.get("value")
                            .and_then(|inner| inner.as_str().map(str::to_string))
                    });
                if let Ok(mut map) = p_clone.lock() {
                    if let Some(tx) = map.remove(&id) {
                        let _ = tx.send(value);
                    }
                }
            }
            let resp = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
            let _ = stream.write_all(resp.as_bytes()).await;
        });
    }
}

// YtWebLogin Coordinator

/// In-app YouTube Music sign-in and silent cookie rotation coordinator.
pub struct YtWebLogin {
    host: Arc<dyn LoginWebviewHost>,
    cached_window: AsyncMutex<Option<Arc<dyn LoginWebviewSession>>>,
    inflight: AtomicBool,
    expect_visible: AtomicBool,
    last_failure: RwLock<YtSignInFailure>,
    window_failure: RwLock<YtSignInFailure>,
    last_captured_page_id: RwLock<Option<String>>,
}

impl YtWebLogin {
    pub fn new(host: Arc<dyn LoginWebviewHost>) -> Self {
        Self {
            host,
            cached_window: AsyncMutex::new(None),
            inflight: AtomicBool::new(false),
            expect_visible: AtomicBool::new(false),
            last_failure: RwLock::new(YtSignInFailure::None),
            window_failure: RwLock::new(YtSignInFailure::Unavailable),
            last_captured_page_id: RwLock::new(None),
        }
    }

    pub fn for_app(app: AppHandle) -> Self {
        Self::new(Arc::new(TauriLoginWebviewHost::new(app)))
    }

    pub fn last_failure(&self) -> YtSignInFailure {
        self.last_failure
            .read()
            .map(|g| *g)
            .unwrap_or(YtSignInFailure::None)
    }

    pub fn last_captured_page_id(&self) -> Option<String> {
        self.last_captured_page_id
            .read()
            .ok()
            .and_then(|g| g.clone())
    }

    pub async fn has_cached_window(&self) -> bool {
        self.cached_window.lock().await.is_some()
    }

    fn set_last_failure(&self, failure: YtSignInFailure) {
        if let Ok(mut g) = self.last_failure.write() {
            *g = failure;
        }
    }

    fn set_window_failure(&self, failure: YtSignInFailure) {
        if let Ok(mut g) = self.window_failure.write() {
            *g = failure;
        }
    }

    fn get_window_failure(&self) -> YtSignInFailure {
        self.window_failure
            .read()
            .map(|g| *g)
            .unwrap_or(YtSignInFailure::Unavailable)
    }

    pub async fn sign_in(self: &Arc<Self>) -> Option<String> {
        if self.inflight.swap(true, Ordering::AcqRel) {
            return None;
        }
        let res = self.run().await;
        self.inflight.store(false, Ordering::Release);
        res
    }

    pub async fn sign_in_fresh(self: &Arc<Self>) -> Option<String> {
        let Some(w) = self.ensure_window(true).await else {
            self.set_last_failure(self.get_window_failure());
            return None;
        };
        let _ = w.launch(LOGOUT_URL, false).await;
        self.foreground(&w).await;
        tokio::time::sleep(Duration::from_secs(4)).await;
        self.sign_in().await
    }

    pub async fn sign_out_webview(self: &Arc<Self>) {
        if self.inflight.load(Ordering::Acquire) {
            return;
        }
        let Some(w) = self.ensure_window(false).await else {
            return;
        };
        let _ = w.launch(LOGOUT_URL, false).await;
        tokio::time::sleep(Duration::from_secs(4)).await;
        self.hide_window().await;
    }

    pub async fn hide_window(&self) {
        self.expect_visible.store(false, Ordering::Release);
        let guard = self.cached_window.lock().await;
        if let Some(w) = guard.as_ref() {
            let _ = w.set_visible(false).await;
        }
    }

    pub async fn reload_login_page(self: &Arc<Self>) {
        let w = self.cached_window.lock().await.clone();
        if let Some(win) = w {
            let _ = win.launch(LOGIN_URL, false).await;
            self.foreground(&win).await;
        }
    }

    pub async fn ensure_window(&self, show: bool) -> Option<Arc<dyn LoginWebviewSession>> {
        {
            let mut guard = self.cached_window.lock().await;
            if let Some(cached) = guard.clone() {
                if !show {
                    return Some(cached);
                }
                if cfg!(target_os = "linux") {
                    if YtWebviewGuard::show() {
                        return Some(cached);
                    }
                } else if cached.set_visible(true).await.is_ok() {
                    self.expect_visible.store(true, Ordering::Release);
                    let _ = cached.unpark_centered().await;
                    let _ = cached.bring_to_foreground().await;
                    return Some(cached);
                }
                *guard = None;
            }
        }

        if !self.host.is_available().await {
            self.set_window_failure(YtSignInFailure::Unavailable);
            return None;
        }

        for attempt in 1..=2 {
            if let Ok(created) = self.host.create_window(show).await {
                let mut guard = self.cached_window.lock().await;
                *guard = Some(Arc::clone(&created));
                if show {
                    self.expect_visible.store(true, Ordering::Release);
                }
                return Some(created);
            }
            if attempt == 1 {
                if cfg!(target_os = "windows") {
                    reset_webview_profile("auth");
                }
                tokio::time::sleep(Duration::from_millis(800)).await;
            }
        }

        self.set_window_failure(YtSignInFailure::StartFailed);
        None
    }

    async fn foreground(self: &Arc<Self>, w: &Arc<dyn LoginWebviewSession>) {
        self.expect_visible.store(true, Ordering::Release);
        if cfg!(target_os = "linux") {
            YtWebviewGuard::show();
            return;
        }
        self.foreground_once(w).await;
        let me = Arc::clone(self);
        let win = Arc::clone(w);
        tokio::spawn(async move {
            for _ in 0..2 {
                tokio::time::sleep(Duration::from_millis(900)).await;
                if !me.expect_visible.load(Ordering::Acquire) {
                    return;
                }
                me.foreground_once(&win).await;
            }
        });
    }

    async fn foreground_once(&self, w: &Arc<dyn LoginWebviewSession>) {
        if !self.expect_visible.load(Ordering::Acquire) {
            return;
        }
        let _ = w.unpark_centered().await;
        let _ = w.set_visible(true).await;
        if !self.expect_visible.load(Ordering::Acquire) {
            return;
        }
        let _ = w.bring_to_foreground().await;
    }

    pub async fn refresh_silently(
        self: &Arc<Self>,
        timeout: Duration,
        allow_create: bool,
    ) -> Option<String> {
        if self.inflight.swap(true, Ordering::AcqRel) {
            return None;
        }
        let result = async {
            let existing = self.cached_window.lock().await.clone();
            let w = match existing {
                Some(w) => w,
                None if allow_create => self.ensure_window(false).await?,
                None => return None,
            };
            self.expect_visible.store(false, Ordering::Release);
            let _ = w.set_visible(false).await;
            tokio::time::sleep(Duration::from_millis(300)).await;
            if w.launch(LOGIN_URL, false).await.is_err() {
                return None;
            }
            let r = tokio::time::timeout(timeout, self.wait_for_login(&w, true))
                .await
                .unwrap_or((None, YtSignInFailure::Timeout));
            r.0
        }
        .await;

        self.expect_visible.store(false, Ordering::Release);
        self.inflight.store(false, Ordering::Release);
        result
    }

    pub async fn wait_for_login_session(
        self: &Arc<Self>,
        webview: &Arc<dyn LoginWebviewSession>,
        timeout: Duration,
    ) -> Option<String> {
        let (header, failure) = tokio::time::timeout(timeout, self.wait_for_login(webview, false))
            .await
            .unwrap_or((None, YtSignInFailure::Timeout));
        let resolved_fail = if header.is_some() {
            YtSignInFailure::None
        } else if failure == YtSignInFailure::None {
            YtSignInFailure::NoSession
        } else {
            failure
        };
        self.set_last_failure(resolved_fail);
        header
    }

    async fn run(self: &Arc<Self>) -> Option<String> {
        self.set_last_failure(YtSignInFailure::None);
        if let Ok(mut g) = self.last_captured_page_id.write() {
            *g = None;
        }
        let Some(w) = self.ensure_window(true).await else {
            self.set_last_failure(self.get_window_failure());
            return None;
        };

        tokio::time::sleep(Duration::from_millis(300)).await;
        let _ = w.launch(LOGIN_URL, false).await;
        self.foreground(&w).await;

        let (header, failure) =
            tokio::time::timeout(DEFAULT_LOGIN_TIMEOUT, self.wait_for_login(&w, false))
                .await
                .unwrap_or((None, YtSignInFailure::Timeout));

        let resolved_fail = if header.is_some() {
            YtSignInFailure::None
        } else if failure == YtSignInFailure::None {
            YtSignInFailure::NoSession
        } else {
            failure
        };
        self.set_last_failure(resolved_fail);

        if header.is_some() {
            let pid = Self::read_delegated_page_id(&*w).await;
            if let Ok(mut g) = self.last_captured_page_id.write() {
                *g = pid;
            }
        }

        self.expect_visible.store(false, Ordering::Release);
        let _ = w.set_visible(false).await;
        header
    }

    async fn wait_for_login(
        self: &Arc<Self>,
        webview: &Arc<dyn LoginWebviewSession>,
        silent: bool,
    ) -> (Option<String>, YtSignInFailure) {
        tokio::time::sleep(Duration::from_secs(4)).await;

        let gone = || async {
            if webview.is_closed() {
                return true;
            }
            if silent {
                return false;
            }
            if cfg!(target_os = "linux") && !YtWebviewGuard::is_visible() {
                return true;
            }
            false
        };

        let mut stuck_cycles = 0u32;
        let mut auto_reloads = 0u32;
        let mut blank_cycles = 0u32;
        let mut blank_reloads = 0u32;

        while !webview.is_closed() {
            if gone().await {
                return (None, YtSignInFailure::WindowClosed);
            }

            let cookies = webview.read_cookies().await;
            let header = cookies.as_deref().and_then(Self::build_cookie_header);
            let page = Self::page_state(&**webview).await;
            let url = page.url.as_deref();

            if gone().await {
                return (None, YtSignInFailure::WindowClosed);
            }

            let is_blank = url.is_some_and(|u| Self::is_blank_url(u) || Self::is_error_url(u));
            if is_blank {
                blank_cycles += 1;
                if blank_cycles >= 2 {
                    blank_cycles = 0;
                    blank_reloads += 1;
                    if blank_reloads > 3 {
                        return (None, YtSignInFailure::BlankPage);
                    }
                    let _ = webview.launch(LOGIN_URL, false).await;
                    let _ = webview
                        .eval_js(&format!("window.location.href = \"{LOGIN_URL}\";"))
                        .await;
                    if !silent && !gone().await {
                        self.foreground(webview).await;
                    }
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            } else {
                blank_cycles = 0;
            }

            let on_google = url.is_none_or(|u| u.contains("google.com"));
            let mut text: Option<String> = None;
            if (header.is_none() || on_google) && !silent {
                text = Self::page_text(&**webview).await;
                if on_google {
                    if let Some(ref t) = text {
                        if Self::is_browser_blocked_page(t) {
                            return (None, YtSignInFailure::BrowserBlocked);
                        }
                    }
                }
                if gone().await {
                    return (None, YtSignInFailure::WindowClosed);
                }
            }

            if let Some(ref h) = header {
                if Self::is_acceptable_host(url) {
                    return (Some(h.clone()), YtSignInFailure::None);
                }
            }

            let stuck = text
                .as_deref()
                .is_some_and(Self::is_passkey_verification_stall);
            if webview.is_closed() {
                return (None, YtSignInFailure::WindowClosed);
            }
            if stuck {
                stuck_cycles += 1;
                if stuck_cycles >= 5 && auto_reloads < 2 {
                    auto_reloads += 1;
                    stuck_cycles = 0;
                    let _ = webview.launch(LOGIN_URL, false).await;
                    self.foreground(webview).await;
                    tokio::time::sleep(Duration::from_secs(8)).await;
                    continue;
                }
            } else {
                stuck_cycles = 0;
            }

            let started = Instant::now();
            while started.elapsed() < POLL_INTERVAL {
                if webview.is_closed() {
                    return (None, YtSignInFailure::WindowClosed);
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }

        (None, YtSignInFailure::WindowClosed)
    }

    pub async fn read_delegated_page_id(webview: &dyn LoginWebviewSession) -> Option<String> {
        let raw = tokio::time::timeout(
            Duration::from_secs(8),
            webview.eval_js(
                "window.ytcfg ? String(window.ytcfg.data_.DELEGATED_SESSION_ID ?? \"\") : \"\"",
            ),
        )
        .await
        .ok()?
        .ok()??;
        Self::parse_delegated_page_id(&raw)
    }

    pub fn parse_delegated_page_id(raw: &str) -> Option<String> {
        static RE_PAGE_ID: OnceLock<Regex> = OnceLock::new();
        let unquoted = Self::unquote(raw);
        let v = unquoted.trim();
        if v.is_empty() || v == "null" || v == "undefined" {
            return None;
        }
        let re = RE_PAGE_ID.get_or_init(|| Regex::new(r"^\d{15,25}$").unwrap());
        if re.is_match(v) {
            Some(v.to_string())
        } else {
            None
        }
    }

    async fn page_text(webview: &dyn LoginWebviewSession) -> Option<String> {
        let raw = tokio::time::timeout(
            Duration::from_secs(4),
            webview.eval_js(
                "document.body ? document.body.innerText.slice(0, 1200) : \"\"",
            ),
        )
        .await
        .ok()?
        .ok()??;
        Some(Self::unquote(&raw))
    }

    async fn page_state(webview: &dyn LoginWebviewSession) -> YtPageState {
        let res = tokio::time::timeout(
            Duration::from_secs(4),
            webview.eval_js(
                "(window.ytcfg && window.ytcfg.data_ && \
                 (window.ytcfg.data_.LOGGED_IN === true || \
                 window.ytcfg.data_.LOGGED_IN === \"true\") ? \"1\" : \"0\") \
                 + \"|\" + window.location.href",
            ),
        )
        .await;
        let Ok(Ok(Some(raw))) = res else {
            return YtPageState {
                url: None,
                signed_in: None,
            };
        };
        Self::parse_page_state(&raw)
    }

    pub fn parse_page_state(raw: &str) -> YtPageState {
        let v = Self::unquote(raw);
        if v.is_empty() {
            return YtPageState {
                url: None,
                signed_in: None,
            };
        }
        let (flag, url) = match v.find('|') {
            Some(bar) => (v[..bar].trim(), v[bar + 1..].trim()),
            None => ("", v.trim()),
        };
        let signed_in = match flag {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        };
        YtPageState {
            url: if url.is_empty() {
                None
            } else {
                Some(url.to_string())
            },
            signed_in,
        }
    }

    pub fn unquote(raw: &str) -> String {
        let v = raw.trim();
        if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
            v[1..v.len() - 1].to_string()
        } else {
            v.to_string()
        }
    }

    pub fn is_passkey_verification_stall(page_text: &str) -> bool {
        let t = page_text.to_lowercase();
        t.contains("verifying") && t.contains("passkey")
    }

    pub fn is_browser_blocked_page(page_text: &str) -> bool {
        let t = page_text.to_lowercase();
        t.contains("may not be secure")
            || t.contains("disallowed_useragent")
            || t.contains("this browser or app")
            || t.contains("try using a different browser")
            || t.contains("couldn't sign you in")
            || t.contains("couldn’t sign you in")
    }

    pub fn is_blank_url(url: &str) -> bool {
        let u = url.trim().to_lowercase();
        u.is_empty() || u == "about:blank" || u.starts_with("data:,")
    }

    pub fn is_error_url(url: &str) -> bool {
        let u = url.trim().to_lowercase();
        u.starts_with("chrome-error:")
            || u.starts_with("edge-error:")
            || u.starts_with("webkit-error")
            || u.contains("chromewebdata")
    }

    pub fn is_acceptable_host(url: Option<&str>) -> bool {
        let Some(u) = url.map(str::trim).filter(|s| !s.is_empty()) else {
            return true;
        };
        let host = Url::parse(u)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_lowercase))
            .unwrap_or_default();
        host == "music.youtube.com" || host == "www.youtube.com"
    }

    fn keep_name(name: &str) -> bool {
        if name.starts_with("__Secure-")
            && (name.contains("PSID") || name.contains("PAPISID"))
        {
            return true;
        }
        matches!(
            name,
            "SID"
                | "HSID"
                | "SSID"
                | "APISID"
                | "SAPISID"
                | "LOGIN_INFO"
                | "VISITOR_INFO1_LIVE"
                | "VISITOR_DATA"
                | "__Secure-YEC"
                | "__Secure-YEN"
                | "SOCS"
                | "PREF"
        )
    }

    pub fn missing_marker_names<'a, I>(names: I) -> Vec<&'static str>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let set: HashSet<String> = names.into_iter().map(Self::norm_name).collect();
        let mut out = Vec::new();
        if !SAPISID_NAMES.iter().any(|n| set.contains(*n)) {
            out.push("SAPISID or __Secure-3PAPISID");
        }
        if !PSID_NAMES.iter().any(|n| set.contains(*n)) {
            out.push("SID or __Secure-3PSID");
        }
        if !LOGIN_NAMES.iter().any(|n| set.contains(*n))
            && !FRESH_NAMES.iter().any(|n| set.contains(*n))
        {
            out.push("LOGIN_INFO or __Secure-*PSIDTS");
        }
        out
    }

    fn domain_rank(domain: &str) -> i32 {
        let d = domain.trim().to_lowercase();
        let bare = d.strip_prefix('.').unwrap_or(&d);
        if bare.is_empty() {
            return 4;
        }
        if bare == "youtube.com" {
            return 0;
        }
        if bare.ends_with(".youtube.com") {
            return 1;
        }
        if bare == "google.com" {
            return 2;
        }
        if bare.ends_with(".google.com") {
            return 3;
        }
        -1
    }

    /// Builds a deterministic Cookie header from `cookies`, preferring
    /// YouTube-scoped duplicates over `.google.com` duplicates, or returns
    /// `None` if YouTube-scoped markers are incomplete.
    pub fn build_cookie_header(cookies: &[YtJarCookie]) -> Option<String> {
        let mut best: BTreeMap<String, (i32, String)> = BTreeMap::new();
        let mut has_youtube_scoped = false;

        for c in cookies {
            let name = Self::norm_name(&c.name);
            let value = Self::norm_value(&c.value);
            if name.is_empty() || value.is_empty() {
                continue;
            }
            let rank = Self::domain_rank(&c.domain);
            if rank < 0 {
                continue;
            }
            if rank <= 1 {
                has_youtube_scoped = true;
            }
            if !Self::keep_name(&name) {
                continue;
            }
            match best.get(&name) {
                Some((prev_rank, _)) if *prev_rank <= rank => {}
                _ => {
                    best.insert(name, (rank, value));
                }
            }
        }

        if best.is_empty() || !has_youtube_scoped {
            return None;
        }
        if !Self::missing_marker_names(best.keys().map(String::as_str)).is_empty() {
            return None;
        }
        Some(
            best.into_iter()
                .map(|(n, (_, v))| format!("{n}={v}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    pub fn norm_name(raw: &str) -> String {
        let mut s = raw.trim_end_matches('\0').trim();
        while let Some(stripped) = s.strip_suffix('=') {
            s = stripped;
        }
        s.to_string()
    }

    pub fn norm_value(raw: &str) -> String {
        raw.trim_end_matches('\0').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_cookie_header_prefers_youtube_scope_and_strips_nuls() {
        let cookies = vec![
            YtJarCookie {
                name: "SAPISID\0".to_string(),
                value: "google_sapi".to_string(),
                domain: "accounts.google.com".to_string(),
            },
            YtJarCookie {
                name: "SAPISID\0".to_string(),
                value: "yt_sapi".to_string(),
                domain: ".youtube.com".to_string(),
            },
            YtJarCookie {
                name: "__Secure-3PSID".to_string(),
                value: "psid3\0".to_string(),
                domain: ".youtube.com".to_string(),
            },
            YtJarCookie {
                name: "LOGIN_INFO".to_string(),
                value: "li=val==".to_string(),
                domain: ".youtube.com".to_string(),
            },
            YtJarCookie {
                name: "UNRELATED".to_string(),
                value: "drop".to_string(),
                domain: ".youtube.com".to_string(),
            },
        ];

        let header = YtWebLogin::build_cookie_header(&cookies).expect("complete session");
        assert_eq!(header, "LOGIN_INFO=li=val==; SAPISID=yt_sapi; __Secure-3PSID=psid3");
    }

    #[test]
    fn build_cookie_header_rejects_google_only_or_missing_markers() {
        let google_only = vec![
            YtJarCookie {
                name: "SAPISID".to_string(),
                value: "a".to_string(),
                domain: "accounts.google.com".to_string(),
            },
            YtJarCookie {
                name: "SID".to_string(),
                value: "b".to_string(),
                domain: ".google.com".to_string(),
            },
            YtJarCookie {
                name: "LOGIN_INFO".to_string(),
                value: "c".to_string(),
                domain: ".google.com".to_string(),
            },
        ];
        assert!(YtWebLogin::build_cookie_header(&google_only).is_none());

        let missing = YtWebLogin::missing_marker_names(["SAPISID"]);
        assert_eq!(missing.len(), 2);
    }

    #[test]
    fn page_checks_and_delegated_id_match_expected_rules() {
        assert!(YtWebLogin::is_passkey_verification_stall(
            "Verifying that it's you with a passkey..."
        ));
        assert!(YtWebLogin::is_browser_blocked_page(
            "This browser or app may not be secure."
        ));
        assert!(YtWebLogin::is_blank_url("about:blank"));
        assert!(YtWebLogin::is_error_url("chrome-error://chromewebdata/"));
        assert!(YtWebLogin::is_acceptable_host(Some("https://music.youtube.com/")));
        assert!(!YtWebLogin::is_acceptable_host(Some("https://accounts.google.com/")));
        assert_eq!(
            YtWebLogin::parse_delegated_page_id("\"108456789012345678901\""),
            Some("108456789012345678901".to_string())
        );
        assert_eq!(YtWebLogin::parse_delegated_page_id("\"null\""), None);
    }
}
