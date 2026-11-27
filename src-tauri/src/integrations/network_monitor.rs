use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Client;
use tokio::sync::{watch, Notify};

/// Consecutive failed probe rounds required before reporting offline.
/// A single failed round holds the last state (reads as "unknown",
/// not "offline" — one DNS blip or slow lookup must never raise the
/// bar); recovery is immediate on the next success.
pub const OFFLINE_STRIKE_THRESHOLD: usize = 2;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const FOREGROUND_INTERVAL: Duration = Duration::from_secs(15);
const BACKGROUND_INTERVAL: Duration = Duration::from_secs(60);
const PRIMARY_URL: &str = "https://www.youtube.com/generate_204";
const SECONDARY_URL: &str = "https://www.apple.com/library/test/success.html";

/// Pure flip logic behind [`NetworkMonitor`]: reached-HTTP resets the
/// count and reports online, silence accumulates strikes and reports
/// offline only at [`OFFLINE_STRIKE_THRESHOLD`].
#[derive(Debug, Default, Clone)]
pub struct ConnectivityStrikes {
    failures: usize,
}

impl ConnectivityStrikes {
    pub fn new() -> Self {
        Self { failures: 0 }
    }

    pub fn failures(&self) -> usize {
        self.failures
    }

    pub fn record_result(&mut self, responded: bool) -> bool {
        if responded {
            self.failures = 0;
            return true;
        }
        self.failures += 1;
        self.failures < OFFLINE_STRIKE_THRESHOLD
    }
}

/// Desktop connectivity monitor.
///
/// Probes real HTTP reachability against captive-portal endpoints
/// (Google edge primary, Apple secondary). Any HTTP response proves
/// packets flow (even a 5xx or portal intercept); only silence
/// (DNS/socket failure, timeout) counts a strike, and it takes two
/// consecutive strikes to flip offline.
pub struct NetworkMonitor {
    client: Client,
    strikes: Mutex<ConnectivityStrikes>,
    online_tx: watch::Sender<bool>,
    online_rx: watch::Receiver<bool>,
    visible: AtomicBool,
    checking: AtomicBool,
    disposed: AtomicBool,
    wake: Arc<Notify>,
}

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkMonitor {
    pub fn new() -> Self {
        let client = Client::builder()
            .connect_timeout(PROBE_TIMEOUT)
            .timeout(PROBE_TIMEOUT)
            .pool_idle_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());
        let (online_tx, online_rx) = watch::channel(true);
        Self {
            client,
            strikes: Mutex::new(ConnectivityStrikes::new()),
            online_tx,
            online_rx,
            visible: AtomicBool::new(true),
            checking: AtomicBool::new(false),
            disposed: AtomicBool::new(false),
            wake: Arc::new(Notify::new()),
        }
    }

    /// Starts the background probe loop. Call once on startup.
    pub fn start(self: &Arc<Self>) {
        let monitor = Arc::clone(self);
        tokio::spawn(async move {
            monitor.check_now().await;
            loop {
                if monitor.disposed.load(Ordering::Acquire) {
                    break;
                }
                let interval = if monitor.visible.load(Ordering::Acquire) {
                    FOREGROUND_INTERVAL
                } else {
                    BACKGROUND_INTERVAL
                };
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {}
                    _ = monitor.wake.notified() => {}
                }
                if monitor.disposed.load(Ordering::Acquire) {
                    break;
                }
                monitor.check_now().await;
            }
        });
    }

    /// Window visibility gate. Backs the probe cadence off while minimized
    /// or tray-hidden and re-probes immediately on restore.
    pub fn set_visible(&self, visible: bool) {
        let prev = self.visible.swap(visible, Ordering::AcqRel);
        if prev == visible {
            return;
        }
        self.wake.notify_one();
    }

    pub fn is_currently_connected(&self) -> bool {
        *self.online_rx.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.online_rx.clone()
    }

    /// True when the URL answers with any HTTP response. Drained so the
    /// persistent connection is reusable; never panics.
    async fn probe(&self, url: &str) -> bool {
        let Ok(resp) = self.client.get(url).send().await else {
            return false;
        };
        let _ = resp.bytes().await;
        true
    }

    /// Runs a single probe round if one is not already in flight.
    pub async fn check_now(&self) -> bool {
        if self.disposed.load(Ordering::Acquire) {
            return self.is_currently_connected();
        }
        if self
            .checking
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return self.is_currently_connected();
        }

        let responded = self.probe(PRIMARY_URL).await || self.probe(SECONDARY_URL).await;
        let next_online = {
            let mut strikes = self.strikes.lock().expect("strikes mutex poisoned");
            strikes.record_result(responded)
        };
        self.checking.store(false, Ordering::Release);

        if !self.disposed.load(Ordering::Acquire) && *self.online_rx.borrow() != next_online {
            let _ = self.online_tx.send(next_online);
        }
        next_online
    }

    pub fn dispose(&self) {
        self.disposed.store(true, Ordering::Release);
        self.wake.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connectivity_strikes_threshold_and_recovery() {
        let mut strikes = ConnectivityStrikes::new();
        assert_eq!(strikes.failures(), 0);

        // Single failure holds online state ("unknown", not offline).
        assert!(strikes.record_result(false));
        assert_eq!(strikes.failures(), 1);

        // Second consecutive failure flips offline.
        assert!(!strikes.record_result(false));
        assert_eq!(strikes.failures(), 2);

        // Third remains offline.
        assert!(!strikes.record_result(false));
        assert_eq!(strikes.failures(), 3);

        // Single success recovers immediately and resets failures.
        assert!(strikes.record_result(true));
        assert_eq!(strikes.failures(), 0);

        // Needs two more consecutive failures to flip offline again.
        assert!(strikes.record_result(false));
        assert!(!strikes.record_result(false));
    }

    #[test]
    fn monitor_starts_online() {
        let monitor = NetworkMonitor::new();
        assert!(monitor.is_currently_connected());
        monitor.dispose();
    }
}
