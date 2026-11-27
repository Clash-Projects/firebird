use std::sync::{Arc, RwLock};

use crate::models::equalizer_models::{
    ClarityPreset, EqPreset, EqualizerPresets, EqualizerState, EQ_BAND_FREQS_HZ, EQ_MAX_GAIN_DB,
};
use crate::storage::prefs::Prefs;

pub type AudioFilterSink = Arc<dyn Fn(&str) + Send + Sync>;

/// Controller for the 10-band Equalizer and Studio Master Clarity DSP chain.
pub struct EqualizerController {
    prefs: Arc<Prefs>,
    state: RwLock<EqualizerState>,
    filter_sink: RwLock<Option<AudioFilterSink>>,
}

impl EqualizerController {
    pub fn new(prefs: Arc<Prefs>) -> Self {
        Self::with_filter_sink(prefs, None)
    }

    pub fn with_filter_sink(prefs: Arc<Prefs>, filter_sink: Option<AudioFilterSink>) -> Self {
        let controller = Self {
            prefs,
            state: RwLock::new(EqualizerState::default()),
            filter_sink: RwLock::new(filter_sink),
        };
        controller.load_from_prefs();
        controller
    }

    pub fn set_filter_sink(&self, filter_sink: Option<AudioFilterSink>) {
        if let Ok(mut guard) = self.filter_sink.write() {
            *guard = filter_sink;
        }
        self.apply_to_player();
    }

    pub fn state(&self) -> EqualizerState {
        self.state
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    fn load_from_prefs(&self) {
        let enabled = self.prefs.equalizer_enabled();
        let preset_name = self.prefs.equalizer_preset();
        let studio_clarity_enabled = self.prefs.studio_clarity_enabled();
        let clarity_preset_key = self.prefs.studio_clarity_preset();

        let mut gains = EqualizerPresets::FLAT.gains_db.to_vec();
        let stored_gains_raw = self.prefs.equalizer_gains();
        if !stored_gains_raw.is_empty() {
            let parsed: Vec<f64> = stored_gains_raw
                .split(',')
                .filter_map(|s| s.trim().parse::<f64>().ok())
                .filter(|g| g.is_finite())
                .collect();
            if parsed.len() == EQ_BAND_FREQS_HZ.len() {
                gains = parsed
                    .into_iter()
                    .map(|g| g.clamp(-EQ_MAX_GAIN_DB, EQ_MAX_GAIN_DB))
                    .collect();
            }
        } else if let Some(found) = EqualizerPresets::by_name(&preset_name) {
            gains = found.gains_db.to_vec();
        }

        if let Ok(mut guard) = self.state.write() {
            *guard = EqualizerState {
                enabled,
                preset_name,
                gains_db: gains,
                studio_clarity_enabled,
                clarity_preset_key,
            };
        }

        self.apply_to_player();
    }

    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut guard) = self.state.write() {
            guard.enabled = enabled;
        }
        self.prefs.set_equalizer_enabled(enabled);
        self.apply_to_player();
    }

    pub fn toggle_enabled(&self) {
        let next = !self.state().enabled;
        self.set_enabled(next);
    }

    pub fn apply_preset(&self, preset: &EqPreset) {
        let gains = preset.gains_db.to_vec();
        let serialized = Self::serialize_gains(&gains);
        if let Ok(mut guard) = self.state.write() {
            guard.enabled = true;
            guard.preset_name = preset.name.to_string();
            guard.gains_db = gains;
        }
        self.prefs.set_equalizer_enabled(true);
        self.prefs.set_equalizer_preset(&preset.name);
        self.prefs.set_equalizer_gains(&serialized);
        self.apply_to_player();
    }

    pub fn set_band_gain(&self, band_index: usize, gain_db: f64) {
        if band_index >= EQ_BAND_FREQS_HZ.len() || !gain_db.is_finite() {
            return;
        }
        let clamped = gain_db.clamp(-EQ_MAX_GAIN_DB, EQ_MAX_GAIN_DB);
        let rounded = format!("{clamped:.1}")
            .parse::<f64>()
            .unwrap_or(clamped);

        let serialized = {
            let Ok(mut guard) = self.state.write() else {
                return;
            };
            if guard.gains_db.len() != EQ_BAND_FREQS_HZ.len() {
                guard.gains_db = vec![0.0; EQ_BAND_FREQS_HZ.len()];
            }
            guard.gains_db[band_index] = rounded;
            guard.enabled = true;
            guard.preset_name = EqualizerPresets::CUSTOM_NAME.to_string();
            Self::serialize_gains(&guard.gains_db)
        };

        self.prefs.set_equalizer_enabled(true);
        self.prefs.set_equalizer_preset(EqualizerPresets::CUSTOM_NAME);
        self.prefs.set_equalizer_gains(&serialized);
        self.apply_to_player();
    }

    pub fn set_studio_clarity_enabled(&self, enabled: bool) {
        if let Ok(mut guard) = self.state.write() {
            guard.studio_clarity_enabled = enabled;
        }
        self.prefs.set_studio_clarity_enabled(enabled);
        self.apply_to_player();
    }

    pub fn toggle_studio_clarity(&self) {
        let next = !self.state().studio_clarity_enabled;
        self.set_studio_clarity_enabled(next);
    }

    pub fn set_clarity_preset(&self, key: &str) {
        if let Ok(mut guard) = self.state.write() {
            guard.clarity_preset_key = key.to_string();
            guard.studio_clarity_enabled = true;
        }
        self.prefs.set_studio_clarity_preset(key);
        self.prefs.set_studio_clarity_enabled(true);
        self.apply_to_player();
    }

    pub fn set_clarity_target(&self, target: &ClarityPreset) {
        self.set_clarity_preset(&target.key);
    }

    pub fn set_preset(&self, preset_name: &str) {
        if let Some(preset) = EqualizerPresets::by_name(preset_name) {
            self.apply_preset(&preset);
        }
    }

    pub fn reset(&self) {
        self.apply_preset(&EqualizerPresets::FLAT);
    }

    pub fn current_mpv_filter(&self) -> String {
        Self::format_mpv_filter(&self.state())
    }

    fn apply_to_player(&self) {
        let af = self.current_mpv_filter();
        if let Ok(guard) = self.filter_sink.read() {
            if let Some(sink) = guard.as_ref() {
                sink(&af);
            }
        }
    }

    fn serialize_gains(gains: &[f64]) -> String {
        gains
            .iter()
            .map(|g| g.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn format_mpv_filter(s: &EqualizerState) -> String {
        if !s.is_processing_active() {
            return String::new();
        }
        let gains = s.combined_gains_db();
        if gains.iter().all(|g| g.abs() < 0.05) {
            return String::new();
        }

        let mut filters: Vec<String> = Vec::new();
        // Auto pre-amp: pull the whole signal down by the largest boost so the
        // EQ can never push peaks past full scale.
        let max_boost = gains.iter().copied().fold(0.0_f64, f64::max);
        // Pre-amp WITHOUT the `volume` filter: shifting every band down by the
        // largest boost gives the same headroom using `equalizer` alone.
        let shift = if max_boost > 0.05 { max_boost } else { 0.0 };

        for (i, &freq) in EQ_BAND_FREQS_HZ.iter().enumerate() {
            let gain = gains[i] - shift;
            if gain.abs() < 0.05 {
                continue;
            }
            filters.push(format!("equalizer=f={freq}:t=o:w=1:g={gain:.1}"));
        }

        // Studio Master Clarity: a touch of transient sharpening on top of the
        // curve, scaled by the output target (gentler for DACs / headphones).
        if s.studio_clarity_enabled {
            let preset = s.clarity_preset();
            let exciter_trim = preset.trims_db.get(7).copied().unwrap_or(0.0);
            let intensity = (1.3 + exciter_trim * 0.15).clamp(0.6, 2.0);
            filters.push(format!("crystalizer=i={intensity:.2}:c=1"));
        }

        format!("format=floatp,lavfi=[{}]", filters.join(","))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::EqualizerController;
    use crate::models::equalizer_models::{ClarityPresets, EqualizerPresets, EqualizerState};
    use crate::storage::prefs::Prefs;

    #[test]
    fn loads_and_persists_preset_and_band_changes() {
        let prefs = Arc::new(Prefs::in_memory());
        let emitted: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_log = Arc::clone(&emitted);

        let ctrl = EqualizerController::with_filter_sink(
            Arc::clone(&prefs),
            Some(Arc::new(move |af| {
                sink_log.lock().unwrap().push(af.to_string());
            })),
        );

        assert!(!ctrl.state().enabled);
        assert_eq!(ctrl.current_mpv_filter(), "");

        ctrl.set_preset("Rock");
        let rock = ctrl.state();
        assert!(rock.enabled);
        assert_eq!(rock.preset_name, "Rock");
        assert!(prefs.equalizer_enabled());
        assert_eq!(prefs.equalizer_preset(), "Rock");
        assert!(ctrl.current_mpv_filter().starts_with("format=floatp,lavfi=["));

        ctrl.set_band_gain(0, 12.34); // clamped to 8.0
        let custom = ctrl.state();
        assert_eq!(custom.preset_name, EqualizerPresets::CUSTOM_NAME);
        assert!((custom.gains_db[0] - 8.0).abs() < 1e-9);

        // Reloading from same prefs restores custom gains and state.
        let reloaded = EqualizerController::new(Arc::clone(&prefs));
        assert!(reloaded.state().enabled);
        assert_eq!(reloaded.state().preset_name, EqualizerPresets::CUSTOM_NAME);
        assert!((reloaded.state().gains_db[0] - 8.0).abs() < 1e-9);

        ctrl.reset();
        assert_eq!(ctrl.state().preset_name, "Default");
        assert_eq!(ctrl.current_mpv_filter(), "");
        assert!(!emitted.lock().unwrap().is_empty());
    }

    #[test]
    fn format_mpv_filter_applies_headroom_shift_and_crystalizer() {
        let mut state = EqualizerState {
            enabled: true,
            preset_name: "Custom".to_string(),
            gains_db: vec![4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            studio_clarity_enabled: false,
            clarity_preset_key: "reference".to_string(),
        };
        let eq_only = EqualizerController::format_mpv_filter(&state);
        // Max boost = 4.0, so band 0 (31 Hz) becomes 0.0 (skipped) and others shift to -4.0.
        assert!(!eq_only.contains("f=31:"));
        assert!(eq_only.contains("equalizer=f=63:t=o:w=1:g=-4.0"));
        assert!(!eq_only.contains("crystalizer"));

        state.enabled = false;
        state.studio_clarity_enabled = true;
        state.clarity_preset_key = ClarityPresets::DAC.key.to_string();
        let clarity = EqualizerController::format_mpv_filter(&state);
        // DAC exciter trim is -1.0 -> 1.3 + (-1.0 * 0.15) = 1.15
        assert!(clarity.contains("crystalizer=i=1.15:c=1"));
    }
}
