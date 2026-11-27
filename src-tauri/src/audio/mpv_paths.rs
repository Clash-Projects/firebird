use std::path::{Path, PathBuf};

/// Demuxer on-disk cache dir (mpv `cache-dir`). mpv's default location
/// fails to create on some setups (`Failed to create file cache` on
/// every open), so the app pins an explicit dir it creates itself.
pub fn mpv_cache_dir() -> PathBuf {
    std::env::temp_dir().join("lastwave").join("mpv-cache")
}

/// String form of [`mpv_cache_dir`].
pub fn mpv_cache_dir_path() -> String {
    mpv_cache_dir().to_string_lossy().into_owned()
}

/// Best-effort recursive mkdir. Never fails/panics; empty or whitespace-only
/// path is a no-op.
pub fn ensure_dir_exists(path: &Path) {
    let raw = path.to_string_lossy();
    if raw.trim().is_empty() {
        return;
    }
    let _ = std::fs::create_dir_all(path);
}

/// Async best-effort recursive mkdir matching Dart's `Future<void> ensureDirExists(String path)`.
pub async fn ensure_dir_exists_async(path: &str) {
    if path.trim().is_empty() {
        return;
    }
    let _ = tokio::fs::create_dir_all(path).await;
}
