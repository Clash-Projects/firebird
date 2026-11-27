//! Tauri IPC command handlers for Prefs, SecureStore, AppLanguage,
//! Performance Mode, and DiagnosticsLog / LogBundle.

use std::path::PathBuf;

use serde_json::{Map, Value};
use tauri::State;

use crate::commands::state::AppState;
use crate::models::app_language::AppLanguage;
use crate::models::performance_controller::{
    is_modest_hardware, performance_optimizations, PerformanceOptimizationItem, PerformanceState,
};
use crate::storage::diagnostics_log::DiagnosticsLog;
use crate::storage::log_bundle::LogBundle;

// Preferences

#[tauri::command]
pub fn prefs_get(state: State<'_, AppState>, key: String) -> Option<Value> {
    state.prefs.get(&key)
}

#[tauri::command]
pub fn prefs_set_bool(state: State<'_, AppState>, key: String, value: bool) {
    state.prefs.set_bool(&key, value);
}

#[tauri::command]
pub fn prefs_set_int(state: State<'_, AppState>, key: String, value: i64) {
    state.prefs.set_int(&key, value);
}

#[tauri::command]
pub fn prefs_set_double(state: State<'_, AppState>, key: String, value: f64) {
    state.prefs.set_double(&key, value);
}

#[tauri::command]
pub fn prefs_set_string(state: State<'_, AppState>, key: String, value: String) {
    state.prefs.set_string(&key, &value);
}

#[tauri::command]
pub fn prefs_set_string_list(state: State<'_, AppState>, key: String, value: Vec<String>) {
    state.prefs.set_string_list(&key, &value);
}

#[tauri::command]
pub fn prefs_remove(state: State<'_, AppState>, key: String) {
    state.prefs.remove(&key);
}

#[tauri::command]
pub fn prefs_get_all_settings(state: State<'_, AppState>) -> Value {
    let p = &state.prefs;
    let mut m = Map::new();
    m.insert("languageTag".into(), Value::String(p.app_language_tag()));
    m.insert("themeMode".into(), Value::String(p.theme_mode()));
    m.insert("amoled".into(), Value::Bool(p.amoled()));
    m.insert("accentMode".into(), Value::String(p.accent_mode()));
    m.insert("accentColor".into(), Value::from(p.accent_color()));
    m.insert("dynamicNowPlaying".into(), Value::Bool(p.dynamic_now_playing()));
    m.insert("closeToTray".into(), Value::Bool(p.close_to_tray()));
    m.insert("miniPlayerStyle".into(), Value::String(p.mini_player_style()));
    m.insert("preferLossless".into(), Value::Bool(p.prefer_lossless()));
    m.insert("losslessQuality".into(), Value::from(p.lossless_quality()));
    m.insert("downloadQuality".into(), Value::from(p.download_quality()));
    m.insert("downloadDir".into(), Value::String(p.download_dir()));
    m.insert("downloadLyrics".into(), Value::Bool(p.download_lyrics()));
    m.insert("downloadStructure".into(), Value::String(p.download_structure()));
    m.insert("autoplaySimilar".into(), Value::Bool(p.autoplay_similar()));
    m.insert("crossfadeEnabled".into(), Value::Bool(p.crossfade_enabled()));
    m.insert("crossfadeSeconds".into(), Value::from(p.crossfade_seconds()));
    m.insert("wasapiExclusive".into(), Value::Bool(p.wasapi_exclusive()));
    m.insert("bitPerfect".into(), Value::Bool(p.bit_perfect()));
    m.insert("audioDeviceId".into(), Value::String(p.audio_device_id()));
    m.insert("loudnessMode".into(), Value::String(p.loudness_mode()));
    m.insert("headphoneProfile".into(), Value::String(p.headphone_profile()));
    m.insert("autoMixEnabled".into(), Value::Bool(p.auto_mix_enabled()));
    m.insert("autoMixBars".into(), Value::from(p.auto_mix_bars()));
    m.insert("autoMixSpectral".into(), Value::Bool(p.auto_mix_spectral()));
    m.insert("scrobblerEnabled".into(), Value::Bool(p.scrobbler_enabled()));
    m.insert("primaryScrobbler".into(), Value::String(p.primary_scrobbler()));
    m.insert("scrobbleNowPlaying".into(), Value::Bool(p.scrobble_now_playing()));
    m.insert("scrobblePercent".into(), Value::from(p.scrobble_percent()));
    m.insert("wordByWord".into(), Value::Bool(p.word_by_word()));
    m.insert("lyricsProviderId".into(), Value::String(p.lyrics_provider_id()));
    m.insert("lyricsAnimation".into(), Value::String(p.lyrics_animation()));
    m.insert("lyricsTextScale".into(), Value::from(p.lyrics_text_scale()));
    m.insert("lyricsTranslationLang".into(), Value::String(p.lyrics_translation_lang()));
    m.insert("lyricsAutoHideSeconds".into(), Value::from(p.lyrics_auto_hide_seconds()));
    m.insert("podcastsEnabled".into(), Value::Bool(p.podcasts_enabled()));
    m.insert("canvasEnabled".into(), Value::Bool(p.canvas_enabled()));
    m.insert("spotifyCanvasEnabled".into(), Value::Bool(p.spotify_canvas_enabled()));
    m.insert("preferSpotifyCanvas".into(), Value::Bool(p.prefer_spotify_canvas()));
    m.insert("recommendationSource".into(), Value::String(p.recommendation_source()));
    m.insert("performanceMode".into(), Value::Bool(p.performance_mode()));
    m.insert("performancePromptDismissed".into(), Value::Bool(p.performance_prompt_dismissed()));
    Value::Object(m)
}

// SecureStore

#[tauri::command]
pub fn secure_read(state: State<'_, AppState>, key: String) -> Option<String> {
    state.secure.read(&key)
}

#[tauri::command]
pub fn secure_write(state: State<'_, AppState>, key: String, value: Option<String>) {
    state.secure.write(&key, value.as_deref());
}

#[tauri::command]
pub fn secure_delete(state: State<'_, AppState>, key: String) {
    state.secure.delete(&key);
}

#[tauri::command]
pub fn secure_clear_all(state: State<'_, AppState>) {
    state.secure.clear_all();
}

// AppLanguage

#[tauri::command]
pub fn language_get(state: State<'_, AppState>) -> AppLanguage {
    let tag = state.prefs.app_language_tag();
    AppLanguage::from_tag(if tag.is_empty() { None } else { Some(tag.as_str()) })
}

#[tauri::command]
pub fn language_set(state: State<'_, AppState>, tag: String) -> AppLanguage {
    state.prefs.set_app_language_tag(&tag);
    AppLanguage::from_tag(if tag.is_empty() { None } else { Some(tag.as_str()) })
}

#[tauri::command]
pub fn language_effective(state: State<'_, AppState>) -> AppLanguage {
    let tag = state.prefs.app_language_tag();
    AppLanguage::from_tag(if tag.is_empty() { None } else { Some(tag.as_str()) })
        .effective_language()
}

// Performance Mode

#[tauri::command]
pub fn performance_get_state(state: State<'_, AppState>) -> PerformanceState {
    PerformanceState {
        performance_mode: state.prefs.performance_mode(),
        prompt_dismissed: state.prefs.performance_prompt_dismissed(),
        is_modest_hardware: is_modest_hardware(),
    }
}

#[tauri::command]
pub fn performance_set_mode(state: State<'_, AppState>, enabled: bool) -> PerformanceState {
    state.prefs.set_performance_mode(enabled);
    performance_get_state(state)
}

#[tauri::command]
pub fn performance_dismiss_prompt(state: State<'_, AppState>) -> PerformanceState {
    state.prefs.set_performance_prompt_dismissed(true);
    performance_get_state(state)
}

#[tauri::command]
pub fn performance_get_optimizations(
    state: State<'_, AppState>,
) -> Vec<PerformanceOptimizationItem> {
    performance_optimizations(state.prefs.performance_mode())
}

// DiagnosticsLog & LogBundle

#[tauri::command]
pub fn diagnostics_is_verbose() -> bool {
    DiagnosticsLog::is_verbose()
}

#[tauri::command]
pub fn diagnostics_set_verbose(state: State<'_, AppState>, verbose: bool) {
    let _ = state;
    DiagnosticsLog::set_verbose(verbose);
}

#[tauri::command]
pub fn diagnostics_previous_exit_clean() -> bool {
    DiagnosticsLog::previous_exit_clean()
}

#[tauri::command]
pub fn diagnostics_log_info(line: String) {
    DiagnosticsLog::info(&line);
}

#[tauri::command]
pub fn diagnostics_session_files() -> Vec<String> {
    DiagnosticsLog::session_files()
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

#[tauri::command]
pub fn diagnostics_suggested_bundle_name() -> String {
    LogBundle::suggested_name(None)
}

#[tauri::command]
pub fn diagnostics_export_bundle(report: String, dest_path: String) -> Result<String, String> {
    LogBundle::export_to_path(&report, &PathBuf::from(dest_path))
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn diagnostics_build_bundle_bytes(report: String) -> Result<Vec<u8>, String> {
    LogBundle::build(&report).map_err(|e| e.to_string())
}
