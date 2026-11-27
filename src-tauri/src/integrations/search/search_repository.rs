//! Unified search: YouTube Music for content tabs + local search history.
//!
//! Ported from `lib/features/search/search_repository.dart`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::extractor::innertube_api::{
    InnerTubeMusicApi, YouTubeMusicEntity, YouTubeMusicTrack,
};
use crate::storage::app_database::AppDatabase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchTab {
    Tracks,
    Artists,
    Albums,
    Playlists,
}

impl SearchTab {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tracks => "tracks",
            Self::Artists => "artists",
            Self::Albums => "albums",
            Self::Playlists => "playlists",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultItem {
    pub name: String,
    pub artist: String,
    pub artwork_url: String,
    pub subtitle: String,
    pub video_id: String,
    pub entity_id: String,
    pub tab: SearchTab,
    pub duration_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSuggestion {
    pub text: String,
    pub tab: SearchTab,
    pub subtitle: String,
    pub artwork_url: String,
    pub artist: String,
}

static SUGGESTION_JUNK_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"\b(edit|slowed|reverb|nightcore|tiktok|gameplay|walkthrough|trailer|metroid|minecraft|fortnite|tv live)\b",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
});
static WORD_SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s\-_/&,+.]+").unwrap());
static MULTI_WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static PAREN_FEAT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s*[\(\[]\s*(feat\.?|ft\.?|featuring)\b[^\)\]]*[\)\]]").unwrap()
});
static TRAIL_FEAT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+(feat\.?|ft\.?|featuring|with)\s+.+$").unwrap());

pub fn suggestion_name_matches_query(name: &str, query: &str) -> bool {
    let n = name.to_lowercase();
    let n = n.trim();
    let q = query.to_lowercase();
    let q = q.trim();
    if q.len() < 2 || n.is_empty() {
        return false;
    }
    if n == q || n.starts_with(q) {
        return true;
    }
    WORD_SPLIT_RE.split(n).any(|word| word.starts_with(q))
}

pub fn suggestion_looks_like_junk(title: &str) -> bool {
    SUGGESTION_JUNK_RE.is_match(title)
}

#[derive(Clone)]
enum MemoValue {
    Suggestions(Vec<SearchSuggestion>),
    Results(Vec<SearchResultItem>),
    Preview(Option<SearchResultItem>),
}

struct MemoEntry {
    at: Instant,
    value: MemoValue,
}

const MEMO_TTL: Duration = Duration::from_secs(10 * 60);
const MEMO_CAP: usize = 80;

#[derive(Default)]
struct MemoCache {
    map: HashMap<String, MemoEntry>,
    order: Vec<String>,
}

impl MemoCache {
    fn get(&mut self, key: &str) -> Option<MemoValue> {
        let entry = self.map.get(key)?;
        if entry.at.elapsed() >= MEMO_TTL {
            self.map.remove(key);
            self.order.retain(|k| k != key);
            return None;
        }
        let val = entry.value.clone();
        self.order.retain(|k| k != key);
        self.order.push(key.to_string());
        Some(val)
    }

    fn insert(&mut self, key: String, value: MemoValue) {
        self.order.retain(|k| k != &key);
        self.map.insert(
            key.clone(),
            MemoEntry {
                at: Instant::now(),
                value,
            },
        );
        self.order.push(key);
        if self.order.len() > MEMO_CAP {
            let oldest = self.order.remove(0);
            self.map.remove(&oldest);
        }
    }
}

pub struct SearchRepository {
    tube: Arc<InnerTubeMusicApi>,
    db: Arc<AppDatabase>,
    memo: Mutex<MemoCache>,
}

impl SearchRepository {
    pub fn new(tube: Arc<InnerTubeMusicApi>, db: Arc<AppDatabase>) -> Self {
        Self {
            tube,
            db,
            memo: Mutex::new(MemoCache::default()),
        }
    }

    pub async fn get_suggestions(&self, query: &str) -> Vec<SearchSuggestion> {
        let q = query.trim();
        if q.len() < 2 {
            return Vec::new();
        }
        let key = format!("sug|{}", q.to_lowercase());
        if let Some(MemoValue::Suggestions(cached)) = self.memo.lock().await.get(&key) {
            return cached;
        }
        let res = self.fetch_suggestions(q).await;
        if !res.is_empty() {
            self.memo
                .lock()
                .await
                .insert(key, MemoValue::Suggestions(res.clone()));
        }
        res
    }

    async fn fetch_suggestions(&self, q: &str) -> Vec<SearchSuggestion> {
        let (artists_res, songs_res) =
            tokio::join!(self.tube.search_artists(q, 5), self.tube.search_songs(q, 8));
        let artists = artists_res.unwrap_or_default();
        let songs = songs_res.unwrap_or_default();

        let mut out = Vec::new();
        let mut seen = HashSet::new();

        let mut artist_count = 0usize;
        for a in &artists {
            if !suggestion_name_matches_query(&a.name, q) || suggestion_looks_like_junk(&a.name) {
                continue;
            }
            let key = format!("artists:{}", a.name.to_lowercase());
            if seen.insert(key) {
                out.push(SearchSuggestion {
                    text: a.name.clone(),
                    tab: SearchTab::Artists,
                    subtitle: "Artist".to_string(),
                    artwork_url: a.artwork_url.clone(),
                    artist: a.name.clone(),
                });
                artist_count += 1;
                if artist_count >= 3 {
                    break;
                }
            }
        }

        let artist_keys: Vec<String> = out
            .iter()
            .filter(|s| s.tab == SearchTab::Artists)
            .map(|s| s.text.to_lowercase())
            .collect();
        let require_known_artist = !q.contains(' ') && !artist_keys.is_empty();

        let mut song_count = 0usize;
        for s in &songs {
            if !suggestion_name_matches_query(&s.title, q) || suggestion_looks_like_junk(&s.title) {
                continue;
            }
            if require_known_artist {
                let a = s.artist.to_lowercase();
                let known = artist_keys
                    .iter()
                    .any(|n| a == *n || a.starts_with(n) || n.starts_with(&a) || a.contains(n));
                if !known {
                    continue;
                }
            }
            let key = format!("tracks:{}", s.title.to_lowercase());
            if seen.insert(key) {
                out.push(SearchSuggestion {
                    text: s.title.clone(),
                    tab: SearchTab::Tracks,
                    subtitle: if s.artist.is_empty() {
                        "Song".to_string()
                    } else {
                        s.artist.clone()
                    },
                    artwork_url: s.artwork_url.clone(),
                    artist: s.artist.clone(),
                });
                song_count += 1;
                if song_count >= 4 {
                    break;
                }
            }
        }
        out
    }

    pub async fn search(&self, tab: SearchTab, query: &str) -> Vec<SearchResultItem> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let key = format!("{}|{}", tab.as_str(), q.to_lowercase());
        if let Some(MemoValue::Results(cached)) = self.memo.lock().await.get(&key) {
            return cached;
        }
        let res = self.fetch_search(tab, q).await;
        if !res.is_empty() {
            self.memo
                .lock()
                .await
                .insert(key, MemoValue::Results(res.clone()));
        }
        res
    }

    async fn fetch_search(&self, tab: SearchTab, q: &str) -> Vec<SearchResultItem> {
        match tab {
            SearchTab::Tracks => self
                .tube
                .search_songs(q, 60)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|t| SearchResultItem {
                    name: t.title,
                    artist: t.artist,
                    artwork_url: t.artwork_url,
                    subtitle: t.album,
                    video_id: t.video_id.clone(),
                    entity_id: t.video_id,
                    tab,
                    duration_seconds: t.duration_seconds,
                })
                .collect(),
            SearchTab::Artists => self
                .tube
                .search_artists(q, 20)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|e| SearchResultItem {
                    name: e.name,
                    artist: String::new(),
                    artwork_url: e.artwork_url,
                    subtitle: e.subtitle,
                    video_id: String::new(),
                    entity_id: e.browse_id,
                    tab,
                    duration_seconds: 0,
                })
                .collect(),
            SearchTab::Albums => self
                .tube
                .search_albums(q, 20)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|e| SearchResultItem {
                    name: e.name,
                    artist: e.artist,
                    artwork_url: e.artwork_url,
                    subtitle: e.subtitle,
                    video_id: String::new(),
                    entity_id: e.browse_id,
                    tab,
                    duration_seconds: 0,
                })
                .collect(),
            SearchTab::Playlists => self
                .tube
                .search_playlists(q, 30)
                .await
                .into_iter()
                .map(|e| {
                    let mut parts = Vec::new();
                    if !e.author.is_empty() {
                        parts.push(e.author.clone());
                    }
                    if !e.track_count_text.is_empty() {
                        parts.push(e.track_count_text);
                    }
                    SearchResultItem {
                        name: e.title,
                        artist: e.author,
                        artwork_url: e.artwork_url,
                        subtitle: parts.join(" • "),
                        video_id: String::new(),
                        entity_id: e.id,
                        tab,
                        duration_seconds: 0,
                    }
                })
                .collect(),
        }
    }

    pub async fn songs_for(&self, item: &SearchResultItem) -> Vec<YouTubeMusicTrack> {
        if item.entity_id.is_empty() {
            return Vec::new();
        }
        if item.tab == SearchTab::Tracks && !item.video_id.is_empty() {
            return self
                .tube
                .fetch_song_details(&item.video_id)
                .await
                .into_iter()
                .collect();
        }
        self.tube
            .browse_songs(&item.entity_id, None, Some(50))
            .await
            .unwrap_or_default()
    }

    pub async fn preview_for_query(&self, query: &str) -> Option<SearchResultItem> {
        let q = query.trim();
        if q.is_empty() {
            return None;
        }
        let key = format!("prev|{}", q.to_lowercase());
        if let Some(MemoValue::Preview(cached)) = self.memo.lock().await.get(&key) {
            return cached;
        }
        let res = self.fetch_preview_for_query(q).await;
        if res.is_some() {
            self.memo
                .lock()
                .await
                .insert(key, MemoValue::Preview(res.clone()));
        }
        res
    }

    async fn fetch_preview_for_query(&self, q: &str) -> Option<SearchResultItem> {
        let (artists_res, songs_res) =
            tokio::join!(self.tube.search_artists(q, 3), self.tube.search_songs(q, 3));
        let artists = artists_res.unwrap_or_default();
        let songs = songs_res.unwrap_or_default();

        let artist_hit: Option<&YouTubeMusicEntity> = artists
            .iter()
            .find(|a| suggestion_name_matches_query(&a.name, q))
            .or_else(|| artists.first());
        let song_hit: Option<&YouTubeMusicTrack> = songs
            .iter()
            .find(|s| suggestion_name_matches_query(&s.title, q))
            .or_else(|| songs.first());

        if let Some(a) = artist_hit {
            if suggestion_name_matches_query(&a.name, q) {
                return Some(SearchResultItem {
                    name: a.name.clone(),
                    artist: a.name.clone(),
                    artwork_url: a.artwork_url.clone(),
                    subtitle: a.subtitle.clone(),
                    video_id: String::new(),
                    entity_id: a.browse_id.clone(),
                    tab: SearchTab::Artists,
                    duration_seconds: 0,
                });
            }
        }
        if let Some(s) = song_hit {
            return Some(SearchResultItem {
                name: s.title.clone(),
                artist: s.artist.clone(),
                artwork_url: s.artwork_url.clone(),
                subtitle: s.album.clone(),
                video_id: s.video_id.clone(),
                entity_id: s.video_id.clone(),
                tab: SearchTab::Tracks,
                duration_seconds: s.duration_seconds,
            });
        }
        artist_hit.map(|a| SearchResultItem {
            name: a.name.clone(),
            artist: a.name.clone(),
            artwork_url: a.artwork_url.clone(),
            subtitle: a.subtitle.clone(),
            video_id: String::new(),
            entity_id: a.browse_id.clone(),
            tab: SearchTab::Artists,
            duration_seconds: 0,
        })
    }

    pub fn history(&self) -> Vec<String> {
        self.db.load_search_history(12)
    }

    pub fn push_history(&self, q: &str) {
        let trimmed = q.trim();
        if !trimmed.is_empty() {
            self.db.push_search_history(trimmed);
        }
    }

    pub fn clear_history(&self) {
        self.db.clear_search_history();
    }

    pub fn remove_history(&self, q: &str) {
        self.db.remove_search_history(q);
    }
}

fn search_artist_key(artist: &str) -> String {
    let mut k = MULTI_WS_RE
        .replace_all(&artist.to_lowercase(), " ")
        .trim()
        .to_string();
    if matches!(
        k.as_str(),
        "" | "unknown artist" | "various artists" | "unknown"
    ) {
        return String::new();
    }
    k = PAREN_FEAT_RE.replace_all(&k, "").trim().to_string();
    k = TRAIL_FEAT_RE.replace_all(&k, "").trim().to_string();
    k
}

pub fn rank_search_albums_by_taste(
    albums: Vec<SearchResultItem>,
    affinities: &HashMap<String, f64>,
) -> Vec<SearchResultItem> {
    if albums.is_empty() || affinities.is_empty() {
        return albums;
    }
    let mut indexed: Vec<(usize, SearchResultItem)> = albums.into_iter().enumerate().collect();
    indexed.sort_by(|x, y| {
        let ax = if x.1.artist.is_empty() {
            &x.1.subtitle
        } else {
            &x.1.artist
        };
        let ay = if y.1.artist.is_empty() {
            &y.1.subtitle
        } else {
            &y.1.artist
        };
        let fa = affinities.get(&search_artist_key(ax)).copied().unwrap_or(-1.0);
        let fb = affinities.get(&search_artist_key(ay)).copied().unwrap_or(-1.0);
        fb.partial_cmp(&fa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| x.0.cmp(&y.0))
    });
    indexed.into_iter().map(|(_, item)| item).collect()
}

pub fn rank_search_playlists_by_taste(
    playlists: Vec<SearchResultItem>,
    affinities: &HashMap<String, f64>,
) -> Vec<SearchResultItem> {
    if playlists.is_empty() || affinities.is_empty() {
        return playlists;
    }
    let score_of = |p: &SearchResultItem| -> f64 {
        let mut score = affinities
            .get(&search_artist_key(&p.artist))
            .copied()
            .unwrap_or(0.0)
            * 2.0;
        let title = p.name.to_lowercase();
        if !title.is_empty() {
            for (key, &val) in affinities {
                if key.len() < 3 {
                    continue;
                }
                if title.contains(key.as_str()) {
                    score += val * 0.5;
                }
            }
        }
        score
    };

    let mut scored: Vec<(usize, bool, f64, SearchResultItem)> = playlists
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            let s = score_of(&item);
            (i, s > 0.0, s, item)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.0.cmp(&b.0))
    });
    scored.into_iter().map(|(_, _, _, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestion_matching_and_junk_filter() {
        assert!(suggestion_name_matches_query("Metro Boomin", "metro"));
        assert!(!suggestion_name_matches_query("Random", "m"));
        assert!(suggestion_looks_like_junk("Song (slowed + reverb)"));
        assert!(!suggestion_looks_like_junk("After Hours"));
    }

    #[test]
    fn rank_albums_and_playlists_by_taste() {
        let mut aff = HashMap::new();
        aff.insert("the weeknd".to_string(), 10.0);
        let albums = vec![
            SearchResultItem {
                name: "Other".to_string(),
                artist: "Someone".to_string(),
                artwork_url: String::new(),
                subtitle: String::new(),
                video_id: String::new(),
                entity_id: "1".to_string(),
                tab: SearchTab::Albums,
                duration_seconds: 0,
            },
            SearchResultItem {
                name: "Dawn FM".to_string(),
                artist: "The Weeknd".to_string(),
                artwork_url: String::new(),
                subtitle: String::new(),
                video_id: String::new(),
                entity_id: "2".to_string(),
                tab: SearchTab::Albums,
                duration_seconds: 0,
            },
        ];
        let ranked = rank_search_albums_by_taste(albums, &aff);
        assert_eq!(ranked[0].name, "Dawn FM");
    }
}
