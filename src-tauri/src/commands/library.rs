//! Tauri IPC command handlers for storage, library, playlists, local music,
//! downloads, backup/restore, and artwork/snapshot caches.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::state::AppState;
use crate::storage::app_database::{
    ExclusionEntry, LocalStatsSummary, RecentPlayEntry, SnapshotCacheStats,
};
use crate::storage::artwork_cache::ArtworkCache;
use crate::storage::backup_repository::{BackupRead, BackupRepository};
use crate::storage::download_manager::{
    DownloadEntry, DownloadTrackRequest, NoopDownloadStreamResolver,
};
use crate::storage::library_index::{
    group_library_albums, group_library_artists, merge_library_tracks, LibraryAlbum, LibraryArtist,
};
use crate::storage::local_music_library::LocalMusicLibrary;
use crate::storage::playlists::{SavedPlaylist, StoredTrack};

// -- DTOs --------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadEnqueueArgs {
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub year: String,
    #[serde(default)]
    pub video_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreviewResponse {
    pub valid: bool,
    pub invalid_reason: Option<String>,
    pub playlist_count: usize,
    pub exclusion_count: usize,
    pub newer_local_titles: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkCacheStats {
    pub object_count: usize,
    pub total_bytes: u64,
}

// -- Search history, exclusions, play history & local stats ------------------

#[tauri::command]
pub fn library_search_history_load(state: State<'_, AppState>, limit: Option<i64>) -> Vec<String> {
    state.db.load_search_history(limit.unwrap_or(20))
}

#[tauri::command]
pub fn library_search_history_push(state: State<'_, AppState>, query: String) {
    state.db.push_search_history(&query);
}

#[tauri::command]
pub fn library_search_history_remove(state: State<'_, AppState>, query: String) {
    state.db.remove_search_history(&query);
}

#[tauri::command]
pub fn library_search_history_clear(state: State<'_, AppState>) {
    state.db.clear_search_history();
}

#[tauri::command]
pub fn library_exclusions_list(state: State<'_, AppState>) -> Vec<ExclusionEntry> {
    state.db.list_exclusions()
}

#[tauri::command]
pub fn library_exclusions_add(state: State<'_, AppState>, name: String, artist: String) {
    state.db.add_exclusion(&name, &artist);
}

#[tauri::command]
pub fn library_exclusions_remove(state: State<'_, AppState>, key: String) {
    state.db.remove_exclusion(&key);
}

#[tauri::command]
pub fn library_recent_plays(
    state: State<'_, AppState>,
    limit: Option<i64>,
    since_millis: Option<i64>,
) -> Vec<RecentPlayEntry> {
    state
        .db
        .load_recent_plays(limit.unwrap_or(100), since_millis)
}

#[tauri::command]
pub fn library_plays_for_date(state: State<'_, AppState>, date: String) -> Vec<RecentPlayEntry> {
    state.db.load_plays_for_date(&date)
}

#[tauri::command]
pub fn library_active_play_dates(state: State<'_, AppState>) -> Vec<String> {
    state.db.load_active_play_dates().into_iter().collect()
}

#[tauri::command]
pub fn library_local_stats_summary(
    state: State<'_, AppState>,
    since_millis: Option<i64>,
    prev_since_millis: Option<i64>,
) -> Option<LocalStatsSummary> {
    state
        .db
        .get_local_stats_summary(since_millis, prev_since_millis)
}

// -- Snapshot & Artwork caches -----------------------------------------------

#[tauri::command]
pub fn library_snapshot_cache_stats(
    state: State<'_, AppState>,
) -> HashMap<String, SnapshotCacheStats> {
    state.db.cache_stats()
}

#[tauri::command]
pub fn library_snapshot_cache_clear(state: State<'_, AppState>, group: Option<String>) {
    state.db.cache_clear(group.as_deref());
}

#[tauri::command]
pub fn library_snapshot_cache_delete(state: State<'_, AppState>, key: String) {
    state.db.cache_delete(&key);
}

#[tauri::command]
pub fn library_artwork_cache_stats(state: State<'_, AppState>) -> ArtworkCacheStats {
    let cache = ArtworkCache::new(&state.app_support_dir);
    ArtworkCacheStats {
        object_count: cache.object_count(),
        total_bytes: cache.total_bytes(),
    }
}

#[tauri::command]
pub fn library_artwork_cache_clear(state: State<'_, AppState>) {
    ArtworkCache::new(&state.app_support_dir).clear();
}

#[tauri::command]
pub fn library_artwork_cache_evict(state: State<'_, AppState>) -> usize {
    ArtworkCache::new(&state.app_support_dir).evict()
}

// -- Playlists & Liked Songs -------------------------------------------------

#[tauri::command]
pub fn library_playlists_all(state: State<'_, AppState>) -> Vec<SavedPlaylist> {
    state.playlists.all()
}

#[tauri::command]
pub fn library_playlists_refresh(state: State<'_, AppState>) {
    state.playlists.refresh();
}

#[tauri::command]
pub fn library_playlists_ensure_liked(state: State<'_, AppState>) -> SavedPlaylist {
    state.playlists.ensure_liked_songs()
}

#[tauri::command]
pub fn library_playlists_liked_keys(state: State<'_, AppState>) -> Vec<String> {
    state.playlists.liked_keys().into_iter().collect()
}

#[tauri::command]
pub fn library_playlists_toggle_liked(state: State<'_, AppState>, track: StoredTrack) -> bool {
    state.playlists.toggle_liked(track)
}

#[tauri::command]
pub fn library_playlists_create(state: State<'_, AppState>, title: String) -> SavedPlaylist {
    state.playlists.create_custom(&title)
}

#[tauri::command]
pub fn library_playlists_create_library(state: State<'_, AppState>, title: String) -> SavedPlaylist {
    state.playlists.create_library(&title)
}

#[tauri::command]
pub fn library_playlists_save_generated(
    state: State<'_, AppState>,
    title: String,
    tracks: Vec<StoredTrack>,
) -> SavedPlaylist {
    state.playlists.save_generated(&title, &tracks)
}

#[tauri::command]
pub fn library_playlists_rename(state: State<'_, AppState>, id: i64, title: String) {
    state.playlists.rename(id, &title);
}

#[tauri::command]
pub fn library_playlists_set_pinned(state: State<'_, AppState>, id: i64, pinned: bool) {
    state.playlists.set_pinned(id, pinned);
}

#[tauri::command]
pub fn library_playlists_set_custom_cover(
    state: State<'_, AppState>,
    id: i64,
    source_path: String,
) -> Result<String, String> {
    state
        .playlists
        .set_custom_cover(id, Path::new(&source_path), &state.app_support_dir)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn library_playlists_delete(state: State<'_, AppState>, id: i64) {
    state.playlists.delete(id);
}

#[tauri::command]
pub fn library_playlists_add_track(
    state: State<'_, AppState>,
    id: i64,
    track: StoredTrack,
) -> bool {
    state.playlists.add_track(id, track)
}

#[tauri::command]
pub fn library_playlists_remove_track(
    state: State<'_, AppState>,
    id: i64,
    track_key: String,
) {
    state.playlists.remove_track(id, &track_key);
}

#[tauri::command]
pub fn library_playlists_apply_batch(
    state: State<'_, AppState>,
    id: i64,
    add: Vec<StoredTrack>,
    remove_keys: Vec<String>,
) {
    let remove_set: HashSet<String> = remove_keys.into_iter().collect();
    state.playlists.apply_batch(id, &add, &remove_set);
}

#[tauri::command]
pub fn library_playlists_replace_tracks(
    state: State<'_, AppState>,
    id: i64,
    tracks: Vec<StoredTrack>,
) {
    state.playlists.replace_tracks(id, &tracks);
}

// -- Local Music Library & Library Index -------------------------------------

#[tauri::command]
pub fn library_local_folders(state: State<'_, AppState>) -> Vec<String> {
    state.db.local_folders()
}

#[tauri::command]
pub fn library_local_add_folder(state: State<'_, AppState>, path: String) -> usize {
    let lib = LocalMusicLibrary::new(&state.db, &state.app_support_dir);
    lib.add_folder(Path::new(&path), &state.playlists, |_| {})
}

#[tauri::command]
pub fn library_local_forget_folder(state: State<'_, AppState>, path: String) -> usize {
    let lib = LocalMusicLibrary::new(&state.db, &state.app_support_dir);
    lib.forget_folder(&path, &state.playlists, |_| {})
}

#[tauri::command]
pub fn library_local_rescan(state: State<'_, AppState>) -> usize {
    let lib = LocalMusicLibrary::new(&state.db, &state.app_support_dir);
    lib.rescan(&state.playlists, |_| {})
}

fn collect_merged_library_tracks(state: &AppState) -> Vec<StoredTrack> {
    let local: Vec<StoredTrack> = state
        .playlists
        .all()
        .into_iter()
        .filter(|p| p.is_local())
        .flat_map(|p| p.tracks)
        .collect();
    let downloads = state.downloads.all();
    merge_library_tracks(&local, &downloads)
}

#[tauri::command]
pub fn library_index_tracks(state: State<'_, AppState>) -> Vec<StoredTrack> {
    collect_merged_library_tracks(&state)
}

#[tauri::command]
pub fn library_index_albums(state: State<'_, AppState>) -> Vec<LibraryAlbum<StoredTrack>> {
    let tracks = collect_merged_library_tracks(&state);
    group_library_albums(&tracks)
}

#[tauri::command]
pub fn library_index_artists(state: State<'_, AppState>) -> Vec<LibraryArtist<StoredTrack>> {
    let tracks = collect_merged_library_tracks(&state);
    group_library_artists(&tracks)
}

// -- Downloads ---------------------------------------------------------------

#[tauri::command]
pub fn library_downloads_all(state: State<'_, AppState>) -> Vec<DownloadEntry> {
    state.downloads.all()
}

#[tauri::command]
pub fn library_downloads_enqueue(state: State<'_, AppState>, args: DownloadEnqueueArgs) {
    let downloads = Arc::clone(&state.downloads);
    let req = DownloadTrackRequest {
        title: args.title,
        artist: args.artist,
        album: args.album,
        artwork_url: args.artwork_url,
        year: args.year,
        video_id: args.video_id,
    };
    tokio::spawn(async move {
        let resolver = NoopDownloadStreamResolver;
        downloads.download_track(&resolver, req).await;
    });
}

#[tauri::command]
pub fn library_downloads_cancel(state: State<'_, AppState>, key: String) {
    state.downloads.cancel_download(&key);
}

#[tauri::command]
pub fn library_downloads_cancel_active(state: State<'_, AppState>) {
    state.downloads.cancel_active();
}

#[tauri::command]
pub fn library_downloads_delete(state: State<'_, AppState>, key: String) {
    state.downloads.delete(&key);
}

#[tauri::command]
pub fn library_downloads_reorganize(state: State<'_, AppState>) -> (usize, usize) {
    state.downloads.reorganize()
}

#[tauri::command]
pub fn library_downloads_is_downloaded(
    state: State<'_, AppState>,
    title: String,
    artist: String,
) -> bool {
    state.downloads.is_downloaded(&title, &artist)
}

#[tauri::command]
pub fn library_downloads_local_path(
    state: State<'_, AppState>,
    title: String,
    artist: String,
) -> Option<String> {
    state.downloads.local_path_for(&title, &artist)
}

// -- Backup & Restore --------------------------------------------------------

#[tauri::command]
pub fn library_backup_export(state: State<'_, AppState>) -> String {
    let repo = BackupRepository::new();
    let playlists = state.playlists.all();
    repo.build(&state.prefs, &state.db, &playlists)
}

#[tauri::command]
pub fn library_backup_export_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let repo = BackupRepository::new();
    let playlists = state.playlists.all();
    let json = repo.build(&state.prefs, &state.db, &playlists);
    repo.write_file(Path::new(&path), &json)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn library_backup_preview(
    state: State<'_, AppState>,
    content: String,
) -> BackupPreviewResponse {
    let repo = BackupRepository::new();
    let local = state.playlists.all();
    match repo.read(&content, &local) {
        BackupRead::Ok(p) => BackupPreviewResponse {
            valid: true,
            invalid_reason: None,
            playlist_count: p.playlists.len(),
            exclusion_count: p.exclusion_count,
            newer_local_titles: p.newer_local.into_iter().map(|pl| pl.title).collect(),
        },
        BackupRead::Invalid => BackupPreviewResponse {
            valid: false,
            invalid_reason: Some("That file is not a LastWave backup.".to_string()),
            playlist_count: 0,
            exclusion_count: 0,
            newer_local_titles: Vec::new(),
        },
        BackupRead::Unsupported => BackupPreviewResponse {
            valid: false,
            invalid_reason: Some("That backup was created by a newer version of LastWave.".to_string()),
            playlist_count: 0,
            exclusion_count: 0,
            newer_local_titles: Vec::new(),
        },
    }
}

#[tauri::command]
pub fn library_backup_restore_file(
    state: State<'_, AppState>,
    content: String,
    overwrite_newer: bool,
) -> Result<usize, String> {
    let repo = BackupRepository::new();
    repo.restore_file(&content, &state.prefs, &state.db, &state.playlists, overwrite_newer)
}
