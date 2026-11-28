use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::storage::app_database::AppDatabase;
use crate::storage::playlists::{PlaylistRepository, SavedPlaylist, StoredTrack};

use super::innertube_api::{
    InnerTubeMusicApi, YouTubeMusicTrack, YouTubePlaylistSummary, YtConnection,
    YtOwnedPlaylistItem,
};

const STATE_KEY: &str = "yt_library_sync_v1";
const LEGACY_KEY: &str = "yt_playlist_sync_v1";

pub const TICK_EVERY: Duration = Duration::from_secs(60);
pub const FULL_READ_EVERY_MS: i64 = 10 * 60 * 1000;
const DEBOUNCE_DELAY: Duration = Duration::from_millis(1500);
const GRACE_FOR_MS: i64 = 5 * 60 * 1000;
const MAX_LIKE_WRITES_PER_RUN: usize = 30;
const MAX_RESOLVES_PER_RUN: usize = 20;
const ROUND_ROBIN_PER_TICK: usize = 2;
const MISSES_BEFORE_REMOTE_DELETE: usize = 2;

pub fn track_key(name: &str, artist: &str) -> String {
    format!("{}|{}", name.to_lowercase(), artist.to_lowercase())
}

fn stored_from_yt(t: &YouTubeMusicTrack) -> StoredTrack {
    StoredTrack {
        name: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        artwork_url: t.artwork_url.clone(),
        video_id: t.video_id.clone(),
        duration_seconds: t.duration_seconds,
        playback_url: String::new(),
        added_at_millis: 0,
    }
}

pub fn base_entries(t: &StoredTrack) -> Vec<String> {
    let mut out = Vec::with_capacity(2);
    if !t.video_id.is_empty() {
        out.push(t.video_id.clone());
    }
    out.push(format!("k:{}", t.key()));
    out
}

/// What one three-way merge wants done. `next_base` is the agreed set
/// assuming remote writes fail; callers fold successful `add_remote` and
/// failed `remove_remote` entries in afterwards.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncPlan {
    pub add_local: Vec<StoredTrack>,
    pub remove_local: Vec<StoredTrack>,
    pub add_remote: Vec<StoredTrack>,
    pub remove_remote: Vec<StoredTrack>,
    pub next_base: HashSet<String>,
}

impl SyncPlan {
    pub fn removals(&self) -> usize {
        self.remove_local.len() + self.remove_remote.len()
    }
}

/// Three-way merge of a track list against the last agreed `base` state.
pub fn plan_sync(
    local: &[StoredTrack],
    remote: &[StoredTrack],
    base: &HashSet<String>,
    remote_complete: bool,
    grace: &HashSet<String>,
) -> SyncPlan {
    let local_ids: HashSet<&str> = local
        .iter()
        .filter(|t| !t.video_id.is_empty())
        .map(|t| t.video_id.as_str())
        .collect();
    let local_keys: HashSet<String> = local.iter().map(StoredTrack::key).collect();
    let remote_ids: HashSet<&str> = remote.iter().map(|t| t.video_id.as_str()).collect();
    let remote_keys: HashSet<String> = remote.iter().map(StoredTrack::key).collect();

    let in_base = |t: &StoredTrack| base_entries(t).iter().any(|e| base.contains(e));

    let mut next = HashSet::new();
    let mut add_local = Vec::new();
    let mut remove_local = Vec::new();
    let mut add_remote = Vec::new();
    let mut remove_remote = Vec::new();

    for r in remote {
        if (!r.video_id.is_empty() && local_ids.contains(r.video_id.as_str()))
            || local_keys.contains(&r.key())
        {
            next.extend(base_entries(r));
        } else if grace.contains(&r.video_id) {
            if in_base(r) {
                next.extend(base_entries(r));
            }
        } else if in_base(r) {
            remove_remote.push(r.clone());
        } else {
            add_local.push(r.clone());
            next.extend(base_entries(r));
        }
    }

    for l in local {
        if (!l.video_id.is_empty() && remote_ids.contains(l.video_id.as_str()))
            || remote_keys.contains(&l.key())
        {
            next.extend(base_entries(l));
        } else if grace.contains(&l.video_id) {
            if in_base(l) {
                next.extend(base_entries(l));
            }
        } else if in_base(l) {
            if remote_complete {
                remove_local.push(l.clone());
            } else {
                next.extend(base_entries(l));
            }
        } else if !l.video_id.is_empty() {
            add_remote.push(l.clone());
        }
    }

    SyncPlan {
        add_local,
        remove_local,
        add_remote,
        remove_remote,
        next_base: next,
    }
}

/// Whether a merge removes enough to look like a broken read or restored backup
/// rather than the user's own edits.
pub fn is_bulk_removal(removals: usize, list_size: usize) -> bool {
    removals > 25 && removals * 10 > list_size * 3
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtSyncStatus {
    pub running: bool,
    pub message: String,
    pub held_removals: usize,
    pub linked_remote_ids: HashSet<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistLink {
    #[serde(default)]
    remote_id: String,
    #[serde(default)]
    base: HashSet<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    remote_count: String,
    #[serde(default)]
    local_sig: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountSyncState {
    #[serde(default)]
    liked_base: HashSet<String>,
    #[serde(default)]
    liked_seeded: bool,
    #[serde(default)]
    links: HashMap<i64, PlaylistLink>,
    #[serde(default)]
    not_owned: HashSet<String>,
}

#[derive(Debug, Clone, Default)]
struct SyncState {
    last_account: Option<String>,
    accounts: HashMap<String, AccountSyncState>,
    legacy_remotes: HashMap<i64, String>,
}

impl SyncState {
    fn owned_by_other(&self, local_id: i64, key: &str) -> bool {
        self.accounts
            .iter()
            .any(|(k, v)| k != key && v.links.contains_key(&local_id))
    }

    fn legacy_remote_for(&self, local_id: i64) -> Option<String> {
        self.legacy_remotes.get(&local_id).cloned()
    }
}

#[derive(Default)]
struct RuntimeFlags {
    running: bool,
    rerun: bool,
    applying_local: bool,
    local_dirty: bool,
    bulk_approved: bool,
    force_full: bool,
    held: usize,
    cursor: usize,
    failures: usize,
    last_liked_full_ms: Option<i64>,
    liked_head: String,
    linked_ids: HashSet<String>,
    grace: HashMap<String, i64>,
    misses: HashMap<String, usize>,
    unresolvable: HashSet<String>,
}

pub type EnabledCheck = Arc<dyn Fn() -> bool + Send + Sync>;
pub type SyncStatusCallback = Arc<dyn Fn(YtSyncStatus) + Send + Sync>;
pub type SyncedCallback = Arc<dyn Fn(bool) + Send + Sync>;
pub type ClockFn = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Keeps Liked Songs and custom playlists synchronized between local SQLite
/// storage and YouTube Music.
pub struct YtLibrarySync {
    db: Arc<AppDatabase>,
    tube: Arc<InnerTubeMusicApi>,
    repo: Arc<PlaylistRepository>,
    enabled: EnabledCheck,
    on_status: Option<SyncStatusCallback>,
    on_synced: Mutex<Option<SyncedCallback>>,
    like_pacing: Duration,
    now_ms: ClockFn,
    rt: Mutex<RuntimeFlags>,
    disposed: AtomicBool,
    debounce_gen: AtomicU64,
    ticker_started: AtomicBool,
}

impl YtLibrarySync {
    pub fn new(
        db: Arc<AppDatabase>,
        tube: Arc<InnerTubeMusicApi>,
        repo: Arc<PlaylistRepository>,
        enabled: EnabledCheck,
        on_status: Option<SyncStatusCallback>,
        on_synced: Option<SyncedCallback>,
    ) -> Self {
        Self::with_options(
            db,
            tube,
            repo,
            enabled,
            on_status,
            on_synced,
            Duration::from_millis(150),
            Arc::new(|| Utc::now().timestamp_millis()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_options(
        db: Arc<AppDatabase>,
        tube: Arc<InnerTubeMusicApi>,
        repo: Arc<PlaylistRepository>,
        enabled: EnabledCheck,
        on_status: Option<SyncStatusCallback>,
        on_synced: Option<SyncedCallback>,
        like_pacing: Duration,
        now_ms: ClockFn,
    ) -> Self {
        let mut rt = RuntimeFlags::default();
        rt.local_dirty = true;
        Self {
            db,
            tube,
            repo,
            enabled,
            on_status,
            on_synced: Mutex::new(on_synced),
            like_pacing,
            now_ms,
            rt: Mutex::new(rt),
            disposed: AtomicBool::new(false),
            debounce_gen: AtomicU64::new(0),
            ticker_started: AtomicBool::new(false),
        }
    }

    pub fn bare_id(id: &str) -> &str {
        id.strip_prefix("VL").unwrap_or(id)
    }

    fn spawn_task<F>(fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(fut);
        } else {
            tauri::async_runtime::spawn(fut);
        }
    }

    pub fn start(self: &Arc<Self>) {
        if !self.ticker_started.swap(true, Ordering::AcqRel) {
            let me = Arc::clone(self);
            Self::spawn_task(async move {
                let mut interval = tokio::time::interval(TICK_EVERY);
                interval.tick().await;
                loop {
                    interval.tick().await;
                    if me.disposed.load(Ordering::Acquire) {
                        break;
                    }
                    me.run(false).await;
                }
            });
        }
        self.kick();
    }

    pub fn set_on_synced(&self, cb: Option<SyncedCallback>) {
        if let Ok(mut g) = self.on_synced.lock() {
            *g = cb;
        }
    }

    fn notify_synced(&self, playlists_changed: bool) {
        if self.disposed.load(Ordering::Acquire) {
            return;
        }
        let cb = self.on_synced.lock().ok().and_then(|g| g.clone());
        if let Some(cb) = cb {
            cb(playlists_changed);
        }
    }

    pub fn dispose(&self) {
        self.disposed.store(true, Ordering::Release);
    }

    pub fn kick(self: &Arc<Self>) {
        let me = Arc::clone(self);
        Self::spawn_task(async move {
            me.run(true).await;
        });
    }

    pub fn on_local_changed(self: &Arc<Self>) {
        if self.disposed.load(Ordering::Acquire) {
            return;
        }
        {
            let Ok(mut g) = self.rt.lock() else {
                return;
            };
            if g.applying_local {
                return;
            }
            g.local_dirty = true;
        }
        let gen = self.debounce_gen.fetch_add(1, Ordering::AcqRel) + 1;
        let me = Arc::clone(self);
        Self::spawn_task(async move {
            tokio::time::sleep(DEBOUNCE_DELAY).await;
            if me.disposed.load(Ordering::Acquire)
                || me.debounce_gen.load(Ordering::Acquire) != gen
            {
                return;
            }
            me.run(false).await;
        });
    }

    pub fn apply_held_removals(self: &Arc<Self>) {
        if let Ok(mut g) = self.rt.lock() {
            g.bulk_approved = true;
        }
        self.kick();
    }

    fn set_applying_local(&self, v: bool) {
        if let Ok(mut g) = self.rt.lock() {
            g.applying_local = v;
        }
    }

    fn active_grace(&self) -> HashSet<String> {
        let now = (self.now_ms)();
        let Ok(mut g) = self.rt.lock() else {
            return HashSet::new();
        };
        g.grace.retain(|_, at| now.saturating_sub(*at) <= GRACE_FOR_MS);
        g.grace.keys().cloned().collect()
    }

    fn mark_written(&self, video_id: &str) {
        let now = (self.now_ms)();
        if let Ok(mut g) = self.rt.lock() {
            g.grace.insert(video_id.to_string(), now);
        }
    }

    fn publish(&self, running: bool, message: &str) {
        if self.disposed.load(Ordering::Acquire) {
            return;
        }
        let (held, linked_ids) = self
            .rt
            .lock()
            .map(|g| (g.held, g.linked_ids.clone()))
            .unwrap_or_default();
        if let Some(cb) = &self.on_status {
            cb(YtSyncStatus {
                running,
                message: message.to_string(),
                held_removals: held,
                linked_remote_ids: linked_ids,
            });
        }
    }

    pub async fn run(self: &Arc<Self>, force: bool) {
        let mut next_force = force;
        loop {
            let rerun = self.run_once(next_force).await;
            if !rerun || self.disposed.load(Ordering::Acquire) {
                break;
            }
            let urgent = self.rt.lock().map(|g| g.force_full).unwrap_or(false);
            if !urgent {
                let gen = self.debounce_gen.fetch_add(1, Ordering::AcqRel) + 1;
                tokio::time::sleep(Duration::from_secs(5)).await;
                if self.disposed.load(Ordering::Acquire)
                    || self.debounce_gen.load(Ordering::Acquire) != gen
                {
                    break;
                }
            }
            next_force = urgent;
        }
    }

    async fn run_once(&self, force: bool) -> bool {
        {
            let Ok(mut g) = self.rt.lock() else {
                return false;
            };
            if force {
                g.force_full = true;
            }
            if self.disposed.load(Ordering::Acquire)
                || !(self.enabled)()
                || !self.tube.connection().connected
            {
                return false;
            }
            if g.running {
                g.rerun = true;
                return false;
            }
            g.running = true;
        }

        self.publish(true, "");

        let (force_flag, local_dirty) = {
            let mut g = self.rt.lock().unwrap();
            let f = g.force_full;
            let ld = g.local_dirty;
            g.force_full = false;
            g.local_dirty = false;
            g.held = 0;
            g.failures = 0;
            (f, ld)
        };

        let mut state = self.load_state();
        let conn = self.tube.connection();
        let key = Self::account_key(&conn);
        Self::adopt_legacy_empty_email_key(&mut state, &key);
        state.accounts.entry(key.clone()).or_default();
        let switched_stripped = self.handle_account_switch(&mut state, &key);
        state.last_account = Some(key.clone());
        if switched_stripped {
            self.save_state(&state);
            self.notify_synced(false);
        }

        let mut account = state.accounts.remove(&key).unwrap_or_default();
        let more_likes = self
            .sync_liked(&mut account, force_flag || local_dirty)
            .await;
        if Self::account_key(&self.tube.connection()) != key {
            if let Ok(mut g) = self.rt.lock() {
                g.running = false;
                g.force_full = true;
                g.rerun = true;
            }
            self.publish(false, "");
            return true;
        }
        let playlists_changed = self
            .sync_playlists(&state, &mut account, &key, force_flag || local_dirty)
            .await;
        if Self::account_key(&self.tube.connection()) != key {
            if let Ok(mut g) = self.rt.lock() {
                g.running = false;
                g.force_full = true;
                g.rerun = true;
            }
            self.publish(false, "");
            return true;
        }

        let linked_ids: HashSet<String> = account
            .links
            .values()
            .map(|l| Self::bare_id(&l.remote_id).to_string())
            .collect();
        state.accounts.insert(key, account);
        self.save_state(&state);

        let (held, failures, rerun) = {
            let mut g = self.rt.lock().unwrap();
            g.linked_ids = linked_ids;
            if more_likes {
                g.local_dirty = true;
                g.rerun = true;
            }
            if g.bulk_approved {
                g.bulk_approved = false;
            }
            let rerun = g.rerun;
            g.rerun = false;
            g.running = false;
            (g.held, g.failures, rerun)
        };

        let message = if held > 0 {
            format!("Held back {held} removals because that is a lot at once. Nothing was deleted.")
        } else if failures > 0 {
            "Some changes didn't reach YouTube Music. Retrying.".to_string()
        } else {
            "Up to date.".to_string()
        };

        self.publish(false, &message);
        self.notify_synced(playlists_changed);

        rerun
    }

    async fn sync_liked(&self, account: &mut AccountSyncState, force: bool) -> bool {
        let now = (self.now_ms)();
        let (due, last_head) = self
            .rt
            .lock()
            .map(|g| {
                let d = g
                    .last_liked_full_ms
                    .is_none_or(|prev| now.saturating_sub(prev) >= FULL_READ_EVERY_MS);
                (d, g.liked_head.clone())
            })
            .unwrap_or((true, String::new()));

        if !force && !due {
            let probe = self.tube.fetch_owned_playlist_items("LM", true).await;
            if probe.items.is_empty() || Self::head_of(&probe.items) == last_head {
                return false;
            }
        }

        let owned = self.tube.fetch_owned_playlist_items("LM", false).await;
        if owned.items.is_empty() && !owned.complete && account.liked_seeded {
            if let Ok(mut g) = self.rt.lock() {
                g.failures += 1;
            }
            return false;
        }
        let head = Self::head_of(&owned.items);
        if let Ok(mut g) = self.rt.lock() {
            g.last_liked_full_ms = Some(now);
            g.liked_head = head;
        }

        let mut liked = self.repo.ensure_liked_songs();
        liked = self.resolve_missing_ids(liked).await;
        let remote: Vec<StoredTrack> = owned
            .items
            .iter()
            .filter_map(|item| item.track.as_ref().map(stored_from_yt))
            .collect();

        let plan = plan_sync(
            &liked.tracks,
            &remote,
            &account.liked_base,
            owned.complete,
            &self.active_grace(),
        );
        let bulk_approved = self.rt.lock().map(|g| g.bulk_approved).unwrap_or(false);
        let max_len = liked.tracks.len().max(remote.len());
        let hold = !bulk_approved && is_bulk_removal(plan.removals(), max_len);

        let mut next = plan.next_base.clone();
        if hold {
            if let Ok(mut g) = self.rt.lock() {
                g.held += plan.removals();
            }
            for t in plan.remove_local.iter().chain(plan.remove_remote.iter()) {
                next.extend(base_entries(t));
            }
        }

        let remove_keys: HashSet<String> = if hold {
            HashSet::new()
        } else {
            plan.remove_local.iter().map(StoredTrack::key).collect()
        };
        if !plan.add_local.is_empty() || !remove_keys.is_empty() {
            self.set_applying_local(true);
            self.repo.apply_batch(liked.id, &plan.add_local, &remove_keys);
            self.set_applying_local(false);
            self.notify_synced(false);
        }

        let mut writes = 0usize;
        let mut queued = false;

        for t in &plan.add_remote {
            if writes >= MAX_LIKE_WRITES_PER_RUN {
                queued = true;
                break;
            }
            if writes > 0 && !self.like_pacing.is_zero() {
                tokio::time::sleep(self.like_pacing).await;
            }
            writes += 1;
            if self.tube.like_video(&t.video_id).await {
                next.extend(base_entries(t));
                self.mark_written(&t.video_id);
            } else if let Ok(mut g) = self.rt.lock() {
                g.failures += 1;
            }
        }

        if !hold {
            for t in &plan.remove_remote {
                if writes >= MAX_LIKE_WRITES_PER_RUN {
                    queued = true;
                    next.extend(base_entries(t));
                    continue;
                }
                if writes > 0 && !self.like_pacing.is_zero() {
                    tokio::time::sleep(self.like_pacing).await;
                }
                writes += 1;
                if self.tube.remove_video_like(&t.video_id).await {
                    self.mark_written(&t.video_id);
                } else {
                    if let Ok(mut g) = self.rt.lock() {
                        g.failures += 1;
                    }
                    next.extend(base_entries(t));
                }
            }
        }

        account.liked_base = next;
        account.liked_seeded = true;
        queued
    }

    fn head_of(items: &[YtOwnedPlaylistItem]) -> String {
        items
            .iter()
            .take(30)
            .map(|i| i.video_id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    async fn resolve_missing_ids(&self, playlist: SavedPlaylist) -> SavedPlaylist {
        let mut budget = MAX_RESOLVES_PER_RUN;
        let mut changed = false;
        let mut tracks = Vec::with_capacity(playlist.tracks.len());
        let empty = HashSet::new();

        for t in &playlist.tracks {
            let unresolvable = self
                .rt
                .lock()
                .map(|g| g.unresolvable.contains(&t.key()))
                .unwrap_or(false);
            if !t.video_id.is_empty() || !t.playback_url.is_empty() || budget == 0 || unresolvable {
                tracks.push(t.clone());
                continue;
            }
            budget -= 1;
            let hit = self
                .tube
                .find_best_match_or_null(&t.name, &t.artist, &empty)
                .await;
            let Some(m) = hit.filter(|h| !h.video_id.is_empty()) else {
                if let Ok(mut g) = self.rt.lock() {
                    g.unresolvable.insert(t.key());
                }
                tracks.push(t.clone());
                continue;
            };
            changed = true;
            let mut updated = t.clone();
            updated.video_id = m.video_id;
            tracks.push(updated);
        }

        if !changed {
            return playlist;
        }
        self.set_applying_local(true);
        self.repo.replace_tracks(playlist.id, &tracks);
        self.set_applying_local(false);
        self.repo
            .all()
            .into_iter()
            .find(|p| p.id == playlist.id)
            .unwrap_or(playlist)
    }

    fn mirrorable(p: &SavedPlaylist) -> bool {
        p.mode == "custom" && !p.is_liked_songs() && !p.is_local()
    }

    fn local_sig(p: &SavedPlaylist) -> String {
        let ids: Vec<String> = p
            .tracks
            .iter()
            .map(|t| {
                if t.video_id.is_empty() {
                    t.key()
                } else {
                    t.video_id.clone()
                }
            })
            .collect();
        format!("{}\n{}", p.title, ids.join(","))
    }

    async fn sync_playlists(
        &self,
        state: &SyncState,
        account: &mut AccountSyncState,
        key: &str,
        force: bool,
    ) -> bool {
        let Some(summaries) = self.tube.fetch_account_playlists_or_null().await else {
            if let Ok(mut g) = self.rt.lock() {
                g.failures += 1;
            }
            return false;
        };

        let mut changed = false;
        let rows_by_id: HashMap<i64, SavedPlaylist> =
            self.repo.all().into_iter().map(|p| (p.id, p)).collect();
        let any_mirrorable = rows_by_id.values().any(Self::mirrorable);

        let existing_link_ids: Vec<i64> = account.links.keys().copied().collect();
        if !any_mirrorable {
            for id in existing_link_ids {
                if !rows_by_id.contains_key(&id) {
                    account.links.remove(&id);
                    changed = true;
                }
            }
        } else {
            for id in existing_link_ids {
                if rows_by_id.contains_key(&id) {
                    continue;
                }
                let remote_id = account.links.get(&id).map(|l| l.remote_id.clone()).unwrap_or_default();
                if self.tube.delete_remote_playlist(&remote_id).await {
                    account.links.remove(&id);
                    changed = true;
                } else if let Ok(mut g) = self.rt.lock() {
                    g.failures += 1;
                }
            }
        }

        let by_bare_id: HashMap<String, &YouTubePlaylistSummary> = summaries
            .iter()
            .map(|s| (Self::bare_id(&s.id).to_string(), s))
            .collect();
        let mut linked_remote: HashSet<String> = account
            .links
            .values()
            .map(|l| Self::bare_id(&l.remote_id).to_string())
            .collect();
        let mut fresh = HashSet::new();

        for row in rows_by_id.values().filter(|p| Self::mirrorable(p)) {
            if account.links.contains_key(&row.id) || state.owned_by_other(row.id, key) {
                continue;
            }
            let mut remote: Option<String> = None;
            for s in &summaries {
                let bare = Self::bare_id(&s.id).to_string();
                if linked_remote.contains(&bare)
                    || account.not_owned.contains(&bare)
                    || !s.title.eq_ignore_ascii_case(&row.title)
                {
                    continue;
                }
                let probe = self.tube.fetch_owned_playlist_items(&bare, true).await;
                if probe.editable {
                    remote = Some(bare);
                    break;
                }
                account.not_owned.insert(bare);
            }
            if remote.is_none() {
                remote = state.legacy_remote_for(row.id);
            }
            let remote_id = match remote {
                Some(r) => r,
                None => {
                    let Some(created) = self
                        .tube
                        .create_remote_playlist(&row.title)
                        .await
                        .filter(|s| !s.is_empty())
                    else {
                        if let Ok(mut g) = self.rt.lock() {
                            g.failures += 1;
                        }
                        continue;
                    };
                    Self::bare_id(&created).to_string()
                }
            };
            account.links.insert(
                row.id,
                PlaylistLink {
                    remote_id: remote_id.clone(),
                    title: row.title.clone(),
                    ..Default::default()
                },
            );
            linked_remote.insert(remote_id);
            fresh.insert(row.id);
            changed = true;
        }

        for s in &summaries {
            let bare = Self::bare_id(&s.id).to_string();
            if !bare.starts_with("PL")
                || linked_remote.contains(&bare)
                || account.not_owned.contains(&bare)
            {
                continue;
            }
            let lowered = s.title.to_lowercase();
            if lowered == SavedPlaylist::LIKED_TITLE.to_lowercase() || lowered == "liked music" {
                continue;
            }
            let owned = self.tube.fetch_owned_playlist_items(&bare, false).await;
            if !owned.editable {
                account.not_owned.insert(bare);
                continue;
            }
            if !owned.complete {
                continue;
            }
            let tracks: Vec<StoredTrack> = owned
                .items
                .iter()
                .filter_map(|item| item.track.as_ref().map(stored_from_yt))
                .collect();
            let empty_rm = HashSet::new();
            self.set_applying_local(true);
            let row = self.repo.create_custom(&s.title);
            self.repo.apply_batch(row.id, &tracks, &empty_rm);
            self.set_applying_local(false);

            let mut base = HashSet::new();
            for t in &tracks {
                base.extend(base_entries(t));
            }
            let added = self
                .repo
                .all()
                .into_iter()
                .find(|p| p.id == row.id)
                .unwrap_or(row.clone());
            account.links.insert(
                row.id,
                PlaylistLink {
                    remote_id: bare.clone(),
                    base,
                    title: s.title.clone(),
                    remote_count: s.track_count_text.clone(),
                    local_sig: Self::local_sig(&added),
                },
            );
            linked_remote.insert(bare);
            changed = true;
        }

        let mut ids: Vec<i64> = account.links.keys().copied().collect();
        ids.sort_unstable();
        if ids.is_empty() {
            return changed;
        }

        let round_robin: HashSet<i64> = {
            let mut g = self.rt.lock().unwrap();
            let mut set = HashSet::new();
            let count = ROUND_ROBIN_PER_TICK.min(ids.len());
            for i in 0..count {
                set.insert(ids[(g.cursor + i) % ids.len()]);
            }
            g.cursor = (g.cursor + ROUND_ROBIN_PER_TICK) % ids.len();
            set
        };

        for id in ids {
            let all_now = self.repo.all();
            let Some(row) = all_now.iter().find(|p| p.id == id).cloned() else {
                continue;
            };
            let Some(link_remote_id) = account.links.get(&id).map(|l| l.remote_id.clone()) else {
                continue;
            };
            let summary = by_bare_id.get(Self::bare_id(&link_remote_id)).copied();
            let just_created = fresh.contains(&id);

            if summary.is_none() && !just_created {
                if self
                    .handle_missing_remote(account, id, &link_remote_id, &summaries)
                    .await
                {
                    changed = true;
                }
                continue;
            }
            if let Ok(mut g) = self.rt.lock() {
                g.misses.remove(&link_remote_id);
            }

            if let Some(sum) = summary {
                let link_title = account
                    .links
                    .get(&id)
                    .map(|l| l.title.clone())
                    .unwrap_or_default();
                if sum.title != link_title && row.title == link_title {
                    self.set_applying_local(true);
                    self.repo.rename(id, &sum.title);
                    self.set_applying_local(false);
                    if let Some(l) = account.links.get_mut(&id) {
                        l.title = sum.title.clone();
                    }
                }
            }

            let current = self
                .repo
                .all()
                .into_iter()
                .find(|p| p.id == id)
                .unwrap_or(row);
            let current_link_title = account
                .links
                .get(&id)
                .map(|l| l.title.clone())
                .unwrap_or_default();
            if current.title != current_link_title {
                if self
                    .tube
                    .rename_remote_playlist(&link_remote_id, &current.title)
                    .await
                {
                    if let Some(l) = account.links.get_mut(&id) {
                        l.title = current.title.clone();
                    }
                } else if let Ok(mut g) = self.rt.lock() {
                    g.failures += 1;
                }
            }

            let sig = Self::local_sig(&current);
            let count = summary
                .map(|s| s.track_count_text.clone())
                .unwrap_or_default();
            let needs_read = {
                let Some(l) = account.links.get(&id) else {
                    continue;
                };
                force
                    || just_created
                    || round_robin.contains(&id)
                    || sig != l.local_sig
                    || count != l.remote_count
            };
            if !needs_read {
                continue;
            }
            if let Some(link) = account.links.get_mut(&id) {
                self.sync_pair(&current, link).await;
                link.remote_count = count;
            }
        }
        changed
    }

    async fn handle_missing_remote(
        &self,
        account: &mut AccountSyncState,
        id: i64,
        remote_id: &str,
        summaries: &[YouTubePlaylistSummary],
    ) -> bool {
        if summaries.is_empty() {
            return false;
        }
        let misses = {
            let mut g = self.rt.lock().unwrap();
            let m = g.misses.get(remote_id).copied().unwrap_or(0) + 1;
            g.misses.insert(remote_id.to_string(), m);
            m
        };
        if misses < MISSES_BEFORE_REMOTE_DELETE {
            return false;
        }
        let direct = self.tube.fetch_owned_playlist_items(remote_id, true).await;
        if direct.editable {
            if let Ok(mut g) = self.rt.lock() {
                g.misses.remove(remote_id);
            }
            return false;
        }
        self.set_applying_local(true);
        self.repo.delete(id);
        self.set_applying_local(false);
        account.links.remove(&id);
        if let Ok(mut g) = self.rt.lock() {
            g.misses.remove(remote_id);
        }
        true
    }

    async fn sync_pair(&self, row: &SavedPlaylist, link: &mut PlaylistLink) {
        let owned = self
            .tube
            .fetch_owned_playlist_items(&link.remote_id, false)
            .await;
        if !owned.editable {
            if let Ok(mut g) = self.rt.lock() {
                g.failures += 1;
            }
            return;
        }
        let resolved = self.resolve_missing_ids(row.clone()).await;
        let remote: Vec<StoredTrack> = owned
            .items
            .iter()
            .filter_map(|item| item.track.as_ref().map(stored_from_yt))
            .collect();

        let plan = plan_sync(
            &resolved.tracks,
            &remote,
            &link.base,
            owned.complete,
            &self.active_grace(),
        );
        let size = resolved.tracks.len().max(remote.len());
        let bulk_approved = self.rt.lock().map(|g| g.bulk_approved).unwrap_or(false);
        let hold = !bulk_approved && is_bulk_removal(plan.removals(), size);

        let mut next = plan.next_base.clone();
        if hold {
            if let Ok(mut g) = self.rt.lock() {
                g.held += plan.removals();
            }
            for t in plan.remove_local.iter().chain(plan.remove_remote.iter()) {
                next.extend(base_entries(t));
            }
        }

        let remove_keys: HashSet<String> = if hold {
            HashSet::new()
        } else {
            plan.remove_local.iter().map(StoredTrack::key).collect()
        };
        self.set_applying_local(true);
        self.repo.apply_batch(row.id, &plan.add_local, &remove_keys);
        self.set_applying_local(false);

        if !plan.add_remote.is_empty() {
            let add_ids: Vec<String> = plan
                .add_remote
                .iter()
                .map(|t| t.video_id.clone())
                .collect();
            if self
                .tube
                .add_videos_to_remote_playlist(&link.remote_id, &add_ids)
                .await
            {
                for t in &plan.add_remote {
                    next.extend(base_entries(t));
                    self.mark_written(&t.video_id);
                }
            } else if let Ok(mut g) = self.rt.lock() {
                g.failures += 1;
            }
        }

        if !hold && !plan.remove_remote.is_empty() {
            let doomed: HashSet<&str> = plan
                .remove_remote
                .iter()
                .map(|t| t.video_id.as_str())
                .collect();
            let removals: Vec<(String, String)> = owned
                .items
                .iter()
                .filter(|item| {
                    doomed.contains(item.video_id.as_str()) && !item.set_video_id.is_empty()
                })
                .map(|item| (item.set_video_id.clone(), item.video_id.clone()))
                .collect();
            let ok = !removals.is_empty()
                && self
                    .tube
                    .remove_videos_from_remote_playlist(&link.remote_id, &removals)
                    .await;
            if ok {
                for vid in doomed {
                    self.mark_written(vid);
                }
            } else {
                if let Ok(mut g) = self.rt.lock() {
                    g.failures += 1;
                }
                for t in &plan.remove_remote {
                    next.extend(base_entries(t));
                }
            }
        }

        link.base = next;
        if let Some(updated_row) = self.repo.all().into_iter().find(|p| p.id == row.id) {
            link.local_sig = Self::local_sig(&updated_row);
        }
    }

    fn account_key(c: &YtConnection) -> String {
        let who = if !c.profile_email.is_empty() {
            &c.profile_email
        } else {
            &c.account_name
        };
        format!("{who}|{}", c.active_page_id)
    }

    fn adopt_legacy_empty_email_key(state: &mut SyncState, key: &str) {
        let Some(last) = state.last_account.clone() else {
            return;
        };
        if last == key || !last.starts_with('|') {
            return;
        }
        let key_suffix = key.split_once('|').map(|(_, s)| s).unwrap_or("");
        if &last[1..] != key_suffix {
            return;
        }
        if let Some(old) = state.accounts.remove(&last) {
            state.accounts.entry(key.to_string()).or_insert(old);
        }
        state.last_account = Some(key.to_string());
    }

    fn handle_account_switch(&self, state: &mut SyncState, key: &str) -> bool {
        let Some(last) = state.last_account.clone() else {
            return false;
        };
        if last == key {
            return false;
        }
        let prev_liked_base = state
            .accounts
            .get(&last)
            .map(|p| p.liked_base.clone())
            .unwrap_or_default();
        let liked = self.repo.ensure_liked_songs();
        let strip: HashSet<String> = liked
            .tracks
            .iter()
            .filter(|t| {
                base_entries(t).iter().any(|e| prev_liked_base.contains(e))
                    || (!t.video_id.is_empty() && t.playback_url.is_empty())
            })
            .map(StoredTrack::key)
            .collect();
        let stripped = !strip.is_empty();
        if stripped {
            self.set_applying_local(true);
            self.repo.apply_batch(liked.id, &[], &strip);
            self.set_applying_local(false);
        }
        if let Some(previous) = state.accounts.get_mut(&last) {
            previous.liked_base.clear();
            previous.liked_seeded = false;
        }
        if let Some(incoming) = state.accounts.get_mut(key) {
            incoming.liked_base.clear();
            incoming.liked_seeded = false;
        }
        if let Ok(mut g) = self.rt.lock() {
            g.liked_head.clear();
            g.last_liked_full_ms = None;
        }
        stripped
    }

    fn load_state(&self) -> SyncState {
        let mut state = SyncState::default();
        if let Some(raw) = self.db.kv_get(STATE_KEY).filter(|s| !s.is_empty()) {
            if let Ok(decoded) = serde_json::from_str::<Value>(&raw) {
                if let Some(obj) = decoded.as_object() {
                    state.last_account = obj
                        .get("lastAccount")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    if let Some(accts) = obj.get("accounts").and_then(Value::as_object) {
                        for (k, v) in accts {
                            if let Ok(parsed) = serde_json::from_value::<AccountSyncState>(v.clone()) {
                                state.accounts.insert(k.clone(), parsed);
                            }
                        }
                    }
                }
            }
        }
        if let Some(legacy) = self.db.kv_get(LEGACY_KEY).filter(|s| !s.is_empty()) {
            if let Ok(decoded) = serde_json::from_str::<Value>(&legacy) {
                if let Some(maps) = decoded.get("mappings").and_then(Value::as_object) {
                    for (k, v) in maps {
                        if let Ok(id) = k.parse::<i64>() {
                            let remote = v
                                .get("remoteId")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            if !remote.is_empty() {
                                state
                                    .legacy_remotes
                                    .insert(id, Self::bare_id(remote).to_string());
                            }
                        }
                    }
                }
            }
        }
        state
    }

    fn save_state(&self, state: &SyncState) {
        let encoded = json!({
            "lastAccount": state.last_account,
            "accounts": state.accounts,
        })
        .to_string();
        if self.db.kv_get(STATE_KEY).as_deref() != Some(encoded.as_str()) {
            self.db.kv_set(STATE_KEY, &encoded);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr(name: &str, artist: &str, vid: &str) -> StoredTrack {
        StoredTrack {
            name: name.to_string(),
            artist: artist.to_string(),
            video_id: vid.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn plan_sync_handles_three_way_add_remove_and_grace() {
        let local = vec![
            tr("Kept", "Artist", "vid_kept"),
            tr("Local New", "Artist", "vid_lnew"),
            tr("Grace Wait", "Artist", "vid_grace"),
        ];
        let remote = vec![
            tr("Kept", "Artist", "vid_kept"),
            tr("Remote New", "Artist", "vid_rnew"),
            tr("Deleted Locally", "Artist", "vid_del_l"),
        ];
        let mut base = HashSet::new();
        base.extend(base_entries(&tr("Kept", "Artist", "vid_kept")));
        base.extend(base_entries(&tr("Deleted Locally", "Artist", "vid_del_l")));
        base.extend(base_entries(&tr("Deleted Remotely", "Artist", "vid_del_r")));

        let local_with_deleted_remotely = vec![
            local[0].clone(),
            local[1].clone(),
            local[2].clone(),
            tr("Deleted Remotely", "Artist", "vid_del_r"),
        ];
        let mut grace = HashSet::new();
        grace.insert("vid_grace".to_string());

        let plan = plan_sync(&local_with_deleted_remotely, &remote, &base, true, &grace);
        assert_eq!(plan.add_local.len(), 1);
        assert_eq!(plan.add_local[0].video_id, "vid_rnew");
        assert_eq!(plan.remove_local.len(), 1);
        assert_eq!(plan.remove_local[0].video_id, "vid_del_r");
        assert_eq!(plan.add_remote.len(), 1);
        assert_eq!(plan.add_remote[0].video_id, "vid_lnew");
        assert_eq!(plan.remove_remote.len(), 1);
        assert_eq!(plan.remove_remote[0].video_id, "vid_del_l");
    }

    #[test]
    fn bulk_removal_guard_respects_threshold() {
        assert!(!is_bulk_removal(25, 30));
        assert!(!is_bulk_removal(26, 100));
        assert!(is_bulk_removal(26, 80));
    }

    #[test]
    fn account_switch_strips_previous_youtube_likes_and_keeps_local_files() {
        let dir = std::env::temp_dir().join(format!(
            "firebird_sync_switch_test_{}",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&dir);
        let db = Arc::new(AppDatabase::open(&dir));
        let tube = Arc::new(InnerTubeMusicApi::with_defaults().expect("api"));
        let repo = Arc::new(PlaylistRepository::new(Arc::clone(&db)));
        let liked = repo.ensure_liked_songs();
        let remote_track = tr("Primary Song", "Primary Artist", "vid_primary");
        let local_track = StoredTrack {
            name: "Local File".to_string(),
            artist: "Local Artist".to_string(),
            playback_url: "C:\\Music\\song.flac".to_string(),
            ..Default::default()
        };
        repo.replace_tracks(liked.id, &[remote_track.clone(), local_track.clone()]);

        let sync = YtLibrarySync::new(
            Arc::clone(&db),
            tube,
            Arc::clone(&repo),
            Arc::new(|| true),
            None,
            None,
        );
        let mut state = SyncState {
            last_account: Some("user@example.com|".to_string()),
            accounts: HashMap::from([("user@example.com|".to_string(), AccountSyncState::default())]),
            legacy_remotes: HashMap::new(),
        };

        let stripped = sync.handle_account_switch(&mut state, "user@example.com|108456789012345678901");
        assert!(stripped);
        let updated = repo.ensure_liked_songs();
        assert_eq!(updated.tracks.len(), 1);
        assert_eq!(updated.tracks[0].name, "Local File");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
