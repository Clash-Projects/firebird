//! Spotify web-player GraphQL ("Pathfinder") client: account playlists,
//! Liked Songs, track search, Home feed, song radio, and Canvas.
//!
//! Ported from `lib/features/spotify/spotify_api.dart`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use chrono::Local;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::integrations::artwork::{
    confirmed_album_match, confirmed_artist_match, confirmed_track_match, known_cover_album,
    normalize_canvas_match,
};
use crate::storage::diagnostics_log::DiagnosticsLog;

use super::spotify_auth::SpotifyAuth;
use super::spotify_web_player::{
    SpotifyWebPlayer, CANVAS_OPERATION, FETCH_PLAYLIST_OPERATION, HOME_OPERATION,
    LIBRARY_OPERATION, SEARCH_OPERATION, USER_AGENT,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyPlaylistSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub artwork_url: String,
    pub track_count: i64,
    pub owner: String,
}

impl SpotifyPlaylistSummary {
    pub const LIKED_ID: &'static str = "liked";

    pub fn is_liked(&self) -> bool {
        self.id == Self::LIKED_ID
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyTrackRow {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub artwork_url: String,
    pub duration_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyPlaylistContent {
    pub title: String,
    pub owner: String,
    pub artwork_url: String,
    pub tracks: Vec<SpotifyTrackRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyHomeItem {
    pub kind: String,
    pub id: String,
    pub name: String,
    pub subtitle: String,
    pub artwork_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyHomeSection {
    pub title: String,
    pub items: Vec<SpotifyHomeItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyCanvasClip {
    pub url: String,
    pub track_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifyCanvasResult {
    pub clip: Option<SpotifyCanvasClip>,
    pub reason: String,
}

impl SpotifyCanvasResult {
    pub fn found(clip: SpotifyCanvasClip) -> Self {
        Self {
            clip: Some(clip),
            reason: String::new(),
        }
    }

    pub fn none(reason: impl Into<String>) -> Self {
        Self {
            clip: None,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone)]
struct SpotifyTrackHit {
    id: String,
    name: String,
    artist: String,
    album: String,
    duration_ms: i64,
}

const V1_ENDPOINT: &str = "https://api-partner.spotify.com/pathfinder/v1/query";
const V2_ENDPOINT: &str = "https://api-partner.spotify.com/pathfinder/v2/query";
const CANVAZ_ENDPOINT: &str = "https://spclient.wg.spotify.com/canvaz-cache/v0/canvases";
const SONG_RADIO_ENDPOINT: &str =
    "https://spclient.wg.spotify.com/inspiredby-mix/v2/seed_to_playlist";

const KNOWN_CANVAS_HASH: &str =
    "575138ab27cd5c1b3e54da54d0a7cc8d85485402de26340c2145f0f6bb5e7a9f";
const KNOWN_LIBRARY_HASH: &str =
    "390c78e5b951029bad359785e69b07b536a509c581cbcd0aded5e5067f187455";
const KNOWN_PLAYLIST_HASH: &str =
    "21ea11a6ac75a5f3e01fd365d29c18e0a4a8b7dc09f89bbb5d73910190c6acaf";
const KNOWN_LIKED_HASH: &str =
    "087278b20b743578a6262c2b0b4bcd20d879c503cc359a2285baf083ef944240";
const SEARCH_TRACKS_HASH: &str =
    "bc1ca2fcd0ba1013a0fc88e6cc4f190af501851e3dafd3e1ef85840297694428";
const KNOWN_SEARCH_HASH: &str =
    "f78953bf9207d73493c27284103f5aeb6e728876d5793851bf79bc706127ff70";

static CLEAN_TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\s*[\(\[](?:feat\.?|ft\.?|official|video|audio|remastered|lyric|lyrics|hd|4k)[^\)\]]*[\)\]]",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static ARTIST_SPLIT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*(?:,|&|feat\.?|ft\.?)\s*").unwrap());
static MP4_EXT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\.mp4(?:[?#]|$)").unwrap());
static PROTO_MP4_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https://[^\x00-\x20"']+\.mp4[^\x00-\x20"']*"#).unwrap());
static WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

pub struct SpotifyApi {
    auth: Arc<SpotifyAuth>,
}

impl SpotifyApi {
    pub fn new(auth: Arc<SpotifyAuth>) -> Self {
        Self { auth }
    }

    async fn hash(&self, operation: &str, known: &str, force: bool) -> String {
        let script = SpotifyWebPlayer::script(self.auth.http(), force).await;
        let live = script
            .as_deref()
            .and_then(|s| SpotifyWebPlayer::query_hash(s, operation));
        live.unwrap_or_else(|| known.to_string())
    }

    async fn auth_headers(&self, app_platform: bool) -> Option<HashMap<String, String>> {
        let personal = self.auth.personal_token().await?;
        let client = self.auth.client_token().await;
        let mut map = HashMap::new();
        map.insert("Authorization".to_string(), format!("Bearer {personal}"));
        if let Some(c) = client {
            map.insert("Client-Token".to_string(), c);
        }
        map.insert("User-Agent".to_string(), USER_AGENT.to_string());
        if app_platform {
            map.insert("App-platform".to_string(), "WebPlayer".to_string());
        }
        map.insert("Accept".to_string(), "application/json".to_string());
        Some(map)
    }

    async fn query(
        &self,
        operation: &str,
        variables: Value,
        hash: &str,
        endpoint: &str,
    ) -> Option<Value> {
        let headers = self.auth_headers(false).await?;
        let ext = json!({
            "persistedQuery": { "version": 1, "sha256Hash": hash }
        })
        .to_string();
        let vars_str = variables.to_string();

        let mut req = self.auth.http().get(endpoint).query(&[
            ("operationName", operation),
            ("variables", &vars_str),
            ("extensions", &ext),
        ]);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        match req.send().await {
            Ok(res) => {
                let status = res.status().as_u16();
                let text = res.text().await.unwrap_or_default();
                if status / 100 != 2 {
                    let brief = WS_RE.replace_all(&text, " ");
                    let clipped: String = brief.chars().take(160).collect();
                    DiagnosticsLog::info(&format!(
                        "spotify query op={operation} http={status} body={clipped}"
                    ));
                    return None;
                }
                serde_json::from_str::<Value>(&text).ok()
            }
            Err(e) => {
                DiagnosticsLog::info(&format!("spotify query op={operation} error={e}"));
                None
            }
        }
    }

    async fn post(&self, operation: &str, variables: Value, hash: &str) -> Option<Value> {
        let headers = self.auth_headers(false).await?;
        let payload = json!({
            "operationName": operation,
            "variables": variables,
            "extensions": {
                "persistedQuery": { "version": 1, "sha256Hash": hash }
            }
        })
        .to_string();
        let mut req = self
            .auth
            .http()
            .post(V2_ENDPOINT)
            .header("Content-Type", "application/json")
            .body(payload);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        match req.send().await {
            Ok(res) => {
                let status = res.status().as_u16();
                if status / 100 != 2 {
                    DiagnosticsLog::info(&format!(
                        "spotify post op={operation} http={status}"
                    ));
                    return None;
                }
                let text = res.text().await.unwrap_or_default();
                serde_json::from_str::<Value>(&text).ok()
            }
            Err(e) => {
                DiagnosticsLog::info(&format!("spotify post op={operation} error={e}"));
                None
            }
        }
    }

    /// The account's playlists.
    pub async fn user_playlists(&self) -> Vec<SpotifyPlaylistSummary> {
        let hash = self.hash(LIBRARY_OPERATION, KNOWN_LIBRARY_HASH, false).await;
        let mut out = Vec::new();
        let mut offset = 0usize;
        for _page in 0..6 {
            let root = self
                .query(
                    "libraryV3",
                    json!({
                        "filters": ["Playlists"],
                        "order": Value::Null,
                        "textFilter": Value::Null,
                        "features": [],
                        "limit": 50,
                        "offset": offset,
                        "flatten": false,
                        "expandedFolders": [],
                        "folderUri": Value::Null,
                        "includeFoldersWhenFlattening": true,
                    }),
                    &hash,
                    V1_ENDPOINT,
                )
                .await;
            let Some(lib) = at(root.as_ref(), &["data", "me", "libraryV3"]).and_then(Value::as_object)
            else {
                break;
            };
            let Some(items) = lib.get("items").and_then(Value::as_array) else {
                break;
            };
            if items.is_empty() {
                break;
            }
            for it in items {
                let Some(wrapper) = it.get("item").and_then(Value::as_object) else {
                    continue;
                };
                if wrapper.get("__typename").and_then(Value::as_str)
                    != Some("PlaylistResponseWrapper")
                {
                    continue;
                }
                let Some(data) = wrapper.get("data") else {
                    continue;
                };
                let uri = text_at(Some(data), "uri");
                if !uri.starts_with("spotify:playlist:") {
                    continue;
                }
                let id = uri.trim_start_matches("spotify:playlist:").to_string();
                if id.is_empty() {
                    continue;
                }
                let name = text_at(Some(data), "name");
                out.push(SpotifyPlaylistSummary {
                    id,
                    name: if name.is_empty() {
                        "Spotify playlist".to_string()
                    } else {
                        name
                    },
                    description: text_at(Some(data), "description"),
                    artwork_url: first_image(data.get("images")),
                    track_count: int_at(at(Some(data), &["content", "totalCount"]))
                        .or_else(|| int_at(at(Some(data), &["tracks", "total"])))
                        .unwrap_or(0),
                    owner: text_val(at(Some(data), &["ownerV2", "data", "name"])),
                });
            }
            let total = int_at(lib.get("totalCount")).unwrap_or(0) as usize;
            offset += items.len();
            if offset >= total {
                break;
            }
        }
        out
    }

    pub async fn playlist(&self, id: &str) -> Option<SpotifyPlaylistContent> {
        if id == SpotifyPlaylistSummary::LIKED_ID {
            self.liked().await
        } else {
            self.fetch_playlist(id).await
        }
    }

    async fn fetch_playlist(&self, id: &str) -> Option<SpotifyPlaylistContent> {
        let hash = self
            .hash(FETCH_PLAYLIST_OPERATION, KNOWN_PLAYLIST_HASH, false)
            .await;
        let mut rows = Vec::new();
        let mut title = "Spotify playlist".to_string();
        let mut owner = String::new();
        let mut art = String::new();
        let mut offset = 0i64;
        let mut total = i64::MAX;

        for page in 0..30 {
            if offset >= total {
                break;
            }
            let root = self
                .query(
                    "fetchPlaylist",
                    json!({
                        "uri": format!("spotify:playlist:{id}"),
                        "offset": offset,
                        "limit": 100,
                        "enableWatchFeedEntrypoint": false,
                    }),
                    &hash,
                    V1_ENDPOINT,
                )
                .await;
            let Some(pl) = at(root.as_ref(), &["data", "playlistV2"]).and_then(Value::as_object)
            else {
                if page == 0 {
                    return None;
                }
                break;
            };
            if page == 0 {
                let n = text_val(pl.get("name"));
                if !n.is_empty() {
                    title = n;
                }
                owner = text_val(at(
                    Some(&Value::Object(pl.clone())),
                    &["ownerV2", "data", "name"],
                ));
                art = first_image(pl.get("images"));
            }
            let content = pl.get("content").and_then(Value::as_object);
            total = int_at(content.and_then(|c| c.get("totalCount"))).unwrap_or(0);
            let Some(items) = content
                .and_then(|c| c.get("items"))
                .and_then(Value::as_array)
            else {
                break;
            };
            if items.is_empty() {
                break;
            }
            for it in items {
                let data = at(Some(it), &["itemV2", "data"]).or_else(|| at(Some(it), &["track"]));
                if let Some(row) = Self::parse_row(data) {
                    rows.push(row);
                }
            }
            offset += items.len() as i64;
        }

        Some(SpotifyPlaylistContent {
            title,
            owner,
            artwork_url: art,
            tracks: rows,
        })
    }

    async fn liked(&self) -> Option<SpotifyPlaylistContent> {
        let hash = self.hash("fetchLibraryTracks", KNOWN_LIKED_HASH, false).await;
        let mut rows = Vec::new();
        let mut offset = 0i64;
        let mut total = i64::MAX;

        for page in 0..40 {
            if offset >= total {
                break;
            }
            let root = self
                .query(
                    "fetchLibraryTracks",
                    json!({
                        "offset": offset,
                        "limit": 50,
                    }),
                    &hash,
                    V1_ENDPOINT,
                )
                .await;
            let Some(tracks) =
                at(root.as_ref(), &["data", "me", "library", "tracks"]).and_then(Value::as_object)
            else {
                if page == 0 {
                    return None;
                }
                break;
            };
            total = int_at(tracks.get("totalCount")).unwrap_or(0);
            let Some(items) = tracks.get("items").and_then(Value::as_array) else {
                break;
            };
            if items.is_empty() {
                break;
            }
            for it in items {
                let t = it.get("track");
                let data = t.and_then(|tr| tr.get("data")).or(t);
                if let Some(row) = Self::parse_row(data) {
                    rows.push(row);
                }
            }
            offset += items.len() as i64;
        }

        Some(SpotifyPlaylistContent {
            title: "Liked Songs".to_string(),
            owner: "Spotify".to_string(),
            artwork_url: String::new(),
            tracks: rows,
        })
    }

    fn parse_row(data: Option<&Value>) -> Option<SpotifyTrackRow> {
        let map = data?.as_object()?;
        let name = clean_nbsp(map.get("name").and_then(Value::as_str).unwrap_or(""));
        if name.is_empty() {
            return None;
        }
        let mut names = Vec::new();
        if let Some(artists) = at(data, &["artists", "items"]).and_then(Value::as_array) {
            for a in artists {
                let p_name = text_val(at(Some(a), &["profile", "name"]));
                let raw = if !p_name.is_empty() {
                    p_name
                } else {
                    text_val(a.get("name"))
                };
                let cleaned = clean_nbsp(&raw);
                if !cleaned.is_empty() {
                    names.push(cleaned);
                }
            }
        }
        let album = map.get("albumOfTrack").or_else(|| map.get("album"));
        let ms = int_at(at(data, &["trackDuration", "totalMilliseconds"]))
            .or_else(|| int_at(at(data, &["duration", "totalMilliseconds"])))
            .or_else(|| int_at(map.get("duration_ms")))
            .unwrap_or(0);
        let artwork_url = match album {
            Some(alb) => {
                let src = first_source(at(Some(alb), &["coverArt", "sources"]));
                if !src.is_empty() {
                    src
                } else {
                    first_image(alb.get("images"))
                }
            }
            None => String::new(),
        };
        Some(SpotifyTrackRow {
            name,
            artist: names.join(", "),
            album: album
                .map(|alb| clean_nbsp(&text_val(alb.get("name"))))
                .unwrap_or_default(),
            artwork_url,
            duration_seconds: ms / 1000,
        })
    }

    pub async fn home(&self) -> Vec<SpotifyHomeSection> {
        let Some(hash) = SpotifyWebPlayer::find_hash(self.auth.http(), HOME_OPERATION).await else {
            return Vec::new();
        };
        let sp_t = self.auth.device_id().replace('-', "");
        let vars = json!({
            "homeEndUserIntegration": "INTEGRATION_WEB_PLAYER",
            "timeZone": Self::time_zone(),
            "sp_t": sp_t,
            "facet": "",
            "sectionItemsLimit": 10,
        });
        let root = match self
            .query(HOME_OPERATION, vars.clone(), &hash, V1_ENDPOINT)
            .await
        {
            Some(r) => Some(r),
            None => self.post(HOME_OPERATION, vars, &hash).await,
        };
        let sections = Self::parse_home(root.as_ref());
        DiagnosticsLog::info(&format!("spotify home sections={}", sections.len()));
        sections
    }

    fn time_zone() -> String {
        let offset_mins = Local::now().offset().local_minus_utc() / 60;
        match offset_mins {
            330 => "Asia/Kolkata".to_string(),
            345 => "Asia/Kathmandu".to_string(),
            390 => "Asia/Yangon".to_string(),
            570 => "Australia/Darwin".to_string(),
            630 => "Australia/Adelaide".to_string(),
            -150 => "America/St_Johns".to_string(),
            270 => "Asia/Kabul".to_string(),
            210 => "Asia/Tehran".to_string(),
            0 => "UTC".to_string(),
            m if m % 60 != 0 => "UTC".to_string(),
            m => {
                let h = m / 60;
                if h > 0 {
                    format!("Etc/GMT-{h}")
                } else {
                    format!("Etc/GMT+{}", -h)
                }
            }
        }
    }

    /// `data.home.sectionContainer.sections.items[]` -> titled card rows.
    pub fn parse_home(root: Option<&Value>) -> Vec<SpotifyHomeSection> {
        let Some(sections) = at(root, &["data", "home", "sectionContainer", "sections", "items"])
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };

        let mut order: Vec<String> = Vec::new();
        let mut grouped: HashMap<String, (String, Vec<SpotifyHomeItem>, HashSet<String>)> =
            HashMap::new();

        for s in sections {
            let transformed = text_val(at(Some(s), &["data", "title", "transformedLabel"]));
            let raw_title = if !transformed.is_empty() {
                transformed
            } else {
                text_val(at(Some(s), &["data", "title", "text"]))
            };
            let title = if raw_title.trim().is_empty() {
                "For you".to_string()
            } else {
                raw_title.trim().to_string()
            };
            let key = title.to_lowercase();

            let Some(items) = at(Some(s), &["sectionItems", "items"]).and_then(Value::as_array)
            else {
                continue;
            };
            let mut cards = Vec::new();
            for it in items {
                if let Some(card) = Self::home_item(at(Some(it), &["content", "data"])) {
                    cards.push(card);
                }
            }
            if cards.is_empty() {
                continue;
            }

            if !grouped.contains_key(&key) {
                order.push(key.clone());
                grouped.insert(key.clone(), (title, Vec::new(), HashSet::new()));
            }
            let entry = grouped.get_mut(&key).unwrap();
            for card in cards {
                let card_key = format!("{}:{}", card.kind, card.id);
                if entry.2.insert(card_key) {
                    entry.1.push(card);
                }
            }
        }

        order
            .into_iter()
            .filter_map(|k| grouped.remove(&k))
            .filter(|(_, items, _)| !items.is_empty())
            .map(|(title, items, _)| SpotifyHomeSection { title, items })
            .collect()
    }

    fn home_item(data: Option<&Value>) -> Option<SpotifyHomeItem> {
        let map = data?.as_object()?;
        let uri = text_val(map.get("uri"));
        let parts: Vec<&str> = uri.split(':').collect();
        if parts.len() != 3 || parts[0] != "spotify" {
            return None;
        }
        let kind = parts[1];
        let name_direct = text_val(map.get("name"));
        let name = if !name_direct.is_empty() {
            name_direct
        } else {
            text_val(at(data, &["profile", "name"]))
        };
        if name.is_empty() {
            return None;
        }
        match kind {
            "playlist" => Some(SpotifyHomeItem {
                kind: kind.to_string(),
                id: parts[2].to_string(),
                name,
                subtitle: text_val(at(data, &["ownerV2", "data", "name"])),
                artwork_url: first_image(map.get("images")),
            }),
            "album" => {
                let artists = at(data, &["artists", "items"]).and_then(Value::as_array);
                let subtitle = artists
                    .and_then(|a| a.first())
                    .map(|first| text_val(at(Some(first), &["profile", "name"])))
                    .unwrap_or_default();
                Some(SpotifyHomeItem {
                    kind: kind.to_string(),
                    id: parts[2].to_string(),
                    name,
                    subtitle,
                    artwork_url: first_source(at(data, &["coverArt", "sources"])),
                })
            }
            "artist" => Some(SpotifyHomeItem {
                kind: kind.to_string(),
                id: parts[2].to_string(),
                name,
                subtitle: "Artist".to_string(),
                artwork_url: first_source(at(data, &["visuals", "avatarImage", "sources"])),
            }),
            _ => None,
        }
    }

    pub async fn song_radio(
        &self,
        title: &str,
        artist: &str,
        duration_ms: i64,
    ) -> Vec<SpotifyTrackRow> {
        let tag = format!("spotify-radio \"{title}\" - \"{artist}\"");
        let Some(matched) = self.search_track(title, artist, duration_ms, None).await else {
            DiagnosticsLog::info(&format!("{tag} stage=search ok=false"));
            return Vec::new();
        };
        let Some(headers) = self.auth_headers(true).await else {
            return Vec::new();
        };
        let url = format!("{SONG_RADIO_ENDPOINT}/spotify:track:{}", matched.id);
        let mut req = self.auth.http().get(&url).query(&[("response-format", "json")]);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let Ok(res) = req.send().await else {
            return Vec::new();
        };
        let status = res.status().as_u16();
        let body = res.text().await.unwrap_or_default();
        let playlist_id = if status / 100 == 2 {
            Self::song_radio_playlist_id(&body)
        } else {
            None
        };
        DiagnosticsLog::info(&format!(
            "{tag} stage=seed http={status} found={}",
            playlist_id.is_some()
        ));
        let Some(pid) = playlist_id else {
            return Vec::new();
        };
        self.fetch_playlist(&pid)
            .await
            .map(|p| p.tracks)
            .unwrap_or_default()
    }

    pub fn song_radio_playlist_id(body: &str) -> Option<String> {
        let Value::Object(root) = serde_json::from_str::<Value>(body).ok()? else {
            return None;
        };
        let items = root.get("mediaItems")?.as_array()?;
        for it in items {
            let uri = text_val(it.get("uri"));
            if let Some(id) = uri.strip_prefix("spotify:playlist:") {
                if !id.is_empty() {
                    return Some(id.to_string());
                }
            }
        }
        None
    }

    pub async fn canvas(
        &self,
        title: &str,
        artist: &str,
        duration_ms: i64,
        album: Option<&str>,
    ) -> SpotifyCanvasResult {
        let tag = format!("spotify-canvas \"{title}\" - \"{artist}\"");
        if self.auth.personal_token().await.is_none() {
            DiagnosticsLog::info(&format!("{tag} stage=session ok=false"));
            return SpotifyCanvasResult::none(
                "Spotify session is not signed in (sign in again in Settings)",
            );
        }
        let Some(matched) = self
            .search_track(
                title, artist, duration_ms,
                Some(known_cover_album(title, album.unwrap_or_default()).unwrap_or_default()),
            )
            .await
        else {
            DiagnosticsLog::info(&format!("{tag} stage=search ok=false"));
            return SpotifyCanvasResult::none("Could not find this track on Spotify");
        };
        let uri = format!("spotify:track:{}", matched.id);
        let mut url = self.canvas_via_protobuf(&uri).await;
        let via_query = url.is_none();
        if url.is_none() {
            url = self.canvas_via_query(&uri, false).await;
        }
        let route = if url.is_none() {
            "-"
        } else if via_query {
            "query"
        } else {
            "protobuf"
        };
        DiagnosticsLog::info(&format!(
            "{tag} stage=canvas track={} found={} route={route}",
            matched.id,
            url.is_some()
        ));
        match url {
            Some(u) => SpotifyCanvasResult::found(SpotifyCanvasClip {
                url: u,
                track_id: matched.id,
            }),
            None => SpotifyCanvasResult::none("This track has no Spotify Canvas"),
        }
    }

    async fn search_track(
        &self,
        title: &str,
        artist: &str,
        duration_ms: i64,
        canvas_album: Option<&str>,
    ) -> Option<SpotifyTrackHit> {
        let pick = |hits: &[SpotifyTrackHit]| match canvas_album {
            Some(album) => Self::pick_canvas(hits, title, artist, album, duration_ms),
            None => Self::pick(hits, title, duration_ms),
        };
        let clean_title = CLEAN_TITLE_RE.replace_all(title, "").trim().to_string();
        let primary = ARTIST_SPLIT_RE.split(artist).next().unwrap_or("").trim();
        let first = format!("{clean_title} {primary}").trim().to_string();
        let mut queries = Vec::new();
        if !first.is_empty() {
            queries.push(first.clone());
        }
        let raw_pair = format!("{title} {artist}");
        if raw_pair != first {
            queries.push(raw_pair);
        }

        let script = SpotifyWebPlayer::script(self.auth.http(), false).await;
        let mut ops = vec![("searchTracks".to_string(), SEARCH_TRACKS_HASH.to_string())];
        for name in ["searchDesktop", SEARCH_OPERATION] {
            if let Some(h) = script
                .as_deref()
                .and_then(|s| SpotifyWebPlayer::query_hash(s, name))
            {
                ops.push((name.to_string(), h));
            }
        }
        ops.push((SEARCH_OPERATION.to_string(), KNOWN_SEARCH_HASH.to_string()));

        for (op, hash) in ops {
            for q in &queries {
                let vars = if op == "searchTracks" {
                    json!({
                        "searchTerm": q,
                        "offset": 0,
                        "limit": 5,
                        "numberOfTopResults": 5,
                        "includeAudiobooks": false,
                        "includePreReleases": false,
                    })
                } else {
                    json!({
                        "searchTerm": q,
                        "term": q,
                        "offset": 0,
                        "limit": 10,
                        "numberOfTopResults": 5,
                        "includeAudiobooks": false,
                        "includePreReleases": false,
                        "includeAuthors": false,
                        "includeArtistHasConcertsField": false,
                    })
                };
                let endpoints: &[&str] = if op == "searchTracks" {
                    &[V1_ENDPOINT]
                } else {
                    &[V1_ENDPOINT, V2_ENDPOINT]
                };
                for &ep in endpoints {
                    let root = if ep == V1_ENDPOINT {
                        self.query(&op, vars.clone(), &hash, V1_ENDPOINT).await
                    } else {
                        self.post(&op, vars.clone(), &hash).await
                    };
                    let Some(root) = root else {
                        continue;
                    };
                    let hits = Self::collect_tracks(&root);
                    if let Some(best) = pick(&hits) {
                        return Some(best);
                    }
                }
            }
        }

        // Web API search fallback
        let headers = self.auth_headers(false).await?;
        let q = format!("{title} {artist}");
        let mut req = self
            .auth
            .http()
            .get("https://api.spotify.com/v1/search")
            .query(&[("type", "track"), ("q", q.as_str()), ("limit", "5")]);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let res = req.send().await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        let body: Value = res.json().await.ok()?;
        let items = at(Some(&body), &["tracks", "items"])?.as_array()?;
        let hits: Vec<SpotifyTrackHit> = items
            .iter()
            .filter_map(Self::hit_from_value)
            .collect();
        pick(&hits)
    }

    fn collect_tracks(root: &Value) -> Vec<SpotifyTrackHit> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();

        fn walk(
            n: &Value,
            depth: usize,
            out: &mut Vec<SpotifyTrackHit>,
            seen: &mut HashSet<String>,
        ) {
            if depth > 14 {
                return;
            }
            match n {
                Value::Object(map) => {
                    if let (Some(Value::String(uri)), Some(Value::String(_))) =
                        (map.get("uri"), map.get("name"))
                    {
                        if let Some(id) = uri.strip_prefix("spotify:track:") {
                            if let Some(hit) = SpotifyApi::hit_from_value(n) {
                                if seen.insert(id.to_string()) {
                                    out.push(hit);
                                }
                            }
                        }
                    }
                    for v in map.values() {
                        walk(v, depth + 1, out, seen);
                    }
                }
                Value::Array(arr) => {
                    for v in arr {
                        walk(v, depth + 1, out, seen);
                    }
                }
                _ => {}
            }
        }

        walk(root, 0, &mut out, &mut seen);
        out
    }

    fn hit_from_value(data: &Value) -> Option<SpotifyTrackHit> {
        let map = data.as_object()?;
        let direct_id = text_val(map.get("id"));
        let id = if !direct_id.is_empty() {
            direct_id
        } else {
            text_val(map.get("uri"))
                .trim_start_matches("spotify:track:")
                .to_string()
        };
        if id.is_empty() {
            return None;
        }
        let artists = map.get("artists");
        let artist = artists.and_then(Value::as_array)
            .or_else(|| artists.and_then(|value| value.get("items")).and_then(Value::as_array))
            .and_then(|items| items.first());
        Some(SpotifyTrackHit {
            id,
            name: text_val(map.get("name")),
            artist: text_val(at(artist, &["profile", "name"]).or_else(|| at(artist, &["name"]))),
            album: text_val(map.get("albumOfTrack").or_else(|| map.get("album")).and_then(|album| album.get("name"))),
            duration_ms: int_at(at(Some(data), &["duration", "totalMilliseconds"]))
                .or_else(|| int_at(at(Some(data), &["trackDuration", "totalMilliseconds"])))
                .or_else(|| int_at(map.get("duration_ms")))
                .unwrap_or(0),
        })
    }

    fn pick_canvas(
        hits: &[SpotifyTrackHit],
        title: &str,
        artist: &str,
        album: &str,
        duration_ms: i64,
    ) -> Option<SpotifyTrackHit> {
        hits.iter()
            .filter(|hit| {
                confirmed_track_match(title, &hit.name) && confirmed_artist_match(artist, &hit.artist)
            })
            .filter(|hit| album.trim().is_empty() || confirmed_album_match(album, &hit.album))
            .filter(|hit| {
                duration_ms <= 0 || (hit.duration_ms > 0 && hit.duration_ms.abs_diff(duration_ms) <= 5_000)
            })
            .min_by_key(|hit| if duration_ms > 0 { hit.duration_ms.abs_diff(duration_ms) } else { 0 })
            .cloned()
    }

    fn pick(hits: &[SpotifyTrackHit], title: &str, duration_ms: i64) -> Option<SpotifyTrackHit> {
        if hits.is_empty() {
            return None;
        }
        if duration_ms > 0 {
            let mut best = &hits[0];
            let mut best_d = i64::MAX;
            for h in hits {
                let d = if h.duration_ms > 0 {
                    (h.duration_ms - duration_ms).abs()
                } else {
                    i64::MAX
                };
                if d < best_d {
                    best_d = d;
                    best = h;
                }
            }
            return Some(best.clone());
        }
        let want = normalize_canvas_match(title);
        for h in hits {
            if h.name.eq_ignore_ascii_case(title) || normalize_canvas_match(&h.name) == want {
                return Some(h.clone());
            }
        }
        Some(hits[0].clone())
    }

    async fn canvas_via_query(&self, track_uri: &str, retry: bool) -> Option<String> {
        let headers = self.auth_headers(true).await?;
        let hash = self.hash(CANVAS_OPERATION, KNOWN_CANVAS_HASH, retry).await;
        let payload = json!({
            "operationName": "canvas",
            "variables": { "trackUri": track_uri },
            "extensions": {
                "persistedQuery": { "version": 1, "sha256Hash": hash }
            }
        })
        .to_string();

        let mut req = self
            .auth
            .http()
            .post(V2_ENDPOINT)
            .header("Content-Type", "application/json")
            .body(payload);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let res = req.send().await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        let body: Value = res.json().await.ok()?;
        let Some(data) = body.get("data").filter(|v| v.is_object()) else {
            let stale = body
                .get("errors")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .any(|e| e.to_string().to_lowercase().contains("persistedquerynotfound"))
                })
                .unwrap_or(false);
            if stale && !retry {
                return Box::pin(self.canvas_via_query(track_uri, true)).await;
            }
            return None;
        };
        let url = text_val(at(Some(data), &["trackUnion", "canvas", "url"]));
        if url.starts_with("https://") && MP4_EXT_RE.is_match(&url) {
            Some(url)
        } else {
            None
        }
    }

    async fn canvas_via_protobuf(&self, track_uri: &str) -> Option<String> {
        let headers = self.auth_headers(false).await?;
        let uri_bytes = track_uri.as_bytes();
        let mut inner = vec![0x0a];
        inner.extend(Self::varint(uri_bytes.len()));
        inner.extend_from_slice(uri_bytes);

        let mut body = vec![0x0a];
        body.extend(Self::varint(inner.len()));
        body.extend(inner);

        let mut req = self.auth.http().post(CANVAZ_ENDPOINT).body(body);
        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req = req
            .header("Accept", "application/protobuf")
            .header("Content-Type", "application/protobuf")
            .header("User-Agent", "Spotify/9.0.34.593 iOS/18.4 (iPhone15,3)");

        let res = req.send().await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        let bytes = res.bytes().await.ok()?;
        let latin1: String = bytes.iter().map(|&b| b as char).collect();
        PROTO_MP4_RE.find(&latin1).map(|m| m.as_str().to_string())
    }

    pub fn varint(mut value: usize) -> Vec<u8> {
        let mut out = Vec::new();
        while value > 0x7f {
            out.push(((value & 0x7f) | 0x80) as u8);
            value >>= 7;
        }
        out.push(value as u8);
        out
    }
}

fn at<'a>(node: Option<&'a Value>, path: &[&str]) -> Option<&'a Value> {
    let mut cur = node?;
    for key in path {
        cur = cur.get(*key)?;
    }
    Some(cur)
}

fn text_val(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").to_string()
}

fn text_at(node: Option<&Value>, key: &str) -> String {
    text_val(node.and_then(|n| n.get(key)))
}

fn int_at(v: Option<&Value>) -> Option<i64> {
    v.and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)))
}

fn clean_nbsp(s: &str) -> String {
    s.replace('\u{00a0}', " ").trim().to_string()
}

fn first_source(sources: Option<&Value>) -> String {
    sources
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|f| f.get("url"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn first_image(images: Option<&Value>) -> String {
    let Some(val) = images else {
        return String::new();
    };
    if let Some(obj) = val.as_object() {
        let first = obj
            .get("items")
            .and_then(Value::as_array)
            .and_then(|a| a.first());
        return first_source(first.and_then(|f| f.get("sources")));
    }
    if val.is_array() {
        return first_source(Some(val));
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_ignores_same_duration_on_the_wrong_track_or_release() {
        let hits = [
            json!({"id":"wrong-title","name":"The Lights","artists":[{"name":"Artist"}],"album":{"name":"Release"},"duration_ms":180000}),
            json!({"id":"wrong-artist","name":"Lights","artists":[{"name":"Another Artist"}],"album":{"name":"Release"},"duration_ms":180000}),
            json!({"id":"wrong-release","name":"Lights","artists":[{"name":"Artist"}],"album":{"name":"Release (Live)"},"duration_ms":180000}),
            json!({"id":"correct","name":"Lights","artists":{"items":[{"profile":{"name":"Artist"}}]},"albumOfTrack":{"name":"Release"},"duration_ms":181000}),
        ].iter().filter_map(SpotifyApi::hit_from_value).collect::<Vec<_>>();
        assert!(SpotifyApi::pick_canvas(&hits[..3], "Lights", "Artist", "Release", 180000).is_none());
        assert_eq!(
            SpotifyApi::pick_canvas(&hits, "Lights", "Artist", "Release", 180000).unwrap().id,
            "correct",
        );
    }

    #[test]
    fn parses_song_radio_and_home_sections() {
        let body = r#"{"mediaItems":[{"uri":"spotify:playlist:37i9dQZF1E8abc"}]}"#;
        assert_eq!(
            SpotifyApi::song_radio_playlist_id(body).as_deref(),
            Some("37i9dQZF1E8abc")
        );

        let home_json = json!({
            "data": {
                "home": {
                    "sectionContainer": {
                        "sections": {
                            "items": [{
                                "data": { "title": { "text": "Made for you" } },
                                "sectionItems": {
                                    "items": [{
                                        "content": {
                                            "data": {
                                                "uri": "spotify:playlist:pl1",
                                                "name": "Daily Mix 1",
                                                "ownerV2": { "data": { "name": "Spotify" } },
                                                "images": { "items": [{ "sources": [{ "url": "https://img/1.jpg" }] }] }
                                            }
                                        }
                                    }]
                                }
                            }]
                        }
                    }
                }
            }
        });
        let sections = SpotifyApi::parse_home(Some(&home_json));
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "Made for you");
        assert_eq!(sections[0].items[0].id, "pl1");
    }

    #[test]
    fn varint_encodes_correctly() {
        assert_eq!(SpotifyApi::varint(127), vec![0x7f]);
        assert_eq!(SpotifyApi::varint(128), vec![0x80, 0x01]);
    }
}
