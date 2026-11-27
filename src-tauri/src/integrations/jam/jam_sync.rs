//! Bridges Jam playback events to the existing `PlaybackService`.
//!
//! Ported from `lib/features/jam/jam_sync.dart`.
//! AutoMix is not part of the protocol and is never read or written here.
//! If the socket is down, this does nothing and local playback continues.

use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::Regex;
use tokio::sync::Mutex;

use crate::audio::playback_service::PlaybackService;
use crate::models::player_state::PlayerSnapshot;
use crate::models::stream_models::PlayableTrack;

use super::jam_client::{
    JamController, JamPhase, JamRemote, JamSnapshot, JamTrack, JAM_MODE_COLLABORATIVE,
};

static YT_VIDEO_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]{11}$").unwrap());

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn latency_ms(server_time_ms: i64) -> i64 {
    if server_time_ms <= 0 {
        return 0;
    }
    let delta = now_ms() - server_time_ms;
    if delta <= 0 {
        0
    } else {
        delta.min(3000)
    }
}

#[derive(Debug, Default)]
pub struct JamBridgeState {
    pub quiet_until: i64,
    pub last_track: Option<String>,
    pub last_playing: Option<bool>,
    pub last_position: i64,
    pub last_wall: i64,
    pub last_queue_key: Option<String>,
    // Live position of a radio station, for snapping listeners back.
    pub live_track: Option<JamTrack>,
    pub live_pos: i64,
    pub live_wall: i64,
    pub live_playing: bool,
    pub last_enforce: i64,
    pub queue_generation: u64,
}

pub fn can_control(session: &JamSnapshot) -> bool {
    session.is_host || (session.room_mode == JAM_MODE_COLLABORATIVE && !session.radio)
}

fn track_key(track: &PlayableTrack) -> String {
    if !track.video_id.is_empty() {
        track.video_id.clone()
    } else {
        track.title.clone()
    }
}

pub struct JamSyncBridge {
    jam: Arc<JamController>,
    player: Arc<PlaybackService>,
    state: Arc<Mutex<JamBridgeState>>,
}

impl JamSyncBridge {
    pub fn new(jam: Arc<JamController>, player: Arc<PlaybackService>) -> Self {
        Self {
            jam,
            player,
            state: Arc::new(Mutex::new(JamBridgeState::default())),
        }
    }

    pub fn note_live(bridge: &mut JamBridgeState, session: &JamSnapshot, event: &JamRemote) {
        let now = now_ms();
        match event {
            JamRemote::Track {
                track,
                is_playing,
                position_ms,
                ..
            } => {
                bridge.live_track = Some(track.clone());
                bridge.live_pos = *position_ms;
                bridge.live_playing = *is_playing;
            }
            JamRemote::Full {
                track,
                is_playing,
                position_ms,
                server_time_ms,
            } => {
                bridge.live_track = track.clone();
                bridge.live_pos = *position_ms + latency_ms(*server_time_ms);
                bridge.live_playing = *is_playing;
            }
            JamRemote::Play {
                position_ms,
                server_time_ms,
                ..
            } => {
                if bridge.live_track.is_none() {
                    bridge.live_track = session.track.clone();
                }
                bridge.live_pos = *position_ms + latency_ms(*server_time_ms);
                bridge.live_playing = true;
            }
            JamRemote::Pause { position_ms, .. } => {
                bridge.live_pos = *position_ms;
                bridge.live_playing = false;
            }
            JamRemote::Seek { position_ms, .. } => {
                bridge.live_pos = *position_ms;
            }
        }
        bridge.live_wall = now;
    }

    /// Live-station listeners can't pause, seek or change the song: anything
    /// the local player does on its own is undone and it rejoins the live position.
    async fn enforce_live(&self, next: &PlayerSnapshot) {
        let (live, expected, need_change, need_resume, need_seek) = {
            let mut bridge = self.state.lock().await;
            let Some(live) = bridge.live_track.clone() else {
                return;
            };
            let now = now_ms();
            if now < bridge.quiet_until || now - bridge.last_enforce < 1500 {
                return;
            }
            let expected = bridge.live_pos
                + if bridge.live_playing {
                    now - bridge.live_wall
                } else {
                    0
                };
            let same_track = next.current.as_ref().is_some_and(|c| {
                c.video_id == live.id || c.title.eq_ignore_ascii_case(&live.title)
            });
            if !same_track {
                bridge.last_enforce = now;
                (Some((live.clone(), bridge.live_playing)), expected, true, false, false)
            } else {
                let mut resume = false;
                let mut seek = false;
                if bridge.live_playing && !next.is_playing && !next.is_buffering {
                    bridge.last_enforce = now;
                    bridge.quiet_until = now + 800;
                    resume = true;
                }
                if (next.position_ms as i64 - expected).abs() > 4000 {
                    bridge.last_enforce = now;
                    bridge.quiet_until = now + 800;
                    seek = true;
                }
                (None, expected, false, resume, seek)
            }
        };

        if need_change {
            if let Some((track, is_playing)) = live {
                self.change_track(next, &track, is_playing, expected).await;
            }
            return;
        }
        if need_resume {
            self.player.play_resume().await;
        }
        if need_seek {
            self.player
                .seek(Duration::from_millis(expected.max(0) as u64))
                .await;
        }
    }

    pub async fn apply_remote(&self, event: JamRemote) {
        let session = self.jam.snapshot().await;
        if session.phase != JamPhase::Active {
            return;
        }
        let from = match &event {
            JamRemote::Play { from_user_id, .. }
            | JamRemote::Pause { from_user_id, .. }
            | JamRemote::Seek { from_user_id, .. }
            | JamRemote::Track { from_user_id, .. } => from_user_id.as_deref(),
            JamRemote::Full { .. } => None,
        };
        {
            let mut bridge = self.state.lock().await;
            Self::note_live(&mut bridge, &session, &event);
        }
        if from.is_some_and(|f| f == session.user_id) {
            return;
        }

        let local = self.player.snapshot();
        match event {
            JamRemote::Track {
                track,
                is_playing,
                position_ms,
                ..
            } => {
                self.change_track(&local, &track, is_playing, position_ms)
                    .await;
            }
            JamRemote::Full {
                track,
                is_playing,
                position_ms,
                server_time_ms,
            } => {
                let Some(t) = track else { return };
                let latency = latency_ms(server_time_ms);
                self.change_track(&local, &t, is_playing, position_ms + latency)
                    .await;
            }
            JamRemote::Play {
                position_ms,
                server_time_ms,
                ..
            } => {
                let target = position_ms + latency_ms(server_time_ms);
                self.state.lock().await.quiet_until = now_ms() + 800;
                if local.current.is_none() {
                    if let Some(ref t) = session.track {
                        self.change_track(&local, t, true, target).await;
                        return;
                    }
                }
                if (local.position_ms as i64 - target).abs() > 1500 {
                    self.player
                        .seek(Duration::from_millis(target.max(0) as u64))
                        .await;
                }
                if !local.is_playing {
                    self.player.play_resume().await;
                }
            }
            JamRemote::Pause { position_ms, .. } => {
                self.state.lock().await.quiet_until = now_ms() + 800;
                if local.is_playing {
                    self.player.pause().await;
                }
                if (local.position_ms as i64 - position_ms).abs() > 1500 {
                    self.player
                        .seek(Duration::from_millis(position_ms.max(0) as u64))
                        .await;
                }
            }
            JamRemote::Seek { position_ms, .. } => {
                self.state.lock().await.quiet_until = now_ms() + 800;
                if (local.position_ms as i64 - position_ms).abs() > 1500 {
                    self.player
                        .seek(Duration::from_millis(position_ms.max(0) as u64))
                        .await;
                }
            }
        }
    }

    async fn change_track(
        &self,
        local: &PlayerSnapshot,
        track: &JamTrack,
        is_playing: bool,
        position_ms: i64,
    ) {
        let same = local.current.as_ref().is_some_and(|c| {
            (!c.video_id.is_empty() && c.video_id == track.id)
                || (c.title.eq_ignore_ascii_case(&track.title)
                    && (c.artist.is_empty()
                        || track.artist.is_empty()
                        || c.artist.eq_ignore_ascii_case(&track.artist)))
        });
        {
            let mut bridge = self.state.lock().await;
            bridge.quiet_until = now_ms() + if same { 800 } else { 2500 };
            bridge.last_track = Some(if track.id.is_empty() {
                track.title.clone()
            } else {
                track.id.clone()
            });
            bridge.last_playing = Some(is_playing);
            bridge.last_position = position_ms;
        }

        if !same {
            let youtube = YT_VIDEO_ID_RE.is_match(&track.id);
            self.player
                .play(
                    PlayableTrack {
                        title: track.title.clone(),
                        artist: track.artist.clone(),
                        album: track.album.clone().unwrap_or_default(),
                        artwork_url: track.artwork_url.clone().unwrap_or_default(),
                        video_id: if youtube {
                            track.id.clone()
                        } else {
                            String::new()
                        },
                        playback_url: String::new(),
                        playback_mime_type: String::new(),
                        duration_ms: track.duration_ms.unwrap_or(0).max(0),
                    },
                    "LastWave Jam",
                    false,
                )
                .await;
            if position_ms > 800 {
                self.player
                    .seek(Duration::from_millis(position_ms as u64))
                    .await;
            }
            if !is_playing {
                self.player.pause().await;
            }
            return;
        }

        if (local.position_ms as i64 - position_ms).abs() > 1500 {
            self.player
                .seek(Duration::from_millis(position_ms.max(0) as u64))
                .await;
        }
        if is_playing && !local.is_playing {
            self.player.play_resume().await;
        } else if !is_playing && local.is_playing {
            self.player.pause().await;
        }
    }

    pub async fn on_player_snapshot(
        &self,
        previous: Option<&PlayerSnapshot>,
        next: &PlayerSnapshot,
    ) {
        self.broadcast(previous, next).await;
        self.publish_queue(false).await;
    }

    pub async fn on_session_changed(&self, previous: Option<&JamSnapshot>, next: &JamSnapshot) {
        let was = previous.is_some_and(|p| p.phase == JamPhase::Active && p.is_main_host());
        let now = next.phase == JamPhase::Active && next.is_main_host();
        if now && !was {
            self.state.lock().await.last_queue_key = None;
            self.publish_queue(true).await;
        }
    }

    async fn broadcast(&self, previous: Option<&PlayerSnapshot>, next: &PlayerSnapshot) {
        let session = self.jam.snapshot().await;
        if session.phase != JamPhase::Active {
            return;
        }
        if session.is_station_listener() {
            self.enforce_live(next).await;
            return;
        }
        if !can_control(&session) {
            return;
        }
        let Some(ref current) = next.current else {
            return;
        };

        let now = now_ms();
        let key = track_key(current);
        let mut position = next.position_ms as i64;

        enum Action {
            Track(JamTrack, i64, bool),
            Play(i64),
            Pause(i64),
            Seek(i64),
            None,
        }

        let action = {
            let mut bridge = self.state.lock().await;
            if now < bridge.quiet_until {
                return;
            }
            if bridge.last_track.as_deref() != Some(&key) {
                if let Some(prev) = previous {
                    if let Some(ref pc) = prev.current {
                        if track_key(pc) != key
                            && position > 1500
                            && (position - prev.position_ms as i64).abs() < 1500
                        {
                            position = 0;
                        }
                    }
                }
                bridge.last_track = Some(key.clone());
                bridge.last_playing = Some(next.is_playing);
                bridge.last_position = position;
                bridge.last_wall = now;
                bridge.quiet_until = now + 1000;
                Action::Track(
                    JamTrack {
                        id: key,
                        title: current.title.clone(),
                        artist: current.artist.clone(),
                        album: Some(current.album.clone()),
                        artwork_url: Some(current.artwork_url.clone()),
                        duration_ms: Some(current.duration_ms),
                    },
                    position,
                    next.is_playing,
                )
            } else if bridge.last_playing != Some(next.is_playing) {
                if !next.is_playing && next.is_buffering {
                    return;
                }
                bridge.last_playing = Some(next.is_playing);
                bridge.last_position = position;
                bridge.last_wall = now;
                bridge.quiet_until = now + 800;
                if next.is_playing {
                    Action::Play(position)
                } else {
                    Action::Pause(position)
                }
            } else if previous.is_none() {
                Action::None
            } else {
                let wall = now - bridge.last_wall;
                let expected = bridge.last_position + if next.is_playing { wall } else { 0 };
                bridge.last_position = position;
                bridge.last_wall = now;
                if (position - expected).abs() > 2000 && wall < 4000 {
                    bridge.quiet_until = now + 800;
                    Action::Seek(position)
                } else {
                    Action::None
                }
            }
        };

        match action {
            Action::Track(t, pos, playing) => self.jam.broadcast_track(&t, pos, playing).await,
            Action::Play(pos) => self.jam.broadcast_play(pos).await,
            Action::Pause(pos) => self.jam.broadcast_pause(pos).await,
            Action::Seek(pos) => self.jam.broadcast_seek(pos).await,
            Action::None => {}
        }
    }

    async fn publish_queue(&self, immediate: bool) {
        let session = self.jam.snapshot().await;
        if session.phase != JamPhase::Active || !session.is_main_host() {
            return;
        }
        let player = self.player.snapshot();
        let start = (player.current_index + 1).max(0) as usize;
        let upcoming = if start < player.queue.len() {
            &player.queue[start..]
        } else {
            &[]
        };
        let tracks: Vec<JamTrack> = upcoming
            .iter()
            .take(5)
            .map(|t| JamTrack {
                id: track_key(t),
                title: t.title.clone(),
                artist: t.artist.clone(),
                album: Some(t.album.clone()),
                artwork_url: Some(t.artwork_url.clone()),
                duration_ms: Some(t.duration_ms),
            })
            .collect();
        let key: String = tracks
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>()
            .join("|");

        let gen = {
            let mut bridge = self.state.lock().await;
            if bridge.last_queue_key.as_deref() == Some(&key) {
                return;
            }
            bridge.queue_generation += 1;
            if immediate {
                bridge.last_queue_key = Some(key);
                None
            } else {
                Some((bridge.queue_generation, key))
            }
        };

        if let Some((expected_gen, key)) = gen {
            let state_ref = Arc::clone(&self.state);
            let jam = Arc::clone(&self.jam);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(600)).await;
                {
                    let mut bridge = state_ref.lock().await;
                    if bridge.queue_generation != expected_gen {
                        return;
                    }
                    bridge.last_queue_key = Some(key);
                }
                jam.broadcast_queue(tracks).await;
            });
        } else {
            self.jam.broadcast_queue(tracks).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_control_respects_host_and_room_mode() {
        let mut snap = JamSnapshot::default();
        snap.room_mode = JAM_MODE_COLLABORATIVE.to_string();
        assert!(can_control(&snap));

        snap.radio = true;
        assert!(!can_control(&snap));

        snap.is_host = true;
        assert!(can_control(&snap));
    }

    #[test]
    fn note_live_records_play_and_pause() {
        let mut bridge = JamBridgeState::default();
        let session = JamSnapshot::default();
        JamSyncBridge::note_live(
            &mut bridge,
            &session,
            &JamRemote::Play {
                position_ms: 5000,
                server_time_ms: 0,
                from_user_id: None,
            },
        );
        assert!(bridge.live_playing);
        assert_eq!(bridge.live_pos, 5000);

        JamSyncBridge::note_live(
            &mut bridge,
            &session,
            &JamRemote::Pause {
                position_ms: 7000,
                from_user_id: None,
            },
        );
        assert!(!bridge.live_playing);
        assert_eq!(bridge.live_pos, 7000);
    }
}
