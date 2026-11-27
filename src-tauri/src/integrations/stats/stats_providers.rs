//! Local SQLite listening stats aggregator.
//!
//! Ported from `lib/features/stats/stats_providers.dart`.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Duration as ChronoDuration, Local};
use serde::{Deserialize, Serialize};

use crate::integrations::artwork::official_artwork_service::OfficialArtworkService;
use crate::models::artist_parser::split_artist_credits;
use crate::storage::app_database::AppDatabase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StatsPeriod {
    #[default]
    Week,
    Month,
    AllTime,
}

impl StatsPeriod {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Week => "7 Days",
            Self::Month => "30 Days",
            Self::AllTime => "All Time",
        }
    }

    pub fn days(&self) -> Option<i64> {
        match self {
            Self::Week => Some(7),
            Self::Month => Some(30),
            Self::AllTime => None,
        }
    }

    pub fn last_fm_period(&self) -> &'static str {
        match self {
            Self::Week => "7day",
            Self::Month => "1month",
            Self::AllTime => "overall",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatTrack {
    pub name: String,
    pub artist: String,
    pub artwork_url: String,
    pub play_count: i64,
    pub timestamp_millis: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatArtist {
    pub name: String,
    pub play_count: i64,
    pub artwork_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatAlbum {
    pub name: String,
    pub artist: String,
    pub artwork_url: String,
    pub play_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentStatPlay {
    pub title: String,
    pub artist: String,
    pub timestamp_millis: i64,
    pub artwork_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyActivityPoint {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListeningStatsData {
    pub period: StatsPeriod,
    pub total_plays: i64,
    pub total_minutes_listened: i64,
    pub has_estimated_duration: bool,
    pub prev_period_plays: Option<i64>,
    pub prev_period_minutes_listened: Option<i64>,
    pub unique_artists: i64,
    pub unique_tracks: i64,
    pub current_streak_days: i64,
    pub peak_listening_hour: i32,
    pub peak_listening_label: String,
    pub top_artists: Vec<StatArtist>,
    pub top_tracks: Vec<StatTrack>,
    pub top_albums: Vec<StatAlbum>,
    pub recent_plays: Vec<RecentStatPlay>,
    pub hourly_distribution: HashMap<i32, i64>,
    pub daily_activity: Vec<DailyActivityPoint>,
    pub is_last_fm_connected: bool,
    pub last_fm_username: String,
    pub scrobbler_name: String,
    pub last_fm_scrobbles: i64,
    pub last_fm_artists_count: i64,
    pub last_fm_albums_count: i64,
    pub last_fm_tracks_count: i64,
    pub last_fm_avatar_url: String,
}

fn format_hour(hour: i32) -> String {
    let period_str = if hour >= 12 { "PM" } else { "AM" };
    let h12 = if hour % 12 == 0 { 12 } else { hour % 12 };
    format!("{h12}:00 {period_str}")
}

pub async fn compute_listening_stats(
    db: &AppDatabase,
    official: Option<&OfficialArtworkService>,
    period: StatsPeriod,
) -> ListeningStatsData {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let since_millis = period.days().map(|d| now_ms - d * 24 * 3600 * 1000);
    let prev_since_millis = period.days().map(|d| now_ms - d * 2 * 24 * 3600 * 1000);

    let summary = db
        .get_local_stats_summary(since_millis, prev_since_millis)
        .unwrap_or_else(|| crate::storage::app_database::LocalStatsSummary {
            total_plays: 0,
            total_duration_ms: 0,
            unmeasured_plays: 0,
            prev_plays: None,
            prev_duration_ms: None,
            prev_unmeasured_plays: None,
            unique_artists: 0,
            unique_tracks: 0,
            top_artists: Vec::new(),
            top_tracks: Vec::new(),
            top_albums: Vec::new(),
            hourly: HashMap::new(),
            daily: Vec::new(),
        });

    let raw_recent = db.load_recent_plays(200, since_millis);
    let mut local_recent = Vec::with_capacity(raw_recent.len());
    for p in raw_recent {
        let art = match official {
            Some(off) => off
                .peek_track(&p.title, &p.artist)
                .await
                .map(|r| r.artwork_url)
                .unwrap_or_default(),
            None => String::new(),
        };
        local_recent.push(RecentStatPlay {
            title: p.title,
            artist: p.artist,
            timestamp_millis: p.at_millis,
            artwork_url: art,
        });
    }

    let active_dates = db.load_active_play_dates();
    let mut streak = 0i64;
    let mut cursor = Local::now().date_naive();
    let today_str = cursor.format("%Y-%m-%d").to_string();
    if !active_dates.contains(&today_str) {
        cursor -= ChronoDuration::days(1);
    }
    loop {
        let day_str = cursor.format("%Y-%m-%d").to_string();
        if active_dates.contains(&day_str) {
            streak += 1;
            cursor -= ChronoDuration::days(1);
        } else {
            break;
        }
    }

    let mut peak_hour = 20i32;
    let mut max_hour_count = 0i64;
    for (&hr, &count) in &summary.hourly {
        if count > max_hour_count {
            max_hour_count = count;
            peak_hour = hr;
        }
    }
    let peak_label = if max_hour_count > 0 {
        format_hour(peak_hour)
    } else {
        "—".to_string()
    };

    let mut merged_top_tracks = Vec::with_capacity(summary.top_tracks.len());
    for t in &summary.top_tracks {
        let art = match official {
            Some(off) => off
                .peek_track(&t.title, &t.artist)
                .await
                .map(|r| r.artwork_url)
                .unwrap_or_default(),
            None => String::new(),
        };
        merged_top_tracks.push(StatTrack {
            name: t.title.clone(),
            artist: t.artist.clone(),
            artwork_url: art,
            play_count: t.count,
            timestamp_millis: None,
        });
    }

    let mut artist_counts: HashMap<String, i64> = HashMap::new();
    for a in &summary.top_artists {
        for single in split_artist_credits(&a.name) {
            *artist_counts.entry(single).or_insert(0) += a.count;
        }
    }
    let mut sorted_artists: Vec<(String, i64)> = artist_counts.into_iter().collect();
    sorted_artists.sort_by(|a, b| b.1.cmp(&a.1));
    let mut merged_top_artists = Vec::new();
    for (name, count) in sorted_artists.into_iter().take(100) {
        let art = match official {
            Some(off) => off
                .peek_artist(&name)
                .await
                .map(|r| r.artwork_url)
                .unwrap_or_default(),
            None => String::new(),
        };
        merged_top_artists.push(StatArtist {
            name,
            play_count: count,
            artwork_url: art,
        });
    }

    let mut album_counts: HashMap<String, (String, String, i64)> = HashMap::new();
    for a in &summary.top_albums {
        let key = format!("{}|{}", a.name.to_lowercase(), a.artist.to_lowercase());
        let entry = album_counts
            .entry(key)
            .or_insert_with(|| (a.name.clone(), a.artist.clone(), 0));
        entry.2 += a.count;
    }
    if album_counts.is_empty() {
        for t in &summary.top_tracks {
            let Some(off) = official else { break };
            let Some(peek) = off.peek_track(&t.title, &t.artist).await else {
                continue;
            };
            let album_title = peek.album_title.trim().to_string();
            if album_title.is_empty() {
                continue;
            }
            let key = format!("{}|{}", album_title.to_lowercase(), t.artist.to_lowercase());
            let entry = album_counts
                .entry(key)
                .or_insert_with(|| (album_title, t.artist.clone(), 0));
            entry.2 += t.count;
        }
    }
    let mut sorted_albums: Vec<(String, String, i64)> = album_counts.into_values().collect();
    sorted_albums.sort_by(|a, b| b.2.cmp(&a.2));
    let mut merged_top_albums = Vec::new();
    for (name, artist, count) in sorted_albums.into_iter().take(100) {
        let art = match official {
            Some(off) => off
                .peek_album(&name, &artist)
                .await
                .map(|r| r.artwork_url)
                .unwrap_or_default(),
            None => String::new(),
        };
        merged_top_albums.push(StatAlbum {
            name,
            artist,
            artwork_url: art,
            play_count: count,
        });
    }

    const FALLBACK_MS_PER_PLAY: i64 = 210_000;
    let total_minutes = ((summary.total_duration_ms
        + summary.unmeasured_plays * FALLBACK_MS_PER_PLAY) as f64
        / 60_000.0)
        .round() as i64;
    let prev_total_minutes = summary.prev_plays.map(|_| {
        ((summary.prev_duration_ms.unwrap_or(0)
            + summary.prev_unmeasured_plays.unwrap_or(0) * FALLBACK_MS_PER_PLAY) as f64
            / 60_000.0)
            .round() as i64
    });

    let daily_activity = summary
        .daily
        .into_iter()
        .map(|d| DailyActivityPoint {
            date: d.date,
            count: d.count,
        })
        .collect();

    ListeningStatsData {
        period,
        total_plays: summary.total_plays,
        total_minutes_listened: total_minutes,
        has_estimated_duration: summary.unmeasured_plays > 0,
        prev_period_plays: summary.prev_plays,
        prev_period_minutes_listened: prev_total_minutes,
        unique_artists: summary.unique_artists,
        unique_tracks: summary.unique_tracks,
        current_streak_days: streak,
        peak_listening_hour: peak_hour,
        peak_listening_label: peak_label,
        top_artists: merged_top_artists,
        top_tracks: merged_top_tracks,
        top_albums: merged_top_albums,
        recent_plays: local_recent,
        hourly_distribution: summary.hourly,
        daily_activity,
        is_last_fm_connected: false,
        last_fm_username: String::new(),
        scrobbler_name: String::new(),
        last_fm_scrobbles: 0,
        last_fm_artists_count: 0,
        last_fm_albums_count: 0,
        last_fm_tracks_count: 0,
        last_fm_avatar_url: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hour_formatting() {
        assert_eq!(format_hour(0), "12:00 AM");
        assert_eq!(format_hour(12), "12:00 PM");
        assert_eq!(format_hour(20), "8:00 PM");
    }
}
