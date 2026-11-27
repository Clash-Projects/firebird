use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};

/// Shared audio/track models and quality tier constants.
///
/// Quality tier constants (YouTube / data saver / 320k / 16·44.1 /
/// 24·96 / 24·192 / Dolby Atmos):
/// -1 = YouTube only, 4 = data saver (smallest YouTube audio; the
/// addon has no 96 kbps code), 5 = 320k MP3, 6 = CD 16/44.1 FLAC,
/// 7 = Hi-Res 24/96, 27 = Max Hi-Res 24/192, 28 = Dolby Atmos.
pub struct AudioQualityTiers;

impl AudioQualityTiers {
    pub const YOUTUBE_ONLY: i32 = -1;
    pub const DATA_SAVER: i32 = 4;
    pub const MP3_320: i32 = 5;
    pub const CD_LOSSLESS: i32 = 6;
    pub const HI_RES_96: i32 = 7;
    pub const MAX_HI_RES: i32 = 27;
    pub const DOLBY_ATMOS: i32 = 28;

    pub const fn label(quality: i32) -> &'static str {
        match quality {
            Self::YOUTUBE_ONLY => "Opus · YouTube",
            Self::DATA_SAVER => "Data saver",
            Self::MP3_320 => "320k MP3",
            Self::CD_LOSSLESS => "Lossless · 16/44.1",
            Self::HI_RES_96 => "Hi-Res · 24/96",
            Self::MAX_HI_RES => "Hi-Res · 24/192",
            Self::DOLBY_ATMOS => "Dolby Atmos",
            _ => "Auto",
        }
    }

    pub fn short_badge(
        is_lossless: bool,
        bit_depth: i32,
        sampling_rate_khz: f64,
        codec: &str,
    ) -> String {
        let c = codec.to_uppercase();
        if c.contains("ATMOS") || c.contains("DOLBY") {
            return "ATMOS".to_string();
        }
        if !is_lossless {
            return c;
        }
        if bit_depth >= 24 && sampling_rate_khz >= 88.0 {
            return "HI-RES".to_string();
        }
        if bit_depth >= 24 {
            return "HI-RES".to_string();
        }
        "LOSSLESS".to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

/// A playable queue item. Serialization keys mirror Android
/// `PlayableTrack` so persisted sessions stay conceptually compatible.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayableTrack {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub video_id: String,
    #[serde(default)]
    pub playback_url: String,
    #[serde(default)]
    pub playback_mime_type: String,
    /// Length the source advertised for this track in milliseconds, when known.
    /// Zero when unknown.
    #[serde(default)]
    pub duration_ms: i64,
}

impl PlayableTrack {
    pub fn new(title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            artist: artist.into(),
            ..Default::default()
        }
    }

    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration_ms.max(0) as u64)
    }

    pub fn media_id(&self) -> String {
        if !self.playback_url.is_empty() {
            return format!("local:{}", self.playback_url);
        }
        if !self.video_id.is_empty() {
            return self.video_id.clone();
        }
        format!(
            "query:{}|{}",
            self.artist.to_lowercase(),
            self.title.to_lowercase()
        )
    }

    pub fn queue_key(&self) -> String {
        format!("{}|{}", self.title.to_lowercase(), self.artist.to_lowercase())
    }

    /// Equality parity with Dart's `PlayableTrack.operator ==` (`queueKey`, `videoId`, `album`).
    pub fn same_identity(&self, other: &Self) -> bool {
        self.queue_key() == other.queue_key()
            && self.video_id == other.video_id
            && self.album == other.album
    }
}

/// A resolved playable stream (lossless or YouTube).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedStream {
    pub url: String,
    #[serde(default = "default_audio_mime_type")]
    pub mime_type: String,
    #[serde(default = "default_bitrate_kbps")]
    pub bitrate_kbps: i32,
    #[serde(default = "default_audio_codec")]
    pub audio_codec: String,
    #[serde(default)]
    pub cache_key: String,
    #[serde(default)]
    pub request_headers: HashMap<String, String>,
    #[serde(default)]
    pub is_lossless: bool,
    #[serde(default = "default_bit_depth")]
    pub bit_depth: i32,
    #[serde(default = "default_sampling_rate_khz")]
    pub sampling_rate_khz: f64,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub album_title: String,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// Pre-signed watchtime ping base (`playbackTracking.videostatsWatchtimeUrl`).
    #[serde(default)]
    pub watchtime_url: String,
}

fn default_audio_mime_type() -> String {
    "audio/webm".to_string()
}

fn default_bitrate_kbps() -> i32 {
    160
}

fn default_audio_codec() -> String {
    "OPUS".to_string()
}

fn default_bit_depth() -> i32 {
    16
}

fn default_sampling_rate_khz() -> f64 {
    44.1
}

impl Default for ResolvedStream {
    fn default() -> Self {
        Self {
            url: String::new(),
            mime_type: default_audio_mime_type(),
            bitrate_kbps: default_bitrate_kbps(),
            audio_codec: default_audio_codec(),
            cache_key: String::new(),
            request_headers: HashMap::new(),
            is_lossless: false,
            bit_depth: default_bit_depth(),
            sampling_rate_khz: default_sampling_rate_khz(),
            artwork_url: String::new(),
            album_title: String::new(),
            expires_at: None,
            watchtime_url: String::new(),
        }
    }
}

impl ResolvedStream {
    pub fn is_expired(&self) -> bool {
        self.is_expired_at(Utc::now())
    }

    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            Some(exp) => now > exp - ChronoDuration::minutes(2),
            None => false,
        }
    }

    pub fn quality_badge(&self) -> String {
        AudioQualityTiers::short_badge(
            self.is_lossless,
            self.bit_depth,
            self.sampling_rate_khz,
            &self.audio_codec,
        )
    }
}

/// In-memory LRU of resolved stream URLs. Entries drop two minutes before
/// the signed URL expires.
#[derive(Debug, Clone)]
pub struct ResolvedStreamCache {
    pub max_entries: usize,
    entries: HashMap<String, ResolvedStream>,
    order: VecDeque<String>,
}

impl Default for ResolvedStreamCache {
    fn default() -> Self {
        Self::new(24)
    }
}

impl ResolvedStreamCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&mut self, key: &str) -> Option<ResolvedStream> {
        let expired = self.entries.get(key).map(ResolvedStream::is_expired)?;
        if expired {
            self.invalidate(key);
            return None;
        }
        self.entries.get(key).cloned()
    }

    pub fn put(&mut self, key: impl Into<String>, stream: ResolvedStream) {
        let key = key.into();
        if key.is_empty() || stream.url.is_empty() || stream.is_expired() || self.max_entries == 0
        {
            return;
        }
        self.invalidate(&key);
        self.order.push_back(key.clone());
        self.entries.insert(key, stream);
        while self.entries.len() > self.max_entries {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }
    }

    pub fn invalidate(&mut self, key: &str) {
        if self.entries.remove(key).is_some() {
            self.order.retain(|k| k != key);
        }
    }

    pub fn invalidate_where<F>(&mut self, mut test: F)
    where
        F: FnMut(&str) -> bool,
    {
        self.entries.retain(|k, _| !test(k));
        self.order.retain(|k| self.entries.contains_key(k));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// One playable music-video URL. Audio playback stays on its own
/// stream; this is only the picture the synced video pane opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicVideoStream {
    pub label: String,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub bitrate: i32,
    pub url: String,
    #[serde(default = "default_video_mime_type")]
    pub mime_type: String,
    #[serde(default)]
    pub request_headers: HashMap<String, String>,
}

fn default_video_mime_type() -> String {
    "video/mp4".to_string()
}

impl Default for MusicVideoStream {
    fn default() -> Self {
        Self {
            label: String::new(),
            height: 0,
            bitrate: 0,
            url: String::new(),
            mime_type: default_video_mime_type(),
            request_headers: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_tiers_labels_and_badges() {
        assert_eq!(
            AudioQualityTiers::label(AudioQualityTiers::YOUTUBE_ONLY),
            "Opus · YouTube"
        );
        assert_eq!(
            AudioQualityTiers::label(AudioQualityTiers::DOLBY_ATMOS),
            "Dolby Atmos"
        );
        assert_eq!(AudioQualityTiers::label(999), "Auto");

        assert_eq!(
            AudioQualityTiers::short_badge(true, 24, 96.0, "flac"),
            "HI-RES"
        );
        assert_eq!(
            AudioQualityTiers::short_badge(true, 16, 44.1, "flac"),
            "LOSSLESS"
        );
        assert_eq!(
            AudioQualityTiers::short_badge(false, 16, 44.1, "opus"),
            "OPUS"
        );
        assert_eq!(
            AudioQualityTiers::short_badge(true, 24, 48.0, "eac3-atmos"),
            "ATMOS"
        );
    }

    #[test]
    fn playable_track_media_id_and_serde_roundtrip() {
        let mut t = PlayableTrack::new("Track One", "Artist A");
        assert_eq!(t.media_id(), "query:artist a|track one");
        assert_eq!(t.queue_key(), "track one|artist a");

        t.video_id = "vid123".to_string();
        assert_eq!(t.media_id(), "vid123");

        t.playback_url = "file:///song.flac".to_string();
        t.duration_ms = 185_000;
        assert_eq!(t.media_id(), "local:file:///song.flac");
        assert_eq!(t.duration(), Duration::from_secs(185));

        let json = serde_json::to_string(&t).unwrap();
        let decoded: PlayableTrack = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, t);
    }

    #[test]
    fn resolved_stream_cache_eviction_and_expiry() {
        let mut cache = ResolvedStreamCache::new(2);
        let future = Utc::now() + ChronoDuration::minutes(10);
        let expired = Utc::now() + ChronoDuration::seconds(30);

        cache.put(
            "k1",
            ResolvedStream {
                url: "https://a".to_string(),
                expires_at: Some(future),
                ..Default::default()
            },
        );
        cache.put(
            "k2",
            ResolvedStream {
                url: "https://b".to_string(),
                expires_at: Some(future),
                ..Default::default()
            },
        );
        cache.put(
            "k3",
            ResolvedStream {
                url: "https://c".to_string(),
                expires_at: Some(future),
                ..Default::default()
            },
        );
        assert_eq!(cache.len(), 2);
        assert!(cache.get("k1").is_none());
        assert!(cache.get("k2").is_some());
        assert!(cache.get("k3").is_some());

        cache.put(
            "expired",
            ResolvedStream {
                url: "https://exp".to_string(),
                expires_at: Some(expired),
                ..Default::default()
            },
        );
        assert!(cache.get("expired").is_none());

        cache.invalidate_where(|k| k == "k2");
        assert_eq!(cache.len(), 1);
    }
}
