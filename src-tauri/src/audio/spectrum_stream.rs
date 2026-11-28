use std::collections::{HashMap, HashSet, VecDeque};
use std::f64::consts::PI;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RANGE};
use serde::Serialize;
use symphonia::core::codecs::audio::well_known::{CODEC_ID_AC3, CODEC_ID_EAC3};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::common::Tier;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};
use symphonia_opus::OpusDecoder;
use tauri::ipc::Channel;
use url::Url;

use crate::audio::audio_analysis::fft;
use crate::models::player_state::PlayerSnapshot;

pub const SPECTRUM_BANDS: usize = 48;
pub const SPECTRUM_FFT_SIZE: usize = 512;
pub const SPECTRUM_INTERVAL: Duration = Duration::from_millis(33);

const MIN_FREQ_HZ: f64 = 20.0;
const MAX_FREQ_HZ: f64 = 20_000.0;
const DB_FLOOR: f64 = -64.0;
const ATTACK_ALPHA: f32 = 0.68;
const RELEASE_ALPHA: f32 = 0.28;

const PCM_RING_CAP: usize = 4096;
const HTTP_PAGE_SIZE: usize = 128 * 1024;
const HTTP_MAX_CACHE_PAGES: usize = 8; // Hard cap: 1 MiB network cache
const HTTP_IO_TIMEOUT: Duration = Duration::from_secs(10);
const SEEK_RESYNC_AHEAD_SEC: f64 = 1.5;
const SEEK_RESYNC_BEHIND_SEC: f64 = 0.5;
const OPEN_RETRY_COOLDOWN: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpectrumFrame {
    pub bins: Vec<f32>,
    pub rms: f32,
}

impl SpectrumFrame {
    pub fn silence() -> Self {
        Self {
            bins: vec![0.0; SPECTRUM_BANDS],
            rms: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
struct PlaybackClock {
    stream_url: String,
    mime_type: String,
    request_headers: HashMap<String, String>,
    position_ms: u64,
    speed: f64,
    updated_at: Instant,
    source_gen: u64,
}

impl Default for PlaybackClock {
    fn default() -> Self {
        Self {
            stream_url: String::new(),
            mime_type: String::new(),
            request_headers: HashMap::new(),
            position_ms: 0,
            speed: 1.0,
            updated_at: Instant::now(),
            source_gen: 0,
        }
    }
}

impl PlaybackClock {
    fn estimated_position_sec(&self) -> f64 {
        let base = self.position_ms as f64 / 1000.0;
        let dt = self.updated_at.elapsed().as_secs_f64();
        (base + dt * self.speed.clamp(0.25, 3.0)).max(0.0)
    }
}

/// Computes 48 log-spaced normalized frequency bands and RMS from mono PCM samples
/// using the 512-point radix-2 FFT in `audio_analysis`.
pub fn compute_spectrum_frame(
    pcm: &[f32],
    sample_rate: u32,
    prev_bins: &mut [f32; SPECTRUM_BANDS],
) -> SpectrumFrame {
    let mut re = [0.0_f64; SPECTRUM_FFT_SIZE];
    let mut im = [0.0_f64; SPECTRUM_FFT_SIZE];

    let copy_len = pcm.len().min(SPECTRUM_FFT_SIZE);
    let offset = pcm.len().saturating_sub(copy_len);
    let mut sq_sum = 0.0_f64;

    for i in 0..copy_len {
        let s = f64::from(pcm[offset + i]);
        sq_sum += s * s;
        let hann = 0.5 - 0.5 * (2.0 * PI * i as f64 / (SPECTRUM_FFT_SIZE - 1) as f64).cos();
        re[i] = s * hann;
    }

    let rms = if copy_len > 0 {
        (sq_sum / copy_len as f64).sqrt().clamp(0.0, 1.0) as f32
    } else {
        0.0
    };

    fft(&mut re, &mut im);

    let sr = f64::from(sample_rate.max(8_000));
    let nyquist = sr * 0.5;
    let hi_hz = MAX_FREQ_HZ.min(nyquist);
    let bin_hz = sr / SPECTRUM_FFT_SIZE as f64;
    let half = SPECTRUM_FFT_SIZE / 2;
    let norm = half as f64 * 0.5;

    let log_min = MIN_FREQ_HZ.ln();
    let log_span = (hi_hz / MIN_FREQ_HZ).ln().max(1e-6);

    let mut out = Vec::with_capacity(SPECTRUM_BANDS);
    for band in 0..SPECTRUM_BANDS {
        let f0 = (log_min + log_span * (band as f64 / SPECTRUM_BANDS as f64)).exp();
        let f1 = (log_min + log_span * ((band + 1) as f64 / SPECTRUM_BANDS as f64)).exp();

        let lo_bin = ((f0 / bin_hz).floor() as usize).clamp(1, half - 1);
        let hi_bin = ((f1 / bin_hz).ceil() as usize).clamp(lo_bin, half - 1);

        let mut peak_mag = 0.0_f64;
        for k in lo_bin..=hi_bin {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt() / norm;
            if mag > peak_mag {
                peak_mag = mag;
            }
        }

        let db = if peak_mag <= 1e-7 {
            DB_FLOOR
        } else {
            20.0 * peak_mag.log10()
        };
        let raw = ((db - DB_FLOOR) / -DB_FLOOR).clamp(0.0, 1.0) as f32;
        let prev = prev_bins[band];
        let alpha = if raw >= prev {
            ATTACK_ALPHA
        } else {
            RELEASE_ALPHA
        };
        let smoothed = prev + (raw - prev) * alpha;
        prev_bins[band] = smoothed;
        out.push(smoothed);
    }

    SpectrumFrame { bins: out, rms }
}

/// Manages on-demand FFT spectrum analysis using an isolated pure-Rust decoder
/// (`symphonia` + `symphonia-opus` + `filmcraft-ac3` for Dolby AC-3 / E-AC-3).
/// Never touches OS audio drivers, WASAPI loopback, COM, or `libmpv`.
pub struct SpectrumStreamController {
    next_id: AtomicU64,
    worker_gen: AtomicU64,
    subscribers: Mutex<HashMap<u64, Channel<SpectrumFrame>>>,
    is_playing: AtomicBool,
    running: AtomicBool,
    stop_flag: Mutex<Option<Arc<AtomicBool>>>,
    clock: Mutex<PlaybackClock>,
}

impl Default for SpectrumStreamController {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumStreamController {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            worker_gen: AtomicU64::new(0),
            subscribers: Mutex::new(HashMap::new()),
            is_playing: AtomicBool::new(false),
            running: AtomicBool::new(false),
            stop_flag: Mutex::new(None),
            clock: Mutex::new(PlaybackClock::default()),
        }
    }

    pub fn subscribe(self: &Arc<Self>, channel: Channel<SpectrumFrame>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut guard) = self.subscribers.lock() {
            guard.insert(id, channel);
        }
        self.reconcile_worker();
        id
    }

    pub fn unsubscribe(self: &Arc<Self>, subscription_id: Option<u64>) {
        if let Ok(mut guard) = self.subscribers.lock() {
            match subscription_id {
                Some(id) => {
                    guard.remove(&id);
                }
                None => {
                    guard.clear();
                }
            }
        }
        self.reconcile_worker();
    }

    pub fn update_playback(self: &Arc<Self>, snap: &PlayerSnapshot) {
        let stream_url = snap
            .stream
            .as_ref()
            .map(|s| s.url.clone())
            .or_else(|| snap.current.as_ref().map(|t| t.playback_url.clone()))
            .unwrap_or_default();
        let mime_type = snap
            .stream
            .as_ref()
            .map(|s| s.mime_type.clone())
            .unwrap_or_default();
        let request_headers = snap
            .stream
            .as_ref()
            .map(|s| s.request_headers.clone())
            .unwrap_or_default();

        if let Ok(mut clock) = self.clock.lock() {
            let url_changed = clock.stream_url != stream_url;
            let new_sec = snap.position_ms as f64 / 1000.0;
            let old_est = clock.estimated_position_sec();
            let jumped = (new_sec - old_est).abs() > 1.25;

            if url_changed || jumped {
                clock.source_gen = clock.source_gen.wrapping_add(1);
            }
            clock.stream_url = stream_url;
            clock.mime_type = mime_type;
            clock.request_headers = request_headers;
            clock.position_ms = snap.position_ms;
            clock.speed = if snap.speed > 0.0 { snap.speed } else { 1.0 };
            clock.updated_at = Instant::now();
        }

        let active_playing = snap.is_playing && !snap.is_buffering;
        self.set_playing(active_playing);
    }

    pub fn set_playing(self: &Arc<Self>, playing: bool) {
        let was_playing = self.is_playing.swap(playing, Ordering::SeqCst);
        if was_playing && !playing {
            self.stop_worker();
            self.broadcast(SpectrumFrame::silence());
        } else if playing {
            self.reconcile_worker();
        }
    }

    fn has_subscribers(&self) -> bool {
        self.subscribers
            .lock()
            .map(|g| !g.is_empty())
            .unwrap_or(false)
    }

    fn reconcile_worker(self: &Arc<Self>) {
        if self.is_playing.load(Ordering::SeqCst) && self.has_subscribers() {
            self.start_worker();
        } else {
            self.stop_worker();
        }
    }

    fn start_worker(self: &Arc<Self>) {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let gen = self.worker_gen.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        let stop = Arc::new(AtomicBool::new(false));
        if let Ok(mut guard) = self.stop_flag.lock() {
            if let Some(prev) = guard.replace(Arc::clone(&stop)) {
                prev.store(true, Ordering::SeqCst);
            }
        }

        let ctrl = Arc::clone(self);
        if std::thread::Builder::new()
            .name("lw-spectrum-symphonia".to_string())
            .spawn(move || {
                ctrl.run_decode_loop(&stop);
                ctrl.running.store(false, Ordering::SeqCst);
                if ctrl.worker_gen.load(Ordering::SeqCst) == gen
                    && ctrl.is_playing.load(Ordering::SeqCst)
                    && ctrl.has_subscribers()
                    && !stop.load(Ordering::SeqCst)
                {
                    ctrl.start_worker();
                }
            })
            .is_err()
        {
            self.running.store(false, Ordering::SeqCst);
        }
    }

    fn stop_worker(&self) {
        self.worker_gen.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut guard) = self.stop_flag.lock() {
            if let Some(flag) = guard.take() {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }

    fn broadcast(&self, frame: SpectrumFrame) -> bool {
        let mut dead = Vec::new();
        let remaining = if let Ok(mut guard) = self.subscribers.lock() {
            for (&id, ch) in guard.iter() {
                if ch.send(frame.clone()).is_err() {
                    dead.push(id);
                }
            }
            for id in dead {
                guard.remove(&id);
            }
            guard.len()
        } else {
            0
        };
        remaining > 0
    }

    fn run_decode_loop(&self, stop: &Arc<AtomicBool>) {
        let mut decoder_tap = SpectrumTap::new();
        let mut fft_window = [0.0_f32; SPECTRUM_FFT_SIZE];
        let mut prev_bins = [0.0_f32; SPECTRUM_BANDS];

        while !stop.load(Ordering::SeqCst)
            && self.is_playing.load(Ordering::SeqCst)
            && self.has_subscribers()
        {
            let tick_start = Instant::now();
            let clock = self
                .clock
                .lock()
                .map(|c| c.clone())
                .unwrap_or_default();

            let target_sec = clock.estimated_position_sec();
            let sr = decoder_tap.read_window(
                &clock.stream_url,
                &clock.mime_type,
                &clock.request_headers,
                target_sec,
                &mut fft_window,
                stop,
            );

            let frame = compute_spectrum_frame(&fft_window, sr, &mut prev_bins);
            if !self.broadcast(frame) {
                break;
            }

            let elapsed = tick_start.elapsed();
            if elapsed < SPECTRUM_INTERVAL {
                std::thread::sleep(SPECTRUM_INTERVAL - elapsed);
            }
        }
    }
}

fn spectrum_codec_registry() -> &'static CodecRegistry {
    static REGISTRY: OnceLock<CodecRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        registry.register_audio_decoder_at_tier::<OpusDecoder>(Tier::Fallback);
        registry
    })
}

fn spectrum_http_runtime() -> Option<&'static tokio::runtime::Runtime> {
    static RT: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()
    })
    .as_ref()
}

fn spectrum_http_client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            // No whole-body request `.timeout()` here: long-lived sequential streams
            // use per-operation `tokio::time::timeout(HTTP_IO_TIMEOUT, ...)` instead.
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::limited(6))
                .build()
                .ok()
        })
        .as_ref()
}

fn local_path_from_url(url: &str) -> Option<PathBuf> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return None;
    }
    if trimmed.starts_with("file://") {
        if let Ok(parsed) = Url::parse(trimmed) {
            if let Ok(path) = parsed.to_file_path() {
                return Some(path);
            }
        }
        let rest = trimmed.strip_prefix("file://").unwrap_or(trimmed);
        #[cfg(windows)]
        let normalized = rest
            .strip_prefix('/')
            .filter(|p| p.as_bytes().get(1) == Some(&b':'))
            .unwrap_or(rest)
            .replace('/', "\\");
        #[cfg(not(windows))]
        let normalized = rest.to_string();
        return Some(PathBuf::from(normalized));
    }
    Some(PathBuf::from(trimmed))
}

fn is_hls_stream(url: &str, mime_type: &str) -> bool {
    let lower_url = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    let lower_mime = mime_type.to_ascii_lowercase();
    lower_url.ends_with(".m3u8")
        || lower_mime.contains("mpegurl")
        || lower_mime.contains("application/x-mpegurl")
}

fn build_hint(url: &str, mime_type: &str) -> Hint {
    let mut hint = Hint::new();
    let clean_mime = mime_type.split(';').next().unwrap_or("").trim();
    if !clean_mime.is_empty() {
        hint.mime_type(clean_mime);
    }
    let path_part = url.split('?').next().unwrap_or(url);
    if let Some(ext) = std::path::Path::new(path_part)
        .extension()
        .and_then(|e| e.to_str())
    {
        if !ext.is_empty() {
            hint.with_extension(ext);
        }
    }
    hint
}

fn build_header_map(headers: &HashMap<String, String>) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in headers {
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.trim().as_bytes()),
            HeaderValue::from_str(v),
        ) {
            map.insert(name, val);
        }
    }
    map
}

fn parse_content_range_total(val: Option<&HeaderValue>) -> Option<u64> {
    let s = val?.to_str().ok()?;
    let slash = s.rsplit('/').next()?;
    if slash == "*" {
        return None;
    }
    slash.trim().parse::<u64>().ok()
}

fn run_http<F, T>(rt: &tokio::runtime::Runtime, fut: F) -> io::Result<T>
where
    F: std::future::Future<Output = Result<T, reqwest::Error>>,
{
    match rt.block_on(tokio::time::timeout(HTTP_IO_TIMEOUT, fut)) {
        Ok(Ok(val)) => Ok(val),
        Ok(Err(e)) => Err(io::Error::other(e)),
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "http timeout")),
    }
}

enum HttpTransport {
    Paged {
        total_len: Option<u64>,
        pages: VecDeque<(u64, Vec<u8>)>,
    },
    Sequential {
        buf: Vec<u8>,
        cursor: usize,
        stream: Option<reqwest::Response>,
    },
}

struct HttpMediaSource {
    client: reqwest::Client,
    rt: &'static tokio::runtime::Runtime,
    url: String,
    headers: HeaderMap,
    stop: Arc<AtomicBool>,
    pos: u64,
    transport: HttpTransport,
}

impl HttpMediaSource {
    fn open(
        url: &str,
        request_headers: &HashMap<String, String>,
        stop: Arc<AtomicBool>,
    ) -> io::Result<Self> {
        let rt = spectrum_http_runtime()
            .ok_or_else(|| io::Error::other("http runtime unavailable"))?;
        let client = spectrum_http_client()
            .ok_or_else(|| io::Error::other("http client unavailable"))?
            .clone();
        let headers = build_header_map(request_headers);

        let initial_range = format!("bytes=0-{}", HTTP_PAGE_SIZE - 1);
        let resp = run_http(
            rt,
            client
                .get(url)
                .headers(headers.clone())
                .header(RANGE, &initial_range)
                .send(),
        )?;

        if !resp.status().is_success() && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(io::Error::other(format!("HTTP {}", resp.status())));
        }

        if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            let total_len =
                parse_content_range_total(resp.headers().get(reqwest::header::CONTENT_RANGE));
            let bytes = run_http(rt, resp.bytes())?;
            let mut pages = VecDeque::with_capacity(HTTP_MAX_CACHE_PAGES);
            pages.push_back((0, bytes.to_vec()));
            return Ok(Self {
                client,
                rt,
                url: url.to_string(),
                headers,
                stop,
                pos: 0,
                transport: HttpTransport::Paged { total_len, pages },
            });
        }

        Ok(Self {
            client,
            rt,
            url: url.to_string(),
            headers,
            stop,
            pos: 0,
            transport: HttpTransport::Sequential {
                buf: Vec::new(),
                cursor: 0,
                stream: Some(resp),
            },
        })
    }

    fn fetch_page(&mut self, page_idx: u64) -> io::Result<&[u8]> {
        let HttpTransport::Paged { total_len, pages } = &mut self.transport else {
            return Err(io::Error::other("not paged"));
        };

        if let Some(pos) = pages.iter().position(|(idx, _)| *idx == page_idx) {
            if let Some(entry) = pages.remove(pos) {
                pages.push_back(entry);
            }
            return Ok(&pages.back().expect("just pushed").1);
        }

        if self.stop.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"));
        }

        let start = page_idx.saturating_mul(HTTP_PAGE_SIZE as u64);
        if let Some(total) = *total_len {
            if start >= total {
                return Ok(&[]);
            }
        }
        let end = start + HTTP_PAGE_SIZE as u64 - 1;
        let range_val = format!("bytes={start}-{end}");

        let client = &self.client;
        let url = &self.url;
        let headers = &self.headers;
        let data = run_http(self.rt, async {
            let resp = client
                .get(url)
                .headers(headers.clone())
                .header(RANGE, &range_val)
                .send()
                .await?;
            if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
                return Ok::<Vec<u8>, reqwest::Error>(Vec::new());
            }
            let bytes = resp.bytes().await?;
            Ok(bytes.to_vec())
        })?;

        if pages.len() >= HTTP_MAX_CACHE_PAGES {
            pages.pop_front();
        }
        pages.push_back((page_idx, data));
        Ok(&pages.back().expect("just pushed").1)
    }
}

impl Read for HttpMediaSource {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.stop.load(Ordering::Relaxed) {
            return Ok(0);
        }

        match &mut self.transport {
            HttpTransport::Paged { total_len, .. } => {
                if let Some(total) = *total_len {
                    if self.pos >= total {
                        return Ok(0);
                    }
                }
                let page_idx = self.pos / HTTP_PAGE_SIZE as u64;
                let page_off = (self.pos % HTTP_PAGE_SIZE as u64) as usize;
                let page = self.fetch_page(page_idx)?;
                if page_off >= page.len() {
                    return Ok(0);
                }
                let avail = page.len() - page_off;
                let n = out.len().min(avail);
                out[..n].copy_from_slice(&page[page_off..page_off + n]);
                self.pos += n as u64;
                Ok(n)
            }
            HttpTransport::Sequential {
                buf,
                cursor,
                stream,
            } => {
                if *cursor >= buf.len() {
                    buf.clear();
                    *cursor = 0;
                    if let Some(resp) = stream.as_mut() {
                        match run_http(self.rt, resp.chunk())? {
                            Some(chunk) => buf.extend_from_slice(&chunk),
                            None => {
                                *stream = None;
                                return Ok(0);
                            }
                        }
                    } else {
                        return Ok(0);
                    }
                }
                let avail = buf.len() - *cursor;
                let n = out.len().min(avail);
                out[..n].copy_from_slice(&buf[*cursor..*cursor + n]);
                *cursor += n;
                self.pos += n as u64;
                Ok(n)
            }
        }
    }
}

impl Seek for HttpMediaSource {
    fn seek(&mut self, style: SeekFrom) -> io::Result<u64> {
        let HttpTransport::Paged { total_len, .. } = &self.transport else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "sequential HTTP stream is not seekable",
            ));
        };
        let target: i128 = match style {
            SeekFrom::Start(p) => i128::from(p),
            SeekFrom::Current(d) => i128::from(self.pos) + i128::from(d),
            SeekFrom::End(d) => {
                let len = (*total_len).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::Unsupported, "unknown HTTP stream length")
                })?;
                i128::from(len) + i128::from(d)
            }
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start of stream",
            ));
        }
        self.pos = target as u64;
        Ok(self.pos)
    }
}

impl MediaSource for HttpMediaSource {
    fn is_seekable(&self) -> bool {
        matches!(self.transport, HttpTransport::Paged { .. })
    }

    fn byte_len(&self) -> Option<u64> {
        match &self.transport {
            HttpTransport::Paged { total_len, .. } => *total_len,
            HttpTransport::Sequential { .. } => None,
        }
    }
}

/// Lightweight sequential HLS (`.m3u8`) audio stream reader for live radio & HLS streams.
struct HlsMediaSource {
    client: reqwest::Client,
    rt: &'static tokio::runtime::Runtime,
    playlist_url: String,
    headers: HeaderMap,
    stop: Arc<AtomicBool>,
    pending_segments: VecDeque<String>,
    seen_segments: HashSet<String>,
    init_loaded: bool,
    endlist: bool,
    buf: Vec<u8>,
    cursor: usize,
    pos: u64,
    first_ext: Option<String>,
}

impl HlsMediaSource {
    fn open(
        url: &str,
        request_headers: &HashMap<String, String>,
        stop: Arc<AtomicBool>,
    ) -> io::Result<Self> {
        let rt = spectrum_http_runtime()
            .ok_or_else(|| io::Error::other("http runtime unavailable"))?;
        let client = spectrum_http_client()
            .ok_or_else(|| io::Error::other("http client unavailable"))?
            .clone();
        let headers = build_header_map(request_headers);

        let mut src = Self {
            client,
            rt,
            playlist_url: url.to_string(),
            headers,
            stop,
            pending_segments: VecDeque::new(),
            seen_segments: HashSet::new(),
            init_loaded: false,
            endlist: false,
            buf: Vec::new(),
            cursor: 0,
            pos: 0,
            first_ext: None,
        };
        src.resolve_initial_playlist()?;
        Ok(src)
    }

    fn fetch_text(&self, url: &str) -> io::Result<String> {
        let client = &self.client;
        let headers = &self.headers;
        run_http(self.rt, async {
            let resp = client.get(url).headers(headers.clone()).send().await?;
            let resp = resp.error_for_status()?;
            resp.text().await
        })
    }

    fn resolve_initial_playlist(&mut self) -> io::Result<()> {
        let text = self.fetch_text(&self.playlist_url)?;
        if text.contains("#EXT-X-STREAM-INF") {
            let base = Url::parse(&self.playlist_url).map_err(io::Error::other)?;
            let mut next_is_uri = false;
            for line in text.lines().map(str::trim) {
                if line.starts_with("#EXT-X-STREAM-INF") {
                    next_is_uri = true;
                    continue;
                }
                if next_is_uri && !line.is_empty() && !line.starts_with('#') {
                    if let Ok(resolved) = base.join(line) {
                        self.playlist_url = resolved.to_string();
                        break;
                    }
                    next_is_uri = false;
                }
            }
            let media_text = self.fetch_text(&self.playlist_url)?;
            self.enqueue_media_playlist(&media_text)?;
        } else {
            self.enqueue_media_playlist(&text)?;
        }
        Ok(())
    }

    fn enqueue_media_playlist(&mut self, text: &str) -> io::Result<()> {
        let base = Url::parse(&self.playlist_url).map_err(io::Error::other)?;
        if text.contains("#EXT-X-ENDLIST") {
            self.endlist = true;
        }
        for line in text.lines().map(str::trim) {
            if line.is_empty() {
                continue;
            }
            if !self.init_loaded && line.starts_with("#EXT-X-MAP:") {
                if let Some(uri) = extract_attr_uri(line) {
                    if let Ok(resolved) = base.join(uri) {
                        self.init_loaded = true;
                        self.pending_segments.push_back(resolved.to_string());
                    }
                }
                continue;
            }
            if line.starts_with('#') {
                continue;
            }
            if let Ok(resolved) = base.join(line) {
                let s = resolved.to_string();
                if self.first_ext.is_none() {
                    let clean = s.split('?').next().unwrap_or(&s);
                    if let Some(ext) = std::path::Path::new(clean)
                        .extension()
                        .and_then(|e| e.to_str())
                    {
                        self.first_ext = Some(ext.to_string());
                    }
                }
                if self.seen_segments.insert(s.clone()) {
                    if self.seen_segments.len() > 256 {
                        self.seen_segments.clear();
                        self.seen_segments.insert(s.clone());
                    }
                    self.pending_segments.push_back(s);
                }
            }
        }
        Ok(())
    }

    fn load_next_segment(&mut self) -> io::Result<bool> {
        if self.pending_segments.is_empty() && !self.endlist {
            let text = self.fetch_text(&self.playlist_url)?;
            self.enqueue_media_playlist(&text)?;
        }
        let Some(seg_url) = self.pending_segments.pop_front() else {
            return Ok(false);
        };
        let client = &self.client;
        let headers = &self.headers;
        let bytes = run_http(self.rt, async {
            let resp = client.get(&seg_url).headers(headers.clone()).send().await?;
            let resp = resp.error_for_status()?;
            resp.bytes().await
        })?;
        self.buf = bytes.to_vec();
        self.cursor = 0;
        Ok(!self.buf.is_empty())
    }
}

fn extract_attr_uri(line: &str) -> Option<&str> {
    let idx = line.find("URI=\"")?;
    let rest = &line[idx + 5..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

impl Read for HlsMediaSource {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.stop.load(Ordering::Relaxed) {
            return Ok(0);
        }
        while self.cursor >= self.buf.len() {
            if !self.load_next_segment()? {
                return Ok(0);
            }
        }
        let avail = self.buf.len() - self.cursor;
        let n = out.len().min(avail);
        out[..n].copy_from_slice(&self.buf[self.cursor..self.cursor + n]);
        self.cursor += n;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for HlsMediaSource {
    fn seek(&mut self, _style: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "HLS stream is sequential",
        ))
    }
}

impl MediaSource for HlsMediaSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

fn push_mono_sample(ring: &mut VecDeque<f32>, sample: f32) {
    if ring.len() >= PCM_RING_CAP {
        ring.pop_front();
    }
    ring.push_back(sample);
}

fn copy_ring_window(ring: &VecDeque<f32>, eof: bool, out: &mut [f32; SPECTRUM_FFT_SIZE]) {
    if ring.is_empty() || eof {
        out.fill(0.0);
        return;
    }
    let len = ring.len();
    let take = len.min(SPECTRUM_FFT_SIZE);
    let pad = SPECTRUM_FFT_SIZE - take;
    out[..pad].fill(0.0);
    let start = len - take;
    for (i, &sample) in ring.iter().skip(start).enumerate() {
        out[pad + i] = sample;
    }
}

enum SpectrumAudioDecoder {
    Symphonia(Box<dyn AudioDecoder>),
    Ac3(filmcraft_ac3::Decoder),
}

impl SpectrumAudioDecoder {
    fn reset(&mut self) {
        match self {
            Self::Symphonia(dec) => dec.reset(),
            Self::Ac3(dec) => dec.reset(),
        }
    }

    fn decode_into_ring(
        &mut self,
        pkt_data: &[u8],
        pkt_ref: symphonia::core::packet::PacketRef<'_>,
        interleaved_scratch: &mut Vec<f32>,
        pcm_ring: &mut VecDeque<f32>,
    ) -> Option<(u32, usize)> {
        match self {
            Self::Symphonia(dec) => {
                let decoded = dec.decode_ref(&pkt_ref).ok()?;
                let frames = decoded.frames();
                if frames == 0 {
                    return None;
                }
                let spec = decoded.spec();
                let sr = spec.rate().max(8_000);
                let channels = spec.channels().count().max(1);
                interleaved_scratch.clear();
                decoded.copy_to_vec_interleaved(interleaved_scratch);

                let needed = frames * channels;
                for frame_slice in interleaved_scratch
                    [..needed.min(interleaved_scratch.len())]
                    .chunks_exact(channels)
                {
                    let sum: f32 = frame_slice.iter().copied().sum();
                    push_mono_sample(pcm_ring, sum / channels as f32);
                }
                Some((sr, frames))
            }
            Self::Ac3(dec) => {
                let mut offset = 0usize;
                let mut total_frames = 0usize;
                let mut last_sr = 48_000u32;

                while offset + 7 <= pkt_data.len() {
                    let slice = &pkt_data[offset..];
                    if slice[0] != 0x0B || slice[1] != 0x77 {
                        if let Some(pos) = slice.windows(2).position(|w| w == [0x0B, 0x77]) {
                            offset += pos;
                            continue;
                        }
                        break;
                    }
                    let Ok(hdr) = filmcraft_ac3::parse_header(slice) else {
                        offset += 2;
                        continue;
                    };
                    if hdr.frame_bytes == 0 || slice.len() < hdr.frame_bytes {
                        break;
                    }
                    if let Ok(decoded) = dec.decode(&slice[..hdr.frame_bytes]) {
                        last_sr = decoded.header.sample_rate.max(8_000);
                        let ch_count = decoded.channels.len();
                        if ch_count > 0 {
                            let frames = decoded.channels[0].len();
                            for i in 0..frames {
                                let mut sum = 0.0_f32;
                                for ch in &decoded.channels {
                                    sum += ch[i];
                                }
                                push_mono_sample(pcm_ring, sum / ch_count as f32);
                            }
                            total_frames += frames;
                        }
                    }
                    offset += hdr.frame_bytes;
                }

                if total_frames > 0 {
                    Some((last_sr, total_frames))
                } else {
                    None
                }
            }
        }
    }
}

struct ContainerDecoder {
    url: String,
    format: Box<dyn FormatReader>,
    decoder: SpectrumAudioDecoder,
    track_id: u32,
    time_base: Option<TimeBase>,
    sample_rate: u32,
    decoded_sec: f64,
    pcm_ring: VecDeque<f32>,
    interleaved_scratch: Vec<f32>,
    eof: bool,
}

impl ContainerDecoder {
    fn open(
        url: &str,
        mime_type: &str,
        request_headers: &HashMap<String, String>,
        start_sec: f64,
        stop: Arc<AtomicBool>,
    ) -> Option<Self> {
        let mut hls_ext: Option<String> = None;
        let source: Box<dyn MediaSource> = if let Some(path) = local_path_from_url(url) {
            let file = File::open(path).ok()?;
            Box::new(file)
        } else if is_hls_stream(url, mime_type) {
            let hls = HlsMediaSource::open(url, request_headers, stop).ok()?;
            hls_ext = hls.first_ext.clone();
            Box::new(hls)
        } else {
            let mut http = HttpMediaSource::open(url, request_headers, Arc::clone(&stop)).ok()?;
            // Peek initial bytes without extra round-trips to auto-detect extensionless HLS playlists.
            let mut sniff = [0u8; 7];
            let sniff_n = http.read(&mut sniff).unwrap_or(0);
            if sniff[..sniff_n].starts_with(b"#EXTM3U") {
                let hls = HlsMediaSource::open(url, request_headers, stop).ok()?;
                hls_ext = hls.first_ext.clone();
                Box::new(hls)
            } else if http.seek(SeekFrom::Start(0)).is_ok() {
                Box::new(http)
            } else {
                Box::new(PrependSource::new(sniff[..sniff_n].to_vec(), http))
            }
        };

        let mut hint = if let Some(ext) = hls_ext {
            let mut h = Hint::new();
            h.with_extension(&ext);
            h
        } else {
            build_hint(url, mime_type)
        };

        let mss = MediaSourceStream::new(source, Default::default());
        let format = match symphonia::default::get_probe().probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        ) {
            Ok(fmt) => fmt,
            Err(_) => {
                // Retry with clean hint for extensionless / mislabeled streams if local/paged.
                if let Some(path) = local_path_from_url(url) {
                    let file = File::open(path).ok()?;
                    let mss2 = MediaSourceStream::new(Box::new(file), Default::default());
                    hint = Hint::new();
                    symphonia::default::get_probe()
                        .probe(
                            &hint,
                            mss2,
                            FormatOptions::default(),
                            MetadataOptions::default(),
                        )
                        .ok()?
                } else {
                    return None;
                }
            }
        };

        let track = format.default_track(TrackType::Audio)?.clone();
        let Some(CodecParameters::Audio(ref audio_params)) = track.codec_params else {
            return None;
        };

        let decoder = if audio_params.codec == CODEC_ID_AC3 || audio_params.codec == CODEC_ID_EAC3 {
            SpectrumAudioDecoder::Ac3(filmcraft_ac3::Decoder::new())
        } else {
            let inner = spectrum_codec_registry()
                .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
                .ok()?;
            SpectrumAudioDecoder::Symphonia(inner)
        };

        let sample_rate = audio_params.sample_rate.unwrap_or(48_000).max(8_000);
        let mut active = Self {
            url: url.to_string(),
            format,
            decoder,
            track_id: track.id,
            time_base: track.time_base,
            sample_rate,
            decoded_sec: 0.0,
            pcm_ring: VecDeque::with_capacity(PCM_RING_CAP),
            interleaved_scratch: Vec::with_capacity(4096),
            eof: false,
        };

        if start_sec > 0.5 {
            active.seek_to(start_sec);
        }
        Some(active)
    }

    fn seek_to(&mut self, sec: f64) {
        let Some(time) = Time::try_from_secs_f64(sec.max(0.0)) else {
            return;
        };
        if self
            .format
            .seek(
                SeekMode::Coarse,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .is_ok()
        {
            self.decoder.reset();
            self.pcm_ring.clear();
            self.decoded_sec = sec.max(0.0);
            self.eof = false;
        }
    }

    fn decode_until(&mut self, target_sec: f64, stop: &AtomicBool) {
        if self.eof || stop.load(Ordering::Relaxed) {
            return;
        }

        if target_sec > self.decoded_sec + SEEK_RESYNC_AHEAD_SEC
            || target_sec + SEEK_RESYNC_BEHIND_SEC < self.decoded_sec
        {
            self.seek_to(target_sec);
        }

        let decode_horizon = target_sec + 0.035;
        let mut packets_this_tick = 0usize;

        while self.decoded_sec < decode_horizon
            && packets_this_tick < 24
            && !stop.load(Ordering::Relaxed)
        {
            let pkt = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.eof = true;
                    break;
                }
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(SymphoniaError::IoError(ref e))
                    if e.kind() == io::ErrorKind::UnexpectedEof =>
                {
                    self.eof = true;
                    break;
                }
                Err(_) => {
                    self.eof = true;
                    break;
                }
            };

            if pkt.track_id != self.track_id {
                continue;
            }
            packets_this_tick += 1;

            if let Some(tb) = self.time_base {
                if let Some(t) = tb.calc_time(pkt.pts) {
                    let pkt_sec = t.as_secs_f64();
                    if pkt_sec.is_finite() && pkt_sec >= 0.0 {
                        self.decoded_sec = pkt_sec;
                    }
                }
            }

            if let Some((sr, frames)) = self.decoder.decode_into_ring(
                &pkt.data,
                pkt.as_packet_ref(),
                &mut self.interleaved_scratch,
                &mut self.pcm_ring,
            ) {
                self.sample_rate = sr;
                self.decoded_sec += frames as f64 / f64::from(sr);
            }
        }
    }

    fn copy_latest_window(&self, out: &mut [f32; SPECTRUM_FFT_SIZE]) {
        copy_ring_window(&self.pcm_ring, self.eof, out);
    }
}

struct PrependSource<S> {
    head: Vec<u8>,
    cursor: usize,
    inner: S,
}

impl<S> PrependSource<S> {
    fn new(head: Vec<u8>, inner: S) -> Self {
        Self {
            head,
            cursor: 0,
            inner,
        }
    }
}

impl<S: Read> Read for PrependSource<S> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.cursor < self.head.len() {
            let avail = self.head.len() - self.cursor;
            let n = out.len().min(avail);
            out[..n].copy_from_slice(&self.head[self.cursor..self.cursor + n]);
            self.cursor += n;
            return Ok(n);
        }
        self.inner.read(out)
    }
}

impl<S: Seek> Seek for PrependSource<S> {
    fn seek(&mut self, _style: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "sequential stream is not seekable",
        ))
    }
}

impl<S: MediaSource> MediaSource for PrependSource<S> {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// Raw elementary AC-3 / E-AC-3 stream reader (`.ac3`, `.ec3` files or HTTP feeds without MP4/MKV).
struct RawAc3Tap {
    url: String,
    source: Box<dyn MediaSource>,
    decoder: filmcraft_ac3::Decoder,
    sample_rate: u32,
    buf: Vec<u8>,
    pcm_ring: VecDeque<f32>,
    eof: bool,
}

impl RawAc3Tap {
    fn try_open(
        url: &str,
        mime_type: &str,
        request_headers: &HashMap<String, String>,
        stop: Arc<AtomicBool>,
    ) -> Option<Self> {
        let lower_url = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
        let lower_mime = mime_type.to_ascii_lowercase();
        let hinted_ac3 = lower_url.ends_with(".ac3")
            || lower_url.ends_with(".ec3")
            || lower_url.ends_with(".eac3")
            || lower_mime.contains("ac3");

        let mut source: Box<dyn MediaSource> = if let Some(path) = local_path_from_url(url) {
            Box::new(File::open(path).ok()?)
        } else {
            Box::new(HttpMediaSource::open(url, request_headers, stop).ok()?)
        };

        let mut head = vec![0u8; 4096];
        let n = source.read(&mut head).ok()?;
        head.truncate(n);
        let has_sync = head.windows(2).any(|w| w == [0x0B, 0x77])
            && (hinted_ac3 || filmcraft_ac3::parse_header(&head).is_ok());
        if !has_sync {
            return None;
        }

        Some(Self {
            url: url.to_string(),
            source,
            decoder: filmcraft_ac3::Decoder::new(),
            sample_rate: 48_000,
            buf: head,
            pcm_ring: VecDeque::with_capacity(PCM_RING_CAP),
            eof: false,
        })
    }

    fn read_step(&mut self, stop: &AtomicBool) {
        if self.eof || stop.load(Ordering::Relaxed) {
            return;
        }
        if self.buf.len() < 4096 {
            let mut tmp = [0u8; 4096];
            match self.source.read(&mut tmp) {
                Ok(0) => self.eof = true,
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(_) => self.eof = true,
            }
        }
        let mut consumed = 0usize;
        for _ in 0..4 {
            let slice = &self.buf[consumed..];
            if slice.len() < 7 {
                break;
            }
            if slice[0] != 0x0B || slice[1] != 0x77 {
                if let Some(pos) = slice.windows(2).position(|w| w == [0x0B, 0x77]) {
                    consumed += pos;
                    continue;
                }
                consumed += slice.len().saturating_sub(1);
                break;
            }
            let Ok(hdr) = filmcraft_ac3::parse_header(slice) else {
                consumed += 2;
                continue;
            };
            if hdr.frame_bytes == 0 || slice.len() < hdr.frame_bytes {
                break;
            }
            if let Ok(decoded) = self.decoder.decode(&slice[..hdr.frame_bytes]) {
                self.sample_rate = decoded.header.sample_rate.max(8_000);
                let ch_count = decoded.channels.len();
                if ch_count > 0 {
                    let frames = decoded.channels[0].len();
                    for i in 0..frames {
                        let mut sum = 0.0_f32;
                        for ch in &decoded.channels {
                            sum += ch[i];
                        }
                        push_mono_sample(&mut self.pcm_ring, sum / ch_count as f32);
                    }
                }
            }
            consumed += hdr.frame_bytes;
        }
        if consumed > 0 {
            self.buf.drain(..consumed.min(self.buf.len()));
        }
    }

    fn copy_latest_window(&self, out: &mut [f32; SPECTRUM_FFT_SIZE]) {
        copy_ring_window(&self.pcm_ring, self.eof, out);
    }
}

enum ActiveStreamTap {
    Container(ContainerDecoder),
    RawAc3(RawAc3Tap),
}

impl ActiveStreamTap {
    fn url(&self) -> &str {
        match self {
            Self::Container(c) => &c.url,
            Self::RawAc3(r) => &r.url,
        }
    }
}

struct SpectrumTap {
    active: Option<ActiveStreamTap>,
    failed_url: String,
    last_failed_at: Option<Instant>,
}

impl SpectrumTap {
    fn new() -> Self {
        Self {
            active: None,
            failed_url: String::new(),
            last_failed_at: None,
        }
    }

    fn read_window(
        &mut self,
        url: &str,
        mime_type: &str,
        request_headers: &HashMap<String, String>,
        target_sec: f64,
        out: &mut [f32; SPECTRUM_FFT_SIZE],
        stop: &Arc<AtomicBool>,
    ) -> u32 {
        if url.trim().is_empty() {
            self.active = None;
            out.fill(0.0);
            return 48_000;
        }

        let needs_reopen = match &self.active {
            None => true,
            Some(a) => a.url() != url,
        };

        if needs_reopen {
            self.active = None;
            if self.failed_url == url
                && self
                    .last_failed_at
                    .is_some_and(|t| t.elapsed() < OPEN_RETRY_COOLDOWN)
            {
                out.fill(0.0);
                return 48_000;
            }
            let opened = ContainerDecoder::open(
                url,
                mime_type,
                request_headers,
                target_sec,
                Arc::clone(stop),
            )
            .map(ActiveStreamTap::Container)
            .or_else(|| {
                RawAc3Tap::try_open(url, mime_type, request_headers, Arc::clone(stop))
                    .map(ActiveStreamTap::RawAc3)
            });

            match opened {
                Some(tap) => {
                    self.failed_url.clear();
                    self.last_failed_at = None;
                    self.active = Some(tap);
                }
                None => {
                    self.failed_url = url.to_string();
                    self.last_failed_at = Some(Instant::now());
                    out.fill(0.0);
                    return 48_000;
                }
            }
        }

        match self.active.as_mut() {
            Some(ActiveStreamTap::Container(dec)) => {
                dec.decode_until(target_sec, stop);
                dec.copy_latest_window(out);
                dec.sample_rate
            }
            Some(ActiveStreamTap::RawAc3(raw)) => {
                raw.read_step(stop);
                raw.copy_latest_window(out);
                raw.sample_rate
            }
            None => {
                out.fill(0.0);
                48_000
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_wave(freq_hz: f64, sr: u32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| (2.0 * PI * freq_hz * i as f64 / f64::from(sr)).sin() as f32)
            .collect()
    }

    #[test]
    fn silence_produces_zero_bins_and_rms() {
        let mut prev = [0.0_f32; SPECTRUM_BANDS];
        let frame = compute_spectrum_frame(&[0.0; SPECTRUM_FFT_SIZE], 48_000, &mut prev);
        assert_eq!(frame.bins.len(), SPECTRUM_BANDS);
        assert_eq!(frame.rms, 0.0);
        assert!(frame.bins.iter().all(|&b| b == 0.0));
    }

    #[test]
    fn sine_tones_peak_in_expected_log_bands() {
        let mut prev_low = [0.0_f32; SPECTRUM_BANDS];
        let low = compute_spectrum_frame(
            &sine_wave(180.0, 48_000, SPECTRUM_FFT_SIZE),
            48_000,
            &mut prev_low,
        );

        let mut prev_high = [0.0_f32; SPECTRUM_BANDS];
        let high = compute_spectrum_frame(
            &sine_wave(6_000.0, 48_000, SPECTRUM_FFT_SIZE),
            48_000,
            &mut prev_high,
        );

        assert_eq!(low.bins.len(), SPECTRUM_BANDS);
        assert_eq!(high.bins.len(), SPECTRUM_BANDS);
        assert!(low.rms > 0.6 && low.rms <= 1.0);
        assert!(low.bins.iter().all(|&b| (0.0..=1.0).contains(&b)));
        assert!(high.bins.iter().all(|&b| (0.0..=1.0).contains(&b)));

        let low_peak = low
            .bins
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .unwrap();
        let high_peak = high
            .bins
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            low_peak < high_peak,
            "expected low peak ({low_peak}) < high peak ({high_peak})"
        );
    }

    #[test]
    fn update_playback_tracks_pause_and_source_changes() {
        let ctrl = Arc::new(SpectrumStreamController::new());
        let mut snap = PlayerSnapshot::default();
        snap.is_playing = true;
        snap.position_ms = 2_000;
        ctrl.update_playback(&snap);
        assert!(ctrl.is_playing.load(Ordering::SeqCst));

        snap.is_playing = false;
        ctrl.update_playback(&snap);
        assert!(!ctrl.is_playing.load(Ordering::SeqCst));
    }

    #[test]
    fn local_file_url_decodes_percent_spaces() {
        #[cfg(windows)]
        {
            let p = local_path_from_url("file:///C:/My%20Music/Track%2001.flac").unwrap();
            assert_eq!(p, PathBuf::from(r"C:\My Music\Track 01.flac"));
        }
        assert!(local_path_from_url("https://example.com/audio.webm").is_none());
    }
}
