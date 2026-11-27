/// ReplayGain-style loudness correction aimed at about -14 LUFS.
///
/// Tag math follows LastWave-Native `LoudnessNormalizer`: stored
/// ReplayGain already targets the -18 LUFS reference, so a +4 dB preamp
/// lands near -14 LUFS. This module does not measure loudness. When a
/// stream or file exposes no gain tag, playback uses [`LoudnessMath::FALLBACK_DB`].
pub struct LoudnessMath;

impl LoudnessMath {
    pub const TARGET_LUFS: f64 = -14.0;
    pub const REPLAY_GAIN_REFERENCE_LUFS: f64 = -18.0;

    /// Added on top of a real ReplayGain tag. Not applied to [`Self::FALLBACK_DB`]
    /// (mpv's `replaygain-fallback` replaces the preamp).
    pub const PREAMP_DB: f64 = Self::TARGET_LUFS - Self::REPLAY_GAIN_REFERENCE_LUFS;

    /// Conservative attenuation when no ReplayGain metadata is exposed.
    /// A boost would push already-loud streams past full scale.
    pub const FALLBACK_DB: f64 = -3.0;

    pub const CLAMP_DB: f64 = 15.0;

    /// Two album-gain readings count as the same tag when they are this
    /// close. Farther apart, album mode does not force one queue gain.
    pub const SHARED_ALBUM_TOLERANCE_DB: f64 = 0.25;

    pub const MODES: [&'static str; 3] = ["off", "track", "album"];

    pub fn normalize_mode(raw: Option<&str>) -> &'static str {
        let id = raw.unwrap_or("").trim().to_ascii_lowercase();
        match id.as_str() {
            "off" => "off",
            "track" => "track",
            "album" => "album",
            _ => "off",
        }
    }

    /// Parsed ReplayGain in dB, or `None` when the string is missing or
    /// implausible. Accepts `"-7.5 dB"` and `"3.0"`.
    pub fn parse_db(value: Option<&str>) -> Option<f64> {
        let raw = value?;
        let mut text = raw.trim().to_ascii_lowercase();
        if text.is_empty() || text == "null" || text == "none" {
            return None;
        }
        if let Some(stripped) = text.strip_suffix("db") {
            text = stripped.trim().to_string();
        }
        let parsed = text.parse::<f64>().ok()?;
        if !parsed.is_finite() {
            return None;
        }
        if !(-51.0..=51.0).contains(&parsed) {
            return None;
        }
        Some(parsed)
    }

    /// Track or album tag to feed ReplayGain, before preamp. `None` when
    /// this mode has nothing to apply (`off`, or no gain tag). `album` prefers
    /// the album tag and falls back to the track tag.
    pub fn tag_gain_db(
        mode: &str,
        track_gain_db: Option<f64>,
        album_gain_db: Option<f64>,
    ) -> Option<f64> {
        let norm = Self::normalize_mode(Some(mode));
        if norm == "off" {
            return None;
        }
        let selected = if norm == "album" {
            album_gain_db.or(track_gain_db)
        } else {
            track_gain_db.or(album_gain_db)
        }?;
        if !selected.is_finite() {
            return None;
        }
        Some(selected)
    }

    pub fn same_album_gain(a: Option<f64>, b: Option<f64>) -> bool {
        let (Some(a), Some(b)) = (a, b) else {
            return false;
        };
        if !a.is_finite() || !b.is_finite() {
            return false;
        }
        (a - b).abs() <= Self::SHARED_ALBUM_TOLERANCE_DB
    }

    /// Tag gain plus the -14 LUFS preamp, clamped, then lowered so a
    /// known peak stays at or under 0 dBFS. `None` tag means "use fallback",
    /// which is the caller's job — this returns `None` rather than inventing
    /// a measurement.
    pub fn correction_db(
        mode: &str,
        track_gain_db: Option<f64>,
        album_gain_db: Option<f64>,
        peak_linear: Option<f64>,
    ) -> Option<f64> {
        let selected = Self::tag_gain_db(mode, track_gain_db, album_gain_db)?;
        let mut gain = (selected + Self::PREAMP_DB).clamp(-Self::CLAMP_DB, Self::CLAMP_DB);
        if let Some(peak) = peak_linear {
            if peak.is_finite() && peak > 0.0 {
                let ceiling = 20.0 * (1.0 / peak).log10();
                if ceiling.is_finite() && gain > ceiling {
                    gain = ceiling;
                }
            }
        }
        Some(gain)
    }

    pub fn settings_subtitle(mode: &str) -> String {
        let fallback = format!("{:.0}", Self::FALLBACK_DB);
        match Self::normalize_mode(Some(mode)) {
            "track" => format!(
                "Targets about -14 LUFS when the file exposes ReplayGain (+{:.0} dB on the -18 LUFS reference). No gain tag: fixed {} dB, not a loudness scan.",
                Self::PREAMP_DB,
                fallback,
            ),
            "album" => format!(
                "Uses one album ReplayGain for the current album when those tags agree; otherwise the track gain. No gain tag: fixed {} dB, not a loudness scan.",
                fallback,
            ),
            _ => "Leaves the decoded level alone.".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LoudnessMath;

    #[test]
    fn normalize_mode_handles_known_and_unknown() {
        assert_eq!(LoudnessMath::normalize_mode(Some(" TRACK ")), "track");
        assert_eq!(LoudnessMath::normalize_mode(Some("Album")), "album");
        assert_eq!(LoudnessMath::normalize_mode(Some("invalid")), "off");
        assert_eq!(LoudnessMath::normalize_mode(None), "off");
    }

    #[test]
    fn parse_db_accepts_units_and_rejects_implausible() {
        assert_eq!(LoudnessMath::parse_db(Some("-7.5 dB")), Some(-7.5));
        assert_eq!(LoudnessMath::parse_db(Some(" 3.0 ")), Some(3.0));
        assert_eq!(LoudnessMath::parse_db(Some("none")), None);
        assert_eq!(LoudnessMath::parse_db(Some("null")), None);
        assert_eq!(LoudnessMath::parse_db(Some("")), None);
        assert_eq!(LoudnessMath::parse_db(Some("-52.0")), None);
        assert_eq!(LoudnessMath::parse_db(Some("52.0")), None);
        assert_eq!(LoudnessMath::parse_db(Some("NaN")), None);
    }

    #[test]
    fn tag_gain_and_same_album_gain() {
        assert_eq!(LoudnessMath::tag_gain_db("off", Some(-5.0), Some(-6.0)), None);
        assert_eq!(
            LoudnessMath::tag_gain_db("track", Some(-5.0), Some(-6.0)),
            Some(-5.0)
        );
        assert_eq!(
            LoudnessMath::tag_gain_db("album", Some(-5.0), Some(-6.0)),
            Some(-6.0)
        );
        assert_eq!(
            LoudnessMath::tag_gain_db("album", Some(-5.0), None),
            Some(-5.0)
        );
        assert!(LoudnessMath::same_album_gain(Some(-6.0), Some(-6.2)));
        assert!(!LoudnessMath::same_album_gain(Some(-6.0), Some(-6.3)));
        assert!(!LoudnessMath::same_album_gain(Some(-6.0), None));
    }

    #[test]
    fn correction_db_applies_preamp_clamp_and_peak_ceiling() {
        // -7.0 + 4.0 preamp = -3.0
        assert_eq!(
            LoudnessMath::correction_db("track", Some(-7.0), None, None),
            Some(-3.0)
        );
        // Clamped to +15.0
        assert_eq!(
            LoudnessMath::correction_db("track", Some(14.0), None, None),
            Some(15.0)
        );
        // Peak 1.0 -> 0 dBFS ceiling caps +2.0 to 0.0
        let capped = LoudnessMath::correction_db("track", Some(-2.0), None, Some(1.0)).unwrap();
        assert!((capped - 0.0).abs() < 1e-9);
    }

    #[test]
    fn settings_subtitle_matches_modes() {
        assert_eq!(
            LoudnessMath::settings_subtitle("off"),
            "Leaves the decoded level alone."
        );
        assert!(LoudnessMath::settings_subtitle("track").contains("-14 LUFS"));
        assert!(LoudnessMath::settings_subtitle("album").contains("album ReplayGain"));
    }
}
