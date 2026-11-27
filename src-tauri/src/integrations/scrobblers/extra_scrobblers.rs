use std::collections::{BTreeMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde_json::{json, Map, Value};
use tokio::sync::Mutex;
use url::Url;

use crate::extractor::auth_crypto::AuthCrypto;
use crate::extractor::http_client::HttpClientFactory;
use crate::models::app_info::APP_VERSION;
use crate::models::lastfm_home_models::{
    HomeStats, HomeTrack, LastFmResponseParser, TopAlbumItem, TopArtistItem,
};
use crate::storage::diagnostics_log::DiagnosticsLog;
use crate::storage::prefs::Prefs;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

const MAX_QUEUE_LEN: usize = 200;

/// One listen to send: what played and when it started (UTC seconds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listen {
    pub artist: String,
    pub track: String,
    pub album: String,
    pub started_at_sec: i64,
}

impl Listen {
    pub fn new(
        artist: impl Into<String>,
        track: impl Into<String>,
        album: impl Into<String>,
        started_at_sec: i64,
    ) -> Self {
        Self {
            artist: artist.into(),
            track: track.into(),
            album: album.into(),
            started_at_sec,
        }
    }

    pub fn key(&self) -> (String, String, i64) {
        (
            self.artist.trim().to_ascii_lowercase(),
            self.track.trim().to_ascii_lowercase(),
            self.started_at_sec,
        )
    }
}

/// Scrobble services besides Last.fm. Each is independent: its own
/// connection, its own offline queue, and a failure in one never holds up
/// another (or Last.fm).
pub trait ExtraScrobbler: Send + Sync {
    fn name(&self) -> &'static str;
    fn connected(&self) -> bool;
    fn username(&self) -> String;
    fn now_playing(&self, l: Listen) -> BoxFuture<'_, bool>;
    /// `false` keeps this listen queued; `true` also consumes permanent rejections.
    fn submit(&self, l: Listen) -> BoxFuture<'_, bool>;
    /// Lifetime listens on the service, or `None` when unavailable.
    fn listen_count(&self) -> BoxFuture<'_, Option<i64>>;
    fn profile_url(&self) -> String;
    fn as_read_backend(&self) -> Option<&dyn ScrobblerReadBackend> {
        None
    }
}

/// Extended read + love capabilities for scrobbler backends that support
/// fetching history, user stats, top charts, and track feedback.
pub trait ScrobblerReadBackend: Send + Sync {
    fn love_track<'a>(
        &'a self,
        artist: &'a str,
        track: &'a str,
        loved: bool,
    ) -> BoxFuture<'a, bool>;

    fn fetch_recent_tracks<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        limit: usize,
        page: usize,
    ) -> BoxFuture<'a, Vec<HomeTrack>>;

    fn fetch_stats<'a>(&'a self, viewing_as: Option<&'a str>) -> BoxFuture<'a, HomeStats>;

    fn fetch_period_play_count<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        from_millis: i64,
        to_millis: i64,
    ) -> BoxFuture<'a, i64>;

    fn fetch_top_tracks<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        period: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Vec<HomeTrack>>;

    fn fetch_top_artists<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        period: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Vec<TopArtistItem>>;

    fn fetch_top_albums<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        period: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Vec<TopAlbumItem>>;
}

// -- ListenBrainz -------------------------------------------------------------

/// ListenBrainz (or any server speaking its API: Maloja, Koito, ...).
pub struct ListenBrainzScrobbler {
    prefs: Arc<Prefs>,
    client: Client,
}

impl ListenBrainzScrobbler {
    pub const DEFAULT_SERVER: &'static str = "https://api.listenbrainz.org";

    pub fn new(prefs: Arc<Prefs>) -> Self {
        let client = HttpClientFactory::create().unwrap_or_else(|_| Client::new());
        Self { prefs, client }
    }

    pub fn with_client(prefs: Arc<Prefs>, client: Client) -> Self {
        Self { prefs, client }
    }

    fn base_url(&self) -> String {
        normalize_server(&self.prefs.listen_brainz_server(), Self::DEFAULT_SERVER)
    }

    pub fn clean_token(token: &str) -> String {
        let mut t = token.trim();
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("token ") {
            t = t[6..].trim();
        } else if lower.starts_with("bearer ") {
            t = t[7..].trim();
        }
        t.replace(['"', '\''], "").trim().to_string()
    }

    /// Checks `token` against `server` and stores it with the user name.
    /// Returns `None` on success, else the reason.
    pub async fn connect(&self, token: &str, server: &str) -> Option<String> {
        let t = Self::clean_token(token);
        if t.is_empty() {
            return Some("Paste your user token.".to_string());
        }
        let base = normalize_server(server, Self::DEFAULT_SERVER);
        let url = format!("{base}/1/validate-token");
        let Ok(resp) = self
            .client
            .get(&url)
            .header(AUTHORIZATION, format!("Token {t}"))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await
        else {
            return Some(format!("Could not reach {base}."));
        };

        let status = resp.status().as_u16();
        let body: Option<Value> = resp.json().await.ok();
        let valid = status / 100 == 2
            && body
                .as_ref()
                .and_then(|v| v.get("valid"))
                .and_then(Value::as_bool)
                == Some(true);
        if !valid {
            if status == 0 {
                return Some(format!("Could not reach {base}."));
            }
            if let Some(msg) = body
                .as_ref()
                .and_then(|v| v.get("message"))
                .and_then(Value::as_str)
            {
                return Some(msg.to_string());
            }
            return Some("The token was not accepted.".to_string());
        }

        let user_name = body
            .as_ref()
            .and_then(|v| v.get("user_name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.prefs.set_listen_brainz(&t, user_name, server.trim());
        None
    }

    pub fn disconnect(&self) {
        self.prefs.set_listen_brainz("", "", "");
    }

    fn metadata(l: &Listen) -> Value {
        let mut obj = Map::new();
        obj.insert("artist_name".to_string(), json!(l.artist));
        obj.insert("track_name".to_string(), json!(l.track));
        if !l.album.is_empty() {
            obj.insert("release_name".to_string(), json!(l.album));
        }
        obj.insert(
            "additional_info".to_string(),
            json!({
                "media_player": "LastWave",
                "submission_client": "LastWave Desktop",
                "submission_client_version": APP_VERSION,
            }),
        );
        Value::Object(obj)
    }

    async fn post(&self, listen_type: &str, l: &Listen) -> bool {
        if !self.connected() {
            return false;
        }
        let timestamp = if l.started_at_sec > 0 {
            l.started_at_sec
        } else {
            now_unix_sec()
        };
        let mut entry = Map::new();
        if listen_type == "single" {
            entry.insert("listened_at".to_string(), json!(timestamp));
        }
        entry.insert("track_metadata".to_string(), Self::metadata(l));

        let payload = json!({
            "listen_type": listen_type,
            "payload": [Value::Object(entry)],
        });
        let token = Self::clean_token(&self.prefs.listen_brainz_token());
        let url = format!("{}/1/submit-listens", self.base_url());
        let Ok(resp) = self
            .client
            .post(&url)
            .header(AUTHORIZATION, format!("Token {token}"))
            .header(CONTENT_TYPE, "application/json")
            .json(&payload)
            .send()
            .await
        else {
            return false;
        };
        let status = resp.status().as_u16();
        let ok = status / 100 == 2;
        if !ok {
            DiagnosticsLog::info(&format!("listenbrainz {listen_type} http={status}"));
        }
        // A rejected payload cannot become valid by retrying and would block later listens.
        ok || (listen_type == "single" && status == 400)
    }

    pub fn profile_url_for(&self, user: &str) -> String {
        let base = self.base_url();
        let site = if base == Self::DEFAULT_SERVER {
            "https://listenbrainz.org"
        } else {
            base.as_str()
        };
        push_user_segment(site, user, true)
    }

    fn effective_user(&self, viewing_as: Option<&str>) -> Option<String> {
        let u = viewing_as
            .map(str::trim)
            .unwrap_or(&self.username())
            .trim()
            .to_string();
        if u.is_empty() {
            None
        } else {
            Some(u)
        }
    }

    async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Option<Value> {
        let url = format!("{}{}", self.base_url(), path);
        let mut req = self.client.get(&url).query(query);
        if self.connected() {
            let token = Self::clean_token(&self.prefs.listen_brainz_token());
            req = req
                .header(AUTHORIZATION, format!("Token {token}"))
                .header(CONTENT_TYPE, "application/json");
        }
        let resp = req.send().await.ok()?;
        let status = resp.status().as_u16();
        if status / 100 != 2 || status == 204 {
            return None;
        }
        let val: Value = resp.json().await.ok()?;
        if val.is_object() {
            Some(val)
        } else {
            None
        }
    }

    fn lb_range(period: &str) -> &'static str {
        match period {
            "7day" => "week",
            "1month" => "month",
            "3month" | "6month" | "12month" => "year",
            _ => "all_time",
        }
    }

    fn caa_thumb(map: &Map<String, Value>) -> String {
        let rel = map
            .get("caa_release_mbid")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if rel.is_empty() {
            return String::new();
        }
        let caa_id = map
            .get("caa_id")
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .unwrap_or_default();
        if !caa_id.is_empty() {
            format!("https://coverartarchive.org/release/{rel}/{caa_id}-250.jpg")
        } else {
            format!("https://coverartarchive.org/release/{rel}/front-250")
        }
    }
}

impl ExtraScrobbler for ListenBrainzScrobbler {
    fn name(&self) -> &'static str {
        "ListenBrainz"
    }

    fn connected(&self) -> bool {
        !self.prefs.listen_brainz_token().is_empty()
    }

    fn username(&self) -> String {
        self.prefs.listen_brainz_user()
    }

    fn now_playing(&self, l: Listen) -> BoxFuture<'_, bool> {
        Box::pin(async move { self.post("playing_now", &l).await })
    }

    fn submit(&self, l: Listen) -> BoxFuture<'_, bool> {
        Box::pin(async move { self.post("single", &l).await })
    }

    fn listen_count(&self) -> BoxFuture<'_, Option<i64>> {
        Box::pin(async move {
            let user = self.username();
            if !self.connected() || user.is_empty() {
                return None;
            }
            let path = format!("/1/user/{}/listen-count", encode_segment(&user));
            let json = self.get_json(&path, &[]).await?;
            json.get("payload")
                .and_then(|p| p.get("count"))
                .and_then(Value::as_i64)
        })
    }

    fn profile_url(&self) -> String {
        self.profile_url_for(&self.username())
    }

    fn as_read_backend(&self) -> Option<&dyn ScrobblerReadBackend> {
        Some(self)
    }
}

impl ScrobblerReadBackend for ListenBrainzScrobbler {
    fn love_track<'a>(
        &'a self,
        artist: &'a str,
        track: &'a str,
        loved: bool,
    ) -> BoxFuture<'a, bool> {
        Box::pin(async move {
            if !self.connected() || artist.trim().is_empty() || track.trim().is_empty() {
                return false;
            }
            let Some(lookup) = self
                .get_json(
                    "/1/metadata/lookup/",
                    &[
                        ("artist_name", artist.to_string()),
                        ("recording_name", track.to_string()),
                    ],
                )
                .await
            else {
                return false;
            };
            let mbid = lookup
                .get("recording_mbid")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if mbid.is_empty() {
                return false;
            }
            let token = Self::clean_token(&self.prefs.listen_brainz_token());
            let url = format!("{}/1/feedback/recording-feedback", self.base_url());
            let Ok(resp) = self
                .client
                .post(&url)
                .header(AUTHORIZATION, format!("Token {token}"))
                .header(CONTENT_TYPE, "application/json")
                .json(&json!({
                    "recording_mbid": mbid,
                    "score": if loved { 1 } else { 0 },
                }))
                .send()
                .await
            else {
                return false;
            };
            resp.status().as_u16() / 100 == 2
        })
    }

    fn fetch_recent_tracks<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        limit: usize,
        _page: usize,
    ) -> BoxFuture<'a, Vec<HomeTrack>> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return Vec::new();
            };
            let path = format!("/1/user/{}/listens", encode_segment(&user));
            let Some(json) = self.get_json(&path, &[("count", limit.to_string())]).await else {
                return Vec::new();
            };
            let Some(listens) = json
                .get("payload")
                .and_then(|p| p.get("listens"))
                .and_then(Value::as_array)
            else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for item in listens {
                let Some(meta) = item.get("track_metadata").and_then(Value::as_object) else {
                    continue;
                };
                let name = meta
                    .get("track_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                let artist = meta
                    .get("artist_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let ts_sec = item.get("listened_at").and_then(json_as_i64);
                let artwork_url = meta
                    .get("mbid_mapping")
                    .and_then(Value::as_object)
                    .map(Self::caa_thumb)
                    .unwrap_or_default();
                out.push(HomeTrack {
                    name,
                    artist,
                    artwork_url,
                    timestamp_millis: ts_sec.map(|s| s * 1000),
                    play_count: 0,
                });
            }
            out
        })
    }

    fn fetch_stats<'a>(&'a self, viewing_as: Option<&'a str>) -> BoxFuture<'a, HomeStats> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return HomeStats::default();
            };
            let enc = encode_segment(&user);
            let p0 = format!("/1/user/{enc}/listen-count");
            let p1 = format!("/1/stats/user/{enc}/recordings");
            let p2 = format!("/1/stats/user/{enc}/artists");
            let p3 = format!("/1/stats/user/{enc}/releases");
            let q = [("range", "all_time".to_string()), ("count", "1".to_string())];
            let (r0, r1, r2, r3) = tokio::join!(
                self.get_json(&p0, &[]),
                self.get_json(&p1, &q),
                self.get_json(&p2, &q),
                self.get_json(&p3, &q),
            );
            let payload_int = |root: &Option<Value>, key: &str| -> Option<i64> {
                root.as_ref()?
                    .get("payload")?
                    .get(key)
                    .and_then(json_as_i64)
            };
            HomeStats {
                scrobbles: payload_int(&r0, "count").unwrap_or(0),
                track_count: payload_int(&r1, "total_recording_count")
                    .or_else(|| payload_int(&r1, "total_recordings_count"))
                    .unwrap_or(0),
                artist_count: payload_int(&r2, "total_artist_count").unwrap_or(0),
                album_count: payload_int(&r3, "total_release_count").unwrap_or(0),
                avatar_url: String::new(),
            }
        })
    }

    fn fetch_period_play_count<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        from_millis: i64,
        to_millis: i64,
    ) -> BoxFuture<'a, i64> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return 0;
            };
            let path = format!("/1/user/{}/listens", encode_segment(&user));
            let Some(json) = self
                .get_json(
                    &path,
                    &[
                        ("min_ts", (from_millis / 1000).to_string()),
                        ("max_ts", (to_millis / 1000).to_string()),
                        ("count", "100".to_string()),
                    ],
                )
                .await
            else {
                return 0;
            };
            let Some(payload) = json.get("payload").and_then(Value::as_object) else {
                return 0;
            };
            if let Some(count) = payload.get("count").and_then(json_as_i64) {
                return count;
            }
            payload
                .get("listens")
                .and_then(Value::as_array)
                .map(|a| a.len() as i64)
                .unwrap_or(0)
        })
    }

    fn fetch_top_tracks<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        period: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Vec<HomeTrack>> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return Vec::new();
            };
            let path = format!("/1/stats/user/{}/recordings", encode_segment(&user));
            let Some(json) = self
                .get_json(
                    &path,
                    &[
                        ("range", Self::lb_range(period).to_string()),
                        ("count", limit.to_string()),
                    ],
                )
                .await
            else {
                return Vec::new();
            };
            let Some(recs) = json
                .get("payload")
                .and_then(|p| p.get("recordings"))
                .and_then(Value::as_array)
            else {
                return Vec::new();
            };
            recs.iter()
                .filter_map(|v| v.as_object())
                .filter_map(|r| {
                    let name = r.get("track_name").and_then(Value::as_str)?.to_string();
                    if name.is_empty() {
                        return None;
                    }
                    Some(HomeTrack {
                        name,
                        artist: r
                            .get("artist_name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        artwork_url: Self::caa_thumb(r),
                        timestamp_millis: None,
                        play_count: r.get("listen_count").and_then(json_as_i64).unwrap_or(0),
                    })
                })
                .collect()
        })
    }

    fn fetch_top_artists<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        period: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Vec<TopArtistItem>> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return Vec::new();
            };
            let path = format!("/1/stats/user/{}/artists", encode_segment(&user));
            let Some(json) = self
                .get_json(
                    &path,
                    &[
                        ("range", Self::lb_range(period).to_string()),
                        ("count", limit.to_string()),
                    ],
                )
                .await
            else {
                return Vec::new();
            };
            let Some(artists) = json
                .get("payload")
                .and_then(|p| p.get("artists"))
                .and_then(Value::as_array)
            else {
                return Vec::new();
            };
            artists
                .iter()
                .filter_map(|v| v.as_object())
                .filter_map(|a| {
                    let name = a.get("artist_name").and_then(Value::as_str)?.to_string();
                    if name.is_empty() {
                        return None;
                    }
                    Some(TopArtistItem {
                        name,
                        play_count: a.get("listen_count").and_then(json_as_i64).unwrap_or(0),
                        artwork_url: String::new(),
                    })
                })
                .collect()
        })
    }

    fn fetch_top_albums<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        period: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Vec<TopAlbumItem>> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return Vec::new();
            };
            let path = format!("/1/stats/user/{}/releases", encode_segment(&user));
            let Some(json) = self
                .get_json(
                    &path,
                    &[
                        ("range", Self::lb_range(period).to_string()),
                        ("count", limit.to_string()),
                    ],
                )
                .await
            else {
                return Vec::new();
            };
            let Some(releases) = json
                .get("payload")
                .and_then(|p| p.get("releases"))
                .and_then(Value::as_array)
            else {
                return Vec::new();
            };
            releases
                .iter()
                .filter_map(|v| v.as_object())
                .filter_map(|r| {
                    let name = r.get("release_name").and_then(Value::as_str)?.to_string();
                    if name.is_empty() {
                        return None;
                    }
                    Some(TopAlbumItem {
                        name,
                        artist: r
                            .get("artist_name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        play_count: r.get("listen_count").and_then(json_as_i64).unwrap_or(0),
                        artwork_url: Self::caa_thumb(r),
                    })
                })
                .collect()
        })
    }
}

// -- Libre.fm -----------------------------------------------------------------

/// Libre.fm (or any GNU FM server): the Last.fm 2.0 protocol at another
/// address, with a mobile session from username + password.
pub struct LibreFmScrobbler {
    prefs: Arc<Prefs>,
    client: Client,
}

impl LibreFmScrobbler {
    pub const DEFAULT_SERVER: &'static str = "https://libre.fm";
    const API_KEY: &'static str = "lastwavedesktop00000000000000000";
    const API_SECRET: &'static str = "lastwavedesktopsecret00000000000";

    pub fn new(prefs: Arc<Prefs>) -> Self {
        let client = HttpClientFactory::create().unwrap_or_else(|_| Client::new());
        Self { prefs, client }
    }

    pub fn with_client(prefs: Arc<Prefs>, client: Client) -> Self {
        Self { prefs, client }
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/2.0/",
            normalize_server(&self.prefs.libre_fm_server(), Self::DEFAULT_SERVER)
        )
    }

    pub fn sign(params: &BTreeMap<String, String>, secret: &str) -> String {
        AuthCrypto::api_sig(
            params.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            secret,
        )
    }

    async fn call(&self, endpoint: &str, params: &BTreeMap<String, String>) -> Option<Value> {
        let mut signed = params.clone();
        signed.insert("api_key".to_string(), Self::API_KEY.to_string());
        let sig = Self::sign(&signed, Self::API_SECRET);
        signed.insert("api_sig".to_string(), sig);
        signed.insert("format".to_string(), "json".to_string());

        let resp = self.client.post(endpoint).form(&signed).send().await.ok()?;
        let status = resp.status().as_u16();
        if status / 100 != 2 {
            let method = params.get("method").map(String::as_str).unwrap_or("");
            DiagnosticsLog::info(&format!("librefm {method} http={status}"));
            return None;
        }
        let raw = resp.text().await.ok()?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.starts_with('{') {
            let val: Value = serde_json::from_str(trimmed).ok()?;
            return if val.is_object() { Some(val) } else { None };
        }
        if trimmed.starts_with('<') {
            return Self::parse_xml(trimmed);
        }
        None
    }

    pub fn parse_xml(xml: &str) -> Option<Value> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut buf = Vec::new();
        let mut map = Map::new();
        let mut in_lfm = false;
        let mut stack: Vec<String> = Vec::new();

        let mut session_key: Option<String> = None;
        let mut session_name: Option<String> = None;
        let mut has_session = false;
        let mut playcount: Option<String> = None;
        let mut has_scrobbles = false;
        let mut accepted: Option<String> = None;
        let mut ignored: Option<String> = None;
        let mut has_scrobble = false;
        let mut has_nowplaying = false;
        let mut error_text: Option<String> = None;
        let mut ignored_code: Option<String> = None;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                    if stack.is_empty() {
                        if local != "lfm" {
                            return Some(Value::Object(Map::new()));
                        }
                        in_lfm = true;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"status" {
                                if let Ok(v) = attr.unescape_value() {
                                    map.insert("status".to_string(), json!(v.into_owned()));
                                }
                            }
                        }
                    } else if in_lfm {
                        match local.as_str() {
                            "session" => has_session = true,
                            "scrobbles" => {
                                has_scrobbles = true;
                                for attr in e.attributes().flatten() {
                                    let k = attr.key.as_ref();
                                    if let Ok(v) = attr.unescape_value() {
                                        if k == b"accepted" {
                                            accepted = Some(v.into_owned());
                                        } else if k == b"ignored" {
                                            ignored = Some(v.into_owned());
                                        }
                                    }
                                }
                            }
                            "scrobble" if stack.last().map(String::as_str) == Some("scrobbles") => {
                                has_scrobble = true;
                            }
                            "nowplaying" => has_nowplaying = true,
                            "ignoredMessage" => {
                                for attr in e.attributes().flatten() {
                                    if attr.key.as_ref() == b"code" {
                                        if let Ok(v) = attr.unescape_value() {
                                            ignored_code = Some(v.into_owned());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    stack.push(local);
                }
                Ok(Event::Empty(e)) => {
                    let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                    if stack.is_empty() && local != "lfm" {
                        return Some(Value::Object(Map::new()));
                    }
                    if in_lfm {
                        match local.as_str() {
                            "scrobbles" => {
                                has_scrobbles = true;
                                for attr in e.attributes().flatten() {
                                    let k = attr.key.as_ref();
                                    if let Ok(v) = attr.unescape_value() {
                                        if k == b"accepted" {
                                            accepted = Some(v.into_owned());
                                        } else if k == b"ignored" {
                                            ignored = Some(v.into_owned());
                                        }
                                    }
                                }
                            }
                            "scrobble" if stack.last().map(String::as_str) == Some("scrobbles") => {
                                has_scrobble = true;
                            }
                            "nowplaying" => has_nowplaying = true,
                            "ignoredMessage" => {
                                for attr in e.attributes().flatten() {
                                    if attr.key.as_ref() == b"code" {
                                        if let Ok(v) = attr.unescape_value() {
                                            ignored_code = Some(v.into_owned());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Event::Text(t)) => {
                    if in_lfm {
                        if let Ok(text) = t.unescape() {
                            let val = text.into_owned();
                            let path: Vec<&str> = stack.iter().map(String::as_str).collect();
                            match path.as_slice() {
                                ["lfm", "error"] => error_text = Some(val),
                                ["lfm", "session", "key"] => session_key = Some(val),
                                ["lfm", "session", "name"] => session_name = Some(val),
                                ["lfm", "user", "playcount"] => playcount = Some(val),
                                _ => {}
                            }
                        }
                    }
                }
                Ok(Event::End(_)) => {
                    stack.pop();
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(_) => return None,
            }
            buf.clear();
        }

        if let Some(err) = error_text {
            map.insert("error".to_string(), json!(err));
            map.insert("message".to_string(), json!(err));
        }
        if map.get("status").and_then(Value::as_str) != Some("ok") {
            return Some(Value::Object(map));
        }
        if has_session {
            map.insert(
                "session".to_string(),
                json!({
                    "key": session_key,
                    "name": session_name,
                }),
            );
        }
        if let Some(pc) = playcount {
            map.insert("user".to_string(), json!({ "playcount": pc }));
        }
        if has_scrobbles {
            let mut scr = Map::new();
            scr.insert(
                "@attr".to_string(),
                json!({
                    "accepted": accepted,
                    "ignored": ignored,
                }),
            );
            if has_scrobble {
                scr.insert("scrobble".to_string(), json!({}));
            }
            map.insert("scrobbles".to_string(), Value::Object(scr));
        }
        if has_nowplaying {
            map.insert("nowplaying".to_string(), json!({}));
        }
        if let Some(code) = ignored_code {
            map.insert("ignoredMessageCode".to_string(), json!(code));
        }
        Some(Value::Object(map))
    }

    /// Signs in and stores the session. `None` on success, else the reason.
    pub async fn connect(&self, user: &str, password: &str, server: &str) -> Option<String> {
        let u = user.trim();
        if u.is_empty() || password.is_empty() {
            return Some("Enter your username and password.".to_string());
        }
        let base = normalize_server(server, Self::DEFAULT_SERVER);
        let endpoint = format!("{base}/2.0/");
        let pass_md5 = AuthCrypto::md5_hex(password);

        let token_exact = AuthCrypto::md5_hex(&format!("{u}{pass_md5}"));
        let mut p1 = BTreeMap::new();
        p1.insert("method".to_string(), "auth.getMobileSession".to_string());
        p1.insert("username".to_string(), u.to_string());
        p1.insert("authToken".to_string(), token_exact.clone());
        let mut body = self.call(&endpoint, &p1).await;

        let has_session = |b: &Option<Value>| {
            b.as_ref()
                .and_then(|v| v.get("session"))
                .is_some_and(|s| !s.is_null())
        };

        if !has_session(&body) {
            let token_lower = AuthCrypto::md5_hex(&format!("{}{pass_md5}", u.to_ascii_lowercase()));
            if token_lower != token_exact {
                let mut p2 = BTreeMap::new();
                p2.insert("method".to_string(), "auth.getMobileSession".to_string());
                p2.insert("username".to_string(), u.to_string());
                p2.insert("authToken".to_string(), token_lower);
                let retry = self.call(&endpoint, &p2).await;
                if has_session(&retry) {
                    body = retry;
                }
            }
        }

        if !has_session(&body) {
            let mut p3 = BTreeMap::new();
            p3.insert("method".to_string(), "auth.getMobileSession".to_string());
            p3.insert("username".to_string(), u.to_string());
            p3.insert("password".to_string(), password.to_string());
            let retry_plain = self.call(&endpoint, &p3).await;
            if has_session(&retry_plain) {
                body = retry_plain;
            }
        }

        let Some(body) = body else {
            return Some(format!("Could not reach {base}."));
        };
        let key = body
            .get("session")
            .and_then(|s| s.get("key"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if key.is_empty() {
            let reason = body
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| body.get("error").and_then(Value::as_str))
                .unwrap_or("Sign-in was not accepted.");
            return Some(reason.to_string());
        }
        let name = body
            .get("session")
            .and_then(|s| s.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(u);
        self.prefs.set_libre_fm(key, name, server.trim());
        None
    }

    pub fn disconnect(&self) {
        self.prefs.set_libre_fm("", "", "");
    }

    async fn write(&self, method: &str, extra: &[(&str, String)]) -> bool {
        if !self.connected() {
            return false;
        }
        let mut params = BTreeMap::new();
        params.insert("method".to_string(), method.to_string());
        params.insert("sk".to_string(), self.prefs.libre_fm_session());
        for (k, v) in extra {
            if !v.is_empty() {
                params.insert((*k).to_string(), v.clone());
            }
        }
        let body = self.call(&self.endpoint(), &params).await;
        let scrobbles = body.as_ref().and_then(|b| b.get("scrobbles"));
        let accepted = scrobbles
            .and_then(|s| s.get("@attr"))
            .and_then(|a| a.get("accepted"))
            .and_then(json_as_i64)
            .unwrap_or(0);
        let result = if method == "track.scrobble" {
            scrobbles.and_then(|s| s.get("scrobble"))
        } else {
            body.as_ref().and_then(|b| b.get("nowplaying"))
        };
        let raw_code = body
            .as_ref()
            .and_then(|b| b.get("ignoredMessageCode"))
            .or_else(|| {
                result
                    .and_then(|r| r.get("ignoredMessage"))
                    .and_then(|im| im.get("code"))
            });
        let code = raw_code.and_then(json_as_i64);
        let consumed_code = raw_code.is_none()
            || code.is_some_and(|c| (0..=4).contains(&c) || c == 91);

        let status_ok = body
            .as_ref()
            .and_then(|b| b.get("status"))
            .and_then(Value::as_str)
            .is_none_or(|s| s == "ok");
        let ok = body
            .as_ref()
            .is_some_and(|b| b.get("error").is_none())
            && status_ok
            && (result.is_some_and(Value::is_object)
                || (method == "track.scrobble" && accepted > 0))
            && consumed_code;

        if !ok {
            DiagnosticsLog::info(&format!("librefm {method} ok=false"));
        } else if let Some(c) = code {
            if (1..=4).contains(&c) {
                DiagnosticsLog::info(&format!("librefm {method} ignored={c}"));
            }
        }
        ok
    }

    fn effective_user(&self, viewing_as: Option<&str>) -> Option<String> {
        let u = viewing_as
            .map(str::trim)
            .unwrap_or(&self.username())
            .trim()
            .to_string();
        if u.is_empty() {
            None
        } else {
            Some(u)
        }
    }

    async fn read(&self, params: BTreeMap<String, String>) -> Option<Value> {
        let body = self.call(&self.endpoint(), &params).await?;
        if body.get("error").is_some() {
            None
        } else {
            Some(body)
        }
    }
}

impl ExtraScrobbler for LibreFmScrobbler {
    fn name(&self) -> &'static str {
        "Libre.fm"
    }

    fn connected(&self) -> bool {
        !self.prefs.libre_fm_session().is_empty()
    }

    fn username(&self) -> String {
        self.prefs.libre_fm_user()
    }

    fn now_playing(&self, l: Listen) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            self.write(
                "track.updateNowPlaying",
                &[("artist", l.artist), ("track", l.track), ("album", l.album)],
            )
            .await
        })
    }

    fn submit(&self, l: Listen) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            self.write(
                "track.scrobble",
                &[
                    ("artist[0]", l.artist),
                    ("track[0]", l.track),
                    ("album[0]", l.album),
                    ("timestamp[0]", l.started_at_sec.to_string()),
                ],
            )
            .await
        })
    }

    fn listen_count(&self) -> BoxFuture<'_, Option<i64>> {
        Box::pin(async move {
            let user = self.username();
            if !self.connected() || user.is_empty() {
                return None;
            }
            let mut params = BTreeMap::new();
            params.insert("method".to_string(), "user.getInfo".to_string());
            params.insert("user".to_string(), user);
            let body = self.call(&self.endpoint(), &params).await?;
            body.get("user")
                .and_then(|u| u.get("playcount"))
                .and_then(json_as_i64)
        })
    }

    fn profile_url(&self) -> String {
        let base = normalize_server(&self.prefs.libre_fm_server(), Self::DEFAULT_SERVER);
        push_user_segment(&base, &self.username(), false)
    }

    fn as_read_backend(&self) -> Option<&dyn ScrobblerReadBackend> {
        Some(self)
    }
}

impl ScrobblerReadBackend for LibreFmScrobbler {
    fn love_track<'a>(
        &'a self,
        artist: &'a str,
        track: &'a str,
        loved: bool,
    ) -> BoxFuture<'a, bool> {
        Box::pin(async move {
            self.write(
                if loved { "track.love" } else { "track.unlove" },
                &[
                    ("artist", artist.to_string()),
                    ("track", track.to_string()),
                ],
            )
            .await
        })
    }

    fn fetch_recent_tracks<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        limit: usize,
        page: usize,
    ) -> BoxFuture<'a, Vec<HomeTrack>> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return Vec::new();
            };
            let mut params = BTreeMap::new();
            params.insert("method".to_string(), "user.getrecenttracks".to_string());
            params.insert("user".to_string(), user);
            params.insert("limit".to_string(), limit.to_string());
            params.insert("page".to_string(), page.to_string());
            params.insert("extended".to_string(), "0".to_string());
            let Some(json) = self.read(params).await else {
                return Vec::new();
            };
            LastFmResponseParser::parse_recent_tracks(&json)
        })
    }

    fn fetch_stats<'a>(&'a self, viewing_as: Option<&'a str>) -> BoxFuture<'a, HomeStats> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return HomeStats::default();
            };
            let mut p0 = BTreeMap::new();
            p0.insert("method".to_string(), "user.getinfo".to_string());
            p0.insert("user".to_string(), user.clone());

            let make_top = |method: &str| {
                let mut m = BTreeMap::new();
                m.insert("method".to_string(), method.to_string());
                m.insert("user".to_string(), user.clone());
                m.insert("limit".to_string(), "1".to_string());
                m.insert("period".to_string(), "overall".to_string());
                m
            };

            let (r0, r1, r2, r3) = tokio::join!(
                self.read(p0),
                self.read(make_top("user.gettoptracks")),
                self.read(make_top("user.gettopartists")),
                self.read(make_top("user.gettopalbums")),
            );
            LastFmResponseParser::parse_stats(
                r0.as_ref().unwrap_or(&json!({})),
                r1.as_ref(),
                r2.as_ref(),
                r3.as_ref(),
            )
        })
    }

    fn fetch_period_play_count<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        from_millis: i64,
        to_millis: i64,
    ) -> BoxFuture<'a, i64> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return 0;
            };
            let mut params = BTreeMap::new();
            params.insert("method".to_string(), "user.getrecenttracks".to_string());
            params.insert("user".to_string(), user);
            params.insert("from".to_string(), (from_millis / 1000).to_string());
            params.insert("to".to_string(), (to_millis / 1000).to_string());
            params.insert("limit".to_string(), "1".to_string());
            params.insert("page".to_string(), "1".to_string());
            let Some(json) = self.read(params).await else {
                return 0;
            };
            LastFmResponseParser::parse_period_play_count(&json)
        })
    }

    fn fetch_top_tracks<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        period: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Vec<HomeTrack>> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return Vec::new();
            };
            let mut params = BTreeMap::new();
            params.insert("method".to_string(), "user.gettoptracks".to_string());
            params.insert("user".to_string(), user);
            params.insert("period".to_string(), period.to_string());
            params.insert("limit".to_string(), limit.to_string());
            let Some(json) = self.read(params).await else {
                return Vec::new();
            };
            LastFmResponseParser::parse_top_tracks(&json)
        })
    }

    fn fetch_top_artists<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        period: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Vec<TopArtistItem>> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return Vec::new();
            };
            let mut params = BTreeMap::new();
            params.insert("method".to_string(), "user.gettopartists".to_string());
            params.insert("user".to_string(), user);
            params.insert("period".to_string(), period.to_string());
            params.insert("limit".to_string(), limit.to_string());
            let Some(json) = self.read(params).await else {
                return Vec::new();
            };
            LastFmResponseParser::parse_top_artists(&json)
        })
    }

    fn fetch_top_albums<'a>(
        &'a self,
        viewing_as: Option<&'a str>,
        period: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Vec<TopAlbumItem>> {
        Box::pin(async move {
            let Some(user) = self.effective_user(viewing_as) else {
                return Vec::new();
            };
            let mut params = BTreeMap::new();
            params.insert("method".to_string(), "user.gettopalbums".to_string());
            params.insert("user".to_string(), user);
            params.insert("period".to_string(), period.to_string());
            params.insert("limit".to_string(), limit.to_string());
            let Some(json) = self.read(params).await else {
                return Vec::new();
            };
            LastFmResponseParser::parse_top_albums(&json)
        })
    }
}

/// `https://host[:port][/path]` without a trailing slash; `fallback` when
/// blank. A bare host gets `https://`.
pub fn normalize_server(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    let mut s = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    while s.ends_with('/') {
        s.pop();
    }
    s
}

// -- fan-out ------------------------------------------------------------------

#[derive(Default)]
struct ScrobbleQueue {
    listens: VecDeque<Listen>,
    consumed_order: VecDeque<(String, String, i64)>,
    consumed_set: HashSet<(String, String, i64)>,
}

struct ServiceSlot {
    service: Arc<dyn ExtraScrobbler>,
    queue: Arc<Mutex<ScrobbleQueue>>,
}

/// Sends every listen to each connected service. Failed submits wait in a
/// strictly serialized per-service queue (max 200) and go out before that
/// service's next listen.
pub struct ExtraScrobblers {
    slots: Vec<ServiceSlot>,
}

impl ExtraScrobblers {
    pub fn new(services: Vec<Arc<dyn ExtraScrobbler>>) -> Self {
        let slots = services
            .into_iter()
            .map(|service| ServiceSlot {
                service,
                queue: Arc::new(Mutex::new(ScrobbleQueue::default())),
            })
            .collect();
        Self { slots }
    }

    pub fn services(&self) -> Vec<Arc<dyn ExtraScrobbler>> {
        self.slots.iter().map(|s| Arc::clone(&s.service)).collect()
    }

    pub fn now_playing(&self, l: Listen) {
        for slot in &self.slots {
            if slot.service.connected() {
                let svc = Arc::clone(&slot.service);
                let listen = l.clone();
                tokio::spawn(async move {
                    let _ = svc.now_playing(listen).await;
                });
            }
        }
    }

    pub fn submit(&self, l: Listen) {
        for slot in &self.slots {
            if slot.service.connected() {
                let svc = Arc::clone(&slot.service);
                let queue = Arc::clone(&slot.queue);
                let listen = l.clone();
                tokio::spawn(async move {
                    submit_to_service(&*svc, &queue, listen).await;
                });
            }
        }
    }

    /// Awaits serialized per-service submission across all connected services.
    pub async fn submit_and_wait(&self, l: Listen) {
        let futs = self.slots.iter().filter(|slot| slot.service.connected()).map(|slot| {
            let svc = Arc::clone(&slot.service);
            let queue = Arc::clone(&slot.queue);
            let listen = l.clone();
            async move {
                submit_to_service(&*svc, &queue, listen).await;
            }
        });
        futures_util::future::join_all(futs).await;
    }

    pub fn love_track(&self, artist: &str, track: &str, loved: bool) {
        for slot in &self.slots {
            if slot.service.connected() && slot.service.as_read_backend().is_some() {
                let svc = Arc::clone(&slot.service);
                let a = artist.to_string();
                let t = track.to_string();
                tokio::spawn(async move {
                    if let Some(backend) = svc.as_read_backend() {
                        let _ = backend.love_track(&a, &t, loved).await;
                    }
                });
            }
        }
    }

    pub async fn pending_len(&self, service_index: usize) -> usize {
        let Some(slot) = self.slots.get(service_index) else {
            return 0;
        };
        slot.queue.lock().await.listens.len()
    }
}

async fn submit_to_service(
    service: &dyn ExtraScrobbler,
    queue_mutex: &Mutex<ScrobbleQueue>,
    listen: Listen,
) {
    let mut state = queue_mutex.lock().await;
    let key = listen.key();
    if !state.consumed_set.contains(&key) && !state.listens.iter().any(|p| p.key() == key) {
        state.listens.push_back(listen);
    }
    while state.listens.len() > MAX_QUEUE_LEN {
        state.listens.pop_front();
    }
    while service.connected() {
        let Some(pending) = state.listens.front().cloned() else {
            break;
        };
        if !service.submit(pending.clone()).await {
            DiagnosticsLog::info(&format!("{} submission failed", service.name()));
            break;
        }
        state.listens.pop_front();
        let consumed_key = pending.key();
        if state.consumed_set.insert(consumed_key.clone()) {
            state.consumed_order.push_back(consumed_key);
        }
        while state.consumed_order.len() > MAX_QUEUE_LEN {
            if let Some(oldest) = state.consumed_order.pop_front() {
                state.consumed_set.remove(&oldest);
            }
        }
    }
}

fn now_unix_sec() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn json_as_i64(val: &Value) -> Option<i64> {
    match val {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn encode_segment(raw: &str) -> String {
    let mut url = Url::parse("https://example.invalid/").expect("valid base");
    if let Ok(mut segs) = url.path_segments_mut() {
        segs.push(raw);
    }
    url.path().trim_start_matches('/').to_string()
}

fn push_user_segment(base: &str, user: &str, trailing_slash: bool) -> String {
    let Ok(mut url) = Url::parse(base) else {
        return format!("{base}/user/{user}");
    };
    if let Ok(mut segs) = url.path_segments_mut() {
        segs.pop_if_empty();
        segs.push("user");
        segs.push(user);
        if trailing_slash {
            segs.push("");
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct MockScrobbler {
        label: &'static str,
        should_succeed: AtomicBool,
        in_flight: AtomicUsize,
        max_concurrent: AtomicUsize,
        delivered: Mutex<Vec<String>>,
    }

    impl MockScrobbler {
        fn new(label: &'static str, should_succeed: bool) -> Self {
            Self {
                label,
                should_succeed: AtomicBool::new(should_succeed),
                in_flight: AtomicUsize::new(0),
                max_concurrent: AtomicUsize::new(0),
                delivered: Mutex::new(Vec::new()),
            }
        }
    }

    impl ExtraScrobbler for MockScrobbler {
        fn name(&self) -> &'static str {
            self.label
        }

        fn connected(&self) -> bool {
            true
        }

        fn username(&self) -> String {
            "tester".to_string()
        }

        fn now_playing(&self, _l: Listen) -> BoxFuture<'_, bool> {
            Box::pin(async { true })
        }

        fn submit(&self, l: Listen) -> BoxFuture<'_, bool> {
            Box::pin(async move {
                let active = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_concurrent.fetch_max(active, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                let ok = self.should_succeed.load(Ordering::SeqCst);
                if ok {
                    self.delivered.lock().await.push(l.track);
                }
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                ok
            })
        }

        fn listen_count(&self) -> BoxFuture<'_, Option<i64>> {
            Box::pin(async { None })
        }

        fn profile_url(&self) -> String {
            "https://example.com/user/tester".to_string()
        }
    }

    #[test]
    fn normalize_server_and_clean_token() {
        assert_eq!(
            normalize_server("", ListenBrainzScrobbler::DEFAULT_SERVER),
            "https://api.listenbrainz.org"
        );
        assert_eq!(
            normalize_server("maloja.local:42010///", ListenBrainzScrobbler::DEFAULT_SERVER),
            "https://maloja.local:42010"
        );
        assert_eq!(
            ListenBrainzScrobbler::clean_token("  Token 'abc-123' "),
            "abc-123"
        );
        assert_eq!(
            ListenBrainzScrobbler::clean_token("Bearer \"xyz\""),
            "xyz"
        );
    }

    #[test]
    fn librefm_xml_and_signing() {
        let xml = r#"<lfm status="ok"><scrobbles accepted="1" ignored="0"><scrobble><ignoredMessage code="0"></ignoredMessage></scrobble></scrobbles></lfm>"#;
        let parsed = LibreFmScrobbler::parse_xml(xml).unwrap();
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["scrobbles"]["@attr"]["accepted"], "1");
        assert_eq!(parsed["ignoredMessageCode"], "0");

        let mut params = BTreeMap::new();
        params.insert("method".to_string(), "track.scrobble".to_string());
        params.insert("format".to_string(), "json".to_string());
        let expected = AuthCrypto::md5_hex("methodtrack.scrobblesecret");
        assert_eq!(LibreFmScrobbler::sign(&params, "secret"), expected);
    }

    #[tokio::test]
    async fn extra_scrobblers_serialize_queues_and_isolate_failures() {
        let lb = Arc::new(MockScrobbler::new("ListenBrainz", false));
        let lf = Arc::new(MockScrobbler::new("Libre.fm", true));
        let fanout = ExtraScrobblers::new(vec![lb.clone(), lf.clone()]);

        // First submit: LB fails and keeps track1 queued; LF succeeds immediately.
        fanout
            .submit_and_wait(Listen::new("Artist", "Track 1", "", 100))
            .await;
        assert_eq!(fanout.pending_len(0).await, 1);
        assert_eq!(fanout.pending_len(1).await, 0);
        assert_eq!(*lf.delivered.lock().await, vec!["Track 1"]);

        // Recover LB and fire concurrent submissions; queue must drain Track 1 -> Track 2 -> Track 3
        // strictly serialized (max_concurrent == 1).
        lb.should_succeed.store(true, Ordering::SeqCst);
        tokio::join!(
            fanout.submit_and_wait(Listen::new("Artist", "Track 2", "", 200)),
            fanout.submit_and_wait(Listen::new("Artist", "Track 3", "", 300)),
        );
        assert_eq!(lb.max_concurrent.load(Ordering::SeqCst), 1);
        assert_eq!(fanout.pending_len(0).await, 0);
        let lb_delivered = lb.delivered.lock().await.clone();
        assert_eq!(lb_delivered.len(), 3);
        assert_eq!(lb_delivered[0], "Track 1");

        // Re-submitting an already consumed listen is deduplicated.
        fanout
            .submit_and_wait(Listen::new("Artist", "Track 1", "", 100))
            .await;
        assert_eq!(lb.delivered.lock().await.len(), 3);
    }
}
