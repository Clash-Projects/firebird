//! Fatal-error breadcrumbs: format + synchronously append fatal errors
//! to the ops trail (`<temp>/lastwave/mpv-ops.log`) so the next fail-fast
//! names its line. Sync writes only; all best-effort, never panics.

use std::fmt::Display;
use std::fs::{self, OpenOptions};
use std::future::Future;
use std::io::Write;
use std::panic::{self, UnwindSafe};

use chrono::Local;

use super::diagnostics_log::DiagnosticsLog;
use super::trace_crumb::TraceCrumb;

const MAX_MESSAGE_CHARS: usize = 500;

/// One single-line, bounded breadcrumb for a fatal error.
pub fn fatal_crumb(error_type: &str, error_message: &str, stack: Option<&str>) -> String {
    let collapsed: String = error_message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut message = collapsed;
    if message.chars().count() > MAX_MESSAGE_CHARS {
        let truncated: String = message.chars().take(MAX_MESSAGE_CHARS).collect();
        message = format!("{truncated}...");
    }

    let mut out = format!("FATAL {error_type}: {message}");
    if let Some(bt) = stack {
        if let Some(frame) = bt
            .lines()
            .map(str::trim)
            .find(|l| l.contains("package:lastwave") || l.contains("lastwave"))
        {
            if !frame.is_empty() {
                out.push_str(" @ ");
                out.push_str(frame);
            }
        }
    }
    out
}

/// Convenience helper formatting any `Display` error into a `fatal_crumb`.
pub fn fatal_crumb_from_error<E: Display>(error: &E, stack: Option<&str>) -> String {
    fatal_crumb(std::any::type_name::<E>(), &error.to_string(), stack)
}

/// Append `line` to the ops trail. Never panics.
pub fn write_fatal_crumb(line: &str) {
    let dir = TraceCrumb::directory();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("mpv-ops.log");
    let text = format!("{} {line}\n", iso_now());
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(text.as_bytes());
        let _ = f.flush();
    }
    // Also into the session log: a fatal crumb is the single most useful
    // line in an exported bundle, and temp may be gone by then.
    DiagnosticsLog::raw(line);
}

fn emit_guard_line(scope: &str, crumb: &str, on_error: Option<&dyn Fn(&str)>) {
    let line = format!("GUARD {scope}: {crumb}");
    if let Some(cb) = on_error {
        let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| cb(&line)));
    } else {
        write_fatal_crumb(&line);
    }
}

/// Runs `body` and converts a synchronous panic into a crumb instead of an
/// unhandled abort. Returns the body value, or `None` when it panicked.
pub fn run_guarded<T, F>(scope: &str, body: F, on_error: Option<&dyn Fn(&str)>) -> Option<T>
where
    F: FnOnce() -> T + UnwindSafe,
{
    match panic::catch_unwind(body) {
        Ok(val) => Some(val),
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "panic".to_string()
            };
            let bt = std::backtrace::Backtrace::capture().to_string();
            let crumb = fatal_crumb("Panic", &msg, Some(&bt));
            emit_guard_line(scope, &crumb, on_error);
            None
        }
    }
}

/// Runs `body` and converts an `Err` into a crumb. Returns `Some(val)` on `Ok`,
/// or `None` on `Err`.
pub fn run_guarded_result<T, E, F>(
    scope: &str,
    body: F,
    on_error: Option<&dyn Fn(&str)>,
) -> Option<T>
where
    E: Display,
    F: FnOnce() -> Result<T, E>,
{
    match body() {
        Ok(val) => Some(val),
        Err(err) => {
            let crumb = fatal_crumb_from_error(&err, None);
            emit_guard_line(scope, &crumb, on_error);
            None
        }
    }
}

/// Async twin of `run_guarded_result` for listener bodies that await.
pub async fn run_guarded_async<T, E, Fut>(
    scope: &str,
    body: Fut,
    on_error: Option<&dyn Fn(&str)>,
) -> Option<T>
where
    E: Display,
    Fut: Future<Output = Result<T, E>>,
{
    match body.await {
        Ok(val) => Some(val),
        Err(err) => {
            let crumb = fatal_crumb_from_error(&err, None);
            emit_guard_line(scope, &crumb, on_error);
            None
        }
    }
}

fn iso_now() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn fatal_crumb_truncates_and_extracts_frame() {
        let long_msg = "a".repeat(600);
        let stack = "at std::panicking\n  at lastwave_lib::audio::open (src/audio/mod.rs:42)\n";
        let crumb = fatal_crumb("TestErr", &long_msg, Some(stack));
        assert!(crumb.starts_with("FATAL TestErr: "));
        assert!(crumb.contains("... @ at lastwave_lib::audio::open"));
    }

    #[test]
    fn run_guarded_result_invokes_callback_on_err() {
        let captured = RefCell::new(String::new());
        let ok = run_guarded_result::<_, &str, _>("decode", || Ok(42), None);
        assert_eq!(ok, Some(42));

        let err = run_guarded_result::<i32, _, _>(
            "decode",
            || Err("bad\n  frame"),
            Some(&|line| {
                *captured.borrow_mut() = line.to_string();
            }),
        );
        assert_eq!(err, None);
        let msg = captured.into_inner();
        assert!(msg.starts_with("GUARD decode: FATAL "));
        assert!(msg.contains("bad frame"));
    }
}
