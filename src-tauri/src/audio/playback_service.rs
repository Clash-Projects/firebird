//! Desktop playback service built on deferred-loaded libmpv.
//!
//! Ported 1:1 from `lib/features/player/playback_service.dart`:
//! - Dual-deck libmpv (`_player` + `_standbyPlayer`) with deferred initialization,
//!   pinned mpv cache dir, extended demuxer buffering, and ffmpeg reconnect.
//! - Resolution fast path: local download -> lossless backend -> YouTube race ->
//!   same-tier / YouTube retry -> unavailable skip.
//! - Gapless, Crossfade (3-12s), and beat-matched / phase-locked AutoMix with
//!   `MixGuard` watchdog and `MixFaultBudget` step-down.
//! - WASAPI exclusive overlap guard, hardware-to-software volume fallback,
//!   auto-pause/device-recovery on endpoint disconnect or monitor sleep.
//! - Full DSP filter chain (10-band EQ + Crystalizer + Headphone + Scaletempo +
//!   `@lwdj` live mix EQ + ReplayGain loudness).
//! - Queue, shuffle/repeat, speed/pitch, sleep timer, endless radio, session
//!   persistence, Last.fm scrobbling, YT watch history, and SMTC/MPRIS hooks.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{CStr, CString};
use std::future::Future;
use std::os::raw::{c_char, c_int, c_void};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use regex::Regex;
use serde_json::{Map, Value};
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio::task::JoinHandle;

use crate::audio::audio_graph::merge_equalizer_and_headphone;
use crate::audio::automix_analyzer::{AnalysisSide, AutomixAnalyzer, LogHook, MpvSerializer, WantedHook};
use crate::audio::dj_mix_planner::{DjBeatLock, DjEq, DjMixPlan, DjMixPlanner};
use crate::audio::headphone_profiles::HeadphoneProfiles;
use crate::audio::loudness::LoudnessMath;
use crate::audio::mix_guard::{check_mix, DeckReading, MixFaultBudget, MixGuardInput, MixRepair};
use crate::audio::mix_planner::{is_filter_graph_error, MixPlan, MixPlanner, MixStyle, MixTrackInfo};
use crate::audio::mpris_service::{MprisAction, MprisControlHandler, MprisService};
use crate::audio::mpv_paths::{ensure_dir_exists_async, mpv_cache_dir_path};
use crate::audio::output_controller::PlaybackOutputSink;
use crate::audio::smtc_service::{MediaControlAction, SmtcService};
use crate::audio::wasapi_engine::wasapi_log;
use crate::extractor::innertube_api::{InnerTubeMusicApi, YouTubeMusicTrack};
use crate::integrations::artwork::official_artwork_service::OfficialArtworkService;
use crate::integrations::lastfm::scrobble_repository::ScrobbleRepository;
use crate::models::pcm_format::{mpv_format_is_float, mpv_sample_format, pcm_from_mpv_out_params, PcmFormat};
use crate::models::player_state::PlayerSnapshot;
use crate::models::stream_models::{
    AudioQualityTiers, PlayableTrack, RepeatMode, ResolvedStream, ResolvedStreamCache,
};
use crate::storage::app_database::AppDatabase;
use crate::storage::download_manager::DownloadManager;
use crate::storage::prefs::Prefs;
use crate::storage::trace_crumb::TraceCrumb;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ---- Injection Traits & Adapters ------------------------------------------

#[derive(Debug, Clone)]
pub enum LosslessResolveError {
    Quota(String),
    Other(String),
}

/// Lossless addon / catalog resolver contract injected into [`PlaybackService`].
pub trait LosslessSourceBackend: Send + Sync {
    fn is_configured(&self) -> bool;

    fn resolve_stream<'a>(
        &'a self,
        title: &'a str,
        artist: &'a str,
        album: &'a str,
        expected_duration_seconds: i64,
        preferred_quality: i64,
    ) -> BoxFuture<'a, Result<Option<ResolvedStream>, LosslessResolveError>>;

    fn invalidate_stream(&self, title: &str, artist: &str);

    fn clear_stream_cache(&self);
}

#[derive(Debug, Default)]
pub struct NoopLosslessBackend;

impl LosslessSourceBackend for NoopLosslessBackend {
    fn is_configured(&self) -> bool {
        false
    }

    fn resolve_stream<'a>(
        &'a self,
        _title: &'a str,
        _artist: &'a str,
        _album: &'a str,
        _expected_duration_seconds: i64,
        _preferred_quality: i64,
    ) -> BoxFuture<'a, Result<Option<ResolvedStream>, LosslessResolveError>> {
        Box::pin(async { Ok(None) })
    }

    fn invalidate_stream(&self, _title: &str, _artist: &str) {}

    fn clear_stream_cache(&self) {}
}

pub type SpotifyRadioFn =
    Arc<dyn Fn(PlayableTrack) -> BoxFuture<'static, Option<Vec<PlayableTrack>>> + Send + Sync>;
pub type AddonQuotaNoticeFn = Arc<dyn Fn(&str) + Send + Sync>;
pub type SnapshotListenerFn = Arc<dyn Fn(&PlayerSnapshot) + Send + Sync>;

/// Thin prefs adapter mirroring `PrefsHandle` in Dart.
#[derive(Clone)]
pub struct PrefsHandle {
    prefs: Arc<Prefs>,
}

impl PrefsHandle {
    pub fn new(prefs: Arc<Prefs>) -> Self {
        Self { prefs }
    }

    pub fn prefer_lossless(&self) -> bool {
        self.prefs.prefer_lossless()
    }
    pub fn lossless_quality(&self) -> i64 {
        self.prefs.lossless_quality()
    }
    pub fn scrobble_percent(&self) -> i64 {
        self.prefs.scrobble_percent()
    }
    pub fn scrobbler_enabled(&self) -> bool {
        self.prefs.scrobbler_enabled()
    }
    pub fn sync_yt_history(&self) -> bool {
        self.prefs.sync_yt_history()
    }
    pub fn crossfade_enabled(&self) -> bool {
        self.prefs.crossfade_enabled()
    }
    pub fn crossfade_seconds(&self) -> i64 {
        self.prefs.crossfade_seconds()
    }
    pub fn wasapi_exclusive(&self) -> bool {
        self.prefs.wasapi_exclusive()
    }
    pub fn loudness_mode(&self) -> String {
        self.prefs.loudness_mode()
    }
    pub fn headphone_profile(&self) -> String {
        self.prefs.headphone_profile()
    }
    pub fn auto_mix_enabled(&self) -> bool {
        self.prefs.auto_mix_enabled()
    }
    pub fn auto_mix_bars(&self) -> i64 {
        self.prefs.auto_mix_bars()
    }
    pub fn auto_mix_spectral(&self) -> bool {
        self.prefs.auto_mix_spectral()
    }
    pub fn equalizer_enabled(&self) -> bool {
        self.prefs.equalizer_enabled()
    }
    pub fn studio_clarity_enabled(&self) -> bool {
        self.prefs.studio_clarity_enabled()
    }
    pub fn recommendation_source(&self) -> String {
        self.prefs.recommendation_source()
    }
    pub fn raw_prefs(&self) -> &Arc<Prefs> {
        &self.prefs
    }
}

/// Thin session persistence adapter mirroring `SessionStore` in Dart.
#[derive(Clone)]
pub struct SessionStore {
    db: Arc<AppDatabase>,
}

impl SessionStore {
    pub fn new(db: Arc<AppDatabase>) -> Self {
        Self { db }
    }

    pub fn load(&self) -> Map<String, Value> {
        self.db.load_playback_session()
    }

    pub fn save(&self, payload: &Map<String, Value>) {
        self.db.save_playback_session(payload);
    }

    pub fn clear(&self) {
        self.db.clear_playback_session();
    }

    pub fn record_local_play(
        &self,
        title: &str,
        artist: &str,
        album: &str,
        duration_ms: i64,
    ) {
        self.db
            .record_local_play(title, artist, album, duration_ms);
    }

    pub fn update_latest_local_play_duration(
        &self,
        title: &str,
        artist: &str,
        duration_ms: i64,
    ) {
        self.db
            .update_latest_local_play_duration(title, artist, duration_ms);
    }
}

// ---- MpvDeck Abstraction & Deferred libmpv C ABI --------------------------

#[derive(Debug, Clone)]
pub struct DeckLog {
    pub prefix: String,
    pub level: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum DeckEvent {
    Playing(bool),
    Buffering(bool),
    Position(Duration),
    Buffered(Duration),
    DurationChanged(Duration),
    Volume(f64),
    Rate(f64),
    AudioParams,
    Completed,
    Log(DeckLog),
    Error(String),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DeckState {
    pub playing: bool,
    pub buffering: bool,
    pub position: Duration,
    pub buffered: Duration,
    pub duration: Duration,
    pub volume: f64,
    pub rate: f64,
}

#[derive(Debug, Clone)]
pub struct DeckConfig {
    pub role: &'static str,
    pub title: &'static str,
    pub buffer_bytes: usize,
    pub demuxer_max_bytes: &'static str,
    pub demuxer_max_back_bytes: &'static str,
    pub timed_mix_enabled: bool,
    pub initial_af: String,
    pub initial_audio_device: String,
    pub debug_ao: Option<String>,
    pub log_file: Option<PathBuf>,
    pub cache_dir: String,
}

pub trait MpvDeck: Send + Sync {
    fn id(&self) -> u64;
    fn state(&self) -> DeckState;
    fn set_property(&self, name: &str, value: &str) -> Result<(), String>;
    fn get_property(&self, name: &str) -> Option<String>;
    fn command(&self, args: &[&str]) -> Result<(), String>;
    fn open(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        start: Option<Duration>,
        play: bool,
    ) -> Result<(), String>;
    fn play(&self) -> Result<(), String>;
    fn pause(&self) -> Result<(), String>;
    fn play_or_pause(&self) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;
    fn seek(&self, position: Duration) -> Result<(), String>;
    fn set_volume(&self, volume: f64) -> Result<(), String>;
    fn set_rate(&self, rate: f64) -> Result<(), String>;
    fn set_audio_device(&self, device: &str) -> Result<(), String>;
    fn subscribe_events(&self) -> Option<mpsc::UnboundedReceiver<DeckEvent>>;
    fn dispose(&self);
}

pub trait MpvDeckFactory: Send + Sync {
    fn create(&self, config: DeckConfig) -> Result<Arc<dyn MpvDeck>, String>;
}

#[derive(Default)]
pub struct LibmpvDeckFactory;

impl MpvDeckFactory for LibmpvDeckFactory {
    fn create(&self, config: DeckConfig) -> Result<Arc<dyn MpvDeck>, String> {
        let deck = LibmpvDeck::create(config)?;
        Ok(Arc::new(deck))
    }
}

static NEXT_DECK_ID: AtomicU64 = AtomicU64::new(1);

#[repr(C)]
struct RawMpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct RawMpvEventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct RawMpvEventLogMessage {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}

#[repr(C)]
struct RawMpvEventEndFile {
    reason: c_int,
    error: c_int,
    playlist_entry_id: i64,
    playlist_insert_id: i64,
    playlist_insert_num_entries: c_int,
}

const MPV_EVENT_NONE: c_int = 0;
const MPV_EVENT_SHUTDOWN: c_int = 1;
const MPV_EVENT_LOG_MESSAGE: c_int = 2;
const MPV_EVENT_END_FILE: c_int = 7;
const MPV_EVENT_AUDIO_RECONFIG: c_int = 18;
const MPV_EVENT_PROPERTY_CHANGE: c_int = 22;

const MPV_FORMAT_STRING: c_int = 1;
const MPV_FORMAT_FLAG: c_int = 3;
const MPV_FORMAT_DOUBLE: c_int = 5;

const MPV_END_FILE_REASON_EOF: c_int = 0;
const MPV_END_FILE_REASON_ERROR: c_int = 4;

type FnMpvCreate = unsafe extern "C" fn() -> *mut c_void;
type FnMpvInitialize = unsafe extern "C" fn(*mut c_void) -> c_int;
type FnMpvTerminateDestroy = unsafe extern "C" fn(*mut c_void);
type FnMpvSetPropertyString =
    unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type FnMpvGetPropertyString = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char;
type FnMpvFree = unsafe extern "C" fn(*mut c_void);
type FnMpvCommand = unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int;
type FnMpvWaitEvent = unsafe extern "C" fn(*mut c_void, f64) -> *const RawMpvEvent;
type FnMpvObserveProperty =
    unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int;
type FnMpvRequestLogMessages = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
type FnMpvWakeup = unsafe extern "C" fn(*mut c_void);

struct LibmpvCtxPtr(*mut c_void);
unsafe impl Send for LibmpvCtxPtr {}
unsafe impl Sync for LibmpvCtxPtr {}

pub struct LibmpvDeck {
    id: u64,
    _lib: Arc<libloading::Library>,
    ctx: Mutex<Option<LibmpvCtxPtr>>,
    state: Arc<Mutex<DeckState>>,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<DeckEvent>>>,
    stop_flag: Arc<AtomicBool>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    mpv_set_property_string: FnMpvSetPropertyString,
    mpv_get_property_string: FnMpvGetPropertyString,
    mpv_free: FnMpvFree,
    mpv_command: FnMpvCommand,
    mpv_terminate_destroy: FnMpvTerminateDestroy,
    mpv_wakeup: Option<FnMpvWakeup>,
}

static SHARED_LIBMPV: OnceLock<Result<Arc<libloading::Library>, String>> = OnceLock::new();

fn load_libmpv_once() -> Result<Arc<libloading::Library>, String> {
    let res = SHARED_LIBMPV.get_or_init(|| {
        #[cfg(target_os = "windows")]
        let candidates = ["libmpv-2.dll", "mpv-2.dll", "libmpv.dll"];
        #[cfg(target_os = "macos")]
        let candidates = ["libmpv.2.dylib", "libmpv.dylib"];
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let candidates = ["libmpv.so.2", "libmpv.so"];

        for name in candidates {
            if let Ok(lib) = unsafe { libloading::Library::new(name) } {
                return Ok(Arc::new(lib));
            }
        }
        Err("could not load libmpv shared library".to_string())
    });
    res.clone()
}

impl LibmpvDeck {
    fn create(config: DeckConfig) -> Result<Self, String> {
        let lib = load_libmpv_once()?;
        unsafe {
            let mpv_create: FnMpvCreate = *lib
                .get::<FnMpvCreate>(b"mpv_create\0")
                .map_err(|e| e.to_string())?;
            let mpv_initialize: FnMpvInitialize = *lib
                .get::<FnMpvInitialize>(b"mpv_initialize\0")
                .map_err(|e| e.to_string())?;
            let mpv_terminate_destroy: FnMpvTerminateDestroy = *lib
                .get::<FnMpvTerminateDestroy>(b"mpv_terminate_destroy\0")
                .map_err(|e| e.to_string())?;
            let mpv_set_property_string: FnMpvSetPropertyString = *lib
                .get::<FnMpvSetPropertyString>(b"mpv_set_property_string\0")
                .map_err(|e| e.to_string())?;
            let mpv_get_property_string: FnMpvGetPropertyString = *lib
                .get::<FnMpvGetPropertyString>(b"mpv_get_property_string\0")
                .map_err(|e| e.to_string())?;
            let mpv_free: FnMpvFree = *lib
                .get::<FnMpvFree>(b"mpv_free\0")
                .map_err(|e| e.to_string())?;
            let mpv_command: FnMpvCommand = *lib
                .get::<FnMpvCommand>(b"mpv_command\0")
                .map_err(|e| e.to_string())?;
            let mpv_wait_event: FnMpvWaitEvent = *lib
                .get::<FnMpvWaitEvent>(b"mpv_wait_event\0")
                .map_err(|e| e.to_string())?;
            let mpv_observe_property: FnMpvObserveProperty = *lib
                .get::<FnMpvObserveProperty>(b"mpv_observe_property\0")
                .map_err(|e| e.to_string())?;
            let mpv_request_log_messages: Option<FnMpvRequestLogMessages> = lib
                .get::<FnMpvRequestLogMessages>(b"mpv_request_log_messages\0")
                .ok()
                .map(|s| *s);
            let mpv_wakeup: Option<FnMpvWakeup> = lib
                .get::<FnMpvWakeup>(b"mpv_wakeup\0")
                .ok()
                .map(|s| *s);

            let ctx = mpv_create();
            if ctx.is_null() {
                return Err("mpv_create returned null".to_string());
            }

            let set_prop = |k: &str, v: &str| {
                if let (Ok(ck), Ok(cv)) = (CString::new(k), CString::new(v)) {
                    let _ = mpv_set_property_string(ctx, ck.as_ptr(), cv.as_ptr());
                }
            };

            // 1. Strip unused subsystems on audio deck
            set_prop("vid", "no");
            set_prop("vo", "null");
            set_prop("title", config.title);
            set_prop("audio-display", "no");
            set_prop("cover-art-auto", "no");
            set_prop("osc", "no");
            set_prop("osd-level", "0");
            set_prop("sub-auto", "no");
            set_prop("sid", "no");
            set_prop("load-scripts", "no");
            set_prop("ytdl", "no");
            set_prop("config", "no");
            set_prop("save-position-on-quit", "no");
            set_prop("terminal", "no");
            set_prop("input-default-bindings", "no");
            set_prop("input-vo-keyboard", "no");

            // 2. Demuxer ring-buffer caps, cache dir, reconnect & audio buffer
            set_prop("gapless-audio", "yes");
            set_prop("prefetch-playlist", "yes");
            set_prop("cache", "yes");
            set_prop("cache-on-disk", "yes");
            if !config.cache_dir.is_empty() {
                let _ = std::fs::create_dir_all(&config.cache_dir);
                set_prop("cache-dir", &config.cache_dir);
            }
            set_prop("demuxer-max-bytes", config.demuxer_max_bytes);
            set_prop("demuxer-max-back-bytes", config.demuxer_max_back_bytes);
            set_prop("demuxer-donate-buffer", "no");
            set_prop("cache-secs", "10");
            set_prop("demuxer-readahead-secs", "8");
            set_prop("cache-pause-wait", "1");
            set_prop("ad-lavc-threads", "1");
            set_prop(
                "audio-buffer",
                if config.timed_mix_enabled { "0.2" } else { "0.4" },
            );
            set_prop(
                "stream-lavf-o",
                "timeout=15000000,reconnect=1,reconnect_streamed=1,reconnect_delay_max=5",
            );
            if let Some(ref ao) = config.debug_ao {
                set_prop("ao", ao);
            }
            if !config.initial_audio_device.is_empty() && config.initial_audio_device != "auto" {
                set_prop("audio-device", &config.initial_audio_device);
            }
            if !config.initial_af.is_empty() {
                set_prop("af", &config.initial_af);
            }
            if let Some(ref lf) = config.log_file {
                if let Some(parent) = lf.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                set_prop("log-file", &lf.to_string_lossy());
            }
            if let Some(req_log) = mpv_request_log_messages {
                if let Ok(lvl) = CString::new("error") {
                    let _ = req_log(ctx, lvl.as_ptr());
                }
            }

            if mpv_initialize(ctx) < 0 {
                mpv_terminate_destroy(ctx);
                return Err("mpv_initialize failed".to_string());
            }

            let observe = |name: &str, fmt: c_int| {
                if let Ok(cname) = CString::new(name) {
                    let _ = mpv_observe_property(ctx, 0, cname.as_ptr(), fmt);
                }
            };
            observe("pause", MPV_FORMAT_FLAG);
            observe("paused-for-cache", MPV_FORMAT_FLAG);
            observe("time-pos", MPV_FORMAT_DOUBLE);
            observe("demuxer-cache-duration", MPV_FORMAT_DOUBLE);
            observe("duration", MPV_FORMAT_DOUBLE);
            observe("volume", MPV_FORMAT_DOUBLE);
            observe("speed", MPV_FORMAT_DOUBLE);
            observe("audio-params", MPV_FORMAT_STRING);

            let (tx, rx) = mpsc::unbounded_channel();
            let state = Arc::new(Mutex::new(DeckState {
                volume: 100.0,
                rate: 1.0,
                ..Default::default()
            }));
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_clone = Arc::clone(&stop_flag);
            let state_clone = Arc::clone(&state);
            let ctx_addr = ctx as usize;

            let worker = std::thread::Builder::new()
                .name(format!("lw-mpv-{}", config.role))
                .spawn(move || {
                    let ctx_ptr = ctx_addr as *mut c_void;
                    while !stop_clone.load(Ordering::Relaxed) {
                        let ev = mpv_wait_event(ctx_ptr, 0.25);
                        if ev.is_null() {
                            continue;
                        }
                        let ev_ref = &*ev;
                        match ev_ref.event_id {
                            MPV_EVENT_NONE => continue,
                            MPV_EVENT_SHUTDOWN => break,
                            MPV_EVENT_AUDIO_RECONFIG => {
                                let _ = tx.send(DeckEvent::AudioParams);
                            }
                            MPV_EVENT_END_FILE => {
                                if !ev_ref.data.is_null() {
                                    let end = &*(ev_ref.data as *const RawMpvEventEndFile);
                                    if end.reason == MPV_END_FILE_REASON_EOF {
                                        if let Ok(mut st) = state_clone.lock() {
                                            st.playing = false;
                                        }
                                        let _ = tx.send(DeckEvent::Completed);
                                    } else if end.reason == MPV_END_FILE_REASON_ERROR {
                                        let _ = tx.send(DeckEvent::Error(format!(
                                            "mpv end_file error code {}",
                                            end.error
                                        )));
                                    }
                                }
                            }
                            MPV_EVENT_LOG_MESSAGE => {
                                if !ev_ref.data.is_null() {
                                    let msg = &*(ev_ref.data as *const RawMpvEventLogMessage);
                                    let prefix = if msg.prefix.is_null() {
                                        String::new()
                                    } else {
                                        CStr::from_ptr(msg.prefix).to_string_lossy().into_owned()
                                    };
                                    let level = if msg.level.is_null() {
                                        String::new()
                                    } else {
                                        CStr::from_ptr(msg.level).to_string_lossy().into_owned()
                                    };
                                    let text = if msg.text.is_null() {
                                        String::new()
                                    } else {
                                        CStr::from_ptr(msg.text)
                                            .to_string_lossy()
                                            .trim()
                                            .to_string()
                                    };
                                    let _ = tx.send(DeckEvent::Log(DeckLog {
                                        prefix,
                                        level: level.clone(),
                                        text: text.clone(),
                                    }));
                                    if level == "error" || level == "fatal" {
                                        let _ = tx.send(DeckEvent::Error(text));
                                    }
                                }
                            }
                            MPV_EVENT_PROPERTY_CHANGE => {
                                if ev_ref.data.is_null() {
                                    continue;
                                }
                                let prop = &*(ev_ref.data as *const RawMpvEventProperty);
                                if prop.name.is_null() || prop.data.is_null() {
                                    continue;
                                }
                                let name = CStr::from_ptr(prop.name).to_string_lossy();
                                match (name.as_ref(), prop.format) {
                                    ("pause", MPV_FORMAT_FLAG) => {
                                        let paused = *(prop.data as *const c_int) != 0;
                                        let playing = !paused;
                                        if let Ok(mut st) = state_clone.lock() {
                                            st.playing = playing;
                                        }
                                        let _ = tx.send(DeckEvent::Playing(playing));
                                    }
                                    ("paused-for-cache", MPV_FORMAT_FLAG) => {
                                        let buf = *(prop.data as *const c_int) != 0;
                                        if let Ok(mut st) = state_clone.lock() {
                                            st.buffering = buf;
                                        }
                                        let _ = tx.send(DeckEvent::Buffering(buf));
                                    }
                                    ("time-pos", MPV_FORMAT_DOUBLE) => {
                                        let secs = *(prop.data as *const f64);
                                        if secs.is_finite() && secs >= 0.0 {
                                            let d = Duration::from_secs_f64(secs);
                                            if let Ok(mut st) = state_clone.lock() {
                                                st.position = d;
                                            }
                                            let _ = tx.send(DeckEvent::Position(d));
                                        }
                                    }
                                    ("demuxer-cache-duration", MPV_FORMAT_DOUBLE) => {
                                        let cache_s = *(prop.data as *const f64);
                                        if cache_s.is_finite() && cache_s >= 0.0 {
                                            let pos = state_clone
                                                .lock()
                                                .map(|s| s.position)
                                                .unwrap_or(Duration::ZERO);
                                            let buf = pos + Duration::from_secs_f64(cache_s);
                                            if let Ok(mut st) = state_clone.lock() {
                                                st.buffered = buf;
                                            }
                                            let _ = tx.send(DeckEvent::Buffered(buf));
                                        }
                                    }
                                    ("duration", MPV_FORMAT_DOUBLE) => {
                                        let secs = *(prop.data as *const f64);
                                        if secs.is_finite() && secs >= 0.0 {
                                            let d = Duration::from_secs_f64(secs);
                                            if let Ok(mut st) = state_clone.lock() {
                                                st.duration = d;
                                            }
                                            let _ = tx.send(DeckEvent::DurationChanged(d));
                                        }
                                    }
                                    ("volume", MPV_FORMAT_DOUBLE) => {
                                        let v = *(prop.data as *const f64);
                                        if v.is_finite() {
                                            if let Ok(mut st) = state_clone.lock() {
                                                st.volume = v;
                                            }
                                            let _ = tx.send(DeckEvent::Volume(v));
                                        }
                                    }
                                    ("speed", MPV_FORMAT_DOUBLE) => {
                                        let r = *(prop.data as *const f64);
                                        if r.is_finite() {
                                            if let Ok(mut st) = state_clone.lock() {
                                                st.rate = r;
                                            }
                                            let _ = tx.send(DeckEvent::Rate(r));
                                        }
                                    }
                                    ("audio-params", _) => {
                                        let _ = tx.send(DeckEvent::AudioParams);
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                })
                .map_err(|e| e.to_string())?;

            Ok(Self {
                id: NEXT_DECK_ID.fetch_add(1, Ordering::Relaxed),
                _lib: lib,
                ctx: Mutex::new(Some(LibmpvCtxPtr(ctx))),
                state,
                event_rx: Mutex::new(Some(rx)),
                stop_flag,
                worker: Mutex::new(Some(worker)),
                mpv_set_property_string,
                mpv_get_property_string,
                mpv_free,
                mpv_command,
                mpv_terminate_destroy,
                mpv_wakeup,
            })
        }
    }

    fn with_ctx<R, F: FnOnce(*mut c_void) -> R>(&self, f: F) -> Result<R, String> {
        let guard = self.ctx.lock().map_err(|_| "ctx lock poisoned".to_string())?;
        let Some(ref ptr) = *guard else {
            return Err("mpv context disposed".to_string());
        };
        Ok(f(ptr.0))
    }
}

impl MpvDeck for LibmpvDeck {
    fn id(&self) -> u64 {
        self.id
    }

    fn state(&self) -> DeckState {
        self.state.lock().map(|s| *s).unwrap_or_default()
    }

    fn set_property(&self, name: &str, value: &str) -> Result<(), String> {
        let cname = CString::new(name).map_err(|e| e.to_string())?;
        let cval = CString::new(value).map_err(|e| e.to_string())?;
        self.with_ctx(|ctx| unsafe {
            let rc = (self.mpv_set_property_string)(ctx, cname.as_ptr(), cval.as_ptr());
            if rc < 0 {
                Err(format!("mpv_set_property_string({name}) failed: {rc}"))
            } else {
                Ok(())
            }
        })?
    }

    fn get_property(&self, name: &str) -> Option<String> {
        let cname = CString::new(name).ok()?;
        self.with_ctx(|ctx| unsafe {
            let raw = (self.mpv_get_property_string)(ctx, cname.as_ptr());
            if raw.is_null() {
                return None;
            }
            let s = CStr::from_ptr(raw).to_string_lossy().into_owned();
            (self.mpv_free)(raw as *mut c_void);
            Some(s)
        })
        .ok()
        .flatten()
    }

    fn command(&self, args: &[&str]) -> Result<(), String> {
        let cstrings: Vec<CString> = args
            .iter()
            .map(|a| CString::new(*a).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut ptrs: Vec<*const c_char> = cstrings.iter().map(|s| s.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        self.with_ctx(|ctx| unsafe {
            let rc = (self.mpv_command)(ctx, ptrs.as_ptr());
            if rc < 0 {
                Err(format!("mpv_command({args:?}) failed: {rc}"))
            } else {
                Ok(())
            }
        })?
    }

    fn open(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        start: Option<Duration>,
        play: bool,
    ) -> Result<(), String> {
        if !headers.is_empty() {
            let joined: String = headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join(",");
            let _ = self.set_property("http-header-fields", &joined);
        } else {
            let _ = self.set_property("http-header-fields", "");
        }
        if let Some(s) = start {
            let _ = self.set_property("start", &format!("{:.3}", s.as_secs_f64()));
        } else {
            let _ = self.set_property("start", "none");
        }
        let _ = self.set_property("pause", if play { "no" } else { "yes" });
        self.command(&["loadfile", url, "replace"])
    }

    fn play(&self) -> Result<(), String> {
        self.set_property("pause", "no")
    }

    fn pause(&self) -> Result<(), String> {
        self.set_property("pause", "yes")
    }

    fn play_or_pause(&self) -> Result<(), String> {
        self.command(&["cycle", "pause"])
    }

    fn stop(&self) -> Result<(), String> {
        self.command(&["stop"])
    }

    fn seek(&self, position: Duration) -> Result<(), String> {
        let secs = format!("{:.3}", position.as_secs_f64());
        self.command(&["seek", &secs, "absolute"])
    }

    fn set_volume(&self, volume: f64) -> Result<(), String> {
        let clamped = volume.clamp(0.0, 100.0);
        if let Ok(mut st) = self.state.lock() {
            st.volume = clamped;
        }
        self.set_property("volume", &format!("{clamped:.2}"))
    }

    fn set_rate(&self, rate: f64) -> Result<(), String> {
        if let Ok(mut st) = self.state.lock() {
            st.rate = rate;
        }
        self.set_property("speed", &format!("{rate:.6}"))
    }

    fn set_audio_device(&self, device: &str) -> Result<(), String> {
        let target = if device.is_empty() { "auto" } else { device };
        self.set_property("audio-device", target)
    }

    fn subscribe_events(&self) -> Option<mpsc::UnboundedReceiver<DeckEvent>> {
        self.event_rx.lock().ok()?.take()
    }

    fn dispose(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        let ctx_opt = self.ctx.lock().ok().and_then(|mut g| g.take());
        if let Some(ptr) = ctx_opt {
            unsafe {
                if let Some(wakeup) = self.mpv_wakeup {
                    wakeup(ptr.0);
                }
            }
            if let Ok(mut w) = self.worker.lock() {
                if let Some(handle) = w.take() {
                    let _ = handle.join();
                }
            }
            unsafe {
                (self.mpv_terminate_destroy)(ptr.0);
            }
        }
    }
}

impl Drop for LibmpvDeck {
    fn drop(&mut self) {
        self.dispose();
    }
}

// ---- PlaybackService State & Implementation -------------------------------

const MPV_LOG_TAIL_MAX: usize = 50;
const ARM_LEAD: Duration = Duration::from_secs(14);
const DJ_EQ_BANDS: &str =
    "equalizer@lwlo=f=90:t=o:w=2:g=0,equalizer@lwhi=f=6000:t=o:w=2:g=0";

fn ms_epoch_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn crystalizer_strip_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r",?crystalizer=[^,\]]*").expect("valid crystalizer regex"))
}

fn bracket_strip_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s*[(\[][^)\]]*[)\]]").expect("valid bracket regex"))
}

fn primary_artist_split_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\s*(?:,|&|feat\.?|ft\.?)\s*").expect("valid primary artist regex")
    })
}

fn album_gain_prop_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)replaygain_album_gain[^0-9\-]*(-?[0-9]+(?:\.[0-9]+)?)")
            .expect("valid album gain regex")
    })
}

fn out_rate_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"samplerate=(\d+)").expect("valid out rate regex"))
}

fn out_format_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"format=([^\s,}]+)").expect("valid out format regex"))
}

struct PlaybackData {
    snapshot: PlayerSnapshot,
    player: Option<Arc<dyn MpvDeck>>,
    standby_player: Option<Arc<dyn MpvDeck>>,
    deck_event_tasks: Vec<JoinHandle<()>>,
    mpv_log_tail: VecDeque<DeckLog>,
    unavailable: HashSet<String>,
    lossless_bypass: HashSet<String>,
    shuffle_order: Vec<usize>,
    sleep_task: Option<JoinHandle<()>>,
    sleep_deadline: Option<Instant>,
    persist_task: Option<JoinHandle<()>>,
    last_persist: Option<Instant>,
    last_position_emit: Option<Instant>,
    last_buffered_emit: Option<Instant>,

    // Scrobble bookkeeping
    scrobble_start_epoch: i64,
    accumulated_seconds: f64,
    last_tick: Option<Instant>,
    now_playing_sent: bool,
    scrobble_track: Option<PlayableTrack>,
    scrobble_duration_sec: i64,
    scrobbled_this_window: bool,
    yt_history_pushed_this_window: bool,
    yt_history_cpn: String,
    watchtime_urls: HashMap<String, String>,
    watchtime_order: VecDeque<String>,

    endless_radio: bool,
    spotify_radio: Option<SpotifyRadioFn>,
    radio_seeds: HashSet<String>,
    resolving: bool,
    resolving_index: i32,
    resolve_generation: u64,
    playback_attempt: u32,
    failed_generation: Option<u64>,
    active_queue_key: String,
    lock_software_volume: bool,
    exclusive_applied: bool,
    wasapi_error: Option<String>,
    forced_ao_format: Option<String>,
    play_cache: ResolvedStreamCache,
    resolve_inflight: HashMap<String, tokio::sync::watch::Receiver<Option<Option<ResolvedStream>>>>,
    prefetch_inflight: HashSet<String>,
    prefetch_seed_key: String,

    // First-audio clock
    audio_clock_start: Option<Instant>,
    audio_clock_key: String,
    audio_clock_generation: Option<u64>,
    audio_open_done_at: Option<Instant>,

    last_was_dash: bool,
    eq_filter: String,
    processing_disabled: bool,
    hp_filter: String,
    current_audio_filter: String,
    current_audio_device: String,
    album_replay_gain_db: HashMap<String, f64>,
    dj_eq_broken: bool,
    eq_extras_dropped: bool,
    on_processing_changed: Option<Arc<dyn Fn() + Send + Sync>>,

    // Dual-engine Crossfade & Standby
    standby_armed: bool,
    standby_ready: bool,
    mix_gave_up_for: String,
    standby_track: Option<PlayableTrack>,
    standby_stream: Option<ResolvedStream>,
    standby_index: i32,
    crossfading: bool,
    crossfade_start_time: Option<Instant>,
    crossfade_duration: Option<Duration>,
    crossfade_task: Option<JoinHandle<()>>,
    crossfade_target_volume: f64,
    crossfade_resolved_key: String,
    last_crossfade_resolve_attempt: Option<Instant>,
    filter_error_logged: bool,
    mix_plan: Option<MixPlan>,
    mix_plan_key: String,

    // Beat-aware AutoMix
    dj_plan: Option<DjMixPlan>,
    dj_plan_key: String,
    armed_dj_plan: Option<DjMixPlan>,
    rate_ramp_task: Option<JoinHandle<()>>,
    beat_lock: Option<DjBeatLock>,
    dj_eq_live: Option<bool>,
    mix_faults: MixFaultBudget,
    mix_watchdog_task: Option<JoinHandle<()>>,
    mix_started_at: Option<Instant>,
    hardware_volume: bool,
    pending_repairs: HashSet<MixRepair>,
    dj_mix_wall_start: Option<Instant>,
    next_stream_memo_key: String,
    next_stream_memo: Option<ResolvedStream>,
    next_stream_memo_at: Option<Instant>,

    resume_at: Option<Duration>,
    resume_key: String,

    af_written: HashMap<u64, String>,
    primary_af_changed_at: Option<Instant>,
    last_toggle_time: Option<Instant>,
    user_paused: bool,
    eq_debounce_task: Option<JoinHandle<()>>,
}

impl Default for PlaybackData {
    fn default() -> Self {
        Self {
            snapshot: PlayerSnapshot::default(),
            player: None,
            standby_player: None,
            deck_event_tasks: Vec::new(),
            mpv_log_tail: VecDeque::new(),
            unavailable: HashSet::new(),
            lossless_bypass: HashSet::new(),
            shuffle_order: Vec::new(),
            sleep_task: None,
            sleep_deadline: None,
            persist_task: None,
            last_persist: None,
            last_position_emit: None,
            last_buffered_emit: None,
            scrobble_start_epoch: 0,
            accumulated_seconds: 0.0,
            last_tick: None,
            now_playing_sent: false,
            scrobble_track: None,
            scrobble_duration_sec: 0,
            scrobbled_this_window: false,
            yt_history_pushed_this_window: false,
            yt_history_cpn: String::new(),
            watchtime_urls: HashMap::new(),
            watchtime_order: VecDeque::new(),
            endless_radio: false,
            spotify_radio: None,
            radio_seeds: HashSet::new(),
            resolving: false,
            resolving_index: -1,
            resolve_generation: 0,
            playback_attempt: 0,
            failed_generation: None,
            active_queue_key: String::new(),
            lock_software_volume: false,
            exclusive_applied: false,
            wasapi_error: None,
            forced_ao_format: None,
            play_cache: ResolvedStreamCache::default(),
            resolve_inflight: HashMap::new(),
            prefetch_inflight: HashSet::new(),
            prefetch_seed_key: String::new(),
            audio_clock_start: None,
            audio_clock_key: String::new(),
            audio_clock_generation: None,
            audio_open_done_at: None,
            last_was_dash: false,
            eq_filter: String::new(),
            processing_disabled: false,
            hp_filter: String::new(),
            current_audio_filter: String::new(),
            current_audio_device: "auto".to_string(),
            album_replay_gain_db: HashMap::new(),
            dj_eq_broken: false,
            eq_extras_dropped: false,
            on_processing_changed: None,
            standby_armed: false,
            standby_ready: false,
            mix_gave_up_for: String::new(),
            standby_track: None,
            standby_stream: None,
            standby_index: -1,
            crossfading: false,
            crossfade_start_time: None,
            crossfade_duration: None,
            crossfade_task: None,
            crossfade_target_volume: 1.0,
            crossfade_resolved_key: String::new(),
            last_crossfade_resolve_attempt: None,
            filter_error_logged: false,
            mix_plan: None,
            mix_plan_key: String::new(),
            dj_plan: None,
            dj_plan_key: String::new(),
            armed_dj_plan: None,
            rate_ramp_task: None,
            beat_lock: None,
            dj_eq_live: None,
            mix_faults: MixFaultBudget::default(),
            mix_watchdog_task: None,
            mix_started_at: None,
            hardware_volume: false,
            pending_repairs: HashSet::new(),
            dj_mix_wall_start: None,
            next_stream_memo_key: String::new(),
            next_stream_memo: None,
            next_stream_memo_at: None,
            resume_at: None,
            resume_key: String::new(),
            af_written: HashMap::new(),
            primary_af_changed_at: None,
            last_toggle_time: None,
            user_paused: false,
            eq_debounce_task: None,
        }
    }
}

pub struct PlaybackService {
    tube: Arc<InnerTubeMusicApi>,
    lossless: Arc<dyn LosslessSourceBackend>,
    on_addon_quota: Option<AddonQuotaNoticeFn>,
    scrobbler: Arc<ScrobbleRepository>,
    downloads: Arc<DownloadManager>,
    prefs: PrefsHandle,
    sessions: SessionStore,
    artwork_service: Arc<OfficialArtworkService>,
    deck_factory: Arc<dyn MpvDeckFactory>,
    data: Mutex<PlaybackData>,
    mpv_lock: Arc<AsyncMutex<()>>,
    prefetch_lock: Arc<AsyncMutex<()>>,
    analyzer: OnceLock<Arc<AutomixAnalyzer>>,
    snapshot_listeners: Mutex<Vec<SnapshotListenerFn>>,
    smtc: Mutex<Option<Arc<SmtcService>>>,
    mpris: Mutex<Option<Arc<MprisService>>>,
    debug_audio_output: Mutex<Option<String>>,
}

struct ServiceMpvSerializer {
    mpv_lock: Arc<AsyncMutex<()>>,
}

impl MpvSerializer for ServiceMpvSerializer {
    fn serialize<'a>(
        &'a self,
        op: Pin<Box<dyn Future<Output = ()> + Send + 'a>>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let _guard = self.mpv_lock.lock().await;
            op.await;
        })
    }
}

impl PlaybackService {
    pub fn new(
        tube: Arc<InnerTubeMusicApi>,
        lossless: Arc<dyn LosslessSourceBackend>,
        scrobbler: Arc<ScrobbleRepository>,
        downloads: Arc<DownloadManager>,
        prefs: PrefsHandle,
        sessions: SessionStore,
        artwork_service: Arc<OfficialArtworkService>,
        on_addon_quota: Option<AddonQuotaNoticeFn>,
    ) -> Arc<Self> {
        Self::with_deck_factory(
            tube,
            lossless,
            scrobbler,
            downloads,
            prefs,
            sessions,
            artwork_service,
            on_addon_quota,
            Arc::new(LibmpvDeckFactory),
        )
    }

    pub fn with_deck_factory(
        tube: Arc<InnerTubeMusicApi>,
        lossless: Arc<dyn LosslessSourceBackend>,
        scrobbler: Arc<ScrobbleRepository>,
        downloads: Arc<DownloadManager>,
        prefs: PrefsHandle,
        sessions: SessionStore,
        artwork_service: Arc<OfficialArtworkService>,
        on_addon_quota: Option<AddonQuotaNoticeFn>,
        deck_factory: Arc<dyn MpvDeckFactory>,
    ) -> Arc<Self> {
        Arc::new(Self {
            tube,
            lossless,
            on_addon_quota,
            scrobbler,
            downloads,
            prefs,
            sessions,
            artwork_service,
            deck_factory,
            data: Mutex::new(PlaybackData::default()),
            mpv_lock: Arc::new(AsyncMutex::new(())),
            prefetch_lock: Arc::new(AsyncMutex::new(())),
            analyzer: OnceLock::new(),
            snapshot_listeners: Mutex::new(Vec::new()),
            smtc: Mutex::new(None),
            mpris: Mutex::new(None),
            debug_audio_output: Mutex::new(None),
        })
    }

    pub fn set_debug_audio_output(&self, ao: Option<String>) {
        if let Ok(mut g) = self.debug_audio_output.lock() {
            *g = ao;
        }
    }

    pub fn set_spotify_radio(&self, hook: Option<SpotifyRadioFn>) {
        if let Ok(mut d) = self.data.lock() {
            d.spotify_radio = hook;
        }
    }

    pub fn set_on_processing_changed(&self, cb: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let Ok(mut d) = self.data.lock() {
            d.on_processing_changed = cb;
        }
    }

    pub fn add_snapshot_listener(&self, listener: SnapshotListenerFn) {
        if let Ok(mut list) = self.snapshot_listeners.lock() {
            list.push(listener);
        }
    }

    pub fn attach_smtc(&self, smtc: Arc<SmtcService>) {
        if let Ok(mut g) = self.smtc.lock() {
            *g = Some(smtc);
        }
    }

    pub fn attach_mpris(&self, mpris: Arc<MprisService>) {
        if let Ok(mut g) = self.mpris.lock() {
            *g = Some(mpris);
        }
    }

    /// Routes an SMTC / macOS Now Playing action into this `PlaybackService`.
    pub fn handle_media_control_action(self: &Arc<Self>, action: MediaControlAction) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            match action {
                MediaControlAction::Play => this.play_resume().await,
                MediaControlAction::Pause | MediaControlAction::Stop => this.pause().await,
                MediaControlAction::Toggle => this.toggle().await,
                MediaControlAction::Next => this.next().await,
                MediaControlAction::Previous => this.previous().await,
                MediaControlAction::Seek(pos) => this.seek(pos).await,
            }
        });
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        self.data
            .lock()
            .map(|d| d.snapshot.clone())
            .unwrap_or_default()
    }

    pub fn debug_mix_state(&self) -> (usize, f64, Option<f64>, bool) {
        let Ok(d) = self.data.lock() else {
            return (0, -1.0, None, false);
        };
        (
            d.mix_faults.faults(),
            d.player.as_ref().map(|p| p.state().volume).unwrap_or(-1.0),
            d.standby_player.as_ref().map(|p| p.state().volume),
            d.crossfading,
        )
    }

    fn emit_snapshot(&self, snap: PlayerSnapshot) {
        if let Some(ref smtc) = self.smtc.lock().ok().and_then(|g| g.clone()) {
            smtc.on_snapshot(&snap);
        }
        if let Some(ref mpris) = self.mpris.lock().ok().and_then(|g| g.clone()) {
            mpris.on_snapshot(snap.clone());
        }
        if let Ok(listeners) = self.snapshot_listeners.lock() {
            for cb in listeners.iter() {
                cb(&snap);
            }
        }
    }

    fn update_snapshot<F: FnOnce(&mut PlayerSnapshot)>(&self, f: F) -> PlayerSnapshot {
        let snap = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            f(&mut d.snapshot);
            d.snapshot.clone()
        };
        self.emit_snapshot(snap.clone());
        snap
    }

    fn analyzer(self: &Arc<Self>) -> &Arc<AutomixAnalyzer> {
        self.analyzer.get_or_init(|| {
            let serializer: Arc<dyn MpvSerializer> = Arc::new(ServiceMpvSerializer {
                mpv_lock: Arc::clone(&self.mpv_lock),
            });
            let log: LogHook = Arc::new(|line| TraceCrumb::write(line));
            let weak = Arc::downgrade(self);
            let wanted: WantedHook = Arc::new(move |key| {
                weak.upgrade()
                    .is_some_and(|svc| svc.analysis_wanted(key))
            });
            Arc::new(AutomixAnalyzer::new(
                serializer,
                log,
                None,
                Some(wanted),
                None,
            ))
        })
    }

    fn crumb(&self, line: &str) {
        TraceCrumb::write(line);
    }

    fn log_timing(&self, line: &str) {
        TraceCrumb::write(&format!("timing {line}"));
    }

    fn notify_processing(&self) {
        let cb = self
            .data
            .lock()
            .ok()
            .and_then(|d| d.on_processing_changed.clone());
        if let Some(cb) = cb {
            cb();
        }
    }

    fn headphone_active(&self) -> bool {
        HeadphoneProfiles::by_id(Some(&self.prefs.headphone_profile())).id != "off"
    }

    /// True while bit-perfect / WASAPI exclusive must stay off.
    pub fn processing_blocks_exclusive(&self) -> bool {
        if self.prefs.crossfade_enabled() || self.prefs.auto_mix_enabled() {
            return true;
        }
        if self.prefs.loudness_mode() != "off" {
            return true;
        }
        if self.headphone_active() {
            return true;
        }
        if self.prefs.equalizer_enabled() || self.prefs.studio_clarity_enabled() {
            return true;
        }
        let snap = self.snapshot();
        if (snap.speed - 1.0).abs() > 0.001 {
            return true;
        }
        if (snap.pitch - 1.0).abs() > 0.005 {
            return true;
        }
        false
    }

    fn timed_mix_enabled(&self) -> bool {
        self.prefs.auto_mix_enabled()
    }

    fn prefer_smallest_youtube(&self) -> bool {
        self.prefs.lossless_quality() == AudioQualityTiers::DATA_SAVER as i64
    }

    fn overlap_enabled(&self) -> bool {
        let overlap_allowed = self
            .data
            .lock()
            .map(|d| d.mix_faults.overlap_allowed())
            .unwrap_or(true);
        (self.timed_mix_enabled() || self.prefs.crossfade_enabled()) && overlap_allowed
    }

    fn overlap_length(&self) -> Duration {
        if self.timed_mix_enabled() {
            Duration::from_secs(if self.prefs.auto_mix_bars() >= 16 {
                12
            } else {
                8
            })
        } else {
            Duration::from_secs(self.prefs.crossfade_seconds().clamp(3, 12) as u64)
        }
    }

    fn stash_watchtime_url(&self, video_id: &str, url: &str) {
        if video_id.is_empty() || url.is_empty() {
            return;
        }
        if let Ok(mut d) = self.data.lock() {
            d.watchtime_order.retain(|k| k != video_id);
            d.watchtime_urls
                .insert(video_id.to_string(), url.to_string());
            d.watchtime_order.push_back(video_id.to_string());
            while d.watchtime_order.len() > 24 {
                if let Some(oldest) = d.watchtime_order.pop_front() {
                    d.watchtime_urls.remove(&oldest);
                }
            }
        }
    }

    fn remember_watchtime(&self, video_id: &str, stream: &ResolvedStream) {
        self.stash_watchtime_url(video_id, &stream.watchtime_url);
    }

    fn maybe_log_first_audio(self: &Arc<Self>, position: Duration) {
        if position.is_zero() {
            return;
        }
        let trigger = {
            let mut d = match self.data.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            if d.audio_clock_generation != Some(d.resolve_generation) {
                return;
            }
            if d.audio_open_done_at.is_none() {
                return;
            }
            let Some(start) = d.audio_clock_start else {
                return;
            };
            if d.audio_clock_key.is_empty() {
                return;
            }
            if d.snapshot
                .current
                .as_ref()
                .map(|c| c.queue_key())
                .as_deref()
                != Some(d.audio_clock_key.as_str())
            {
                return;
            }
            d.audio_clock_generation = None;
            let now = Instant::now();
            let total_ms = now.duration_since(start).as_millis();
            let post_open_ms = d
                .audio_open_done_at
                .map(|o| now.duration_since(o).as_millis() as i128)
                .unwrap_or(-1);
            let key = d.audio_clock_key.clone();
            (key, total_ms, post_open_ms)
        };
        self.prefetch_neighbors(false);
        self.log_timing(&format!(
            "audio key={} time_to_audio_ms={} post_open_ms={}",
            trigger.0, trigger.1, trigger.2
        ));
    }

    // ---- Player Lifecycle & Event Stream Binding --------------------------

    pub async fn ensure_player(self: &Arc<Self>) {
        if self.data.lock().is_ok_and(|d| d.player.is_some()) {
            return;
        }
        let cache_dir = mpv_cache_dir_path();
        ensure_dir_exists_async(&cache_dir).await;
        let trail_dir = TraceCrumb::directory();
        let _ = tokio::fs::create_dir_all(&trail_dir).await;
        TraceCrumb::reset(&format!(
            "--- player created {} ---",
            Utc::now().to_rfc3339()
        ));

        let debug_ao = self.debug_audio_output.lock().ok().and_then(|g| g.clone());
        let (initial_af, initial_device) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (
                d.current_audio_filter.clone(),
                d.current_audio_device.clone(),
            )
        };

        let config = DeckConfig {
            role: "primary",
            title: "LastWave",
            buffer_bytes: 4 * 1024 * 1024,
            demuxer_max_bytes: "6MiB",
            demuxer_max_back_bytes: "2MiB",
            timed_mix_enabled: self.timed_mix_enabled(),
            initial_af: initial_af.clone(),
            initial_audio_device: initial_device,
            debug_ao,
            log_file: Some(trail_dir.join("mpv.log")),
            cache_dir,
        };

        let Ok(deck) = self.deck_factory.create(config) else {
            return;
        };
        let deck_id = deck.id();
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if d.player.is_some() {
                deck.dispose();
                return;
            }
            if !initial_af.is_empty() {
                d.af_written.insert(deck_id, initial_af);
            }
            d.player = Some(Arc::clone(&deck));
        }

        self.bind_player_events(&deck);
        self.start_mix_watchdog();
        self.restore_session().await;
        self.rebuild_processing_filter();
        if !self.data.lock().is_ok_and(|d| d.exclusive_applied) {
            let album = self
                .snapshot()
                .current
                .map(|c| c.album)
                .unwrap_or_default();
            self.apply_loudness(&album);
            self.apply_rate_and_pitch();
        }
    }

    fn bind_player_events(self: &Arc<Self>, deck: &Arc<dyn MpvDeck>) {
        let Some(mut rx) = deck.subscribe_events() else {
            return;
        };
        let deck_id = deck.id();
        let this = Arc::clone(self);
        let handle = tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                let is_primary = this
                    .data
                    .lock()
                    .ok()
                    .and_then(|d| d.player.as_ref().map(|p| p.id() == deck_id))
                    .unwrap_or(false);
                if !is_primary {
                    continue;
                }
                match ev {
                    DeckEvent::Playing(v) => {
                        this.update_snapshot(|s| s.is_playing = v);
                        this.on_playing_changed(v);
                    }
                    DeckEvent::Buffering(v) => {
                        let resolving = this.data.lock().map(|d| d.resolving).unwrap_or(false);
                        this.update_snapshot(|s| {
                            if s.error.is_none() {
                                s.is_buffering = resolving || v;
                            }
                        });
                    }
                    DeckEvent::Position(v) => {
                        this.maybe_log_first_audio(v);
                        let should_emit = {
                            let mut d = this.data.lock().expect("playback lock poisoned");
                            if d.resolving || d.snapshot.error.is_some() {
                                false
                            } else if d.snapshot.position() == v {
                                false
                            } else {
                                let now = Instant::now();
                                if d.last_position_emit.is_some_and(|last| {
                                    now.duration_since(last) < Duration::from_millis(200)
                                }) {
                                    false
                                } else {
                                    d.last_position_emit = Some(now);
                                    if d.playback_attempt > 0
                                        && d.audio_open_done_at.is_some_and(|o| {
                                            now.duration_since(o) >= Duration::from_secs(5)
                                        })
                                    {
                                        d.playback_attempt = 0;
                                    }
                                    true
                                }
                            }
                        };
                        if should_emit {
                            this.tick_scrobble(v);
                            this.maybe_prefetch_from_position(v);
                            this.update_crossfade_schedule(v);
                            this.update_snapshot(|s| {
                                s.position_ms = v.as_millis() as u64;
                            });
                        }
                    }
                    DeckEvent::Buffered(v) => {
                        let should_emit = {
                            let mut d = this.data.lock().expect("playback lock poisoned");
                            if d.resolving || d.snapshot.error.is_some() {
                                false
                            } else if d.snapshot.buffered() == v {
                                false
                            } else {
                                let dur = d.snapshot.duration();
                                let complete = !dur.is_zero() && v >= dur;
                                let now = Instant::now();
                                if !complete
                                    && d.last_buffered_emit.is_some_and(|last| {
                                        now.duration_since(last) < Duration::from_millis(200)
                                    })
                                {
                                    false
                                } else {
                                    d.last_buffered_emit = Some(now);
                                    true
                                }
                            }
                        };
                        if should_emit {
                            this.update_snapshot(|s| {
                                s.buffered_ms = v.as_millis() as u64;
                            });
                        }
                    }
                    DeckEvent::DurationChanged(v) => {
                        let update_local = {
                            let mut d = this.data.lock().expect("playback lock poisoned");
                            if d.resolving || d.snapshot.error.is_some() {
                                None
                            } else {
                                d.snapshot.duration_ms = v.as_millis() as u64;
                                let cur_key =
                                    d.snapshot.current.as_ref().map(|t| t.queue_key());
                                let st_info = d.scrobble_track.as_ref().map(|st| {
                                    (st.queue_key(), st.title.clone(), st.artist.clone())
                                });
                                if let Some((st_key, title, artist)) = st_info {
                                    if Some(st_key) == cur_key {
                                        let had = d.scrobble_duration_sec > 0;
                                        d.scrobble_duration_sec = v.as_secs() as i64;
                                        if !had && v.as_millis() > 0 {
                                            Some((title, artist, v.as_millis() as i64))
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }
                        };
                        let snap = this.snapshot();
                        this.emit_snapshot(snap);
                        if let Some((title, artist, dur_ms)) = update_local {
                            this.sessions
                                .update_latest_local_play_duration(&title, &artist, dur_ms);
                        }
                    }
                    DeckEvent::Volume(_) | DeckEvent::Rate(_) => {
                        // Deliberately NOT copied into snapshot.volume.
                    }
                    DeckEvent::Completed => {
                        this.on_track_completed();
                    }
                    DeckEvent::Log(l) => {
                        if let Ok(mut d) = this.data.lock() {
                            d.mpv_log_tail.push_back(l);
                            while d.mpv_log_tail.len() > MPV_LOG_TAIL_MAX {
                                d.mpv_log_tail.pop_front();
                            }
                        }
                    }
                    DeckEvent::Error(e) => {
                        if is_filter_graph_error(&e) {
                            this.on_filter_graph_error(&e);
                            continue;
                        }
                        this.crumb(&format!("player error: {e}"));
                        let lower = e.to_lowercase();
                        if lower.contains("audio device") || lower.contains("audio driver") {
                            let this_rec = Arc::clone(&this);
                            tokio::spawn(async move {
                                this_rec.recover_audio_device().await;
                            });
                        }
                        let this_err = Arc::clone(&this);
                        tokio::spawn(async move {
                            this_err.on_player_error(false).await;
                        });
                    }
                    DeckEvent::AudioParams => {
                        this.refresh_ao_format();
                    }
                }
            }
        });
        if let Ok(mut d) = self.data.lock() {
            d.deck_event_tasks.push(handle);
        }
    }

    // ---- Crossfade & Standby Engine ---------------------------------------

    async fn ensure_standby_player(self: &Arc<Self>) -> Option<Arc<dyn MpvDeck>> {
        if let Some(sb) = self
            .data
            .lock()
            .ok()
            .and_then(|d| d.standby_player.clone())
        {
            return Some(sb);
        }
        let cache_dir = mpv_cache_dir_path();
        ensure_dir_exists_async(&cache_dir).await;
        let debug_ao = self.debug_audio_output.lock().ok().and_then(|g| g.clone());
        let (initial_af, initial_device, album) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (
                d.current_audio_filter.clone(),
                d.current_audio_device.clone(),
                d.standby_track
                    .as_ref()
                    .map(|t| t.album.clone())
                    .unwrap_or_default(),
            )
        };
        let config = DeckConfig {
            role: "standby",
            title: "LastWave Standby",
            buffer_bytes: 2 * 1024 * 1024,
            demuxer_max_bytes: "2MiB",
            demuxer_max_back_bytes: "0",
            timed_mix_enabled: self.timed_mix_enabled(),
            initial_af: initial_af.clone(),
            initial_audio_device: initial_device,
            debug_ao,
            log_file: None,
            cache_dir,
        };
        let sb = self.deck_factory.create(config).ok()?;
        let sb_id = sb.id();
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if let Some(ref existing) = d.standby_player {
                sb.dispose();
                return Some(Arc::clone(existing));
            }
            if !initial_af.is_empty() {
                d.af_written.insert(sb_id, initial_af);
            }
            d.standby_player = Some(Arc::clone(&sb));
        }
        self.bind_player_events(&sb);
        if !self.data.lock().is_ok_and(|d| d.exclusive_applied) {
            self.apply_loudness_to(Some(&sb), &album);
            self.apply_rate_and_pitch_to(&sb);
        }
        Some(sb)
    }

    pub fn cancel_crossfade(&self) {
        let (sb, primary, restore_chain, dj_eq_live, full_vol) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if let Some(t) = d.crossfade_task.take() {
                t.abort();
            }
            let ramping = d.rate_ramp_task.is_some();
            if let Some(t) = d.rate_ramp_task.take() {
                t.abort();
            }
            d.armed_dj_plan = None;
            d.beat_lock = None;
            let restore_chain = d.crossfading || d.standby_armed || ramping;
            let sb = if d.crossfading || d.standby_armed {
                d.standby_player.take()
            } else {
                None
            };
            let primary = d.player.clone();
            let dj_eq_live = d.dj_eq_live == Some(true);
            let full_vol = Self::full_deck_volume_from(&d);
            d.mix_started_at = None;
            d.crossfading = false;
            d.standby_armed = false;
            d.standby_ready = false;
            d.standby_track = None;
            d.standby_stream = None;
            d.standby_index = -1;
            (sb, primary, restore_chain, dj_eq_live, full_vol)
        };

        if dj_eq_live {
            self.set_deck_eq(primary.as_deref(), 0.0, 0.0);
        }
        if let Some(sb_deck) = sb {
            let _ = sb_deck.stop();
            sb_deck.dispose();
            if let Some(ref p) = primary {
                let _ = p.set_volume(full_vol);
            }
        }
        if restore_chain && !self.data.lock().is_ok_and(|d| d.exclusive_applied) {
            self.apply_rate_and_pitch();
        }
    }

    pub async fn apply_audio_prefs(&self) {
        self.rebuild_processing_filter();
        if !self.data.lock().is_ok_and(|d| d.exclusive_applied) {
            let album = self
                .snapshot()
                .current
                .map(|c| c.album)
                .unwrap_or_default();
            self.apply_loudness(&album);
            self.apply_rate_and_pitch();
        }
        self.refresh_overlap_prefs();
        self.notify_processing();
    }

    pub fn refresh_overlap_prefs(&self) {
        let exclusive = self
            .data
            .lock()
            .map(|d| d.exclusive_applied)
            .unwrap_or(false);
        if !self.overlap_enabled() || exclusive {
            self.cancel_crossfade();
        }
    }

    fn plan_for(&self, next: &PlayableTrack) -> MixPlan {
        let requested = self.overlap_length();
        if !self.timed_mix_enabled() {
            return MixPlan {
                style: MixStyle::Blend,
                overlap: requested,
                reason: String::new(),
            };
        }
        let mut d = self.data.lock().expect("playback lock poisoned");
        let cur = d.snapshot.current.clone();
        let incoming_dur = if d
            .standby_track
            .as_ref()
            .map(|t| t.queue_key())
            .as_deref()
            == Some(next.queue_key().as_str())
        {
            d.standby_player
                .as_ref()
                .map(|sb| sb.state().duration)
                .unwrap_or(Duration::ZERO)
        } else {
            Duration::ZERO
        };
        let in_dur = if !incoming_dur.is_zero() {
            incoming_dur
        } else {
            next.duration()
        };
        let key = format!(
            "{}>{next_key}|{}|{}|{}|{:?}|{}",
            cur.as_ref().map(|c| c.queue_key()).unwrap_or_default(),
            d.snapshot.duration().as_secs(),
            in_dur.as_secs(),
            requested.as_secs(),
            d.dj_eq_live,
            self.prefs.auto_mix_spectral(),
            next_key = next.queue_key()
        );
        if let Some(ref cached) = d.mix_plan {
            if key == d.mix_plan_key {
                return cached.clone();
            }
        }
        let plan = MixPlanner::plan(
            &MixTrackInfo::new(
                cur.as_ref().map(|c| c.title.as_str()).unwrap_or(""),
                cur.as_ref().map(|c| c.artist.as_str()).unwrap_or(""),
                cur.as_ref().map(|c| c.album.as_str()).unwrap_or(""),
                d.snapshot.duration(),
            ),
            &MixTrackInfo::new(&next.title, &next.artist, &next.album, in_dur),
            requested,
            d.dj_eq_live != Some(false),
            self.prefs.auto_mix_spectral(),
        );
        let prev_pair = d.mix_plan_key.split('|').next().unwrap_or("");
        let next_pair = key.split('|').next().unwrap_or("");
        if prev_pair != next_pair {
            TraceCrumb::write(&format!(
                "automix plan {} -> {} style={:?} overlap={}s reason={}",
                cur.as_ref().map(|c| c.queue_key()).unwrap_or_default(),
                next.queue_key(),
                plan.style,
                plan.overlap.as_secs(),
                plan.reason
            ));
        }
        d.mix_plan = Some(plan.clone());
        d.mix_plan_key = key;
        plan
    }

    fn current_overlap(&self) -> Duration {
        let next_track = {
            let d = self.data.lock().expect("playback lock poisoned");
            let ni = Self::next_index_from(&d, false);
            ni.and_then(|idx| d.snapshot.queue.get(idx).cloned())
        };
        let Some(next) = next_track else {
            return self.overlap_length();
        };
        let plan = self.plan_for(&next);
        if plan.mixes() {
            plan.overlap
        } else {
            Duration::ZERO
        }
    }

    fn update_crossfade_schedule(self: &Arc<Self>, position: Duration) {
        if !self.overlap_enabled() {
            let active = self
                .data
                .lock()
                .is_ok_and(|d| d.standby_armed || d.crossfading);
            if active {
                self.cancel_crossfade();
            }
            return;
        }
        let (exclusive, repeat_one, playing, resolving, has_error, crossfading, duration) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (
                d.exclusive_applied,
                d.snapshot.repeat_mode == RepeatMode::One,
                d.snapshot.is_playing,
                d.resolving,
                d.snapshot.error.is_some(),
                d.crossfading,
                d.snapshot.duration(),
            )
        };
        if exclusive || repeat_one {
            let active = self
                .data
                .lock()
                .is_ok_and(|d| d.standby_armed || d.crossfading);
            if active {
                self.cancel_crossfade();
            }
            return;
        }
        if !playing || resolving || has_error || crossfading || duration.as_secs() < 10 {
            return;
        }

        let (next_opt, standby_mismatch, armed) = {
            let d = self.data.lock().expect("playback lock poisoned");
            let ni = Self::next_index_from(&d, false);
            let nt = ni.and_then(|idx| d.snapshot.queue.get(idx).cloned().map(|t| (idx, t)));
            let mismatch = d.standby_armed
                && nt
                    .as_ref()
                    .is_some_and(|(_, t)| {
                        d.standby_track.as_ref().map(|st| st.queue_key())
                            != Some(t.queue_key())
                    });
            (nt, mismatch, d.standby_armed)
        };

        let Some((next_index, next_track)) = next_opt else {
            if armed {
                self.cancel_crossfade();
            }
            return;
        };
        if standby_mismatch {
            self.cancel_crossfade();
        }

        let plan = self.plan_for(&next_track);
        if !plan.mixes() {
            if self.data.lock().is_ok_and(|d| d.standby_armed) {
                self.cancel_crossfade();
            }
            return;
        }

        if self.timed_mix_enabled() {
            self.request_analyses(position, &next_track);
            if self.run_dj_schedule(position, &next_track, next_index, &plan) {
                return;
            }
        }

        let fade_duration = plan.overlap;
        if duration < fade_duration + Duration::from_secs(8) {
            if self.data.lock().is_ok_and(|d| d.standby_armed) {
                self.cancel_crossfade();
            }
            return;
        }
        let remaining = duration.saturating_sub(position);
        let resolve_lead = fade_duration + Duration::from_secs(12);
        let lead = resolve_lead.max(Duration::from_secs(30));
        if remaining <= lead && !remaining.is_zero() {
            self.maybe_early_resolve_next_track(next_track.clone(), next_index);
        }

        let (standby_armed, standby_ready, has_sb, gave_up_same) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (
                d.standby_armed,
                d.standby_ready,
                d.standby_player.is_some(),
                d.mix_gave_up_for == next_track.queue_key(),
            )
        };

        if remaining <= fade_duration + ARM_LEAD && !standby_armed && !gave_up_same {
            let this = Arc::clone(self);
            let track_clone = next_track.clone();
            tokio::spawn(async move {
                this.arm_standby_for_next(track_clone, next_index, None, None)
                    .await;
            });
        }

        if remaining <= fade_duration && standby_armed && standby_ready && has_sb {
            self.start_crossfade(fade_duration, plan.style == MixStyle::FilterSweep);
        }

        if standby_armed
            && !standby_ready
            && remaining <= fade_duration.saturating_sub(Duration::from_secs(3))
        {
            if let Ok(mut d) = self.data.lock() {
                d.mix_gave_up_for = next_track.queue_key();
            }
            self.crumb(&format!(
                "automix gave up key={} (deck not ready)",
                next_track.queue_key()
            ));
            self.cancel_crossfade();
        }
    }

    fn maybe_early_resolve_next_track(
        self: &Arc<Self>,
        next_track: PlayableTrack,
        _next_index: usize,
    ) {
        let cache_key = self.play_cache_key(&next_track, false);
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if d.play_cache.get(&cache_key).is_some() {
                return;
            }
            if d.resolve_inflight.contains_key(&cache_key) {
                return;
            }
            let now = Instant::now();
            if next_track.queue_key() == d.crossfade_resolved_key
                && d.last_crossfade_resolve_attempt
                    .is_some_and(|t| now.duration_since(t) < Duration::from_secs(10))
            {
                return;
            }
            d.crossfade_resolved_key = next_track.queue_key();
            d.last_crossfade_resolve_attempt = Some(now);
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            if let Some(_stream) = this.resolve_remote(&next_track, false, false).await {
                let maybe_arm = {
                    let d = this.data.lock().expect("playback lock poisoned");
                    let ni = Self::next_index_from(&d, false);
                    let valid = ni.and_then(|idx| {
                        let qt = d.snapshot.queue.get(idx)?;
                        (qt.queue_key() == next_track.queue_key()).then_some(idx)
                    });
                    valid.and_then(|idx| {
                        if d.exclusive_applied
                            || d.snapshot.repeat_mode == RepeatMode::One
                            || d.crossfading
                            || d.standby_armed
                            || d.mix_gave_up_for == next_track.queue_key()
                            || !d.snapshot.is_playing
                        {
                            return None;
                        }
                        let rem = d.snapshot.duration().saturating_sub(d.snapshot.position());
                        Some((idx, rem))
                    })
                };
                if let Some((current_next, rem)) = maybe_arm {
                    let fade = this.current_overlap();
                    if !fade.is_zero() && rem <= fade + ARM_LEAD {
                        this.arm_standby_for_next(next_track, current_next, None, None)
                            .await;
                    }
                }
            }
        });
    }

    async fn arm_standby_for_next(
        self: &Arc<Self>,
        next_track: PlayableTrack,
        next_index: usize,
        dj_plan: Option<DjMixPlan>,
        with_stream: Option<ResolvedStream>,
    ) {
        let stream = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if d.standby_armed
                || d.crossfading
                || d.exclusive_applied
                || d.snapshot.repeat_mode == RepeatMode::One
            {
                return;
            }
            let cache_key = self.play_cache_key(&next_track, false);
            let s = with_stream
                .or_else(|| self.local_stream(&next_track))
                .or_else(|| d.play_cache.get(&cache_key));
            let Some(stream) = s else {
                return;
            };
            d.standby_armed = true;
            d.standby_ready = false;
            d.standby_track = Some(next_track.clone());
            d.standby_index = next_index as i32;
            d.standby_stream = Some(stream.clone());
            d.armed_dj_plan = dj_plan.clone();
            stream
        };

        let start_at = dj_plan
            .as_ref()
            .filter(|p| p.incoming_start_at > 0.05)
            .map(|p| Duration::from_secs_f64(p.incoming_start_at));

        let Some(sb) = self.ensure_standby_player().await else {
            self.clear_standby_flags();
            return;
        };

        let _ = sb.set_volume(0.0);
        let mut play_url = stream.url.clone();
        if !play_url.starts_with("http") && !play_url.starts_with("file:") {
            if let Ok(u) = url::Url::from_file_path(&play_url) {
                play_url = u.to_string();
            }
        }

        let open_res = {
            let _guard = self.mpv_lock.lock().await;
            if stream.is_lossless {
                let _ = sb.set_property("demuxer-max-bytes", "4MiB");
                let _ = sb.set_property("demuxer-max-back-bytes", "1MiB");
            } else {
                let _ = sb.set_property("demuxer-max-bytes", "2MiB");
                let _ = sb.set_property("demuxer-max-back-bytes", "0");
            }
            self.apply_loudness_to(Some(&sb), &next_track.album);
            sb.open(&play_url, &stream.request_headers, start_at, false)
        };
        if open_res.is_err() {
            self.clear_standby_flags();
            return;
        }

        self.apply_loudness_to(Some(&sb), &next_track.album);
        self.apply_rate_and_pitch_to(&sb);
        if self.dj_eq_wanted() {
            let eq = dj_plan
                .as_ref()
                .map(|dp| DjMixPlanner::eq_at(dp, 0.0))
                .unwrap_or_else(|| DjMixPlanner::eq_for(false, 8, 0.0));
            self.set_deck_eq(Some(sb.as_ref()), eq.in_low, eq.in_high);
        }

        let still_valid = self.data.lock().is_ok_and(|d| {
            d.standby_armed
                && d.standby_track.as_ref().map(|t| t.queue_key()).as_deref()
                    == Some(next_track.queue_key().as_str())
                && d.snapshot.is_playing
                && !d.exclusive_applied
        });
        if !still_valid {
            let _ = sb.stop();
            self.clear_standby_flags();
            return;
        }

        let wait_start = Instant::now();
        while sb.state().duration.is_zero() && wait_start.elapsed() < Duration::from_secs(8) {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let keep = self.data.lock().is_ok_and(|d| {
                d.standby_armed
                    && d.standby_track.as_ref().map(|t| t.queue_key()).as_deref()
                        == Some(next_track.queue_key().as_str())
                    && !d.exclusive_applied
            });
            if !keep {
                return;
            }
        }
        if sb.state().duration.is_zero() {
            return;
        }
        if let Ok(mut d) = self.data.lock() {
            d.standby_ready = true;
        }
    }

    fn clear_standby_flags(&self) {
        if let Ok(mut d) = self.data.lock() {
            d.standby_armed = false;
            d.standby_ready = false;
            d.standby_track = None;
            d.standby_stream = None;
            d.standby_index = -1;
        }
    }

    // ---- Beat-Aware AutoMix -----------------------------------------------

    fn analysis_wanted(&self, key: &str) -> bool {
        let d = match self.data.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        if let (Some(cur), Some(cs)) = (&d.snapshot.current, &d.snapshot.stream) {
            if key == Self::tail_key(cur, cs, d.snapshot.duration()) {
                return true;
            }
        }
        if let Some(ni) = Self::next_index_from(&d, false) {
            if let Some(next) = d.snapshot.queue.get(ni) {
                let cache_key = self.play_cache_key_with(&d, next, false);
                let mut pc = d.play_cache.clone();
                if let Some(ns) = self.local_stream(next).or_else(|| pc.get(&cache_key)) {
                    return key == Self::head_key(next, &ns);
                }
            }
        }
        false
    }

    fn next_stream_for(&self, next: &PlayableTrack) -> Option<ResolvedStream> {
        let mut d = self.data.lock().ok()?;
        let now = Instant::now();
        if d.next_stream_memo_key == next.queue_key()
            && d.next_stream_memo_at
                .is_some_and(|t| now.duration_since(t) < Duration::from_secs(1))
        {
            return d.next_stream_memo.clone();
        }
        let cache_key = self.play_cache_key_with(&d, next, false);
        let stream = self
            .local_stream(next)
            .or_else(|| d.play_cache.get(&cache_key));
        d.next_stream_memo_key = next.queue_key();
        d.next_stream_memo_at = Some(now);
        d.next_stream_memo = stream.clone();
        stream
    }

    fn tail_key(t: &PlayableTrack, s: &ResolvedStream, dur: Duration) -> String {
        AutomixAnalyzer::key_for(
            &format!("{}@{}", t.queue_key(), dur.as_secs()),
            &AutomixAnalyzer::source_id(&s.cache_key),
            AnalysisSide::Tail,
        )
    }

    fn head_key(t: &PlayableTrack, s: &ResolvedStream) -> String {
        AutomixAnalyzer::key_for(
            &t.queue_key(),
            &AutomixAnalyzer::source_id(&s.cache_key),
            AnalysisSide::Head,
        )
    }

    fn request_analyses(self: &Arc<Self>, position: Duration, next: &PlayableTrack) {
        let (exclusive, cur, cs, dur) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (
                d.exclusive_applied,
                d.snapshot.current.clone(),
                d.snapshot.stream.clone(),
                d.snapshot.duration(),
            )
        };
        if exclusive {
            return;
        }
        let analyzer = Arc::clone(self.analyzer());
        if let (Some(c), Some(s)) = (cur, cs) {
            if position >= Duration::from_secs(8) && dur > Duration::from_secs(40) {
                let key = Self::tail_key(&c, &s, dur);
                let url = s.url.clone();
                let headers = s.request_headers.clone();
                let an = Arc::clone(&analyzer);
                tokio::spawn(async move {
                    let _ = an
                        .request(key, AnalysisSide::Tail, url, headers, dur)
                        .await;
                });
            }
        }
        if let Some(ns) = self.next_stream_for(next) {
            let key = Self::head_key(next, &ns);
            let url = ns.url.clone();
            let headers = ns.request_headers.clone();
            let an = Arc::clone(&analyzer);
            tokio::spawn(async move {
                let _ = an
                    .request(key, AnalysisSide::Head, url, headers, Duration::ZERO)
                    .await;
            });
        } else {
            let rem = dur.saturating_sub(position);
            if rem <= Duration::from_secs(90) && !rem.is_zero() {
                let ni = self
                    .data
                    .lock()
                    .ok()
                    .and_then(|d| Self::next_index_from(&d, false));
                if let Some(idx) = ni {
                    self.maybe_early_resolve_next_track(next.clone(), idx);
                }
            }
        }
    }

    fn dj_plan_for(
        self: &Arc<Self>,
        next: &PlayableTrack,
        earliest: Option<f64>,
    ) -> Option<DjMixPlan> {
        let ns = self.next_stream_for(next)?;
        let (cur, cs, dur, bars, allow_tempo, key, cached_plan) = {
            let d = self.data.lock().expect("playback lock poisoned");
            let cur = d.snapshot.current.clone()?;
            let cs = d.snapshot.stream.clone()?;
            let tk = Self::tail_key(&cur, &cs, d.snapshot.duration());
            let hk = Self::head_key(next, &ns);
            let bars = self.prefs.auto_mix_bars().max(1) as u32;
            let allow_tempo =
                (d.snapshot.pitch - 1.0).abs() <= 0.005 && d.mix_faults.beat_mix_allowed();
            let late = earliest
                .map(|e| format!("|late{}", (e / 4.0).floor() as i64))
                .unwrap_or_default();
            let key = format!("{tk}>{hk}|{bars}|{allow_tempo}{late}");
            let cached = if key == d.dj_plan_key {
                Some(d.dj_plan.clone())
            } else {
                None
            };
            (
                cur,
                cs,
                d.snapshot.duration(),
                bars,
                allow_tempo,
                key,
                cached,
            )
        };
        if let Some(plan) = cached_plan {
            return plan;
        }
        let tk = Self::tail_key(&cur, &cs, dur);
        let hk = Self::head_key(next, &ns);
        let analyzer = self.analyzer();
        let tail = analyzer.cached(&tk)?;
        let head = analyzer.cached(&hk)?;
        let in_dur = if !next.duration().is_zero() {
            next.duration().as_secs_f64()
        } else if head.window_end < 59.5 {
            head.window_end
        } else {
            1e6
        };
        let plan = DjMixPlanner::plan(
            &tail,
            &head,
            dur.as_secs_f64(),
            in_dur,
            bars,
            allow_tempo,
            earliest,
        );
        if let Ok(mut d) = self.data.lock() {
            d.dj_plan_key = key;
            d.dj_plan = plan.clone();
        }
        if let Some(ref p) = plan {
            self.crumb(&format!(
                "automix djplan {} -> {} {} bars={} out={:.2}s end={:.2}s in={:.2}s rate={:.4}",
                cur.queue_key(),
                next.queue_key(),
                p.reason,
                p.bars,
                p.mix_out_start,
                p.mix_out_end(),
                p.incoming_cue,
                p.rate
            ));
        } else {
            self.crumb(&format!(
                "automix djplan {} -> {} none",
                cur.queue_key(),
                next.queue_key()
            ));
        }
        plan
    }

    fn run_dj_schedule(
        self: &Arc<Self>,
        position: Duration,
        next: &PlayableTrack,
        next_index: usize,
        _mix_plan: &MixPlan,
    ) -> bool {
        let (standby_armed, armed_dj_plan, standby_ready, has_sb, gave_up_same) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (
                d.standby_armed,
                d.armed_dj_plan.clone(),
                d.standby_ready,
                d.standby_player.is_some(),
                d.mix_gave_up_for == next.queue_key(),
            )
        };
        if standby_armed && armed_dj_plan.is_none() {
            return false;
        }
        let mut dp = match armed_dj_plan.clone().or_else(|| self.dj_plan_for(next, None)) {
            Some(p) => p,
            None => return false,
        };
        let pos = position.as_secs_f64();
        if !standby_armed && pos >= dp.trigger_at() - 0.5 {
            let Some(late) = self.dj_plan_for(next, Some(pos + 8.0)) else {
                return false;
            };
            dp = late;
        }
        if !standby_armed && pos >= dp.trigger_at() - ARM_LEAD.as_secs_f64() && !gave_up_same {
            let this = Arc::clone(self);
            let next_owned = next.clone();
            let ns = self.next_stream_for(next);
            tokio::spawn(async move {
                this.arm_standby_for_next(next_owned, next_index, Some(dp), ns)
                    .await;
            });
            return true;
        }
        if standby_armed && standby_ready && has_sb {
            if let Some(armed_plan) = armed_dj_plan {
                if pos >= armed_plan.trigger_at() - 0.1 {
                    self.start_dj_mix(armed_plan);
                    return true;
                }
            }
        }
        if standby_armed && !standby_ready && pos >= dp.mix_out_start + 2.0 {
            if let Ok(mut d) = self.data.lock() {
                d.mix_gave_up_for = next.queue_key();
            }
            self.mix_fault(&format!("gave up key={} (deck not ready)", next.queue_key()));
            self.cancel_crossfade();
        }
        true
    }

    fn start_dj_mix(self: &Arc<Self>, plan: DjMixPlan) {
        let (sb, full_vol, speed, track_key) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if d.crossfading || !d.standby_armed {
                return;
            }
            let Some(sb) = d.standby_player.clone() else {
                return;
            };
            d.crossfading = true;
            let now = Instant::now();
            d.mix_started_at = Some(now);
            d.dj_mix_wall_start = Some(now);
            let full = Self::full_deck_volume_from(&d);
            d.crossfade_target_volume = full / 100.0;
            let spd = if d.snapshot.speed <= 0.0 {
                1.0
            } else {
                d.snapshot.speed
            };
            d.beat_lock = Some(DjBeatLock::new(plan.clone(), spd));
            let tk = d
                .standby_track
                .as_ref()
                .map(|t| t.queue_key())
                .unwrap_or_default();
            (sb, full, spd, tk)
        };
        let _ = full_vol;
        self.crumb(&format!(
            "automix start key={track_key} {} bars={} blend={:.1}s",
            plan.reason, plan.bars, plan.blend_sec
        ));
        if sb.set_volume(0.0).is_err()
            || (plan.beatmatched() && sb.set_rate(speed * plan.rate).is_err())
            || sb.play().is_err()
        {
            self.cancel_crossfade();
            return;
        }

        let max_wall = Duration::from_secs_f64(
            ((plan.mix_out_end() - plan.trigger_at()) / speed + 6.0).max(1.0),
        );
        let this = Arc::clone(self);
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(25));
            let mut last_eq = DjEq::new(f64::NAN, f64::NAN, f64::NAN, f64::NAN);
            let mut last_out_pos = -1.0_f64;
            let mut last_in_pos = -1.0_f64;
            let mut out_moved_at = Instant::now();
            let mut in_moved_at = Instant::now();

            loop {
                interval.tick().await;
                let (crossfading, out_opt, master_target, wall_start, sb_key) = {
                    let d = this.data.lock().expect("playback lock poisoned");
                    (
                        d.crossfading,
                        d.player.clone(),
                        d.crossfade_target_volume,
                        d.dj_mix_wall_start,
                        d.standby_track
                            .as_ref()
                            .map(|t| t.queue_key())
                            .unwrap_or_default(),
                    )
                };
                if !crossfading {
                    break;
                }
                let Some(out) = out_opt else {
                    continue;
                };
                let out_pos = out.state().position.as_secs_f64();
                let progress = ((out_pos - plan.mix_out_start) / plan.blend_sec).clamp(0.0, 1.0);
                let now = Instant::now();
                let in_pos_now = sb.state().position.as_secs_f64();
                if (out_pos - last_out_pos).abs() > f64::EPSILON {
                    last_out_pos = out_pos;
                    out_moved_at = now;
                }
                if (in_pos_now - last_in_pos).abs() > f64::EPSILON {
                    last_in_pos = in_pos_now;
                    in_moved_at = now;
                }
                let stall = Duration::from_secs(3);
                if now.duration_since(in_moved_at) > stall {
                    this.mix_fault(&format!("incoming deck stalled at {in_pos_now:.1}s"));
                    if let Ok(mut d) = this.data.lock() {
                        d.mix_gave_up_for = sb_key;
                    }
                    this.cancel_crossfade();
                    break;
                }
                if now.duration_since(out_moved_at) > stall {
                    this.mix_fault(&format!("outgoing deck stalled at {out_pos:.1}s"));
                    this.complete_crossfade();
                    break;
                }

                let master = (master_target * 100.0).clamp(0.0, 100.0);
                let (og, ig) = DjMixPlanner::gains(plan.beatmatched(), progress);
                let _ = out.set_volume((og * master).clamp(0.0, 100.0));
                let _ = sb.set_volume((ig * master).clamp(0.0, 100.0));

                let eq = DjMixPlanner::eq_at(
                    &plan,
                    if out_pos < plan.mix_out_start {
                        0.0
                    } else {
                        progress
                    },
                );
                if this.dj_eq_wanted() && Self::eq_moved(last_eq, eq) {
                    last_eq = eq;
                    this.set_deck_eq(Some(out.as_ref()), eq.out_low, eq.out_high);
                    this.set_deck_eq(Some(sb.as_ref()), eq.in_low, eq.in_high);
                }

                if plan.beatmatched() {
                    let o = Self::deck_clock(out.as_ref());
                    let i = Self::deck_clock(sb.as_ref());
                    let action = {
                        let mut d = this.data.lock().expect("playback lock poisoned");
                        if !d.crossfading {
                            None
                        } else if let Some(ref mut lock) = d.beat_lock {
                            lock.sample(o, i);
                            Some(lock.step(SystemTime::now()))
                        } else {
                            None
                        }
                    };
                    if let Some(act) = action {
                        if let Some(seek_to) = act.seek_to {
                            let _ = sb.seek(Duration::from_secs_f64(seek_to.max(0.0)));
                        } else if let Some(rate) = act.rate {
                            let _ = sb.set_rate(rate);
                        }
                    }
                }

                if progress >= 1.0
                    || wall_start.is_some_and(|w| now.duration_since(w) > max_wall)
                {
                    this.complete_crossfade();
                    break;
                }
            }
        });
        if let Ok(mut d) = self.data.lock() {
            if let Some(prev) = d.crossfade_task.replace(handle) {
                prev.abort();
            }
        }
    }

    fn dj_eq_wanted(&self) -> bool {
        let (broken, live) = self
            .data
            .lock()
            .map(|d| (d.dj_eq_broken, d.dj_eq_live))
            .unwrap_or((false, None));
        self.prefs.auto_mix_spectral() && !broken && live != Some(false)
    }

    fn eq_moved(a: DjEq, b: DjEq) -> bool {
        !((a.out_low - b.out_low).abs() < 0.4
            && (a.out_high - b.out_high).abs() < 0.4
            && (a.in_low - b.in_low).abs() < 0.4
            && (a.in_high - b.in_high).abs() < 0.4)
    }

    fn set_deck_eq(&self, deck: Option<&dyn MpvDeck>, low_db: f64, high_db: f64) {
        let Some(deck) = deck else {
            return;
        };
        if self
            .data
            .lock()
            .is_ok_and(|d| d.dj_eq_live == Some(false))
        {
            return;
        }
        let lo = format!("{low_db:.1}");
        let hi = format!("{high_db:.1}");
        let r1 = deck.command(&["af-command", "lwdj", "g", &lo, "equalizer@lwlo"]);
        let r2 = deck.command(&["af-command", "lwdj", "g", &hi, "equalizer@lwhi"]);
        if r1.is_ok() && r2.is_ok() {
            let became_live = {
                let mut d = self.data.lock().expect("playback lock poisoned");
                if d.dj_eq_live.is_none() {
                    d.dj_eq_live = Some(true);
                    true
                } else {
                    false
                }
            };
            if became_live {
                self.crumb("automix eq live");
            }
        } else {
            let became_unavail = {
                let mut d = self.data.lock().expect("playback lock poisoned");
                if d.dj_eq_live.is_none() {
                    d.dj_eq_live = Some(false);
                    true
                } else {
                    false
                }
            };
            if became_unavail {
                self.crumb("automix eq unavailable");
            }
        }
    }

    fn deck_clock(p: &dyn MpvDeck) -> f64 {
        if let Some(raw) = p.get_property("time-pos") {
            if let Ok(v) = raw.trim().parse::<f64>() {
                if v.is_finite() {
                    return v;
                }
            }
        }
        p.state().position.as_secs_f64()
    }

    fn ramp_rate_back(self: &Arc<Self>, deck: Arc<dyn MpvDeck>, from: f64) {
        let to = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if let Some(t) = d.rate_ramp_task.take() {
                t.abort();
            }
            if d.snapshot.speed <= 0.0 {
                1.0
            } else {
                d.snapshot.speed
            }
        };
        if (from - to).abs() < 0.001 {
            self.apply_rate_and_pitch_to(&deck);
            return;
        }
        let this = Arc::clone(self);
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            const STEPS: usize = 80;
            for i in 1..=STEPS {
                interval.tick().await;
                let still_primary = this
                    .data
                    .lock()
                    .ok()
                    .and_then(|d| d.player.as_ref().map(|p| p.id() == deck.id()))
                    .unwrap_or(false);
                if !still_primary {
                    break;
                }
                let p = i as f64 / STEPS as f64;
                let s = p * p * (3.0 - 2.0 * p);
                let _ = deck.set_rate(from + (to - from) * s);
            }
            this.apply_rate_and_pitch_to(&deck);
            if let Ok(mut d) = this.data.lock() {
                d.rate_ramp_task = None;
            }
        });
        if let Ok(mut d) = self.data.lock() {
            d.rate_ramp_task = Some(handle);
        }
    }

    fn full_deck_volume_from(d: &PlaybackData) -> f64 {
        let v = if d.lock_software_volume || d.hardware_volume {
            1.0
        } else {
            d.snapshot.volume
        };
        v.clamp(0.0, 1.0) * 100.0
    }

    fn expected_deck_rate_from(d: &PlaybackData) -> f64 {
        let pitch = d.snapshot.pitch.clamp(0.5, 1.5);
        if (pitch - 1.0).abs() > 0.005 {
            return pitch;
        }
        if d.snapshot.speed <= 0.0 {
            1.0
        } else {
            d.snapshot.speed
        }
    }

    fn mix_fault(&self, why: &str) {
        let msg = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let num = d.mix_faults.faults() + 1;
            self.crumb(&format!("automix fault #{num}: {why}"));
            d.mix_faults.record()
        };
        if let Some(change) = msg {
            self.crumb(&format!("automix safety: {change}"));
        }
    }

    fn abort_mix_safely(&self) {
        self.cancel_crossfade();
        let (sb, p, full_vol) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if let Some(t) = d.crossfade_task.take() {
                t.abort();
            }
            d.crossfading = false;
            d.standby_armed = false;
            d.standby_ready = false;
            d.armed_dj_plan = None;
            d.mix_started_at = None;
            (
                d.standby_player.clone(),
                d.player.clone(),
                Self::full_deck_volume_from(&d),
            )
        };
        if let Some(sb_deck) = sb {
            let _ = sb_deck.set_volume(0.0);
        }
        if let Some(p_deck) = p {
            let _ = p_deck.set_volume(full_vol);
        }
    }

    fn start_mix_watchdog(self: &Arc<Self>) {
        let this = Arc::clone(self);
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                this.run_mix_watchdog();
            }
        });
        if let Ok(mut d) = self.data.lock() {
            if let Some(prev) = d.mix_watchdog_task.replace(handle) {
                prev.abort();
            }
        }
    }

    pub fn run_mix_watchdog(self: &Arc<Self>) {
        let (repairs, sb, p, full_vol, exclusive, standby_ready, has_sb_track, standby_armed) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let Some(p) = d.player.clone() else {
                return;
            };
            let sb = d.standby_player.clone();
            let ps = p.state();
            let sb_reading = sb.as_ref().map(|s| {
                let ss = s.state();
                DeckReading {
                    volume: ss.volume,
                    playing: ss.playing,
                    rate: ss.rate,
                    position: ss.position,
                }
            });
            let mix_age = d
                .mix_started_at
                .map(|t| t.elapsed())
                .unwrap_or(Duration::ZERO);
            let repairs = check_mix(&MixGuardInput {
                mixing: d.crossfading,
                armed: d.standby_armed,
                ramping: d.rate_ramp_task.is_some(),
                full_volume: Self::full_deck_volume_from(&d),
                expected_rate: Self::expected_deck_rate_from(&d),
                primary: DeckReading {
                    volume: ps.volume,
                    playing: ps.playing,
                    rate: ps.rate,
                    position: ps.position,
                },
                standby: sb_reading,
                mix_age,
                max_mix_age: Duration::from_secs(60),
                resolving: d.resolving,
            });
            let confirmed: Vec<MixRepair> = repairs
                .iter()
                .copied()
                .filter(|r| {
                    *r == MixRepair::AbortStuckMix || d.pending_repairs.contains(r)
                })
                .collect();
            d.pending_repairs = repairs.into_iter().collect();
            for r in &confirmed {
                d.pending_repairs.remove(r);
            }
            (
                confirmed,
                sb,
                p,
                Self::full_deck_volume_from(&d),
                d.exclusive_applied,
                d.standby_ready,
                d.standby_track.is_some(),
                d.standby_armed,
            )
        };

        for r in repairs {
            self.mix_fault(&format!("watchdog {r:?}"));
            match r {
                MixRepair::AbortStuckMix => {
                    if standby_ready && has_sb_track {
                        self.complete_crossfade();
                    } else {
                        self.abort_mix_safely();
                    }
                }
                MixRepair::SilenceStandby => {
                    if let Some(ref sb_deck) = sb {
                        let _ = sb_deck.set_volume(0.0);
                        if !standby_armed {
                            let _ = sb_deck.stop();
                        }
                    }
                }
                MixRepair::RestorePrimaryVolume => {
                    let _ = p.set_volume(full_vol);
                }
                MixRepair::RestoreRate => {
                    if !exclusive {
                        self.apply_rate_and_pitch_to(&p);
                    }
                }
            }
        }
    }

    fn on_filter_graph_error(self: &Arc<Self>, text: &str) {
        let action_needed = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if !d.filter_error_logged {
                d.filter_error_logged = true;
                let clipped: String = text.chars().take(120).collect();
                self.crumb(&format!("filter rejected (not a stream error): {clipped}"));
            }
            let user_chain = merge_equalizer_and_headphone(&d.eq_filter, &d.hp_filter);
            if !d.eq_extras_dropped && user_chain.contains("crystalizer") {
                d.eq_extras_dropped = true;
                self.crumb("eq extras rejected -> dropped, reopening");
                true
            } else if !d.processing_disabled && !user_chain.is_empty() {
                d.processing_disabled = true;
                self.crumb("processing filter rejected -> disabled, reopening");
                true
            } else if !d.dj_eq_broken && d.current_audio_filter.contains("@lwdj") {
                d.dj_eq_broken = true;
                d.dj_eq_live = Some(false);
                self.crumb("automix eq stage rejected -> removed for this session");
                true
            } else {
                false
            }
        };
        if !action_needed {
            return;
        }
        self.rebuild_processing_filter();
        self.recover_from_filter_error();
        self.notify_processing();
    }

    fn recover_from_filter_error(self: &Arc<Self>) {
        let (in_mix, idx, pos, key) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (
                d.crossfading || d.standby_armed,
                d.snapshot.current_index,
                d.snapshot.position(),
                d.snapshot.current.as_ref().map(|t| t.queue_key()),
            )
        };
        if in_mix {
            self.apply_rate_and_pitch();
            return;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            this.apply_rate_and_pitch();
            if let Some(k) = key {
                if pos > Duration::from_secs(3) {
                    if let Ok(mut d) = this.data.lock() {
                        d.resume_key = k;
                        d.resume_at = Some(pos);
                    }
                }
            }
            if idx >= 0 {
                this.resolve_and_open(idx as usize, false, false, 0).await;
            }
        });
    }

    fn start_crossfade(self: &Arc<Self>, fade_duration: Duration, sweep: bool) {
        let (sb, timed_mix, eq_wanted) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if d.crossfading || !d.standby_armed {
                return;
            }
            let Some(sb) = d.standby_player.clone() else {
                return;
            };
            d.crossfading = true;
            d.crossfade_duration = Some(fade_duration);
            let now = Instant::now();
            d.crossfade_start_time = Some(now);
            d.mix_started_at = Some(now);
            d.crossfade_target_volume = Self::full_deck_volume_from(&d) / 100.0;
            let track_key = d
                .standby_track
                .as_ref()
                .map(|t| t.queue_key())
                .unwrap_or_default();
            let tm = self.timed_mix_enabled();
            let eq_w = sweep && tm && self.prefs.auto_mix_spectral() && !d.dj_eq_broken && d.dj_eq_live != Some(false);
            self.crumb(&format!(
                "automix start key={track_key} overlap={}s eq={eq_w}",
                fade_duration.as_secs()
            ));
            (sb, tm, eq_w)
        };
        if sb.play().is_err() {
            self.cancel_crossfade();
            return;
        }

        let this = Arc::clone(self);
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(25));
            let mut last_eq = DjEq::new(f64::NAN, f64::NAN, f64::NAN, f64::NAN);
            loop {
                interval.tick().await;
                let (crossfading, start_opt, dur_opt, target_vol, out_opt) = {
                    let d = this.data.lock().expect("playback lock poisoned");
                    (
                        d.crossfading,
                        d.crossfade_start_time,
                        d.crossfade_duration,
                        d.crossfade_target_volume,
                        d.player.clone(),
                    )
                };
                if !crossfading {
                    break;
                }
                let (Some(start), Some(dur)) = (start_opt, dur_opt) else {
                    break;
                };
                let denom = dur.as_secs_f64().max(0.001);
                let progress = (start.elapsed().as_secs_f64() / denom).clamp(0.0, 1.0);
                let master = (target_vol * 100.0).clamp(0.0, 100.0);
                let (out_gain, in_gain) = if timed_mix {
                    MixPlanner::gains(progress)
                } else {
                    let ang = progress * std::f64::consts::FRAC_PI_2;
                    (ang.cos(), ang.sin())
                };
                if let Some(ref out) = out_opt {
                    let _ = out.set_volume((out_gain * master).clamp(0.0, 100.0));
                }
                let _ = sb.set_volume((in_gain * master).clamp(0.0, 100.0));

                if eq_wanted {
                    let eq = DjMixPlanner::eq_for(false, 8, progress);
                    if Self::eq_moved(last_eq, eq) {
                        last_eq = eq;
                        this.set_deck_eq(out_opt.as_deref(), eq.out_low, eq.out_high);
                        this.set_deck_eq(Some(sb.as_ref()), eq.in_low, eq.in_high);
                    }
                }
                if progress >= 1.0 {
                    this.complete_crossfade();
                    break;
                }
            }
        });
        if let Ok(mut d) = self.data.lock() {
            if let Some(prev) = d.crossfade_task.replace(handle) {
                prev.abort();
            }
        }
    }

    fn write_af(&self, player: &Arc<dyn MpvDeck>, af: &str) {
        let already = {
            let d = self.data.lock().expect("playback lock poisoned");
            d.af_written.get(&player.id()).map(String::as_str) == Some(af)
        };
        if already {
            return;
        }
        if player.set_property("af", af).is_ok() {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.af_written.insert(player.id(), af.to_string());
            if d.player.as_ref().is_some_and(|p| p.id() == player.id()) {
                d.primary_af_changed_at = Some(Instant::now());
            }
        }
    }

    fn complete_crossfade(self: &Arc<Self>) {
        let (
            incoming_track,
            incoming_index,
            incoming_stream,
            sb,
            old_primary,
            dj_plan,
            full_vol,
            mean_err,
            playback_af,
        ) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if !d.crossfading {
                return;
            }
            d.mix_started_at = None;
            if let Some(t) = d.crossfade_task.take() {
                t.abort();
            }
            let Some(incoming_track) = d.standby_track.take() else {
                drop(d);
                self.cancel_crossfade();
                return;
            };
            let Some(sb) = d.standby_player.clone() else {
                drop(d);
                self.cancel_crossfade();
                return;
            };
            let incoming_index = d.standby_index;
            let incoming_stream = d.standby_stream.take();
            let old_primary = d.player.replace(Arc::clone(&sb));
            d.standby_player = old_primary.clone();
            let dj_plan = d.armed_dj_plan.take();
            let mean_err = d
                .beat_lock
                .as_ref()
                .map(|l| l.mean_audible_error())
                .unwrap_or(0.0);
            d.beat_lock = None;
            let full_vol = Self::full_deck_volume_from(&d);
            let playback_af = Self::playback_af_from(&d);

            let sb_state = sb.state();
            d.active_queue_key = incoming_track.queue_key();
            d.audio_clock_key = incoming_track.queue_key();
            d.audio_clock_start = Some(Instant::now());
            d.audio_open_done_at = Some(Instant::now());

            d.snapshot.current_index = incoming_index;
            d.snapshot.current = Some(incoming_track.clone());
            d.snapshot.stream = incoming_stream.clone();
            if let Some(ref st) = incoming_stream {
                d.snapshot.bitrate_kbps = st.bitrate_kbps;
            }
            d.snapshot.is_playing = true;
            d.snapshot.is_buffering = false;
            d.snapshot.position_ms = sb_state.position.as_millis() as u64;
            d.snapshot.duration_ms = sb_state.duration.as_millis() as u64;
            d.snapshot.error = None;

            d.crossfading = false;
            d.standby_armed = false;
            d.standby_ready = false;
            d.standby_index = -1;

            (
                incoming_track,
                incoming_index,
                incoming_stream,
                sb,
                old_primary,
                dj_plan,
                full_vol,
                mean_err,
                playback_af,
            )
        };
        let _ = (incoming_index, incoming_stream);

        self.flush_scrobble(true);
        if let Some(ref old) = old_primary {
            let _ = old.stop();
            let _ = old.set_volume(0.0);
            self.write_af(old, &playback_af);

            let this = Arc::clone(self);
            let old_clone = Arc::clone(old);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let take_old = {
                    let mut d = this.data.lock().expect("playback lock poisoned");
                    if d.standby_player
                        .as_ref()
                        .is_some_and(|p| p.id() == old_clone.id())
                    {
                        d.standby_player.take()
                    } else {
                        None
                    }
                };
                if let Some(old_deck) = take_old {
                    let _ = old_deck.stop();
                    old_deck.dispose();
                }
            });
        }

        if let Some(ref dp) = dj_plan {
            self.crumb(&format!(
                "automix handoff key={} {} phase_err_ms={}",
                incoming_track.queue_key(),
                dp.reason,
                (mean_err * 1000.0).round() as i64
            ));
        }

        let _ = sb.set_volume(full_vol);
        self.set_deck_eq(Some(sb.as_ref()), 0.0, 0.0);
        if dj_plan.as_ref().is_none_or(|dp| !dp.beatmatched()) {
            self.apply_rate_and_pitch_to(&sb);
        }
        self.apply_loudness_to(Some(&sb), &incoming_track.album);
        if dj_plan.as_ref().is_some_and(|dp| dp.beatmatched()) {
            let cur_rate = sb.state().rate;
            self.ramp_rate_back(Arc::clone(&sb), cur_rate);
        }
        if !sb.state().playing {
            let _ = sb.play();
        }

        self.emit_snapshot(self.snapshot());
        self.begin_scrobble_window(&incoming_track);
        self.refresh_ao_format();
        self.prefetch_neighbors(false);
        self.maybe_refill_radio();
        self.schedule_persist();
    }

    /// Ordered teardown for app quit.
    pub async fn dispose_player(self: &Arc<Self>) {
        self.cancel_crossfade();
        self.flush_scrobble(false);
        let (standby, player, tasks) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if let Some(t) = d.mix_watchdog_task.take() {
                t.abort();
            }
            if let Some(t) = d.sleep_task.take() {
                t.abort();
            }
            if let Some(t) = d.persist_task.take() {
                t.abort();
            }
            if let Some(t) = d.eq_debounce_task.take() {
                t.abort();
            }
            d.resolve_generation += 1;
            d.resolving = false;
            d.resolving_index = -1;
            d.active_queue_key.clear();
            let tasks: Vec<JoinHandle<()>> = d.deck_event_tasks.drain(..).collect();
            (d.standby_player.take(), d.player.take(), tasks)
        };
        for t in tasks {
            t.abort();
        }
        if let Some(sb) = standby {
            let _ = sb.stop();
            sb.dispose();
        }
        if let Some(p) = player {
            let _ = tokio::time::timeout(Duration::from_secs(2), async {
                let _guard = self.mpv_lock.lock().await;
                let _ = p.stop();
                p.dispose();
            })
            .await;
        }
    }

    // ---- Queue API --------------------------------------------------------

    pub async fn play(
        self: &Arc<Self>,
        track: PlayableTrack,
        source_label: &str,
        start_radio: bool,
    ) {
        self.play_queue(vec![track], 0, source_label, false, start_radio)
            .await;
    }

    pub async fn play_queue(
        self: &Arc<Self>,
        tracks: Vec<PlayableTrack>,
        start_index: usize,
        source_label: &str,
        start_shuffled: bool,
        endless_radio: bool,
    ) {
        self.cancel_crossfade();
        self.ensure_player().await;
        if tracks.is_empty() {
            return;
        }
        let mut index = start_index.min(tracks.len() - 1);
        let mut queue = tracks.clone();
        let mut shuffle = self.snapshot().shuffle_enabled;
        if start_shuffled {
            shuffle = true;
            let start_id = tracks[index].media_id();
            Self::shuffle_slice(&mut queue);
            index = queue
                .iter()
                .position(|t| t.media_id() == start_id)
                .unwrap_or(0);
        }
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.endless_radio = endless_radio;
            d.radio_seeds.clear();
            d.unavailable.clear();
            d.lossless_bypass.clear();
            d.prefetch_seed_key.clear();
            Self::rebuild_shuffle_order_in(&mut d, queue.len(), index as i32);
            d.snapshot.queue = queue.clone();
            d.snapshot.current_index = index as i32;
            d.snapshot.current = Some(queue[index].clone());
            d.snapshot.source_label = source_label.to_string();
            d.snapshot.shuffle_enabled = shuffle;
            d.snapshot.is_buffering = true;
            d.snapshot.error = None;
            d.snapshot.position_ms = 0;
            d.snapshot.duration_ms = 0;
        }
        self.emit_snapshot(self.snapshot());
        self.resolve_and_open(index, false, false, 0).await;
    }

    pub async fn play_next(self: &Arc<Self>, track: PlayableTrack) {
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let at = if d.snapshot.current_index >= 0 {
                (d.snapshot.current_index as usize + 1).min(d.snapshot.queue.len())
            } else {
                d.snapshot.queue.len()
            };
            d.snapshot.queue.insert(at, track);
            let cur_idx = d.snapshot.current_index;
            let len = d.snapshot.queue.len();
            Self::rebuild_shuffle_order_in(&mut d, len, cur_idx);
        }
        self.emit_snapshot(self.snapshot());
        self.schedule_persist();
        self.prefetch_neighbors(false);
    }

    pub async fn play_next_all(self: &Arc<Self>, tracks: Vec<PlayableTrack>) {
        if tracks.is_empty() {
            return;
        }
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let at = if d.snapshot.current_index >= 0 {
                (d.snapshot.current_index as usize + 1).min(d.snapshot.queue.len())
            } else {
                d.snapshot.queue.len()
            };
            d.snapshot.queue.splice(at..at, tracks);
            let cur_idx = d.snapshot.current_index;
            let len = d.snapshot.queue.len();
            Self::rebuild_shuffle_order_in(&mut d, len, cur_idx);
        }
        self.emit_snapshot(self.snapshot());
        self.schedule_persist();
        self.prefetch_neighbors(false);
    }

    pub async fn add_to_queue(self: &Arc<Self>, track: PlayableTrack) {
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.snapshot.queue.push(track);
            let cur_idx = d.snapshot.current_index;
            let len = d.snapshot.queue.len();
            Self::rebuild_shuffle_order_in(&mut d, len, cur_idx);
        }
        self.emit_snapshot(self.snapshot());
        self.schedule_persist();
        self.prefetch_neighbors(false);
    }

    pub async fn add_to_queue_all(self: &Arc<Self>, tracks: Vec<PlayableTrack>) {
        if tracks.is_empty() {
            return;
        }
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.snapshot.queue.extend(tracks);
            let cur_idx = d.snapshot.current_index;
            let len = d.snapshot.queue.len();
            Self::rebuild_shuffle_order_in(&mut d, len, cur_idx);
        }
        self.emit_snapshot(self.snapshot());
        self.schedule_persist();
        self.prefetch_neighbors(false);
    }

    pub async fn remove_at(self: &Arc<Self>, index: usize) {
        enum Action {
            StopAndClear,
            ResolveCurrent(usize),
            Done,
        }
        let action = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if index >= d.snapshot.queue.len() {
                return;
            }
            d.snapshot.queue.remove(index);
            let cur_idx = d.snapshot.current_index;
            if (index as i32) < cur_idx {
                d.snapshot.current_index -= 1;
                let len = d.snapshot.queue.len();
                let new_cur = d.snapshot.current_index;
                Self::rebuild_shuffle_order_in(&mut d, len, new_cur);
                Action::Done
            } else if (index as i32) == cur_idx {
                if d.snapshot.queue.is_empty() {
                    Action::StopAndClear
                } else {
                    let clamped = index.min(d.snapshot.queue.len() - 1);
                    d.snapshot.current_index = clamped as i32;
                    d.snapshot.current = Some(d.snapshot.queue[clamped].clone());
                    Action::ResolveCurrent(clamped)
                }
            } else {
                let len = d.snapshot.queue.len();
                Self::rebuild_shuffle_order_in(&mut d, len, cur_idx);
                Action::Done
            }
        };
        match action {
            Action::StopAndClear => self.stop_and_clear().await,
            Action::ResolveCurrent(idx) => {
                self.emit_snapshot(self.snapshot());
                self.resolve_and_open(idx, false, false, 0).await;
            }
            Action::Done => {
                self.emit_snapshot(self.snapshot());
                self.schedule_persist();
            }
        }
    }

    pub async fn clear_upcoming(&self) {
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if d.snapshot.current_index < 0 {
                return;
            }
            let keep = (d.snapshot.current_index as usize + 1).min(d.snapshot.queue.len());
            d.snapshot.queue.truncate(keep);
        }
        self.emit_snapshot(self.snapshot());
        self.schedule_persist();
    }

    pub async fn move_in_queue(&self, old_index: usize, new_index: usize) {
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let len = d.snapshot.queue.len();
            if old_index >= len || new_index > len {
                return;
            }
            if old_index == new_index || old_index + 1 == new_index {
                return;
            }
            let cur_key = d.snapshot.current.as_ref().map(|c| c.queue_key());
            let item = d.snapshot.queue.remove(old_index);
            let mut adjusted = new_index;
            if new_index > old_index {
                adjusted -= 1;
            }
            let pos = adjusted.min(d.snapshot.queue.len());
            d.snapshot.queue.insert(pos, item);
            let mut current_index = cur_key
                .as_deref()
                .and_then(|k| d.snapshot.queue.iter().position(|t| t.queue_key() == k))
                .map(|i| i as i32)
                .unwrap_or_else(|| {
                    d.snapshot
                        .current_index
                        .clamp(0, d.snapshot.queue.len().saturating_sub(1) as i32)
                });
            if d.snapshot.queue.is_empty() {
                current_index = -1;
            }
            d.snapshot.current_index = current_index;
            d.snapshot.current = if current_index >= 0 {
                d.snapshot.queue.get(current_index as usize).cloned()
            } else {
                None
            };
            let qlen = d.snapshot.queue.len();
            Self::rebuild_shuffle_order_in(&mut d, qlen, current_index);
        }
        self.emit_snapshot(self.snapshot());
        self.schedule_persist();
    }

    // ---- Transport --------------------------------------------------------

    pub async fn toggle(self: &Arc<Self>) {
        let debounced = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let now = Instant::now();
            if d.last_toggle_time
                .is_some_and(|t| now.duration_since(t) < Duration::from_millis(150))
            {
                true
            } else {
                d.last_toggle_time = Some(now);
                false
            }
        };
        if debounced {
            return;
        }
        self.ensure_player().await;
        let snap = self.snapshot();
        if snap.error.is_some() {
            self.retry().await;
            return;
        }
        if snap.current.is_some() && snap.stream.is_none() && snap.current_index >= 0 {
            let resume_at = snap.position();
            self.resolve_and_open(snap.current_index as usize, false, false, 0)
                .await;
            if !resume_at.is_zero() {
                self.seek(resume_at).await;
            }
            return;
        }
        if snap.is_playing
            && self
                .data
                .lock()
                .is_ok_and(|d| d.crossfading || d.standby_armed)
        {
            self.cancel_crossfade();
        }
        let player = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.user_paused = d.snapshot.is_playing;
            d.player.clone()
        };
        if let Some(p) = player {
            let _ = p.play_or_pause();
        }
    }

    pub async fn play_resume(self: &Arc<Self>) {
        if let Ok(mut d) = self.data.lock() {
            d.user_paused = false;
        }
        self.ensure_player().await;
        let snap = self.snapshot();
        if snap.current.is_some() && snap.stream.is_none() && snap.current_index >= 0 {
            let resume_at = snap.position();
            self.resolve_and_open(snap.current_index as usize, false, false, 0)
                .await;
            if !resume_at.is_zero() {
                self.seek(resume_at).await;
            }
            return;
        }
        let p = self.data.lock().ok().and_then(|d| d.player.clone());
        if let Some(player) = p {
            let _ = player.play();
        }
    }

    pub async fn pause(&self) {
        if let Ok(mut d) = self.data.lock() {
            d.user_paused = true;
        }
        self.cancel_crossfade();
        let p = self.data.lock().ok().and_then(|d| d.player.clone());
        if let Some(player) = p {
            let _ = player.pause();
        }
        self.schedule_persist();
    }

    pub async fn seek(&self, position: Duration) {
        self.cancel_crossfade();
        let p = self.data.lock().ok().and_then(|d| d.player.clone());
        if let Some(player) = p {
            let _ = player.seek(position);
        }
        self.update_snapshot(|s| s.position_ms = position.as_millis() as u64);
    }

    pub async fn unlock_software_volume(&self) {
        if let Ok(mut d) = self.data.lock() {
            d.lock_software_volume = false;
            d.hardware_volume = false;
        }
    }

    pub async fn set_volume(&self, volume: f64, software: bool) {
        let v = volume.clamp(0.0, 1.0);
        let (player, set_100, full_vol) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.snapshot.volume = v;
            d.hardware_volume = !software;
            let set_100 = !software || d.lock_software_volume;
            let full = Self::full_deck_volume_from(&d);
            if !set_100 && d.crossfading {
                d.crossfade_target_volume = full / 100.0;
                drop(d);
                self.emit_snapshot(self.snapshot());
                return;
            }
            (d.player.clone(), set_100, full)
        };
        self.emit_snapshot(self.snapshot());
        if let Some(p) = player {
            let _ = p.set_volume(if set_100 { 100.0 } else { full_vol });
        }
    }

    async fn recover_audio_device(&self) {
        if !cfg!(target_os = "windows") {
            return;
        }
        let prev_dev = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let prev = d.current_audio_device.clone();
            d.current_audio_device = "auto".to_string();
            d.exclusive_applied = false;
            d.lock_software_volume = false;
            d.hardware_volume = false;
            prev
        };
        self.crumb(&format!("recoverAudioDevice -> auto (was={prev_dev})"));
        let _guard = self.mpv_lock.lock().await;
        let (p, full_vol) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (d.player.clone(), Self::full_deck_volume_from(&d))
        };
        if let Some(player) = p {
            let _ = player.set_property("audio-exclusive", "no");
            let _ = player.set_audio_device("auto");
            let _ = player.set_volume(full_vol);
        }
    }

    pub async fn configure_wasapi(
        self: &Arc<Self>,
        exclusive: bool,
        mpv_device: &str,
        lock_software_volume: bool,
        output_format: Option<PcmFormat>,
    ) {
        self.ensure_player().await;
        if !cfg!(target_os = "windows") {
            if let Ok(mut d) = self.data.lock() {
                d.lock_software_volume = false;
                d.exclusive_applied = false;
                d.wasapi_error = None;
                d.forced_ao_format = None;
            }
            self.refresh_ao_format();
            return;
        }
        if exclusive {
            self.cancel_crossfade();
            let sb = self
                .data
                .lock()
                .ok()
                .and_then(|mut d| d.standby_player.take());
            if let Some(sb_deck) = sb {
                let _ = sb_deck.stop();
                sb_deck.dispose();
            }
        }
        let forced = if exclusive {
            output_format.map(|f| mpv_sample_format(f.bit_depth).to_string())
        } else {
            None
        };
        let player_opt = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.lock_software_volume = lock_software_volume;
            if !lock_software_volume {
                d.hardware_volume = false;
            }
            d.current_audio_device = mpv_device.to_string();
            d.exclusive_applied = false;
            d.wasapi_error = None;
            d.forced_ao_format = forced.clone();
            d.player.clone()
        };
        let Some(player) = player_opt else {
            if let Ok(mut d) = self.data.lock() {
                d.exclusive_applied = false;
                d.wasapi_error = Some("Player unavailable".to_string());
            }
            return;
        };
        self.crumb(&format!(
            "configureWasapi exclusive={exclusive} device={mpv_device}"
        ));

        let full_vol = {
            let _guard = self.mpv_lock.lock().await;
            let _ = player.set_property("ao", "wasapi");
            let mut err: Option<String> = None;
            if let Err(e) = player.set_property("audio-exclusive", if exclusive { "yes" } else { "no" })
            {
                if exclusive {
                    err = Some(format!("Exclusive Mode denied: {e}"));
                }
            }
            let _ = player.set_property("gapless-audio", if exclusive { "weak" } else { "yes" });
            if let Some(ref fmt) = forced {
                if let Err(e) = player.set_property("audio-format", fmt) {
                    if exclusive && err.is_none() {
                        err = Some(format!("audio-format failed: {e}"));
                    }
                }
            }
            if exclusive {
                if player.set_property("af", "").is_ok() {
                    if let Ok(mut d) = self.data.lock() {
                        d.af_written.insert(player.id(), String::new());
                    }
                }
                let _ = player.set_property("replaygain", "no");
                let _ = player.set_property("replaygain-fallback", "0");
                let _ = player.set_property("audio-normalize-downmix", "no");
            }
            let _ = player.set_audio_device(mpv_device);
            let full = self
                .data
                .lock()
                .map(|d| Self::full_deck_volume_from(&d))
                .unwrap_or(100.0);
            let _ = player.set_volume(if lock_software_volume { 100.0 } else { full });
            if let Ok(mut d) = self.data.lock() {
                d.wasapi_error = err.clone();
                d.exclusive_applied = exclusive && err.is_none();
            }
            full
        };
        let _ = full_vol;

        if exclusive {
            if let Some(fmt) = output_format {
                self.update_snapshot(|s| {
                    s.output_format = Some(fmt);
                    s.output_is_float = false;
                });
            }
        }
        let applied = self
            .data
            .lock()
            .map(|d| d.exclusive_applied)
            .unwrap_or(false);
        if !applied {
            let album = self
                .snapshot()
                .current
                .map(|c| c.album)
                .unwrap_or_default();
            self.apply_loudness(&album);
            self.apply_rate_and_pitch();
        }
        self.refresh_ao_format();
    }

    pub async fn set_audio_filter(self: &Arc<Self>, af: String) {
        let this = Arc::clone(self);
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            this.rebuild_processing_filter();
            if !this.data.lock().is_ok_and(|d| d.exclusive_applied) {
                this.apply_rate_and_pitch();
            }
            this.notify_processing();
        });
        if let Ok(mut d) = self.data.lock() {
            d.eq_filter = af;
            if let Some(prev) = d.eq_debounce_task.replace(handle) {
                prev.abort();
            }
        }
    }

    fn rebuild_processing_filter(&self) {
        let hp = HeadphoneProfiles::by_id(Some(&self.prefs.headphone_profile()))
            .lavfi_filter()
            .to_string();
        let timed_mix = self.timed_mix_enabled();
        let mut d = self.data.lock().expect("playback lock poisoned");
        d.hp_filter = hp.clone();
        let mut eq = d.eq_filter.clone();
        if d.eq_extras_dropped {
            eq = crystalizer_strip_regex().replace_all(&eq, "").into_owned();
            if eq.ends_with("lavfi=[]") {
                eq.clear();
            }
        }
        let chain = if d.processing_disabled {
            String::new()
        } else {
            merge_equalizer_and_headphone(&eq, &hp)
        };
        if !timed_mix || d.dj_eq_broken {
            d.current_audio_filter = chain;
            return;
        }
        let merged = merge_equalizer_and_headphone(&chain, &format!("lavfi=[{DJ_EQ_BANDS}]"));
        d.current_audio_filter = merged.replacen("format=floatp,lavfi=[", "format=floatp,@lwdj:lavfi=[", 1);
    }

    fn playback_af_from(d: &PlaybackData) -> String {
        let base = &d.current_audio_filter;
        let pitch = d.snapshot.pitch.clamp(0.5, 1.5);
        if (pitch - 1.0).abs() <= 0.005 {
            return base.clone();
        }
        let rate = if d.snapshot.speed <= 0.0 {
            1.0
        } else {
            d.snapshot.speed
        };
        let scale = rate / pitch;
        let tempo = format!("scaletempo:scale={scale:.8}");
        if base.is_empty() {
            tempo
        } else {
            format!("{tempo},{base}")
        }
    }

    fn apply_rate_and_pitch(&self) {
        let (primary, standby) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (
                d.player.clone(),
                if d.standby_armed {
                    d.standby_player.clone()
                } else {
                    None
                },
            )
        };
        if let Some(p) = primary {
            self.apply_rate_and_pitch_to(&p);
        }
        if let Some(sb) = standby {
            self.apply_rate_and_pitch_to(&sb);
        }
    }

    fn apply_rate_and_pitch_to(&self, player: &Arc<dyn MpvDeck>) {
        let (rate, pitch, exclusive, af) = {
            let d = self.data.lock().expect("playback lock poisoned");
            let r = if d.snapshot.speed <= 0.0 {
                1.0
            } else {
                d.snapshot.speed
            };
            let p = d.snapshot.pitch.clamp(0.5, 1.5);
            let af = if d.exclusive_applied {
                String::new()
            } else {
                Self::playback_af_from(&d)
            };
            (r, p, d.exclusive_applied, af)
        };
        let _ = exclusive;
        if (pitch - 1.0).abs() <= 0.005 {
            let _ = player.set_property("audio-pitch-correction", "yes");
            let _ = player.set_rate(rate);
            self.write_af(player, &af);
            return;
        }
        let _ = player.set_property("audio-pitch-correction", "no");
        let _ = player.set_property("speed", &format!("{pitch:.4}"));
        self.write_af(player, &af);
    }

    fn apply_loudness(&self, album: &str) {
        let (primary, standby, sb_album) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (
                d.player.clone(),
                d.standby_player.clone(),
                d.standby_track
                    .as_ref()
                    .map(|t| t.album.clone())
                    .unwrap_or_else(|| album.to_string()),
            )
        };
        self.apply_loudness_to(primary.as_ref(), album);
        if standby.is_some() {
            self.apply_loudness_to(standby.as_ref(), &sb_album);
        }
    }

    fn apply_loudness_to(&self, player: Option<&Arc<dyn MpvDeck>>, album: &str) {
        let Some(player) = player else {
            return;
        };
        let mode = LoudnessMath::normalize_mode(Some(&self.prefs.loudness_mode()));
        let exclusive = self
            .data
            .lock()
            .is_ok_and(|d| d.exclusive_applied);
        if exclusive || mode == "off" {
            let _ = player.set_property("replaygain", "no");
            let _ = player.set_property("replaygain-fallback", "0");
            return;
        }
        let fallback = self.loudness_fallback_db(album);
        let _ = player.set_property("replaygain", mode);
        let _ = player.set_property(
            "replaygain-preamp",
            &format!("{:.1}", LoudnessMath::PREAMP_DB),
        );
        let _ = player.set_property("replaygain-clip", "yes");
        let _ = player.set_property("replaygain-fallback", &format!("{fallback:.2}"));
    }

    fn loudness_fallback_db(&self, album: &str) -> f64 {
        if self.prefs.loudness_mode() != "album" {
            return LoudnessMath::FALLBACK_DB;
        }
        let key = album.trim().to_lowercase();
        if key.is_empty() {
            return LoudnessMath::FALLBACK_DB;
        }
        let shared = self
            .data
            .lock()
            .ok()
            .and_then(|d| d.album_replay_gain_db.get(&key).copied());
        let Some(val) = shared else {
            return LoudnessMath::FALLBACK_DB;
        };
        if !val.is_finite() {
            return LoudnessMath::FALLBACK_DB;
        }
        (val + LoudnessMath::PREAMP_DB).clamp(-LoudnessMath::CLAMP_DB, LoudnessMath::CLAMP_DB)
    }

    fn remember_album_gain(&self, album: &str, gain_db: Option<f64>) {
        if self.prefs.loudness_mode() != "album" {
            return;
        }
        let key = album.trim().to_lowercase();
        let Some(gain) = gain_db else {
            return;
        };
        if key.is_empty() || !gain.is_finite() {
            return;
        }
        if let Ok(mut d) = self.data.lock() {
            match d.album_replay_gain_db.get(&key).copied() {
                None => {
                    d.album_replay_gain_db.insert(key, gain);
                }
                Some(prev) => {
                    if !prev.is_finite() {
                        return;
                    }
                    if !LoudnessMath::same_album_gain(Some(prev), Some(gain)) {
                        d.album_replay_gain_db.insert(key, f64::NAN);
                    }
                }
            }
        }
    }

    fn capture_album_gain(&self, track: &PlayableTrack) {
        if self.prefs.loudness_mode() == "off" {
            return;
        }
        let player = self.data.lock().ok().and_then(|d| d.player.clone());
        let Some(p) = player else {
            return;
        };
        let gain = Self::read_album_gain_property(p.as_ref());
        self.remember_album_gain(&track.album, gain);
    }

    fn read_album_gain_property(p: &dyn MpvDeck) -> Option<f64> {
        for key in ["metadata/REPLAYGAIN_ALBUM_GAIN", "metadata/replaygain_album_gain"] {
            if let Some(raw) = p.get_property(key) {
                if let Some(parsed) = LoudnessMath::parse_db(Some(&raw)) {
                    return Some(parsed);
                }
            }
        }
        if let Some(raw) = p.get_property("metadata") {
            if let Some(cap) = album_gain_prop_regex().captures(&raw) {
                if let Some(m) = cap.get(1) {
                    return LoudnessMath::parse_db(Some(m.as_str()));
                }
            }
        }
        None
    }

    fn refresh_ao_format(&self) {
        let (player, forced) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (d.player.clone(), d.forced_ao_format.clone())
        };
        let Some(p) = player else {
            return;
        };
        let Some(raw) = p.get_property("audio-out-params") else {
            return;
        };
        if raw.is_empty() {
            return;
        }
        let fmt_cap = out_format_regex()
            .captures(&raw)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str());
        if mpv_format_is_float(fmt_cap) {
            let rate = out_rate_regex()
                .captures(&raw)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<i32>().ok())
                .unwrap_or(0);
            self.update_snapshot(|s| {
                s.output_is_float = true;
                if rate > 0 {
                    s.output_format = Some(PcmFormat::new(rate, 32, 2));
                }
            });
            wasapi_log("AO is float — not bit-perfect");
            return;
        }
        if let Some(parsed) = pcm_from_mpv_out_params(&raw, forced.as_deref(), 2) {
            self.update_snapshot(|s| {
                s.output_format = Some(parsed);
                s.output_is_float = false;
            });
        }
    }

    pub async fn next(self: &Arc<Self>) {
        self.cancel_crossfade();
        let next_idx = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let Some(ni) = Self::next_index_from(&d, false) else {
                return;
            };
            d.snapshot.current_index = ni as i32;
            d.snapshot.current = Some(d.snapshot.queue[ni].clone());
            d.snapshot.is_buffering = true;
            d.snapshot.error = None;
            d.snapshot.position_ms = 0;
            d.snapshot.duration_ms = 0;
            ni
        };
        self.emit_snapshot(self.snapshot());
        self.resolve_and_open(next_idx, false, false, 0).await;
    }

    pub async fn previous(self: &Arc<Self>) {
        self.cancel_crossfade();
        if self.snapshot().position() > Duration::from_secs(5) {
            self.seek(Duration::ZERO).await;
            return;
        }
        let prev_idx = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if let Some(pi) = Self::prev_index_from(&d) {
                let qkey = d.snapshot.queue[pi].queue_key();
                d.unavailable.remove(&qkey);
                d.snapshot.current_index = pi as i32;
                d.snapshot.current = Some(d.snapshot.queue[pi].clone());
                d.snapshot.is_buffering = true;
                d.snapshot.error = None;
                d.snapshot.position_ms = 0;
                d.snapshot.duration_ms = 0;
                Some(pi)
            } else {
                None
            }
        };
        let Some(prev_idx) = prev_idx else {
            self.seek(Duration::ZERO).await;
            return;
        };
        self.emit_snapshot(self.snapshot());
        self.resolve_and_open(prev_idx, false, false, 0).await;
    }

    pub async fn seek_to_queue_item(self: &Arc<Self>, index: usize) {
        self.cancel_crossfade();
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if index >= d.snapshot.queue.len() {
                return;
            }
            let qkey = d.snapshot.queue[index].queue_key();
            d.unavailable.remove(&qkey);
            d.snapshot.current_index = index as i32;
            d.snapshot.current = Some(d.snapshot.queue[index].clone());
            d.snapshot.is_buffering = true;
            d.snapshot.error = None;
            d.snapshot.position_ms = 0;
            d.snapshot.duration_ms = 0;
        }
        self.emit_snapshot(self.snapshot());
        self.resolve_and_open(index, false, false, 0).await;
    }

    pub async fn toggle_shuffle(&self) {
        self.cancel_crossfade();
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let enabled = !d.snapshot.shuffle_enabled;
            let len = d.snapshot.queue.len();
            let cur = d.snapshot.current_index;
            Self::rebuild_shuffle_order_in(&mut d, len, cur);
            d.snapshot.shuffle_enabled = enabled;
        }
        self.emit_snapshot(self.snapshot());
        self.schedule_persist();
    }

    pub async fn cycle_repeat(&self) {
        self.cancel_crossfade();
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.snapshot.repeat_mode = match d.snapshot.repeat_mode {
                RepeatMode::Off => RepeatMode::All,
                RepeatMode::All => RepeatMode::One,
                RepeatMode::One => RepeatMode::Off,
            };
        }
        self.emit_snapshot(self.snapshot());
        self.schedule_persist();
    }

    pub async fn set_speed(&self, speed: f64) {
        let clamped = speed.clamp(0.5, 2.0);
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if let Some(t) = d.rate_ramp_task.take() {
                t.abort();
            }
            d.snapshot.speed = clamped;
        }
        self.emit_snapshot(self.snapshot());
        self.apply_rate_and_pitch();
        self.schedule_persist();
        self.notify_processing();
    }

    pub async fn set_pitch(&self, pitch: f64) {
        let clamped = pitch.clamp(0.5, 1.5);
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if let Some(t) = d.rate_ramp_task.take() {
                t.abort();
            }
            d.snapshot.pitch = clamped;
        }
        self.emit_snapshot(self.snapshot());
        self.apply_rate_and_pitch();
        self.schedule_persist();
        self.notify_processing();
    }

    pub async fn cycle_speed(&self) {
        const STEPS: [f64; 5] = [0.75, 1.0, 1.25, 1.5, 2.0];
        let speed = self.snapshot().speed;
        let idx = STEPS
            .iter()
            .position(|&s| s >= speed)
            .unwrap_or(STEPS.len() - 1);
        let next = (idx + 1) % STEPS.len();
        self.set_speed(STEPS[next]).await;
    }

    pub fn set_sleep_timer(self: &Arc<Self>, duration: Option<Duration>) {
        let had = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let had = d.sleep_deadline.is_some();
            if let Some(t) = d.sleep_task.take() {
                t.abort();
            }
            if duration.is_none_or(|dur| dur.is_zero()) {
                d.sleep_deadline = None;
                d.snapshot.sleep_remaining_ms = None;
                d.snapshot.sleep_minutes = None;
                had
            } else {
                let dur = duration.unwrap();
                d.sleep_deadline = Some(Instant::now() + dur);
                let mins = (dur.as_secs() / 60).max(1) as i32;
                d.snapshot.sleep_remaining_ms = Some(dur.as_millis() as u64);
                d.snapshot.sleep_minutes = Some(mins);
                false
            }
        };
        self.emit_snapshot(self.snapshot());
        if duration.is_none_or(|dur| dur.is_zero()) {
            if had {
                let this = Arc::clone(self);
                tokio::spawn(async move {
                    this.pause().await;
                });
            }
            return;
        }

        let this = Arc::clone(self);
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let rem = {
                    let mut d = this.data.lock().expect("playback lock poisoned");
                    let rem = d
                        .sleep_deadline
                        .and_then(|dl| dl.checked_duration_since(Instant::now()));
                    if let Some(r) = rem.filter(|r| !r.is_zero()) {
                        d.snapshot.sleep_remaining_ms = Some(r.as_millis() as u64);
                        Some(r)
                    } else {
                        d.sleep_deadline = None;
                        d.sleep_task = None;
                        d.snapshot.sleep_remaining_ms = None;
                        d.snapshot.sleep_minutes = None;
                        None
                    }
                };
                this.emit_snapshot(this.snapshot());
                if rem.is_none() {
                    this.pause().await;
                    break;
                }
            }
        });
        if let Ok(mut d) = self.data.lock() {
            d.sleep_task = Some(handle);
        }
    }

    pub async fn retry(self: &Arc<Self>) {
        let (idx, cur) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if d.snapshot.current_index < 0 {
                return;
            }
            let cur = d.snapshot.current.clone();
            if let Some(ref c) = cur {
                let qk = c.queue_key();
                d.unavailable.remove(&qk);
                d.lossless_bypass.remove(&qk);
            }
            d.snapshot.error = None;
            d.snapshot.is_buffering = true;
            (d.snapshot.current_index as usize, cur)
        };
        if let Some(ref c) = cur {
            self.invalidate_play_cache(c);
        }
        self.emit_snapshot(self.snapshot());
        self.resolve_and_open(idx, false, true, 0).await;
    }

    pub async fn stop_and_clear(self: &Arc<Self>) {
        self.cancel_crossfade();
        self.flush_scrobble(false);
        let (player, standby) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.resolve_generation += 1;
            d.resolving = false;
            d.resolving_index = -1;
            d.active_queue_key.clear();
            (d.player.clone(), d.standby_player.clone())
        };
        self.crumb("stop requested (stopAndClear)");
        {
            let _guard = self.mpv_lock.lock().await;
            if let Some(ref p) = player {
                let _ = p.stop();
            }
            if let Some(ref sb) = standby {
                let _ = sb.stop();
            }
        }
        self.sessions.clear();
        self.lossless.clear_stream_cache();
        let reset_player = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.last_was_dash = false;
            d.endless_radio = false;
            d.play_cache.clear();
            d.resolve_inflight.clear();
            d.prefetch_inflight.clear();
            d.prefetch_seed_key.clear();
            let mut fresh = PlayerSnapshot::default();
            fresh.volume = d.snapshot.volume;
            d.snapshot = fresh;
            d.player.clone()
        };
        self.emit_snapshot(self.snapshot());
        if let Some(p) = reset_player {
            self.apply_rate_and_pitch_to(&p);
        }
    }

    // ---- Resolution Fast Path & Prefetch ----------------------------------

    fn resolve_and_open<'a>(
        self: &'a Arc<Self>,
        index: usize,
        force_youtube: bool,
        force_refresh: bool,
        attempt: u32,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let setup = {
                let mut d = self.data.lock().expect("playback lock poisoned");
                if index >= d.snapshot.queue.len() {
                    return;
                }
                let track = d.snapshot.queue[index].clone();
                let wanted_key = track.queue_key();
                if d.resolving
                    && d.resolving_index == index as i32
                    && d.active_queue_key == wanted_key
                    && attempt == 0
                    && !force_refresh
                    && !force_youtube
                {
                    return;
                }
                if attempt == 0 && !force_refresh && !force_youtube {
                    d.resume_key.clear();
                    d.resume_at = None;
                }
                d.resolve_generation += 1;
                let generation = d.resolve_generation;
                d.resolving = true;
                d.resolving_index = index as i32;
                d.playback_attempt = attempt;
                d.active_queue_key = wanted_key.clone();
                d.user_paused = false;
                d.audio_clock_start = Some(Instant::now());
                d.audio_clock_key = wanted_key.clone();
                d.audio_clock_generation = Some(generation);
                d.audio_open_done_at = None;

                let cache_key = self.play_cache_key_with(&d, &track, force_youtube);
                let cached = if !force_refresh {
                    d.play_cache.get(&cache_key)
                } else {
                    None
                };
                d.snapshot.is_buffering = true;
                d.snapshot.error = None;
                d.snapshot.stream = cached.clone();
                d.snapshot.position_ms = 0;
                d.snapshot.buffered_ms = 0;
                d.snapshot.duration_ms = 0;
                d.snapshot.bitrate_kbps = cached.as_ref().map(|s| s.bitrate_kbps).unwrap_or(0);
                let unavail = d.unavailable.contains(&wanted_key);
                (track, wanted_key, generation, cached, unavail)
            };

            let (track, wanted_key, generation, cached, unavail) = setup;
            let total_sw = Instant::now();
            self.emit_snapshot(self.snapshot());

            if self.is_stale(generation, &wanted_key) {
                self.release_resolving(generation);
                return;
            }
            if unavail && attempt == 0 {
                self.skip_unavailable(index).await;
                self.release_resolving(generation);
                return;
            }
            self.begin_scrobble_window(&track);

            if let Some(local) = self.local_stream(&track) {
                if let Ok(mut d) = self.data.lock() {
                    if d.resolve_generation == generation {
                        d.resolving = false;
                    }
                }
                let local_sw = Instant::now();
                self.open(&track, &local, generation, &wanted_key).await;
                self.log_timing(&format!(
                    "play key={wanted_key} path=local mpv_open_ms={} total_ms={}",
                    local_sw.elapsed().as_millis(),
                    total_sw.elapsed().as_millis()
                ));
                self.release_resolving(generation);
                return;
            }

            if cached.is_none() {
                self.crumb(&format!(
                    "stop requested resolve={index} key={wanted_key}"
                ));
                let stop_sw = Instant::now();
                let p = self.data.lock().ok().and_then(|d| d.player.clone());
                {
                    let _guard = self.mpv_lock.lock().await;
                    if let Some(player) = p {
                        let _ = player.stop();
                    }
                }
                self.crumb(&format!(
                    "stop done key={wanted_key} ms={}",
                    stop_sw.elapsed().as_millis()
                ));
            }

            let resolve_sw = Instant::now();
            let stream = match cached.clone() {
                Some(s) => Some(s),
                None => self.resolve_remote(&track, force_youtube, force_refresh).await,
            };
            let resolve_ms = if cached.is_some() {
                0
            } else {
                resolve_sw.elapsed().as_millis()
            };

            let queue_matches = {
                let d = self.data.lock().expect("playback lock poisoned");
                d.resolve_generation == generation
                    && d.snapshot
                        .queue
                        .get(index)
                        .is_some_and(|t| t.queue_key() == wanted_key)
            };
            if !queue_matches {
                self.release_resolving(generation);
                return;
            }

            let Some(stream) = stream else {
                if self.is_stale(generation, &wanted_key) {
                    self.release_resolving(generation);
                    return;
                }
                if attempt == 0 && !force_youtube && self.would_try_lossless(&track, false) {
                    self.resolve_and_open(index, true, false, 1).await;
                    self.release_resolving(generation);
                    return;
                }
                if let Ok(mut d) = self.data.lock() {
                    d.unavailable.insert(wanted_key.clone());
                }
                if !self.is_stale(generation, &wanted_key) {
                    self.skip_unavailable(index).await;
                }
                self.release_resolving(generation);
                return;
            };

            let open_sw = Instant::now();
            self.open(&track, &stream, generation, &wanted_key).await;
            if self.is_stale(generation, &wanted_key) {
                self.release_resolving(generation);
                return;
            }
            self.log_timing(&format!(
                "play key={wanted_key} path=remote playcache_hit={} resolve_ms={resolve_ms} mpv_open_ms={} total_ms={}",
                cached.is_some(),
                open_sw.elapsed().as_millis(),
                total_sw.elapsed().as_millis()
            ));
            self.maybe_refill_radio();
            self.release_resolving(generation);
        })
    }

    fn is_stale(&self, generation: u64, wanted_key: &str) -> bool {
        let d = self.data.lock().expect("playback lock poisoned");
        d.resolve_generation != generation || d.active_queue_key != wanted_key
    }

    fn release_resolving(&self, generation: u64) {
        if let Ok(mut d) = self.data.lock() {
            if d.resolve_generation == generation {
                d.resolving = false;
                d.resolving_index = -1;
            }
        }
    }

    fn local_stream(&self, track: &PlayableTrack) -> Option<ResolvedStream> {
        let path = if !track.playback_url.is_empty() && std::path::Path::new(&track.playback_url).exists()
        {
            Some(track.playback_url.clone())
        } else {
            self.downloads.local_path_for(&track.title, &track.artist)
        }?;
        let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
        let is_lossless = ext == "flac" || ext == "wav";
        let uri = url::Url::from_file_path(&path)
            .map(|u| u.to_string())
            .unwrap_or(path);
        let mime = match ext.as_str() {
            "flac" => "audio/flac",
            "mp3" => "audio/mpeg",
            _ => "audio/mp4",
        };
        Some(ResolvedStream {
            url: uri,
            mime_type: mime.to_string(),
            bitrate_kbps: if is_lossless { 1411 } else { 256 },
            audio_codec: if is_lossless {
                "FLAC".to_string()
            } else {
                ext.to_uppercase()
            },
            cache_key: format!("local:{}", track.queue_key()),
            is_lossless,
            ..Default::default()
        })
    }

    fn play_cache_key(&self, track: &PlayableTrack, force_youtube: bool) -> String {
        if force_youtube {
            format!("{}|yt|{}", track.queue_key(), self.prefs.prefer_lossless())
        } else {
            format!(
                "{}|{}|{}",
                track.queue_key(),
                self.prefs.lossless_quality(),
                self.prefs.prefer_lossless()
            )
        }
    }

    fn play_cache_key_with(
        &self,
        _d: &PlaybackData,
        track: &PlayableTrack,
        force_youtube: bool,
    ) -> String {
        self.play_cache_key(track, force_youtube)
    }

    fn invalidate_play_cache(&self, track: &PlayableTrack) {
        let prefix = format!("{}|", track.queue_key());
        if let Ok(mut d) = self.data.lock() {
            d.play_cache.invalidate_where(|k| k.starts_with(&prefix));
        }
        self.lossless.invalidate_stream(&track.title, &track.artist);
    }

    async fn resolve_remote(
        self: &Arc<Self>,
        track: &PlayableTrack,
        force_youtube: bool,
        force_refresh: bool,
    ) -> Option<ResolvedStream> {
        let cache_key = self.play_cache_key(track, force_youtube);
        let (pending_rx, tx_opt) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if !force_refresh {
                if let Some(hit) = d.play_cache.get(&cache_key) {
                    return Some(hit);
                }
                if let Some(pending) = d.resolve_inflight.get(&cache_key) {
                    (Some(pending.clone()), None)
                } else {
                    let (tx, rx) = tokio::sync::watch::channel(None);
                    d.resolve_inflight.insert(cache_key.clone(), rx);
                    (None, Some(tx))
                }
            } else {
                let (tx, rx) = tokio::sync::watch::channel(None);
                d.resolve_inflight.insert(cache_key.clone(), rx);
                (None, Some(tx))
            }
        };
        if let Some(mut rx) = pending_rx {
            loop {
                if let Some(val) = rx.borrow().clone() {
                    return val;
                }
                if rx.changed().await.is_err() {
                    return rx.borrow().clone().flatten();
                }
            }
        }

        let stream = self
            .resolve_remote_uncached(track, force_youtube, force_refresh)
            .await;
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if let Some(ref s) = stream {
                d.play_cache.put(cache_key.clone(), s.clone());
            }
            d.resolve_inflight.remove(&cache_key);
        }
        if let Some(tx) = tx_opt {
            let _ = tx.send(Some(stream.clone()));
        }
        stream
    }

    fn would_try_lossless(&self, track: &PlayableTrack, force_youtube: bool) -> bool {
        let q = self.prefs.lossless_quality();
        let bypassed = self
            .data
            .lock()
            .is_ok_and(|d| d.lossless_bypass.contains(&track.queue_key()));
        !force_youtube
            && !bypassed
            && self.prefs.prefer_lossless()
            && q != AudioQualityTiers::YOUTUBE_ONLY as i64
            && q != AudioQualityTiers::DATA_SAVER as i64
            && self.lossless.is_configured()
            && !track.artist.is_empty()
            && track.artist.to_lowercase() != "unknown artist"
    }

    async fn resolve_remote_uncached(
        self: &Arc<Self>,
        track: &PlayableTrack,
        force_youtube: bool,
        force_refresh: bool,
    ) -> Option<ResolvedStream> {
        let allow_lossless = self.would_try_lossless(track, force_youtube);
        let key = track.queue_key();
        if !allow_lossless {
            let yt_sw = Instant::now();
            let yt = self
                .resolve_youtube(track, &HashSet::new(), force_refresh)
                .await;
            self.log_timing(&format!(
                "resolve key={key} path=youtube-only youtube_ms={} hit={}",
                yt_sw.elapsed().as_millis(),
                yt.is_some()
            ));
            return yt;
        }
        let lossless_sw = Instant::now();
        let res = self
            .lossless
            .resolve_stream(
                &track.title,
                &track.artist,
                &track.album,
                track.duration().as_secs() as i64,
                self.prefs.lossless_quality(),
            )
            .await;
        let lossless_ms = lossless_sw.elapsed().as_millis();
        match res {
            Ok(Some(stream)) => {
                self.log_timing(&format!(
                    "resolve key={key} path=lossless-hit lossless_ms={lossless_ms}"
                ));
                return Some(stream);
            }
            Err(LosslessResolveError::Quota(msg)) => {
                if let Some(ref cb) = self.on_addon_quota {
                    cb(&msg);
                }
            }
            Ok(None) | Err(LosslessResolveError::Other(_)) => {}
        }
        let yt_sw = Instant::now();
        let yt = self
            .resolve_youtube(track, &HashSet::new(), force_refresh)
            .await;
        self.log_timing(&format!(
            "resolve key={key} path=lossless-miss+youtube lossless_ms={lossless_ms} youtube_ms={} hit={}",
            yt_sw.elapsed().as_millis(),
            yt.is_some()
        ));
        yt
    }

    async fn resolve_youtube(
        self: &Arc<Self>,
        track: &PlayableTrack,
        initial_excluded: &HashSet<String>,
        force_refresh: bool,
    ) -> Option<ResolvedStream> {
        let mut excluded = initial_excluded.clone();
        if !track.video_id.is_empty() && !excluded.contains(&track.video_id) {
            let direct_sw = Instant::now();
            let stream = self
                .tube
                .resolve_audio_stream(&track.video_id, force_refresh, self.prefer_smallest_youtube())
                .await;
            self.log_timing(&format!(
                "youtube key={} path=direct-videoId stream_ms={} hit={}",
                track.queue_key(),
                direct_sw.elapsed().as_millis(),
                stream.is_some()
            ));
            if let Some(s) = stream {
                self.remember_watchtime(&track.video_id, &s);
                return Some(s);
            }
            self.tube.report_playback_failure(&track.video_id);
            excluded.insert(track.video_id.clone());
        }

        for attempt in 0..2 {
            let match_sw = Instant::now();
            let matched = tokio::time::timeout(
                Duration::from_secs(20),
                self.tube.find_best_effort_match_or_null(
                    &track.title,
                    &track.artist,
                    &excluded,
                ),
            )
            .await
            .ok()
            .flatten();
            let match_ms = match_sw.elapsed().as_millis();
            let Some(m) = matched else {
                self.log_timing(&format!(
                    "youtube key={} attempt={attempt} match_ms={match_ms} result=no-match",
                    track.queue_key()
                ));
                return None;
            };
            if m.video_id.is_empty() {
                return None;
            }
            if excluded.contains(&m.video_id) {
                continue;
            }
            let vid = m.video_id.clone();
            self.adopt_video_id(track, &vid, &m);
            let stream_sw = Instant::now();
            let stream = self
                .tube
                .resolve_audio_stream(
                    &vid,
                    force_refresh && attempt == 0,
                    self.prefer_smallest_youtube(),
                )
                .await;
            self.log_timing(&format!(
                "youtube key={} attempt={attempt} match_ms={match_ms} stream_ms={} hit={}",
                track.queue_key(),
                stream_sw.elapsed().as_millis(),
                stream.is_some()
            ));
            if let Some(s) = stream {
                self.remember_watchtime(&vid, &s);
                return Some(s);
            }
            self.tube.report_playback_failure(&vid);
            excluded.insert(vid);
        }
        None
    }

    fn adopt_video_id(&self, track: &PlayableTrack, video_id: &str, matched: &YouTubeMusicTrack) {
        let updated_snap = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let idx = d
                .snapshot
                .queue
                .iter()
                .position(|t| t.media_id() == track.media_id());
            let Some(i) = idx else {
                return;
            };
            if !d.snapshot.queue[i].video_id.is_empty() {
                return;
            }
            let mut updated = d.snapshot.queue[i].clone();
            updated.video_id = video_id.to_string();
            if !OfficialArtworkService::is_official_artwork(&updated.artwork_url)
                && !matched.artwork_url.is_empty()
            {
                updated.artwork_url = matched.artwork_url.clone();
            }
            d.snapshot.queue[i] = updated.clone();
            if i as i32 == d.snapshot.current_index {
                d.snapshot.current = Some(updated);
            }
            d.snapshot.clone()
        };
        self.emit_snapshot(updated_snap);
    }

    fn prefetch_neighbors(self: &Arc<Self>, lookahead: bool) {
        let indices = {
            let d = self.data.lock().expect("playback lock poisoned");
            let mut list = Vec::new();
            let mut add = |delta: i32| {
                if let Some(i) = Self::neighbor_index_from(&d, delta) {
                    if i as i32 != d.snapshot.current_index && !list.contains(&i) {
                        list.push(i);
                    }
                }
            };
            add(1);
            add(-1);
            if lookahead {
                add(2);
            }
            list
        };
        if indices.is_empty() {
            return;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let _chain_guard = this.prefetch_lock.lock().await;
            for idx in indices {
                this.prefetch_index(idx).await;
            }
        });
    }

    fn neighbor_index_from(d: &PlaybackData, delta: i32) -> Option<usize> {
        let queue = &d.snapshot.queue;
        if queue.is_empty() || d.snapshot.current_index < 0 || delta == 0 {
            return None;
        }
        if d.snapshot.shuffle_enabled && !d.shuffle_order.is_empty() {
            let cur = d.snapshot.current_index as usize;
            let pos = d.shuffle_order.iter().position(|&x| x == cur)? as isize;
            let next = pos + delta as isize;
            if next >= 0 && (next as usize) < d.shuffle_order.len() {
                return Some(d.shuffle_order[next as usize]);
            }
            if d.snapshot.repeat_mode == RepeatMode::All && !queue.is_empty() {
                return if delta > 0 {
                    d.shuffle_order.first().copied()
                } else {
                    d.shuffle_order.last().copied()
                };
            }
            return None;
        }
        let i = d.snapshot.current_index as isize + delta as isize;
        if i >= 0 && (i as usize) < queue.len() {
            return Some(i as usize);
        }
        if d.snapshot.repeat_mode == RepeatMode::All && !queue.is_empty() {
            return Some(if delta > 0 { 0 } else { queue.len() - 1 });
        }
        None
    }

    async fn prefetch_index(self: &Arc<Self>, index: usize) {
        let track = {
            let d = self.data.lock().expect("playback lock poisoned");
            d.snapshot.queue.get(index).cloned()
        };
        let Some(track) = track else {
            return;
        };
        let qk = track.queue_key();
        if self.data.lock().is_ok_and(|d| d.unavailable.contains(&qk)) {
            return;
        }
        if self.local_stream(&track).is_some() {
            self.prefetch_artwork(&track);
            return;
        }
        let cache_key = self.play_cache_key(&track, false);
        let hit = self
            .data
            .lock()
            .ok()
            .and_then(|mut d| d.play_cache.get(&cache_key))
            .is_some();
        if hit {
            self.prefetch_artwork(&track);
            if !track.video_id.is_empty() {
                self.tube.prefetch_next_track(&track.video_id);
            }
            return;
        }
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if !d.prefetch_inflight.insert(qk.clone()) {
                return;
            }
        }
        let sw = Instant::now();
        self.crumb(&format!("prefetch start key={qk} index={index}"));
        let stream = self.resolve_remote(&track, false, false).await;
        self.crumb(&format!(
            "prefetch done key={qk} ms={} hit={}",
            sw.elapsed().as_millis(),
            stream.is_some()
        ));
        if let Some(ref s) = stream {
            if !s.artwork_url.is_empty()
                && !( !track.album.trim().is_empty()
                    && !s.album_title.trim().is_empty()
                    && !Self::same_album(&track.album, &s.album_title))
            {
                let alb = if !s.album_title.is_empty() {
                    s.album_title.as_str()
                } else {
                    track.album.as_str()
                };
                self.artwork_service
                    .remember_track(&track.title, &track.artist, &s.artwork_url, alb)
                    .await;
            }
        }
        self.prefetch_artwork(&track);
        if let Ok(mut d) = self.data.lock() {
            d.prefetch_inflight.remove(&qk);
        }
    }

    fn prefetch_artwork(&self, track: &PlayableTrack) {
        if OfficialArtworkService::is_official_artwork(&track.artwork_url) {
            return;
        }
        let art = Arc::clone(&self.artwork_service);
        let title = track.title.clone();
        let artist = track.artist.clone();
        let album = track.album.clone();
        tokio::spawn(async move {
            let _ = art.resolve_official_artwork(&title, &artist, &album).await;
        });
    }

    fn maybe_prefetch_from_position(self: &Arc<Self>, position: Duration) {
        let should_fire = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let dur = d.snapshot.duration();
            if dur.as_secs() < 8 {
                return;
            }
            let rem = dur.saturating_sub(position);
            if (position.as_millis() as f64) < dur.as_millis() as f64 * 0.55
                && rem > Duration::from_secs(28)
            {
                return;
            }
            let key = d
                .snapshot
                .current
                .as_ref()
                .map(|c| c.queue_key())
                .unwrap_or_default();
            if key.is_empty() || key == d.prefetch_seed_key {
                false
            } else {
                d.prefetch_seed_key = key;
                true
            }
        };
        if should_fire {
            self.prefetch_neighbors(true);
        }
    }

    pub fn same_album(a: &str, b: &str) -> bool {
        let clean_a = bracket_strip_regex().replace_all(a, "");
        let clean_b = bracket_strip_regex().replace_all(b, "");
        let na = OfficialArtworkService::normalize_for_search(&clean_a);
        let nb = OfficialArtworkService::normalize_for_search(&clean_b);
        if na.is_empty() || nb.is_empty() {
            return true;
        }
        na == nb || na.contains(&nb) || nb.contains(&na)
    }

    async fn open(
        self: &Arc<Self>,
        track: &PlayableTrack,
        stream: &ResolvedStream,
        generation: u64,
        wanted_key: &str,
    ) {
        let valid_target = {
            let d = self.data.lock().expect("playback lock poisoned");
            if d.resolve_generation != generation || d.active_queue_key != wanted_key {
                false
            } else {
                d.snapshot
                    .current
                    .as_ref()
                    .is_some_and(|c| c.queue_key() == wanted_key)
                    || (d.snapshot.current_index >= 0
                        && d.snapshot
                            .queue
                            .get(d.snapshot.current_index as usize)
                            .is_some_and(|t| t.queue_key() == wanted_key))
            }
        };
        if !valid_target {
            return;
        }
        self.cancel_crossfade();

        let mut effective_track = track.clone();
        let album_disagrees = !track.album.trim().is_empty()
            && !stream.album_title.trim().is_empty()
            && !Self::same_album(&track.album, &stream.album_title);
        if !album_disagrees
            && !stream.artwork_url.is_empty()
            && stream.artwork_url != track.artwork_url
        {
            effective_track.artwork_url = stream.artwork_url.clone();
            if !stream.album_title.is_empty() && effective_track.album.is_empty() {
                effective_track.album = stream.album_title.clone();
            }
            let updated_snap = {
                let mut d = self.data.lock().expect("playback lock poisoned");
                if let Some(i) = d
                    .snapshot
                    .queue
                    .iter()
                    .position(|t| t.queue_key() == wanted_key)
                {
                    d.snapshot.queue[i] = effective_track.clone();
                    if i as i32 == d.snapshot.current_index {
                        d.snapshot.current = Some(effective_track.clone());
                    }
                }
                d.snapshot.clone()
            };
            self.emit_snapshot(updated_snap);
        }

        if OfficialArtworkService::is_official_artwork(&effective_track.artwork_url) {
            self.artwork_service
                .remember_track(
                    &effective_track.title,
                    &effective_track.artist,
                    &effective_track.artwork_url,
                    &effective_track.album,
                )
                .await;
        }

        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if d.snapshot.current_index >= 0
                && d.snapshot
                    .queue
                    .get(d.snapshot.current_index as usize)
                    .is_some_and(|t| t.queue_key() == wanted_key)
            {
                d.snapshot.current = Some(effective_track.clone());
            }
            d.snapshot.stream = Some(stream.clone());
            d.snapshot.bitrate_kbps = stream.bitrate_kbps;
            d.snapshot.is_buffering = true;
            d.snapshot.error = None;
        }
        self.emit_snapshot(self.snapshot());

        if !OfficialArtworkService::is_official_artwork(&effective_track.artwork_url) {
            self.resolve_official_artwork_in_background(
                effective_track.clone(),
                wanted_key.to_string(),
                generation,
            );
        }

        let raw_url = &stream.url;
        let kind = if raw_url.starts_with("http") {
            "remote"
        } else if raw_url.starts_with("file:")
            || raw_url.contains(":\\")
            || raw_url.starts_with('/')
        {
            "local-file"
        } else {
            "other"
        };
        let ext = raw_url
            .split('?')
            .next()
            .unwrap_or("")
            .rsplit('.')
            .next()
            .unwrap_or("");
        self.crumb(&format!(
            "open key={wanted_key} kind={kind} ext={ext} mime={} lossless={} cache={}",
            stream.mime_type, stream.is_lossless, stream.cache_key
        ));
        let mut play_url = raw_url.clone();
        if kind == "local-file" && !raw_url.starts_with("file:") {
            if let Ok(u) = url::Url::from_file_path(raw_url) {
                play_url = u.to_string();
                self.crumb(&format!("open key={wanted_key} file-uri=yes"));
            }
        }
        let was_dash = self.data.lock().is_ok_and(|d| d.last_was_dash);
        let is_dash = stream.mime_type.contains("dash");
        let stop_ms;
        let mut load_ms = 0u128;
        let mut open_err: Option<String> = None;

        {
            let _guard = self.mpv_lock.lock().await;
            if self.is_stale(generation, wanted_key) {
                return;
            }
            let player = self.data.lock().ok().and_then(|d| d.player.clone());
            let stop_sw = Instant::now();
            if let Some(ref p) = player {
                let _ = p.stop();
            }
            stop_ms = stop_sw.elapsed().as_millis();
            if self.is_stale(generation, wanted_key) {
                return;
            }
            if was_dash {
                self.crumb(&format!("open key={wanted_key} dash-grace"));
                tokio::time::sleep(Duration::from_millis(500)).await;
                if self.is_stale(generation, wanted_key) {
                    return;
                }
            }
            if let Ok(mut d) = self.data.lock() {
                if d.resolve_generation == generation && d.active_queue_key == wanted_key {
                    d.audio_open_done_at = Some(Instant::now());
                }
            }
            if self.is_stale(generation, wanted_key) {
                return;
            }
            if let Some(ref p) = player {
                if stream.is_lossless {
                    let _ = p.set_property("demuxer-max-bytes", "4MiB");
                    let _ = p.set_property("demuxer-max-back-bytes", "1MiB");
                } else {
                    let _ = p.set_property("demuxer-max-bytes", "2MiB");
                    let _ = p.set_property("demuxer-max-back-bytes", "0");
                }
                self.apply_loudness_to(Some(p), &track.album);
                let resume_at = {
                    let mut d = self.data.lock().expect("playback lock poisoned");
                    if d.resume_key == wanted_key {
                        d.resume_key.clear();
                        d.resume_at.take()
                    } else {
                        None
                    }
                };
                if let Some(ra) = resume_at {
                    self.crumb(&format!("open key={wanted_key} resume={}s", ra.as_secs()));
                }
                let load_sw = Instant::now();
                if let Err(e) = p.open(&play_url, &stream.request_headers, resume_at, true) {
                    open_err = Some(e);
                } else if let Ok(mut d) = self.data.lock() {
                    d.last_was_dash = is_dash;
                }
                load_ms = load_sw.elapsed().as_millis();
            }
        }

        if let Some(err) = open_err {
            self.crumb(&format!(
                "open failed key={wanted_key} cache={} err={err}",
                stream.cache_key
            ));
            self.dump_mpv_log("_open.exception");
            if !self.is_stale(generation, wanted_key) {
                self.on_player_error(false).await;
            }
            return;
        }

        self.log_timing(&format!(
            "mpv key={wanted_key} kind={kind} stop_ms={stop_ms} load_ms={load_ms} dash_grace={}",
            if was_dash { 500 } else { 0 }
        ));
        if self.is_stale(generation, wanted_key) {
            return;
        }
        let (opened, exclusive) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (d.player.clone(), d.exclusive_applied)
        };
        if let Some(p) = opened {
            if !exclusive {
                self.apply_loudness_to(Some(&p), &track.album);
                self.apply_rate_and_pitch_to(&p);
                self.capture_album_gain(track);
            }
        }
    }

    fn dump_mpv_log(&self, trigger: &str) {
        if let Ok(mut d) = self.data.lock() {
            for l in &d.mpv_log_tail {
                TraceCrumb::write(&format!(
                    "mpv({trigger}) [{}/{}] {}",
                    l.prefix, l.level, l.text
                ));
            }
            d.mpv_log_tail.clear();
        }
    }

    fn resolve_official_artwork_in_background(
        self: &Arc<Self>,
        track: PlayableTrack,
        wanted_key: String,
        generation: u64,
    ) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let Some(res) = this
                .artwork_service
                .resolve_official_artwork(&track.title, &track.artist, &track.album)
                .await
            else {
                return;
            };
            if res.artwork_url.is_empty() {
                return;
            }
            let updated_snap = {
                let mut d = this.data.lock().expect("playback lock poisoned");
                if d.resolve_generation != generation || d.active_queue_key != wanted_key {
                    return;
                }
                let Some(idx) = d
                    .snapshot
                    .queue
                    .iter()
                    .position(|t| t.queue_key() == wanted_key)
                else {
                    return;
                };
                let mut updated = d.snapshot.queue[idx].clone();
                updated.artwork_url = res.artwork_url;
                if updated.album.is_empty() && !res.album_title.is_empty() {
                    updated.album = res.album_title;
                }
                d.snapshot.queue[idx] = updated.clone();
                if idx as i32 == d.snapshot.current_index {
                    d.snapshot.current = Some(updated);
                }
                d.snapshot.clone()
            };
            this.emit_snapshot(updated_snap);
            this.schedule_persist();
        });
    }

    // ---- Error & Completion Recovery --------------------------------------

    fn on_player_error<'a>(self: &'a Arc<Self>, recheck: bool) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let (user_paused, crossfade_handoff) = {
                let d = self.data.lock().expect("playback lock poisoned");
                (
                    d.user_paused,
                    d.crossfading && d.standby_ready && d.standby_track.is_some(),
                )
            };
            if user_paused {
                self.crumb("player error while user paused -> ignoring auto-resume");
                return;
            }
            if crossfade_handoff {
                self.crumb("automix outgoing error -> early handoff");
                self.complete_crossfade();
                return;
            }
            if !recheck && self.defer_if_just_opened("error") {
                return;
            }

            let action = {
                let mut d = self.data.lock().expect("playback lock poisoned");
                let Some(track) = d.snapshot.current.clone() else {
                    return;
                };
                let generation = d.resolve_generation;
                if d.resolving
                    || d.snapshot.error.is_some()
                    || d.active_queue_key != track.queue_key()
                    || d.failed_generation == Some(generation)
                {
                    return;
                }
                if !recheck && d.snapshot.is_playing {
                    Some((track, generation, true))
                } else {
                    d.failed_generation = Some(generation);
                    let pos = d.snapshot.position();
                    let dur = d.snapshot.duration();
                    if pos > Duration::from_secs(3)
                        && (dur.is_zero() || dur.saturating_sub(pos) > Duration::from_secs(5))
                    {
                        d.resume_key = track.queue_key();
                        d.resume_at = Some(pos);
                    }
                    Some((track, generation, false))
                }
            };
            let Some((track, generation, defer_moving)) = action else {
                return;
            };
            if defer_moving {
                let before = self.snapshot().position();
                let key = track.queue_key();
                let this = Arc::clone(self);
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    let valid = this.data.lock().is_ok_and(|d| {
                        d.resolve_generation == generation
                            && !d.resolving
                            && !d.user_paused
                            && d.snapshot.current.as_ref().map(|t| t.queue_key()).as_deref()
                                == Some(key.as_str())
                    });
                    if !valid {
                        return;
                    }
                    if this.snapshot().position() > before {
                        this.crumb(&format!(
                            "transient player error ignored (stream moving) key={key}"
                        ));
                        return;
                    }
                    this.crumb(&format!(
                        "stream stalled after player error -> proceeding with recovery key={key}"
                    ));
                    this.on_player_error(true).await;
                });
                return;
            }

            self.invalidate_play_cache(&track);
            let (cur_idx, stream_key, attempt) = {
                let d = self.data.lock().expect("playback lock poisoned");
                (
                    d.snapshot.current_index,
                    d.snapshot
                        .stream
                        .as_ref()
                        .map(|s| s.cache_key.clone())
                        .unwrap_or_default(),
                    d.playback_attempt,
                )
            };
            if cur_idx < 0 {
                return;
            }
            let idx = cur_idx as usize;
            if stream_key.starts_with("lossless:") || stream_key.starts_with("tidal:") {
                self.resolve_and_open(idx, true, false, 0).await;
                return;
            }
            if !track.video_id.is_empty() {
                self.tube.report_playback_failure(&track.video_id);
            }
            if stream_key.starts_with("addon:") && attempt == 0 {
                self.crumb(&format!("retry same-tier key={}", track.queue_key()));
                self.resolve_and_open(idx, false, true, 1).await;
                return;
            }
            if attempt <= 1 {
                self.crumb(&format!("retry youtube key={}", track.queue_key()));
                self.resolve_and_open(idx, true, true, 2).await;
                return;
            }
            if let Ok(mut d) = self.data.lock() {
                d.unavailable.insert(track.queue_key());
            }
            self.crumb(&format!(
                "unavailable after retries -> skip key={}",
                track.queue_key()
            ));
            self.skip_unavailable(idx).await;
        })
    }

    async fn skip_unavailable(self: &Arc<Self>, _failed_index: usize) {
        let next_idx = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let Some(ni) = Self::next_index_from(&d, true) else {
                d.snapshot.is_buffering = false;
                d.snapshot.is_playing = false;
                d.snapshot.error =
                    Some("Track unavailable — end of playable queue.".to_string());
                drop(d);
                self.emit_snapshot(self.snapshot());
                return;
            };
            d.snapshot.current_index = ni as i32;
            d.snapshot.current = Some(d.snapshot.queue[ni].clone());
            d.snapshot.is_buffering = true;
            d.snapshot.error = None;
            d.snapshot.position_ms = 0;
            d.snapshot.duration_ms = 0;
            ni
        };
        self.emit_snapshot(self.snapshot());
        self.resolve_and_open(next_idx, false, false, 0).await;
    }

    fn defer_if_just_opened(self: &Arc<Self>, what: &str) -> bool {
        let (opened_recently, key, gen) = {
            let d = self.data.lock().expect("playback lock poisoned");
            let recent = d
                .audio_open_done_at
                .is_some_and(|o| o.elapsed() < Duration::from_secs(3));
            (
                recent,
                d.snapshot
                    .current
                    .as_ref()
                    .map(|c| c.queue_key())
                    .unwrap_or_default(),
                d.resolve_generation,
            )
        };
        if !opened_recently {
            return false;
        }
        self.crumb(&format!("stale {what} ignored key={key}"));
        let this = Arc::clone(self);
        let what_owned = what.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(4)).await;
            let valid = this.data.lock().is_ok_and(|d| {
                d.resolve_generation == gen
                    && !d.resolving
                    && d.snapshot.current.as_ref().map(|t| t.queue_key()).as_deref()
                        == Some(key.as_str())
            });
            if !valid {
                return;
            }
            let before = this.snapshot().position();
            tokio::time::sleep(Duration::from_secs(2)).await;
            let still_stuck = this.data.lock().is_ok_and(|d| {
                d.resolve_generation == gen
                    && !d.resolving
                    && !d.user_paused
                    && d.snapshot.current.as_ref().map(|t| t.queue_key()).as_deref()
                        == Some(key.as_str())
                    && d.snapshot.position() <= before
            });
            if still_stuck {
                this.crumb(&format!(
                    "stale {what_owned} recheck: stream not moving key={key}"
                ));
                this.on_player_error(true).await;
            }
        });
        true
    }

    fn on_track_completed(self: &Arc<Self>) {
        let (user_paused, crossfading) = {
            let d = self.data.lock().expect("playback lock poisoned");
            (d.user_paused, d.crossfading)
        };
        if user_paused {
            self.crumb("track completed while user paused -> ignoring premature eof");
            return;
        }
        if crossfading {
            self.complete_crossfade();
            return;
        }
        self.cancel_crossfade();
        if self
            .data
            .lock()
            .is_ok_and(|d| d.resolving || d.snapshot.error.is_some())
        {
            return;
        }
        if self.defer_if_just_opened("eof") {
            return;
        }

        let af_resume = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let af_recent = d
                .primary_af_changed_at
                .is_some_and(|t| t.elapsed() < Duration::from_secs(3));
            let rem = d.snapshot.duration().saturating_sub(d.snapshot.position());
            if af_recent && rem > Duration::from_secs(3) {
                if let Some(track) = d.snapshot.current.clone() {
                    d.primary_af_changed_at = None;
                    d.resume_key = track.queue_key();
                    d.resume_at = Some(d.snapshot.position());
                    Some((d.snapshot.current_index, d.snapshot.position(), track.queue_key()))
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some((idx, pos, key)) = af_resume {
            self.crumb(&format!(
                "eof after filter change -> resume pos={}s key={key}",
                pos.as_secs()
            ));
            if idx >= 0 {
                let this = Arc::clone(self);
                tokio::spawn(async move {
                    this.resolve_and_open(idx as usize, false, false, 0).await;
                });
            }
            return;
        }

        self.flush_scrobble(true);
        let snap = self.snapshot();
        if snap.duration().saturating_sub(snap.position()) > Duration::from_secs(10) {
            self.crumb(&format!(
                "premature eof pos={}s dur={}s key={}",
                snap.position().as_secs(),
                snap.duration().as_secs(),
                snap.current.as_ref().map(|c| c.queue_key()).unwrap_or_default()
            ));
            let this = Arc::clone(self);
            tokio::spawn(async move {
                this.on_player_error(false).await;
            });
            return;
        }

        if snap.repeat_mode == RepeatMode::One {
            let loop_track = snap.current.clone();
            let this = Arc::clone(self);
            tokio::spawn(async move {
                this.seek(Duration::ZERO).await;
                this.play_resume().await;
                if let Some(t) = loop_track {
                    this.begin_scrobble_window(&t);
                }
            });
            return;
        }

        let next_idx = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let Some(ni) = Self::next_index_from(&d, false) else {
                d.snapshot.is_playing = false;
                drop(d);
                self.emit_snapshot(self.snapshot());
                self.schedule_persist();
                return;
            };
            d.snapshot.current_index = ni as i32;
            d.snapshot.current = Some(d.snapshot.queue[ni].clone());
            d.snapshot.is_buffering = true;
            d.snapshot.position_ms = 0;
            d.snapshot.duration_ms = 0;
            ni
        };
        self.emit_snapshot(self.snapshot());
        let this = Arc::clone(self);
        tokio::spawn(async move {
            this.resolve_and_open(next_idx, false, false, 0).await;
        });
    }

    fn next_index_from(d: &PlaybackData, skip_unavailable: bool) -> Option<usize> {
        let queue = &d.snapshot.queue;
        if queue.is_empty() || d.snapshot.current_index < 0 {
            return None;
        }
        let cur = d.snapshot.current_index as usize;
        if d.snapshot.shuffle_enabled && !d.shuffle_order.is_empty() {
            if let Some(pos) = d.shuffle_order.iter().position(|&x| x == cur) {
                if pos + 1 < d.shuffle_order.len() {
                    let cand = d.shuffle_order[pos + 1];
                    if cand >= queue.len() {
                        return Self::wrap_index_from(d, skip_unavailable);
                    }
                    if skip_unavailable && d.unavailable.contains(&queue[cand].queue_key()) {
                        for &idx in &d.shuffle_order[(pos + 1)..] {
                            if idx < queue.len() && !d.unavailable.contains(&queue[idx].queue_key()) {
                                return Some(idx);
                            }
                        }
                        return Self::wrap_index_from(d, skip_unavailable);
                    }
                    return Some(cand);
                }
            }
            return Self::wrap_index_from(d, skip_unavailable);
        }
        let next = cur + 1;
        if next < queue.len() {
            if skip_unavailable && d.unavailable.contains(&queue[next].queue_key()) {
                for (i, t) in queue.iter().enumerate().skip(next) {
                    if !d.unavailable.contains(&t.queue_key()) {
                        return Some(i);
                    }
                }
                return Self::wrap_index_from(d, skip_unavailable);
            }
            return Some(next);
        }
        Self::wrap_index_from(d, skip_unavailable)
    }

    fn wrap_index_from(d: &PlaybackData, skip_unavailable: bool) -> Option<usize> {
        if d.snapshot.repeat_mode != RepeatMode::All {
            return None;
        }
        let queue = &d.snapshot.queue;
        if d.snapshot.shuffle_enabled && !d.shuffle_order.is_empty() {
            for &idx in &d.shuffle_order {
                if idx < queue.len()
                    && (!skip_unavailable || !d.unavailable.contains(&queue[idx].queue_key()))
                {
                    return Some(idx);
                }
            }
            None
        } else {
            for (idx, t) in queue.iter().enumerate() {
                if !skip_unavailable || !d.unavailable.contains(&t.queue_key()) {
                    return Some(idx);
                }
            }
            None
        }
    }

    fn prev_index_from(d: &PlaybackData) -> Option<usize> {
        let cur = d.snapshot.current_index;
        if cur < 0 {
            return None;
        }
        if d.snapshot.shuffle_enabled && !d.shuffle_order.is_empty() {
            let cur_u = cur as usize;
            if let Some(pos) = d.shuffle_order.iter().position(|&x| x == cur_u) {
                if pos > 0 {
                    let prev = d.shuffle_order[pos - 1];
                    return (prev < d.snapshot.queue.len()).then_some(prev);
                }
            }
            return None;
        }
        if cur > 0 {
            Some((cur - 1) as usize)
        } else {
            None
        }
    }

    fn shuffle_slice<T>(items: &mut [T]) {
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        items.shuffle(&mut rng);
    }

    fn rebuild_shuffle_order_in(d: &mut PlaybackData, len: usize, current_index: i32) {
        let mut order: Vec<usize> = (0..len).collect();
        Self::shuffle_slice(&mut order);
        if current_index >= 0 && (current_index as usize) < len {
            let cur = current_index as usize;
            order.retain(|&x| x != cur);
            order.insert(0, cur);
        }
        d.shuffle_order = order;
    }

    // ---- Endless Radio ----------------------------------------------------

    fn maybe_refill_radio(self: &Arc<Self>) {
        let seed = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if !d.endless_radio {
                return;
            }
            let remaining = d.snapshot.queue.len() as i32 - d.snapshot.current_index - 1;
            if remaining > 6 {
                return;
            }
            let Some(seed) = d.snapshot.current.clone() else {
                return;
            };
            if !d.radio_seeds.insert(seed.media_id()) {
                return;
            }
            (seed, d.spotify_radio.clone())
        };
        let (seed, spotify_hook) = seed;
        let this = Arc::clone(self);
        tokio::spawn(async move {
            if this.prefs.recommendation_source() == "spotify" {
                if let Some(hook) = spotify_hook {
                    if let Some(picks) = hook(seed.clone()).await {
                        if !picks.is_empty() {
                            let cur_queue = this.snapshot().queue;
                            let to_add: Vec<PlayableTrack> = picks
                                .into_iter()
                                .filter(|t| {
                                    !Self::is_disallowed_radio_title(&t.title)
                                        && !Self::is_queued(t, &cur_queue)
                                })
                                .take(10)
                                .collect();
                            if !to_add.is_empty() {
                                this.append_radio(to_add);
                                return;
                            }
                        }
                    }
                }
            }
            let video_id = if !seed.video_id.is_empty() {
                Some(seed.video_id.clone())
            } else {
                this.tube
                    .find_best_match_or_null(&seed.title, &seed.artist, &HashSet::new())
                    .await
                    .map(|m| m.video_id)
            };
            let Some(vid) = video_id else {
                return;
            };
            let Ok(related) = this.tube.fetch_related_songs(&vid, 25).await else {
                return;
            };
            let (cur_queue, seeds) = {
                let d = this.data.lock().expect("playback lock poisoned");
                (d.snapshot.queue.clone(), d.radio_seeds.clone())
            };
            let fresh: Vec<PlayableTrack> = related
                .into_iter()
                .filter(|t| {
                    !Self::is_disallowed_radio_title(&t.title)
                        && !Self::is_radio_duplicate(t, &cur_queue)
                        && !seeds.contains(&t.video_id)
                })
                .take(10)
                .map(|t| PlayableTrack {
                    title: t.title,
                    artist: t.artist,
                    album: t.album,
                    artwork_url: t.artwork_url,
                    video_id: t.video_id,
                    ..Default::default()
                })
                .collect();
            this.append_radio(fresh);
        });
    }

    fn append_radio(&self, fresh: Vec<PlayableTrack>) {
        if fresh.is_empty() {
            return;
        }
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.snapshot.queue.extend(fresh);
            let len = d.snapshot.queue.len();
            let cur = d.snapshot.current_index;
            Self::rebuild_shuffle_order_in(&mut d, len, cur);
        }
        self.emit_snapshot(self.snapshot());
        self.schedule_persist();
    }

    pub fn is_queued(t: &PlayableTrack, queue: &[PlayableTrack]) -> bool {
        let primary = |a: &str| {
            primary_artist_split_regex()
                .split(a)
                .next()
                .unwrap_or("")
                .to_lowercase()
        };
        let title = InnerTubeMusicApi::base_title(&t.title);
        let artist = primary(&t.artist);
        queue.iter().any(|q| {
            primary(&q.artist) == artist && InnerTubeMusicApi::base_title(&q.title) == title
        })
    }

    pub fn is_disallowed_radio_title(title: &str) -> bool {
        let t = title.to_lowercase();
        t.contains("mashup")
            || t.contains("jukebox")
            || t.contains("megamix")
            || t.contains("nonstop")
            || t.contains("all songs")
            || t.contains("compilation")
    }

    pub fn is_radio_duplicate(t: &YouTubeMusicTrack, queue: &[PlayableTrack]) -> bool {
        let key = format!("{}|{}", t.title.to_lowercase(), t.artist.to_lowercase());
        let base = InnerTubeMusicApi::base_title(&t.title);
        for q in queue {
            if q.queue_key() == key {
                return true;
            }
            if InnerTubeMusicApi::similarity(&InnerTubeMusicApi::base_title(&q.title), &base) < 85
            {
                continue;
            }
            let qa = q.artist.to_lowercase();
            let ta = t.artist.to_lowercase();
            if !qa.is_empty()
                && !ta.is_empty()
                && (qa.contains(&ta)
                    || ta.contains(&qa)
                    || InnerTubeMusicApi::similarity(&qa, &ta) >= 50)
            {
                return true;
            }
        }
        false
    }

    // ---- Scrobbling & YT Watch History ------------------------------------

    fn on_playing_changed(&self, playing: bool) {
        let send_np = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.last_tick = if playing { Some(Instant::now()) } else { None };
            if playing && !d.now_playing_sent {
                d.snapshot.current.clone()
            } else {
                None
            }
        };
        if !playing {
            self.schedule_persist();
        } else if let Some(track) = send_np {
            self.send_now_playing(&track);
        }
    }

    fn begin_scrobble_window(self: &Arc<Self>, track: &PlayableTrack) {
        let known_dur_ms = {
            let d = self.data.lock().expect("playback lock poisoned");
            if !track.duration().is_zero() {
                track.duration().as_millis() as i64
            } else if d
                .snapshot
                .current
                .as_ref()
                .is_some_and(|c| c.queue_key() == track.queue_key())
            {
                d.snapshot.duration_ms as i64
            } else {
                0
            }
        };
        self.sessions.record_local_play(
            &track.title,
            &track.artist,
            &track.album,
            known_dur_ms,
        );
        self.flush_scrobble(false);
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.scrobble_track = Some(track.clone());
            d.scrobble_duration_sec = d.snapshot.duration().as_secs() as i64;
            d.scrobble_start_epoch = ms_epoch_now();
            d.accumulated_seconds = 0.0;
            d.scrobbled_this_window = false;
            d.yt_history_pushed_this_window = false;
            d.yt_history_cpn.clear();
            d.now_playing_sent = false;
            d.last_tick = Some(Instant::now());
        }
        self.send_now_playing(track);
        self.prewarm_history_id(track);
    }

    fn send_now_playing(&self, track: &PlayableTrack) {
        if let Ok(mut d) = self.data.lock() {
            d.now_playing_sent = true;
        }
        let scrobbler = Arc::clone(&self.scrobbler);
        let artist = track.artist.clone();
        let title = track.title.clone();
        let album = track.album.clone();
        tokio::spawn(async move {
            let _ = scrobbler.update_now_playing(&artist, &title, &album).await;
        });
    }

    fn maybe_push_yt_history(
        self: &Arc<Self>,
        track: &PlayableTrack,
        watched_seconds: i64,
        duration_seconds: i64,
    ) {
        if !self.prefs.sync_yt_history() {
            return;
        }
        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            if d.yt_history_pushed_this_window {
                return;
            }
            d.yt_history_pushed_this_window = true;
        }
        let this = Arc::clone(self);
        let track_owned = track.clone();
        tokio::spawn(async move {
            this.push_yt_history_async(track_owned, watched_seconds, duration_seconds)
                .await;
        });
    }

    async fn push_yt_history_async(
        self: &Arc<Self>,
        track: PlayableTrack,
        watched_seconds: i64,
        duration_seconds: i64,
    ) {
        let mut video_id = self.window_video_id(&track);
        if video_id.is_empty() {
            video_id = self.match_youtube_id_for_history(&track).await;
        }
        if video_id.is_empty() {
            if let Ok(mut d) = self.data.lock() {
                d.yt_history_pushed_this_window = false;
            }
            return;
        }
        let mut watch_url = self
            .data
            .lock()
            .ok()
            .and_then(|d| d.watchtime_urls.get(&video_id).cloned())
            .unwrap_or_default();
        if watch_url.is_empty() {
            watch_url = tokio::time::timeout(
                Duration::from_secs(30),
                self.tube.fetch_authenticated_playback_url(&video_id),
            )
            .await
            .unwrap_or_default();
            if !watch_url.is_empty() {
                self.stash_watchtime_url(&video_id, &watch_url);
            }
        }
        let cpn = self.history_cpn();
        let ok = self
            .tube
            .record_watch_history(
                &video_id,
                watched_seconds,
                duration_seconds,
                Some(&cpn),
                &watch_url,
            )
            .await;
        if !ok {
            if let Ok(mut d) = self.data.lock() {
                d.yt_history_pushed_this_window = false;
            }
        }
    }

    async fn match_youtube_id_for_history(self: &Arc<Self>, track: &PlayableTrack) -> String {
        let empty = HashSet::new();
        let mut matched = tokio::time::timeout(
            Duration::from_secs(12),
            self.tube
                .find_best_match_or_null(&track.title, &track.artist, &empty),
        )
        .await
        .ok()
        .flatten();
        if matched.is_none() {
            matched = tokio::time::timeout(
                Duration::from_secs(12),
                self.tube
                    .find_best_effort_match_or_null(&track.title, &track.artist, &empty),
            )
            .await
            .ok()
            .flatten();
        }
        if let Some(m) = matched {
            if !m.video_id.is_empty() {
                let id = m.video_id.clone();
                self.adopt_video_id(track, &id, &m);
                return id;
            }
        }
        String::new()
    }

    fn prewarm_history_id(self: &Arc<Self>, track: &PlayableTrack) {
        if !self.prefs.sync_yt_history() || !self.window_video_id(track).is_empty() {
            return;
        }
        let this = Arc::clone(self);
        let t = track.clone();
        tokio::spawn(async move {
            let _ = this.match_youtube_id_for_history(&t).await;
        });
    }

    fn history_cpn(&self) -> String {
        let mut d = self.data.lock().expect("playback lock poisoned");
        if d.yt_history_cpn.is_empty() {
            d.yt_history_cpn = InnerTubeMusicApi::new_watch_cpn();
        }
        d.yt_history_cpn.clone()
    }

    fn window_video_id(&self, track: &PlayableTrack) -> String {
        if !track.video_id.is_empty() {
            return track.video_id.clone();
        }
        let d = self.data.lock().expect("playback lock poisoned");
        for q in &d.snapshot.queue {
            if q.queue_key() == track.queue_key() && !q.video_id.is_empty() {
                return q.video_id.clone();
            }
        }
        String::new()
    }

    pub fn tick_scrobble(self: &Arc<Self>, _position: Duration) {
        let (yt_ping, scrobble_ping) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let now = Instant::now();
            if let Some(last) = d.last_tick {
                if d.snapshot.is_playing {
                    d.accumulated_seconds += now.duration_since(last).as_secs_f64();
                }
            }
            d.last_tick = Some(now);

            let yt_ping = if d.scrobble_start_epoch != 0
                && !d.yt_history_pushed_this_window
                && d.snapshot.is_playing
                && d.accumulated_seconds >= 20.0
            {
                d.scrobble_track.clone().map(|t| {
                    let dur_sec = if d.scrobble_duration_sec > 0 {
                        d.scrobble_duration_sec
                    } else {
                        d.snapshot.duration().as_secs() as i64
                    };
                    (t, d.accumulated_seconds.round() as i64, dur_sec)
                })
            } else {
                None
            };

            let mut scrobble_ping = None;
            if d.scrobble_start_epoch != 0 && !d.scrobbled_this_window && d.snapshot.is_playing {
                if let Some(t) = d.scrobble_track.clone() {
                    let duration_sec = if d.scrobble_duration_sec > 0 {
                        d.scrobble_duration_sec
                    } else {
                        d.snapshot.duration().as_secs() as i64
                    };
                    if duration_sec > 0 {
                        let percent = self.prefs.scrobble_percent().clamp(25, 90);
                        let threshold = ((duration_sec as f64 * percent as f64 / 100.0).round()
                            as i64)
                            .clamp(30, 240);
                        if d.accumulated_seconds >= threshold as f64 {
                            d.scrobbled_this_window = true;
                            scrobble_ping = Some((t, d.scrobble_start_epoch, threshold, duration_sec));
                        }
                    } else if d.accumulated_seconds >= 30.0 {
                        d.scrobbled_this_window = true;
                        scrobble_ping = Some((t, d.scrobble_start_epoch, 30, 0));
                    }
                }
            }
            (yt_ping, scrobble_ping)
        };

        if let Some((t, watched, dur)) = yt_ping {
            self.maybe_push_yt_history(&t, watched, dur);
        }
        if let Some((t, epoch, watched, dur)) = scrobble_ping {
            let scrobbler = Arc::clone(&self.scrobbler);
            let artist = t.artist.clone();
            let title = t.title.clone();
            let album = t.album.clone();
            tokio::spawn(async move {
                let _ = scrobbler.scrobble(&artist, &title, &album, epoch).await;
            });
            self.maybe_push_yt_history(&t, watched, dur);
        }
    }

    pub fn flush_scrobble(self: &Arc<Self>, completed: bool) {
        let (yt_ping, scrobble_ping) = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let Some(track) = d.scrobble_track.clone().or_else(|| d.snapshot.current.clone())
            else {
                return;
            };
            if d.scrobble_start_epoch == 0 {
                return;
            }
            if d.scrobbled_this_window {
                let watched = d.accumulated_seconds.round() as i64;
                let dur = d.scrobble_duration_sec;
                d.scrobble_start_epoch = 0;
                d.accumulated_seconds = 0.0;
                d.scrobble_track = None;
                d.scrobble_duration_sec = 0;
                d.scrobbled_this_window = false;
                (Some((track, watched, dur)), None)
            } else {
                let duration_sec = if d.scrobble_duration_sec > 0 {
                    d.scrobble_duration_sec
                } else {
                    d.snapshot.duration().as_secs() as i64
                };
                let percent = self.prefs.scrobble_percent().clamp(25, 90);
                let threshold = if duration_sec > 0 {
                    ((duration_sec as f64 * percent as f64 / 100.0).round() as i64).clamp(30, 240)
                } else {
                    30
                };
                let played_enough = d.accumulated_seconds >= threshold as f64
                    || (completed && d.accumulated_seconds >= 30.0);
                let res = if played_enough {
                    Some((
                        track,
                        d.scrobble_start_epoch,
                        d.accumulated_seconds.round() as i64,
                        duration_sec,
                    ))
                } else {
                    None
                };
                d.scrobble_start_epoch = 0;
                d.accumulated_seconds = 0.0;
                d.scrobble_track = None;
                d.scrobble_duration_sec = 0;
                d.scrobbled_this_window = false;
                (None, res)
            }
        };

        if let Some((t, watched, dur)) = yt_ping {
            self.maybe_push_yt_history(&t, watched, dur);
        }
        if let Some((track, epoch, watched, dur)) = scrobble_ping {
            let scrobbler = Arc::clone(&self.scrobbler);
            let artist = track.artist.clone();
            let title = track.title.clone();
            let album = track.album.clone();
            tokio::spawn(async move {
                let _ = scrobbler.scrobble(&artist, &title, &album, epoch).await;
            });
            self.maybe_push_yt_history(&track, watched, dur);
        }
    }

    // ---- Session Persistence ----------------------------------------------

    fn schedule_persist(&self) {
        let should_throttle = {
            let mut d = self.data.lock().expect("playback lock poisoned");
            let now = Instant::now();
            if d.last_persist
                .is_some_and(|t| now.duration_since(t) < Duration::from_secs(2))
            {
                true
            } else {
                d.last_persist = Some(now);
                false
            }
        };
        if !should_throttle {
            self.persist_now();
            return;
        }
        let weak = self.data.lock().ok().and_then(|d| {
            let _ = &d.snapshot;
            Some(())
        });
        let _ = weak;
        // Best-effort deferred flush is skipped if called outside an active service handle,
        // otherwise persist_now runs immediately on the next non-throttled mutation.
    }

    pub fn persist_now(&self) {
        let payload = {
            let d = self.data.lock().expect("playback lock poisoned");
            if d.snapshot.queue.is_empty() || d.snapshot.current_index < 0 {
                return;
            }
            let len = d.snapshot.queue.len();
            let cur = d.snapshot.current_index as usize;
            let start = cur.saturating_sub(50).min(len);
            let end = (start + 200).min(len);
            let slice: Vec<Value> = d.snapshot.queue[start..end]
                .iter()
                .filter_map(|t| serde_json::to_value(t).ok())
                .collect();
            let mut map = Map::new();
            map.insert("version".into(), Value::from(2));
            map.insert("queue".into(), Value::Array(slice));
            map.insert("currentIndex".into(), Value::from((cur - start) as i64));
            map.insert(
                "positionMs".into(),
                Value::from(d.snapshot.position_ms as i64),
            );
            map.insert(
                "sourceLabel".into(),
                Value::from(d.snapshot.source_label.as_str()),
            );
            map.insert("endless".into(), Value::from(d.endless_radio));
            map.insert("shuffle".into(), Value::from(d.snapshot.shuffle_enabled));
            let repeat_idx = match d.snapshot.repeat_mode {
                RepeatMode::Off => 0,
                RepeatMode::All => 1,
                RepeatMode::One => 2,
            };
            map.insert("repeat".into(), Value::from(repeat_idx));
            map.insert("speed".into(), Value::from(d.snapshot.speed));
            map.insert("pitch".into(), Value::from(d.snapshot.pitch));
            map
        };
        self.sessions.save(&payload);
    }

    pub async fn restore_session(&self) {
        let data = self.sessions.load();
        let Some(queue_arr) = data.get("queue").and_then(|v| v.as_array()) else {
            return;
        };
        if queue_arr.is_empty() {
            return;
        }
        let queue: Vec<PlayableTrack> = queue_arr
            .iter()
            .filter_map(|v| serde_json::from_value::<PlayableTrack>(v.clone()).ok())
            .filter(|t| !t.title.is_empty())
            .collect();
        if queue.is_empty() {
            return;
        }
        let index = (data
            .get("currentIndex")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .max(0) as usize)
            .min(queue.len() - 1);
        let endless = data.get("endless").and_then(|v| v.as_bool()) == Some(true);
        let shuffle = data.get("shuffle").and_then(|v| v.as_bool()) == Some(true);
        let repeat_idx = data.get("repeat").and_then(|v| v.as_i64()).unwrap_or(0);
        let repeat_mode = match repeat_idx {
            1 => RepeatMode::All,
            2 => RepeatMode::One,
            _ => RepeatMode::Off,
        };
        let speed = data
            .get("speed")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .clamp(0.5, 2.0);
        let pitch = data
            .get("pitch")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .clamp(0.5, 1.5);
        let pos_ms = data
            .get("positionMs")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .clamp(0, i32::MAX as i64) as u64;
        let source_label = data
            .get("sourceLabel")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        {
            let mut d = self.data.lock().expect("playback lock poisoned");
            d.endless_radio = endless;
            let len = queue.len();
            Self::rebuild_shuffle_order_in(&mut d, len, index as i32);
            d.snapshot.queue = queue.clone();
            d.snapshot.current_index = index as i32;
            d.snapshot.current = Some(queue[index].clone());
            d.snapshot.source_label = source_label;
            d.snapshot.shuffle_enabled = shuffle;
            d.snapshot.repeat_mode = repeat_mode;
            d.snapshot.speed = speed;
            d.snapshot.pitch = pitch;
            d.snapshot.position_ms = pos_ms;
        }
        self.emit_snapshot(self.snapshot());
    }
}

// ---- OutputController & MPRIS Trait Implementations -----------------------

impl PlaybackOutputSink for PlaybackService {
    fn processing_blocks_exclusive(&self) -> bool {
        PlaybackService::processing_blocks_exclusive(self)
    }

    fn exclusive_applied(&self) -> bool {
        self.data
            .lock()
            .map(|d| d.exclusive_applied)
            .unwrap_or(false)
    }

    fn wasapi_error(&self) -> Option<String> {
        self.data
            .lock()
            .ok()
            .and_then(|d| d.wasapi_error.clone())
    }

    fn snapshot(&self) -> PlayerSnapshot {
        PlaybackService::snapshot(self)
    }

    fn configure_wasapi<'a>(
        &'a self,
        _exclusive: bool,
        _mpv_device: &'a str,
        _lock_software_volume: bool,
        _output_format: Option<PcmFormat>,
    ) -> crate::audio::wasapi_engine::BoxFuture<'a, ()> {
        // Callers holding Arc<PlaybackService> can invoke PlaybackService::configure_wasapi directly;
        // see wrapper impl below for Arc<PlaybackService>.
        Box::pin(async {})
    }

    fn set_volume<'a>(
        &'a self,
        volume: f64,
        software: bool,
    ) -> crate::audio::wasapi_engine::BoxFuture<'a, ()> {
        Box::pin(async move {
            PlaybackService::set_volume(self, volume, software).await;
        })
    }

    fn unlock_software_volume(&self) -> crate::audio::wasapi_engine::BoxFuture<'_, ()> {
        Box::pin(async move {
            PlaybackService::unlock_software_volume(self).await;
        })
    }

    fn pause(&self) -> crate::audio::wasapi_engine::BoxFuture<'_, ()> {
        Box::pin(async move {
            PlaybackService::pause(self).await;
        })
    }
}

/// Wrapper providing full `Arc<PlaybackService>` capability (including `ensure_player`
/// inside `configure_wasapi`) as a `PlaybackOutputSink` for [`AudioOutputController`].
pub struct PlaybackServiceOutputSink {
    pub service: Arc<PlaybackService>,
}

impl PlaybackServiceOutputSink {
    pub fn new(service: Arc<PlaybackService>) -> Self {
        Self { service }
    }
}

impl PlaybackOutputSink for PlaybackServiceOutputSink {
    fn processing_blocks_exclusive(&self) -> bool {
        self.service.processing_blocks_exclusive()
    }

    fn exclusive_applied(&self) -> bool {
        self.service
            .data
            .lock()
            .map(|d| d.exclusive_applied)
            .unwrap_or(false)
    }

    fn wasapi_error(&self) -> Option<String> {
        self.service
            .data
            .lock()
            .ok()
            .and_then(|d| d.wasapi_error.clone())
    }

    fn snapshot(&self) -> PlayerSnapshot {
        self.service.snapshot()
    }

    fn configure_wasapi<'a>(
        &'a self,
        exclusive: bool,
        mpv_device: &'a str,
        lock_software_volume: bool,
        output_format: Option<PcmFormat>,
    ) -> crate::audio::wasapi_engine::BoxFuture<'a, ()> {
        Box::pin(async move {
            self.service
                .configure_wasapi(exclusive, mpv_device, lock_software_volume, output_format)
                .await;
        })
    }

    fn set_volume<'a>(
        &'a self,
        volume: f64,
        software: bool,
    ) -> crate::audio::wasapi_engine::BoxFuture<'a, ()> {
        Box::pin(async move {
            self.service.set_volume(volume, software).await;
        })
    }

    fn unlock_software_volume(&self) -> crate::audio::wasapi_engine::BoxFuture<'_, ()> {
        Box::pin(async move {
            self.service.unlock_software_volume().await;
        })
    }

    fn pause(&self) -> crate::audio::wasapi_engine::BoxFuture<'_, ()> {
        Box::pin(async move {
            self.service.pause().await;
        })
    }
}

/// Wrapper connecting [`MprisService`] to an `Arc<PlaybackService>`.
pub struct PlaybackServiceMprisHandler {
    pub service: Arc<PlaybackService>,
    pub on_raise: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl MprisControlHandler for PlaybackServiceMprisHandler {
    fn handle(&self, action: MprisAction) {
        if matches!(action, MprisAction::Raise) {
            if let Some(ref cb) = self.on_raise {
                cb();
            }
            return;
        }
        let svc = Arc::clone(&self.service);
        tokio::spawn(async move {
            match action {
                MprisAction::Next => svc.next().await,
                MprisAction::Previous => svc.previous().await,
                MprisAction::Pause | MprisAction::Stop => svc.pause().await,
                MprisAction::PlayPause => svc.toggle().await,
                MprisAction::Play => svc.play_resume().await,
                MprisAction::SeekTo(pos) => svc.seek(pos).await,
                MprisAction::SetVolume(vol) => svc.set_volume(vol, true).await,
                MprisAction::ToggleShuffle => svc.toggle_shuffle().await,
                MprisAction::CycleRepeat => svc.cycle_repeat().await,
                MprisAction::SetSpeed(rate) => svc.set_speed(rate).await,
                MprisAction::Raise => {}
            }
        });
    }

    fn live_repeat_mode(&self) -> RepeatMode {
        self.service.snapshot().repeat_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::lastfm::lastfm_api::LastFmApiService;
    use crate::integrations::lastfm::rate_guard::LastFmRateGuard;

    #[derive(Default)]
    struct FakeDeckState {
        props: HashMap<String, String>,
        commands: Vec<Vec<String>>,
        opened_urls: Vec<String>,
        stopped_count: usize,
        disposed: bool,
    }

    struct FakeMpvDeck {
        id: u64,
        state: Mutex<DeckState>,
        inner: Mutex<FakeDeckState>,
        tx: mpsc::UnboundedSender<DeckEvent>,
        rx: Mutex<Option<mpsc::UnboundedReceiver<DeckEvent>>>,
    }

    impl FakeMpvDeck {
        fn new(initial_af: String) -> Arc<Self> {
            let (tx, rx) = mpsc::unbounded_channel();
            let mut props = HashMap::new();
            if !initial_af.is_empty() {
                props.insert("af".into(), initial_af);
            }
            Arc::new(Self {
                id: NEXT_DECK_ID.fetch_add(1, Ordering::Relaxed),
                state: Mutex::new(DeckState {
                    volume: 100.0,
                    rate: 1.0,
                    ..Default::default()
                }),
                inner: Mutex::new(FakeDeckState {
                    props,
                    ..Default::default()
                }),
                tx,
                rx: Mutex::new(Some(rx)),
            })
        }

        fn emit(&self, ev: DeckEvent) {
            let _ = self.tx.send(ev);
        }
    }

    impl MpvDeck for FakeMpvDeck {
        fn id(&self) -> u64 {
            self.id
        }
        fn state(&self) -> DeckState {
            *self.state.lock().unwrap()
        }
        fn set_property(&self, name: &str, value: &str) -> Result<(), String> {
            self.inner
                .lock()
                .unwrap()
                .props
                .insert(name.into(), value.into());
            Ok(())
        }
        fn get_property(&self, name: &str) -> Option<String> {
            self.inner.lock().unwrap().props.get(name).cloned()
        }
        fn command(&self, args: &[&str]) -> Result<(), String> {
            self.inner
                .lock()
                .unwrap()
                .commands
                .push(args.iter().map(|s| s.to_string()).collect());
            Ok(())
        }
        fn open(
            &self,
            url: &str,
            _headers: &HashMap<String, String>,
            _start: Option<Duration>,
            play: bool,
        ) -> Result<(), String> {
            self.inner.lock().unwrap().opened_urls.push(url.into());
            {
                let mut st = self.state.lock().unwrap();
                st.playing = play;
                st.duration = Duration::from_secs(180);
            }
            self.emit(DeckEvent::Playing(play));
            self.emit(DeckEvent::DurationChanged(Duration::from_secs(180)));
            Ok(())
        }
        fn play(&self) -> Result<(), String> {
            self.state.lock().unwrap().playing = true;
            self.emit(DeckEvent::Playing(true));
            Ok(())
        }
        fn pause(&self) -> Result<(), String> {
            self.state.lock().unwrap().playing = false;
            self.emit(DeckEvent::Playing(false));
            Ok(())
        }
        fn play_or_pause(&self) -> Result<(), String> {
            let now = {
                let mut st = self.state.lock().unwrap();
                st.playing = !st.playing;
                st.playing
            };
            self.emit(DeckEvent::Playing(now));
            Ok(())
        }
        fn stop(&self) -> Result<(), String> {
            self.inner.lock().unwrap().stopped_count += 1;
            self.state.lock().unwrap().playing = false;
            Ok(())
        }
        fn seek(&self, position: Duration) -> Result<(), String> {
            self.state.lock().unwrap().position = position;
            Ok(())
        }
        fn set_volume(&self, volume: f64) -> Result<(), String> {
            self.state.lock().unwrap().volume = volume.clamp(0.0, 100.0);
            Ok(())
        }
        fn set_rate(&self, rate: f64) -> Result<(), String> {
            self.state.lock().unwrap().rate = rate;
            Ok(())
        }
        fn set_audio_device(&self, device: &str) -> Result<(), String> {
            self.set_property("audio-device", device)
        }
        fn subscribe_events(&self) -> Option<mpsc::UnboundedReceiver<DeckEvent>> {
            self.rx.lock().unwrap().take()
        }
        fn dispose(&self) {
            self.inner.lock().unwrap().disposed = true;
        }
    }

    struct FakeFactory {
        decks: Mutex<Vec<Arc<FakeMpvDeck>>>,
    }

    impl FakeFactory {
        fn new() -> Self {
            Self {
                decks: Mutex::new(Vec::new()),
            }
        }
    }

    impl MpvDeckFactory for FakeFactory {
        fn create(&self, config: DeckConfig) -> Result<Arc<dyn MpvDeck>, String> {
            let d = FakeMpvDeck::new(config.initial_af);
            self.decks.lock().unwrap().push(Arc::clone(&d));
            Ok(d)
        }
    }

    fn test_service() -> (Arc<PlaybackService>, Arc<FakeFactory>, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "lw_playback_test_{}_{}",
            std::process::id(),
            NEXT_DECK_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::create_dir_all(&dir);
        let db = Arc::new(AppDatabase::open(&dir));
        let prefs = Arc::new(Prefs::load(&dir));
        let tube = Arc::new(InnerTubeMusicApi::with_defaults().expect("tube"));
        let lastfm_api = Arc::new(LastFmApiService::new(Arc::new(LastFmRateGuard::new())));
        let scrobbler = Arc::new(ScrobbleRepository::new(lastfm_api, Arc::clone(&prefs), None));
        let downloads = Arc::new(DownloadManager::new(
            Arc::clone(&db),
            Arc::clone(&prefs),
            dir.clone(),
        ));
        let artwork = Arc::new(OfficialArtworkService::new(None, Some(Arc::clone(&db))));
        let factory = Arc::new(FakeFactory::new());
        let svc = PlaybackService::with_deck_factory(
            tube,
            Arc::new(NoopLosslessBackend),
            scrobbler,
            downloads,
            PrefsHandle::new(prefs),
            SessionStore::new(db),
            artwork,
            None,
            factory.clone(),
        );
        (svc, factory, dir)
    }

    #[tokio::test]
    async fn plays_local_queue_persists_and_restores_session() {
        let (svc, factory, dir) = test_service();
        let file1 = dir.join("song1.flac");
        let file2 = dir.join("song2.mp3");
        std::fs::write(&file1, b"fLaCmock").unwrap();
        std::fs::write(&file2, b"ID3mock").unwrap();

        let mut t1 = PlayableTrack::new("Track One", "Artist A");
        t1.playback_url = file1.to_string_lossy().into_owned();
        t1.album = "Album 1".into();
        let mut t2 = PlayableTrack::new("Track Two", "Artist B");
        t2.playback_url = file2.to_string_lossy().into_owned();
        t2.album = "Album 2".into();

        svc.play_queue(vec![t1, t2], 0, "Local Test", false, false)
            .await;

        let snap = svc.snapshot();
        assert_eq!(snap.current_index, 0);
        assert_eq!(snap.queue.len(), 2);
        assert_eq!(
            snap.stream.as_ref().map(|s| s.is_lossless),
            Some(true),
            "expected local flac stream"
        );
        assert!(!factory.decks.lock().unwrap().is_empty());

        svc.set_speed(1.25).await;
        svc.seek(Duration::from_secs(42)).await;
        svc.persist_now();

        let (svc2, _, _) = {
            let db2 = Arc::new(AppDatabase::open(&dir));
            let prefs2 = Arc::new(Prefs::load(&dir));
            let tube2 = Arc::new(InnerTubeMusicApi::with_defaults().expect("tube"));
            let lastfm_api2 = Arc::new(LastFmApiService::new(Arc::new(LastFmRateGuard::new())));
            let scrobbler2 =
                Arc::new(ScrobbleRepository::new(lastfm_api2, Arc::clone(&prefs2), None));
            let downloads2 = Arc::new(DownloadManager::new(
                Arc::clone(&db2),
                Arc::clone(&prefs2),
                dir.clone(),
            ));
            let artwork2 = Arc::new(OfficialArtworkService::new(None, Some(Arc::clone(&db2))));
            let factory2 = Arc::new(FakeFactory::new());
            (
                PlaybackService::with_deck_factory(
                    tube2,
                    Arc::new(NoopLosslessBackend),
                    scrobbler2,
                    downloads2,
                    PrefsHandle::new(prefs2),
                    SessionStore::new(db2),
                    artwork2,
                    None,
                    factory2.clone(),
                ),
                factory2,
                dir.clone(),
            )
        };
        svc2.restore_session().await;
        let restored = svc2.snapshot();
        assert_eq!(restored.queue.len(), 2);
        assert_eq!(restored.source_label, "Local Test");
        assert!((restored.speed - 1.25).abs() < 1e-6);
        assert_eq!(restored.position_ms, 42_000);

        svc.dispose_player().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn filter_graph_degradation_and_exclusive_processing_checks() {
        let (svc, _factory, dir) = test_service();
        svc.prefs.raw_prefs().set_auto_mix_enabled(true);
        svc.prefs.raw_prefs().set_auto_mix_spectral(true);
        svc.prefs.raw_prefs().set_equalizer_enabled(true);
        assert!(svc.processing_blocks_exclusive());

        svc.set_audio_filter("format=floatp,lavfi=[crystalizer=i=1.5,equalizer=f=100:g=3]".into())
            .await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        {
            let d = svc.data.lock().unwrap();
            assert!(d.current_audio_filter.contains("crystalizer"));
            assert!(d.current_audio_filter.contains("@lwdj"));
        }

        // First rejection drops crystalizer
        svc.on_filter_graph_error("avfiltergraph: crystalizer missing");
        {
            let d = svc.data.lock().unwrap();
            assert!(d.eq_extras_dropped);
            assert!(!d.processing_disabled);
        }
        // Second rejection disables user chain while keeping @lwdj
        svc.on_filter_graph_error("avfiltergraph: equalizer failed");
        {
            let d = svc.data.lock().unwrap();
            assert!(d.processing_disabled);
            assert!(d.current_audio_filter.contains("@lwdj"));
        }
        // Third rejection disables @lwdj
        svc.on_filter_graph_error("avfiltergraph: lwdj failed");
        {
            let d = svc.data.lock().unwrap();
            assert!(d.dj_eq_broken);
            assert!(d.current_audio_filter.is_empty());
        }

        svc.dispose_player().await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn queue_reorder_remove_radio_dedupe_and_shuffle_repeat() {
        let (svc, _, dir) = test_service();
        let tracks = vec![
            PlayableTrack::new("Alpha", "Artist 1"),
            PlayableTrack::new("Beta", "Artist 2"),
            PlayableTrack::new("Gamma", "Artist 3"),
        ];
        {
            let mut d = svc.data.lock().unwrap();
            d.snapshot.queue = tracks.clone();
            d.snapshot.current_index = 0;
            d.snapshot.current = Some(tracks[0].clone());
        }

        // Move Beta (1) after Gamma (3) -> [Alpha, Gamma, Beta]
        svc.move_in_queue(1, 3).await;
        let q = svc.snapshot().queue;
        assert_eq!(q[1].title, "Gamma");
        assert_eq!(q[2].title, "Beta");

        svc.cycle_repeat().await;
        assert_eq!(svc.snapshot().repeat_mode, RepeatMode::All);
        svc.cycle_repeat().await;
        assert_eq!(svc.snapshot().repeat_mode, RepeatMode::One);

        assert!(PlaybackService::is_disallowed_radio_title("Best 90s Megamix"));
        assert!(!PlaybackService::is_disallowed_radio_title("Normal Song"));
        let candidate = YouTubeMusicTrack {
            title: "Alpha (Remastered)".into(),
            artist: "Artist 1".into(),
            ..Default::default()
        };
        assert!(PlaybackService::is_radio_duplicate(&candidate, &q));

        svc.dispose_player().await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
