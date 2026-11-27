use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::models::artist_parser::split_artist_credits;

use super::diagnostics_log::DiagnosticsLog;

pub const SCHEMA_VERSION: i64 = 11;
pub const DEFAULT_CACHE_CAP: i64 = 100;
pub const LOCAL_PLAYS_CAP: i64 = 10000;

pub fn cache_cap_for(group: &str) -> i64 {
    match group {
        "home" => 12,
        "artist" => 150,
        "album" => 200,
        "playlist" => 100,
        "library" => 40,
        "lyrics" => 600,
        "lastfm" => 80,
        "genre" => 60,
        _ => DEFAULT_CACHE_CAP,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExclusionEntry {
    pub key: String,
    pub name: String,
    pub artist: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PodcastProgressRow {
    pub video_id: String,
    pub show_id: String,
    pub show_title: String,
    pub title: String,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub completed: bool,
    pub updated_at_millis: i64,
}

impl PodcastProgressRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            video_id: row.get::<_, Option<String>>("video_id")?.unwrap_or_default(),
            show_id: row.get::<_, Option<String>>("show_id")?.unwrap_or_default(),
            show_title: row
                .get::<_, Option<String>>("show_title")?
                .unwrap_or_default(),
            title: row.get::<_, Option<String>>("title")?.unwrap_or_default(),
            position_ms: row.get::<_, Option<i64>>("position_ms")?.unwrap_or(0),
            duration_ms: row.get::<_, Option<i64>>("duration_ms")?.unwrap_or(0),
            completed: row.get::<_, Option<i64>>("completed")?.unwrap_or(0) == 1,
            updated_at_millis: row
                .get::<_, Option<i64>>("updated_at_millis")?
                .unwrap_or(0),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotCacheEntry {
    pub value: String,
    pub saved_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotCacheStats {
    pub entries: i64,
    pub chars: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedCount {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackPlayCount {
    pub title: String,
    pub artist: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumPlayCount {
    pub name: String,
    pub artist: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyPlayCount {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalStatsSummary {
    pub total_plays: i64,
    pub total_duration_ms: i64,
    pub unmeasured_plays: i64,
    pub prev_plays: Option<i64>,
    pub prev_duration_ms: Option<i64>,
    pub prev_unmeasured_plays: Option<i64>,
    pub unique_artists: i64,
    pub unique_tracks: i64,
    pub top_artists: Vec<NamedCount>,
    pub top_tracks: Vec<TrackPlayCount>,
    pub top_albums: Vec<AlbumPlayCount>,
    pub hourly: HashMap<i32, i64>,
    pub daily: Vec<DailyPlayCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentPlayEntry {
    pub title: String,
    pub artist: String,
    pub at_millis: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamCacheRow {
    pub video_id: String,
    pub client_profile: String,
    pub itag: i64,
    pub url: String,
    pub headers_json: String,
    pub mime: String,
    pub bitrate_kbps: i64,
    pub codec: String,
    pub expires_at_ms: i64,
    pub cached_at_ms: i64,
    pub auth_scope: String,
    pub watchtime_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchCacheRow {
    pub key: String,
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork_url: String,
    pub duration_seconds: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtworkEntry {
    pub url: String,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtworkRecord {
    pub url: String,
    pub provider: String,
    pub timestamp_millis: i64,
}

/// Desktop persistent storage backed by SQLite (`rusqlite`).
///
/// Migrations are additive and never drop user data.
pub struct AppDatabase {
    conn: Mutex<Connection>,
}

impl AppDatabase {
    pub const SCHEMA_VERSION: i64 = SCHEMA_VERSION;
    pub const DEFAULT_CACHE_CAP: i64 = DEFAULT_CACHE_CAP;
    pub const LOCAL_PLAYS_CAP: i64 = LOCAL_PLAYS_CAP;

    /// Opens `<app_support_dir>/lastwave.db` with WAL sidecar recovery,
    /// corrupt DB quarantine, and in-memory fallback.
    pub fn open(app_support_dir: &Path) -> Self {
        let _ = fs::create_dir_all(app_support_dir);
        let file = app_support_dir.join("lastwave.db");
        Self::open_at(&file)
    }

    /// Opens a specific database file path with sidecar/quarantine recovery.
    pub fn open_at(file: &Path) -> Self {
        match Self::open_and_migrate(file) {
            Ok(db) => db,
            Err(first_err) => {
                DiagnosticsLog::info(&format!(
                    "AppDatabase.open failed ({first_err}); clearing WAL sidecars"
                ));
                Self::delete_sidecars(file);
                match Self::open_and_migrate(file) {
                    Ok(db) => db,
                    Err(second_err) => {
                        DiagnosticsLog::info(&format!(
                            "AppDatabase.open retry failed ({second_err}); quarantining DB"
                        ));
                        Self::quarantine_db(file);
                        match Self::open_and_migrate(file) {
                            Ok(db) => db,
                            Err(third_err) => {
                                DiagnosticsLog::info(&format!(
                                    "AppDatabase.open fallback to in-memory ({third_err})"
                                ));
                                Self::in_memory()
                            }
                        }
                    }
                }
            }
        }
    }

    fn open_and_migrate(file: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(file)?;
        let instance = Self {
            conn: Mutex::new(conn),
        };
        instance.migrate()?;
        Ok(instance)
    }

    fn delete_sidecars(file: &Path) {
        let base = file.to_string_lossy();
        for suffix in ["-wal", "-shm", "-journal"] {
            let sidecar = format!("{base}{suffix}");
            let path = Path::new(&sidecar);
            if path.exists() {
                let _ = fs::remove_file(path);
            }
        }
    }

    fn quarantine_db(file: &Path) {
        Self::delete_sidecars(file);
        if !file.exists() {
            return;
        }
        let backup_str = format!("{}.corrupt", file.to_string_lossy());
        let backup = Path::new(&backup_str);
        if backup.exists() {
            let _ = fs::remove_file(backup);
        }
        if fs::rename(file, backup).is_err() {
            let _ = fs::remove_file(file);
        }
    }

    /// In-memory instance for tests and emergency fallback.
    pub fn in_memory() -> Self {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let instance = Self {
            conn: Mutex::new(conn),
        };
        instance
            .migrate()
            .expect("in-memory sqlite migration must succeed");
        instance
    }

    /// Executes a closure with exclusive access to the underlying `rusqlite::Connection`.
    pub fn with_conn<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Connection) -> R,
    {
        let guard = self.conn.lock().expect("database mutex poisoned");
        f(&guard)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute_batch(
            "
            PRAGMA busy_timeout=5000;
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA cache_size=-2000;
            PRAGMA mmap_size=0;
            PRAGMA temp_store=FILE;
            ",
        )?;

        let version: i64 = conn.query_row("PRAGMA user_version;", [], |r| r.get(0))?;

        if version < 1 {
            Self::create_v1(&conn)?;
            conn.execute_batch("PRAGMA user_version=1;")?;
        }
        if version < 2 {
            Self::create_v2(&conn)?;
            conn.execute_batch("PRAGMA user_version=2;")?;
        }
        if version < 3 {
            Self::create_v3(&conn);
            conn.execute_batch("PRAGMA user_version=3;")?;
        }
        if version < 4 {
            Self::create_v4(&conn);
            conn.execute_batch("PRAGMA user_version=4;")?;
        }
        if version < 5 {
            Self::create_v5(&conn)?;
            conn.execute_batch("PRAGMA user_version=5;")?;
        }
        if version < 6 {
            Self::create_v6(&conn);
            conn.execute_batch("PRAGMA user_version=6;")?;
        }
        if version < 7 {
            Self::create_v7(&conn);
            conn.execute_batch("PRAGMA user_version=7;")?;
        }
        if version < 8 {
            Self::create_v8(&conn)?;
            conn.execute_batch("PRAGMA user_version=8;")?;
        }
        if version < 9 {
            Self::create_v9(&conn)?;
            conn.execute_batch("PRAGMA user_version=9;")?;
        }
        if version < 10 {
            Self::create_v10(&conn);
            conn.execute_batch("PRAGMA user_version=10;")?;
        }
        if version < 11 {
            Self::create_v11(&conn);
            conn.execute_batch("PRAGMA user_version=11;")?;
        }
        Ok(())
    }

    fn create_v1(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS artwork_cache (
                cache_key TEXT PRIMARY KEY,
                url TEXT NOT NULL DEFAULT '',
                provider TEXT NOT NULL DEFAULT '',
                timestamp_millis INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS recommendation_exclusions (
                track_key TEXT PRIMARY KEY,
                excluded_at_millis INTEGER NOT NULL DEFAULT 0,
                track_name TEXT NOT NULL DEFAULT '',
                artist_name TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS saved_playlists (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                subtitle TEXT NOT NULL DEFAULT '',
                mode TEXT NOT NULL DEFAULT 'custom',
                tracks_json TEXT NOT NULL DEFAULT '[]',
                created_at_millis INTEGER NOT NULL DEFAULT 0,
                discover_signature TEXT,
                custom_cover_uri TEXT,
                is_pinned INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS downloaded_tracks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                track_key TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                artist TEXT NOT NULL DEFAULT '',
                album TEXT NOT NULL DEFAULT '',
                artwork_url TEXT NOT NULL DEFAULT '',
                file_path TEXT NOT NULL DEFAULT '',
                file_size_bytes INTEGER NOT NULL DEFAULT 0,
                format_badge TEXT NOT NULL DEFAULT '',
                duration_ms INTEGER NOT NULL DEFAULT 0,
                bitrate_kbps INTEGER NOT NULL DEFAULT 0,
                is_lossless INTEGER NOT NULL DEFAULT 0,
                has_lyrics INTEGER NOT NULL DEFAULT 0,
                lrc_file_path TEXT NOT NULL DEFAULT '',
                downloaded_at_millis INTEGER NOT NULL DEFAULT 0
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_downloaded_track_key
            ON downloaded_tracks(track_key);
            CREATE TABLE IF NOT EXISTS kv_store (
                k TEXT PRIMARY KEY,
                v TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS playback_session (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                payload_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE IF NOT EXISTS search_history (
                query TEXT PRIMARY KEY,
                updated_at_millis INTEGER NOT NULL DEFAULT 0
            );
            ",
        )
    }

    fn create_v2(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS stream_cache (
                video_id TEXT NOT NULL,
                client_profile TEXT NOT NULL DEFAULT '',
                itag INTEGER NOT NULL DEFAULT -1,
                url TEXT NOT NULL DEFAULT '',
                headers_json TEXT NOT NULL DEFAULT '{}',
                mime TEXT NOT NULL DEFAULT '',
                bitrate_kbps INTEGER NOT NULL DEFAULT 0,
                codec TEXT NOT NULL DEFAULT '',
                expires_at_ms INTEGER NOT NULL DEFAULT 0,
                cached_at_ms INTEGER NOT NULL DEFAULT 0,
                auth_scope TEXT NOT NULL DEFAULT 'anonymous',
                watchtime_url TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (video_id, client_profile, itag, auth_scope)
            );
            CREATE INDEX IF NOT EXISTS idx_stream_cache_video
            ON stream_cache(video_id, cached_at_ms DESC);
            CREATE TABLE IF NOT EXISTS match_cache (
                key TEXT PRIMARY KEY,
                video_id TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL DEFAULT '',
                artist TEXT NOT NULL DEFAULT '',
                album TEXT NOT NULL DEFAULT '',
                artwork_url TEXT NOT NULL DEFAULT '',
                updated_at_ms INTEGER NOT NULL DEFAULT 0
            );
            ",
        )
    }

    fn create_v3(conn: &Connection) {
        let _ = conn.execute(
            "ALTER TABLE match_cache ADD COLUMN duration_seconds INTEGER NOT NULL DEFAULT 0;",
            [],
        );
    }

    fn create_v4(conn: &Connection) {
        let _ = conn.execute("DELETE FROM match_cache;", []);
    }

    fn create_v5(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS local_plays (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL DEFAULT '',
                artist TEXT NOT NULL DEFAULT '',
                played_at_millis INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_local_plays_recent
            ON local_plays(played_at_millis DESC);
            ",
        )
    }

    /// v6: pre-signed YT watchtime base on cached streams.
    fn create_v6(conn: &Connection) {
        let _ = conn.execute(
            "ALTER TABLE stream_cache ADD COLUMN watchtime_url TEXT NOT NULL DEFAULT '';",
            [],
        );
    }

    /// v7: tagging outcome per download (`full` / `text` / `raw:<reason>`).
    fn create_v7(conn: &Connection) {
        let _ = conn.execute(
            "ALTER TABLE downloaded_tracks ADD COLUMN tag_status TEXT NOT NULL DEFAULT '';",
            [],
        );
    }

    /// v8: podcast resume positions and watched local-music folders.
    fn create_v8(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS podcast_progress (
                video_id TEXT PRIMARY KEY,
                show_id TEXT NOT NULL DEFAULT '',
                show_title TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL DEFAULT '',
                position_ms INTEGER NOT NULL DEFAULT 0,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                completed INTEGER NOT NULL DEFAULT 0,
                updated_at_millis INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS local_folders (
                path TEXT PRIMARY KEY,
                added_at_millis INTEGER NOT NULL DEFAULT 0
            );
            ",
        )
    }

    /// v9: capped, LRU-pruned snapshot cache.
    fn create_v9(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS snapshot_cache (
                k TEXT PRIMARY KEY,
                grp TEXT NOT NULL,
                v TEXT NOT NULL,
                saved_at INTEGER NOT NULL DEFAULT 0,
                hit_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_snapshot_grp_hit
            ON snapshot_cache(grp, hit_at);
            ",
        )?;
        for prefix in ["artist_detail_v2|", "home_feed_v1|", "home_yt_v1|"] {
            let _ = conn.execute(
                "DELETE FROM kv_store WHERE k LIKE ?;",
                params![format!("{prefix}%")],
            );
        }
        Ok(())
    }

    /// v10: album name on local plays.
    fn create_v10(conn: &Connection) {
        let _ = conn.execute(
            "ALTER TABLE local_plays ADD COLUMN album TEXT NOT NULL DEFAULT '';",
            [],
        );
    }

    /// v11: track duration in milliseconds on local plays.
    fn create_v11(conn: &Connection) {
        let _ = conn.execute(
            "ALTER TABLE local_plays ADD COLUMN duration_ms INTEGER NOT NULL DEFAULT 0;",
            [],
        );
    }

    // -- kv helpers ----------------------------------------------------------

    pub fn kv_get(&self, key: &str) -> Option<String> {
        self.with_conn(|conn| {
            conn.query_row("SELECT v FROM kv_store WHERE k = ?;", [key], |r| r.get(0))
                .optional()
                .ok()
                .flatten()
        })
    }

    pub fn kv_set(&self, key: &str, value: &str) {
        self.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO kv_store(k, v) VALUES(?, ?) \
                 ON CONFLICT(k) DO UPDATE SET v = excluded.v;",
                params![key, value],
            );
        });
    }

    // -- search history ------------------------------------------------------

    pub fn load_search_history(&self, limit: i64) -> Vec<String> {
        self.with_conn(|conn| {
            let Ok(mut stmt) = conn.prepare(
                "SELECT query FROM search_history ORDER BY updated_at_millis DESC LIMIT ?;",
            ) else {
                return Vec::new();
            };
            let Ok(rows) = stmt.query_map([limit], |r| r.get::<_, String>(0)) else {
                return Vec::new();
            };
            rows.filter_map(Result::ok).collect()
        })
    }

    pub fn push_search_history(&self, query: &str) {
        let now = now_millis();
        self.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO search_history(query, updated_at_millis) VALUES(?, ?) \
                 ON CONFLICT(query) DO UPDATE SET updated_at_millis = excluded.updated_at_millis;",
                params![query, now],
            );
            let _ = conn.execute(
                "DELETE FROM search_history WHERE query NOT IN (\
                 SELECT query FROM search_history ORDER BY updated_at_millis DESC LIMIT 25);",
                [],
            );
        });
    }

    pub fn clear_search_history(&self) {
        self.with_conn(|conn| {
            let _ = conn.execute("DELETE FROM search_history;", []);
        });
    }

    pub fn remove_search_history(&self, query: &str) {
        self.with_conn(|conn| {
            let _ = conn.execute("DELETE FROM search_history WHERE query = ?;", [query]);
        });
    }

    // -- recommendation exclusions ("don't recommend") -----------------------

    pub fn exclusion_key(name: &str, artist: &str) -> String {
        format!("{}|{}", name.to_lowercase(), artist.to_lowercase())
    }

    pub fn load_exclusion_keys(&self) -> HashSet<String> {
        self.with_conn(|conn| {
            let Ok(mut stmt) = conn.prepare("SELECT track_key FROM recommendation_exclusions;")
            else {
                return HashSet::new();
            };
            let Ok(rows) = stmt.query_map([], |r| r.get::<_, Option<String>>(0)) else {
                return HashSet::new();
            };
            rows.filter_map(Result::ok)
                .flatten()
                .filter(|k| !k.is_empty())
                .collect()
        })
    }

    pub fn add_exclusion(&self, name: &str, artist: &str) {
        let key = Self::exclusion_key(name, artist);
        let now = now_millis();
        self.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO recommendation_exclusions \
                 (track_key, excluded_at_millis, track_name, artist_name) \
                 VALUES (?, ?, ?, ?);",
                params![key, now, name, artist],
            );
        });
    }

    pub fn remove_exclusion(&self, key: &str) {
        self.with_conn(|conn| {
            let _ = conn.execute(
                "DELETE FROM recommendation_exclusions WHERE track_key = ?;",
                [key],
            );
        });
    }

    pub fn list_exclusions(&self) -> Vec<ExclusionEntry> {
        self.with_conn(|conn| {
            let Ok(mut stmt) = conn.prepare(
                "SELECT track_key, track_name, artist_name \
                 FROM recommendation_exclusions ORDER BY excluded_at_millis DESC;",
            ) else {
                return Vec::new();
            };
            let Ok(rows) = stmt.query_map([], |r| {
                Ok(ExclusionEntry {
                    key: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    name: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    artist: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                })
            }) else {
                return Vec::new();
            };
            rows.filter_map(Result::ok)
                .filter(|e| !e.key.is_empty())
                .collect()
        })
    }

    // -- podcast progress ----------------------------------------------------

    pub fn upsert_podcast_progress(
        &self,
        video_id: &str,
        show_id: &str,
        show_title: &str,
        title: &str,
        duration_ms: i64,
    ) {
        if video_id.is_empty() {
            return;
        }
        let existing = self.podcast_progress(video_id);
        let pos = existing.as_ref().map(|e| e.position_ms).unwrap_or(0);
        let dur = if duration_ms > 0 {
            duration_ms
        } else {
            existing.as_ref().map(|e| e.duration_ms).unwrap_or(0)
        };
        let completed = existing.as_ref().is_some_and(|e| e.completed);
        let updated = existing
            .as_ref()
            .map(|e| e.updated_at_millis)
            .unwrap_or_else(now_millis);

        self.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO podcast_progress(\
                 video_id, show_id, show_title, title, position_ms, duration_ms, \
                 completed, updated_at_millis) VALUES(?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(video_id) DO UPDATE SET \
                 show_id = CASE WHEN excluded.show_id = '' THEN podcast_progress.show_id ELSE excluded.show_id END, \
                 show_title = CASE WHEN excluded.show_title = '' THEN podcast_progress.show_title ELSE excluded.show_title END, \
                 title = CASE WHEN excluded.title = '' THEN podcast_progress.title ELSE excluded.title END, \
                 duration_ms = CASE WHEN excluded.duration_ms = 0 THEN podcast_progress.duration_ms ELSE excluded.duration_ms END;",
                params![
                    video_id,
                    show_id,
                    show_title,
                    title,
                    pos,
                    dur,
                    if completed { 1 } else { 0 },
                    updated,
                ],
            );
        });
    }

    pub fn podcast_progress(&self, video_id: &str) -> Option<PodcastProgressRow> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT * FROM podcast_progress WHERE video_id = ?;",
                [video_id],
                PodcastProgressRow::from_row,
            )
            .optional()
            .ok()
            .flatten()
        })
    }

    pub fn podcast_progress_for(&self, ids: &[String]) -> HashMap<String, PodcastProgressRow> {
        let list: Vec<&str> = ids.iter().map(String::as_str).filter(|s| !s.is_empty()).collect();
        if list.is_empty() {
            return HashMap::new();
        }
        let marks = vec!["?"; list.len()].join(",");
        let sql = format!("SELECT * FROM podcast_progress WHERE video_id IN ({marks});");
        self.with_conn(|conn| {
            let Ok(mut stmt) = conn.prepare(&sql) else {
                return HashMap::new();
            };
            let Ok(rows) = stmt.query_map(params_from_iter(list.iter().copied()), PodcastProgressRow::from_row) else {
                return HashMap::new();
            };
            rows.filter_map(Result::ok)
                .map(|row| (row.video_id.clone(), row))
                .collect()
        })
    }

    /// Position to resume from. Completed episodes and ones within 15s of
    /// the end start over.
    pub fn podcast_resume_ms(&self, video_id: &str) -> i64 {
        let Some(row) = self.podcast_progress(video_id) else {
            return 0;
        };
        if row.completed {
            return 0;
        }
        if row.duration_ms > 0 && row.position_ms > row.duration_ms - 15000 {
            return 0;
        }
        row.position_ms
    }

    pub fn save_podcast_position(&self, video_id: &str, position_ms: i64, duration_ms: i64) {
        if video_id.is_empty() {
            return;
        }
        let Some(existing) = self.podcast_progress(video_id) else {
            return;
        };
        let duration = if duration_ms > 0 {
            duration_ms
        } else {
            existing.duration_ms
        };
        let completed = duration > 0 && (position_ms as f64) > (duration as f64) * 0.95;
        let now = now_millis();
        self.with_conn(|conn| {
            let _ = conn.execute(
                "UPDATE podcast_progress SET position_ms = ?, duration_ms = ?, \
                 completed = ?, updated_at_millis = ? WHERE video_id = ?;",
                params![
                    position_ms.max(0),
                    duration,
                    if completed { 1 } else { 0 },
                    now,
                    video_id,
                ],
            );
        });
    }

    // -- local folders -------------------------------------------------------

    pub fn local_folders(&self) -> Vec<String> {
        self.with_conn(|conn| {
            let Ok(mut stmt) =
                conn.prepare("SELECT path FROM local_folders ORDER BY added_at_millis ASC;")
            else {
                return Vec::new();
            };
            let Ok(rows) = stmt.query_map([], |r| r.get::<_, Option<String>>(0)) else {
                return Vec::new();
            };
            rows.filter_map(Result::ok)
                .flatten()
                .filter(|p| !p.is_empty())
                .collect()
        })
    }

    pub fn add_local_folder(&self, path: &str) {
        let now = now_millis();
        self.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO local_folders(path, added_at_millis) VALUES(?, ?) \
                 ON CONFLICT(path) DO NOTHING;",
                params![path, now],
            );
        });
    }

    pub fn remove_local_folder(&self, path: &str) {
        self.with_conn(|conn| {
            let _ = conn.execute("DELETE FROM local_folders WHERE path = ?;", [path]);
        });
    }

    // -- playback session ----------------------------------------------------

    pub fn load_playback_session(&self) -> Map<String, Value> {
        let raw: Option<String> = self.with_conn(|conn| {
            conn.query_row(
                "SELECT payload_json FROM playback_session WHERE id = 1;",
                [],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten()
        });
        let Some(json) = raw else {
            return Map::new();
        };
        match serde_json::from_str::<Value>(&json) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        }
    }

    pub fn save_playback_session(&self, payload: &Map<String, Value>) {
        if let Ok(encoded) = serde_json::to_string(payload) {
            self.with_conn(|conn| {
                let _ = conn.execute(
                    "INSERT INTO playback_session(id, payload_json) VALUES(1, ?) \
                     ON CONFLICT(id) DO UPDATE SET payload_json = excluded.payload_json;",
                    [encoded],
                );
            });
        }
    }

    pub fn clear_playback_session(&self) {
        self.with_conn(|conn| {
            let _ = conn.execute("DELETE FROM playback_session WHERE id = 1;", []);
        });
    }

    // -- snapshot cache ------------------------------------------------------

    pub fn cache_get(&self, key: &str, now_ms: Option<i64>) -> Option<SnapshotCacheEntry> {
        let at = now_ms.unwrap_or_else(now_millis);
        self.with_conn(|conn| {
            let row = conn
                .query_row(
                    "SELECT v, saved_at FROM snapshot_cache WHERE k = ?;",
                    [key],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                )
                .optional()
                .ok()
                .flatten()?;
            let _ = conn.execute(
                "UPDATE snapshot_cache SET hit_at = ? WHERE k = ?;",
                params![at, key],
            );
            Some(SnapshotCacheEntry {
                value: row.0,
                saved_at_ms: row.1,
            })
        })
    }

    pub fn cache_put(&self, key: &str, group: &str, value: &str, now_ms: Option<i64>) {
        let at = now_ms.unwrap_or_else(now_millis);
        let cap = cache_cap_for(group);
        self.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO snapshot_cache(k, grp, v, saved_at, hit_at) \
                 VALUES(?, ?, ?, ?, ?) \
                 ON CONFLICT(k) DO UPDATE SET grp = excluded.grp, v = excluded.v, \
                 saved_at = excluded.saved_at, hit_at = excluded.hit_at;",
                params![key, group, value, at, at],
            );
            let _ = conn.execute(
                "DELETE FROM snapshot_cache WHERE grp = ? AND k NOT IN (\
                 SELECT k FROM snapshot_cache WHERE grp = ? \
                 ORDER BY hit_at DESC LIMIT ?);",
                params![group, group, cap],
            );
        });
    }

    pub fn cache_delete(&self, key: &str) {
        self.with_conn(|conn| {
            let _ = conn.execute("DELETE FROM snapshot_cache WHERE k = ?;", [key]);
        });
    }

    pub fn cache_clear(&self, group: Option<&str>) {
        self.with_conn(|conn| match group {
            None => {
                let _ = conn.execute("DELETE FROM snapshot_cache;", []);
            }
            Some(g) => {
                let _ = conn.execute("DELETE FROM snapshot_cache WHERE grp = ?;", [g]);
            }
        });
    }

    pub fn cache_stats(&self) -> HashMap<String, SnapshotCacheStats> {
        self.with_conn(|conn| {
            let Ok(mut stmt) = conn.prepare(
                "SELECT grp, COUNT(*) AS n, COALESCE(SUM(LENGTH(v)), 0) AS chars \
                 FROM snapshot_cache GROUP BY grp;",
            ) else {
                return HashMap::new();
            };
            let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    SnapshotCacheStats {
                        entries: r.get(1)?,
                        chars: r.get(2)?,
                    },
                ))
            }) else {
                return HashMap::new();
            };
            rows.filter_map(Result::ok).collect()
        })
    }

    // -- local play log (keyless-guest taste) --------------------------------

    pub fn record_local_play(&self, title: &str, artist: &str, album: &str, duration_ms: i64) {
        let t = title.trim();
        let a = artist.trim();
        let al = album.trim();
        let dur = duration_ms.max(0);
        if t.is_empty() || a.is_empty() {
            return;
        }
        let now = now_millis();
        self.with_conn(|conn| {
            let last: Option<(String, String, i64)> = conn
                .query_row(
                    "SELECT title, artist, played_at_millis FROM local_plays \
                     ORDER BY id DESC LIMIT 1;",
                    [],
                    |r| {
                        Ok((
                            r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                            r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                            r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .ok()
                .flatten();

            if let Some((lt, la, lat)) = last {
                if lt == t && la == a && now - lat < 10 * 60 * 1000 {
                    return;
                }
            }

            let _ = conn.execute(
                "INSERT INTO local_plays (title, artist, album, duration_ms, played_at_millis) \
                 VALUES (?, ?, ?, ?, ?);",
                params![t, a, al, dur, now],
            );
            let _ = conn.execute(
                "DELETE FROM local_plays WHERE id NOT IN (\
                 SELECT id FROM local_plays ORDER BY id DESC LIMIT ?);",
                [LOCAL_PLAYS_CAP],
            );
        });
    }

    pub fn update_latest_local_play_duration(&self, title: &str, artist: &str, duration_ms: i64) {
        let t = title.trim();
        let a = artist.trim();
        if t.is_empty() || a.is_empty() || duration_ms <= 0 {
            return;
        }
        self.with_conn(|conn| {
            let last: Option<(i64, String, String, i64)> = conn
                .query_row(
                    "SELECT id, title, artist, duration_ms FROM local_plays \
                     ORDER BY id DESC LIMIT 1;",
                    [],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                            r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                            r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                        ))
                    },
                )
                .optional()
                .ok()
                .flatten();

            if let Some((id, lt, la, dur)) = last {
                if lt == t && la == a && dur <= 0 {
                    let _ = conn.execute(
                        "UPDATE local_plays SET duration_ms = ? WHERE id = ?;",
                        params![duration_ms, id],
                    );
                }
            }
        });
    }

    pub fn get_local_stats_summary(
        &self,
        since_millis: Option<i64>,
        prev_since_millis: Option<i64>,
    ) -> Option<LocalStatsSummary> {
        self.with_conn(|conn| {
            let where_clause = if let Some(s) = since_millis {
                format!("WHERE played_at_millis >= {s}")
            } else {
                String::new()
            };

            let (total_plays, total_duration_ms, unmeasured_plays): (i64, i64, i64) = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) as cnt, \
                         COALESCE(SUM(CASE WHEN duration_ms > 0 THEN duration_ms ELSE 0 END), 0) as dur_ms, \
                         COALESCE(SUM(CASE WHEN duration_ms <= 0 THEN 1 ELSE 0 END), 0) as est_cnt \
                         FROM local_plays {where_clause};"
                    ),
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .ok()?;

            let (prev_plays, prev_duration_ms, prev_unmeasured_plays) =
                if let (Some(since), Some(prev)) = (since_millis, prev_since_millis) {
                    let row: (i64, i64, i64) = conn
                        .query_row(
                            "SELECT COUNT(*) as cnt, \
                             COALESCE(SUM(CASE WHEN duration_ms > 0 THEN duration_ms ELSE 0 END), 0) as dur_ms, \
                             COALESCE(SUM(CASE WHEN duration_ms <= 0 THEN 1 ELSE 0 END), 0) as est_cnt \
                             FROM local_plays WHERE played_at_millis >= ? AND played_at_millis < ?;",
                            params![prev, since],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                        )
                        .ok()?;
                    (Some(row.0), Some(row.1), Some(row.2))
                } else {
                    (None, None, None)
                };

            let unique_tracks: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(DISTINCT lower(title || '|' || artist)) as tracks \
                         FROM local_plays {where_clause};"
                    ),
                    [],
                    |r| r.get(0),
                )
                .ok()?;

            let mut artist_counts: HashMap<String, i64> = HashMap::new();
            {
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT artist, COUNT(*) as cnt FROM local_plays {where_clause} GROUP BY artist;"
                    ))
                    .ok()?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                            r.get::<_, i64>(1)?,
                        ))
                    })
                    .ok()?;
                for row in rows.flatten() {
                    for single in split_artist_credits(&row.0) {
                        *artist_counts.entry(single).or_insert(0) += row.1;
                    }
                }
            }
            let unique_artists = artist_counts.len() as i64;
            let mut sorted_artists: Vec<(String, i64)> = artist_counts.into_iter().collect();
            sorted_artists.sort_by(|a, b| b.1.cmp(&a.1));
            let top_artists: Vec<NamedCount> = sorted_artists
                .into_iter()
                .take(100)
                .map(|(name, count)| NamedCount { name, count })
                .collect();

            let top_tracks: Vec<TrackPlayCount> = {
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT title, artist, COUNT(*) as cnt FROM local_plays {where_clause} \
                         GROUP BY lower(title || '|' || artist) ORDER BY cnt DESC LIMIT 100;"
                    ))
                    .ok()?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok(TrackPlayCount {
                            title: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                            artist: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                            count: r.get(2)?,
                        })
                    })
                    .ok()?;
                rows.flatten().filter(|t| !t.title.is_empty()).collect()
            };

            let album_where_clause = if where_clause.is_empty() {
                "WHERE album != ''".to_string()
            } else {
                format!("{where_clause} AND album != ''")
            };
            let top_albums: Vec<AlbumPlayCount> = {
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT album, artist, COUNT(*) as cnt FROM local_plays {album_where_clause} \
                         GROUP BY lower(album || '|' || artist) ORDER BY cnt DESC LIMIT 100;"
                    ))
                    .ok()?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok(AlbumPlayCount {
                            name: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                            artist: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                            count: r.get(2)?,
                        })
                    })
                    .ok()?;
                rows.flatten().filter(|a| !a.name.is_empty()).collect()
            };

            let mut hourly: HashMap<i32, i64> = (0..24).map(|h| (h, 0)).collect();
            {
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT strftime('%H', played_at_millis / 1000, 'unixepoch', 'localtime') as hr, \
                         COUNT(*) as cnt FROM local_plays {where_clause} GROUP BY hr ORDER BY hr ASC;"
                    ))
                    .ok()?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, Option<String>>(0)?,
                            r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        ))
                    })
                    .ok()?;
                for (hr_str, cnt) in rows.flatten() {
                    if let Some(s) = hr_str {
                        if let Ok(hr) = s.parse::<i32>() {
                            hourly.insert(hr, cnt);
                        }
                    }
                }
            }

            let daily: Vec<DailyPlayCount> = {
                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT strftime('%Y-%m-%d', played_at_millis / 1000, 'unixepoch', 'localtime') as dt, \
                         COUNT(*) as cnt FROM local_plays {where_clause} GROUP BY dt ORDER BY dt DESC LIMIT 90;"
                    ))
                    .ok()?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok(DailyPlayCount {
                            date: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                            count: r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        })
                    })
                    .ok()?;
                rows.flatten().filter(|d| !d.date.is_empty()).collect()
            };

            Some(LocalStatsSummary {
                total_plays,
                total_duration_ms,
                unmeasured_plays,
                prev_plays,
                prev_duration_ms,
                prev_unmeasured_plays,
                unique_artists,
                unique_tracks,
                top_artists,
                top_tracks,
                top_albums,
                hourly,
                daily,
            })
        })
    }

    pub fn load_active_play_dates(&self) -> HashSet<String> {
        self.with_conn(|conn| {
            let Ok(mut stmt) = conn.prepare(
                "SELECT DISTINCT strftime('%Y-%m-%d', played_at_millis / 1000, 'unixepoch', 'localtime') AS dt \
                 FROM local_plays;",
            ) else {
                return HashSet::new();
            };
            let Ok(rows) = stmt.query_map([], |r| r.get::<_, Option<String>>(0)) else {
                return HashSet::new();
            };
            rows.filter_map(Result::ok)
                .flatten()
                .filter(|d| !d.is_empty())
                .collect()
        })
    }

    pub fn load_recent_plays(&self, limit: i64, since_millis: Option<i64>) -> Vec<RecentPlayEntry> {
        self.with_conn(|conn| {
            let where_clause = if let Some(s) = since_millis {
                format!("WHERE played_at_millis >= {s}")
            } else {
                String::new()
            };
            let sql = format!(
                "SELECT title, artist, played_at_millis FROM local_plays \
                 {where_clause} ORDER BY id DESC LIMIT ?;"
            );
            let Ok(mut stmt) = conn.prepare(&sql) else {
                return Vec::new();
            };
            let Ok(rows) = stmt.query_map([limit], |r| {
                Ok(RecentPlayEntry {
                    title: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    artist: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    at_millis: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                })
            }) else {
                return Vec::new();
            };
            rows.filter_map(Result::ok)
                .filter(|p| !p.title.is_empty())
                .collect()
        })
    }

    pub fn load_plays_for_date(&self, date: &str) -> Vec<RecentPlayEntry> {
        self.with_conn(|conn| {
            let Ok(mut stmt) = conn.prepare(
                "SELECT title, artist, played_at_millis FROM local_plays \
                 WHERE strftime('%Y-%m-%d', played_at_millis / 1000, 'unixepoch', 'localtime') = ? \
                 ORDER BY id DESC LIMIT 100;",
            ) else {
                return Vec::new();
            };
            let Ok(rows) = stmt.query_map([date], |r| {
                Ok(RecentPlayEntry {
                    title: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    artist: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    at_millis: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                })
            }) else {
                return Vec::new();
            };
            rows.filter_map(Result::ok)
                .filter(|p| !p.title.is_empty())
                .collect()
        })
    }

    // -- stream disk cache ---------------------------------------------------

    pub fn load_stream_entries(&self, limit: i64) -> Vec<StreamCacheRow> {
        self.with_conn(|conn| {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT video_id, client_profile, itag, url, headers_json, \
                 mime, bitrate_kbps, codec, expires_at_ms, cached_at_ms, \
                 auth_scope, watchtime_url FROM stream_cache \
                 ORDER BY cached_at_ms DESC LIMIT ?;",
            ) {
                if let Ok(rows) = stmt.query_map([limit], |r| {
                    Ok(StreamCacheRow {
                        video_id: r.get(0)?,
                        client_profile: r.get(1)?,
                        itag: r.get(2)?,
                        url: r.get(3)?,
                        headers_json: r.get(4)?,
                        mime: r.get(5)?,
                        bitrate_kbps: r.get(6)?,
                        codec: r.get(7)?,
                        expires_at_ms: r.get(8)?,
                        cached_at_ms: r.get(9)?,
                        auth_scope: r.get(10)?,
                        watchtime_url: r.get::<_, Option<String>>(11)?.unwrap_or_default(),
                    })
                }) {
                    return rows.filter_map(Result::ok).collect();
                }
            }

            let Ok(mut stmt) = conn.prepare(
                "SELECT video_id, client_profile, itag, url, headers_json, \
                 mime, bitrate_kbps, codec, expires_at_ms, cached_at_ms, \
                 auth_scope FROM stream_cache \
                 ORDER BY cached_at_ms DESC LIMIT ?;",
            ) else {
                return Vec::new();
            };
            let Ok(rows) = stmt.query_map([limit], |r| {
                Ok(StreamCacheRow {
                    video_id: r.get(0)?,
                    client_profile: r.get(1)?,
                    itag: r.get(2)?,
                    url: r.get(3)?,
                    headers_json: r.get(4)?,
                    mime: r.get(5)?,
                    bitrate_kbps: r.get(6)?,
                    codec: r.get(7)?,
                    expires_at_ms: r.get(8)?,
                    cached_at_ms: r.get(9)?,
                    auth_scope: r.get(10)?,
                    watchtime_url: String::new(),
                })
            }) else {
                return Vec::new();
            };
            rows.filter_map(Result::ok).collect()
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_stream_entry(
        &self,
        video_id: &str,
        client_profile: &str,
        itag: i64,
        url: &str,
        headers_json: &str,
        mime: &str,
        bitrate_kbps: i64,
        codec: &str,
        expires_at_ms: i64,
        cached_at_ms: i64,
        auth_scope: &str,
        watchtime_url: &str,
    ) {
        self.with_conn(|conn| {
            let v6_res = conn.execute(
                "INSERT INTO stream_cache(video_id, client_profile, itag, url, \
                 headers_json, mime, bitrate_kbps, codec, expires_at_ms, \
                 cached_at_ms, auth_scope, watchtime_url) \
                 VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(video_id, client_profile, itag, auth_scope) \
                 DO UPDATE SET url = excluded.url, \
                 headers_json = excluded.headers_json, mime = excluded.mime, \
                 bitrate_kbps = excluded.bitrate_kbps, codec = excluded.codec, \
                 expires_at_ms = excluded.expires_at_ms, \
                 cached_at_ms = excluded.cached_at_ms, \
                 watchtime_url = excluded.watchtime_url;",
                params![
                    video_id,
                    client_profile,
                    itag,
                    url,
                    headers_json,
                    mime,
                    bitrate_kbps,
                    codec,
                    expires_at_ms,
                    cached_at_ms,
                    auth_scope,
                    watchtime_url,
                ],
            );
            if v6_res.is_err() {
                let _ = conn.execute(
                    "INSERT INTO stream_cache(video_id, client_profile, itag, url, \
                     headers_json, mime, bitrate_kbps, codec, expires_at_ms, \
                     cached_at_ms, auth_scope) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                     ON CONFLICT(video_id, client_profile, itag, auth_scope) \
                     DO UPDATE SET url = excluded.url, \
                     headers_json = excluded.headers_json, mime = excluded.mime, \
                     bitrate_kbps = excluded.bitrate_kbps, codec = excluded.codec, \
                     expires_at_ms = excluded.expires_at_ms, \
                     cached_at_ms = excluded.cached_at_ms;",
                    params![
                        video_id,
                        client_profile,
                        itag,
                        url,
                        headers_json,
                        mime,
                        bitrate_kbps,
                        codec,
                        expires_at_ms,
                        cached_at_ms,
                        auth_scope,
                    ],
                );
            }
            let _ = conn.execute(
                "DELETE FROM stream_cache WHERE rowid NOT IN (\
                 SELECT rowid FROM stream_cache ORDER BY cached_at_ms DESC LIMIT 256);",
                [],
            );
        });
    }

    pub fn delete_stream_entries(&self, video_id: &str) {
        self.with_conn(|conn| {
            let _ = conn.execute("DELETE FROM stream_cache WHERE video_id = ?;", [video_id]);
        });
    }

    pub fn prune_expired_streams(&self, now_ms: i64) {
        self.with_conn(|conn| {
            let _ = conn.execute(
                "DELETE FROM stream_cache WHERE expires_at_ms > 0 AND expires_at_ms < ?;",
                [now_ms],
            );
        });
    }

    // -- match cache ---------------------------------------------------------

    pub fn load_match_entry(&self, key: &str) -> Option<MatchCacheRow> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT key, video_id, title, artist, album, artwork_url, \
                 duration_seconds, updated_at_ms FROM match_cache WHERE key = ?;",
                [key],
                |r| {
                    Ok(MatchCacheRow {
                        key: r.get(0)?,
                        video_id: r.get(1)?,
                        title: r.get(2)?,
                        artist: r.get(3)?,
                        album: r.get(4)?,
                        artwork_url: r.get(5)?,
                        duration_seconds: r.get(6)?,
                        updated_at_ms: r.get(7)?,
                    })
                },
            )
            .optional()
            .ok()
            .flatten()
        })
    }

    pub fn save_match_entry(
        &self,
        key: &str,
        video_id: &str,
        title: &str,
        artist: &str,
        album: &str,
        artwork_url: &str,
        duration_seconds: i64,
    ) {
        let now = now_millis();
        self.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO match_cache(key, video_id, title, artist, album, \
                 artwork_url, duration_seconds, updated_at_ms) \
                 VALUES(?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(key) DO UPDATE SET video_id = excluded.video_id, \
                 title = excluded.title, artist = excluded.artist, \
                 album = excluded.album, artwork_url = excluded.artwork_url, \
                 duration_seconds = excluded.duration_seconds, \
                 updated_at_ms = excluded.updated_at_ms;",
                params![
                    key,
                    video_id,
                    title,
                    artist,
                    album,
                    artwork_url,
                    duration_seconds,
                    now,
                ],
            );
            let _ = conn.execute(
                "DELETE FROM match_cache WHERE key NOT IN (\
                 SELECT key FROM match_cache ORDER BY updated_at_ms DESC LIMIT 1024);",
                [],
            );
        });
    }

    pub fn delete_matches_for_video(&self, video_id: &str) {
        self.with_conn(|conn| {
            let _ = conn.execute("DELETE FROM match_cache WHERE video_id = ?;", [video_id]);
        });
    }

    // -- artwork cache -------------------------------------------------------

    pub fn load_artwork_entry(&self, cache_key: &str) -> Option<ArtworkEntry> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT url, provider FROM artwork_cache WHERE cache_key = ?;",
                [cache_key],
                |r| {
                    Ok(ArtworkEntry {
                        url: r.get(0)?,
                        provider: r.get(1)?,
                    })
                },
            )
            .optional()
            .ok()
            .flatten()
        })
    }

    pub fn load_artwork_record(&self, cache_key: &str) -> Option<ArtworkRecord> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT url, provider, timestamp_millis FROM artwork_cache WHERE cache_key = ?;",
                [cache_key],
                |r| {
                    Ok(ArtworkRecord {
                        url: r.get(0)?,
                        provider: r.get(1)?,
                        timestamp_millis: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    })
                },
            )
            .optional()
            .ok()
            .flatten()
        })
    }

    pub fn save_artwork_entry(&self, cache_key: &str, url: &str, provider: &str) {
        let now = now_millis();
        self.with_conn(|conn| {
            let _ = conn.execute(
                "INSERT INTO artwork_cache(cache_key, url, provider, timestamp_millis) \
                 VALUES(?, ?, ?, ?) \
                 ON CONFLICT(cache_key) DO UPDATE SET \
                 url = excluded.url, \
                 provider = excluded.provider, \
                 timestamp_millis = excluded.timestamp_millis;",
                params![cache_key, url, provider, now],
            );
        });
    }

    /// Flush WAL log to disk and truncate it to reduce file handles and memory.
    pub fn checkpoint(&self) {
        self.with_conn(|conn| {
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        });
    }
}

fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lastwave_db_test_{}_{}_{}",
            label,
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn schema_version_and_tables_created() {
        let db = AppDatabase::in_memory();
        let version: i64 = db.with_conn(|c| {
            c.query_row("PRAGMA user_version;", [], |r| r.get(0))
                .unwrap()
        });
        assert_eq!(version, SCHEMA_VERSION);

        let tables: HashSet<String> = db.with_conn(|c| {
            let mut stmt = c
                .prepare("SELECT name FROM sqlite_master WHERE type='table';")
                .unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .filter_map(Result::ok)
                .collect()
        });
        for expected in [
            "artwork_cache",
            "recommendation_exclusions",
            "saved_playlists",
            "downloaded_tracks",
            "kv_store",
            "playback_session",
            "search_history",
            "stream_cache",
            "match_cache",
            "local_plays",
            "podcast_progress",
            "local_folders",
            "snapshot_cache",
        ] {
            assert!(tables.contains(expected), "missing table {expected}");
        }
    }

    #[test]
    fn local_play_deduplication_and_stats_aggregation() {
        let db = AppDatabase::in_memory();
        // Immediate repeat of same track within 10 minutes is deduplicated.
        db.record_local_play("Song One", "Drake, Future", "Album A", 0);
        db.record_local_play("Song One", "Drake, Future", "Album A", 180_000);
        db.update_latest_local_play_duration("Song One", "Drake, Future", 200_000);
        db.record_local_play("Song Two", "Tyler, The Creator", "IGOR", 190_000);

        let summary = db.get_local_stats_summary(None, None).unwrap();
        assert_eq!(summary.total_plays, 2);
        assert_eq!(summary.total_duration_ms, 390_000);
        assert_eq!(summary.unmeasured_plays, 0);
        assert_eq!(summary.unique_tracks, 2);
        assert_eq!(summary.unique_artists, 3);
        assert!(summary.top_artists.iter().any(|a| a.name == "Drake"));
        assert!(summary.top_artists.iter().any(|a| a.name == "Future"));
        assert!(summary
            .top_artists
            .iter()
            .any(|a| a.name == "Tyler, The Creator"));
    }

    #[test]
    fn snapshot_cache_lru_and_corrupt_db_quarantine() {
        let dir = temp_dir("quarantine");
        let db_path = dir.join("lastwave.db");
        fs::write(&db_path, b"not a sqlite file").unwrap();

        let db = AppDatabase::open(&dir);
        assert!(dir.join("lastwave.db.corrupt").exists());

        // Group 'home' cap is 12: inserting 15 items keeps 12 newest by hit_at.
        for i in 0..15 {
            db.cache_put(&format!("k{i}"), "home", "payload", Some(1000 + i));
        }
        // Touch k3 so it becomes most recently used before adding k15.
        assert!(db.cache_get("k3", Some(2000)).is_some());
        db.cache_put("k15", "home", "payload", Some(2001));
        assert!(db.cache_get("k3", Some(2002)).is_some());
        assert_eq!(db.cache_stats().get("home").unwrap().entries, 12);

        drop(db);
        let _ = fs::remove_dir_all(&dir);
    }
}
