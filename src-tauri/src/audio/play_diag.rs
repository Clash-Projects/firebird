use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use chrono::Local;

/// Playback diagnostic ring buffer: WHY a tapped song didn't play.
///
/// Resolve/match/open/skip events append one short line each —
/// unconditionally (release builds too), so a "spinner then dead"
/// report can be captured via Settings -> Playback -> Copy log and
/// pasted back. Never holds URLs, cookies, or headers: track
/// titles/artists, match tiers, videoIds, and stream shapes only.
pub struct PlayDiag;

static LINES: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();

fn lines_lock() -> &'static Mutex<VecDeque<String>> {
    LINES.get_or_init(|| Mutex::new(VecDeque::with_capacity(PlayDiag::CAP)))
}

impl PlayDiag {
    pub const CAP: usize = 120;

    pub fn log(line: &str) {
        let stamp = Local::now().format("%H:%M:%S");
        if let Ok(mut guard) = lines_lock().lock() {
            guard.push_back(format!("[{stamp}] {line}"));
            while guard.len() > Self::CAP {
                guard.pop_front();
            }
        }
        #[cfg(debug_assertions)]
        eprintln!("LastWavePlay: {line}");
    }

    pub fn dump() -> String {
        let Ok(guard) = lines_lock().lock() else {
            return String::new();
        };
        guard.iter().cloned().collect::<Vec<_>>().join("\n")
    }

    pub fn clear() {
        if let Ok(mut guard) = lines_lock().lock() {
            guard.clear();
        }
    }
}
