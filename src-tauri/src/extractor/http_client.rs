use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::{Client, Response, StatusCode};
use thiserror::Error;

/// Desktop User-Agent string shared across LastWave HTTP clients.
pub const DESKTOP_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0 LastWave-Desktop/1.0";

/// Default connect and request timeout matching `DioFactory`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// Default maximum redirect hops.
pub const DEFAULT_MAX_REDIRECTS: usize = 10;

/// Cooldown applied when a rate-limited response (429/503) is encountered.
pub const DEFAULT_RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(2);

/// Delay before retrying once on a rate-limited response.
pub const RATE_LIMIT_RETRY_DELAY: Duration = Duration::from_millis(1500);

/// Errors returned by HTTP client construction or retry helpers.
#[derive(Debug, Error)]
pub enum HttpClientError {
    #[error("failed to build HTTP client: {0}")]
    Build(#[from] reqwest::Error),

    #[error("invalid header value for {name}: {source}")]
    InvalidHeader {
        name: &'static str,
        #[source]
        source: reqwest::header::InvalidHeaderValue,
    },
}

/// Configuration for building a shared `reqwest::Client`.
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    pub user_agent: String,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub max_redirects: usize,
    pub cookie_store: bool,
    pub accept: String,
    pub accept_language: String,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            user_agent: DESKTOP_USER_AGENT.to_string(),
            connect_timeout: DEFAULT_TIMEOUT,
            request_timeout: DEFAULT_TIMEOUT,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            cookie_store: false,
            accept: "application/json, text/plain, */*".to_string(),
            accept_language: "en-US,en;q=0.9".to_string(),
        }
    }
}

/// Thread-safe cooldown tracker for rate-limited APIs (e.g. Last.fm 429/503).
#[derive(Debug, Clone, Default)]
pub struct RateGuard {
    cooldown_until: Arc<Mutex<Option<Instant>>>,
}

impl RateGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` while the guard is actively cooling down.
    pub fn is_cooling_down(&self) -> bool {
        self.remaining() > Duration::ZERO
    }

    /// Remaining cooldown duration, or `Duration::ZERO` if clear.
    pub fn remaining(&self) -> Duration {
        let Ok(guard) = self.cooldown_until.lock() else {
            return Duration::ZERO;
        };
        let Some(until) = *guard else {
            return Duration::ZERO;
        };
        until.saturating_duration_since(Instant::now())
    }

    /// Extends the cooldown window by `extra`.
    pub fn on_request_limited(&self, extra: Duration) {
        let Ok(mut guard) = self.cooldown_until.lock() else {
            return;
        };
        let now = Instant::now();
        let base = match *guard {
            Some(until) if until > now => until,
            _ => now,
        };
        *guard = Some(base + extra);
    }

    /// Clears the cooldown window after a successful request.
    pub fn on_request_succeeded(&self) {
        if let Ok(mut guard) = self.cooldown_until.lock() {
            *guard = None;
        }
    }

    /// Sleeps asynchronously until any active cooldown expires.
    pub async fn await_clearance(&self) {
        let wait = self.remaining();
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

/// Factory for constructing configured `reqwest::Client` instances and retrying transient calls.
pub struct HttpClientFactory;

impl HttpClientFactory {
    /// Builds a `reqwest::Client` using the default desktop configuration.
    pub fn create() -> Result<Client, HttpClientError> {
        Self::create_with(&HttpClientConfig::default())
    }

    /// Builds a `reqwest::Client` from a custom [`HttpClientConfig`].
    pub fn create_with(config: &HttpClientConfig) -> Result<Client, HttpClientError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&config.user_agent).map_err(|source| {
                HttpClientError::InvalidHeader {
                    name: "User-Agent",
                    source,
                }
            })?,
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_str(&config.accept).map_err(|source| {
                HttpClientError::InvalidHeader {
                    name: "Accept",
                    source,
                }
            })?,
        );
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_str(&config.accept_language).map_err(|source| {
                HttpClientError::InvalidHeader {
                    name: "Accept-Language",
                    source,
                }
            })?,
        );

        let redirect_policy = if config.max_redirects == 0 {
            Policy::none()
        } else {
            Policy::limited(config.max_redirects)
        };

        let mut builder = Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .redirect(redirect_policy)
            .default_headers(headers);

        if config.cookie_store {
            builder = builder.cookie_store(true);
        }

        builder.build().map_err(HttpClientError::Build)
    }

    /// Executes an async HTTP request factory with exponential backoff for transient failures
    /// (408/429/5xx, timeouts, and connection errors), matching `DioFactory.withRetry`.
    pub async fn with_retry<T, E, F, Fut>(
        max_attempts: usize,
        mut op: F,
    ) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: TransientError,
    {
        let attempts = max_attempts.max(1);
        let mut attempt = 1;
        loop {
            match op().await {
                Ok(val) => return Ok(val),
                Err(err) => {
                    if !err.is_transient() || attempt >= attempts {
                        return Err(err);
                    }
                    let shift = (attempt - 1).min(10) as u32;
                    let backoff = Duration::from_millis(250 * (1u64 << shift));
                    tokio::time::sleep(backoff).await;
                    attempt += 1;
                }
            }
        }
    }

    /// Executes a request builder factory while honoring a [`RateGuard`], updating cooldowns
    /// and retrying once after [`RATE_LIMIT_RETRY_DELAY`] on HTTP 429 or 503.
    pub async fn send_with_rate_guard<F>(
        rate_guard: &RateGuard,
        mut build_request: F,
    ) -> Result<Response, reqwest::Error>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        rate_guard.await_clearance().await;
        let response = build_request().send().await?;
        let status = response.status();
        if is_rate_limited_status(status) {
            rate_guard.on_request_limited(DEFAULT_RATE_LIMIT_COOLDOWN);
            tokio::time::sleep(RATE_LIMIT_RETRY_DELAY).await;
            let retry_response = build_request().send().await?;
            if is_rate_limited_status(retry_response.status()) {
                rate_guard.on_request_limited(DEFAULT_RATE_LIMIT_COOLDOWN);
            } else {
                rate_guard.on_request_succeeded();
            }
            return Ok(retry_response);
        }
        rate_guard.on_request_succeeded();
        Ok(response)
    }

    /// Suppresses noisy debug error logging for known benign upstream statuses.
    pub fn should_suppress_error_log(status: Option<u16>, host: &str) -> bool {
        match status {
            Some(403) if host.contains("itunes.apple.com") => true,
            Some(404) if host.contains("lrclib.net") => true,
            Some(404) if host.contains("artwork.m8tec.top") => true,
            _ => false,
        }
    }
}

/// Trait implemented by errors that can be classified as transient for [`HttpClientFactory::with_retry`].
pub trait TransientError {
    fn is_transient(&self) -> bool;
}

impl TransientError for reqwest::Error {
    fn is_transient(&self) -> bool {
        if self.is_connect() || self.is_timeout() {
            return true;
        }
        self.status()
            .is_some_and(|code| is_transient_status(code.as_u16()))
    }
}

/// Returns `true` for HTTP status codes considered transient (408, 429, 5xx).
pub fn is_transient_status(code: u16) -> bool {
    code == 408 || code == 429 || code >= 500
}

/// Returns `true` for HTTP 429 or 503 responses.
pub fn is_rate_limited_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE
}
