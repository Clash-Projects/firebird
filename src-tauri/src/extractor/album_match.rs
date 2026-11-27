use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;

use regex::Regex;

use crate::models::artist_parser::split_artist_credits;

const VERSION_WORDS: &[&str] = &[
    "remix",
    "edm",
    "mix",
    "slowed",
    "reverb",
    "lofi",
    "lo",
    "acoustic",
    "live",
    "unplugged",
    "reprise",
    "sped",
    "mashup",
    "bass",
];

/// Normalizes a title or artist string for catalog matching (mirrors
/// `OfficialArtworkService.normalizeForSearch`).
pub fn normalize_for_search(text: &str) -> String {
    static RE_APOSTROPHE: OnceLock<Regex> = OnceLock::new();
    static RE_DASH_NOISE: OnceLock<Regex> = OnceLock::new();
    static RE_PAREN_NOISE: OnceLock<Regex> = OnceLock::new();
    static RE_BRACKET_NOISE: OnceLock<Regex> = OnceLock::new();
    static RE_PREFIX_ARTIST: OnceLock<Regex> = OnceLock::new();
    static RE_DOLLAR_DIGIT: OnceLock<Regex> = OnceLock::new();
    static RE_NON_WORD: OnceLock<Regex> = OnceLock::new();
    static RE_MULTI_SPACE: OnceLock<Regex> = OnceLock::new();

    let mut s = text.to_lowercase();
    s = RE_APOSTROPHE
        .get_or_init(|| Regex::new(r"['’ʼ`´]").unwrap())
        .replace_all(&s, "")
        .into_owned();
    s = RE_DASH_NOISE
        .get_or_init(|| {
            Regex::new(r"(?i)\s*-\s*(?:official|video|audio|remaster|remastered|lyrics?).*$")
                .unwrap()
        })
        .replace_all(&s, "")
        .into_owned();
    s = RE_PAREN_NOISE
        .get_or_init(|| {
            Regex::new(r"(?i)\s*\([^)]*(?:feat|ft\.?|featuring|official|video|audio|remaster|visualizer|lyrics?|explicit|clean)[^)]*\)")
                .unwrap()
        })
        .replace_all(&s, "")
        .into_owned();
    s = RE_BRACKET_NOISE
        .get_or_init(|| {
            Regex::new(r"(?i)\s*\[[^\]]*(?:feat|ft\.?|featuring|official|video|audio|remaster|visualizer|lyrics?|explicit|clean)[^\]]*\]")
                .unwrap()
        })
        .replace_all(&s, "")
        .into_owned();
    s = RE_PREFIX_ARTIST
        .get_or_init(|| Regex::new(r"^\s*[\w&.\- ]+\s*-\s+").unwrap())
        .replace_all(&s, "")
        .into_owned();
    s = RE_DOLLAR_DIGIT
        .get_or_init(|| Regex::new(r"\$\d").unwrap())
        .replace_all(&s, |caps: &regex::Captures<'_>| caps[0][1..].to_string())
        .into_owned();
    s = s.replace('$', "s");
    s = RE_NON_WORD
        .get_or_init(|| Regex::new(r"[^\p{L}\p{N}]+").unwrap())
        .replace_all(&s, " ")
        .into_owned();
    RE_MULTI_SPACE
        .get_or_init(|| Regex::new(r"\s+").unwrap())
        .replace_all(&s, " ")
        .trim()
        .to_string()
}

/// Read-only view of an album search candidate used by [`pick_best_album_match`].
pub trait AlbumCandidate {
    fn browse_id(&self) -> &str;
    fn name(&self) -> &str;
    fn artist(&self) -> &str;
    fn subtitle(&self) -> &str;
}

/// Read-only view of a song search candidate used as fallback in [`resolve_album_browse_id`].
pub trait SongAlbumCandidate {
    fn album_browse_id(&self) -> &str;
    fn title(&self) -> &str;
    fn artist(&self) -> &str;
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Minimal InnerTube lookup abstraction needed by [`resolve_album_browse_id`].
pub trait AlbumLookupApi: Send + Sync {
    type AlbumItem: AlbumCandidate + Clone + Send + Sync;
    type SongItem: SongAlbumCandidate + Send + Sync;

    fn lookup_albums<'a>(
        &'a self,
        query: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Self::AlbumItem>, String>>;

    fn lookup_songs<'a>(
        &'a self,
        query: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Self::SongItem>, String>>;
}

/// Picks the album entity that actually corresponds to a target `(title, artist)`
/// pair from YouTube Music album search results.
pub fn pick_best_album_match<'a, T: AlbumCandidate>(
    results: &'a [T],
    title: &str,
    artist: &str,
    min_score: i32,
    require_artist: bool,
) -> Option<&'a T> {
    let target_title = normalize_for_search(title);
    let target_artist = normalize_for_search(artist);
    if target_title.is_empty() {
        return None;
    }

    let version_set: HashSet<&str> = VERSION_WORDS.iter().copied().collect();
    let target_tokens: HashSet<&str> = target_title.split(' ').collect();
    let billed: Vec<String> = if require_artist && !target_artist.is_empty() {
        split_artist_credits(artist)
            .into_iter()
            .map(|c| normalize_for_search(&c))
            .filter(|c| !c.is_empty())
            .collect()
    } else {
        Vec::new()
    };

    let mut best: Option<&T> = None;
    let mut best_score: i32 = -1;

    for e in results {
        if e.browse_id().is_empty() {
            continue;
        }
        let name = normalize_for_search(e.name());
        let by = normalize_for_search(e.artist());
        if name.is_empty() {
            continue;
        }

        let mut score: i32 = 0;
        if name == target_title {
            score += 100;
        } else if name.contains(&target_title) || target_title.contains(&name) {
            score += 60;
        } else {
            continue;
        }

        if !target_artist.is_empty() && !by.is_empty() {
            if by == target_artist {
                score += 100;
            } else if by.contains(&target_artist) || target_artist.contains(&by) {
                score += 50;
            }
        }

        if require_artist && !target_artist.is_empty() {
            let credited = format!("{by} {}", normalize_for_search(e.subtitle()));
            if !billed.iter().any(|b| credited.contains(b.as_str())) {
                continue;
            }
        }

        let name_tokens: HashSet<&str> = name.split(' ').collect();
        let has_extra_version = name_tokens
            .intersection(&version_set)
            .any(|tok| !target_tokens.contains(tok));
        if has_extra_version {
            score -= 80;
        }

        let noise = format!("{name} {}", normalize_for_search(e.subtitle()));
        if noise.contains("commentary")
            || noise.contains("karaoke")
            || noise.contains("tribute")
            || noise.contains("instrumental")
            || noise.contains("track by track")
            || noise.contains("cover")
        {
            score -= 100;
        }

        if score > best_score {
            best_score = score;
            best = Some(e);
        }
    }

    if best_score < min_score {
        None
    } else {
        best
    }
}

fn strip_trailing_bracket(s: &str) -> String {
    static RE_TRAIL: OnceLock<Regex> = OnceLock::new();
    RE_TRAIL
        .get_or_init(|| Regex::new(r"\s*[(\[].*$").unwrap())
        .replace(s, "")
        .trim()
        .to_string()
}

/// Resolves the YouTube Music `browseId` for a Last.fm-style `(title, artist)` album,
/// or returns an empty string if none matches cleanly.
pub async fn resolve_album_browse_id<A: AlbumLookupApi>(
    tube: &A,
    title: &str,
    artist: &str,
) -> String {
    let plain = strip_trailing_bracket(title);
    let mut queries = Vec::with_capacity(2);
    queries.push(title.to_string());
    if !plain.is_empty() && plain != title {
        queries.push(plain.clone());
    }

    for t in &queries {
        let q = format!("{artist} {t}");
        if let Ok(results) = tube.lookup_albums(&q, 6).await {
            if let Some(hit) = pick_best_album_match(&results, t, artist, 100, true) {
                return hit.browse_id().to_string();
            }
        }
    }

    let want_title = normalize_for_search(if !plain.is_empty() { &plain } else { title });
    let billed: Vec<String> = split_artist_credits(artist)
        .into_iter()
        .map(|c| normalize_for_search(&c))
        .filter(|c| !c.is_empty())
        .collect();

    let song_q = format!("{artist} {title}");
    if let Ok(songs) = tube.lookup_songs(&song_q, 8).await {
        for s in songs {
            if s.album_browse_id().is_empty() {
                continue;
            }
            let name = normalize_for_search(&strip_trailing_bracket(s.title()));
            let by = normalize_for_search(s.artist());
            if name == want_title && billed.iter().any(|b| by.contains(b.as_str())) {
                return s.album_browse_id().to_string();
            }
        }
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestAlbum {
        name: &'static str,
        artist: &'static str,
        subtitle: &'static str,
        browse_id: &'static str,
    }

    impl AlbumCandidate for TestAlbum {
        fn browse_id(&self) -> &str {
            self.browse_id
        }
        fn name(&self) -> &str {
            self.name
        }
        fn artist(&self) -> &str {
            self.artist
        }
        fn subtitle(&self) -> &str {
            self.subtitle
        }
    }

    #[test]
    fn skips_commentary_and_picks_exact_album() {
        let items = vec![
            TestAlbum {
                name: "DS2: Track by Track Commentary",
                artist: "Future",
                subtitle: "Album • 2015",
                browse_id: "MPRE_commentary",
            },
            TestAlbum {
                name: "DS2 (Deluxe)",
                artist: "Future",
                subtitle: "Album • 2015",
                browse_id: "MPRE_real",
            },
        ];
        let picked = pick_best_album_match(&items, "DS2 (Deluxe)", "Future", 60, false)
            .expect("should pick real album");
        assert_eq!(picked.browse_id, "MPRE_real");
    }

    #[test]
    fn penalizes_unrequested_remix_edition() {
        let items = vec![
            TestAlbum {
                name: "After Hours Remix",
                artist: "The Weeknd",
                subtitle: "",
                browse_id: "MPRE_remix",
            },
            TestAlbum {
                name: "After Hours",
                artist: "The Weeknd",
                subtitle: "",
                browse_id: "MPRE_orig",
            },
        ];
        let picked = pick_best_album_match(&items, "After Hours", "The Weeknd", 60, true)
            .expect("should pick original");
        assert_eq!(picked.browse_id, "MPRE_orig");
    }
}
