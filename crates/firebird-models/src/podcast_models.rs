//! YouTube Music podcast surfaces. Episodes are normal video ids, so the
//! existing player can open them once a row is remembered for resume.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodcastShow {
    pub show_id: String,
    pub title: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub artwork_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodcastEpisode {
    pub video_id: String,
    pub title: String,
    #[serde(default)]
    pub show_id: String,
    #[serde(default)]
    pub show_title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub duration_ms: i64,
    #[serde(default)]
    pub published_label: String,
}

impl PodcastEpisode {
    pub fn with_show(&self, show_id: Option<String>, show_title: Option<String>) -> Self {
        Self {
            video_id: self.video_id.clone(),
            title: self.title.clone(),
            show_id: show_id.unwrap_or_else(|| self.show_id.clone()),
            show_title: show_title.unwrap_or_else(|| self.show_title.clone()),
            description: self.description.clone(),
            artwork_url: self.artwork_url.clone(),
            duration_ms: self.duration_ms,
            published_label: self.published_label.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodcastShelf {
    pub title: String,
    #[serde(default)]
    pub shows: Vec<PodcastShow>,
    #[serde(default)]
    pub episodes: Vec<PodcastEpisode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodcastShowPage {
    pub show: PodcastShow,
    #[serde(default)]
    pub episodes: Vec<PodcastEpisode>,
    #[serde(default)]
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodcastEpisodePage {
    #[serde(default)]
    pub episodes: Vec<PodcastEpisode>,
    #[serde(default)]
    pub continuation: Option<String>,
}
