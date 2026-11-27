//! Top genres for 7 days, 30 days, and all time.
//!
//! Ported from `lib/features/feed/genre_source.dart`.
//! A signed-in Last.fm session uses `user.getTopTags` (all time) and
//! `user.getTopArtists` plus `artist.getTopTags` (every period). Without
//! a session, plays in `local_plays` are scored by genre words in the
//! track and artist names.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::future::join_all;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::integrations::lastfm::home_repository::HomeRepository;
use crate::integrations::lastfm::lastfm_api::LastFmApiService;
use crate::storage::app_database::AppDatabase;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenreWeight {
    pub name: String,
    pub weight: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenreChart {
    pub bars: Vec<GenreWeight>,
    /// True when the bars came from a Last.fm session. False means the
    /// play log and track names were the only source.
    pub from_last_fm: bool,
}

#[derive(Debug, Clone)]
struct TagItem {
    name: String,
    count: i64,
}

pub struct GenreSource {
    api: Arc<LastFmApiService>,
    home: Arc<HomeRepository>,
    db: Arc<AppDatabase>,
    api_key: String,
    username: String,
}

impl GenreSource {
    pub fn new(
        api: Arc<LastFmApiService>,
        home: Arc<HomeRepository>,
        db: Arc<AppDatabase>,
        api_key: impl Into<String>,
        username: impl Into<String>,
    ) -> Self {
        Self {
            api,
            home,
            db,
            api_key: api_key.into(),
            username: username.into(),
        }
    }

    pub async fn load(&self, period: &str, session: bool) -> GenreChart {
        if session && !self.api_key.is_empty() && !self.username.is_empty() {
            if period == "overall" {
                let direct = self.user_top_tags().await;
                if !direct.is_empty() {
                    return GenreChart {
                        bars: direct,
                        from_last_fm: true,
                    };
                }
            }
            let aggregated = self.from_artists(period).await;
            return GenreChart {
                bars: aggregated,
                from_last_fm: true,
            };
        }
        GenreChart {
            bars: self.from_local_plays(period),
            from_last_fm: false,
        }
    }

    async fn user_top_tags(&self) -> Vec<GenreWeight> {
        let mut q = BTreeMap::new();
        q.insert("method".to_string(), "user.getTopTags".to_string());
        q.insert("user".to_string(), self.username.clone());
        q.insert("api_key".to_string(), self.api_key.clone());
        q.insert("limit".to_string(), "20".to_string());
        let Ok(Ok(json)) = tokio::time::timeout(Duration::from_secs(12), self.api.get(&q)).await
        else {
            return Vec::new();
        };
        if json.get("error").is_some() {
            return Vec::new();
        }
        Self::ranked(&tags_of(&json))
    }

    async fn from_artists(&self, period: &str) -> Vec<GenreWeight> {
        let artists = self
            .home
            .fetch_top_artists(None, period, 8)
            .await;
        if artists.is_empty() {
            return Vec::new();
        }
        let futs = artists.into_iter().map(|artist| async move {
            let mut q = BTreeMap::new();
            q.insert("method".to_string(), "artist.getTopTags".to_string());
            q.insert("artist".to_string(), artist.name);
            q.insert("api_key".to_string(), self.api_key.clone());
            q.insert("autocorrect".to_string(), "1".to_string());
            let tags = match tokio::time::timeout(Duration::from_secs(8), self.api.get(&q)).await {
                Ok(Ok(json)) if json.get("error").is_none() => tags_of(&json),
                _ => Vec::new(),
            };
            (artist.play_count, tags)
        });
        let batches = join_all(futs).await;

        let mut scores: HashMap<String, i64> = HashMap::new();
        let mut labels: HashMap<String, String> = HashMap::new();
        for (play_count, tags) in batches {
            let plays = if play_count <= 0 { 1 } else { play_count } as f64;
            for (i, tag) in tags.into_iter().take(5).enumerate() {
                let key = canon(&tag.name);
                if key.is_empty() {
                    continue;
                }
                let add = (plays / (i + 1) as f64).round() as i64;
                if add <= 0 {
                    continue;
                }
                *scores.entry(key.clone()).or_insert(0) += add;
                labels.entry(key).or_insert_with(|| pretty(&tag.name));
            }
        }
        Self::from_scores(&scores, &labels)
    }

    fn from_local_plays(&self, period: &str) -> Vec<GenreWeight> {
        let since = cutoff(period);
        let mut scores: HashMap<String, i64> = HashMap::new();
        let mut labels: HashMap<String, String> = HashMap::new();
        for play in self.db.load_recent_plays(500, since) {
            let blob = format!("{} {}", play.title, play.artist).to_lowercase();
            if blob.trim().is_empty() {
                continue;
            }
            for genre in LOCAL_GENRES {
                if !mentions(&blob, genre.needles) {
                    continue;
                }
                *scores.entry(genre.key.to_string()).or_insert(0) += 1;
                labels
                    .entry(genre.key.to_string())
                    .or_insert_with(|| genre.label.to_string());
            }
        }
        Self::from_scores(&scores, &labels)
    }

    fn ranked(tags: &[TagItem]) -> Vec<GenreWeight> {
        let mut scores: HashMap<String, i64> = HashMap::new();
        let mut labels: HashMap<String, String> = HashMap::new();
        for tag in tags {
            let key = canon(&tag.name);
            if key.is_empty() || tag.count <= 0 {
                continue;
            }
            *scores.entry(key.clone()).or_insert(0) += tag.count;
            labels.entry(key).or_insert_with(|| pretty(&tag.name));
        }
        Self::from_scores(&scores, &labels)
    }

    fn from_scores(
        scores: &HashMap<String, i64>,
        labels: &HashMap<String, String>,
    ) -> Vec<GenreWeight> {
        let mut rows: Vec<GenreWeight> = scores
            .iter()
            .filter(|(k, &v)| v > 0 && is_music_genre_tag(k))
            .map(|(k, &v)| GenreWeight {
                name: labels.get(k).cloned().unwrap_or_else(|| pretty(k)),
                weight: v,
            })
            .collect();
        rows.sort_by(|a, b| b.weight.cmp(&a.weight));
        rows.truncate(12);
        rows
    }
}

fn tags_of(json: &Value) -> Vec<TagItem> {
    let Some(root) = json.get("toptags").and_then(Value::as_object) else {
        return Vec::new();
    };
    let Some(raw) = root.get("tag") else {
        return Vec::new();
    };
    let maps: Vec<&serde_json::Map<String, Value>> = match raw {
        Value::Array(arr) => arr.iter().filter_map(Value::as_object).collect(),
        Value::Object(map) => vec![map],
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for tag in maps {
        let name = tag
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let count = tag
            .get("count")
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
            })
            .unwrap_or(0);
        out.push(TagItem {
            name: name.to_string(),
            count,
        });
    }
    out
}

static YEAR_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(19|20)\d{2}s?$").unwrap());
static DIGITS_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());
static MULTI_WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

static JUNK_TAGS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "seen live",
        "favorites",
        "favourite",
        "my music",
        "spotify",
        "youtube",
        "lastfm",
        "last.fm",
        "coke",
        "coca cola",
        "under 2000 listeners",
        "all",
        "music",
        "songs",
        "various",
        "misc",
        "other",
        "female vocalists",
        "male vocalists",
        "awesome",
        "cool",
        "love",
        "beautiful",
        "epic",
        "sexy",
        "fun",
        "sad",
        "happy",
        "chillout",
    ]
    .into_iter()
    .collect()
});

static PLACE_TAGS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "india",
        "indian",
        "pakistan",
        "pakistani",
        "hindi",
        "punjabi",
        "usa",
        "american",
        "uk",
        "british",
        "england",
        "english",
        "canada",
        "canadian",
        "australia",
        "australian",
        "japan",
        "japanese",
        "korea",
        "korean",
        "china",
        "chinese",
        "france",
        "french",
        "germany",
        "german",
        "spain",
        "spanish",
        "brazil",
        "brazilian",
        "mexico",
        "mexican",
        "russia",
        "russian",
        "italy",
        "italian",
        "turkey",
        "turkish",
        "arabic",
        "africa",
        "african",
        "europe",
        "european",
        "asia",
        "asian",
        "bangladesh",
        "bangladeshi",
        "nepal",
        "nepali",
        "sri lanka",
        "tamil",
        "telugu",
        "marathi",
        "urdu",
    ]
    .into_iter()
    .collect()
});

/// Last.fm tags include countries, languages, and one-off words
/// ("India", "Hindi", "Coke"). Those are not styles you can play.
pub fn is_music_genre_tag(name: &str) -> bool {
    let key = canon(name);
    if key.len() < 2 {
        return false;
    }
    if JUNK_TAGS.contains(key.as_str()) || PLACE_TAGS.contains(key.as_str()) {
        return false;
    }
    if YEAR_TAG_RE.is_match(&key) || DIGITS_TAG_RE.is_match(&key) {
        return false;
    }
    true
}

fn canon(name: &str) -> String {
    let key = MULTI_WS_RE
        .replace_all(&name.to_lowercase(), " ")
        .trim()
        .replace('-', " ");
    match key.as_str() {
        "hiphop" | "hip hop" => "hip hop".to_string(),
        "rnb" | "r&b" | "rhythm and blues" => "r&b".to_string(),
        "electronic dance music" | "edm" => "electronic".to_string(),
        "lo fi" | "lofi" | "lo-fi" => "lo-fi".to_string(),
        _ => key,
    }
}

fn pretty(name: &str) -> String {
    let key = canon(name);
    match key.as_str() {
        "r&b" => return "R&B".to_string(),
        "edm" => return "EDM".to_string(),
        "lo-fi" => return "Lo-fi".to_string(),
        "k pop" => return "K-Pop".to_string(),
        "hip hop" => return "Hip-Hop".to_string(),
        _ => {}
    }
    key.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn cutoff(period: &str) -> Option<i64> {
    let days: u64 = match period {
        "7day" => 7,
        "1month" => 30,
        _ => return None,
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Some(now_ms - (days as i64) * 24 * 3600 * 1000)
}

fn mentions(blob: &str, needles: &[&str]) -> bool {
    for &needle in needles {
        let pattern = format!(r"\b{}\b", regex::escape(needle));
        if let Ok(re) = RegexBuilder::new(&pattern).build() {
            if re.is_match(blob) {
                return true;
            }
        }
    }
    false
}

struct LocalGenreDef {
    key: &'static str,
    label: &'static str,
    needles: &'static [&'static str],
}

const LOCAL_GENRES: &[LocalGenreDef] = &[
    LocalGenreDef {
        key: "hip hop",
        label: "Hip-Hop",
        needles: &["hip hop", "hip-hop", "hiphop", "rap", "trap", "drill"],
    },
    LocalGenreDef {
        key: "r&b",
        label: "R&B",
        needles: &["r&b", "rnb"],
    },
    LocalGenreDef {
        key: "rock",
        label: "Rock",
        needles: &["rock", "metal", "punk", "grunge"],
    },
    LocalGenreDef {
        key: "pop",
        label: "Pop",
        needles: &["pop", "k-pop", "kpop"],
    },
    LocalGenreDef {
        key: "electronic",
        label: "Electronic",
        needles: &["electronic", "edm", "house", "techno", "trance", "dubstep"],
    },
    LocalGenreDef {
        key: "jazz",
        label: "Jazz",
        needles: &["jazz"],
    },
    LocalGenreDef {
        key: "lo-fi",
        label: "Lo-fi",
        needles: &["lofi", "lo-fi", "lo fi"],
    },
    LocalGenreDef {
        key: "classical",
        label: "Classical",
        needles: &["classical", "orchestra", "piano"],
    },
    LocalGenreDef {
        key: "country",
        label: "Country",
        needles: &["country"],
    },
    LocalGenreDef {
        key: "folk",
        label: "Folk",
        needles: &["folk", "acoustic"],
    },
    LocalGenreDef {
        key: "soul",
        label: "Soul",
        needles: &["soul", "funk", "disco"],
    },
    LocalGenreDef {
        key: "reggae",
        label: "Reggae",
        needles: &["reggae", "dancehall"],
    },
    LocalGenreDef {
        key: "latin",
        label: "Latin",
        needles: &["latin", "reggaeton", "salsa"],
    },
    LocalGenreDef {
        key: "blues",
        label: "Blues",
        needles: &["blues"],
    },
    LocalGenreDef {
        key: "ambient",
        label: "Ambient",
        needles: &["ambient", "chill"],
    },
    LocalGenreDef {
        key: "indie",
        label: "Indie",
        needles: &["indie"],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_non_music_tags_and_formats_labels() {
        assert!(!is_music_genre_tag("India"));
        assert!(!is_music_genre_tag("seen live"));
        assert!(!is_music_genre_tag("1990s"));
        assert!(is_music_genre_tag("hiphop"));
        assert_eq!(pretty("hiphop"), "Hip-Hop");
        assert_eq!(pretty("rnb"), "R&B");
    }
}
