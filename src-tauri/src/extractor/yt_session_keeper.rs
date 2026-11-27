use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::innertube_api::{InnerTubeMusicApi, YtConnection};
use super::yt_library_providers::YtLibraryProviders;
use super::yt_web_login::{YtWebLogin, SILENT_REFRESH_TIMEOUT};

struct RunningGuard<'a>(&'a AtomicBool);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Optional callback notified after a silent refresh verifies and persists a new cookie jar.
pub type CaptureSyncCallback = dyn Fn(&str, &str, &YtConnection) + Send + Sync;

/// Silent session maintenance for the YouTube Music connection.
///
/// Reuses the hidden sign-in WebView (or spins it up once on demand via
/// [`Self::auto_recover`]) so Google re-mints rotated `__Secure-*PSIDTS` cookies
/// against the persisted WebView2 profile without disrupting the user.
pub struct YtSessionKeeper {
    running: AtomicBool,
    auto_attempted: AtomicBool,
}

impl Default for YtSessionKeeper {
    fn default() -> Self {
        Self::new()
    }
}

impl YtSessionKeeper {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            auto_attempted: AtomicBool::new(false),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Whether this run already spent its automatic recovery attempt.
    pub fn auto_attempted(&self) -> bool {
        self.auto_attempted.load(Ordering::Acquire)
    }

    /// One automatic, zero-UI recovery per app run. Only the first call
    /// performs work; subsequent calls return `false` immediately.
    pub async fn auto_recover(
        &self,
        api: &Arc<InnerTubeMusicApi>,
        web_login: &Arc<YtWebLogin>,
        library_providers: Option<&YtLibraryProviders>,
        on_capture: Option<&CaptureSyncCallback>,
    ) -> bool {
        if self.auto_attempted.load(Ordering::Acquire) || self.running.load(Ordering::Acquire) {
            return false;
        }
        if self.auto_attempted.swap(true, Ordering::AcqRel) {
            return false;
        }
        self.refresh(api, web_login, library_providers, on_capture, true)
            .await
    }

    /// Attempts a zero-UI refresh. Returns `true` when the server accepted a
    /// freshly captured session and the stored jar was replaced.
    pub async fn refresh(
        &self,
        api: &Arc<InnerTubeMusicApi>,
        web_login: &Arc<YtWebLogin>,
        library_providers: Option<&YtLibraryProviders>,
        on_capture: Option<&CaptureSyncCallback>,
        allow_create_window: bool,
    ) -> bool {
        if self.running.load(Ordering::Acquire) {
            return false;
        }
        let conn = api.connection();
        if !conn.connected {
            return false;
        }
        if !allow_create_window && !web_login.has_cached_window().await {
            return false;
        }
        if self.running.swap(true, Ordering::AcqRel) {
            return false;
        }
        let _guard = RunningGuard(&self.running);

        let Some(header) = web_login
            .refresh_silently(SILENT_REFRESH_TIMEOUT, allow_create_window)
            .await
        else {
            return false;
        };
        if header.is_empty() {
            return false;
        }

        if api
            .connect_as(&header, &conn.profile_email, &conn.active_page_id)
            .await
            .is_err()
        {
            return false;
        }

        let updated_conn = api.connection();
        if let Some(cb) = on_capture {
            cb(&header, &conn.active_page_id, &updated_conn);
        }
        if let Some(providers) = library_providers {
            providers.invalidate_account_surfaces();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::http_client::HttpClientFactory;
    use crate::extractor::yt_web_login::{
        BoxFuture, LoginWebviewHost, LoginWebviewSession, YtJarCookie,
    };
    use crate::storage::secure_store::SecureStore;

    struct DummyHost;

    impl LoginWebviewHost for DummyHost {
        fn is_available(&self) -> BoxFuture<'_, bool> {
            Box::pin(async { false })
        }

        fn create_window(
            &self,
            _show: bool,
        ) -> BoxFuture<'_, Result<Arc<dyn LoginWebviewSession>, String>> {
            Box::pin(async { Err("unavailable".to_string()) })
        }
    }

    #[tokio::test]
    async fn auto_recover_marks_attempt_once_and_skips_when_disconnected() {
        let client = HttpClientFactory::create().expect("client");
        let secure = Arc::new(SecureStore::in_memory());
        let api = Arc::new(InnerTubeMusicApi::new(client, secure, None));
        let web_login = Arc::new(YtWebLogin::new(Arc::new(DummyHost)));
        let keeper = YtSessionKeeper::new();

        assert!(!keeper.auto_attempted());
        assert!(!keeper.is_running());

        let ok1 = keeper.auto_recover(&api, &web_login, None, None).await;
        assert!(!ok1);
        assert!(keeper.auto_attempted());
        assert!(!keeper.is_running());

        // Second call short-circuits cleanly.
        let ok2 = keeper.auto_recover(&api, &web_login, None, None).await;
        assert!(!ok2);
        let _ = YtJarCookie::default();
    }
}
