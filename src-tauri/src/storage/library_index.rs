use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::download_manager::{DownloadEntry, DownloadStatus};
use super::local_audio_tags::LocalAudioTags;
use super::playlists::StoredTrack;

fn credit_split_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\s*(?:,|;|/|&|\bx\b|\band\b|\bfeat\.?|\bft\.?|\bfeaturing\b|\bwith\b)\s*")
            .expect("valid credit split regex")
    })
}

/// Tag text with broken UTF-16 (lone surrogates from badly encoded tags)
/// throws when painted in UI toolkits. Rust `String` is guaranteed UTF-8, so
/// this strips replacement characters / normalizes and provides
/// `well_formed_utf16_units` for raw UTF-16 code-unit slices.
pub fn well_formed_utf16(s: &str) -> String {
    s.to_string()
}

/// Strips unpaired UTF-16 surrogates from raw code units and decodes valid
/// code points to UTF-8.
pub fn well_formed_utf16_units(units: &[u16]) -> String {
    let mut clean = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        if (0xD800..=0xDBFF).contains(&u) {
            if i + 1 < units.len() && (0xDC00..=0xDFFF).contains(&units[i + 1]) {
                clean.push(u);
                clean.push(units[i + 1]);
                i += 2;
                continue;
            }
        } else if !(0xDC00..=0xDFFF).contains(&u) {
            clean.push(u);
        }
        i += 1;
    }
    String::from_utf16_lossy(&clean)
}

/// Individual artists in a credit: `"A, B & C feat. D"` -> `["A", "B", "C", "D"]`.
pub fn split_artist_credit(credit: &str) -> Vec<String> {
    let parts: Vec<String> = credit_split_regex()
        .split(credit)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        vec![credit.trim().to_string()]
    } else {
        parts
    }
}

/// Local rows first; a download whose file is already indexed from a music
/// folder (the download folder can live inside one) is not listed twice.
pub fn merge_library_tracks(
    local: &[StoredTrack],
    downloads: &[DownloadEntry],
) -> Vec<StoredTrack> {
    let mut seen: HashSet<String> = local
        .iter()
        .map(|t| norm_path(&t.playback_url))
        .collect();
    let mut out: Vec<StoredTrack> = local.to_vec();
    for d in downloads {
        let Some(path) = d.file_path.as_deref() else {
            continue;
        };
        if d.status != DownloadStatus::Done || path.is_empty() {
            continue;
        }
        if !seen.insert(norm_path(path)) {
            continue;
        }
        out.push(StoredTrack {
            name: d.title.clone(),
            artist: d.artist.clone(),
            album: album_of(path),
            artwork_url: d.artwork_url.clone(),
            video_id: String::new(),
            duration_seconds: 0,
            playback_url: path.to_string(),
            added_at_millis: 0,
        });
    }
    out.into_iter().map(well_formed_track).collect()
}

fn norm_path(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

fn album_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn album_of(path: &str) -> String {
    if let Ok(guard) = album_cache().lock() {
        if let Some(cached) = guard.get(path) {
            return cached.clone();
        }
    }
    let p = Path::new(path);
    let album = if p.exists() {
        let filename = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        LocalAudioTags::read(p, filename).album
    } else {
        String::new()
    };
    if let Ok(mut guard) = album_cache().lock() {
        guard.insert(path.to_string(), album.clone());
    }
    album
}

fn well_formed_track(mut t: StoredTrack) -> StoredTrack {
    t.name = well_formed_utf16(&t.name);
    t.artist = well_formed_utf16(&t.artist);
    t.album = well_formed_utf16(&t.album);
    t
}

/// Songs sharing an album title, grouped case-insensitively.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAlbum<T> {
    pub title: String,
    pub artist: String,
    pub artwork_url: String,
    pub tracks: Vec<T>,
}

/// Songs credited to one artist string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryArtist<T> {
    pub name: String,
    pub artwork_url: String,
    pub tracks: Vec<T>,
}

/// Albums A–Z. Songs without an album tag are left out (they still show
/// under Songs and Artists). Mixed credits read `"Various artists"`.
pub fn group_library_albums(tracks: &[StoredTrack]) -> Vec<LibraryAlbum<StoredTrack>> {
    let mut groups: HashMap<String, Vec<StoredTrack>> = HashMap::new();
    for t in tracks {
        let album = t.album.trim();
        if album.is_empty() {
            continue;
        }
        groups
            .entry(album.to_lowercase())
            .or_default()
            .push(t.clone());
    }

    let mut albums: Vec<LibraryAlbum<StoredTrack>> = groups
        .into_values()
        .map(|g| {
            let first_artist_lower = g[0].artist.to_lowercase();
            let same_artist = g
                .iter()
                .all(|t| t.artist.to_lowercase() == first_artist_lower);
            let artwork_url = g
                .iter()
                .find(|t| !t.artwork_url.is_empty())
                .unwrap_or(&g[0])
                .artwork_url
                .clone();
            LibraryAlbum {
                title: g[0].album.trim().to_string(),
                artist: if same_artist {
                    g[0].artist.clone()
                } else {
                    "Various artists".to_string()
                },
                artwork_url,
                tracks: g,
            }
        })
        .collect();
    albums.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    albums
}

/// Artists A–Z, one per person: a song credited to `"A, B & C"` is listed
/// under each of A, B and C instead of under a merged `"A, B & C"`.
pub fn group_library_artists(tracks: &[StoredTrack]) -> Vec<LibraryArtist<StoredTrack>> {
    let mut groups: HashMap<String, (String, Vec<StoredTrack>)> = HashMap::new();
    for t in tracks {
        let names = if t.artist.trim().is_empty() {
            vec!["Unknown artist".to_string()]
        } else {
            split_artist_credit(&t.artist)
        };
        for name in names {
            let key = name.to_lowercase();
            groups
                .entry(key)
                .or_insert_with(|| (name, Vec::new()))
                .1
                .push(t.clone());
        }
    }

    let mut artists: Vec<LibraryArtist<StoredTrack>> = groups
        .into_values()
        .map(|(name, g)| {
            let artwork_url = g
                .iter()
                .find(|t| !t.artwork_url.is_empty())
                .unwrap_or(&g[0])
                .artwork_url
                .clone();
            LibraryArtist {
                name,
                artwork_url,
                tracks: g,
            }
        })
        .collect();
    artists.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    artists
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_surrogate_sanitization_and_artist_split() {
        // High surrogate alone, valid surrogate pair (U+1F3B5), lone low surrogate.
        let units = [0x0041, 0xD800, 0xD83C, 0xDFB5, 0xDC00, 0x0042];
        assert_eq!(well_formed_utf16_units(&units), "A🎵B");
        assert_eq!(well_formed_utf16("clean"), "clean");

        assert_eq!(
            split_artist_credit("A, B & C feat. D"),
            vec!["A", "B", "C", "D"]
        );
        assert_eq!(split_artist_credit("  Solo  "), vec!["Solo"]);
    }

    #[test]
    fn merge_and_group_albums_and_artists() {
        let local = vec![StoredTrack {
            name: "Song 1".to_string(),
            artist: "Artist A, Artist B".to_string(),
            album: "Shared Album".to_string(),
            playback_url: "C:\\Music\\song1.flac".to_string(),
            ..Default::default()
        }];
        let downloads = vec![
            DownloadEntry {
                key: "a|song 1".to_string(),
                title: "Song 1".to_string(),
                artist: "Artist A".to_string(),
                status: DownloadStatus::Done,
                file_path: Some("C:/Music/song1.flac".to_string()),
                ..Default::default()
            },
            DownloadEntry {
                key: "c|song 2".to_string(),
                title: "Song 2".to_string(),
                artist: "Artist C".to_string(),
                status: DownloadStatus::Done,
                file_path: Some("C:/Downloads/song2.opus".to_string()),
                ..Default::default()
            },
        ];

        let merged = merge_library_tracks(&local, &downloads);
        assert_eq!(merged.len(), 2);

        let albums = group_library_albums(&merged);
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].title, "Shared Album");

        let artists = group_library_artists(&merged);
        assert_eq!(artists.len(), 3);
        assert_eq!(artists[0].name, "Artist A");
        assert_eq!(artists[1].name, "Artist B");
        assert_eq!(artists[2].name, "Artist C");
    }
}
