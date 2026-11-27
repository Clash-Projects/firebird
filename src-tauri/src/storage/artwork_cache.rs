use std::fs::{self, FileTimes, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

/// Cache namespace matching Dart's `ArtworkCache._key`.
pub const CACHE_KEY: &str = "lw_artwork_v2";

/// ~2500 covers at 50–250 KB each ≈ 125–600 MB worst case on disk.
pub const MAX_OBJECTS: usize = 2500;

/// Cached artwork stale period (60 days).
pub const STALE_PERIOD_DAYS: u64 = 60;
pub const STALE_PERIOD: Duration = Duration::from_secs(STALE_PERIOD_DAYS * 24 * 60 * 60);

/// Decode edges every artwork request snaps UP to (physical px).
///
/// A coarse ladder keeps one URL + one decoded image per size class, so an
/// animating or resizing tile never mints a new network request or a new
/// decode per frame.
pub const LADDER: [u32; 8] = [64, 128, 256, 384, 512, 640, 768, 1024];

/// `needed` physical px -> the ladder step at or above it (capped at the
/// top step). Returns `None` for non-positive or NaN input.
pub fn bucket(needed: f64) -> Option<u32> {
    if needed.is_nan() || needed <= 0.0 {
        return None;
    }
    for &step in &LADDER {
        if needed <= f64::from(step) {
            return Some(step);
        }
    }
    LADDER.last().copied()
}

/// Single disk cache for artwork, rooted at `<app_support_dir>/cache/lw_artwork_v2`
/// instead of `%TEMP%` (which OS storage cleanup purges).
///
/// Stores one original file per URL on disk (never disk-resized PNGs) and
/// enforces the 60-day stale period and `MAX_OBJECTS` (2500) LRU budget.
pub struct ArtworkCache {
    dir: PathBuf,
    max_objects: usize,
    stale_period: Duration,
    lock: Mutex<()>,
}

impl ArtworkCache {
    pub const CACHE_KEY: &'static str = CACHE_KEY;
    pub const MAX_OBJECTS: usize = MAX_OBJECTS;
    pub const STALE_PERIOD: Duration = STALE_PERIOD;
    pub const LADDER: [u32; 8] = LADDER;

    /// Creates an `ArtworkCache` rooted at `<app_support_dir>/cache/lw_artwork_v2`.
    /// Directory creation is deferred until first file access.
    pub fn new(app_support_dir: &Path) -> Self {
        Self::with_limits(app_support_dir, MAX_OBJECTS, STALE_PERIOD)
    }

    /// Creates an `ArtworkCache` with custom object-count and TTL limits.
    pub fn with_limits(
        app_support_dir: &Path,
        max_objects: usize,
        stale_period: Duration,
    ) -> Self {
        Self {
            dir: app_support_dir.join("cache").join(CACHE_KEY),
            max_objects,
            stale_period,
            lock: Mutex::new(()),
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.dir
    }

    pub fn bucket(needed: f64) -> Option<u32> {
        bucket(needed)
    }

    /// Deterministic cache filename for a URL.
    pub fn cache_filename(url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        format!("{}.img", hex::encode(hasher.finalize()))
    }

    pub fn path_for(&self, url: &str) -> PathBuf {
        self.dir.join(Self::cache_filename(url))
    }

    fn ensure_dir(&self) -> std::io::Result<()> {
        if !self.dir.exists() {
            fs::create_dir_all(&self.dir)?;
        }
        Ok(())
    }

    fn is_stale(&self, modified: SystemTime, now: SystemTime) -> bool {
        now.duration_since(modified)
            .map(|age| age > self.stale_period)
            .unwrap_or(false)
    }

    fn touch(path: &Path, now: SystemTime) {
        if let Ok(file) = OpenOptions::new().write(true).open(path) {
            let times = FileTimes::new().set_modified(now);
            let _ = file.set_times(times);
        }
    }

    /// Returns the cached bytes for `url` if present and fresh; removes stale
    /// entries on read and touches the mtime on hit for LRU ordering.
    pub fn get(&self, url: &str) -> Option<Vec<u8>> {
        let _guard = self.lock.lock().ok()?;
        let path = self.path_for(url);
        let meta = fs::metadata(&path).ok()?;
        if !meta.is_file() {
            return None;
        }
        let now = SystemTime::now();
        if let Ok(modified) = meta.modified() {
            if self.is_stale(modified, now) {
                let _ = fs::remove_file(&path);
                return None;
            }
        }
        let bytes = fs::read(&path).ok()?;
        Self::touch(&path, now);
        Some(bytes)
    }

    /// Returns the path to a fresh cached file for `url`, or `None` if absent/stale.
    pub fn get_path(&self, url: &str) -> Option<PathBuf> {
        let _guard = self.lock.lock().ok()?;
        let path = self.path_for(url);
        let meta = fs::metadata(&path).ok()?;
        if !meta.is_file() {
            return None;
        }
        let now = SystemTime::now();
        if let Ok(modified) = meta.modified() {
            if self.is_stale(modified, now) {
                let _ = fs::remove_file(&path);
                return None;
            }
        }
        Self::touch(&path, now);
        Some(path)
    }

    /// Stores `bytes` for `url` atomically and enforces the cache budget.
    pub fn put(&self, url: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
        let _guard = self.lock.lock().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::Other, "artwork cache mutex poisoned")
        })?;
        self.ensure_dir()?;
        let target = self.path_for(url);
        let tmp = self
            .dir
            .join(format!("{}.{}.tmp", Self::cache_filename(url), std::process::id()));

        {
            let mut f = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&tmp)?;
            f.write_all(bytes)?;
            f.flush()?;
        }
        if fs::rename(&tmp, &target).is_err() {
            let _ = fs::remove_file(&target);
            if let Err(e) = fs::rename(&tmp, &target) {
                let _ = fs::remove_file(&tmp);
                return Err(e);
            }
        }

        self.evict_locked();
        Ok(target)
    }

    pub fn remove(&self, url: &str) {
        if let Ok(_guard) = self.lock.lock() {
            let _ = fs::remove_file(self.path_for(url));
        }
    }

    pub fn clear(&self) {
        if let Ok(_guard) = self.lock.lock() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    /// Evicts stale entries and trims oldest-modified files above `max_objects`.
    /// Returns the number of evicted files.
    pub fn evict(&self) -> usize {
        let Ok(_guard) = self.lock.lock() else {
            return 0;
        };
        self.evict_locked()
    }

    fn evict_locked(&self) -> usize {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return 0;
        };
        let now = SystemTime::now();
        let mut evicted = 0usize;
        let mut live: Vec<(PathBuf, SystemTime)> = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "tmp")
            {
                let _ = fs::remove_file(&path);
                continue;
            }
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if self.is_stale(modified, now) {
                if fs::remove_file(&path).is_ok() {
                    evicted += 1;
                }
            } else {
                live.push((path, modified));
            }
        }

        if live.len() > self.max_objects {
            // Sort oldest-modified first so LRU entries are evicted first.
            live.sort_by_key(|(_, modified)| *modified);
            let to_remove = live.len() - self.max_objects;
            for (path, _) in live.into_iter().take(to_remove) {
                if fs::remove_file(path).is_ok() {
                    evicted += 1;
                }
            }
        }

        evicted
    }

    pub fn object_count(&self) -> usize {
        let Ok(_guard) = self.lock.lock() else {
            return 0;
        };
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return 0;
        };
        entries
            .flatten()
            .filter(|e| {
                e.metadata().is_ok_and(|m| m.is_file())
                    && e.path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        != Some("tmp")
            })
            .count()
    }

    pub fn total_bytes(&self) -> u64 {
        let Ok(_guard) = self.lock.lock() else {
            return 0;
        };
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return 0;
        };
        entries
            .flatten()
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                if m.is_file() {
                    Some(m.len())
                } else {
                    None
                }
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lastwave_art_test_{}_{}",
            label,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ladder_bucket_snaps_up_and_caps() {
        assert_eq!(bucket(-1.0), None);
        assert_eq!(bucket(0.0), None);
        assert_eq!(bucket(f64::NAN), None);
        assert_eq!(bucket(1.0), Some(64));
        assert_eq!(bucket(64.0), Some(64));
        assert_eq!(bucket(65.0), Some(128));
        assert_eq!(bucket(500.0), Some(512));
        assert_eq!(bucket(2048.0), Some(1024));
    }

    #[test]
    fn disk_cache_put_get_and_lru_eviction() {
        let dir = temp_dir("evict");
        let cache = ArtworkCache::with_limits(&dir, 2, Duration::from_secs(3600));
        assert_eq!(cache.cache_dir(), dir.join("cache").join(CACHE_KEY));

        cache.put("https://img/1", b"one").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        cache.put("https://img/2", b"two").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        // Hit url 1 so url 2 becomes the oldest.
        assert_eq!(cache.get("https://img/1").as_deref(), Some(b"one".as_slice()));
        std::thread::sleep(Duration::from_millis(20));

        cache.put("https://img/3", b"three").unwrap();
        assert_eq!(cache.object_count(), 2);
        assert!(cache.get("https://img/1").is_some());
        assert!(cache.get("https://img/3").is_some());
        assert!(cache.get("https://img/2").is_none());

        let _ = fs::remove_dir_all(&dir);
    }
}
