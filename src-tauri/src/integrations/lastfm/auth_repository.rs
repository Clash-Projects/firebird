//! Last.fm authentication repository.
//!
//! Ported from `lib/features/lastfm/auth_repository.dart`:
//! - web auth via `https://www.last.fm/api/auth/?api_key=..&token=..`
//! - `auth.getToken` / `auth.getSession` exchange
//! - guest mode (`AuthStatus::Guest`)
//! - sign-out preserving API credentials

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::extractor::auth_crypto::AuthCrypto;
use crate::integrations::lastfm::lastfm_api::{LastFmApiError, LastFmApiService};
use crate::storage::prefs::Prefs;
use crate::storage::secure_store::SecureStore;

/// Last.fm authentication state.
/// `Guest` = keyless entry via the welcome Skip button: inside the
/// shell with no keys and no session; Last.fm features stay off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AuthStatus {
    #[default]
    Unknown,
    SignedOut,
    SigningIn,
    SignedIn,
    Guest,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    pub status: AuthStatus,
    pub username: String,
    pub message: String,
}

/// Pending web-auth handshake (token obtained, awaiting approval).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebAuthHandshake {
    pub token: String,
    pub url: String,
}

pub struct AuthRepository {
    api: Arc<LastFmApiService>,
    prefs: Arc<Prefs>,
    secure: Arc<SecureStore>,
    state: RwLock<AuthState>,
}

impl AuthRepository {
    pub fn new(api: Arc<LastFmApiService>, prefs: Arc<Prefs>, secure: Arc<SecureStore>) -> Self {
        let initial = Self::restore_state(&prefs);
        Self {
            api,
            prefs,
            secure,
            state: RwLock::new(initial),
        }
    }

    fn restore_state(prefs: &Prefs) -> AuthState {
        let username = prefs.username();
        if !username.is_empty() {
            AuthState {
                status: AuthStatus::SignedIn,
                username,
                message: String::new(),
            }
        } else if prefs.is_guest() {
            AuthState {
                status: AuthStatus::Guest,
                username: String::new(),
                message: String::new(),
            }
        } else {
            AuthState {
                status: AuthStatus::SignedOut,
                username: String::new(),
                message: String::new(),
            }
        }
    }

    pub async fn state(&self) -> AuthState {
        self.state.read().await.clone()
    }

    /// Keyless entry: enter the shell without Last.fm keys or session.
    /// Sticky via Prefs; cleared by full sign-out and by real sessions.
    pub async fn enter_guest_mode(&self) {
        self.prefs.set_guest_mode(true);
        let mut st = self.state.write().await;
        *st = AuthState {
            status: AuthStatus::Guest,
            username: String::new(),
            message: String::new(),
        };
    }

    /// True when custom API keys are present. Web auth and all signed
    /// calls require this; repositories no-op otherwise.
    pub fn is_configured(&self) -> bool {
        self.prefs.is_last_fm_configured()
    }

    /// Save user-supplied API keys (Settings / welcome share this path).
    ///
    /// Session keys are bound to the API key that minted them, so saving
    /// *different* keys signs the current session out first. The
    /// candidate keys are validated with a signed `auth.getToken` call
    /// and persisted only on success.
    pub async fn save_custom_keys(
        &self,
        api_key: &str,
        api_secret: &str,
    ) -> Result<(), LastFmApiError> {
        let key = api_key.trim().to_string();
        let secret = api_secret.trim().to_string();
        if key.is_empty() || secret.is_empty() {
            {
                let mut st = self.state.write().await;
                st.status = AuthStatus::Error;
                st.message = "Enter both the API key and the shared secret.".to_string();
            }
            return Err(LastFmApiError::Api {
                code: None,
                message: "Incomplete Last.fm API keys".to_string(),
            });
        }

        if key != self.prefs.last_fm_api_key() || secret != self.prefs.last_fm_api_secret() {
            let was_guest = self.prefs.is_guest();
            self.prefs.sign_out();
            self.secure.write_session_key(None);
            if was_guest {
                self.prefs.set_guest_mode(true);
            }
            let mut st = self.state.write().await;
            *st = AuthState {
                status: if was_guest {
                    AuthStatus::Guest
                } else {
                    AuthStatus::SignedOut
                },
                username: String::new(),
                message: String::new(),
            };
        }

        {
            let mut st = self.state.write().await;
            st.status = AuthStatus::SigningIn;
            st.message.clear();
        }

        let mut params = BTreeMap::new();
        params.insert("method".to_string(), "auth.getToken".to_string());
        params.insert("api_key".to_string(), key.clone());
        let sig = AuthCrypto::api_sig(
            params.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            &secret,
        );
        let mut signed = params;
        signed.insert("api_sig".to_string(), sig);
        signed.insert("format".to_string(), "json".to_string());

        let res = async {
            let json = self.api.get(&signed).await?;
            LastFmApiError::throw_if_error(&json)?;
            let token = json.get("token").and_then(Value::as_str).unwrap_or("");
            if token.is_empty() {
                return Err(LastFmApiError::Api {
                    code: None,
                    message: "Last.fm rejected these API keys".to_string(),
                });
            }
            self.prefs.save_last_fm_keys(&key, &secret);
            let mut st = self.state.write().await;
            *st = AuthState {
                status: if self.prefs.is_guest() {
                    AuthStatus::Guest
                } else {
                    AuthStatus::SignedOut
                },
                username: String::new(),
                message: String::new(),
            };
            Ok(())
        }
        .await;

        if let Err(ref e) = res {
            let mut st = self.state.write().await;
            st.status = AuthStatus::Error;
            st.message = e.to_string();
        }
        res
    }

    /// Clear the custom API keys (Settings). Also signs out: sessions
    /// cannot survive without keys. Preserves guest mode when already inside.
    pub async fn clear_custom_keys(&self) {
        let was_guest = self.prefs.is_guest();
        self.sign_out().await;
        self.prefs.clear_last_fm_keys();
        if was_guest {
            self.prefs.set_guest_mode(true);
            let mut st = self.state.write().await;
            *st = AuthState {
                status: AuthStatus::Guest,
                username: String::new(),
                message: String::new(),
            };
        }
    }

    /// Step 1 of web auth: fetch a token and build the approval URL.
    pub async fn begin_web_auth(&self) -> Result<WebAuthHandshake, LastFmApiError> {
        if !self.is_configured() {
            {
                let mut st = self.state.write().await;
                st.status = AuthStatus::Error;
                st.message = "Enter your Last.fm API keys first.".to_string();
            }
            return Err(LastFmApiError::Api {
                code: None,
                message: "Last.fm API keys not set".to_string(),
            });
        }

        {
            let mut st = self.state.write().await;
            st.status = AuthStatus::SigningIn;
            st.message.clear();
        }

        let api_key = self.prefs.last_fm_api_key();
        let api_secret = self.prefs.last_fm_api_secret();
        let mut params = BTreeMap::new();
        params.insert("method".to_string(), "auth.getToken".to_string());
        params.insert("api_key".to_string(), api_key.clone());
        let sig = AuthCrypto::api_sig(
            params.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            &api_secret,
        );
        let mut signed = params;
        signed.insert("api_sig".to_string(), sig);
        signed.insert("format".to_string(), "json".to_string());

        let res = async {
            let json = self.api.get(&signed).await?;
            LastFmApiError::throw_if_error(&json)?;
            let token = json
                .get("token")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if token.is_empty() {
                return Err(LastFmApiError::Api {
                    code: None,
                    message: "Could not start Last.fm approval.".to_string(),
                });
            }
            let url = format!("https://www.last.fm/api/auth/?api_key={api_key}&token={token}");
            Ok(WebAuthHandshake { token, url })
        }
        .await;

        if let Err(ref e) = res {
            let mut st = self.state.write().await;
            st.status = AuthStatus::Error;
            st.message = e.to_string();
        }
        res
    }

    /// Step 2: exchange the approved token for a session key.
    pub async fn complete_web_auth(&self, token: &str) -> Result<(), LastFmApiError> {
        let api_key = self.prefs.last_fm_api_key();
        let api_secret = self.prefs.last_fm_api_secret();
        let mut params = BTreeMap::new();
        params.insert("method".to_string(), "auth.getSession".to_string());
        params.insert("api_key".to_string(), api_key);
        params.insert("token".to_string(), token.to_string());
        let sig = AuthCrypto::api_sig(
            params.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            &api_secret,
        );
        let mut signed = params;
        signed.insert("api_sig".to_string(), sig);
        signed.insert("format".to_string(), "json".to_string());

        let res = async {
            let json = self.api.get(&signed).await?;
            LastFmApiError::throw_if_error(&json)?;
            let session = json.get("session").and_then(Value::as_object);
            let key = session
                .and_then(|s| s.get("key"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let name = session
                .and_then(|s| s.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if key.is_empty() || name.is_empty() {
                return Err(LastFmApiError::Api {
                    code: None,
                    message: "Approval not completed yet".to_string(),
                });
            }
            self.prefs.save_session(&key, &name);
            self.secure.write_session_key(Some(&key));
            let mut st = self.state.write().await;
            *st = AuthState {
                status: AuthStatus::SignedIn,
                username: name,
                message: String::new(),
            };
            Ok(())
        }
        .await;

        if let Err(ref e) = res {
            let mut st = self.state.write().await;
            st.status = AuthStatus::Error;
            st.message = e.to_string();
        }
        res
    }

    pub async fn sign_out(&self) {
        self.prefs.sign_out();
        self.prefs.set_guest_mode(false);
        self.secure.write_session_key(None);
        let mut st = self.state.write().await;
        *st = AuthState {
            status: AuthStatus::SignedOut,
            username: String::new(),
            message: String::new(),
        };
    }

    pub async fn clear_error(&self) {
        let mut st = self.state.write().await;
        if st.status == AuthStatus::Error {
            st.status = if !self.prefs.username().is_empty() {
                AuthStatus::SignedIn
            } else if self.prefs.is_guest() {
                AuthStatus::Guest
            } else {
                AuthStatus::SignedOut
            };
            st.message.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::lastfm::rate_guard::LastFmRateGuard;

    #[tokio::test]
    async fn guest_mode_and_sign_out_transitions() {
        let prefs = Arc::new(Prefs::in_memory());
        let secure = Arc::new(SecureStore::in_memory());
        let api = Arc::new(LastFmApiService::new(Arc::new(LastFmRateGuard::new())));
        let repo = AuthRepository::new(api, prefs, secure);

        assert_eq!(repo.state().await.status, AuthStatus::SignedOut);
        repo.enter_guest_mode().await;
        assert_eq!(repo.state().await.status, AuthStatus::Guest);
        repo.sign_out().await;
        assert_eq!(repo.state().await.status, AuthStatus::SignedOut);
    }
}
