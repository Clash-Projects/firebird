//! Self-healing checks for the two-deck mixer.
//!
//! Every bug AutoMix has had so far ended the same way: the decks were
//! left in a state nobody asked for (a primary deck stuck at volume 0, a
//! standby deck still audible, a tempo nudge never undone). Rather than
//! trusting every code path to clean up, a watchdog reads the decks once
//! a second and this pure function says what is wrong; the service then
//! applies the repairs and counts them as faults.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckReading {
    /// mpv volume, 0..100.
    pub volume: f64,
    pub playing: bool,
    pub rate: f64,
    pub position: Duration,
}

impl DeckReading {
    pub const fn new(volume: f64, playing: bool, rate: f64) -> Self {
        Self {
            volume,
            playing,
            rate,
            position: Duration::ZERO,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MixGuardInput {
    /// An overlap is running (both decks may be audible).
    pub mixing: bool,
    /// The standby deck is loaded for the next mix (must stay silent).
    pub armed: bool,
    /// The post-mix tempo glide is running on the primary deck.
    pub ramping: bool,
    /// What the primary deck's volume must be outside a mix (0..100).
    pub full_volume: f64,
    /// mpv `speed` the primary deck must have outside a mix / glide.
    pub expected_rate: f64,
    pub primary: DeckReading,
    pub standby: Option<DeckReading>,
    pub mix_age: Duration,
    /// Well above the longest blend (24 s plus pre-roll): a running blend
    /// must never be cut off as "stuck".
    pub max_mix_age: Duration,
    /// A track change is in progress; volume/rate are in flux.
    pub resolving: bool,
}

impl Default for MixGuardInput {
    fn default() -> Self {
        Self {
            mixing: false,
            armed: false,
            ramping: false,
            full_volume: 100.0,
            expected_rate: 1.0,
            primary: DeckReading::new(100.0, false, 1.0),
            standby: None,
            mix_age: Duration::ZERO,
            max_mix_age: Duration::from_secs(60),
            resolving: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixRepair {
    /// Primary deck is not at the listener's volume outside a mix.
    RestorePrimaryVolume,
    /// Standby deck is audible or playing while no mix runs.
    SilenceStandby,
    /// Primary deck kept a mix tempo after the mix ended.
    RestoreRate,
    /// An overlap has run far longer than any planned mix.
    AbortStuckMix,
}

pub fn check_mix(i: &MixGuardInput) -> Vec<MixRepair> {
    let mut out = Vec::new();
    if i.mixing {
        if i.mix_age > i.max_mix_age {
            out.push(MixRepair::AbortStuckMix);
        }
        return out;
    }
    if i.resolving {
        return out;
    }
    if let Some(sb) = i.standby {
        let audible = sb.volume > 0.5;
        // Armed decks sit paused at 0; anything else is a leftover.
        if audible || (!i.armed && sb.playing) {
            out.push(MixRepair::SilenceStandby);
        }
    }
    if (i.primary.volume - i.full_volume).abs() > 1.0 {
        out.push(MixRepair::RestorePrimaryVolume);
    }
    if !i.ramping && (i.primary.rate - i.expected_rate).abs() > 0.002 {
        out.push(MixRepair::RestoreRate);
    }
    out
}

/// Counts AutoMix faults in a session and steps down to safer modes, so
/// a mixer that keeps misbehaving on this machine stops trying.
#[derive(Debug, Clone)]
pub struct MixFaultBudget {
    pub beat_limit: usize,
    pub overlap_limit: usize,
    faults: usize,
}

impl Default for MixFaultBudget {
    fn default() -> Self {
        Self::new(3, 6)
    }
}

impl MixFaultBudget {
    pub fn new(beat_limit: usize, overlap_limit: usize) -> Self {
        Self {
            beat_limit,
            overlap_limit,
            faults: 0,
        }
    }

    pub fn faults(&self) -> usize {
        self.faults
    }

    /// Tempo matching (and its phase lock) still allowed.
    pub fn beat_mix_allowed(&self) -> bool {
        self.faults < self.beat_limit
    }

    /// Any overlap still allowed (otherwise tracks change gaplessly).
    pub fn overlap_allowed(&self) -> bool {
        self.faults < self.overlap_limit
    }

    /// Records one fault; returns a message when this fault changed mode.
    pub fn record(&mut self) -> Option<String> {
        self.faults += 1;
        if self.faults == self.beat_limit {
            return Some(format!(
                "tempo matching off for this session after {} faults",
                self.faults
            ));
        }
        if self.faults == self.overlap_limit {
            return Some(format!(
                "overlap off for this session after {} faults",
                self.faults
            ));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_stuck_mix_and_standby_and_rate_drift() {
        let mut inp = MixGuardInput {
            mixing: true,
            mix_age: Duration::from_secs(65),
            ..Default::default()
        };
        assert_eq!(check_mix(&inp), vec![MixRepair::AbortStuckMix]);

        inp.mixing = false;
        inp.primary = DeckReading::new(50.0, true, 1.04);
        inp.standby = Some(DeckReading::new(10.0, false, 1.0));
        assert_eq!(
            check_mix(&inp),
            vec![
                MixRepair::SilenceStandby,
                MixRepair::RestorePrimaryVolume,
                MixRepair::RestoreRate,
            ]
        );

        inp.resolving = true;
        assert!(check_mix(&inp).is_empty());
    }

    #[test]
    fn fault_budget_steps_down_at_thresholds() {
        let mut b = MixFaultBudget::default();
        assert!(b.beat_mix_allowed());
        assert!(b.overlap_allowed());
        assert!(b.record().is_none());
        assert!(b.record().is_none());
        let msg1 = b.record().expect("beat limit reached");
        assert!(msg1.contains("tempo matching off"));
        assert!(!b.beat_mix_allowed());
        assert!(b.overlap_allowed());
        assert!(b.record().is_none());
        assert!(b.record().is_none());
        let msg2 = b.record().expect("overlap limit reached");
        assert!(msg2.contains("overlap off"));
        assert!(!b.overlap_allowed());
    }
}
