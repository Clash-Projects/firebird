use serde::{Deserialize, Serialize};
use std::f64::consts::{LN_10, LN_2, PI};

/// Rhythm and level facts about one window of a track (the tail of the
/// outgoing song or the head of the incoming one).
///
/// All times are seconds on the TRACK's own timeline (window offset
/// already added), so the planner never has to know which window a
/// number came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackAnalysis {
    #[serde(rename = "ws")]
    pub window_start: f64,
    #[serde(rename = "we")]
    pub window_end: f64,
    /// 0 when no steady pulse was found.
    pub bpm: f64,
    /// 0..1 — how clearly the beat grid stands out from the onset noise.
    #[serde(rename = "conf")]
    pub confidence: f64,
    /// Beat grid across the window (track seconds).
    pub beats: Vec<f64>,
    /// Subset of `beats` judged to be bar starts ("the one").
    #[serde(rename = "down")]
    pub downbeats: Vec<f64>,
    /// First moment with real signal (skips silent / near-silent intros).
    #[serde(rename = "as")]
    pub audible_start: f64,
    /// Last moment with real signal (ignores silent tails).
    #[serde(rename = "ce")]
    pub content_end: f64,
    /// RMS level in dBFS every `energy_hop` seconds from `window_start`.
    #[serde(rename = "e")]
    pub energy_db: Vec<f64>,
    #[serde(rename = "eh")]
    pub energy_hop: f64,
}

impl TrackAnalysis {
    pub fn beat_period(&self) -> f64 {
        if self.bpm > 0.0 {
            60.0 / self.bpm
        } else {
            0.0
        }
    }

    pub fn has_beat(&self) -> bool {
        self.bpm > 0.0 && self.confidence >= 0.15 && self.beats.len() >= 8
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "v": 1,
            "ws": self.window_start,
            "we": self.window_end,
            "bpm": self.bpm,
            "conf": self.confidence,
            "beats": self.beats,
            "down": self.downbeats,
            "as": self.audible_start,
            "ce": self.content_end,
            "e": self.energy_db,
            "eh": self.energy_hop,
        })
    }

    pub fn from_json(j: &serde_json::Value) -> Option<Self> {
        let obj = j.as_object()?;
        if obj.get("v")?.as_i64()? != 1 {
            return None;
        }
        let dl = |key: &str| -> Option<Vec<f64>> {
            obj.get(key)?
                .as_array()?
                .iter()
                .map(|v| v.as_f64())
                .collect()
        };
        Some(Self {
            window_start: obj.get("ws")?.as_f64()?,
            window_end: obj.get("we")?.as_f64()?,
            bpm: obj.get("bpm")?.as_f64()?,
            confidence: obj.get("conf")?.as_f64()?,
            beats: dl("beats")?,
            downbeats: dl("down")?,
            audible_start: obj.get("as")?.as_f64()?,
            content_end: obj.get("ce")?.as_f64()?,
            energy_db: dl("e")?,
            energy_hop: obj.get("eh")?.as_f64()?,
        })
    }
}

/// Input for `analyze_pcm` — a plain struct so it can cross thread boundaries.
#[derive(Debug, Clone)]
pub struct PcmWindow {
    /// Mono signed 16-bit PCM.
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub offset_seconds: f64,
}

impl PcmWindow {
    pub fn new(samples: Vec<i16>, sample_rate: u32, offset_seconds: f64) -> Self {
        Self {
            samples,
            sample_rate,
            offset_seconds,
        }
    }
}

pub fn analyze_pcm_window(w: &PcmWindow) -> TrackAnalysis {
    analyze_pcm(&w.samples, w.sample_rate, w.offset_seconds)
}

const FFT_SIZE: usize = 512;
const HOP: usize = 128;

/// Tempo, beat grid, downbeats and audible bounds of a mono PCM window.
///
/// Onset strength is log-magnitude spectral flux; tempo is the
/// autocorrelation peak of that envelope (with harmonics, weighted
/// toward 120 BPM); the grid is the best comb phase, then refined by a
/// least-squares fit through the real onset peaks so a small tempo
/// error does not drift across a 60s window.
pub fn analyze_pcm(pcm: &[i16], sample_rate: u32, offset_seconds: f64) -> TrackAnalysis {
    let sr = sample_rate as f64;
    let window_end = offset_seconds + pcm.len() as f64 / sr;
    let fps = sr / HOP as f64;
    let n_frames = if pcm.len() < FFT_SIZE {
        0
    } else {
        1 + (pcm.len() - FFT_SIZE) / HOP
    };

    // ---- levels -----------------------------------------------------------
    let energy_hop = 0.5_f64;
    let e_len = ((pcm.len() as f64) / (sr * energy_hop)).ceil().max(1.0) as usize;
    let mut energy_db = vec![-120.0_f64; e_len];
    let e_win = (sr * energy_hop).round() as usize;
    if e_win > 0 {
        for (i, slot) in energy_db.iter_mut().enumerate() {
            let s = i * e_win;
            let e = pcm.len().min(s + e_win);
            if e <= s {
                break;
            }
            let mut acc = 0.0_f64;
            for &sample in &pcm[s..e] {
                let v = sample as f64 / 32768.0;
                acc += v * v;
            }
            let rms = (acc / (e - s) as f64).sqrt();
            *slot = if rms <= 1e-6 {
                -120.0
            } else {
                20.0 * rms.ln() / LN_10
            };
        }
    }
    let (audible_start, content_end) = audible_bounds(pcm, sample_rate, offset_seconds);

    if (n_frames as f64) < fps * 8.0 {
        return TrackAnalysis {
            window_start: offset_seconds,
            window_end,
            bpm: 0.0,
            confidence: 0.0,
            beats: Vec::new(),
            downbeats: Vec::new(),
            audible_start,
            content_end,
            energy_db,
            energy_hop,
        };
    }

    // ---- onset envelope (spectral flux) -----------------------------------
    let mut window = vec![0.0_f64; FFT_SIZE];
    for (i, w) in window.iter_mut().enumerate() {
        *w = 0.5 - 0.5 * (2.0 * PI * i as f64 / (FFT_SIZE - 1) as f64).cos();
    }
    let half = FFT_SIZE / 2;
    let low_bins = ((150.0 / (sr / FFT_SIZE as f64)).floor() as usize).max(2);
    let mut re = vec![0.0_f64; FFT_SIZE];
    let mut im = vec![0.0_f64; FFT_SIZE];
    let mut prev = vec![0.0_f64; half];
    let mut cur = vec![0.0_f64; half];
    let mut flux = vec![0.0_f64; n_frames];
    let mut low_flux = vec![0.0_f64; n_frames];
    for f in 0..n_frames {
        let base = f * HOP;
        for i in 0..FFT_SIZE {
            re[i] = (pcm[base + i] as f64 / 32768.0) * window[i];
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        let mut sum = 0.0_f64;
        let mut low = 0.0_f64;
        for b in 1..half {
            let mag = (re[b] * re[b] + im[b] * im[b]).sqrt();
            let v = (1.0 + 100.0 * mag).ln();
            cur[b] = v;
            let d = v - prev[b];
            if d > 0.0 {
                sum += d;
                if b <= low_bins {
                    low += d;
                }
            }
        }
        flux[f] = if f == 0 { 0.0 } else { sum };
        low_flux[f] = if f == 0 { 0.0 } else { low };
        std::mem::swap(&mut prev, &mut cur);
    }
    let detrend_win = (fps * 0.4).round() as usize;
    let full_onset = detrend(&flux, detrend_win);
    let low_onset = detrend(&low_flux, detrend_win);

    // Beats live in the low end (kick, bass); broadband flux alone locks
    // just as happily onto off-beat hi-hats. Blend the two, each scaled to
    // its own mean, with the low band dominant.
    let mut onset = vec![0.0_f64; full_onset.len()];
    {
        let mut mf = 0.0_f64;
        let mut ml = 0.0_f64;
        for i in 0..full_onset.len() {
            mf += full_onset[i];
            ml += low_onset[i];
        }
        mf = if mf <= 1e-12 {
            1.0
        } else {
            mf / full_onset.len() as f64
        };
        ml = if ml <= 1e-12 {
            0.0
        } else {
            ml / low_onset.len() as f64
        };
        for i in 0..onset.len() {
            onset[i] = full_onset[i] / mf + if ml > 0.0 { 2.5 * low_onset[i] / ml } else { 0.0 };
        }
    }

    // ---- tempo ------------------------------------------------------------
    let min_lag = (fps * 60.0 / 200.0).floor() as usize;
    let max_lag = (onset.len() / 3).min((fps * 60.0 / 60.0).ceil() as usize);
    if max_lag <= min_lag + 2 {
        return no_beat(
            offset_seconds,
            window_end,
            audible_start,
            content_end,
            energy_db,
            energy_hop,
        );
    }
    let acf_alloc = (max_lag * 4 + 2).max(onset.len() / 2);
    let mut acf = vec![0.0_f64; acf_alloc];
    let acf_len = acf.len().min(onset.len().saturating_sub(1));
    let zero: f64 = onset.iter().map(|&x| x * x).sum();
    if zero <= 1e-12 {
        return no_beat(
            offset_seconds,
            window_end,
            audible_start,
            content_end,
            energy_db,
            energy_hop,
        );
    }
    for lag in 1..acf_len {
        let mut s = 0.0_f64;
        for t in 0..(onset.len() - lag) {
            s += onset[t] * onset[t + lag];
        }
        acf[lag] = s / zero;
    }
    let acf_at = |lag: f64| -> f64 {
        let i = lag.floor() as isize;
        if i < 1 || (i as usize) + 1 >= acf_len {
            return 0.0;
        }
        let idx = i as usize;
        let fr = lag - i as f64;
        acf[idx] * (1.0 - fr) + acf[idx + 1] * fr
    };

    // Score every lag: autocorrelation plus its harmonics, with a mild
    // prior toward 120 BPM. Keep the strongest few local peaks: rival
    // readings of one groove (3:4, 2:3) score within a few percent of each
    // other, so the winner is picked on how well its grid holds, below.
    let mut scores = vec![0.0_f64; max_lag + 2];
    for lag in min_lag..=max_lag {
        let bpm = 60.0 * fps / lag as f64;
        let lg = (bpm / 120.0).ln() / LN_2;
        let prior = (-0.5 * (lg / 0.8) * (lg / 0.8)).exp();
        let l = lag as f64;
        scores[lag] = (acf[lag]
            + 0.5 * acf_at(l * 2.0)
            + 0.33 * acf_at(l * 3.0)
            + 0.25 * acf_at(l * 4.0))
            * (0.5 + prior);
    }
    let mut peaks = Vec::<usize>::new();
    for lag in (min_lag + 1)..max_lag {
        if scores[lag] >= scores[lag - 1] && scores[lag] > scores[lag + 1] {
            peaks.push(lag);
        }
    }
    peaks.sort_by(|&x, &y| {
        scores[y]
            .partial_cmp(&scores[x])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut candidates = Vec::<usize>::new();
    if let Some(&first_peak) = peaks.first() {
        for &l in &peaks {
            if candidates
                .iter()
                .any(|&c| (c as f64 - l as f64).abs() <= c as f64 * 0.04)
            {
                continue;
            }
            if scores[l] < scores[first_peak] * 0.6 {
                break;
            }
            candidates.push(l);
            if candidates.len() == 3 {
                break;
            }
        }
    }
    if candidates.is_empty() {
        return no_beat(
            offset_seconds,
            window_end,
            audible_start,
            content_end,
            energy_db,
            energy_hop,
        );
    }

    let first_cand_score = scores[candidates[0]];
    let mut best: Option<Grid> = None;
    for &lag in &candidates {
        if let Some(mut g) = fit_grid(&onset, &acf, acf_len, lag) {
            // Stability decides; the autocorrelation score breaks near-ties.
            let rank = g.stability + 0.1 * scores[lag] / first_cand_score;
            if best.as_ref().is_none_or(|b| rank > b.rank) {
                g.rank = rank;
                best = Some(g);
            }
        }
    }
    let Some(best) = best else {
        return no_beat(
            offset_seconds,
            window_end,
            audible_start,
            content_end,
            energy_db,
            energy_hop,
        );
    };

    let period = best.period;
    let phase = best.phase;
    // A steady grid on which onsets clearly pile up. Noise reaches an
    // onset contrast of ~1.4 at most; real grooves sit at 2-15.
    let confidence = best.stability * ((best.contrast - 1.5) / 1.5).clamp(0.0, 1.0);

    // ---- grid, downbeats ---------------------------------------------------
    let mut beats = Vec::<f64>::new();
    let mut beat_frames = Vec::<f64>::new();
    let mut p = phase;
    while p < onset.len() as f64 {
        beat_frames.push(p);
        // Frame f covers samples [f*hop, f*hop+fft); its onset is centred.
        beats.push(offset_seconds + (p * HOP as f64 + FFT_SIZE as f64 / 2.0) / sr);
        p += period;
    }
    let mut bar_score = [0.0_f64; 4];
    for (i, &bf) in beat_frames.iter().enumerate() {
        bar_score[i % 4] += sample_at(&low_onset, bf) + 0.1 * sample_at(&full_onset, bf);
    }
    let mut bar = 0usize;
    for i in 1..4 {
        if bar_score[i] > bar_score[bar] {
            bar = i;
        }
    }
    let downbeats = (bar..beats.len()).step_by(4).map(|i| beats[i]).collect();

    TrackAnalysis {
        window_start: offset_seconds,
        window_end,
        bpm: 60.0 * fps / period,
        confidence,
        beats,
        downbeats,
        audible_start,
        content_end,
        energy_db,
        energy_hop,
    }
}

fn sample_at(x: &[f64], pos: f64) -> f64 {
    let i = pos.floor() as isize;
    if i < 0 || (i as usize) + 1 >= x.len() {
        return 0.0;
    }
    let idx = i as usize;
    let fr = pos - i as f64;
    x[idx] * (1.0 - fr) + x[idx + 1] * fr
}

struct Grid {
    period: f64,
    phase: f64,
    stability: f64,
    contrast: f64,
    rank: f64,
}

/// Best comb phase for `period` over frames `[from, to)`.
fn best_phase(onset: &[f64], period: f64, from: usize, to: usize) -> f64 {
    let mut best = 0.0_f64;
    let mut best_score = -1e9_f64;
    let mut ph = 0.0_f64;
    while ph < period {
        let mut s = 0.0_f64;
        let mut n = 0usize;
        let from_f = from as f64;
        let start = from_f + (((ph - from_f) % period + period) % period);
        let mut p = start;
        while p < to as f64 {
            s += sample_at(onset, p);
            n += 1;
            p += period;
        }
        if n > 0 && s / n as f64 > best_score {
            best_score = s / n as f64;
            best = start % period;
        }
        ph += 0.5;
    }
    best
}

/// Refines a tempo candidate into a grid and measures how steady it is.
fn fit_grid(onset: &[f64], acf: &[f64], acf_len: usize, lag: usize) -> Option<Grid> {
    let mut period = lag as f64;
    if lag > 1 && lag + 1 < acf_len {
        let (a, b, c) = (acf[lag - 1], acf[lag], acf[lag + 1]);
        let den = a - 2.0 * b + c;
        if den.abs() > 1e-12 {
            let shift = 0.5 * (a - c) / den;
            if shift.abs() < 1.0 {
                period += shift;
            }
        }
    }
    // A peak at k·period pins the period k times more precisely.
    for mult in [8_usize, 4, 2] {
        let c = (period * mult as f64).round() as usize;
        if c + 4 >= acf_len || c < 3 {
            continue;
        }
        let mut bi = c;
        for i in (c - 3)..=(c + 3) {
            if acf[i] > acf[bi] {
                bi = i;
            }
        }
        let mut peak = bi as f64;
        if bi > 0 && bi + 1 < acf_len {
            let (a, b, cc) = (acf[bi - 1], acf[bi], acf[bi + 1]);
            let den = a - 2.0 * b + cc;
            if den.abs() > 1e-12 {
                let shift = 0.5 * (a - cc) / den;
                if shift.abs() < 1.0 {
                    peak += shift;
                }
            }
        }
        let refined = peak / mult as f64;
        if (refined - period).abs() < period * 0.03 {
            period = refined;
        }
        break;
    }

    let mut phase = best_phase(onset, period, 0, onset.len());

    // Least squares through the real onset peaks near each predicted beat.
    let mut sorted = onset.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let floor = sorted[(sorted.len() as f64 * 0.6).floor() as usize];
    for pass in 0..3 {
        let mut ks = Vec::<f64>::new();
        let mut ts = Vec::<f64>::new();
        let radius = ((period * if pass == 0 { 0.2 } else { 0.12 }).round() as isize).max(1);
        let mut k = 0usize;
        let mut p = phase;
        while p < onset.len() as f64 {
            let c = p.round() as isize;
            let mut bi: isize = -1;
            let mut bv = floor;
            for i in (c - radius)..=(c + radius) {
                if i <= 0 || (i as usize) + 1 >= onset.len() {
                    continue;
                }
                let iu = i as usize;
                let v = onset[iu];
                if v > bv && v >= onset[iu - 1] && v >= onset[iu + 1] {
                    bv = v;
                    bi = i;
                }
            }
            if bi >= 0 {
                ks.push(k as f64);
                ts.push(bi as f64);
            }
            p += period;
            k += 1;
        }
        if ks.len() < 8 {
            break;
        }
        let n = ks.len() as f64;
        let mk: f64 = ks.iter().sum::<f64>() / n;
        let mt: f64 = ts.iter().sum::<f64>() / n;
        let mut sxy = 0.0_f64;
        let mut sxx = 0.0_f64;
        for i in 0..ks.len() {
            sxy += (ks[i] - mk) * (ts[i] - mt);
            sxx += (ks[i] - mk) * (ks[i] - mk);
        }
        if sxx <= 0.0 {
            break;
        }
        let slope = sxy / sxx;
        if (slope - period).abs() >= period * 0.04 {
            break;
        }
        period = slope;
        phase = mt - slope * mk;
        phase = (phase % period + period) % period;
    }

    // Stability: split into ~10 s sections and find each section's own
    // best phase for this period.
    const SECTION_FRAMES: usize = 861; // ≈10 s at 86 frames/s
    let sections = (onset.len() / SECTION_FRAMES).max(1);
    let len = onset.len() / sections;
    if (len as f64) < period * 6.0 {
        return None;
    }
    let phases: Vec<f64> = (0..sections)
        .map(|s| {
            let end = if s == sections - 1 {
                onset.len()
            } else {
                (s + 1) * len
            };
            best_phase(onset, period, s * len, end)
        })
        .collect();

    let mut agree = 0usize;
    for &c in &phases {
        let mut n = 0usize;
        for &q in &phases {
            let mut d = (q - c).abs() % period;
            if d > period / 2.0 {
                d = period - d;
            }
            if d <= period * 0.06 {
                n += 1;
            }
        }
        agree = agree.max(n);
    }
    let share = agree as f64 / sections as f64;
    let stability = if sections < 4 {
        if share >= 1.0 {
            0.5
        } else {
            0.0
        }
    } else {
        ((share - 0.4) / 0.6).clamp(0.0, 1.0)
    };

    // How much onset energy sits on the grid points (vs. the average).
    let mut on = 0.0_f64;
    let mut cnt = 0usize;
    let mut p = phase;
    while p < onset.len() as f64 {
        on += sample_at(onset, p);
        cnt += 1;
        p += period;
    }
    let mean = onset.iter().sum::<f64>() / onset.len() as f64;
    let contrast = if mean <= 1e-12 || cnt == 0 {
        0.0
    } else {
        (on / cnt as f64) / mean
    };
    Some(Grid {
        period,
        phase,
        stability,
        contrast,
        rank: 0.0,
    })
}

fn no_beat(
    ws: f64,
    we: f64,
    audible_start: f64,
    content_end: f64,
    energy_db: Vec<f64>,
    energy_hop: f64,
) -> TrackAnalysis {
    TrackAnalysis {
        window_start: ws,
        window_end: we,
        bpm: 0.0,
        confidence: 0.0,
        beats: Vec::new(),
        downbeats: Vec::new(),
        audible_start,
        content_end,
        energy_db,
        energy_hop,
    }
}

/// First / last 50 ms block above (loudest block − 40 dB), floored at
/// −55 dBFS, so tape hiss and reverb tails do not count as music.
fn audible_bounds(pcm: &[i16], sr: u32, offset: f64) -> (f64, f64) {
    let sr_f = sr as f64;
    let block = (sr as usize / 20).max(1);
    let n = pcm.len() / block;
    if n == 0 {
        return (offset, offset);
    }
    let mut db = vec![-120.0_f64; n];
    let mut peak = -120.0_f64;
    for b in 0..n {
        let mut acc = 0.0_f64;
        for &sample in &pcm[(b * block)..((b + 1) * block)] {
            let v = sample as f64 / 32768.0;
            acc += v * v;
        }
        let rms = (acc / block as f64).sqrt();
        db[b] = if rms <= 1e-6 {
            -120.0
        } else {
            20.0 * rms.ln() / LN_10
        };
        if db[b] > peak {
            peak = db[b];
        }
    }
    let thr = (peak - 40.0).max(-55.0);
    let mut first = 0usize;
    while first < n && db[first] < thr {
        first += 1;
    }
    let mut last = n - 1;
    while last > first && db[last] < thr {
        last -= 1;
    }
    if first >= n {
        return (offset, offset + pcm.len() as f64 / sr_f);
    }
    (
        offset + (first * block) as f64 / sr_f,
        offset + ((last + 1) * block) as f64 / sr_f,
    )
}

/// Subtract a moving average and half-wave rectify.
fn detrend(x: &[f64], win: usize) -> Vec<f64> {
    let mut out = vec![0.0_f64; x.len()];
    if x.is_empty() {
        return out;
    }
    let w = win.max(1);
    let mut acc = 0.0_f64;
    let mut pre = vec![0.0_f64; x.len() + 1];
    for (i, &val) in x.iter().enumerate() {
        acc += val;
        pre[i + 1] = acc;
    }
    for i in 0..x.len() {
        let a = i.saturating_sub(w);
        let b = x.len().min(i + w + 1);
        let avg = (pre[b] - pre[a]) / (b - a) as f64;
        let v = x[i] - avg;
        out[i] = if v > 0.0 { v } else { 0.0 };
    }
    out
}

/// In-place iterative radix-2 FFT.
pub(crate) fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while (j & bit) != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2usize;
    while len <= n {
        let ang = -2.0 * PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        let half = len / 2;
        for i in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0_f64, 0.0_f64);
            for j in 0..half {
                let (ar, ai) = (re[i + j], im[i + j]);
                let (br, bi) = (re[i + j + half], im[i + j + half]);
                let tr = br * cr - bi * ci;
                let ti = br * ci + bi * cr;
                re[i + j] = ar + tr;
                im[i + j] = ai + ti;
                re[i + j + half] = ar - tr;
                im[i + j + half] = ai - ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_click_track(bpm: f64, sr: u32, seconds: f64) -> Vec<i16> {
        let len = (sr as f64 * seconds) as usize;
        let mut pcm = vec![0_i16; len];
        let beat_interval = 60.0 / bpm;
        let mut t = 0.25_f64;
        while t < seconds {
            let start = (t * sr as f64) as usize;
            // Short 80 Hz kick-like transient + high click
            let dur = (0.04 * sr as f64) as usize;
            for i in 0..dur {
                if start + i >= len {
                    break;
                }
                let env = (-8.0 * i as f64 / dur as f64).exp();
                let s = (2.0 * PI * 80.0 * i as f64 / sr as f64).sin() * 0.7
                    + (2.0 * PI * 1200.0 * i as f64 / sr as f64).sin() * 0.2;
                pcm[start + i] = (s * env * 28000.0).clamp(-32767.0, 32767.0) as i16;
            }
            t += beat_interval;
        }
        pcm
    }

    #[test]
    fn silence_has_no_beats_and_zero_bpm() {
        let pcm = vec![0_i16; 11025 * 12];
        let res = analyze_pcm(&pcm, 11025, 10.0);
        assert_eq!(res.bpm, 0.0);
        assert!(!res.has_beat());
        assert_eq!(res.window_start, 10.0);
        assert!((res.window_end - 22.0).abs() < 0.01);
    }

    #[test]
    fn detects_120_bpm_click_track_and_roundtrips_json() {
        let sr = 11025;
        let pcm = generate_click_track(120.0, sr, 42.0);
        let res = analyze_pcm(&pcm, sr, 5.0);
        assert!(res.bpm > 115.0 && res.bpm < 125.0, "got bpm={}", res.bpm);
        assert!(res.has_beat(), "conf={} beats={}", res.confidence, res.beats.len());
        assert!(!res.downbeats.is_empty());

        let json = res.to_json();
        let parsed = TrackAnalysis::from_json(&json).expect("roundtrip json");
        assert_eq!(parsed, res);
    }
}
