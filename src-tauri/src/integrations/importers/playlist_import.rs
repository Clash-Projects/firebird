//! Playlist link & file import flow: preview a pasted link (YouTube direct,
//! Spotify/Apple matched to YouTube) or file (CSV, M3U, text) and create
//! the local playlist.
//!
//! Ported from `lib/features/library/playlist_import.dart`.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use futures_util::future::join_all;
use serde::{Deserialize, Serialize};

use crate::storage::playlists::{PlaylistRepository, SavedPlaylist, StoredTrack};

use super::playlist_file::{
    decode_playlist_bytes, parse_playlist_file, suggested_playlist_title, PlaylistFileRow,
};
use super::playlist_link::{
    detect_playlist_link, extract_playlist_id, fetch_apple_playlist, fetch_spotify_playlist,
    PlaylistLinkSource,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistImportPreview {
    pub title: String,
    pub total_rows: usize,
    pub matched_tracks: Vec<StoredTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTrack {
    pub title: String,
    pub artist: String,
    pub artwork_url: String,
    pub video_id: String,
    pub album: String,
    pub duration_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPlaylist {
    pub title: String,
    pub tracks: Vec<ResolvedTrack>,
}

#[derive(Debug, thiserror::Error)]
pub enum PlaylistImportError {
    #[error("{0}")]
    InvalidFormat(String),
    #[error("{0}")]
    State(String),
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// YouTube access needed by the import (fetch + match).
pub trait PlaylistTrackSource: Send + Sync {
    fn fetch_playlist<'a>(&'a self, id_or_url: &'a str) -> BoxFuture<'a, Option<ResolvedPlaylist>>;
    fn find_match<'a>(&'a self, title: &'a str, artist: &'a str) -> BoxFuture<'a, Option<ResolvedTrack>>;
    fn fetch_song<'a>(&'a self, video_id: &'a str) -> BoxFuture<'a, Option<ResolvedTrack>>;
}

/// Local-playlist writes needed by the import.
pub trait PlaylistWriter: Send + Sync {
    fn create_custom(&self, title: &str) -> SavedPlaylist;
    fn add_track(&self, id: i64, track: StoredTrack) -> bool;
}

impl PlaylistWriter for PlaylistRepository {
    fn create_custom(&self, title: &str) -> SavedPlaylist {
        PlaylistRepository::create_custom(self, title)
    }

    fn add_track(&self, id: i64, track: StoredTrack) -> bool {
        PlaylistRepository::add_track(self, id, track)
    }
}

pub async fn preview_playlist_link_with(
    source: &dyn PlaylistTrackSource,
    http: &reqwest::Client,
    raw_link: &str,
    fetch_timeout: Duration,
) -> Result<PlaylistImportPreview, PlaylistImportError> {
    let Some(detected) = detect_playlist_link(raw_link) else {
        return Err(PlaylistImportError::InvalidFormat(
            "That does not look like a playlist link.".to_string(),
        ));
    };

    if detected == PlaylistLinkSource::Youtube {
        let Some(id) = extract_playlist_id(raw_link, PlaylistLinkSource::Youtube) else {
            return Err(PlaylistImportError::InvalidFormat(
                "That does not look like a playlist link.".to_string(),
            ));
        };
        if id.is_empty() {
            return Err(PlaylistImportError::InvalidFormat(
                "That does not look like a playlist link.".to_string(),
            ));
        }
        if id.starts_with("RD") {
            return Err(PlaylistImportError::InvalidFormat(
                "Mixes and radio cannot be imported. Paste a playlist link instead.".to_string(),
            ));
        }

        let fetched = tokio::time::timeout(fetch_timeout, source.fetch_playlist(raw_link))
            .await
            .map_err(|_| {
                PlaylistImportError::State(
                    "Could not load that YouTube playlist. Check the link is public and try again."
                        .to_string(),
                )
            })?;

        let Some(fetched) = fetched.filter(|p| !p.tracks.is_empty()) else {
            return Err(PlaylistImportError::State(
                "No playable tracks found in that playlist.".to_string(),
            ));
        };

        let title = if fetched.title.is_empty() {
            "YouTube Playlist".to_string()
        } else {
            fetched.title
        };
        let total_rows = fetched.tracks.len();
        let matched_tracks = fetched
            .tracks
            .into_iter()
            .map(|t| StoredTrack {
                name: t.title,
                artist: t.artist,
                artwork_url: t.artwork_url,
                video_id: t.video_id,
                album: t.album,
                duration_seconds: t.duration_seconds,
                ..Default::default()
            })
            .collect();

        return Ok(PlaylistImportPreview {
            title,
            total_rows,
            matched_tracks,
        });
    }

    let page = match detected {
        PlaylistLinkSource::Spotify => fetch_spotify_playlist(http, raw_link)
            .await
            .map_err(|e| PlaylistImportError::State(e.to_string()))?,
        PlaylistLinkSource::AppleMusic => fetch_apple_playlist(http, raw_link)
            .await
            .map_err(|e| PlaylistImportError::State(e.to_string()))?,
        PlaylistLinkSource::Youtube => unreachable!(),
    };

    if page.rows.is_empty() {
        return Err(PlaylistImportError::State(
            "No playable tracks found in that playlist.".to_string(),
        ));
    }

    let file_rows: Vec<PlaylistFileRow> = page
        .rows
        .iter()
        .map(|row| PlaylistFileRow {
            title: row.title.clone(),
            artist: row.artist.clone(),
            album: None,
            video_id: None,
        })
        .collect();

    let matched = match_rows(source, &file_rows).await;
    if matched.is_empty() {
        return Err(PlaylistImportError::State(
            "No playable tracks found in that playlist.".to_string(),
        ));
    }

    Ok(PlaylistImportPreview {
        title: page.title,
        total_rows: page.rows.len(),
        matched_tracks: matched,
    })
}

pub async fn preview_playlist_file_with(
    source: &dyn PlaylistTrackSource,
    bytes: &[u8],
    filename: &str,
) -> Result<PlaylistImportPreview, PlaylistImportError> {
    let mut seen = HashSet::new();
    let decoded = decode_playlist_bytes(bytes);
    let rows: Vec<PlaylistFileRow> = parse_playlist_file(&decoded, filename)
        .into_iter()
        .filter(|r| seen.insert(r.identity()))
        .collect();

    if rows.is_empty() {
        return Err(PlaylistImportError::InvalidFormat(
            "No tracks found in that file.".to_string(),
        ));
    }

    let mut ids = HashSet::new();
    let matched: Vec<StoredTrack> = match_rows(source, &rows)
        .await
        .into_iter()
        .filter(|t| ids.insert(t.video_id.clone()))
        .collect();

    if matched.is_empty() {
        return Err(PlaylistImportError::State(
            "No playable tracks found in that file.".to_string(),
        ));
    }

    Ok(PlaylistImportPreview {
        title: suggested_playlist_title(filename),
        total_rows: rows.len(),
        matched_tracks: matched,
    })
}

/// Bounded fan-out (5 at a time); misses and errors are dropped.
async fn match_rows(source: &dyn PlaylistTrackSource, rows: &[PlaylistFileRow]) -> Vec<StoredTrack> {
    let mut matched = Vec::new();
    for chunk in rows.chunks(5) {
        let futs = chunk.iter().map(|row| async move {
            if let Some(ref id) = row.video_id {
                if let Ok(Some(by_id)) =
                    tokio::time::timeout(Duration::from_secs(10), source.fetch_song(id)).await
                {
                    if by_id.video_id == *id {
                        return Some(by_id);
                    }
                }
            }
            if row.title.is_empty() {
                return None;
            }
            tokio::time::timeout(
                Duration::from_secs(10),
                source.find_match(&row.title, &row.artist),
            )
            .await
            .ok()
            .flatten()
        });

        let results = join_all(futs).await;
        for (row, maybe_match) in chunk.iter().zip(results.into_iter()) {
            let Some(m) = maybe_match else {
                continue;
            };
            matched.push(StoredTrack {
                name: if row.title.is_empty() {
                    m.title
                } else {
                    row.title.clone()
                },
                artist: if row.artist.is_empty() {
                    m.artist
                } else {
                    row.artist.clone()
                },
                artwork_url: m.artwork_url,
                video_id: m.video_id,
                album: row.album.clone().unwrap_or(m.album),
                duration_seconds: m.duration_seconds,
                ..Default::default()
            });
        }
    }
    matched
}

/// Testable create core: one playlist, exactly the matched tracks.
/// An empty preview is refused — imports never create trackless playlists.
pub fn import_preview_with(
    writer: &dyn PlaylistWriter,
    preview: &PlaylistImportPreview,
) -> Result<SavedPlaylist, PlaylistImportError> {
    if preview.matched_tracks.is_empty() {
        return Err(PlaylistImportError::State(
            "No playable tracks found in that playlist.".to_string(),
        ));
    }
    let created = writer.create_custom(&preview.title);
    for track in &preview.matched_tracks {
        writer.add_track(created.id, track.clone());
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockSource;
    impl PlaylistTrackSource for MockSource {
        fn fetch_playlist<'a>(
            &'a self,
            _id_or_url: &'a str,
        ) -> BoxFuture<'a, Option<ResolvedPlaylist>> {
            Box::pin(async {
                Some(ResolvedPlaylist {
                    title: "Mock YT".to_string(),
                    tracks: vec![ResolvedTrack {
                        title: "Track 1".to_string(),
                        artist: "Artist 1".to_string(),
                        artwork_url: String::new(),
                        video_id: "dQw4w9WgXcQ".to_string(),
                        album: "Album 1".to_string(),
                        duration_seconds: 180,
                    }],
                })
            })
        }

        fn find_match<'a>(
            &'a self,
            title: &'a str,
            artist: &'a str,
        ) -> BoxFuture<'a, Option<ResolvedTrack>> {
            let t = title.to_string();
            let a = artist.to_string();
            Box::pin(async move {
                Some(ResolvedTrack {
                    title: t,
                    artist: a,
                    artwork_url: String::new(),
                    video_id: "vid12345678".to_string(),
                    album: String::new(),
                    duration_seconds: 200,
                })
            })
        }

        fn fetch_song<'a>(&'a self, _video_id: &'a str) -> BoxFuture<'a, Option<ResolvedTrack>> {
            Box::pin(async { None })
        }
    }

    struct MockWriter {
        added: Mutex<Vec<StoredTrack>>,
    }
    impl PlaylistWriter for MockWriter {
        fn create_custom(&self, title: &str) -> SavedPlaylist {
            SavedPlaylist {
                id: 42,
                title: title.to_string(),
                subtitle: String::new(),
                mode: "custom".to_string(),
                tracks: Vec::new(),
                created_at_millis: 0,
                is_pinned: false,
                custom_cover_uri: String::new(),
            }
        }
        fn add_track(&self, _id: i64, track: StoredTrack) -> bool {
            self.added.lock().unwrap().push(track);
            true
        }
    }

    #[tokio::test]
    async fn preview_youtube_and_import_creates_playlist() {
        let preview = preview_playlist_link_with(
            &MockSource,
            &reqwest::Client::new(),
            "https://music.youtube.com/playlist?list=PL12345",
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(preview.title, "Mock YT");
        assert_eq!(preview.matched_tracks.len(), 1);

        let writer = MockWriter {
            added: Mutex::new(Vec::new()),
        };
        let saved = import_preview_with(&writer, &preview).unwrap();
        assert_eq!(saved.id, 42);
        assert_eq!(writer.added.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rejects_mix_links() {
        let err = preview_playlist_link_with(
            &MockSource,
            &reqwest::Client::new(),
            "https://music.youtube.com/playlist?list=RD12345",
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Mixes and radio"));
    }
}
