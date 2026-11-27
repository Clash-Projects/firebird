use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use crate::extractor::http_client::HttpClientFactory;
use crate::models::app_info::APP_VERSION;

pub const REPO_OWNER: &str = "Clash-Projects";
pub const REPO_NAME: &str = "LastWave-Desktop";
pub const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/Clash-Projects/LastWave-Desktop/releases/latest";

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("no suitable release asset found for this platform")]
    NoSuitableAsset,
    #[error("update download cancelled")]
    Cancelled,
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("downloaded file size mismatch: expected {expected} bytes, got {actual} bytes")]
    SizeMismatch { expected: u64, actual: u64 },
}

/// Semantic version helper for release comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub build: u64,
    pub raw: String,
}

impl SemVer {
    pub fn parse(version_str: &str) -> Self {
        let mut v = version_str.trim();
        if let Some(rest) = v.strip_prefix('v').or_else(|| v.strip_prefix('V')) {
            v = rest.trim();
        }
        let mut build_num = 0u64;
        if let Some((head, tail)) = v.split_once('+') {
            v = head;
            build_num = tail.parse::<u64>().unwrap_or(0);
        }
        if let Some((head, _)) = v.split_once('-') {
            v = head;
        }
        let mut parts = v.split('.');
        let major = parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let minor = parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let patch = parts
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        Self {
            major,
            minor,
            patch,
            build: build_num,
            raw: version_str.to_string(),
        }
    }

    pub fn is_greater_than(&self, other: &SemVer) -> bool {
        if self.major != other.major {
            return self.major > other.major;
        }
        if self.minor != other.minor {
            return self.minor > other.minor;
        }
        if self.patch != other.patch {
            return self.patch > other.patch;
        }
        self.build > other.build
    }
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.build > 0 {
            write!(f, "{}.{}.{}+{}", self.major, self.minor, self.patch, self.build)
        } else {
            write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

/// A release asset published on GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
}

impl AppReleaseAsset {
    pub fn from_json(json: &Value) -> Self {
        Self {
            name: json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            download_url: json
                .get("browser_download_url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            size: json.get("size").and_then(Value::as_u64).unwrap_or(0),
        }
    }
}

/// Installation type of the running application instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppInstallType {
    WindowsInstalled,
    WindowsPortable,
    LinuxAppImage,
    LinuxDeb,
    LinuxRpm,
    MacosZip,
    Other,
}

impl AppInstallType {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::WindowsInstalled => "Windows Setup (Installed)",
            Self::WindowsPortable => "Windows Portable (.zip)",
            Self::LinuxAppImage => "Linux AppImage",
            Self::LinuxDeb => "Linux Debian / Ubuntu (.deb)",
            Self::LinuxRpm => "Linux Fedora / RHEL (.rpm)",
            Self::MacosZip => "macOS Archive (.zip)",
            Self::Other => "Universal / Other",
        }
    }
}

/// Detect whether the current app is running as an installed setup, portable zip,
/// or specific distro format.
pub fn detect_current_install_type() -> AppInstallType {
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let has_uninstaller = exe_dir.join("unins000.exe").exists()
                    || exe_dir.join("unins001.exe").exists();
                let lower = exe_path.to_string_lossy().to_ascii_lowercase();
                let in_programs =
                    lower.contains(r"programs\lastwave") || lower.contains("program files");
                if has_uninstaller || in_programs {
                    return AppInstallType::WindowsInstalled;
                }
                return AppInstallType::WindowsPortable;
            }
        }
        return AppInstallType::WindowsInstalled;
    }

    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("APPIMAGE").is_some() {
            return AppInstallType::LinuxAppImage;
        }
        if Path::new("/etc/debian_version").exists() {
            return AppInstallType::LinuxDeb;
        }
        if Path::new("/etc/fedora-release").exists() || Path::new("/etc/redhat-release").exists() {
            return AppInstallType::LinuxRpm;
        }
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            let lower = content.to_ascii_lowercase();
            if lower.contains("ubuntu") || lower.contains("debian") {
                return AppInstallType::LinuxDeb;
            }
            if lower.contains("fedora") || lower.contains("rhel") || lower.contains("suse") {
                return AppInstallType::LinuxRpm;
            }
        }
        return AppInstallType::LinuxAppImage;
    }

    #[cfg(target_os = "macos")]
    {
        return AppInstallType::MacosZip;
    }

    #[allow(unreachable_code)]
    AppInstallType::Other
}

/// Metadata describing an available GitHub release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub tag_name: String,
    pub version: String,
    pub title: String,
    pub changelog: String,
    pub html_url: String,
    pub published_at: Option<DateTime<Utc>>,
    pub assets: Vec<AppReleaseAsset>,
}

impl AppUpdateInfo {
    pub fn from_github_json(json: &Value) -> Self {
        let tag_name = json
            .get("tag_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let ver = tag_name
            .strip_prefix('v')
            .or_else(|| tag_name.strip_prefix('V'))
            .unwrap_or(&tag_name)
            .to_string();
        let title = json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&tag_name)
            .to_string();
        let changelog = json
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let html_url = json
            .get("html_url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let published_at = json
            .get("published_at")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let assets = json
            .get("assets")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter(|v| v.is_object())
                    .map(AppReleaseAsset::from_json)
                    .collect()
            })
            .unwrap_or_default();

        Self {
            tag_name,
            version: ver,
            title,
            changelog,
            html_url,
            published_at,
            assets,
        }
    }

    /// Locate the most suitable binary or installer asset for the host platform.
    pub fn find_best_asset(&self, target_type: Option<AppInstallType>) -> Option<&AppReleaseAsset> {
        if self.assets.is_empty() {
            return None;
        }
        let kind = target_type.unwrap_or_else(detect_current_install_type);
        let first = &self.assets[0];
        let find = |pred: fn(&str) -> bool| -> Option<&AppReleaseAsset> {
            self.assets
                .iter()
                .find(|a| pred(&a.name.to_ascii_lowercase()))
        };

        match kind {
            AppInstallType::WindowsInstalled => Some(
                find(|n| n.ends_with(".exe") && n.contains("setup"))
                    .or_else(|| find(|n| n.ends_with(".exe")))
                    .unwrap_or(first),
            ),
            AppInstallType::WindowsPortable => Some(
                find(|n| n.contains("portable") && n.ends_with(".zip"))
                    .or_else(|| find(|n| n.contains("windows") && n.ends_with(".zip")))
                    .or_else(|| find(|n| n.ends_with(".exe")))
                    .unwrap_or(first),
            ),
            AppInstallType::LinuxAppImage => {
                Some(find(|n| n.ends_with(".appimage")).unwrap_or(first))
            }
            AppInstallType::LinuxDeb => Some(
                find(|n| n.ends_with(".deb"))
                    .or_else(|| find(|n| n.ends_with(".appimage")))
                    .unwrap_or(first),
            ),
            AppInstallType::LinuxRpm => Some(
                find(|n| n.ends_with(".rpm"))
                    .or_else(|| find(|n| n.ends_with(".appimage")))
                    .unwrap_or(first),
            ),
            AppInstallType::MacosZip => Some(
                find(|n| n.contains("macos") && n.ends_with(".zip"))
                    .or_else(|| find(|n| n.ends_with(".dmg")))
                    .unwrap_or(first),
            ),
            AppInstallType::Other => Some(first),
        }
    }
}

/// Service handling GitHub release checks, downloading and running updates.
pub struct UpdateService {
    client: Client,
}

impl Default for UpdateService {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateService {
    pub fn new() -> Self {
        let client = HttpClientFactory::create().unwrap_or_else(|_| Client::new());
        Self { client }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// Query GitHub for the latest release and determine if an update is available.
    pub async fn check_for_update(&self) -> Option<AppUpdateInfo> {
        let resp = self
            .client
            .get(RELEASES_LATEST_URL)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let data: Value = resp.json().await.ok()?;
        let update = AppUpdateInfo::from_github_json(&data);
        let latest = SemVer::parse(&update.version);
        let current = SemVer::parse(APP_VERSION);
        if latest.is_greater_than(&current) {
            Some(update)
        } else {
            None
        }
    }

    /// Download the selected or best installer asset for the current OS, verify
    /// its size (when advertised), and launch it.
    pub async fn download_and_install<F>(
        &self,
        update: &AppUpdateInfo,
        target_asset: Option<&AppReleaseAsset>,
        cancel_flag: Option<&Arc<AtomicBool>>,
        mut on_progress: F,
    ) -> Result<PathBuf, UpdateError>
    where
        F: FnMut(u64, u64),
    {
        let asset = target_asset
            .or_else(|| update.find_best_asset(None))
            .filter(|a| !a.download_url.is_empty())
            .ok_or(UpdateError::NoSuitableAsset)?;

        let temp_dir = std::env::temp_dir();
        let file_name = if asset.name.is_empty() {
            format!("lastwave_update_{}.exe", update.version)
        } else {
            asset.name.clone()
        };
        let save_path = temp_dir.join(file_name);

        if save_path.exists() {
            let _ = std::fs::remove_file(&save_path);
        }

        let mut resp = self
            .client
            .get(&asset.download_url)
            .send()
            .await?
            .error_for_status()?;
        let total = resp.content_length().unwrap_or(asset.size);
        let mut received = 0u64;

        let mut file = tokio::fs::File::create(&save_path).await?;
        while let Some(chunk) = resp.chunk().await? {
            if cancel_flag.is_some_and(|flag| flag.load(Ordering::Acquire)) {
                drop(file);
                let _ = tokio::fs::remove_file(&save_path).await;
                return Err(UpdateError::Cancelled);
            }
            file.write_all(&chunk).await?;
            received += chunk.len() as u64;
            on_progress(received, total);
        }
        file.flush().await?;
        drop(file);

        if asset.size > 0 && received != asset.size {
            let _ = tokio::fs::remove_file(&save_path).await;
            return Err(UpdateError::SizeMismatch {
                expected: asset.size,
                actual: received,
            });
        }

        launch_downloaded_package(&save_path, &temp_dir).await;
        Ok(save_path)
    }
}

async fn launch_downloaded_package(save_path: &Path, temp_dir: &Path) {
    let lower = save_path.to_string_lossy().to_ascii_lowercase();

    #[cfg(target_os = "windows")]
    {
        let _ = temp_dir;
        if lower.ends_with(".exe") {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", "", &save_path.to_string_lossy()])
                .spawn();
            tokio::time::sleep(Duration::from_millis(400)).await;
        } else {
            let _ = std::process::Command::new("explorer.exe")
                .arg(format!("/select,{}", save_path.display()))
                .spawn();
        }
    }

    #[cfg(target_os = "linux")]
    {
        if lower.ends_with(".appimage") {
            let _ = std::process::Command::new("chmod")
                .arg("+x")
                .arg(save_path)
                .status();
            let current = std::env::var("APPIMAGE").unwrap_or_default();
            let launch = if !current.is_empty()
                && replace_appimage_in_place(save_path, Path::new(&current)).await
            {
                PathBuf::from(current)
            } else {
                save_path.to_path_buf()
            };
            let _ = std::process::Command::new(launch).spawn();
            tokio::time::sleep(Duration::from_millis(400)).await;
        } else if lower.ends_with(".deb") || lower.ends_with(".rpm") {
            let _ = std::process::Command::new("xdg-open").arg(save_path).spawn();
        } else {
            let _ = std::process::Command::new("xdg-open").arg(temp_dir).spawn();
        }
    }

    #[cfg(target_os = "macos")]
    {
        let _ = temp_dir;
        if lower.ends_with(".zip") {
            let _ = std::process::Command::new("open")
                .arg("-R")
                .arg(save_path)
                .spawn();
        } else {
            let _ = std::process::Command::new("open").arg(save_path).spawn();
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = (lower, temp_dir);
    }
}

/// Swaps the AppImage at `current` for `downloaded`, keeping its path.
pub async fn replace_appimage_in_place(downloaded: &Path, current: &Path) -> bool {
    let staged = PathBuf::from(format!("{}.update", current.display()));
    if tokio::fs::copy(downloaded, &staged).await.is_err() {
        let _ = tokio::fs::remove_file(&staged).await;
        return false;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("chmod")
            .arg("+x")
            .arg(&staged)
            .status();
    }
    if tokio::fs::rename(&staged, current).await.is_err() {
        let _ = tokio::fs::remove_file(&staged).await;
        return false;
    }
    let _ = tokio::fs::remove_file(downloaded).await;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn semver_parsing_and_comparison() {
        let a = SemVer::parse("v2.6.0");
        let b = SemVer::parse("V2.6.1+12");
        let c = SemVer::parse("2.6.1-beta.1");
        let d = SemVer::parse("2.6.1+13");

        assert_eq!(a.to_string(), "2.6.0");
        assert_eq!(b.to_string(), "2.6.1+12");
        assert_eq!(c.major, 2);
        assert_eq!(c.minor, 6);
        assert_eq!(c.patch, 1);
        assert_eq!(c.build, 0);

        assert!(b.is_greater_than(&a));
        assert!(!a.is_greater_than(&b));
        assert!(d.is_greater_than(&b));
        assert!(b.is_greater_than(&c));
    }

    #[test]
    fn github_release_asset_selection() {
        let payload = json!({
            "tag_name": "v2.7.0",
            "name": "LastWave v2.7.0",
            "body": "Release notes",
            "html_url": "https://github.com/Clash-Projects/LastWave-Desktop/releases/tag/v2.7.0",
            "published_at": "2026-11-26T10:00:00Z",
            "assets": [
                {
                    "name": "LastWave-Portable-Windows.zip",
                    "browser_download_url": "https://example.com/portable.zip",
                    "size": 100
                },
                {
                    "name": "LastWave-Setup-2.7.0.exe",
                    "browser_download_url": "https://example.com/setup.exe",
                    "size": 200
                },
                {
                    "name": "LastWave-2.7.0.AppImage",
                    "browser_download_url": "https://example.com/app.AppImage",
                    "size": 300
                }
            ]
        });

        let info = AppUpdateInfo::from_github_json(&payload);
        assert_eq!(info.version, "2.7.0");
        assert_eq!(
            info.find_best_asset(Some(AppInstallType::WindowsInstalled))
                .map(|a| a.name.as_str()),
            Some("LastWave-Setup-2.7.0.exe")
        );
        assert_eq!(
            info.find_best_asset(Some(AppInstallType::WindowsPortable))
                .map(|a| a.name.as_str()),
            Some("LastWave-Portable-Windows.zip")
        );
        assert_eq!(
            info.find_best_asset(Some(AppInstallType::LinuxAppImage))
                .map(|a| a.name.as_str()),
            Some("LastWave-2.7.0.AppImage")
        );
    }
}
