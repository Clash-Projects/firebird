//! 10-band Equalizer models and presets ported from LastWave-Native.
//!
//! Center frequencies map directly to the ISO octave bands. Gain range is
//! ±8 dB to guarantee clean, unclipped, audiophile-grade output.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

pub const EQ_BAND_FREQS_HZ: [i32; 10] = [31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];

pub const EQ_MAX_GAIN_DB: f64 = 8.0;

/// Compact frequency label under each band slider ("31", "125", "1k", "16k"...).
pub fn eq_band_label(hz: i32) -> String {
    if hz >= 1000 {
        format!("{}k", hz / 1000)
    } else {
        hz.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqPreset {
    pub name: Cow<'static, str>,
    pub gains_db: Cow<'static, [f64]>,
}

impl EqPreset {
    pub const fn new(name: &'static str, gains_db: &'static [f64; 10]) -> Self {
        Self {
            name: Cow::Borrowed(name),
            gains_db: Cow::Borrowed(gains_db),
        }
    }
}

/// Listening-tested curves from LastWave-Native.
pub struct EqualizerPresets;

impl EqualizerPresets {
    pub const CUSTOM_NAME: &'static str = "Custom";

    pub const FLAT: EqPreset = EqPreset::new(
        "Default",
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    );

    /// Crystal-open audiophile reference curve for Studio Master Clarity.
    /// Tight, controlled sub-bass depth, deliberate low-mid dip that removes
    /// congestion, natural vocal body, presence lift and a silky air shelf.
    pub const STUDIO_MASTER: EqPreset = EqPreset::new(
        "Studio Master",
        &[2.7, 2.2, 0.6, -2.6, -0.6, 1.2, 3.0, 4.0, 4.3, 4.8],
    );

    pub const VOCAL_CLARITY: EqPreset = EqPreset::new(
        "Vocal Clarity",
        &[-1.2, -0.5, 0.0, 1.0, 2.5, 3.5, 2.8, 2.0, 1.5, 1.0],
    );

    pub const DEEP_BASS_CLEAN: EqPreset = EqPreset::new(
        "Deep Bass Clean",
        &[4.8, 3.5, 2.5, 0.0, -0.5, 0.0, 0.0, 0.0, 0.5, 0.5],
    );

    pub const ROCK: EqPreset = EqPreset::new(
        "Rock",
        &[3.8, 3.0, 1.5, -1.0, -0.8, 0.5, 2.0, 3.0, 3.8, 4.0],
    );

    pub const POP: EqPreset = EqPreset::new(
        "Pop",
        &[-0.8, 0.0, 1.0, 2.0, 1.8, 1.0, 0.5, 1.0, 1.8, 2.5],
    );

    pub const ELECTRONIC: EqPreset = EqPreset::new(
        "Electronic",
        &[4.2, 3.0, 2.0, -0.5, 0.5, 2.0, 2.5, 2.5, 3.2, 4.0],
    );

    pub const RNB: EqPreset = EqPreset::new(
        "R&B",
        &[4.8, 3.5, 2.0, -0.5, -0.8, 0.5, 1.8, 2.5, 3.0, 3.0],
    );

    pub const ACOUSTIC: EqPreset = EqPreset::new(
        "Acoustic",
        &[2.8, 2.0, 1.5, 1.0, 1.8, 2.0, 1.2, 1.0, 1.2, 1.5],
    );

    pub const CLASSICAL: EqPreset = EqPreset::new(
        "Classical",
        &[2.8, 2.0, 1.5, 0.0, 0.0, 0.5, 1.8, 2.0, 2.8, 3.0],
    );

    pub const JAZZ: EqPreset = EqPreset::new(
        "Jazz",
        &[2.2, 1.5, 1.0, -0.5, 0.2, 1.0, 1.8, 2.0, 2.5, 2.5],
    );

    pub const TREBLE_AIR: EqPreset = EqPreset::new(
        "Treble Air",
        &[0.0, 0.0, -0.5, -0.5, 0.2, 1.0, 1.8, 3.0, 4.0, 5.0],
    );

    pub const BASS_BOOST: EqPreset = EqPreset::new(
        "Bass Boost",
        &[6.0, 5.0, 3.5, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    );

    pub const WARM: EqPreset = EqPreset::new(
        "Warm",
        &[2.0, 2.0, 1.5, 1.0, 0.5, 0.0, -0.5, -1.2, -2.0, -2.8],
    );

    pub const BRIGHT: EqPreset = EqPreset::new(
        "Bright",
        &[-1.0, -0.5, 0.0, 0.0, 0.0, 0.5, 1.5, 3.0, 4.0, 4.5],
    );

    pub const LATE_NIGHT: EqPreset = EqPreset::new(
        "Late Night",
        &[1.5, 1.0, 0.5, 0.0, 0.0, 0.5, 1.0, 1.0, 0.5, 0.0],
    );

    pub const SPEECH: EqPreset = EqPreset::new(
        "Speech / Podcast",
        &[-4.0, -3.0, -1.5, 0.0, 1.5, 3.0, 3.5, 2.5, 0.5, -1.0],
    );

    pub const ALL: [EqPreset; 17] = [
        Self::FLAT,
        Self::STUDIO_MASTER,
        Self::VOCAL_CLARITY,
        Self::DEEP_BASS_CLEAN,
        Self::BASS_BOOST,
        Self::WARM,
        Self::BRIGHT,
        Self::LATE_NIGHT,
        Self::ROCK,
        Self::POP,
        Self::ELECTRONIC,
        Self::RNB,
        Self::ACOUSTIC,
        Self::CLASSICAL,
        Self::JAZZ,
        Self::TREBLE_AIR,
        Self::SPEECH,
    ];

    pub fn by_name(name: &str) -> Option<EqPreset> {
        let target = name.to_lowercase();
        Self::ALL
            .iter()
            .find(|preset| preset.name.to_lowercase() == target)
            .cloned()
    }
}

/// Output preset descriptor for the native Studio Master Clarity chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClarityPreset {
    pub index: i32,
    pub key: Cow<'static, str>,
    pub display_name: Cow<'static, str>,
    pub description: Cow<'static, str>,
    pub trims_db: Cow<'static, [f64]>,
}

pub struct ClarityPresets;

impl ClarityPresets {
    pub const REFERENCE: ClarityPreset = ClarityPreset {
        index: 0,
        key: Cow::Borrowed("reference"),
        display_name: Cow::Borrowed("Reference"),
        description: Cow::Borrowed(
            "Unmodified audiophile reference curve; all stage trims 0 dB.",
        ),
        trims_db: Cow::Borrowed(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    };

    pub const SPEAKER: ClarityPreset = ClarityPreset {
        index: 1,
        key: Cow::Borrowed("speaker"),
        display_name: Cow::Borrowed("Speaker"),
        description: Cow::Borrowed(
            "Reduced sub-bass (-1.5 dB) and gentler air (-0.5 dB) for small desktop drivers.",
        ),
        trims_db: Cow::Borrowed(&[0.0, -1.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0]),
    };

    pub const HEADPHONE: ClarityPreset = ClarityPreset {
        index: 2,
        key: Cow::Borrowed("headphone"),
        display_name: Cow::Borrowed("Headphone"),
        description: Cow::Borrowed(
            "Lighter bass (-0.5 dB) and softer presence (-1.0 dB) for close-coupled drivers.",
        ),
        trims_db: Cow::Borrowed(&[0.0, -0.5, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0]),
    };

    pub const DAC: ClarityPreset = ClarityPreset {
        index: 3,
        key: Cow::Borrowed("dac"),
        display_name: Cow::Borrowed("DAC"),
        description: Cow::Borrowed(
            "Gentler air (-1.5 dB shelf, -1.0 dB exciter) for revealing external DAC chains.",
        ),
        trims_db: Cow::Borrowed(&[0.0, 0.0, 0.0, 0.0, 0.0, -1.5, 0.0, -1.0]),
    };

    pub const ALL: [ClarityPreset; 4] = [
        Self::REFERENCE,
        Self::SPEAKER,
        Self::HEADPHONE,
        Self::DAC,
    ];

    pub fn by_key(key: &str) -> ClarityPreset {
        let target = key.to_lowercase();
        Self::ALL
            .iter()
            .find(|p| p.key.to_lowercase() == target)
            .cloned()
            .unwrap_or(Self::REFERENCE)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqualizerState {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_preset_name")]
    pub preset_name: String,
    #[serde(default = "default_gains_db")]
    pub gains_db: Vec<f64>,
    #[serde(default)]
    pub studio_clarity_enabled: bool,
    #[serde(default = "default_clarity_preset_key")]
    pub clarity_preset_key: String,
}

fn default_preset_name() -> String {
    "Default".to_string()
}

fn default_gains_db() -> Vec<f64> {
    vec![0.0; EQ_BAND_FREQS_HZ.len()]
}

fn default_clarity_preset_key() -> String {
    "reference".to_string()
}

impl Default for EqualizerState {
    fn default() -> Self {
        Self {
            enabled: false,
            preset_name: default_preset_name(),
            gains_db: default_gains_db(),
            studio_clarity_enabled: false,
            clarity_preset_key: default_clarity_preset_key(),
        }
    }
}

impl EqualizerState {
    /// Active [`ClarityPreset`] descriptor.
    pub fn clarity_preset(&self) -> ClarityPreset {
        ClarityPresets::by_key(&self.clarity_preset_key)
    }

    /// Combined curve with Studio Master Clarity trims factored in.
    pub fn combined_gains_db(&self) -> Vec<f64> {
        let clarity = ClarityPresets::by_key(&self.clarity_preset_key);
        let trims = clarity.trims_db.as_ref();
        (0..EQ_BAND_FREQS_HZ.len())
            .map(|i| {
                let mut gain = if self.enabled {
                    self.gains_db.get(i).copied().unwrap_or(0.0)
                } else {
                    0.0
                };
                if self.studio_clarity_enabled {
                    let sm_gain = EqualizerPresets::STUDIO_MASTER.gains_db[i];
                    let mut trim = 0.0;
                    if i == 1 {
                        trim = trims.get(1).copied().unwrap_or(0.0); // 63Hz
                    } else if i == 3 {
                        trim = trims.get(2).copied().unwrap_or(0.0); // 250Hz
                    } else if i == 4 {
                        trim = trims.get(3).copied().unwrap_or(0.0); // 500Hz
                    } else if i == 6 || i == 7 {
                        trim = trims.get(4).copied().unwrap_or(0.0); // 2k-4k presence
                    } else if i >= 8 {
                        trim = trims.get(5).copied().unwrap_or(0.0); // 8k-16k air
                        // The "exciter" stage trims the very top band too (DAC target).
                        if i == 9 {
                            trim += trims.get(7).copied().unwrap_or(0.0);
                        }
                    }
                    gain += sm_gain + trim;
                }
                gain.clamp(-EQ_MAX_GAIN_DB, EQ_MAX_GAIN_DB)
            })
            .collect()
    }

    /// Whether any audio filtering is active (EQ enabled or Studio Clarity active).
    pub fn is_processing_active(&self) -> bool {
        self.enabled || self.studio_clarity_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_labels_and_preset_lookups() {
        assert_eq!(eq_band_label(31), "31");
        assert_eq!(eq_band_label(1000), "1k");
        assert_eq!(eq_band_label(16000), "16k");

        assert_eq!(
            EqualizerPresets::by_name("studio master").unwrap().name,
            "Studio Master"
        );
        assert!(EqualizerPresets::by_name("nonexistent").is_none());
        assert_eq!(ClarityPresets::by_key("DAC").index, 3);
        assert_eq!(ClarityPresets::by_key("unknown").key, "reference");
    }

    #[test]
    fn combined_gains_clamps_and_applies_clarity_trims() {
        let state = EqualizerState {
            enabled: true,
            preset_name: "Bass Boost".to_string(),
            gains_db: vec![6.0, 5.0, 3.5, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 5.0],
            studio_clarity_enabled: true,
            clarity_preset_key: "dac".to_string(),
        };
        let combined = state.combined_gains_db();
        assert_eq!(combined.len(), 10);
        // Band 0: 6.0 + 2.7 = 8.7 -> clamped to 8.0
        assert!((combined[0] - 8.0).abs() < 1e-9);
        // Band 8: 0.0 + 4.3 + (-1.5) = 2.8
        assert!((combined[8] - 2.8).abs() < 1e-9);
        // Band 9: 5.0 + 4.8 + (-1.5) + (-1.0) = 7.3
        assert!((combined[9] - 7.3).abs() < 1e-9);
        assert!(state.is_processing_active());
    }
}
