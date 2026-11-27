//! On-disk snapshots of Home and Library data for stale-while-revalidate loading.
//!
//! 1:1 port of `features/home/home_snapshot.dart` (without Flutter Riverpod `Ref`).

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use crate::extractor::innertube_api::{
    YouTubeEntityKind, YouTubeMusicEntity, YouTubeMusicTrack, YouTubePlaylistResult,
    YouTubePlaylistSummary, YtConnection, YtHomeShelf,
};
use crate::integrations::feed::feed_repository::{FeedData, FeedEmptyReason, GeneratedTrack};
use crate::storage::app_database::AppDatabase;

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Cache group (and cap) for Home snapshots.
pub const HOME_CACHE_GROUP: &str = "home";

/// A snapshot older than this is not shown (7 days).
pub const HOME_SNAPSHOT_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub const CACHE_GROUP_LIBRARY: &str = "library";
pub const CACHE_GROUP_PLAYLIST: &str = "playlist";
pub const CACHE_GROUP_ALBUM: &str = "album";

/// Library lists change when you like or sync; a 30-day window keeps cached lists usable.
pub const LIBRARY_CACHE_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

const FEED_KEY_PREFIX: &str = "home_feed_v1|";
const SHELVES_KEY_PREFIX: &str = "home_yt_v1|";
const VERSION: i64 = 1;

/// Default maximum encoded character length persisted to `snapshot_cache`.
pub const DEFAULT_SNAPSHOT_MAX_CHARS: usize = 1_500_000;

/// Cache key for a Last.fm user's feed; `None` when there is no user.
pub fn home_feed_snapshot_key(username: &str) -> Option<String> {
    let u = username.trim().to_lowercase();
    if u.is_empty() {
        None
    } else {
        Some(format!("{FEED_KEY_PREFIX}{u}"))
    }
}

/// Cache key for a YouTube Music identity (login + channel); `None` when signed out.
pub fn home_shelves_snapshot_key(
    connected: bool,
    profile_email: &str,
    active_page_id: &str,
) -> Option<String> {
    if !connected {
        return None;
    }
    Some(format!(
        "{SHELVES_KEY_PREFIX}{}|{}",
        profile_email.trim().to_lowercase(),
        active_page_id
    ))
}

/// Cache key for the new-releases shelf. Scoped to the YouTube identity when
/// signed in, or `"home_albums_v1|public"` when signed out.
pub fn home_new_albums_snapshot_key(
    connected: bool,
    profile_email: &str,
    active_page_id: &str,
) -> String {
    if connected {
        format!(
            "home_albums_v1|{}|{}",
            profile_email.trim().to_lowercase(),
            active_page_id
        )
    } else {
        "home_albums_v1|public".to_string()
    }
}

/// Cache key for the personal mix; `None` when signed out.
pub fn home_personal_mix_snapshot_key(
    connected: bool,
    profile_email: &str,
    active_page_id: &str,
) -> Option<String> {
    if connected {
        Some(format!(
            "home_mix_v1|{}|{}",
            profile_email.trim().to_lowercase(),
            active_page_id
        ))
    } else {
        None
    }
}

/// The part of a cache key that scopes data to a YouTube login + channel.
pub fn yt_identity_key(c: &YtConnection) -> String {
    format!(
        "{}|{}",
        c.profile_email.trim().to_lowercase(),
        c.active_page_id
    )
}

/// Cache key for an account-scoped library list (`liked`, `history`, `playlists`);
/// `None` when signed out.
pub fn library_cache_key(what: &str, c: &YtConnection) -> Option<String> {
    if c.connected {
        Some(format!("lib_{what}_v1|{}", yt_identity_key(c)))
    } else {
        None
    }
}

/// One playlist's tracks, scoped to the YouTube identity.
pub fn playlist_cache_key(id: &str, c: &YtConnection) -> Option<String> {
    if id.is_empty() {
        None
    } else {
        Some(format!("playlist_v1|{}|{id}", yt_identity_key(c)))
    }
}

/// Albums are public pages: one copy for everybody.
pub fn album_cache_key(browse_id: &str) -> Option<String> {
    if browse_id.is_empty() {
        None
    } else {
        Some(format!("album_v1|{browse_id}"))
    }
}

/// How one kind of cached data is serialized, deserialized, and emptiness-checked.
pub trait SnapshotCodec<T> {
    /// Serializes `value` to a JSON string.
    fn encode(&self, value: &T) -> String;

    /// Returns `None` when the payload is unreadable or from another version.
    fn decode(&self, raw: &str) -> Option<T>;

    /// Empty results are never persisted and never replace a good snapshot.
    fn is_empty(&self, value: &T) -> bool;
}

// -- JSON helpers ------------------------------------------------------------

fn str_field(m: &Map<String, Value>, key: &str) -> String {
    match m.get(key) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

fn int_field(m: &Map<String, Value>, key: &str) -> i64 {
    m.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn list_of<T>(raw: Option<&Value>, mut read: impl FnMut(&Map<String, Value>) -> T) -> Vec<T> {
    let Some(arr) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(Value::as_object)
        .map(&mut read)
        .collect()
}

fn yt_track_to_json(t: &YouTubeMusicTrack) -> Value {
    json!({
        "v": t.video_id,
        "n": t.title,
        "a": t.artist,
        "al": t.album,
        "art": t.artwork_url,
        "d": t.duration_seconds,
        "ab": t.album_browse_id,
    })
}

fn yt_track_from_json(m: &Map<String, Value>) -> YouTubeMusicTrack {
    YouTubeMusicTrack {
        video_id: str_field(m, "v"),
        title: str_field(m, "n"),
        artist: str_field(m, "a"),
        album: str_field(m, "al"),
        artwork_url: str_field(m, "art"),
        duration_seconds: int_field(m, "d"),
        album_browse_id: str_field(m, "ab"),
    }
}

fn entity_kind_name(kind: YouTubeEntityKind) -> &'static str {
    match kind {
        YouTubeEntityKind::Artist => "artist",
        YouTubeEntityKind::Album => "album",
        YouTubeEntityKind::Playlist => "playlist",
        YouTubeEntityKind::Mix => "mix",
    }
}

fn entity_kind_from_name(s: &str) -> YouTubeEntityKind {
    match s {
        "artist" => YouTubeEntityKind::Artist,
        "album" => YouTubeEntityKind::Album,
        "playlist" => YouTubeEntityKind::Playlist,
        "mix" => YouTubeEntityKind::Mix,
        _ => YouTubeEntityKind::Album,
    }
}

fn entity_to_json(e: &YouTubeMusicEntity) -> Value {
    json!({
        "k": entity_kind_name(e.kind),
        "n": e.name,
        "a": e.artist,
        "s": e.subtitle,
        "b": e.browse_id,
        "p": e.playlist_id,
        "art": e.artwork_url,
    })
}

fn entity_from_json(m: &Map<String, Value>) -> YouTubeMusicEntity {
    YouTubeMusicEntity {
        kind: entity_kind_from_name(&str_field(m, "k")),
        name: str_field(m, "n"),
        artist: str_field(m, "a"),
        subtitle: str_field(m, "s"),
        browse_id: str_field(m, "b"),
        playlist_id: str_field(m, "p"),
        artwork_url: str_field(m, "art"),
    }
}

fn decode_versioned_map(raw: &str) -> Option<Map<String, Value>> {
    let val: Value = serde_json::from_str(raw).ok()?;
    let obj = val.as_object()?;
    if obj.get("v").and_then(Value::as_i64) != Some(VERSION) {
        return None;
    }
    Some(obj.clone())
}

// -- Codecs ------------------------------------------------------------------

/// Serializes a `GeneratedTrack` into compact snapshot JSON.
pub fn generated_track_to_json(t: &GeneratedTrack) -> Value {
    json!({
        "n": t.name,
        "a": t.artist,
        "al": t.album,
        "art": t.artwork_url,
        "v": t.video_id,
        "l": t.listeners,
        "m": t.match_score,
        "d": t.duration_seconds,
        "p": t.playback_url,
    })
}

/// Deserializes a `GeneratedTrack` from compact snapshot JSON.
pub fn generated_track_from_json(m: &Map<String, Value>) -> GeneratedTrack {
    GeneratedTrack {
        name: str_field(m, "n"),
        artist: str_field(m, "a"),
        album: str_field(m, "al"),
        artwork_url: str_field(m, "art"),
        video_id: str_field(m, "v"),
        listeners: str_field(m, "l"),
        match_score: str_field(m, "m"),
        duration_seconds: int_field(m, "d"),
        playback_url: str_field(m, "p"),
    }
}

/// Codec for `FeedData`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FeedSnapshotCodec;

impl SnapshotCodec<FeedData> for FeedSnapshotCodec {
    fn encode(&self, f: &FeedData) -> String {
        json!({
            "v": VERSION,
            "quick": f.quick_picks.iter().map(generated_track_to_json).collect::<Vec<_>>(),
            "heavy": f.heavy_rotation.iter().map(generated_track_to_json).collect::<Vec<_>>(),
            "fresh": f.fresh_finds.iter().map(generated_track_to_json).collect::<Vec<_>>(),
            "jump": f.jump_back_in.iter().map(generated_track_to_json).collect::<Vec<_>>(),
            "because": f.because_you_listened.iter().map(generated_track_to_json).collect::<Vec<_>>(),
            "charts": f.charts.iter().map(generated_track_to_json).collect::<Vec<_>>(),
            "tags": f.taste_tags,
            "seed": f.because_seed,
            "aff": f.taste_affinities,
        })
        .to_string()
    }

    fn decode(&self, raw: &str) -> Option<FeedData> {
        let obj = decode_versioned_map(raw)?;
        let taste_tags = obj
            .get("tags")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let taste_affinities = obj
            .get("aff")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
                    .collect()
            })
            .unwrap_or_default();

        Some(FeedData {
            quick_picks: list_of(obj.get("quick"), generated_track_from_json),
            heavy_rotation: list_of(obj.get("heavy"), generated_track_from_json),
            fresh_finds: list_of(obj.get("fresh"), generated_track_from_json),
            jump_back_in: list_of(obj.get("jump"), generated_track_from_json),
            because_you_listened: list_of(obj.get("because"), generated_track_from_json),
            charts: list_of(obj.get("charts"), generated_track_from_json),
            taste_tags,
            because_seed: str_field(&obj, "seed"),
            taste_affinities,
            last_fm_failed: false,
            charts_failed: false,
            empty_reason: FeedEmptyReason::None,
        })
    }

    fn is_empty(&self, value: &FeedData) -> bool {
        value.is_empty()
    }
}

/// Personal mix payload (`seed` + `tracks`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PersonalMixData {
    pub seed: GeneratedTrack,
    pub tracks: Vec<GeneratedTrack>,
}

/// Stand-in for "no mix" (`swr_load` needs a non-empty/typed sentinel).
pub fn no_personal_mix() -> PersonalMixData {
    PersonalMixData::default()
}

/// Codec for `PersonalMixData`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PersonalMixSnapshotCodec;

impl SnapshotCodec<PersonalMixData> for PersonalMixSnapshotCodec {
    fn encode(&self, m: &PersonalMixData) -> String {
        json!({
            "v": VERSION,
            "seed": generated_track_to_json(&m.seed),
            "tracks": m.tracks.iter().map(generated_track_to_json).collect::<Vec<_>>(),
        })
        .to_string()
    }

    fn decode(&self, raw: &str) -> Option<PersonalMixData> {
        let obj = decode_versioned_map(raw)?;
        let seed_obj = obj.get("seed").and_then(Value::as_object)?;
        Some(PersonalMixData {
            seed: generated_track_from_json(seed_obj),
            tracks: list_of(obj.get("tracks"), generated_track_from_json),
        })
    }

    fn is_empty(&self, value: &PersonalMixData) -> bool {
        value.tracks.is_empty()
    }
}

/// Codec for `Vec<YtHomeShelf>`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShelvesSnapshotCodec;

impl SnapshotCodec<Vec<YtHomeShelf>> for ShelvesSnapshotCodec {
    fn encode(&self, shelves: &Vec<YtHomeShelf>) -> String {
        let arr: Vec<Value> = shelves
            .iter()
            .map(|s| {
                json!({
                    "title": s.title,
                    "sub": s.subtitle,
                    "tracks": s.tracks.iter().map(yt_track_to_json).collect::<Vec<_>>(),
                    "cards": s.track_cards.iter().map(yt_track_to_json).collect::<Vec<_>>(),
                    "ents": s.entities.iter().map(entity_to_json).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({ "v": VERSION, "shelves": arr }).to_string()
    }

    fn decode(&self, raw: &str) -> Option<Vec<YtHomeShelf>> {
        let obj = decode_versioned_map(raw)?;
        Some(list_of(obj.get("shelves"), |s| YtHomeShelf {
            title: str_field(s, "title"),
            subtitle: str_field(s, "sub"),
            tracks: list_of(s.get("tracks"), yt_track_from_json),
            track_cards: list_of(s.get("cards"), yt_track_from_json),
            entities: list_of(s.get("ents"), entity_from_json),
        }))
    }

    fn is_empty(&self, value: &Vec<YtHomeShelf>) -> bool {
        value.is_empty()
    }
}

/// Codec for `Vec<YouTubeMusicEntity>`.
#[derive(Debug, Clone, Copy, Default)]
pub struct EntitiesSnapshotCodec;

impl SnapshotCodec<Vec<YouTubeMusicEntity>> for EntitiesSnapshotCodec {
    fn encode(&self, list: &Vec<YouTubeMusicEntity>) -> String {
        json!({
            "v": VERSION,
            "items": list.iter().map(entity_to_json).collect::<Vec<_>>(),
        })
        .to_string()
    }

    fn decode(&self, raw: &str) -> Option<Vec<YouTubeMusicEntity>> {
        let obj = decode_versioned_map(raw)?;
        Some(list_of(obj.get("items"), entity_from_json))
    }

    fn is_empty(&self, value: &Vec<YouTubeMusicEntity>) -> bool {
        value.is_empty()
    }
}

/// Codec for `Vec<YouTubeMusicTrack>`.
#[derive(Debug, Clone, Copy, Default)]
pub struct YtTracksSnapshotCodec;

impl SnapshotCodec<Vec<YouTubeMusicTrack>> for YtTracksSnapshotCodec {
    fn encode(&self, list: &Vec<YouTubeMusicTrack>) -> String {
        json!({
            "v": VERSION,
            "items": list.iter().map(yt_track_to_json).collect::<Vec<_>>(),
        })
        .to_string()
    }

    fn decode(&self, raw: &str) -> Option<Vec<YouTubeMusicTrack>> {
        let obj = decode_versioned_map(raw)?;
        Some(list_of(obj.get("items"), yt_track_from_json))
    }

    fn is_empty(&self, value: &Vec<YouTubeMusicTrack>) -> bool {
        value.is_empty()
    }
}

/// Codec for `Vec<YouTubePlaylistSummary>`.
#[derive(Debug, Clone, Copy, Default)]
pub struct YtPlaylistSummariesSnapshotCodec;

impl SnapshotCodec<Vec<YouTubePlaylistSummary>> for YtPlaylistSummariesSnapshotCodec {
    fn encode(&self, list: &Vec<YouTubePlaylistSummary>) -> String {
        let items: Vec<Value> = list
            .iter()
            .map(|p| {
                json!({
                    "id": p.id,
                    "t": p.title,
                    "a": p.author,
                    "c": p.track_count_text,
                    "art": p.artwork_url,
                })
            })
            .collect();
        json!({ "v": VERSION, "items": items }).to_string()
    }

    fn decode(&self, raw: &str) -> Option<Vec<YouTubePlaylistSummary>> {
        let obj = decode_versioned_map(raw)?;
        Some(list_of(obj.get("items"), |m| YouTubePlaylistSummary {
            id: str_field(m, "id"),
            title: str_field(m, "t"),
            author: str_field(m, "a"),
            track_count_text: str_field(m, "c"),
            artwork_url: str_field(m, "art"),
        }))
    }

    fn is_empty(&self, value: &Vec<YouTubePlaylistSummary>) -> bool {
        value.is_empty()
    }
}

/// Stand-in for "playlist unavailable" (`swr_load` needs a non-empty/typed sentinel).
pub fn no_playlist_result() -> YouTubePlaylistResult {
    YouTubePlaylistResult::default()
}

/// Codec for `YouTubePlaylistResult`.
#[derive(Debug, Clone, Copy, Default)]
pub struct YtPlaylistResultSnapshotCodec;

impl SnapshotCodec<YouTubePlaylistResult> for YtPlaylistResultSnapshotCodec {
    fn encode(&self, p: &YouTubePlaylistResult) -> String {
        json!({
            "v": VERSION,
            "id": p.id,
            "t": p.title,
            "a": p.author,
            "art": p.artwork_url,
            "n": p.track_count,
            "tracks": p.tracks.iter().map(yt_track_to_json).collect::<Vec<_>>(),
        })
        .to_string()
    }

    fn decode(&self, raw: &str) -> Option<YouTubePlaylistResult> {
        let obj = decode_versioned_map(raw)?;
        Some(YouTubePlaylistResult {
            id: str_field(&obj, "id"),
            title: str_field(&obj, "t"),
            author: str_field(&obj, "a"),
            artwork_url: str_field(&obj, "art"),
            track_count: int_field(&obj, "n").max(0) as usize,
            tracks: list_of(obj.get("tracks"), yt_track_from_json),
        })
    }

    fn is_empty(&self, value: &YouTubePlaylistResult) -> bool {
        value.tracks.is_empty()
    }
}

// -- Stale-while-revalidate --------------------------------------------------

/// Per-container bookkeeping for `swr_load`.
#[derive(Default)]
pub struct SwrState {
    served: Mutex<HashSet<String>>,
    handoff: Mutex<HashMap<String, Box<dyn Any + Send + Sync>>>,
}

impl SwrState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks `key` as served in this session, returning `true` if it had not
    /// been served before.
    pub fn mark_served(&self, key: &str) -> bool {
        self.served
            .lock()
            .map(|mut s| s.insert(key.to_string()))
            .unwrap_or(false)
    }

    /// Stores a fresh value for the next rebuild/call.
    pub fn put_handoff<T: Send + Sync + 'static>(&self, key: &str, value: T) {
        if let Ok(mut map) = self.handoff.lock() {
            map.insert(key.to_string(), Box::new(value));
        }
    }

    /// Removes and returns a handed-off value of type `T` if present.
    pub fn take_handoff<T: 'static>(&self, key: &str) -> Option<T> {
        let boxed = self.handoff.lock().ok()?.remove(key)?;
        boxed.downcast::<T>().ok().map(|b| *b)
    }
}

/// Reads the stored value for `key`, or `None` when absent, unreadable, empty,
/// or older than `max_age`.
pub fn read_snapshot<T, C: SnapshotCodec<T>>(
    db: &AppDatabase,
    key: &str,
    codec: &C,
    now_ms: i64,
    max_age: Duration,
) -> Option<T> {
    let row = db.cache_get(key, Some(now_ms))?;
    let max_age_ms = i64::try_from(max_age.as_millis()).unwrap_or(i64::MAX);
    if now_ms.saturating_sub(row.saved_at_ms) > max_age_ms {
        return None;
    }
    let value = codec.decode(&row.value)?;
    if codec.is_empty(&value) {
        return None;
    }
    Some(value)
}

/// Persists `value` into `db` if non-empty and within `max_chars`.
pub fn persist_snapshot<T, C: SnapshotCodec<T>>(
    db: &AppDatabase,
    key: &str,
    group: &str,
    codec: &C,
    value: &T,
    max_chars: usize,
    now_ms: i64,
) {
    if codec.is_empty(value) {
        return;
    }
    let encoded = codec.encode(value);
    if encoded.len() > max_chars {
        return;
    }
    db.cache_put(key, group, &encoded, Some(now_ms));
}

/// Stale-while-revalidate loader backed by `AppDatabase.cache_get/cache_put`.
///
/// - `key == None`: calls `fetch()` directly.
/// - If a handed-off value exists in `state`, returns it immediately.
/// - On first load of `key` (or when `every_build` is `true`) with a valid
///   cached snapshot, returns the snapshot immediately and spawns a background
///   refresh via `tokio::spawn` that persists the fresh result, stores it in
///   `state.handoff`, and invokes `on_refresh`.
/// - Otherwise awaits `fetch()`, persists when non-empty, and returns it.
#[allow(clippy::too_many_arguments)]
pub async fn swr_load<T, E, C, F, Fut, R>(
    state: Arc<SwrState>,
    db: Arc<AppDatabase>,
    key: Option<&str>,
    codec: C,
    fetch: F,
    group: &str,
    max_age: Duration,
    every_build: bool,
    max_chars: usize,
    on_refresh: R,
) -> Result<T, E>
where
    T: Send + Sync + 'static,
    E: Send + 'static,
    C: SnapshotCodec<T> + Send + Sync + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    R: FnOnce() + Send + 'static,
{
    let Some(key_str) = key else {
        return fetch().await;
    };

    if let Some(handed) = state.take_handoff::<T>(key_str) {
        return Ok(handed);
    }

    let now = now_millis();
    if state.mark_served(key_str) || every_build {
        if let Some(cached) = read_snapshot(&db, key_str, &codec, now, max_age) {
            let state_bg = Arc::clone(&state);
            let db_bg = Arc::clone(&db);
            let key_bg = key_str.to_string();
            let group_bg = group.to_string();
            tokio::spawn(async move {
                if let Ok(fresh) = fetch().await {
                    if !codec.is_empty(&fresh) {
                        persist_snapshot(
                            &db_bg,
                            &key_bg,
                            &group_bg,
                            &codec,
                            &fresh,
                            max_chars,
                            now_millis(),
                        );
                        state_bg.put_handoff(&key_bg, fresh);
                        on_refresh();
                    }
                }
            });
            return Ok(cached);
        }
    }

    let fresh = fetch().await?;
    persist_snapshot(&db, key_str, group, &codec, &fresh, max_chars, now_millis());
    Ok(fresh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_builders_match_dart_format() {
        assert_eq!(home_feed_snapshot_key("  "), None);
        assert_eq!(
            home_feed_snapshot_key(" Alice "),
            Some("home_feed_v1|alice".to_string())
        );

        assert_eq!(home_shelves_snapshot_key(false, "a@b.com", "p1"), None);
        assert_eq!(
            home_shelves_snapshot_key(true, " A@B.COM ", "p1"),
            Some("home_yt_v1|a@b.com|p1".to_string())
        );

        assert_eq!(
            home_new_albums_snapshot_key(false, "a@b.com", "p1"),
            "home_albums_v1|public"
        );
        assert_eq!(
            home_new_albums_snapshot_key(true, " A@B.COM ", "p1"),
            "home_albums_v1|a@b.com|p1"
        );

        assert_eq!(home_personal_mix_snapshot_key(false, "a@b.com", "p1"), None);
        assert_eq!(
            home_personal_mix_snapshot_key(true, "A@B.COM", "p1"),
            Some("home_mix_v1|a@b.com|p1".to_string())
        );

        let conn = YtConnection {
            connected: true,
            profile_email: " User@Example.COM ".to_string(),
            active_page_id: "page42".to_string(),
            ..Default::default()
        };
        assert_eq!(
            library_cache_key("liked", &conn),
            Some("lib_liked_v1|user@example.com|page42".to_string())
        );
        assert_eq!(
            playlist_cache_key("PL123", &conn),
            Some("playlist_v1|user@example.com|page42|PL123".to_string())
        );
        assert_eq!(playlist_cache_key("", &conn), None);
        assert_eq!(
            album_cache_key("MPREb_123"),
            Some("album_v1|MPREb_123".to_string())
        );
        assert_eq!(album_cache_key(""), None);
    }

    #[test]
    fn shelves_and_read_snapshot_roundtrip_and_expiry() {
        let db = AppDatabase::in_memory();
        let codec = ShelvesSnapshotCodec;
        let shelves = vec![YtHomeShelf {
            title: "Quick picks".to_string(),
            subtitle: "Listen again".to_string(),
            tracks: vec![YouTubeMusicTrack {
                video_id: "vid1".to_string(),
                title: "Song A".to_string(),
                artist: "Artist A".to_string(),
                album: "Album A".to_string(),
                artwork_url: "https://img/a.jpg".to_string(),
                duration_seconds: 210,
                album_browse_id: "MPREb_a".to_string(),
            }],
            track_cards: vec![],
            entities: vec![YouTubeMusicEntity {
                kind: YouTubeEntityKind::Playlist,
                name: "Chill Mix".to_string(),
                artist: String::new(),
                subtitle: "For you".to_string(),
                browse_id: "VLPL1".to_string(),
                playlist_id: "PL1".to_string(),
                artwork_url: "https://img/p.jpg".to_string(),
            }],
        }];

        let now = 1_700_000_000_000i64;
        persist_snapshot(
            &db,
            "home_yt_v1|u|p",
            HOME_CACHE_GROUP,
            &codec,
            &shelves,
            DEFAULT_SNAPSHOT_MAX_CHARS,
            now,
        );

        let loaded = read_snapshot(
            &db,
            "home_yt_v1|u|p",
            &codec,
            now + 1000,
            HOME_SNAPSHOT_MAX_AGE,
        );
        assert_eq!(loaded.as_ref(), Some(&shelves));

        // Expired snapshot returns None
        let too_late = now + HOME_SNAPSHOT_MAX_AGE.as_millis() as i64 + 1;
        assert_eq!(
            read_snapshot(&db, "home_yt_v1|u|p", &codec, too_late, HOME_SNAPSHOT_MAX_AGE),
            None
        );
    }

    #[test]
    fn feed_and_personal_mix_codecs_roundtrip() {
        let track = GeneratedTrack {
            name: "Midnight".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            artwork_url: "https://img/m.jpg".to_string(),
            video_id: "vid99".to_string(),
            listeners: "1000".to_string(),
            match_score: "0.9".to_string(),
            duration_seconds: 180,
            playback_url: String::new(),
        };
        let mut affinities = HashMap::new();
        affinities.insert("indie".to_string(), 0.85);
        let feed = FeedData {
            quick_picks: vec![track.clone()],
            taste_tags: vec!["indie".to_string()],
            because_seed: "Seed Artist".to_string(),
            taste_affinities: affinities,
            ..Default::default()
        };

        let fcodec = FeedSnapshotCodec;
        let raw_feed = fcodec.encode(&feed);
        assert!(!fcodec.is_empty(&feed));
        assert_eq!(fcodec.decode(&raw_feed), Some(feed));
        assert!(fcodec.is_empty(&FeedData::default()));

        let mix = PersonalMixData {
            seed: track.clone(),
            tracks: vec![track],
        };
        let mcodec = PersonalMixSnapshotCodec;
        let raw_mix = mcodec.encode(&mix);
        assert!(!mcodec.is_empty(&mix));
        assert_eq!(mcodec.decode(&raw_mix), Some(mix));
        assert!(mcodec.is_empty(&no_personal_mix()));
    }
}
