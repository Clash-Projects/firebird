//! Profile/stats/home repository.
//!
//! Ported from `lib/features/lastfm/home_repository.dart`.
//! Dispatches reads to whichever scrobbler backend is currently active
//! (`Last.fm`, `ListenBrainz`, or `Libre.fm`), or returns empty when running
//! in Guest / Local-only mode.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::integrations::lastfm::lastfm_api::{LastFmApiError, LastFmApiService};
use crate::integrations::scrobblers::extra_scrobblers::{
    LibreFmScrobbler, ListenBrainzScrobbler, ScrobblerReadBackend,
};
use crate::integrations::scrobblers::scrobbler_session::ScrobblerKind;
use crate::models::lastfm_home_models::{
    HomeStats, HomeTrack, LastFmResponseParser, TopAlbumItem, TopArtistItem,
};
use crate::storage::prefs::Prefs;

pub struct HomeRepository {
    api: Arc<LastFmApiService>,
    prefs: Arc<Prefs>,
    active_kind: Option<ScrobblerKind>,
    listen_brainz: Option<Arc<ListenBrainzScrobbler>>,
    libre_fm: Option<Arc<LibreFmScrobbler>>,
}

impl HomeRepository {
    pub fn new(
        api: Arc<LastFmApiService>,
        prefs: Arc<Prefs>,
        active_kind: Option<ScrobblerKind>,
        listen_brainz: Option<Arc<ListenBrainzScrobbler>>,
        libre_fm: Option<Arc<LibreFmScrobbler>>,
    ) -> Self {
        Self {
            api,
            prefs,
            active_kind,
            listen_brainz,
            libre_fm,
        }
    }

    fn api_key(&self) -> String {
        self.prefs.last_fm_api_key()
    }

    fn active_extra_backend(&self) -> Option<&dyn ScrobblerReadBackend> {
        match self.active_kind {
            Some(ScrobblerKind::Listenbrainz) => self
                .listen_brainz
                .as_deref()
                .map(|b| b as &dyn ScrobblerReadBackend),
            Some(ScrobblerKind::Librefm) => self
                .libre_fm
                .as_deref()
                .map(|b| b as &dyn ScrobblerReadBackend),
            _ => None,
        }
    }

    fn effective_last_fm_user(&self, viewing_as: Option<&str>) -> Option<String> {
        let user = match viewing_as {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => {
                let u = self.prefs.username();
                if u.is_empty() {
                    return None;
                }
                u
            }
        };
        if self.api_key().is_empty() {
            return None;
        }
        Some(user)
    }

    pub async fn fetch_recent_tracks(
        &self,
        viewing_as: Option<&str>,
        limit: usize,
        page: usize,
    ) -> Result<Vec<HomeTrack>, LastFmApiError> {
        if let Some(extra) = self.active_extra_backend() {
            return Ok(extra.fetch_recent_tracks(viewing_as, limit, page).await);
        }
        if self.active_kind.is_none() && viewing_as.is_none() {
            return Ok(Vec::new());
        }
        let Some(user) = self.effective_last_fm_user(viewing_as) else {
            return Ok(Vec::new());
        };
        let mut q = BTreeMap::new();
        q.insert("method".to_string(), "user.getrecenttracks".to_string());
        q.insert("user".to_string(), user);
        q.insert("api_key".to_string(), self.api_key());
        q.insert("limit".to_string(), limit.to_string());
        q.insert("page".to_string(), page.to_string());
        q.insert("extended".to_string(), "0".to_string());
        let json = self.api.get(&q).await?;
        Ok(LastFmResponseParser::parse_recent_tracks(&json))
    }

    pub async fn fetch_stats(
        &self,
        viewing_as: Option<&str>,
    ) -> Result<HomeStats, LastFmApiError> {
        if let Some(extra) = self.active_extra_backend() {
            return Ok(extra.fetch_stats(viewing_as).await);
        }
        if self.active_kind.is_none() && viewing_as.is_none() {
            return Ok(HomeStats::default());
        }
        let Some(user) = self.effective_last_fm_user(viewing_as) else {
            return Ok(HomeStats::default());
        };
        let key = self.api_key();
        let make_q = |method: &str, extra: &[(&str, &str)]| {
            let mut q = BTreeMap::new();
            q.insert("method".to_string(), method.to_string());
            q.insert("user".to_string(), user.clone());
            q.insert("api_key".to_string(), key.clone());
            for &(k, v) in extra {
                q.insert(k.to_string(), v.to_string());
            }
            q
        };

        let q0 = make_q("user.getinfo", &[]);
        let q1 = make_q("user.gettoptracks", &[("limit", "1"), ("period", "overall")]);
        let q2 = make_q("user.gettopartists", &[("limit", "1"), ("period", "overall")]);
        let q3 = make_q("user.gettopalbums", &[("limit", "1"), ("period", "overall")]);

        let (r0, r1, r2, r3) = tokio::try_join!(
            self.api.get(&q0),
            self.api.get(&q1),
            self.api.get(&q2),
            self.api.get(&q3),
        )?;

        Ok(LastFmResponseParser::parse_stats(&r0, Some(&r1), Some(&r2), Some(&r3)))
    }

    pub async fn fetch_period_play_count(
        &self,
        viewing_as: Option<&str>,
        from_millis: i64,
        to_millis: i64,
    ) -> Result<i64, LastFmApiError> {
        if let Some(extra) = self.active_extra_backend() {
            return Ok(extra
                .fetch_period_play_count(viewing_as, from_millis, to_millis)
                .await);
        }
        if self.active_kind.is_none() && viewing_as.is_none() {
            return Ok(0);
        }
        let Some(user) = self.effective_last_fm_user(viewing_as) else {
            return Ok(0);
        };
        let mut q = BTreeMap::new();
        q.insert("method".to_string(), "user.getrecenttracks".to_string());
        q.insert("user".to_string(), user);
        q.insert("api_key".to_string(), self.api_key());
        q.insert("from".to_string(), (from_millis / 1000).to_string());
        q.insert("to".to_string(), (to_millis / 1000).to_string());
        q.insert("limit".to_string(), "1".to_string());
        q.insert("page".to_string(), "1".to_string());
        let json: Value = self.api.get(&q).await?;
        Ok(LastFmResponseParser::parse_period_play_count(&json))
    }

    pub async fn fetch_top_tracks(
        &self,
        viewing_as: Option<&str>,
        period: &str,
        limit: usize,
    ) -> Result<Vec<HomeTrack>, LastFmApiError> {
        if let Some(extra) = self.active_extra_backend() {
            return Ok(extra.fetch_top_tracks(viewing_as, period, limit).await);
        }
        if self.active_kind.is_none() && viewing_as.is_none() {
            return Ok(Vec::new());
        }
        let Some(user) = self.effective_last_fm_user(viewing_as) else {
            return Ok(Vec::new());
        };
        let mut q = BTreeMap::new();
        q.insert("method".to_string(), "user.gettoptracks".to_string());
        q.insert("user".to_string(), user);
        q.insert("api_key".to_string(), self.api_key());
        q.insert("period".to_string(), period.to_string());
        q.insert("limit".to_string(), limit.to_string());
        let json = self.api.get(&q).await?;
        Ok(LastFmResponseParser::parse_top_tracks(&json))
    }

    pub async fn fetch_top_artists(
        &self,
        viewing_as: Option<&str>,
        period: &str,
        limit: usize,
    ) -> Vec<TopArtistItem> {
        if let Some(extra) = self.active_extra_backend() {
            return extra.fetch_top_artists(viewing_as, period, limit).await;
        }
        if self.active_kind.is_none() && viewing_as.is_none() {
            return Vec::new();
        }
        let Some(user) = self.effective_last_fm_user(viewing_as) else {
            return Vec::new();
        };
        let mut q = BTreeMap::new();
        q.insert("method".to_string(), "user.gettopartists".to_string());
        q.insert("user".to_string(), user);
        q.insert("api_key".to_string(), self.api_key());
        q.insert("period".to_string(), period.to_string());
        q.insert("limit".to_string(), limit.to_string());
        match self.api.get(&q).await {
            Ok(json) => LastFmResponseParser::parse_top_artists(&json),
            Err(_) => Vec::new(),
        }
    }

    pub async fn fetch_top_albums(
        &self,
        viewing_as: Option<&str>,
        period: &str,
        limit: usize,
    ) -> Vec<TopAlbumItem> {
        if let Some(extra) = self.active_extra_backend() {
            return extra.fetch_top_albums(viewing_as, period, limit).await;
        }
        if self.active_kind.is_none() && viewing_as.is_none() {
            return Vec::new();
        }
        let Some(user) = self.effective_last_fm_user(viewing_as) else {
            return Vec::new();
        };
        let mut q = BTreeMap::new();
        q.insert("method".to_string(), "user.gettopalbums".to_string());
        q.insert("user".to_string(), user);
        q.insert("api_key".to_string(), self.api_key());
        q.insert("period".to_string(), period.to_string());
        q.insert("limit".to_string(), limit.to_string());
        match self.api.get(&q).await {
            Ok(json) => LastFmResponseParser::parse_top_albums(&json),
            Err(_) => Vec::new(),
        }
    }
}
