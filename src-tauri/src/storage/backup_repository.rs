use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::models::app_info::APP_VERSION;

use super::app_database::AppDatabase;
use super::playlists::{PlaylistRepository, SavedPlaylist};
use super::prefs::Prefs;

pub const BACKUP_TYPE: &str = "lastwave-backup";
pub const BACKUP_SCHEMA: i64 = 8;
pub const HOME_SHELF_LAYOUT_KEY: &str = "home_shelf_layout";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPlaylist {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default = "default_custom_mode")]
    pub mode: String,
    #[serde(default = "default_empty_tracks")]
    pub tracks_json: String,
    #[serde(default)]
    pub created_at_millis: i64,
    #[serde(default)]
    pub discover_signature: Option<String>,
    #[serde(default)]
    pub custom_cover_uri: Option<String>,
    #[serde(default)]
    pub is_pinned: bool,
}

fn default_custom_mode() -> String {
    "custom".to_string()
}

fn default_empty_tracks() -> String {
    "[]".to_string()
}

impl BackupPlaylist {
    pub fn from_json_map(map: &Map<String, Value>) -> Self {
        Self {
            id: map.get("id").and_then(Value::as_i64).unwrap_or(0),
            title: val_to_string(map.get("title")),
            subtitle: val_to_string(map.get("subtitle")),
            mode: map
                .get("mode")
                .and_then(|v| (!v.is_null()).then(|| val_to_string(Some(v))))
                .unwrap_or_else(default_custom_mode),
            tracks_json: map
                .get("tracksJson")
                .and_then(|v| (!v.is_null()).then(|| val_to_string(Some(v))))
                .unwrap_or_else(default_empty_tracks),
            created_at_millis: map
                .get("createdAtMillis")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            discover_signature: map
                .get("discoverSignature")
                .and_then(|v| (!v.is_null()).then(|| val_to_string(Some(v)))),
            custom_cover_uri: map
                .get("customCoverUri")
                .and_then(|v| (!v.is_null()).then(|| val_to_string(Some(v)))),
            is_pinned: map.get("isPinned") == Some(&Value::Bool(true)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackupPreview {
    pub playlists: Vec<BackupPlaylist>,
    pub newer_local: Vec<BackupPlaylist>,
    pub exclusion_count: usize,
    pub home_shelf_layout: Option<String>,
    pub prefs: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BackupRead {
    Ok(BackupPreview),
    Invalid,
    Unsupported,
}

pub struct BackupRepository;

impl Default for BackupRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl BackupRepository {
    pub fn new() -> Self {
        Self
    }

    pub fn build(
        &self,
        prefs: &Prefs,
        db: &AppDatabase,
        playlists: &[SavedPlaylist],
    ) -> String {
        let mut strings = Map::new();
        let mut booleans = Map::new();
        let mut integers = Map::new();
        let mut doubles = Map::new();
        let mut lists = Map::new();

        for key in prefs.portable_pref_keys() {
            let Some(value) = prefs.portable_pref(&key) else {
                continue;
            };
            match value {
                Value::String(s) => {
                    strings.insert(key, Value::String(s));
                }
                Value::Bool(b) => {
                    booleans.insert(key, Value::Bool(b));
                }
                Value::Number(n) => {
                    if n.is_i64() || n.is_u64() {
                        integers.insert(key, Value::Number(n));
                    } else {
                        doubles.insert(key, Value::Number(n));
                    }
                }
                Value::Array(arr) => {
                    let strs: Vec<Value> = arr
                        .into_iter()
                        .map(|item| Value::String(val_to_string(Some(&item))))
                        .collect();
                    lists.insert(key, Value::Array(strs));
                }
                Value::Null | Value::Object(_) => {}
            }
        }

        let exclusions: Vec<Value> = db.with_conn(|conn| {
            let Ok(mut stmt) = conn.prepare(
                "SELECT track_key, excluded_at_millis, track_name, artist_name \
                 FROM recommendation_exclusions;",
            ) else {
                return Vec::new();
            };
            let Ok(rows) = stmt.query_map([], |r| {
                Ok(json!({
                    "trackKey": r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    "excludedAtMillis": r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    "trackName": r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    "artistName": r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                }))
            }) else {
                return Vec::new();
            };
            rows.filter_map(Result::ok).collect()
        });

        let playlist_entries: Vec<Value> = playlists
            .iter()
            .map(|p| {
                let tracks_json =
                    serde_json::to_string(&p.tracks).unwrap_or_else(|_| "[]".to_string());
                json!({
                    "id": p.id,
                    "title": p.title,
                    "subtitle": p.subtitle,
                    "mode": p.mode,
                    "tracksJson": tracks_json,
                    "createdAtMillis": p.created_at_millis,
                    "discoverSignature": Value::Null,
                    "customCoverUri": p.custom_cover_uri,
                    "isPinned": p.is_pinned,
                })
            })
            .collect();

        let file = json!({
            "type": BACKUP_TYPE,
            "schemaVersion": BACKUP_SCHEMA,
            "createdAt": Utc::now().timestamp_millis(),
            "appVersion": APP_VERSION,
            "prefs": {
                "strings": strings,
                "booleans": booleans,
                "integers": integers,
                "doubles": doubles,
                "stringSets": lists,
            },
            "playlists": playlist_entries,
            "recommendationExclusions": exclusions,
            "homeShelfLayout": db.kv_get(HOME_SHELF_LAYOUT_KEY).unwrap_or_default(),
        });

        serde_json::to_string_pretty(&file).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn read(&self, content: &str, local: &[SavedPlaylist]) -> BackupRead {
        let Ok(Value::Object(decoded)) = serde_json::from_str::<Value>(content) else {
            return BackupRead::Invalid;
        };
        if decoded.get("type").and_then(Value::as_str) != Some(BACKUP_TYPE) {
            return BackupRead::Invalid;
        }
        let version = decoded
            .get("schemaVersion")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if version > BACKUP_SCHEMA {
            return BackupRead::Unsupported;
        }

        let mut playlists = Vec::new();
        if let Some(Value::Array(arr)) = decoded.get("playlists") {
            for item in arr {
                if let Some(map) = item.as_object() {
                    playlists.push(BackupPlaylist::from_json_map(map));
                }
            }
        }

        let by_id: HashMap<i64, &SavedPlaylist> = local.iter().map(|p| (p.id, p)).collect();
        let mut newer_local = Vec::new();
        for incoming in &playlists {
            if let Some(existing) = by_id.get(&incoming.id) {
                if existing.created_at_millis > incoming.created_at_millis {
                    newer_local.push(incoming.clone());
                }
            }
        }

        let exclusion_count = decoded
            .get("recommendationExclusions")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);

        let home_shelf_layout = decoded
            .get("homeShelfLayout")
            .and_then(|v| (!v.is_null()).then(|| val_to_string(Some(v))));

        let prefs = decoded
            .get("prefs")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        BackupRead::Ok(BackupPreview {
            playlists,
            newer_local,
            exclusion_count,
            home_shelf_layout,
            prefs,
        })
    }

    pub fn restore(
        &self,
        preview: &BackupPreview,
        prefs: &Prefs,
        db: &AppDatabase,
        playlists: &PlaylistRepository,
        overwrite_newer: bool,
    ) -> usize {
        self.restore_prefs(prefs, &preview.prefs);
        let blocked: HashSet<i64> = preview.newer_local.iter().map(|p| p.id).collect();
        let mut applied = 0usize;

        for playlist in &preview.playlists {
            if playlist.id == 0 {
                continue;
            }
            if !overwrite_newer && blocked.contains(&playlist.id) {
                continue;
            }
            playlists.upsert_backup_row(
                playlist.id,
                &playlist.title,
                &playlist.subtitle,
                &playlist.mode,
                &playlist.tracks_json,
                playlist.created_at_millis,
                playlist.is_pinned,
                playlist.discover_signature.as_deref(),
                playlist.custom_cover_uri.as_deref(),
            );
            applied += 1;
        }
        playlists.refresh();

        if let Some(layout) = preview.home_shelf_layout.as_deref() {
            if !layout.trim().is_empty() {
                db.kv_set(HOME_SHELF_LAYOUT_KEY, layout);
            }
        }
        applied
    }

    pub fn restore_file(
        &self,
        content: &str,
        prefs: &Prefs,
        db: &AppDatabase,
        playlists: &PlaylistRepository,
        overwrite_newer: bool,
    ) -> Result<usize, String> {
        let BackupRead::Ok(preview) = self.read(content, &playlists.all()) else {
            return Err("That file is not a LastWave backup.".to_string());
        };
        let applied = self.restore(&preview, prefs, db, playlists, overwrite_newer);

        let Ok(Value::Object(decoded)) = serde_json::from_str::<Value>(content) else {
            return Ok(applied);
        };
        if let Some(Value::Array(exclusions)) = decoded.get("recommendationExclusions") {
            db.with_conn(|conn| {
                for item in exclusions {
                    let Some(map) = item.as_object() else {
                        continue;
                    };
                    let key = val_to_string(map.get("trackKey"));
                    if key.is_empty() {
                        continue;
                    }
                    let at = map
                        .get("excludedAtMillis")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    let name = val_to_string(map.get("trackName"));
                    let artist = val_to_string(map.get("artistName"));
                    let _ = conn.execute(
                        "INSERT INTO recommendation_exclusions(\
                         track_key, excluded_at_millis, track_name, artist_name) \
                         VALUES(?, ?, ?, ?) \
                         ON CONFLICT(track_key) DO UPDATE SET \
                         excluded_at_millis = excluded.excluded_at_millis, \
                         track_name = excluded.track_name, \
                         artist_name = excluded.artist_name;",
                        params![key, at, name, artist],
                    );
                }
            });
        }
        Ok(applied)
    }

    fn restore_prefs(&self, prefs: &Prefs, raw: &Map<String, Value>) {
        fn apply(prefs: &Prefs, bucket: Option<&Value>) {
            let Some(map) = bucket.and_then(Value::as_object) else {
                return;
            };
            for (key, value) in map {
                if value.is_null() {
                    continue;
                }
                prefs.set_portable_pref(key, value.clone());
            }
        }

        apply(prefs, raw.get("strings"));
        apply(prefs, raw.get("booleans"));
        apply(prefs, raw.get("integers"));
        apply(prefs, raw.get("longs"));
        apply(prefs, raw.get("doubles"));

        if let Some(sets) = raw.get("stringSets").and_then(Value::as_object) {
            for (key, value) in sets {
                if value.is_array() {
                    prefs.set_portable_pref(key, value.clone());
                }
            }
        }
    }

    pub fn write_file(&self, path: &Path, json: &str) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, json)
    }
}

fn val_to_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn backup_build_read_and_restore_roundtrip() {
        let db = Arc::new(AppDatabase::in_memory());
        let prefs = Prefs::in_memory();
        prefs.set_amoled(true);
        prefs.save_session("secret-token", "alice");
        db.add_exclusion("Track X", "Artist Y");
        db.kv_set(HOME_SHELF_LAYOUT_KEY, "{\"order\":[\"mixes\"]}");

        let playlists = PlaylistRepository::new(Arc::clone(&db));
        let p = playlists.create_custom("My Mix");
        playlists.add_track(
            p.id,
            crate::storage::playlists::StoredTrack {
                name: "Track 1".to_string(),
                artist: "Artist 1".to_string(),
                ..Default::default()
            },
        );

        let repo = BackupRepository::new();
        let json_str = repo.build(&prefs, &db, &playlists.all());
        assert!(!json_str.contains("secret-token"));

        let db2 = Arc::new(AppDatabase::in_memory());
        let prefs2 = Prefs::in_memory();
        let playlists2 = PlaylistRepository::new(Arc::clone(&db2));
        let applied = repo
            .restore_file(&json_str, &prefs2, &db2, &playlists2, true)
            .unwrap();
        assert_eq!(applied, 1);
        assert!(prefs2.amoled());
        assert_eq!(playlists2.all().len(), 1);
        assert_eq!(playlists2.all()[0].title, "My Mix");
        assert!(db2.load_exclusion_keys().contains("track x|artist y"));
        assert_eq!(
            db2.kv_get(HOME_SHELF_LAYOUT_KEY).as_deref(),
            Some("{\"order\":[\"mixes\"]}")
        );
    }
}
