use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path as AxumPath, State as AxumState};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::routing::get;
use axum::Router;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::{DateTime, NaiveDateTime, Utc};
use rand::Rng;
use regex::Regex;
use reqwest::header::{COOKIE, ORIGIN, REFERER, USER_AGENT};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex as AsyncMutex};
use unicode_normalization::UnicodeNormalization;
use url::Url;

use crate::audio::play_diag::PlayDiag;
use crate::integrations::podcasts::podcast_parser::PodcastParser;
use crate::models::match_vocab::MatchVocab;
use crate::models::podcast_models::{
    PodcastEpisode, PodcastEpisodePage, PodcastShelf, PodcastShowPage,
};
use crate::models::stream_models::{MusicVideoStream, ResolvedStream};
use crate::storage::app_database::AppDatabase;
use crate::storage::diagnostics_log::DiagnosticsLog;
use crate::storage::secure_store::SecureStore;

use super::album_match::{
    AlbumCandidate, AlbumLookupApi, BoxFuture as AlbumBoxFuture, SongAlbumCandidate,
};
use super::auth_crypto::AuthCrypto;
use super::http_client::HttpClientFactory;
use super::potoken_engine::{PoTokenEngine, PoTokenResult};
use super::signature_decipher::{PlayerScript, SignatureDecipher};
use super::yt_auth_store::YtAuthStore;

// Constants & Stream Logging

pub const MUSIC_API: &str = "https://music.youtube.com/youtubei/v1";
pub const YOUTUBE_API: &str = "https://www.youtube.com/youtubei/v1";
pub const MUSIC_ORIGIN: &str = "https://music.youtube.com";
pub const YOUTUBE_ORIGIN: &str = "https://www.youtube.com";
pub const WEB_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0";

pub const FALLBACK_WEB_KEY: &str = "AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30";
pub const FALLBACK_WEB_VERSION: &str = "1.20260707.12.00";

pub const LIBRARY_PLAYLISTS_BROWSE_ID: &str = "FEmusic_liked_playlists";
pub const HISTORY_BROWSE_ID: &str = "FEmusic_history";
pub const LIKED_BROWSE_ID: &str = "VLLM";
pub const HOME_BROWSE_ID: &str = "FEmusic_home";
pub const NEW_RELEASES_BROWSE_ID: &str = "FEmusic_new_releases";
pub const CHARTS_BROWSE_ID: &str = "FEmusic_charts";
pub const PODCASTS_BROWSE_ID: &str = "FEmusic_podcasts";

pub const LIKE_ENDPOINT: &str = "like/like";
pub const REMOVE_LIKE_ENDPOINT: &str = "like/removelike";

pub const SONGS_SEARCH_FILTER: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA==";
pub const ARTIST_SEARCH_FILTER: &str = "EgWKAQIgAWoKEAkQBRAKEAMQBA==";
pub const ALBUM_SEARCH_FILTER: &str = "EgWKAQIYAWoKEAkQBRAKEAMQBA==";
pub const PLAYLIST_SEARCH_FILTER: &str = "Eg-KAQwIABAAGAAgACgB";

const MAX_CONTINUATION_PAGES: usize = 600;
const MAX_PLAYER_REQUEST_ATTEMPTS: usize = 1;
const CLIENT_COOLDOWN_MS: i64 = 60_000;
const MAX_STREAM_CACHE_ENTRIES: usize = 256;
const STREAM_TTL_MS: i64 = 4 * 60 * 60 * 1000;
const MAX_MATCH_CACHE_ENTRIES: usize = 1024;
const MAX_LAST_RESOLVED_ENTRIES: usize = 256;
const URL_EXPIRY_MARGIN_MS: i64 = 2 * 60 * 1000;
const UNKNOWN_EXPIRY_TTL_MS: i64 = 5 * 60 * 1000;
const PREFETCH_DEADLINE_SECS: u64 = 120;
const FAST_PREFERRED_CLIENT: &str = "VISIONOS";
const MAX_STREAM_CLIENTS: usize = 3;
const FAST_CLIENT_TIMEOUT: Duration = Duration::from_secs(8);

const MATCH_NOISE_WORDS: &[&str] = &[
    "official",
    "audio",
    "video",
    "visualizer",
    "lyrics",
    "lyric",
    "hd",
    "hq",
    "4k",
    "track",
    "music",
];

const CONFIRMED_UNAVAILABLE_REASONS: &[&str] = &[
    "video has been removed",
    "video has been deleted",
    "video was removed",
    "video was deleted",
    "this video is private",
    "this is a private video",
    "not available in your country",
    "blocked in your country",
    "copyright claim",
];

fn log_stream(
    stage: &str,
    video_id: &str,
    client: &str,
    itag: i64,
    mime: &str,
    expiry: &str,
    retry: i32,
    http: u16,
    detail: &str,
) {
    let d: String = detail.chars().take(80).collect();
    let line = format!(
        "stream stage={stage} videoId={video_id} client={client} \
         itag={itag} mime={mime} expiry={expiry} retry={retry} http={http} {d}"
    );
    DiagnosticsLog::detail(&line);
}

// Errors

#[derive(Debug, Error)]
pub enum InnerTubeError {
    #[error("InnerTube HTTP {0}")]
    Http(u16),

    #[error("ConfirmedUnplayable: {0}")]
    ConfirmedUnplayable(String),

    #[error("{0}")]
    NoReliableMatch(String),

    #[error("Invalid cookie input: {0}")]
    InvalidFormat(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Transport error: {0}")]
    Transport(String),
}

// Public Models

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YouTubeMusicTrack {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub duration_seconds: i64,
    #[serde(default)]
    pub album_browse_id: String,
}

impl SongAlbumCandidate for YouTubeMusicTrack {
    fn album_browse_id(&self) -> &str {
        &self.album_browse_id
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn artist(&self) -> &str {
        &self.artist
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum YouTubeEntityKind {
    Artist,
    Album,
    Playlist,
    Mix,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YouTubeMusicEntity {
    pub kind: YouTubeEntityKind,
    pub name: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub browse_id: String,
    #[serde(default)]
    pub playlist_id: String,
    #[serde(default)]
    pub artwork_url: String,
}

impl AlbumCandidate for YouTubeMusicEntity {
    fn browse_id(&self) -> &str {
        &self.browse_id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn artist(&self) -> &str {
        &self.artist
    }

    fn subtitle(&self) -> &str {
        &self.subtitle
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtHomeShelf {
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub tracks: Vec<YouTubeMusicTrack>,
    #[serde(default)]
    pub track_cards: Vec<YouTubeMusicTrack>,
    #[serde(default)]
    pub entities: Vec<YouTubeMusicEntity>,
}

impl YtHomeShelf {
    pub fn is_track_shelf(&self) -> bool {
        !self.tracks.is_empty()
    }

    pub fn is_track_card_shelf(&self) -> bool {
        !self.track_cards.is_empty()
    }

    pub fn is_card_shelf(&self) -> bool {
        !self.entities.is_empty()
    }

    pub fn is_renderable(&self) -> bool {
        self.is_track_shelf() || self.is_track_card_shelf() || self.is_card_shelf()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtArtistPage {
    #[serde(default)]
    pub songs: Vec<YouTubeMusicTrack>,
    #[serde(default)]
    pub shelves: Vec<YtHomeShelf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtOwnedPlaylistItem {
    pub video_id: String,
    #[serde(default)]
    pub set_video_id: String,
    #[serde(default)]
    pub track: Option<YouTubeMusicTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtOwnedPlaylist {
    pub items: Vec<YtOwnedPlaylistItem>,
    #[serde(default = "default_true")]
    pub complete: bool,
    #[serde(default)]
    pub editable: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YouTubePlaylistResult {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub track_count: usize,
    #[serde(default)]
    pub tracks: Vec<YouTubeMusicTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YouTubeAlbumResult {
    pub browse_id: String,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub year: String,
    #[serde(default)]
    pub tracks: Vec<YouTubeMusicTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YouTubePlaylistSummary {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub track_count_text: String,
    #[serde(default)]
    pub artwork_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtAccount {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub photo_url: String,
}

impl YtAccount {
    pub fn is_empty(&self) -> bool {
        self.name.is_empty() && self.handle.is_empty() && self.photo_url.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtConnection {
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub cookies: String,
    #[serde(default)]
    pub account_name: String,
    #[serde(default)]
    pub channel_handle: String,
    #[serde(default)]
    pub photo_url: String,
    #[serde(default)]
    pub connected_at_millis: i64,
    #[serde(default)]
    pub profile_email: String,
    #[serde(default)]
    pub active_page_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtChannel {
    #[serde(default)]
    pub page_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub photo_url: String,
}

impl YtChannel {
    pub fn is_main(&self) -> bool {
        self.page_id.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtProfile {
    pub email: String,
    #[serde(default)]
    pub cookies: String,
    #[serde(default)]
    pub channels: Vec<YtChannel>,
    #[serde(default)]
    pub active_page_id: String,
    #[serde(default)]
    pub last_used_millis: i64,
}

impl YtProfile {
    pub fn list_from_json(raw: &str) -> Vec<YtProfile> {
        let Ok(list) = serde_json::from_str::<Vec<YtProfile>>(raw) else {
            return Vec::new();
        };
        list.into_iter()
            .filter(|p| !p.email.is_empty() || !p.cookies.is_empty())
            .collect()
    }

    pub fn list_to_json(profiles: &[YtProfile]) -> String {
        serde_json::to_string(profiles).unwrap_or_else(|_| "[]".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptCue {
    pub time_ms: i64,
    pub text: String,
}

// Player Client Table (14 clients)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum YtClientKind {
    VisionOs,
    AndroidVr,
    Ios,
    Android,
    AndroidTestsuite,
    AndroidMusic,
    IosMusic,
    TvHtml5,
    TvHtml5SimplyEmbeddedPlayer,
    AndroidCreator,
    IosCreator,
    WebRemix,
    WebEmbeddedPlayer,
    Mweb,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayerClient {
    pub kind: YtClientKind,
    pub name: &'static str,
    pub client_id: &'static str,
    pub version: &'static str,
    pub api_key: &'static str,
    pub user_agent: &'static str,
    pub os_name: Option<&'static str>,
    pub os_version: Option<&'static str>,
    pub device_make: Option<&'static str>,
    pub device_model: Option<&'static str>,
    pub android_sdk_version: Option<&'static str>,
}

impl PlayerClient {
    pub fn key(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }

    pub fn origin(&self) -> &'static str {
        if self.name == "WEB_REMIX" {
            MUSIC_ORIGIN
        } else {
            YOUTUBE_ORIGIN
        }
    }

    pub fn referer(&self) -> String {
        match self.name {
            "WEB_REMIX" => format!("{MUSIC_ORIGIN}/"),
            "TVHTML5" | "TVHTML5_SIMPLY_EMBEDDED_PLAYER" => format!("{YOUTUBE_ORIGIN}/tv"),
            "WEB_EMBEDDED_PLAYER" => format!("{YOUTUBE_ORIGIN}/embed"),
            _ => format!("{YOUTUBE_ORIGIN}/"),
        }
    }

    pub fn requires_cipher(&self) -> bool {
        matches!(
            self.name,
            "WEB_REMIX" | "WEB_EMBEDDED_PLAYER" | "MWEB"
        )
    }

    pub fn stream_request_headers(&self) -> HashMap<String, String> {
        let mut h = HashMap::new();
        h.insert("User-Agent".to_string(), self.user_agent.to_string());
        h.insert(
            "X-YouTube-Client-Name".to_string(),
            self.client_id.to_string(),
        );
        h.insert(
            "X-YouTube-Client-Version".to_string(),
            self.version.to_string(),
        );
        if matches!(
            self.name,
            "WEB_REMIX"
                | "TVHTML5"
                | "TVHTML5_SIMPLY_EMBEDDED_PLAYER"
                | "WEB_EMBEDDED_PLAYER"
                | "MWEB"
        ) {
            h.insert("Origin".to_string(), self.origin().to_string());
            h.insert("Referer".to_string(), self.referer());
        }
        h
    }
}

pub fn client_id_for_name(name: &str) -> &str {
    match name {
        "WEB_REMIX" => "67",
        "IOS" => "5",
        "IOS_MUSIC" => "26",
        "IOS_CREATOR" => "15",
        "ANDROID" => "3",
        "ANDROID_MUSIC" => "21",
        "ANDROID_VR" => "28",
        "ANDROID_TESTSUITE" => "30",
        "ANDROID_CREATOR" => "14",
        "TVHTML5" => "7",
        "TVHTML5_SIMPLY_EMBEDDED_PLAYER" => "85",
        "VISIONOS" => "101",
        "WEB_EMBEDDED_PLAYER" => "56",
        "MWEB" => "62",
        _ => name,
    }
}

pub const PLAYER_CLIENTS: &[PlayerClient] = &[
    PlayerClient {
        kind: YtClientKind::VisionOs,
        name: "VISIONOS",
        client_id: "101",
        version: "0.1",
        api_key: "AIzaSyB-63vPrdThhKuerbB2N_l7Kwwcxj6yUAc",
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
        os_name: Some("visionOS"),
        os_version: Some("1.3.21O771"),
        device_make: Some("Apple"),
        device_model: Some("RealityDevice14,1"),
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::AndroidVr,
        name: "ANDROID_VR",
        client_id: "28",
        version: "1.65.10",
        api_key: "AIzaSyD-p045F_WzU-vA_YgX20SCx4KAo",
        user_agent: "com.google.android.apps.youtube.vr.oculus/1.65.10 (Linux; U; Android 12L; eureka-user Build/SQ3A.220605.009.A1) gzip",
        os_name: Some("Android"),
        os_version: Some("12L"),
        device_make: Some("Oculus"),
        device_model: Some("Quest 3"),
        android_sdk_version: Some("32"),
    },
    PlayerClient {
        kind: YtClientKind::Ios,
        name: "IOS",
        client_id: "5",
        version: "21.26.4",
        api_key: "AIzaSyB-63vPrdThhKuerbB2N_l7Kwwcxj6yUAc",
        user_agent: "com.google.ios.youtube/21.26.4 (iPhone16,2; U; CPU iOS 18_3_2;)",
        os_name: Some("iPhone"),
        os_version: Some("18.3.2.22D82"),
        device_make: Some("Apple"),
        device_model: Some("iPhone16,2"),
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::Android,
        name: "ANDROID",
        client_id: "3",
        version: "21.26.364",
        api_key: "AIzaSyA8eiZmM1FaDVjRy-df2KTyQ_vz_yYM39w",
        user_agent: "com.google.android.youtube/21.26.364 (Linux; U; Android 11) gzip",
        os_name: Some("Android"),
        os_version: Some("11"),
        device_make: None,
        device_model: None,
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::AndroidTestsuite,
        name: "ANDROID_TESTSUITE",
        client_id: "30",
        version: "1.9",
        api_key: "AIzaSyD-p045F_WzU-vA_YgX20SCx4KAo",
        user_agent: "com.google.android.youtube/1.9 (Linux; U; Android 12) gzip",
        os_name: Some("Android"),
        os_version: Some("12"),
        device_make: None,
        device_model: None,
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::AndroidMusic,
        name: "ANDROID_MUSIC",
        client_id: "21",
        version: "7.27.52",
        api_key: "AIzaSyA8eiZmM1FaDVjRy-df2KTyQ_vz_yYM39w",
        user_agent: "com.google.android.apps.youtube.music/7.27.52 (Linux; U; Android 14; en_US; Pixel 8; Build/UD1A.230803.041) gzip",
        os_name: Some("Android"),
        os_version: Some("14"),
        device_make: Some("Google"),
        device_model: Some("Pixel 8"),
        android_sdk_version: Some("34"),
    },
    PlayerClient {
        kind: YtClientKind::IosMusic,
        name: "IOS_MUSIC",
        client_id: "26",
        version: "7.27.0",
        api_key: "AIzaSyB-63vPrdThhKuerbB2N_l7Kwwcxj6yUAc",
        user_agent: "com.google.ios.youtubemusic/7.27.0 (iPhone16,2; U; CPU iOS 17_5_1 like Mac OS X;)",
        os_name: Some("iOS"),
        os_version: Some("17.5.1.21F90"),
        device_make: Some("Apple"),
        device_model: Some("iPhone16,2"),
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::TvHtml5,
        name: "TVHTML5",
        client_id: "7",
        version: "7.20260308.08.00",
        api_key: "AIzaSyAO_FJ2SlqAz8GlBg1fA54p0wDE7Xk80mU",
        user_agent: "Mozilla/5.0 (SMART-TV; Linux; Tizen 6.0) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/4.0 Chrome/76.0.3809.146 TV Safari/537.36",
        os_name: None,
        os_version: None,
        device_make: None,
        device_model: None,
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::TvHtml5SimplyEmbeddedPlayer,
        name: "TVHTML5_SIMPLY_EMBEDDED_PLAYER",
        client_id: "85",
        version: "2.0",
        api_key: "AIzaSyAO_FJ2SlqAz8GlBg1fA54p0wDE7Xk80mU",
        user_agent: "Mozilla/5.0 (SMART-TV; Linux; Tizen 6.0) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/4.0 Chrome/76.0.3809.146 TV Safari/537.36",
        os_name: None,
        os_version: None,
        device_make: None,
        device_model: None,
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::AndroidCreator,
        name: "ANDROID_CREATOR",
        client_id: "14",
        version: "24.32.100",
        api_key: "AIzaSyA8eiZmM1FaDVjRy-df2KTyQ_vz_yYM39w",
        user_agent: "com.google.android.apps.youtube.creator/24.32.100 (Linux; U; Android 13; en_US) gzip",
        os_name: Some("Android"),
        os_version: Some("13"),
        device_make: None,
        device_model: None,
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::IosCreator,
        name: "IOS_CREATOR",
        client_id: "15",
        version: "24.32.100",
        api_key: "AIzaSyB-63vPrdThhKuerbB2N_l7Kwwcxj6yUAc",
        user_agent: "com.google.ios.creator/24.32.100 (iPhone16,2; U; CPU iOS 17_5_1 like Mac OS X;)",
        os_name: Some("iOS"),
        os_version: Some("17.5.1.21F90"),
        device_make: None,
        device_model: None,
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::WebRemix,
        name: "WEB_REMIX",
        client_id: "67",
        version: FALLBACK_WEB_VERSION,
        api_key: FALLBACK_WEB_KEY,
        user_agent: WEB_USER_AGENT,
        os_name: None,
        os_version: None,
        device_make: None,
        device_model: None,
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::WebEmbeddedPlayer,
        name: "WEB_EMBEDDED_PLAYER",
        client_id: "56",
        version: FALLBACK_WEB_VERSION,
        api_key: FALLBACK_WEB_KEY,
        user_agent: WEB_USER_AGENT,
        os_name: None,
        os_version: None,
        device_make: None,
        device_model: None,
        android_sdk_version: None,
    },
    PlayerClient {
        kind: YtClientKind::Mweb,
        name: "MWEB",
        client_id: "62",
        version: "2.20260707.01.00",
        api_key: FALLBACK_WEB_KEY,
        user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1",
        os_name: None,
        os_version: None,
        device_make: None,
        device_model: None,
        android_sdk_version: None,
    },
];

// Local HTTP Loopback Stream Proxy

#[derive(Clone)]
struct LoopbackProxyState {
    client: Client,
    routes: Arc<RwLock<HashMap<String, ResolvedStream>>>,
}

/// Ephemeral loopback HTTP proxy allowing local audio engines (`libmpv`) to
/// stream YouTube URLs with custom `User-Agent`, `Referer`, `Cookie`, and
/// `Range` forwarding over `127.0.0.1`.
pub struct LocalLoopbackStreamServer {
    port: u16,
    routes: Arc<RwLock<HashMap<String, ResolvedStream>>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl LocalLoopbackStreamServer {
    /// Starts an HTTP proxy bound to `127.0.0.1:0`.
    pub async fn start(client: Client) -> Result<Self, InnerTubeError> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| InnerTubeError::Transport(e.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|e| InnerTubeError::Transport(e.to_string()))?
            .port();
        let routes = Arc::new(RwLock::new(HashMap::new()));
        let state = LoopbackProxyState {
            client,
            routes: Arc::clone(&routes),
        };
        let app = Router::new()
            .route("/stream/{token}", get(proxy_stream_handler))
            .with_state(state);

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = rx.await;
            });
            let _ = server.await;
        });

        Ok(Self {
            port,
            routes,
            shutdown: Some(tx),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Registers `stream` under `token` and returns its local loopback URL.
    pub fn register(&self, token: &str, stream: ResolvedStream) -> String {
        if let Ok(mut guard) = self.routes.write() {
            if guard.len() > 64 {
                guard.clear();
            }
            guard.insert(token.to_string(), stream);
        }
        format!("http://127.0.0.1:{}/stream/{token}", self.port)
    }

    pub fn unregister(&self, token: &str) {
        if let Ok(mut guard) = self.routes.write() {
            guard.remove(token);
        }
    }
}

impl Drop for LocalLoopbackStreamServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

async fn proxy_stream_handler(
    AxumState(state): AxumState<LoopbackProxyState>,
    AxumPath(token): AxumPath<String>,
    req_headers: HeaderMap,
) -> AxumResponse {
    let stream = state
        .routes
        .read()
        .ok()
        .and_then(|g| g.get(&token).cloned());
    let Some(stream) = stream else {
        return (StatusCode::NOT_FOUND, "stream token not found").into_response();
    };

    let mut req = state.client.get(&stream.url);
    for (k, v) in &stream.request_headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if let Some(range) = req_headers.get("range").and_then(|v| v.to_str().ok()) {
        req = req.header("Range", range);
    }

    let Ok(upstream) = req.send().await else {
        return (StatusCode::BAD_GATEWAY, "upstream request failed").into_response();
    };

    let status = StatusCode::from_u16(upstream.status().as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);
    let mut out_headers = HeaderMap::new();
    for name in [
        "content-type",
        "content-length",
        "content-range",
        "accept-ranges",
    ] {
        if let Some(val) = upstream.headers().get(name) {
            if let (Ok(hn), Ok(hv)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_bytes(val.as_bytes()),
            ) {
                out_headers.insert(hn, hv);
            }
        }
    }

    let body = Body::from_stream(upstream.bytes_stream());
    (status, out_headers, body).into_response()
}

// Internal Cache & State Types

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct CachedStream {
    stream: ResolvedStream,
    client_profile: String,
    itag: i64,
    adaptive: bool,
    auth_scope: String,
    cached_at_ms: i64,
}

#[derive(Debug, Clone)]
struct StreamCandidate {
    stream: ResolvedStream,
    adaptive: bool,
    bitrate: i64,
    itag: i64,
    is_audio_only: bool,
}

#[derive(Debug, Clone)]
struct ResolvedRef {
    client_profile: String,
    auth_scope: String,
}

#[derive(Debug, Clone)]
struct AccountCacheEntry {
    account: YtAccount,
    fetched_at_ms: i64,
}

#[derive(Debug, Clone)]
struct WebConfigState {
    api_key: String,
    client_version: String,
    visitor_data: Option<String>,
    initialized: bool,
}

impl Default for WebConfigState {
    fn default() -> Self {
        Self {
            api_key: FALLBACK_WEB_KEY.to_string(),
            client_version: FALLBACK_WEB_VERSION.to_string(),
            visitor_data: None,
            initialized: false,
        }
    }
}

#[derive(Debug, Clone)]
struct OrderedMap<V> {
    map: HashMap<String, V>,
    order: VecDeque<String>,
}

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }
}

impl<V> OrderedMap<V> {
    fn get(&self, key: &str) -> Option<&V> {
        self.map.get(key)
    }

    fn insert_lru(&mut self, key: String, value: V, max_len: usize) {
        if self.map.contains_key(&key) {
            self.order.retain(|k| k != &key);
        }
        self.order.push_back(key.clone());
        self.map.insert(key, value);
        self.trim(max_len);
    }

    fn remove(&mut self, key: &str) -> Option<V> {
        let removed = self.map.remove(key);
        if removed.is_some() {
            self.order.retain(|k| k != key);
        }
        removed
    }

    fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(&str, &V) -> bool,
    {
        self.map.retain(|k, v| keep(k, v));
        self.order.retain(|k| self.map.contains_key(k));
    }

    fn trim(&mut self, max_len: usize) {
        while self.map.len() > max_len {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            } else {
                break;
            }
        }
    }
}

type CookieRotationCallback = Arc<dyn Fn(&str, &str) + Send + Sync>;
type AuthStaleCallback = Arc<dyn Fn(bool) + Send + Sync>;

// InnerTubeMusicApi

pub struct InnerTubeMusicApi {
    client: Client,
    secure: Arc<SecureStore>,
    decipher: SignatureDecipher,
    po_tokens: Option<Arc<PoTokenEngine>>,
    pub auth_store: Arc<YtAuthStore>,
    disk: RwLock<Option<Arc<AppDatabase>>>,
    disk_loaded: AtomicBool,
    po_token_enabled: AtomicBool,
    web_config: RwLock<WebConfigState>,
    config_lock: AsyncMutex<()>,
    stream_cache: RwLock<HashMap<String, CachedStream>>,
    inflight: AsyncMutex<HashMap<String, watch::Receiver<Option<Option<ResolvedStream>>>>>,
    failed_clients_until: RwLock<OrderedMap<i64>>,
    match_cache: RwLock<OrderedMap<YouTubeMusicTrack>>,
    last_resolved: RwLock<OrderedMap<ResolvedRef>>,
    last_successful_client_name: RwLock<Option<String>>,
    connection: RwLock<YtConnection>,
    persisted_cookies: RwLock<String>,
    rotation_lock: AsyncMutex<()>,
    on_cookies_rotated: RwLock<Option<CookieRotationCallback>>,
    auth_stale: AtomicBool,
    auth_retries: AtomicU32,
    on_auth_stale_changed: RwLock<Option<AuthStaleCallback>>,
    hl: RwLock<String>,
    account_caches: RwLock<HashMap<String, AccountCacheEntry>>,
    podcast_episode_filter: RwLock<Option<String>>,
    loopback_server: AsyncMutex<Option<Arc<LocalLoopbackStreamServer>>>,
}

impl InnerTubeMusicApi {
    pub fn new(
        client: Client,
        secure: Arc<SecureStore>,
        po_tokens: Option<Arc<PoTokenEngine>>,
    ) -> Self {
        let auth_store = po_tokens
            .as_ref()
            .and_then(|p| p.auth_store().cloned())
            .unwrap_or_else(|| Arc::new(YtAuthStore::new(Arc::clone(&secure))));
        let _ = auth_store.load();
        let decipher = SignatureDecipher::new(client.clone(), None);
        Self {
            client,
            secure,
            decipher,
            po_tokens,
            auth_store,
            disk: RwLock::new(None),
            disk_loaded: AtomicBool::new(false),
            po_token_enabled: AtomicBool::new(true),
            web_config: RwLock::new(WebConfigState::default()),
            config_lock: AsyncMutex::new(()),
            stream_cache: RwLock::new(HashMap::new()),
            inflight: AsyncMutex::new(HashMap::new()),
            failed_clients_until: RwLock::new(OrderedMap::default()),
            match_cache: RwLock::new(OrderedMap::default()),
            last_resolved: RwLock::new(OrderedMap::default()),
            last_successful_client_name: RwLock::new(None),
            connection: RwLock::new(YtConnection::default()),
            persisted_cookies: RwLock::new(String::new()),
            rotation_lock: AsyncMutex::new(()),
            on_cookies_rotated: RwLock::new(None),
            auth_stale: AtomicBool::new(false),
            auth_retries: AtomicU32::new(0),
            on_auth_stale_changed: RwLock::new(None),
            hl: RwLock::new("en".to_string()),
            account_caches: RwLock::new(HashMap::new()),
            podcast_episode_filter: RwLock::new(None),
            loopback_server: AsyncMutex::new(None),
        }
    }

    pub fn with_defaults() -> Result<Self, InnerTubeError> {
        let client = HttpClientFactory::create()
            .map_err(|e| InnerTubeError::Transport(e.to_string()))?;
        let secure = Arc::new(SecureStore::new());
        Ok(Self::new(client, secure, None))
    }

    pub fn set_on_cookies_rotated<F>(&self, cb: Option<F>)
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.on_cookies_rotated.write() {
            *guard = cb.map(|f| Arc::new(f) as CookieRotationCallback);
        }
    }

    pub fn set_on_auth_stale_changed<F>(&self, cb: Option<F>)
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.on_auth_stale_changed.write() {
            *guard = cb.map(|f| Arc::new(f) as AuthStaleCallback);
        }
    }

    pub fn po_token_enabled(&self) -> bool {
        self.po_token_enabled.load(Ordering::Relaxed)
    }

    pub fn set_po_token_enabled(&self, enabled: bool) {
        self.po_token_enabled.store(enabled, Ordering::Relaxed);
        if let Some(engine) = &self.po_tokens {
            engine.set_enabled(enabled);
        }
    }

    pub fn pre_warm_botguard(&self) {
        if !self.po_token_enabled() {
            return;
        }
        if let Some(engine) = &self.po_tokens {
            engine.pre_warm();
        }
    }

    pub fn connection(&self) -> YtConnection {
        self.connection
            .read()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub fn set_connection(&self, c: YtConnection) {
        if let Ok(mut g) = self.connection.write() {
            *g = c;
        }
    }

    pub fn hl(&self) -> String {
        self.hl
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "en".to_string())
    }

    pub fn set_hl(&self, val: &str) {
        if val.is_empty() {
            return;
        }
        if let Ok(mut g) = self.hl.write() {
            if *g != val {
                *g = val.to_string();
            }
        }
    }

    pub fn auth_stale(&self) -> bool {
        self.auth_stale.load(Ordering::Relaxed)
    }

    fn set_auth_stale(&self, stale: bool) {
        let prev = self.auth_stale.swap(stale, Ordering::SeqCst);
        if prev == stale {
            return;
        }
        if let Ok(guard) = self.on_auth_stale_changed.read() {
            if let Some(cb) = guard.as_ref() {
                cb(stale);
            }
        }
    }

    /// Lazily starts a local `127.0.0.1` loopback HTTP proxy for wrapping a
    /// resolved stream URL into a header-free local URL for audio decoders.
    pub async fn ensure_loopback_proxy(&self) -> Option<Arc<LocalLoopbackStreamServer>> {
        let mut guard = self.loopback_server.lock().await;
        if let Some(existing) = guard.as_ref() {
            return Some(Arc::clone(existing));
        }
        let started = LocalLoopbackStreamServer::start(self.client.clone()).await.ok()?;
        let arc = Arc::new(started);
        *guard = Some(Arc::clone(&arc));
        Some(arc)
    }

    // Disk Cache Attachment

    pub fn attach_disk_cache(self: &Arc<Self>, db: Arc<AppDatabase>) {
        if let Ok(mut guard) = self.disk.write() {
            *guard = Some(Arc::clone(&db));
        }

        if let Some(raw_cfg) = db.kv_get("yt_web_config") {
            if let Ok(decoded) = serde_json::from_str::<Value>(&raw_cfg) {
                if let Some(obj) = decoded.as_object() {
                    let key = val_str(obj.get("apiKey"));
                    let version = val_str(obj.get("clientVersion"));
                    let visitor = val_str(obj.get("visitorData"));
                    let saved_at = obj.get("savedAtMs").and_then(Value::as_i64).unwrap_or(0);
                    let mut stale_cfg = false;
                    if let Ok(mut cfg) = self.web_config.write() {
                        if !key.is_empty() {
                            cfg.api_key = key;
                        }
                        if !version.is_empty() {
                            cfg.client_version = version;
                        }
                        if !visitor.is_empty() {
                            cfg.visitor_data = Some(visitor);
                        }
                        if saved_at > 0 {
                            cfg.initialized = true;
                            let age_ms = now_unix_ms().saturating_sub(saved_at);
                            stale_cfg = age_ms > 24 * 60 * 60 * 1000;
                        }
                    }
                    if stale_cfg {
                        let me = Arc::clone(self);
                        tokio::spawn(async move {
                            me.fetch_web_config().await;
                        });
                    }
                }
            }
        }

        if self.disk_loaded.swap(true, Ordering::SeqCst) {
            return;
        }

        let now_ms = now_unix_ms();
        db.prune_expired_streams(now_ms + URL_EXPIRY_MARGIN_MS);
        let rows = db.load_stream_entries(256);
        if let Ok(mut cache) = self.stream_cache.write() {
            for r in rows {
                if r.video_id.is_empty() || r.url.is_empty() {
                    continue;
                }
                let expires_at = if r.expires_at_ms > 0 {
                    DateTime::from_timestamp_millis(r.expires_at_ms)
                } else {
                    None
                };
                if let Some(exp) = expires_at {
                    if exp.timestamp_millis() - now_ms <= URL_EXPIRY_MARGIN_MS {
                        continue;
                    }
                }
                let headers: HashMap<String, String> =
                    serde_json::from_str(&r.headers_json).unwrap_or_default();
                let auth_scope = if r.auth_scope.is_empty() {
                    "anonymous".to_string()
                } else {
                    r.auth_scope.clone()
                };
                let mime = if r.mime.is_empty() {
                    "audio/webm".to_string()
                } else {
                    r.mime.clone()
                };
                let codec = if r.codec.is_empty() {
                    "OPUS".to_string()
                } else {
                    r.codec.clone()
                };
                let cache_key = format!(
                    "youtube:{}:{}:{}:{}:{}",
                    r.video_id, r.client_profile, r.itag, auth_scope, r.expires_at_ms
                );
                let stream = ResolvedStream {
                    url: r.url,
                    mime_type: mime.clone(),
                    bitrate_kbps: r.bitrate_kbps as i32,
                    audio_codec: codec,
                    cache_key,
                    request_headers: headers,
                    expires_at,
                    watchtime_url: r.watchtime_url,
                    ..Default::default()
                };
                let adaptive =
                    !(mime.to_lowercase().contains("mp4") && r.itag >= 0 && r.itag < 300);
                let entry_key = format!(
                    "{}|{}|{}|{}|{}",
                    r.video_id, r.client_profile, r.itag, auth_scope, r.expires_at_ms
                );
                cache.insert(
                    entry_key,
                    CachedStream {
                        stream,
                        client_profile: r.client_profile,
                        itag: r.itag,
                        adaptive,
                        auth_scope,
                        cached_at_ms: if r.cached_at_ms > 0 {
                            r.cached_at_ms
                        } else {
                            now_ms
                        },
                    },
                );
            }
        }
    }

    fn current_disk(&self) -> Option<Arc<AppDatabase>> {
        self.disk.read().ok().and_then(|g| g.clone())
    }

    // Cookie Normalization & Set-Cookie Rotation

    /// Merges `Set-Cookie` header lines into a `Cookie` header `jar`.
    /// Only updates keys already present in `jar`, scoped to youtube.com/google.com,
    /// ignoring deletions (`Max-Age <= 0`, expired, empty/`EXPIRED`/`deleted`).
    pub fn merge_rotated_cookies(
        jar: &str,
        set_cookies: &[String],
        host: &str,
    ) -> Option<String> {
        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut index_by_name: HashMap<String, usize> = HashMap::new();
        for part in jar.split(';') {
            let t = part.trim();
            let Some(eq) = t.find('=') else {
                continue;
            };
            if eq == 0 {
                continue;
            }
            let name = t[..eq].trim().to_string();
            let val = t[eq + 1..].trim().to_string();
            if let Some(&idx) = index_by_name.get(&name) {
                pairs[idx].1 = val;
            } else {
                index_by_name.insert(name.clone(), pairs.len());
                pairs.push((name, val));
            }
        }
        if pairs.is_empty() {
            return None;
        }

        let now = Utc::now();
        let mut changed = false;
        for line in set_cookies {
            let mut segments = line.split(';');
            let Some(first) = segments.next() else {
                continue;
            };
            let first = first.trim();
            let Some(eq) = first.find('=') else {
                continue;
            };
            if eq == 0 {
                continue;
            }
            let name = first[..eq].trim();
            let value = first[eq + 1..].trim();
            let Some(&idx) = index_by_name.get(name) else {
                continue;
            };
            if value.is_empty() || value == "EXPIRED" || value == "deleted" {
                continue;
            }
            let mut domain = host.to_lowercase();
            let mut deleted = false;
            for attr in segments {
                let a = attr.trim();
                let (key, val) = match a.find('=') {
                    Some(aeq) => (a[..aeq].trim().to_lowercase(), a[aeq + 1..].trim()),
                    None => (a.to_lowercase(), ""),
                };
                if key == "domain" && !val.is_empty() {
                    domain = val.to_lowercase();
                    if let Some(stripped) = domain.strip_prefix('.') {
                        domain = stripped.to_string();
                    }
                } else if key == "max-age" {
                    if let Ok(age) = val.parse::<i64>() {
                        if age <= 0 {
                            deleted = true;
                        }
                    }
                } else if key == "expires" {
                    if let Some(exp) = parse_http_date(val) {
                        if exp < now {
                            deleted = true;
                        }
                    }
                }
            }
            if deleted {
                continue;
            }
            let scoped = domain == "youtube.com"
                || domain.ends_with(".youtube.com")
                || domain == "google.com"
                || domain.ends_with(".google.com");
            if !scoped {
                continue;
            }
            if pairs[idx].1 != value {
                pairs[idx].1 = value.to_string();
                changed = true;
            }
        }

        if !changed {
            return None;
        }
        Some(
            pairs
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    async fn absorb_rotated_cookies(
        &self,
        sent_cookie: Option<&str>,
        set_cookies: &[String],
        host: &str,
    ) {
        if set_cookies.is_empty() {
            return;
        }
        let current = self.connection();
        if !current.connected || current.cookies.is_empty() {
            return;
        }
        let persisted = self
            .persisted_cookies
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        if current.cookies != persisted {
            return;
        }
        if sent_cookie != Some(current.cookies.as_str()) {
            return;
        }
        let Some(next) = Self::merge_rotated_cookies(&current.cookies, set_cookies, host) else {
            return;
        };
        if next == current.cookies {
            return;
        }

        let previous = current.cookies.clone();
        {
            if let Ok(mut conn_g) = self.connection.write() {
                conn_g.cookies = next.clone();
            }
            if let Ok(mut pers_g) = self.persisted_cookies.write() {
                *pers_g = next.clone();
            }
        }

        let _guard = self.rotation_lock.lock().await;
        self.secure.write_yt_cookies(Some(&next));
        self.auth_store.update_cookies(next.clone());
        if let Ok(cb_guard) = self.on_cookies_rotated.read() {
            if let Some(cb) = cb_guard.as_ref() {
                cb(&previous, &next);
            }
        }
    }

    pub async fn flush_pending_writes(&self) {
        let _guard = self.rotation_lock.lock().await;
    }

    /// Normalizes raw cookie exports (header string, Netscape cookies.txt TSV,
    /// or JSON object/array) into a deterministic `Cookie` header string.
    pub fn normalize_cookies(raw: &str) -> Result<String, InnerTubeError> {
        let input = raw.trim();
        if input.is_empty() {
            return Err(InnerTubeError::InvalidFormat("Empty cookies".to_string()));
        }

        let mut pairs: BTreeMap<String, String> = BTreeMap::new();

        if input.starts_with('[') || input.starts_with('{') {
            if let Ok(decoded) = serde_json::from_str::<Value>(input) {
                match decoded {
                    Value::Array(list) => {
                        for e in list {
                            if let Some(obj) = e.as_object() {
                                let n = val_str(obj.get("name"));
                                let v = val_str(obj.get("value"));
                                add_cookie_pair(&mut pairs, &n, &v);
                            }
                        }
                    }
                    Value::Object(obj) => {
                        if let Some(Value::Array(list)) = obj.get("cookies") {
                            for e in list {
                                if let Some(item) = e.as_object() {
                                    let n = val_str(item.get("name"));
                                    let v = val_str(item.get("value"));
                                    add_cookie_pair(&mut pairs, &n, &v);
                                }
                            }
                        } else {
                            for (k, v) in obj {
                                if k == "cookies" {
                                    continue;
                                }
                                add_cookie_pair(&mut pairs, &k, &scalar_to_string(&v));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if pairs.is_empty() {
            for raw_line in input.lines() {
                let mut line = raw_line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("#HttpOnly") {
                    line = rest.trim().to_string();
                } else if line.starts_with('#') {
                    continue;
                }
                if line.to_lowercase().starts_with("cookie:") {
                    line = line[7..].trim().to_string();
                }
                if line.contains('\t') {
                    let cols: Vec<&str> = line.split('\t').collect();
                    if cols.len() >= 7 {
                        add_cookie_pair(&mut pairs, cols[5], cols[6]);
                        continue;
                    }
                    line = line.replace('\t', "; ");
                }
                for part in line.split(';') {
                    let chunk = part.trim();
                    if chunk.is_empty() || chunk.starts_with('#') {
                        continue;
                    }
                    if let Some(idx) = chunk.find('=') {
                        if idx > 0 {
                            add_cookie_pair(&mut pairs, &chunk[..idx], &chunk[idx + 1..]);
                        }
                    }
                }
            }
        }

        if pairs.is_empty() {
            return Err(InnerTubeError::InvalidFormat(
                "No cookies found — paste the Cookie header or cookies.txt".to_string(),
            ));
        }

        let has_sapisid = ["__Secure-3PAPISID", "SAPISID", "APISID"]
            .iter()
            .any(|k| pairs.contains_key(*k));
        if !has_sapisid {
            return Err(InnerTubeError::InvalidFormat(
                "Missing SAPISID — export cookies from music.youtube.com while signed in (need __Secure-3PAPISID/SAPISID)"
                    .to_string(),
            ));
        }

        Ok(pairs
            .into_iter()
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("; "))
    }

    // Session / Auth Management

    pub async fn load_persisted_connection(&self) {
        let Some(stored) = self.secure.read_yt_cookies() else {
            return;
        };
        if stored.is_empty() {
            return;
        }
        let cookies = Self::normalize_cookies(&stored).unwrap_or_else(|_| stored.trim().to_string());
        if cookies.is_empty() {
            return;
        }
        if let Ok(mut p) = self.persisted_cookies.write() {
            *p = cookies.clone();
        }
        let mut profile_email = String::new();
        let mut active_page_id = String::new();
        if let Some(raw_active) = self.secure.read_yt_active() {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw_active) {
                profile_email = val_str(map.get("email"));
                active_page_id = val_str(map.get("pageId"));
            }
        }
        self.set_connection(YtConnection {
            connected: true,
            cookies,
            connected_at_millis: now_unix_ms(),
            profile_email,
            active_page_id,
            ..Default::default()
        });
    }

    fn persist_active_pointer(&self) {
        let c = self.connection();
        let payload = json!({
            "email": c.profile_email,
            "pageId": c.active_page_id,
        });
        self.secure.write_yt_active(Some(&payload.to_string()));
    }

    pub fn update_enriched_connection(&self, conn: YtConnection) {
        self.set_connection(conn);
        self.persist_active_pointer();
    }

    pub async fn connect(&self, raw_cookies: &str) -> Result<(), InnerTubeError> {
        self.connect_as(raw_cookies, "", "").await
    }

    pub async fn connect_as(
        &self,
        cookies: &str,
        profile_email: &str,
        page_id: &str,
    ) -> Result<(), InnerTubeError> {
        self.clear_account_cache();
        let normalized = Self::normalize_cookies(cookies)?;
        let next = YtConnection {
            connected: true,
            cookies: normalized.clone(),
            connected_at_millis: now_unix_ms(),
            profile_email: profile_email.to_string(),
            active_page_id: page_id.to_string(),
            ..Default::default()
        };
        let prev = self.connection();
        self.set_connection(next);
        if let Err(err) = self.verify_connection(false).await {
            self.set_connection(prev.clone());
            self.set_auth_stale(!prev.connected);
            return Err(err);
        }
        self.set_auth_stale(false);
        self.flush_pending_writes().await;
        self.secure.write_yt_cookies(Some(&normalized));
        if let Ok(mut p) = self.persisted_cookies.write() {
            *p = normalized.clone();
        }
        self.auth_store.update_cookies(normalized);
        self.persist_active_pointer();
        Ok(())
    }

    pub async fn verify_connection(&self, mark_stale: bool) -> Result<(), InnerTubeError> {
        if !self.connection().connected {
            return Err(InnerTubeError::InvalidState("Not connected".to_string()));
        }
        let root = self.browse_root(LIBRARY_PLAYLISTS_BROWSE_ID, true).await?;
        if Self::looks_signed_out(&root) {
            if mark_stale {
                self.set_auth_stale(true);
            }
            return Err(InnerTubeError::InvalidFormat(
                "YouTube rejected the cookies (signed out) — re-export from music.youtube.com while signed in"
                    .to_string(),
            ));
        }
        if mark_stale {
            self.set_auth_stale(false);
        }
        Ok(())
    }

    pub async fn check_auth(self: &Arc<Self>) -> bool {
        if !self.connection().connected {
            self.set_auth_stale(false);
            return false;
        }
        match self.verify_connection(true).await {
            Ok(()) => {
                self.auth_retries.store(0, Ordering::Relaxed);
                true
            }
            Err(InnerTubeError::InvalidFormat(_)) => {
                self.auth_retries.store(0, Ordering::Relaxed);
                false
            }
            Err(InnerTubeError::Http(401)) => {
                self.auth_retries.store(0, Ordering::Relaxed);
                self.set_auth_stale(true);
                false
            }
            Err(_) => {
                self.schedule_auth_retry();
                !self.auth_stale()
            }
        }
    }

    fn schedule_auth_retry(self: &Arc<Self>) {
        let current = self.auth_retries.load(Ordering::Relaxed);
        if current >= 4 {
            return;
        }
        let next = self.auth_retries.fetch_add(1, Ordering::Relaxed) + 1;
        if next > 4 {
            return;
        }
        let me = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20 * u64::from(next))).await;
            if me.connection().connected {
                let _ = me.check_auth().await;
            }
        });
    }

    pub async fn sign_out(&self) {
        if let Ok(mut p) = self.persisted_cookies.write() {
            p.clear();
        }
        self.flush_pending_writes().await;
        self.secure.write_yt_cookies(None);
        self.secure.write_yt_profiles(None);
        self.secure.write_yt_active(None);
        self.set_connection(YtConnection::default());
        self.clear_account_cache();
        self.set_auth_stale(false);
    }

    pub fn logged_in_flag(root: &Value) -> Option<bool> {
        let services = root
            .get("responseContext")?
            .get("serviceTrackingParams")?
            .as_array()?;
        for service in services {
            let Some(params) = service.get("params").and_then(Value::as_array) else {
                continue;
            };
            for param in params {
                if param.get("key").and_then(Value::as_str) == Some("logged_in") {
                    match param.get("value").and_then(Value::as_str) {
                        Some("1") => return Some(true),
                        Some("0") => return Some(false),
                        _ => {}
                    }
                }
            }
        }
        None
    }

    pub fn looks_signed_out(root: &Value) -> bool {
        if let Some(flag) = Self::logged_in_flag(root) {
            return !flag;
        }
        if root.get("contents").map_or(true, Value::is_null) {
            return true;
        }
        let text = root.to_string();
        text.contains("SIGN_IN") && !text.contains("music_liked_playlists")
    }

    fn observe_authenticated_browse(&self, root: &Value) {
        self.set_auth_stale(Self::looks_signed_out(root));
    }

    fn cookie_value(&self, name: &str) -> Option<String> {
        let conn = self.connection();
        if conn.cookies.is_empty() {
            return None;
        }
        for part in conn.cookies.split(';') {
            let Some(idx) = part.find('=') else {
                continue;
            };
            if idx > 0 && part[..idx].trim() == name {
                let val = part[idx + 1..].trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
        None
    }

    fn sapisid(&self) -> Option<String> {
        self.cookie_value("__Secure-3PAPISID")
            .or_else(|| self.cookie_value("SAPISID"))
            .or_else(|| self.cookie_value("APISID"))
    }

    fn cookie_header_value(&self) -> Option<String> {
        let c = self.connection();
        if c.cookies.is_empty() {
            None
        } else {
            Some(c.cookies)
        }
    }

    fn authorization_header_value(&self, origin: Option<&str>) -> Option<String> {
        let orig = origin.unwrap_or(MUSIC_ORIGIN);
        let sapisid = self.cookie_value("SAPISID");
        let sapisid_1p = self.cookie_value("__Secure-1PAPISID");
        let sapisid_3p = self.cookie_value("__Secure-3PAPISID");
        if let Some(combined) = AuthCrypto::sapisid_authorization_header(
            sapisid.as_deref(),
            sapisid_1p.as_deref(),
            sapisid_3p.as_deref(),
            orig,
        ) {
            return Some(combined);
        }
        self.cookie_value("APISID")
            .map(|ap| AuthCrypto::sapisid_hash(&ap, orig))
    }

    fn playback_auth_scope(&self) -> String {
        let conn = self.connection();
        if !conn.connected {
            return "anonymous".to_string();
        }
        let sapisid = self.sapisid().unwrap_or_else(|| conn.cookies.clone());
        let mut hasher = Sha256::new();
        hasher.update(format!("{sapisid}|{}", conn.active_page_id).as_bytes());
        let bytes = hasher.finalize();
        let digest = hex::encode(&bytes[..8]);
        format!("account:{}:{digest}", conn.connected_at_millis)
    }

    // Web Config Bootstrap

    async fn ensure_config(&self) {
        if self
            .web_config
            .read()
            .map(|g| g.initialized)
            .unwrap_or(false)
        {
            return;
        }
        let _lock = self.config_lock.lock().await;
        if self
            .web_config
            .read()
            .map(|g| g.initialized)
            .unwrap_or(false)
        {
            return;
        }
        self.fetch_web_config().await;
    }

    async fn fetch_web_config(&self) {
        let res = tokio::time::timeout(
            Duration::from_secs(4),
            self.client
                .get(format!("{MUSIC_ORIGIN}/"))
                .header(USER_AGENT, WEB_USER_AGENT)
                .send(),
        )
        .await;

        let html = match res {
            Ok(Ok(r)) => r.text().await.unwrap_or_default(),
            _ => String::new(),
        };

        let key = find_config(&html, "INNERTUBE_API_KEY");
        let version = find_config(&html, "INNERTUBE_CONTEXT_CLIENT_VERSION");
        let visitor = find_config(&html, "VISITOR_DATA");

        let snapshot = if let Ok(mut cfg) = self.web_config.write() {
            if let Some(v) = visitor.filter(|s| !s.is_empty()) {
                cfg.visitor_data = Some(v);
            }
            if let Some(k) = key.filter(|s| !s.is_empty()) {
                cfg.api_key = k;
            }
            if let Some(ver) = version.filter(|s| !s.is_empty()) {
                cfg.client_version = ver;
            }
            cfg.initialized = true;
            Some((
                cfg.api_key.clone(),
                cfg.client_version.clone(),
                cfg.visitor_data.clone().unwrap_or_default(),
            ))
        } else {
            None
        };

        if let (Some(disk), Some((api_key, client_version, visitor_data))) =
            (self.current_disk(), snapshot)
        {
            if !visitor_data.is_empty() {
                self.auth_store.update_visitor_data(visitor_data.clone());
            }
            let payload = json!({
                "apiKey": api_key,
                "clientVersion": client_version,
                "visitorData": visitor_data,
                "savedAtMs": now_unix_ms(),
            });
            disk.kv_set("yt_web_config", &payload.to_string());
        }
    }

    fn clear_web_config(&self) {
        if let Ok(mut cfg) = self.web_config.write() {
            cfg.initialized = false;
        }
    }

    fn current_web_config(&self) -> (String, String, Option<String>) {
        self.web_config
            .read()
            .map(|g| {
                (
                    g.api_key.clone(),
                    g.client_version.clone(),
                    g.visitor_data.clone(),
                )
            })
            .unwrap_or_else(|_| {
                (
                    FALLBACK_WEB_KEY.to_string(),
                    FALLBACK_WEB_VERSION.to_string(),
                    None,
                )
            })
    }

    // Request Plumbing

    #[allow(clippy::too_many_arguments)]
    fn build_context(
        &self,
        name: &str,
        version: &str,
        visitor_data: Option<&str>,
        os_name: Option<&str>,
        os_version: Option<&str>,
        device_make: Option<&str>,
        device_model: Option<&str>,
        android_sdk_version: Option<&str>,
        po_token: Option<&str>,
    ) -> Value {
        let mut client_map = Map::new();
        client_map.insert("clientName".to_string(), json!(name));
        client_map.insert("clientVersion".to_string(), json!(version));
        client_map.insert("hl".to_string(), json!(self.hl()));
        client_map.insert("gl".to_string(), json!("US"));

        if let Some(v) = visitor_data.filter(|s| !s.is_empty()) {
            client_map.insert("visitorData".to_string(), json!(v));
        }
        if let Some(v) = os_name.filter(|s| !s.is_empty()) {
            client_map.insert("osName".to_string(), json!(v));
        }
        if let Some(v) = os_version.filter(|s| !s.is_empty()) {
            client_map.insert("osVersion".to_string(), json!(v));
        }
        if let Some(v) = device_make.filter(|s| !s.is_empty()) {
            client_map.insert("deviceMake".to_string(), json!(v));
        }
        if let Some(v) = device_model.filter(|s| !s.is_empty()) {
            client_map.insert("deviceModel".to_string(), json!(v));
        }
        if let Some(v) = android_sdk_version.filter(|s| !s.is_empty()) {
            client_map.insert("androidSdkVersion".to_string(), json!(v));
        }

        let mut root = Map::new();
        root.insert("client".to_string(), Value::Object(client_map));
        if let Some(pot) = po_token.filter(|s| !s.is_empty()) {
            root.insert(
                "serviceIntegrityDimensions".to_string(),
                json!({ "poToken": pot }),
            );
        }
        Value::Object(root)
    }

    fn web_context(&self, version: &str, visitor: Option<&str>) -> Value {
        self.build_context(
            "WEB_REMIX",
            version,
            visitor,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    async fn post_json(
        &self,
        url: &str,
        body: &Value,
        client_name: &str,
        client_version: &str,
        user_agent: &str,
        authenticated: bool,
        origin: Option<&str>,
        referer: Option<&str>,
        visitor_data: Option<&str>,
        max_attempts: usize,
        call_timeout: Option<Duration>,
        page_id: Option<&str>,
    ) -> Result<Value, InnerTubeError> {
        self.ensure_config().await;
        let orig = origin.unwrap_or(if client_name == "WEB_REMIX" {
            MUSIC_ORIGIN
        } else {
            YOUTUBE_ORIGIN
        });
        let ref_owned;
        let ref_val = match referer {
            Some(r) => r,
            None => {
                ref_owned = if client_name == "WEB_REMIX" {
                    format!("{MUSIC_ORIGIN}/")
                } else {
                    format!("{YOUTUBE_ORIGIN}/")
                };
                &ref_owned
            }
        };
        let conn = self.connection();
        let effective_page_id = page_id.unwrap_or(&conn.active_page_id);
        let sent_cookie = if authenticated && conn.connected {
            self.cookie_header_value()
        } else {
            None
        };
        let auth_hdr = if authenticated && conn.connected {
            self.authorization_header_value(Some(orig))
        } else {
            None
        };
        let host = Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default();

        let timeout = call_timeout.unwrap_or(Duration::from_secs(15));
        let attempts = max_attempts.max(1);
        let hl = self.hl();

        let mut last_err = InnerTubeError::Http(0);
        for attempt in 1..=attempts {
            let mut req = self
                .client
                .post(url)
                .header("Content-Type", "application/json")
                .header(USER_AGENT, user_agent)
                .header(ORIGIN, orig)
                .header("X-Origin", orig)
                .header(REFERER, ref_val)
                .header("X-Goog-Api-Format-Version", "1")
                .header("X-YouTube-Client-Name", client_id_for_name(client_name))
                .header("X-YouTube-Client-Version", client_version)
                .header("Accept-Language", format!("{hl}, *;q=0.5"))
                .json(body);

            if let Some(vis) = visitor_data.filter(|s| !s.is_empty()) {
                req = req.header("X-Goog-Visitor-Id", vis);
            }
            if authenticated && conn.connected {
                if let Some(c) = sent_cookie.as_deref() {
                    req = req.header(COOKIE, c);
                }
                if let Some(a) = auth_hdr.as_deref() {
                    req = req.header("Authorization", a);
                }
                if !effective_page_id.is_empty() {
                    req = req
                        .header("X-Goog-PageId", effective_page_id)
                        .header("X-Goog-AuthUser", "0");
                }
            }

            let resp = tokio::time::timeout(timeout, req.send()).await;
            match resp {
                Ok(Ok(res)) => {
                    let set_cookies: Vec<String> = res
                        .headers()
                        .get_all(reqwest::header::SET_COOKIE)
                        .iter()
                        .filter_map(|v| v.to_str().ok().map(str::to_string))
                        .collect();
                    self.absorb_rotated_cookies(sent_cookie.as_deref(), &set_cookies, &host)
                        .await;

                    let status = res.status().as_u16();
                    if status >= 200 && status < 300 {
                        let text = res.text().await.unwrap_or_default();
                        if text.is_empty() {
                            return Ok(json!({}));
                        }
                        let parsed = serde_json::from_str::<Value>(&text)
                            .unwrap_or_else(|_| json!({}));
                        return Ok(if parsed.is_object() {
                            parsed
                        } else {
                            json!({})
                        });
                    }

                    if matches!(status, 400 | 403 | 429) {
                        self.clear_web_config();
                    }
                    let transient = status == 408 || status == 429 || status >= 500;
                    if attempt >= attempts || !transient {
                        return Err(InnerTubeError::Http(status));
                    }
                    last_err = InnerTubeError::Http(status);
                }
                Ok(Err(e)) => {
                    let transient = e.is_timeout() || e.is_connect();
                    if attempt >= attempts || !transient {
                        return Err(InnerTubeError::Transport(e.to_string()));
                    }
                    last_err = InnerTubeError::Transport(e.to_string());
                }
                Err(_) => {
                    if attempt >= attempts {
                        return Err(InnerTubeError::Transport("request timed out".to_string()));
                    }
                    last_err = InnerTubeError::Transport("request timed out".to_string());
                }
            }

            let backoff_ms = 250u64 * (1u64 << (attempt - 1));
            let jitter_ms = rand::thread_rng().gen_range(0..=180u64);
            tokio::time::sleep(Duration::from_millis(backoff_ms + jitter_ms)).await;
        }

        Err(last_err)
    }

    // Account Surfaces (Liked / History / Playlists / Identity / Channels)

    pub async fn fetch_liked_songs(&self, limit: usize) -> Vec<YouTubeMusicTrack> {
        if !self.connection().connected {
            return Vec::new();
        }
        self.ensure_config().await;
        let Ok(root) = self.browse_root(LIKED_BROWSE_ID, true).await else {
            return Vec::new();
        };
        self.observe_authenticated_browse(&root);
        let (tracks, _) = self
            .collect_browse_song_pages(&root, Some(limit), true)
            .await;
        tracks
    }

    pub async fn like_video(&self, video_id: &str) -> bool {
        self.set_video_liked(video_id, true).await
    }

    pub async fn remove_video_like(&self, video_id: &str) -> bool {
        self.set_video_liked(video_id, false).await
    }

    async fn set_video_liked(&self, video_id: &str, liked: bool) -> bool {
        let clean = video_id.trim();
        if clean.is_empty() || !self.connection().connected {
            return false;
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let endpoint = if liked {
            LIKE_ENDPOINT
        } else {
            REMOVE_LIKE_ENDPOINT
        };
        let url = format!("{MUSIC_API}/{endpoint}?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.build_context("WEB_REMIX", &client_ver, visitor.as_deref(), None, None, None, None, None, None),
            "target": { "videoId": clean },
        });
        self.post_json(
            &url,
            &body,
            "WEB_REMIX",
            &client_ver,
            WEB_USER_AGENT,
            true,
            None,
            None,
            None,
            2,
            None,
            None,
        )
        .await
        .is_ok()
    }

    pub async fn fetch_yt_history(&self, limit: usize) -> Vec<YouTubeMusicTrack> {
        if !self.connection().connected {
            return Vec::new();
        }
        self.ensure_config().await;
        let Ok(root) = self.browse_root(HISTORY_BROWSE_ID, true).await else {
            return Vec::new();
        };
        self.observe_authenticated_browse(&root);
        let (tracks, _) = self
            .collect_browse_song_pages(&root, Some(limit), true)
            .await;
        tracks
    }

    pub async fn fetch_account_playlists(&self) -> Vec<YouTubePlaylistSummary> {
        self.fetch_account_playlists_or_null()
            .await
            .unwrap_or_default()
    }

    pub async fn fetch_account_playlists_or_null(&self) -> Option<Vec<YouTubePlaylistSummary>> {
        if !self.connection().connected {
            return None;
        }
        self.ensure_config().await;
        let root = self
            .browse_root(LIBRARY_PLAYLISTS_BROWSE_ID, true)
            .await
            .ok()?;
        self.observe_authenticated_browse(&root);
        let mut parsed = self.parse_playlist_renderers(&root);
        let mut seen_ids: HashSet<String> = parsed.iter().map(|p| p.id.clone()).collect();
        let mut seen_tokens: HashSet<String> = HashSet::new();
        let mut token = self.library_grid_token(&root);
        for _ in 0..20 {
            let Some(tok) = token.take() else {
                break;
            };
            if !seen_tokens.insert(tok.clone()) {
                break;
            }
            let Ok(next) = self.browse_continuation(&tok, true).await else {
                break;
            };
            for p in self.parse_playlist_renderers(&next) {
                if seen_ids.insert(p.id.clone()) {
                    parsed.push(p);
                }
            }
            token = self.library_grid_token(&next);
        }
        Some(
            parsed
                .into_iter()
                .filter(|p| {
                    !p.id.starts_with("UC")
                        && !p.id.starts_with("MPRE")
                        && !Self::is_liked_music_id(&p.id)
                })
                .collect(),
        )
    }

    fn library_grid_token(&self, page: &Value) -> Option<String> {
        let mut grids = Vec::new();
        collect_objects(page, "gridRenderer", &mut grids);
        collect_objects(page, "gridContinuation", &mut grids);
        for grid in grids {
            if let Some(conts) = grid.get("continuations") {
                if let Some(tok) = find_string(conts, "continuation") {
                    if !tok.is_empty() {
                        return Some(tok);
                    }
                }
            }
        }
        let mut items = Vec::new();
        collect_objects(page, "continuationItemRenderer", &mut items);
        for item in items {
            if let Some(tok) = find_string(&Value::Object(item), "token") {
                if !tok.is_empty() {
                    return Some(tok);
                }
            }
        }
        None
    }

    fn clear_account_cache(&self) {
        if let Ok(mut guard) = self.account_caches.write() {
            guard.clear();
        }
    }

    fn cached_account(&self, page_id: &str) -> Option<YtAccount> {
        let now_ms = now_unix_ms();
        if let Ok(guard) = self.account_caches.read() {
            if let Some(entry) = guard.get(page_id) {
                if now_ms.saturating_sub(entry.fetched_at_ms) <= 3_600_000 {
                    return Some(entry.account.clone());
                }
            }
        }
        if let Ok(mut guard) = self.account_caches.write() {
            guard.remove(page_id);
        }
        None
    }

    pub async fn fetch_account_info(&self, force_refresh: bool, page_id: &str) -> Option<YtAccount> {
        if !self.connection().connected {
            return None;
        }
        if !force_refresh {
            if let Some(hit) = self.cached_account(page_id) {
                return Some(hit);
            }
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let mut merged = YtAccount::default();

        for (api, origin) in [(MUSIC_API, MUSIC_ORIGIN), (YOUTUBE_API, YOUTUBE_ORIGIN)] {
            let url = format!("{api}/account/account_menu?key={api_key}&prettyPrint=false");
            let body = json!({ "context": self.web_context(&client_ver, visitor.as_deref()) });
            let ref_url = format!("{origin}/");
            let Ok(root) = self
                .post_json(
                    &url,
                    &body,
                    "WEB_REMIX",
                    &client_ver,
                    WEB_USER_AGENT,
                    true,
                    Some(origin),
                    Some(&ref_url),
                    None,
                    2,
                    None,
                    Some(page_id),
                )
                .await
            else {
                continue;
            };
            let account = self.parse_account(&root);
            if merged.name.is_empty() && !account.name.is_empty() {
                merged.name = account.name;
            }
            if merged.handle.is_empty() && !account.handle.is_empty() {
                merged.handle = account.handle;
            }
            if merged.email.is_empty() && !account.email.is_empty() {
                merged.email = account.email;
            }
            if merged.photo_url.is_empty() && !account.photo_url.is_empty() {
                merged.photo_url = account.photo_url;
            }
            if !merged.name.is_empty() && !merged.photo_url.is_empty() && !merged.handle.is_empty()
            {
                break;
            }
        }

        if merged.is_empty() {
            return None;
        }
        if let Ok(mut guard) = self.account_caches.write() {
            guard.insert(
                page_id.to_string(),
                AccountCacheEntry {
                    account: merged.clone(),
                    fetched_at_ms: now_unix_ms(),
                },
            );
        }
        Some(merged)
    }

    fn parse_account(&self, root: &Value) -> YtAccount {
        let mut active = Vec::new();
        let mut generic = Vec::new();
        let mut items = Vec::new();
        collect_objects(root, "activeAccountHeaderRenderer", &mut active);
        collect_objects(root, "accountHeader", &mut generic);
        collect_objects(root, "accountItemRenderer", &mut items);

        let handle = self.channel_handle_text(root);
        let mut name_only: Option<YtAccount> = None;

        for h in active.into_iter().chain(generic).chain(items) {
            let name = h
                .get("accountName")
                .and_then(|n| n.get("runs"))
                .and_then(runs_text)
                .unwrap_or_default()
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let photo = self.account_photo(&h);
            let email = account_email(&h);
            if !photo.is_empty() {
                return YtAccount {
                    name,
                    handle,
                    email,
                    photo_url: photo,
                };
            }
            if name_only.is_none() {
                name_only = Some(YtAccount {
                    name,
                    handle: handle.clone(),
                    email,
                    photo_url: String::new(),
                });
            }
        }
        name_only.unwrap_or_default()
    }

    fn channel_handle_text(&self, root: &Value) -> String {
        let mut handles = Vec::new();
        collect_objects(root, "channelHandle", &mut handles);
        for h in handles {
            if let Some(hit) = search_handle_text(&Value::Object(h)) {
                return hit;
            }
        }
        String::new()
    }

    fn account_photo(&self, h: &Map<String, Value>) -> String {
        match h.get("accountPhoto") {
            Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
            Some(node) => extract_thumbnails_url(node).unwrap_or_default(),
            None => String::new(),
        }
    }

    pub async fn resolve_identity_for(&self, cookies: &str, page_id: &str) -> Option<YtAccount> {
        let normalized = Self::normalize_cookies(cookies).ok()?;
        let saved_conn = self.connection();
        let saved_caches = self
            .account_caches
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();

        self.set_connection(YtConnection {
            connected: true,
            cookies: normalized,
            connected_at_millis: now_unix_ms(),
            active_page_id: page_id.to_string(),
            ..Default::default()
        });

        let result = self.fetch_account_info(true, page_id).await;
        self.set_connection(saved_conn);
        if let Ok(mut g) = self.account_caches.write() {
            *g = saved_caches;
        }
        result
    }

    pub async fn fetch_channels_for(&self, cookies: &str) -> Vec<YtChannel> {
        let Ok(normalized) = Self::normalize_cookies(cookies) else {
            return Vec::new();
        };
        let saved_conn = self.connection();
        let saved_caches = self
            .account_caches
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();

        self.set_connection(YtConnection {
            connected: true,
            cookies: normalized,
            connected_at_millis: now_unix_ms(),
            ..Default::default()
        });

        let result = self.fetch_available_channels().await;
        self.set_connection(saved_conn);
        if let Ok(mut g) = self.account_caches.write() {
            *g = saved_caches;
        }
        result
    }

    pub async fn fetch_available_channels(&self) -> Vec<YtChannel> {
        if !self.connection().connected {
            return Vec::new();
        }
        self.ensure_config().await;
        let (_, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_ORIGIN}/getAccountSwitcherEndpoint");
        let mut req = self
            .client
            .get(&url)
            .header(USER_AGENT, WEB_USER_AGENT)
            .header(ORIGIN, MUSIC_ORIGIN)
            .header(REFERER, format!("{MUSIC_ORIGIN}/"))
            .header("X-YouTube-Client-Name", client_id_for_name("WEB_REMIX"))
            .header("X-YouTube-Client-Version", &client_ver)
            .header("X-Goog-AuthUser", "0");
        if let Some(vis) = visitor.as_deref().filter(|s| !s.is_empty()) {
            req = req.header("X-Goog-Visitor-Id", vis);
        }
        if let Some(cookie) = self.cookie_header_value() {
            req = req.header(COOKIE, cookie);
        }
        if let Some(auth) = self.authorization_header_value(Some(MUSIC_ORIGIN)) {
            req = req.header("Authorization", auth);
        }

        let Ok(Ok(res)) = tokio::time::timeout(Duration::from_secs(8), req.send()).await else {
            return Vec::new();
        };
        let Ok(text) = res.text().await else {
            return Vec::new();
        };
        let Some(start) = text.find('{') else {
            return Vec::new();
        };
        let Ok(decoded) = serde_json::from_str::<Value>(&text[start..]) else {
            return Vec::new();
        };
        if decoded.get("code").and_then(Value::as_str) != Some("SUCCESS") {
            return Vec::new();
        }
        let root = decoded.get("data").filter(|d| d.is_object()).unwrap_or(&decoded);
        self.parse_switcher_channels(root)
    }

    fn parse_switcher_channels(&self, root: &Value) -> Vec<YtChannel> {
        let mut items = Vec::new();
        collect_objects(root, "accountItem", &mut items);
        let mut out = Vec::new();
        let mut seen = HashSet::new();

        for item in items {
            let page_id = switcher_page_id(&item);
            if page_id.is_empty() || !seen.insert(page_id.clone()) {
                continue;
            }
            let name_node = item.get("accountName");
            let name = name_node
                .and_then(|n| n.get("runs"))
                .and_then(runs_text)
                .or_else(|| name_node.and_then(runs_text))
                .or_else(|| {
                    name_node
                        .and_then(|n| n.get("simpleText"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default()
                .trim()
                .to_string();
            let handle = switcher_handle(&item);
            if name.is_empty() && handle.is_empty() {
                continue;
            }
            out.push(YtChannel {
                page_id,
                name,
                handle,
                photo_url: self.account_photo(&item),
            });
        }
        out
    }

    // Search

    pub async fn search_songs(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<YouTubeMusicTrack>, InnerTubeError> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/search?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "query": q,
            "params": SONGS_SEARCH_FILTER,
        });
        let root = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                false,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await?;
        Ok(self.parse_song_renderers(&root).into_iter().take(limit).collect())
    }

    pub async fn search_mixed(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<YouTubeMusicTrack>, InnerTubeError> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/search?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "query": q,
        });
        let root = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                false,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await?;
        Ok(self.parse_song_renderers(&root).into_iter().take(limit).collect())
    }

    pub async fn search_artists(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<YouTubeMusicEntity>, InnerTubeError> {
        self.search_entities(query, YouTubeEntityKind::Artist, ARTIST_SEARCH_FILTER, limit)
            .await
    }

    pub async fn search_albums(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<YouTubeMusicEntity>, InnerTubeError> {
        self.search_entities(query, YouTubeEntityKind::Album, ALBUM_SEARCH_FILTER, limit)
            .await
    }

    async fn search_entities(
        &self,
        query: &str,
        kind: YouTubeEntityKind,
        filter: &str,
        limit: usize,
    ) -> Result<Vec<YouTubeMusicEntity>, InnerTubeError> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/search?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "query": q,
            "params": filter,
        });
        let root = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                false,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await?;
        Ok(self
            .parse_entity_renderers(&root, kind)
            .into_iter()
            .take(limit)
            .collect())
    }

    pub async fn search_playlists(&self, query: &str, limit: usize) -> Vec<YouTubePlaylistSummary> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/search?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "query": q,
            "params": PLAYLIST_SEARCH_FILTER,
        });
        let Ok(root) = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                false,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await
        else {
            return Vec::new();
        };
        self.parse_playlist_renderers(&root)
            .into_iter()
            .take(limit)
            .collect()
    }

    pub async fn get_suggestions(&self, query: &str) -> Vec<String> {
        let q = query.trim();
        if q.chars().count() < 2 {
            return Vec::new();
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/music/get_search_suggestions?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "input": q,
        });
        let Ok(root) = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                false,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await
        else {
            return Vec::new();
        };

        let mut out = Vec::new();
        let mut seen = HashSet::new();
        fn walk(node: &Value, out: &mut Vec<String>, seen: &mut HashSet<String>) {
            match node {
                Value::Object(obj) => {
                    let endpoint = obj.get("searchEndpoint").or_else(|| {
                        obj.get("navigationEndpoint")
                            .and_then(|n| n.get("searchEndpoint"))
                    });
                    if let Some(Value::Object(ep)) = endpoint {
                        let text = val_str(ep.get("query")).trim().to_string();
                        if !text.is_empty() && seen.insert(text.to_lowercase()) {
                            out.push(text);
                        }
                    }
                    for v in obj.values() {
                        walk(v, out, seen);
                    }
                }
                Value::Array(arr) => {
                    for v in arr {
                        walk(v, out, seen);
                    }
                }
                _ => {}
            }
        }
        walk(root.get("contents").unwrap_or(&root), &mut out, &mut seen);
        out
    }

    // Browse (Home / Artist / Songs / Album / Related / Details)

    pub async fn fetch_home_shelves(
        &self,
        max_shelves: usize,
        max_items_per_shelf: usize,
        max_pages: usize,
        authenticated: bool,
    ) -> Vec<YtHomeShelf> {
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/browse?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "browseId": HOME_BROWSE_ID,
        });
        let Ok(root) = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                authenticated,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await
        else {
            return Vec::new();
        };

        let mut shelves = self.parse_home_shelves(&root, max_shelves, max_items_per_shelf);
        let mut page = root;
        let mut seen_tokens = HashSet::new();
        for _ in 1..max_pages {
            if shelves.len() >= max_shelves {
                break;
            }
            let Some(token) = self.home_continuation_token(&page) else {
                break;
            };
            if !seen_tokens.insert(token.clone()) {
                break;
            }
            let Ok(next_page) = self.browse_continuation(&token, authenticated).await else {
                break;
            };
            let rem = max_shelves.saturating_sub(shelves.len());
            shelves.extend(self.parse_home_shelves(&next_page, rem, max_items_per_shelf));
            page = next_page;
        }
        shelves
    }

    async fn browse_with_params(
        &self,
        browse_id: &str,
        params: Option<&str>,
    ) -> Result<Value, InnerTubeError> {
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/browse?key={api_key}&prettyPrint=false");
        let mut body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "browseId": browse_id,
        });
        if let Some(p) = params.filter(|s| !s.is_empty()) {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("params".to_string(), json!(p));
            }
        }
        self.post_json(
            &url,
            &body,
            "WEB_REMIX",
            &client_ver,
            WEB_USER_AGENT,
            true,
            None,
            None,
            None,
            2,
            None,
            None,
        )
        .await
    }

    pub async fn fetch_artist_page(&self, browse_id: &str) -> Option<YtArtistPage> {
        let bid = browse_id.trim();
        if bid.is_empty() {
            return None;
        }
        let root = self.browse_with_params(bid, None).await.ok()?;
        let mut section_lists = Vec::new();
        collect_objects(&root, "sectionListRenderer", &mut section_lists);

        let mut songs = Vec::new();
        let mut shelves = Vec::new();

        for list in section_lists {
            let Some(contents) = list.get("contents").and_then(Value::as_array) else {
                continue;
            };
            for section in contents {
                let Some(sec_obj) = section.as_object() else {
                    continue;
                };
                if let Some(song_shelf) = sec_obj.get("musicShelfRenderer").filter(|v| v.is_object()) {
                    let on_page = self.parse_song_renderers(song_shelf);
                    if songs.is_empty() {
                        songs = on_page;
                    }
                    let more = song_shelf
                        .get("title")
                        .and_then(first_browse_endpoint)
                        .or_else(|| song_shelf.get("bottomEndpoint").and_then(first_browse_endpoint));
                    if let Some(ep) = more {
                        let more_id = val_str(ep.get("browseId"));
                        let more_params = ep
                            .get("params")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        if !more_id.is_empty() {
                            if let Ok(full) =
                                self.browse_songs(&more_id, more_params.as_deref(), Some(500)).await
                            {
                                if full.len() > songs.len() {
                                    songs = full;
                                }
                            }
                        }
                    }
                    continue;
                }

                let Some(mut shelf) = self.parse_home_section(sec_obj, 500) else {
                    continue;
                };
                if !shelf.is_renderable() {
                    continue;
                }
                if shelf.is_card_shelf() {
                    if let Some(carousel) = sec_obj.get("musicCarouselShelfRenderer").and_then(Value::as_object) {
                        if let Some(ep) = more_endpoint_of_carousel(carousel) {
                            let ep_id = val_str(ep.get("browseId"));
                            let ep_params = ep.get("params").and_then(Value::as_str);
                            if let Ok(grid) = self.browse_with_params(&ep_id, ep_params).await {
                                let mut cards = Vec::new();
                                collect_objects(&grid, "musicTwoRowItemRenderer", &mut cards);
                                let mut more = Vec::new();
                                for card in cards {
                                    let id = card
                                        .get("navigationEndpoint")
                                        .and_then(|n| n.get("browseEndpoint"))
                                        .and_then(|b| b.get("browseId"))
                                        .and_then(Value::as_str)
                                        .unwrap_or("");
                                    if id.is_empty() {
                                        continue;
                                    }
                                    if let Some(e) = self.parse_two_row_entity(&card, Self::kind_for_browse_id(id)) {
                                        if !Self::is_liked_music_id(&e.browse_id) {
                                            more.push(e);
                                        }
                                    }
                                }
                                if more.len() > shelf.entities.len() {
                                    shelf.entities = more;
                                }
                            }
                        }
                    }
                }
                shelves.push(shelf);
            }
        }

        if songs.is_empty() && shelves.is_empty() {
            return None;
        }
        Some(YtArtistPage { songs, shelves })
    }

    pub fn kind_for_browse_id(browse_id: &str) -> YouTubeEntityKind {
        if browse_id.starts_with("UC") {
            return YouTubeEntityKind::Artist;
        }
        let bare = browse_id.strip_prefix("VL").unwrap_or(browse_id);
        if bare.starts_with("PL") {
            return YouTubeEntityKind::Playlist;
        }
        if bare.starts_with("RD") {
            return YouTubeEntityKind::Mix;
        }
        YouTubeEntityKind::Album
    }

    pub fn is_liked_music_id(id: &str) -> bool {
        id.strip_prefix("VL").unwrap_or(id) == "LM"
    }

    pub fn parse_home_shelves(
        &self,
        root: &Value,
        max_shelves: usize,
        max_items_per_shelf: usize,
    ) -> Vec<YtHomeShelf> {
        if max_shelves == 0 {
            return Vec::new();
        }
        let mut shelves = Vec::new();
        let mut section_lists = Vec::new();
        collect_objects(root, "sectionListRenderer", &mut section_lists);
        collect_objects(root, "sectionListContinuation", &mut section_lists);
        for list in section_lists {
            let Some(contents) = list.get("contents").and_then(Value::as_array) else {
                continue;
            };
            for section in contents {
                let Some(sec_obj) = section.as_object() else {
                    continue;
                };
                if let Some(shelf) = self.parse_home_section(sec_obj, max_items_per_shelf) {
                    if shelf.is_renderable() {
                        shelves.push(shelf);
                        if shelves.len() >= max_shelves {
                            return shelves;
                        }
                    }
                }
            }
        }
        shelves
    }

    fn parse_home_section(
        &self,
        section: &Map<String, Value>,
        max_items_per_shelf: usize,
    ) -> Option<YtHomeShelf> {
        if let Some(song_shelf) = section.get("musicShelfRenderer").filter(|v| v.is_object()) {
            let tracks: Vec<YouTubeMusicTrack> = self
                .parse_song_renderers(song_shelf)
                .into_iter()
                .take(max_items_per_shelf)
                .collect();
            if tracks.is_empty() {
                return None;
            }
            return Some(YtHomeShelf {
                title: home_shelf_title(song_shelf.get("title")).unwrap_or_else(|| "Songs".to_string()),
                tracks,
                ..Default::default()
            });
        }

        if let Some(carousel) = section
            .get("musicCarouselShelfRenderer")
            .and_then(Value::as_object)
        {
            let mut title: Option<String> = None;
            let mut subtitle = String::new();
            if let Some(header) = carousel.get("header").and_then(Value::as_object) {
                for key in ["musicCarouselShelfBasicHeaderRenderer", "musicDetailHeaderRenderer"] {
                    let Some(h) = header.get(key) else {
                        continue;
                    };
                    if title.is_none() {
                        title = home_shelf_title(h.get("title"));
                    }
                    if subtitle.is_empty() {
                        subtitle = home_shelf_title(h.get("straplineTextOne"))
                            .or_else(|| home_shelf_title(h.get("straplineTextTwo")))
                            .unwrap_or_default();
                    }
                }
            }

            let carousel_contents = carousel.get("contents").unwrap_or(&Value::Null);
            let mut row_items = Vec::new();
            collect_objects(
                carousel_contents,
                "musicResponsiveListItemRenderer",
                &mut row_items,
            );
            if !row_items.is_empty() {
                let rows: Vec<YouTubeMusicTrack> = self
                    .parse_song_renderers(carousel_contents)
                    .into_iter()
                    .take(max_items_per_shelf)
                    .collect();
                if rows.is_empty() {
                    return None;
                }
                return Some(YtHomeShelf {
                    title: title.unwrap_or_default(),
                    subtitle,
                    tracks: rows,
                    ..Default::default()
                });
            }

            let mut entities = Vec::new();
            let mut track_cards = Vec::new();
            if let Some(contents) = carousel.get("contents").and_then(Value::as_array) {
                for item in contents {
                    let Some(two_row) = item
                        .get("musicTwoRowItemRenderer")
                        .and_then(Value::as_object)
                    else {
                        continue;
                    };
                    let browse_id = two_row
                        .get("navigationEndpoint")
                        .and_then(|n| n.get("browseEndpoint"))
                        .and_then(|b| b.get("browseId"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if !browse_id.is_empty() {
                        if let Some(entity) =
                            self.parse_two_row_entity(two_row, Self::kind_for_browse_id(browse_id))
                        {
                            if !Self::is_liked_music_id(&entity.browse_id)
                                && !Self::is_liked_music_id(&entity.playlist_id)
                            {
                                entities.push(entity);
                            }
                        }
                    } else {
                        let watch = two_row
                            .get("navigationEndpoint")
                            .and_then(|n| n.get("watchEndpoint"));
                        let watch_playlist_id = watch
                            .and_then(|w| w.get("playlistId"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        let watch_video_id = watch
                            .and_then(|w| w.get("videoId"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if !watch_video_id.is_empty() || watch_playlist_id.is_empty() {
                            if let Some(song) = self.parse_two_row_song(two_row) {
                                track_cards.push(song);
                            }
                        } else if let Some(entity) = self.parse_two_row_entity(
                            two_row,
                            Self::kind_for_browse_id(watch_playlist_id),
                        ) {
                            if !Self::is_liked_music_id(&entity.browse_id)
                                && !Self::is_liked_music_id(&entity.playlist_id)
                            {
                                entities.push(entity);
                            }
                        }
                    }
                    if entities.len() + track_cards.len() >= max_items_per_shelf {
                        break;
                    }
                }
            }

            if entities.is_empty() && track_cards.is_empty() {
                return None;
            }
            return Some(YtHomeShelf {
                title: title.unwrap_or_default(),
                subtitle,
                entities,
                track_cards,
                ..Default::default()
            });
        }

        None
    }

    fn home_continuation_token(&self, page: &Value) -> Option<String> {
        let mut lists = Vec::new();
        collect_objects(page, "sectionListRenderer", &mut lists);
        collect_objects(page, "sectionListContinuation", &mut lists);
        for list in lists {
            if let Some(conts) = list.get("continuations") {
                if let Some(tok) = find_string(conts, "continuation") {
                    if !tok.is_empty() {
                        return Some(tok);
                    }
                }
            }
        }
        None
    }

    pub async fn browse_songs(
        &self,
        browse_id: &str,
        params: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<YouTubeMusicTrack>, InnerTubeError> {
        if browse_id.trim().is_empty() {
            return Err(InnerTubeError::InvalidFormat(
                "Missing YouTube Music browse id".to_string(),
            ));
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/browse?key={api_key}&prettyPrint=false");
        let mut body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "browseId": browse_id,
        });
        if let Some(p) = params.filter(|s| !s.is_empty()) {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("params".to_string(), json!(p));
            }
        }
        let root = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                true,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await?;
        let (tracks, _) = self.collect_browse_song_pages(&root, limit, false).await;
        if tracks.is_empty() && browse_id == CHARTS_BROWSE_ID {
            return Ok(self.chart_playlist_tracks(&root, limit).await);
        }
        Ok(tracks)
    }

    async fn chart_playlist_tracks(&self, root: &Value, limit: Option<usize>) -> Vec<YouTubeMusicTrack> {
        let mut items = Vec::new();
        collect_objects(root, "musicTwoRowItemRenderer", &mut items);
        let mut ids = Vec::new();
        for item in items {
            if let Some(id) = item
                .get("navigationEndpoint")
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(Value::as_str)
            {
                if id.starts_with("VL") && !ids.iter().any(|x| x == id) {
                    ids.push(id.to_string());
                }
            }
        }
        for id in ids.into_iter().take(3) {
            if let Ok(tracks) = Box::pin(self.browse_songs(&id, None, limit)).await {
                if !tracks.is_empty() {
                    return tracks;
                }
            }
        }
        Vec::new()
    }

    pub async fn browse_album(
        &self,
        browse_id: &str,
        limit: Option<usize>,
    ) -> Option<YouTubeAlbumResult> {
        if browse_id.trim().is_empty() {
            return None;
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/browse?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "browseId": browse_id,
        });
        let root = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                true,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await
            .ok()?;

        let header = self.playlist_header(&root);
        let title = self
            .extract_title_from_header(header.as_ref(), &root)
            .unwrap_or_default();
        let artist = album_header_artist(header.as_ref()).unwrap_or_default();
        let artwork = self
            .extract_artwork_from_header(header.as_ref(), &root)
            .unwrap_or_default();
        let mut year = String::new();
        if let Some(runs) = header
            .as_ref()
            .and_then(|h| h.get("subtitle"))
            .and_then(|s| s.get("runs"))
            .and_then(Value::as_array)
        {
            for run in runs {
                let text = run.get("text").and_then(Value::as_str).unwrap_or("").trim();
                if text.len() == 4 && text.chars().all(|c| c.is_ascii_digit()) {
                    year = text.to_string();
                    break;
                }
            }
        }

        let (raw_tracks, _) = self.collect_browse_song_pages(&root, limit, false).await;
        let tracks = raw_tracks
            .into_iter()
            .map(|t| YouTubeMusicTrack {
                video_id: t.video_id,
                title: t.title,
                artist: if t.artist.is_empty() || t.artist == "Unknown artist" {
                    if !artist.is_empty() {
                        artist.clone()
                    } else {
                        t.artist
                    }
                } else {
                    t.artist
                },
                album: if t.album.is_empty() {
                    title.clone()
                } else {
                    t.album
                },
                artwork_url: if !t.artwork_url.is_empty() {
                    t.artwork_url
                } else {
                    artwork.clone()
                },
                duration_seconds: t.duration_seconds,
                album_browse_id: t.album_browse_id,
            })
            .collect();

        Some(YouTubeAlbumResult {
            browse_id: browse_id.to_string(),
            title,
            artist,
            artwork_url: artwork,
            year,
            tracks,
        })
    }

    async fn collect_browse_song_pages(
        &self,
        root: &Value,
        limit: Option<usize>,
        authenticated: bool,
    ) -> (Vec<YouTubeMusicTrack>, bool) {
        let mut shelves = Vec::new();
        collect_objects(root, "musicPlaylistShelfRenderer", &mut shelves);
        let has_playlist_shelf = !shelves.is_empty();
        if !has_playlist_shelf {
            collect_objects(root, "musicShelfRenderer", &mut shelves);
        }
        let mut primary: Option<&Map<String, Value>> = None;
        for shelf in &shelves {
            if let Some(heading) = shelf
                .get("title")
                .and_then(|t| t.get("runs"))
                .and_then(runs_text)
            {
                let low = heading.to_lowercase();
                if low == "songs" || low == "tracks" {
                    primary = Some(shelf);
                    break;
                }
            }
        }

        let mut known_ids: HashSet<String> = HashSet::new();
        let mut songs: Vec<YouTubeMusicTrack> = Vec::new();
        if let Some(p) = primary {
            for s in self.parse_song_renderers(&Value::Object(p.clone())) {
                if known_ids.insert(s.video_id.clone()) {
                    songs.push(s);
                }
            }
        } else if !shelves.is_empty() {
            let targets: &[Map<String, Value>] = if has_playlist_shelf {
                &shelves[..1]
            } else {
                &shelves
            };
            for shelf in targets {
                for s in self.parse_song_renderers(&Value::Object(shelf.clone())) {
                    if known_ids.insert(s.video_id.clone()) {
                        songs.push(s);
                    }
                }
            }
        } else {
            for s in self.parse_song_renderers(root) {
                if known_ids.insert(s.video_id.clone()) {
                    songs.push(s);
                }
            }
        }

        if shelves.is_empty() {
            let take = limit.unwrap_or(songs.len());
            songs.truncate(take);
            return (songs, false);
        }

        let mut token = if let Some(p) = primary {
            self.playlist_track_continuation_token(&Value::Object(p.clone()))
        } else {
            shelves
                .iter()
                .rev()
                .find_map(|s| self.playlist_track_continuation_token(&Value::Object(s.clone())))
                .or_else(|| self.playlist_track_continuation_token(root))
        };
        let mut seen_tokens = HashSet::new();
        let max_pages = limit.map(|l| (l / 20 + 1).min(8)).unwrap_or(6);
        let mut page = 0;

        while let Some(tok) = token.take() {
            if tok.is_empty()
                || page >= max_pages
                || limit.is_some_and(|l| songs.len() >= l)
                || !seen_tokens.insert(tok.clone())
            {
                break;
            }
            let Ok(next_page) = self.browse_continuation(&tok, authenticated).await else {
                break;
            };
            let containers = self.playlist_track_containers(&next_page);
            if containers.is_empty() {
                break;
            }
            for c in &containers {
                for s in self.parse_song_renderers(c) {
                    if known_ids.insert(s.video_id.clone()) {
                        songs.push(s);
                    }
                }
            }
            token = containers
                .iter()
                .find_map(|c| self.playlist_track_continuation_token(c).filter(|t| !t.is_empty()));
            page += 1;
        }

        let total = songs.len();
        let result: Vec<YouTubeMusicTrack> = match limit {
            Some(l) => songs.into_iter().take(l).collect(),
            None => songs,
        };
        let is_complete = token.is_none() && result.len() == total;
        (result, is_complete)
    }

    pub async fn fetch_related_songs(
        &self,
        video_id: &str,
        limit: usize,
    ) -> Result<Vec<YouTubeMusicTrack>, InnerTubeError> {
        if video_id.trim().is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/next?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "videoId": video_id,
            "playlistId": format!("RDAMVM{video_id}"),
            "params": "wAEB",
            "isAudioOnly": true,
        });
        let root = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                false,
                None,
                None,
                None,
                2,
                Some(Duration::from_secs(8)),
                None,
            )
            .await?;
        Ok(self
            .parse_song_renderers(&root)
            .into_iter()
            .filter(|t| t.video_id != video_id)
            .take(limit)
            .collect())
    }

    pub async fn fetch_song_details(&self, video_id: &str) -> Option<YouTubeMusicTrack> {
        if video_id.trim().is_empty() {
            return None;
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/player?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "videoId": video_id,
        });
        if let Ok(root) = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                false,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await
        {
            if let Some(details) = root.get("videoDetails").and_then(Value::as_object) {
                let mut title = val_str(details.get("title"));
                let mut artist = val_str(details.get("author"));
                let duration = details
                    .get("lengthSeconds")
                    .and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64()))
                    .unwrap_or(0);
                let artwork = details
                    .get("thumbnail")
                    .and_then(|t| t.get("thumbnails"))
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.last())
                    .and_then(|item| item.get("url"))
                    .and_then(Value::as_str)
                    .map(str::to_string);

                if !title.is_empty() && !artist.is_empty() {
                    if let Some(stripped) = artist.strip_suffix(" - Topic") {
                        artist = stripped.trim().to_string();
                    }
                    if title.contains(" - ") {
                        let parts: Vec<&str> = title.split(" - ").collect();
                        let head = parts[0].trim();
                        if artist.is_empty()
                            || artist == "YouTube Music"
                            || artist.eq_ignore_ascii_case(head)
                        {
                            artist = head.to_string();
                            title = parts[1..].join(" - ").trim().to_string();
                        }
                    }
                    return Some(YouTubeMusicTrack {
                        video_id: video_id.to_string(),
                        title,
                        artist,
                        artwork_url: artwork.unwrap_or_else(|| {
                            format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")
                        }),
                        duration_seconds: duration,
                        ..Default::default()
                    });
                }
            }
        }

        let oembed_url = format!(
            "https://www.youtube.com/oembed?url=https://www.youtube.com/watch?v={video_id}&format=json"
        );
        let res = self
            .client
            .get(&oembed_url)
            .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .send()
            .await
            .ok()?;
        let text = res.text().await.ok()?;
        let obj = serde_json::from_str::<Value>(&text).ok()?;
        let raw_title = val_str(obj.get("title"));
        let mut author = val_str(obj.get("author_name")).trim().to_string();
        if let Some(stripped) = author.strip_suffix(" - Topic") {
            author = stripped.trim().to_string();
        }
        let thumbnail = obj
            .get("thumbnail_url")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg"));

        let mut final_title = raw_title.clone();
        let mut final_artist = if author.is_empty() {
            "YouTube Music".to_string()
        } else {
            author
        };
        if raw_title.contains(" - ") {
            let parts: Vec<&str> = raw_title.split(" - ").collect();
            final_artist = parts[0].trim().to_string();
            final_title = parts[1..].join(" - ").trim().to_string();
        }
        if final_title.is_empty() {
            return None;
        }
        Some(YouTubeMusicTrack {
            video_id: video_id.to_string(),
            title: final_title,
            artist: final_artist,
            artwork_url: thumbnail,
            ..Default::default()
        })
    }

    // Playlists

    pub fn extract_playlist_id(input: &str) -> String {
        let clean = input.trim();
        if let Some((_, rest)) = clean.split_once("list=") {
            return rest
                .split('&')
                .next()
                .unwrap_or("")
                .split('#')
                .next()
                .unwrap_or("")
                .to_string();
        }
        if let Some((_, rest)) = clean.split_once("playlist/") {
            return rest
                .split('?')
                .next()
                .unwrap_or("")
                .split('/')
                .next()
                .unwrap_or("")
                .to_string();
        }
        clean.to_string()
    }

    fn playlist_browse_id(raw_id: &str) -> String {
        if raw_id.starts_with("VL")
            || raw_id.starts_with("FE")
            || raw_id.starts_with("MPRE")
            || raw_id.starts_with("UC")
        {
            raw_id.to_string()
        } else {
            format!("VL{raw_id}")
        }
    }

    pub async fn fetch_playlist(
        &self,
        playlist_id_or_url: &str,
        max_tracks: Option<usize>,
    ) -> Option<YouTubePlaylistResult> {
        let mix_id = Self::extract_playlist_id(playlist_id_or_url);
        let bare_mix = mix_id.strip_prefix("VL").unwrap_or(&mix_id);
        if bare_mix.starts_with("RD") {
            if let Some(quick) = self.fetch_playlist_via_next(&mix_id, max_tracks).await {
                return Some(quick);
            }
        }
        let via_browse = self
            .fetch_playlist_via_browse(playlist_id_or_url, max_tracks)
            .await;
        if let Some(ref res) = via_browse {
            if !res.tracks.is_empty() {
                return via_browse;
            }
        }
        let raw_id = Self::extract_playlist_id(playlist_id_or_url);
        if raw_id.is_empty() {
            return via_browse;
        }
        self.fetch_playlist_via_next(&raw_id, max_tracks)
            .await
            .or(via_browse)
    }

    async fn fetch_playlist_via_next(
        &self,
        raw_id: &str,
        max_tracks: Option<usize>,
    ) -> Option<YouTubePlaylistResult> {
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let id = raw_id.strip_prefix("VL").unwrap_or(raw_id);
        let url = format!("{MUSIC_API}/next?key={api_key}&prettyPrint=false");
        let auth_body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "playlistId": id,
            "isAudioOnly": true,
            "enablePersistentPlaylistPanel": true,
            "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
        });
        let root = match self
            .post_json(
                &url,
                &auth_body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                self.connection().connected,
                None,
                None,
                None,
                2,
                Some(Duration::from_secs(12)),
                None,
            )
            .await
        {
            Ok(r) => r,
            Err(_) => {
                let anon_body = json!({
                    "context": self.web_context(&client_ver, visitor.as_deref()),
                    "playlistId": id,
                    "isAudioOnly": true,
                });
                self.post_json(
                    &url,
                    &anon_body,
                    "WEB_REMIX",
                    &client_ver,
                    WEB_USER_AGENT,
                    false,
                    None,
                    None,
                    None,
                    2,
                    Some(Duration::from_secs(12)),
                    None,
                )
                .await
                .ok()?
            }
        };

        let mut songs = self.parse_song_renderers(&root);
        if let Some(max_t) = max_tracks.filter(|&m| m >= 1) {
            songs.truncate(max_t);
        }
        if songs.is_empty() {
            return None;
        }
        let mut panels = Vec::new();
        collect_objects(&root, "playlistPanelRenderer", &mut panels);
        let title = panels
            .first()
            .and_then(|p| p.get("title"))
            .and_then(|t| t.get("runs").and_then(runs_text).or_else(|| runs_text(t)))
            .unwrap_or_default();
        let artwork_url = songs.first().map(|s| s.artwork_url.clone()).unwrap_or_default();
        let track_count = songs.len();

        Some(YouTubePlaylistResult {
            id: raw_id.to_string(),
            title,
            author: "YouTube Music".to_string(),
            artwork_url,
            track_count,
            tracks: songs,
        })
    }

    async fn fetch_playlist_via_browse(
        &self,
        playlist_id_or_url: &str,
        max_tracks: Option<usize>,
    ) -> Option<YouTubePlaylistResult> {
        let raw_id = Self::extract_playlist_id(playlist_id_or_url);
        if raw_id.is_empty() {
            return None;
        }
        let browse_id = Self::playlist_browse_id(&raw_id);
        let (root, authenticated_as) = self.fetch_playlist_root(&browse_id).await?;
        let header = self.playlist_header(&root);
        let title = self
            .extract_title_from_header(header.as_ref(), &root)
            .unwrap_or_default();
        let author = header
            .as_ref()
            .and_then(|h| {
                first_run_text(h.get("subtitle"))
                    .or_else(|| first_run_text(h.get("straplineTextOne")))
                    .or_else(|| find_first_author(h))
            })
            .unwrap_or_default();
        let artwork_url = self
            .extract_artwork_from_header(header.as_ref(), &root)
            .unwrap_or_default();
        let track_limit = max_tracks.filter(|&m| m >= 1);
        let playlist_page = browse_id.starts_with("VL");

        let containers = if playlist_page {
            self.playlist_track_containers(&root)
        } else {
            vec![root.clone()]
        };
        if containers.is_empty() {
            return None;
        }

        let mut songs = Vec::new();
        let mut known_ids = HashSet::new();
        for c in &containers {
            for s in self.parse_song_renderers(c) {
                if known_ids.insert(s.video_id.clone()) {
                    songs.push(s);
                }
            }
        }

        let mut token = self.continuation_token(&containers, playlist_page);
        let mut seen_tokens = HashSet::new();
        let mut page = 0;

        while let Some(tok) = token.take() {
            if tok.is_empty()
                || page >= MAX_CONTINUATION_PAGES
                || track_limit.is_some_and(|l| songs.len() >= l)
            {
                token = Some(tok);
                break;
            }
            if !seen_tokens.insert(tok.clone()) {
                return None;
            }
            let next_page = self
                .browse_continuation(&tok, authenticated_as)
                .await
                .ok()?;
            let page_containers = if playlist_page {
                self.playlist_track_containers(&next_page)
            } else {
                vec![next_page]
            };
            if page_containers.is_empty() {
                return None;
            }
            for c in &page_containers {
                for s in self.parse_song_renderers(c) {
                    if known_ids.insert(s.video_id.clone()) {
                        songs.push(s);
                    }
                }
            }
            token = self.continuation_token(&page_containers, playlist_page);
            page += 1;
        }

        if token.as_deref().is_some_and(|t| !t.is_empty())
            && track_limit.is_none_or(|l| songs.len() < l)
        {
            return None;
        }

        let total = songs.len();
        let limited = match track_limit {
            Some(l) => songs.into_iter().take(l).collect(),
            None => songs,
        };

        Some(YouTubePlaylistResult {
            id: raw_id,
            title,
            author,
            artwork_url,
            track_count: total,
            tracks: limited,
        })
    }

    pub async fn fetch_playlist_artwork(&self, playlist_id_or_url: &str) -> Option<String> {
        let raw_id = Self::extract_playlist_id(playlist_id_or_url);
        if raw_id.is_empty() {
            return None;
        }
        let (root, _) = self
            .fetch_playlist_root(&Self::playlist_browse_id(&raw_id))
            .await?;
        let header = self.playlist_header(&root);
        self.extract_artwork_from_header(header.as_ref(), &root)
            .or_else(|| {
                self.parse_song_renderers(&root)
                    .into_iter()
                    .map(|s| s.artwork_url)
                    .find(|u| !u.is_empty())
            })
    }

    async fn fetch_playlist_root(&self, browse_id: &str) -> Option<(Value, bool)> {
        if self.connection().connected {
            if let Ok(r) = self.browse_root(browse_id, true).await {
                return Some((r, true));
            }
            if let Ok(r) = self.browse_root_youtube(browse_id, true).await {
                return Some((r, true));
            }
        }
        if let Ok(r) = self.browse_root(browse_id, false).await {
            return Some((r, false));
        }
        if let Ok(r) = self.browse_root_youtube(browse_id, false).await {
            return Some((r, false));
        }
        None
    }

    async fn browse_root(
        &self,
        browse_id: &str,
        authenticated: bool,
    ) -> Result<Value, InnerTubeError> {
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/browse?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "browseId": browse_id,
        });
        self.post_json(
            &url,
            &body,
            "WEB_REMIX",
            &client_ver,
            WEB_USER_AGENT,
            authenticated,
            None,
            None,
            None,
            2,
            None,
            None,
        )
        .await
    }

    async fn browse_root_youtube(
        &self,
        browse_id: &str,
        authenticated: bool,
    ) -> Result<Value, InnerTubeError> {
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{YOUTUBE_API}/browse?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "browseId": browse_id,
        });
        let ref_url = format!("{YOUTUBE_ORIGIN}/");
        self.post_json(
            &url,
            &body,
            "WEB_REMIX",
            &client_ver,
            WEB_USER_AGENT,
            authenticated,
            Some(YOUTUBE_ORIGIN),
            Some(&ref_url),
            None,
            2,
            None,
            None,
        )
        .await
    }

    async fn browse_continuation(
        &self,
        token: &str,
        authenticated: bool,
    ) -> Result<Value, InnerTubeError> {
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/browse?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "continuation": token,
        });
        self.post_json(
            &url,
            &body,
            "WEB_REMIX",
            &client_ver,
            WEB_USER_AGENT,
            authenticated,
            None,
            None,
            None,
            2,
            None,
            None,
        )
        .await
    }

    pub async fn browse_albums(
        &self,
        browse_id: &str,
        limit: usize,
    ) -> Result<Vec<YouTubeMusicEntity>, InnerTubeError> {
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/browse?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "browseId": browse_id,
        });
        let root = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                true,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await?;

        let mut out = Vec::new();
        let mut queue = vec![&root];
        while let Some(current) = queue.pop() {
            match current {
                Value::Object(obj) => {
                    if let Some(two_row) = obj
                        .get("musicTwoRowItemRenderer")
                        .and_then(Value::as_object)
                    {
                        if let Some(entity) =
                            self.parse_two_row_entity(two_row, YouTubeEntityKind::Album)
                        {
                            out.push(entity);
                        }
                    } else {
                        for v in obj.values().take(300) {
                            queue.push(v);
                        }
                    }
                }
                Value::Array(arr) => {
                    for v in arr.iter().take(300) {
                        queue.push(v);
                    }
                }
                _ => {}
            }
            if out.len() >= limit {
                break;
            }
        }
        out.truncate(limit);
        Ok(out)
    }

    // Lyrics & Transcripts

    pub async fn fetch_lyrics_tab_text(&self, video_id: &str) -> Option<String> {
        let id = video_id.trim();
        if id.is_empty() {
            return None;
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let next_url = format!("{MUSIC_API}/next?key={api_key}&prettyPrint=false");
        let next_body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "videoId": id,
            "isAudioOnly": true,
        });
        let next = self
            .post_json(
                &next_url,
                &next_body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                false,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await
            .ok()?;

        let mut tabs = Vec::new();
        collect_objects(&next, "tabRenderer", &mut tabs);
        let lyrics_tab = tabs
            .iter()
            .find(|t| {
                let title = t
                    .get("title")
                    .and_then(|v| runs_text(v).or_else(|| v.as_str().map(str::to_string)))
                    .unwrap_or_default();
                title.to_lowercase().contains("lyrics")
            })
            .or_else(|| tabs.get(1))?;

        let ep = lyrics_tab
            .get("endpoint")
            .and_then(|e| e.get("browseEndpoint"))
            .and_then(Value::as_object)?;
        let browse_id = val_str(ep.get("browseId"));
        if browse_id.is_empty() {
            return None;
        }
        let params = ep.get("params").and_then(Value::as_str);

        let browse_url = format!("{MUSIC_API}/browse?key={api_key}&prettyPrint=false");
        let mut browse_body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "browseId": browse_id,
        });
        if let Some(p) = params.filter(|s| !s.is_empty()) {
            if let Some(obj) = browse_body.as_object_mut() {
                obj.insert("params".to_string(), json!(p));
            }
        }
        let page = self
            .post_json(
                &browse_url,
                &browse_body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                false,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await
            .ok()?;

        let mut shelves = Vec::new();
        collect_objects(&page, "musicDescriptionShelfRenderer", &mut shelves);
        for shelf in shelves {
            let desc_obj = shelf.get("description");
            let desc = desc_obj.and_then(|d| d.get("runs").and_then(runs_text).or_else(|| runs_text(d)));
            if let Some(text) = desc.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
                return Some(text);
            }
        }
        None
    }

    pub async fn fetch_transcript_cues(&self, video_id: &str) -> Option<Vec<TranscriptCue>> {
        let id = video_id.trim();
        if id.is_empty() {
            return None;
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let id_bytes = id.as_bytes();
        let mut param_bytes = Vec::with_capacity(2 + id_bytes.len());
        param_bytes.push(0x0A);
        param_bytes.push(id_bytes.len() as u8);
        param_bytes.extend_from_slice(id_bytes);
        let params = BASE64_STANDARD.encode(param_bytes);

        let url = format!("{MUSIC_API}/get_transcript?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "params": params,
        });
        let res = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                false,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await
            .ok()?;

        let mut cues = Vec::new();
        collect_objects(&res, "transcriptCueRenderer", &mut cues);
        let mut lines = Vec::new();
        for cue in cues {
            let Some(start_offset) = cue
                .get("startOffsetMs")
                .and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64()))
            else {
                continue;
            };
            let cue_obj = cue.get("cue");
            let text = cue_obj
                .and_then(|c| {
                    c.get("runs")
                        .and_then(runs_text)
                        .or_else(|| runs_text(c))
                        .or_else(|| c.as_str().map(str::to_string))
                })
                .unwrap_or_default();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            lines.push(TranscriptCue {
                time_ms: start_offset,
                text: trimmed.to_string(),
            });
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines)
        }
    }

    // Podcasts

    async fn podcast_browse(&self, browse_id: &str) -> Result<Value, InnerTubeError> {
        if self.connection().connected {
            if let Ok(r) = self.browse_root(browse_id, true).await {
                return Ok(r);
            }
        }
        self.browse_root(browse_id, false).await
    }

    pub async fn fetch_podcast_home(&self) -> Result<Vec<PodcastShelf>, InnerTubeError> {
        let root = self.podcast_browse(PODCASTS_BROWSE_ID).await?;
        Ok(PodcastParser::parse_home(&root))
    }

    pub async fn fetch_podcast_show(&self, show_id: &str) -> Result<Option<PodcastShowPage>, InnerTubeError> {
        let clean = show_id.trim();
        if clean.is_empty() {
            return Ok(None);
        }
        let root = self.podcast_browse(clean).await?;
        Ok(Some(PodcastParser::parse_show(&root, clean, "")))
    }

    pub async fn fetch_podcast_show_continuation(
        &self,
        show_id: &str,
        continuation: &str,
        show_title: Option<&str>,
    ) -> Result<PodcastEpisodePage, InnerTubeError> {
        let cont = continuation.trim();
        if cont.is_empty() {
            return Ok(PodcastEpisodePage::default());
        }
        let root = if self.connection().connected {
            match self.browse_continuation(cont, true).await {
                Ok(r) => r,
                Err(_) => self.browse_continuation(cont, false).await?,
            }
        } else {
            self.browse_continuation(cont, false).await?
        };
        Ok(PodcastParser::parse_episode_continuation(
            &root,
            Some(show_id),
            show_title,
        ))
    }

    pub async fn search_podcast_episodes(&self, query: &str, limit: usize) -> Vec<PodcastEpisode> {
        let q = query.trim();
        if q.is_empty() || limit == 0 {
            return Vec::new();
        }
        self.ensure_config().await;
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/search?key={api_key}&prettyPrint=false");

        let run_search = |params: Option<String>| {
            let mut body = json!({
                "context": self.web_context(&client_ver, visitor.as_deref()),
                "query": q,
            });
            if let Some(p) = params.filter(|s| !s.is_empty()) {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("params".to_string(), json!(p));
                }
            }
            let u = url.clone();
            let cv = client_ver.clone();
            async move {
                self.post_json(
                    &u,
                    &body,
                    "WEB_REMIX",
                    &cv,
                    WEB_USER_AGENT,
                    false,
                    None,
                    None,
                    None,
                    2,
                    None,
                    None,
                )
                .await
            }
        };

        let cached_filter = self
            .podcast_episode_filter
            .read()
            .ok()
            .and_then(|g| g.clone());
        let filter = cached_filter.unwrap_or_else(|| PodcastParser::EPISODE_SEARCH_FILTER.to_string());

        let mut episodes = Vec::new();
        if let Ok(filtered) = run_search(Some(filter)).await {
            episodes = PodcastParser::parse_episode_search(&filtered, false);
        }

        if episodes.is_empty() {
            if let Ok(open) = run_search(None).await {
                if let Some(discovered) = PodcastParser::episode_search_params(&open) {
                    if !discovered.is_empty() {
                        if let Ok(retried) = run_search(Some(discovered.clone())).await {
                            let parsed = PodcastParser::parse_episode_search(&retried, false);
                            if !parsed.is_empty() {
                                if let Ok(mut g) = self.podcast_episode_filter.write() {
                                    *g = Some(discovered);
                                }
                                episodes = parsed;
                            }
                        }
                    }
                }
                if episodes.is_empty() {
                    episodes = PodcastParser::parse_episode_search(&open, true);
                }
            }
        }

        let mut seen = HashSet::new();
        episodes
            .into_iter()
            .filter(|ep| seen.insert(ep.video_id.clone()))
            .take(limit)
            .collect()
    }

    // Remote Playlist Mutations & Owned Playlist Fetch

    pub async fn create_remote_playlist(&self, title: &str) -> Option<String> {
        if !self.connection().connected {
            return None;
        }
        self.ensure_config().await;
        let clean = title.replace('<', "(").replace('>', ")").trim().to_string();
        let name: String = if clean.is_empty() {
            "LastWave Playlist".to_string()
        } else {
            clean.chars().take(150).collect()
        };
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/playlist/create?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "title": name,
            "privacyStatus": "PRIVATE",
        });
        let root = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                true,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await
            .ok()?;
        let direct = val_str(root.get("playlistId"));
        if !direct.is_empty() {
            return Some(direct);
        }
        find_string(&root, "playlistId")
    }

    pub async fn rename_remote_playlist(&self, playlist_id: &str, title: &str) -> bool {
        let name: String = title.trim().chars().take(150).collect();
        self.edit_remote_playlist(
            playlist_id,
            vec![json!({
                "action": "ACTION_SET_PLAYLIST_NAME",
                "playlistName": name,
            })],
        )
        .await
    }

    pub async fn delete_remote_playlist(&self, playlist_id: &str) -> bool {
        if !self.connection().connected || playlist_id.trim().is_empty() {
            return false;
        }
        self.ensure_config().await;
        let clean = playlist_id.strip_prefix("VL").unwrap_or(playlist_id);
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/playlist/delete?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "playlistId": clean,
        });
        self.post_json(
            &url,
            &body,
            "WEB_REMIX",
            &client_ver,
            WEB_USER_AGENT,
            true,
            None,
            None,
            None,
            2,
            None,
            None,
        )
        .await
        .is_ok()
    }

    pub async fn add_videos_to_remote_playlist(
        &self,
        playlist_id: &str,
        video_ids: &[String],
    ) -> bool {
        if video_ids.is_empty() {
            return true;
        }
        for chunk in video_ids.chunks(50) {
            let actions: Vec<Value> = chunk
                .iter()
                .map(|id| {
                    json!({
                        "action": "ACTION_ADD_VIDEO",
                        "addedVideoId": id,
                        "dedupeOption": "DEDUPE_OPTION_SKIP",
                    })
                })
                .collect();
            if !self.edit_remote_playlist(playlist_id, actions).await {
                return false;
            }
        }
        true
    }

    pub async fn remove_videos_from_remote_playlist(
        &self,
        playlist_id: &str,
        removals: &[(String, String)],
    ) -> bool {
        if removals.is_empty() {
            return true;
        }
        for chunk in removals.chunks(50) {
            let actions: Vec<Value> = chunk
                .iter()
                .map(|(set_video_id, video_id)| {
                    json!({
                        "action": "ACTION_REMOVE_VIDEO",
                        "setVideoId": set_video_id,
                        "removedVideoId": video_id,
                    })
                })
                .collect();
            if !self.edit_remote_playlist(playlist_id, actions).await {
                return false;
            }
        }
        true
    }

    async fn edit_remote_playlist(&self, playlist_id: &str, actions: Vec<Value>) -> bool {
        if !self.connection().connected || playlist_id.trim().is_empty() || actions.is_empty() {
            return false;
        }
        self.ensure_config().await;
        let clean = playlist_id.strip_prefix("VL").unwrap_or(playlist_id);
        let (api_key, client_ver, visitor) = self.current_web_config();
        let url = format!("{MUSIC_API}/browse/edit_playlist?key={api_key}&prettyPrint=false");
        let body = json!({
            "context": self.web_context(&client_ver, visitor.as_deref()),
            "playlistId": clean,
            "actions": actions,
        });
        let Ok(root) = self
            .post_json(
                &url,
                &body,
                "WEB_REMIX",
                &client_ver,
                WEB_USER_AGENT,
                true,
                None,
                None,
                None,
                2,
                None,
                None,
            )
            .await
        else {
            return false;
        };
        let status = val_str(root.get("status"));
        status.is_empty()
            || status.to_uppercase().contains("SUCCEEDED")
            || root.get("actions").is_some()
    }

    pub async fn fetch_owned_playlist_items(
        &self,
        playlist_id: &str,
        first_page_only: bool,
    ) -> YtOwnedPlaylist {
        if !self.connection().connected {
            return YtOwnedPlaylist {
                items: Vec::new(),
                complete: false,
                editable: false,
            };
        }
        let raw_id = Self::extract_playlist_id(playlist_id);
        if raw_id.is_empty() {
            return YtOwnedPlaylist {
                items: Vec::new(),
                complete: false,
                editable: false,
            };
        }
        let browse_id = Self::playlist_browse_id(&raw_id);
        let Some((root, root_auth)) = self.fetch_playlist_root(&browse_id).await else {
            return YtOwnedPlaylist {
                items: Vec::new(),
                complete: false,
                editable: false,
            };
        };

        let mut editable_headers = Vec::new();
        collect_objects(
            &root,
            "musicEditablePlaylistDetailHeaderRenderer",
            &mut editable_headers,
        );
        let editable = root_auth && !editable_headers.is_empty();

        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let absorb = |el: &Value,
                      items: &mut Vec<YtOwnedPlaylistItem>,
                      seen: &mut HashSet<String>| {
            let parsed: HashMap<String, YouTubeMusicTrack> = self
                .parse_song_renderers(el)
                .into_iter()
                .map(|s| (s.video_id.clone(), s))
                .collect();
            let mut renderers = Vec::new();
            collect_objects(el, "musicResponsiveListItemRenderer", &mut renderers);
            collect_objects(el, "playlistVideoRenderer", &mut renderers);
            for r in renderers {
                let video_id = r
                    .get("playlistItemData")
                    .and_then(|d| d.get("videoId"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| r.get("videoId").and_then(Value::as_str).map(str::to_string))
                    .or_else(|| find_string(&Value::Object(r.clone()), "videoId"))
                    .unwrap_or_default();
                if video_id.len() != 11 {
                    continue;
                }
                let set_video_id =
                    find_string(&Value::Object(r), "setVideoId").unwrap_or_default();
                let key = if !set_video_id.is_empty() {
                    set_video_id.clone()
                } else {
                    video_id.clone()
                };
                if !seen.insert(key) {
                    continue;
                }
                items.push(YtOwnedPlaylistItem {
                    track: parsed.get(&video_id).cloned(),
                    video_id,
                    set_video_id,
                });
            }
        };

        let playlist_page = browse_id.starts_with("VL");
        let containers = if playlist_page {
            self.playlist_track_containers(&root)
        } else {
            vec![root.clone()]
        };
        for c in &containers {
            absorb(c, &mut items, &mut seen);
        }

        let mut token = self.continuation_token(&containers, playlist_page);
        let mut seen_tokens = HashSet::new();
        let mut page = 0;
        let mut complete = true;
        if first_page_only && token.as_deref().is_some_and(|t| !t.is_empty()) {
            complete = false;
            token = None;
        }

        while let Some(tok) = token.take() {
            if tok.is_empty() || page >= MAX_CONTINUATION_PAGES {
                token = Some(tok);
                break;
            }
            if !seen_tokens.insert(tok.clone()) {
                break;
            }
            let Ok(next) = self.browse_continuation(&tok, true).await else {
                complete = false;
                break;
            };
            let page_containers = if playlist_page {
                self.playlist_track_containers(&next)
            } else {
                vec![next.clone()]
            };
            let effective = if page_containers.is_empty() {
                vec![next]
            } else {
                page_containers
            };
            for c in &effective {
                absorb(c, &mut items, &mut seen);
            }
            token = self.continuation_token(&effective, playlist_page);
            page += 1;
        }

        if token.as_deref().is_some_and(|t| !t.is_empty()) && page >= MAX_CONTINUATION_PAGES {
            complete = false;
        }
        let trustworthy = root_auth
            && if Self::is_liked_music_id(&raw_id) {
                !items.is_empty()
            } else {
                editable
            };

        YtOwnedPlaylist {
            items,
            complete: complete && trustworthy,
            editable,
        }
    }

    // Matching & Scoring

    pub fn normalize(s: &str) -> String {
        static RE_SINGLE_Q: OnceLock<Regex> = OnceLock::new();
        static RE_DOUBLE_Q: OnceLock<Regex> = OnceLock::new();
        static RE_DASHES: OnceLock<Regex> = OnceLock::new();
        static RE_DIACRITICS: OnceLock<Regex> = OnceLock::new();
        static RE_DOLLAR_DIGIT: OnceLock<Regex> = OnceLock::new();
        static RE_NON_WORD: OnceLock<Regex> = OnceLock::new();
        static RE_MULTI_SPACE: OnceLock<Regex> = OnceLock::new();

        let mut v = RE_SINGLE_Q
            .get_or_init(|| Regex::new(r"[\u2018\u2019\u201A\u201B`]").unwrap())
            .replace_all(s, "'")
            .into_owned();
        v = RE_DOUBLE_Q
            .get_or_init(|| Regex::new(r"[\u201C\u201D\u201E\u201F]").unwrap())
            .replace_all(&v, "\"")
            .into_owned();
        v = RE_DASHES
            .get_or_init(|| Regex::new(r"[\u2013\u2014\u2212]").unwrap())
            .replace_all(&v, "-")
            .into_owned();

        let nfd: String = v.to_lowercase().nfd().collect();
        v = RE_DIACRITICS
            .get_or_init(|| Regex::new(r"\p{M}+").unwrap())
            .replace_all(&nfd, "")
            .into_owned();
        v = RE_DOLLAR_DIGIT
            .get_or_init(|| Regex::new(r"\$\d").unwrap())
            .replace_all(&v, |caps: &regex::Captures<'_>| caps[0][1..].to_string())
            .into_owned();
        v = v.replace('$', "s");
        v = RE_NON_WORD
            .get_or_init(|| Regex::new(r"[^\p{L}\p{N}]+").unwrap())
            .replace_all(&v, " ")
            .into_owned();
        RE_MULTI_SPACE
            .get_or_init(|| Regex::new(r"\s+").unwrap())
            .replace_all(v.trim(), " ")
            .trim()
            .to_string()
    }

    fn tokens(value: &str) -> HashSet<String> {
        let noise: HashSet<&str> = MATCH_NOISE_WORDS.iter().copied().collect();
        Self::normalize(value)
            .split(' ')
            .filter(|t| !t.is_empty() && !noise.contains(*t))
            .map(str::to_string)
            .collect()
    }

    pub fn base_title(value: &str) -> String {
        static RE_FEAT: OnceLock<Regex> = OnceLock::new();
        static RE_VER: OnceLock<Regex> = OnceLock::new();
        let s1 = RE_FEAT
            .get_or_init(|| {
                Regex::new(r"(?i)[(\[]\s*(feat(?:uring)?|ft)\.?\s+.*?[)\]]").unwrap()
            })
            .replace_all(value, " ");
        RE_VER
            .get_or_init(|| {
                Regex::new(r"(?i)[(\[][^)\]]*(official|music\s*video|audio|video|visualizer|lyrics?|hd|hq|4k|live|remix|acoustic|demo|edit|remaster(?:ed)?|mono|stereo|deluxe|bonus|version|mix|extended|radio|explicit|clean|slowed|sped up|nightcore)[^)\]]*[)\]]").unwrap()
            })
            .replace_all(&s1, " ")
            .into_owned()
    }

    pub fn similarity(a: &str, b: &str) -> i32 {
        let norm_a = Self::normalize(a);
        let norm_b = Self::normalize(b);
        if norm_a == norm_b {
            return 100;
        }
        if norm_a.is_empty() || norm_b.is_empty() {
            return 0;
        }
        if norm_a.contains(&norm_b) || norm_b.contains(&norm_a) {
            let len_a = norm_a.chars().count();
            let len_b = norm_b.chars().count();
            let shorter = len_a.min(len_b);
            let longer = len_a.max(len_b);
            let ratio = ((shorter * 100) / longer) as i32;
            if shorter >= 8 && ratio >= 70 {
                return ratio.max(85);
            }
        }
        let left = Self::tokens(a);
        let right = Self::tokens(b);
        if left.is_empty() || right.is_empty() {
            return 0;
        }
        let common = left.intersection(&right).count();
        let dice = ((200 * common) / (left.len() + right.len())) as i32;

        let mut subset = 0;
        let shorter_count = left.len().min(right.len());
        if common == shorter_count && common > 0 {
            let variant = MatchVocab::version_words();
            let noise: HashSet<&str> = MATCH_NOISE_WORDS.iter().copied().collect();
            let all_allowed = left
                .union(&right)
                .filter(|w| !left.contains(*w) || !right.contains(*w))
                .all(|w| {
                    variant.contains(w.as_str())
                        || noise.contains(w.as_str())
                        || is_digits(w)
                });
            if all_allowed {
                subset = 80;
            }
        }
        dice.max(subset)
    }

    pub fn is_safe_title_match(candidate_title: &str, wanted_title: &str, artist: &str) -> bool {
        let norm_wanted = Self::normalize(wanted_title);
        let norm_candidate = Self::normalize(candidate_title);
        if norm_wanted == norm_candidate {
            return true;
        }
        if norm_wanted.is_empty() || norm_candidate.is_empty() {
            return false;
        }

        let mut clean_candidate = norm_candidate;
        if !artist.trim().is_empty() {
            let norm_artist = Self::normalize(artist);
            if !norm_artist.is_empty() && clean_candidate.starts_with(&norm_artist) {
                clean_candidate = clean_candidate[norm_artist.len()..]
                    .trim()
                    .replace('-', "")
                    .trim()
                    .to_string();
            }
        }
        if clean_candidate == norm_wanted {
            return true;
        }

        let base_wanted = Self::normalize(&Self::base_title(wanted_title));
        let base_candidate = Self::normalize(&Self::base_title(&clean_candidate));
        if !base_wanted.is_empty() && base_wanted == base_candidate {
            return true;
        }

        let wanted_tokens = Self::tokens(wanted_title);
        let candidate_tokens = Self::tokens(&clean_candidate);
        if wanted_tokens.is_empty() || candidate_tokens.is_empty() {
            return false;
        }

        let common_count = wanted_tokens.intersection(&candidate_tokens).count();
        if common_count == wanted_tokens.len() || common_count == candidate_tokens.len() {
            let variant = MatchVocab::version_words();
            let noise: HashSet<&str> = MATCH_NOISE_WORDS.iter().copied().collect();
            let artist_tokens = if artist.trim().is_empty() {
                HashSet::new()
            } else {
                Self::tokens(artist)
            };
            let ok = wanted_tokens
                .union(&candidate_tokens)
                .filter(|w| !wanted_tokens.contains(*w) || !candidate_tokens.contains(*w))
                .all(|w| {
                    variant.contains(w.as_str())
                        || noise.contains(w.as_str())
                        || artist_tokens.contains(w.as_str())
                        || is_digits(w)
                });
            if ok {
                return true;
            }
        }

        let sim = Self::similarity(&clean_candidate, wanted_title)
            .max(Self::similarity(&base_candidate, &base_wanted));
        sim >= 65
    }

    fn feat_part(title: &str) -> String {
        static RE_FEAT_CLAUSE: OnceLock<Regex> = OnceLock::new();
        static RE_TRAILING_FEAT: OnceLock<Regex> = OnceLock::new();
        static RE_LEAD_OPEN: OnceLock<Regex> = OnceLock::new();
        static RE_TRAIL_CLOSE: OnceLock<Regex> = OnceLock::new();
        static RE_FEAT_KW: OnceLock<Regex> = OnceLock::new();
        static RE_TRAIL_KW: OnceLock<Regex> = OnceLock::new();

        let re1 = RE_FEAT_CLAUSE.get_or_init(|| {
            Regex::new(r"(?i)[\(\[]\s*(feat(?:uring)?|ft|with)\.?\s+[^\)\]]*[\)\]]").unwrap()
        });
        if let Some(m) = re1.find(title) {
            let inner = RE_LEAD_OPEN
                .get_or_init(|| Regex::new(r"^[\(\[]\s*").unwrap())
                .replace(m.as_str(), "");
            let inner = RE_TRAIL_CLOSE
                .get_or_init(|| Regex::new(r"[\)\]]\s*$").unwrap())
                .replace(&inner, "");
            let inner = RE_FEAT_KW
                .get_or_init(|| Regex::new(r"(?i)^(?:feat(?:uring)?|ft|with)\.?\s+").unwrap())
                .replace(&inner, "");
            return Self::normalize(&inner);
        }

        let re2 = RE_TRAILING_FEAT
            .get_or_init(|| Regex::new(r"(?i)\s+(?:feat(?:uring)?|ft)\.?\s+.+$").unwrap());
        if let Some(m) = re2.find(title) {
            let tail = RE_TRAIL_KW
                .get_or_init(|| Regex::new(r"(?i)^\s+(?:feat(?:uring)?|ft)\.?\s+").unwrap())
                .replace(m.as_str(), "");
            return Self::normalize(&tail);
        }
        String::new()
    }

    fn feat_bonus(candidate_title: &str, wanted_title: &str) -> i32 {
        let c = Self::feat_part(candidate_title);
        let w = Self::feat_part(wanted_title);
        if w.is_empty() && c.is_empty() {
            return 0;
        }
        if !w.is_empty() && !c.is_empty() {
            if c == w {
                return 300;
            }
            let ct: HashSet<&str> = c.split(' ').collect();
            let wt: HashSet<&str> = w.split(' ').collect();
            if ct.intersection(&wt).next().is_some() {
                return -100;
            }
            return -600;
        }
        if !w.is_empty() {
            -350
        } else {
            -250
        }
    }

    pub fn match_score(candidate: &YouTubeMusicTrack, title: &str, artist: &str) -> i32 {
        let wanted_title = Self::normalize(title);
        let wanted_artist = Self::normalize(artist);
        let candidate_title = Self::normalize(&candidate.title);
        let candidate_artist = Self::normalize(&candidate.artist);
        let title_sim = Self::similarity(&candidate.title, title)
            .max(Self::similarity(&Self::base_title(&candidate.title), &Self::base_title(title)));
        let artist_sim = Self::similarity(&candidate.artist, artist);
        let mut score = title_sim * 5 + artist_sim * 3;
        if candidate_title == wanted_title {
            score += 600;
        }
        if !wanted_artist.is_empty() && candidate_artist == wanted_artist {
            score += 350;
        }
        let variant = MatchVocab::version_words();
        let wanted_variants: HashSet<String> = Self::tokens(title)
            .into_iter()
            .filter(|w| variant.contains(w.as_str()))
            .collect();
        let unexpected_count = Self::tokens(&candidate.title)
            .into_iter()
            .filter(|w| variant.contains(w.as_str()) && !wanted_variants.contains(w))
            .count() as i32;
        score -= unexpected_count * 250;
        score += Self::feat_bonus(&candidate.title, title);
        if !wanted_artist.is_empty() {
            let artist_contains = candidate_artist.contains(&wanted_artist)
                || wanted_artist.contains(&candidate_artist);
            let title_contains_artist = candidate_title.contains(&wanted_artist);
            if artist_sim < 35 && !artist_contains && !title_contains_artist {
                score -= 1000;
            }
        }
        if title_sim < 50 {
            score -= 1500;
        }
        score
    }

    fn passes_gates(c: &YouTubeMusicTrack, title: &str, clean_artist: &str) -> bool {
        if !Self::is_safe_title_match(&c.title, title, clean_artist) {
            return false;
        }
        if clean_artist.trim().is_empty() {
            return true;
        }
        let norm_c = Self::normalize(&c.artist);
        let norm_w = Self::normalize(clean_artist);
        Self::similarity(&c.artist, clean_artist) >= 30
            || (!norm_w.is_empty() && (norm_c.contains(&norm_w) || norm_w.contains(&norm_c)))
            || Self::normalize(&c.title).contains(&norm_w)
    }

    fn cached_row_still_faithful(
        cached: &YouTubeMusicTrack,
        title: &str,
        clean_artist: &str,
    ) -> bool {
        if !Self::passes_gates(cached, title, clean_artist) {
            return false;
        }
        let variant = MatchVocab::version_words();
        let wanted_has_variant = Self::tokens(title)
            .iter()
            .any(|w| variant.contains(w.as_str()));
        let cached_has_variant = Self::tokens(&cached.title)
            .iter()
            .any(|w| variant.contains(w.as_str()));
        if !wanted_has_variant && cached_has_variant {
            return false;
        }
        if Self::feat_bonus(&cached.title, title) <= -600 {
            return false;
        }
        true
    }

    pub async fn find_best_match(
        &self,
        title: &str,
        artist: &str,
        excluded_video_ids: &HashSet<String>,
    ) -> Result<YouTubeMusicTrack, InnerTubeError> {
        let trimmed_a = artist.trim();
        let clean_artist = if trimmed_a.is_empty()
            || trimmed_a.eq_ignore_ascii_case("unknown artist")
        {
            ""
        } else {
            artist
        };
        let cache_key = format!(
            "v2|{}|{}",
            Self::normalize(clean_artist),
            Self::normalize(title)
        );

        let mem_hit = self
            .match_cache
            .read()
            .ok()
            .and_then(|g| g.get(&cache_key).cloned());
        if let Some(cached) = mem_hit {
            if !excluded_video_ids.contains(&cached.video_id) && cached.duration_seconds > 0 {
                if Self::cached_row_still_faithful(&cached, title, clean_artist) {
                    if let Ok(mut g) = self.match_cache.write() {
                        g.insert_lru(cache_key, cached.clone(), MAX_MATCH_CACHE_ENTRIES);
                    }
                    return Ok(cached);
                }
                if let Ok(mut g) = self.match_cache.write() {
                    g.remove(&cache_key);
                }
                if let Some(disk) = self.current_disk() {
                    disk.delete_matches_for_video(&cached.video_id);
                }
            }
        }

        if excluded_video_ids.is_empty() {
            if let Some(disk) = self.current_disk() {
                if let Some(row) = disk.load_match_entry(&cache_key) {
                    if !row.video_id.is_empty() {
                        let track = YouTubeMusicTrack {
                            video_id: row.video_id.clone(),
                            title: if row.title.is_empty() {
                                title.to_string()
                            } else {
                                row.title
                            },
                            artist: if row.artist.is_empty() {
                                artist.to_string()
                            } else {
                                row.artist
                            },
                            album: row.album,
                            artwork_url: row.artwork_url,
                            duration_seconds: row.duration_seconds,
                            album_browse_id: String::new(),
                        };
                        if row.duration_seconds > 0
                            && Self::cached_row_still_faithful(&track, title, clean_artist)
                        {
                            if let Ok(mut g) = self.match_cache.write() {
                                g.insert_lru(cache_key, track.clone(), MAX_MATCH_CACHE_ENTRIES);
                            }
                            return Ok(track);
                        } else if row.duration_seconds > 0 {
                            disk.delete_matches_for_video(&row.video_id);
                        }
                    }
                }
            }
        }

        let query = format!("{title} {artist}");
        let results = self.search_songs(&query, 30).await?;
        let pool: Vec<YouTubeMusicTrack> = results
            .into_iter()
            .filter(|c| !c.video_id.is_empty() && !excluded_video_ids.contains(&c.video_id))
            .collect();

        let variant = MatchVocab::version_words();
        let wanted_has_variant = Self::tokens(title)
            .iter()
            .any(|w| variant.contains(w.as_str()));
        let preferred: Vec<&YouTubeMusicTrack> = if !wanted_has_variant {
            let faithful: Vec<&YouTubeMusicTrack> = pool
                .iter()
                .filter(|c| {
                    !Self::tokens(&c.title)
                        .iter()
                        .any(|w| variant.contains(w.as_str()))
                })
                .collect();
            if faithful
                .iter()
                .any(|c| Self::passes_gates(c, title, clean_artist))
            {
                faithful
            } else {
                pool.iter().collect()
            }
        } else {
            pool.iter().collect()
        };

        let mut best: Option<&YouTubeMusicTrack> = None;
        let mut best_score = i32::MIN;
        for c in preferred {
            if !Self::passes_gates(c, title, clean_artist) {
                continue;
            }
            let score = Self::match_score(c, title, clean_artist);
            if score > best_score {
                best_score = score;
                best = Some(c);
            }
        }

        let Some(best_track) = best.cloned() else {
            return Err(InnerTubeError::NoReliableMatch(format!(
                "No reliable YouTube Music match found for {title} by {artist}"
            )));
        };

        if let Ok(mut g) = self.match_cache.write() {
            g.insert_lru(
                cache_key.clone(),
                best_track.clone(),
                MAX_MATCH_CACHE_ENTRIES,
            );
        }
        if let Some(disk) = self.current_disk() {
            disk.save_match_entry(
                &cache_key,
                &best_track.video_id,
                &best_track.title,
                &best_track.artist,
                &best_track.album,
                &best_track.artwork_url,
                best_track.duration_seconds,
            );
        }
        Ok(best_track)
    }

    pub async fn find_best_match_or_null(
        &self,
        title: &str,
        artist: &str,
        excluded_video_ids: &HashSet<String>,
    ) -> Option<YouTubeMusicTrack> {
        self.find_best_match(title, artist, excluded_video_ids)
            .await
            .ok()
    }

    pub fn primary_artist_for_match(artist: &str) -> String {
        static RE_FEAT_SPLIT: OnceLock<Regex> = OnceLock::new();
        static RE_COMMA_THE_AND: OnceLock<Regex> = OnceLock::new();
        static RE_COMMA_AMP: OnceLock<Regex> = OnceLock::new();

        let head = artist.split(';').next().unwrap_or("").trim();
        if head.is_empty() {
            return String::new();
        }
        let s = RE_FEAT_SPLIT
            .get_or_init(|| Regex::new(r"(?i)\s+[(\[]?(?:feat\.?|ft\.?|featuring)\b").unwrap())
            .split(head)
            .next()
            .unwrap_or("")
            .trim();
        if s.is_empty() {
            return String::new();
        }
        if RE_COMMA_THE_AND
            .get_or_init(|| Regex::new(r"(?i),\s*(the|and)\b").unwrap())
            .is_match(s)
            && !s.contains(" & ")
        {
            return s.to_string();
        }
        let lower = s.to_lowercase();
        if lower == "unknown artist" || lower == "various artists" || lower == "unknown" {
            return String::new();
        }
        RE_COMMA_AMP
            .get_or_init(|| Regex::new(r"\s*[,&]\s*").unwrap())
            .split(s)
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    }

    fn has_known_artist(artist: &str) -> bool {
        let norm = Self::normalize(artist);
        !norm.is_empty()
            && norm != "unknown artist"
            && norm != "various artists"
            && norm != "unknown"
    }

    fn candidate_shares_artist(c: &YouTubeMusicTrack, artist: &str) -> bool {
        if !Self::has_known_artist(artist) {
            return true;
        }
        let primary = Self::primary_artist_for_match(artist);
        let check_artist = if primary.is_empty() { artist } else { &primary };
        let norm_c_artist = Self::normalize(&c.artist);
        let norm_c_title = Self::normalize(&c.title);
        for candidate_a in [artist, check_artist] {
            let norm_w = Self::normalize(candidate_a);
            if norm_w.is_empty() {
                continue;
            }
            if Self::similarity(&c.artist, candidate_a) >= 30
                || norm_c_artist.contains(&norm_w)
                || norm_w.contains(&norm_c_artist)
                || norm_c_title.contains(&norm_w)
            {
                return true;
            }
        }
        let stop = ["the", "and", "feat", "ft", "featuring", "with"];
        let artist_tokens: HashSet<String> = Self::tokens(artist)
            .into_iter()
            .filter(|t| t.chars().count() >= 2 && !stop.contains(&t.as_str()))
            .collect();
        if artist_tokens.is_empty() {
            return false;
        }
        let hay = Self::tokens(&format!("{} {}", c.artist, c.title));
        artist_tokens.intersection(&hay).next().is_some()
    }

    pub async fn find_best_effort_match_or_null(
        &self,
        title: &str,
        artist: &str,
        excluded_video_ids: &HashSet<String>,
    ) -> Option<YouTubeMusicTrack> {
        if title.trim().is_empty() {
            PlayDiag::log(&format!(
                "match MISS all-tiers (empty title) artist=\"{artist}\""
            ));
            return None;
        }
        if let Ok(Ok(strict)) = tokio::time::timeout(
            Duration::from_secs(5),
            self.find_best_match(title, artist, excluded_video_ids),
        )
        .await
        {
            PlayDiag::log(&format!(
                "match HIT strict \"{title}\" — \"{}\" [{}] by \"{}\"",
                strict.title, strict.video_id, strict.artist
            ));
            return Some(strict);
        }

        let primary = Self::primary_artist_for_match(artist);
        if !primary.is_empty() && Self::normalize(&primary) != Self::normalize(artist) {
            if let Ok(Ok(m)) = tokio::time::timeout(
                Duration::from_secs(5),
                self.find_best_match(title, &primary, excluded_video_ids),
            )
            .await
            {
                PlayDiag::log(&format!(
                    "match HIT primary \"{title}\" — \"{}\" [{}] (as \"{primary}\")",
                    m.title, m.video_id
                ));
                return Some(m);
            }
        }

        let known_artist = Self::has_known_artist(artist);
        // Never fall back to blank-artist matching during retry after playback
        // failure when the track has a known artist, and always reject covers
        // from unrelated artists.
        if !known_artist || excluded_video_ids.is_empty() {
            if let Ok(Ok(m)) = tokio::time::timeout(
                Duration::from_secs(5),
                self.find_best_match(title, "", excluded_video_ids),
            )
            .await
            {
                if Self::candidate_shares_artist(&m, artist) {
                    PlayDiag::log(&format!(
                        "match HIT title-only \"{title}\" — \"{}\" [{}] by \"{}\"",
                        m.title, m.video_id, m.artist
                    ));
                    return Some(m);
                }
                PlayDiag::log(&format!(
                    "match REJECT title-only \"{title}\" — \"{}\" [{}] by \"{}\" (wanted \"{artist}\")",
                    m.title, m.video_id, m.artist
                ));
            }
        }

        let core = Self::core_song_title(title, artist);
        if !core.is_empty() && Self::normalize(&core) != Self::normalize(title) {
            let candidates: Vec<&str> = if !known_artist || excluded_video_ids.is_empty() {
                vec![if !primary.is_empty() { &primary } else { artist }, ""]
            } else {
                vec![if !primary.is_empty() { &primary } else { artist }]
            };
            for a in candidates {
                if let Ok(Ok(m)) = tokio::time::timeout(
                    Duration::from_secs(5),
                    self.find_best_match(&core, a, excluded_video_ids),
                )
                .await
                {
                    if !a.is_empty() || Self::candidate_shares_artist(&m, artist) {
                        PlayDiag::log(&format!(
                            "match HIT core \"{core}\" — \"{}\" [{}]",
                            m.title, m.video_id
                        ));
                        return Some(m);
                    }
                }
            }

            let q = format!("{core} {primary}");
            if let Ok(Ok(pool)) =
                tokio::time::timeout(Duration::from_secs(6), self.search_mixed(&q, 20)).await
            {
                let core_tokens = Self::tokens(&core);
                let artist_tokens = Self::tokens(if !primary.is_empty() { &primary } else { artist });
                for c in pool {
                    if c.video_id.is_empty() || excluded_video_ids.contains(&c.video_id) {
                        continue;
                    }
                    let hay = Self::tokens(&format!("{} {}", c.title, c.artist));
                    if !core_tokens.is_empty()
                        && core_tokens.is_subset(&hay)
                        && (artist_tokens.is_empty()
                            || artist_tokens.intersection(&hay).next().is_some())
                    {
                        PlayDiag::log(&format!(
                            "match HIT mixed \"{core}\" — \"{}\" [{}]",
                            c.title, c.video_id
                        ));
                        return Some(c);
                    }
                }
            }
        }
        None
    }

    pub fn core_song_title(title: &str, artist: &str) -> String {
        static RE_HANDLE: OnceLock<Regex> = OnceLock::new();
        static RE_NOISE: OnceLock<Regex> = OnceLock::new();
        static RE_FEAT: OnceLock<Regex> = OnceLock::new();
        static RE_DASH: OnceLock<Regex> = OnceLock::new();
        static RE_SPACES: OnceLock<Regex> = OnceLock::new();

        let s1 = RE_HANDLE
            .get_or_init(|| Regex::new(r"@\w+").unwrap())
            .replace_all(title, " ");
        let s2 = RE_NOISE
            .get_or_init(|| {
                Regex::new(r"(?i)[\(\[][^\)\]]*(?:video|official|lyric|audio|visuali[sz]er|amv|\bmv\b|remaster|hd|4k)[^\)\]]*[\)\]]").unwrap()
            })
            .replace_all(&s1, " ");
        let mut s = RE_FEAT
            .get_or_init(|| Regex::new(r"(?i)\s+[(\[]?(?:feat\.?|ft\.?|featuring)\b").unwrap())
            .split(&s2)
            .next()
            .unwrap_or("")
            .to_string();

        if let Some(m) = RE_DASH
            .get_or_init(|| Regex::new(r"\s[-–—]\s").unwrap())
            .find(&s)
        {
            let head = &s[..m.start()];
            let a = Self::normalize(artist);
            let h = Self::normalize(head);
            if !h.is_empty() && !a.is_empty() && (a.contains(&h) || h.contains(&a)) {
                s = s[m.end()..].to_string();
            }
        }
        RE_SPACES
            .get_or_init(|| Regex::new(r"\s+").unwrap())
            .replace_all(&s, " ")
            .trim()
            .to_string()
    }

    pub fn remember_match(
        &self,
        track: &YouTubeMusicTrack,
        title: Option<&str>,
        artist: Option<&str>,
    ) {
        if track.video_id.is_empty() {
            return;
        }
        let cache_key = format!(
            "v2|{}|{}",
            Self::normalize(artist.unwrap_or(&track.artist)),
            Self::normalize(title.unwrap_or(&track.title))
        );
        if let Ok(mut g) = self.match_cache.write() {
            g.insert_lru(cache_key.clone(), track.clone(), MAX_MATCH_CACHE_ENTRIES);
        }
        if let Some(disk) = self.current_disk() {
            disk.save_match_entry(
                &cache_key,
                &track.video_id,
                &track.title,
                &track.artist,
                &track.album,
                &track.artwork_url,
                track.duration_seconds,
            );
        }
    }

    pub async fn is_playable(&self, title: &str, artist: &str) -> bool {
        let empty = HashSet::new();
        self.find_best_effort_match_or_null(title, artist, &empty)
            .await
            .is_some()
    }

    pub fn invalidate_cache(&self, video_id: &str) {
        let prefix = format!("{video_id}|");
        if let Ok(mut g) = self.stream_cache.write() {
            g.retain(|k, _| !k.starts_with(&prefix));
        }
        if let Ok(mut g) = self.match_cache.write() {
            g.retain(|_, v| v.video_id != video_id);
        }
        if let Some(disk) = self.current_disk() {
            disk.delete_stream_entries(video_id);
            disk.delete_matches_for_video(video_id);
        }
    }

    // Stream Resolution & Caching

    pub fn prefetch_stream(self: &Arc<Self>, video_id: &str) {
        self.prefetch_next_track(video_id);
    }

    /// Background pre-resolve for the next queue track, bounded by a 120s
    /// deadline and deduplicated by single-flight inflight map.
    pub fn prefetch_next_track(self: &Arc<Self>, video_id: &str) {
        if video_id.is_empty() {
            return;
        }
        if self.peek_cached_stream(video_id, false).is_some() {
            return;
        }
        let me = Arc::clone(self);
        let vid = video_id.to_string();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_secs(PREFETCH_DEADLINE_SECS),
                me.resolve_audio_stream(&vid, false, false),
            )
            .await;
        });
    }

    pub fn peek_cached_stream(&self, video_id: &str, prefer_smallest: bool) -> Option<ResolvedStream> {
        if video_id.is_empty() {
            return None;
        }
        let auth_scope = self.playback_auth_scope();
        let now_ms = now_unix_ms();
        let guard = self.stream_cache.read().ok()?;
        let mut freshest: Option<&CachedStream> = None;
        for e in guard.values() {
            if e.stream.url.is_empty() || !e.stream.cache_key.contains(video_id) {
                continue;
            }
            let is_min = e.stream.cache_key.starts_with("youtube-min:");
            if is_min != prefer_smallest {
                continue;
            }
            if e.auth_scope != auth_scope {
                continue;
            }
            if !Self::is_fresh(e, now_ms) {
                continue;
            }
            if freshest.is_none_or(|prev| e.cached_at_ms > prev.cached_at_ms) {
                freshest = Some(e);
            }
        }
        freshest.map(|e| e.stream.clone())
    }

    pub async fn resolve_audio_stream(
        &self,
        video_id: &str,
        force_refresh: bool,
        prefer_smallest: bool,
    ) -> Option<ResolvedStream> {
        if video_id.is_empty() {
            return None;
        }
        let auth_scope = self.playback_auth_scope();
        let now_ms = now_unix_ms();

        if !force_refresh {
            if let Some(peek) = self.peek_cached_stream(video_id, prefer_smallest) {
                let mut hit_client = String::new();
                let mut hit_itag: i64 = -1;
                let parts: Vec<&str> = peek.cache_key.split(':').collect();
                if parts.len() >= 5 && (parts[0] == "youtube" || parts[0] == "youtube-min") {
                    hit_client = parts[2].to_string();
                    hit_itag = parts[3].parse::<i64>().unwrap_or(-1);
                }
                log_stream(
                    "cache-hit",
                    video_id,
                    &hit_client,
                    hit_itag,
                    &peek.mime_type,
                    Self::expiry_state(peek.expires_at),
                    0,
                    0,
                    "",
                );
                return Some(peek);
            }
            if let Ok(mut g) = self.stream_cache.write() {
                g.retain(|_, c| {
                    !(c.stream.cache_key.contains(video_id) && !Self::is_fresh(c, now_ms))
                });
            }
        } else if let Ok(mut g) = self.stream_cache.write() {
            let prefix = format!("{video_id}|");
            g.retain(|k, _| !k.starts_with(&prefix));
        }

        let tier = if prefer_smallest { "min" } else { "max" };
        let request_key = format!("{video_id}|{auth_scope}|{tier}");

        let existing_rx = {
            let inflight = self.inflight.lock().await;
            inflight.get(&request_key).cloned()
        };
        if let Some(mut rx) = existing_rx {
            while rx.borrow().is_none() {
                if rx.changed().await.is_err() {
                    break;
                }
            }
            return rx.borrow().as_ref().and_then(|res| res.clone());
        }

        let (tx, rx) = watch::channel(None);
        {
            let mut inflight = self.inflight.lock().await;
            inflight.insert(request_key.clone(), rx);
        }

        let resolved = self
            .resolve_audio_stream_internal(video_id, &auth_scope, prefer_smallest)
            .await;

        let _ = tx.send(Some(resolved.clone()));
        {
            let mut inflight = self.inflight.lock().await;
            inflight.remove(&request_key);
        }
        resolved
    }

    async fn resolve_audio_stream_internal(
        &self,
        video_id: &str,
        auth_scope: &str,
        prefer_smallest: bool,
    ) -> Option<ResolvedStream> {
        let _ = tokio::time::timeout(Duration::from_secs(4), self.ensure_config()).await;
        let visitor_data = self.current_web_config().2;

        let mut script: Option<PlayerScript> = None;
        let mut sts: Option<i64> = None;
        let mut player_po_token: Option<String> = None;
        let mut gvs_po_token: Option<String> = None;
        let mut aux_ready = false;

        let available = self.ordered_fast_clients(video_id);
        if available.is_empty() {
            log_stream(
                "resolve-failed",
                video_id,
                "",
                -1,
                "",
                "unknown",
                0,
                0,
                "all clients cooling down",
            );
            return None;
        }

        for client in available {
            if client.requires_cipher() && !aux_ready {
                aux_ready = true;
                let (scr_res, po_res) = tokio::join!(
                    tokio::time::timeout(Duration::from_secs(8), self.decipher.script_for(video_id)),
                    tokio::time::timeout(
                        Duration::from_secs(9),
                        self.mint_po_token(video_id, false)
                    )
                );
                script = scr_res.ok().flatten();
                sts = script.as_ref().and_then(|s| s.sts);
                if let Ok(Some(po)) = po_res {
                    if !po.player_token.is_empty() {
                        player_po_token = Some(po.player_token);
                    }
                    if !po.session_token.is_empty() {
                        gvs_po_token = Some(po.session_token);
                    }
                }
            }

            let gvs_arg = if visitor_data.is_some() {
                gvs_po_token.as_deref()
            } else {
                None
            };
            let res = tokio::time::timeout(
                FAST_CLIENT_TIMEOUT,
                self.resolve_direct_client_stream(
                    video_id,
                    client,
                    visitor_data.as_deref(),
                    sts,
                    script.as_ref(),
                    player_po_token.as_deref(),
                    gvs_arg,
                    auth_scope,
                    prefer_smallest,
                ),
            )
            .await;

            match res {
                Ok(Ok(candidate)) => {
                    if let Ok(mut g) = self.last_successful_client_name.write() {
                        *g = Some(client.name.to_string());
                    }
                    let ckey = client.key();
                    self.cache_resolved_stream(
                        video_id,
                        &candidate,
                        &ckey,
                        auth_scope,
                        now_unix_ms(),
                        prefer_smallest,
                    );
                    if let Ok(mut g) = self.last_resolved.write() {
                        g.insert_lru(
                            video_id.to_string(),
                            ResolvedRef {
                                client_profile: ckey.clone(),
                                auth_scope: auth_scope.to_string(),
                            },
                            MAX_LAST_RESOLVED_ENTRIES,
                        );
                    }
                    log_stream(
                        "resolved",
                        video_id,
                        &ckey,
                        candidate.itag,
                        &candidate.stream.mime_type,
                        Self::expiry_state(candidate.stream.expires_at),
                        0,
                        0,
                        "",
                    );
                    return Some(candidate.stream);
                }
                Ok(Err(InnerTubeError::ConfirmedUnplayable(msg))) => {
                    let ckey = client.key();
                    self.cool_client(video_id, &ckey, Some(auth_scope));
                    DiagnosticsLog::info(&format!(
                        "stream_resolve_fail video_id={video_id} client={ckey} confirmed_unplayable={}",
                        safe_error_detail(&msg)
                    ));
                    log_stream(
                        "client-resolve-failed",
                        video_id,
                        &ckey,
                        -1,
                        "",
                        "unknown",
                        0,
                        0,
                        &safe_error_detail(&msg),
                    );
                    return None;
                }
                Ok(Err(err)) => {
                    let ckey = client.key();
                    self.cool_client(video_id, &ckey, Some(auth_scope));
                    DiagnosticsLog::info(&format!(
                        "stream_resolve_fail video_id={video_id} client={ckey} err={}",
                        safe_error_detail(&err.to_string())
                    ));
                    log_stream(
                        "client-resolve-failed",
                        video_id,
                        &ckey,
                        -1,
                        "",
                        "unknown",
                        0,
                        0,
                        &safe_error_detail(&err.to_string()),
                    );
                }
                Err(_) => {
                    let ckey = client.key();
                    self.cool_client(video_id, &ckey, Some(auth_scope));
                    log_stream(
                        "client-resolve-failed",
                        video_id,
                        &ckey,
                        -1,
                        "",
                        "unknown",
                        0,
                        0,
                        "client timeout",
                    );
                }
            }
        }

        None
    }

    pub async fn resolve_music_video(&self, video_id: &str) -> Vec<MusicVideoStream> {
        if video_id.is_empty() {
            return Vec::new();
        }
        let _ = tokio::time::timeout(Duration::from_secs(4), self.ensure_config()).await;
        let pot = tokio::time::timeout(Duration::from_secs(6), self.mint_po_token(video_id, false))
            .await
            .ok()
            .flatten()
            .map(|p| p.session_token)
            .filter(|s| !s.is_empty());

        let mut script: Option<PlayerScript> = None;
        let clients = self.ordered_fast_clients(video_id);
        for client in clients.into_iter().take(3) {
            if client.requires_cipher() && script.is_none() {
                script = tokio::time::timeout(
                    Duration::from_secs(8),
                    self.decipher.script_for(video_id),
                )
                .await
                .ok()
                .flatten();
            }
            if let Ok(Ok(options)) = tokio::time::timeout(
                FAST_CLIENT_TIMEOUT,
                self.music_video_options(video_id, client, script.as_ref(), pot.as_deref()),
            )
            .await
            {
                if !options.is_empty() {
                    return options;
                }
            }
        }
        Vec::new()
    }

    async fn music_video_options(
        &self,
        video_id: &str,
        client: &PlayerClient,
        script: Option<&PlayerScript>,
        gvs_po_token: Option<&str>,
    ) -> Result<Vec<MusicVideoStream>, InnerTubeError> {
        let visitor = self.current_web_config().2;
        let api_base = if client.name == "WEB_REMIX" {
            MUSIC_API
        } else {
            YOUTUBE_API
        };
        let url = format!("{api_base}/player?key={}&prettyPrint=false", client.api_key);
        let mut body = json!({
            "context": self.build_context(
                client.name,
                client.version,
                visitor.as_deref(),
                client.os_name,
                client.os_version,
                client.device_make,
                client.device_model,
                client.android_sdk_version,
                None,
            ),
            "videoId": video_id,
            "contentCheckOk": true,
            "racyCheckOk": true,
        });
        if let Some(sts) = script.and_then(|s| s.sts) {
            if let Some(obj) = body.as_object_mut() {
                obj.insert(
                    "playbackContext".to_string(),
                    json!({ "contentPlaybackContext": { "signatureTimestamp": sts } }),
                );
            }
        }

        let origin = client.origin();
        let referer = client.referer();
        let root = self
            .post_json(
                &url,
                &body,
                client.name,
                client.version,
                client.user_agent,
                client.name == "WEB_REMIX" && self.connection().connected,
                Some(origin),
                Some(&referer),
                visitor.as_deref(),
                MAX_PLAYER_REQUEST_ATTEMPTS,
                None,
                None,
            )
            .await?;

        if root
            .get("playabilityStatus")
            .and_then(|s| s.get("status"))
            .and_then(Value::as_str)
            != Some("OK")
        {
            return Ok(Vec::new());
        }

        let mut headers = client.stream_request_headers();
        if let Some(v) = visitor.as_deref().filter(|s| !s.is_empty()) {
            headers.insert("X-Goog-Visitor-Id".to_string(), v.to_string());
        }

        let streaming = root.get("streamingData");
        let mut formats = Vec::new();
        for key in ["formats", "adaptiveFormats"] {
            if let Some(arr) = streaming.and_then(|s| s.get(key)).and_then(Value::as_array) {
                for f in arr {
                    if let Some(obj) = f.as_object() {
                        formats.push(obj);
                    }
                }
            }
        }

        let mut by_height: BTreeMap<i32, MusicVideoStream> = BTreeMap::new();
        for format in formats {
            let mime = val_str(format.get("mimeType"));
            let clean_mime = mime
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_lowercase();
            if !clean_mime.starts_with("video/") {
                continue;
            }
            let raw_url = format
                .get("url")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    let cipher = format
                        .get("signatureCipher")
                        .or_else(|| format.get("cipher"))
                        .and_then(Value::as_str)?;
                    self.decipher.decipher_url(cipher, script)
                });
            let Some(mut stream_url) = raw_url.filter(|u| !u.is_empty()) else {
                continue;
            };
            stream_url = Self::append_pot(&stream_url, gvs_po_token);
            let height = format
                .get("height")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32;
            if height < 144 {
                continue;
            }
            let bitrate = format
                .get("bitrate")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32;
            let label_raw = val_str(format.get("qualityLabel"));
            let label = if label_raw.is_empty() {
                format!("{height}p")
            } else {
                label_raw
            };
            let next = MusicVideoStream {
                label,
                height,
                bitrate,
                url: stream_url,
                mime_type: clean_mime,
                request_headers: headers.clone(),
            };
            let replace = match by_height.get(&height) {
                None => true,
                Some(prev) => prefer_video(&next, prev),
            };
            if replace {
                by_height.insert(height, next);
            }
        }

        Ok(by_height.into_values().rev().take(5).collect())
    }

    fn ordered_fast_clients(&self, video_id: &str) -> Vec<&'static PlayerClient> {
        let mut order: Vec<&'static PlayerClient> = Vec::new();
        let mut add = |name: &str| {
            for c in PLAYER_CLIENTS {
                let ckey = c.key();
                if c.name == name
                    && !self.is_cooling(video_id, &ckey)
                    && !order.iter().any(|e| e.key() == ckey)
                {
                    order.push(c);
                    break;
                }
            }
        };

        add(FAST_PREFERRED_CLIENT);
        if let Ok(g) = self.last_successful_client_name.read() {
            if let Some(last) = g.as_deref() {
                if last != FAST_PREFERRED_CLIENT {
                    add(last);
                }
            }
        }
        for c in PLAYER_CLIENTS {
            let ckey = c.key();
            if !self.is_cooling(video_id, &ckey) && !order.iter().any(|e| e.key() == ckey) {
                order.push(c);
            }
        }
        order
    }

    async fn mint_po_token(&self, video_id: &str, force_refresh: bool) -> Option<PoTokenResult> {
        let engine = self.po_tokens.as_ref()?;
        if !self.po_token_enabled() || !engine.is_enabled() {
            return None;
        }
        if !force_refresh {
            let current = self.auth_store.current();
            if !current.is_po_token_stale() && !current.po_token.is_empty() {
                return Some(PoTokenResult {
                    player_token: current.po_token.clone(),
                    session_token: current.po_token,
                });
            }
        }
        tokio::time::timeout(
            Duration::from_secs(15),
            engine.mint_token(video_id, "lastwave_session", force_refresh),
        )
        .await
        .ok()
        .flatten()
    }

    #[allow(clippy::too_many_arguments)]
    async fn resolve_direct_client_stream(
        &self,
        video_id: &str,
        client: &PlayerClient,
        visitor_data: Option<&str>,
        signature_timestamp: Option<i64>,
        script: Option<&PlayerScript>,
        player_po_token: Option<&str>,
        gvs_po_token: Option<&str>,
        auth_scope: &str,
        prefer_smallest: bool,
    ) -> Result<StreamCandidate, InnerTubeError> {
        let client_needs_cipher = client.requires_cipher();
        let mut body = json!({
            "context": self.build_context(
                client.name,
                client.version,
                visitor_data,
                client.os_name,
                client.os_version,
                client.device_make,
                client.device_model,
                client.android_sdk_version,
                player_po_token,
            ),
            "videoId": video_id,
            "contentCheckOk": true,
            "racyCheckOk": true,
        });
        if client_needs_cipher {
            if let Some(sts) = signature_timestamp {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert(
                        "playbackContext".to_string(),
                        json!({ "contentPlaybackContext": { "signatureTimestamp": sts } }),
                    );
                }
            }
        }

        let player_api = if client.name == "WEB_REMIX" {
            MUSIC_API
        } else {
            YOUTUBE_API
        };
        let url = format!("{player_api}/player?key={}&prettyPrint=false", client.api_key);
        let origin = client.origin();
        let referer = client.referer();
        let use_auth = self.connection().connected
            && matches!(client.name, "WEB_REMIX" | "TVHTML5" | "MWEB");
        let root = self
            .post_json(
                &url,
                &body,
                client.name,
                client.version,
                client.user_agent,
                use_auth,
                Some(origin),
                Some(&referer),
                visitor_data,
                MAX_PLAYER_REQUEST_ATTEMPTS,
                None,
                None,
            )
            .await?;

        let status = root.get("playabilityStatus");
        let state = status
            .and_then(|s| s.get("status"))
            .and_then(Value::as_str)
            .unwrap_or("missing");
        if state != "OK" {
            let reason = status
                .and_then(|s| s.get("reason"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if state == "UNPLAYABLE" && is_confirmed_unavailable(reason) {
                return Err(InnerTubeError::ConfirmedUnplayable(if reason.is_empty() {
                    state.to_string()
                } else {
                    reason.to_string()
                }));
            }
            return Err(InnerTubeError::InvalidState(if reason.is_empty() {
                format!("Player status {state}")
            } else {
                reason.to_string()
            }));
        }

        let mut headers = client.stream_request_headers();
        if let Some(v) = visitor_data.filter(|s| !s.is_empty()) {
            headers.insert("X-Goog-Visitor-Id".to_string(), v.to_string());
        }
        if use_auth {
            if let Some(cookies) = self.cookie_header_value() {
                headers.insert("Cookie".to_string(), cookies);
            }
            if let Some(auth) = self.authorization_header_value(Some(origin)) {
                headers.insert("Authorization".to_string(), auth);
            }
        }

        let streaming = root.get("streamingData");
        let now_ms = now_unix_ms();
        let response_expiry = streaming
            .and_then(|s| s.get("expiresInSeconds"))
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
            })
            .map(|secs| now_ms + secs.max(1) * 1000);

        let mut formats = Vec::new();
        if let Some(arr) = streaming
            .and_then(|s| s.get("formats"))
            .and_then(Value::as_array)
        {
            for f in arr {
                if let Some(obj) = f.as_object() {
                    formats.push((obj, false));
                }
            }
        }
        if let Some(arr) = streaming
            .and_then(|s| s.get("adaptiveFormats"))
            .and_then(Value::as_array)
        {
            for f in arr {
                if let Some(obj) = f.as_object() {
                    formats.push((obj, true));
                }
            }
        }

        let client_key = client.key();
        let mut candidates = Vec::new();
        let mut sabr_count = 0usize;
        let mut decipher_fail_count = 0usize;

        for (format, adaptive) in formats {
            let raw_url = format
                .get("url")
                .and_then(Value::as_str)
                .map(|u| {
                    if client_needs_cipher {
                        self.decipher.decipher_direct_url(u, script)
                    } else {
                        u.to_string()
                    }
                })
                .or_else(|| {
                    let cipher = format
                        .get("signatureCipher")
                        .or_else(|| format.get("cipher"))
                        .and_then(Value::as_str)?;
                    let res = self.decipher.decipher_url(cipher, script);
                    if res.is_none() {
                        decipher_fail_count += 1;
                    }
                    res
                });
            let Some(mut stream_url) = raw_url.filter(|u| !u.is_empty()) else {
                continue;
            };
            if stream_url.starts_with("sabr:") {
                sabr_count += 1;
                continue;
            }
            stream_url = Self::append_pot(&stream_url, gvs_po_token);
            let mime = val_str(format.get("mimeType"));
            let codec = Self::extract_codec(&mime);
            let is_audio_only = Self::is_compatible_audio(&mime, codec.as_deref());
            let is_muxed = !is_audio_only && Self::is_compatible_muxed_audio(&mime, codec.as_deref());
            if !is_audio_only && !is_muxed {
                continue;
            }
            let bitrate = format
                .get("bitrate")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let url_expiry = Self::url_expiry_ms(&stream_url);
            let exp_ms = match (url_expiry, response_expiry) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            }
            .unwrap_or(now_ms + UNKNOWN_EXPIRY_TTL_MS);
            let expires_at = DateTime::from_timestamp_millis(exp_ms);

            let codec_label = match codec.as_deref().filter(|c| !c.is_empty()) {
                Some(c) => {
                    let lc = c.to_lowercase();
                    if lc.contains("opus") {
                        "OPUS".to_string()
                    } else if lc.contains("mp4a") || lc.contains("aac") {
                        "AAC".to_string()
                    } else {
                        c.to_uppercase()
                    }
                }
                None => {
                    let lm = mime.to_lowercase();
                    if lm.contains("opus") {
                        "OPUS".to_string()
                    } else if lm.contains("mp4") {
                        "AAC".to_string()
                    } else {
                        "AUDIO".to_string()
                    }
                }
            };
            let itag = format.get("itag").and_then(Value::as_i64).unwrap_or(-1);
            let clean_mime = mime.split(';').next().unwrap_or("").trim().to_string();

            candidates.push(StreamCandidate {
                stream: ResolvedStream {
                    url: stream_url,
                    mime_type: clean_mime,
                    bitrate_kbps: ((bitrate as f64) / 1000.0).round() as i32,
                    audio_codec: codec_label,
                    cache_key: format!(
                        "youtube:{video_id}:{client_key}:{itag}:{auth_scope}:{exp_ms}"
                    ),
                    request_headers: headers.clone(),
                    expires_at,
                    ..Default::default()
                },
                adaptive,
                bitrate,
                itag,
                is_audio_only,
            });
        }

        let hls_url = streaming
            .and_then(|s| s.get("hlsUrl").or_else(|| s.get("hlsManifestUrl")))
            .and_then(Value::as_str);
        if let Some(hls) = hls_url.filter(|u| !u.is_empty() && !u.starts_with("sabr:")) {
            let url_expiry = Self::url_expiry_ms(hls);
            let exp_ms = match (url_expiry, response_expiry) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            }
            .unwrap_or(now_ms + UNKNOWN_EXPIRY_TTL_MS);
            let expires_at = DateTime::from_timestamp_millis(exp_ms);
            candidates.push(StreamCandidate {
                stream: ResolvedStream {
                    url: hls.to_string(),
                    mime_type: "application/vnd.apple.mpegurl".to_string(),
                    bitrate_kbps: 128,
                    audio_codec: "AAC".to_string(),
                    cache_key: format!(
                        "youtube:{video_id}:{client_key}:hls:{auth_scope}:{exp_ms}"
                    ),
                    request_headers: headers,
                    expires_at,
                    ..Default::default()
                },
                adaptive: false,
                bitrate: 128_000,
                itag: -1,
                is_audio_only: false,
            });
        }

        Self::sort_stream_candidates(&mut candidates, prefer_smallest);
        if candidates.is_empty() {
            let total_fmt = streaming
                .and_then(|s| s.get("formats"))
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
                + streaming
                    .and_then(|s| s.get("adaptiveFormats"))
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
            let has_hls = hls_url.is_some();
            DiagnosticsLog::info(&format!(
                "stream_empty client={client_key} video_id={video_id} raw_formats={total_fmt} sabr={sabr_count} decipher_fail={decipher_fail_count} has_hls={has_hls} has_script={} qjs_active={}",
                script.is_some(),
                self.decipher.quick_js.active_player().is_some()
            ));
        }
        candidates
            .into_iter()
            .next()
            .ok_or_else(|| InnerTubeError::InvalidState(format!("{client_key} returned no usable audio URL")))
    }

    fn sort_stream_candidates(
        candidates: &mut [StreamCandidate],
        prefer_smallest: bool,
    ) {
        candidates.sort_by(|a, b| {
            if a.is_audio_only != b.is_audio_only {
                return if a.is_audio_only {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            let a_hls = a.stream.mime_type.contains("mpegurl");
            let b_hls = b.stream.mime_type.contains("mpegurl");
            if a_hls != b_hls {
                return if a_hls {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Less
                };
            }
            let ad = (b.adaptive as i32).cmp(&(a.adaptive as i32));
            if ad != std::cmp::Ordering::Equal {
                return ad;
            }
            if prefer_smallest {
                let ab = if a.bitrate <= 0 { i64::MAX } else { a.bitrate };
                let bb = if b.bitrate <= 0 { i64::MAX } else { b.bitrate };
                ab.cmp(&bb)
            } else {
                b.bitrate.cmp(&a.bitrate)
            }
        });
    }

    pub async fn resolve_download_stream(&self, video_id: &str) -> Option<ResolvedStream> {
        self.resolve_download_stream_kind(video_id, false).await
    }

    pub async fn resolve_opus_download_stream(&self, video_id: &str) -> Option<ResolvedStream> {
        self.resolve_download_stream_kind(video_id, true).await
    }

    async fn resolve_download_stream_kind(
        &self,
        video_id: &str,
        prefer_opus: bool,
    ) -> Option<ResolvedStream> {
        if video_id.is_empty() {
            return None;
        }
        let script = tokio::time::timeout(
            Duration::from_secs(30),
            self.decipher.script_for(video_id),
        )
        .await
        .ok()
        .flatten();
        let sts = script.as_ref().and_then(|s| s.sts);
        self.ensure_config().await;
        let visitor = self.current_web_config().2;

        let mut collected = Vec::new();
        for client in PLAYER_CLIENTS {
            let api = if client.name == "WEB_REMIX" {
                MUSIC_API
            } else {
                YOUTUBE_API
            };
            let url = format!("{api}/player?key={}&prettyPrint=false", client.api_key);
            let mut body = json!({
                "context": self.build_context(
                    client.name,
                    client.version,
                    visitor.as_deref(),
                    client.os_name,
                    client.os_version,
                    client.device_make,
                    client.device_model,
                    client.android_sdk_version,
                    None,
                ),
                "videoId": video_id,
                "contentCheckOk": true,
                "racyCheckOk": true,
            });
            if client.requires_cipher() {
                if let Some(s) = sts {
                    if let Some(obj) = body.as_object_mut() {
                        obj.insert(
                            "playbackContext".to_string(),
                            json!({ "contentPlaybackContext": { "signatureTimestamp": s } }),
                        );
                    }
                }
            }
            let Ok(root) = self
                .post_json(
                    &url,
                    &body,
                    client.name,
                    client.version,
                    client.user_agent,
                    false,
                    Some(client.origin()),
                    Some(&client.referer()),
                    visitor.as_deref(),
                    1,
                    None,
                    None,
                )
                .await
            else {
                continue;
            };
            if root
                .get("playabilityStatus")
                .and_then(|s| s.get("status"))
                .and_then(Value::as_str)
                != Some("OK")
            {
                continue;
            }
            let streaming = root.get("streamingData");
            for key in ["formats", "adaptiveFormats"] {
                let Some(arr) = streaming.and_then(|s| s.get(key)).and_then(Value::as_array) else {
                    continue;
                };
                for f in arr.iter().filter_map(Value::as_object) {
                    let raw_url = f
                        .get("url")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| {
                            let cipher = f
                                .get("signatureCipher")
                                .or_else(|| f.get("cipher"))
                                .and_then(Value::as_str)?;
                            self.decipher.decipher_url(cipher, script.as_ref())
                        });
                    let Some(stream_url) = raw_url.filter(|u| !u.starts_with("sabr:")) else {
                        continue;
                    };
                    let mime = val_str(f.get("mimeType"));
                    let codec = Self::extract_codec(&mime);
                    if prefer_opus {
                        if !mime.to_lowercase().starts_with("audio/") {
                            continue;
                        }
                    } else {
                        let audio_only = Self::is_compatible_audio(&mime, codec.as_deref());
                        let muxed =
                            !audio_only && Self::is_compatible_muxed_audio(&mime, codec.as_deref());
                        if !audio_only && !muxed {
                            continue;
                        }
                    }
                    let is_audio_only = Self::is_compatible_audio(&mime, codec.as_deref());
                    let bitrate = f.get("bitrate").and_then(Value::as_i64).unwrap_or(0);
                    let itag = f.get("itag").and_then(Value::as_i64).unwrap_or(-1);
                    let clean_mime = mime.split(';').next().unwrap_or("").trim().to_string();
                    let audio_codec = if prefer_opus {
                        codec
                            .as_deref()
                            .filter(|c| !c.is_empty())
                            .map(str::to_uppercase)
                            .unwrap_or_else(|| "AUDIO".to_string())
                    } else {
                        "AUDIO".to_string()
                    };
                    let exp = Self::url_expiry_ms(&stream_url)
                        .and_then(DateTime::from_timestamp_millis)
                        .or_else(|| {
                            DateTime::from_timestamp_millis(now_unix_ms() + UNKNOWN_EXPIRY_TTL_MS)
                        });
                    let cache_prefix = if prefer_opus { "yt-opus-dl" } else { "yt-dl" };
                    collected.push(StreamCandidate {
                        stream: ResolvedStream {
                            url: stream_url,
                            mime_type: clean_mime,
                            bitrate_kbps: ((bitrate as f64) / 1000.0).round() as i32,
                            audio_codec,
                            cache_key: format!("{cache_prefix}:{video_id}:{itag}"),
                            request_headers: client.stream_request_headers(),
                            expires_at: exp,
                            ..Default::default()
                        },
                        adaptive: true,
                        bitrate,
                        itag,
                        is_audio_only,
                    });
                }
            }
            if collected.len() >= 12 {
                break;
            }
        }

        let best = if prefer_opus {
            collected
                .iter()
                .filter(|c| {
                    c.stream.audio_codec.to_uppercase().contains("OPUS")
                        || c.stream.mime_type.to_lowercase().contains("opus")
                })
                .max_by_key(|c| c.bitrate)
                .or_else(|| {
                    collected
                        .iter()
                        .filter(|c| c.stream.mime_type.to_lowercase().contains("webm"))
                        .max_by_key(|c| c.bitrate)
                })
                .or_else(|| collected.iter().max_by_key(|c| c.bitrate))
                .cloned()
        } else {
            let audio_only: Vec<&StreamCandidate> =
                collected.iter().filter(|c| c.is_audio_only).collect();
            let pool: Vec<&StreamCandidate> = if audio_only.is_empty() {
                collected.iter().collect()
            } else {
                audio_only
            };
            pool.iter()
                .filter(|c| {
                    let m = c.stream.mime_type.to_lowercase();
                    m.contains("mp4") || m.contains("m4a")
                })
                .max_by_key(|c| c.bitrate)
                .copied()
                .or_else(|| pool.into_iter().max_by_key(|c| c.bitrate))
                .cloned()
        };

        let Some(candidate) = best else {
            return self.resolve_audio_stream(video_id, false, false).await;
        };
        let stage = if prefer_opus {
            "opus-download-probe"
        } else {
            "download-probe"
        };
        if self.probe_stream(&candidate.stream, stage, true).await {
            Some(candidate.stream)
        } else {
            self.resolve_audio_stream(video_id, false, false).await
        }
    }

    fn is_cooling(&self, video_id: &str, client_key: &str) -> bool {
        let prefix = format!("{video_id}|{client_key}|");
        let now_ms = now_unix_ms();
        let mut cooling = false;
        let mut has_expired = false;
        if let Ok(g) = self.failed_clients_until.read() {
            for (key, until_ms) in &g.map {
                if !key.starts_with(&prefix) {
                    continue;
                }
                if now_ms < *until_ms {
                    cooling = true;
                } else {
                    has_expired = true;
                }
            }
        }
        if has_expired {
            if let Ok(mut g) = self.failed_clients_until.write() {
                g.retain(|k, v| !(k.starts_with(&prefix) && now_ms >= *v));
            }
        }
        cooling
    }

    fn cool_client(&self, video_id: &str, client_key: &str, auth_scope: Option<&str>) {
        let scope = auth_scope
            .map(str::to_string)
            .unwrap_or_else(|| self.playback_auth_scope());
        let key = format!("{video_id}|{client_key}|{scope}");
        let until_ms = now_unix_ms() + CLIENT_COOLDOWN_MS;
        if let Ok(mut g) = self.failed_clients_until.write() {
            g.insert_lru(key, until_ms, 128);
        }
    }

    pub fn report_playback_failure(&self, video_id: &str) {
        let prefix = format!("{video_id}|");
        if let Ok(mut g) = self.stream_cache.write() {
            g.retain(|k, _| !k.starts_with(&prefix));
        }
        if let Some(disk) = self.current_disk() {
            disk.delete_stream_entries(video_id);
        }
        let prev_ref = self.last_resolved.write().ok().and_then(|mut g| g.remove(video_id));
        if let Some(r) = prev_ref {
            self.cool_client(video_id, &r.client_profile, Some(&r.auth_scope));
            log_stream("report-failure", video_id, &r.client_profile, -1, "", "unknown", 0, 0, "");
        } else {
            for c in PLAYER_CLIENTS.iter().take(MAX_STREAM_CLIENTS) {
                self.cool_client(video_id, &c.key(), None);
            }
            log_stream("report-failure", video_id, "", -1, "", "unknown", 0, 0, "");
        }
    }

    fn cache_resolved_stream(
        &self,
        video_id: &str,
        candidate: &StreamCandidate,
        client_profile: &str,
        auth_scope: &str,
        now_ms: i64,
        prefer_smallest: bool,
    ) {
        let stored = if prefer_smallest {
            mark_smallest_stream(&candidate.stream)
        } else {
            candidate.stream.clone()
        };
        let tier = if prefer_smallest { "min" } else { "max" };
        let exp_ms = stored
            .expires_at
            .map(|e| e.timestamp_millis())
            .unwrap_or(0);
        let key = format!("{video_id}|{client_profile}|{}|{auth_scope}|{exp_ms}|{tier}", candidate.itag);

        if let Ok(mut cache) = self.stream_cache.write() {
            cache.insert(
                key,
                CachedStream {
                    stream: stored,
                    client_profile: client_profile.to_string(),
                    itag: candidate.itag,
                    adaptive: candidate.adaptive,
                    auth_scope: auth_scope.to_string(),
                    cached_at_ms: now_ms,
                },
            );
            if cache.len() > MAX_STREAM_CACHE_ENTRIES {
                let overflow = cache.len() - MAX_STREAM_CACHE_ENTRIES;
                let mut sorted: Vec<(String, i64)> = cache
                    .iter()
                    .map(|(k, v)| (k.clone(), v.cached_at_ms))
                    .collect();
                sorted.sort_by_key(|(_, ts)| *ts);
                for (doomed, _) in sorted.into_iter().take(overflow) {
                    cache.remove(&doomed);
                }
            }
        }

        if prefer_smallest {
            return;
        }
        if let Some(disk) = self.current_disk() {
            let headers_json =
                serde_json::to_string(&candidate.stream.request_headers).unwrap_or_else(|_| "{}".to_string());
            disk.save_stream_entry(
                video_id,
                client_profile,
                candidate.itag,
                &candidate.stream.url,
                &headers_json,
                &candidate.stream.mime_type,
                i64::from(candidate.stream.bitrate_kbps),
                &candidate.stream.audio_codec,
                exp_ms,
                now_ms,
                auth_scope,
                &candidate.stream.watchtime_url,
            );
        }
    }

    fn is_fresh(c: &CachedStream, now_ms: i64) -> bool {
        if now_ms.saturating_sub(c.cached_at_ms) >= STREAM_TTL_MS {
            return false;
        }
        match c.stream.expires_at {
            None => true,
            Some(exp) => exp.timestamp_millis() - now_ms > URL_EXPIRY_MARGIN_MS,
        }
    }

    fn expiry_state(exp: Option<DateTime<Utc>>) -> &'static str {
        match exp {
            None => "unknown",
            Some(e) if e < Utc::now() => "expired",
            Some(_) => "fresh",
        }
    }

    async fn probe_stream(&self, stream: &ResolvedStream, stage: &str, adaptive: bool) -> bool {
        let now_ms = now_unix_ms();
        if let Some(exp) = stream.expires_at {
            if exp.timestamp_millis() - now_ms <= URL_EXPIRY_MARGIN_MS {
                log_stream(stage, "", "", -1, &stream.mime_type, "expired", 0, 0, "expired=true");
                return false;
            }
        }
        let mut req = self
            .client
            .get(&stream.url)
            .header("Accept-Encoding", "identity");
        if !adaptive {
            req = req.header("Range", "bytes=0-1");
        }
        for (k, v) in &stream.request_headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let res = match tokio::time::timeout(Duration::from_secs(10), req.send()).await {
            Ok(Ok(r)) => r,
            _ => {
                log_stream(
                    stage,
                    "",
                    "",
                    -1,
                    &stream.mime_type,
                    Self::expiry_state(stream.expires_at),
                    0,
                    0,
                    "error=timeout_or_net",
                );
                return false;
            }
        };

        let status = res.status().as_u16();
        let content_type = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();
        let valid_type = !content_type.contains("text/html")
            && !content_type.contains("application/json")
            && !content_type.contains("text/plain");
        let got_byte = tokio::time::timeout(Duration::from_secs(10), res.bytes())
            .await
            .ok()
            .and_then(Result::ok)
            .is_some_and(|b| !b.is_empty());

        let valid = (status == 200 || status == 206) && valid_type && got_byte;
        let short_ct: String = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .chars()
            .take(40)
            .collect();
        log_stream(
            stage,
            "",
            "",
            -1,
            &stream.mime_type,
            Self::expiry_state(stream.expires_at),
            0,
            status,
            &format!("valid={valid} type={short_ct}"),
        );
        valid
    }

    // Stream Format & URL Helpers

    pub fn has_video_codec(mime: &str) -> bool {
        let lower = mime.to_lowercase();
        ["avc1", "avc3", "hvc1", "hev1", "vp8", "vp9", "vp09", "av01", "mp4v", "theora"]
            .iter()
            .any(|k| lower.contains(k))
    }

    pub fn is_compatible_audio(mime: &str, codec: Option<&str>) -> bool {
        let m = mime.to_lowercase();
        let c = codec.unwrap_or("").to_lowercase();
        if Self::has_video_codec(&m) || Self::has_video_codec(&c) {
            return false;
        }
        if m.starts_with("video/") {
            return false;
        }
        let base = m.split(';').next().unwrap_or("").trim();
        match base {
            "audio/webm" | "audio/ogg" => {
                c.is_empty() || c.contains("opus") || c.contains("vorbis")
            }
            "audio/mp4" | "audio/m4a" => {
                c.is_empty() || c.contains("mp4a") || c.contains("aac")
            }
            "audio/mpeg" => true,
            _ => false,
        }
    }

    pub fn is_compatible_muxed_audio(mime: &str, codec: Option<&str>) -> bool {
        let m = mime.to_lowercase();
        let c = codec.unwrap_or("").to_lowercase();
        let has_audio = c.contains("mp4a")
            || c.contains("aac")
            || c.contains("opus")
            || c.contains("vorbis");
        if !has_audio {
            return false;
        }
        let base = m.split(';').next().unwrap_or("").trim();
        base == "video/mp4" || base == "video/webm"
    }

    pub fn extract_codec(mime: &str) -> Option<String> {
        static RE_CODEC: OnceLock<Regex> = OnceLock::new();
        let re = RE_CODEC.get_or_init(|| Regex::new(r#"(?i)codecs?=["']([^"']+)["']"#).unwrap());
        let cap = re.captures(mime)?.get(1)?.as_str();
        let parts: Vec<&str> = cap
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        for p in &parts {
            let lp = p.to_lowercase();
            if lp.contains("mp4a")
                || lp.contains("aac")
                || lp.contains("opus")
                || lp.contains("vorbis")
            {
                return Some((*p).to_string());
            }
        }
        parts.first().map(|s| (*s).to_string())
    }

    pub fn url_expiry_ms(url: &str) -> Option<i64> {
        let parsed = Url::parse(url).ok()?;
        let exp = parsed
            .query_pairs()
            .find(|(k, _)| k == "expire")
            .map(|(_, v)| v.into_owned())?;
        let secs = exp.parse::<i64>().ok()?;
        Some(secs * 1000)
    }

    pub fn append_pot(url: &str, token: Option<&str>) -> String {
        let Some(tok) = token.filter(|t| !t.is_empty()) else {
            return url.to_string();
        };
        let Ok(parsed) = Url::parse(url) else {
            return url.to_string();
        };
        if parsed.query_pairs().any(|(k, _)| k == "pot") {
            return url.to_string();
        }
        let (base, fragment) = match url.find('#') {
            Some(idx) => (&url[..idx], &url[idx..]),
            None => (url, ""),
        };
        let sep = if base.ends_with('?') || base.ends_with('&') {
            ""
        } else if base.contains('?') {
            "&"
        } else {
            "?"
        };
        let encoded_tok: String = url::form_urlencoded::byte_serialize(tok.as_bytes()).collect();
        format!("{base}{sep}pot={encoded_tok}{fragment}")
    }

    pub fn high_resolution_artwork(url: &str) -> String {
        static RE_GUSER_WH: OnceLock<Regex> = OnceLock::new();
        let mut u = if url.starts_with("//") {
            format!("https:{url}")
        } else {
            url.to_string()
        };
        if u.contains("googleusercontent.com") || u.contains("ggpht.com") {
            // Split off any ?query or #fragment so we never truncate signed params.
            let suffix_idx = u.find(['?', '#']).unwrap_or(u.len());
            let (path_part, tail) = u.split_at(suffix_idx);
            if let Some(eq_idx) = path_part.rfind('=') {
                let (base, params) = path_part.split_at(eq_idx);
                let wh_re = RE_GUSER_WH.get_or_init(|| {
                    Regex::new(r"^=w\d+-h\d+(?:-l\d+)?(?:-rj)?$").unwrap()
                });
                if wh_re.is_match(params) {
                    u = format!("{base}=w512-h512-l90-rj{tail}");
                }
            }
        }
        u
    }

    pub fn parse_duration(value: &str) -> Option<i64> {
        static RE_BIDI: OnceLock<Regex> = OnceLock::new();
        static RE_TIME: OnceLock<Regex> = OnceLock::new();
        let cleaned = RE_BIDI
            .get_or_init(|| Regex::new(r"[\u200e\u200f\u202a-\u202e\u2066-\u2069]").unwrap())
            .replace_all(value.trim(), "")
            .replace('：', ":");
        let cleaned = cleaned.trim();
        if cleaned.is_empty() {
            return None;
        }
        let token = RE_TIME
            .get_or_init(|| Regex::new(r"(\d{1,2}:)+\d{2}").unwrap())
            .find(cleaned)
            .map(|m| m.as_str())
            .unwrap_or(cleaned);
        let nums: Option<Vec<i64>> = token
            .split(':')
            .map(|p| p.parse::<i64>().ok())
            .collect();
        let nums = nums?;
        if nums.len() < 2 || nums.len() > 3 {
            return None;
        }
        let mut total = 0i64;
        for n in nums {
            total = total * 60 + n;
        }
        if total <= 0 || total > 24 * 3600 {
            None
        } else {
            Some(total)
        }
    }

    // JSON Renderer Parsers

    fn parse_two_row_song(&self, r: &Map<String, Value>) -> Option<YouTubeMusicTrack> {
        let video_id = r
            .get("navigationEndpoint")
            .and_then(|n| n.get("watchEndpoint"))
            .and_then(|w| w.get("videoId"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if video_id.is_empty() {
            return None;
        }
        let title_obj = r.get("title")?;
        let title = title_obj
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| title_obj.get("runs").and_then(runs_text))
            .unwrap_or_default()
            .trim()
            .to_string();
        if title.is_empty() {
            return None;
        }
        let mut artist = String::new();
        if let Some(sub) = r.get("subtitle") {
            let text = sub
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| sub.get("runs").and_then(runs_text))
                .unwrap_or_default();
            let parts: Vec<&str> = text.split('•').map(str::trim).collect();
            if !parts.is_empty() {
                let first_low = parts[0].to_lowercase();
                let is_type = matches!(
                    first_low.as_str(),
                    "song" | "video" | "single" | "ep" | "album"
                );
                if parts.len() > 1 && is_type {
                    artist = parts[1].to_string();
                } else {
                    artist = parts[0].to_string();
                }
            }
        }
        if artist.is_empty() {
            artist = "Unknown artist".to_string();
        }
        Some(YouTubeMusicTrack {
            video_id: video_id.to_string(),
            title,
            artist,
            artwork_url: extract_thumbnails_url(&Value::Object(r.clone())).unwrap_or_default(),
            ..Default::default()
        })
    }

    fn parse_two_row_entity(
        &self,
        r: &Map<String, Value>,
        kind: YouTubeEntityKind,
    ) -> Option<YouTubeMusicEntity> {
        let title_obj = r.get("title")?;
        let name = title_obj
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| title_obj.get("runs").and_then(runs_text))
            .unwrap_or_default();
        let subtitle = r
            .get("subtitle")
            .and_then(|s| {
                s.get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| s.get("runs").and_then(runs_text))
            })
            .unwrap_or_default();

        let nav = r.get("navigationEndpoint");
        let browse_id = nav
            .and_then(|n| n.get("browseEndpoint"))
            .and_then(|b| b.get("browseId"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let playlist_id = nav
            .and_then(|n| n.get("watchEndpoint"))
            .and_then(|w| w.get("playlistId"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        if name.is_empty() || (browse_id.is_empty() && playlist_id.is_empty()) {
            return None;
        }
        Some(YouTubeMusicEntity {
            kind,
            name,
            artist: String::new(),
            subtitle,
            browse_id,
            playlist_id,
            artwork_url: extract_thumbnails_url(&Value::Object(r.clone())).unwrap_or_default(),
        })
    }

    fn parse_song_renderers(&self, root: &Value) -> Vec<YouTubeMusicTrack> {
        let mut renderers = Vec::new();
        collect_objects(root, "musicResponsiveListItemRenderer", &mut renderers);
        let mut songs: Vec<YouTubeMusicTrack> = renderers
            .iter()
            .filter_map(|r| self.parse_song(r))
            .collect();

        let mut queue_renderers = Vec::new();
        collect_objects(root, "playlistPanelVideoRenderer", &mut queue_renderers);
        songs.extend(
            queue_renderers
                .iter()
                .filter_map(|r| self.parse_playlist_panel_song(r)),
        );

        if songs.is_empty() {
            let mut yt_videos = Vec::new();
            collect_objects(root, "playlistVideoRenderer", &mut yt_videos);
            songs.extend(
                yt_videos
                    .iter()
                    .filter_map(|r| self.parse_playlist_video_renderer(r)),
            );
        }

        let mut seen = HashSet::new();
        songs
            .into_iter()
            .filter(|s| seen.insert(s.video_id.clone()))
            .collect()
    }

    fn direct_watch_video_id(&self, r: &Map<String, Value>) -> Option<String> {
        if let Some(vid) = r
            .get("playlistItemData")
            .and_then(|d| d.get("videoId"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            return Some(vid.to_string());
        }
        if let Some(vid) = r
            .get("navigationEndpoint")
            .and_then(|n| n.get("watchEndpoint"))
            .and_then(|w| w.get("videoId"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            return Some(vid.to_string());
        }
        if let Some(vid) = r
            .get("thumbnailOverlay")
            .and_then(|o| o.get("musicItemThumbnailOverlayRenderer"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.get("musicPlayButtonRenderer"))
            .and_then(|p| p.get("playNavigationEndpoint"))
            .and_then(|n| n.get("watchEndpoint"))
            .and_then(|w| w.get("videoId"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            return Some(vid.to_string());
        }
        None
    }

    fn parse_playlist_video_renderer(&self, r: &Map<String, Value>) -> Option<YouTubeMusicTrack> {
        let video_id = r.get("videoId").and_then(Value::as_str)?.to_string();
        if video_id.is_empty() {
            return None;
        }
        let title_obj = r.get("title")?;
        let title = title_obj
            .get("runs")
            .and_then(runs_text)
            .or_else(|| {
                title_obj
                    .get("simpleText")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })?;
        if title.is_empty() {
            return None;
        }
        let artist = r
            .get("shortBylineText")
            .and_then(|b| b.get("runs"))
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(|f| f.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("Unknown artist")
            .to_string();
        let duration = r
            .get("lengthSeconds")
            .and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64()))
            .unwrap_or(0);
        let artwork_url = r
            .get("thumbnail")
            .and_then(|t| t.get("thumbnails"))
            .and_then(Value::as_array)
            .and_then(|arr| arr.last())
            .and_then(|item| item.get("url"))
            .and_then(Value::as_str)
            .map(Self::high_resolution_artwork)
            .unwrap_or_default();

        Some(YouTubeMusicTrack {
            video_id,
            title,
            artist,
            artwork_url,
            duration_seconds: duration,
            ..Default::default()
        })
    }

    fn parse_playlist_panel_song(&self, r: &Map<String, Value>) -> Option<YouTubeMusicTrack> {
        if r.contains_key("unplayableText") {
            return None;
        }
        let video_id = r
            .get("videoId")
            .and_then(Value::as_str)
            .or_else(|| {
                r.get("navigationEndpoint")
                    .and_then(|n| n.get("watchEndpoint"))
                    .and_then(|w| w.get("videoId"))
                    .and_then(Value::as_str)
            })?
            .to_string();
        if video_id.is_empty() {
            return None;
        }
        let title_obj = r.get("title")?;
        let mut title = title_obj
            .get("runs")
            .and_then(runs_text)
            .unwrap_or_default()
            .trim()
            .to_string();
        if title.is_empty() {
            title = title_obj
                .get("simpleText")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
        }
        if title.is_empty() {
            return None;
        }

        let details: Vec<&Map<String, Value>> = r
            .get("longBylineText")
            .or_else(|| r.get("shortBylineText"))
            .and_then(|b| b.get("runs"))
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_object).collect())
            .unwrap_or_default();

        let mut artist = String::new();
        for run in &details {
            let bid = run
                .get("navigationEndpoint")
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if bid.starts_with("UC") {
                artist = val_str(run.get("text"));
                break;
            }
        }
        if artist.is_empty() {
            artist = details
                .iter()
                .filter_map(|e| e.get("text").and_then(Value::as_str))
                .find(|t| is_likely_artist_detail(t))
                .unwrap_or("Unknown artist")
                .to_string();
        }
        if artist.is_empty() {
            artist = "Unknown artist".to_string();
        }

        let mut album = String::new();
        let mut album_browse_id = String::new();
        for run in &details {
            let bid = run
                .get("navigationEndpoint")
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if bid.starts_with("MPRE") {
                album = val_str(run.get("text"));
                album_browse_id = bid.to_string();
                break;
            }
        }

        let mut duration = 0i64;
        if let Some(runs) = r
            .get("lengthText")
            .and_then(|l| l.get("runs"))
            .and_then(Value::as_array)
        {
            for e in runs {
                if let Some(d) = e
                    .get("text")
                    .and_then(Value::as_str)
                    .and_then(Self::parse_duration)
                {
                    duration = d;
                    break;
                }
            }
        }

        Some(YouTubeMusicTrack {
            video_id,
            title,
            artist,
            album,
            artwork_url: renderer_artwork(r),
            duration_seconds: duration,
            album_browse_id,
        })
    }

    fn parse_song(&self, r: &Map<String, Value>) -> Option<YouTubeMusicTrack> {
        let video_id = self
            .direct_watch_video_id(r)
            .or_else(|| find_string(&Value::Object(r.clone()), "videoId"))?;
        if video_id.is_empty() {
            return None;
        }
        let columns = r.get("flexColumns").and_then(Value::as_array);
        let col_text = |i: usize| -> Option<&Value> {
            columns?
                .get(i)?
                .get("musicResponsiveListItemFlexColumnRenderer")?
                .get("text")
        };
        let title = col_text(0)
            .and_then(|t| t.get("runs"))
            .and_then(runs_text)
            .unwrap_or_default()
            .trim()
            .to_string();
        if title.is_empty() {
            return None;
        }
        let details: Vec<&Map<String, Value>> = col_text(1)
            .and_then(|t| t.get("runs"))
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_object).collect())
            .unwrap_or_default();

        let mut artist = String::new();
        for run in &details {
            let bid = run
                .get("navigationEndpoint")
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if bid.starts_with("UC") {
                artist = val_str(run.get("text"));
                break;
            }
        }
        if artist.is_empty() {
            artist = details
                .iter()
                .filter_map(|e| e.get("text").and_then(Value::as_str))
                .find(|t| is_useful_detail(t) && Self::parse_duration(t).is_none())
                .unwrap_or("Unknown artist")
                .to_string();
        }
        if artist.is_empty() {
            artist = "Unknown artist".to_string();
        }

        let mut album = String::new();
        let mut album_browse_id = String::new();
        for run in &details {
            let bid = run
                .get("navigationEndpoint")
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if bid.starts_with("MPRE") {
                album = val_str(run.get("text"));
                album_browse_id = bid.to_string();
                break;
            }
        }

        Some(YouTubeMusicTrack {
            video_id,
            title,
            artist,
            album,
            artwork_url: renderer_artwork(r),
            duration_seconds: duration_from_renderer(r),
            album_browse_id,
        })
    }

    fn parse_entity_renderers(
        &self,
        root: &Value,
        kind: YouTubeEntityKind,
    ) -> Vec<YouTubeMusicEntity> {
        let mut renderers = Vec::new();
        collect_objects(root, "musicResponsiveListItemRenderer", &mut renderers);
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for r in renderers {
            if let Some(e) = self.parse_entity(&r, kind) {
                if seen.insert(e.browse_id.clone()) {
                    out.push(e);
                }
            }
        }
        out
    }

    fn parse_entity(
        &self,
        r: &Map<String, Value>,
        kind: YouTubeEntityKind,
    ) -> Option<YouTubeMusicEntity> {
        let browse = r
            .get("navigationEndpoint")
            .and_then(|n| n.get("browseEndpoint"))
            .and_then(Value::as_object)?;
        let browse_id = val_str(browse.get("browseId"));
        if browse_id.is_empty() {
            return None;
        }
        if kind == YouTubeEntityKind::Artist && !browse_id.starts_with("UC") {
            return None;
        }
        if kind == YouTubeEntityKind::Album && !browse_id.starts_with("MPRE") {
            return None;
        }
        let columns = r.get("flexColumns").and_then(Value::as_array);
        let name = columns
            .and_then(|cols| cols.first())
            .and_then(|c| c.get("musicResponsiveListItemFlexColumnRenderer"))
            .and_then(|m| m.get("text"))
            .and_then(|t| t.get("runs"))
            .and_then(runs_text)
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            return None;
        }
        let details: Vec<&Map<String, Value>> = columns
            .and_then(|cols| cols.get(1))
            .and_then(|c| c.get("musicResponsiveListItemFlexColumnRenderer"))
            .and_then(|m| m.get("text"))
            .and_then(|t| t.get("runs"))
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_object).collect())
            .unwrap_or_default();

        let mut artist = String::new();
        if kind == YouTubeEntityKind::Album {
            for run in &details {
                let bid = run
                    .get("navigationEndpoint")
                    .and_then(|n| n.get("browseEndpoint"))
                    .and_then(|b| b.get("browseId"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if bid.starts_with("UC") {
                    artist = val_str(run.get("text"));
                    break;
                }
            }
        }
        let subtitle = details
            .iter()
            .filter_map(|e| e.get("text").and_then(Value::as_str))
            .map(str::trim)
            .filter(|t| {
                !t.is_empty()
                    && !matches!(*t, "•" | "Artist" | "Album" | "EP" | "Single")
            })
            .collect::<Vec<_>>()
            .join(" • ");

        Some(YouTubeMusicEntity {
            kind,
            name,
            artist,
            subtitle,
            browse_id,
            playlist_id: find_string(&Value::Object(r.clone()), "playlistId").unwrap_or_default(),
            artwork_url: renderer_artwork(r),
        })
    }

    fn parse_playlist_renderers(&self, root: &Value) -> Vec<YouTubePlaylistSummary> {
        let mut renderers = Vec::new();
        for k in [
            "musicResponsiveListItemRenderer",
            "musicTwoRowItemRenderer",
            "gridPlaylistRenderer",
            "musicGridItemRenderer",
            "playlistRenderer",
        ] {
            collect_objects(root, k, &mut renderers);
        }
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for r in renderers {
            if let Some(s) = parse_playlist_summary(&r) {
                if seen.insert(s.id.clone()) {
                    out.push(s);
                }
            }
        }
        out
    }

    fn playlist_track_containers(&self, root: &Value) -> Vec<Value> {
        let mut shelves = Vec::new();
        collect_objects(root, "musicPlaylistShelfRenderer", &mut shelves);
        collect_objects(root, "musicPlaylistShelfContinuation", &mut shelves);
        collect_objects(root, "playlistVideoListRenderer", &mut shelves);
        collect_objects(root, "playlistVideoListContinuation", &mut shelves);
        if !shelves.is_empty() {
            return shelves.into_iter().map(Value::Object).collect();
        }
        let mut music_shelves = Vec::new();
        collect_objects(root, "musicShelfRenderer", &mut music_shelves);
        for shelf in music_shelves {
            let v = Value::Object(shelf);
            if !self.parse_song_renderers(&v).is_empty() {
                return vec![v];
            }
        }
        if let Some(cont) = root
            .get("continuationContents")
            .and_then(|c| c.get("musicShelfContinuation"))
        {
            if cont.is_object() {
                return vec![cont.clone()];
            }
        }
        let mut out = Vec::new();
        for key in [
            "onResponseReceivedActions",
            "onResponseReceivedEndpoints",
            "onResponseReceivedCommands",
        ] {
            let Some(arr) = root.get(key).and_then(Value::as_array) else {
                continue;
            };
            for action in arr {
                let items = action
                    .get("appendContinuationItemsAction")
                    .and_then(|a| a.get("continuationItems"))
                    .or_else(|| {
                        action
                            .get("reloadContinuationItemsCommand")
                            .and_then(|r| r.get("continuationItems"))
                    });
                if let Some(it) = items {
                    out.push(it.clone());
                }
            }
        }
        out
    }

    fn continuation_token(&self, containers: &[Value], playlist_page: bool) -> Option<String> {
        for c in containers {
            let tok = if playlist_page {
                self.playlist_track_continuation_token(c)
            } else {
                generic_continuation_token(c)
            };
            if tok.as_deref().is_some_and(|t| !t.is_empty()) {
                return tok;
            }
        }
        None
    }

    fn playlist_track_continuation_token(&self, container: &Value) -> Option<String> {
        let contents = container
            .as_array()
            .or_else(|| container.get("contents").and_then(Value::as_array));
        if let Some(arr) = contents {
            if let Some(endpoint) = arr
                .last()
                .and_then(|l| l.get("continuationItemRenderer"))
                .and_then(|c| c.get("continuationEndpoint"))
            {
                let mut cmds = Vec::new();
                collect_objects(endpoint, "continuationCommand", &mut cmds);
                for cmd in cmds {
                    let token = val_str(cmd.get("token"));
                    let request = cmd.get("request").and_then(Value::as_str);
                    if !token.is_empty()
                        && (request.is_none()
                            || request == Some("CONTINUATION_REQUEST_TYPE_BROWSE"))
                    {
                        return Some(token);
                    }
                }
            }
        }
        if let Some(conts) = container.get("continuations").and_then(Value::as_array) {
            for c in conts {
                if let Some(token) = c
                    .get("nextContinuationData")
                    .and_then(|n| n.get("continuation"))
                    .and_then(Value::as_str)
                {
                    if !token.is_empty() {
                        return Some(token.to_string());
                    }
                }
            }
        }
        None
    }

    fn playlist_header(&self, root: &Value) -> Option<Map<String, Value>> {
        let first_header = |keys: &[&str]| -> Option<Map<String, Value>> {
            for k in keys {
                let mut found = Vec::new();
                collect_objects(root, k, &mut found);
                if let Some(first) = found.into_iter().next() {
                    return Some(first);
                }
            }
            None
        };

        if let Some(header) = root.get("header").and_then(Value::as_object) {
            let at = |path: &[&str]| -> Option<Map<String, Value>> {
                let mut node = Value::Object(header.clone());
                for p in path {
                    node = node.get(*p)?.clone();
                }
                node.as_object().cloned()
            };
            return at(&["musicDetailHeaderRenderer"])
                .or_else(|| at(&["musicResponsiveHeaderRenderer"]))
                .or_else(|| {
                    at(&[
                        "musicEditablePlaylistDetailHeaderRenderer",
                        "header",
                        "musicResponsiveHeaderRenderer",
                    ])
                })
                .or_else(|| {
                    at(&[
                        "musicEditablePlaylistDetailHeaderRenderer",
                        "header",
                        "musicDetailHeaderRenderer",
                    ])
                })
                .or_else(|| at(&["musicEditablePlaylistDetailHeaderRenderer"]))
                .or_else(|| at(&["musicVisualHeaderRenderer"]))
                .or_else(|| at(&["musicHeaderRenderer"]))
                .or_else(|| at(&["playlistHeaderRenderer"]))
                .or_else(|| {
                    first_header(&[
                        "musicResponsiveHeaderRenderer",
                        "musicDetailHeaderRenderer",
                        "musicEditablePlaylistDetailHeaderRenderer",
                        "musicVisualHeaderRenderer",
                        "musicHeaderRenderer",
                    ])
                });
        }

        first_header(&[
            "musicResponsiveHeaderRenderer",
            "musicDetailHeaderRenderer",
            "musicEditablePlaylistDetailHeaderRenderer",
            "musicVisualHeaderRenderer",
            "musicHeaderRenderer",
        ])
    }

    fn extract_title_from_header(
        &self,
        header: Option<&Map<String, Value>>,
        root: &Value,
    ) -> Option<String> {
        if let Some(h) = header {
            if let Some(text) = h
                .get("title")
                .and_then(|t| t.get("runs"))
                .and_then(runs_text)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                return Some(text);
            }
            if let Some(nested) = h.get("header").and_then(Value::as_object) {
                let inner = nested
                    .get("musicResponsiveHeaderRenderer")
                    .or_else(|| nested.get("musicDetailHeaderRenderer"))
                    .and_then(Value::as_object)
                    .unwrap_or(nested);
                if let Some(text) = inner
                    .get("title")
                    .and_then(|t| t.get("runs"))
                    .and_then(runs_text)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    return Some(text);
                }
            }
            if let Some(simple) = h
                .get("title")
                .and_then(|t| t.get("simpleText").or(Some(t)))
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                return Some(simple);
            }
        }
        let mut titles = Vec::new();
        collect_objects(root, "musicResponsiveHeaderRenderer", &mut titles);
        for h in titles {
            if let Some(text) = h
                .get("title")
                .and_then(|t| t.get("runs"))
                .and_then(runs_text)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                return Some(text);
            }
        }
        None
    }

    fn extract_artwork_from_header(
        &self,
        header: Option<&Map<String, Value>>,
        root: &Value,
    ) -> Option<String> {
        if let Some(h) = header {
            if let Some(url) = extract_thumbnails_url(&Value::Object(h.clone())) {
                return Some(url);
            }
        }
        for k in [
            "musicResponsiveHeaderRenderer",
            "musicDetailHeaderRenderer",
            "musicEditablePlaylistDetailHeaderRenderer",
            "musicVisualHeaderRenderer",
            "musicThumbnailRenderer",
        ] {
            let mut found = Vec::new();
            collect_objects(root, k, &mut found);
            for f in found {
                if let Some(url) = extract_thumbnails_url(&Value::Object(f)) {
                    return Some(url);
                }
            }
        }
        None
    }
}

impl AlbumLookupApi for InnerTubeMusicApi {
    type AlbumItem = YouTubeMusicEntity;
    type SongItem = YouTubeMusicTrack;

    fn lookup_albums<'a>(
        &'a self,
        query: &'a str,
        limit: usize,
    ) -> AlbumBoxFuture<'a, Result<Vec<Self::AlbumItem>, String>> {
        Box::pin(async move {
            self.search_albums(query, limit)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn lookup_songs<'a>(
        &'a self,
        query: &'a str,
        limit: usize,
    ) -> AlbumBoxFuture<'a, Result<Vec<Self::SongItem>, String>> {
        Box::pin(async move {
            self.search_songs(query, limit)
                .await
                .map_err(|e| e.to_string())
        })
    }
}

// Standalone Utility Helpers

fn add_cookie_pair(pairs: &mut BTreeMap<String, String>, name: &str, value: &str) {
    let n = name.trim();
    let mut v = value.trim();
    if n.is_empty() || v.is_empty() {
        return;
    }
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"'))
            || (v.starts_with('\'') && v.ends_with('\'')))
    {
        v = &v[1..v.len() - 1];
    }
    if v.is_empty() {
        return;
    }
    pairs.insert(n.to_string(), v.to_string());
}

fn now_unix_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn find_config(html: &str, key: &str) -> Option<String> {
    if html.is_empty() {
        return None;
    }
    let pattern = format!(r#""{}"\s*:\s*"([^"]+)""#, regex::escape(key));
    let re = Regex::new(&pattern).ok()?;
    let m = re.captures(html)?.get(1)?.as_str();
    Some(
        m.replace(r"\u003d", "=")
            .replace(r"\x3d", "=")
            .replace(r"\/", "/"),
    )
}

fn val_str(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").to_string()
}

fn scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn parse_http_date(raw: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc2822(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in [
        "%a, %d-%b-%Y %H:%M:%S GMT",
        "%a, %d %b %Y %H:%M:%S GMT",
        "%A, %d-%b-%y %H:%M:%S GMT",
        "%a %b %e %H:%M:%S %Y",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw.trim(), fmt) {
            return Some(DateTime::from_naive_utc_and_offset(naive, Utc));
        }
    }
    None
}

fn is_digits(w: &str) -> bool {
    !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit())
}

fn is_confirmed_unavailable(reason: &str) -> bool {
    let lower = reason.to_lowercase();
    CONFIRMED_UNAVAILABLE_REASONS
        .iter()
        .any(|r| lower.contains(r))
}

fn safe_error_detail(msg: &str) -> String {
    static RE_URL: OnceLock<Regex> = OnceLock::new();
    static RE_SPACE: OnceLock<Regex> = OnceLock::new();
    let stripped = RE_URL
        .get_or_init(|| Regex::new(r"https?://\S+").unwrap())
        .replace_all(msg, "<url>");
    let compact = RE_SPACE
        .get_or_init(|| Regex::new(r"\s+").unwrap())
        .replace_all(&stripped, " ")
        .trim()
        .to_string();
    if compact.chars().count() > 160 {
        let prefix: String = compact.chars().take(160).collect();
        format!("{prefix}…")
    } else {
        compact
    }
}

fn prefer_video(next: &MusicVideoStream, prev: &MusicVideoStream) -> bool {
    let next_mp4 = i32::from(next.mime_type.contains("mp4"));
    let prev_mp4 = i32::from(prev.mime_type.contains("mp4"));
    if next_mp4 != prev_mp4 {
        next_mp4 > prev_mp4
    } else {
        next.bitrate > prev.bitrate
    }
}

fn mark_smallest_stream(stream: &ResolvedStream) -> ResolvedStream {
    let key = if let Some(rest) = stream.cache_key.strip_prefix("youtube:") {
        format!("youtube-min:{rest}")
    } else {
        format!("youtube-min:{}", stream.cache_key)
    };
    let mut cloned = stream.clone();
    cloned.cache_key = key;
    cloned
}

fn collect_objects(element: &Value, key: &str, out: &mut Vec<Map<String, Value>>) {
    match element {
        Value::Object(map) => {
            for (name, child) in map {
                if name == key {
                    if let Some(obj) = child.as_object() {
                        out.push(obj.clone());
                    }
                }
                collect_objects(child, key, out);
            }
        }
        Value::Array(arr) => {
            for child in arr {
                collect_objects(child, key, out);
            }
        }
        _ => {}
    }
}

fn find_string(element: &Value, key: &str) -> Option<String> {
    match element {
        Value::Object(map) => {
            if let Some(Value::String(s)) = map.get(key) {
                return Some(s.clone());
            }
            for v in map.values() {
                if let Some(found) = find_string(v, key) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => {
            for v in arr {
                if let Some(found) = find_string(v, key) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn runs_text(runs: &Value) -> Option<String> {
    let arr = runs.as_array()?;
    let mut out = String::new();
    for item in arr {
        if let Some(text) = item.get("text").and_then(Value::as_str) {
            out.push_str(text);
        }
    }
    Some(out)
}

fn search_handle_text(node: &Value) -> Option<String> {
    static RE_HANDLE: OnceLock<Regex> = OnceLock::new();
    let re = RE_HANDLE.get_or_init(|| Regex::new(r"@[\w.\-·]{1,39}").unwrap());
    match node {
        Value::String(s) => re.find(s).map(|m| m.as_str().to_string()),
        Value::Object(map) => {
            for k in ["runs", "simpleText", "text"] {
                if let Some(val) = map.get(k) {
                    if let Some(hit) = search_handle_text(val) {
                        return Some(hit);
                    }
                }
            }
            for (k, v) in map {
                if k == "runs" || k == "simpleText" || k == "text" {
                    continue;
                }
                if let Some(hit) = search_handle_text(v) {
                    return Some(hit);
                }
            }
            None
        }
        Value::Array(arr) => {
            for v in arr {
                if let Some(hit) = search_handle_text(v) {
                    return Some(hit);
                }
            }
            None
        }
        _ => None,
    }
}

fn account_email(h: &Map<String, Value>) -> String {
    match h.get("email") {
        Some(Value::Object(map)) => {
            if let Some(r) = map.get("runs").and_then(runs_text) {
                let t = r.trim();
                if !t.is_empty() {
                    return t.to_string();
                }
            }
            map.get("simpleText")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string()
        }
        Some(Value::String(s)) => s.trim().to_string(),
        _ => String::new(),
    }
}

fn switcher_page_id(item: &Map<String, Value>) -> String {
    static RE_PAGE_ID: OnceLock<Regex> = OnceLock::new();
    static RE_PAGE_PARAM: OnceLock<Regex> = OnceLock::new();
    let page_re = RE_PAGE_ID.get_or_init(|| Regex::new(r"^\d{15,25}$").unwrap());
    let param_re = RE_PAGE_PARAM.get_or_init(|| Regex::new(r"[?&]pageid=(\d+)").unwrap());

    let val = Value::Object(item.clone());
    let direct = find_string(&val, "pageId").unwrap_or_default().trim().to_string();
    if page_re.is_match(&direct) {
        return direct;
    }
    let url = find_string(&val, "signinUrl")
        .or_else(|| find_string(&val, "signinurl"))
        .unwrap_or_default();
    if let Some(caps) = param_re.captures(&url) {
        let from_url = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if page_re.is_match(from_url) {
            return from_url.to_string();
        }
    }
    String::new()
}

fn switcher_handle(item: &Map<String, Value>) -> String {
    let Some(node) = item.get("channelHandle") else {
        return String::new();
    };
    let runs = node
        .get("runs")
        .and_then(runs_text)
        .unwrap_or_default()
        .trim()
        .to_string();
    if runs.starts_with('@') {
        return runs;
    }
    search_handle_text(node).unwrap_or_default()
}

fn first_browse_endpoint(node: &Value) -> Option<Map<String, Value>> {
    match node {
        Value::Object(map) => {
            if let Some(be) = map.get("browseEndpoint").and_then(Value::as_object) {
                if !val_str(be.get("browseId")).is_empty() {
                    return Some(be.clone());
                }
            }
            for v in map.values() {
                if let Some(found) = first_browse_endpoint(v) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => {
            for v in arr {
                if let Some(found) = first_browse_endpoint(v) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn more_endpoint_of_carousel(carousel: &Map<String, Value>) -> Option<Map<String, Value>> {
    let header = carousel.get("header")?.as_object()?;
    for h in header.values() {
        if let Some(btn) = h.get("moreContentButton") {
            return first_browse_endpoint(btn);
        }
    }
    None
}

fn home_shelf_title(node: Option<&Value>) -> Option<String> {
    let obj = node?.as_object()?;
    let text = val_str(obj.get("text")).trim().to_string();
    if !text.is_empty() {
        return Some(text);
    }
    let runs = obj.get("runs").and_then(runs_text)?;
    let trimmed = runs.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn album_header_artist(header: Option<&Map<String, Value>>) -> Option<String> {
    let h = header?;
    for key in ["subtitle", "straplineTextOne"] {
        let Some(runs) = h
            .get(key)
            .and_then(|v| v.get("runs"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for run in runs {
            let bid = run
                .get("navigationEndpoint")
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if bid.starts_with("UC") {
                let text = run.get("text").and_then(Value::as_str).unwrap_or("").trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }
        for run in runs {
            let text = run.get("text").and_then(Value::as_str).unwrap_or("").trim();
            if text.is_empty() {
                continue;
            }
            let lower = text.to_lowercase();
            if matches!(lower.as_str(), "album" | "single" | "ep" | "song" | "playlist" | "•")
                || (text.len() == 4 && text.chars().all(|c| c.is_ascii_digit()))
            {
                continue;
            }
            return Some(text.to_string());
        }
    }
    None
}

fn first_run_text(node: Option<&Value>) -> Option<String> {
    let runs = node?.get("runs")?.as_array()?;
    let text = runs.first()?.get("text")?.as_str()?;
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn find_first_author(header: &Map<String, Value>) -> Option<String> {
    for k in ["subtitle", "straplineTextOne", "secondSubtitle"] {
        if let Some(text) = first_run_text(header.get(k)) {
            return Some(text);
        }
    }
    None
}

fn generic_continuation_token(root: &Value) -> Option<String> {
    let mut cmds = Vec::new();
    collect_objects(root, "continuationCommand", &mut cmds);
    for cmd in cmds {
        let token = val_str(cmd.get("token"));
        if !token.is_empty() {
            return Some(token);
        }
    }
    None
}

fn renderer_artwork(r: &Map<String, Value>) -> String {
    let Some(thumb) = r.get("thumbnail").and_then(Value::as_object) else {
        return String::new();
    };
    if let Some(arr) = thumb
        .get("musicThumbnailRenderer")
        .and_then(|m| m.get("thumbnail"))
        .and_then(|t| t.get("thumbnails"))
        .and_then(Value::as_array)
    {
        if let Some(url) = arr.last().and_then(|u| u.get("url")).and_then(Value::as_str) {
            return InnerTubeMusicApi::high_resolution_artwork(url);
        }
    }
    if let Some(arr) = thumb.get("thumbnails").and_then(Value::as_array) {
        if let Some(url) = arr.last().and_then(|u| u.get("url")).and_then(Value::as_str) {
            return InnerTubeMusicApi::high_resolution_artwork(url);
        }
    }
    String::new()
}

fn extract_thumbnails_url(renderer: &Value) -> Option<String> {
    fn find<'a>(el: &'a Value, arrays: &mut Vec<&'a Vec<Value>>) {
        match el {
            Value::Object(map) => {
                if let Some(Value::Array(th)) = map.get("thumbnails") {
                    if !th.is_empty() {
                        arrays.push(th);
                    }
                }
                for v in map.values() {
                    find(v, arrays);
                }
            }
            Value::Array(arr) => {
                for v in arr {
                    find(v, arrays);
                }
            }
            _ => {}
        }
    }

    let node = match renderer {
        Value::Object(map) => map
            .get("thumbnail")
            .or_else(|| map.get("thumbnailRenderer"))
            .unwrap_or(renderer),
        other => other,
    };
    let mut arrays = Vec::new();
    find(node, &mut arrays);
    let arr = arrays.first()?;
    let url = arr
        .iter()
        .rev()
        .find_map(|e| {
            e.get("url")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            arr.iter().find_map(|e| {
                e.get("url")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
            })
        })?;
    Some(InnerTubeMusicApi::high_resolution_artwork(url))
}

fn duration_from_renderer(r: &Map<String, Value>) -> i64 {
    let length_seconds = r
        .get("lengthSeconds")
        .and_then(|v| v.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| v.as_i64()))
        .unwrap_or(0);
    if length_seconds > 0 && length_seconds <= 24 * 3600 {
        return length_seconds;
    }

    let from_text = |text: &Value| -> Option<i64> {
        match text {
            Value::String(s) => InnerTubeMusicApi::parse_duration(s),
            Value::Object(map) => {
                if let Some(simple) = map.get("simpleText").and_then(Value::as_str) {
                    if let Some(d) = InnerTubeMusicApi::parse_duration(simple) {
                        return Some(d);
                    }
                }
                if let Some(runs) = map.get("runs").and_then(Value::as_array) {
                    for run in runs {
                        if let Some(t) = run.get("text").and_then(Value::as_str) {
                            if let Some(d) = InnerTubeMusicApi::parse_duration(t) {
                                return Some(d);
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    };

    for key in ["flexColumns", "fixedColumns"] {
        if let Some(cols) = r.get(key).and_then(Value::as_array) {
            for col in cols {
                if let Some(obj) = col.as_object() {
                    for renderer in obj.values() {
                        if let Some(t) = renderer.get("text") {
                            if let Some(d) = from_text(t) {
                                return d;
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(lt) = r.get("lengthText") {
        if let Some(d) = from_text(lt) {
            return d;
        }
    }

    let mut overlays = Vec::new();
    collect_objects(&Value::Object(r.clone()), "thumbnailOverlayTimeStatusRenderer", &mut overlays);
    for o in overlays {
        if let Some(t) = o.get("text") {
            if let Some(d) = from_text(t) {
                return d;
            }
        }
    }
    0
}

fn is_useful_detail(value: &str) -> bool {
    let v = value.trim();
    !v.is_empty() && !matches!(v, "•" | "·" | "Song" | "Video")
}

fn is_likely_artist_detail(value: &str) -> bool {
    let v = value.trim();
    if !is_useful_detail(v) {
        return false;
    }
    let lower = v.to_lowercase();
    if matches!(lower.as_str(), "album" | "single" | "ep" | "playlist") {
        return false;
    }
    if InnerTubeMusicApi::parse_duration(v).is_some() {
        return false;
    }
    if v.len() == 4
        && (v.starts_with("19") || v.starts_with("20"))
        && v.chars().all(|c| c.is_ascii_digit())
    {
        return false;
    }
    if lower.contains(" view") || lower.contains(" song") {
        return false;
    }
    true
}

fn parse_playlist_summary(r: &Map<String, Value>) -> Option<YouTubePlaylistSummary> {
    let nav = r
        .get("navigationEndpoint")
        .and_then(|n| n.get("browseEndpoint"))
        .and_then(Value::as_object)
        .or_else(|| {
            r.get("title")
                .and_then(|t| t.get("runs"))
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(|f| f.get("navigationEndpoint"))
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(Value::as_object)
        })
        .or_else(|| {
            r.get("thumbnailOverlay")
                .and_then(|o| o.get("musicItemThumbnailOverlayRenderer"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.get("musicPlayButtonRenderer"))
                .and_then(|p| p.get("playNavigationEndpoint"))
                .and_then(|n| n.get("watchEndpoint"))
                .and_then(Value::as_object)
        })
        .or_else(|| {
            r.get("onTap")
                .and_then(|o| o.get("browseEndpoint"))
                .and_then(Value::as_object)
        })?;

    let browse_id = nav
        .get("browseId")
        .or_else(|| nav.get("playlistId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if browse_id.is_empty() {
        return None;
    }
    let playlist_id = browse_id.strip_prefix("VL").unwrap_or(browse_id).to_string();

    let flex = r.get("flexColumns").and_then(Value::as_array);
    let title = flex
        .and_then(|cols| cols.first())
        .and_then(|c| c.get("musicResponsiveListItemFlexColumnRenderer"))
        .and_then(|m| m.get("text"))
        .and_then(|t| t.get("runs"))
        .and_then(runs_text)
        .or_else(|| {
            let t = r.get("title")?;
            t.get("runs")
                .and_then(runs_text)
                .or_else(|| t.get("simpleText").and_then(Value::as_str).map(str::to_string))
        })?;
    if title.is_empty() {
        return None;
    }

    let subtitle_runs = flex
        .and_then(|cols| cols.get(1))
        .and_then(|c| c.get("musicResponsiveListItemFlexColumnRenderer"))
        .and_then(|m| m.get("text"))
        .and_then(|t| t.get("runs"))
        .and_then(Value::as_array)
        .or_else(|| {
            r.get("subtitle")
                .and_then(|s| s.get("runs"))
                .and_then(Value::as_array)
        });

    let mut author = String::new();
    let mut track_count_text = String::new();
    if let Some(runs) = subtitle_runs {
        for e in runs {
            let bid = e
                .get("navigationEndpoint")
                .and_then(|n| n.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if bid.starts_with("UC") {
                author = val_str(e.get("text"));
                break;
            }
        }
        if author.is_empty() {
            if let Some(first) = runs.first().and_then(|f| f.get("text")).and_then(Value::as_str) {
                if !first.eq_ignore_ascii_case("playlist") {
                    author = first.to_string();
                }
            }
        }
        for e in runs {
            let txt = e.get("text").and_then(Value::as_str).unwrap_or("");
            let low = txt.to_lowercase();
            if low.contains("song") || low.contains("track") {
                track_count_text = txt.to_string();
                break;
            }
        }
    }

    Some(YouTubePlaylistSummary {
        id: playlist_id,
        title: title.trim().to_string(),
        author: author.trim().to_string(),
        track_count_text,
        artwork_url: extract_thumbnails_url(&Value::Object(r.clone())).unwrap_or_default(),
    })
}

// Unit Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_cookies_supports_header_netscape_and_json() {
        let header = r#"SAPISID="abc/123"; SID=xyz; PREF=f6=400"#;
        let norm1 = InnerTubeMusicApi::normalize_cookies(header).expect("header ok");
        assert_eq!(norm1, "PREF=f6=400; SAPISID=abc/123; SID=xyz");

        let netscape = "\
# Netscape HTTP Cookie File
#HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t1900000000\t__Secure-3PAPISID\t3ptok
.youtube.com\tTRUE\t/\tTRUE\t1900000000\tSID\tsidval
";
        let norm2 = InnerTubeMusicApi::normalize_cookies(netscape).expect("netscape ok");
        assert_eq!(norm2, "SID=sidval; __Secure-3PAPISID=3ptok");

        let json_arr = r#"[{"name":"APISID","value":"ap1"},{"name":"HSID","value":"h1"}]"#;
        let norm3 = InnerTubeMusicApi::normalize_cookies(json_arr).expect("json ok");
        assert_eq!(norm3, "APISID=ap1; HSID=h1");

        let missing_sapisid = InnerTubeMusicApi::normalize_cookies("SID=1; HSID=2");
        assert!(missing_sapisid.is_err());
    }

    #[test]
    fn merge_rotated_cookies_updates_existing_scoped_keys_only() {
        let jar = "SAPISID=old_sapi; __Secure-1PAPISIDTS=old_ts; SID=keep_me";
        let set_cookies = vec![
            "__Secure-1PAPISIDTS=new_ts; Domain=.youtube.com; Path=/; Secure".to_string(),
            "VISITOR_INFO1_LIVE=ignore_untracked; Domain=.youtube.com".to_string(),
            "SID=deleted; Max-Age=0; Domain=.youtube.com".to_string(),
            "SAPISID=evil; Domain=.example.com".to_string(),
        ];
        let rotated = InnerTubeMusicApi::merge_rotated_cookies(
            jar,
            &set_cookies,
            "music.youtube.com",
        )
        .expect("should update __Secure-1PAPISIDTS");
        assert_eq!(
            rotated,
            "SAPISID=old_sapi; __Secure-1PAPISIDTS=new_ts; SID=keep_me"
        );
    }

    #[test]
    fn sort_stream_candidates_prefers_audio_only_adaptive_and_tier() {
        let mk = |itag: i64, bitrate: i64, adaptive: bool, audio_only: bool, mime: &str| {
            StreamCandidate {
                stream: ResolvedStream {
                    url: format!("https://example.com/{itag}"),
                    mime_type: mime.to_string(),
                    bitrate_kbps: (bitrate / 1000) as i32,
                    ..Default::default()
                },
                adaptive,
                bitrate,
                itag,
                is_audio_only: audio_only,
            }
        };
        let mut list = vec![
            mk(18, 96_000, false, false, "video/mp4"),
            mk(251, 160_000, true, true, "audio/webm"),
            mk(249, 50_000, true, true, "audio/webm"),
            mk(999, 128_000, false, false, "application/vnd.apple.mpegurl"),
        ];
        InnerTubeMusicApi::sort_stream_candidates(&mut list, false);
        assert_eq!(list[0].itag, 251);
        assert_eq!(list[1].itag, 249);
        assert_eq!(list[2].itag, 18);
        assert_eq!(list[3].itag, 999);

        InnerTubeMusicApi::sort_stream_candidates(&mut list, true);
        assert_eq!(list[0].itag, 249);
        assert_eq!(list[1].itag, 251);
    }

    #[test]
    fn match_scoring_and_normalization_parity() {
        assert_eq!(InnerTubeMusicApi::normalize("Ke$ha – TiK ToK"), "kesha tik tok");
        assert_eq!(
            InnerTubeMusicApi::core_song_title("Artist - Song Name (Official Video) ft. Guest", "Artist"),
            "Song Name"
        );
        let orig = YouTubeMusicTrack {
            video_id: "id1".to_string(),
            title: "Blinding Lights".to_string(),
            artist: "The Weeknd".to_string(),
            duration_seconds: 200,
            ..Default::default()
        };
        let remix = YouTubeMusicTrack {
            video_id: "id2".to_string(),
            title: "Blinding Lights (Slowed + Reverb Remix)".to_string(),
            artist: "The Weeknd".to_string(),
            duration_seconds: 230,
            ..Default::default()
        };
        assert!(
            InnerTubeMusicApi::match_score(&orig, "Blinding Lights", "The Weeknd")
                > InnerTubeMusicApi::match_score(&remix, "Blinding Lights", "The Weeknd")
        );

        let bieber = YouTubeMusicTrack {
            video_id: "DcDbKDAb7go".to_string(),
            title: "What Do You Mean?".to_string(),
            artist: "Justin Bieber".to_string(),
            ..Default::default()
        };
        let cover = YouTubeMusicTrack {
            video_id: "BlGOExzHJVI".to_string(),
            title: "What Do You Mean".to_string(),
            artist: "SibeRya".to_string(),
            ..Default::default()
        };
        let title_tagged = YouTubeMusicTrack {
            video_id: "vid3".to_string(),
            title: "Justin Bieber - What Do You Mean? (Audio)".to_string(),
            artist: "Pop Hits".to_string(),
            ..Default::default()
        };
        assert!(InnerTubeMusicApi::candidate_shares_artist(&bieber, "Justin Bieber"));
        assert!(!InnerTubeMusicApi::candidate_shares_artist(&cover, "Justin Bieber"));
        assert!(InnerTubeMusicApi::candidate_shares_artist(&title_tagged, "Justin Bieber"));
        assert!(InnerTubeMusicApi::candidate_shares_artist(&cover, ""));
        assert!(InnerTubeMusicApi::candidate_shares_artist(&cover, "Unknown Artist"));
    }

    #[tokio::test]
    async fn loopback_proxy_starts_and_handles_missing_token() {
        let client = HttpClientFactory::create().expect("client");
        let server = LocalLoopbackStreamServer::start(client.clone())
            .await
            .expect("loopback server");
        let url = server.register(
            "tok1",
            ResolvedStream {
                url: "https://example.invalid/audio.webm".to_string(),
                ..Default::default()
            },
        );
        assert!(url.starts_with(&format!("http://127.0.0.1:{}/stream/tok1", server.port())));
        let missing_res = client
            .get(format!("http://127.0.0.1:{}/stream/unknown", server.port()))
            .send()
            .await
            .expect("request");
        assert_eq!(missing_res.status().as_u16(), 404);
    }

    #[test]
    fn parse_switcher_channels_extracts_runs_name_and_preserves_avatar_url() {
        let api = InnerTubeMusicApi::with_defaults().expect("api");
        let payload = json!({
            "actions": [{
                "accountItem": {
                    "accountName": { "runs": [{ "text": "Metanoia Music" }] },
                    "channelHandle": { "runs": [{ "text": "@TheMetanoia17" }] },
                    "accountPhoto": {
                        "thumbnails": [{
                            "url": "https://yt3.ggpht.com/ytc/AIdro_test=s88-c-k-c0x00ffffff-no-rj"
                        }]
                    },
                    "pageId": "108456789012345678901"
                }
            }]
        });
        let channels = api.parse_switcher_channels(&payload);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].name, "Metanoia Music");
        assert_eq!(channels[0].handle, "@TheMetanoia17");
        assert_eq!(channels[0].page_id, "108456789012345678901");
        assert_eq!(
            channels[0].photo_url,
            "https://yt3.ggpht.com/ytc/AIdro_test=s88-c-k-c0x00ffffff-no-rj"
        );
    }

    #[test]
    fn high_resolution_artwork_preserves_avatars_and_query_params() {
        assert_eq!(
            InnerTubeMusicApi::high_resolution_artwork(
                "https://lh3.googleusercontent.com/abc=w120-h120-l90-rj"
            ),
            "https://lh3.googleusercontent.com/abc=w512-h512-l90-rj"
        );
        assert_eq!(
            InnerTubeMusicApi::high_resolution_artwork(
                "//lh3.googleusercontent.com/abc=w60-h60-l90-rj?foo=bar"
            ),
            "https://lh3.googleusercontent.com/abc=w512-h512-l90-rj?foo=bar"
        );
        assert_eq!(
            InnerTubeMusicApi::high_resolution_artwork(
                "https://yt3.ggpht.com/ytc/avatar=s88-c-k-c0x00ffffff-no-rj"
            ),
            "https://yt3.ggpht.com/ytc/avatar=s88-c-k-c0x00ffffff-no-rj"
        );
        assert_eq!(
            InnerTubeMusicApi::high_resolution_artwork(
                "https://lh3.googleusercontent.com/xyz=w544-h544-p-k-no-nu"
            ),
            "https://lh3.googleusercontent.com/xyz=w544-h544-p-k-no-nu"
        );
    }
}
