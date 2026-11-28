//! Shared Tauri application state holding backend service & store handles
//! plus `AppHandle` event emission helpers.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::audio::equalizer_controller::EqualizerController;
use crate::audio::output_controller::AudioOutputController;
use crate::audio::playback_service::{
    NoopLosslessBackend, PlaybackService, PrefsHandle, SessionStore,
};
use crate::audio::spectrum_stream::SpectrumStreamController;
use crate::extractor::http_client::HttpClientFactory;
use crate::extractor::innertube_api::InnerTubeMusicApi;
use crate::extractor::potoken_engine::PoTokenEngine;
use crate::extractor::yt_library_sync::YtLibrarySync;
use crate::extractor::yt_session_keeper::YtSessionKeeper;
use crate::integrations::artwork::animated_artwork_service::AnimatedArtworkService;
use crate::integrations::artwork::official_artwork_service::OfficialArtworkService;
use crate::integrations::feed::feed_repository::{FeedHooks, FeedRepository};
use crate::integrations::jam::jam_client::JamController;
use crate::integrations::jam::jam_sync::JamSyncBridge;
use crate::integrations::lastfm::auth_repository::AuthRepository;
use crate::integrations::lastfm::home_repository::HomeRepository;
use crate::integrations::lastfm::lastfm_api::LastFmApiService;
use crate::integrations::lastfm::rate_guard::LastFmRateGuard;
use crate::integrations::lastfm::scrobble_repository::ScrobbleRepository;
use crate::integrations::lyrics::lyrics_repository::LyricsRepository;
use crate::integrations::presence::discord_presence_service::DiscordPresenceService;
use crate::integrations::scrobblers::extra_scrobblers::{
    ExtraScrobbler, ExtraScrobblers, LibreFmScrobbler, ListenBrainzScrobbler,
};
use crate::integrations::scrobblers::scrobbler_session::ScrobblerKind;
use crate::integrations::search::search_repository::SearchRepository;
use crate::integrations::spotify::spotify_api::SpotifyApi;
use crate::integrations::spotify::spotify_auth::SpotifyAuth;
use crate::integrations::updates::update_service::UpdateService;
use crate::storage::app_database::AppDatabase;
use crate::storage::download_manager::DownloadManager;
use crate::storage::playlists::PlaylistRepository;
use crate::storage::prefs::Prefs;
use crate::storage::secure_store::SecureStore;

/// Central Tauri managed state holding all backend services and stores.
pub struct AppState {
    pub app_support_dir: PathBuf,
    pub http: reqwest::Client,
    pub prefs: Arc<Prefs>,
    pub db: Arc<AppDatabase>,
    pub secure: Arc<SecureStore>,
    pub po_tokens: Arc<PoTokenEngine>,
    pub tube: Arc<InnerTubeMusicApi>,
    pub session_keeper: Arc<YtSessionKeeper>,
    pub rate_guard: Arc<LastFmRateGuard>,
    pub lastfm_api: Arc<LastFmApiService>,
    pub auth_repo: Arc<AuthRepository>,
    pub listen_brainz: Arc<ListenBrainzScrobbler>,
    pub libre_fm: Arc<LibreFmScrobbler>,
    pub extra_scrobblers: Arc<ExtraScrobblers>,
    pub scrobbler: Arc<ScrobbleRepository>,
    pub lastfm_home: Arc<HomeRepository>,
    pub official_artwork: Arc<OfficialArtworkService>,
    pub animated_artwork: Arc<AnimatedArtworkService>,
    pub lyrics_repo: Arc<LyricsRepository>,
    pub search_repo: Arc<SearchRepository>,
    pub feed_repo: Arc<FeedRepository>,
    pub spotify_auth: Arc<SpotifyAuth>,
    pub spotify_api: Arc<SpotifyApi>,
    pub jam: Arc<JamController>,
    pub playlists: Arc<PlaylistRepository>,
    pub downloads: Arc<DownloadManager>,
    pub yt_library_sync: Arc<YtLibrarySync>,
    pub discord_presence: Arc<DiscordPresenceService>,
    pub update_service: Arc<UpdateService>,
    pub equalizer: Arc<EqualizerController>,
    pub audio_output: Arc<AudioOutputController>,
    pub playback: Arc<PlaybackService>,
    pub spectrum: Arc<SpectrumStreamController>,
    pub jam_sync: Arc<JamSyncBridge>,
    app_handle: Mutex<Option<AppHandle>>,
}

impl AppState {
    /// Resolves the application support directory:
    /// 1) Portable mode: `<exe_dir>/data` when `--portable`, `<exe_dir>/.portable`,
    ///    or an existing `<exe_dir>/data` folder is present.
    /// 2) Desktop mode: `%APPDATA%/Clash-Projects/LastWave` (matching official
    ///    LastWave Desktop builds so existing library, stats, and prefs persist).
    pub fn default_app_support_dir() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let portable_data = exe_dir.join("data");
                let has_flag = std::env::args().any(|a| a == "--portable");
                if has_flag
                    || portable_data.is_dir()
                    || exe_dir.join(".portable").exists()
                    || exe_dir.join("lastwave.db").exists()
                {
                    return portable_data;
                }
            }
        }
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Clash-Projects")
            .join("LastWave")
    }

    /// Initializes all backend stores and services rooted at `app_support_dir`.
    pub fn new(app_support_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&app_support_dir);

        let http = HttpClientFactory::create().unwrap_or_else(|_| reqwest::Client::new());
        let prefs = Arc::new(Prefs::load(&app_support_dir));
        crate::storage::diagnostics_log::DiagnosticsLog::init(
            &app_support_dir.join("logs"),
            false,
        );
        let db = Arc::new(AppDatabase::open(&app_support_dir));
        let secure = Arc::new(SecureStore::from_app_support_dir(&app_support_dir));

        let auth_store = Arc::new(crate::extractor::yt_auth_store::YtAuthStore::new(Arc::clone(&secure)));
        let po_tokens = Arc::new(PoTokenEngine::new(http.clone(), Some(auth_store), None));
        let tube = Arc::new(InnerTubeMusicApi::new(
            http.clone(),
            Arc::clone(&secure),
            Some(Arc::clone(&po_tokens)),
        ));
        tube.attach_disk_cache(Arc::clone(&db));
        let session_keeper = Arc::new(YtSessionKeeper::new());

        let rate_guard = Arc::new(LastFmRateGuard::new());
        let lastfm_api = Arc::new(LastFmApiService::new(Arc::clone(&rate_guard)));
        let auth_repo = Arc::new(AuthRepository::new(
            Arc::clone(&lastfm_api),
            Arc::clone(&prefs),
            Arc::clone(&secure),
        ));

        let listen_brainz = Arc::new(ListenBrainzScrobbler::new(Arc::clone(&prefs)));
        let libre_fm = Arc::new(LibreFmScrobbler::new(Arc::clone(&prefs)));
        let extra_services: Vec<Arc<dyn ExtraScrobbler>> = vec![
            Arc::clone(&listen_brainz) as Arc<dyn ExtraScrobbler>,
            Arc::clone(&libre_fm) as Arc<dyn ExtraScrobbler>,
        ];
        let extra_scrobblers = Arc::new(ExtraScrobblers::new(extra_services));
        let scrobbler = Arc::new(ScrobbleRepository::new(
            Arc::clone(&lastfm_api),
            Arc::clone(&prefs),
            Some(Arc::clone(&extra_scrobblers)),
        ));

        let active_scrobbler_kind = match prefs.primary_scrobbler().as_str() {
            "listenbrainz" => Some(ScrobblerKind::Listenbrainz),
            "librefm" => Some(ScrobblerKind::Librefm),
            _ => Some(ScrobblerKind::Lastfm),
        };
        let lastfm_home = Arc::new(HomeRepository::new(
            Arc::clone(&lastfm_api),
            Arc::clone(&prefs),
            active_scrobbler_kind,
            Some(Arc::clone(&listen_brainz)),
            Some(Arc::clone(&libre_fm)),
        ));

        let official_artwork = Arc::new(OfficialArtworkService::new(
            Some(http.clone()),
            Some(Arc::clone(&db)),
        ));
        let animated_artwork = Arc::new(AnimatedArtworkService::new(
            http.clone(),
            Some(Arc::clone(&db)),
            Some(Arc::clone(&official_artwork)),
            None,
        ));

        let lyrics_repo = Arc::new(LyricsRepository::new(
            Some(http.clone()),
            Some(Arc::clone(&tube)),
            Some(Arc::clone(&db)),
        ));
        let search_repo = Arc::new(SearchRepository::new(
            Arc::clone(&tube),
            Arc::clone(&db),
        ));
        let feed_repo = Arc::new(FeedRepository::new(
            Arc::clone(&lastfm_api),
            Arc::clone(&tube),
            Arc::clone(&lastfm_home),
            {
                let p = Arc::clone(&prefs);
                Arc::new(move || p.last_fm_api_key())
            },
            Some(Arc::clone(&db)),
            Some(Arc::clone(&official_artwork)),
            FeedHooks::default(),
        ));

        let spotify_auth = Arc::new(SpotifyAuth::new(
            Arc::clone(&secure),
            Arc::clone(&prefs),
            http.clone(),
        ));
        let spotify_api = Arc::new(SpotifyApi::new(Arc::clone(&spotify_auth)));
        let jam = Arc::new(JamController::new(Arc::clone(&prefs), http.clone()));

        let playlists = Arc::new(PlaylistRepository::new(Arc::clone(&db)));
        let downloads = Arc::new(DownloadManager::new(
            Arc::clone(&db),
            Arc::clone(&prefs),
            &app_support_dir,
        ));
        let yt_library_sync = Arc::new(YtLibrarySync::new(
            Arc::clone(&db),
            Arc::clone(&tube),
            Arc::clone(&playlists),
            {
                let p = Arc::clone(&prefs);
                Arc::new(move || p.sync_yt_library())
            },
            None,
            None,
        ));

        let discord_presence = DiscordPresenceService::new(Arc::clone(&prefs));
        let update_service = Arc::new(UpdateService::new());

        let equalizer = Arc::new(EqualizerController::new(Arc::clone(&prefs)));
        let audio_output = Arc::new(AudioOutputController::new(Arc::clone(&prefs)));
        let playback = PlaybackService::new(
            Arc::clone(&tube),
            Arc::new(NoopLosslessBackend),
            Arc::clone(&scrobbler),
            Arc::clone(&downloads),
            PrefsHandle::new(Arc::clone(&prefs)),
            SessionStore::new(Arc::clone(&db)),
            Arc::clone(&official_artwork),
            None,
        );
        let spectrum = Arc::new(SpectrumStreamController::new());
        let jam_sync = Arc::new(JamSyncBridge::new(
            Arc::clone(&jam),
            Arc::clone(&playback),
        ));

        Self {
            app_support_dir,
            http,
            prefs,
            db,
            secure,
            po_tokens,
            tube,
            session_keeper,
            rate_guard,
            lastfm_api,
            auth_repo,
            listen_brainz,
            libre_fm,
            extra_scrobblers,
            scrobbler,
            lastfm_home,
            official_artwork,
            animated_artwork,
            lyrics_repo,
            search_repo,
            feed_repo,
            spotify_auth,
            spotify_api,
            jam,
            playlists,
            downloads,
            yt_library_sync,
            discord_presence,
            update_service,
            equalizer,
            audio_output,
            playback,
            spectrum,
            jam_sync,
            app_handle: Mutex::new(None),
        }
    }

    /// Attaches the Tauri `AppHandle` once the application setup hook runs.
    pub fn set_app_handle(&self, handle: AppHandle) {
        self.po_tokens.set_host(Arc::new(
            crate::extractor::ephemeral_webview_broker::TauriBotGuardHost::new(handle.clone()),
        ));
        if let Ok(mut guard) = self.app_handle.lock() {
            *guard = Some(handle);
        }
    }

    /// Returns a cloned `AppHandle` if attached.
    pub fn app_handle(&self) -> Option<AppHandle> {
        self.app_handle.lock().ok().and_then(|g| g.clone())
    }

    /// Emits a Tauri event to all windows if `AppHandle` is attached.
    pub fn emit_event<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(handle) = self.app_handle() {
            let _ = handle.emit(event, payload);
        }
    }
}
