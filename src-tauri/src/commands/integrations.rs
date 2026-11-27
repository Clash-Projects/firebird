//! Tauri IPC command handlers for all `integrations/` services.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::state::AppState;
use crate::extractor::innertube_api::{
    InnerTubeMusicApi, YouTubeMusicEntity, YouTubeMusicTrack,
};
use crate::integrations::artwork::animated_artwork_service::AnimatedArtworkQuery;
use crate::integrations::artwork::official_artwork_service::OfficialArtworkResult;
use crate::integrations::artwork::AnimatedArtwork;
use crate::integrations::feed::feed_repository::{FeedData, GeneratedTrack};
use crate::integrations::feed::genre_browse::fetch_genre_tracks;
use crate::integrations::home::home_providers::{
    load_home_feed, load_new_albums, load_personal_mix,
};
use crate::integrations::importers::playlist_file::{
    export_playlist_csv, export_playlist_m3u, PlaylistFileRow,
};
use crate::integrations::importers::playlist_import::{
    import_preview_with, preview_playlist_file_with, preview_playlist_link_with, BoxFuture,
    PlaylistImportPreview, PlaylistTrackSource, ResolvedPlaylist, ResolvedTrack,
};
use crate::integrations::importers::playlist_link::{detect_playlist_link, PlaylistLinkSource};
use crate::integrations::jam::jam_client::{JamController, JamSnapshot, JamTrack};
use crate::integrations::lastfm::auth_repository::{AuthState, WebAuthHandshake};
use crate::integrations::lastfm::profile_providers::fetch_user_profile_stats;
use crate::integrations::lyrics::lyrics_translation::LyricsTranslator;
use crate::integrations::podcasts::podcast_repository::{
    PodcastApiError, PodcastRepository, PodcastTubeClient,
};
use crate::integrations::scrobblers::extra_scrobblers::Listen;
use crate::integrations::scrobblers::scrobbler_session::ScrobblerSession;
use crate::integrations::search::search_repository::{
    SearchResultItem, SearchSuggestion, SearchTab,
};
use crate::integrations::spotify::spotify_api::{
    SpotifyCanvasResult, SpotifyHomeSection, SpotifyPlaylistContent, SpotifyPlaylistSummary,
    SpotifyTrackRow,
};
use crate::integrations::spotify::spotify_auth::SpotifySessionCheck;
use crate::integrations::stats::stats_providers::{
    compute_listening_stats, ListeningStatsData, StatsPeriod,
};
use crate::integrations::updates::update_service::AppUpdateInfo;
use crate::models::lastfm_home_models::{HomeStats, HomeTrack, TopAlbumItem, TopArtistItem};
use crate::models::lyrics_models::LyricsResult;
use crate::models::player_state::PlayerSnapshot;
use crate::models::podcast_models::{
    PodcastEpisode, PodcastEpisodePage, PodcastShelf, PodcastShowPage,
};
use crate::storage::home_snapshot::{PersonalMixData, SwrState};
use crate::storage::playlists::SavedPlaylist;

// Adapters for trait-object repositories

struct TubePlaylistSource(Arc<InnerTubeMusicApi>);

impl PlaylistTrackSource for TubePlaylistSource {
    fn fetch_playlist<'a>(&'a self, id_or_url: &'a str) -> BoxFuture<'a, Option<ResolvedPlaylist>> {
        Box::pin(async move {
            let res = self.0.fetch_playlist(id_or_url, None).await?;
            Some(ResolvedPlaylist {
                title: res.title,
                tracks: res
                    .tracks
                    .into_iter()
                    .map(|t| ResolvedTrack {
                        title: t.title,
                        artist: t.artist,
                        artwork_url: t.artwork_url,
                        video_id: t.video_id,
                        album: t.album,
                        duration_seconds: t.duration_seconds,
                    })
                    .collect(),
            })
        })
    }

    fn find_match<'a>(
        &'a self,
        title: &'a str,
        artist: &'a str,
    ) -> BoxFuture<'a, Option<ResolvedTrack>> {
        Box::pin(async move {
            let empty = HashSet::new();
            let t = self.0.find_best_match_or_null(title, artist, &empty).await?;
            Some(ResolvedTrack {
                title: t.title,
                artist: t.artist,
                artwork_url: t.artwork_url,
                video_id: t.video_id,
                album: t.album,
                duration_seconds: t.duration_seconds,
            })
        })
    }

    fn fetch_song<'a>(&'a self, video_id: &'a str) -> BoxFuture<'a, Option<ResolvedTrack>> {
        Box::pin(async move {
            let t = self.0.fetch_song_details(video_id).await?;
            Some(ResolvedTrack {
                title: t.title,
                artist: t.artist,
                artwork_url: t.artwork_url,
                video_id: t.video_id,
                album: t.album,
                duration_seconds: t.duration_seconds,
            })
        })
    }
}

struct TubePodcastClient(Arc<InnerTubeMusicApi>);

impl PodcastTubeClient for TubePodcastClient {
    fn fetch_podcast_home(
        &self,
    ) -> crate::integrations::podcasts::podcast_repository::BoxFuture<
        '_,
        Result<Vec<PodcastShelf>, PodcastApiError>,
    > {
        Box::pin(async move {
            self.0
                .fetch_podcast_home()
                .await
                .map_err(|e| {
                    let code = match e {
                        crate::extractor::innertube_api::InnerTubeError::Http(c) => Some(c),
                        _ => None,
                    };
                    PodcastApiError::new(code, e.to_string())
                })
        })
    }

    fn search_podcast_episodes<'a>(
        &'a self,
        query: &'a str,
        limit: usize,
    ) -> crate::integrations::podcasts::podcast_repository::BoxFuture<
        'a,
        Result<Vec<PodcastEpisode>, PodcastApiError>,
    > {
        Box::pin(async move { Ok(self.0.search_podcast_episodes(query, limit).await) })
    }

    fn fetch_podcast_show<'a>(
        &'a self,
        show_id: &'a str,
    ) -> crate::integrations::podcasts::podcast_repository::BoxFuture<
        'a,
        Result<Option<PodcastShowPage>, PodcastApiError>,
    > {
        Box::pin(async move {
            self.0
                .fetch_podcast_show(show_id)
                .await
                .map_err(|e| {
                    let code = match e {
                        crate::extractor::innertube_api::InnerTubeError::Http(c) => Some(c),
                        _ => None,
                    };
                    PodcastApiError::new(code, e.to_string())
                })
        })
    }

    fn fetch_podcast_show_continuation<'a>(
        &'a self,
        show_id: &'a str,
        continuation: &'a str,
        show_title: Option<&'a str>,
    ) -> crate::integrations::podcasts::podcast_repository::BoxFuture<
        'a,
        Result<PodcastEpisodePage, PodcastApiError>,
    > {
        Box::pin(async move {
            self.0
                .fetch_podcast_show_continuation(show_id, continuation, show_title)
                .await
                .map_err(|e| {
                    let code = match e {
                        crate::extractor::innertube_api::InnerTubeError::Http(c) => Some(c),
                        _ => None,
                    };
                    PodcastApiError::new(code, e.to_string())
                })
        })
    }
}

fn podcast_repo(state: &AppState) -> PodcastRepository {
    PodcastRepository::new(
        Arc::new(TubePodcastClient(Arc::clone(&state.tube))),
        Arc::clone(&state.db),
        Arc::clone(&state.prefs),
    )
}

// Lyrics commands

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn lyrics_fetch(
    state: State<'_, AppState>,
    title: String,
    artist: String,
    album: String,
    duration_seconds: Option<i64>,
    force_refresh: bool,
    word_by_word: bool,
    video_id: Option<String>,
    preferred_provider_id: String,
    exclude_provider_ids: Vec<String>,
) -> Result<LyricsResult, String> {
    let excludes: HashSet<String> = exclude_provider_ids.into_iter().collect();
    Ok(state
        .lyrics_repo
        .get_lyrics(
            &title,
            &artist,
            &album,
            duration_seconds,
            force_refresh,
            word_by_word,
            video_id.as_deref(),
            &preferred_provider_id,
            &excludes,
        )
        .await)
}

#[tauri::command]
pub async fn lyrics_fetch_provider(
    state: State<'_, AppState>,
    provider_id: String,
    title: String,
    artist: String,
    album: String,
    duration_seconds: Option<i64>,
    video_id: Option<String>,
) -> Result<Option<LyricsResult>, String> {
    Ok(state
        .lyrics_repo
        .fetch_from_provider(
            &provider_id,
            &title,
            &artist,
            &album,
            duration_seconds,
            video_id.as_deref(),
        )
        .await)
}

#[tauri::command]
pub async fn lyrics_translate(
    state: State<'_, AppState>,
    track_key: String,
    target: String,
    lines: Vec<String>,
) -> Result<Vec<Option<String>>, String> {
    let translator = LyricsTranslator::with_client(state.http.clone());
    Ok(translator.translate(&track_key, &target, &lines).await)
}

// Last.fm & Extra Scrobblers commands

#[tauri::command]
pub async fn lastfm_auth_state(state: State<'_, AppState>) -> Result<AuthState, String> {
    Ok(state.auth_repo.state().await)
}

#[tauri::command]
pub async fn lastfm_enter_guest_mode(state: State<'_, AppState>) -> Result<(), String> {
    state.auth_repo.enter_guest_mode().await;
    Ok(())
}

#[tauri::command]
pub async fn lastfm_begin_web_auth(
    state: State<'_, AppState>,
) -> Result<WebAuthHandshake, String> {
    state
        .auth_repo
        .begin_web_auth()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lastfm_complete_web_auth(
    state: State<'_, AppState>,
    token: String,
) -> Result<(), String> {
    state
        .auth_repo
        .complete_web_auth(&token)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lastfm_save_custom_keys(
    state: State<'_, AppState>,
    api_key: String,
    api_secret: String,
) -> Result<(), String> {
    state
        .auth_repo
        .save_custom_keys(&api_key, &api_secret)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lastfm_clear_custom_keys(state: State<'_, AppState>) -> Result<(), String> {
    state.auth_repo.clear_custom_keys().await;
    Ok(())
}

#[tauri::command]
pub async fn lastfm_sign_out(state: State<'_, AppState>) -> Result<(), String> {
    state.auth_repo.sign_out().await;
    Ok(())
}

#[tauri::command]
pub async fn lastfm_clear_error(state: State<'_, AppState>) -> Result<(), String> {
    state.auth_repo.clear_error().await;
    Ok(())
}

#[tauri::command]
pub async fn lastfm_scrobble_now_playing(
    state: State<'_, AppState>,
    artist: String,
    track: String,
    album: Option<String>,
) -> Result<(), String> {
    state
        .scrobbler
        .update_now_playing(&artist, &track, album.as_deref().unwrap_or(""))
        .await;
    Ok(())
}

#[tauri::command]
pub async fn lastfm_scrobble_track(
    state: State<'_, AppState>,
    artist: String,
    track: String,
    timestamp: i64,
    album: Option<String>,
) -> Result<(), String> {
    state
        .scrobbler
        .scrobble(&artist, &track, album.as_deref().unwrap_or(""), timestamp)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn lastfm_set_loved(
    state: State<'_, AppState>,
    artist: String,
    track: String,
    loved: bool,
) -> Result<bool, String> {
    Ok(state.scrobbler.set_loved(&artist, &track, loved).await)
}

#[tauri::command]
pub async fn lastfm_queued_count(state: State<'_, AppState>) -> Result<usize, String> {
    Ok(state.scrobbler.queued_count().await)
}

#[tauri::command]
pub async fn lastfm_recent_tracks(
    state: State<'_, AppState>,
    username: Option<String>,
    limit: usize,
    page: usize,
) -> Result<Vec<HomeTrack>, String> {
    state
        .lastfm_home
        .fetch_recent_tracks(username.as_deref(), limit, page)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lastfm_stats(
    state: State<'_, AppState>,
    username: Option<String>,
) -> Result<HomeStats, String> {
    state
        .lastfm_home
        .fetch_stats(username.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lastfm_top_tracks(
    state: State<'_, AppState>,
    username: Option<String>,
    period: String,
    limit: usize,
) -> Result<Vec<HomeTrack>, String> {
    state
        .lastfm_home
        .fetch_top_tracks(username.as_deref(), &period, limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lastfm_top_artists(
    state: State<'_, AppState>,
    username: Option<String>,
    period: String,
    limit: usize,
) -> Result<Vec<TopArtistItem>, String> {
    Ok(state
        .lastfm_home
        .fetch_top_artists(username.as_deref(), &period, limit)
        .await)
}

#[tauri::command]
pub async fn lastfm_top_albums(
    state: State<'_, AppState>,
    username: Option<String>,
    period: String,
    limit: usize,
) -> Result<Vec<TopAlbumItem>, String> {
    Ok(state
        .lastfm_home
        .fetch_top_albums(username.as_deref(), &period, limit)
        .await)
}

#[tauri::command]
pub async fn lastfm_profile_stats(
    state: State<'_, AppState>,
    username: Option<String>,
) -> Result<HomeStats, String> {
    fetch_user_profile_stats(&state.lastfm_home, &ScrobblerSession::none(), username.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn extra_scrobblers_connect_listenbrainz(
    state: State<'_, AppState>,
    token: String,
    server: String,
) -> Result<Option<String>, String> {
    Ok(state.listen_brainz.connect(&token, &server).await)
}

#[tauri::command]
pub fn extra_scrobblers_disconnect_listenbrainz(state: State<'_, AppState>) {
    state.listen_brainz.disconnect();
}

#[tauri::command]
pub async fn extra_scrobblers_connect_librefm(
    state: State<'_, AppState>,
    username: String,
    password: String,
    server: String,
) -> Result<Option<String>, String> {
    Ok(state.libre_fm.connect(&username, &password, &server).await)
}

#[tauri::command]
pub fn extra_scrobblers_disconnect_librefm(state: State<'_, AppState>) {
    state.libre_fm.disconnect();
}

#[tauri::command]
pub async fn extra_scrobblers_submit(
    state: State<'_, AppState>,
    artist: String,
    track: String,
    album: Option<String>,
    timestamp: i64,
) -> Result<(), String> {
    let listen = Listen::new(artist, track, album.unwrap_or_default(), timestamp);
    state.extra_scrobblers.submit_and_wait(listen).await;
    Ok(())
}

// Discord Presence commands

#[tauri::command]
pub async fn discord_presence_update(
    state: State<'_, AppState>,
    snapshot: PlayerSnapshot,
) -> Result<(), String> {
    state.discord_presence.on_snapshot(snapshot).await;
    Ok(())
}

#[tauri::command]
pub async fn discord_presence_shutdown(state: State<'_, AppState>) -> Result<(), String> {
    state.discord_presence.shutdown().await;
    Ok(())
}

// Jam commands

#[tauri::command]
pub async fn jam_snapshot(state: State<'_, AppState>) -> Result<JamSnapshot, String> {
    Ok(state.jam.snapshot().await)
}

#[tauri::command]
pub async fn jam_create_room(
    state: State<'_, AppState>,
    display_name: String,
    collaborative: bool,
    browser_guests: bool,
) -> Result<(), String> {
    state
        .jam
        .create_room(&display_name, collaborative, browser_guests)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn jam_join_room(
    state: State<'_, AppState>,
    room_code: String,
    display_name: String,
) -> Result<(), String> {
    state.jam.join_room(&room_code, &display_name).await;
    Ok(())
}

#[tauri::command]
pub async fn jam_leave(state: State<'_, AppState>) -> Result<(), String> {
    state.jam.leave().await;
    Ok(())
}

#[tauri::command]
pub async fn jam_send_chat(
    state: State<'_, AppState>,
    text: String,
    reply_to_id: Option<String>,
) -> Result<(), String> {
    state.jam.send_chat(&text, reply_to_id.as_deref()).await;
    Ok(())
}

#[tauri::command]
pub async fn jam_grant_host(
    state: State<'_, AppState>,
    user_id: String,
) -> Result<(), String> {
    state.jam.grant_host(&user_id).await;
    Ok(())
}

#[tauri::command]
pub async fn jam_revoke_host(
    state: State<'_, AppState>,
    user_id: String,
) -> Result<(), String> {
    state.jam.revoke_host(&user_id).await;
    Ok(())
}

#[tauri::command]
pub async fn jam_transfer_admin(
    state: State<'_, AppState>,
    user_id: String,
) -> Result<(), String> {
    state.jam.transfer_admin(&user_id).await;
    Ok(())
}

#[tauri::command]
pub async fn jam_broadcast_queue(
    state: State<'_, AppState>,
    queue: Vec<JamTrack>,
) -> Result<(), String> {
    state.jam.broadcast_queue(queue).await;
    Ok(())
}

#[tauri::command]
pub async fn jam_set_web_guests(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    state.jam.set_web_guests(enabled).await;
    Ok(())
}

#[tauri::command]
pub async fn jam_room_counts(
    state: State<'_, AppState>,
) -> Result<HashMap<String, i64>, String> {
    Ok(state.jam.fetch_room_counts().await)
}

#[tauri::command]
pub fn jam_parse_code(raw: String) -> String {
    JamController::parse_code(&raw)
}

#[tauri::command]
pub fn jam_invite_link(state: State<'_, AppState>, code: String) -> String {
    state.jam.invite_link(&code)
}

#[tauri::command]
pub async fn jam_on_player_snapshot(
    state: State<'_, AppState>,
    previous: Option<PlayerSnapshot>,
    next: PlayerSnapshot,
) -> Result<(), String> {
    state
        .jam_sync
        .on_player_snapshot(previous.as_ref(), &next)
        .await;
    Ok(())
}

// Spotify commands

#[tauri::command]
pub async fn spotify_connected(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.spotify_auth.connected().await)
}

#[tauri::command]
pub async fn spotify_connect_sp_dc(
    state: State<'_, AppState>,
    raw: String,
) -> Result<SpotifySessionCheck, String> {
    Ok(state.spotify_auth.connect_with_sp_dc(&raw).await)
}

#[tauri::command]
pub async fn spotify_disconnect(state: State<'_, AppState>) -> Result<(), String> {
    state.spotify_auth.disconnect().await;
    Ok(())
}

#[tauri::command]
pub async fn spotify_verify(state: State<'_, AppState>) -> Result<SpotifySessionCheck, String> {
    Ok(state.spotify_auth.verify().await)
}

#[tauri::command]
pub async fn spotify_user_playlists(
    state: State<'_, AppState>,
) -> Result<Vec<SpotifyPlaylistSummary>, String> {
    Ok(state.spotify_api.user_playlists().await)
}

#[tauri::command]
pub async fn spotify_playlist(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<SpotifyPlaylistContent>, String> {
    Ok(state.spotify_api.playlist(&id).await)
}

#[tauri::command]
pub async fn spotify_home(
    state: State<'_, AppState>,
) -> Result<Vec<SpotifyHomeSection>, String> {
    Ok(state.spotify_api.home().await)
}

#[tauri::command]
pub async fn spotify_song_radio(
    state: State<'_, AppState>,
    title: String,
    artist: String,
    duration_ms: i64,
) -> Result<Vec<SpotifyTrackRow>, String> {
    Ok(state
        .spotify_api
        .song_radio(&title, &artist, duration_ms)
        .await)
}

#[tauri::command]
pub async fn spotify_canvas(
    state: State<'_, AppState>,
    title: String,
    artist: String,
    duration_ms: i64,
) -> Result<SpotifyCanvasResult, String> {
    Ok(state
        .spotify_api
        .canvas(&title, &artist, duration_ms)
        .await)
}

// Podcasts commands

#[tauri::command]
pub async fn podcasts_home(
    state: State<'_, AppState>,
    force: bool,
) -> Result<Vec<PodcastShelf>, String> {
    podcast_repo(&state)
        .home(force)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn podcasts_show(
    state: State<'_, AppState>,
    show_id: String,
    force: bool,
) -> Result<Option<PodcastShowPage>, String> {
    podcast_repo(&state)
        .show(&show_id, force)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn podcasts_more(
    state: State<'_, AppState>,
    show_id: String,
    continuation: Option<String>,
) -> Result<PodcastEpisodePage, String> {
    podcast_repo(&state)
        .more(&show_id, continuation.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn podcasts_search(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<PodcastEpisode>, String> {
    podcast_repo(&state)
        .search(&query)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn podcasts_remember(state: State<'_, AppState>, episode: PodcastEpisode) {
    podcast_repo(&state).remember(&episode);
}

#[tauri::command]
pub fn podcasts_resume_ms(state: State<'_, AppState>, video_id: String) -> i64 {
    podcast_repo(&state).resume_ms(&video_id)
}

// Playlist Importer commands

#[tauri::command]
pub async fn playlist_preview_link(
    state: State<'_, AppState>,
    raw_link: String,
) -> Result<PlaylistImportPreview, String> {
    let source = TubePlaylistSource(Arc::clone(&state.tube));
    preview_playlist_link_with(&source, &state.http, &raw_link, Duration::from_secs(15))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn playlist_preview_file(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
    filename: String,
) -> Result<PlaylistImportPreview, String> {
    let source = TubePlaylistSource(Arc::clone(&state.tube));
    preview_playlist_file_with(&source, &bytes, &filename)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn playlist_import_preview(
    state: State<'_, AppState>,
    preview: PlaylistImportPreview,
) -> Result<SavedPlaylist, String> {
    import_preview_with(state.playlists.as_ref(), &preview).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn playlist_export_csv(rows: Vec<PlaylistFileRow>) -> String {
    export_playlist_csv(&rows)
}

#[tauri::command]
pub fn playlist_export_m3u(title: String, rows: Vec<PlaylistFileRow>) -> String {
    export_playlist_m3u(&title, &rows)
}

#[tauri::command]
pub fn playlist_detect_link(raw: String) -> Option<PlaylistLinkSource> {
    detect_playlist_link(&raw)
}

// Artwork commands

#[tauri::command]
pub async fn artwork_resolve_track(
    state: State<'_, AppState>,
    title: String,
    artist: String,
    album: Option<String>,
) -> Result<Option<OfficialArtworkResult>, String> {
    Ok(state
        .official_artwork
        .resolve_official_artwork(&title, &artist, album.as_deref().unwrap_or(""))
        .await)
}

#[tauri::command]
pub async fn artwork_resolve_album(
    state: State<'_, AppState>,
    album: String,
    artist: String,
) -> Result<Option<OfficialArtworkResult>, String> {
    Ok(state
        .official_artwork
        .resolve_album_artwork(&album, &artist)
        .await)
}

#[tauri::command]
pub async fn artwork_resolve_artist(
    state: State<'_, AppState>,
    artist: String,
) -> Result<Option<OfficialArtworkResult>, String> {
    Ok(state.official_artwork.resolve_artist_artwork(&artist).await)
}

#[tauri::command]
pub async fn artwork_lookup_animated(
    state: State<'_, AppState>,
    artist: String,
    album: String,
    title: String,
) -> Result<Option<AnimatedArtwork>, String> {
    let query = AnimatedArtworkQuery::new(artist, album, title);
    Ok(state.animated_artwork.lookup(&query).await)
}

// Home, Feed, Search, Stats, Updates commands

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalMixResponse {
    pub seed: GeneratedTrack,
    pub tracks: Vec<GeneratedTrack>,
}

impl From<PersonalMixData> for PersonalMixResponse {
    fn from(m: PersonalMixData) -> Self {
        Self {
            seed: m.seed,
            tracks: m.tracks,
        }
    }
}

#[tauri::command]
pub async fn home_load_feed(
    state: State<'_, AppState>,
    charts_only: bool,
) -> Result<FeedData, String> {
    let swr = Arc::new(SwrState::new());
    Ok(load_home_feed(
        Arc::clone(&state.feed_repo),
        Arc::clone(&state.db),
        &state.prefs,
        swr,
        charts_only,
    )
    .await)
}

#[tauri::command]
pub async fn home_load_new_albums(
    state: State<'_, AppState>,
    connected: bool,
    profile_email: String,
    active_page_id: String,
) -> Result<Vec<YouTubeMusicEntity>, String> {
    let swr = Arc::new(SwrState::new());
    Ok(load_new_albums(
        Arc::clone(&state.feed_repo),
        Arc::clone(&state.db),
        swr,
        connected,
        &profile_email,
        &active_page_id,
    )
    .await)
}

#[tauri::command]
pub async fn home_load_personal_mix(
    state: State<'_, AppState>,
    connected: bool,
    profile_email: String,
    active_page_id: String,
) -> Result<Option<PersonalMixResponse>, String> {
    let swr = Arc::new(SwrState::new());
    Ok(load_personal_mix(
        Arc::clone(&state.feed_repo),
        Arc::clone(&state.db),
        swr,
        connected,
        &profile_email,
        &active_page_id,
    )
    .await
    .map(PersonalMixResponse::from))
}

#[tauri::command]
pub async fn feed_fetch_mix(
    state: State<'_, AppState>,
    total: usize,
) -> Result<Vec<GeneratedTrack>, String> {
    Ok(state.feed_repo.fetch_mix(total).await)
}

#[tauri::command]
pub async fn feed_generate_for_tag(
    state: State<'_, AppState>,
    tag: String,
    total: usize,
) -> Result<Vec<GeneratedTrack>, String> {
    Ok(state.feed_repo.generate_for_tag(&tag, total).await)
}

#[tauri::command]
pub async fn feed_fetch_genre_tracks(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<GeneratedTrack>, String> {
    Ok(fetch_genre_tracks(&state.tube, &query).await)
}

#[tauri::command]
pub async fn search_suggestions(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<SearchSuggestion>, String> {
    Ok(state.search_repo.get_suggestions(&query).await)
}

#[tauri::command]
pub async fn search_query(
    state: State<'_, AppState>,
    tab: SearchTab,
    query: String,
) -> Result<Vec<SearchResultItem>, String> {
    Ok(state.search_repo.search(tab, &query).await)
}

#[tauri::command]
pub async fn search_songs_for(
    state: State<'_, AppState>,
    item: SearchResultItem,
) -> Result<Vec<YouTubeMusicTrack>, String> {
    Ok(state.search_repo.songs_for(&item).await)
}

#[tauri::command]
pub fn search_history(state: State<'_, AppState>) -> Vec<String> {
    state.search_repo.history()
}

#[tauri::command]
pub fn search_push_history(state: State<'_, AppState>, query: String) {
    state.search_repo.push_history(&query);
}

#[tauri::command]
pub fn search_clear_history(state: State<'_, AppState>) {
    state.search_repo.clear_history();
}

#[tauri::command]
pub fn search_remove_history(state: State<'_, AppState>, query: String) {
    state.search_repo.remove_history(&query);
}

#[tauri::command]
pub async fn stats_compute_listening(
    state: State<'_, AppState>,
    period: StatsPeriod,
) -> Result<ListeningStatsData, String> {
    Ok(compute_listening_stats(&state.db, Some(&state.official_artwork), period).await)
}

#[tauri::command]
pub async fn updates_check(state: State<'_, AppState>) -> Result<Option<AppUpdateInfo>, String> {
    Ok(state.update_service.check_for_update().await)
}
