use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use chrono::Local;

use super::diagnostics_log::DiagnosticsLog;

#[derive(Default)]
struct TraceState {
    path: Option<PathBuf>,
    resolved: bool,
    failed: bool,
}

static STATE: OnceLock<Mutex<TraceState>> = OnceLock::new();

fn state() -> &'static Mutex<TraceState> {
    STATE.get_or_init(|| Mutex::new(TraceState::default()))
}

/// Crash-surviving breadcrumb sink shared by the player and the addon
/// resolver: `<temp>/lastwave/mpv-ops.log`.
///
/// Every line is mirrored into `DiagnosticsLog` as well, because the temp
/// trail is truncated per player creation and temp gets cleaned: the
/// session files are what a user can still export tomorrow.
pub struct TraceCrumb;

impl TraceCrumb {
    /// `<temp>/lastwave` — shared with `mpv.log`.
    pub fn directory() -> PathBuf {
        std::env::temp_dir().join("lastwave")
    }

    /// `<temp>/lastwave/mpv-ops.log` (directory created on first use).
    pub fn path() -> Option<PathBuf> {
        let mut guard = state().lock().ok()?;
        resolve_path(&mut guard)
    }

    /// Append one line, best-effort. `line` carries its own prefix
    /// (`open …`, `resolve …`, `timing …`).
    ///
    /// The temp mirror and the session log are independent: a broken temp
    /// file must not cost the session trail (or the other way round).
    pub fn write(line: &str) {
        Self::mirror(line);
        DiagnosticsLog::raw(line);
    }

    fn mirror(line: &str) {
        let Ok(mut guard) = state().lock() else {
            return;
        };
        if guard.failed {
            return;
        }
        let Some(path) = resolve_path(&mut guard) else {
            return;
        };
        let text = format!("{}{line}\n", iso_now());
        let ok = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| {
                f.write_all(text.as_bytes())?;
                f.flush()
            })
            .is_ok();
        if !ok {
            // A broken trail must never break playback.
            guard.failed = true;
        }
    }

    /// Start a fresh per-session timeline. Player creation truncates the
    /// temp file so a crash report only carries the run that died; the
    /// session log keeps the whole run and only gets a divider.
    pub fn reset(header: &str) {
        Self::reset_mirror(header);
        DiagnosticsLog::divider(header);
    }

    fn reset_mirror(header: &str) {
        let Ok(mut guard) = state().lock() else {
            return;
        };
        if guard.failed {
            return;
        }
        let Some(path) = resolve_path(&mut guard) else {
            return;
        };
        let text = format!("{}{header}\n", iso_now());
        let ok = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .and_then(|mut f| {
                f.write_all(text.as_bytes())?;
                f.flush()
            })
            .is_ok();
        if !ok {
            guard.failed = true;
        }
    }
}

fn resolve_path(guard: &mut TraceState) -> Option<PathBuf> {
    if guard.resolved {
        return guard.path.clone();
    }
    guard.resolved = true;
    let dir = TraceCrumb::directory();
    if fs::create_dir_all(&dir).is_err() {
        guard.failed = true;
        return None;
    }
    let path = dir.join("mpv-ops.log");
    guard.path = Some(path.clone());
    Some(path)
}

fn iso_now() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
}
