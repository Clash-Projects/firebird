//! Browse Categories genre track fetcher.
//!
//! Ported from `lib/features/feed/genre_browse.dart`.

use crate::extractor::innertube_api::InnerTubeMusicApi;

use super::feed_repository::GeneratedTrack;

pub async fn fetch_genre_tracks(tube: &InnerTubeMusicApi, query: &str) -> Vec<GeneratedTrack> {
    let Ok(found) = tube.search_songs(&format!("{query} songs"), 40).await else {
        return Vec::new();
    };
    found
        .into_iter()
        .filter(|t| !t.video_id.is_empty())
        .map(|t| GeneratedTrack {
            name: t.title,
            artist: t.artist,
            album: t.album,
            artwork_url: t.artwork_url,
            video_id: t.video_id,
            duration_seconds: t.duration_seconds,
            ..Default::default()
        })
        .collect()
}
