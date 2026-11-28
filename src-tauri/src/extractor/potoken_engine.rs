use std::sync::Arc;

use reqwest::Client;

use super::ephemeral_webview_broker::{BotGuardWebviewHost, EphemeralWebViewBroker};
use super::yt_auth_store::YtAuthStore;

/// Minted BotGuard Proof-of-Origin tokens for YouTube playback and session binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoTokenResult {
    pub player_token: String,
    pub session_token: String,
}

/// Desktop BotGuard `poToken` engine powered by [`EphemeralWebViewBroker`].
///
/// Startup does not spawn a WebView; it loads the persisted session bundle via
/// [`YtAuthStore`] and only runs off-screen bursts on expired tokens, login, or 403 recovery.
pub struct PoTokenEngine {
    broker: EphemeralWebViewBroker,
    auth_store: Option<Arc<YtAuthStore>>,
}

impl PoTokenEngine {
    pub fn new(
        client: Client,
        auth_store: Option<Arc<YtAuthStore>>,
        host: Option<Arc<dyn BotGuardWebviewHost>>,
    ) -> Self {
        let broker = EphemeralWebViewBroker::new(client, auth_store.clone(), host);
        Self { broker, auth_store }
    }

    pub fn is_enabled(&self) -> bool {
        self.broker.is_enabled()
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.broker.set_enabled(enabled);
    }

    pub fn set_host(&self, host: Arc<dyn BotGuardWebviewHost>) {
        self.broker.set_host(host);
    }

    pub fn auth_store(&self) -> Option<&Arc<YtAuthStore>> {
        self.auth_store.as_ref()
    }

    /// Startup pre-warm without spawning a WebView: loads persisted session state into memory.
    pub fn pre_warm(&self) {
        if let Some(store) = &self.auth_store {
            let _ = store.load();
        }
    }

    /// Mints a fresh BotGuard token via an ephemeral burst if needed, or returns the cached token.
    pub async fn mint_token(
        &self,
        video_id: &str,
        session_id: &str,
        force_refresh: bool,
    ) -> Option<PoTokenResult> {
        if !self.is_enabled() {
            return None;
        }
        let res = self
            .broker
            .mint_token(video_id, session_id, force_refresh)
            .await?;
        Some(PoTokenResult {
            player_token: res.player_token,
            session_token: res.session_token,
        })
    }

    /// Releases any held resources (ephemeral bursts already tear down after each capture).
    pub async fn dispose(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::http_client::HttpClientFactory;
    use crate::storage::secure_store::SecureStore;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn pre_warm_loads_cached_po_token_without_webview() {
        let store = Arc::new(YtAuthStore::new(Arc::new(SecureStore::in_memory())));
        let future_exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
            + 3_600_000;
        store.update_po_token("warm_tok".to_string(), future_exp);

        let client = HttpClientFactory::create().expect("client");
        let engine = PoTokenEngine::new(client, Some(store), None);
        engine.pre_warm();

        let minted = engine
            .mint_token("vid999", "lastwave_session", false)
            .await
            .expect("cached token");
        assert_eq!(minted.player_token, "warm_tok");
        assert_eq!(minted.session_token, "warm_tok");
    }
}
