use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::storage::secure_store::SecureStore;

/// 15-minute refresh buffer before `po_token_expires_at` (mirrors Dart `isPoTokenStale`).
const PO_TOKEN_STALE_BUFFER_MS: i64 = 15 * 60 * 1000;

/// Persisted YouTube session bundle for instant headless boot.
///
/// Eliminates the need to spawn a WebView on startup when valid session
/// credentials and PO-Token are cached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSessionBundle {
    #[serde(default)]
    pub cookies: String,
    #[serde(default)]
    pub visitor_data: String,
    #[serde(default)]
    pub po_token: String,
    #[serde(default, deserialize_with = "deserialize_positive_ms")]
    pub po_token_expires_at: Option<i64>,
    #[serde(default)]
    pub data_sync_id: String,
    #[serde(default = "default_account_type")]
    pub account_type: String,
}

fn default_account_type() -> String {
    "guest".to_string()
}

fn deserialize_positive_ms<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    let Some(v) = val else {
        return Ok(None);
    };
    let ms = match v {
        serde_json::Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        _ => None,
    };
    Ok(ms.filter(|&m| m > 0))
}

impl Default for YtSessionBundle {
    fn default() -> Self {
        Self {
            cookies: String::new(),
            visitor_data: String::new(),
            po_token: String::new(),
            po_token_expires_at: None,
            data_sync_id: String::new(),
            account_type: default_account_type(),
        }
    }
}

impl YtSessionBundle {
    /// Returns `true` if `po_token` is empty, missing an expiration, or within 15 minutes of expiry.
    pub fn is_po_token_stale(&self) -> bool {
        self.is_po_token_stale_at(now_unix_ms())
    }

    /// Deterministic version of [`Self::is_po_token_stale`] evaluated at `now_ms`.
    pub fn is_po_token_stale_at(&self, now_ms: i64) -> bool {
        if self.po_token.is_empty() {
            return true;
        }
        let Some(exp) = self.po_token_expires_at else {
            return true;
        };
        let cutoff = exp.saturating_sub(PO_TOKEN_STALE_BUFFER_MS);
        now_ms > cutoff
    }

    /// Returns `true` if `po_token_expires_at` is missing or in the past.
    pub fn is_expired(&self) -> bool {
        self.is_expired_at(now_unix_ms())
    }

    /// Deterministic version of [`Self::is_expired`] evaluated at `now_ms`.
    pub fn is_expired_at(&self, now_ms: i64) -> bool {
        let Some(exp) = self.po_token_expires_at else {
            return true;
        };
        now_ms > exp
    }

    /// Returns `true` if cookies contain a `SAPISID` entry.
    pub fn is_signed_in(&self) -> bool {
        !self.cookies.is_empty() && self.cookies.contains("SAPISID")
    }

    /// Returns `true` when `account_type == "premium"`.
    pub fn is_premium(&self) -> bool {
        self.account_type == "premium"
    }
}

/// Disk persistence manager for [`YtSessionBundle`] backed by [`SecureStore`].
pub struct YtAuthStore {
    secure_store: Arc<SecureStore>,
    cached: Mutex<YtSessionBundle>,
}

impl YtAuthStore {
    pub const SESSION_KEY: &'static str = "yt_session_bundle";

    pub fn new(secure_store: Arc<SecureStore>) -> Self {
        Self {
            secure_store,
            cached: Mutex::new(YtSessionBundle::default()),
        }
    }

    /// Returns the currently cached [`YtSessionBundle`] in memory.
    pub fn current(&self) -> YtSessionBundle {
        self.cached
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Loads the persisted session bundle from [`SecureStore`] into memory.
    pub fn load(&self) -> YtSessionBundle {
        let loaded = self
            .secure_store
            .read(Self::SESSION_KEY)
            .filter(|raw| !raw.is_empty())
            .and_then(|raw| serde_json::from_str::<YtSessionBundle>(&raw).ok())
            .unwrap_or_default();

        if let Ok(mut guard) = self.cached.lock() {
            *guard = loaded.clone();
        }
        loaded
    }

    /// Persists `bundle` to [`SecureStore`] and updates the in-memory cache.
    pub fn save(&self, bundle: YtSessionBundle) {
        if let Ok(mut guard) = self.cached.lock() {
            *guard = bundle.clone();
        }
        if let Ok(json_str) = serde_json::to_string(&bundle) {
            self.secure_store.write(Self::SESSION_KEY, Some(&json_str));
        }
    }

    /// Updates only the BotGuard PO-Token and its expiration timestamp (ms since UNIX epoch).
    pub fn update_po_token(&self, po_token: String, expires_at_ms: i64) {
        let mut next = self.current();
        next.po_token = po_token;
        next.po_token_expires_at = Some(expires_at_ms);
        self.save(next);
    }

    /// Updates cookies and persists the bundle.
    pub fn update_cookies(&self, cookies: String) {
        let mut next = self.current();
        next.cookies = cookies;
        self.save(next);
    }

    /// Updates `visitor_data` and persists the bundle.
    pub fn update_visitor_data(&self, visitor_data: String) {
        let mut next = self.current();
        next.visitor_data = visitor_data;
        self.save(next);
    }

    /// Clears the stored session bundle on sign out.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.cached.lock() {
            *guard = YtSessionBundle::default();
        }
        self.secure_store.delete(Self::SESSION_KEY);
    }
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

    #[test]
    fn bundle_roundtrip_and_expiry_checks() {
        let store = Arc::new(SecureStore::in_memory());
        let auth = YtAuthStore::new(store);
        assert_eq!(auth.current(), YtSessionBundle::default());

        let bundle = YtSessionBundle {
            cookies: "SID=1; SAPISID=abc".to_string(),
            visitor_data: "Cgt...".to_string(),
            po_token: "po123".to_string(),
            po_token_expires_at: Some(2_000_000),
            data_sync_id: "sync".to_string(),
            account_type: "premium".to_string(),
        };
        assert!(bundle.is_signed_in());
        assert!(bundle.is_premium());
        assert!(!bundle.is_po_token_stale_at(1_000_000));
        assert!(bundle.is_po_token_stale_at(1_500_000));
        assert!(!bundle.is_expired_at(2_000_000));
        assert!(bundle.is_expired_at(2_000_001));

        auth.save(bundle.clone());
        let auth2 = YtAuthStore::new(Arc::clone(&auth.secure_store));
        assert_eq!(auth2.load(), bundle);

        auth.clear();
        assert_eq!(auth2.load(), YtSessionBundle::default());
    }
}
