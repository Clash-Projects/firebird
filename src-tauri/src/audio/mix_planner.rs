use std::f64::consts::PI;
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;

/// What the planner needs to know about one side of a transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixTrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Zero when not known yet (the incoming deck has not loaded).
    pub duration: Duration,
}

impl MixTrackInfo {
    pub fn new(
        title: impl Into<String>,
        artist: impl Into<String>,
        album: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            duration,
        }
    }
}

/// How a planned AutoMix transition is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixStyle {
    /// No overlap: the next track starts when this one ends (album
    /// continuity, live sets, spoken word).
    Gapless,
    /// Smoothed equal-power volume blend.
    Blend,
    /// Blend plus a low-pass / high-pass sweep (only when libmpv ships the
    /// filters — see PlaybackService filter probing).
    FilterSweep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixPlan {
    pub style: MixStyle,
    pub overlap: Duration,
    /// Short tag for the diagnostics log (`automix plan ... reason=`).
    pub reason: String,
}

impl MixPlan {
    pub fn mixes(&self) -> bool {
        self.style != MixStyle::Gapless && !self.overlap.is_zero()
    }
}

/// Decides *whether* and *how long* two tracks should overlap.
///
/// Rules, in order:
///  * consecutive tracks of the same album are left gapless — albums are
///    sequenced on purpose (continuous mixes, segues, hidden transitions);
///  * live recordings and spoken word are left gapless;
///  * the overlap shrinks for short tracks so a 30s mix never eats a
///    quarter of a 2-minute song;
///  * the filter sweep is used only when the audio engine supports it.
pub struct MixPlanner;

impl MixPlanner {
    /// Longest share of either track an overlap may take.
    pub const MAX_SHARE_OF_TRACK: f64 = 0.12;

    /// Shortest overlap worth doing; below this a plain cut sounds better.
    pub const MIN_OVERLAP: Duration = Duration::from_secs(6);

    pub fn plan(
        outgoing: &MixTrackInfo,
        incoming: &MixTrackInfo,
        requested: Duration,
        sweep_available: bool,
        sweep_wanted: bool,
    ) -> MixPlan {
        if Self::same_album(outgoing, incoming) {
            return MixPlan {
                style: MixStyle::Gapless,
                overlap: Duration::ZERO,
                reason: "album".to_string(),
            };
        }
        if Self::is_spoken_or_live(outgoing) || Self::is_spoken_or_live(incoming) {
            return MixPlan {
                style: MixStyle::Gapless,
                overlap: Duration::ZERO,
                reason: "live".to_string(),
            };
        }

        let mut overlap_ms = requested.as_millis() as f64;
        for d in [outgoing.duration, incoming.duration] {
            if !d.is_zero() {
                let capped = (d.as_millis() as f64 * Self::MAX_SHARE_OF_TRACK).round();
                overlap_ms = overlap_ms.min(capped);
            }
        }
        if overlap_ms < Self::MIN_OVERLAP.as_millis() as f64 {
            return MixPlan {
                style: MixStyle::Gapless,
                overlap: Duration::ZERO,
                reason: "short".to_string(),
            };
        }
        // Whole seconds keep the log readable and the schedule stable.
        let overlap = Duration::from_secs((overlap_ms / 1000.0).floor() as u64);
        let style = if sweep_wanted && sweep_available {
            MixStyle::FilterSweep
        } else {
            MixStyle::Blend
        };
        let reason = if overlap < requested {
            "shortened"
        } else {
            "full"
        };
        MixPlan {
            style,
            overlap,
            reason: reason.to_string(),
        }
    }

    /// Outgoing / incoming gains (0..1) at `progress` through the overlap.
    ///
    /// Equal-power (cos/sin) so loudness stays level, driven by a smoothstep
    /// so both ends move slowly: the incoming track creeps in under the
    /// outgoing one and the outgoing tail lets go gently rather than
    /// dropping linearly from the first frame.
    pub fn gains(progress: f64) -> (f64, f64) {
        let p = progress.clamp(0.0, 1.0);
        let s = p * p * (3.0 - 2.0 * p);
        let angle = s * PI / 2.0;
        (angle.cos(), angle.sin())
    }

    fn same_album(a: &MixTrackInfo, b: &MixTrackInfo) -> bool {
        let x = Self::norm(&a.album);
        let y = Self::norm(&b.album);
        if x.is_empty() || y.is_empty() {
            return false;
        }
        // "Singles" style buckets are not sequenced albums.
        if x == "single" || x == "singles" {
            return false;
        }
        if x != y {
            return false;
        }
        let ar = Self::norm(&a.artist);
        let br = Self::norm(&b.artist);
        ar.is_empty() || br.is_empty() || ar == br || ar.contains(&br) || br.contains(&ar)
    }

    fn is_spoken_or_live(t: &MixTrackInfo) -> bool {
        static SPOKEN: OnceLock<Regex> = OnceLock::new();
        static LIVE_TAG: OnceLock<Regex> = OnceLock::new();
        let spoken = SPOKEN.get_or_init(|| {
            Regex::new(r"(?i)\b(podcast|episode|audiobook|chapter \d+|interview|skit)\b")
                .expect("valid spoken regex")
        });
        let live_tag = LIVE_TAG.get_or_init(|| {
            Regex::new(
                r"(?i)[(\[]\s*(?:[^)\]]*\s)?live\b[^)\]]*[)\]]|[-–—]\s*live\b|^\s*live\s*(?::|\s(?:at|from|in)\b)",
            )
            .expect("valid live regex")
        });
        let text = format!("{} {}", t.title, t.album);
        if spoken.is_match(&text) {
            return true;
        }
        live_tag.is_match(&t.title) || live_tag.is_match(&t.album)
    }

    fn norm(s: &str) -> String {
        static BRACKETS: OnceLock<Regex> = OnceLock::new();
        static NON_WORD: OnceLock<Regex> = OnceLock::new();
        let brackets = BRACKETS.get_or_init(|| {
            Regex::new(r"\s*[(\[][^)\]]*[)\]]").expect("valid brackets regex")
        });
        let non_word = NON_WORD.get_or_init(|| {
            Regex::new(r"[^a-z0-9\u{0080}-\u{ffff}]+").expect("valid non_word regex")
        });
        let lower = s.to_lowercase();
        let stripped = brackets.replace_all(&lower, "");
        non_word.replace_all(&stripped, " ").trim().to_string()
    }
}

/// True when an mpv error line is about the audio filter chain (a missing
/// lavfi filter, a graph that failed to build) rather than the stream.
///
/// The bundled libmpv lacks some lavfi filters (`lowpass`, `highpass`):
/// mpv reports that as `[cplayer] Audio filter initialized failed!`,
/// which media_kit forwards on `stream.error`. That is NOT a broken
/// stream — the audio keeps playing — and must never trigger a retry.
pub fn is_filter_graph_error(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("filter") || t.contains("lavfi") || t.contains("avfiltergraph")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_live_spoken_short_and_blend_rules() {
        let a = MixTrackInfo::new("Track 1", "Artist", "Album X", Duration::from_secs(240));
        let b = MixTrackInfo::new("Track 2", "Artist", "Album X (Deluxe)", Duration::from_secs(240));
        let p_album = MixPlanner::plan(&a, &b, Duration::from_secs(12), true, true);
        assert_eq!(p_album.style, MixStyle::Gapless);
        assert_eq!(p_album.reason, "album");
        assert!(!p_album.mixes());

        let c = MixTrackInfo::new("Song (Live at Wembley)", "Artist", "Other", Duration::from_secs(240));
        let p_live = MixPlanner::plan(&c, &b, Duration::from_secs(12), true, true);
        assert_eq!(p_live.style, MixStyle::Gapless);
        assert_eq!(p_live.reason, "live");

        let short = MixTrackInfo::new("Intro", "A", "Alb 1", Duration::from_secs(30));
        let normal = MixTrackInfo::new("Next", "B", "Alb 2", Duration::from_secs(240));
        let p_short = MixPlanner::plan(&short, &normal, Duration::from_secs(12), true, true);
        assert_eq!(p_short.style, MixStyle::Gapless);
        assert_eq!(p_short.reason, "short");

        let d1 = MixTrackInfo::new("One", "A", "Alb 1", Duration::from_secs(240));
        let d2 = MixTrackInfo::new("Two", "B", "Alb 2", Duration::from_secs(240));
        let p_sweep = MixPlanner::plan(&d1, &d2, Duration::from_secs(12), true, true);
        assert_eq!(p_sweep.style, MixStyle::FilterSweep);
        assert_eq!(p_sweep.overlap, Duration::from_secs(12));
        assert_eq!(p_sweep.reason, "full");
        assert!(p_sweep.mixes());
    }

    #[test]
    fn gains_and_filter_error_detection() {
        let (o0, i0) = MixPlanner::gains(0.0);
        assert!((o0 - 1.0).abs() < 1e-9 && i0.abs() < 1e-9);
        let (om, im) = MixPlanner::gains(0.5);
        assert!((om * om + im * im - 1.0).abs() < 1e-9);
        assert!(is_filter_graph_error("[cplayer] Audio filter initialized failed!"));
        assert!(!is_filter_graph_error("tcp: connection reset"));
    }
}
