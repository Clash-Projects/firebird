use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;

use crate::audio::audio_analysis::fft;
use crate::models::player_state::PlayerSnapshot;

pub const SPECTRUM_BANDS: usize = 48;
pub const SPECTRUM_FFT_SIZE: usize = 512;
pub const SPECTRUM_INTERVAL: Duration = Duration::from_millis(22);

const MIN_FREQ_HZ: f64 = 20.0;
const MAX_FREQ_HZ: f64 = 20_000.0;
const DB_FLOOR: f64 = -64.0;
const ATTACK_ALPHA: f32 = 0.68;
const RELEASE_ALPHA: f32 = 0.28;

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
    position_ms: u64,
    speed: f64,
    updated_at: Instant,
}

impl Default for PlaybackClock {
    fn default() -> Self {
        Self {
            stream_url: String::new(),
            position_ms: 0,
            speed: 1.0,
            updated_at: Instant::now(),
        }
    }
}

impl PlaybackClock {
    fn estimated_position_sec(&self) -> f64 {
        let base = self.position_ms as f64 / 1000.0;
        let dt = self.updated_at.elapsed().as_secs_f64();
        (base + dt * self.speed.max(0.25)).max(0.0)
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

/// Manages on-demand FFT spectrum capture and throttled Tauri Channel delivery.
/// Uses OS loopback when available, falling back seamlessly to a silent headless
/// `libmpv` PCM shadow tap on macOS or when Exclusive Mode locks the OS mixer.
pub struct SpectrumStreamController {
    next_id: AtomicU64,
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

        if let Ok(mut clock) = self.clock.lock() {
            clock.stream_url = stream_url;
            clock.position_ms = snap.position_ms;
            clock.speed = if snap.speed > 0.0 { snap.speed } else { 1.0 };
            clock.updated_at = Instant::now();
        }
        self.set_playing(snap.is_playing);
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

        let stop = Arc::new(AtomicBool::new(false));
        if let Ok(mut guard) = self.stop_flag.lock() {
            *guard = Some(Arc::clone(&stop));
        }

        let ctrl = Arc::clone(self);
        std::thread::Builder::new()
            .name("lw-spectrum-fft".to_string())
            .spawn(move || {
                ctrl.run_capture_loop(&stop);
                ctrl.running.store(false, Ordering::SeqCst);
                if ctrl.is_playing.load(Ordering::SeqCst)
                    && ctrl.has_subscribers()
                    && !stop.load(Ordering::SeqCst)
                {
                    ctrl.start_worker();
                }
            })
            .ok();
    }

    fn stop_worker(&self) {
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

    fn run_capture_loop(&self, stop: &AtomicBool) {
        let mut tap = platform::PcmLoopbackTap::open();
        let mut shadow = if tap.is_active() {
            None
        } else {
            Some(shadow_mpv::ShadowMpvTap::new())
        };
        let mut ring = vec![0.0_f32; SPECTRUM_FFT_SIZE];
        let mut scratch = Vec::with_capacity(2048);
        let mut prev_bins = [0.0_f32; SPECTRUM_BANDS];

        while !stop.load(Ordering::SeqCst) && self.is_playing.load(Ordering::SeqCst) {
            let tick_start = Instant::now();
            scratch.clear();

            let sr = if let Some(ref mut shadow_tap) = shadow {
                let clock = self
                    .clock
                    .lock()
                    .map(|c| c.clone())
                    .unwrap_or_default();
                shadow_tap.read_window(&clock.stream_url, clock.estimated_position_sec(), &mut scratch)
            } else {
                tap.read_mono(&mut scratch)
            };

            if !scratch.is_empty() {
                if scratch.len() >= SPECTRUM_FFT_SIZE {
                    let start = scratch.len() - SPECTRUM_FFT_SIZE;
                    ring.copy_from_slice(&scratch[start..]);
                } else {
                    let keep = SPECTRUM_FFT_SIZE - scratch.len();
                    ring.copy_within(scratch.len().., 0);
                    ring[keep..].copy_from_slice(&scratch);
                }
            } else {
                ring.fill(0.0);
            }

            let frame = compute_spectrum_frame(&ring, sr, &mut prev_bins);
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

/// Cross-platform silent headless `libmpv` PCM shadow tap used on macOS and whenever
/// OS loopback is unavailable (e.g. WASAPI Exclusive Mode or direct ALSA hardware output).
mod shadow_mpv {
    use std::ffi::{c_char, c_int, c_void, CString};
    use std::fs::{self, File};
    use std::io::{Read, Seek, SeekFrom};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::SPECTRUM_FFT_SIZE;

    const SHADOW_SAMPLE_RATE: u32 = 22_050;
    const SEEK_RESYNC_THRESHOLD_SEC: f64 = 1.5;

    static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

    type FnMpvCreate = unsafe extern "C" fn() -> *mut c_void;
    type FnMpvInitialize = unsafe extern "C" fn(*mut c_void) -> c_int;
    type FnMpvTerminateDestroy = unsafe extern "C" fn(*mut c_void);
    type FnMpvSetPropertyString =
        unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
    type FnMpvCommand = unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int;

    struct ActiveShadow {
        _lib: libloading::Library,
        ctx: *mut c_void,
        mpv_terminate_destroy: FnMpvTerminateDestroy,
        pcm_path: PathBuf,
        url: String,
        start_sec: f64,
        last_read_sec: f64,
    }

    impl Drop for ActiveShadow {
        fn drop(&mut self) {
            if !self.ctx.is_null() {
                unsafe {
                    (self.mpv_terminate_destroy)(self.ctx);
                }
                self.ctx = std::ptr::null_mut();
            }
            let _ = fs::remove_file(&self.pcm_path);
        }
    }

    pub struct ShadowMpvTap {
        active: Option<ActiveShadow>,
    }

    impl ShadowMpvTap {
        pub fn new() -> Self {
            Self { active: None }
        }

        pub fn read_window(&mut self, url: &str, pos_sec: f64, out: &mut Vec<f32>) -> u32 {
            if url.trim().is_empty() {
                self.active = None;
                return SHADOW_SAMPLE_RATE;
            }

            let needs_reopen = match &self.active {
                None => true,
                Some(a) => {
                    a.url != url
                        || pos_sec < a.start_sec
                        || (pos_sec - a.last_read_sec).abs() > SEEK_RESYNC_THRESHOLD_SEC
                }
            };

            if needs_reopen {
                self.active = None;
                self.active = open_shadow_deck(url, pos_sec);
            }

            let Some(ref mut active) = self.active else {
                return SHADOW_SAMPLE_RATE;
            };
            active.last_read_sec = pos_sec;

            let rel_sec = (pos_sec - active.start_sec).max(0.0);
            let sample_offset = (rel_sec * f64::from(SHADOW_SAMPLE_RATE)) as u64;
            let byte_offset = sample_offset.saturating_mul(4);

            let Ok(mut file) = File::open(&active.pcm_path) else {
                return SHADOW_SAMPLE_RATE;
            };
            if file.seek(SeekFrom::Start(byte_offset)).is_err() {
                return SHADOW_SAMPLE_RATE;
            }

            let mut buf = [0u8; SPECTRUM_FFT_SIZE * 4];
            if file.read_exact(&mut buf).is_ok() {
                for chunk in buf.chunks_exact(4) {
                    out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            SHADOW_SAMPLE_RATE
        }
    }

    fn load_libmpv() -> Option<libloading::Library> {
        #[cfg(target_os = "windows")]
        let candidates = ["libmpv-2.dll", "mpv-2.dll", "libmpv.dll"];
        #[cfg(target_os = "macos")]
        let candidates = ["libmpv.2.dylib", "libmpv.dylib"];
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let candidates = ["libmpv.so.2", "libmpv.so"];

        for name in candidates {
            if let Ok(lib) = unsafe { libloading::Library::new(name) } {
                return Some(lib);
            }
        }
        None
    }

    fn open_shadow_deck(url: &str, start_sec: f64) -> Option<ActiveShadow> {
        let lib = load_libmpv()?;
        let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let pcm_path = std::env::temp_dir().join(format!(
            "lw_spectrum_{}_{}.pcm",
            std::process::id(),
            id
        ));
        let pcm_path_str = pcm_path.to_str()?;

        unsafe {
            let mpv_create: FnMpvCreate = *lib.get(b"mpv_create\0").ok()?;
            let mpv_initialize: FnMpvInitialize = *lib.get(b"mpv_initialize\0").ok()?;
            let mpv_set_prop: FnMpvSetPropertyString =
                *lib.get(b"mpv_set_property_string\0").ok()?;
            let mpv_command: FnMpvCommand = *lib.get(b"mpv_command\0").ok()?;
            let mpv_terminate_destroy: FnMpvTerminateDestroy =
                *lib.get(b"mpv_terminate_destroy\0").ok()?;

            let ctx = mpv_create();
            if ctx.is_null() {
                return None;
            }

            let set = |k: &str, v: &str| {
                if let (Ok(ck), Ok(cv)) = (CString::new(k), CString::new(v)) {
                    mpv_set_prop(ctx, ck.as_ptr(), cv.as_ptr());
                }
            };

            set("vid", "no");
            set("terminal", "no");
            set("audio-display", "no");
            set("ao", "pcm");
            set("ao-pcm-waveheader", "no");
            set("ao-pcm-file", pcm_path_str);
            set("audio-channels", "mono");
            set("audio-samplerate", &SHADOW_SAMPLE_RATE.to_string());
            set("audio-format", "float");
            if start_sec > 0.1 {
                set("start", &format!("{start_sec:.3}"));
            }

            if mpv_initialize(ctx) < 0 {
                mpv_terminate_destroy(ctx);
                let _ = fs::remove_file(&pcm_path);
                return None;
            }

            let cmd_load = CString::new("loadfile").ok()?;
            let cmd_url = CString::new(url).ok()?;
            let args = [cmd_load.as_ptr(), cmd_url.as_ptr(), std::ptr::null()];
            if mpv_command(ctx, args.as_ptr()) < 0 {
                mpv_terminate_destroy(ctx);
                let _ = fs::remove_file(&pcm_path);
                return None;
            }

            Some(ActiveShadow {
                _lib: lib,
                ctx,
                mpv_terminate_destroy,
                pcm_path,
                url: url.to_string(),
                start_sec,
                last_read_sec: start_sec,
            })
        }
    }
}

#[cfg(windows)]
mod platform {
    use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};
    use windows::Win32::Media::Audio::{
        eConsole, eRender, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
    };
    use windows::Win32::Media::KernelStreaming::{
        KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE,
    };
    use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    const WAVE_FORMAT_PCM_TAG: u16 = 1;
    const WAVE_FORMAT_IEEE_FLOAT_TAG: u16 = 3;

    enum SampleFormat {
        Float32,
        Int16,
    }

    pub struct PcmLoopbackTap {
        com_uninit: bool,
        audio_client: Option<IAudioClient>,
        capture_client: Option<IAudioCaptureClient>,
        sample_rate: u32,
        channels: usize,
        format: SampleFormat,
    }

    impl PcmLoopbackTap {
        pub fn open() -> Self {
            let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let com_uninit = hr == S_OK || hr == S_FALSE;
            if !com_uninit && hr != RPC_E_CHANGED_MODE {
                return Self::inactive(false);
            }

            match unsafe { Self::init_wasapi_loopback(com_uninit) } {
                Ok(tap) => tap,
                Err(_) => Self::inactive(com_uninit),
            }
        }

        pub fn is_active(&self) -> bool {
            self.capture_client.is_some()
        }

        fn inactive(com_uninit: bool) -> Self {
            Self {
                com_uninit,
                audio_client: None,
                capture_client: None,
                sample_rate: 48_000,
                channels: 2,
                format: SampleFormat::Float32,
            }
        }

        unsafe fn init_wasapi_loopback(com_uninit: bool) -> Result<Self, ()> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|_| ())?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|_| ())?;
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|_| ())?;
            let mix_ptr = client.GetMixFormat().map_err(|_| ())?;
            if mix_ptr.is_null() {
                return Err(());
            }

            let wfex = *mix_ptr;
            let sample_rate = wfex.nSamplesPerSec.max(8_000);
            let channels = (wfex.nChannels as usize).max(1);
            let format = detect_format(mix_ptr, &wfex);

            let buffer_duration_100ns = 1_000_000;
            let init_res = client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                buffer_duration_100ns,
                0,
                mix_ptr,
                None,
            );
            CoTaskMemFree(Some(mix_ptr.cast()));
            init_res.map_err(|_| ())?;

            let capture: IAudioCaptureClient = client.GetService().map_err(|_| ())?;
            client.Start().map_err(|_| ())?;

            Ok(Self {
                com_uninit,
                audio_client: Some(client),
                capture_client: Some(capture),
                sample_rate,
                channels,
                format,
            })
        }

        pub fn read_mono(&mut self, out: &mut Vec<f32>) -> u32 {
            let Some(ref capture) = self.capture_client else {
                return self.sample_rate;
            };

            unsafe {
                loop {
                    let Ok(packet_frames) = capture.GetNextPacketSize() else {
                        break;
                    };
                    if packet_frames == 0 {
                        break;
                    }

                    let mut data_ptr = std::ptr::null_mut();
                    let mut frames_available = 0u32;
                    let mut flags = 0u32;
                    if capture
                        .GetBuffer(
                            &mut data_ptr,
                            &mut frames_available,
                            &mut flags,
                            None,
                            None,
                        )
                        .is_err()
                    {
                        break;
                    }

                    let frames = frames_available as usize;
                    if (flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)) != 0 || data_ptr.is_null() {
                        out.resize(out.len() + frames, 0.0);
                    } else {
                        match self.format {
                            SampleFormat::Float32 => {
                                let total = frames * self.channels;
                                let slice =
                                    std::slice::from_raw_parts(data_ptr.cast::<f32>(), total);
                                for frame in slice.chunks_exact(self.channels) {
                                    let sum: f32 = frame.iter().copied().sum();
                                    out.push(sum / self.channels as f32);
                                }
                            }
                            SampleFormat::Int16 => {
                                let total = frames * self.channels;
                                let slice =
                                    std::slice::from_raw_parts(data_ptr.cast::<i16>(), total);
                                for frame in slice.chunks_exact(self.channels) {
                                    let sum: f32 =
                                        frame.iter().map(|&s| f32::from(s) / 32768.0).sum();
                                    out.push(sum / self.channels as f32);
                                }
                            }
                        }
                    }

                    let _ = capture.ReleaseBuffer(frames_available);
                }
            }

            self.sample_rate
        }
    }

    impl Drop for PcmLoopbackTap {
        fn drop(&mut self) {
            if let Some(ref client) = self.audio_client {
                unsafe {
                    let _ = client.Stop();
                }
            }
            self.capture_client = None;
            self.audio_client = None;
            if self.com_uninit {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }

    unsafe fn detect_format(ptr: *const WAVEFORMATEX, wfex: &WAVEFORMATEX) -> SampleFormat {
        if wfex.wFormatTag == WAVE_FORMAT_IEEE_FLOAT_TAG {
            return SampleFormat::Float32;
        }
        if wfex.wFormatTag == WAVE_FORMAT_PCM_TAG && wfex.wBitsPerSample == 16 {
            return SampleFormat::Int16;
        }
        if u32::from(wfex.wFormatTag) == WAVE_FORMAT_EXTENSIBLE {
            let ext = &*(ptr.cast::<WAVEFORMATEXTENSIBLE>());
            let sub = ext.SubFormat;
            if sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
                return SampleFormat::Float32;
            }
            if sub == KSDATAFORMAT_SUBTYPE_PCM && wfex.wBitsPerSample == 16 {
                return SampleFormat::Int16;
            }
        }
        SampleFormat::Float32
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::io::Read;
    use std::process::{Child, Command, Stdio};

    const LINUX_SAMPLE_RATE: u32 = 48_000;

    pub struct PcmLoopbackTap {
        child: Option<Child>,
    }

    impl PcmLoopbackTap {
        pub fn open() -> Self {
            let child = Command::new("parec")
                .args([
                    "--raw",
                    "--format=float32le",
                    "--rate=48000",
                    "--channels=1",
                    "--latency-msec=20",
                    "--device=@DEFAULT_MONITOR@",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok();
            Self { child }
        }

        pub fn is_active(&self) -> bool {
            self.child.is_some()
        }

        pub fn read_mono(&mut self, out: &mut Vec<f32>) -> u32 {
            let Some(ref mut child) = self.child else {
                return LINUX_SAMPLE_RATE;
            };
            let Some(ref mut stdout) = child.stdout else {
                return LINUX_SAMPLE_RATE;
            };
            let mut buf = [0u8; 512 * 4];
            if stdout.read_exact(&mut buf).is_ok() {
                for chunk in buf.chunks_exact(4) {
                    out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            LINUX_SAMPLE_RATE
        }
    }

    impl Drop for PcmLoopbackTap {
        fn drop(&mut self) {
            if let Some(ref mut child) = self.child {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    pub struct PcmLoopbackTap;

    impl PcmLoopbackTap {
        pub fn open() -> Self {
            Self
        }

        pub fn is_active(&self) -> bool {
            false
        }

        pub fn read_mono(&mut self, _out: &mut Vec<f32>) -> u32 {
            48_000
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
}
