use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::models::player_state::PlayerSnapshot;
use crate::models::stream_models::RepeatMode;

pub const BUS_NAME: &str = "org.mpris.MediaPlayer2.lastwave";
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
pub const ROOT_IFACE: &str = "org.mpris.MediaPlayer2";
pub const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
pub const NO_TRACK_PATH: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

pub const MIN_RATE: f64 = 0.25;
pub const MAX_RATE: f64 = 4.0;
pub const SEEK_THRESHOLD: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq)]
pub enum MprisAction {
    Next,
    Previous,
    Pause,
    PlayPause,
    Stop,
    Play,
    SeekTo(Duration),
    SetVolume(f64),
    ToggleShuffle,
    CycleRepeat,
    SetSpeed(f64),
    Raise,
}

/// Callback target bridging MPRIS commands back into the playback engine and window.
pub trait MprisControlHandler: Send + Sync {
    fn handle(&self, action: MprisAction);
    fn live_repeat_mode(&self) -> RepeatMode;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MprisMetadata {
    pub track_id: String,
    pub length_us: Option<i64>,
    pub art_url: Option<String>,
    pub title: String,
    pub artist: Vec<String>,
    pub album: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MprisDiff {
    pub changed_metadata: bool,
    pub changed_playback_status: bool,
    pub changed_loop_status: bool,
    pub changed_shuffle: bool,
    pub changed_rate: bool,
    pub changed_volume: bool,
    pub seeked_us: Option<i64>,
}

impl MprisDiff {
    pub fn any_property_changed(&self) -> bool {
        self.changed_metadata
            || self.changed_playback_status
            || self.changed_loop_status
            || self.changed_shuffle
            || self.changed_rate
            || self.changed_volume
    }
}

pub fn playback_status_for(snap: &PlayerSnapshot) -> &'static str {
    if snap.current.is_none() {
        "Stopped"
    } else if snap.is_playing {
        "Playing"
    } else {
        "Paused"
    }
}

pub fn loop_status_for(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "None",
        RepeatMode::All => "Playlist",
        RepeatMode::One => "Track",
    }
}

/// Shared MPRIS snapshot state, track-ID sequencer, and property diffing.
pub struct MprisState {
    pub snap: PlayerSnapshot,
    pub track_id: String,
    track_key: String,
    track_seq: u64,
}

impl Default for MprisState {
    fn default() -> Self {
        Self {
            snap: PlayerSnapshot::default(),
            track_id: NO_TRACK_PATH.to_string(),
            track_key: String::new(),
            track_seq: 0,
        }
    }
}

impl MprisState {
    pub fn metadata(&self) -> MprisMetadata {
        match &self.snap.current {
            None => MprisMetadata {
                track_id: NO_TRACK_PATH.to_string(),
                length_us: None,
                art_url: None,
                title: String::new(),
                artist: Vec::new(),
                album: None,
            },
            Some(t) => {
                let dur_us = if self.snap.duration_ms > 0 {
                    Some((self.snap.duration_ms as i64) * 1000)
                } else {
                    None
                };
                MprisMetadata {
                    track_id: self.track_id.clone(),
                    length_us: dur_us,
                    art_url: (!t.artwork_url.is_empty()).then(|| t.artwork_url.clone()),
                    title: t.title.clone(),
                    artist: if t.artist.is_empty() {
                        Vec::new()
                    } else {
                        vec![t.artist.clone()]
                    },
                    album: (!t.album.is_empty()).then(|| t.album.clone()),
                }
            }
        }
    }

    pub fn apply(&mut self, next: PlayerSnapshot) -> MprisDiff {
        let prev = std::mem::replace(&mut self.snap, next);
        let key = self
            .snap
            .current
            .as_ref()
            .map(|t| t.media_id())
            .unwrap_or_default();
        if key != self.track_key {
            self.track_key = key.clone();
            if key.is_empty() {
                self.track_id = NO_TRACK_PATH.to_string();
            } else {
                self.track_seq += 1;
                self.track_id = format!("/org/lastwave/track/{}", self.track_seq);
            }
        }

        let prev_id = prev.current.as_ref().map(|t| t.media_id());
        let next_id = self.snap.current.as_ref().map(|t| t.media_id());
        let prev_art = prev.current.as_ref().map(|t| t.artwork_url.as_str());
        let next_art = self.snap.current.as_ref().map(|t| t.artwork_url.as_str());

        let changed_metadata =
            prev_id != next_id || prev.duration_ms != self.snap.duration_ms || prev_art != next_art;
        let changed_playback_status = prev.is_playing != self.snap.is_playing
            || prev.current.is_none() != self.snap.current.is_none();
        let changed_loop_status = prev.repeat_mode != self.snap.repeat_mode;
        let changed_shuffle = prev.shuffle_enabled != self.snap.shuffle_enabled;
        let changed_rate = (prev.speed - self.snap.speed).abs() > f64::EPSILON;
        let changed_volume = (prev.volume - self.snap.volume).abs() > 0.001;

        let delta = self.snap.position().abs_diff(prev.position());
        let seeked_us = if prev_id == next_id
            && delta > SEEK_THRESHOLD
            && prev.is_playing == self.snap.is_playing
        {
            Some((self.snap.position_ms as i64) * 1000)
        } else {
            None
        };

        MprisDiff {
            changed_metadata,
            changed_playback_status,
            changed_loop_status,
            changed_shuffle,
            changed_rate,
            changed_volume,
            seeked_us,
        }
    }
}

/// MPRIS (`org.mpris.MediaPlayer2`) endpoint for Linux desktops.
///
/// Exposes the current track and transport controls on the session bus so
/// status bars, `playerctl`, media keys and lock screens can see and drive
/// LastWave. Linux only; on other OSes this is a safe no-op stub while still
/// keeping snapshot state tracking testable everywhere.
pub struct MprisService {
    state: Arc<Mutex<MprisState>>,
    handler: Arc<Mutex<Option<Arc<dyn MprisControlHandler>>>>,
    disposed: AtomicBool,
    #[cfg(target_os = "linux")]
    connection: Mutex<Option<zbus::Connection>>,
}

impl Default for MprisService {
    fn default() -> Self {
        Self::new()
    }
}

impl MprisService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MprisState::default())),
            handler: Arc::new(Mutex::new(None)),
            disposed: AtomicBool::new(false),
            #[cfg(target_os = "linux")]
            connection: Mutex::new(None),
        }
    }

    pub const fn is_supported() -> bool {
        cfg!(target_os = "linux")
    }

    pub fn set_handler(&self, handler: Arc<dyn MprisControlHandler>) {
        if let Ok(mut guard) = self.handler.lock() {
            *guard = Some(handler);
        }
    }

    pub fn state_snapshot(&self) -> Option<(PlayerSnapshot, String)> {
        self.state
            .lock()
            .ok()
            .map(|s| (s.snap.clone(), s.track_id.clone()))
    }

    pub async fn startup(&self, initial: Option<PlayerSnapshot>) {
        if self.disposed.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(target_os = "linux")]
        {
            if self.connection.lock().ok().is_some_and(|c| c.is_some()) {
                return;
            }
            if let Some(snap) = initial.clone() {
                if let Ok(mut st) = self.state.lock() {
                    let _ = st.apply(snap);
                }
            }
            match linux_dbus::connect(Arc::clone(&self.state), Arc::clone(&self.handler)).await {
                Ok(conn) => {
                    if let Ok(mut guard) = self.connection.lock() {
                        *guard = Some(conn);
                    }
                    if let Some(snap) = initial {
                        self.on_snapshot(snap);
                    }
                }
                Err(_) => {
                    self.shutdown().await;
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            if let Some(snap) = initial {
                self.on_snapshot(snap);
            }
        }
    }

    pub fn on_snapshot(&self, snap: PlayerSnapshot) {
        if self.disposed.load(Ordering::SeqCst) {
            return;
        }
        let diff = {
            let Ok(mut st) = self.state.lock() else {
                return;
            };
            st.apply(snap)
        };

        #[cfg(target_os = "linux")]
        {
            let conn = self.connection.lock().ok().and_then(|g| g.clone());
            if let Some(conn) = conn {
                let state_ref = Arc::clone(&self.state);
                tokio::spawn(async move {
                    linux_dbus::emit_diff(&conn, &state_ref, &diff).await;
                });
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = diff;
    }

    pub async fn shutdown(&self) {
        #[cfg(target_os = "linux")]
        {
            let conn = self.connection.lock().ok().and_then(|mut g| g.take());
            if let Some(c) = conn {
                let _ = c.close().await;
            }
        }
    }

    pub fn dispose(&self) {
        self.disposed.store(true, Ordering::SeqCst);
        #[cfg(target_os = "linux")]
        {
            if let Some(conn) = self.connection.lock().ok().and_then(|mut g| g.take()) {
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let _ = conn.close().await;
                    });
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod linux_dbus {
    use super::*;
    use std::borrow::Cow;
    use std::collections::HashMap;
    use zbus::fdo::PropertiesSignals;
    use zbus::fdo::RequestNameFlags;
    use zbus::names::InterfaceName;
    use zbus::object_server::SignalEmitter;
    use zbus::zvariant::{ObjectPath, OwnedValue, Value};

    pub struct MprisRoot {
        handler: Arc<Mutex<Option<Arc<dyn MprisControlHandler>>>>,
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl MprisRoot {
        async fn raise(&self) {
            emit_action(&self.handler, MprisAction::Raise);
        }

        async fn quit(&self) -> zbus::fdo::Result<()> {
            // CanQuit is false: quitting via MPRIS is unsupported.
            Err(zbus::fdo::Error::Failed("CanQuit is false".to_string()))
        }

        #[zbus(property)]
        async fn can_quit(&self) -> bool {
            false
        }

        #[zbus(property)]
        async fn can_raise(&self) -> bool {
            true
        }

        #[zbus(property)]
        async fn has_track_list(&self) -> bool {
            false
        }

        #[zbus(property)]
        async fn identity(&self) -> &'static str {
            "LastWave"
        }

        #[zbus(property)]
        async fn desktop_entry(&self) -> &'static str {
            "lastwave"
        }

        #[zbus(property)]
        async fn supported_uri_schemes(&self) -> Vec<String> {
            Vec::new()
        }

        #[zbus(property)]
        async fn supported_mime_types(&self) -> Vec<String> {
            Vec::new()
        }
    }

    pub struct MprisPlayer {
        state: Arc<Mutex<MprisState>>,
        handler: Arc<Mutex<Option<Arc<dyn MprisControlHandler>>>>,
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl MprisPlayer {
        async fn next(&self) {
            emit_action(&self.handler, MprisAction::Next);
        }

        async fn previous(&self) {
            emit_action(&self.handler, MprisAction::Previous);
        }

        async fn pause(&self) {
            emit_action(&self.handler, MprisAction::Pause);
        }

        async fn play_pause(&self) {
            emit_action(&self.handler, MprisAction::PlayPause);
        }

        async fn stop(&self) {
            emit_action(&self.handler, MprisAction::Stop);
        }

        async fn play(&self) {
            let is_playing = self.state.lock().ok().is_some_and(|s| s.snap.is_playing);
            if !is_playing {
                emit_action(&self.handler, MprisAction::Play);
            }
        }

        async fn seek(&self, offset_us: i64) {
            let cur_ms = self
                .state
                .lock()
                .ok()
                .map(|s| s.snap.position_ms as i64)
                .unwrap_or(0);
            let target_ms = (cur_ms + offset_us / 1000).max(0) as u64;
            emit_action(
                &self.handler,
                MprisAction::SeekTo(Duration::from_millis(target_ms)),
            );
        }

        async fn set_position(&self, track_id: ObjectPath<'_>, position_us: i64) {
            let matches = self
                .state
                .lock()
                .ok()
                .is_some_and(|s| s.track_id == track_id.as_str());
            if matches {
                let target_ms = (position_us / 1000).max(0) as u64;
                emit_action(
                    &self.handler,
                    MprisAction::SeekTo(Duration::from_millis(target_ms)),
                );
            }
        }

        #[zbus(signal)]
        pub async fn seeked(ctxt: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

        #[zbus(property)]
        async fn playback_status(&self) -> String {
            self.state
                .lock()
                .ok()
                .map(|s| playback_status_for(&s.snap).to_string())
                .unwrap_or_else(|| "Stopped".to_string())
        }

        #[zbus(property)]
        async fn loop_status(&self) -> String {
            self.state
                .lock()
                .ok()
                .map(|s| loop_status_for(s.snap.repeat_mode).to_string())
                .unwrap_or_else(|| "None".to_string())
        }

        #[zbus(property)]
        async fn set_loop_status(&self, want: String) -> zbus::fdo::Result<()> {
            let handler = self.handler.lock().ok().and_then(|g| g.clone());
            if let Some(h) = handler {
                for _ in 0..3 {
                    if loop_status_for(h.live_repeat_mode()) == want {
                        break;
                    }
                    h.handle(MprisAction::CycleRepeat);
                }
            }
            Ok(())
        }

        #[zbus(property)]
        async fn rate(&self) -> f64 {
            self.state.lock().ok().map(|s| s.snap.speed).unwrap_or(1.0)
        }

        #[zbus(property)]
        async fn set_rate(&self, rate: f64) -> zbus::fdo::Result<()> {
            emit_action(
                &self.handler,
                MprisAction::SetSpeed(rate.clamp(MIN_RATE, MAX_RATE)),
            );
            Ok(())
        }

        #[zbus(property)]
        async fn shuffle(&self) -> bool {
            self.state
                .lock()
                .ok()
                .is_some_and(|s| s.snap.shuffle_enabled)
        }

        #[zbus(property)]
        async fn set_shuffle(&self, shuffle: bool) -> zbus::fdo::Result<()> {
            let current = self
                .state
                .lock()
                .ok()
                .is_some_and(|s| s.snap.shuffle_enabled);
            if shuffle != current {
                emit_action(&self.handler, MprisAction::ToggleShuffle);
            }
            Ok(())
        }

        #[zbus(property)]
        async fn metadata(&self) -> HashMap<String, OwnedValue> {
            let Ok(st) = self.state.lock() else {
                return HashMap::new();
            };
            build_metadata_map(&st.metadata())
        }

        #[zbus(property)]
        async fn volume(&self) -> f64 {
            self.state.lock().ok().map(|s| s.snap.volume).unwrap_or(1.0)
        }

        #[zbus(property)]
        async fn set_volume(&self, volume: f64) -> zbus::fdo::Result<()> {
            emit_action(
                &self.handler,
                MprisAction::SetVolume(volume.clamp(0.0, 1.0)),
            );
            Ok(())
        }

        #[zbus(property)]
        async fn position(&self) -> i64 {
            self.state
                .lock()
                .ok()
                .map(|s| (s.snap.position_ms as i64) * 1000)
                .unwrap_or(0)
        }

        #[zbus(property)]
        async fn minimum_rate(&self) -> f64 {
            MIN_RATE
        }

        #[zbus(property)]
        async fn maximum_rate(&self) -> f64 {
            MAX_RATE
        }

        #[zbus(property)]
        async fn can_go_next(&self) -> bool {
            self.state.lock().ok().is_some_and(|s| s.snap.current.is_some())
        }

        #[zbus(property)]
        async fn can_go_previous(&self) -> bool {
            self.state.lock().ok().is_some_and(|s| s.snap.current.is_some())
        }

        #[zbus(property)]
        async fn can_play(&self) -> bool {
            self.state.lock().ok().is_some_and(|s| s.snap.current.is_some())
        }

        #[zbus(property)]
        async fn can_pause(&self) -> bool {
            self.state.lock().ok().is_some_and(|s| s.snap.current.is_some())
        }

        #[zbus(property)]
        async fn can_seek(&self) -> bool {
            self.state
                .lock()
                .ok()
                .is_some_and(|s| s.snap.current.is_some() && s.snap.duration_ms > 0)
        }

        #[zbus(property)]
        async fn can_control(&self) -> bool {
            true
        }
    }

    fn emit_action(
        handler: &Arc<Mutex<Option<Arc<dyn MprisControlHandler>>>>,
        action: MprisAction,
    ) {
        let cb = handler.lock().ok().and_then(|g| g.clone());
        if let Some(h) = cb {
            h.handle(action);
        }
    }

    fn build_metadata_map(meta: &MprisMetadata) -> HashMap<String, OwnedValue> {
        let mut map = HashMap::new();
        if let Ok(path) = ObjectPath::try_from(meta.track_id.as_str()) {
            if let Ok(val) = OwnedValue::try_from(Value::from(path)) {
                map.insert("mpris:trackid".to_string(), val);
            }
        }
        if let Some(len_us) = meta.length_us {
            map.insert("mpris:length".to_string(), OwnedValue::from(len_us));
        }
        if let Some(ref art) = meta.art_url {
            if let Ok(val) = OwnedValue::try_from(Value::from(art.as_str())) {
                map.insert("mpris:artUrl".to_string(), val);
            }
        }
        if !meta.title.is_empty() {
            if let Ok(val) = OwnedValue::try_from(Value::from(meta.title.as_str())) {
                map.insert("xesam:title".to_string(), val);
            }
        }
        if !meta.artist.is_empty() {
            let arr = Value::from(meta.artist.clone());
            if let Ok(val) = OwnedValue::try_from(arr) {
                map.insert("xesam:artist".to_string(), val);
            }
        }
        if let Some(ref alb) = meta.album {
            if let Ok(val) = OwnedValue::try_from(Value::from(alb.as_str())) {
                map.insert("xesam:album".to_string(), val);
            }
        }
        map
    }

    pub async fn connect(
        state: Arc<Mutex<MprisState>>,
        handler: Arc<Mutex<Option<Arc<dyn MprisControlHandler>>>>,
    ) -> zbus::Result<zbus::Connection> {
        let conn = zbus::Connection::session().await?;
        let root = MprisRoot {
            handler: Arc::clone(&handler),
        };
        let player = MprisPlayer { state, handler };
        conn.object_server().at(OBJECT_PATH, root).await?;
        conn.object_server().at(OBJECT_PATH, player).await?;
        conn.request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into())
            .await?;
        Ok(conn)
    }

    pub async fn emit_diff(
        conn: &zbus::Connection,
        state: &Arc<Mutex<MprisState>>,
        diff: &MprisDiff,
    ) {
        let Ok(iface_ref) = conn
            .object_server()
            .interface::<_, MprisPlayer>(OBJECT_PATH)
            .await
        else {
            return;
        };
        let emitter = iface_ref.signal_emitter();

        if diff.any_property_changed() {
            let snapshot_data = {
                let Ok(st) = state.lock() else {
                    return;
                };
                (
                    st.snap.clone(),
                    st.metadata(),
                )
            };
            let (snap, meta) = snapshot_data;
            let mut changed: HashMap<&str, Value<'_>> = HashMap::new();
            let owned_meta;
            if diff.changed_metadata {
                owned_meta = build_metadata_map(&meta);
                if let Ok(v) = Value::try_from(owned_meta) {
                    changed.insert("Metadata", v);
                }
                let has_track = snap.current.is_some();
                changed.insert("CanGoNext", Value::from(has_track));
                changed.insert("CanGoPrevious", Value::from(has_track));
                changed.insert("CanPlay", Value::from(has_track));
                changed.insert("CanPause", Value::from(has_track));
                changed.insert("CanSeek", Value::from(has_track && snap.duration_ms > 0));
            }
            if diff.changed_playback_status {
                changed.insert("PlaybackStatus", Value::from(playback_status_for(&snap)));
            }
            if diff.changed_loop_status {
                changed.insert("LoopStatus", Value::from(loop_status_for(snap.repeat_mode)));
            }
            if diff.changed_shuffle {
                changed.insert("Shuffle", Value::from(snap.shuffle_enabled));
            }
            if diff.changed_rate {
                changed.insert("Rate", Value::from(snap.speed));
            }
            if diff.changed_volume {
                changed.insert("Volume", Value::from(snap.volume));
            }
            changed.insert("Position", Value::from((snap.position_ms as i64) * 1000));

            if let Ok(iface_name) = InterfaceName::try_from(PLAYER_IFACE) {
                let _ = emitter
                    .properties_changed(iface_name, changed, Cow::Borrowed(&[]))
                    .await;
            }
        }

        if let Some(pos_us) = diff.seeked_us {
            let _ = MprisPlayer::seeked(emitter, pos_us).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stream_models::PlayableTrack;

    #[test]
    fn state_assigns_track_ids_diffs_properties_and_detects_seeks() {
        let mut st = MprisState::default();
        assert_eq!(playback_status_for(&st.snap), "Stopped");
        assert_eq!(loop_status_for(RepeatMode::All), "Playlist");

        let mut track = PlayableTrack::new("Track 1", "Artist");
        track.video_id = "v1".into();
        track.album = "Album".into();
        let mut snap = PlayerSnapshot::default();
        snap.current = Some(track);
        snap.is_playing = true;
        snap.duration_ms = 200_000;

        let d1 = st.apply(snap.clone());
        assert!(d1.changed_metadata);
        assert!(d1.changed_playback_status);
        assert_eq!(st.track_id, "/org/lastwave/track/1");
        assert!(d1.seeked_us.is_none());

        // Normal playback tick
        let mut snap2 = snap.clone();
        snap2.position_ms = 1_000;
        let d2 = st.apply(snap2.clone());
        assert!(!d2.any_property_changed());
        assert!(d2.seeked_us.is_none());

        // Seek jump > 3s while still playing
        let mut snap3 = snap2;
        snap3.position_ms = 20_000;
        let d3 = st.apply(snap3);
        assert_eq!(d3.seeked_us, Some(20_000_000));

        // Stopped resets track id to NoTrack
        let d4 = st.apply(PlayerSnapshot::default());
        assert!(d4.changed_metadata);
        assert_eq!(st.track_id, NO_TRACK_PATH);
    }
}
