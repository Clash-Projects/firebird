use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use chrono::{DateTime, Local};
use regex::Regex;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::diagnostics_log::DiagnosticsLog;
use super::trace_crumb::TraceCrumb;

const README: &str = "\
LastWave Desktop diagnostics bundle
===================================

diagnostics.txt            App/build/platform summary, audio output path,
                           quality prefs, library counts, queue at export.
logs/                      One file per app run (newest first). Each line is
                           timestamped. `timing ...` lines split a cold open
                           into resolve / assembly / first-audio; `resolve
                           stage=...` lines explain lossless matching.
crash-trail/mpv-ops.log    The flushed temp trail: its last line names the
                           native call in progress if the app died.
crash-trail/mpv.log        libmpv's own log, when it wrote one.

Every text file has been scrubbed: URLs (they carry signed credentials),
cookies, authorization headers and tokens are replaced with <url> and
<redacted>. Track titles and artist names are left in — they are what makes
a \"wrong song\" report actionable.

Nothing here was uploaded by the app. You are looking at a file created only
because someone clicked \"Export diagnostic logs\".
";

fn url_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"https?://\S+").expect("valid url regex"))
}

fn secret_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(authorization|cookie|set-cookie|sapisidhash|api[_-]?key|apikey|token|secret|password|signature|sig)\b(\s*[:=]\s*)(\S+)",
        )
        .expect("valid secret regex")
    })
}

/// Builds the zip a user attaches when reporting a problem:
///
/// - `diagnostics.txt`: summary (version, output path, prefs, counts)
/// - `logs/session-*.log`: this run + the previous runs on disk
/// - `crash-trail/mpv-ops.log`: the flushed temp mirror (native crash tail)
/// - `crash-trail/mpv.log`: libmpv's own log, when it wrote one
/// - `README.txt`: what this is
pub struct LogBundle;

impl LogBundle {
    /// `LastWave-logs-20261023-171500.zip`.
    pub fn suggested_name(now: Option<DateTime<Local>>) -> String {
        let t = now.unwrap_or_else(Local::now);
        format!("LastWave-logs-{}.zip", t.format("%Y%m%d-%H%M%S"))
    }

    /// Name -> text content. Split out from `build` so tests can assert on
    /// the bundle without unzipping.
    pub fn collect(report: &str) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for file in DiagnosticsLog::session_files() {
            if let Some(name) = file.file_name().and_then(|n| n.to_str()) {
                out.insert(format!("logs/{name}"), Self::redact(&read_lossy(&file)));
            }
        }
        if let Some(trail) = TraceCrumb::path() {
            if trail.exists() {
                out.insert(
                    "crash-trail/mpv-ops.log".to_string(),
                    Self::redact(&read_lossy(&trail)),
                );
            }
            let mpv_log = TraceCrumb::directory().join("mpv.log");
            if let Ok(meta) = fs::metadata(&mpv_log) {
                if meta.is_file() && meta.len() > 0 {
                    out.insert(
                        "crash-trail/mpv.log".to_string(),
                        Self::redact(&read_lossy(&mpv_log)),
                    );
                }
            }
        }
        out.insert("diagnostics.txt".to_string(), Self::redact(report));
        out.insert("README.txt".to_string(), README.to_string());
        out
    }

    /// Zip of `collect` (deflated, no directory entries needed).
    pub fn build(report: &str) -> std::io::Result<Vec<u8>> {
        let files = Self::collect(report);
        let cursor = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        for (name, content) in files {
            zip.start_file(name, options)
                .map_err(std::io::Error::other)?;
            zip.write_all(content.as_bytes())?;
        }

        let cursor = zip.finish().map_err(std::io::Error::other)?;
        Ok(cursor.into_inner())
    }

    /// Build the diagnostic zip bundle and write it directly to `dest`.
    pub fn export_to_path(report: &str, dest: &Path) -> std::io::Result<PathBuf> {
        let bytes = Self::build(report)?;
        if let Some(parent) = dest.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(dest, bytes)?;
        Ok(dest.to_path_buf())
    }

    /// Belt-and-braces scrub. The writers avoid URLs and credentials on
    /// purpose, but a bundle must be safe to hand over even if one slipped
    /// through (e.g. an exception message quoting a request).
    pub fn redact(text: &str) -> String {
        let step1 = secret_pattern().replace_all(text, "${1}${2}<redacted>");
        url_pattern().replace_all(&step1, "<url>").into_owned()
    }

    /// Opens the folder containing `path` (a file) in the OS file manager.
    pub fn reveal_file(path: &Path) {
        #[cfg(target_os = "windows")]
        {
            let _ = Command::new("explorer")
                .arg("/select,")
                .arg(path)
                .spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open").arg("-R").arg(path).spawn();
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            if let Some(parent) = path.parent() {
                Self::open_folder(parent);
            }
        }
    }

    /// Opens `path` (a directory) in the OS file manager.
    pub fn open_folder(path: &Path) {
        #[cfg(target_os = "windows")]
        {
            let _ = Command::new("explorer").arg(path).spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open").arg(path).spawn();
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            let _ = Command::new("xdg-open").arg(path).spawn();
        }
    }
}

fn read_lossy(path: &Path) -> String {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use zip::ZipArchive;

    #[test]
    fn redact_scrubs_urls_and_secrets_case_insensitively() {
        let raw = "GET https://example.com/stream?sig=abc123&token=xyz\n\
                   Authorization: Bearer_secret123\n\
                   cookie=sp_dc=supersecret\n\
                   api_key: my-key-123\n\
                   Track: Daft Punk - Giorgio by Moroder";
        let scrubbed = LogBundle::redact(raw);
        assert!(!scrubbed.contains("https://example.com"));
        assert!(!scrubbed.contains("Bearer_secret123"));
        assert!(!scrubbed.contains("sp_dc=supersecret"));
        assert!(!scrubbed.contains("my-key-123"));
        assert!(scrubbed.contains("<url>"));
        assert!(scrubbed.contains("Authorization: <redacted>"));
        assert!(scrubbed.contains("Track: Daft Punk - Giorgio by Moroder"));
    }

    #[test]
    fn build_creates_valid_zip_with_redacted_diagnostics() {
        let bytes = LogBundle::build("token=abc123 https://secret.invalid/x").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();

        let mut diag = String::new();
        archive
            .by_name("diagnostics.txt")
            .unwrap()
            .read_to_string(&mut diag)
            .unwrap();
        assert_eq!(diag, "token=<redacted> <url>");

        let mut readme = String::new();
        archive
            .by_name("README.txt")
            .unwrap()
            .read_to_string(&mut readme)
            .unwrap();
        assert!(readme.contains("LastWave Desktop diagnostics bundle"));
    }
}
