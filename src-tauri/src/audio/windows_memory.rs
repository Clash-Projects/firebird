use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const DEFAULT_TRIM_INTERVAL: Duration = Duration::from_secs(15);

static LAST_TRIM: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

fn last_trim_lock() -> &'static Mutex<Option<Instant>> {
    LAST_TRIM.get_or_init(|| Mutex::new(None))
}

/// Trims the process working set on Windows by calling
/// `SetProcessWorkingSetSize(GetCurrentProcess(), -1, -1)`.
/// Safely does nothing on non-Windows platforms.
pub fn trim_windows_working_set() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
        unsafe {
            let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
        }
    }
}

/// Throttled trim: safely reclaims physical RAM pages from the Windows
/// working set at most once every `min_interval` (default 15 seconds).
pub fn trim_windows_working_set_throttled(min_interval: Duration) {
    let now = Instant::now();
    if let Ok(mut guard) = last_trim_lock().lock() {
        if let Some(prev) = *guard {
            if now.duration_since(prev) < min_interval {
                return;
            }
        }
        *guard = Some(now);
    }
    trim_windows_working_set();
}

/// Convenience wrapper matching Dart's default `minInterval = 15s`.
pub fn trim_windows_working_set_default() {
    trim_windows_working_set_throttled(DEFAULT_TRIM_INTERVAL);
}
