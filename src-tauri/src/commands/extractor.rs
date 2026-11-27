use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::extractor::album_match::resolve_album_browse_id;
use crate::extractor::innertube_api::{
    TranscriptCue, YouTubeAlbumResult, YouTubeMusicEntity, YouTubeMusicTrack,
    YouTubePlaylistResult, YouTubePlaylistSummary, YtAccount, YtArtistPage, YtChannel,
    YtConnection, YtHomeShelf, YtProfile,
};
use crate::extractor::yt_auth_store::YtSessionBundle;
use crate::extractor::yt_library_providers::YtLibraryProviders;
use crate::extractor::yt_web_login::{YtSignInFailure, YtWebLogin};
use crate::extractor::yt_webview_guard::YtWebviewGuard;
use crate::models::podcast_models::{
    PodcastEpisode, PodcastEpisodePage, PodcastShelf, PodcastShowPage,
};
use crate::models::stream_models::{MusicVideoStream, ResolvedStream};

use super::state::AppState;

static WEB_LOGIN: OnceLock<Arc<YtWebLogin>> = OnceLock::new();
static LIBRARY_PROVIDERS: OnceLock<Arc<YtLibraryProviders>> = OnceLock::new();

fn web_login_for(state: &AppState) -> Result<Arc<YtWebLogin>, String> {
    if let Some(existing) = WEB_LOGIN.get() {
        return Ok(Arc::clone(existing));
    }
    let handle = state
        .app_handle()
        .ok_or_else(|| "Tauri AppHandle is not initialized".to_string())?;
    let instance = Arc::new(YtWebLogin::for_app(handle));
    Ok(Arc::clone(WEB_LOGIN.get_or_init(|| instance)))
}

fn library_providers_for(state: &AppState) -> Arc<YtLibraryProviders> {
    Arc::clone(LIBRARY_PROVIDERS.get_or_init(|| {
        Arc::new(YtLibraryProviders::new(
            Arc::clone(&state.tube),
            Arc::clone(&state.db),
        ))
    }))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoTokenDto {
    pub player_token: String,
    pub session_token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistRemovalItem {
    pub set_video_id: String,
    pub video_id: String,
}

#[tauri::command]
pub async fn yt_search_songs(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<YouTubeMusicTrack>, String> {
    state
        .tube
        .search_songs(&query, limit.unwrap_or(30))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_search_mixed(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<YouTubeMusicTrack>, String> {
    state
        .tube
        .search_mixed(&query, limit.unwrap_or(20))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_search_artists(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<YouTubeMusicEntity>, String> {
    state
        .tube
        .search_artists(&query, limit.unwrap_or(30))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_search_albums(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<YouTubeMusicEntity>, String> {
    state
        .tube
        .search_albums(&query, limit.unwrap_or(30))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_search_playlists(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<YouTubePlaylistSummary>, String> {
    Ok(state
        .tube
        .search_playlists(&query, limit.unwrap_or(30))
        .await)
}

#[tauri::command]
pub async fn yt_get_suggestions(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<String>, String> {
    Ok(state.tube.get_suggestions(&query).await)
}

#[tauri::command]
pub async fn yt_fetch_home_shelves(
    state: State<'_, AppState>,
    max_shelves: Option<usize>,
    max_items_per_shelf: Option<usize>,
    max_pages: Option<usize>,
    authenticated: Option<bool>,
) -> Result<Vec<YtHomeShelf>, String> {
    Ok(state
        .tube
        .fetch_home_shelves(
            max_shelves.unwrap_or(20),
            max_items_per_shelf.unwrap_or(20),
            max_pages.unwrap_or(4),
            authenticated.unwrap_or(true),
        )
        .await)
}

#[tauri::command]
pub async fn yt_fetch_artist_page(
    state: State<'_, AppState>,
    browse_id: String,
) -> Result<Option<YtArtistPage>, String> {
    Ok(state.tube.fetch_artist_page(&browse_id).await)
}

#[tauri::command]
pub async fn yt_browse_songs(
    state: State<'_, AppState>,
    browse_id: String,
    params: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<YouTubeMusicTrack>, String> {
    state
        .tube
        .browse_songs(&browse_id, params.as_deref(), limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_browse_album(
    state: State<'_, AppState>,
    browse_id: String,
    limit: Option<usize>,
) -> Result<Option<YouTubeAlbumResult>, String> {
    Ok(state.tube.browse_album(&browse_id, limit).await)
}

#[tauri::command]
pub async fn yt_browse_albums(
    state: State<'_, AppState>,
    browse_id: String,
    limit: Option<usize>,
) -> Result<Vec<YouTubeMusicEntity>, String> {
    state
        .tube
        .browse_albums(&browse_id, limit.unwrap_or(30))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_fetch_playlist(
    state: State<'_, AppState>,
    playlist_id_or_url: String,
    max_tracks: Option<usize>,
) -> Result<Option<YouTubePlaylistResult>, String> {
    Ok(state
        .tube
        .fetch_playlist(&playlist_id_or_url, max_tracks)
        .await)
}

#[tauri::command]
pub async fn yt_fetch_related_songs(
    state: State<'_, AppState>,
    video_id: String,
    limit: Option<usize>,
) -> Result<Vec<YouTubeMusicTrack>, String> {
    state
        .tube
        .fetch_related_songs(&video_id, limit.unwrap_or(30))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_fetch_song_details(
    state: State<'_, AppState>,
    video_id: String,
) -> Result<Option<YouTubeMusicTrack>, String> {
    Ok(state.tube.fetch_song_details(&video_id).await)
}

#[tauri::command]
pub async fn yt_resolve_album_browse_id(
    state: State<'_, AppState>,
    title: String,
    artist: String,
) -> Result<String, String> {
    Ok(resolve_album_browse_id(&*state.tube, &title, &artist).await)
}

#[tauri::command]
pub async fn yt_resolve_audio_stream(
    state: State<'_, AppState>,
    video_id: String,
    force_refresh: Option<bool>,
    prefer_smallest: Option<bool>,
) -> Result<Option<ResolvedStream>, String> {
    Ok(state
        .tube
        .resolve_audio_stream(
            &video_id,
            force_refresh.unwrap_or(false),
            prefer_smallest.unwrap_or(false),
        )
        .await)
}

#[tauri::command]
pub async fn yt_resolve_music_video(
    state: State<'_, AppState>,
    video_id: String,
) -> Result<Vec<MusicVideoStream>, String> {
    Ok(state.tube.resolve_music_video(&video_id).await)
}

#[tauri::command]
pub async fn yt_fetch_lyrics_tab(
    state: State<'_, AppState>,
    video_id: String,
) -> Result<Option<String>, String> {
    Ok(state.tube.fetch_lyrics_tab_text(&video_id).await)
}

#[tauri::command]
pub async fn yt_fetch_transcript_cues(
    state: State<'_, AppState>,
    video_id: String,
) -> Result<Option<Vec<TranscriptCue>>, String> {
    Ok(state.tube.fetch_transcript_cues(&video_id).await)
}

#[tauri::command]
pub async fn yt_fetch_podcast_home(
    state: State<'_, AppState>,
) -> Result<Vec<PodcastShelf>, String> {
    state
        .tube
        .fetch_podcast_home()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_fetch_podcast_show(
    state: State<'_, AppState>,
    show_id: String,
) -> Result<Option<PodcastShowPage>, String> {
    state
        .tube
        .fetch_podcast_show(&show_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_fetch_podcast_show_continuation(
    state: State<'_, AppState>,
    show_id: String,
    continuation: String,
    show_title: Option<String>,
) -> Result<PodcastEpisodePage, String> {
    state
        .tube
        .fetch_podcast_show_continuation(&show_id, &continuation, show_title.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_search_podcast_episodes(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<PodcastEpisode>, String> {
    Ok(state
        .tube
        .search_podcast_episodes(&query, limit.unwrap_or(30))
        .await)
}

#[tauri::command]
pub fn yt_get_connection(state: State<'_, AppState>) -> YtConnection {
    state.tube.connection()
}

#[tauri::command]
pub fn yt_is_auth_stale(state: State<'_, AppState>) -> bool {
    state.tube.auth_stale()
}

#[tauri::command]
pub async fn yt_check_auth(state: State<'_, AppState>) -> Result<bool, String> {
    let ok = state.tube.check_auth().await;
    state.emit_event("yt-auth-stale", state.tube.auth_stale());
    Ok(ok)
}

#[tauri::command]
pub async fn yt_connect_cookies(
    state: State<'_, AppState>,
    cookies: String,
    profile_email: Option<String>,
    page_id: Option<String>,
) -> Result<YtConnection, String> {
    state
        .tube
        .connect_as(
            &cookies,
            profile_email.as_deref().unwrap_or(""),
            page_id.as_deref().unwrap_or(""),
        )
        .await
        .map_err(|e| e.to_string())?;
    let conn = state.tube.connection();
    state.emit_event("yt-connection-changed", conn.clone());
    state.yt_library_sync.kick();
    Ok(conn)
}

#[tauri::command]
pub async fn yt_sign_out(state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(login) = web_login_for(&state) {
        login.sign_out_webview().await;
    }
    state.tube.sign_out().await;
    state.emit_event("yt-connection-changed", state.tube.connection());
    Ok(())
}

#[tauri::command]
pub async fn yt_fetch_account_info(
    state: State<'_, AppState>,
    force_refresh: Option<bool>,
    page_id: Option<String>,
) -> Result<Option<YtAccount>, String> {
    let pid = page_id.unwrap_or_else(|| state.tube.connection().active_page_id);
    Ok(state
        .tube
        .fetch_account_info(force_refresh.unwrap_or(false), &pid)
        .await)
}

#[tauri::command]
pub async fn yt_fetch_available_channels(
    state: State<'_, AppState>,
) -> Result<Vec<YtChannel>, String> {
    Ok(state.tube.fetch_available_channels().await)
}

#[tauri::command]
pub fn yt_list_profiles(state: State<'_, AppState>) -> Vec<YtProfile> {
    let raw = state.secure.read_yt_profiles().unwrap_or_default();
    if raw.is_empty() {
        Vec::new()
    } else {
        YtProfile::list_from_json(&raw)
    }
}

#[tauri::command]
pub async fn yt_switch_identity(
    state: State<'_, AppState>,
    profile_email: String,
    page_id: String,
) -> Result<YtConnection, String> {
    let profiles = yt_list_profiles(state.clone());
    let target = profiles
        .into_iter()
        .find(|p| p.email == profile_email)
        .ok_or_else(|| format!("Profile not found: {profile_email}"))?;
    state
        .tube
        .connect_as(&target.cookies, &target.email, &page_id)
        .await
        .map_err(|e| e.to_string())?;
    let conn = state.tube.connection();
    state.emit_event("yt-connection-changed", conn.clone());
    state.yt_library_sync.kick();
    Ok(conn)
}

#[tauri::command]
pub async fn yt_fetch_liked_songs(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<YouTubeMusicTrack>, String> {
    Ok(state.tube.fetch_liked_songs(limit.unwrap_or(200)).await)
}

#[tauri::command]
pub async fn yt_like_video(
    state: State<'_, AppState>,
    video_id: String,
) -> Result<bool, String> {
    Ok(state.tube.like_video(&video_id).await)
}

#[tauri::command]
pub async fn yt_remove_video_like(
    state: State<'_, AppState>,
    video_id: String,
) -> Result<bool, String> {
    Ok(state.tube.remove_video_like(&video_id).await)
}

#[tauri::command]
pub async fn yt_fetch_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<YouTubeMusicTrack>, String> {
    Ok(state.tube.fetch_yt_history(limit.unwrap_or(100)).await)
}

#[tauri::command]
pub async fn yt_record_watch_history(
    state: State<'_, AppState>,
    video_id: String,
    watched_seconds: i64,
    duration_seconds: Option<i64>,
    cpn: Option<String>,
    watch_url: Option<String>,
) -> Result<bool, String> {
    Ok(state
        .tube
        .record_watch_history(
            &video_id,
            watched_seconds,
            duration_seconds.unwrap_or(0),
            cpn.as_deref(),
            watch_url.as_deref().unwrap_or(""),
        )
        .await)
}

#[tauri::command]
pub async fn yt_fetch_account_playlists(
    state: State<'_, AppState>,
) -> Result<Vec<YouTubePlaylistSummary>, String> {
    Ok(state.tube.fetch_account_playlists().await)
}

#[tauri::command]
pub async fn yt_create_remote_playlist(
    state: State<'_, AppState>,
    title: String,
) -> Result<Option<String>, String> {
    Ok(state.tube.create_remote_playlist(&title).await)
}

#[tauri::command]
pub async fn yt_rename_remote_playlist(
    state: State<'_, AppState>,
    playlist_id: String,
    title: String,
) -> Result<bool, String> {
    Ok(state.tube.rename_remote_playlist(&playlist_id, &title).await)
}

#[tauri::command]
pub async fn yt_delete_remote_playlist(
    state: State<'_, AppState>,
    playlist_id: String,
) -> Result<bool, String> {
    Ok(state.tube.delete_remote_playlist(&playlist_id).await)
}

#[tauri::command]
pub async fn yt_add_videos_to_remote_playlist(
    state: State<'_, AppState>,
    playlist_id: String,
    video_ids: Vec<String>,
) -> Result<bool, String> {
    Ok(state
        .tube
        .add_videos_to_remote_playlist(&playlist_id, &video_ids)
        .await)
}

#[tauri::command]
pub async fn yt_remove_videos_from_remote_playlist(
    state: State<'_, AppState>,
    playlist_id: String,
    removals: Vec<PlaylistRemovalItem>,
) -> Result<bool, String> {
    let pairs: Vec<(String, String)> = removals
        .into_iter()
        .map(|r| (r.set_video_id, r.video_id))
        .collect();
    Ok(state
        .tube
        .remove_videos_from_remote_playlist(&playlist_id, &pairs)
        .await)
}

#[tauri::command]
pub async fn yt_web_sign_in(
    state: State<'_, AppState>,
    fresh: Option<bool>,
) -> Result<YtConnection, String> {
    let login = web_login_for(&state)?;
    let cookies_opt = if fresh.unwrap_or(false) {
        login.sign_in_fresh().await
    } else {
        login.sign_in().await
    };
    let Some(cookies) = cookies_opt else {
        let failure = login.last_failure();
        let reason = match failure {
            YtSignInFailure::None | YtSignInFailure::NoSession => "noSession",
            YtSignInFailure::Unavailable => "unavailable",
            YtSignInFailure::StartFailed => "startFailed",
            YtSignInFailure::WindowClosed => "windowClosed",
            YtSignInFailure::Timeout => "timeout",
            YtSignInFailure::BlankPage => "blankPage",
            YtSignInFailure::BrowserBlocked => "browserBlocked",
        };
        return Err(reason.to_string());
    };
    let page_id = login.last_captured_page_id().unwrap_or_default();
    let email = state
        .tube
        .resolve_identity_for(&cookies, &page_id)
        .await
        .map(|a| a.email)
        .unwrap_or_default();
    state
        .tube
        .connect_as(&cookies, &email, &page_id)
        .await
        .map_err(|e| e.to_string())?;
    let conn = state.tube.connection();
    state.emit_event("yt-connection-changed", conn.clone());
    state.yt_library_sync.kick();
    Ok(conn)
}

#[tauri::command]
pub async fn yt_web_sign_in_cancel(state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(login) = web_login_for(&state) {
        login.hide_window().await;
    } else {
        let _ = YtWebviewGuard::hide();
    }
    Ok(())
}

#[tauri::command]
pub async fn yt_session_refresh(
    state: State<'_, AppState>,
    allow_create_window: Option<bool>,
) -> Result<bool, String> {
    let login = web_login_for(&state)?;
    let ok = state
        .session_keeper
        .refresh(
            &state.tube,
            &login,
            None,
            None,
            allow_create_window.unwrap_or(false),
        )
        .await;
    if ok {
        state.emit_event("yt-connection-changed", state.tube.connection());
    }
    Ok(ok)
}

#[tauri::command]
pub fn yt_library_sync_kick(state: State<'_, AppState>) {
    state.yt_library_sync.kick();
}

#[tauri::command]
pub fn yt_library_sync_apply_held_removals(state: State<'_, AppState>) {
    state.yt_library_sync.apply_held_removals();
}

#[tauri::command]
pub fn yt_po_token_set_enabled(state: State<'_, AppState>, enabled: bool) {
    state.tube.set_po_token_enabled(enabled);
}

#[tauri::command]
pub fn yt_po_token_is_enabled(state: State<'_, AppState>) -> bool {
    state.tube.po_token_enabled()
}

#[tauri::command]
pub fn yt_po_token_pre_warm(state: State<'_, AppState>) {
    state.tube.pre_warm_botguard();
}

#[tauri::command]
pub async fn yt_po_token_mint(
    state: State<'_, AppState>,
    video_id: String,
    force_refresh: Option<bool>,
) -> Result<Option<PoTokenDto>, String> {
    let minted = state
        .po_tokens
        .mint_token(&video_id, "lastwave_session", force_refresh.unwrap_or(false))
        .await;
    Ok(minted.map(|p| PoTokenDto {
        player_token: p.player_token,
        session_token: p.session_token,
    }))
}

#[tauri::command]
pub async fn yt_find_best_match(
    state: State<'_, AppState>,
    title: String,
    artist: String,
    excluded_video_ids: Option<Vec<String>>,
) -> Result<YouTubeMusicTrack, String> {
    let excluded: HashSet<String> = excluded_video_ids.unwrap_or_default().into_iter().collect();
    state
        .tube
        .find_best_match(&title, &artist, &excluded)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn yt_find_best_effort_match(
    state: State<'_, AppState>,
    title: String,
    artist: String,
    excluded_video_ids: Option<Vec<String>>,
) -> Result<Option<YouTubeMusicTrack>, String> {
    let excluded: HashSet<String> = excluded_video_ids.unwrap_or_default().into_iter().collect();
    Ok(state
        .tube
        .find_best_effort_match_or_null(&title, &artist, &excluded)
        .await)
}

#[tauri::command]
pub async fn yt_fetch_playlist_artwork(
    state: State<'_, AppState>,
    playlist_id_or_url: String,
) -> Result<Option<String>, String> {
    Ok(state.tube.fetch_playlist_artwork(&playlist_id_or_url).await)
}

#[tauri::command]
pub fn yt_report_playback_failure(state: State<'_, AppState>, video_id: String) {
    state.tube.report_playback_failure(&video_id);
}

#[tauri::command]
pub fn yt_invalidate_cache(state: State<'_, AppState>, video_id: String) {
    state.tube.invalidate_cache(&video_id);
}

#[tauri::command]
pub fn yt_auth_bundle(state: State<'_, AppState>) -> YtSessionBundle {
    state.tube.auth_store.current()
}

#[tauri::command]
pub async fn yt_resolve_identity_for(
    state: State<'_, AppState>,
    cookies: String,
    page_id: Option<String>,
) -> Result<Option<YtAccount>, String> {
    Ok(state
        .tube
        .resolve_identity_for(&cookies, page_id.as_deref().unwrap_or(""))
        .await)
}

#[tauri::command]
pub async fn yt_fetch_channels_for(
    state: State<'_, AppState>,
    cookies: String,
) -> Result<Vec<YtChannel>, String> {
    Ok(state.tube.fetch_channels_for(&cookies).await)
}

#[tauri::command]
pub fn yt_save_profiles(state: State<'_, AppState>, profiles: Vec<YtProfile>) {
    let encoded = YtProfile::list_to_json(&profiles);
    state.secure.write_yt_profiles(Some(&encoded));
}

#[tauri::command]
pub async fn yt_swr_liked_songs(
    state: State<'_, AppState>,
    force_fresh: Option<bool>,
) -> Result<Vec<YouTubeMusicTrack>, String> {
    let providers = library_providers_for(&state);
    Ok(providers
        .yt_liked_songs(force_fresh.unwrap_or(false))
        .await)
}

#[tauri::command]
pub async fn yt_swr_history(
    state: State<'_, AppState>,
    force_fresh: Option<bool>,
) -> Result<Vec<YouTubeMusicTrack>, String> {
    let providers = library_providers_for(&state);
    Ok(providers.yt_history(force_fresh.unwrap_or(false)).await)
}

#[tauri::command]
pub async fn yt_swr_account_playlists(
    state: State<'_, AppState>,
    force_fresh: Option<bool>,
) -> Result<Vec<YouTubePlaylistSummary>, String> {
    let providers = library_providers_for(&state);
    Ok(providers
        .yt_account_playlists(force_fresh.unwrap_or(false))
        .await)
}

#[tauri::command]
pub async fn yt_swr_home_shelves(
    state: State<'_, AppState>,
    force_fresh: Option<bool>,
) -> Result<Vec<YtHomeShelf>, String> {
    let providers = library_providers_for(&state);
    Ok(providers
        .yt_home_shelves(force_fresh.unwrap_or(false))
        .await)
}

#[tauri::command]
pub async fn yt_swr_playlist_detail(
    state: State<'_, AppState>,
    id: String,
    force_fresh: Option<bool>,
) -> Result<Option<YouTubePlaylistResult>, String> {
    let providers = library_providers_for(&state);
    Ok(providers
        .yt_playlist_detail(&id, force_fresh.unwrap_or(false))
        .await)
}
