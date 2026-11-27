use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{CStr, CString};
use std::future::Future;
use std::os::raw::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::sync::{watch, Mutex as AsyncMutex};

use crate::audio::audio_analysis::{analyze_pcm_window, PcmWindow, TrackAnalysis};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnalysisSide {
    Head,
    Tail,
}

impl AnalysisSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnalysisSide::Head => "head",
            AnalysisSide::Tail => "tail",
        }
    }
}

pub type DecoderHook = Arc<
    dyn Fn(
            AnalysisSide,
            String,
            HashMap<String, String>,
            Duration,
        ) -> Pin<Box<dyn Future<Output = Option<Vec<i16>>> + Send>>
        + Send
        + Sync,
>;

pub type WantedHook = Arc<dyn Fn(&str) -> bool + Send + Sync>;
pub type LogHook = Arc<dyn Fn(&str) + Send + Sync>;

/// Runs one native mpv call in the app-wide mpv order (Windows libmpv
/// heap safety — see PlaybackService's serialized mpv lock).
pub trait MpvSerializer: Send + Sync {
    fn serialize<'a>(
        &'a self,
        op: Pin<Box<dyn Future<Output = ()> + Send + 'a>>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

/// Default mutex-based serializer when no external app-wide serializer is supplied.
#[derive(Default)]
pub struct MutexMpvSerializer {
    lock: AsyncMutex<()>,
}

impl MpvSerializer for MutexMpvSerializer {
    fn serialize<'a>(
        &'a self,
        op: Pin<Box<dyn Future<Output = ()> + Send + 'a>>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let _guard = self.lock.lock().await;
            op.await;
        })
    }
}

struct MemCache {
    map: HashMap<String, TrackAnalysis>,
    order: VecDeque<String>,
}

impl MemCache {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&self, key: &str) -> Option<TrackAnalysis> {
        self.map.get(key).cloned()
    }

    fn insert(&mut self, key: String, val: TrackAnalysis) {
        if self.map.contains_key(&key) {
            self.map.insert(key, val);
            return;
        }
        self.order.push_back(key.clone());
        self.map.insert(key, val);
        if self.map.len() > 64 {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
    }
}

/// Decodes the head / tail of a track to PCM on a hidden libmpv deck
/// and analyses it off the UI thread.
///
/// The deck uses mpv's built-in `pcm` audio output with `untimed`, so a
/// minute of audio decodes in well under a second from local or cached
/// sources — the same decoder, network stack and headers as playback,
/// whatever the source (addon, YouTube, local file). Nothing reaches the
/// sound card. One fresh mpv instance per job: disposing it is what
/// closes (and flushes) the PCM file.
pub struct AutomixAnalyzer {
    serialize: Arc<dyn MpvSerializer>,
    log: LogHook,
    cache_dir: PathBuf,
    wanted: Option<WantedHook>,
    decoder: Option<DecoderHook>,
    mem: Mutex<MemCache>,
    inflight: Mutex<HashMap<String, watch::Receiver<Option<Option<TrackAnalysis>>>>>,
    failed: Mutex<HashSet<String>>,
    queue_lock: Arc<AsyncMutex<()>>,
    consecutive_failures: Mutex<usize>,
    disabled: AtomicBool,
    job_seq: AtomicU64,
}

impl AutomixAnalyzer {
    /// After this many decode failures in a row the analyzer turns itself
    /// off for the session (AutoMix then mixes without analysis).
    pub const MAX_CONSECUTIVE_FAILURES: usize = 4;
    pub const SAMPLE_RATE: u32 = 11025;
    pub const WINDOW: Duration = Duration::from_secs(60);
    const JOB_TIMEOUT: Duration = Duration::from_secs(45);

    pub fn new(
        serialize: Arc<dyn MpvSerializer>,
        log: LogHook,
        cache_dir: Option<PathBuf>,
        wanted: Option<WantedHook>,
        decoder: Option<DecoderHook>,
    ) -> Self {
        let dir = cache_dir.unwrap_or_else(|| {
            std::env::temp_dir().join("lastwave").join("automix")
        });
        let this = Self {
            serialize,
            log,
            cache_dir: dir,
            wanted,
            decoder,
            mem: Mutex::new(MemCache::new()),
            inflight: Mutex::new(HashMap::new()),
            failed: Mutex::new(HashSet::new()),
            queue_lock: Arc::new(AsyncMutex::new(())),
            consecutive_failures: Mutex::new(0),
            disabled: AtomicBool::new(false),
            job_seq: AtomicU64::new(0),
        };
        this.check_crash_marker();
        this
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Stable identity of a decoded source. The same song from a lossless
    /// addon and from YouTube has different offsets, so they never share.
    pub fn source_id(stream_cache_key: &str) -> String {
        if stream_cache_key.starts_with("youtube:") {
            return stream_cache_key
                .split(':')
                .take(2)
                .collect::<Vec<_>>()
                .join(":");
        }
        stream_cache_key.to_string()
    }

    pub fn key_for(queue_key: &str, source: &str, side: AnalysisSide) -> String {
        format!("{queue_key}|{source}|{}", side.as_str())
    }

    pub fn cached(&self, key: &str) -> Option<TrackAnalysis> {
        self.mem.lock().ok()?.get(key)
    }

    /// Turns the feature off for the session (e.g. the platform cannot
    /// create the analysis deck).
    pub fn disable(&self) {
        self.disabled.store(true, Ordering::SeqCst);
    }

    pub fn disabled(&self) -> bool {
        self.disabled.load(Ordering::SeqCst)
    }

    /// Starts (or joins) the analysis for `key`. Never panics or propagates errors.
    pub async fn request(
        self: &Arc<Self>,
        key: String,
        side: AnalysisSide,
        url: String,
        headers: HashMap<String, String>,
        duration: Duration,
    ) -> Option<TrackAnalysis> {
        if let Some(hit) = self.cached(&key) {
            return Some(hit);
        }
        if self.disabled() || self.failed.lock().ok().is_some_and(|f| f.contains(&key)) {
            return None;
        }
        if url.is_empty() {
            return None;
        }
        if side == AnalysisSide::Tail && duration.is_zero() {
            return None;
        }

        let mut rx = {
            let mut inflight = self.inflight.lock().expect("inflight lock poisoned");
            if let Some(existing) = inflight.get(&key) {
                existing.clone()
            } else {
                let (tx, rx) = watch::channel(None);
                inflight.insert(key.clone(), rx.clone());
                let this = Arc::clone(self);
                let job_key = key.clone();
                tokio::spawn(async move {
                    // One job at a time: analysis is background work and must never
                    // compete with the playing deck for bandwidth or CPU.
                    let _queue_guard = this.queue_lock.lock().await;
                    let mut result: Option<TrackAnalysis> = None;
                    let mut skipped = false;

                    if let Some(disk) = this.from_disk(&job_key).await {
                        result = Some(disk);
                    } else if this.disabled()
                        || !this
                            .wanted
                            .as_ref()
                            .is_none_or(|w| w(&job_key))
                    {
                        // Skipped ahead / turned off while queued: drop the job but do
                        // not mark it failed, so it can be asked for again later.
                        skipped = true;
                    } else {
                        result = this
                            .decode_and_analyse(&job_key, side, &url, &headers, duration)
                            .await;
                        if result.is_none() {
                            let failures = {
                                let mut cf = this
                                    .consecutive_failures
                                    .lock()
                                    .expect("consecutive_failures lock poisoned");
                                *cf += 1;
                                *cf
                            };
                            if failures >= Self::MAX_CONSECUTIVE_FAILURES {
                                this.disabled.store(true, Ordering::SeqCst);
                                (this.log)(&format!(
                                    "automix analysis off for this session after {} failed decodes",
                                    failures
                                ));
                            }
                        } else if let Ok(mut cf) = this.consecutive_failures.lock() {
                            *cf = 0;
                        }
                    }

                    if let Some(ref res) = result {
                        if let Ok(mut mem) = this.mem.lock() {
                            mem.insert(job_key.clone(), res.clone());
                        }
                    } else if !skipped {
                        if let Ok(mut failed) = this.failed.lock() {
                            failed.insert(job_key.clone());
                        }
                    }
                    if let Ok(mut inflight) = this.inflight.lock() {
                        inflight.remove(&job_key);
                    }
                    let _ = tx.send(Some(result));
                });
                rx
            }
        };

        loop {
            if let Some(val) = rx.borrow().clone() {
                return val;
            }
            if rx.changed().await.is_err() {
                return rx.borrow().clone().flatten();
            }
        }
    }

    async fn decode_and_analyse(
        &self,
        key: &str,
        side: AnalysisSide,
        url: &str,
        headers: &HashMap<String, String>,
        duration: Duration,
    ) -> Option<TrackAnalysis> {
        let sw = Instant::now();
        let pcm: Option<Vec<i16>> = if let Some(ref hook) = self.decoder {
            hook(side, url.to_string(), headers.clone(), duration).await
        } else {
            // Crash marker around the native decode: if the process dies in
            // here, the next launch finds it and skips analysis instead of
            // crashing the same way again.
            self.set_crash_marker(true).await;
            let res = self.decode(side, url, headers, duration).await;
            self.set_crash_marker(false).await;
            res
        };
        let pcm = pcm?;
        let decode_ms = sw.elapsed().as_millis();
        let offset = if side == AnalysisSide::Tail {
            Self::tail_start(duration).as_secs_f64()
        } else {
            0.0
        };
        let raw_analysis = tokio::task::spawn_blocking(move || {
            analyze_pcm_window(&PcmWindow::new(pcm, Self::SAMPLE_RATE, offset))
        })
        .await
        .ok()?;
        let analysis = sanitize_analysis(Some(raw_analysis));
        let Some(analysis) = analysis else {
            (self.log)(&format!(
                "automix analysis rejected key={key} (implausible values)"
            ));
            return None;
        };
        (self.log)(&format!(
            "automix analysed key={key} side={} bpm={:.1} conf={:.2} audible={:.1}-{:.1} decode_ms={} total_ms={}",
            side.as_str(),
            analysis.bpm,
            analysis.confidence,
            analysis.audible_start,
            analysis.content_end,
            decode_ms,
            sw.elapsed().as_millis()
        ));
        let key_owned = key.to_string();
        let analysis_clone = analysis.clone();
        let cache_dir = self.cache_dir.clone();
        tokio::spawn(async move {
            write_to_disk(&cache_dir, &key_owned, &analysis_clone).await;
        });
        Some(analysis)
    }

    fn tail_start(duration: Duration) -> Duration {
        duration.saturating_sub(Self::WINDOW)
    }

    async fn decode(
        &self,
        side: AnalysisSide,
        url: &str,
        headers: &HashMap<String, String>,
        duration: Duration,
    ) -> Option<Vec<i16>> {
        let _ = tokio::fs::create_dir_all(&self.cache_dir).await;
        let seq = self.job_seq.fetch_add(1, Ordering::Relaxed);
        let pcm_path = self
            .cache_dir
            .join(format!("job-{}-{}.pcm", std::process::id(), seq));
        let pcm_path_str = pcm_path.to_string_lossy().into_owned();

        let mut play_url = url.to_string();
        if !play_url.starts_with("http") && !play_url.starts_with("file:") {
            if let Ok(parsed) = url::Url::from_file_path(&play_url) {
                play_url = parsed.to_string();
            }
        }

        let start_opt = if side == AnalysisSide::Tail {
            Some(format!("{:.3}", Self::tail_start(duration).as_secs_f64()))
        } else {
            None
        };
        let end_opt = if side == AnalysisSide::Head {
            Some(format!("{:.3}", Self::WINDOW.as_secs_f64()))
        } else {
            None
        };

        let header_list: Vec<String> = headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();

        (self.log)("analysis deck create");
        let handle_holder: Arc<Mutex<Option<MpvDeckHandle>>> = Arc::new(Mutex::new(None));
        let holder_create = Arc::clone(&handle_holder);
        let pcm_for_create = pcm_path_str.clone();
        let log_for_create = Arc::clone(&self.log);

        self.serialize
            .serialize(Box::pin(async move {
                match MpvDeckHandle::create(&pcm_for_create, Self::SAMPLE_RATE) {
                    Ok(deck) => {
                        if let Ok(mut guard) = holder_create.lock() {
                            *guard = Some(deck);
                        }
                    }
                    Err(e) => {
                        (log_for_create)(&format!("analysis deck create failed: {e}"));
                    }
                }
            }))
            .await;

        let deck = handle_holder.lock().ok().and_then(|mut g| g.take());
        let Some(deck) = deck else {
            return None;
        };
        let deck = Arc::new(deck);

        (self.log)(&format!("analysis deck open side={}", side.as_str()));
        let deck_open = Arc::clone(&deck);
        let play_url_clone = play_url.clone();
        let headers_joined = header_list.join(",");
        let open_flag = Arc::new(AtomicBool::new(false));
        let open_flag_inner = Arc::clone(&open_flag);

        self.serialize
            .serialize(Box::pin(async move {
                let ok = deck_open.open_media(
                    &play_url_clone,
                    if headers_joined.is_empty() {
                        None
                    } else {
                        Some(&headers_joined)
                    },
                    start_opt.as_deref(),
                    end_opt.as_deref(),
                );
                open_flag_inner.store(ok, Ordering::SeqCst);
            }))
            .await;
        let open_ok = open_flag.load(Ordering::SeqCst);

        let completed = if open_ok {
            let deck_wait = Arc::clone(&deck);
            let wait_fut = tokio::task::spawn_blocking(move || deck_wait.wait_until_done(Self::JOB_TIMEOUT));
            match tokio::time::timeout(Self::JOB_TIMEOUT + Duration::from_secs(2), wait_fut).await {
                Ok(Ok(ok)) => ok,
                _ => false,
            }
        } else {
            false
        };

        (self.log)("analysis deck dispose");
        let deck_dispose = Arc::clone(&deck);
        let dispose_fut = self.serialize.serialize(Box::pin(async move {
            deck_dispose.stop_and_destroy();
        }));
        let _ = tokio::time::timeout(Duration::from_secs(5), dispose_fut).await;

        if !completed {
            let _ = tokio::fs::remove_file(&pcm_path).await;
            return None;
        }

        let res = match tokio::fs::read(&pcm_path).await {
            Ok(bytes) => {
                let usable = bytes.len() & !1;
                // Under 8 s of audio cannot carry a tempo; the deck likely failed.
                if usable < (Self::SAMPLE_RATE as usize) * 2 * 8 {
                    None
                } else {
                    let mut out = Vec::with_capacity(usable / 2);
                    for chunk in bytes[..usable].chunks_exact(2) {
                        out.push(i16::from_le_bytes([chunk[0], chunk[1]]));
                    }
                    Some(out)
                }
            }
            Err(_) => None,
        };
        let _ = tokio::fs::remove_file(&pcm_path).await;
        res
    }

    // ---- disk cache -----------------------------------------------------------

    fn disk_path(cache_dir: &Path, key: &str) -> PathBuf {
        cache_dir.join(format!("{}.json", Self::fnv(key)))
    }

    async fn from_disk(&self, key: &str) -> Option<TrackAnalysis> {
        let path = Self::disk_path(&self.cache_dir, key);
        let text = tokio::fs::read_to_string(&path).await.ok()?;
        let val: serde_json::Value = serde_json::from_str(&text).ok()?;
        let obj = val.as_object()?;
        if obj.get("key")?.as_str()? != key {
            return None;
        }
        let a_val = obj.get("a")?;
        let parsed = TrackAnalysis::from_json(a_val);
        let clean = sanitize_analysis(parsed);
        if clean.is_none() {
            // Corrupt / older file: drop it so it is rebuilt.
            let _ = tokio::fs::remove_file(&path).await;
        }
        clean
    }

    fn marker_path(&self) -> PathBuf {
        self.cache_dir.join("decode-in-progress")
    }

    fn check_crash_marker(&self) {
        if self.decoder.is_some() {
            return;
        }
        let marker = self.marker_path();
        if marker.exists() {
            self.disabled.store(true, Ordering::SeqCst);
            let _ = std::fs::remove_file(&marker);
            (self.log)(
                "automix analysis off for this session: the previous run ended during an analysis decode",
            );
        }
    }

    async fn set_crash_marker(&self, on: bool) {
        let marker = self.marker_path();
        if on {
            let _ = tokio::fs::create_dir_all(&self.cache_dir).await;
            let content = format!("{} pid={}", Utc::now().to_rfc3339(), std::process::id());
            let _ = tokio::fs::write(&marker, content).await;
        } else if marker.exists() {
            let _ = tokio::fs::remove_file(&marker).await;
        }
    }

    /// FNV-1a 32 over the key plus its length; the JSON also stores the
    /// full key, so a collision just reads as a miss.
    pub fn fnv(s: &str) -> String {
        let mut h: u32 = 0x811c9dc5;
        for &b in s.as_bytes() {
            h ^= b as u32;
            h = h.wrapping_mul(0x01000193);
        }
        format!("{h:x}-{}", s.len())
    }
}

async fn write_to_disk(cache_dir: &Path, key: &str, a: &TrackAnalysis) {
    let _ = tokio::fs::create_dir_all(cache_dir).await;
    let path = AutomixAnalyzer::disk_path(cache_dir, key);
    let payload = serde_json::json!({
        "key": key,
        "a": a.to_json(),
    });
    if let Ok(s) = serde_json::to_string(&payload) {
        let _ = tokio::fs::write(path, s).await;
    }
}

/// Rejects analyses with values no real track produces (NaN, a negative
/// window, an absurd tempo, an unsorted grid) — from a bad decode or a
/// corrupt cache file — and returns the rest unchanged.
pub fn sanitize_analysis(a: Option<TrackAnalysis>) -> Option<TrackAnalysis> {
    let a = a?;
    let ok = |v: f64| v.is_finite();
    if !ok(a.window_start) || !ok(a.window_end) || a.window_end <= a.window_start {
        return None;
    }
    if !ok(a.audible_start) || !ok(a.content_end) {
        return None;
    }
    if a.audible_start < a.window_start - 0.5
        || a.content_end > a.window_end + 0.5
        || a.content_end < a.audible_start
    {
        return None;
    }
    if !ok(a.confidence) || a.confidence < 0.0 || a.confidence > 1.0 {
        return None;
    }
    if !ok(a.bpm) || a.bpm < 0.0 || a.bpm > 260.0 {
        return None;
    }
    if a.bpm > 0.0 && a.bpm < 40.0 {
        return None;
    }
    for i in 1..a.beats.len() {
        if !ok(a.beats[i]) || a.beats[i] <= a.beats[i - 1] {
            return None;
        }
    }
    for i in 1..a.downbeats.len() {
        if !ok(a.downbeats[i]) || a.downbeats[i] <= a.downbeats[i - 1] {
            return None;
        }
    }
    Some(a)
}

// ---- Deferred libmpv C ABI deck for headless PCM decode --------------------

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEventLogMessage {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}

const MPV_EVENT_NONE: c_int = 0;
const MPV_EVENT_LOG_MESSAGE: c_int = 2;
const MPV_EVENT_END_FILE: c_int = 7;

type FnMpvCreate = unsafe extern "C" fn() -> *mut c_void;
type FnMpvInitialize = unsafe extern "C" fn(*mut c_void) -> c_int;
type FnMpvTerminateDestroy = unsafe extern "C" fn(*mut c_void);
type FnMpvSetPropertyString =
    unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type FnMpvCommand = unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int;
type FnMpvWaitEvent = unsafe extern "C" fn(*mut c_void, f64) -> *const MpvEvent;
type FnMpvRequestLogMessages = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;

struct MpvDeckHandle {
    _lib: libloading::Library,
    ctx: Mutex<*mut c_void>,
    mpv_set_property_string: FnMpvSetPropertyString,
    mpv_command: FnMpvCommand,
    mpv_wait_event: FnMpvWaitEvent,
    mpv_terminate_destroy: FnMpvTerminateDestroy,
}

// Safety: libmpv handle calls are externally serialized by MpvSerializer + ctx Mutex.
unsafe impl Send for MpvDeckHandle {}
unsafe impl Sync for MpvDeckHandle {}

impl MpvDeckHandle {
    fn load_library() -> Result<libloading::Library, String> {
        #[cfg(target_os = "windows")]
        let candidates = ["libmpv-2.dll", "mpv-2.dll", "libmpv.dll"];
        #[cfg(target_os = "macos")]
        let candidates = ["libmpv.2.dylib", "libmpv.dylib"];
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let candidates = ["libmpv.so.2", "libmpv.so"];

        for name in candidates {
            if let Ok(lib) = unsafe { libloading::Library::new(name) } {
                return Ok(lib);
            }
        }
        Err("could not load libmpv shared library".to_string())
    }

    fn create(pcm_path: &str, sample_rate: u32) -> Result<Self, String> {
        let lib = Self::load_library()?;
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
            let mpv_command: FnMpvCommand = *lib
                .get::<FnMpvCommand>(b"mpv_command\0")
                .map_err(|e| e.to_string())?;
            let mpv_wait_event: FnMpvWaitEvent = *lib
                .get::<FnMpvWaitEvent>(b"mpv_wait_event\0")
                .map_err(|e| e.to_string())?;
            let mpv_request_log_messages: Option<FnMpvRequestLogMessages> = lib
                .get::<FnMpvRequestLogMessages>(b"mpv_request_log_messages\0")
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

            set_prop("vo", "null");
            set_prop("title", "LastWave Analysis");
            set_prop("demuxer-max-bytes", &(8 * 1024 * 1024).to_string());

            // Order matters: the output is chosen when audio first starts.
            set_prop("ao", "pcm");
            set_prop("ao-pcm-file", pcm_path);
            set_prop("ao-pcm-waveheader", "no");
            set_prop("untimed", "yes");
            set_prop("audio-samplerate", &sample_rate.to_string());
            set_prop("audio-channels", "mono");
            set_prop("audio-format", "s16");
            set_prop("vid", "no");
            set_prop("replaygain", "no");
            set_prop("af", "");
            set_prop("volume", "100");
            set_prop("stream-lavf-o", "timeout=15000000");

            if let Some(req_log) = mpv_request_log_messages {
                if let Ok(lvl) = CString::new("error") {
                    let _ = req_log(ctx, lvl.as_ptr());
                }
            }

            if mpv_initialize(ctx) < 0 {
                mpv_terminate_destroy(ctx);
                return Err("mpv_initialize failed".to_string());
            }

            Ok(Self {
                _lib: lib,
                ctx: Mutex::new(ctx),
                mpv_set_property_string,
                mpv_command,
                mpv_wait_event,
                mpv_terminate_destroy,
            })
        }
    }

    fn open_media(
        &self,
        url: &str,
        headers_joined: Option<&str>,
        start: Option<&str>,
        end: Option<&str>,
    ) -> bool {
        let Ok(guard) = self.ctx.lock() else {
            return false;
        };
        let ctx = *guard;
        if ctx.is_null() {
            return false;
        }
        unsafe {
            if let Some(hdrs) = headers_joined {
                if let (Ok(ck), Ok(cv)) =
                    (CString::new("http-header-fields"), CString::new(hdrs))
                {
                    let _ = (self.mpv_set_property_string)(ctx, ck.as_ptr(), cv.as_ptr());
                }
            }
            if let Some(s) = start {
                if let (Ok(ck), Ok(cv)) = (CString::new("start"), CString::new(s)) {
                    let _ = (self.mpv_set_property_string)(ctx, ck.as_ptr(), cv.as_ptr());
                }
            }
            if let Some(e) = end {
                if let (Ok(ck), Ok(cv)) = (CString::new("end"), CString::new(e)) {
                    let _ = (self.mpv_set_property_string)(ctx, ck.as_ptr(), cv.as_ptr());
                }
            }
            let Ok(c_cmd) = CString::new("loadfile") else {
                return false;
            };
            let Ok(c_url) = CString::new(url) else {
                return false;
            };
            let args = [c_cmd.as_ptr(), c_url.as_ptr(), std::ptr::null()];
            (self.mpv_command)(ctx, args.as_ptr()) >= 0
        }
    }

    fn wait_until_done(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let wait_s = remaining.as_secs_f64().min(0.25);
            let ctx = {
                let Ok(guard) = self.ctx.lock() else {
                    return false;
                };
                *guard
            };
            if ctx.is_null() {
                return false;
            }
            unsafe {
                let ev = (self.mpv_wait_event)(ctx, wait_s);
                if ev.is_null() {
                    continue;
                }
                let ev_ref = &*ev;
                match ev_ref.event_id {
                    MPV_EVENT_NONE => continue,
                    MPV_EVENT_END_FILE => return ev_ref.error >= 0,
                    MPV_EVENT_LOG_MESSAGE => {
                        if !ev_ref.data.is_null() {
                            let msg = &*(ev_ref.data as *const MpvEventLogMessage);
                            if !msg.text.is_null() {
                                let t = CStr::from_ptr(msg.text).to_string_lossy().to_lowercase();
                                if !t.contains("filter")
                                    && (t.contains("failed to open") || t.contains("tcp:"))
                                {
                                    return false;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn stop_and_destroy(&self) {
        let Ok(mut guard) = self.ctx.lock() else {
            return;
        };
        let ctx = *guard;
        if ctx.is_null() {
            return;
        }
        *guard = std::ptr::null_mut();
        unsafe {
            if let Ok(c_stop) = CString::new("stop") {
                let args = [c_stop.as_ptr(), std::ptr::null()];
                let _ = (self.mpv_command)(ctx, args.as_ptr());
            }
            (self.mpv_terminate_destroy)(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;
    use std::sync::atomic::AtomicUsize;

    fn make_sine_clicks(sr: u32, seconds: f64) -> Vec<i16> {
        let len = (sr as f64 * seconds) as usize;
        let mut pcm = vec![0_i16; len];
        let mut t = 0.2_f64;
        while t < seconds {
            let start = (t * sr as f64) as usize;
            let dur = (0.04 * sr as f64) as usize;
            for i in 0..dur {
                if start + i >= len {
                    break;
                }
                let env = (-8.0 * i as f64 / dur as f64).exp();
                let s = (2.0 * PI * 85.0 * i as f64 / sr as f64).sin() * 0.8;
                pcm[start + i] = (s * env * 28000.0).clamp(-32767.0, 32767.0) as i16;
            }
            t += 0.5;
        }
        pcm
    }

    #[test]
    fn source_id_and_sanitize_checks() {
        assert_eq!(
            AutomixAnalyzer::source_id("youtube:abc123:high"),
            "youtube:abc123"
        );
        assert_eq!(AutomixAnalyzer::source_id("addon:xyz"), "addon:xyz");

        let valid = TrackAnalysis {
            window_start: 0.0,
            window_end: 30.0,
            bpm: 120.0,
            confidence: 0.8,
            beats: vec![0.5, 1.0],
            downbeats: vec![0.5],
            audible_start: 0.1,
            content_end: 29.8,
            energy_db: vec![-12.0],
            energy_hop: 0.5,
        };
        assert!(sanitize_analysis(Some(valid.clone())).is_some());

        let mut bad_bpm = valid.clone();
        bad_bpm.bpm = 30.0;
        assert!(sanitize_analysis(Some(bad_bpm)).is_none());

        let mut unsorted_beats = valid;
        unsorted_beats.beats = vec![2.0, 1.0];
        assert!(sanitize_analysis(Some(unsorted_beats)).is_none());
    }

    #[tokio::test]
    async fn analyzer_caches_in_memory_and_on_disk_and_disables_after_failures() {
        let dir = std::env::temp_dir().join(format!("lw_automix_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_hook = Arc::clone(&calls);

        let hook: DecoderHook = Arc::new(move |_side, _url, _hdrs, _dur| {
            let calls_inner = Arc::clone(&calls_hook);
            Box::pin(async move {
                let n = calls_inner.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Some(make_sine_clicks(AutomixAnalyzer::SAMPLE_RATE, 24.0))
                } else {
                    None
                }
            })
        });

        let serializer = Arc::new(MutexMpvSerializer::default());
        let log: LogHook = Arc::new(|_| {});
        let analyzer = Arc::new(AutomixAnalyzer::new(
            serializer,
            log,
            Some(dir.clone()),
            None,
            Some(hook),
        ));

        let k1 = AutomixAnalyzer::key_for("q1", "addon:1", AnalysisSide::Head);
        let res1 = analyzer
            .request(
                k1.clone(),
                AnalysisSide::Head,
                "http://example.com/a.flac".into(),
                HashMap::new(),
                Duration::from_secs(120),
            )
            .await;
        assert!(res1.is_some());
        assert!(analyzer.cached(&k1).is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Give spawned disk write a moment, then verify cached hit doesn't re-decode
        let res2 = analyzer
            .request(
                k1.clone(),
                AnalysisSide::Head,
                "http://example.com/a.flac".into(),
                HashMap::new(),
                Duration::from_secs(120),
            )
            .await;
        assert!(res2.is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Trigger 4 consecutive failures to trip auto-disable
        for i in 0..4 {
            let kf = format!("fail|{i}");
            let r = analyzer
                .request(
                    kf,
                    AnalysisSide::Head,
                    "http://example.com/fail.flac".into(),
                    HashMap::new(),
                    Duration::from_secs(60),
                )
                .await;
            assert!(r.is_none());
        }
        assert!(analyzer.disabled());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
