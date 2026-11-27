use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::app_database::AppDatabase;
use super::library_index::well_formed_utf16;
use super::local_audio_tags::LocalAudioTags;
use super::playlists::{PlaylistRepository, StoredTrack};

pub const LOCAL_AUDIO_EXTENSIONS: [&str; 7] = [
    "mp3", "flac", "m4a", "ogg", "opus", "wav", "aac",
];

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalScanState {
    pub running: bool,
    pub label: String,
}

/// Scanned local track metadata before persisting into the `"Local songs"` playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedLocalTrack {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub artwork_url: String,
    pub playback_url: String,
    pub added_at_millis: i64,
}

/// Indexes folders the user picks and plays the files in place.
///
/// Rows live in the built-in "Local songs" playlist (`mode: local`).
/// That playlist is never uploaded to YouTube Music.
pub struct LocalMusicLibrary<'a> {
    db: &'a AppDatabase,
    app_support_dir: PathBuf,
}

impl<'a> LocalMusicLibrary<'a> {
    pub fn new(db: &'a AppDatabase, app_support_dir: impl Into<PathBuf>) -> Self {
        Self {
            db,
            app_support_dir: app_support_dir.into(),
        }
    }

    pub fn folders(&self) -> Vec<String> {
        self.db.local_folders()
    }

    pub fn add_folder_path(&self, path: &Path) -> bool {
        if !path.is_dir() {
            return false;
        }
        let abs = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.db.add_local_folder(&abs.to_string_lossy());
        true
    }

    pub fn forget_folder_path(&self, path: &str) {
        self.db.remove_local_folder(path);
    }

    pub fn cover_file(&self, source_path: &str, bytes: Option<&[u8]>) -> String {
        let Some(bytes) = bytes else {
            return String::new();
        };
        if bytes.len() < 32 {
            return String::new();
        }
        let dir = self.app_support_dir.join("local_art");
        if fs::create_dir_all(&dir).is_err() {
            return String::new();
        }
        let mut hasher = Md5::new();
        hasher.update(source_path.as_bytes());
        let hash = hex::encode(hasher.finalize());
        let png = bytes.len() > 3 && bytes[0] == 0x89 && bytes[1] == 0x50 && bytes[2] == 0x4E;
        let ext = if png { "png" } else { "jpg" };
        let file = dir.join(format!("{hash}.{ext}"));
        let needs_write = match fs::metadata(&file) {
            Ok(meta) => meta.len() != bytes.len() as u64,
            Err(_) => true,
        };
        if needs_write && fs::write(&file, bytes).is_err() {
            return String::new();
        }
        file.to_string_lossy().into_owned()
    }

    /// Scans all configured local folders recursively and returns sorted tracks.
    pub fn scan_tracks<F>(&self, mut report: F) -> Vec<ScannedLocalTrack>
    where
        F: FnMut(LocalScanState),
    {
        report(LocalScanState {
            running: true,
            label: "Scanning folders…".to_string(),
        });

        let mut tracks = Vec::new();
        let mut seen = HashSet::new();
        let mut found = 0usize;

        for folder in self.db.local_folders() {
            let root = Path::new(&folder);
            if !root.is_dir() {
                continue;
            }
            for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry.path();
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_lowercase)
                    .unwrap_or_default();
                if !LOCAL_AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                    continue;
                }
                let path_str = path.to_string_lossy().into_owned();
                if !seen.insert(path_str.to_lowercase()) {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let tags = LocalAudioTags::read(path, &name);
                let title = well_formed_utf16(if tags.title.is_empty() {
                    &name
                } else {
                    &tags.title
                });
                let artist = well_formed_utf16(if tags.artist.is_empty() {
                    "Local file"
                } else {
                    &tags.artist
                });
                let art = self.cover_file(&path_str, tags.cover_bytes.as_deref());
                let clean_album = well_formed_utf16(&tags.album);
                let album = if clean_album.is_empty() {
                    String::new()
                } else if tags.year.is_empty() {
                    clean_album
                } else {
                    format!("{clean_album} ({})", tags.year)
                };
                let added_at_millis = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                tracks.push(ScannedLocalTrack {
                    name: title,
                    artist,
                    album,
                    artwork_url: art,
                    playback_url: path_str,
                    added_at_millis,
                });
                found += 1;
                if found.is_multiple_of(40) {
                    report(LocalScanState {
                        running: true,
                        label: format!("Scanning folders… {found} files"),
                    });
                }
            }
        }

        tracks.sort_by(|a, b| {
            let by_artist = a.artist.to_lowercase().cmp(&b.artist.to_lowercase());
            if by_artist != std::cmp::Ordering::Equal {
                return by_artist;
            }
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        });

        report(LocalScanState {
            running: false,
            label: if tracks.is_empty() {
                "No audio files found".to_string()
            } else {
                format!("{} local songs", tracks.len())
            },
        });
        tracks
    }

    pub fn add_folder<F>(&self, path: &Path, playlists: &PlaylistRepository, report: F) -> usize
    where
        F: FnMut(LocalScanState),
    {
        if !self.add_folder_path(path) {
            return 0;
        }
        self.rescan(playlists, report)
    }

    pub fn forget_folder<F>(
        &self,
        path: &str,
        playlists: &PlaylistRepository,
        report: F,
    ) -> usize
    where
        F: FnMut(LocalScanState),
    {
        self.forget_folder_path(path);
        self.rescan(playlists, report)
    }

    pub fn rescan<F>(&self, playlists: &PlaylistRepository, report: F) -> usize
    where
        F: FnMut(LocalScanState),
    {
        let scanned = self.scan_tracks(report);
        let tracks: Vec<StoredTrack> = scanned
            .into_iter()
            .map(|t| StoredTrack {
                name: t.name,
                artist: t.artist,
                album: t.album,
                artwork_url: t.artwork_url,
                video_id: String::new(),
                duration_seconds: 0,
                playback_url: t.playback_url,
                added_at_millis: t.added_at_millis,
            })
            .collect();
        let count = tracks.len();
        let playlist = playlists.ensure_local_songs();
        playlists.replace_tracks(playlist.id, &tracks);
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn scans_audio_files_into_local_songs_playlist() {
        let dir = std::env::temp_dir().join(format!(
            "lastwave_local_lib_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let music_dir = dir.join("music");
        fs::create_dir_all(&music_dir).unwrap();
        fs::write(music_dir.join("Daft Punk - Voyager.mp3"), b"\xFF\xFB\x90\x00").unwrap();
        fs::write(music_dir.join("ignore.txt"), b"hello").unwrap();

        let db = Arc::new(AppDatabase::in_memory());
        let playlists = PlaylistRepository::new(Arc::clone(&db));
        let lib = LocalMusicLibrary::new(&db, dir.join("support"));
        let count = lib.add_folder(&music_dir, &playlists, |_| {});
        assert_eq!(count, 1);

        let local = playlists.ensure_local_songs();
        assert_eq!(local.tracks.len(), 1);
        assert_eq!(local.tracks[0].artist, "Daft Punk");
        assert_eq!(local.tracks[0].name, "Voyager");

        let _ = fs::remove_dir_all(&dir);
    }
}
