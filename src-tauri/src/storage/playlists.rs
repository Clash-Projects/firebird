use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::models::stream_models::PlayableTrack;

use super::app_database::AppDatabase;

pub const LIKED_MODE: &str = "liked";
pub const LIKED_TITLE: &str = "Liked Songs";
pub const LOCAL_MODE: &str = "local";
pub const LOCAL_TITLE: &str = "Local songs";
/// A playlist made in the Library from songs on this PC. Never mirrored
/// to YouTube Music (only `custom` is), and listed only in the Library.
pub const LIBRARY_MODE: &str = "library";

fn de_flexible_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(deserializer)?;
    Ok(match v {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse::<i64>().unwrap_or(0),
        _ => 0,
    })
}

/// Stored playlist track (JSON-serialised inside `saved_playlists`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTrack {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default, alias = "youtubeVideoId")]
    pub video_id: String,
    #[serde(default)]
    pub album: String,
    #[serde(default, deserialize_with = "de_flexible_i64")]
    pub duration_seconds: i64,
    /// On-disk path for a Local songs row. Empty for streamed tracks.
    #[serde(default)]
    pub playback_url: String,
    /// When the row was added. Zero on playlists saved before this field.
    #[serde(default, deserialize_with = "de_flexible_i64")]
    pub added_at_millis: i64,
}

impl StoredTrack {
    pub fn key(&self) -> String {
        format!("{}|{}", self.name.to_lowercase(), self.artist.to_lowercase())
    }

    pub fn to_playable(&self) -> PlayableTrack {
        PlayableTrack {
            title: self.name.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            artwork_url: self.artwork_url.clone(),
            video_id: self.video_id.clone(),
            playback_url: self.playback_url.clone(),
            playback_mime_type: String::new(),
            duration_ms: self.duration_seconds.max(0) * 1000,
        }
    }
}

/// Saved playlist entity. Mirrors Android `SavedPlaylist`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlaylist {
    pub id: i64,
    pub title: String,
    pub subtitle: String,
    pub mode: String,
    pub tracks: Vec<StoredTrack>,
    pub created_at_millis: i64,
    pub is_pinned: bool,
    pub custom_cover_uri: String,
}

impl SavedPlaylist {
    pub const LIKED_MODE: &'static str = LIKED_MODE;
    pub const LIKED_TITLE: &'static str = LIKED_TITLE;
    pub const LOCAL_MODE: &'static str = LOCAL_MODE;
    pub const LOCAL_TITLE: &'static str = LOCAL_TITLE;
    pub const LIBRARY_MODE: &'static str = LIBRARY_MODE;

    pub fn is_liked_songs(&self) -> bool {
        self.mode == LIKED_MODE
            || self.title.eq_ignore_ascii_case(LIKED_TITLE)
            || self.title.eq_ignore_ascii_case("liked music")
    }

    pub fn is_local(&self) -> bool {
        self.mode == LOCAL_MODE || self.title.eq_ignore_ascii_case(LOCAL_TITLE)
    }

    pub fn is_library(&self) -> bool {
        self.mode == LIBRARY_MODE
    }
}

/// Playlist + liked-songs repository backed by SQLite.
/// Mirrors Android `PlaylistRepository` + `LikedSongsManager`.
pub struct PlaylistRepository {
    db: Arc<AppDatabase>,
    state: RwLock<Vec<SavedPlaylist>>,
}

impl PlaylistRepository {
    pub fn new(db: Arc<AppDatabase>) -> Self {
        let repo = Self {
            db,
            state: RwLock::new(Vec::new()),
        };
        repo.refresh();
        repo.collapse_duplicate_liked_rows();
        repo
    }

    /// Current rows snapshot.
    pub fn all(&self) -> Vec<SavedPlaylist> {
        self.state.read().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn refresh(&self) {
        let rows: Vec<SavedPlaylist> = self.db.with_conn(|conn| {
            let Ok(mut stmt) = conn.prepare(
                "SELECT id, title, subtitle, mode, tracks_json, created_at_millis, \
                 is_pinned, custom_cover_uri FROM saved_playlists \
                 ORDER BY is_pinned DESC, created_at_millis DESC;",
            ) else {
                return Vec::new();
            };
            let Ok(mapped) = stmt.query_map([], |r| {
                let tracks_json: String = r.get::<_, Option<String>>(4)?.unwrap_or_default();
                let tracks: Vec<StoredTrack> =
                    serde_json::from_str::<Vec<StoredTrack>>(&tracks_json).unwrap_or_default();
                Ok(SavedPlaylist {
                    id: r.get(0)?,
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    subtitle: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    mode: r
                        .get::<_, Option<String>>(3)?
                        .unwrap_or_else(|| "custom".to_string()),
                    tracks,
                    created_at_millis: r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                    is_pinned: r.get::<_, Option<i64>>(6)?.unwrap_or(0) == 1,
                    custom_cover_uri: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                })
            }) else {
                return Vec::new();
            };
            mapped.filter_map(Result::ok).collect()
        });
        if let Ok(mut guard) = self.state.write() {
            *guard = rows;
        }
    }

    /// Collapses pre-existing duplicate Liked Songs rows into the one with the
    /// most tracks, merging distinct tracks and deleting leftovers from SQLite.
    pub fn collapse_duplicate_liked_rows(&self) {
        let mut liked: Vec<SavedPlaylist> = self
            .all()
            .into_iter()
            .filter(SavedPlaylist::is_liked_songs)
            .collect();
        if liked.len() <= 1 {
            return;
        }
        liked.sort_by(|a, b| {
            let cmp = b.tracks.len().cmp(&a.tracks.len());
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
            a.created_at_millis.cmp(&b.created_at_millis)
        });
        let keeper_id = liked[0].id;
        let other_ids: Vec<i64> = liked[1..].iter().map(|p| p.id).collect();

        let mut seen = HashSet::new();
        let mut merged = Vec::new();
        for p in liked {
            for t in p.tracks {
                if seen.insert(t.key()) {
                    merged.push(t);
                }
            }
        }

        let encoded = serde_json::to_string(&merged).unwrap_or_else(|_| "[]".to_string());
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "UPDATE saved_playlists SET title = ?, subtitle = ?, mode = ?, tracks_json = ? WHERE id = ?;",
                params![LIKED_TITLE, "Your loved tracks", LIKED_MODE, encoded, keeper_id],
            );
            for other_id in other_ids {
                let _ = conn.execute("DELETE FROM saved_playlists WHERE id = ?;", [other_id]);
            }
        });
        self.refresh();
    }

    fn new_id(&self) -> i64 {
        let now = Utc::now().timestamp_millis();
        let max_id: i64 = self.db.with_conn(|conn| {
            conn.query_row("SELECT MAX(id) FROM saved_playlists;", [], |r| {
                r.get::<_, Option<i64>>(0)
            })
            .ok()
            .flatten()
            .unwrap_or(0)
        });
        if now > max_id {
            now
        } else {
            max_id + 1
        }
    }

    pub fn ensure_liked_songs(&self) -> SavedPlaylist {
        self.collapse_duplicate_liked_rows();
        if let Some(existing) = self.all().into_iter().find(SavedPlaylist::is_liked_songs) {
            return existing;
        }
        let id = self.new_id();
        let now = Utc::now().timestamp_millis();
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO saved_playlists(id, title, subtitle, mode, tracks_json, created_at_millis, is_pinned) \
                 VALUES(?, ?, ?, ?, ?, ?, 1);",
                params![id, LIKED_TITLE, "Your loved tracks", LIKED_MODE, "[]", now],
            );
        });
        self.refresh();
        self.all()
            .into_iter()
            .find(SavedPlaylist::is_liked_songs)
            .expect("liked songs row just inserted")
    }

    pub fn liked_keys(&self) -> HashSet<String> {
        let Some(liked) = self.all().into_iter().find(SavedPlaylist::is_liked_songs) else {
            return HashSet::new();
        };
        liked.tracks.iter().map(StoredTrack::key).collect()
    }

    pub fn create_custom(&self, title: &str) -> SavedPlaylist {
        let clean = title.trim();
        if clean.eq_ignore_ascii_case(LIKED_TITLE) || clean.eq_ignore_ascii_case("liked music") {
            return self.ensure_liked_songs();
        }
        if let Some(existing) = self
            .all()
            .into_iter()
            .find(|p| p.mode == "custom" && p.title.eq_ignore_ascii_case(clean))
        {
            return existing;
        }

        let id = self.new_id();
        let now = Utc::now().timestamp_millis();
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO saved_playlists(id, title, mode, tracks_json, created_at_millis) \
                 VALUES(?, ?, ?, ?, ?);",
                params![id, clean, "custom", "[]", now],
            );
        });
        self.refresh();
        self.all()
            .into_iter()
            .find(|p| p.id == id)
            .expect("custom playlist just inserted")
    }

    pub fn rename(&self, id: i64, title: &str) {
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "UPDATE saved_playlists SET title = ? WHERE id = ?;",
                params![title.trim(), id],
            );
        });
        self.refresh();
    }

    pub fn set_pinned(&self, id: i64, pinned: bool) {
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "UPDATE saved_playlists SET is_pinned = ? WHERE id = ?;",
                params![if pinned { 1 } else { 0 }, id],
            );
        });
        self.refresh();
    }

    pub fn delete(&self, id: i64) {
        self.db.with_conn(|conn| {
            let _ = conn.execute("DELETE FROM saved_playlists WHERE id = ?;", [id]);
        });
        self.refresh();
    }

    pub fn add_track(&self, id: i64, track: StoredTrack) -> bool {
        let Some(playlist) = self.all().into_iter().find(|p| p.id == id) else {
            return false;
        };
        let key = track.key();
        if playlist.tracks.iter().any(|t| t.key() == key) {
            return false;
        }
        let stamped = if track.added_at_millis == 0 {
            StoredTrack {
                added_at_millis: Utc::now().timestamp_millis(),
                ..track
            }
        } else {
            track
        };
        let mut updated = playlist.tracks;
        updated.push(stamped);
        let encoded = serde_json::to_string(&updated).unwrap_or_else(|_| "[]".to_string());
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "UPDATE saved_playlists SET tracks_json = ? WHERE id = ?;",
                params![encoded, id],
            );
        });
        self.refresh();
        true
    }

    pub fn remove_track(&self, id: i64, track_key: &str) {
        let Some(playlist) = self.all().into_iter().find(|p| p.id == id) else {
            return;
        };
        let updated: Vec<StoredTrack> = playlist
            .tracks
            .into_iter()
            .filter(|t| t.key() != track_key)
            .collect();
        let encoded = serde_json::to_string(&updated).unwrap_or_else(|_| "[]".to_string());
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "UPDATE saved_playlists SET tracks_json = ? WHERE id = ?;",
                params![encoded, id],
            );
        });
        self.refresh();
    }

    /// One atomic edit: drop `remove_keys`, then append `add` (deduped by `name|artist`).
    pub fn apply_batch(&self, id: i64, add: &[StoredTrack], remove_keys: &HashSet<String>) {
        if add.is_empty() && remove_keys.is_empty() {
            return;
        }
        let Some(playlist) = self.all().into_iter().find(|p| p.id == id) else {
            return;
        };
        let mut updated: Vec<StoredTrack> = playlist
            .tracks
            .into_iter()
            .filter(|t| !remove_keys.contains(&t.key()))
            .collect();
        let mut seen: HashSet<String> = updated.iter().map(StoredTrack::key).collect();
        let now = Utc::now().timestamp_millis();
        for t in add {
            if !seen.insert(t.key()) {
                continue;
            }
            let mut copy = t.clone();
            if copy.added_at_millis == 0 {
                copy.added_at_millis = now;
            }
            updated.push(copy);
        }
        let encoded = serde_json::to_string(&updated).unwrap_or_else(|_| "[]".to_string());
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "UPDATE saved_playlists SET tracks_json = ? WHERE id = ?;",
                params![encoded, id],
            );
        });
        self.refresh();
    }

    pub fn replace_tracks(&self, id: i64, tracks: &[StoredTrack]) {
        let encoded = serde_json::to_string(tracks).unwrap_or_else(|_| "[]".to_string());
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "UPDATE saved_playlists SET tracks_json = ? WHERE id = ?;",
                params![encoded, id],
            );
        });
        self.refresh();
    }

    /// New Library playlist (`mode: library`).
    pub fn create_library(&self, title: &str) -> SavedPlaylist {
        let id = self.new_id();
        let now = Utc::now().timestamp_millis();
        let trimmed = title.trim();
        let clean = if trimmed.is_empty() {
            "New playlist"
        } else {
            trimmed
        };
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO saved_playlists(id, title, subtitle, mode, tracks_json, created_at_millis, is_pinned) \
                 VALUES(?, ?, ?, ?, ?, ?, 0);",
                params![id, clean, "On this PC", LIBRARY_MODE, "[]", now],
            );
        });
        self.refresh();
        self.all()
            .into_iter()
            .find(|p| p.id == id)
            .expect("library playlist just inserted")
    }

    pub fn ensure_local_songs(&self) -> SavedPlaylist {
        if let Some(existing) = self.all().into_iter().find(SavedPlaylist::is_local) {
            return existing;
        }
        let id = self.new_id();
        let now = Utc::now().timestamp_millis();
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO saved_playlists(id, title, subtitle, mode, tracks_json, created_at_millis, is_pinned) \
                 VALUES(?, ?, ?, ?, ?, ?, 0);",
                params![
                    id,
                    LOCAL_TITLE,
                    "Files from folders you added",
                    LOCAL_MODE,
                    "[]",
                    now
                ],
            );
        });
        self.refresh();
        self.all()
            .into_iter()
            .find(|p| p.id == id)
            .expect("local songs playlist just inserted")
    }

    pub fn save_generated(&self, title: &str, tracks: &[StoredTrack]) -> SavedPlaylist {
        let trimmed = title.trim();
        let clean = if trimmed.is_empty() {
            "Generated mix"
        } else {
            trimmed
        };
        if let Some(existing) = self
            .all()
            .into_iter()
            .find(|p| p.mode == "custom" && p.title.eq_ignore_ascii_case(clean))
        {
            self.replace_tracks(existing.id, tracks);
            return self
                .all()
                .into_iter()
                .find(|p| p.id == existing.id)
                .unwrap_or(existing);
        }
        let created = self.create_custom(clean);
        self.replace_tracks(created.id, tracks);
        self.all()
            .into_iter()
            .find(|p| p.id == created.id)
            .unwrap_or(created)
    }

    pub fn set_custom_cover(
        &self,
        id: i64,
        source_path: &Path,
        app_support_dir: &Path,
    ) -> std::io::Result<String> {
        let covers = app_support_dir.join("playlist_covers");
        fs::create_dir_all(&covers)?;
        let ext = source_path
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| !e.is_empty())
            .unwrap_or("jpg");
        let dest = covers.join(format!("{id}.{ext}"));
        fs::copy(source_path, &dest)?;
        let dest_str = dest.to_string_lossy().into_owned();
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "UPDATE saved_playlists SET custom_cover_uri = ? WHERE id = ?;",
                params![dest_str, id],
            );
        });
        self.refresh();
        Ok(dest_str)
    }

    /// Idempotent like toggle; returns new liked state.
    pub fn toggle_liked(&self, track: StoredTrack) -> bool {
        let liked = self.ensure_liked_songs();
        let key = track.key();
        let is_liked = liked.tracks.iter().any(|t| t.key() == key);
        if is_liked {
            self.remove_track(liked.id, &key);
            false
        } else {
            self.add_track(liked.id, track);
            true
        }
    }

    /// Insert or replace one playlist row from a backup.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_backup_row(
        &self,
        id: i64,
        title: &str,
        subtitle: &str,
        mode: &str,
        tracks_json: &str,
        created_at_millis: i64,
        is_pinned: bool,
        discover_signature: Option<&str>,
        custom_cover_uri: Option<&str>,
    ) {
        self.db.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO saved_playlists(\
                 id, title, subtitle, mode, tracks_json, created_at_millis, \
                 discover_signature, custom_cover_uri, is_pinned) \
                 VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(id) DO UPDATE SET \
                 title = excluded.title, subtitle = excluded.subtitle, \
                 mode = excluded.mode, tracks_json = excluded.tracks_json, \
                 created_at_millis = excluded.created_at_millis, \
                 discover_signature = excluded.discover_signature, \
                 custom_cover_uri = excluded.custom_cover_uri, \
                 is_pinned = excluded.is_pinned;",
                params![
                    id,
                    title,
                    subtitle,
                    mode,
                    tracks_json,
                    created_at_millis,
                    discover_signature,
                    custom_cover_uri,
                    if is_pinned { 1 } else { 0 },
                ],
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_duplicate_liked_rows_and_toggle_liked() {
        let db = Arc::new(AppDatabase::in_memory());
        db.with_conn(|conn| {
            let t1 = serde_json::to_string(&vec![StoredTrack {
                name: "Song A".to_string(),
                artist: "Artist 1".to_string(),
                ..Default::default()
            }])
            .unwrap();
            let t2 = serde_json::to_string(&vec![
                StoredTrack {
                    name: "Song A".to_string(),
                    artist: "Artist 1".to_string(),
                    ..Default::default()
                },
                StoredTrack {
                    name: "Song B".to_string(),
                    artist: "Artist 2".to_string(),
                    ..Default::default()
                },
            ])
            .unwrap();
            conn.execute(
                "INSERT INTO saved_playlists(id, title, mode, tracks_json, created_at_millis) VALUES(1, 'Liked Music', 'liked', ?, 100);",
                [t1],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO saved_playlists(id, title, mode, tracks_json, created_at_millis) VALUES(2, 'Liked Songs', 'liked', ?, 200);",
                [t2],
            )
            .unwrap();
        });

        let repo = PlaylistRepository::new(Arc::clone(&db));
        let all = repo.all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, 2);
        assert_eq!(all[0].tracks.len(), 2);

        let extra = StoredTrack {
            name: "Song C".to_string(),
            artist: "Artist 3".to_string(),
            ..Default::default()
        };
        assert!(repo.toggle_liked(extra.clone()));
        assert!(repo.liked_keys().contains("song c|artist 3"));
        assert!(!repo.toggle_liked(extra));
        assert!(!repo.liked_keys().contains("song c|artist 3"));
    }

    #[test]
    fn stored_track_parses_youtube_video_id_alias_and_string_ints() {
        let raw = r#"{"name":"T","artist":"A","youtubeVideoId":"vid9","durationSeconds":"180","addedAtMillis":"42"}"#;
        let parsed: StoredTrack = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.video_id, "vid9");
        assert_eq!(parsed.duration_seconds, 180);
        assert_eq!(parsed.added_at_millis, 42);
    }
}
