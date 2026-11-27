use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde_json::Value;

use super::diagnostics_log::DiagnosticsLog;

pub const PREFS_FILENAME: &str = "shared_preferences.json";
pub const CORRUPT_PREFS_FILENAME: &str = "shared_preferences.json.corrupt";

pub const ALLOWED_QUALITIES: [i64; 7] = [-1, 4, 5, 6, 7, 27, 28];

pub const MINI_PLAYER_STYLES: [&str; 5] = [
    "classic",
    "compact",
    "card",
    "ambient",
    "lyrics",
];

pub const LYRICS_ANIMATION_OPTIONS: [(&str, &str); 9] = [
    ("apple_fluid", "Apple Fluid"),
    ("karaoke_bounce", "Rhythm Bounce"),
    ("kinetic_pop", "Kinetic Pop"),
    ("neon_cyber", "Neon Cyber"),
    ("italic_wave", "Silk Italic"),
    ("glitch_snap", "Chromatic Glitch"),
    ("elastic_tilt", "3D Stage Tilt"),
    ("hyper_warp", "Hyperdrive Warp"),
    ("chaos_vortex", "Chaos Vortex"),
];

pub const LYRICS_TEXT_SCALES: [(f64, &str); 5] = [
    (0.85, "Small"),
    (1.0, "Medium"),
    (1.2, "Large"),
    (1.4, "Extra large"),
    (1.6, "Huge"),
];

pub const LYRICS_TRANSLATION_OPTIONS: [(&str, &str); 9] = [
    ("off", "Off"),
    ("en", "English"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("pt", "Portuguese"),
    ("hi", "Hindi"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
];

pub const LYRICS_AUTO_HIDE_OPTIONS: [(i64, &str); 6] = [
    (0, "Always on"),
    (2, "2 seconds"),
    (4, "4 seconds"),
    (6, "6 seconds"),
    (8, "8 seconds"),
    (10, "10 seconds"),
];

pub const NOW_PLAYING_STYLES: [&str; 3] = ["fullBleed", "minimal", "standard"];
pub const NOW_PLAYING_AUTO_HIDE_OPTIONS: [i64; 4] = [3, 4, 5, 0];

pub const DOWNLOAD_STRUCTURES: [&str; 5] = [
    "flat",
    "artist_album",
    "artist_year_album",
    "album_only",
    "year_album",
];

pub const DEFAULT_JAM_SERVER_URL: &str = "wss://jam.clashgram.org/ws";

#[derive(Debug)]
struct PrefsInner {
    file_path: Option<PathBuf>,
    data: BTreeMap<String, Value>,
}

/// Settings + session persistence backed by JSON with atomic writes and
/// corrupt-file quarantine.
///
/// Key names mirror LastWave-native `SettingsPreferences` /
/// `SessionPreferences` (`lw_*`) so behaviour stays comparable.
pub struct Prefs {
    inner: RwLock<PrefsInner>,
}

impl Prefs {
    /// Creates an in-memory `Prefs` store (used as fallback or in unit tests).
    pub fn in_memory() -> Self {
        Self {
            inner: RwLock::new(PrefsInner {
                file_path: None,
                data: BTreeMap::new(),
            }),
        }
    }

    /// Loads preferences from `<app_support_dir>/shared_preferences.json`.
    ///
    /// If the JSON file is corrupt or unreadable, it is quarantined to
    /// `shared_preferences.json.corrupt` (matching Dart's recovery path) and
    /// loading is retried; if recovery still fails, falls back to in-memory.
    pub fn load(app_support_dir: &Path) -> Self {
        let file_path = app_support_dir.join(PREFS_FILENAME);
        match Self::try_load_file(&file_path) {
            Ok(data) => Self {
                inner: RwLock::new(PrefsInner {
                    file_path: Some(file_path),
                    data,
                }),
            },
            Err(e) => {
                DiagnosticsLog::info(&format!(
                    "Prefs.load failed ({e}); recovering preferences"
                ));
                Self::quarantine_corrupt_prefs_file(app_support_dir);
                match Self::try_load_file(&file_path) {
                    Ok(data) => Self {
                        inner: RwLock::new(PrefsInner {
                            file_path: Some(file_path),
                            data,
                        }),
                    },
                    Err(retry_err) => {
                        DiagnosticsLog::info(&format!(
                            "Prefs.load retry failed ({retry_err}); using memory fallback"
                        ));
                        Self::in_memory()
                    }
                }
            }
        }
    }

    fn try_load_file(file_path: &Path) -> Result<BTreeMap<String, Value>, String> {
        if let Some(parent) = file_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        if !file_path.exists() {
            return Ok(BTreeMap::new());
        }
        let raw = fs::read_to_string(file_path).map_err(|e| e.to_string())?;
        if raw.trim().is_empty() {
            return Err("empty preferences file".to_string());
        }
        let parsed: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let obj = parsed
            .as_object()
            .ok_or_else(|| "preferences root is not a JSON object".to_string())?;
        Ok(obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }

    fn quarantine_corrupt_prefs_file(app_support_dir: &Path) {
        let file = app_support_dir.join(PREFS_FILENAME);
        if !file.exists() {
            return;
        }
        let backup = app_support_dir.join(CORRUPT_PREFS_FILENAME);
        if backup.exists() {
            let _ = fs::remove_file(&backup);
        }
        if fs::rename(&file, &backup).is_err() {
            let _ = fs::remove_file(&file);
        }
    }

    fn persist_locked(inner: &PrefsInner) {
        let Some(path) = inner.file_path.as_ref() else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if !parent.as_os_str().is_empty() && fs::create_dir_all(parent).is_err() {
            return;
        }
        let Ok(json) = serde_json::to_string_pretty(&inner.data) else {
            return;
        };
        let tmp_name = format!("{PREFS_FILENAME}.{}.tmp", std::process::id());
        let tmp_path = parent.join(tmp_name);
        let write_ok = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)
            .and_then(|mut f| {
                f.write_all(json.as_bytes())?;
                f.flush()?;
                f.sync_all()
            })
            .is_ok();
        if !write_ok {
            let _ = fs::remove_file(&tmp_path);
            return;
        }
        if fs::rename(&tmp_path, path).is_err() {
            let _ = fs::remove_file(path);
            if fs::rename(&tmp_path, path).is_err() {
                let _ = fs::remove_file(&tmp_path);
            }
        }
    }

    pub fn file_path(&self) -> Option<PathBuf> {
        self.inner.read().ok()?.file_path.clone()
    }

    fn lookup<'a>(data: &'a BTreeMap<String, Value>, key: &str) -> Option<&'a Value> {
        data.get(key)
            .or_else(|| data.get(&format!("flutter.{key}")))
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        let guard = self.inner.read().ok()?;
        Self::lookup(&guard.data, key).cloned()
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        let guard = self.inner.read().ok()?;
        Self::lookup(&guard.data, key)?.as_bool()
    }

    pub fn get_int(&self, key: &str) -> Option<i64> {
        let guard = self.inner.read().ok()?;
        let val = Self::lookup(&guard.data, key)?;
        val.as_i64()
            .or_else(|| val.as_u64().and_then(|u| i64::try_from(u).ok()))
    }

    pub fn get_double(&self, key: &str) -> Option<f64> {
        let guard = self.inner.read().ok()?;
        Self::lookup(&guard.data, key)?.as_f64()
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        let guard = self.inner.read().ok()?;
        Self::lookup(&guard.data, key)?
            .as_str()
            .map(ToString::to_string)
    }

    pub fn get_string_list(&self, key: &str) -> Option<Vec<String>> {
        let guard = self.inner.read().ok()?;
        let arr = Self::lookup(&guard.data, key)?.as_array()?;
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            out.push(item.as_str()?.to_string());
        }
        Some(out)
    }

    pub fn set_bool(&self, key: &str, value: bool) {
        self.set_value(key, Value::Bool(value));
    }

    pub fn set_int(&self, key: &str, value: i64) {
        self.set_value(key, Value::Number(value.into()));
    }

    pub fn set_double(&self, key: &str, value: f64) {
        if let Some(num) = serde_json::Number::from_f64(value) {
            self.set_value(key, Value::Number(num));
        }
    }

    pub fn set_string(&self, key: &str, value: &str) {
        self.set_value(key, Value::String(value.to_string()));
    }

    pub fn set_string_list(&self, key: &str, value: &[String]) {
        let arr = value.iter().map(|s| Value::String(s.clone())).collect();
        self.set_value(key, Value::Array(arr));
    }

    pub fn remove(&self, key: &str) {
        if let Ok(mut guard) = self.inner.write() {
            let r1 = guard.data.remove(key).is_some();
            let r2 = guard.data.remove(&format!("flutter.{key}")).is_some();
            if r1 || r2 {
                Self::persist_locked(&guard);
            }
        }
    }

    fn set_value(&self, key: &str, value: Value) {
        if let Ok(mut guard) = self.inner.write() {
            let flutter_key = format!("flutter.{key}");
            if guard.data.contains_key(&flutter_key) {
                guard.data.insert(flutter_key, value.clone());
            }
            guard.data.insert(key.to_string(), value);
            Self::persist_locked(&guard);
        }
    }

    // -- Last.fm BYOK (bring your own keys) ---------------------------------
    pub fn last_fm_api_key(&self) -> String {
        self.get_string("lw_lastfm_api_key").unwrap_or_default()
    }

    pub fn last_fm_api_secret(&self) -> String {
        self.get_string("lw_lastfm_api_secret").unwrap_or_default()
    }

    pub fn is_last_fm_configured(&self) -> bool {
        !self.last_fm_api_key().is_empty() && !self.last_fm_api_secret().is_empty()
    }

    pub fn save_last_fm_keys(&self, api_key: &str, api_secret: &str) {
        if let Ok(mut guard) = self.inner.write() {
            guard.data.insert(
                "lw_lastfm_api_key".to_string(),
                Value::String(api_key.to_string()),
            );
            guard.data.insert(
                "lw_lastfm_api_secret".to_string(),
                Value::String(api_secret.to_string()),
            );
            Self::persist_locked(&guard);
        }
    }

    pub fn clear_last_fm_keys(&self) {
        if let Ok(mut guard) = self.inner.write() {
            let a = guard.data.remove("lw_lastfm_api_key").is_some();
            let b = guard.data.remove("lw_lastfm_api_secret").is_some();
            if a || b {
                Self::persist_locked(&guard);
            }
        }
    }

    // -- Guest mode (keyless entry) ------------------------------------------
    pub fn is_guest(&self) -> bool {
        self.get_bool("lw_guest_mode").unwrap_or(false)
    }

    pub fn set_guest_mode(&self, v: bool) {
        self.set_bool("lw_guest_mode", v);
    }

    // -- Last.fm session -----------------------------------------------------
    pub fn session_key(&self) -> String {
        self.get_string("lw_sessionkey").unwrap_or_default()
    }

    pub fn username(&self) -> String {
        self.get_string("lw_username").unwrap_or_default()
    }

    pub fn is_authenticated(&self) -> bool {
        !self.username().is_empty() && !self.session_key().is_empty()
    }

    pub fn save_session(&self, session_key: &str, username: &str) {
        if let Ok(mut guard) = self.inner.write() {
            guard.data.insert(
                "lw_sessionkey".to_string(),
                Value::String(session_key.to_string()),
            );
            guard.data.insert(
                "lw_username".to_string(),
                Value::String(username.to_string()),
            );
            // Drop any legacy guest flag from older builds.
            guard.data.remove("lw_guest_mode");
            Self::persist_locked(&guard);
        }
    }

    pub fn sign_out(&self) {
        if let Ok(mut guard) = self.inner.write() {
            let a = guard.data.remove("lw_sessionkey").is_some();
            let b = guard.data.remove("lw_username").is_some();
            let c = guard.data.remove("lw_guest_mode").is_some();
            if a || b || c {
                Self::persist_locked(&guard);
            }
        }
    }

    // -- Audio quality -------------------------------------------------------
    pub fn clamp_quality(q: i64) -> i64 {
        if ALLOWED_QUALITIES.contains(&q) {
            q
        } else {
            27
        }
    }

    pub fn prefer_lossless(&self) -> bool {
        self.get_bool("lw_prefer_lossless_streaming")
            .unwrap_or(true)
    }

    pub fn lossless_quality(&self) -> i64 {
        Self::clamp_quality(self.get_int("lw_lossless_quality").unwrap_or(27))
    }

    pub fn download_quality(&self) -> i64 {
        Self::clamp_quality(self.get_int("lw_download_quality").unwrap_or(27))
    }

    pub fn set_prefer_lossless(&self, v: bool) {
        self.set_bool("lw_prefer_lossless_streaming", v);
    }

    pub fn set_lossless_quality(&self, q: i64) {
        self.set_int("lw_lossless_quality", Self::clamp_quality(q));
    }

    pub fn set_download_quality(&self, q: i64) {
        self.set_int("lw_download_quality", Self::clamp_quality(q));
    }

    // -- Window behaviour ----------------------------------------------------
    pub fn close_to_tray(&self) -> bool {
        self.get_bool("lw_close_to_tray").unwrap_or(false)
    }

    pub fn set_close_to_tray(&self, v: bool) {
        self.set_bool("lw_close_to_tray", v);
    }

    pub fn mini_player_style(&self) -> String {
        let raw = self
            .get_string("lw_mini_style")
            .unwrap_or_else(|| "lyrics".to_string());
        if MINI_PLAYER_STYLES.contains(&raw.as_str()) {
            raw
        } else {
            "lyrics".to_string()
        }
    }

    pub fn set_mini_player_style(&self, style: &str) {
        let chosen = if MINI_PLAYER_STYLES.contains(&style) {
            style
        } else {
            "lyrics"
        };
        self.set_string("lw_mini_style", chosen);
    }

    // -- Playback behaviour --------------------------------------------------
    pub fn autoplay_similar(&self) -> bool {
        self.get_bool("lw_autoplay_similar").unwrap_or(true)
    }

    pub fn set_autoplay_similar(&self, v: bool) {
        self.set_bool("lw_autoplay_similar", v);
    }

    pub fn crossfade_enabled(&self) -> bool {
        self.get_bool("lw_crossfade_enabled").unwrap_or(false)
    }

    pub fn crossfade_seconds(&self) -> i64 {
        self.get_int("lw_crossfade_seconds")
            .unwrap_or(5)
            .clamp(3, 12)
    }

    pub fn set_crossfade(&self, enabled: bool, seconds: Option<i64>) {
        if let Ok(mut guard) = self.inner.write() {
            guard
                .data
                .insert("lw_crossfade_enabled".to_string(), Value::Bool(enabled));
            if enabled {
                guard
                    .data
                    .insert("lw_automix_enabled".to_string(), Value::Bool(false));
            }
            if let Some(sec) = seconds {
                guard.data.insert(
                    "lw_crossfade_seconds".to_string(),
                    Value::Number(sec.clamp(3, 12).into()),
                );
            }
            Self::persist_locked(&guard);
        }
    }

    pub fn set_crossfade_seconds(&self, seconds: i64) {
        self.set_int("lw_crossfade_seconds", seconds.clamp(3, 12));
    }

    pub fn bit_perfect(&self) -> bool {
        self.get_bool("lw_bit_perfect").unwrap_or(false)
    }

    pub fn set_bit_perfect(&self, v: bool) {
        self.set_bool("lw_bit_perfect", v);
    }

    pub fn audio_device_id(&self) -> String {
        self.get_string("lw_audio_device_id").unwrap_or_default()
    }

    pub fn set_audio_device_id(&self, v: &str) {
        self.set_string("lw_audio_device_id", v);
    }

    pub fn playlist_sort(&self, id: i64) -> String {
        self.get_string(&format!("lw_playlist_sort_{id}"))
            .unwrap_or_else(|| "default:asc".to_string())
    }

    pub fn set_playlist_sort(&self, id: i64, v: &str) {
        self.set_string(&format!("lw_playlist_sort_{id}"), v);
    }

    pub fn wasapi_exclusive(&self) -> bool {
        self.get_bool("lw_wasapi_exclusive").unwrap_or(false)
    }

    pub fn set_wasapi_exclusive(&self, v: bool) {
        self.set_bool("lw_wasapi_exclusive", v);
    }

    // -- Loudness ------------------------------------------------------------
    pub fn loudness_mode(&self) -> String {
        let raw = self
            .get_string("lw_loudness_mode")
            .unwrap_or_else(|| "off".to_string())
            .to_lowercase();
        if raw == "track" || raw == "album" {
            raw
        } else {
            "off".to_string()
        }
    }

    pub fn set_loudness_mode(&self, mode: &str) {
        let raw = mode.to_lowercase();
        let stored = if raw == "track" || raw == "album" {
            raw.as_str()
        } else {
            "off"
        };
        self.set_string("lw_loudness_mode", stored);
    }

    pub fn headphone_profile(&self) -> String {
        self.get_string("lw_headphone_profile")
            .unwrap_or_else(|| "off".to_string())
    }

    pub fn set_headphone_profile(&self, id: &str) {
        self.set_string("lw_headphone_profile", id.trim());
    }

    // -- Timed AutoMix -------------------------------------------------------
    pub fn auto_mix_enabled(&self) -> bool {
        self.get_bool("lw_automix_enabled").unwrap_or(false)
    }

    pub fn auto_mix_bars(&self) -> i64 {
        if self.get_int("lw_automix_bars").unwrap_or(16) >= 16 {
            16
        } else {
            8
        }
    }

    pub fn auto_mix_spectral(&self) -> bool {
        self.get_bool("lw_automix_spectral").unwrap_or(true)
    }

    pub fn set_auto_mix_enabled(&self, enabled: bool) {
        if let Ok(mut guard) = self.inner.write() {
            guard
                .data
                .insert("lw_automix_enabled".to_string(), Value::Bool(enabled));
            if enabled {
                guard
                    .data
                    .insert("lw_crossfade_enabled".to_string(), Value::Bool(false));
            }
            Self::persist_locked(&guard);
        }
    }

    pub fn set_auto_mix_bars(&self, bars: i64) {
        self.set_int("lw_automix_bars", if bars >= 16 { 16 } else { 8 });
    }

    pub fn set_auto_mix_spectral(&self, enabled: bool) {
        self.set_bool("lw_automix_spectral", enabled);
    }

    // -- Downloads -----------------------------------------------------------
    pub fn download_lyrics(&self) -> bool {
        self.get_bool("lw_download_lyrics").unwrap_or(true)
    }

    pub fn set_download_lyrics(&self, v: bool) {
        self.set_bool("lw_download_lyrics", v);
    }

    pub fn download_dir(&self) -> String {
        self.get_string("lw_download_dir").unwrap_or_default()
    }

    pub fn set_download_dir(&self, v: &str) {
        self.set_string("lw_download_dir", v.trim());
    }

    pub fn reset_download_dir(&self) {
        self.remove("lw_download_dir");
    }

    // -- Lyrics --------------------------------------------------------------
    pub fn word_by_word(&self) -> bool {
        self.get_bool("lw_word_by_word").unwrap_or(true)
    }

    pub fn set_word_by_word(&self, v: bool) {
        self.set_bool("lw_word_by_word", v);
    }

    pub fn lyrics_provider_id(&self) -> String {
        self.get_string("lw_lyrics_provider")
            .unwrap_or_else(|| "auto".to_string())
    }

    pub fn set_lyrics_provider_id(&self, v: &str) {
        self.set_string("lw_lyrics_provider", v);
    }

    pub fn lyrics_animation(&self) -> String {
        let v = self
            .get_string("lw_lyrics_animation")
            .unwrap_or_else(|| "apple_fluid".to_string());
        if LYRICS_ANIMATION_OPTIONS.iter().any(|(k, _)| *k == v) {
            v
        } else {
            "apple_fluid".to_string()
        }
    }

    pub fn set_lyrics_animation(&self, v: &str) {
        let chosen = if LYRICS_ANIMATION_OPTIONS.iter().any(|(k, _)| *k == v) {
            v
        } else {
            "apple_fluid"
        };
        self.set_string("lw_lyrics_animation", chosen);
    }

    pub fn lyrics_text_scale(&self) -> f64 {
        let raw = self.get_double("lw_lyrics_text_scale").unwrap_or(1.0);
        nearest_double(raw, LYRICS_TEXT_SCALES.iter().map(|(s, _)| *s))
    }

    pub fn set_lyrics_text_scale(&self, v: f64) {
        let snapped = nearest_double(v, LYRICS_TEXT_SCALES.iter().map(|(s, _)| *s));
        self.set_double("lw_lyrics_text_scale", snapped);
    }

    pub fn lyrics_translation_lang(&self) -> String {
        let v = self
            .get_string("lw_lyrics_translation")
            .unwrap_or_else(|| "off".to_string());
        if LYRICS_TRANSLATION_OPTIONS.iter().any(|(k, _)| *k == v) {
            v
        } else {
            "off".to_string()
        }
    }

    pub fn set_lyrics_translation_lang(&self, v: &str) {
        let chosen = if LYRICS_TRANSLATION_OPTIONS.iter().any(|(k, _)| *k == v) {
            v
        } else {
            "off"
        };
        self.set_string("lw_lyrics_translation", chosen);
    }

    pub fn lyrics_auto_hide_seconds(&self) -> i64 {
        let v = self.get_int("lw_lyrics_autohide_seconds").unwrap_or(0);
        if v <= 0 {
            return 0;
        }
        nearest_int(v, &[2, 4, 6, 8, 10])
    }

    pub fn set_lyrics_auto_hide_seconds(&self, seconds: i64) {
        let v = if seconds <= 0 {
            0
        } else {
            nearest_int(seconds, &[2, 4, 6, 8, 10])
        };
        self.set_int("lw_lyrics_autohide_seconds", v);
    }

    // -- Appearance ----------------------------------------------------------
    pub fn amoled(&self) -> bool {
        self.get_bool("lw_amoled").unwrap_or(false)
    }

    pub fn set_amoled(&self, v: bool) {
        self.set_bool("lw_amoled", v);
    }

    pub fn theme_mode(&self) -> String {
        self.get_string("lw_theme_mode")
            .unwrap_or_else(|| "dark".to_string())
    }

    pub fn set_theme_mode(&self, v: &str) {
        self.set_string("lw_theme_mode", v);
    }

    pub fn is_light(&self) -> bool {
        self.theme_mode() == "light"
    }

    pub fn is_custom_theme(&self) -> bool {
        self.theme_mode() == "custom"
    }

    pub fn is_artwork_theme(&self) -> bool {
        self.theme_mode() == "artwork"
    }

    pub fn custom_theme_base(&self) -> i64 {
        self.get_int("lw_custom_theme_base").unwrap_or(0xFF1A2A44)
    }

    pub fn set_custom_theme_base(&self, v: i64) {
        self.set_int("lw_custom_theme_base", v);
    }

    pub fn accent_mode(&self) -> String {
        self.get_string("lw_accent_mode")
            .unwrap_or_else(|| "manual".to_string())
    }

    pub fn set_accent_mode(&self, v: &str) {
        self.set_string("lw_accent_mode", v);
    }

    pub fn accent_color(&self) -> i64 {
        self.get_int("lw_accent").unwrap_or(0xFFE03030)
    }

    pub fn set_accent_color(&self, v: i64) {
        self.set_int("lw_accent", v);
    }

    pub fn dynamic_now_playing(&self) -> bool {
        self.get_bool("lw_dynamic_now_playing").unwrap_or(false)
    }

    pub fn set_dynamic_now_playing(&self, v: bool) {
        self.set_bool("lw_dynamic_now_playing", v);
    }

    pub fn macos_now_playing(&self) -> bool {
        self.get_bool("lw_macos_now_playing").unwrap_or(false)
    }

    pub fn set_macos_now_playing(&self, v: bool) {
        self.set_bool("lw_macos_now_playing", v);
    }

    // -- Haze / material -----------------------------------------------------
    pub fn haze_material(&self) -> String {
        self.get_string("lw_haze_material")
            .unwrap_or_else(|| "automatic".to_string())
    }

    pub fn set_haze_material(&self, v: &str) {
        self.set_string("lw_haze_material", v);
    }

    pub fn haze_intensity(&self) -> String {
        self.get_string("lw_haze_intensity")
            .unwrap_or_else(|| "medium".to_string())
    }

    pub fn set_haze_intensity(&self, v: &str) {
        self.set_string("lw_haze_intensity", v);
    }

    pub fn ui_scale(&self) -> f64 {
        self.get_double("lw_ui_scale").unwrap_or(1.0)
    }

    pub fn set_ui_scale(&self, v: f64) {
        self.set_double("lw_ui_scale", v);
    }

    pub fn accent_source(&self) -> String {
        let v = self
            .get_string("lw_accent_source")
            .or_else(|| self.get_string("lw_accent_mode"))
            .unwrap_or_else(|| "custom".to_string());
        if v == "manual" {
            "custom".to_string()
        } else {
            v
        }
    }

    pub fn set_accent_source(&self, v: &str) {
        if let Ok(mut guard) = self.inner.write() {
            guard
                .data
                .insert("lw_accent_source".to_string(), Value::String(v.to_string()));
            guard
                .data
                .insert("lw_accent_mode".to_string(), Value::String(v.to_string()));
            Self::persist_locked(&guard);
        }
    }

    pub fn liquid_glass(&self) -> bool {
        self.get_bool("lw_liquid_glass").unwrap_or(false)
    }

    pub fn set_liquid_glass(&self, v: bool) {
        self.set_bool("lw_liquid_glass", v);
    }

    pub fn glassmorphism(&self) -> bool {
        self.get_bool("lw_glassmorphism").unwrap_or(false)
    }

    pub fn set_glassmorphism(&self, v: bool) {
        self.set_bool("lw_glassmorphism", v);
    }

    pub fn app_font(&self) -> String {
        self.get_string("lw_app_font").unwrap_or_default()
    }

    pub fn set_app_font(&self, v: &str) {
        self.set_string("lw_app_font", v);
    }

    pub fn wavy_seekbar(&self) -> bool {
        self.get_bool("lw_wavy_seekbar").unwrap_or(false)
    }

    pub fn set_wavy_seekbar(&self, v: bool) {
        self.set_bool("lw_wavy_seekbar", v);
    }

    // -- Scrobbler -----------------------------------------------------------
    pub fn scrobbler_enabled(&self) -> bool {
        self.get_bool("lw_scrobbler_enabled").unwrap_or(false)
    }

    pub fn listen_brainz_token(&self) -> String {
        self.get_string("lw_lb_token").unwrap_or_default()
    }

    pub fn listen_brainz_user(&self) -> String {
        self.get_string("lw_lb_user").unwrap_or_default()
    }

    pub fn listen_brainz_server(&self) -> String {
        self.get_string("lw_lb_server").unwrap_or_default()
    }

    pub fn set_listen_brainz(&self, token: &str, user: &str, server: &str) {
        if let Ok(mut guard) = self.inner.write() {
            guard
                .data
                .insert("lw_lb_token".to_string(), Value::String(token.to_string()));
            guard
                .data
                .insert("lw_lb_user".to_string(), Value::String(user.to_string()));
            guard.data.insert(
                "lw_lb_server".to_string(),
                Value::String(server.to_string()),
            );
            Self::persist_locked(&guard);
        }
    }

    pub fn libre_fm_session(&self) -> String {
        self.get_string("lw_librefm_session").unwrap_or_default()
    }

    pub fn libre_fm_user(&self) -> String {
        self.get_string("lw_librefm_user").unwrap_or_default()
    }

    pub fn libre_fm_server(&self) -> String {
        self.get_string("lw_librefm_server").unwrap_or_default()
    }

    pub fn set_libre_fm(&self, session: &str, user: &str, server: &str) {
        if let Ok(mut guard) = self.inner.write() {
            guard.data.insert(
                "lw_librefm_session".to_string(),
                Value::String(session.to_string()),
            );
            guard.data.insert(
                "lw_librefm_user".to_string(),
                Value::String(user.to_string()),
            );
            guard.data.insert(
                "lw_librefm_server".to_string(),
                Value::String(server.to_string()),
            );
            Self::persist_locked(&guard);
        }
    }

    pub fn primary_scrobbler(&self) -> String {
        self.get_string("lw_primary_scrobbler").unwrap_or_default()
    }

    pub fn set_primary_scrobbler(&self, v: &str) {
        self.set_string("lw_primary_scrobbler", v);
    }

    pub fn scrobble_now_playing(&self) -> bool {
        self.get_bool("lw_submit_now_playing").unwrap_or(true)
    }

    pub fn scrobble_percent(&self) -> i64 {
        self.get_int("lw_scrobble_percent")
            .unwrap_or(50)
            .clamp(25, 90)
    }

    pub fn set_scrobbler(
        &self,
        enabled: Option<bool>,
        now_playing: Option<bool>,
        percent: Option<i64>,
    ) {
        if let Ok(mut guard) = self.inner.write() {
            let mut changed = false;
            if let Some(e) = enabled {
                guard
                    .data
                    .insert("lw_scrobbler_enabled".to_string(), Value::Bool(e));
                changed = true;
            }
            if let Some(np) = now_playing {
                guard
                    .data
                    .insert("lw_submit_now_playing".to_string(), Value::Bool(np));
                changed = true;
            }
            if let Some(p) = percent {
                guard.data.insert(
                    "lw_scrobble_percent".to_string(),
                    Value::Number(p.clamp(25, 90).into()),
                );
                changed = true;
            }
            if changed {
                Self::persist_locked(&guard);
            }
        }
    }

    // -- YouTube Music -------------------------------------------------------
    pub fn sync_yt_history(&self) -> bool {
        self.get_bool("lw_sync_yt_history").unwrap_or(true)
    }

    pub fn sync_yt_library(&self) -> bool {
        self.get_bool("lw_sync_yt_library").unwrap_or(true)
    }

    pub fn set_sync_yt_library(&self, v: bool) {
        self.set_bool("lw_sync_yt_library", v);
    }

    pub fn set_sync_yt_history(&self, v: bool) {
        self.set_bool("lw_sync_yt_history", v);
    }

    pub fn disable_po_token(&self) -> bool {
        self.get_bool("lw_disable_potoken").unwrap_or(false)
    }

    pub fn set_disable_po_token(&self, v: bool) {
        self.set_bool("lw_disable_potoken", v);
    }

    // -- Desktop-app Parity (Karaoke Lyrics & Visualizer & CD Mode) ----------
    pub fn get_lyrics_offset(&self, track_key: &str) -> i64 {
        self.get_int(&format!("lw_lyrics_offset_{track_key}"))
            .unwrap_or(0)
    }

    pub fn set_lyrics_offset(&self, track_key: &str, offset_ms: i64) {
        self.set_int(&format!("lw_lyrics_offset_{track_key}"), offset_ms);
    }

    pub fn reset_lyrics_offset(&self, track_key: &str) {
        self.remove(&format!("lw_lyrics_offset_{track_key}"));
    }

    pub fn lyrics_transliteration(&self) -> bool {
        self.get_bool("lw_lyrics_transliteration").unwrap_or(true)
    }

    pub fn set_lyrics_transliteration(&self, v: bool) {
        self.set_bool("lw_lyrics_transliteration", v);
    }

    pub fn visualizer_enabled(&self) -> bool {
        self.get_bool("lw_visualizer_enabled").unwrap_or(true)
    }

    pub fn set_visualizer_enabled(&self, v: bool) {
        self.set_bool("lw_visualizer_enabled", v);
    }

    pub fn ambient_style(&self) -> String {
        self.get_string("lw_ambient_style")
            .unwrap_or_else(|| "apple".to_string())
    }

    pub fn set_ambient_style(&self, v: &str) {
        self.set_string("lw_ambient_style", v);
    }

    pub fn now_playing_style(&self) -> String {
        let raw = self
            .get_string("lw_now_playing_style")
            .unwrap_or_else(|| "fullBleed".to_string());
        if NOW_PLAYING_STYLES.contains(&raw.as_str()) {
            raw
        } else {
            "fullBleed".to_string()
        }
    }

    pub fn set_now_playing_style(&self, style: &str) {
        let chosen = if NOW_PLAYING_STYLES.contains(&style) {
            style
        } else {
            "fullBleed"
        };
        self.set_string("lw_now_playing_style", chosen);
    }

    pub fn now_playing_auto_hide_seconds(&self) -> i64 {
        let raw = self.get_int("lw_now_playing_autohide_sec").unwrap_or(3);
        if NOW_PLAYING_AUTO_HIDE_OPTIONS.contains(&raw) {
            raw
        } else {
            3
        }
    }

    pub fn set_now_playing_auto_hide_seconds(&self, seconds: i64) {
        let chosen = if NOW_PLAYING_AUTO_HIDE_OPTIONS.contains(&seconds) {
            seconds
        } else {
            3
        };
        self.set_int("lw_now_playing_autohide_sec", chosen);
    }

    pub fn cd_mode(&self) -> bool {
        self.get_bool("lw_cd_mode").unwrap_or(false)
    }

    pub fn set_cd_mode(&self, v: bool) {
        self.set_bool("lw_cd_mode", v);
    }

    // -- Discord Rich Presence -----------------------------------------------
    pub fn discord_rich_presence(&self) -> bool {
        self.get_bool("lw_discord_rich_presence").unwrap_or(true)
    }

    pub fn set_discord_rich_presence(&self, v: bool) {
        self.set_bool("lw_discord_rich_presence", v);
    }

    // -- Diagnostics ---------------------------------------------------------
    pub fn verbose_logs(&self) -> bool {
        self.get_bool("lw_verbose_logs").unwrap_or(false)
    }

    pub fn set_verbose_logs(&self, v: bool) {
        self.set_bool("lw_verbose_logs", v);
    }

    // -- Addon sources (personal addon URLs) ---------------------------------
    pub fn addons_enabled(&self) -> bool {
        self.get_bool("lw_addons_enabled").unwrap_or(true)
    }

    pub fn set_addons_enabled(&self, v: bool) {
        self.set_bool("lw_addons_enabled", v);
    }

    pub fn addon_urls(&self) -> Vec<String> {
        let raw = self.get_string("lw_addon_urls").unwrap_or_default();
        decode_trimmed_string_list(&raw)
    }

    pub fn set_addon_urls(&self, urls: &[String]) {
        let cleaned: Vec<String> = urls
            .iter()
            .map(|e| e.trim().to_string())
            .filter(|e| !e.is_empty())
            .collect();
        if let Ok(encoded) = serde_json::to_string(&cleaned) {
            self.set_string("lw_addon_urls", &encoded);
        }
    }

    pub fn disabled_addon_urls(&self) -> BTreeSet<String> {
        let raw = self.get_string("lw_disabled_addon_urls").unwrap_or_default();
        decode_trimmed_string_list(&raw).into_iter().collect()
    }

    pub fn set_disabled_addon_urls(&self, urls: &BTreeSet<String>) {
        let cleaned: Vec<String> = urls
            .iter()
            .map(|e| e.trim().to_string())
            .filter(|e| !e.is_empty())
            .collect();
        if let Ok(encoded) = serde_json::to_string(&cleaned) {
            self.set_string("lw_disabled_addon_urls", &encoded);
        }
    }

    pub fn is_addon_enabled(&self, url: &str) -> bool {
        self.addons_enabled() && !self.disabled_addon_urls().contains(url.trim())
    }

    pub fn set_addon_enabled(&self, url: &str, enabled: bool) {
        let mut next = self.disabled_addon_urls();
        let clean = url.trim().to_string();
        if enabled {
            next.remove(&clean);
        } else if !clean.is_empty() {
            next.insert(clean);
        }
        self.set_disabled_addon_urls(&next);
    }

    pub fn active_addon_urls(&self) -> Vec<String> {
        if !self.addons_enabled() {
            return Vec::new();
        }
        let disabled = self.disabled_addon_urls();
        self.addon_urls()
            .into_iter()
            .filter(|u| !disabled.contains(u))
            .collect()
    }

    // -- Equalizer & Studio Master Clarity -----------------------------------
    pub fn equalizer_enabled(&self) -> bool {
        self.get_bool("lw_eq_enabled").unwrap_or(false)
    }

    pub fn set_equalizer_enabled(&self, v: bool) {
        self.set_bool("lw_eq_enabled", v);
    }

    pub fn equalizer_preset(&self) -> String {
        self.get_string("lw_eq_preset")
            .unwrap_or_else(|| "Default".to_string())
    }

    pub fn set_equalizer_preset(&self, v: &str) {
        self.set_string("lw_eq_preset", v);
    }

    pub fn equalizer_gains(&self) -> String {
        self.get_string("lw_eq_gains").unwrap_or_default()
    }

    pub fn set_equalizer_gains(&self, v: &str) {
        self.set_string("lw_eq_gains", v);
    }

    pub fn studio_clarity_enabled(&self) -> bool {
        self.get_bool("lw_studio_clarity_enabled").unwrap_or(false)
    }

    pub fn set_studio_clarity_enabled(&self, v: bool) {
        self.set_bool("lw_studio_clarity_enabled", v);
    }

    pub fn studio_clarity_preset(&self) -> String {
        self.get_string("lw_studio_clarity_preset")
            .unwrap_or_else(|| "reference".to_string())
    }

    pub fn set_studio_clarity_preset(&self, v: &str) {
        self.set_string("lw_studio_clarity_preset", v);
    }

    // -- Profile picture & display name (local overrides) --------------------
    pub fn profile_picture_path(&self) -> String {
        self.get_string("lw_profile_picture_path")
            .unwrap_or_default()
    }

    pub fn set_profile_picture_path(&self, path: &str) {
        self.set_string("lw_profile_picture_path", path.trim());
    }

    pub fn display_name(&self) -> String {
        self.get_string("lw_display_name").unwrap_or_default()
    }

    pub fn set_display_name(&self, name: &str) {
        self.set_string("lw_display_name", name.trim());
    }

    // -- App Updates ---------------------------------------------------------
    pub fn ignored_update_version(&self) -> String {
        self.get_string("lw_ignored_update_version")
            .unwrap_or_default()
    }

    pub fn set_ignored_update_version(&self, v: &str) {
        self.set_string("lw_ignored_update_version", v);
    }

    pub fn clear_ignored_update_version(&self) {
        self.remove("lw_ignored_update_version");
    }

    pub fn podcasts_enabled(&self) -> bool {
        self.get_bool("lw_podcasts_enabled").unwrap_or(true)
    }

    pub fn set_podcasts_enabled(&self, v: bool) {
        self.set_bool("lw_podcasts_enabled", v);
    }

    // -- Language ------------------------------------------------------------
    pub fn app_language_tag(&self) -> String {
        self.get_string("app_language_tag")
            .unwrap_or_else(|| "system".to_string())
    }

    pub fn set_app_language_tag(&self, tag: &str) {
        self.set_string("app_language_tag", tag.trim());
    }

    // -- Jam -----------------------------------------------------------------
    pub fn jam_server_url(&self) -> String {
        let raw = self
            .get_string("jamServerUrl")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if raw.is_empty() || raw.contains("jam.clashprojects.qd.je") {
            DEFAULT_JAM_SERVER_URL.to_string()
        } else {
            raw
        }
    }

    pub fn set_jam_server_url(&self, url: &str) {
        self.set_string("jamServerUrl", url.trim());
    }

    pub fn jam_display_name(&self) -> String {
        self.get_string("lw_jam_display_name").unwrap_or_default()
    }

    pub fn set_jam_display_name(&self, name: &str) {
        self.set_string("lw_jam_display_name", name.trim());
    }

    // -- Canvas & animated artwork -------------------------------------------
    pub fn canvas_enabled(&self) -> bool {
        self.get_bool("lw_canvas_enabled").unwrap_or(true)
    }

    pub fn set_canvas_enabled(&self, v: bool) {
        self.set_bool("lw_canvas_enabled", v);
    }

    pub fn spotify_canvas_enabled(&self) -> bool {
        self.get_bool("spotify_canvas_enabled").unwrap_or(true)
    }

    pub fn set_spotify_canvas_enabled(&self, v: bool) {
        self.set_bool("spotify_canvas_enabled", v);
    }

    pub fn prefer_spotify_canvas(&self) -> bool {
        self.get_bool("lw_prefer_spotify_canvas").unwrap_or(false)
    }

    pub fn set_prefer_spotify_canvas(&self, v: bool) {
        self.set_bool("lw_prefer_spotify_canvas", v);
    }

    pub fn recommendation_source(&self) -> String {
        if self.get_string("lw_reco_source").as_deref() == Some("spotify") {
            "spotify".to_string()
        } else {
            "youtube".to_string()
        }
    }

    pub fn set_recommendation_source(&self, v: &str) {
        self.set_string(
            "lw_reco_source",
            if v == "spotify" { "spotify" } else { "youtube" },
        );
    }

    pub fn spotify_device_id(&self) -> String {
        self.get_string("spotify_device_id").unwrap_or_default()
    }

    pub fn set_spotify_device_id(&self, v: &str) {
        self.set_string("spotify_device_id", v);
    }

    pub fn spotify_otp_secret(&self) -> String {
        self.get_string("spotify_otp_secret").unwrap_or_default()
    }

    pub fn set_spotify_otp_secret(&self, v: &str) {
        self.set_string("spotify_otp_secret", v);
    }

    // -- Download folder layout ----------------------------------------------
    pub fn download_structure(&self) -> String {
        let raw = self
            .get_string("lw_download_structure")
            .unwrap_or_else(|| "flat".to_string());
        if DOWNLOAD_STRUCTURES.contains(&raw.as_str()) {
            raw
        } else {
            "flat".to_string()
        }
    }

    pub fn set_download_structure(&self, id: &str) {
        self.set_string("lw_download_structure", id);
    }

    // -- Portable backup (non-secret prefs only) -----------------------------
    pub fn is_secret_pref(key: &str) -> bool {
        let k = key.to_lowercase();
        const EXACT: [&str; 13] = [
            "lw_sessionkey",
            "lw_lastfm_api_key",
            "lw_lastfm_api_secret",
            "lw_apikey",
            "lw_apisecret",
            "lw_addon_url",
            "lw_addon_urls",
            "ytm_cookies",
            "spotify_sp_dc",
            "spotify_personal_token",
            "spotify_client_token",
            "spotify_personal_token_expires",
            "spotify_client_token_expires",
        ];
        if EXACT.contains(&k.as_str()) {
            return true;
        }
        if k.contains("cookie")
            || k.contains("secret")
            || k.contains("sp_dc")
            || k.contains("sessionkey")
        {
            return true;
        }
        false
    }

    pub fn is_device_bound_pref(key: &str) -> bool {
        let k = key.to_lowercase();
        matches!(
            k.as_str(),
            "lw_download_dir"
                | "lw_audio_device_id"
                | "lw_username"
                | "lw_profile_picture_path"
        )
    }

    pub fn is_portable(key: &str) -> bool {
        !Self::is_secret_pref(key) && !Self::is_device_bound_pref(key)
    }

    pub fn portable_pref_keys(&self) -> BTreeSet<String> {
        let Ok(guard) = self.inner.read() else {
            return BTreeSet::new();
        };
        guard
            .data
            .keys()
            .filter(|k| Self::is_portable(k))
            .cloned()
            .collect()
    }

    pub fn portable_pref(&self, key: &str) -> Option<Value> {
        if !Self::is_portable(key) {
            return None;
        }
        self.get(key)
    }

    pub fn set_portable_pref(&self, key: &str, value: Value) {
        if !Self::is_portable(key) {
            return;
        }
        match value {
            Value::Bool(b) => self.set_bool(key, b),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    self.set_int(key, i);
                } else if let Some(f) = n.as_f64() {
                    self.set_double(key, f);
                }
            }
            Value::String(s) => self.set_string(key, &s),
            Value::Array(arr) => {
                let strings: Vec<String> = arr
                    .into_iter()
                    .map(|v| match v {
                        Value::String(s) => s,
                        other => other.to_string(),
                    })
                    .collect();
                self.set_string_list(key, &strings);
            }
            Value::Null | Value::Object(_) => {}
        }
    }
}

fn decode_trimmed_string_list(raw: &str) -> Vec<String> {
    if raw.is_empty() {
        return Vec::new();
    }
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|v| match v {
            Value::String(s) => Some(s),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn nearest_double(value: f64, steps: impl IntoIterator<Item = f64>) -> f64 {
    let mut iter = steps.into_iter();
    let mut best = iter.next().unwrap_or(1.0);
    let mut best_dist = (best - value).abs();
    for step in iter {
        let dist = (step - value).abs();
        if dist < best_dist {
            best_dist = dist;
            best = step;
        }
    }
    best
}

fn nearest_int(value: i64, steps: &[i64]) -> i64 {
    let mut best = steps[0];
    let mut best_dist = i64::MAX;
    for &step in steps {
        let dist = (step - value).abs();
        if dist < best_dist {
            best_dist = dist;
            best = step;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lastwave_prefs_test_{}_{}_{}",
            label,
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn roundtrip_and_corrupt_quarantine() {
        let dir = temp_dir("roundtrip");
        {
            let prefs = Prefs::load(&dir);
            prefs.save_last_fm_keys("k1", "s1");
            prefs.save_session("sk1", "alice");
            prefs.set_lossless_quality(6);
            prefs.set_crossfade(true, Some(9));
            assert!(prefs.is_last_fm_configured());
            assert!(prefs.is_authenticated());
            assert_eq!(prefs.lossless_quality(), 6);
            assert!(prefs.crossfade_enabled());
            assert_eq!(prefs.crossfade_seconds(), 9);
            assert!(!prefs.auto_mix_enabled());
        }

        // Reload from disk and verify persistence.
        {
            let reloaded = Prefs::load(&dir);
            assert_eq!(reloaded.last_fm_api_key(), "k1");
            assert_eq!(reloaded.username(), "alice");
            assert_eq!(reloaded.lossless_quality(), 6);
            assert_eq!(reloaded.crossfade_seconds(), 9);
        }

        // Corrupt the JSON file and verify quarantine + clean recovery.
        fs::write(dir.join(PREFS_FILENAME), "{not valid json!!!").unwrap();
        let recovered = Prefs::load(&dir);
        assert!(dir.join(CORRUPT_PREFS_FILENAME).exists());
        assert_eq!(recovered.last_fm_api_key(), "");
        assert_eq!(recovered.lossless_quality(), 27);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clamps_and_portable_filtering_match_dart() {
        let prefs = Prefs::in_memory();
        prefs.set_lossless_quality(999);
        assert_eq!(prefs.lossless_quality(), 27);

        prefs.set_lyrics_text_scale(1.33);
        assert!((prefs.lyrics_text_scale() - 1.4).abs() < 1e-6);

        prefs.set_lyrics_auto_hide_seconds(7);
        assert_eq!(prefs.lyrics_auto_hide_seconds(), 6);

        prefs.set_jam_server_url("wss://jam.clashprojects.qd.je/ws");
        assert_eq!(prefs.jam_server_url(), DEFAULT_JAM_SERVER_URL);

        prefs.set_crossfade(true, None);
        assert!(prefs.crossfade_enabled());
        prefs.set_auto_mix_enabled(true);
        assert!(prefs.auto_mix_enabled());
        assert!(!prefs.crossfade_enabled());

        prefs.save_session("secret-sk", "bob");
        prefs.set_download_dir("C:/Music");
        prefs.set_amoled(true);
        let portable_keys = prefs.portable_pref_keys();
        assert!(portable_keys.contains("lw_amoled"));
        assert!(!portable_keys.contains("lw_sessionkey"));
        assert!(!portable_keys.contains("lw_username"));
        assert!(!portable_keys.contains("lw_download_dir"));
    }

    #[test]
    fn reads_flutter_prefixed_legacy_keys() {
        let dir = std::env::temp_dir().join(format!("lw_prefs_flutter_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(PREFS_FILENAME),
            r#"{"flutter.lw_lastfm_api_key":"legacy_key","flutter.lw_lossless_quality":28}"#,
        )
        .unwrap();

        let prefs = Prefs::load(&dir);
        assert_eq!(prefs.last_fm_api_key(), "legacy_key");
        assert_eq!(prefs.lossless_quality(), 28);

        prefs.set_lossless_quality(6);
        assert_eq!(prefs.lossless_quality(), 6);
        assert_eq!(prefs.get_int("flutter.lw_lossless_quality"), Some(6));

        let _ = fs::remove_dir_all(&dir);
    }
}
