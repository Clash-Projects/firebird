use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use thiserror::Error;
use tokio::sync::Mutex;

use crate::models::player_state::PlayerSnapshot;
use crate::models::podcast_models::{
    PodcastEpisode, PodcastEpisodePage, PodcastShelf, PodcastShowPage,
};
use crate::models::stream_models::PlayableTrack;
use crate::storage::app_database::{AppDatabase, PodcastProgressRow};
use crate::storage::diagnostics_log::DiagnosticsLog;
use crate::storage::prefs::Prefs;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

const HOME_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const FALLBACK_SHELVES: [(&str, &str); 3] = [
    ("Popular episodes", "podcast"),
    ("Interviews", "interview podcast"),
    ("News & stories", "news podcast"),
];

#[derive(Debug, Clone, Error)]
#[error("Podcast API error (status={status_code:?}): {message}")]
pub struct PodcastApiError {
    pub status_code: Option<u16>,
    pub message: String,
}

impl PodcastApiError {
    pub fn new(status_code: Option<u16>, message: impl Into<String>) -> Self {
        Self {
            status_code,
            message: message.into(),
        }
    }
}

/// InnerTube Music API abstraction required by [`PodcastRepository`].
pub trait PodcastTubeClient: Send + Sync {
    fn fetch_podcast_home(&self) -> BoxFuture<'_, Result<Vec<PodcastShelf>, PodcastApiError>>;

    fn search_podcast_episodes<'a>(
        &'a self,
        query: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<PodcastEpisode>, PodcastApiError>>;

    fn fetch_podcast_show<'a>(
        &'a self,
        show_id: &'a str,
    ) -> BoxFuture<'a, Result<Option<PodcastShowPage>, PodcastApiError>>;

    fn fetch_podcast_show_continuation<'a>(
        &'a self,
        show_id: &'a str,
        continuation: &'a str,
        show_title: Option<&'a str>,
    ) -> BoxFuture<'a, Result<PodcastEpisodePage, PodcastApiError>>;
}

#[derive(Default)]
struct PodcastCache {
    home: Option<Vec<PodcastShelf>>,
    home_at: Option<Instant>,
    shows: HashMap<String, PodcastShowPage>,
}

pub struct PodcastRepository {
    tube: Arc<dyn PodcastTubeClient>,
    db: Arc<AppDatabase>,
    prefs: Arc<Prefs>,
    cache: Mutex<PodcastCache>,
}

impl PodcastRepository {
    pub fn new(
        tube: Arc<dyn PodcastTubeClient>,
        db: Arc<AppDatabase>,
        prefs: Arc<Prefs>,
    ) -> Self {
        Self {
            tube,
            db,
            prefs,
            cache: Mutex::new(PodcastCache::default()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.prefs.podcasts_enabled()
    }

    pub async fn home(&self, force: bool) -> Result<Vec<PodcastShelf>, PodcastApiError> {
        if !self.enabled() {
            return Ok(Vec::new());
        }
        if !force {
            let cache = self.cache.lock().await;
            if let (Some(cached), Some(at)) = (&cache.home, cache.home_at) {
                if Instant::now().duration_since(at) < HOME_CACHE_TTL {
                    return Ok(cached.clone());
                }
            }
        }

        let shelves = match self.tube.fetch_podcast_home().await {
            Ok(shelves) if !shelves.is_empty() => shelves,
            Ok(_) => {
                DiagnosticsLog::info("podcasts home empty -> fallback=search");
                self.search_home().await
            }
            Err(err) => {
                DiagnosticsLog::info(&format!(
                    "podcasts home err={err} status={:?} -> fallback=search",
                    err.status_code
                ));
                let fallback = self.search_home().await;
                if fallback.is_empty() {
                    return Err(err);
                }
                fallback
            }
        };

        if !shelves.is_empty() {
            let mut cache = self.cache.lock().await;
            cache.home = Some(shelves.clone());
            cache.home_at = Some(Instant::now());
        }
        Ok(shelves)
    }

    async fn search_home(&self) -> Vec<PodcastShelf> {
        let futs = FALLBACK_SHELVES.iter().map(|&(_, query)| async move {
            self.tube
                .search_podcast_episodes(query, 20)
                .await
                .unwrap_or_default()
        });
        let results = futures_util::future::join_all(futs).await;
        FALLBACK_SHELVES
            .iter()
            .zip(results)
            .filter_map(|((title, _), episodes)| {
                if episodes.is_empty() {
                    None
                } else {
                    Some(PodcastShelf {
                        title: (*title).to_string(),
                        shows: Vec::new(),
                        episodes,
                    })
                }
            })
            .collect()
    }

    pub async fn show(
        &self,
        show_id: &str,
        force: bool,
    ) -> Result<Option<PodcastShowPage>, PodcastApiError> {
        if !self.enabled() || show_id.trim().is_empty() {
            return Ok(None);
        }
        if !force {
            let cache = self.cache.lock().await;
            if let Some(page) = cache.shows.get(show_id) {
                return Ok(Some(page.clone()));
            }
        }
        let page = self.tube.fetch_podcast_show(show_id).await?;
        if let Some(ref p) = page {
            let mut cache = self.cache.lock().await;
            cache.shows.insert(show_id.to_string(), p.clone());
        }
        Ok(page)
    }

    pub async fn more(
        &self,
        show_id: &str,
        continuation: Option<&str>,
    ) -> Result<PodcastEpisodePage, PodcastApiError> {
        let Some(cont) = continuation.filter(|c| !c.is_empty()) else {
            return Ok(PodcastEpisodePage::default());
        };
        if !self.enabled() {
            return Ok(PodcastEpisodePage::default());
        }
        let existing_title = {
            let cache = self.cache.lock().await;
            cache.shows.get(show_id).map(|p| p.show.title.clone())
        };
        let page = self
            .tube
            .fetch_podcast_show_continuation(show_id, cont, existing_title.as_deref())
            .await?;

        if !page.episodes.is_empty() {
            let mut cache = self.cache.lock().await;
            if let Some(current) = cache.shows.get_mut(show_id) {
                let mut seen: HashSet<String> = current
                    .episodes
                    .iter()
                    .map(|e| e.video_id.clone())
                    .collect();
                for ep in &page.episodes {
                    if seen.insert(ep.video_id.clone()) {
                        current.episodes.push(ep.clone());
                    }
                }
                current.continuation = page.continuation.clone();
            }
        }
        Ok(page)
    }

    pub async fn search(&self, query: &str) -> Result<Vec<PodcastEpisode>, PodcastApiError> {
        if !self.enabled() || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.tube.search_podcast_episodes(query, 20).await
    }

    pub fn remember(&self, episode: &PodcastEpisode) {
        self.db.upsert_podcast_progress(
            &episode.video_id,
            &episode.show_id,
            &episode.show_title,
            &episode.title,
            episode.duration_ms,
        );
    }

    pub fn resume_ms(&self, video_id: &str) -> i64 {
        self.db.podcast_resume_ms(video_id)
    }

    pub fn progress_for(&self, video_ids: &[String]) -> HashMap<String, PodcastProgressRow> {
        self.db.podcast_progress_for(video_ids)
    }
}

pub fn track_from_episode(episode: &PodcastEpisode) -> PlayableTrack {
    let show = episode.show_title.trim();
    PlayableTrack {
        title: episode.title.clone(),
        artist: if show.is_empty() {
            "Podcast".to_string()
        } else {
            show.to_string()
        },
        artwork_url: episode.artwork_url.clone(),
        video_id: episode.video_id.clone(),
        duration_ms: episode.duration_ms.max(0),
        ..Default::default()
    }
}

/// Tracks podcast episode resume seeks and throttled playhead writes.
#[derive(Debug, Default)]
pub struct PodcastProgressCoordinator {
    seeked_for: String,
    last_write_ms: i64,
    last_video: String,
    was_playing: bool,
}

impl PodcastProgressCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates a playback snapshot against `podcast_progress`.
    /// Returns `Some(Duration)` when playback starts near zero on a known
    /// episode with a saved resume position (> 8s).
    pub fn on_snapshot(
        &mut self,
        next: &PlayerSnapshot,
        db: &AppDatabase,
    ) -> Option<Duration> {
        let track = next.current.as_ref()?;
        let id = track.video_id.as_str();
        if id.is_empty() {
            self.was_playing = next.is_playing;
            return None;
        }
        let Some(row) = db.podcast_progress(id) else {
            self.was_playing = next.is_playing;
            return None;
        };

        let mut seek_to = None;
        let resume = db.podcast_resume_ms(id);
        if self.seeked_for != id
            && next.is_playing
            && next.position_ms < 2000
            && resume > 8000
        {
            self.seeked_for = id.to_string();
            seek_to = Some(Duration::from_millis(resume as u64));
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let paused = self.was_playing && !next.is_playing;
        self.was_playing = next.is_playing;

        if !paused && now_ms - self.last_write_ms < 5000 && self.last_video == id {
            return seek_to;
        }
        self.last_write_ms = now_ms;
        self.last_video = id.to_string();

        let duration_ms = if next.duration_ms > 0 {
            next.duration_ms as i64
        } else {
            row.duration_ms
        };
        db.save_podcast_position(id, next.position_ms as i64, duration_ms);
        seek_to
    }
}
