use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Local};

use crate::models::app_info::APP_VERSION;

/// Runs kept on disk (this one included).
pub const MAX_FILES: usize = 5;

/// Cap per file; the trail rotates into a new file when it is reached.
pub const MAX_BYTES_PER_FILE: usize = 2 << 20;

const PREFIX: &str = "session-";

static VERBOSE: AtomicBool = AtomicBool::new(false);
static PREVIOUS_EXIT_CLEAN: AtomicBool = AtomicBool::new(true);
static STATE: OnceLock<Mutex<DiagnosticsLogger>> = OnceLock::new();

fn state() -> &'static Mutex<DiagnosticsLogger> {
    STATE.get_or_init(|| Mutex::new(DiagnosticsLogger::new()))
}

/// Stateful logger instance backing `DiagnosticsLog` (also directly usable in isolated tests).
#[derive(Debug, Default)]
pub struct DiagnosticsLogger {
    dir: Option<PathBuf>,
    file: Option<PathBuf>,
    bytes: usize,
    rotation: usize,
    failed: bool,
    boot_stamp: String,
    verbose: bool,
    previous_exit_clean: bool,
}

impl DiagnosticsLogger {
    pub fn new() -> Self {
        Self {
            dir: None,
            file: None,
            bytes: 0,
            rotation: 0,
            failed: false,
            boot_stamp: String::new(),
            verbose: false,
            previous_exit_clean: true,
        }
    }

    pub fn is_verbose(&self) -> bool {
        self.verbose
    }

    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    pub fn previous_exit_clean(&self) -> bool {
        self.previous_exit_clean
    }

    pub fn directory(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    pub fn init(&mut self, dir: &Path, verbose: bool) {
        self.verbose = verbose;
        self.failed = false;
        self.file = None;
        self.bytes = 0;
        self.rotation = 0;

        if fs::create_dir_all(dir).is_err() {
            self.failed = true;
            return;
        }
        self.dir = Some(dir.to_path_buf());
        self.boot_stamp = stamp(Local::now());
        self.prune(MAX_FILES.saturating_sub(1));
        self.previous_exit_clean = self.check_previous_exit_clean();
        self.open(0);

        let prev = if self.previous_exit_clean {
            "clean"
        } else {
            "unclean"
        };
        self.info(&format!(
            "session start version={APP_VERSION} mode={} os={} previous_exit={prev}",
            mode(),
            std::env::consts::OS,
        ));
        self.info(&format!(
            "runtime rust={} locale={} logs={}",
            option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("stable"),
            locale_name(),
            dir.display(),
        ));
    }

    pub fn info(&mut self, line: &str) {
        self.write_line(line);
    }

    pub fn detail(&mut self, line: &str) {
        if !self.verbose {
            return;
        }
        self.write_line(line);
    }

    pub fn raw(&mut self, line: &str) {
        self.write_line(line);
    }

    pub fn divider(&mut self, header: &str) {
        self.write_line(header);
    }

    pub fn mark_clean_exit(&mut self) {
        if self.failed {
            return;
        }
        let Some(path) = self.file.as_ref() else {
            return;
        };
        let text = format!("{}session end clean\n", iso_now());
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(mut f) => {
                if f.write_all(text.as_bytes()).is_err() || f.flush().is_err() {
                    self.failed = true;
                }
            }
            Err(_) => {
                self.failed = true;
            }
        }
    }

    pub fn session_files(&self) -> Vec<PathBuf> {
        self.sorted_files()
    }

    fn sorted_files(&self) -> Vec<PathBuf> {
        let Some(dir) = self.dir.as_ref() else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(PREFIX))
            })
            .collect();

        files.sort_by(|a, b| {
            let by_stamp = base_stamp(b).cmp(&base_stamp(a));
            if by_stamp != std::cmp::Ordering::Equal {
                return by_stamp;
            }
            rotation_of(b).cmp(&rotation_of(a))
        });
        files
    }

    fn write_line(&mut self, line: &str) {
        if self.failed {
            return;
        }
        let Some(path) = self.file.as_ref() else {
            return;
        };
        let text = format!("{}{line}\n", iso_now());
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(mut f) => {
                if f.write_all(text.as_bytes()).is_err() {
                    self.failed = true;
                    return;
                }
                self.bytes += text.len();
                if self.bytes >= MAX_BYTES_PER_FILE {
                    self.open(self.rotation + 1);
                }
            }
            Err(_) => {
                self.failed = true;
            }
        }
    }

    fn open(&mut self, rotation: usize) {
        let Some(dir) = self.dir.as_ref() else {
            return;
        };
        self.rotation = rotation;
        let suffix = if rotation == 0 {
            String::new()
        } else {
            format!("-r{rotation}")
        };
        let path = dir.join(format!("{PREFIX}{}{suffix}.log", self.boot_stamp));
        self.bytes = fs::metadata(&path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);
        self.file = Some(path);
    }

    fn prune(&self, keep: usize) {
        let files = self.sorted_files();
        for f in files.into_iter().skip(keep) {
            let _ = fs::remove_file(f);
        }
    }

    fn check_previous_exit_clean(&self) -> bool {
        let files = self.sorted_files();
        let Some(newest) = files.first() else {
            return true;
        };
        let Ok(mut f) = File::open(newest) else {
            return true;
        };
        let Ok(meta) = f.metadata() else {
            return true;
        };
        let len = meta.len();
        let start = len.saturating_sub(4096);
        if f.seek(SeekFrom::Start(start)).is_err() {
            return true;
        }
        let mut buf = Vec::with_capacity((len - start) as usize);
        if f.read_to_end(&mut buf).is_err() {
            return true;
        }
        let tail = String::from_utf8_lossy(&buf);
        let last_line = tail.lines().rfind(|l| !l.trim().is_empty());
        match last_line {
            None => true,
            Some(line) => line.contains("session end clean"),
        }
    }
}

/// Persistent diagnostics trail: one file per app run under `<appSupport>/logs/`,
/// mirrored into `%TEMP%/lastwave/mpv-ops.log` by `TraceCrumb`.
pub struct DiagnosticsLog;

impl DiagnosticsLog {
    pub const MAX_FILES: usize = MAX_FILES;
    pub const MAX_BYTES_PER_FILE: usize = MAX_BYTES_PER_FILE;

    pub fn is_verbose() -> bool {
        VERBOSE.load(Ordering::Relaxed)
    }

    pub fn set_verbose(verbose: bool) {
        VERBOSE.store(verbose, Ordering::Relaxed);
        if let Ok(mut guard) = state().lock() {
            guard.set_verbose(verbose);
        }
    }

    pub fn previous_exit_clean() -> bool {
        PREVIOUS_EXIT_CLEAN.load(Ordering::Relaxed)
    }

    pub fn directory() -> Option<PathBuf> {
        state()
            .lock()
            .ok()
            .and_then(|g| g.directory().map(Path::to_path_buf))
    }

    pub fn file() -> Option<PathBuf> {
        state()
            .lock()
            .ok()
            .and_then(|g| g.file().map(Path::to_path_buf))
    }

    /// Point the sink at `dir` (app support `logs/`) and open this run's file.
    /// Lines written before `init` are dropped.
    pub fn init(dir: &Path, verbose: bool) {
        VERBOSE.store(verbose, Ordering::Relaxed);
        if let Ok(mut guard) = state().lock() {
            guard.init(dir, verbose);
            PREVIOUS_EXIT_CLEAN.store(guard.previous_exit_clean(), Ordering::Relaxed);
        }
    }

    /// Always recorded.
    pub fn info(line: &str) {
        if let Ok(mut guard) = state().lock() {
            guard.info(line);
        }
    }

    /// Only when `verbose` is on.
    pub fn detail(line: &str) {
        if !Self::is_verbose() {
            return;
        }
        if let Ok(mut guard) = state().lock() {
            guard.detail(line);
        }
    }

    /// Raw line (already carries its own prefix, e.g. a crumb). Recorded
    /// whether or not `verbose` is on — callers decide the level.
    pub fn raw(line: &str) {
        if let Ok(mut guard) = state().lock() {
            guard.raw(line);
        }
    }

    /// Section divider (`--- player created ---`), mirrors the temp trail.
    pub fn divider(header: &str) {
        if let Ok(mut guard) = state().lock() {
            guard.divider(header);
        }
    }

    /// Marks this run as having shut down on purpose, so the next start
    /// does not report it as a crash.
    pub fn mark_clean_exit() {
        if let Ok(mut guard) = state().lock() {
            guard.mark_clean_exit();
        }
    }

    /// Every file of this run (usually one; rotation adds more), newest first.
    pub fn session_files() -> Vec<PathBuf> {
        state()
            .lock()
            .map(|g| g.session_files())
            .unwrap_or_default()
    }
}

/// `session-20261023-170024-r1.log` -> `session-20261023-170024`.
pub(crate) fn base_stamp(path: &Path) -> String {
    let mut name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    if let Some(stripped) = name.strip_suffix(".log") {
        name = stripped.to_string();
    }
    if let Some(marker) = name.rfind("-r") {
        if name[marker + 2..].parse::<usize>().is_ok() {
            name.truncate(marker);
        }
    }
    name
}

pub(crate) fn rotation_of(path: &Path) -> usize {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let Some(stem) = name.strip_suffix(".log") else {
        return 0;
    };
    let Some(marker) = stem.rfind("-r") else {
        return 0;
    };
    stem[marker + 2..].parse::<usize>().unwrap_or(0)
}

fn iso_now() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
}

fn stamp(t: DateTime<Local>) -> String {
    t.format("%Y%m%d-%H%M%S").to_string()
}

fn mode() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn locale_name() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lastwave_diag_test_{}_{}_{}",
            label,
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn clean_vs_unclean_exit_and_verbose_filtering() {
        let dir = temp_dir("clean");
        let mut logger = DiagnosticsLogger::new();
        logger.init(&dir, false);
        assert!(logger.previous_exit_clean());
        logger.detail("should-be-dropped");
        logger.info("always-recorded");

        let file = logger.file().unwrap().to_path_buf();
        let contents = fs::read_to_string(&file).unwrap();
        assert!(!contents.contains("should-be-dropped"));
        assert!(contents.contains("always-recorded"));

        // Without mark_clean_exit, next logger sees previous_exit_clean == false.
        let mut logger2 = DiagnosticsLogger::new();
        // Force a distinct filename by writing an older session file without clean marker.
        let old_file = dir.join("session-20200101-000000.log");
        fs::write(&old_file, "2020-01-01T00:00:00session start\n").unwrap();
        let dir2 = temp_dir("unclean");
        fs::write(
            dir2.join("session-20200101-000000.log"),
            "2020-01-01T00:00:00session start\n",
        )
        .unwrap();
        logger2.init(&dir2, true);
        assert!(!logger2.previous_exit_clean());

        logger2.detail("verbose-line");
        logger2.mark_clean_exit();
        let contents2 = fs::read_to_string(logger2.file().unwrap()).unwrap();
        assert!(contents2.contains("verbose-line"));
        assert!(contents2.contains("session end clean"));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn rotation_and_pruning_respect_limits() {
        let dir = temp_dir("prune");
        for i in 1..=7 {
            let p = dir.join(format!("session-2026010{i}-000000.log"));
            fs::write(p, "line\n").unwrap();
        }
        let mut logger = DiagnosticsLogger::new();
        logger.init(&dir, false);
        // 4 pruned old files + 1 current file = MAX_FILES (5).
        assert_eq!(logger.session_files().len(), MAX_FILES);

        let _ = fs::remove_dir_all(&dir);
    }
}
