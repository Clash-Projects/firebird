//! On-device taste snapshot for keyless guests (no Last.fm account).
//!
//! Ported from `lib/features/feed/local_taste.dart`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::models::lastfm_home_models::HomeTrack;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTaste {
    /// Newest-first play log rows (cap ~100).
    pub recent_plays: Vec<HomeTrack>,
    /// Local Liked Songs playlist contents.
    pub liked_tracks: Vec<HomeTrack>,
    /// Downloaded tracks (owned collection).
    pub downloaded_tracks: Vec<HomeTrack>,
}

impl LocalTaste {
    pub fn is_empty(&self) -> bool {
        self.recent_plays.is_empty()
            && self.liked_tracks.is_empty()
            && self.downloaded_tracks.is_empty()
    }

    /// Seed pool for discovery/personal-mix: recents first (current
    /// taste), then liked, then downloads — deduped, artists capped
    /// downstream by the diversifier.
    pub fn seed_pool(&self, limit: usize) -> Vec<HomeTrack> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for t in self
            .recent_plays
            .iter()
            .chain(self.liked_tracks.iter())
            .chain(self.downloaded_tracks.iter())
        {
            if t.name.is_empty() || !seen.insert(t.key()) {
                continue;
            }
            out.push(t.clone());
            if out.len() >= limit {
                break;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_pool_prioritizes_recents_and_dedupes() {
        let t1 = HomeTrack {
            name: "Song A".to_string(),
            artist: "Artist 1".to_string(),
            ..Default::default()
        };
        let t2 = HomeTrack {
            name: "Song B".to_string(),
            artist: "Artist 2".to_string(),
            ..Default::default()
        };
        let taste = LocalTaste {
            recent_plays: vec![t1.clone()],
            liked_tracks: vec![t1, t2],
            downloaded_tracks: vec![],
        };
        let pool = taste.seed_pool(12);
        assert_eq!(pool.len(), 2);
        assert_eq!(pool[0].name, "Song A");
        assert_eq!(pool[1].name, "Song B");
    }
}
