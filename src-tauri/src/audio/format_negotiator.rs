use crate::models::dac_device::DacDevice;
use crate::models::output_path_status::{BitPerfectReason, FormatDecision};
use crate::models::pcm_format::PcmFormat;

/// Inputs to [`FormatNegotiator::evaluate`].
#[derive(Debug, Clone)]
pub struct BitPerfectEvalInput<'a> {
    pub decision: &'a FormatDecision,
    pub exclusive_requested: bool,
    pub exclusive_active: bool,
    pub device_available: bool,
    pub dsp_active: bool,
    pub peq_active: bool,
    pub software_volume: bool,
    pub crossfade: bool,
    pub speed: f64,
    pub loudness: bool,
    pub auto_mix: bool,
    pub pitch: f64,
}

impl<'a> BitPerfectEvalInput<'a> {
    pub fn new(decision: &'a FormatDecision) -> Self {
        Self {
            decision,
            exclusive_requested: false,
            exclusive_active: false,
            device_available: true,
            dsp_active: false,
            peq_active: false,
            software_volume: false,
            crossfade: false,
            speed: 1.0,
            loudness: false,
            auto_mix: false,
            pitch: 1.0,
        }
    }
}

/// Negotiates a DAC exclusive format from the source PCM.
///
/// Exact source match is required for a native/bit-perfect decision.
/// Fallbacks never pretend to be bit-perfect.
#[derive(Debug, Default, Clone, Copy)]
pub struct FormatNegotiator;

impl FormatNegotiator {
    pub fn negotiate(
        &self,
        source: PcmFormat,
        device: Option<&DacDevice>,
        exclusive_requested: bool,
    ) -> FormatDecision {
        let Some(device) = device else {
            return FormatDecision {
                source,
                output: if exclusive_requested {
                    None
                } else {
                    Some(source)
                },
                native_match: false,
                resampling: exclusive_requested,
                format_conversion: exclusive_requested,
                note: if exclusive_requested {
                    "No output device selected".to_string()
                } else {
                    "Shared mode — Windows mixer may resample".to_string()
                },
            };
        };

        if !exclusive_requested {
            return FormatDecision {
                source,
                output: Some(source),
                native_match: false,
                resampling: true,
                format_conversion: true,
                note: "WASAPI Shared — Windows mixer in path".to_string(),
            };
        }

        if !device.exclusive_supported || device.formats.is_empty() {
            return FormatDecision {
                source,
                output: None,
                native_match: false,
                resampling: true,
                format_conversion: true,
                note: format!("WASAPI Exclusive unavailable on {}", device.name),
            };
        }

        if device.supports(&source) {
            return FormatDecision {
                source,
                output: Some(source),
                native_match: true,
                resampling: false,
                format_conversion: false,
                note: format!("DAC supports {}", source.label()),
            };
        }

        let mut same_rate: Vec<PcmFormat> = device
            .formats
            .iter()
            .copied()
            .filter(|f| f.sample_rate_hz == source.sample_rate_hz && f.channels == source.channels)
            .collect();
        same_rate.sort_by(|a, b| b.bit_depth.cmp(&a.bit_depth));
        if !same_rate.is_empty() {
            let pick = Self::closest_depth(&same_rate, source.bit_depth);
            return FormatDecision {
                source,
                output: Some(pick),
                native_match: false,
                resampling: false,
                format_conversion: pick.bit_depth != source.bit_depth,
                note: format!("Native bit depth unavailable — using {}", pick.label()),
            };
        }

        let mut same_depth: Vec<PcmFormat> = device
            .formats
            .iter()
            .copied()
            .filter(|f| f.bit_depth == source.bit_depth && f.channels == source.channels)
            .collect();
        same_depth.sort_by(|a, b| a.sample_rate_hz.cmp(&b.sample_rate_hz));
        if !same_depth.is_empty() {
            let pick = Self::closest_rate(&same_depth, source.sample_rate_hz);
            return FormatDecision {
                source,
                output: Some(pick),
                native_match: false,
                resampling: true,
                format_conversion: false,
                note: format!("Native sample rate unavailable — using {}", pick.label()),
            };
        }

        if !device.formats.is_empty() {
            let pick = Self::closest_overall(&device.formats, source);
            return FormatDecision {
                source,
                output: Some(pick),
                native_match: false,
                resampling: pick.sample_rate_hz != source.sample_rate_hz,
                format_conversion: pick.bit_depth != source.bit_depth,
                note: format!("Native format unavailable — using {}", pick.label()),
            };
        }

        FormatDecision {
            source,
            output: None,
            native_match: false,
            resampling: true,
            format_conversion: true,
            note: "No compatible exclusive PCM format".to_string(),
        }
    }

    pub fn evaluate(&self, input: &BitPerfectEvalInput<'_>) -> BitPerfectReason {
        if !input.device_available {
            return BitPerfectReason::DeviceUnavailable;
        }
        if input.peq_active {
            return BitPerfectReason::PeqActive;
        }
        if input.loudness {
            return BitPerfectReason::LoudnessActive;
        }
        if input.auto_mix {
            return BitPerfectReason::AutoMixActive;
        }
        if input.dsp_active {
            return BitPerfectReason::DspActive;
        }
        if input.crossfade {
            return BitPerfectReason::CrossfadeActive;
        }
        if (input.speed - 1.0).abs() > 0.001 {
            return BitPerfectReason::SpeedNotUnity;
        }
        if (input.pitch - 1.0).abs() > 0.005 {
            return BitPerfectReason::PitchNotUnity;
        }
        if !input.exclusive_requested || !input.exclusive_active {
            return if input.exclusive_requested {
                BitPerfectReason::ExclusiveUnavailable
            } else {
                BitPerfectReason::SharedMode
            };
        }
        if input.decision.output.is_none() {
            return BitPerfectReason::FormatUnsupported;
        }
        if input.decision.resampling {
            return BitPerfectReason::Resampling;
        }
        if input.decision.format_conversion || !input.decision.native_match {
            return BitPerfectReason::FormatConversion;
        }
        if input.software_volume {
            return BitPerfectReason::SoftwareVolume;
        }
        BitPerfectReason::BitPerfect
    }

    fn closest_depth(options: &[PcmFormat], depth: i32) -> PcmFormat {
        let mut best = options[0];
        let mut best_delta = (best.bit_depth - depth).abs();
        for &f in options {
            let d = (f.bit_depth - depth).abs();
            if d < best_delta {
                best = f;
                best_delta = d;
            }
        }
        best
    }

    fn closest_rate(options: &[PcmFormat], rate: i32) -> PcmFormat {
        let mut best = options[0];
        let mut best_delta = (best.sample_rate_hz - rate).abs();
        for &f in options {
            let d = (f.sample_rate_hz - rate).abs();
            if d < best_delta {
                best = f;
                best_delta = d;
            }
        }
        best
    }

    fn closest_overall(options: &[PcmFormat], source: PcmFormat) -> PcmFormat {
        let mut best = options[0];
        let mut best_score = 1_i64 << 30;
        for &f in options {
            let score = (f.sample_rate_hz as i64 - source.sample_rate_hz as i64).abs()
                + (f.bit_depth as i64 - source.bit_depth as i64).abs() * 4000;
            if score < best_score {
                best = f;
                best_score = score;
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::{BitPerfectEvalInput, FormatNegotiator};
    use crate::models::dac_device::DacDevice;
    use crate::models::output_path_status::BitPerfectReason;
    use crate::models::pcm_format::PcmFormat;

    fn sample_dac(formats: Vec<PcmFormat>) -> DacDevice {
        DacDevice {
            id: "dac-1".to_string(),
            name: "USB DAC".to_string(),
            exclusive_supported: true,
            hardware_volume: true,
            formats,
            ..Default::default()
        }
    }

    #[test]
    fn negotiate_exact_match_and_fallbacks() {
        let neg = FormatNegotiator;
        let src = PcmFormat::new(96000, 24, 2);

        // No device selected
        let no_dev = neg.negotiate(src, None, true);
        assert!(no_dev.output.is_none());
        assert!(!no_dev.native_match);

        // Shared mode
        let dac = sample_dac(vec![PcmFormat::new(96000, 24, 2)]);
        let shared = neg.negotiate(src, Some(&dac), false);
        assert!(!shared.native_match);
        assert!(shared.resampling);

        // Exact exclusive match
        let exact = neg.negotiate(src, Some(&dac), true);
        assert!(exact.native_match);
        assert!(!exact.resampling);
        assert!(!exact.format_conversion);
        assert_eq!(exact.output, Some(src));

        // Same rate, different depth -> picks 32-bit without resampling
        let depth_fallback_dac = sample_dac(vec![
            PcmFormat::new(96000, 16, 2),
            PcmFormat::new(96000, 32, 2),
        ]);
        let depth_fb = neg.negotiate(src, Some(&depth_fallback_dac), true);
        assert!(!depth_fb.native_match);
        assert!(!depth_fb.resampling);
        assert!(depth_fb.format_conversion);
        assert_eq!(depth_fb.output, Some(PcmFormat::new(96000, 32, 2)));

        // Same depth, closest rate -> resampling = true, format_conversion = false
        let rate_fallback_dac = sample_dac(vec![
            PcmFormat::new(48000, 24, 2),
            PcmFormat::new(192000, 24, 2),
        ]);
        let rate_fb = neg.negotiate(src, Some(&rate_fallback_dac), true);
        assert!(!rate_fb.native_match);
        assert!(rate_fb.resampling);
        assert!(!rate_fb.format_conversion);
        assert_eq!(rate_fb.output, Some(PcmFormat::new(48000, 24, 2)));
    }

    #[test]
    fn evaluate_reason_precedence() {
        let neg = FormatNegotiator;
        let dac = sample_dac(vec![PcmFormat::new(44100, 16, 2)]);
        let decision = neg.negotiate(PcmFormat::new(44100, 16, 2), Some(&dac), true);

        let mut input = BitPerfectEvalInput::new(&decision);
        input.exclusive_requested = true;
        input.exclusive_active = true;
        assert_eq!(neg.evaluate(&input), BitPerfectReason::BitPerfect);

        input.software_volume = true;
        assert_eq!(neg.evaluate(&input), BitPerfectReason::SoftwareVolume);

        input.pitch = 1.02;
        assert_eq!(neg.evaluate(&input), BitPerfectReason::PitchNotUnity);

        input.speed = 1.1;
        assert_eq!(neg.evaluate(&input), BitPerfectReason::SpeedNotUnity);

        input.crossfade = true;
        assert_eq!(neg.evaluate(&input), BitPerfectReason::CrossfadeActive);

        input.dsp_active = true;
        assert_eq!(neg.evaluate(&input), BitPerfectReason::DspActive);

        input.auto_mix = true;
        assert_eq!(neg.evaluate(&input), BitPerfectReason::AutoMixActive);

        input.loudness = true;
        assert_eq!(neg.evaluate(&input), BitPerfectReason::LoudnessActive);

        input.peq_active = true;
        assert_eq!(neg.evaluate(&input), BitPerfectReason::PeqActive);

        input.device_available = false;
        assert_eq!(neg.evaluate(&input), BitPerfectReason::DeviceUnavailable);
    }
}
