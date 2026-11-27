use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::storage::app_database::AppDatabase;

use super::innertube_api::{
    InnerTubeMusicApi, YouTubeEntityKind, YouTubeMusicEntity, YouTubeMusicTrack,
    YouTubePlaylistResult, YouTubePlaylistSummary, YtAccount, YtConnection, YtHomeShelf,
};

pub const CACHE_GROUP_HOME: &str = "home";
pub const CACHE_GROUP_LIBRARY: &str = "library";
pub const CACHE_GROUP_PLAYLIST: &str = "playlist";

pub const HOME_SNAPSHOT_MAX_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
pub const LIBRARY_CACHE_MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;
pub const DEFAULT_SNAPSHOT_MAX_CHARS: usize = 1_500_000;
const SNAPSHOT_VERSION: i64 = 1;

pub fn yt_identity_key(conn: &YtConnection) -> String {
    format!(
        "{}|{}",
        conn.profile_email.trim().to_lowercase(),
        conn.active_page_id
    )
}

pub fn library_cache_key(what: &str, conn: &YtConnection) -> Option<String> {
    if conn.connected {
        Some(format!("lib_{what}_v1|{}", yt_identity_key(conn)))
    } else {
        None
    }
}

pub fn playlist_cache_key(id: &str, conn: &YtConnection) -> Option<String> {
    if id.is_empty() {
        None
    } else {
        Some(format!("playlist_v1|{}|{id}", yt_identity_key(conn)))
    }
}

pub fn home_shelves_snapshot_key(
    connected: bool,
    profile_email: &str,
    active_page_id: &str,
) -> Option<String> {
    if !connected {
        return None;
    }
    Some(format!(
        "home_yt_v1|{}|{active_page_id}",
        profile_email.trim().to_lowercase()
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrackWire {
    #[serde(default)]
    v: String,
    #[serde(default)]
    n: String,
    #[serde(default)]
    a: String,
    #[serde(default)]
    al: String,
    #[serde(default)]
    art: String,
    #[serde(default)]
    d: i64,
    #[serde(default)]
    ab: String,
}

impl From<&YouTubeMusicTrack> for TrackWire {
    fn from(t: &YouTubeMusicTrack) -> Self {
        Self {
            v: t.video_id.clone(),
            n: t.title.clone(),
            a: t.artist.clone(),
            al: t.album.clone(),
            art: t.artwork_url.clone(),
            d: t.duration_seconds,
            ab: t.album_browse_id.clone(),
        }
    }
}

impl From<TrackWire> for YouTubeMusicTrack {
    fn from(w: TrackWire) -> Self {
        Self {
            video_id: w.v,
            title: w.n,
            artist: w.a,
            album: w.al,
            artwork_url: w.art,
            duration_seconds: w.d,
            album_browse_id: w.ab,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntityWire {
    #[serde(default)]
    k: String,
    #[serde(default)]
    n: String,
    #[serde(default)]
    a: String,
    #[serde(default)]
    s: String,
    #[serde(default)]
    b: String,
    #[serde(default)]
    p: String,
    #[serde(default)]
    art: String,
}

impl From<&YouTubeMusicEntity> for EntityWire {
    fn from(e: &YouTubeMusicEntity) -> Self {
        let k = match e.kind {
            YouTubeEntityKind::Artist => "artist",
            YouTubeEntityKind::Album => "album",
            YouTubeEntityKind::Playlist => "playlist",
            YouTubeEntityKind::Mix => "mix",
        };
        Self {
            k: k.to_string(),
            n: e.name.clone(),
            a: e.artist.clone(),
            s: e.subtitle.clone(),
            b: e.browse_id.clone(),
            p: e.playlist_id.clone(),
            art: e.artwork_url.clone(),
        }
    }
}

impl From<EntityWire> for YouTubeMusicEntity {
    fn from(w: EntityWire) -> Self {
        let kind = match w.k.as_str() {
            "artist" => YouTubeEntityKind::Artist,
            "playlist" => YouTubeEntityKind::Playlist,
            "mix" => YouTubeEntityKind::Mix,
            _ => YouTubeEntityKind::Album,
        };
        Self {
            kind,
            name: w.n,
            artist: w.a,
            subtitle: w.s,
            browse_id: w.b,
            playlist_id: w.p,
            artwork_url: w.art,
        }
    }
}

pub fn encode_yt_tracks(list: &[YouTubeMusicTrack]) -> String {
    let items: Vec<TrackWire> = list.iter().map(TrackWire::from).collect();
    json!({
        "v": SNAPSHOT_VERSION,
        "items": items,
    })
    .to_string()
}

pub fn decode_yt_tracks(raw: &str) -> Option<Vec<YouTubeMusicTrack>> {
    let val: Value = serde_json::from_str(raw).ok()?;
    if val.get("v").and_then(Value::as_i64) != Some(SNAPSHOT_VERSION) {
        return None;
    }
    let items: Vec<TrackWire> = serde_json::from_value(val.get("items")?.clone()).ok()?;
    Some(items.into_iter().map(Into::into).collect())
}

pub fn encode_playlist_summaries(list: &[YouTubePlaylistSummary]) -> String {
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
    json!({
        "v": SNAPSHOT_VERSION,
        "items": items,
    })
    .to_string()
}

pub fn decode_playlist_summaries(raw: &str) -> Option<Vec<YouTubePlaylistSummary>> {
    let val: Value = serde_json::from_str(raw).ok()?;
    if val.get("v").and_then(Value::as_i64) != Some(SNAPSHOT_VERSION) {
        return None;
    }
    let arr = val.get("items")?.as_array()?;
    let out = arr
        .iter()
        .filter_map(|m| {
            let obj = m.as_object()?;
            Some(YouTubePlaylistSummary {
                id: obj.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                title: obj.get("t").and_then(Value::as_str).unwrap_or("").to_string(),
                author: obj.get("a").and_then(Value::as_str).unwrap_or("").to_string(),
                track_count_text: obj.get("c").and_then(Value::as_str).unwrap_or("").to_string(),
                artwork_url: obj.get("art").and_then(Value::as_str).unwrap_or("").to_string(),
            })
        })
        .collect();
    Some(out)
}

pub fn encode_home_shelves(shelves: &[YtHomeShelf]) -> String {
    let items: Vec<Value> = shelves
        .iter()
        .map(|s| {
            let tracks: Vec<TrackWire> = s.tracks.iter().map(TrackWire::from).collect();
            let cards: Vec<TrackWire> = s.track_cards.iter().map(TrackWire::from).collect();
            let ents: Vec<EntityWire> = s.entities.iter().map(EntityWire::from).collect();
            json!({
                "title": s.title,
                "sub": s.subtitle,
                "tracks": tracks,
                "cards": cards,
                "ents": ents,
            })
        })
        .collect();
    json!({
        "v": SNAPSHOT_VERSION,
        "shelves": items,
    })
    .to_string()
}

pub fn decode_home_shelves(raw: &str) -> Option<Vec<YtHomeShelf>> {
    let val: Value = serde_json::from_str(raw).ok()?;
    if val.get("v").and_then(Value::as_i64) != Some(SNAPSHOT_VERSION) {
        return None;
    }
    let arr = val.get("shelves")?.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for s in arr {
        let Some(obj) = s.as_object() else {
            continue;
        };
        let tracks: Vec<TrackWire> = obj
            .get("tracks")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let cards: Vec<TrackWire> = obj
            .get("cards")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let ents: Vec<EntityWire> = obj
            .get("ents")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        out.push(YtHomeShelf {
            title: obj.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
            subtitle: obj.get("sub").and_then(Value::as_str).unwrap_or("").to_string(),
            tracks: tracks.into_iter().map(Into::into).collect(),
            track_cards: cards.into_iter().map(Into::into).collect(),
            entities: ents.into_iter().map(Into::into).collect(),
        });
    }
    Some(out)
}

pub fn encode_playlist_result(p: &YouTubePlaylistResult) -> String {
    let tracks: Vec<TrackWire> = p.tracks.iter().map(TrackWire::from).collect();
    json!({
        "v": SNAPSHOT_VERSION,
        "id": p.id,
        "t": p.title,
        "a": p.author,
        "art": p.artwork_url,
        "n": p.track_count,
        "tracks": tracks,
    })
    .to_string()
}

pub fn decode_playlist_result(raw: &str) -> Option<YouTubePlaylistResult> {
    let val: Value = serde_json::from_str(raw).ok()?;
    if val.get("v").and_then(Value::as_i64) != Some(SNAPSHOT_VERSION) {
        return None;
    }
    let tracks: Vec<TrackWire> = val
        .get("tracks")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    Some(YouTubePlaylistResult {
        id: val.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
        title: val.get("t").and_then(Value::as_str).unwrap_or("").to_string(),
        author: val.get("a").and_then(Value::as_str).unwrap_or("").to_string(),
        artwork_url: val.get("art").and_then(Value::as_str).unwrap_or("").to_string(),
        track_count: val.get("n").and_then(Value::as_u64).unwrap_or(0) as usize,
        tracks: tracks.into_iter().map(Into::into).collect(),
    })
}

#[derive(Default)]
pub struct SwrState {
    served: Mutex<HashSet<String>>,
    handoff: Mutex<HashMap<String, String>>,
}

impl SwrState {
    pub fn mark_served(&self, key: &str) -> bool {
        self.served
            .lock()
            .map(|mut g| g.insert(key.to_string()))
            .unwrap_or(false)
    }

    pub fn invalidate_all(&self) {
        if let Ok(mut g) = self.served.lock() {
            g.clear();
        }
        if let Ok(mut g) = self.handoff.lock() {
            g.clear();
        }
    }

    pub fn take_handoff(&self, key: &str) -> Option<String> {
        self.handoff.lock().ok().and_then(|mut g| g.remove(key))
    }

    pub fn put_handoff(&self, key: &str, encoded: String) {
        if let Ok(mut g) = self.handoff.lock() {
            g.insert(key.to_string(), encoded);
        }
    }
}

pub type SwrRefreshCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Stale-while-revalidate YouTube Music account library provider service.
pub struct YtLibraryProviders {
    tube: Arc<InnerTubeMusicApi>,
    db: Arc<AppDatabase>,
    swr: Arc<SwrState>,
    on_refreshed: Mutex<Option<SwrRefreshCallback>>,
}

impl YtLibraryProviders {
    pub fn new(tube: Arc<InnerTubeMusicApi>, db: Arc<AppDatabase>) -> Self {
        Self {
            tube,
            db,
            swr: Arc::new(SwrState::default()),
            on_refreshed: Mutex::new(None),
        }
    }

    pub fn set_on_refreshed<F>(&self, cb: Option<F>)
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        if let Ok(mut g) = self.on_refreshed.lock() {
            *g = cb.map(|f| Arc::new(f) as SwrRefreshCallback);
        }
    }

    pub fn invalidate_account_surfaces(&self) {
        self.swr.invalidate_all();
    }

    pub async fn yt_liked_songs(&self, force_fresh: bool) -> Vec<YouTubeMusicTrack> {
        let conn = self.tube.connection();
        if !conn.connected {
            return Vec::new();
        }
        let key = library_cache_key("liked", &conn);
        let tube = Arc::clone(&self.tube);
        self.swr_load(
            key,
            CACHE_GROUP_LIBRARY,
            LIBRARY_CACHE_MAX_AGE_MS,
            force_fresh,
            |v: &Vec<YouTubeMusicTrack>| encode_yt_tracks(v),
            decode_yt_tracks,
            |v| v.is_empty(),
            move || async move { tube.fetch_liked_songs(200).await },
        )
        .await
    }

    pub async fn yt_history(&self, force_fresh: bool) -> Vec<YouTubeMusicTrack> {
        let conn = self.tube.connection();
        if !conn.connected {
            return Vec::new();
        }
        let key = library_cache_key("history", &conn);
        let tube = Arc::clone(&self.tube);
        self.swr_load(
            key,
            CACHE_GROUP_LIBRARY,
            LIBRARY_CACHE_MAX_AGE_MS,
            force_fresh,
            |v: &Vec<YouTubeMusicTrack>| encode_yt_tracks(v),
            decode_yt_tracks,
            |v| v.is_empty(),
            move || async move { tube.fetch_yt_history(100).await },
        )
        .await
    }

    pub async fn yt_account_playlists(&self, force_fresh: bool) -> Vec<YouTubePlaylistSummary> {
        let conn = self.tube.connection();
        if !conn.connected {
            return Vec::new();
        }
        let key = library_cache_key("playlists", &conn);
        let tube = Arc::clone(&self.tube);
        self.swr_load(
            key,
            CACHE_GROUP_LIBRARY,
            LIBRARY_CACHE_MAX_AGE_MS,
            force_fresh,
            |v: &Vec<YouTubePlaylistSummary>| encode_playlist_summaries(v),
            decode_playlist_summaries,
            |v| v.is_empty(),
            move || async move { tube.fetch_account_playlists().await },
        )
        .await
    }

    pub async fn yt_account(&self) -> Option<YtAccount> {
        let conn = self.tube.connection();
        if !conn.connected {
            return None;
        }
        self.tube.fetch_account_info(false, &conn.active_page_id).await
    }

    pub async fn yt_home_shelves(&self, force_fresh: bool) -> Vec<YtHomeShelf> {
        let conn = self.tube.connection();
        if !conn.connected {
            return Vec::new();
        }
        let key = home_shelves_snapshot_key(
            conn.connected,
            &conn.profile_email,
            &conn.active_page_id,
        );
        let tube = Arc::clone(&self.tube);
        self.swr_load(
            key,
            CACHE_GROUP_HOME,
            HOME_SNAPSHOT_MAX_AGE_MS,
            force_fresh,
            |v: &Vec<YtHomeShelf>| encode_home_shelves(v),
            decode_home_shelves,
            |v| v.is_empty(),
            move || async move { tube.fetch_home_shelves(20, 20, 4, true).await },
        )
        .await
    }

    pub async fn yt_playlist_detail(
        &self,
        id: &str,
        force_fresh: bool,
    ) -> Option<YouTubePlaylistResult> {
        if id.is_empty() {
            return None;
        }
        let conn = self.tube.connection();
        let key = playlist_cache_key(id, &conn);
        let tube = Arc::clone(&self.tube);
        let pid = id.to_string();
        let res = self
            .swr_load(
                key,
                CACHE_GROUP_PLAYLIST,
                LIBRARY_CACHE_MAX_AGE_MS,
                force_fresh,
                encode_playlist_result,
                decode_playlist_result,
                |r| r.tracks.is_empty(),
                move || async move { tube.fetch_playlist(&pid, None).await.unwrap_or_default() },
            )
            .await;
        if res.tracks.is_empty() && res.id.is_empty() {
            None
        } else {
            Some(res)
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn swr_load<T, F, Fut>(
        &self,
        key: Option<String>,
        group: &'static str,
        max_age_ms: i64,
        force_fresh: bool,
        encode: fn(&T) -> String,
        decode: fn(&str) -> Option<T>,
        is_empty: fn(&T) -> bool,
        fetch: F,
    ) -> T
    where
        T: Send + 'static,
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        let Some(k) = key else {
            return fetch().await;
        };

        if !force_fresh {
            if let Some(handed_raw) = self.swr.take_handoff(&k) {
                if let Some(val) = decode(&handed_raw) {
                    return val;
                }
            }

            if self.swr.mark_served(&k) {
                if let Some(cached) = read_snapshot(&self.db, &k, decode, is_empty, max_age_ms) {
                    let db = Arc::clone(&self.db);
                    let swr = Arc::clone(&self.swr);
                    let cb = self.on_refreshed.lock().ok().and_then(|g| g.clone());
                    let k_bg = k.clone();
                    tokio::spawn(async move {
                        let fresh = fetch().await;
                        if is_empty(&fresh) {
                            return;
                        }
                        let encoded = encode(&fresh);
                        if encoded.len() <= DEFAULT_SNAPSHOT_MAX_CHARS {
                            db.cache_put(&k_bg, group, &encoded, None);
                        }
                        swr.put_handoff(&k_bg, encoded);
                        if let Some(notify) = cb {
                            notify(&k_bg);
                        }
                    });
                    return cached;
                }
            }
        }

        let fresh = fetch().await;
        if !is_empty(&fresh) {
            let encoded = encode(&fresh);
            if encoded.len() <= DEFAULT_SNAPSHOT_MAX_CHARS {
                self.db.cache_put(&k, group, &encoded, None);
            }
        }
        fresh
    }
}

pub fn read_snapshot<T>(
    db: &AppDatabase,
    key: &str,
    decode: fn(&str) -> Option<T>,
    is_empty: fn(&T) -> bool,
    max_age_ms: i64,
) -> Option<T> {
    let now_ms = Utc::now().timestamp_millis();
    let row = db.cache_get(key, Some(now_ms))?;
    if now_ms.saturating_sub(row.saved_at_ms) > max_age_ms {
        return None;
    }
    let value = decode(&row.value)?;
    if is_empty(&value) {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_keys_scope_to_identity() {
        let disconnected = YtConnection::default();
        assert!(library_cache_key("liked", &disconnected).is_none());
        assert!(home_shelves_snapshot_key(false, "User@Test.com", "").is_none());

        let conn = YtConnection {
            connected: true,
            profile_email: " User@Example.com ".to_string(),
            active_page_id: "12345".to_string(),
            ..Default::default()
        };
        assert_eq!(
            library_cache_key("liked", &conn).as_deref(),
            Some("lib_liked_v1|user@example.com|12345")
        );
        assert_eq!(
            playlist_cache_key("PL99", &conn).as_deref(),
            Some("playlist_v1|user@example.com|12345|PL99")
        );
        assert_eq!(
            home_shelves_snapshot_key(true, " User@Example.com ", "12345").as_deref(),
            Some("home_yt_v1|user@example.com|12345")
        );
    }

    #[test]
    fn snapshot_codecs_roundtrip_tracks_shelves_and_playlists() {
        let tracks = vec![YouTubeMusicTrack {
            video_id: "vid1".to_string(),
            title: "Song".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            artwork_url: "https://img/1".to_string(),
            duration_seconds: 180,
            album_browse_id: "MPRE1".to_string(),
        }];
        let decoded_tracks = decode_yt_tracks(&encode_yt_tracks(&tracks)).expect("decode tracks");
        assert_eq!(decoded_tracks, tracks);

        let shelves = vec![YtHomeShelf {
            title: "Quick picks".to_string(),
            subtitle: "For you".to_string(),
            tracks: tracks.clone(),
            track_cards: Vec::new(),
            entities: vec![YouTubeMusicEntity {
                kind: YouTubeEntityKind::Mix,
                name: "My Mix".to_string(),
                artist: String::new(),
                subtitle: "Artist A".to_string(),
                browse_id: "VLRD1".to_string(),
                playlist_id: String::new(),
                artwork_url: String::new(),
            }],
        }];
        let decoded_shelves =
            decode_home_shelves(&encode_home_shelves(&shelves)).expect("decode shelves");
        assert_eq!(decoded_shelves, shelves);

        let playlist = YouTubePlaylistResult {
            id: "PL1".to_string(),
            title: "Favs".to_string(),
            author: "Me".to_string(),
            artwork_url: "https://img/p".to_string(),
            track_count: 1,
            tracks,
        };
        let decoded_pl =
            decode_playlist_result(&encode_playlist_result(&playlist)).expect("decode playlist");
        assert_eq!(decoded_pl, playlist);
    }
}
