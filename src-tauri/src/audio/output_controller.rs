use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use super::format_negotiator::{BitPerfectEvalInput, FormatNegotiator};
use super::headphone_profiles::HeadphoneProfiles;
use super::wasapi_engine::{
    wasapi_log, BoxFuture, WasapiBackend, WasapiDeviceEvent, WasapiEngine, HOTPLUG_DEBOUNCE,
};
use crate::models::dac_device::DacDevice;
use crate::models::output_path_status::{BitPerfectReason, FormatDecision, OutputPathStatus, WasapiShareMode};
use crate::models::pcm_format::{pcm_from_stream, PcmFormat};
use crate::models::player_state::PlayerSnapshot;
use crate::models::stream_models::ResolvedStream;
use crate::storage::prefs::Prefs;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioOutputState {
    #[serde(default)]
    pub devices: Vec<DacDevice>,
    #[serde(default)]
    pub selected_id: String,
    #[serde(default)]
    pub exclusive_requested: bool,
    #[serde(default)]
    pub bit_perfect_requested: bool,
    #[serde(default)]
    pub path: OutputPathStatus,
    #[serde(default)]
    pub last_opened_format: Option<PcmFormat>,
    #[serde(default)]
    pub probing: bool,
}

impl AudioOutputState {
    pub fn selected(&self) -> Option<&DacDevice> {
        if self.devices.is_empty() {
            return None;
        }
        if self.selected_id.is_empty() {
            if let Some(def) = self.devices.iter().find(|d| d.is_default) {
                return Some(def);
            }
            return self.devices.first();
        }
        self.devices.iter().find(|d| d.id == self.selected_id)
    }
}

/// Bridge to the playback engine (`PlaybackService`) consumed by [`AudioOutputController`].
pub trait PlaybackOutputSink: Send + Sync {
    fn processing_blocks_exclusive(&self) -> bool;
    fn exclusive_applied(&self) -> bool;
    fn wasapi_error(&self) -> Option<String>;
    fn snapshot(&self) -> PlayerSnapshot;
    fn configure_wasapi<'a>(
        &'a self,
        exclusive: bool,
        mpv_device: &'a str,
        lock_software_volume: bool,
        output_format: Option<PcmFormat>,
    ) -> BoxFuture<'a, ()>;
    fn set_volume<'a>(&'a self, volume: f64, software: bool) -> BoxFuture<'a, ()>;
    fn unlock_software_volume(&self) -> BoxFuture<'_, ()>;
    fn pause(&self) -> BoxFuture<'_, ()>;
}

/// Default no-op sink used before a playback service is wired in.
#[derive(Debug, Default)]
pub struct NoopPlaybackSink;

impl PlaybackOutputSink for NoopPlaybackSink {
    fn processing_blocks_exclusive(&self) -> bool {
        false
    }

    fn exclusive_applied(&self) -> bool {
        false
    }

    fn wasapi_error(&self) -> Option<String> {
        None
    }

    fn snapshot(&self) -> PlayerSnapshot {
        PlayerSnapshot::default()
    }

    fn configure_wasapi<'a>(
        &'a self,
        _exclusive: bool,
        _mpv_device: &'a str,
        _lock_software_volume: bool,
        _output_format: Option<PcmFormat>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    fn set_volume<'a>(&'a self, _volume: f64, _software: bool) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    fn unlock_software_volume(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }

    fn pause(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

struct ControllerInner {
    state: AudioOutputState,
    attached: bool,
    last_logged: Option<String>,
    // Fingerprint of every input rebuild_path consumes, so position-only
    // ticks can skip it.
    last_exclusive_applied: Option<bool>,
    last_wasapi_error: Option<String>,
    last_device_id: Option<String>,
    last_exclusive_requested: Option<bool>,
    last_crossfade: Option<bool>,
    pre_mute_volume: f64,
    pending_hotplug_events: Vec<WasapiDeviceEvent>,
    hotplug_debounce_task: Option<JoinHandle<()>>,
    hotplug_listener_task: Option<JoinHandle<()>>,
}

/// Owns WASAPI device selection, exclusive-mode mpv properties, hardware
/// volume, and the bit-perfect status machine.
pub struct AudioOutputController {
    prefs: Arc<Prefs>,
    engine: Arc<dyn WasapiBackend>,
    playback: Arc<dyn PlaybackOutputSink>,
    negotiator: FormatNegotiator,
    inner: Mutex<ControllerInner>,
}

impl AudioOutputController {
    pub fn new(prefs: Arc<Prefs>) -> Self {
        Self::with_backends(
            prefs,
            Arc::new(WasapiEngine::new()),
            Arc::new(NoopPlaybackSink),
        )
    }

    pub fn with_backends(
        prefs: Arc<Prefs>,
        engine: Arc<dyn WasapiBackend>,
        playback: Arc<dyn PlaybackOutputSink>,
    ) -> Self {
        let exclusive = prefs.wasapi_exclusive() || prefs.bit_perfect();
        let initial_state = AudioOutputState {
            selected_id: prefs.audio_device_id(),
            exclusive_requested: exclusive,
            bit_perfect_requested: exclusive,
            ..Default::default()
        };
        Self {
            prefs,
            engine,
            playback,
            negotiator: FormatNegotiator,
            inner: Mutex::new(ControllerInner {
                state: initial_state,
                attached: false,
                last_logged: None,
                last_exclusive_applied: None,
                last_wasapi_error: None,
                last_device_id: None,
                last_exclusive_requested: None,
                last_crossfade: None,
                pre_mute_volume: 1.0,
                pending_hotplug_events: Vec::new(),
                hotplug_debounce_task: None,
                hotplug_listener_task: None,
            }),
        }
    }

    pub fn state(&self) -> AudioOutputState {
        self.inner
            .lock()
            .map(|g| g.state.clone())
            .unwrap_or_default()
    }

    pub fn dispose(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(handle) = guard.hotplug_debounce_task.take() {
                handle.abort();
            }
            if let Some(handle) = guard.hotplug_listener_task.take() {
                handle.abort();
            }
            guard.pending_hotplug_events.clear();
            guard.attached = false;
        }
        self.engine.stop_watching();
    }

    pub async fn attach(self: &Arc<Self>) {
        {
            let Ok(mut guard) = self.inner.lock() else {
                return;
            };
            if guard.attached {
                return;
            }
            guard.attached = true;
        }

        self.refresh_devices().await;
        if let Err(e) = self.engine.start_watching() {
            wasapi_log(&format!("startWatching failed: {e}"));
        } else {
            let mut rx = self.engine.subscribe_events();
            let this = Arc::clone(self);
            let listener = tokio::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    this.on_hotplug(event);
                }
            });
            if let Ok(mut guard) = self.inner.lock() {
                guard.hotplug_listener_task = Some(listener);
            }
        }

        self.apply_to_player().await;
        self.rebuild_path(None);
    }

    pub async fn on_playback_changed(
        &self,
        prev: Option<&PlayerSnapshot>,
        next: &PlayerSnapshot,
    ) {
        if let Some(p) = prev {
            if p.stream == next.stream
                && (p.volume - next.volume).abs() < f64::EPSILON
                && (p.speed - next.speed).abs() < f64::EPSILON
                && (p.pitch - next.pitch).abs() < f64::EPSILON
                && p.output_format == next.output_format
                && p.output_is_float == next.output_is_float
                && self.path_inputs_unchanged()
            {
                return;
            }
        }

        let prev_fmt = Self::source_of(prev.and_then(|p| p.stream.as_ref()));
        let next_fmt = Self::source_of(next.stream.as_ref());
        let (exclusive_requested, selected_device) = {
            let state = self.state();
            (state.exclusive_requested, state.selected().cloned())
        };

        if let Some(next_f) = next_fmt {
            if exclusive_requested {
                let changed = prev_fmt.map(|p| !p.matches(&next_f)).unwrap_or(true);
                if changed {
                    wasapi_log("Format change detected");
                    if let Some(p) = prev_fmt {
                        wasapi_log(&format!("Previous: {}", p.label()));
                    }
                    wasapi_log(&format!("Requested: {}", next_f.label()));
                    if let Some(dev) = selected_device.as_ref().filter(|d| d.supports(&next_f)) {
                        let _ = dev;
                        wasapi_log("DAC supports requested format");
                        wasapi_log("Reinitializing exclusive stream");
                    } else {
                        wasapi_log(&format!(
                            "DAC does not list {} as exclusive PCM",
                            next_f.label()
                        ));
                    }
                    self.apply_to_player().await;
                }
            }
        }

        let prev_vol = prev.map(|p| p.volume).unwrap_or(next.volume);
        if (next.volume - prev_vol).abs() >= f64::EPSILON {
            let device = self.state().selected().cloned();
            if exclusive_requested
                && !self.playback.processing_blocks_exclusive()
                && device.as_ref().is_some_and(|d| d.hardware_volume)
            {
                if let Some(dev) = device {
                    let _ = self.engine.set_hardware_volume(&dev.id, next.volume).await;
                }
            }
        }

        self.rebuild_path(Some(next));
    }

    pub async fn refresh_devices(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.state.probing = true;
        }
        let devices = self.engine.enumerate_devices().await;
        if let Ok(mut guard) = self.inner.lock() {
            let mut selected = guard.state.selected_id.clone();
            if !selected.is_empty()
                && !devices.is_empty()
                && devices.iter().all(|d| d.id != selected)
            {
                selected.clear();
            }
            guard.state.devices = devices;
            guard.state.selected_id = selected;
            guard.state.probing = false;
        }
        self.rebuild_path(None);
        self.log_caps();
    }

    pub async fn select_device(&self, id: &str) {
        self.prefs.set_audio_device_id(id);
        let name = {
            let Ok(mut guard) = self.inner.lock() else {
                return;
            };
            guard.state.selected_id = id.to_string();
            guard
                .state
                .selected()
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "System default".to_string())
        };
        wasapi_log(&format!("Device: {name}"));
        self.apply_to_player().await;
        self.rebuild_path(None);
    }

    pub async fn set_exclusive(&self, exclusive: bool) {
        self.prefs.set_wasapi_exclusive(exclusive);
        self.prefs.set_bit_perfect(exclusive);
        if let Ok(mut guard) = self.inner.lock() {
            guard.state.exclusive_requested = exclusive;
            guard.state.bit_perfect_requested = exclusive;
        }
        wasapi_log(&format!("Exclusive Mode: {exclusive}"));
        self.apply_to_player().await;
        self.rebuild_path(None);
    }

    pub async fn set_bit_perfect(&self, enabled: bool) {
        self.set_exclusive(enabled).await;
    }

    pub fn refresh_path(&self) {
        self.rebuild_path(None);
    }

    /// Drops or restores WASAPI exclusive when DSP, loudness, pitch,
    /// speed, crossfade, or timed AutoMix changes. Exclusive stays off
    /// while any of those are active.
    pub async fn sync_exclusive_gate(&self) {
        let want = self.state().exclusive_requested && !self.playback.processing_blocks_exclusive();
        if self.playback.exclusive_applied() == want {
            self.rebuild_path(None);
            return;
        }
        self.apply_to_player().await;
        self.rebuild_path(None);
    }

    pub async fn apply_to_player(&self) {
        let state = self.state();
        let device = state.selected().cloned();
        let exclusive =
            state.exclusive_requested && !self.playback.processing_blocks_exclusive();
        let hw = exclusive && device.as_ref().is_some_and(|d| d.hardware_volume);
        let snap = self.playback.snapshot();
        let source = Self::source_of(snap.stream.as_ref());
        let decision =
            source.map(|s| self.negotiator.negotiate(s, device.as_ref(), exclusive));

        // When using System Default in shared mode, leave mpv on `auto` instead
        // of pinning the current default endpoint's concrete WASAPI GUID.
        let want_auto = state.selected_id.is_empty() && !exclusive;
        let mpv_device = if want_auto {
            "auto".to_string()
        } else {
            device
                .as_ref()
                .map(DacDevice::mpv_device_name)
                .unwrap_or_else(|| "auto".to_string())
        };
        let output_format = if exclusive {
            decision.as_ref().and_then(|d| d.output)
        } else {
            None
        };
        self.playback
            .configure_wasapi(exclusive, &mpv_device, hw, output_format)
            .await;

        let vol = self.playback.snapshot().volume;
        if hw {
            let dev_id = device.as_ref().map(|d| d.id.as_str()).unwrap_or("");
            let mut ok = self.engine.set_hardware_volume(dev_id, vol).await;
            if !ok {
                tokio::time::sleep(Duration::from_millis(100)).await;
                ok = self.engine.set_hardware_volume(dev_id, vol).await;
            }
            if ok {
                self.playback.set_volume(vol, false).await;
                wasapi_log("Hardware Volume: TRUE");
            } else {
                wasapi_log("Hardware volume write failed — falling back to software volume");
                self.playback.unlock_software_volume().await;
                self.playback.set_volume(vol, true).await;
            }
        } else {
            self.playback.unlock_software_volume().await;
            self.playback.set_volume(vol, true).await;
            wasapi_log("Hardware Volume: FALSE");
        }
    }

    pub async fn set_volume(&self, volume: f64) {
        if volume > 0.0 {
            if let Ok(mut guard) = self.inner.lock() {
                guard.pre_mute_volume = volume.clamp(0.0, 1.0);
            }
        }
        let state = self.state();
        let device = state.selected().cloned();
        let exclusive_live =
            state.exclusive_requested && !self.playback.processing_blocks_exclusive();
        let hw = exclusive_live && device.as_ref().is_some_and(|d| d.hardware_volume);

        if hw {
            let dev_id = device.as_ref().map(|d| d.id.as_str()).unwrap_or("");
            let ok = self.engine.set_hardware_volume(dev_id, volume).await;
            if ok {
                self.playback.set_volume(volume, false).await;
                return;
            }
            // Endpoint refused the hardware write: don't leave the slider dead.
            wasapi_log("Hardware volume write failed — using software volume");
            self.playback.unlock_software_volume().await;
        } else if exclusive_live {
            self.playback.unlock_software_volume().await;
        }
        self.playback.set_volume(volume, true).await;
        self.rebuild_path(None);
    }

    /// Mute toggle: mute remembers the current level, unmute restores it
    /// (never jumps to max).
    pub async fn toggle_mute(&self) {
        let current = self.playback.snapshot().volume;
        if current <= 0.0 {
            let target = self
                .inner
                .lock()
                .map(|g| if g.pre_mute_volume > 0.0 { g.pre_mute_volume } else { 1.0 })
                .unwrap_or(1.0);
            self.set_volume(target).await;
        } else {
            if let Ok(mut guard) = self.inner.lock() {
                guard.pre_mute_volume = current;
            }
            self.set_volume(0.0).await;
        }
    }

    pub fn on_hotplug(self: &Arc<Self>, event: WasapiDeviceEvent) {
        self.on_hotplug_with_delay(event, HOTPLUG_DEBOUNCE);
    }

    pub fn on_hotplug_with_delay(
        self: &Arc<Self>,
        event: WasapiDeviceEvent,
        debounce: Duration,
    ) {
        wasapi_log(&format!("Device event: {} {}", event.reason, event.id));
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        guard.pending_hotplug_events.push(event);
        if let Some(prev_timer) = guard.hotplug_debounce_task.take() {
            prev_timer.abort();
        }
        let this = Arc::clone(self);
        guard.hotplug_debounce_task = Some(tokio::spawn(async move {
            tokio::time::sleep(debounce).await;
            let (events, selected) = {
                let Ok(mut g) = this.inner.lock() else {
                    return;
                };
                let drained = std::mem::take(&mut g.pending_hotplug_events);
                let sel = g.state.selected_id.clone();
                (drained, sel)
            };
            this.refresh_devices().await;
            let was_removed = events
                .iter()
                .any(|e| e.reason == "removed" && (selected.is_empty() || selected == e.id));
            if was_removed {
                this.playback.pause().await;
                if let Ok(mut g) = this.inner.lock() {
                    g.state.path.reason = BitPerfectReason::DeviceUnavailable;
                    g.state.path.error = Some(if !selected.is_empty() {
                        "DAC disconnected".to_string()
                    } else {
                        "Audio device disconnected".to_string()
                    });
                }
            }
            let should_apply = events.iter().any(|e| {
                matches!(
                    e.reason.as_str(),
                    "added" | "default" | "state" | "property"
                )
            });
            if should_apply || was_removed {
                this.apply_to_player().await;
            }
        }));
    }

    fn path_inputs_unchanged(&self) -> bool {
        let Ok(guard) = self.inner.lock() else {
            return false;
        };
        let selected_id = guard.state.selected().map(|d| d.id.as_str()).unwrap_or("");
        guard.last_exclusive_applied == Some(self.playback.exclusive_applied())
            && guard.last_wasapi_error == self.playback.wasapi_error()
            && guard.last_device_id.as_deref().unwrap_or("") == selected_id
            && guard.last_exclusive_requested == Some(guard.state.exclusive_requested)
            && guard.last_crossfade == Some(self.prefs.crossfade_enabled())
    }

    pub fn source_of(stream: Option<&ResolvedStream>) -> Option<PcmFormat> {
        let s = stream?;
        // Opus is always decoded at 48 kHz, whatever rate the container
        // advertises (YouTube reports 44.1 kHz for the same stream).
        let opus = s.audio_codec.to_ascii_lowercase().contains("opus");
        Some(pcm_from_stream(
            s.bit_depth,
            if opus { 48.0 } else { s.sampling_rate_khz },
            2,
        ))
    }

    fn rebuild_path(&self, snapshot: Option<&PlayerSnapshot>) {
        let owned_snap;
        let snap = match snapshot {
            Some(s) => s,
            None => {
                owned_snap = self.playback.snapshot();
                &owned_snap
            }
        };

        let (device, exclusive_requested) = {
            let state = self.state();
            (state.selected().cloned(), state.exclusive_requested)
        };
        let source = Self::source_of(snap.stream.as_ref());
        let live = snap.output_format;
        let mut decision = source.map(|s| {
            self.negotiator
                .negotiate(s, device.as_ref(), exclusive_requested)
        });

        if exclusive_requested && snap.output_is_float {
            if let Some(s) = source {
                decision = Some(FormatDecision {
                    source: s,
                    output: live.or_else(|| decision.as_ref().and_then(|d| d.output)),
                    native_match: false,
                    resampling: true,
                    format_conversion: true,
                    note: "Output is float — not bit-perfect".to_string(),
                });
            }
        } else if exclusive_requested {
            if let (Some(l), Some(s), Some(d)) = (live, source, decision.as_ref()) {
                if d.native_match && l.sample_rate_hz != s.sample_rate_hz {
                    decision = Some(FormatDecision {
                        source: s,
                        output: Some(l),
                        native_match: false,
                        resampling: true,
                        format_conversion: false,
                        note: format!("AO sample rate {} ≠ source {}", l.label(), s.label()),
                    });
                }
            }
        }

        let hw = exclusive_requested && device.as_ref().is_some_and(|d| d.hardware_volume);
        let software_vol = !hw && snap.volume < 0.999;
        let blocked = self.playback.processing_blocks_exclusive();
        let wasapi_error = self.playback.wasapi_error();
        let exclusive_applied = self.playback.exclusive_applied();
        let exclusive_active = exclusive_requested
            && !blocked
            && exclusive_applied
            && wasapi_error.is_none()
            && device.as_ref().is_some_and(|d| d.exclusive_supported)
            && decision.as_ref().and_then(|d| d.output).is_some();

        let peq = self.prefs.equalizer_enabled() || self.prefs.studio_clarity_enabled();
        let headphone = HeadphoneProfiles::by_id(Some(&self.prefs.headphone_profile())).id != "off";
        let loudness_on = self.prefs.loudness_mode() != "off";
        let auto_mix = self.prefs.auto_mix_enabled();
        let crossfade = self.prefs.crossfade_enabled();

        let fallback_decision = FormatDecision {
            source: source.unwrap_or_else(|| PcmFormat::new(44100, 16, 2)),
            output: live.or(source),
            native_match: false,
            resampling: true,
            format_conversion: true,
            note: String::new(),
        };
        let eval_decision = decision.as_ref().unwrap_or(&fallback_decision);
        let eval_input = BitPerfectEvalInput {
            decision: eval_decision,
            exclusive_requested,
            exclusive_active,
            device_available: device.is_some() || !self.engine.is_supported(),
            dsp_active: headphone,
            peq_active: peq,
            software_volume: software_vol,
            crossfade,
            speed: snap.speed,
            loudness: loudness_on,
            auto_mix,
            pitch: snap.pitch,
        };
        let reason = self.negotiator.evaluate(&eval_input);

        // Exclusive DAC format is the negotiated integer PCM — never the
        // decoder's s32/float container that libmpv reports as audio-params.
        let output = if exclusive_requested {
            decision.as_ref().and_then(|d| d.output).or(live).or(source)
        } else {
            live.or(source)
        };

        let path = OutputPathStatus {
            device_name: device
                .as_ref()
                .map(DacDevice::display_name)
                .unwrap_or_else(|| "System default".to_string()),
            device_id: device
                .as_ref()
                .map(|d| d.id.clone())
                .unwrap_or_default(),
            mode: if exclusive_active {
                WasapiShareMode::Exclusive
            } else {
                WasapiShareMode::Shared
            },
            exclusive_requested,
            exclusive_active,
            source,
            output,
            resampling: decision.as_ref().is_some_and(|d| d.resampling),
            dsp_active: headphone || loudness_on || auto_mix,
            peq_active: peq,
            software_volume: software_vol,
            hardware_volume: hw,
            crossfade,
            speed: snap.speed,
            reason,
            error: wasapi_error.clone(),
        };

        let line = format!(
            "Device: {} Exclusive: {} Source: {} Output: {} Resampling: {} DSP: {} Software Volume: {} Hardware Volume: {} Bit-Perfect: {}",
            path.device_name,
            path.exclusive_active,
            path.source_label(),
            path.output_label(),
            path.resampling,
            path.dsp_active,
            path.software_volume,
            path.hardware_volume,
            path.bit_perfect(),
        );

        let should_log = if let Ok(mut guard) = self.inner.lock() {
            guard.state.path = path;
            if let Some(src) = source {
                guard.state.last_opened_format = Some(src);
            }
            guard.last_exclusive_applied = Some(exclusive_applied);
            guard.last_wasapi_error = wasapi_error;
            guard.last_device_id = Some(
                guard
                    .state
                    .selected()
                    .map(|d| d.id.clone())
                    .unwrap_or_default(),
            );
            guard.last_exclusive_requested = Some(exclusive_requested);
            guard.last_crossfade = Some(crossfade);

            if guard.last_logged.as_deref() != Some(line.as_str()) {
                guard.last_logged = Some(line.clone());
                true
            } else {
                false
            }
        } else {
            false
        };

        if should_log {
            wasapi_log(&line);
            if let Some(out) = output {
                wasapi_log(&format!("New format active: {}", out.label()));
            }
        }
    }

    fn log_caps(&self) {
        let state = self.state();
        let Some(device) = state.selected() else {
            return;
        };
        wasapi_log(&format!("Device: {}", device.display_name()));
        wasapi_log(&format!("Exclusive Mode: {}", device.exclusive_supported));
        wasapi_log(&format!("Hardware Volume: {}", device.hardware_volume));
        for line in device.supported_summary().lines() {
            wasapi_log(&format!("PCM {line}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio::sync::broadcast;

    use super::{AudioOutputController, PlaybackOutputSink};
    use crate::audio::wasapi_engine::{BoxFuture, WasapiBackend, WasapiDeviceEvent};
    use crate::models::dac_device::DacDevice;
    use crate::models::output_path_status::{BitPerfectReason, WasapiShareMode};
    use crate::models::pcm_format::PcmFormat;
    use crate::models::player_state::PlayerSnapshot;
    use crate::models::stream_models::ResolvedStream;
    use crate::storage::prefs::Prefs;

    struct MockWasapi {
        devices: Mutex<Vec<DacDevice>>,
        events_tx: broadcast::Sender<WasapiDeviceEvent>,
        fail_hw_volume: AtomicBool,
        last_hw_volume: Mutex<Option<(String, f64)>>,
    }

    impl MockWasapi {
        fn new(devices: Vec<DacDevice>) -> Self {
            let (events_tx, _) = broadcast::channel(16);
            Self {
                devices: Mutex::new(devices),
                events_tx,
                fail_hw_volume: AtomicBool::new(false),
                last_hw_volume: Mutex::new(None),
            }
        }
    }

    impl WasapiBackend for MockWasapi {
        fn is_supported(&self) -> bool {
            true
        }

        fn subscribe_events(&self) -> broadcast::Receiver<WasapiDeviceEvent> {
            self.events_tx.subscribe()
        }

        fn start_watching(&self) -> Result<(), String> {
            Ok(())
        }

        fn stop_watching(&self) {}

        fn enumerate_devices<'a>(&'a self) -> BoxFuture<'a, Vec<DacDevice>> {
            let list = self.devices.lock().unwrap().clone();
            Box::pin(async move { list })
        }

        fn probe_device<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Option<DacDevice>> {
            let found = self
                .devices
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.id == id)
                .cloned();
            Box::pin(async move { found })
        }

        fn is_format_supported<'a>(
            &'a self,
            id: &'a str,
            sample_rate: i32,
            channels: i32,
            bit_depth: i32,
        ) -> BoxFuture<'a, bool> {
            let fmt = PcmFormat::new(sample_rate, bit_depth, channels);
            let ok = self
                .devices
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.id == id)
                .is_some_and(|d| d.supports(&fmt));
            Box::pin(async move { ok })
        }

        fn set_hardware_volume<'a>(&'a self, id: &'a str, scalar: f64) -> BoxFuture<'a, bool> {
            let fail = self.fail_hw_volume.load(Ordering::Relaxed);
            if !fail {
                *self.last_hw_volume.lock().unwrap() = Some((id.to_string(), scalar));
            }
            Box::pin(async move { !fail })
        }

        fn get_hardware_volume<'a>(&'a self, _id: &'a str) -> BoxFuture<'a, Option<f64>> {
            let v = self.last_hw_volume.lock().unwrap().as_ref().map(|x| x.1);
            Box::pin(async move { v })
        }

        fn default_device_id<'a>(&'a self) -> BoxFuture<'a, String> {
            let id = self
                .devices
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.is_default)
                .map(|d| d.id.clone())
                .unwrap_or_default();
            Box::pin(async move { id })
        }
    }

    struct MockPlayback {
        snap: Mutex<PlayerSnapshot>,
        exclusive_applied: AtomicBool,
        blocks_exclusive: AtomicBool,
        wasapi_error: Mutex<Option<String>>,
        software_volume_locked: AtomicBool,
        pause_count: AtomicUsize,
        last_configure: Mutex<Option<(bool, String, bool, Option<PcmFormat>)>>,
    }

    impl MockPlayback {
        fn new(snap: PlayerSnapshot) -> Self {
            Self {
                snap: Mutex::new(snap),
                exclusive_applied: AtomicBool::new(false),
                blocks_exclusive: AtomicBool::new(false),
                wasapi_error: Mutex::new(None),
                software_volume_locked: AtomicBool::new(false),
                pause_count: AtomicUsize::new(0),
                last_configure: Mutex::new(None),
            }
        }
    }

    impl PlaybackOutputSink for MockPlayback {
        fn processing_blocks_exclusive(&self) -> bool {
            self.blocks_exclusive.load(Ordering::Relaxed)
        }

        fn exclusive_applied(&self) -> bool {
            self.exclusive_applied.load(Ordering::Relaxed)
        }

        fn wasapi_error(&self) -> Option<String> {
            self.wasapi_error.lock().unwrap().clone()
        }

        fn snapshot(&self) -> PlayerSnapshot {
            self.snap.lock().unwrap().clone()
        }

        fn configure_wasapi<'a>(
            &'a self,
            exclusive: bool,
            mpv_device: &'a str,
            lock_software_volume: bool,
            output_format: Option<PcmFormat>,
        ) -> BoxFuture<'a, ()> {
            self.exclusive_applied.store(exclusive, Ordering::Relaxed);
            self.software_volume_locked
                .store(lock_software_volume, Ordering::Relaxed);
            *self.last_configure.lock().unwrap() = Some((
                exclusive,
                mpv_device.to_string(),
                lock_software_volume,
                output_format,
            ));
            Box::pin(async {})
        }

        fn set_volume<'a>(&'a self, volume: f64, _software: bool) -> BoxFuture<'a, ()> {
            self.snap.lock().unwrap().volume = volume;
            Box::pin(async {})
        }

        fn unlock_software_volume(&self) -> BoxFuture<'_, ()> {
            self.software_volume_locked.store(false, Ordering::Relaxed);
            Box::pin(async {})
        }

        fn pause(&self) -> BoxFuture<'_, ()> {
            self.pause_count.fetch_add(1, Ordering::Relaxed);
            Box::pin(async {})
        }
    }

    fn hi_res_dac() -> DacDevice {
        DacDevice {
            id: "{dac-1}".to_string(),
            name: "Mojo 2".to_string(),
            is_default: true,
            exclusive_supported: true,
            hardware_volume: true,
            formats: vec![
                PcmFormat::new(44100, 16, 2),
                PcmFormat::new(48000, 24, 2),
                PcmFormat::new(96000, 24, 2),
            ],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn exclusive_bit_perfect_and_auto_mpv_device_rule() {
        let prefs = Arc::new(Prefs::in_memory());
        let engine = Arc::new(MockWasapi::new(vec![hi_res_dac()]));
        let snap = PlayerSnapshot {
            stream: Some(ResolvedStream {
                audio_codec: "flac".to_string(),
                bit_depth: 24,
                sampling_rate_khz: 96.0,
                ..Default::default()
            }),
            volume: 0.75,
            ..Default::default()
        };
        let playback = Arc::new(MockPlayback::new(snap));
        let ctrl = Arc::new(AudioOutputController::with_backends(
            Arc::clone(&prefs),
            engine.clone(),
            playback.clone(),
        ));

        ctrl.attach().await;
        // Shared mode with System Default leaves mpv on `auto`.
        let cfg_shared = playback.last_configure.lock().unwrap().clone().unwrap();
        assert!(!cfg_shared.0);
        assert_eq!(cfg_shared.1, "auto");
        assert_eq!(ctrl.state().path.mode, WasapiShareMode::Shared);
        assert_eq!(ctrl.state().path.reason, BitPerfectReason::SharedMode);

        // Enabling exclusive pins the concrete `wasapi/{id}` device and locks software volume.
        ctrl.set_exclusive(true).await;
        let cfg_excl = playback.last_configure.lock().unwrap().clone().unwrap();
        assert!(cfg_excl.0);
        assert_eq!(cfg_excl.1, "wasapi/{dac-1}");
        assert!(cfg_excl.2);
        assert_eq!(cfg_excl.3, Some(PcmFormat::new(96000, 24, 2)));
        assert_eq!(ctrl.state().path.mode, WasapiShareMode::Exclusive);
        assert_eq!(ctrl.state().path.reason, BitPerfectReason::BitPerfect);
        assert!(ctrl.state().path.bit_perfect());

        // Hardware volume write failure unlocks software volume fallback.
        engine.fail_hw_volume.store(true, Ordering::Relaxed);
        ctrl.set_volume(0.5).await;
        assert!(!playback.software_volume_locked.load(Ordering::Relaxed));
        assert!((playback.snapshot().volume - 0.5).abs() < 1e-9);

        // Device without hardware volume reports SoftwareVolume below 100%.
        let mut sw_dac = hi_res_dac();
        sw_dac.hardware_volume = false;
        *engine.devices.lock().unwrap() = vec![sw_dac];
        ctrl.refresh_devices().await;
        assert_eq!(ctrl.state().path.reason, BitPerfectReason::SoftwareVolume);

        ctrl.dispose();
    }

    #[tokio::test]
    async fn toggle_mute_restores_pre_mute_level_and_opus_uses_48khz() {
        let prefs = Arc::new(Prefs::in_memory());
        let engine = Arc::new(MockWasapi::new(vec![hi_res_dac()]));
        let snap = PlayerSnapshot {
            stream: Some(ResolvedStream {
                audio_codec: "opus".to_string(),
                bit_depth: 16,
                sampling_rate_khz: 44.1,
                ..Default::default()
            }),
            volume: 0.65,
            ..Default::default()
        };
        let playback = Arc::new(MockPlayback::new(snap));
        let ctrl = AudioOutputController::with_backends(prefs, engine, playback.clone());

        let src = AudioOutputController::source_of(playback.snapshot().stream.as_ref()).unwrap();
        assert_eq!(src.sample_rate_hz, 48000);

        ctrl.toggle_mute().await;
        assert!((playback.snapshot().volume - 0.0).abs() < 1e-9);

        ctrl.toggle_mute().await;
        assert!((playback.snapshot().volume - 0.65).abs() < 1e-9);
    }

    #[tokio::test]
    async fn hotplug_debounce_pauses_and_marks_device_unavailable_on_removal() {
        let prefs = Arc::new(Prefs::in_memory());
        let engine = Arc::new(MockWasapi::new(vec![hi_res_dac()]));
        let playback = Arc::new(MockPlayback::new(PlayerSnapshot::default()));
        let ctrl = Arc::new(AudioOutputController::with_backends(
            prefs,
            engine.clone(),
            playback.clone(),
        ));

        ctrl.refresh_devices().await;
        assert_eq!(ctrl.state().devices.len(), 1);

        // Simulate disconnect
        engine.devices.lock().unwrap().clear();
        ctrl.on_hotplug_with_delay(
            WasapiDeviceEvent {
                reason: "removed".to_string(),
                id: "{dac-1}".to_string(),
            },
            Duration::from_millis(20),
        );
        tokio::time::sleep(Duration::from_millis(80)).await;

        assert_eq!(playback.pause_count.load(Ordering::Relaxed), 1);
        assert_eq!(
            ctrl.state().path.reason,
            BitPerfectReason::DeviceUnavailable
        );
        assert_eq!(
            ctrl.state().path.error.as_deref(),
            Some("Audio device disconnected")
        );
        ctrl.dispose();
    }
}
