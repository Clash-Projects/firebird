use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::future::{FutureExt, Shared};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::extractor::auth_crypto::AuthCrypto;
use crate::extractor::http_client::HttpClientFactory;

use super::rate_guard::{last_fm_friendly_message, LastFmRateGuard};

pub const LASTFM_BASE_URL: &str = "https://ws.audioscrobbler.com/";
const LASTFM_ENDPOINT: &str = "https://ws.audioscrobbler.com/2.0/";
const MEMO_CAP: usize = 200;

#[derive(Debug, Clone, Error)]
pub enum LastFmApiError {
    #[error("LastFmException({code:?}): {message}")]
    Api { code: Option<i32>, message: String },
    #[error("LastFmHttpError({status:?}): {message}")]
    Http { status: Option<u16>, message: String },
    #[error("LastFmDecodeError: {0}")]
    Decode(String),
}

impl LastFmApiError {
    /// Checks for `{"error": n, "message": ...}` and returns a friendly error.
    pub fn throw_if_error(json: &Value) -> Result<(), Self> {
        let Some(obj) = json.as_object() else {
            return Ok(());
        };
        if !obj.contains_key("error") {
            return Ok(());
        }
        let code = obj.get("error").and_then(Value::as_i64).map(|n| n as i32);
        let raw = obj
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Last.fm error");
        Err(Self::Api {
            code,
            message: last_fm_friendly_message(code, raw),
        })
    }
}

type SharedFetch = Shared<Pin<Box<dyn Future<Output = Result<Value, LastFmApiError>> + Send>>>;

struct MemoEntry {
    at: Instant,
    future: SharedFetch,
}

#[derive(Default)]
struct MemoCache {
    map: HashMap<String, MemoEntry>,
    order: VecDeque<String>,
}

impl MemoCache {
    fn touch(&mut self, key: &str, ttl: Duration, now: Instant) -> Option<SharedFetch> {
        let entry = self.map.remove(key)?;
        self.order.retain(|k| k != key);
        if now.duration_since(entry.at) < ttl {
            let fut = entry.future.clone();
            self.map.insert(key.to_string(), entry);
            self.order.push_back(key.to_string());
            Some(fut)
        } else {
            None
        }
    }

    fn insert(&mut self, key: String, at: Instant, future: SharedFetch) {
        self.map.remove(&key);
        self.order.retain(|k| k != &key);
        self.map.insert(key.clone(), MemoEntry { at, future });
        self.order.push_back(key);
        while self.map.len() > MEMO_CAP {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            } else {
                break;
            }
        }
    }

    fn remove_if_same(&mut self, key: &str, target: &SharedFetch) {
        let matches = self
            .map
            .get(key)
            .is_some_and(|e| e.future.ptr_eq(target));
        if matches {
            self.map.remove(key);
            self.order.retain(|k| k != key);
        }
    }
}

/// Thin Last.fm `2.0/` client.
///
/// Unsigned reads via GET (with per-method TTL memoization and in-flight
/// deduplication), signed writes via form POST, all calls multiplexed through
/// `method` on `https://ws.audioscrobbler.com/2.0/`.
pub struct LastFmApiService {
    client: Client,
    rate_guard: Arc<LastFmRateGuard>,
    memo: Arc<Mutex<MemoCache>>,
    endpoint: String,
}

impl LastFmApiService {
    pub fn new(rate_guard: Arc<LastFmRateGuard>) -> Self {
        let client = HttpClientFactory::create().unwrap_or_else(|_| Client::new());
        Self {
            client,
            rate_guard,
            memo: Arc::new(Mutex::new(MemoCache::default())),
            endpoint: LASTFM_ENDPOINT.to_string(),
        }
    }

    pub fn with_client(client: Client, rate_guard: Arc<LastFmRateGuard>) -> Self {
        Self {
            client,
            rate_guard,
            memo: Arc::new(Mutex::new(MemoCache::default())),
            endpoint: LASTFM_ENDPOINT.to_string(),
        }
    }

    fn ttl_for(method: &str) -> Option<Duration> {
        match method.to_ascii_lowercase().as_str() {
            "artist.getinfo"
            | "artist.getsimilar"
            | "artist.gettoptracks"
            | "artist.gettoptags"
            | "track.getsimilar"
            | "tag.gettoptracks" => Some(Duration::from_secs(6 * 3600)),
            "user.gettoptracks"
            | "user.gettopartists"
            | "user.gettopalbums"
            | "user.gettoptags"
            | "user.getinfo" => Some(Duration::from_secs(2 * 60)),
            _ => None,
        }
    }

    /// Unsigned read, replayed from memory for TTL-eligible methods. Shares
    /// an in-flight request, and never keeps a failure or an API error body.
    pub async fn get(&self, query: &BTreeMap<String, String>) -> Result<Value, LastFmApiError> {
        let method = query.get("method").map(String::as_str).unwrap_or_default();
        let Some(ttl) = Self::ttl_for(method) else {
            return self.fetch(query.clone()).await;
        };

        let key = query
            .iter()
            .map(|(k, v)| {
                if k == "method" {
                    format!("{k}={}", v.to_ascii_lowercase())
                } else {
                    format!("{k}={v}")
                }
            })
            .collect::<Vec<_>>()
            .join("&");

        let (shared, is_owner) = {
            let mut memo = self.memo.lock().await;
            let now = Instant::now();
            if let Some(hit) = memo.touch(&key, ttl, now) {
                (hit, false)
            } else {
                let client = self.client.clone();
                let rate_guard = Arc::clone(&self.rate_guard);
                let endpoint = self.endpoint.clone();
                let q = query.clone();
                let boxed: Pin<Box<dyn Future<Output = Result<Value, LastFmApiError>> + Send>> =
                    Box::pin(async move { fetch_get(&client, &rate_guard, &endpoint, q).await });
                let fut: SharedFetch = boxed.shared();
                memo.insert(key.clone(), now, fut.clone());
                (fut, true)
            }
        };

        let res = shared.clone().await;
        if is_owner {
            let should_evict = match &res {
                Err(_) => true,
                Ok(val) => val.as_object().is_some_and(|m| m.contains_key("error")),
            };
            if should_evict {
                let mut memo = self.memo.lock().await;
                memo.remove_if_same(&key, &shared);
            }
        }
        res
    }

    async fn fetch(&self, query: BTreeMap<String, String>) -> Result<Value, LastFmApiError> {
        fetch_get(&self.client, &self.rate_guard, &self.endpoint, query).await
    }

    /// Form POST to Last.fm `2.0/`.
    pub async fn post(&self, fields: &BTreeMap<String, String>) -> Result<Value, LastFmApiError> {
        self.rate_guard.await_clearance().await;
        let resp = self
            .client
            .post(&self.endpoint)
            .form(fields)
            .send()
            .await
            .map_err(|e| LastFmApiError::Http {
                status: e.status().map(|s| s.as_u16()),
                message: e.to_string(),
            })?;

        let status = resp.status();
        if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE {
            self.rate_guard.on_request_limited_default();
            return Err(LastFmApiError::Http {
                status: Some(status.as_u16()),
                message: format!("HTTP {}", status.as_u16()),
            });
        }
        if status.as_u16() >= 500 {
            return Err(LastFmApiError::Http {
                status: Some(status.as_u16()),
                message: format!("HTTP {}", status.as_u16()),
            });
        }

        self.rate_guard.on_request_succeeded();
        let body = resp.text().await.map_err(|e| LastFmApiError::Http {
            status: Some(status.as_u16()),
            message: e.to_string(),
        })?;
        decode_lastfm_body(&body)
    }

    /// Signs `params` with `AuthCrypto::api_sig` and sends a form POST.
    pub async fn post_signed(
        &self,
        params: &BTreeMap<String, String>,
        api_key: &str,
        api_secret: &str,
    ) -> Result<Value, LastFmApiError> {
        let mut signed = params.clone();
        signed.insert("api_key".to_string(), AuthCrypto::normalize_key(api_key));
        let secret = AuthCrypto::normalize_key(api_secret);
        let sig = AuthCrypto::api_sig(
            signed.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            &secret,
        );
        signed.insert("api_sig".to_string(), sig);
        signed.insert("format".to_string(), "json".to_string());
        self.post(&signed).await
    }
}

async fn fetch_get(
    client: &Client,
    rate_guard: &LastFmRateGuard,
    endpoint: &str,
    query: BTreeMap<String, String>,
) -> Result<Value, LastFmApiError> {
    rate_guard.await_clearance().await;
    let mut params: Vec<(String, String)> = Vec::with_capacity(query.len() + 1);
    params.push(("format".to_string(), "json".to_string()));
    for (k, v) in query {
        params.push((k, v));
    }

    let resp = client
        .get(endpoint)
        .query(&params)
        .send()
        .await
        .map_err(|e| LastFmApiError::Http {
            status: e.status().map(|s| s.as_u16()),
            message: e.to_string(),
        })?;

    let status = resp.status();
    if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE {
        rate_guard.on_request_limited_default();
        return Err(LastFmApiError::Http {
            status: Some(status.as_u16()),
            message: format!("HTTP {}", status.as_u16()),
        });
    }
    if status.as_u16() >= 500 {
        return Err(LastFmApiError::Http {
            status: Some(status.as_u16()),
            message: format!("HTTP {}", status.as_u16()),
        });
    }

    rate_guard.on_request_succeeded();
    let body = resp.text().await.map_err(|e| LastFmApiError::Http {
        status: Some(status.as_u16()),
        message: e.to_string(),
    })?;
    decode_lastfm_body(&body)
}

fn decode_lastfm_body(body: &str) -> Result<Value, LastFmApiError> {
    if body.is_empty() {
        return Ok(json!({}));
    }
    let decoded: Value =
        serde_json::from_str(body).map_err(|e| LastFmApiError::Decode(e.to_string()))?;
    if decoded.is_object() {
        Ok(decoded)
    } else {
        Ok(json!({ "value": decoded }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throw_if_error_maps_known_codes() {
        assert!(LastFmApiError::throw_if_error(&json!({ "artist": {} })).is_ok());

        let err = LastFmApiError::throw_if_error(&json!({
            "error": 9,
            "message": "Invalid session key"
        }))
        .unwrap_err();
        match err {
            LastFmApiError::Api { code, message } => {
                assert_eq!(code, Some(9));
                assert_eq!(message, "Invalid session — reconnect Last.fm.");
            }
            _ => panic!("expected Api error"),
        }
    }

    #[test]
    fn decode_body_handles_empty_and_non_object() {
        assert_eq!(decode_lastfm_body("").unwrap(), json!({}));
        assert_eq!(decode_lastfm_body("[1,2]").unwrap(), json!({ "value": [1, 2] }));
    }
}
