//! Canvas panel clip models (Spotify Canvas & animated motion artwork).

use serde::{Deserialize, Serialize};

/// What the Canvas panel plays for a track.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPanelClip {
    pub url: String,
    #[serde(default)]
    pub fallback_url: String,
    /// Provider label shown on the panel ("Spotify", "Apple Music", "Tidal", "Community").
    #[serde(default)]
    pub source: String,
    /// True when the clip is a real vertical canvas. Square album motion
    /// art is cover-cropped into the 9:16 frame instead.
    #[serde(default)]
    pub vertical: bool,
}

/// A track identity the panel resolves a clip for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPanelQuery {
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration_ms: i64,
}

/// The panel's answer: a clip, or a note saying why there is none.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasPanelOutcome {
    #[serde(default)]
    pub clip: Option<CanvasPanelClip>,
    #[serde(default)]
    pub note: String,
}

impl CanvasPanelOutcome {
    pub fn with_clip(clip: CanvasPanelClip) -> Self {
        Self {
            clip: Some(clip),
            note: String::new(),
        }
    }

    pub fn with_note(note: impl Into<String>) -> Self {
        Self {
            clip: None,
            note: note.into(),
        }
    }
}
