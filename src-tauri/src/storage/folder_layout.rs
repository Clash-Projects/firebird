use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

fn feat_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\s+(?:feat\.?|ft\.?|featuring|with)\s+.*$").expect("valid feat regex")
    })
}

fn collab_split_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[,;&/、×]").expect("valid collab split regex"))
}

fn x_split_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\s+x\s+").expect("valid x split regex"))
}

fn invalid_path_chars_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"[\\/:*?"<>|]"#).expect("valid invalid chars regex"))
}

/// Subfolder layout for new downloads.
///
/// Ids match Android `DownloadFolderStructure`. Existing files stay put
/// until the user runs Reorganize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadFolderLayout {
    #[default]
    Flat,
    ArtistAlbum,
    ArtistYearAlbum,
    AlbumOnly,
    YearAlbum,
}

impl DownloadFolderLayout {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::ArtistAlbum => "artist_album",
            Self::ArtistYearAlbum => "artist_year_album",
            Self::AlbumOnly => "album_only",
            Self::YearAlbum => "year_album",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Flat => "Flat",
            Self::ArtistAlbum => "Artist/Album",
            Self::ArtistYearAlbum => "Artist/[Year] Album",
            Self::AlbumOnly => "Album",
            Self::YearAlbum => "[Year] Album",
        }
    }

    pub const fn example(self) -> &'static str {
        match self {
            Self::Flat => "Music/song.flac",
            Self::ArtistAlbum => "Artist/Album/",
            Self::ArtistYearAlbum => "Artist/[2025] Album/",
            Self::AlbumOnly => "Album/",
            Self::YearAlbum => "[2025] Album/",
        }
    }

    pub fn from_id(id: Option<&str>) -> Self {
        match id {
            Some("artist_album") | Some("artist_album_singles") | Some("artist_album_singles_flat") => {
                Self::ArtistAlbum
            }
            Some("artist_year_album") => Self::ArtistYearAlbum,
            Some("album_only") => Self::AlbumOnly,
            Some("year_album") => Self::YearAlbum,
            _ => Self::Flat,
        }
    }
}

/// Relative folder under the download root. Empty means flat.
///
/// Port of `TrackDownloadManager.downloadSubpath`.
pub fn download_subpath(
    structure: DownloadFolderLayout,
    artist: &str,
    album: &str,
    album_artist: Option<&str>,
    year: &str,
) -> String {
    if structure == DownloadFolderLayout::Flat {
        return String::new();
    }

    let mut folder_artist = match album_artist {
        Some(aa) if !aa.trim().is_empty() => aa.to_string(),
        _ => artist.to_string(),
    };
    folder_artist = primary_download_artist(&folder_artist);
    folder_artist = sanitize_download_segment(&folder_artist);
    if folder_artist.is_empty() {
        folder_artist = "Unknown Artist".to_string();
    }

    let raw_album = album.trim();
    let is_single = raw_album.is_empty() || raw_album.eq_ignore_ascii_case("singles");
    let mut album_seg = sanitize_download_segment(raw_album);
    if album_seg.is_empty() {
        album_seg = "Singles".to_string();
    }

    let year_digits: String = year.chars().filter(char::is_ascii_digit).collect();
    let year_seg = if year_digits.len() >= 4 {
        &year_digits[..4]
    } else {
        ""
    };
    let year_album_seg = if year_seg.len() == 4 && !is_single {
        format!("[{year_seg}] {album_seg}")
    } else {
        album_seg.clone()
    };

    let segments: Vec<String> = match structure {
        DownloadFolderLayout::Flat => Vec::new(),
        DownloadFolderLayout::ArtistAlbum => {
            vec![
                folder_artist,
                if is_single {
                    "Singles".to_string()
                } else {
                    album_seg
                },
            ]
        }
        DownloadFolderLayout::ArtistYearAlbum => {
            vec![
                folder_artist,
                if is_single {
                    "Singles".to_string()
                } else {
                    year_album_seg
                },
            ]
        }
        DownloadFolderLayout::AlbumOnly => {
            vec![if is_single {
                "Singles".to_string()
            } else {
                album_seg
            }]
        }
        DownloadFolderLayout::YearAlbum => {
            vec![if is_single {
                "Singles".to_string()
            } else {
                year_album_seg
            }]
        }
    };

    segments
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// `"A feat. B"`, `"A, B"`, `"A & B"`, `"A x B"` -> `"A"`.
pub fn primary_download_artist(artist: &str) -> String {
    let trimmed = artist.trim();
    let mut name = feat_regex().replace(trimmed, "").trim().to_string();
    if name.is_empty() {
        name = trimmed.to_string();
    }
    name = collab_split_regex()
        .split(&name)
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        name = trimmed.to_string();
    }
    name = x_split_regex()
        .split(&name)
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        trimmed.to_string()
    } else {
        name
    }
}

pub fn sanitize_download_segment(raw: &str) -> String {
    let mut name = raw.trim().to_string();
    if name.contains('/') || name.contains('\\') {
        name = name
            .split(['\\', '/'])
            .next_back()
            .unwrap_or("")
            .trim()
            .to_string();
    }
    name = invalid_path_chars_regex()
        .replace_all(&name, "_")
        .trim()
        .to_string();
    while name.starts_with('.') || name.starts_with('_') {
        name = name[1..].trim().to_string();
    }
    while name.ends_with('.') || name.ends_with('_') {
        if name.is_empty() {
            break;
        }
        name = name[..name.len() - 1].trim().to_string();
    }
    if name.chars().count() > 80 {
        name = name.chars().take(80).collect::<String>().trim().to_string();
    }
    if name.is_empty() || name == "." || name == ".." {
        return String::new();
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subpath_and_sanitization_parity() {
        assert_eq!(
            download_subpath(
                DownloadFolderLayout::Flat,
                "Artist",
                "Album",
                None,
                "2025",
            ),
            ""
        );
        assert_eq!(
            download_subpath(
                DownloadFolderLayout::ArtistYearAlbum,
                "Drake feat. Future",
                "What A Time",
                None,
                "2015-09-20",
            ),
            "Drake/[2015] What A Time"
        );
        assert_eq!(
            download_subpath(
                DownloadFolderLayout::ArtistAlbum,
                "Solo",
                "",
                None,
                "2024",
            ),
            "Solo/Singles"
        );
        assert_eq!(
            DownloadFolderLayout::from_id(Some("artist_album_singles")),
            DownloadFolderLayout::ArtistAlbum
        );
        assert_eq!(sanitize_download_segment(".."), "");
        assert_eq!(sanitize_download_segment("foo:bar*baz?"), "foo_bar_baz");
    }
}
