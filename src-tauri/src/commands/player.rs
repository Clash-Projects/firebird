//! Tauri IPC command handlers for playback, queue, equalizer/DSP, audio output
//! (WASAPI / bit-perfect), crossfade/AutoMix, and sleep timer.

use std::time::Duration;

use tauri::ipc::Channel;
use tauri::State;

use crate::audio::headphone_profiles::{HeadphoneProfile, HeadphoneProfiles};
use crate::audio::output_controller::AudioOutputState;
use crate::audio::spectrum_stream::SpectrumFrame;
use crate::commands::state::AppState;
use crate::models::equalizer_models::{
    ClarityPreset, ClarityPresets, EqPreset, EqualizerPresets, EqualizerState,
};
use crate::models::player_state::PlayerSnapshot;
use crate::models::stream_models::PlayableTrack;

// Playback & queue

#[tauri::command]
pub fn player_snapshot(state: State<'_, AppState>) -> PlayerSnapshot {
    state.playback.snapshot()
}

#[tauri::command]
pub fn spectrum_subscribe(
    state: State<'_, AppState>,
    channel: Channel<SpectrumFrame>,
) -> u64 {
    state
        .spectrum
        .set_playing(state.playback.snapshot().is_playing);
    state.spectrum.subscribe(channel)
}

#[tauri::command]
pub fn spectrum_unsubscribe(state: State<'_, AppState>, subscription_id: Option<u64>) {
    state.spectrum.unsubscribe(subscription_id);
}

#[tauri::command]
pub async fn player_play(
    state: State<'_, AppState>,
    track: PlayableTrack,
    source_label: Option<String>,
    start_radio: Option<bool>,
) -> Result<(), String> {
    state
        .playback
        .play(
            track,
            source_label.as_deref().unwrap_or(""),
            start_radio.unwrap_or(false),
        )
        .await;
    Ok(())
}

#[tauri::command]
pub async fn player_play_queue(
    state: State<'_, AppState>,
    tracks: Vec<PlayableTrack>,
    start_index: Option<usize>,
    source_label: Option<String>,
    start_shuffled: Option<bool>,
    endless_radio: Option<bool>,
) -> Result<(), String> {
    state
        .playback
        .play_queue(
            tracks,
            start_index.unwrap_or(0),
            source_label.as_deref().unwrap_or(""),
            start_shuffled.unwrap_or(false),
            endless_radio.unwrap_or(false),
        )
        .await;
    Ok(())
}

#[tauri::command]
pub async fn player_play_next(
    state: State<'_, AppState>,
    track: PlayableTrack,
) -> Result<(), String> {
    state.playback.play_next(track).await;
    Ok(())
}

#[tauri::command]
pub async fn player_play_next_all(
    state: State<'_, AppState>,
    tracks: Vec<PlayableTrack>,
) -> Result<(), String> {
    state.playback.play_next_all(tracks).await;
    Ok(())
}

#[tauri::command]
pub async fn player_add_to_queue(
    state: State<'_, AppState>,
    track: PlayableTrack,
) -> Result<(), String> {
    state.playback.add_to_queue(track).await;
    Ok(())
}

#[tauri::command]
pub async fn player_add_to_queue_all(
    state: State<'_, AppState>,
    tracks: Vec<PlayableTrack>,
) -> Result<(), String> {
    state.playback.add_to_queue_all(tracks).await;
    Ok(())
}

#[tauri::command]
pub async fn player_remove_at(state: State<'_, AppState>, index: usize) -> Result<(), String> {
    state.playback.remove_at(index).await;
    Ok(())
}

#[tauri::command]
pub async fn player_clear_upcoming(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.clear_upcoming().await;
    Ok(())
}

#[tauri::command]
pub async fn player_move_in_queue(
    state: State<'_, AppState>,
    old_index: usize,
    new_index: usize,
) -> Result<(), String> {
    state.playback.move_in_queue(old_index, new_index).await;
    Ok(())
}

#[tauri::command]
pub async fn player_toggle(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.toggle().await;
    Ok(())
}

#[tauri::command]
pub async fn player_resume(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.play_resume().await;
    Ok(())
}

#[tauri::command]
pub async fn player_pause(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.pause().await;
    Ok(())
}

#[tauri::command]
pub async fn player_seek(state: State<'_, AppState>, position_ms: u64) -> Result<(), String> {
    state
        .playback
        .seek(Duration::from_millis(position_ms))
        .await;
    Ok(())
}

#[tauri::command]
pub async fn player_set_volume(
    state: State<'_, AppState>,
    volume: f64,
    software: Option<bool>,
) -> Result<(), String> {
    state
        .playback
        .set_volume(volume, software.unwrap_or(false))
        .await;
    Ok(())
}

#[tauri::command]
pub async fn player_next(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.next().await;
    Ok(())
}

#[tauri::command]
pub async fn player_previous(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.previous().await;
    Ok(())
}

#[tauri::command]
pub async fn player_seek_to_queue_item(
    state: State<'_, AppState>,
    index: usize,
) -> Result<(), String> {
    state.playback.seek_to_queue_item(index).await;
    Ok(())
}

#[tauri::command]
pub async fn player_toggle_shuffle(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.toggle_shuffle().await;
    Ok(())
}

#[tauri::command]
pub async fn player_cycle_repeat(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.cycle_repeat().await;
    Ok(())
}

#[tauri::command]
pub async fn player_set_speed(state: State<'_, AppState>, speed: f64) -> Result<(), String> {
    state.playback.set_speed(speed).await;
    Ok(())
}

#[tauri::command]
pub async fn player_set_pitch(state: State<'_, AppState>, pitch: f64) -> Result<(), String> {
    state.playback.set_pitch(pitch).await;
    Ok(())
}

#[tauri::command]
pub async fn player_cycle_speed(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.cycle_speed().await;
    Ok(())
}

#[tauri::command]
pub fn player_set_sleep_timer(state: State<'_, AppState>, minutes: Option<u64>) {
    state
        .playback
        .set_sleep_timer(minutes.map(|m| Duration::from_secs(m.saturating_mul(60))));
}

#[tauri::command]
pub async fn player_retry(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.retry().await;
    Ok(())
}

#[tauri::command]
pub async fn player_stop_and_clear(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.stop_and_clear().await;
    Ok(())
}

#[tauri::command]
pub async fn player_restore_session(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.restore_session().await;
    Ok(())
}

#[tauri::command]
pub fn player_persist_now(state: State<'_, AppState>) {
    state.playback.persist_now();
}

// Crossfade / AutoMix / Audio prefs

#[tauri::command]
pub async fn player_apply_audio_prefs(state: State<'_, AppState>) -> Result<(), String> {
    state.playback.apply_audio_prefs().await;
    Ok(())
}

#[tauri::command]
pub fn player_refresh_overlap_prefs(state: State<'_, AppState>) {
    state.playback.refresh_overlap_prefs();
}

#[tauri::command]
pub fn player_cancel_crossfade(state: State<'_, AppState>) {
    state.playback.cancel_crossfade();
}

#[tauri::command]
pub fn player_debug_mix_state(state: State<'_, AppState>) -> (usize, f64, Option<f64>, bool) {
    state.playback.debug_mix_state()
}

// Equalizer & DSP

#[tauri::command]
pub fn eq_get_state(state: State<'_, AppState>) -> EqualizerState {
    state.equalizer.state()
}

#[tauri::command]
pub fn eq_set_enabled(state: State<'_, AppState>, enabled: bool) -> EqualizerState {
    state.equalizer.set_enabled(enabled);
    state.equalizer.state()
}

#[tauri::command]
pub fn eq_toggle_enabled(state: State<'_, AppState>) -> EqualizerState {
    state.equalizer.toggle_enabled();
    state.equalizer.state()
}

#[tauri::command]
pub fn eq_set_preset(state: State<'_, AppState>, preset_name: String) -> EqualizerState {
    state.equalizer.set_preset(&preset_name);
    state.equalizer.state()
}

#[tauri::command]
pub fn eq_set_band_gain(
    state: State<'_, AppState>,
    band_index: usize,
    gain_db: f64,
) -> EqualizerState {
    state.equalizer.set_band_gain(band_index, gain_db);
    state.equalizer.state()
}

#[tauri::command]
pub fn eq_set_studio_clarity_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> EqualizerState {
    state.equalizer.set_studio_clarity_enabled(enabled);
    state.equalizer.state()
}

#[tauri::command]
pub fn eq_toggle_studio_clarity(state: State<'_, AppState>) -> EqualizerState {
    state.equalizer.toggle_studio_clarity();
    state.equalizer.state()
}

#[tauri::command]
pub fn eq_set_clarity_preset(state: State<'_, AppState>, key: String) -> EqualizerState {
    state.equalizer.set_clarity_preset(&key);
    state.equalizer.state()
}

#[tauri::command]
pub fn eq_reset(state: State<'_, AppState>) -> EqualizerState {
    state.equalizer.reset();
    state.equalizer.state()
}

#[tauri::command]
pub fn eq_presets() -> Vec<EqPreset> {
    EqualizerPresets::ALL.to_vec()
}

#[tauri::command]
pub fn eq_clarity_presets() -> Vec<ClarityPreset> {
    ClarityPresets::ALL.to_vec()
}

#[tauri::command]
pub fn headphone_profiles_list() -> Vec<HeadphoneProfile> {
    HeadphoneProfiles::ALL.to_vec()
}

// Audio output / WASAPI / bit-perfect

#[tauri::command]
pub fn audio_output_state(state: State<'_, AppState>) -> AudioOutputState {
    state.audio_output.state()
}

#[tauri::command]
pub async fn audio_output_refresh_devices(
    state: State<'_, AppState>,
) -> Result<AudioOutputState, String> {
    state.audio_output.refresh_devices().await;
    Ok(state.audio_output.state())
}

#[tauri::command]
pub async fn audio_output_select_device(
    state: State<'_, AppState>,
    id: String,
) -> Result<AudioOutputState, String> {
    state.audio_output.select_device(&id).await;
    Ok(state.audio_output.state())
}

#[tauri::command]
pub async fn audio_output_set_exclusive(
    state: State<'_, AppState>,
    exclusive: bool,
) -> Result<AudioOutputState, String> {
    state.audio_output.set_exclusive(exclusive).await;
    Ok(state.audio_output.state())
}

#[tauri::command]
pub async fn audio_output_set_bit_perfect(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AudioOutputState, String> {
    state.audio_output.set_bit_perfect(enabled).await;
    Ok(state.audio_output.state())
}

#[tauri::command]
pub async fn audio_output_set_volume(
    state: State<'_, AppState>,
    volume: f64,
) -> Result<(), String> {
    state.audio_output.set_volume(volume).await;
    Ok(())
}

#[tauri::command]
pub async fn audio_output_toggle_mute(state: State<'_, AppState>) -> Result<(), String> {
    state.audio_output.toggle_mute().await;
    Ok(())
}
