use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::pcm_format::PcmFormat;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DacDevice {
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_device_name")]
    pub name: String,
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub enumerator: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub exclusive_supported: bool,
    #[serde(default)]
    pub hardware_volume: bool,
    #[serde(default)]
    pub formats: Vec<PcmFormat>,
}

fn default_device_name() -> String {
    "Audio device".to_string()
}

impl Default for DacDevice {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: default_device_name(),
            manufacturer: String::new(),
            enumerator: String::new(),
            is_default: false,
            exclusive_supported: false,
            hardware_volume: false,
            formats: Vec::new(),
        }
    }
}

impl DacDevice {
    pub fn display_name(&self) -> String {
        if !self.manufacturer.is_empty()
            && !self
                .name
                .to_lowercase()
                .contains(&self.manufacturer.to_lowercase())
        {
            format!("{} · {}", self.manufacturer, self.name)
        } else {
            self.name.clone()
        }
    }

    pub fn mpv_device_name(&self) -> String {
        if self.id.is_empty() {
            "auto".to_string()
        } else if self.id.starts_with("wasapi/") {
            self.id.clone()
        } else {
            format!("wasapi/{}", self.id)
        }
    }

    pub fn supports(&self, format: &PcmFormat) -> bool {
        self.formats.iter().any(|f| f.matches(format))
    }

    pub fn supported_summary(&self) -> String {
        if self.formats.is_empty() {
            return "No exclusive PCM formats reported".to_string();
        }
        let mut by_depth: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
        for f in &self.formats {
            by_depth
                .entry(f.bit_depth)
                .or_default()
                .push(f.sample_rate_hz);
        }
        by_depth
            .into_iter()
            .map(|(d, mut rates)| {
                rates.sort_unstable();
                let formatted: Vec<String> = rates.into_iter().map(PcmFormat::rate_label).collect();
                format!("{d}-bit: {}", formatted.join(" / "))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
