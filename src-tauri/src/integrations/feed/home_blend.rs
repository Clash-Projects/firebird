//! The connected Home: YouTube's own shelves re-ordered by Last.fm taste.
//!
//! Ported from `lib/features/feed/home_blend.dart`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::extractor::innertube_api::{YouTubeMusicTrack, YtHomeShelf};
use crate::models::artist_parser::split_artist_credits;
use crate::storage::app_database::AppDatabase;

use super::feed_repository::{normalize_artist_key, FeedData, GeneratedTrack};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum YtShelfTier {
    QuickPicks,
    ListenAgain,
    Personal,
    General,
    Unknown,
    Junk,
}

pub fn classify_yt_shelf(title: &str) -> YtShelfTier {
    let t = title.to_lowercase();
    let t = t.trim();
    let any = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if any(&[
        "video",
        "short",
        "podcast",
        "episode",
        "live performance",
        "long listening",
        "cover",
        "remix",
        "community",
        "mood",
        "genre",
        "audiobook",
        "sing along",
    ]) {
        return YtShelfTier::Junk;
    }
    if t.contains("quick pick") {
        return YtShelfTier::QuickPicks;
    }
    if t.contains("listen again") {
        return YtShelfTier::ListenAgain;
    }
    if any(&[
        "for you",
        "your ",
        "mix",
        "recommended",
        "similar to",
        "more from",
        "because",
        "favorite",
        "favourite",
        "keep listening",
        "recap",
        "discover",
    ]) {
        return YtShelfTier::Personal;
    }
    if any(&["new release", "chart", "trending", "top ", "hits", "album"]) {
        return YtShelfTier::General;
    }
    YtShelfTier::Unknown
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeBlend {
    pub quick_picks: Vec<GeneratedTrack>,
    pub listen_again: Vec<GeneratedTrack>,
    pub personal: Vec<YtHomeShelf>,
    pub general: Vec<YtHomeShelf>,
    pub artists: Vec<GeneratedTrack>,
    pub has_you_tube: bool,
}

fn off_topic(title: &str) -> bool {
    let t = title.to_lowercase();
    ["podcast", "episode", "short", "audiobook"]
        .iter()
        .any(|w| t.contains(w))
}

pub fn blend_home_feed(
    shelves: &[YtHomeShelf],
    feed: &FeedData,
    excluded: &HashSet<String>,
    max_shelves: usize,
    min_items: usize,
    quick_pick_count: usize,
    listen_again_count: usize,
) -> HomeBlend {
    let affinities = &feed.taste_affinities;
    let max_affinity = affinities.values().copied().fold(0.0_f64, f64::max);

    let taste = |artist: &str| -> f64 {
        if max_affinity <= 0.0 || artist.is_empty() {
            return 0.0;
        }
        let mut best = 0.0_f64;
        let credits = split_artist_credits(artist);
        let pool: Vec<&str> = if credits.is_empty() {
            vec![artist]
        } else {
            credits.iter().map(String::as_str).collect()
        };
        for name in pool {
            let v = affinities
                .get(&normalize_artist_key(name))
                .copied()
                .unwrap_or(0.0);
            if v > best {
                best = v;
            }
        }
        best / max_affinity
    };

    let allowed = |t: &YouTubeMusicTrack| {
        !excluded.contains(&AppDatabase::exclusion_key(&t.title, &t.artist))
    };

    let mut yt_quick = Vec::new();
    let mut yt_again = Vec::new();
    let mut feed_shelves = Vec::new();
    let mut titles = HashSet::new();

    for raw in shelves {
        if raw.title.is_empty() || !titles.insert(raw.title.to_lowercase()) {
            continue;
        }
        if off_topic(&raw.title) {
            continue;
        }
        let shelf = YtHomeShelf {
            title: raw.title.clone(),
            subtitle: raw.subtitle.clone(),
            tracks: raw.tracks.iter().filter(|t| allowed(t)).cloned().collect(),
            track_cards: raw
                .track_cards
                .iter()
                .filter(|t| allowed(t))
                .cloned()
                .collect(),
            entities: raw.entities.clone(),
        };
        match classify_yt_shelf(&raw.title) {
            YtShelfTier::QuickPicks => {
                yt_quick.extend(shelf.tracks);
                yt_quick.extend(shelf.track_cards);
            }
            YtShelfTier::ListenAgain => {
                yt_again.extend(shelf.track_cards);
                yt_again.extend(shelf.tracks);
            }
            _ => {
                if shelf.tracks.len() + shelf.track_cards.len() + shelf.entities.len() >= min_items
                {
                    feed_shelves.push(shelf);
                }
            }
        }
    }

    let from_yt = |t: &YouTubeMusicTrack| GeneratedTrack {
        name: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        artwork_url: t.artwork_url.clone(),
        video_id: t.video_id.clone(),
        duration_seconds: t.duration_seconds,
        ..Default::default()
    };

    let taste_picks: Vec<GeneratedTrack> = feed
        .heavy_rotation
        .iter()
        .chain(feed.quick_picks.iter())
        .filter(|t| !t.artwork_url.is_empty() && taste(&t.artist) > 0.0)
        .cloned()
        .collect();

    let quick = interleave(
        &yt_quick.iter().map(from_yt).collect::<Vec<_>>(),
        if yt_quick.is_empty() { &taste_picks } else { &[] },
        quick_pick_count,
        3,
    );

    let jump_back: Vec<GeneratedTrack> = feed
        .jump_back_in
        .iter()
        .filter(|t| !t.artwork_url.is_empty())
        .cloned()
        .collect();
    let again = interleave(
        &yt_again.iter().map(from_yt).collect::<Vec<_>>(),
        if yt_again.is_empty() { &jump_back } else { &[] },
        listen_again_count,
        4,
    );

    let mut artists = Vec::new();
    let mut seen = HashSet::new();
    for t in feed
        .heavy_rotation
        .iter()
        .chain(feed.jump_back_in.iter())
        .chain(taste_picks.iter())
    {
        if artists.len() >= 12 {
            break;
        }
        for name in split_artist_credits(&t.artist) {
            if artists.len() >= 12 {
                break;
            }
            if taste(&name) <= 0.0 || !seen.insert(name.to_lowercase()) {
                continue;
            }
            artists.push(GeneratedTrack {
                name: name.clone(),
                artist: name,
                video_id: t.video_id.clone(),
                ..Default::default()
            });
        }
    }

    let has_yt = !yt_quick.is_empty() || !yt_again.is_empty() || !feed_shelves.is_empty();
    feed_shelves.truncate(max_shelves);

    HomeBlend {
        quick_picks: quick,
        listen_again: again,
        personal: feed_shelves,
        general: Vec::new(),
        artists,
        has_you_tube: has_yt,
    }
}

fn try_take(
    t: &GeneratedTrack,
    out: &mut Vec<GeneratedTrack>,
    seen: &mut HashSet<String>,
    per_artist: &mut HashMap<String, usize>,
    max_per_artist: usize,
) -> bool {
    let key = format!("{}|{}", t.name.to_lowercase(), t.artist.to_lowercase());
    let artist = normalize_artist_key(&t.artist);
    if seen.contains(&key)
        || (!t.video_id.is_empty() && seen.contains(&t.video_id))
        || per_artist.get(&artist).copied().unwrap_or(0) >= max_per_artist
    {
        return false;
    }
    seen.insert(key);
    if !t.video_id.is_empty() {
        seen.insert(t.video_id.clone());
    }
    *per_artist.entry(artist).or_insert(0) += 1;
    out.push(t.clone());
    true
}

fn interleave(
    primary: &[GeneratedTrack],
    secondary: &[GeneratedTrack],
    limit: usize,
    max_per_artist: usize,
) -> Vec<GeneratedTrack> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut per_artist: HashMap<String, usize> = HashMap::new();

    let (mut p, mut s) = (0usize, 0usize);
    while out.len() < limit && (p < primary.len() || s < secondary.len()) {
        while p < primary.len() {
            let item = &primary[p];
            p += 1;
            if try_take(item, &mut out, &mut seen, &mut per_artist, max_per_artist) {
                break;
            }
        }
        if out.len() >= limit {
            break;
        }
        while p < primary.len() {
            let item = &primary[p];
            p += 1;
            if try_take(item, &mut out, &mut seen, &mut per_artist, max_per_artist) {
                break;
            }
        }
        if out.len() >= limit {
            break;
        }
        while s < secondary.len() {
            let item = &secondary[s];
            s += 1;
            if try_take(item, &mut out, &mut seen, &mut per_artist, max_per_artist) {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_shelves_with_junk_priority() {
        assert_eq!(
            classify_yt_shelf("Recommended music videos"),
            YtShelfTier::Junk
        );
        assert_eq!(classify_yt_shelf("Quick picks"), YtShelfTier::QuickPicks);
        assert_eq!(classify_yt_shelf("Listen again"), YtShelfTier::ListenAgain);
        assert_eq!(classify_yt_shelf("Mixed for you"), YtShelfTier::Personal);
    }
}
