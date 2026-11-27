use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::models::dac_device::DacDevice;
use crate::storage::diagnostics_log::DiagnosticsLog;

pub const HOTPLUG_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasapiDeviceEvent {
    pub reason: String,
    pub id: String,
}

pub fn wasapi_log(message: &str) {
    // Verbose: probes fire per device/format, so this only lands in the
    // exported session log when the user enables verbose logging.
    DiagnosticsLog::detail(&format!("wasapi {message}"));
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Abstraction over the WASAPI engine so `AudioOutputController` can be unit
/// tested with a mock backend on any OS.
pub trait WasapiBackend: Send + Sync {
    fn is_supported(&self) -> bool;
    fn subscribe_events(&self) -> broadcast::Receiver<WasapiDeviceEvent>;
    fn start_watching(&self) -> Result<(), String>;
    fn stop_watching(&self);
    fn enumerate_devices<'a>(&'a self) -> BoxFuture<'a, Vec<DacDevice>>;
    fn probe_device<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Option<DacDevice>>;
    fn is_format_supported<'a>(
        &'a self,
        id: &'a str,
        sample_rate: i32,
        channels: i32,
        bit_depth: i32,
    ) -> BoxFuture<'a, bool>;
    fn set_hardware_volume<'a>(&'a self, id: &'a str, scalar: f64) -> BoxFuture<'a, bool>;
    fn get_hardware_volume<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Option<f64>>;
    fn default_device_id<'a>(&'a self) -> BoxFuture<'a, String>;
}

/// Native Windows WASAPI endpoint / capability / hardware-volume engine
/// (stubbed on non-Windows targets).
///
/// Does not render PCM — libmpv (`ao=wasapi`) remains the exclusive-mode
/// renderer.
pub struct WasapiEngine {
    events_tx: broadcast::Sender<WasapiDeviceEvent>,
}

impl Default for WasapiEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WasapiEngine {
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(64);
        Self { events_tx }
    }

    pub fn is_supported(&self) -> bool {
        cfg!(windows)
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<WasapiDeviceEvent> {
        self.events_tx.subscribe()
    }

    pub fn start_watching(&self) -> Result<(), String> {
        #[cfg(windows)]
        {
            win::start_watching(self.events_tx.clone())
        }
        #[cfg(not(windows))]
        {
            Ok(())
        }
    }

    pub fn stop_watching(&self) {
        #[cfg(windows)]
        win::stop_watching();
    }

    pub async fn enumerate_devices(&self) -> Vec<DacDevice> {
        if !self.is_supported() {
            return Vec::new();
        }
        #[cfg(windows)]
        {
            let res = tokio::time::timeout(
                Duration::from_secs(4),
                tokio::task::spawn_blocking(win::enumerate_render_devices),
            )
            .await;
            match res {
                Ok(Ok(Ok(devices))) => devices,
                Ok(Ok(Err(e))) => {
                    wasapi_log(&format!("enumerateDevices failed: {e}"));
                    Vec::new()
                }
                Ok(Err(e)) => {
                    wasapi_log(&format!("Unexpected error in enumerateDevices: {e}"));
                    Vec::new()
                }
                Err(_) => {
                    wasapi_log("enumerateDevices timed out");
                    Vec::new()
                }
            }
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }

    pub async fn probe_device(&self, id: &str) -> Option<DacDevice> {
        if !self.is_supported() || id.trim().is_empty() {
            return None;
        }
        #[cfg(windows)]
        {
            let id_owned = id.to_string();
            let id_log = id_owned.clone();
            let res = tokio::time::timeout(
                Duration::from_secs(5),
                tokio::task::spawn_blocking(move || win::probe_device(&id_owned)),
            )
            .await;
            match res {
                Ok(Ok(Ok(device))) => Some(device),
                Ok(Ok(Err(e))) => {
                    wasapi_log(&format!("probeDevice failed for {id_log}:{e}"));
                    None
                }
                Ok(Err(e)) => {
                    wasapi_log(&format!("Unexpected error in probeDevice for {id_log}:{e}"));
                    None
                }
                Err(_) => {
                    wasapi_log(&format!("probeDevice timed out for device {id_log}"));
                    None
                }
            }
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// Verifies whether the specified DAC device supports a given PCM format
    /// before requesting exclusive mode from libmpv.
    pub async fn is_format_supported(
        &self,
        id: &str,
        sample_rate: i32,
        channels: i32,
        bit_depth: i32,
    ) -> bool {
        if !self.is_supported() || id.trim().is_empty() {
            return false;
        }
        #[cfg(windows)]
        {
            let id_owned = id.to_string();
            let res = tokio::time::timeout(
                Duration::from_secs(3),
                tokio::task::spawn_blocking(move || {
                    win::is_format_supported(&id_owned, sample_rate, channels, bit_depth)
                }),
            )
            .await;
            match res {
                Ok(Ok(Ok(supported))) => supported,
                Ok(Ok(Err(e))) => {
                    wasapi_log(&format!(
                        "Format check error ({sample_rate} Hz, {bit_depth}-bit):{e}"
                    ));
                    false
                }
                Ok(Err(e)) => {
                    wasapi_log(&format!(
                        "Format check error ({sample_rate} Hz, {bit_depth}-bit):{e}"
                    ));
                    false
                }
                Err(_) => {
                    wasapi_log(&format!(
                        "Format check timed out ({sample_rate} Hz, {bit_depth}-bit)"
                    ));
                    false
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (sample_rate, channels, bit_depth);
            false
        }
    }

    pub async fn set_hardware_volume(&self, id: &str, scalar: f64) -> bool {
        if !self.is_supported() || id.trim().is_empty() {
            return false;
        }
        #[cfg(windows)]
        {
            let id_owned = id.to_string();
            let clamped = scalar.clamp(0.0, 1.0) as f32;
            let res = tokio::time::timeout(
                Duration::from_secs(2),
                tokio::task::spawn_blocking(move || win::set_hardware_volume(&id_owned, clamped)),
            )
            .await;
            match res {
                Ok(Ok(Ok(()))) => true,
                Ok(Ok(Err(e))) => {
                    wasapi_log(&format!("setHardwareVolume failed: {e}"));
                    false
                }
                _ => false,
            }
        }
        #[cfg(not(windows))]
        {
            let _ = scalar;
            false
        }
    }

    pub async fn get_hardware_volume(&self, id: &str) -> Option<f64> {
        if !self.is_supported() || id.trim().is_empty() {
            return None;
        }
        #[cfg(windows)]
        {
            let id_owned = id.to_string();
            let res = tokio::time::timeout(
                Duration::from_secs(2),
                tokio::task::spawn_blocking(move || win::get_hardware_volume(&id_owned)),
            )
            .await;
            match res {
                Ok(Ok(Ok(v))) => Some(v as f64),
                Ok(Ok(Err(e))) => {
                    wasapi_log(&format!("getHardwareVolume failed: {e}"));
                    None
                }
                _ => None,
            }
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    pub async fn default_device_id(&self) -> String {
        if !self.is_supported() {
            return String::new();
        }
        #[cfg(windows)]
        {
            let res = tokio::time::timeout(
                Duration::from_secs(2),
                tokio::task::spawn_blocking(win::default_device_id),
            )
            .await;
            match res {
                Ok(Ok(Ok(id))) => id,
                Ok(Ok(Err(e))) => {
                    wasapi_log(&format!("defaultDeviceId failed: {e}"));
                    String::new()
                }
                _ => String::new(),
            }
        }
        #[cfg(not(windows))]
        {
            String::new()
        }
    }
}

impl WasapiBackend for WasapiEngine {
    fn is_supported(&self) -> bool {
        WasapiEngine::is_supported(self)
    }

    fn subscribe_events(&self) -> broadcast::Receiver<WasapiDeviceEvent> {
        WasapiEngine::subscribe_events(self)
    }

    fn start_watching(&self) -> Result<(), String> {
        WasapiEngine::start_watching(self)
    }

    fn stop_watching(&self) {
        WasapiEngine::stop_watching(self)
    }

    fn enumerate_devices<'a>(&'a self) -> BoxFuture<'a, Vec<DacDevice>> {
        Box::pin(WasapiEngine::enumerate_devices(self))
    }

    fn probe_device<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Option<DacDevice>> {
        Box::pin(WasapiEngine::probe_device(self, id))
    }

    fn is_format_supported<'a>(
        &'a self,
        id: &'a str,
        sample_rate: i32,
        channels: i32,
        bit_depth: i32,
    ) -> BoxFuture<'a, bool> {
        Box::pin(WasapiEngine::is_format_supported(
            self,
            id,
            sample_rate,
            channels,
            bit_depth,
        ))
    }

    fn set_hardware_volume<'a>(&'a self, id: &'a str, scalar: f64) -> BoxFuture<'a, bool> {
        Box::pin(WasapiEngine::set_hardware_volume(self, id, scalar))
    }

    fn get_hardware_volume<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Option<f64>> {
        Box::pin(WasapiEngine::get_hardware_volume(self, id))
    }

    fn default_device_id<'a>(&'a self) -> BoxFuture<'a, String> {
        Box::pin(WasapiEngine::default_device_id(self))
    }
}

#[cfg(windows)]
mod win {
    use std::collections::HashMap;
    use std::mem::size_of;
    use std::sync::{mpsc, Mutex, OnceLock};
    use std::thread::JoinHandle;

    use tokio::sync::broadcast;
    use windows::core::{implement, HSTRING, PCWSTR, PWSTR};
    use windows::Win32::Devices::FunctionDiscovery::{
        PKEY_DeviceInterface_FriendlyName, PKEY_Device_DeviceDesc, PKEY_Device_EnumeratorName,
        PKEY_Device_FriendlyName,
    };
    use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, AUDCLNT_SHAREMODE_EXCLUSIVE, DEVICE_STATE, DEVICE_STATE_ACTIVE,
        EDataFlow, ENDPOINT_HARDWARE_SUPPORT_VOLUME, ERole, IAudioClient, IMMDevice,
        IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
        MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
    };
    use windows::Win32::Media::KernelStreaming::{
        KSDATAFORMAT_SUBTYPE_PCM, SPEAKER_FRONT_CENTER, SPEAKER_FRONT_LEFT, SPEAKER_FRONT_RIGHT,
        WAVE_FORMAT_EXTENSIBLE,
    };
    use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED, STGM_READ,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PROPERTYKEY};

    use super::WasapiDeviceEvent;
    use crate::models::dac_device::DacDevice;
    use crate::models::pcm_format::PcmFormat;

    const RATES: [i32; 8] = [44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000];
    const DEPTHS: [i32; 3] = [16, 24, 32];
    const CHANNELS: [i32; 1] = [2];

    type ProbeCacheMap = HashMap<String, (bool, Vec<PcmFormat>)>;

    static PROBE_CACHE: OnceLock<Mutex<ProbeCacheMap>> = OnceLock::new();

    fn probe_cache() -> &'static Mutex<ProbeCacheMap> {
        PROBE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn invalidate_probe_cache(id: &str) {
        if let Ok(mut guard) = probe_cache().lock() {
            guard.remove(id);
        }
    }

    struct ComScope {
        should_uninit: bool,
    }

    impl ComScope {
        fn enter() -> Result<Self, String> {
            let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if hr == S_OK || hr == S_FALSE {
                Ok(Self {
                    should_uninit: true,
                })
            } else if hr == RPC_E_CHANGED_MODE {
                Ok(Self {
                    should_uninit: false,
                })
            } else {
                Err(format!("CoInitializeEx failed {}", hr_hex(hr.0)))
            }
        }
    }

    impl Drop for ComScope {
        fn drop(&mut self) {
            if self.should_uninit {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn hr_hex(hr: i32) -> String {
        format!("0x{:08X}", hr as u32)
    }

    fn pwstr_to_string(ptr: PWSTR) -> String {
        if ptr.is_null() {
            return String::new();
        }
        unsafe { ptr.to_string().unwrap_or_default() }
    }

    fn pcwstr_to_string(ptr: &PCWSTR) -> String {
        if ptr.is_null() {
            return String::new();
        }
        unsafe { ptr.to_string().unwrap_or_default() }
    }

    fn create_enumerator() -> Result<IMMDeviceEnumerator, String> {
        unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("MMDeviceEnumerator failed {}", hr_hex(e.code().0)))
        }
    }

    fn device_by_id(id: &str) -> Result<IMMDevice, String> {
        let enumerator = create_enumerator()?;
        unsafe {
            if id.is_empty() {
                enumerator
                    .GetDefaultAudioEndpoint(eRender, eConsole)
                    .map_err(|e| format!("GetDevice failed {}", hr_hex(e.code().0)))
            } else {
                let wid = HSTRING::from(id);
                enumerator
                    .GetDevice(&wid)
                    .map_err(|e| format!("GetDevice failed {}", hr_hex(e.code().0)))
            }
        }
    }

    fn default_id_from_enumerator(enumerator: &IMMDeviceEnumerator) -> String {
        unsafe {
            let Ok(device) = enumerator.GetDefaultAudioEndpoint(eRender, eConsole) else {
                return String::new();
            };
            let Ok(raw) = device.GetId() else {
                return String::new();
            };
            let id = pwstr_to_string(raw);
            CoTaskMemFree(Some(raw.as_ptr().cast()));
            id
        }
    }

    fn prop_string(store: &IPropertyStore, key: &PROPERTYKEY) -> String {
        unsafe {
            let Ok(var) = store.GetValue(key) else {
                return String::new();
            };
            let raw = var.as_raw();
            if raw.Anonymous.Anonymous.vt == VT_LPWSTR.0 {
                let ptr = raw.Anonymous.Anonymous.Anonymous.pwszVal;
                if !ptr.is_null() {
                    return PWSTR(ptr).to_string().unwrap_or_default();
                }
            }
            String::new()
        }
    }

    fn fill_identity(device: &IMMDevice, info: &mut DacDevice) -> bool {
        let raw_id = match unsafe { device.GetId() } {
            Ok(id) if !id.is_null() => id,
            _ => return false,
        };
        info.id = pwstr_to_string(raw_id);
        unsafe { CoTaskMemFree(Some(raw_id.as_ptr().cast())) };
        if info.id.is_empty() {
            return false;
        }

        if let Ok(store) = unsafe { device.OpenPropertyStore(STGM_READ) } {
            info.name = prop_string(&store, &PKEY_Device_FriendlyName);
            if info.name.is_empty() {
                info.name = prop_string(&store, &PKEY_Device_DeviceDesc);
            }
            info.manufacturer = prop_string(&store, &PKEY_DeviceInterface_FriendlyName);
            info.enumerator = prop_string(&store, &PKEY_Device_EnumeratorName);
        }
        if info.name.is_empty() {
            info.name = info.id.clone();
        }
        true
    }

    fn fill_hardware_volume(device: &IMMDevice, info: &mut DacDevice) {
        let Ok(volume) = (unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) })
        else {
            info.hardware_volume = false;
            return;
        };
        if let Ok(mask) = unsafe { volume.QueryHardwareSupport() } {
            info.hardware_volume = (mask & ENDPOINT_HARDWARE_SUPPORT_VOLUME) != 0;
        } else {
            // Some USB DACs expose the volume interface without the hardware bit.
            info.hardware_volume = unsafe { volume.GetMasterVolumeLevelScalar() }.is_ok();
        }
    }

    fn make_pcm(rate: i32, bit_depth: i32, channels: i32, packed24: bool) -> WAVEFORMATEXTENSIBLE {
        let container = if bit_depth == 24 && packed24 {
            24
        } else if bit_depth == 24 {
            32
        } else {
            bit_depth
        };
        let block_align = (channels * (container / 8)) as u16;
        let samples_per_sec = rate as u32;
        WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
                nChannels: channels as u16,
                nSamplesPerSec: samples_per_sec,
                nAvgBytesPerSec: samples_per_sec * block_align as u32,
                nBlockAlign: block_align,
                wBitsPerSample: container as u16,
                cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
            },
            Samples: WAVEFORMATEXTENSIBLE_0 {
                wValidBitsPerSample: bit_depth as u16,
            },
            dwChannelMask: if channels >= 2 {
                SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT
            } else {
                SPEAKER_FRONT_CENTER
            },
            SubFormat: KSDATAFORMAT_SUBTYPE_PCM,
        }
    }

    fn make_float32(rate: i32, channels: i32) -> WAVEFORMATEXTENSIBLE {
        let mut fmt = make_pcm(rate, 32, channels, false);
        fmt.SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
        fmt.Samples = WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: 32,
        };
        fmt
    }

    fn exclusive_supports(client: &IAudioClient, format: &WAVEFORMATEXTENSIBLE) -> bool {
        let mut closest: *mut WAVEFORMATEX = std::ptr::null_mut();
        let hr = unsafe {
            client.IsFormatSupported(
                AUDCLNT_SHAREMODE_EXCLUSIVE,
                &raw const format.Format,
                Some(&mut closest),
            )
        };
        if !closest.is_null() {
            unsafe { CoTaskMemFree(Some(closest.cast())) };
        }
        hr == S_OK
    }

    fn supports_format_tuple(client: &IAudioClient, rate: i32, depth: i32, ch: i32) -> bool {
        if depth == 24 {
            let packed = make_pcm(rate, 24, ch, true);
            let padded = make_pcm(rate, 24, ch, false);
            exclusive_supports(client, &packed) || exclusive_supports(client, &padded)
        } else if depth == 32 {
            let pcm = make_pcm(rate, 32, ch, false);
            let flt = make_float32(rate, ch);
            exclusive_supports(client, &pcm) || exclusive_supports(client, &flt)
        } else {
            let pcm = make_pcm(rate, depth, ch, false);
            exclusive_supports(client, &pcm)
        }
    }

    fn probe_formats(device: &IMMDevice, info: &mut DacDevice) {
        if let Ok(guard) = probe_cache().lock() {
            if let Some((supported, formats)) = guard.get(&info.id) {
                info.exclusive_supported = *supported;
                info.formats = formats.clone();
                return;
            }
        }

        let Ok(client) = (unsafe { device.Activate::<IAudioClient>(CLSCTX_ALL, None) }) else {
            info.exclusive_supported = false;
            return;
        };

        let mut found: Vec<PcmFormat> = Vec::new();
        let mut any = false;
        for ch in CHANNELS {
            for depth in DEPTHS {
                for rate in RATES {
                    if supports_format_tuple(&client, rate, depth, ch) {
                        let candidate = PcmFormat::new(rate, depth, ch);
                        if !found.iter().any(|f| f == &candidate) {
                            found.push(candidate);
                        }
                        any = true;
                    }
                }
            }
        }

        if let Ok(mut guard) = probe_cache().lock() {
            guard.insert(info.id.clone(), (any, found.clone()));
        }
        info.exclusive_supported = any;
        info.formats = found;
    }

    pub fn enumerate_render_devices() -> Result<Vec<DacDevice>, String> {
        let _com = ComScope::enter()?;
        let enumerator = create_enumerator()?;
        let default_id = default_id_from_enumerator(&enumerator);
        let collection = unsafe {
            enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .map_err(|e| format!("EnumAudioEndpoints failed {}", hr_hex(e.code().0)))?
        };
        let count = unsafe { collection.GetCount() }.unwrap_or(0);
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let Ok(device) = (unsafe { collection.Item(i) }) else {
                continue;
            };
            let mut info = DacDevice::default();
            if fill_identity(&device, &mut info) {
                info.is_default = !default_id.is_empty() && info.id == default_id;
                fill_hardware_volume(&device, &mut info);
                probe_formats(&device, &mut info);
                out.push(info);
            }
        }
        Ok(out)
    }

    pub fn probe_device(device_id: &str) -> Result<DacDevice, String> {
        let _com = ComScope::enter()?;
        let enumerator = create_enumerator()?;
        let default_id = default_id_from_enumerator(&enumerator);
        let device = device_by_id(device_id)?;
        let mut info = DacDevice::default();
        if !fill_identity(&device, &mut info) {
            return Err("Device identity unavailable".to_string());
        }
        info.is_default = !default_id.is_empty() && info.id == default_id;
        fill_hardware_volume(&device, &mut info);
        probe_formats(&device, &mut info);
        Ok(info)
    }

    pub fn is_format_supported(
        device_id: &str,
        sample_rate: i32,
        channels: i32,
        bit_depth: i32,
    ) -> Result<bool, String> {
        let _com = ComScope::enter()?;
        let device = device_by_id(device_id)?;
        let client = unsafe { device.Activate::<IAudioClient>(CLSCTX_ALL, None) }
            .map_err(|e| format!("IAudioClient unavailable {}", hr_hex(e.code().0)))?;
        Ok(supports_format_tuple(&client, sample_rate, bit_depth, channels))
    }

    pub fn set_hardware_volume(device_id: &str, scalar: f32) -> Result<(), String> {
        let _com = ComScope::enter()?;
        let device = device_by_id(device_id)?;
        let volume = unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
            .map_err(|e| format!("IAudioEndpointVolume unavailable {}", hr_hex(e.code().0)))?;
        let clamped = scalar.clamp(0.0, 1.0);
        unsafe { volume.SetMasterVolumeLevelScalar(clamped, std::ptr::null()) }
            .map_err(|e| format!("SetMasterVolumeLevelScalar failed {}", hr_hex(e.code().0)))
    }

    pub fn get_hardware_volume(device_id: &str) -> Result<f32, String> {
        let _com = ComScope::enter()?;
        let device = device_by_id(device_id)?;
        let volume = unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
            .map_err(|e| format!("IAudioEndpointVolume unavailable {}", hr_hex(e.code().0)))?;
        unsafe { volume.GetMasterVolumeLevelScalar() }
            .map_err(|e| format!("GetMasterVolumeLevelScalar failed {}", hr_hex(e.code().0)))
    }

    pub fn default_device_id() -> Result<String, String> {
        let _com = ComScope::enter()?;
        let enumerator = create_enumerator()?;
        Ok(default_id_from_enumerator(&enumerator))
    }

    fn key_eq(a: &PROPERTYKEY, b: &PROPERTYKEY) -> bool {
        a.pid == b.pid && a.fmtid == b.fmtid
    }

    #[implement(IMMNotificationClient)]
    struct NotificationClient {
        tx: broadcast::Sender<WasapiDeviceEvent>,
    }

    impl NotificationClient {
        fn emit(&self, reason: &str, id: &PCWSTR) {
            let _ = self.tx.send(WasapiDeviceEvent {
                reason: reason.to_string(),
                id: pcwstr_to_string(id),
            });
        }
    }

    impl IMMNotificationClient_Impl for NotificationClient_Impl {
        fn OnDeviceStateChanged(
            &self,
            pwstrdeviceid: &PCWSTR,
            _dwnewstate: DEVICE_STATE,
        ) -> windows::core::Result<()> {
            invalidate_probe_cache(&pcwstr_to_string(pwstrdeviceid));
            self.emit("state", pwstrdeviceid);
            Ok(())
        }

        fn OnDeviceAdded(&self, pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
            invalidate_probe_cache(&pcwstr_to_string(pwstrdeviceid));
            self.emit("added", pwstrdeviceid);
            Ok(())
        }

        fn OnDeviceRemoved(&self, pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
            invalidate_probe_cache(&pcwstr_to_string(pwstrdeviceid));
            self.emit("removed", pwstrdeviceid);
            Ok(())
        }

        fn OnDefaultDeviceChanged(
            &self,
            flow: EDataFlow,
            _role: ERole,
            pwstrdefaultdeviceid: &PCWSTR,
        ) -> windows::core::Result<()> {
            if flow == eRender {
                self.emit("default", pwstrdefaultdeviceid);
            }
            Ok(())
        }

        fn OnPropertyValueChanged(
            &self,
            pwstrdeviceid: &PCWSTR,
            key: &PROPERTYKEY,
        ) -> windows::core::Result<()> {
            // Only forward relevant device identity properties while ignoring
            // high-frequency playback, volume, and meter property updates.
            if key_eq(key, &PKEY_Device_FriendlyName)
                || key_eq(key, &PKEY_DeviceInterface_FriendlyName)
                || key_eq(key, &PKEY_Device_DeviceDesc)
                || key_eq(key, &PKEY_Device_EnumeratorName)
            {
                invalidate_probe_cache(&pcwstr_to_string(pwstrdeviceid));
                self.emit("property", pwstrdeviceid);
            }
            Ok(())
        }
    }

    struct WatcherHandle {
        stop_tx: mpsc::Sender<()>,
        thread: JoinHandle<()>,
    }

    static WATCHER: OnceLock<Mutex<Option<WatcherHandle>>> = OnceLock::new();

    fn watcher_lock() -> &'static Mutex<Option<WatcherHandle>> {
        WATCHER.get_or_init(|| Mutex::new(None))
    }

    pub fn start_watching(tx: broadcast::Sender<WasapiDeviceEvent>) -> Result<(), String> {
        stop_watching();

        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        let thread = std::thread::Builder::new()
            .name("wasapi-notify".to_string())
            .spawn(move || {
                let _com = match ComScope::enter() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let enumerator = match create_enumerator() {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let client: IMMNotificationClient = NotificationClient { tx }.into();
                if let Err(e) = unsafe { enumerator.RegisterEndpointNotificationCallback(&client) } {
                    let _ = ready_tx.send(Err(format!(
                        "RegisterEndpointNotificationCallback failed {}",
                        hr_hex(e.code().0)
                    )));
                    return;
                }
                let _ = ready_tx.send(Ok(()));
                let _ = stop_rx.recv();
                let _ = unsafe { enumerator.UnregisterEndpointNotificationCallback(&client) };
            })
            .map_err(|e| format!("Failed to spawn WASAPI watcher thread: {e}"))?;

        match ready_rx.recv() {
            Ok(Ok(())) => {
                if let Ok(mut guard) = watcher_lock().lock() {
                    *guard = Some(WatcherHandle { stop_tx, thread });
                }
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(e) => {
                let _ = thread.join();
                Err(format!("WASAPI watcher startup failed: {e}"))
            }
        }
    }

    pub fn stop_watching() {
        let handle = watcher_lock().lock().ok().and_then(|mut g| g.take());
        if let Some(h) = handle {
            let _ = h.stop_tx.send(());
            let _ = h.thread.join();
        }
    }
}
