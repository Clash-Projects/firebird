use std::f64::consts::LN_10;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::audio::audio_analysis::TrackAnalysis;

/// How a planned blend is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DjMixStyle {
    /// Incoming tempo matched to the outgoing one and held on its beat;
    /// basses swapped at the midpoint.
    Beatmatch,
    /// Tempos too far apart to match: both play at their own speed, the
    /// incoming one starts on an outgoing downbeat, the outgoing one drains
    /// its lows while the incoming one opens up its highs.
    Filter,
}

/// A DJ blend placed on real audio, in each track's own seconds.
///
/// Nothing is cut: the blend ends on the outgoing song's last audible
/// moment and the incoming song starts at its first downbeat (its first
/// sound), so every song is heard from its start to its end. The blend
/// starts on an outgoing downbeat with the incoming downbeat on it.
#[derive(Debug, Clone, PartialEq)]
pub struct DjMixPlan {
    /// Outgoing position (a downbeat) where the blend starts.
    pub mix_out_start: f64,
    /// Blend length in seconds of OUTGOING time.
    pub blend_sec: f64,
    /// Incoming position that lines up with `mix_out_start` (a downbeat).
    pub incoming_cue: f64,
    /// Where the incoming deck is opened: a muted pre-roll before
    /// `incoming_cue` lets the phase lock settle before it is heard.
    pub incoming_start_at: f64,
    /// Incoming playback-rate multiplier while blending (1 = untouched).
    pub rate: f64,
    pub bars: u32,
    pub style: DjMixStyle,
    pub reason: String,
}

impl DjMixPlan {
    pub fn beatmatched(&self) -> bool {
        self.style == DjMixStyle::Beatmatch
    }

    pub fn mix_out_end(&self) -> f64 {
        self.mix_out_start + self.blend_sec
    }

    /// Outgoing position at which the incoming deck must start playing.
    pub fn trigger_at(&self) -> f64 {
        self.mix_out_start - (self.incoming_cue - self.incoming_start_at) / self.rate
    }

    /// Where the incoming track should be at outgoing position `out_pos`.
    pub fn expected_incoming(&self, out_pos: f64) -> f64 {
        self.incoming_cue + (out_pos - self.mix_out_start) * self.rate
    }
}

/// EQ gains in dB for both decks at one moment of a blend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DjEq {
    pub out_low: f64,
    pub out_high: f64,
    pub in_low: f64,
    pub in_high: f64,
}

impl DjEq {
    pub const FLAT: DjEq = DjEq {
        out_low: 0.0,
        out_high: 0.0,
        in_low: 0.0,
        in_high: 0.0,
    };

    pub const fn new(out_low: f64, out_high: f64, in_low: f64, in_high: f64) -> Self {
        Self {
            out_low,
            out_high,
            in_low,
            in_high,
        }
    }
}

pub struct DjMixPlanner;

impl DjMixPlanner {
    /// How far the incoming song may be slowed down / sped up to match.
    /// Within ±8% a tempo change is not heard; beyond it the pair gets a
    /// filter blend at both songs' own speed instead of a stretched one.
    pub const MAX_SLOW_DOWN: f64 = 0.08;
    pub const MAX_SPEED_UP: f64 = 0.08;

    /// Both beat grids must be at least this clear before the incoming song
    /// is retimed: matching a doubtful grid only adds wobble.
    pub const BEATMATCH_CONFIDENCE: f64 = 0.25;

    /// Longest blend in seconds, whatever the bar count.
    pub const MAX_BLEND_SEC: f64 = 24.0;

    /// Most of the outgoing song a blend may cover, so a short song keeps
    /// its body.
    pub const MAX_SHARE_OF_SONG: f64 = 0.2;

    /// Below this beat confidence a 16-bar request is played as 8 bars.
    pub const LONG_BLEND_CONFIDENCE: f64 = 0.4;

    pub const PREROLL: f64 = 2.0;

    /// `bars` is the listener's choice (8 or 16). Returns `None` when the
    /// outgoing song has no usable beat grid (the player then crossfades at
    /// the end) or the windows cannot hold the blend.
    pub fn plan(
        outgoing_tail: &TrackAnalysis,
        incoming_head: &TrackAnalysis,
        outgoing_duration: f64,
        incoming_duration: f64,
        bars: u32,
        allow_tempo: bool,
        earliest_start: Option<f64>,
    ) -> Option<DjMixPlan> {
        let requested = if bars >= 16 { 16 } else { 8 };
        let tries: &[u32] = if requested == 16 { &[16, 8] } else { &[8] };
        for &b in tries {
            if let Some(p) = Self::plan_inner(
                outgoing_tail,
                incoming_head,
                outgoing_duration,
                incoming_duration,
                b,
                allow_tempo,
                earliest_start,
            ) {
                if Self::is_safe_plan(&p, outgoing_duration) {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Whatever the analysis said, a plan the engine runs must be finite,
    /// inside both tracks and within the tempo limits.
    pub fn is_safe_plan(p: &DjMixPlan, outgoing_duration: f64) -> bool {
        let f = |v: f64| v.is_finite();
        if !f(p.mix_out_start)
            || !f(p.blend_sec)
            || !f(p.incoming_cue)
            || !f(p.incoming_start_at)
            || !f(p.rate)
        {
            return false;
        }
        if p.blend_sec < 2.0 || p.blend_sec > 60.0 {
            return false;
        }
        if !Self::rate_allowed(p.rate) {
            return false;
        }
        if !p.beatmatched() && p.rate != 1.0 {
            return false;
        }
        if p.mix_out_start < 0.0 || p.trigger_at() < 0.0 {
            return false;
        }
        if p.mix_out_end() > outgoing_duration + 0.05 {
            return false;
        }
        if p.incoming_cue < 0.0 || p.incoming_start_at < 0.0 {
            return false;
        }
        if p.incoming_start_at > p.incoming_cue + 1e-6 {
            return false;
        }
        if p.incoming_cue - p.incoming_start_at > Self::PREROLL + 1e-6 {
            return false;
        }
        true
    }

    pub fn rate_allowed(rate: f64) -> bool {
        if rate >= 1.0 {
            rate - 1.0 <= Self::MAX_SPEED_UP + 1e-9
        } else {
            1.0 - rate <= Self::MAX_SLOW_DOWN + 1e-9
        }
    }

    fn plan_inner(
        out: &TrackAnalysis,
        inc: &TrackAnalysis,
        out_dur: f64,
        in_dur: f64,
        bars: u32,
        allow_tempo: bool,
        earliest_start: Option<f64>,
    ) -> Option<DjMixPlan> {
        if !out.has_beat() {
            return None;
        }
        let out_beat = out.beat_period();
        let bar = 4.0 * out_beat;

        // ---- tempo ----------------------------------------------------------
        let mut rate = 1.0_f64;
        let mut style = DjMixStyle::Filter;
        if allow_tempo
            && inc.has_beat()
            && out.confidence >= Self::BEATMATCH_CONFIDENCE
            && inc.confidence >= Self::BEATMATCH_CONFIDENCE
        {
            let mut best = f64::INFINITY;
            // Half / double time count as the same groove.
            for m in [0.5_f64, 1.0, 2.0] {
                let r = out.bpm / (inc.bpm * m);
                if (r - 1.0).abs() < (best - 1.0).abs() {
                    best = r;
                }
            }
            if Self::rate_allowed(best) {
                rate = best;
                style = DjMixStyle::Beatmatch;
            }
        }
        // A shaky grid keeps to 8 bars. A filter blend only needs the
        // outgoing grid (the incoming song is placed on its own downbeat).
        let conf = if style == DjMixStyle::Beatmatch {
            out.confidence.min(inc.confidence)
        } else {
            out.confidence
        };
        if bars == 16 && conf < Self::LONG_BLEND_CONFIDENCE {
            return None;
        }
        let blend_sec = bars as f64 * bar;
        // Too long for this tempo or this song: the caller tries 8 bars.
        if blend_sec > Self::MAX_BLEND_SEC || blend_sec > out_dur * Self::MAX_SHARE_OF_SONG {
            return None;
        }

        // ---- outgoing: the blend ends on the song's last sound ---------------
        let content_end = out.content_end.min(out_dur);
        let out_grid: &[f64] = if !out.downbeats.is_empty() {
            &out.downbeats
        } else {
            &out.beats
        };
        let mut start: Option<f64> = None;
        for &d in out_grid.iter().rev() {
            if d <= content_end - blend_sec + 0.02 {
                start = Some(d);
                break;
            }
        }
        // The listener jumped past that point: start on the first downbeat from
        // `earliest_start` if `bars` bars still fit (the caller then tries 8).
        if let Some(earliest) = earliest_start {
            if start.is_none_or(|s| s < earliest) {
                start = None;
                for &d in out_grid {
                    if d >= earliest {
                        start = Some(d);
                        break;
                    }
                }
                let s = start?;
                if s + blend_sec > content_end + 0.05 {
                    return None;
                }
            }
        }
        let start = start?;
        if start < out.window_start + 0.5 {
            return None;
        }
        let run_sec = content_end - start;

        // ---- incoming: from its first downbeat, on the outgoing downbeat -----
        let in_grid: &[f64] = if !inc.downbeats.is_empty() {
            &inc.downbeats
        } else {
            &inc.beats
        };
        let first_sound = (inc.audible_start - 0.05).max(0.0);
        let first_down = in_grid
            .iter()
            .copied()
            .find(|&d| d >= inc.audible_start - 0.08 && d - first_sound <= 4.0);
        let cue = first_down.unwrap_or(first_sound);
        if cue + run_sec * rate + 5.0 >= in_dur {
            return None;
        }
        let start_at = if style == DjMixStyle::Beatmatch {
            first_sound.max(cue - Self::PREROLL)
        } else {
            cue
        };

        let reason = if style == DjMixStyle::Beatmatch {
            format!("beatmatch {:.1}->{:.1}", out.bpm, inc.bpm)
        } else {
            let inc_str = if inc.has_beat() {
                format!("{:.1}", inc.bpm)
            } else {
                "?".to_string()
            };
            format!("filter {:.1}->{inc_str}", out.bpm)
        };

        Some(DjMixPlan {
            mix_out_start: start,
            blend_sec: run_sec,
            incoming_cue: cue,
            incoming_start_at: start_at,
            rate,
            bars,
            style,
            reason,
        })
    }

    /// Deck gains (outgoing, incoming) at `progress` (0..1) through a blend.
    pub fn gains(_beatmatched: bool, progress: f64) -> (f64, f64) {
        let h = Self::smooth(progress);
        ((1.0 - h).sqrt(), h.sqrt())
    }

    fn smooth(x: f64) -> f64 {
        let t = x.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn db(gain: f64) -> f64 {
        if gain <= 0.001 {
            -60.0
        } else {
            20.0 * gain.ln() / LN_10
        }
    }

    /// Deck EQ at `progress` (0..1) through the blend.
    pub fn eq_at(p: &DjMixPlan, progress: f64) -> DjEq {
        Self::eq_for(p.beatmatched(), p.bars, progress)
    }

    pub fn eq_for(_beatmatched: bool, _bars: u32, progress: f64) -> DjEq {
        let t = progress.clamp(0.0, 1.0);
        let h = Self::smooth(t);
        let out_bass = 1.0 - 0.85 * Self::smooth((t - 0.25) / 0.35);
        let in_bass = 0.15 + 0.85 * Self::smooth((t - 0.45) / 0.30);
        DjEq::new(
            Self::db(out_bass),
            Self::db(1.0 - 0.5 * h),
            Self::db(in_bass),
            Self::db(0.7 + 0.3 * h),
        )
    }
}

/// What the phase lock wants done to the incoming deck this tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DjLockAction {
    /// Jump the (still muted) incoming deck here, in seconds.
    pub seek_to: Option<f64>,
    /// New playback rate for the incoming deck (absolute, includes speed).
    pub rate: Option<f64>,
}

impl DjLockAction {
    pub const NONE: DjLockAction = DjLockAction {
        seek_to: None,
        rate: None,
    };
}

/// Keeps the incoming deck on the outgoing deck's beat grid.
pub struct DjBeatLock {
    pub plan: DjMixPlan,
    pub speed: f64,
    sum: f64,
    n: usize,
    last_out: f64,
    last_rate: f64,
    last_step: SystemTime,
    last_seek: SystemTime,
    abs_sum: f64,
    count: usize,
}

impl DjBeatLock {
    pub const INTERVAL: Duration = Duration::from_millis(500);

    pub fn new(plan: DjMixPlan, speed: f64) -> Self {
        Self {
            plan,
            speed,
            sum: 0.0,
            n: 0,
            last_out: 0.0,
            last_rate: 0.0,
            last_step: UNIX_EPOCH,
            last_seek: UNIX_EPOCH,
            abs_sum: 0.0,
            count: 0,
        }
    }

    /// Mean absolute phase error while audible, seconds.
    pub fn mean_audible_error(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.abs_sum / self.count as f64
        }
    }

    pub fn sample(&mut self, out_pos: f64, in_pos: f64) {
        if !self.plan.beatmatched() || in_pos <= 0.0 || out_pos <= 0.0 {
            return;
        }
        self.sum += in_pos - self.plan.expected_incoming(out_pos);
        self.n += 1;
        self.last_out = out_pos;
    }

    pub fn step(&mut self, now: SystemTime) -> DjLockAction {
        if !self.plan.beatmatched() {
            return DjLockAction::NONE;
        }
        let since_step = now
            .duration_since(self.last_step)
            .unwrap_or(Duration::ZERO);
        if since_step < Self::INTERVAL || self.n < 4 {
            return DjLockAction::NONE;
        }
        self.last_step = now;
        let err = self.sum / self.n as f64;
        self.sum = 0.0;
        self.n = 0;

        let audible = self.last_out >= self.plan.mix_out_start;
        if audible {
            self.abs_sum += err.abs();
            self.count += 1;
        }
        let since_seek = now
            .duration_since(self.last_seek)
            .unwrap_or(Duration::ZERO);
        if !audible && err.abs() > 0.12 && since_seek > Duration::from_secs(1) {
            self.last_seek = now;
            return DjLockAction {
                seek_to: Some(self.plan.expected_incoming(self.last_out) + 0.03),
                rate: None,
            };
        }
        // Remove the error over ~1 s; ignore what is inside the noise floor.
        let e = if err.abs() < 0.008 { 0.0 } else { err };
        let corr = (-e).clamp(-0.03, 0.03);
        let rate = self.speed * self.plan.rate * (1.0 + corr);
        if (rate - self.last_rate).abs() > 0.0005 {
            self.last_rate = rate;
            return DjLockAction {
                seek_to: None,
                rate: Some(rate),
            };
        }
        DjLockAction::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_analysis(start: f64, end: f64, bpm: f64, conf: f64) -> TrackAnalysis {
        let period = 60.0 / bpm;
        let mut beats = Vec::new();
        let mut downbeats = Vec::new();
        let mut t = start + 0.5;
        let mut idx = 0usize;
        while t < end - 0.5 {
            beats.push(t);
            if idx % 4 == 0 {
                downbeats.push(t);
            }
            t += period;
            idx += 1;
        }
        TrackAnalysis {
            window_start: start,
            window_end: end,
            bpm,
            confidence: conf,
            beats,
            downbeats,
            audible_start: start + 0.4,
            content_end: end - 0.5,
            energy_db: vec![-14.0; 20],
            energy_hop: 0.5,
        }
    }

    #[test]
    fn plans_beatmatch_and_half_time_and_filter_fallback() {
        let out = make_analysis(120.0, 180.0, 120.0, 0.8);
        let inc_same = make_analysis(0.0, 60.0, 124.0, 0.75);
        let p = DjMixPlanner::plan(&out, &inc_same, 180.0, 180.0, 8, true, None)
            .expect("beatmatch plan");
        assert_eq!(p.style, DjMixStyle::Beatmatch);
        assert_eq!(p.bars, 8);
        assert!((p.rate - (120.0 / 124.0)).abs() < 1e-6);

        // Double/half-time match (62 BPM vs 120 BPM => 62*2=124 -> rate ~0.9677)
        let inc_half = make_analysis(0.0, 60.0, 62.0, 0.8);
        let p_half = DjMixPlanner::plan(&out, &inc_half, 180.0, 180.0, 8, true, None)
            .expect("half-time beatmatch");
        assert_eq!(p_half.style, DjMixStyle::Beatmatch);

        // Far apart BPM -> filter blend at rate 1.0
        let inc_far = make_analysis(0.0, 60.0, 90.0, 0.8);
        let p_filter = DjMixPlanner::plan(&out, &inc_far, 180.0, 180.0, 8, true, None)
            .expect("filter plan");
        assert_eq!(p_filter.style, DjMixStyle::Filter);
        assert_eq!(p_filter.rate, 1.0);
    }

    #[test]
    fn gains_preserve_power_and_eq_swaps_bass() {
        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let (g_out, g_in) = DjMixPlanner::gains(true, t);
            assert!((g_out * g_out + g_in * g_in - 1.0).abs() < 1e-9);
        }
        let eq_start = DjMixPlanner::eq_for(true, 8, 0.0);
        let eq_end = DjMixPlanner::eq_for(true, 8, 1.0);
        assert!(eq_start.out_low > eq_end.out_low);
        assert!(eq_end.in_low > eq_start.in_low);
    }

    #[test]
    fn beat_lock_seeks_before_audible_then_nudges_rate() {
        let plan = DjMixPlan {
            mix_out_start: 160.0,
            blend_sec: 16.0,
            incoming_cue: 2.5,
            incoming_start_at: 0.5,
            rate: 1.0,
            bars: 8,
            style: DjMixStyle::Beatmatch,
            reason: "test".into(),
        };
        let mut lock = DjBeatLock::new(plan, 1.0);
        // Pre-roll phase error > 0.12s -> seeks
        for _ in 0..4 {
            lock.sample(158.5, 1.5); // expected 1.0, err +0.5
        }
        let t1 = UNIX_EPOCH + Duration::from_secs(10);
        let a1 = lock.step(t1);
        assert!(a1.seek_to.is_some());

        // Audible small phase error -> rate nudge
        for _ in 0..4 {
            lock.sample(162.0, 4.52); // expected 4.5, err +0.02
        }
        let t2 = t1 + Duration::from_secs(2);
        let a2 = lock.step(t2);
        assert!(a2.seek_to.is_none());
        assert!(a2.rate.is_some_and(|r| r < 1.0));
        assert!(lock.mean_audible_error() > 0.01);
    }
}
