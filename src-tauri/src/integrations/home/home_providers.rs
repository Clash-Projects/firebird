//! Home data providers: feed sections and new-release records load
//! independently with stale-while-revalidate snapshot caching.
//!
//! Ported from `lib/features/home/home_providers.dart`.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use crate::extractor::innertube_api::YouTubeMusicEntity;
use crate::integrations::feed::feed_repository::{FeedData, FeedRepository};
use crate::storage::app_database::AppDatabase;
use crate::storage::home_snapshot::{
    home_feed_snapshot_key, home_new_albums_snapshot_key, home_personal_mix_snapshot_key,
    no_personal_mix, swr_load, EntitiesSnapshotCodec, PersonalMixData, PersonalMixSnapshotCodec,
    SnapshotCodec, SwrState, HOME_CACHE_GROUP,
};
use crate::storage::prefs::Prefs;

const DEFAULT_HOME_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

struct FeedDataJsonCodec;

impl SnapshotCodec<FeedData> for FeedDataJsonCodec {
    fn encode(&self, value: &FeedData) -> String {
        serde_json::to_string(value).unwrap_or_default()
    }

    fn decode(&self, raw: &str) -> Option<FeedData> {
        serde_json::from_str(raw).ok()
    }

    fn is_empty(&self, value: &FeedData) -> bool {
        value.is_empty()
    }
}

pub async fn load_home_feed(
    repo: Arc<FeedRepository>,
    db: Arc<AppDatabase>,
    prefs: &Prefs,
    swr: Arc<SwrState>,
    charts_only: bool,
) -> FeedData {
    let key = home_feed_snapshot_key(&prefs.username());
    swr_load(
        swr,
        db,
        key.as_deref(),
        FeedDataJsonCodec,
        move || async move { Ok::<FeedData, Infallible>(repo.load_feed(charts_only).await) },
        HOME_CACHE_GROUP,
        DEFAULT_HOME_MAX_AGE,
        false,
        0,
        || {},
    )
    .await
    .unwrap_or_default()
}

pub async fn load_new_albums(
    repo: Arc<FeedRepository>,
    db: Arc<AppDatabase>,
    swr: Arc<SwrState>,
    connected: bool,
    profile_email: &str,
    active_page_id: &str,
) -> Vec<YouTubeMusicEntity> {
    let key = home_new_albums_snapshot_key(connected, profile_email, active_page_id);
    swr_load(
        swr,
        db,
        Some(&key),
        EntitiesSnapshotCodec,
        move || async move {
            Ok::<Vec<YouTubeMusicEntity>, Infallible>(repo.fetch_new_release_albums(60).await)
        },
        HOME_CACHE_GROUP,
        DEFAULT_HOME_MAX_AGE,
        false,
        0,
        || {},
    )
    .await
    .unwrap_or_default()
}

pub async fn load_personal_mix(
    repo: Arc<FeedRepository>,
    db: Arc<AppDatabase>,
    swr: Arc<SwrState>,
    connected: bool,
    profile_email: &str,
    active_page_id: &str,
) -> Option<PersonalMixData> {
    let key = home_personal_mix_snapshot_key(connected, profile_email, active_page_id);
    let mix = swr_load(
        swr,
        db,
        key.as_deref(),
        PersonalMixSnapshotCodec,
        move || async move {
            Ok::<PersonalMixData, Infallible>(
                repo.fetch_personal_mix(20)
                    .await
                    .unwrap_or_else(no_personal_mix),
            )
        },
        HOME_CACHE_GROUP,
        DEFAULT_HOME_MAX_AGE,
        false,
        0,
        || {},
    )
    .await
    .unwrap_or_else(|_| no_personal_mix());

    if mix.tracks.is_empty() {
        None
    } else {
        Some(mix)
    }
}
