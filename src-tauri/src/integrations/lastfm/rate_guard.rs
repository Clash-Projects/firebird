use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Last.fm rate-limit guard.
///
/// A short cooldown is applied after HTTP 429/503 so clients back off
/// instead of hammering the API.
#[derive(Debug, Default)]
pub struct LastFmRateGuard {
    cooldown_until: Mutex<Option<Instant>>,
}

impl LastFmRateGuard {
    pub fn new() -> Self {
        Self {
            cooldown_until: Mutex::new(None),
        }
    }

    pub fn is_cooling_down(&self) -> bool {
        let guard = self.cooldown_until.lock().expect("rate guard mutex poisoned");
        matches!(*guard, Some(until) if Instant::now() < until)
    }

    pub fn remaining(&self) -> Duration {
        let guard = self.cooldown_until.lock().expect("rate guard mutex poisoned");
        let Some(until) = *guard else {
            return Duration::ZERO;
        };
        until.saturating_duration_since(Instant::now())
    }

    pub fn on_request_limited(&self, extra: Duration) {
        let now = Instant::now();
        let mut guard = self.cooldown_until.lock().expect("rate guard mutex poisoned");
        let base = match *guard {
            Some(until) if until > now => until,
            _ => now,
        };
        *guard = Some(base + extra);
    }

    pub fn on_request_limited_default(&self) {
        self.on_request_limited(Duration::from_secs(2));
    }

    pub fn on_request_succeeded(&self) {
        let mut guard = self.cooldown_until.lock().expect("rate guard mutex poisoned");
        *guard = None;
    }

    pub async fn await_clearance(&self) {
        let wait = self.remaining();
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

/// Maps Last.fm error codes to friendly messages.
pub fn last_fm_friendly_message(code: Option<i32>, raw: &str) -> String {
    match code {
        Some(2) => "Invalid service — please try again later.".to_string(),
        Some(3) => "Invalid method — this build may be outdated.".to_string(),
        Some(4) => "Authentication failed — reconnect Last.fm.".to_string(),
        Some(5) => "Invalid API signature — reconnect Last.fm.".to_string(),
        Some(6) => "This track is not loved on Last.fm.".to_string(),
        Some(7) => "Invalid session key — reconnect Last.fm.".to_string(),
        Some(8) => "Operation failed — please retry.".to_string(),
        Some(9) => "Invalid session — reconnect Last.fm.".to_string(),
        Some(10) => "Invalid API key.".to_string(),
        Some(11) => "Service is offline — try again later.".to_string(),
        Some(13) => "Invalid method signature.".to_string(),
        Some(14) => "Unauthorized token — approve access again.".to_string(),
        Some(15) => "This app is temporarily suspended.".to_string(),
        Some(16) => "Service is down for maintenance.".to_string(),
        Some(26) => "Suspended API key.".to_string(),
        Some(29) => "Rate limited — backing off.".to_string(),
        _ => {
            if raw.is_empty() {
                "Last.fm request failed.".to_string()
            } else {
                raw.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_guard_cooldown_lifecycle() {
        let guard = LastFmRateGuard::new();
        assert!(!guard.is_cooling_down());
        assert_eq!(guard.remaining(), Duration::ZERO);

        guard.on_request_limited(Duration::from_secs(5));
        assert!(guard.is_cooling_down());
        let first_remaining = guard.remaining();
        assert!(first_remaining > Duration::ZERO && first_remaining <= Duration::from_secs(5));

        // Extending while cooling down stacks onto the existing deadline.
        guard.on_request_limited(Duration::from_secs(5));
        assert!(guard.remaining() > Duration::from_secs(5));

        guard.on_request_succeeded();
        assert!(!guard.is_cooling_down());
        assert_eq!(guard.remaining(), Duration::ZERO);
    }

    #[test]
    fn friendly_error_messages() {
        assert_eq!(
            last_fm_friendly_message(Some(9), "raw"),
            "Invalid session — reconnect Last.fm."
        );
        assert_eq!(
            last_fm_friendly_message(Some(29), ""),
            "Rate limited — backing off."
        );
        assert_eq!(last_fm_friendly_message(Some(99), "custom err"), "custom err");
        assert_eq!(
            last_fm_friendly_message(None, ""),
            "Last.fm request failed."
        );
    }
}
