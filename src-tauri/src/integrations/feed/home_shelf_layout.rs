//! Home shelf order & visibility layout persistence.
//!
//! Ported from `lib/features/feed/home_shelf_layout.dart`.

use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::storage::app_database::AppDatabase;

/// `kv_store` key for the home shelf order and hidden set.
pub const HOME_SHELF_LAYOUT_KEY: &str = "home_shelf_layout";

pub mod home_shelf_id {
    pub const MIXES: &str = "mixes";
    pub const YOUTUBE: &str = "youtube";
    pub const YOUTUBE_MORE: &str = "youtube_more";
    pub const QUICK_PICKS: &str = "quick_picks";
    pub const CONTINUE_LISTENING: &str = "continue";
    pub const BECAUSE: &str = "because";
    pub const ALBUMS: &str = "albums";
    pub const ARTISTS: &str = "artists";
    pub const CHARTS: &str = "charts";
    pub const FRESH: &str = "fresh";
    pub const RELEASES: &str = "releases";
    pub const POPULAR: &str = "popular";
    pub const PODCASTS: &str = "podcasts";
}

pub const HOME_SHELF_DEFAULTS: &[&str] = &[
    home_shelf_id::MIXES,
    home_shelf_id::YOUTUBE,
    home_shelf_id::QUICK_PICKS,
    home_shelf_id::CONTINUE_LISTENING,
    home_shelf_id::BECAUSE,
    home_shelf_id::ALBUMS,
    home_shelf_id::ARTISTS,
    home_shelf_id::CHARTS,
    home_shelf_id::FRESH,
    home_shelf_id::RELEASES,
    home_shelf_id::POPULAR,
    home_shelf_id::PODCASTS,
    home_shelf_id::YOUTUBE_MORE,
];

pub const HOME_SHELF_CONNECTED: &[&str] = &[
    home_shelf_id::MIXES,
    home_shelf_id::QUICK_PICKS,
    home_shelf_id::CONTINUE_LISTENING,
    home_shelf_id::YOUTUBE,
    home_shelf_id::BECAUSE,
    home_shelf_id::ARTISTS,
    home_shelf_id::YOUTUBE_MORE,
];

pub fn shelf_label(id: &str) -> &str {
    match id {
        home_shelf_id::MIXES => "Mixes",
        home_shelf_id::YOUTUBE => "YouTube Music",
        home_shelf_id::YOUTUBE_MORE => "More from YouTube Music",
        home_shelf_id::QUICK_PICKS => "Quick Picks",
        home_shelf_id::CONTINUE_LISTENING => "Continue Listening",
        home_shelf_id::BECAUSE => "Because You Listened",
        home_shelf_id::ALBUMS => "Albums",
        home_shelf_id::ARTISTS => "Artists",
        home_shelf_id::CHARTS => "Charts",
        home_shelf_id::FRESH => "Fresh Finds",
        home_shelf_id::RELEASES => "New Releases",
        home_shelf_id::POPULAR => "Popular",
        home_shelf_id::PODCASTS => "Podcasts",
        other => other,
    }
}

/// Saved order merged with `defaults`.
///
/// Ids present in `saved` keep that relative order. Ids `saved` never
/// mentions stay at their default index instead of being appended or dropped.
pub fn resolve_home_shelf_order(saved: &[String], defaults: &[&str]) -> Vec<String> {
    if saved.is_empty() || defaults.is_empty() {
        return defaults.iter().map(|s| s.to_string()).collect();
    }
    let known: HashSet<&str> = defaults.iter().copied().collect();
    let mut mentioned = Vec::new();
    let mut seen = HashSet::new();
    for id in saved {
        if known.contains(id.as_str()) && seen.insert(id.clone()) {
            mentioned.push(id.clone());
        }
    }
    if mentioned.is_empty() {
        return defaults.iter().map(|s| s.to_string()).collect();
    }
    if mentioned.len() == defaults.len() {
        return mentioned;
    }

    let mentioned_set: HashSet<&str> = mentioned.iter().map(String::as_str).collect();
    let mut placed: Vec<Option<String>> = vec![None; defaults.len()];
    for (i, &def) in defaults.iter().enumerate() {
        if !mentioned_set.contains(def) {
            placed[i] = Some(def.to_string());
        }
    }
    let mut cursor = 0usize;
    for id in mentioned {
        while cursor < placed.len() && placed[cursor].is_some() {
            cursor += 1;
        }
        if cursor >= placed.len() {
            break;
        }
        placed[cursor] = Some(id);
        cursor += 1;
    }
    placed.into_iter().flatten().collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeShelfLayout {
    pub order: Vec<String>,
    pub hidden: HashSet<String>,
}

impl HomeShelfLayout {
    pub fn fresh() -> Self {
        Self {
            order: HOME_SHELF_DEFAULTS.iter().map(|s| s.to_string()).collect(),
            hidden: HashSet::new(),
        }
    }

    pub fn visible_ids(&self) -> Vec<String> {
        self.order
            .iter()
            .filter(|id| !self.hidden.contains(*id))
            .cloned()
            .collect()
    }

    pub fn encode(&self) -> String {
        let hidden_vec: Vec<&String> = self.hidden.iter().collect();
        serde_json::json!({
            "order": self.order,
            "hidden": hidden_vec,
        })
        .to_string()
    }

    pub fn decode(raw: Option<&str>) -> Self {
        let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
            return Self::fresh();
        };
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw) else {
            return Self::fresh();
        };
        let saved: Vec<String> = map
            .get("order")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let known: HashSet<&str> = HOME_SHELF_DEFAULTS.iter().copied().collect();
        let hidden: HashSet<String> = map
            .get("hidden")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter(|s| known.contains(s))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        Self {
            order: resolve_home_shelf_order(&saved, HOME_SHELF_DEFAULTS),
            hidden,
        }
    }
}

pub struct HomeShelfLayoutController {
    db: Arc<AppDatabase>,
    state: RwLock<HomeShelfLayout>,
}

impl HomeShelfLayoutController {
    pub fn new(db: Arc<AppDatabase>) -> Self {
        let initial = HomeShelfLayout::decode(db.kv_get(HOME_SHELF_LAYOUT_KEY).as_deref());
        Self {
            db,
            state: RwLock::new(initial),
        }
    }

    pub async fn state(&self) -> HomeShelfLayout {
        self.state.read().await.clone()
    }

    pub async fn toggle(&self, id: &str) {
        let mut st = self.state.write().await;
        if !st.order.iter().any(|x| x == id) {
            return;
        }
        if !st.hidden.remove(id) {
            st.hidden.insert(id.to_string());
        }
        self.db.kv_set(HOME_SHELF_LAYOUT_KEY, &st.encode());
    }

    pub async fn reorder(&self, old_index: usize, new_index: usize) {
        let mut st = self.state.write().await;
        if old_index >= st.order.len() || new_index >= st.order.len() || old_index == new_index {
            return;
        }
        let id = st.order.remove(old_index);
        st.order.insert(new_index, id);
        self.db.kv_set(HOME_SHELF_LAYOUT_KEY, &st.encode());
    }

    pub async fn reset(&self) {
        let mut st = self.state.write().await;
        *st = HomeShelfLayout::fresh();
        self.db.kv_set(HOME_SHELF_LAYOUT_KEY, &st.encode());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_order_keeps_new_defaults_in_place() {
        let saved = vec!["charts".to_string(), "mixes".to_string()];
        let resolved = resolve_home_shelf_order(&saved, HOME_SHELF_DEFAULTS);
        assert_eq!(resolved[0], "charts");
        assert_eq!(resolved[1], "youtube");
        assert_eq!(resolved[2], "quick_picks");
        assert_eq!(resolved[7], "mixes");
        assert_eq!(resolved.len(), HOME_SHELF_DEFAULTS.len());
    }
}
