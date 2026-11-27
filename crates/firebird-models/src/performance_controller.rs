//! Performance mode state, hardware heuristics, and optimization descriptors.

use serde::{Deserialize, Serialize};

/// Dual core with hyperthreading or older 2–4 core CPUs (e.g. i3-2105).
pub const MODEST_HARDWARE_MAX_CORES: usize = 4;

/// Returns `true` when the reported logical processor count falls within
/// [`MODEST_HARDWARE_MAX_CORES`].
pub const fn is_modest_hardware_for_cores(cores: usize) -> bool {
    cores <= MODEST_HARDWARE_MAX_CORES
}

/// Simple, honest hardware detection for modest systems (e.g. dual-core /
/// quad-thread CPUs, integrated graphics, budget laptops).
pub fn is_modest_hardware() -> bool {
    std::thread::available_parallelism()
        .map(|n| is_modest_hardware_for_cores(n.get()))
        .unwrap_or(false)
}

/// Persisted / runtime snapshot of Performance Mode state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceState {
    #[serde(default)]
    pub performance_mode: bool,
    #[serde(default)]
    pub prompt_dismissed: bool,
    #[serde(default)]
    pub is_modest_hardware: bool,
}

/// Explainer item describing what Performance Mode optimizes without
/// technical AI-jargon.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceOptimizationItem {
    pub title: String,
    pub subtitle: String,
    pub impact: String,
    pub active: bool,
}

/// Build the 5 Performance Mode optimization explainer items for `is_perf_on`.
pub fn performance_optimizations(is_perf_on: bool) -> Vec<PerformanceOptimizationItem> {
    vec![
        PerformanceOptimizationItem {
            title: "Glass Blur & Shaders".to_string(),
            subtitle: if is_perf_on {
                "Replaced with clean, crisp surfaces (Zero GPU lag)"
            } else {
                "Multi-layer blur filters active on window and panels"
            }
            .to_string(),
            impact: "Reduces GPU render load & stops UI stutter".to_string(),
            active: is_perf_on,
        },
        PerformanceOptimizationItem {
            title: "Background Video Artwork (Canvas)".to_string(),
            subtitle: if is_perf_on {
                "Paused (Displaying high-resolution static album covers)"
            } else {
                "Continuously decoding looping video clips in background"
            }
            .to_string(),
            impact: "Saves 30%–50% CPU usage during playback".to_string(),
            active: is_perf_on,
        },
        PerformanceOptimizationItem {
            title: "Background Mesh & Glow".to_string(),
            subtitle: if is_perf_on {
                "Replaced with smooth static gradient wash"
            } else {
                "Multi-layer mesh painters animating continuously"
            }
            .to_string(),
            impact: "Stops continuous off-screen render loops".to_string(),
            active: is_perf_on,
        },
        PerformanceOptimizationItem {
            title: "Animated Lyrics".to_string(),
            subtitle: if is_perf_on {
                "Smooth line-by-line scrolling"
            } else {
                "High-refresh per-word syllable animations"
            }
            .to_string(),
            impact: "Reduces screen redraw frequency".to_string(),
            active: is_perf_on,
        },
        PerformanceOptimizationItem {
            title: "Audio Visualizer Bars".to_string(),
            subtitle: if is_perf_on {
                "Paused"
            } else {
                "Live audio frequency analysis on UI thread"
            }
            .to_string(),
            impact: "Frees up CPU cores for music playback".to_string(),
            active: is_perf_on,
        },
    ]
}
