pub mod audio;
pub mod commands;
pub mod extractor;
pub mod integrations;
pub use firebird_models as models;
pub mod storage;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, RunEvent, WebviewWindowBuilder};

use crate::commands::state::AppState;

static ALLOW_EXIT: AtomicBool = AtomicBool::new(false);
static TRAY_READY: AtomicBool = AtomicBool::new(false);

fn show_or_recreate_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        return;
    }
    if let Some(cfg) = app.config().app.windows.first() {
        if let Ok(builder) = WebviewWindowBuilder::from_config(app, cfg) {
            if let Ok(win) = builder.build() {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }
    }
}

fn shutdown_audio_and_resources(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    state.spectrum.unsubscribe(None);
    state.playback.shutdown_sync();
    state.audio_output.dispose();
    crate::storage::diagnostics_log::DiagnosticsLog::mark_clean_exit();
}

fn quit_application(app: &tauri::AppHandle) {
    ALLOW_EXIT.store(true, Ordering::SeqCst);
    shutdown_audio_and_resources(app);
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new(AppState::default_app_support_dir());

    let app = tauri::Builder::default()
        .manage(app_state)
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<AppState>();
            state.set_app_handle(handle.clone());
            tauri::async_runtime::block_on(async {
                state.tube.load_persisted_connection().await;
                state.spotify_auth.load().await;
            });

            let emit_handle = handle.clone();
            let spectrum = Arc::clone(&state.spectrum);
            let discord = Arc::clone(&state.discord_presence);
            let playback_for_eq = Arc::clone(&state.playback);
            state.equalizer.set_filter_sink(Some(Arc::new(move |af: &str| {
                let pb = Arc::clone(&playback_for_eq);
                let filter = af.to_string();
                tauri::async_runtime::spawn(async move {
                    pb.set_audio_filter(filter).await;
                });
            })));

            let smtc = Arc::new(crate::audio::smtc_service::SmtcService::new());
            smtc.set_macos_now_playing(state.prefs.macos_now_playing());
            {
                let pb_smtc = Arc::clone(&state.playback);
                smtc.set_action_handler(Arc::new(move |action| {
                    pb_smtc.handle_media_control_action(action);
                }));
            }
            state.playback.attach_smtc(Arc::clone(&smtc));
            #[cfg(windows)]
            {
                let hwnd = app
                    .get_webview_window("main")
                    .and_then(|w| w.hwnd().ok())
                    .map(|h| h.0 as isize)
                    .unwrap_or(0);
                smtc.startup(hwnd, None);
            }
            #[cfg(target_os = "macos")]
            smtc.startup(0, None);

            {
                let discord_hb = Arc::clone(&state.discord_presence);
                let pb_hb = Arc::clone(&state.playback);
                tauri::async_runtime::spawn(async move {
                    discord_hb
                        .start_heartbeat(move || Some(pb_hb.snapshot()))
                        .await;
                });
            }

            state.playback.add_snapshot_listener(Arc::new(move |snap| {
                spectrum.update_playback(snap);
                let discord_snap = Arc::clone(&discord);
                let snap_clone = snap.clone();
                tauri::async_runtime::spawn(async move {
                    discord_snap.on_snapshot(snap_clone).await;
                });
                let _ = emit_handle.emit("player:snapshot", snap);
            }));

            let tray_result = (|| -> tauri::Result<()> {
                let open_item = MenuItem::with_id(app, "open", "Open LastWave", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "Quit LastWave", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

                let mut builder = TrayIconBuilder::with_id("main-tray")
                    .tooltip("LastWave")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "open" => show_or_recreate_main_window(app),
                        "quit" => quit_application(app),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            show_or_recreate_main_window(tray.app_handle());
                        }
                    });

                if let Some(icon) = app.default_window_icon().cloned() {
                    builder = builder.icon(icon);
                }

                let _tray = builder.build(app)?;
                Ok(())
            })();

            TRAY_READY.store(tray_result.is_ok(), Ordering::SeqCst);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // commands::player
            commands::player::player_snapshot,
            commands::player::spectrum_subscribe,
            commands::player::spectrum_unsubscribe,
            commands::player::player_play,
            commands::player::player_play_queue,
            commands::player::player_play_next,
            commands::player::player_play_next_all,
            commands::player::player_add_to_queue,
            commands::player::player_add_to_queue_all,
            commands::player::player_remove_at,
            commands::player::player_clear_upcoming,
            commands::player::player_move_in_queue,
            commands::player::player_toggle,
            commands::player::player_resume,
            commands::player::player_pause,
            commands::player::player_seek,
            commands::player::player_set_volume,
            commands::player::player_next,
            commands::player::player_previous,
            commands::player::player_seek_to_queue_item,
            commands::player::player_toggle_shuffle,
            commands::player::player_cycle_repeat,
            commands::player::player_set_speed,
            commands::player::player_set_pitch,
            commands::player::player_cycle_speed,
            commands::player::player_set_sleep_timer,
            commands::player::player_retry,
            commands::player::player_stop_and_clear,
            commands::player::player_restore_session,
            commands::player::player_persist_now,
            commands::player::player_apply_audio_prefs,
            commands::player::player_refresh_overlap_prefs,
            commands::player::player_cancel_crossfade,
            commands::player::player_debug_mix_state,
            commands::player::eq_get_state,
            commands::player::eq_set_enabled,
            commands::player::eq_toggle_enabled,
            commands::player::eq_set_preset,
            commands::player::eq_set_band_gain,
            commands::player::eq_set_studio_clarity_enabled,
            commands::player::eq_toggle_studio_clarity,
            commands::player::eq_set_clarity_preset,
            commands::player::eq_reset,
            commands::player::eq_presets,
            commands::player::eq_clarity_presets,
            commands::player::headphone_profiles_list,
            commands::player::audio_output_state,
            commands::player::audio_output_refresh_devices,
            commands::player::audio_output_select_device,
            commands::player::audio_output_set_exclusive,
            commands::player::audio_output_set_bit_perfect,
            commands::player::audio_output_set_volume,
            commands::player::audio_output_toggle_mute,
            // commands::settings
            commands::settings::prefs_get,
            commands::settings::prefs_set_bool,
            commands::settings::prefs_set_int,
            commands::settings::prefs_set_double,
            commands::settings::prefs_set_string,
            commands::settings::prefs_set_string_list,
            commands::settings::prefs_remove,
            commands::settings::prefs_get_all_settings,
            commands::settings::secure_read,
            commands::settings::secure_write,
            commands::settings::secure_delete,
            commands::settings::secure_clear_all,
            commands::settings::language_get,
            commands::settings::language_set,
            commands::settings::language_effective,
            commands::settings::diagnostics_is_verbose,
            commands::settings::diagnostics_set_verbose,
            commands::settings::diagnostics_previous_exit_clean,
            commands::settings::diagnostics_log_info,
            commands::settings::diagnostics_session_files,
            commands::settings::diagnostics_suggested_bundle_name,
            commands::settings::diagnostics_export_bundle,
            commands::settings::diagnostics_build_bundle_bytes,
            commands::settings::diagnostics_play_diag,
            commands::settings::diagnostics_export_logs,
            // commands::extractor
            commands::extractor::yt_search_songs,
            commands::extractor::yt_search_mixed,
            commands::extractor::yt_search_artists,
            commands::extractor::yt_search_albums,
            commands::extractor::yt_search_playlists,
            commands::extractor::yt_get_suggestions,
            commands::extractor::yt_fetch_home_shelves,
            commands::extractor::yt_fetch_artist_page,
            commands::extractor::yt_browse_songs,
            commands::extractor::yt_browse_album,
            commands::extractor::yt_browse_albums,
            commands::extractor::yt_fetch_playlist,
            commands::extractor::yt_fetch_related_songs,
            commands::extractor::yt_fetch_song_details,
            commands::extractor::yt_resolve_album_browse_id,
            commands::extractor::yt_resolve_audio_stream,
            commands::extractor::yt_resolve_music_video,
            commands::extractor::yt_fetch_lyrics_tab,
            commands::extractor::yt_fetch_transcript_cues,
            commands::extractor::yt_fetch_podcast_home,
            commands::extractor::yt_fetch_podcast_show,
            commands::extractor::yt_fetch_podcast_show_continuation,
            commands::extractor::yt_search_podcast_episodes,
            commands::extractor::yt_get_connection,
            commands::extractor::yt_is_auth_stale,
            commands::extractor::yt_check_auth,
            commands::extractor::yt_connect_cookies,
            commands::extractor::yt_sign_out,
            commands::extractor::yt_fetch_account_info,
            commands::extractor::yt_fetch_available_channels,
            commands::extractor::yt_list_profiles,
            commands::extractor::yt_switch_identity,
            commands::extractor::yt_fetch_liked_songs,
            commands::extractor::yt_like_video,
            commands::extractor::yt_remove_video_like,
            commands::extractor::yt_fetch_history,
            commands::extractor::yt_record_watch_history,
            commands::extractor::yt_fetch_account_playlists,
            commands::extractor::yt_create_remote_playlist,
            commands::extractor::yt_rename_remote_playlist,
            commands::extractor::yt_delete_remote_playlist,
            commands::extractor::yt_add_videos_to_remote_playlist,
            commands::extractor::yt_remove_videos_from_remote_playlist,
            commands::extractor::yt_web_sign_in,
            commands::extractor::yt_web_sign_in_cancel,
            commands::extractor::yt_session_refresh,
            commands::extractor::yt_library_sync_kick,
            commands::extractor::yt_library_sync_apply_held_removals,
            commands::extractor::yt_po_token_set_enabled,
            commands::extractor::yt_po_token_is_enabled,
            commands::extractor::yt_po_token_pre_warm,
            commands::extractor::yt_po_token_mint,
            commands::extractor::yt_find_best_match,
            commands::extractor::yt_find_best_effort_match,
            commands::extractor::yt_fetch_playlist_artwork,
            commands::extractor::yt_report_playback_failure,
            commands::extractor::yt_invalidate_cache,
            commands::extractor::yt_auth_bundle,
            commands::extractor::yt_resolve_identity_for,
            commands::extractor::yt_fetch_channels_for,
            commands::extractor::yt_save_profiles,
            commands::extractor::yt_swr_liked_songs,
            commands::extractor::yt_swr_history,
            commands::extractor::yt_swr_account_playlists,
            commands::extractor::yt_swr_home_shelves,
            commands::extractor::yt_swr_playlist_detail,
            // commands::library
            commands::library::library_search_history_load,
            commands::library::library_search_history_push,
            commands::library::library_search_history_remove,
            commands::library::library_search_history_clear,
            commands::library::library_exclusions_list,
            commands::library::library_exclusions_add,
            commands::library::library_exclusions_remove,
            commands::library::library_recent_plays,
            commands::library::library_plays_for_date,
            commands::library::library_active_play_dates,
            commands::library::library_local_stats_summary,
            commands::library::library_snapshot_cache_stats,
            commands::library::library_snapshot_cache_clear,
            commands::library::library_snapshot_cache_delete,
            commands::library::library_artwork_cache_stats,
            commands::library::library_artwork_cache_clear,
            commands::library::library_artwork_cache_evict,
            commands::library::library_playlists_all,
            commands::library::library_playlists_refresh,
            commands::library::library_playlists_ensure_liked,
            commands::library::library_playlists_liked_keys,
            commands::library::library_playlists_toggle_liked,
            commands::library::library_playlists_create,
            commands::library::library_playlists_create_library,
            commands::library::library_playlists_save_generated,
            commands::library::library_playlists_rename,
            commands::library::library_playlists_set_pinned,
            commands::library::library_playlists_set_custom_cover,
            commands::library::library_playlists_delete,
            commands::library::library_playlists_add_track,
            commands::library::library_playlists_remove_track,
            commands::library::library_playlists_apply_batch,
            commands::library::library_playlists_replace_tracks,
            commands::library::library_local_folders,
            commands::library::library_local_add_folder,
            commands::library::library_local_forget_folder,
            commands::library::library_local_rescan,
            commands::library::library_index_tracks,
            commands::library::library_index_albums,
            commands::library::library_index_artists,
            commands::library::library_downloads_all,
            commands::library::library_downloads_enqueue,
            commands::library::library_downloads_cancel,
            commands::library::library_downloads_cancel_active,
            commands::library::library_downloads_delete,
            commands::library::library_downloads_reorganize,
            commands::library::library_downloads_is_downloaded,
            commands::library::library_downloads_local_path,
            commands::library::library_backup_export,
            commands::library::library_backup_export_file,
            commands::library::library_backup_preview,
            commands::library::library_backup_restore_file,
            // commands::integrations
            commands::integrations::lyrics_fetch,
            commands::integrations::lyrics_fetch_provider,
            commands::integrations::lyrics_translate,
            commands::integrations::lastfm_auth_state,
            commands::integrations::lastfm_enter_guest_mode,
            commands::integrations::lastfm_begin_web_auth,
            commands::integrations::lastfm_complete_web_auth,
            commands::integrations::lastfm_save_custom_keys,
            commands::integrations::lastfm_clear_custom_keys,
            commands::integrations::lastfm_sign_out,
            commands::integrations::lastfm_clear_error,
            commands::integrations::lastfm_scrobble_now_playing,
            commands::integrations::lastfm_scrobble_track,
            commands::integrations::lastfm_set_loved,
            commands::integrations::lastfm_queued_count,
            commands::integrations::lastfm_recent_tracks,
            commands::integrations::lastfm_stats,
            commands::integrations::lastfm_top_tracks,
            commands::integrations::lastfm_top_artists,
            commands::integrations::lastfm_top_albums,
            commands::integrations::lastfm_profile_stats,
            commands::integrations::extra_scrobblers_connect_listenbrainz,
            commands::integrations::extra_scrobblers_disconnect_listenbrainz,
            commands::integrations::extra_scrobblers_connect_librefm,
            commands::integrations::extra_scrobblers_disconnect_librefm,
            commands::integrations::extra_scrobblers_submit,
            commands::integrations::discord_presence_update,
            commands::integrations::discord_presence_shutdown,
            commands::integrations::jam_snapshot,
            commands::integrations::jam_create_room,
            commands::integrations::jam_join_room,
            commands::integrations::jam_leave,
            commands::integrations::jam_send_chat,
            commands::integrations::jam_grant_host,
            commands::integrations::jam_revoke_host,
            commands::integrations::jam_transfer_admin,
            commands::integrations::jam_broadcast_queue,
            commands::integrations::jam_set_web_guests,
            commands::integrations::jam_room_counts,
            commands::integrations::jam_parse_code,
            commands::integrations::jam_invite_link,
            commands::integrations::jam_on_player_snapshot,
            commands::integrations::spotify_connected,
            commands::integrations::spotify_connect_sp_dc,
            commands::integrations::spotify_disconnect,
            commands::integrations::spotify_verify,
            commands::integrations::spotify_user_playlists,
            commands::integrations::spotify_playlist,
            commands::integrations::spotify_home,
            commands::integrations::spotify_song_radio,
            commands::integrations::spotify_canvas,
            commands::integrations::podcasts_home,
            commands::integrations::podcasts_show,
            commands::integrations::podcasts_more,
            commands::integrations::podcasts_search,
            commands::integrations::podcasts_remember,
            commands::integrations::podcasts_resume_ms,
            commands::integrations::playlist_preview_link,
            commands::integrations::playlist_preview_file,
            commands::integrations::playlist_import_preview,
            commands::integrations::playlist_export_csv,
            commands::integrations::playlist_export_m3u,
            commands::integrations::playlist_detect_link,
            commands::integrations::artwork_resolve_track,
            commands::integrations::artwork_resolve_album,
            commands::integrations::artwork_resolve_artist,
            commands::integrations::artwork_lookup_animated,
            commands::integrations::home_load_feed,
            commands::integrations::home_load_new_albums,
            commands::integrations::home_load_personal_mix,
            commands::integrations::feed_fetch_mix,
            commands::integrations::feed_generate_for_tag,
            commands::integrations::feed_fetch_genre_tracks,
            commands::integrations::search_suggestions,
            commands::integrations::search_query,
            commands::integrations::search_songs_for,
            commands::integrations::search_history,
            commands::integrations::search_push_history,
            commands::integrations::search_clear_history,
            commands::integrations::search_remove_history,
            commands::integrations::stats_compute_listening,
            commands::integrations::updates_check,
            // commands::plugins
            commands::plugins::plugins_is_safe_mode,
            commands::plugins::plugins_state,
            commands::plugins::plugins_install,
            commands::plugins::plugins_set_enabled,
            commands::plugins::plugins_set_all_disabled,
            commands::plugins::plugins_uninstall,
            commands::plugins::plugins_fetch_registry,
            commands::plugins::plugins_fetch_remote_text,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| match event {
        RunEvent::ExitRequested { api, .. } => {
            if !ALLOW_EXIT.load(Ordering::SeqCst) && TRAY_READY.load(Ordering::SeqCst) {
                api.prevent_exit();
            } else {
                shutdown_audio_and_resources(app_handle);
            }
        }
        RunEvent::Exit => {
            shutdown_audio_and_resources(app_handle);
        }
        _ => {}
    });
}
