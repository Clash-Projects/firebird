use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::pcm_format::PcmFormat;
use super::stream_models::{PlayableTrack, RepeatMode, ResolvedStream};

/// Immutable playback snapshot consumed by the UI.
/// Mirrors Android `MusicPlayerState` (tracker-friendly subset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    #[serde(default)]
    pub current: Option<PlayableTrack>,
    #[serde(default)]
    pub queue: Vec<PlayableTrack>,
    #[serde(default = "default_current_index")]
    pub current_index: i32,
    #[serde(default)]
    pub source_label: String,
    #[serde(default)]
    pub is_playing: bool,
    #[serde(default)]
    pub is_buffering: bool,
    #[serde(default)]
    pub position_ms: u64,
    #[serde(default)]
    pub buffered_ms: u64,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub shuffle_enabled: bool,
    #[serde(default)]
    pub repeat_mode: RepeatMode,
    #[serde(default = "default_unity")]
    pub speed: f64,
    /// Independent of [`Self::speed`]. `1.0` is concert pitch. Range used by
    /// the dock is `0.5–1.5`.
    #[serde(default = "default_unity")]
    pub pitch: f64,
    #[serde(default)]
    pub stream: Option<ResolvedStream>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub sleep_remaining_ms: Option<u64>,
    /// Length the user picked, in minutes. `None` while the timer is off.
    /// Stays put as [`Self::sleep_remaining_ms`] counts down so the menu can
    /// keep the chosen row checked.
    #[serde(default)]
    pub sleep_minutes: Option<i32>,
    #[serde(default)]
    pub bitrate_kbps: i32,
    #[serde(default = "default_unity")]
    pub volume: f64,
    #[serde(default)]
    pub output_format: Option<PcmFormat>,
    #[serde(default)]
    pub output_is_float: bool,
}

const fn default_current_index() -> i32 {
    -1
}

const fn default_unity() -> f64 {
    1.0
}

impl Default for PlayerSnapshot {
    fn default() -> Self {
        Self {
            current: None,
            queue: Vec::new(),
            current_index: default_current_index(),
            source_label: String::new(),
            is_playing: false,
            is_buffering: false,
            position_ms: 0,
            buffered_ms: 0,
            duration_ms: 0,
            shuffle_enabled: false,
            repeat_mode: RepeatMode::Off,
            speed: 1.0,
            pitch: 1.0,
            stream: None,
            error: None,
            sleep_remaining_ms: None,
            sleep_minutes: None,
            bitrate_kbps: 0,
            volume: 1.0,
            output_format: None,
            output_is_float: false,
        }
    }
}

impl PlayerSnapshot {
    pub fn position(&self) -> Duration {
        Duration::from_millis(self.position_ms)
    }

    pub fn buffered(&self) -> Duration {
        Duration::from_millis(self.buffered_ms)
    }

    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration_ms)
    }

    pub fn sleep_remaining(&self) -> Option<Duration> {
        self.sleep_remaining_ms.map(Duration::from_millis)
    }
}
