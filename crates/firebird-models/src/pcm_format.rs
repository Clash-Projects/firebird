use std::fmt;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// PCM format used for WASAPI exclusive negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PcmFormat {
    #[serde(default)]
    pub sample_rate_hz: i32,
    #[serde(default)]
    pub bit_depth: i32,
    #[serde(default = "default_channels")]
    pub channels: i32,
}

const fn default_channels() -> i32 {
    2
}

impl Default for PcmFormat {
    fn default() -> Self {
        Self {
            sample_rate_hz: 0,
            bit_depth: 0,
            channels: 2,
        }
    }
}

impl PcmFormat {
    pub const fn new(sample_rate_hz: i32, bit_depth: i32, channels: i32) -> Self {
        Self {
            sample_rate_hz,
            bit_depth,
            channels,
        }
    }

    pub fn sample_rate_khz(&self) -> f64 {
        self.sample_rate_hz as f64 / 1000.0
    }

    pub fn label(&self) -> String {
        format!("{}-bit / {}", self.bit_depth, Self::rate_label(self.sample_rate_hz))
    }

    pub fn rate_label(hz: i32) -> String {
        if hz % 1000 == 0 {
            format!("{} kHz", hz / 1000)
        } else {
            let k = hz as f64 / 1000.0;
            format!("{k:.1} kHz")
        }
    }

    pub fn matches(&self, other: &Self) -> bool {
        self.sample_rate_hz == other.sample_rate_hz
            && self.bit_depth == other.bit_depth
            && self.channels == other.channels
    }
}

impl fmt::Display for PcmFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

pub fn pcm_from_stream(bit_depth: i32, sampling_rate_khz: f64, channels: i32) -> PcmFormat {
    let hz = (sampling_rate_khz * 1000.0).round() as i32;
    PcmFormat {
        sample_rate_hz: if hz <= 0 { 44100 } else { hz },
        bit_depth: if bit_depth <= 0 { 16 } else { bit_depth },
        channels,
    }
}

pub fn bit_depth_from_mpv_format(format: Option<&str>, forced_format: Option<&str>) -> i32 {
    let Some(format) = format else {
        return 0;
    };
    if format.is_empty() {
        return 0;
    }
    let f = format.to_lowercase();
    if f.contains("float") || f.contains("dbl") || f.contains("double") {
        return 0;
    }
    if f.contains("s24") || f.contains("24") {
        return 24;
    }
    if f.contains("s16") || f.contains("16") {
        return 16;
    }
    if f.contains("s32") || f.contains("32") {
        // WASAPI 24-bit exclusive is almost always a 32-bit container.
        if forced_format == Some("s24") {
            return 24;
        }
        return 32;
    }
    if f.contains("s8") || f.contains("u8") {
        return 8;
    }
    0
}

pub fn mpv_format_is_float(format: Option<&str>) -> bool {
    let Some(format) = format else {
        return false;
    };
    if format.is_empty() {
        return false;
    }
    let f = format.to_lowercase();
    f.contains("float") || f.contains("dbl") || f.contains("double")
}

pub const fn mpv_sample_format(bit_depth: i32) -> &'static str {
    match bit_depth {
        16 => "s16",
        24 => "s24",
        32 => "s32",
        _ => {
            if bit_depth > 24 {
                "s32"
            } else {
                "s16"
            }
        }
    }
}

fn out_format_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"format=([^\s,}]+)").expect("valid out format regex"))
}

fn out_rate_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"samplerate=(\d+)").expect("valid out rate regex"))
}

fn out_channels_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"channel-count=(\d+)").expect("valid out channels regex"))
}

pub fn pcm_from_mpv_out_params(
    raw: &str,
    forced_format: Option<&str>,
    channels: i32,
) -> Option<PcmFormat> {
    if raw.is_empty() {
        return None;
    }
    let format = out_format_regex()
        .captures(raw)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str());
    if mpv_format_is_float(format) {
        return None;
    }
    let rate = out_rate_regex()
        .captures(raw)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .unwrap_or(0);
    if rate <= 0 {
        return None;
    }
    let depth = bit_depth_from_mpv_format(format, forced_format);
    if depth <= 0 {
        return None;
    }
    let ch = out_channels_regex()
        .captures(raw)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .unwrap_or(channels);
    Some(PcmFormat {
        sample_rate_hz: rate,
        bit_depth: depth,
        channels: if ch <= 0 { 2 } else { ch },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_and_label_formatting() {
        let p44 = PcmFormat::new(44100, 16, 2);
        assert_eq!(p44.label(), "16-bit / 44.1 kHz");
        assert!((p44.sample_rate_khz() - 44.1).abs() < f64::EPSILON);

        let p96 = PcmFormat::new(96000, 24, 2);
        assert_eq!(p96.label(), "24-bit / 96 kHz");
    }

    #[test]
    fn pcm_from_stream_fallbacks_and_conversion() {
        assert_eq!(pcm_from_stream(0, 0.0, 2), PcmFormat::new(44100, 16, 2));
        assert_eq!(pcm_from_stream(24, 192.0, 2), PcmFormat::new(192000, 24, 2));
    }

    #[test]
    fn mpv_format_parsing() {
        assert_eq!(bit_depth_from_mpv_format(Some("floatp"), None), 0);
        assert_eq!(bit_depth_from_mpv_format(Some("s32"), Some("s24")), 24);
        assert_eq!(bit_depth_from_mpv_format(Some("s32"), None), 32);
        assert!(mpv_format_is_float(Some("float")));
        assert_eq!(mpv_sample_format(24), "s24");

        let parsed = pcm_from_mpv_out_params(
            "{format=s32, samplerate=96000, channel-count=2}",
            Some("s24"),
            2,
        );
        assert_eq!(parsed, Some(PcmFormat::new(96000, 24, 2)));
        assert_eq!(
            pcm_from_mpv_out_params("{format=floatp, samplerate=48000}", None, 2),
            None
        );
    }
}
