use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::models::player_state::PlayerSnapshot;

/// SMTC status codes consumed by the Windows/macOS media transport layer.
pub const SMTC_STATUS_STOPPED: i32 = 0;
pub const SMTC_STATUS_PLAYING: i32 = 1;
pub const SMTC_STATUS_PAUSED: i32 = 2;
pub const SMTC_STATUS_CHANGING: i32 = 3;

/// Positions that moved further than playback time explains count as a
/// seek (mirrors the MPRIS Seeked threshold).
pub const SMTC_SEEK_THRESHOLD: Duration = Duration::from_secs(3);

/// Single process / SMTC AppMediaId matching the Windows shortcut AppUserModelID.
pub const APP_USER_MODEL_ID: &str = "com.lastwave.lastwave_desktop";

#[derive(Debug, Clone, PartialEq)]
pub struct SmtcUpdate {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub status: i32,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// The shell advances the timeline itself at this rate; 0 freezes it.
    pub rate: f64,
    pub has_track: bool,
    pub can_next: bool,
    pub can_prev: bool,
}

impl SmtcUpdate {
    pub fn stopped() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            art_url: String::new(),
            status: SMTC_STATUS_STOPPED,
            position_ms: 0,
            duration_ms: 0,
            rate: 0.0,
            has_track: false,
            can_next: false,
            can_prev: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaControlAction {
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    Seek(Duration),
}

pub type MediaActionHandler = Arc<dyn Fn(MediaControlAction) + Send + Sync>;

/// Maps a snapshot to the SMTC playback status.
///
/// `Stopped` only when there is no track; buffering while playing reports
/// `Changing` so the flyout shows a spinner instead of flickering.
pub fn smtc_status_for(snap: &PlayerSnapshot) -> i32 {
    if snap.current.is_none() {
        return SMTC_STATUS_STOPPED;
    }
    if snap.is_playing && snap.is_buffering {
        return SMTC_STATUS_CHANGING;
    }
    if snap.is_playing {
        SMTC_STATUS_PLAYING
    } else {
        SMTC_STATUS_PAUSED
    }
}

/// Builds the `update` payload for `next`, or `None` when nothing worth
/// pushing changed.
///
/// The shell extrapolates position from `rate` (0 while paused), so
/// position is only pushed on track/status/seek changes — never per tick.
pub fn smtc_update_for(prev: Option<&PlayerSnapshot>, next: &PlayerSnapshot) -> Option<SmtcUpdate> {
    let track = next.current.as_ref();
    let has_track = track.is_some();
    let status = smtc_status_for(next);
    let can_go = has_track;

    let mut changed = prev.is_none();
    if let Some(p) = prev {
        let prev_track = p.current.as_ref();
        changed = prev_track.map(|t| t.media_id()) != track.map(|t| t.media_id())
            || p.duration_ms != next.duration_ms
            || prev_track.map(|t| t.artwork_url.as_str()) != track.map(|t| t.artwork_url.as_str())
            || smtc_status_for(p) != status
            || (p.speed - next.speed).abs() > f64::EPSILON
            || p.current.is_none() != track.is_none();

        if !changed {
            // User seek on the same track: position jumped further than the
            // snapshot cadence explains while the play state held steady.
            let delta = next.position().abs_diff(p.position());
            changed = prev_track.map(|t| t.media_id()) == track.map(|t| t.media_id())
                && delta > SMTC_SEEK_THRESHOLD
                && p.is_playing == next.is_playing;
        }
    }

    if !changed {
        return None;
    }

    Some(SmtcUpdate {
        title: track.map(|t| t.title.clone()).unwrap_or_default(),
        artist: track.map(|t| t.artist.clone()).unwrap_or_default(),
        album: track.map(|t| t.album.clone()).unwrap_or_default(),
        art_url: track.map(|t| t.artwork_url.clone()).unwrap_or_default(),
        status,
        position_ms: next.position_ms,
        duration_ms: next.duration_ms,
        rate: if next.is_playing { next.speed } else { 0.0 },
        has_track,
        can_next: can_go,
        can_prev: can_go,
    })
}

/// System media transport endpoint for Windows (SMTC) and macOS (Now Playing)
/// — the counterpart to the MPRIS service on Linux.
///
/// Pushes now-playing metadata + transport state natively to Windows SMTC
/// (via WinRT `SystemMediaTransportControls` interop on `HWND`) and routes
/// button presses back into the playback handler. Every failure degrades to
/// silence.
pub struct SmtcService {
    last: Mutex<Option<PlayerSnapshot>>,
    disposed: AtomicBool,
    cleared: AtomicBool,
    macos_now_playing: AtomicBool,
    action_handler: Arc<Mutex<Option<MediaActionHandler>>>,
    #[cfg(windows)]
    backend: Mutex<Option<win::WindowsSmtc>>,
}

impl Default for SmtcService {
    fn default() -> Self {
        Self::new()
    }
}

impl SmtcService {
    pub fn new() -> Self {
        Self {
            last: Mutex::new(None),
            disposed: AtomicBool::new(false),
            cleared: AtomicBool::new(false),
            macos_now_playing: AtomicBool::new(true),
            action_handler: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            backend: Mutex::new(None),
        }
    }

    /// Platforms whose desktop shell uses this transport endpoint.
    pub const fn is_supported() -> bool {
        cfg!(any(target_os = "windows", target_os = "macos"))
    }

    pub fn set_action_handler(&self, handler: MediaActionHandler) {
        if let Ok(mut guard) = self.action_handler.lock() {
            *guard = Some(handler);
        }
    }

    pub fn set_macos_now_playing(&self, enabled: bool) {
        self.macos_now_playing.store(enabled, Ordering::SeqCst);
    }

    /// Initializes the native SMTC endpoint bound to `hwnd` (Windows) and
    /// applies the initial `snapshot`. Safe to call with `hwnd == 0` (headless /
    /// deferred window attach).
    pub fn startup(&self, hwnd: isize, snapshot: Option<&PlayerSnapshot>) {
        if !Self::is_supported() || self.disposed.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(windows)]
        {
            if hwnd != 0 {
                if let Ok(mut guard) = self.backend.lock() {
                    if guard.is_none() {
                        let handler_ref = Arc::clone(&self.action_handler);
                        if let Some(smtc) = win::WindowsSmtc::new(hwnd, handler_ref) {
                            *guard = Some(smtc);
                        }
                    }
                }
            }
        }
        #[cfg(not(windows))]
        let _ = hwnd;

        if let Some(snap) = snapshot {
            self.refresh(snap);
        }
    }

    /// Re-applies the current snapshot after a settings change.
    pub fn refresh(&self, snap: &PlayerSnapshot) {
        if !Self::is_supported() || self.disposed.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(mut last) = self.last.lock() {
            *last = None;
        }
        self.cleared.store(false, Ordering::SeqCst);
        self.on_snapshot(snap);
    }

    pub fn on_snapshot(&self, snap: &PlayerSnapshot) {
        if !Self::is_supported() || self.disposed.load(Ordering::SeqCst) {
            return;
        }
        if cfg!(target_os = "macos") && !self.macos_now_playing.load(Ordering::SeqCst) {
            if let Ok(mut last) = self.last.lock() {
                *last = Some(snap.clone());
            }
            if !self.cleared.swap(true, Ordering::SeqCst) {
                self.push_update(&SmtcUpdate::stopped());
            }
            return;
        }
        self.cleared.store(false, Ordering::SeqCst);
        let update = {
            let mut last = match self.last.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let upd = smtc_update_for(last.as_ref(), snap);
            *last = Some(snap.clone());
            upd
        };
        if let Some(upd) = update {
            self.push_update(&upd);
        }
    }

    pub fn handle_native_button(&self, button: &str) {
        let action = match button {
            "play" => MediaControlAction::Play,
            "pause" => MediaControlAction::Pause,
            "toggle" => MediaControlAction::Toggle,
            // No quit-via-SMTC (MPRIS CanQuit=false parity): stop = pause.
            "stop" => MediaControlAction::Stop,
            "next" => MediaControlAction::Next,
            "previous" => MediaControlAction::Previous,
            _ => return,
        };
        self.emit_action(action);
    }

    pub fn handle_native_seek(&self, ms: i64) {
        if ms >= 0 {
            self.emit_action(MediaControlAction::Seek(Duration::from_millis(ms as u64)));
        }
    }

    fn emit_action(&self, action: MediaControlAction) {
        let handler = self
            .action_handler
            .lock()
            .ok()
            .and_then(|g| g.clone());
        if let Some(cb) = handler {
            cb(action);
        }
    }

    fn push_update(&self, update: &SmtcUpdate) {
        #[cfg(windows)]
        {
            if let Ok(mut guard) = self.backend.lock() {
                if let Some(ref mut backend) = *guard {
                    backend.apply_update(update);
                }
            }
        }
        #[cfg(not(windows))]
        let _ = update;
    }

    pub fn dispose(&self) {
        self.disposed.store(true, Ordering::SeqCst);
        if let Ok(mut last) = self.last.lock() {
            *last = None;
        }
        if cfg!(target_os = "macos") {
            self.push_update(&SmtcUpdate::stopped());
        }
        #[cfg(windows)]
        {
            if let Ok(mut guard) = self.backend.lock() {
                *guard = None;
            }
        }
    }
}

impl Drop for SmtcService {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::c_void;
    use windows::core::{factory, HSTRING};
    use windows::Foundation::{EventRegistrationToken, TimeSpan, TypedEventHandler, Uri};
    use windows::Media::{
        MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
        SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
        SystemMediaTransportControlsTimelineProperties,
    };
    use windows::Storage::Streams::RandomAccessStreamReference;
    use windows::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE};
    use windows::Win32::System::WinRT::{
        ISystemMediaTransportControlsInterop, RoInitialize, RO_INIT_SINGLETHREADED,
    };

    const MS_TO_100NS: i64 = 10_000;

    pub(super) struct WindowsSmtc {
        smtc: SystemMediaTransportControls,
        token: EventRegistrationToken,
        last_art_url: String,
    }

    // Safety: WinRT agile / marshaled proxy for SystemMediaTransportControls is thread-safe
    // under MTA / standard WinRT apartment marshaling; all mutations are serialized by Mutex.
    unsafe impl Send for WindowsSmtc {}

    impl WindowsSmtc {
        pub fn new(
            hwnd_raw: isize,
            handler_ref: Arc<Mutex<Option<MediaActionHandler>>>,
        ) -> Option<Self> {
            if hwnd_raw == 0 {
                return None;
            }
            unsafe {
                if let Err(err) = RoInitialize(RO_INIT_SINGLETHREADED) {
                    if err.code() != RPC_E_CHANGED_MODE {
                        return None;
                    }
                }
                let interop: ISystemMediaTransportControlsInterop =
                    factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()
                        .ok()?;
                let hwnd = HWND(hwnd_raw as *mut c_void);
                let smtc: SystemMediaTransportControls = interop.GetForWindow(hwnd).ok()?;

                let _ = smtc.SetIsEnabled(true);
                let _ = smtc.SetIsPlayEnabled(false);
                let _ = smtc.SetIsPauseEnabled(false);
                let _ = smtc.SetIsStopEnabled(false);
                let _ = smtc.SetIsNextEnabled(false);
                let _ = smtc.SetIsPreviousEnabled(false);
                let _ = smtc.SetPlaybackStatus(MediaPlaybackStatus::Stopped);

                let cb = TypedEventHandler::<
                    SystemMediaTransportControls,
                    SystemMediaTransportControlsButtonPressedEventArgs,
                >::new(move |_, args| {
                    if let Some(args) = args.as_ref() {
                        if let Ok(button) = args.Button() {
                            let action = match button {
                                SystemMediaTransportControlsButton::Play => {
                                    Some(MediaControlAction::Play)
                                }
                                SystemMediaTransportControlsButton::Pause => {
                                    Some(MediaControlAction::Pause)
                                }
                                SystemMediaTransportControlsButton::Stop => {
                                    Some(MediaControlAction::Stop)
                                }
                                SystemMediaTransportControlsButton::Next => {
                                    Some(MediaControlAction::Next)
                                }
                                SystemMediaTransportControlsButton::Previous => {
                                    Some(MediaControlAction::Previous)
                                }
                                _ => None,
                            };
                            if let Some(act) = action {
                                let cb_opt = handler_ref
                                    .lock()
                                    .ok()
                                    .and_then(|g| g.clone());
                                if let Some(cb) = cb_opt {
                                    cb(act);
                                }
                            }
                        }
                    }
                    Ok(())
                });

                let token = smtc.ButtonPressed(&cb).ok()?;
                Some(Self {
                    smtc,
                    token,
                    last_art_url: String::new(),
                })
            }
        }

        pub fn apply_update(&mut self, update: &SmtcUpdate) {
            if let Ok(updater) = self.smtc.DisplayUpdater() {
                let _ = updater.SetType(MediaPlaybackType::Music);
                let app_id = HSTRING::from(APP_USER_MODEL_ID);
                let _ = updater.SetAppMediaId(&app_id);

                if let Ok(music) = updater.MusicProperties() {
                    if !update.title.is_empty() {
                        let _ = music.SetTitle(&HSTRING::from(update.title.as_str()));
                    }
                    if !update.artist.is_empty() {
                        let _ = music.SetArtist(&HSTRING::from(update.artist.as_str()));
                    }
                    if !update.album.is_empty() {
                        let _ = music.SetAlbumTitle(&HSTRING::from(update.album.as_str()));
                    }
                }

                if update.art_url != self.last_art_url {
                    self.last_art_url = update.art_url.clone();
                    if !update.art_url.is_empty() {
                        if let Ok(uri) = Uri::CreateUri(&HSTRING::from(update.art_url.as_str())) {
                            if let Ok(thumb) = RandomAccessStreamReference::CreateFromUri(&uri) {
                                let _ = updater.SetThumbnail(&thumb);
                            }
                        }
                    } else {
                        let _ = updater.SetThumbnail(None);
                    }
                }
                let _ = updater.Update();
            }

            let playback_status = match update.status {
                SMTC_STATUS_PLAYING => MediaPlaybackStatus::Playing,
                SMTC_STATUS_PAUSED => MediaPlaybackStatus::Paused,
                SMTC_STATUS_CHANGING => MediaPlaybackStatus::Changing,
                _ => MediaPlaybackStatus::Stopped,
            };
            let _ = self.smtc.SetPlaybackStatus(playback_status);
            let _ = self
                .smtc
                .SetPlaybackRate(if update.has_track { update.rate } else { 0.0 });
            let _ = self.smtc.SetIsPlayEnabled(update.has_track);
            let _ = self.smtc.SetIsPauseEnabled(update.has_track);
            let _ = self.smtc.SetIsStopEnabled(update.has_track);
            let _ = self.smtc.SetIsNextEnabled(update.can_next);
            let _ = self.smtc.SetIsPreviousEnabled(update.can_prev);

            if update.duration_ms > 0 {
                if let Ok(tl) = SystemMediaTransportControlsTimelineProperties::new() {
                    let dur = update.duration_ms as i64;
                    let clamped = (update.position_ms as i64).clamp(0, dur);
                    let _ = tl.SetStartTime(ms_to_span(0));
                    let _ = tl.SetMinSeekTime(ms_to_span(0));
                    let _ = tl.SetEndTime(ms_to_span(dur));
                    let _ = tl.SetMaxSeekTime(ms_to_span(dur));
                    let _ = tl.SetPosition(ms_to_span(clamped));
                    let _ = self.smtc.UpdateTimelineProperties(&tl);
                }
            }
        }
    }

    impl Drop for WindowsSmtc {
        fn drop(&mut self) {
            if self.token.Value != 0 {
                let _ = self.smtc.RemoveButtonPressed(self.token);
            }
        }
    }

    fn ms_to_span(ms: i64) -> TimeSpan {
        TimeSpan {
            Duration: ms.max(0) * MS_TO_100NS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stream_models::PlayableTrack;

    #[test]
    fn smtc_status_and_update_diffing() {
        let mut snap = PlayerSnapshot::default();
        assert_eq!(smtc_status_for(&snap), SMTC_STATUS_STOPPED);

        let mut track = PlayableTrack::new("Song A", "Artist 1");
        track.video_id = "vid1".into();
        track.album = "Album 1".into();
        track.artwork_url = "https://img/1.jpg".into();
        snap.current = Some(track);
        snap.is_playing = true;
        snap.is_buffering = true;
        snap.duration_ms = 180_000;
        assert_eq!(smtc_status_for(&snap), SMTC_STATUS_CHANGING);

        snap.is_buffering = false;
        assert_eq!(smtc_status_for(&snap), SMTC_STATUS_PLAYING);

        let first = smtc_update_for(None, &snap).expect("first update emitted");
        assert_eq!(first.title, "Song A");
        assert_eq!(first.status, SMTC_STATUS_PLAYING);
        assert!(first.can_next && first.can_prev);

        // Normal 500ms tick should NOT push an update
        let mut tick = snap.clone();
        tick.position_ms = 500;
        assert!(smtc_update_for(Some(&snap), &tick).is_none());

        // Seek jump > 3s SHOULD push an update
        let mut seeked = snap.clone();
        seeked.position_ms = 12_000;
        assert!(smtc_update_for(Some(&snap), &seeked).is_some());
    }

    #[test]
    fn routes_buttons_and_seek_to_action_handler() {
        let svc = SmtcService::new();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_cb = Arc::clone(&captured);
        svc.set_action_handler(Arc::new(move |act| {
            captured_cb.lock().unwrap().push(act);
        }));

        svc.handle_native_button("play");
        svc.handle_native_button("stop");
        svc.handle_native_button("next");
        svc.handle_native_seek(45_000);

        let got = captured.lock().unwrap().clone();
        assert_eq!(
            got,
            vec![
                MediaControlAction::Play,
                MediaControlAction::Stop,
                MediaControlAction::Next,
                MediaControlAction::Seek(Duration::from_secs(45)),
            ]
        );
    }
}
