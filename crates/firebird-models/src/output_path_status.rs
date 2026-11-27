use serde::{Deserialize, Serialize};

use super::pcm_format::PcmFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BitPerfectReason {
    BitPerfect,
    DspActive,
    PeqActive,
    SoftwareVolume,
    Resampling,
    FormatConversion,
    ExclusiveUnavailable,
    DeviceUnavailable,
    FormatUnsupported,
    #[default]
    SharedMode,
    SpeedNotUnity,
    CrossfadeActive,
    PitchNotUnity,
    LoudnessActive,
    AutoMixActive,
}

impl BitPerfectReason {
    pub const fn label(self) -> &'static str {
        match self {
            Self::BitPerfect => "Bit-Perfect",
            Self::DspActive => "Bit-Perfect unavailable — DSP is active.",
            Self::PeqActive => "Bit-Perfect unavailable — PEQ is active.",
            Self::SoftwareVolume => "Software — Bit-Perfect unavailable",
            Self::Resampling => "Bit-Perfect unavailable — resampling.",
            Self::FormatConversion => "Native format unavailable",
            Self::ExclusiveUnavailable => "WASAPI Exclusive unavailable",
            Self::DeviceUnavailable => "Output device unavailable",
            Self::FormatUnsupported => "Native format unavailable",
            Self::SharedMode => {
                if cfg!(target_os = "windows") {
                    "WASAPI Shared — mixer in path"
                } else {
                    "Shared — mixer in path"
                }
            }
            Self::SpeedNotUnity => "Bit-Perfect unavailable — playback speed ≠ 1.0.",
            Self::CrossfadeActive => "Bit-Perfect unavailable — crossfade is enabled.",
            Self::PitchNotUnity => "Bit-Perfect unavailable — pitch ≠ 1.0.",
            Self::LoudnessActive => "Bit-Perfect unavailable — loudness normalization is on.",
            Self::AutoMixActive => "Bit-Perfect unavailable — timed AutoMix is on.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WasapiShareMode {
    Exclusive,
    #[default]
    Shared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatDecision {
    pub source: PcmFormat,
    #[serde(default)]
    pub output: Option<PcmFormat>,
    pub native_match: bool,
    pub resampling: bool,
    pub format_conversion: bool,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputPathStatus {
    #[serde(default = "default_device_name")]
    pub device_name: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub mode: WasapiShareMode,
    #[serde(default)]
    pub exclusive_requested: bool,
    #[serde(default)]
    pub exclusive_active: bool,
    #[serde(default)]
    pub source: Option<PcmFormat>,
    #[serde(default)]
    pub output: Option<PcmFormat>,
    #[serde(default)]
    pub resampling: bool,
    #[serde(default)]
    pub dsp_active: bool,
    #[serde(default)]
    pub peq_active: bool,
    #[serde(default = "default_true")]
    pub software_volume: bool,
    #[serde(default)]
    pub hardware_volume: bool,
    #[serde(default)]
    pub crossfade: bool,
    #[serde(default = "default_speed")]
    pub speed: f64,
    #[serde(default)]
    pub reason: BitPerfectReason,
    #[serde(default)]
    pub error: Option<String>,
}

fn default_device_name() -> String {
    "System default".to_string()
}

const fn default_true() -> bool {
    true
}

const fn default_speed() -> f64 {
    1.0
}

impl Default for OutputPathStatus {
    fn default() -> Self {
        Self {
            device_name: default_device_name(),
            device_id: String::new(),
            mode: WasapiShareMode::Shared,
            exclusive_requested: false,
            exclusive_active: false,
            source: None,
            output: None,
            resampling: false,
            dsp_active: false,
            peq_active: false,
            software_volume: true,
            hardware_volume: false,
            crossfade: false,
            speed: 1.0,
            reason: BitPerfectReason::SharedMode,
            error: None,
        }
    }
}

impl OutputPathStatus {
    pub fn bit_perfect(&self) -> bool {
        self.reason == BitPerfectReason::BitPerfect
    }

    pub fn output_label(&self) -> String {
        self.output
            .as_ref()
            .map(PcmFormat::label)
            .unwrap_or_else(|| "—".to_string())
    }

    pub fn source_label(&self) -> String {
        self.source
            .as_ref()
            .map(PcmFormat::label)
            .unwrap_or_else(|| "—".to_string())
    }
}
