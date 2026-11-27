//! Built-in headphone corrections.
//!
//! A handful of well-known models, expressed as peaking filters mpv's
//! lavfi equalizer can run. These sit beside the 10-band EQ. They are
//! simplified curves in the direction of a Harman-style target, not a
//! download of the AutoEQ database and not a lab measurement of the
//! pair on your head.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadphoneBand {
    pub hz: f64,
    pub q: f64,
    pub gain_db: f64,
}

impl HeadphoneBand {
    pub const fn new(hz: f64, q: f64, gain_db: f64) -> Self {
        Self { hz, q, gain_db }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadphoneProfile {
    pub id: Cow<'static, str>,
    pub name: Cow<'static, str>,
    pub description: Cow<'static, str>,
    pub preamp_db: f64,
    pub bands: Cow<'static, [HeadphoneBand]>,
}

impl HeadphoneProfile {
    pub const fn new(
        id: &'static str,
        name: &'static str,
        description: &'static str,
        preamp_db: f64,
        bands: &'static [HeadphoneBand],
    ) -> Self {
        Self {
            id: Cow::Borrowed(id),
            name: Cow::Borrowed(name),
            description: Cow::Borrowed(description),
            preamp_db,
            bands: Cow::Borrowed(bands),
        }
    }

    pub fn is_off(&self) -> bool {
        self.id == "off" || self.bands.is_empty()
    }

    /// Extra lavfi graph. Empty when the profile is Off.
    pub fn lavfi_filter(&self) -> String {
        if self.is_off() {
            return String::new();
        }
        let parts: Vec<String> = self
            .bands
            .iter()
            .map(|band| {
                format!(
                    "equalizer=f={:.0}:t=q:w={:.2}:g={:.1}",
                    band.hz, band.q, band.gain_db
                )
            })
            .collect();
        format!("lavfi=[{}]", parts.join(","))
    }
}

pub struct HeadphoneProfiles;

impl HeadphoneProfiles {
    pub const OFF: HeadphoneProfile =
        HeadphoneProfile::new("off", "Off", "No headphone correction.", 0.0, &[]);

    pub const WH1000XM5: HeadphoneProfile = HeadphoneProfile::new(
        "sony-wh-1000xm5",
        "Sony WH-1000XM5",
        "Less sub-bass, a little more presence.",
        -1.0,
        &[
            HeadphoneBand::new(45.0, 0.70, -4.0),
            HeadphoneBand::new(180.0, 0.90, -1.5),
            HeadphoneBand::new(2800.0, 1.20, 2.5),
            HeadphoneBand::new(6500.0, 2.00, 1.0),
        ],
    );

    pub const QC_ULTRA: HeadphoneProfile = HeadphoneProfile::new(
        "bose-qc-ultra",
        "Bose QC Ultra",
        "Tames the bass shelf and the upper treble.",
        -1.0,
        &[
            HeadphoneBand::new(55.0, 0.70, -3.5),
            HeadphoneBand::new(300.0, 1.00, -1.0),
            HeadphoneBand::new(3200.0, 1.00, 2.0),
            HeadphoneBand::new(8500.0, 2.00, -2.0),
        ],
    );

    pub const AIRPODS_MAX: HeadphoneProfile = HeadphoneProfile::new(
        "airpods-max",
        "AirPods Max",
        "Light bass cut and a softer 8 kHz region.",
        -1.0,
        &[
            HeadphoneBand::new(50.0, 0.80, -2.5),
            HeadphoneBand::new(250.0, 1.00, -1.0),
            HeadphoneBand::new(3000.0, 1.20, 1.5),
            HeadphoneBand::new(7500.0, 2.50, -2.5),
        ],
    );

    pub const HD600: HeadphoneProfile = HeadphoneProfile::new(
        "sennheiser-hd600",
        "Sennheiser HD 600",
        "Bass lift and a cut on the 6 kHz peak.",
        -5.5,
        &[
            HeadphoneBand::new(105.0, 0.70, 5.5),
            HeadphoneBand::new(200.0, 0.80, -2.0),
            HeadphoneBand::new(5800.0, 3.00, -3.0),
            HeadphoneBand::new(10000.0, 0.70, -1.5),
        ],
    );

    pub const M50X: HeadphoneProfile = HeadphoneProfile::new(
        "ath-m50x",
        "Audio-Technica ATH-M50x",
        "Pulls back the V-shaped bass and treble.",
        -1.0,
        &[
            HeadphoneBand::new(60.0, 0.70, -4.5),
            HeadphoneBand::new(300.0, 1.00, 2.0),
            HeadphoneBand::new(4000.0, 1.50, -2.0),
            HeadphoneBand::new(10000.0, 2.00, -3.5),
        ],
    );

    pub const DT770: HeadphoneProfile = HeadphoneProfile::new(
        "dt-770",
        "Beyerdynamic DT 770",
        "Cuts the bass shelf and the 8 kHz spike.",
        -1.0,
        &[
            HeadphoneBand::new(55.0, 0.70, -3.0),
            HeadphoneBand::new(250.0, 1.00, 1.0),
            HeadphoneBand::new(7800.0, 3.50, -5.0),
            HeadphoneBand::new(12000.0, 1.50, -1.5),
        ],
    );

    pub const ALL: [HeadphoneProfile; 7] = [
        Self::OFF,
        Self::WH1000XM5,
        Self::QC_ULTRA,
        Self::AIRPODS_MAX,
        Self::HD600,
        Self::M50X,
        Self::DT770,
    ];

    pub fn by_id(id: Option<&str>) -> HeadphoneProfile {
        let key = id.unwrap_or("").trim().to_ascii_lowercase();
        Self::ALL
            .iter()
            .find(|p| p.id == key.as_str())
            .cloned()
            .unwrap_or(Self::OFF)
    }
}

#[cfg(test)]
mod tests {
    use super::HeadphoneProfiles;

    #[test]
    fn lookup_resolves_profiles_and_defaults_to_off() {
        assert_eq!(HeadphoneProfiles::by_id(None).id, "off");
        assert_eq!(HeadphoneProfiles::by_id(Some("  ")).id, "off");
        assert_eq!(HeadphoneProfiles::by_id(Some("unknown")).id, "off");
        assert_eq!(
            HeadphoneProfiles::by_id(Some(" SENNHEISER-HD600 ")).name,
            "Sennheiser HD 600"
        );
    }

    #[test]
    fn off_has_empty_lavfi_and_active_profiles_build_peaking_filters() {
        assert!(HeadphoneProfiles::OFF.is_off());
        assert_eq!(HeadphoneProfiles::OFF.lavfi_filter(), "");

        let hd600 = HeadphoneProfiles::HD600;
        assert!(!hd600.is_off());
        assert_eq!(
            hd600.lavfi_filter(),
            "lavfi=[equalizer=f=105:t=q:w=0.70:g=5.5,equalizer=f=200:t=q:w=0.80:g=-2.0,equalizer=f=5800:t=q:w=3.00:g=-3.0,equalizer=f=10000:t=q:w=0.70:g=-1.5]"
        );
    }
}
