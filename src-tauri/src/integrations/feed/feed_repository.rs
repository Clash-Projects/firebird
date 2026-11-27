//! Generation + feed repository.
//!
//! Ported from `lib/features/feed/feed_repository.dart`:
//! - affinity-weighted blending of Last.fm signals + YTM radio/charts + local taste
//! - per-artist caps, daily-seeded rotation jitter, cross-source deduplication
//! - artwork hydration (OfficialArtworkService -> strict YTM -> loose YTM)

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use chrono::{Datelike, Local, TimeZone};
use futures_util::future::join_all;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::extractor::innertube_api::{
    InnerTubeMusicApi, YouTubeMusicEntity, YouTubeMusicTrack, NEW_RELEASES_BROWSE_ID,
};
use crate::integrations::artwork::official_artwork_service::OfficialArtworkService;
use crate::integrations::lastfm::home_repository::HomeRepository;
use crate::integrations::lastfm::lastfm_api::LastFmApiService;
use crate::models::lastfm_home_models::HomeTrack;
use crate::storage::app_database::AppDatabase;

use super::genre_source::is_music_genre_tag;
use super::local_taste::LocalTaste;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedTrack {
    pub name: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub video_id: String,
    #[serde(default)]
    pub listeners: String,
    #[serde(default)]
    pub match_score: String,
    #[serde(default)]
    pub duration_seconds: i64,
    #[serde(default)]
    pub playback_url: String,
}

impl GeneratedTrack {
    pub fn key(&self) -> String {
        format!("{}|{}", self.name.to_lowercase(), self.artist.to_lowercase())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeedEmptyReason {
    #[default]
    None,
    NoTaste,
    LastfmError,
    ChartsError,
    AllFailed,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedData {
    pub quick_picks: Vec<GeneratedTrack>,
    pub heavy_rotation: Vec<GeneratedTrack>,
    pub fresh_finds: Vec<GeneratedTrack>,
    pub jump_back_in: Vec<GeneratedTrack>,
    pub because_you_listened: Vec<GeneratedTrack>,
    pub charts: Vec<GeneratedTrack>,
    pub taste_tags: Vec<String>,
    pub because_seed: String,
    pub taste_affinities: HashMap<String, f64>,
    pub last_fm_failed: bool,
    pub charts_failed: bool,
    pub empty_reason: FeedEmptyReason,
}

impl FeedData {
    pub fn is_empty(&self) -> bool {
        self.quick_picks.is_empty()
            && self.heavy_rotation.is_empty()
            && self.fresh_finds.is_empty()
            && self.jump_back_in.is_empty()
            && self.because_you_listened.is_empty()
            && self.charts.is_empty()
    }
}

pub fn resolve_empty_reason(online: bool, data: &FeedData) -> FeedEmptyReason {
    if !data.is_empty() {
        return FeedEmptyReason::None;
    }
    if !online {
        return FeedEmptyReason::Offline;
    }
    data.empty_reason
}

pub fn feed_day_seed(now: chrono::DateTime<Local>) -> u64 {
    let midnight = Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .map(|d| d.timestamp_millis())
        .unwrap_or(0);
    midnight as u64
}

static MULTI_WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static TOPIC_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*-\s*topic$").unwrap());
static PAREN_FEAT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s*[\(\[]\s*(feat\.?|ft\.?|featuring|with|prod\.?|produced by)\b[^\)\]]*[\)\]]")
        .unwrap()
});
static TRAIL_FEAT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s+(feat\.?|ft\.?|featuring|with|prod\.?|produced by)\s+.+$").unwrap()
});
static COLLAB_SPLIT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+([x&]|vs\.?)\s+.+$").unwrap());
static NON_ALNUM_ASCII_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9]").unwrap());

/// Canonical artist key for taste math.
pub fn normalize_artist_key(artist: &str) -> String {
    let mut k = MULTI_WS_RE
        .replace_all(&artist.to_lowercase(), " ")
        .trim()
        .to_string();
    if matches!(
        k.as_str(),
        "" | "unknown artist" | "various artists" | "various" | "unknown" | "null" | "n/a"
    ) {
        return String::new();
    }
    k = TOPIC_SUFFIX_RE.replace_all(&k, "").trim().to_string();
    k = PAREN_FEAT_RE.replace_all(&k, "").trim().to_string();
    k = TRAIL_FEAT_RE.replace_all(&k, "").trim().to_string();
    k = COLLAB_SPLIT_RE.replace_all(&k, "").trim().to_string();
    k
}

pub fn apply_exclusions(
    tracks: Vec<GeneratedTrack>,
    excluded: &HashSet<String>,
) -> Vec<GeneratedTrack> {
    if excluded.is_empty() {
        return tracks;
    }
    tracks
        .into_iter()
        .filter(|t| !excluded.contains(&t.key()))
        .collect()
}

pub fn rank_new_releases(
    albums: Vec<YouTubeMusicEntity>,
    affinities: &HashMap<String, f64>,
) -> Vec<YouTubeMusicEntity> {
    let mut indexed: Vec<(usize, YouTubeMusicEntity)> = albums.into_iter().enumerate().collect();
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
        let fa = affinities
            .get(&normalize_artist_key(ax))
            .copied()
            .unwrap_or(-1.0);
        let fb = affinities
            .get(&normalize_artist_key(ay))
            .copied()
            .unwrap_or(-1.0);
        fb.partial_cmp(&fa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| x.0.cmp(&y.0))
    });
    indexed.into_iter().map(|(_, e)| e).collect()
}

pub fn diversify_feed_tracks(
    mut scored: Vec<(GeneratedTrack, f64)>,
    limit: usize,
    max_per_artist: usize,
    shared_counts: Option<&mut HashMap<String, usize>>,
) -> Vec<GeneratedTrack> {
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut local_counts = HashMap::new();
    let counts = shared_counts.unwrap_or(&mut local_counts);
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for (track, _) in scored {
        if out.len() >= limit {
            break;
        }
        if !seen.insert(track.key()) {
            continue;
        }
        let artist = normalize_artist_key(&track.artist);
        if artist.is_empty() {
            continue;
        }
        let c = counts.get(&artist).copied().unwrap_or(0);
        if c >= max_per_artist {
            continue;
        }
        counts.insert(artist, c + 1);
        out.push(track);
    }
    out
}

pub fn dedupe_feed_heads(
    list: Vec<GeneratedTrack>,
    headlined: &mut HashSet<String>,
    head_n: usize,
) -> Vec<GeneratedTrack> {
    let out: Vec<GeneratedTrack> = list
        .into_iter()
        .filter(|t| !headlined.contains(&t.key()))
        .collect();
    for t in out.iter().take(head_n) {
        headlined.insert(t.key());
    }
    out
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub struct FeedHooks {
    pub fetch_yt_liked: Option<Arc<dyn Fn() -> BoxFuture<'static, Vec<YouTubeMusicTrack>> + Send + Sync>>,
    pub fetch_yt_history: Option<Arc<dyn Fn() -> BoxFuture<'static, Vec<YouTubeMusicTrack>> + Send + Sync>>,
    pub fetch_lfm_recent: Option<Arc<dyn Fn(usize) -> BoxFuture<'static, Result<Vec<HomeTrack>, ()>> + Send + Sync>>,
    pub fetch_lfm_top: Option<Arc<dyn Fn(String, usize) -> BoxFuture<'static, Result<Vec<HomeTrack>, ()>> + Send + Sync>>,
    pub fetch_charts: Option<Arc<dyn Fn(usize) -> BoxFuture<'static, Result<Vec<YouTubeMusicTrack>, ()>> + Send + Sync>>,
    pub fetch_similar: Option<Arc<dyn Fn(String, String, usize) -> BoxFuture<'static, Vec<GeneratedTrack>> + Send + Sync>>,
    pub fetch_local_taste: Option<Arc<dyn Fn() -> BoxFuture<'static, Option<LocalTaste>> + Send + Sync>>,
}

impl Default for FeedHooks {
    fn default() -> Self {
        Self {
            fetch_yt_liked: None,
            fetch_yt_history: None,
            fetch_lfm_recent: None,
            fetch_lfm_top: None,
            fetch_charts: None,
            fetch_similar: None,
            fetch_local_taste: None,
        }
    }
}

pub struct FeedRepository {
    api: Arc<LastFmApiService>,
    tube: Arc<InnerTubeMusicApi>,
    home: Arc<HomeRepository>,
    api_key: Arc<dyn Fn() -> String + Send + Sync>,
    db: Option<Arc<AppDatabase>>,
    official: Option<Arc<OfficialArtworkService>>,
    hooks: FeedHooks,
}

impl FeedRepository {
    pub fn new(
        api: Arc<LastFmApiService>,
        tube: Arc<InnerTubeMusicApi>,
        home: Arc<HomeRepository>,
        api_key: Arc<dyn Fn() -> String + Send + Sync>,
        db: Option<Arc<AppDatabase>>,
        official: Option<Arc<OfficialArtworkService>>,
        hooks: FeedHooks,
    ) -> Self {
        Self {
            api,
            tube,
            home,
            api_key,
            db,
            official,
            hooks,
        }
    }

    fn add_affinity(affinities: &mut HashMap<String, f64>, artist: &str, weight: f64) {
        let k = normalize_artist_key(artist);
        if k.is_empty() {
            return;
        }
        *affinities.entry(k).or_insert(0.0) += weight;
    }

    #[allow(clippy::too_many_arguments)]
    fn artist_affinities(
        top: &[HomeTrack],
        recent: &[HomeTrack],
        long_term: &[HomeTrack],
        yt_liked: &[YouTubeMusicTrack],
        yt_history: &[YouTubeMusicTrack],
        local_liked: &[HomeTrack],
        local_plays: &[HomeTrack],
        local_downloads: &[HomeTrack],
        lfm_active: bool,
    ) -> HashMap<String, f64> {
        let mut affinities = HashMap::new();
        for (i, t) in top.iter().enumerate() {
            Self::add_affinity(&mut affinities, &t.artist, 1.45 / (1.0 + i as f64 / 9.0));
        }
        for (i, t) in recent.iter().enumerate() {
            Self::add_affinity(&mut affinities, &t.artist, 0.48 / (1.0 + i as f64 / 12.0));
        }
        for (i, t) in long_term.iter().enumerate() {
            Self::add_affinity(&mut affinities, &t.artist, 0.8 / (1.0 + i as f64 / 15.0));
        }
        for (i, t) in yt_liked.iter().enumerate() {
            let w = if lfm_active { 1.1 } else { 1.45 };
            Self::add_affinity(&mut affinities, &t.artist, w / (1.0 + i as f64 / 15.0));
        }
        for (i, t) in yt_history.iter().enumerate() {
            let w = if lfm_active { 0.6 } else { 0.9 };
            Self::add_affinity(&mut affinities, &t.artist, w / (1.0 + i as f64 / 15.0));
        }
        for (i, t) in local_liked.iter().enumerate() {
            Self::add_affinity(&mut affinities, &t.artist, 1.3 / (1.0 + i as f64 / 12.0));
        }
        for (i, t) in local_downloads.iter().enumerate() {
            Self::add_affinity(&mut affinities, &t.artist, 0.9 / (1.0 + i as f64 / 15.0));
        }
        for (i, t) in local_plays.iter().enumerate() {
            Self::add_affinity(&mut affinities, &t.artist, 0.5 / (1.0 + i as f64 / 12.0));
        }
        affinities
    }

    async fn similar_tracks(&self, name: &str, artist: &str, limit: usize) -> Vec<GeneratedTrack> {
        let ytm_fut = async {
            let mut out = Vec::new();
            let empty_excluded = HashSet::new();
            if let Ok(Some(seed)) = tokio::time::timeout(
                Duration::from_secs(4),
                self.tube
                    .find_best_match_or_null(name, artist, &empty_excluded),
            )
            .await
            {
                if let Ok(Ok(related)) = tokio::time::timeout(
                    Duration::from_secs(4),
                    self.tube.fetch_related_songs(&seed.video_id, limit),
                )
                .await
                {
                    for t in related {
                        out.push(GeneratedTrack {
                            name: t.title,
                            artist: t.artist,
                            artwork_url: t.artwork_url,
                            video_id: t.video_id,
                            ..Default::default()
                        });
                    }
                }
            }
            out
        };

        let lfm_fut = async {
            let mut out = Vec::new();
            let key = (self.api_key)();
            if key.is_empty() {
                return out;
            }
            let mut q = BTreeMap::new();
            q.insert("method".to_string(), "track.getsimilar".to_string());
            q.insert("artist".to_string(), artist.to_string());
            q.insert("track".to_string(), name.to_string());
            q.insert("api_key".to_string(), key);
            q.insert("limit".to_string(), "20".to_string());
            q.insert("autocorrect".to_string(), "1".to_string());

            if let Ok(Ok(json)) = tokio::time::timeout(Duration::from_secs(4), self.api.get(&q)).await {
                if let Some(arr) = json
                    .get("similartracks")
                    .and_then(|s| s.get("track"))
                    .and_then(Value::as_array)
                {
                    for s in arr {
                        let n = s.get("name").and_then(Value::as_str).unwrap_or("");
                        let a = s
                            .get("artist")
                            .and_then(|ar| ar.get("name").or(Some(ar)))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if n.is_empty() || a.is_empty() {
                            continue;
                        }
                        out.push(GeneratedTrack {
                            name: n.to_string(),
                            artist: a.to_string(),
                            match_score: s
                                .get("match")
                                .and_then(|v| v.as_str().map(str::to_string).or_else(|| Some(v.to_string())))
                                .unwrap_or_default(),
                            ..Default::default()
                        });
                    }
                }
            }
            out
        };

        let (ytm, lfm) = tokio::join!(ytm_fut, lfm_fut);
        let mut combined = ytm;
        combined.extend(lfm);
        combined
    }

    pub async fn resolve_videos(
        &self,
        tracks: Vec<GeneratedTrack>,
        limit: usize,
        item_timeout: Duration,
        total_timeout: Duration,
    ) -> Vec<GeneratedTrack> {
        let queue: Vec<GeneratedTrack> = tracks.into_iter().take(limit).collect();
        let mut out = Vec::with_capacity(queue.len());

        let work = async {
            for slice in queue.chunks(4) {
                let futs = slice.iter().map(|t| async move {
                    if !t.video_id.is_empty() || !t.playback_url.is_empty() {
                        return t.clone();
                    }
                    let empty_excluded = HashSet::new();
                    let Ok(Some(m)) = tokio::time::timeout(
                        item_timeout,
                        self.tube
                            .find_best_match_or_null(&t.name, &t.artist, &empty_excluded),
                    )
                    .await
                    else {
                        return t.clone();
                    };
                    GeneratedTrack {
                        name: t.name.clone(),
                        artist: t.artist.clone(),
                        album: if t.album.is_empty() { m.album } else { t.album.clone() },
                        artwork_url: if t.artwork_url.is_empty() {
                            m.artwork_url
                        } else {
                            t.artwork_url.clone()
                        },
                        video_id: m.video_id,
                        listeners: t.listeners.clone(),
                        match_score: t.match_score.clone(),
                        duration_seconds: if t.duration_seconds > 0 {
                            t.duration_seconds
                        } else {
                            m.duration_seconds
                        },
                        playback_url: t.playback_url.clone(),
                    }
                });
                out.extend(join_all(futs).await);
            }
        };

        let _ = tokio::time::timeout(total_timeout, work).await;
        out
    }

    pub async fn fill_missing_metadata(&self, t: GeneratedTrack) -> GeneratedTrack {
        if t.duration_seconds > 0 && !t.album.is_empty() {
            return t;
        }
        let Ok(songs) = self
            .tube
            .search_songs(&format!("{} {}", t.name, t.artist), 8)
            .await
        else {
            return t;
        };
        let mut hit: Option<YouTubeMusicTrack> = None;
        let mut best = -1;
        for s in songs {
            let title_sim = InnerTubeMusicApi::similarity(
                &InnerTubeMusicApi::base_title(&s.title),
                &InnerTubeMusicApi::base_title(&t.name),
            );
            if title_sim < 85 {
                continue;
            }
            if !t.artist.is_empty() && InnerTubeMusicApi::similarity(&s.artist, &t.artist) < 50 {
                continue;
            }
            if title_sim > best {
                best = title_sim;
                hit = Some(s);
            }
        }
        let Some(h) = hit else {
            return t;
        };
        GeneratedTrack {
            name: t.name,
            artist: t.artist,
            album: if t.album.is_empty() { h.album } else { t.album },
            artwork_url: if t.artwork_url.is_empty() {
                h.artwork_url
            } else {
                t.artwork_url
            },
            video_id: t.video_id,
            listeners: t.listeners,
            match_score: t.match_score,
            duration_seconds: if t.duration_seconds > 0 {
                t.duration_seconds
            } else {
                h.duration_seconds
            },
            playback_url: t.playback_url,
        }
    }

    pub async fn fetch_new_release_albums(&self, limit: usize) -> Vec<YouTubeMusicEntity> {
        self.tube
            .browse_albums(NEW_RELEASES_BROWSE_ID, limit)
            .await
            .unwrap_or_default()
    }

    pub async fn album_tracks(&self, album: &YouTubeMusicEntity) -> Vec<GeneratedTrack> {
        if album.browse_id.is_empty() {
            return Vec::new();
        }
        let Ok(songs) = self.tube.browse_songs(&album.browse_id, None, Some(50)).await else {
            return Vec::new();
        };
        let fallback_artist = album
            .subtitle
            .split(" • ")
            .nth(1)
            .unwrap_or(&album.artist);
        songs
            .into_iter()
            .map(|t| GeneratedTrack {
                name: t.title,
                artist: if t.artist.is_empty() {
                    fallback_artist.to_string()
                } else {
                    t.artist
                },
                artwork_url: if t.artwork_url.is_empty() {
                    album.artwork_url.clone()
                } else {
                    t.artwork_url
                },
                video_id: t.video_id,
                album: if t.album.is_empty() {
                    album.name.clone()
                } else {
                    t.album
                },
                duration_seconds: t.duration_seconds,
                ..Default::default()
            })
            .collect()
    }

    pub async fn load_feed(&self, charts_only: bool) -> FeedData {
        let local = match &self.hooks.fetch_local_taste {
            Some(f) => tokio::time::timeout(Duration::from_secs(5), f())
                .await
                .ok()
                .flatten()
                .unwrap_or_default(),
            None => LocalTaste::default(),
        };

        let lfm_ready = !(self.api_key)().is_empty();
        let mut last_fm_failed = false;
        let mut charts_failed = false;

        let recent_fut = async {
            if !lfm_ready {
                return Ok(Vec::new());
            }
            match &self.hooks.fetch_lfm_recent {
                Some(f) => tokio::time::timeout(Duration::from_secs(12), f(30))
                    .await
                    .map_err(|_| ())?,
                None => tokio::time::timeout(
                    Duration::from_secs(12),
                    self.home.fetch_recent_tracks(None, 30, 1),
                )
                .await
                .map_err(|_| ())?
                .map_err(|_| ()),
            }
        };

        let top_fut = async {
            if !lfm_ready {
                return Ok(Vec::new());
            }
            match &self.hooks.fetch_lfm_top {
                Some(f) => tokio::time::timeout(Duration::from_secs(12), f("7day".to_string(), 30))
                    .await
                    .map_err(|_| ())?,
                None => tokio::time::timeout(
                    Duration::from_secs(12),
                    self.home.fetch_top_tracks(None, "7day", 30),
                )
                .await
                .map_err(|_| ())?
                .map_err(|_| ()),
            }
        };

        let long_term_fut = async {
            if !lfm_ready {
                return Ok(Vec::new());
            }
            match &self.hooks.fetch_lfm_top {
                Some(f) => {
                    tokio::time::timeout(Duration::from_secs(12), f("12month".to_string(), 30))
                        .await
                        .map_err(|_| ())?
                }
                None => tokio::time::timeout(
                    Duration::from_secs(12),
                    self.home.fetch_top_tracks(None, "12month", 30),
                )
                .await
                .map_err(|_| ())?
                .map_err(|_| ()),
            }
        };

        let charts_fut = async {
            match &self.hooks.fetch_charts {
                Some(f) => tokio::time::timeout(Duration::from_secs(12), f(30))
                    .await
                    .map_err(|_| ())?,
                None => tokio::time::timeout(
                    Duration::from_secs(12),
                    self.tube.browse_songs("FEmusic_charts", None, Some(30)),
                )
                .await
                .map_err(|_| ())?
                .map_err(|_| ()),
            }
        };

        let yt_liked_fut = async {
            match &self.hooks.fetch_yt_liked {
                Some(f) => tokio::time::timeout(Duration::from_secs(12), f())
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .take(60)
                    .collect(),
                None => Vec::new(),
            }
        };

        let yt_history_fut = async {
            match &self.hooks.fetch_yt_history {
                Some(f) => tokio::time::timeout(Duration::from_secs(12), f())
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .take(60)
                    .collect(),
                None => Vec::new(),
            }
        };

        let (recent_res, top_res, charts_res, long_term_res, yt_liked, yt_history) = tokio::join!(
            recent_fut,
            top_fut,
            charts_fut,
            long_term_fut,
            yt_liked_fut,
            yt_history_fut
        );

        let recent = match recent_res {
            Ok(v) => v,
            Err(_) => {
                last_fm_failed = true;
                Vec::new()
            }
        };
        let top = match top_res {
            Ok(v) => v,
            Err(_) => {
                last_fm_failed = true;
                Vec::new()
            }
        };
        let long_term = match long_term_res {
            Ok(v) => v,
            Err(_) => {
                last_fm_failed = true;
                Vec::new()
            }
        };
        let charts = match charts_res {
            Ok(v) => v,
            Err(_) => {
                charts_failed = true;
                Vec::new()
            }
        };

        let lfm_active = !top.is_empty() || !recent.is_empty() || !long_term.is_empty();
        let affinities = Self::artist_affinities(
            &top,
            &recent,
            &long_term,
            &yt_liked,
            &yt_history,
            &local.liked_tracks,
            &local.recent_plays,
            &local.downloaded_tracks,
            lfm_active,
        );

        let excluded: HashSet<String> = self
            .db
            .as_ref()
            .map(|db| db.load_exclusion_keys())
            .unwrap_or_default();

        let mut rotation = StdRng::seed_from_u64(feed_day_seed(Local::now()));
        let max_affinity = affinities.values().copied().fold(0.0_f64, f64::max);
        let norm_affinity = |artist: &str| -> f64 {
            if max_affinity <= 0.0 {
                0.0
            } else {
                affinities
                    .get(&normalize_artist_key(artist))
                    .copied()
                    .unwrap_or(0.0)
                    / max_affinity
            }
        };

        let from_home = |t: &HomeTrack| GeneratedTrack {
            name: t.name.clone(),
            artist: t.artist.clone(),
            artwork_url: t.artwork_url.clone(),
            ..Default::default()
        };
        let from_yt = |t: &YouTubeMusicTrack| GeneratedTrack {
            name: t.title.clone(),
            artist: t.artist.clone(),
            artwork_url: t.artwork_url.clone(),
            video_id: t.video_id.clone(),
            ..Default::default()
        };

        let mut score_fn = |tracks: Vec<GeneratedTrack>, boost: f64| -> Vec<(GeneratedTrack, f64)> {
            tracks
                .into_iter()
                .enumerate()
                .map(|(i, t)| {
                    let s = norm_affinity(&t.artist) * 50.0
                        + boost * 20.0 / (1.0 + i as f64 / 9.0)
                        + if t.video_id.is_empty() { -2.0 } else { 4.0 }
                        + rotation.gen::<f64>() * 12.0;
                    (t, s)
                })
                .collect()
        };

        let mut page_counts = HashMap::new();
        let mut heavy_pool = score_fn(
            apply_exclusions(top.iter().take(25).map(from_home).collect(), &excluded),
            3.0,
        );
        heavy_pool.extend(score_fn(
            apply_exclusions(
                local.liked_tracks.iter().take(10).map(from_home).collect(),
                &excluded,
            ),
            2.8,
        ));
        if !lfm_active {
            heavy_pool.extend(score_fn(
                apply_exclusions(yt_liked.iter().take(15).map(from_yt).collect(), &excluded),
                2.7,
            ));
        }
        let heavy = diversify_feed_tracks(heavy_pool, 15, 2, Some(&mut page_counts));

        let mut quick_pool = score_fn(
            apply_exclusions(top.iter().take(15).map(from_home).collect(), &excluded),
            2.6,
        );
        quick_pool.extend(score_fn(
            apply_exclusions(recent.iter().take(15).map(from_home).collect(), &excluded),
            1.6,
        ));
        quick_pool.extend(score_fn(
            apply_exclusions(charts.iter().take(15).map(from_yt).collect(), &excluded),
            1.2,
        ));
        if !lfm_active {
            quick_pool.extend(score_fn(
                apply_exclusions(yt_liked.iter().take(15).map(from_yt).collect(), &excluded),
                2.4,
            ));
            quick_pool.extend(score_fn(
                apply_exclusions(yt_history.iter().take(15).map(from_yt).collect(), &excluded),
                1.6,
            ));
        }
        let quick = diversify_feed_tracks(quick_pool, 18, 2, Some(&mut page_counts));

        let banned = |t: &HomeTrack| excluded.contains(&AppDatabase::exclusion_key(&t.name, &t.artist));
        let yt_seed = |t: &YouTubeMusicTrack| HomeTrack {
            name: t.title.clone(),
            artist: t.artist.clone(),
            artwork_url: t.artwork_url.clone(),
            ..Default::default()
        };

        let mut seeds: Vec<HomeTrack> = Vec::new();
        let mut seen_artists = HashSet::new();
        for t in top
            .iter()
            .cloned()
            .chain(recent.iter().cloned())
            .chain(yt_liked.iter().take(20).map(yt_seed))
            .chain(yt_history.iter().take(20).map(yt_seed))
            .chain(local.seed_pool(8))
        {
            if banned(&t) {
                continue;
            }
            let k = normalize_artist_key(&t.artist);
            if k.is_empty() || !seen_artists.insert(k) {
                continue;
            }
            seeds.push(t);
            if seeds.len() >= 4 {
                break;
            }
        }
        if seeds.is_empty() {
            for c in charts.iter().take(8) {
                let t = yt_seed(c);
                if banned(&t) {
                    continue;
                }
                let k = normalize_artist_key(&t.artist);
                if k.is_empty() || !seen_artists.insert(k) {
                    continue;
                }
                seeds.push(t);
                if seeds.len() >= 4 {
                    break;
                }
            }
        }

        let discovery_batches = join_all(seeds.iter().map(|seed| async move {
            let fut = match &self.hooks.fetch_similar {
                Some(f) => f(seed.name.clone(), seed.artist.clone(), 8),
                None => Box::pin(self.similar_tracks(&seed.name, &seed.artist, 8)),
            };
            tokio::time::timeout(Duration::from_secs(4), fut)
                .await
                .unwrap_or_default()
        }))
        .await;

        let mut discovery = Vec::new();
        for batch in &discovery_batches {
            discovery.extend(batch.iter().cloned());
        }

        let fresh = diversify_feed_tracks(
            score_fn(apply_exclusions(discovery.clone(), &excluded), 2.0),
            12,
            1,
            Some(&mut page_counts),
        );

        let mut jump_pool = score_fn(
            recent
                .iter()
                .filter(|t| !banned(t))
                .take(20)
                .map(from_home)
                .collect(),
            2.0,
        );
        jump_pool.extend(score_fn(
            local
                .recent_plays
                .iter()
                .filter(|t| !banned(t))
                .take(20)
                .map(from_home)
                .collect(),
            1.8,
        ));
        if !lfm_active {
            jump_pool.extend(score_fn(
                yt_history
                    .iter()
                    .filter(|t| !excluded.contains(&AppDatabase::exclusion_key(&t.title, &t.artist)))
                    .take(20)
                    .map(from_yt)
                    .collect(),
                1.8,
            ));
        }
        let jump_back = diversify_feed_tracks(jump_pool, 12, 2, None);

        let mut because_seed = String::new();
        let mut because_pool = Vec::new();
        let mut seeded = false;
        if !seeds.is_empty() {
            let mut best = -1.0_f64;
            let mut best_batch: Option<usize> = None;
            for (b, s) in seeds.iter().enumerate() {
                if b >= discovery_batches.len() {
                    break;
                }
                let aff = norm_affinity(&s.artist);
                if aff > best {
                    best = aff;
                    best_batch = Some(b);
                }
            }
            if let Some(bb) = best_batch {
                if !discovery_batches[bb].is_empty() {
                    because_pool = discovery_batches[bb].clone();
                    because_seed = seeds[bb].artist.clone();
                    seeded = true;
                }
            }
        }
        if !seeded {
            because_pool = discovery.into_iter().rev().take(20).collect();
        }
        let because = diversify_feed_tracks(
            score_fn(apply_exclusions(because_pool, &excluded), 2.2),
            12,
            1,
            Some(&mut page_counts),
        );

        let mut headlined = HashSet::new();
        let deduped_heavy = dedupe_feed_heads(heavy, &mut headlined, 3);
        let deduped_quick = dedupe_feed_heads(quick, &mut headlined, 3);
        let deduped_fresh = dedupe_feed_heads(fresh, &mut headlined, 3);
        let deduped_because = dedupe_feed_heads(because, &mut headlined, 3);

        let (final_quick, final_heavy, final_jump, final_fresh, final_because) = if charts_only {
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new())
        } else {
            tokio::join!(
                self.guard_hydrate(deduped_quick, 8),
                self.guard_hydrate(deduped_heavy, 6),
                self.guard_hydrate(jump_back, 6),
                self.guard_hydrate(deduped_fresh, 6),
                self.guard_hydrate(deduped_because, 6),
            )
        };

        let feed_charts = apply_exclusions(charts.iter().take(15).map(from_yt).collect(), &excluded);
        let is_feed_empty = final_quick.is_empty()
            && final_heavy.is_empty()
            && final_fresh.is_empty()
            && final_jump.is_empty()
            && final_because.is_empty()
            && feed_charts.is_empty();

        let reason = if !is_feed_empty {
            FeedEmptyReason::None
        } else if last_fm_failed && charts_failed {
            FeedEmptyReason::AllFailed
        } else if last_fm_failed {
            FeedEmptyReason::LastfmError
        } else if charts_failed {
            FeedEmptyReason::ChartsError
        } else {
            FeedEmptyReason::NoTaste
        };

        let mut taste_tags: Vec<String> = affinities.keys().take(8).cloned().collect();
        taste_tags.sort();

        FeedData {
            quick_picks: final_quick,
            heavy_rotation: final_heavy,
            fresh_finds: final_fresh,
            jump_back_in: final_jump,
            because_you_listened: final_because,
            charts: feed_charts,
            taste_tags,
            because_seed,
            taste_affinities: affinities,
            last_fm_failed,
            charts_failed,
            empty_reason: reason,
        }
    }

    async fn guard_hydrate(
        &self,
        list: Vec<GeneratedTrack>,
        limit: usize,
    ) -> Vec<GeneratedTrack> {
        let fallback = list.clone();
        let futs = list.into_iter().enumerate().map(|(i, t)| async move {
            if i >= limit || !t.artwork_url.is_empty() || t.name.is_empty() {
                return t;
            }
            if let Some(ref off) = self.official {
                if let Ok(Some(art)) = tokio::time::timeout(
                    Duration::from_millis(1500),
                    off.resolve_official_artwork(&t.name, &t.artist, ""),
                )
                .await
                {
                    if !art.artwork_url.is_empty() {
                        return GeneratedTrack {
                            artwork_url: art.artwork_url,
                            ..t
                        };
                    }
                }
            }
            let empty_excluded = HashSet::new();
            if let Ok(Some(m)) = tokio::time::timeout(
                Duration::from_secs(2),
                self.tube
                    .find_best_match_or_null(&t.name, &t.artist, &empty_excluded),
            )
            .await
            {
                if !m.artwork_url.is_empty() {
                    return GeneratedTrack {
                        album: if t.album.is_empty() { m.album } else { t.album },
                        artwork_url: m.artwork_url,
                        video_id: if t.video_id.is_empty() {
                            m.video_id
                        } else {
                            t.video_id
                        },
                        duration_seconds: if t.duration_seconds > 0 {
                            t.duration_seconds
                        } else {
                            m.duration_seconds
                        },
                        ..t
                    };
                }
            }
            if let Ok(Ok(songs)) = tokio::time::timeout(
                Duration::from_secs(2),
                self.tube.search_songs(&format!("{} {}", t.name, t.artist), 8),
            )
            .await
            {
                let mut loose: Option<&YouTubeMusicTrack> = None;
                let mut loose_score = -1;
                for s in &songs {
                    if s.artwork_url.is_empty() {
                        continue;
                    }
                    let sim = InnerTubeMusicApi::similarity(
                        &InnerTubeMusicApi::base_title(&s.title),
                        &InnerTubeMusicApi::base_title(&t.name),
                    );
                    if sim >= 60 && sim > loose_score {
                        loose_score = sim;
                        loose = Some(s);
                    }
                }
                loose = loose.or_else(|| songs.iter().find(|s| !s.artwork_url.is_empty()));
                if let Some(l) = loose {
                    return GeneratedTrack {
                        artwork_url: l.artwork_url.clone(),
                        ..t
                    };
                }
            }
            t
        });

        tokio::time::timeout(Duration::from_secs(6), join_all(futs))
            .await
            .unwrap_or(fallback)
    }

    pub async fn fetch_personal_mix(
        &self,
        limit: usize,
    ) -> Option<crate::storage::home_snapshot::PersonalMixData> {
        let liked = match &self.hooks.fetch_yt_liked {
            Some(f) => tokio::time::timeout(Duration::from_secs(12), f())
                .await
                .unwrap_or_default()
                .into_iter()
                .take(40)
                .collect(),
            None => Vec::new(),
        };
        let history = match &self.hooks.fetch_yt_history {
            Some(f) => tokio::time::timeout(Duration::from_secs(12), f())
                .await
                .unwrap_or_default()
                .into_iter()
                .take(40)
                .collect(),
            None => Vec::new(),
        };
        let mix_local = match &self.hooks.fetch_local_taste {
            Some(f) => tokio::time::timeout(Duration::from_secs(5), f())
                .await
                .ok()
                .flatten()
                .unwrap_or_default(),
            None => LocalTaste::default(),
        };

        let mut pool: Vec<(String, String, String, String)> = Vec::new();
        let mut seen = HashSet::new();
        let mut add_seed = |title: &str, artist: &str, video_id: &str, art: &str| {
            let key = format!("{}|{}", title.to_lowercase(), artist.to_lowercase());
            if title.is_empty() || !seen.insert(key) {
                return;
            }
            pool.push((
                title.to_string(),
                artist.to_string(),
                video_id.to_string(),
                art.to_string(),
            ));
        };

        for t in history.iter().chain(liked.iter()) {
            add_seed(&t.title, &t.artist, &t.video_id, &t.artwork_url);
        }
        for t in mix_local.seed_pool(12) {
            add_seed(&t.name, &t.artist, "", &t.artwork_url);
        }
        if pool.is_empty() {
            return None;
        }

        let mut rotation = StdRng::seed_from_u64(feed_day_seed(Local::now()));
        let idx = rotation.gen_range(0..pool.len());
        let (seed_title, seed_artist, mut seed_id, mut seed_art) = pool[idx].clone();
        if seed_id.is_empty() {
            let empty_excluded = HashSet::new();
            let matched = tokio::time::timeout(
                Duration::from_secs(8),
                self.tube
                    .find_best_match_or_null(&seed_title, &seed_artist, &empty_excluded),
            )
            .await
            .ok()
            .flatten();
            if let Some(m) = matched {
                seed_id = m.video_id;
                if seed_art.is_empty() {
                    seed_art = m.artwork_url;
                }
            }
        }
        if seed_id.is_empty() {
            return None;
        }

        let radio = tokio::time::timeout(
            Duration::from_secs(8),
            self.tube.fetch_related_songs(&seed_id, limit + 5),
        )
        .await
        .ok()?
        .ok()?;

        let mut tracks = Vec::new();
        let mut track_seen = HashSet::new();
        track_seen.insert(format!(
            "{}|{}",
            seed_title.to_lowercase(),
            seed_artist.to_lowercase()
        ));
        for t in radio {
            let key = format!("{}|{}", t.title.to_lowercase(), t.artist.to_lowercase());
            if t.title.is_empty() || !track_seen.insert(key) {
                continue;
            }
            tracks.push(GeneratedTrack {
                name: t.title,
                artist: t.artist,
                artwork_url: t.artwork_url,
                video_id: t.video_id,
                ..Default::default()
            });
            if tracks.len() >= limit {
                break;
            }
        }
        if tracks.is_empty() {
            return None;
        }

        Some(crate::storage::home_snapshot::PersonalMixData {
            seed: GeneratedTrack {
                name: seed_title,
                artist: seed_artist,
                artwork_url: seed_art,
                video_id: seed_id,
                ..Default::default()
            },
            tracks,
        })
    }

    /// Generate a 30-35 track mood mix from taste signals.
    pub async fn fetch_mix(&self, total: usize) -> Vec<GeneratedTrack> {
        let recent = self
            .home
            .fetch_recent_tracks(None, 20, 1)
            .await
            .unwrap_or_default();
        let top = self
            .home
            .fetch_top_tracks(None, "overall", 20)
            .await
            .unwrap_or_default();
        let excluded: HashSet<String> = self
            .db
            .as_ref()
            .map(|db| db.load_exclusion_keys())
            .unwrap_or_default();
        let banned = |t: &HomeTrack| excluded.contains(&AppDatabase::exclusion_key(&t.name, &t.artist));

        let mix_local = match &self.hooks.fetch_local_taste {
            Some(f) => tokio::time::timeout(Duration::from_secs(5), f())
                .await
                .ok()
                .flatten()
                .unwrap_or_default(),
            None => LocalTaste::default(),
        };

        let mut pool_seeds: Vec<HomeTrack> = Vec::new();
        pool_seeds.extend(recent.iter().filter(|t| !banned(t)).take(3).cloned());
        pool_seeds.extend(top.iter().filter(|t| !banned(t)).take(3).cloned());
        pool_seeds.extend(
            mix_local
                .seed_pool(6)
                .into_iter()
                .filter(|t| !banned(t)),
        );

        let pooled_batches = join_all(pool_seeds.iter().map(|t| async move {
            tokio::time::timeout(Duration::from_secs(8), self.similar_tracks(&t.name, &t.artist, 8))
                .await
                .unwrap_or_default()
        }))
        .await;
        let mut pooled = Vec::new();
        for batch in pooled_batches {
            pooled.extend(batch);
        }

        if pooled.is_empty() {
            return self
                .tube
                .browse_songs("FEmusic_charts", None, Some(total))
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|t| GeneratedTrack {
                    name: t.title,
                    artist: t.artist,
                    artwork_url: t.artwork_url,
                    video_id: t.video_id,
                    ..Default::default()
                })
                .collect();
        }

        let long_term = self
            .home
            .fetch_top_tracks(None, "12month", 20)
            .await
            .unwrap_or_default();
        let lfm_active = !top.is_empty() || !recent.is_empty() || !long_term.is_empty();
        let affinities = Self::artist_affinities(
            &top,
            &recent,
            &long_term,
            &[],
            &[],
            &mix_local.liked_tracks,
            &mix_local.recent_plays,
            &mix_local.downloaded_tracks,
            lfm_active,
        );
        let max_affinity = affinities.values().copied().fold(0.0_f64, f64::max);
        let mut rng = StdRng::from_entropy();
        let scored: Vec<(GeneratedTrack, f64)> = pooled
            .into_iter()
            .map(|t| {
                let norm = if max_affinity <= 0.0 {
                    0.0
                } else {
                    affinities
                        .get(&normalize_artist_key(&t.artist))
                        .copied()
                        .unwrap_or(0.0)
                        / max_affinity
                };
                let score = norm * 30.0 + rng.gen::<f64>() * 10.0;
                (t, score)
            })
            .collect();

        let picked = apply_exclusions(diversify_feed_tracks(scored, total, 2, None), &excluded);
        self.resolve_videos(picked, total, Duration::from_secs(10), Duration::from_secs(40))
            .await
    }

    pub async fn generate_for_tag(&self, tag: &str, total: usize) -> Vec<GeneratedTrack> {
        let count = total.clamp(5, 35);
        let excluded: HashSet<String> = self
            .db
            .as_ref()
            .map(|db| db.load_exclusion_keys())
            .unwrap_or_default();

        let mut rows = apply_exclusions(
            Self::dedupe_tracks(self.generate_tag(count, tag).await),
            &excluded,
        );
        if rows.len() < count {
            let charts = apply_exclusions(self.chart_fallback(count).await, &excluded);
            rows.extend(charts);
            rows = Self::dedupe_tracks(rows);
        }
        let mut resolved = self
            .resolve_videos(rows, count, Duration::from_secs(10), Duration::from_secs(40))
            .await;
        resolved.truncate(count);
        resolved
    }

    fn dedupe_tracks(tracks: Vec<GeneratedTrack>) -> Vec<GeneratedTrack> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for t in tracks {
            if t.name.trim().is_empty() {
                continue;
            }
            let key = if !t.playback_url.is_empty() {
                format!("file:{}", t.playback_url.to_lowercase())
            } else if !t.video_id.is_empty() {
                format!("id:{}", t.video_id)
            } else {
                t.key()
            };
            if seen.insert(key) {
                out.push(t);
            }
        }
        out
    }

    async fn generate_tag(&self, total: usize, tag: &str) -> Vec<GeneratedTrack> {
        let query = tag.trim();
        if query.is_empty() || !is_music_genre_tag(query) {
            return Vec::new();
        }
        let mut out = Vec::new();
        let key = (self.api_key)();
        if !key.is_empty() {
            let mut q = BTreeMap::new();
            q.insert("method".to_string(), "tag.gettoptracks".to_string());
            q.insert("tag".to_string(), query.to_string());
            q.insert("api_key".to_string(), key);
            q.insert("limit".to_string(), total.to_string());
            if let Ok(Ok(json)) = tokio::time::timeout(Duration::from_secs(8), self.api.get(&q)).await
            {
                if let Some(arr) = json
                    .get("tracks")
                    .and_then(|t| t.get("track"))
                    .and_then(Value::as_array)
                {
                    for s in arr {
                        let n = s.get("name").and_then(Value::as_str).unwrap_or("");
                        let a = s
                            .get("artist")
                            .and_then(|ar| ar.get("name").or(Some(ar)))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if n.is_empty() || Self::title_is_the_tag(n, query) {
                            continue;
                        }
                        out.push(GeneratedTrack {
                            name: n.to_string(),
                            artist: a.to_string(),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        if out.len() < total {
            if let Ok(Ok(songs)) = tokio::time::timeout(
                Duration::from_secs(8),
                self.tube.search_songs(&format!("{query} music"), total),
            )
            .await
            {
                for s in songs {
                    if Self::title_is_the_tag(&s.title, query) {
                        continue;
                    }
                    out.push(GeneratedTrack {
                        name: s.title,
                        artist: s.artist,
                        album: s.album,
                        artwork_url: s.artwork_url,
                        video_id: s.video_id,
                        duration_seconds: s.duration_seconds,
                        ..Default::default()
                    });
                }
            }
        }
        out
    }

    fn title_is_the_tag(title: &str, tag: &str) -> bool {
        let name = title.to_lowercase().trim().to_string();
        let genre = tag.to_lowercase().trim().to_string();
        if name.is_empty() || genre.is_empty() {
            return false;
        }
        if name == genre || name == format!("{genre}s") {
            return true;
        }
        let stripped = NON_ALNUM_ASCII_RE.replace_all(&name, "");
        let genre_key = NON_ALNUM_ASCII_RE.replace_all(&genre, "");
        stripped == genre_key || stripped == format!("{genre_key}s")
    }

    async fn chart_fallback(&self, total: usize) -> Vec<GeneratedTrack> {
        tokio::time::timeout(
            Duration::from_secs(12),
            self.tube.browse_songs("FEmusic_charts", None, Some(total)),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default()
        .into_iter()
        .map(|s| GeneratedTrack {
            name: s.title,
            artist: s.artist,
            album: s.album,
            artwork_url: s.artwork_url,
            video_id: s.video_id,
            duration_seconds: s.duration_seconds,
            ..Default::default()
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_artist_key_strips_feats_and_topics() {
        assert_eq!(normalize_artist_key("The Weeknd - Topic"), "the weeknd");
        assert_eq!(
            normalize_artist_key("Drake feat. Future & Metro Boomin"),
            "drake"
        );
        assert_eq!(normalize_artist_key("Various Artists"), "");
    }

    #[test]
    fn diversify_caps_per_artist_and_dedupes() {
        let mk = |name: &str, artist: &str| GeneratedTrack {
            name: name.to_string(),
            artist: artist.to_string(),
            ..Default::default()
        };
        let scored = vec![
            (mk("S1", "Artist A"), 100.0),
            (mk("S2", "Artist A"), 90.0),
            (mk("S3", "Artist A"), 80.0),
            (mk("S4", "Artist B"), 70.0),
        ];
        let out = diversify_feed_tracks(scored, 10, 2, None);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].name, "S1");
        assert_eq!(out[1].name, "S2");
        assert_eq!(out[2].name, "S4");
    }
}
