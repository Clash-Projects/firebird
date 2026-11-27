use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use url::Url;

static UNDERSCORE_DASH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[_-]+").expect("valid underscore dash regex"));
static APOSTROPHE_QUOTE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("['\u{2019}\"\u{201C}\u{201D}]").expect("valid quote regex"));
static NON_ALNUM_CELL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\p{L}\p{M}\p{N}]+").expect("valid non alnum regex"));
static DIGITS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+$").expect("valid digits regex"));
static NUMBERING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:\[?\d+[.)\]]|\d+\s*[-\u{2013}\u{2014}])\s*")
        .expect("valid numbering regex")
});
static BY_SEPARATOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s+by\s+").expect("valid by separator regex"));
static DASH_SEPARATOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s+[-\u{2013}\u{2014}|:]\s+").expect("valid dash separator regex")
});
static EXTINF_ARTIST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)artist="([^"]+)""#).expect("valid extinf artist regex"));
static EXTINF_TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)title="([^"]+)""#).expect("valid extinf title regex"));
static VIDEO_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{11}$").expect("valid video id regex"));

const DELIMITERS: [char; 4] = [',', ';', '\t', '|'];

const INDEX_HEADERS: &[&str] = &[
    "index",
    "#",
    "no",
    "no.",
    "number",
    "num",
    "track number",
    "track no",
    "track num",
    "track count",
    "track index",
    "track id",
    "position",
    "pos",
    "order",
    "disc",
    "disc number",
    "disc no",
    "disc num",
    "disc count",
    "side",
    "duration",
    "duration ms",
    "duration_ms",
    "time",
    "length",
    "bpm",
    "year",
    "date",
    "genre",
    "genres",
    "isrc",
    "key",
    "popularity",
    "bitrate",
    "added at",
    "added by",
];

const PRIMARY_TITLE_HEADERS: &[&str] = &[
    "track name",
    "trackname",
    "song title",
    "songtitle",
    "track title",
    "tracktitle",
    "song name",
    "songname",
    "title",
    "titles",
    "video title",
    "item name",
    "audio title",
    "titre",
    "titres",
    "titulo",
    "titulos",
    "cancion",
    "canciones",
    "lied",
    "lieder",
    "titel",
];

const SECONDARY_TITLE_HEADERS: &[&str] =
    &["song", "songs", "name", "music", "item", "headline"];

const PRIMARY_ARTIST_HEADERS: &[&str] = &[
    "artist name s",
    "artist names",
    "artist name",
    "artistname",
    "track artist",
    "track artists",
    "lead artist",
    "primary artist",
    "main artist",
    "channel title",
    "channel name",
    "track artist name",
    "artiste",
    "artistes",
    "artista",
    "artistas",
    "interpret",
    "interpreten",
    "kuenstler",
    "künstler",
];

const SECONDARY_ARTIST_HEADERS: &[&str] = &[
    "artist s",
    "artist",
    "artists",
    "channel",
    "uploader",
    "author",
    "creator",
    "performer",
    "performers",
    "singer",
    "singers",
    "band",
    "by",
];

const ALBUM_HEADERS: &[&str] = &[
    "album name",
    "albumname",
    "album",
    "albums",
    "release",
    "collection",
    "record",
    "album title",
    "album name s",
    "record title",
    "album_name",
    "album_title",
];

const URL_HEADERS: &[&str] = &[
    "url",
    "uri",
    "track url",
    "track uri",
    "youtube url",
    "video id",
    "videoid",
    "link",
    "track link",
    "spotify uri",
    "spotify url",
    "youtube link",
    "video url",
    "spotify id",
    "spotify track id",
    "id",
    "yt url",
    "youtube",
    "soundcloud url",
];

fn is_title_header(h: &str) -> bool {
    PRIMARY_TITLE_HEADERS.contains(&h)
        || SECONDARY_TITLE_HEADERS.contains(&h)
        || h == "track"
        || h == "tracks"
}

fn is_artist_header(h: &str) -> bool {
    PRIMARY_ARTIST_HEADERS.contains(&h) || SECONDARY_ARTIST_HEADERS.contains(&h)
}

/// One row read off a playlist file. `video_id` is set when the row carries
/// a YouTube link or id.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistFileRow {
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub video_id: Option<String>,
}

impl PlaylistFileRow {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            artist: String::new(),
            album: None,
            video_id: None,
        }
    }

    pub fn with_video_id(&self, id: Option<String>) -> Self {
        Self {
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            video_id: id,
        }
    }

    /// Key used to collapse repeated rows before matching.
    pub fn identity(&self) -> String {
        if let Some(ref vid) = self.video_id {
            format!("id:{vid}")
        } else {
            format!(
                "text:{}|{}",
                normalize_cell(&self.title),
                normalize_cell(&self.artist)
            )
        }
    }
}

/// Decodes file bytes: UTF-16 by BOM, else UTF-8, else Latin-1.
pub fn decode_playlist_bytes(bytes: &[u8]) -> String {
    let text = if bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] == 0xfe {
        decode_utf16(&bytes[2..], true)
    } else if bytes.len() >= 2 && bytes[0] == 0xfe && bytes[1] == 0xff {
        decode_utf16(&bytes[2..], false)
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        s.to_string()
    } else {
        bytes.iter().map(|&b| b as char).collect()
    };
    text.strip_prefix('\u{FEFF}').unwrap_or(&text).to_string()
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// `"My_Playlist-2024.csv"` -> `"My Playlist 2024"`.
pub fn suggested_playlist_title(filename: &str) -> String {
    let stem = match filename.rfind('.') {
        Some(dot) => &filename[..dot],
        None => filename,
    };
    let replaced = UNDERSCORE_DASH_RE.replace_all(stem, " ");
    let base = replaced.trim();
    if base.is_empty() {
        "Imported Playlist".to_string()
    } else {
        base.to_string()
    }
}

pub fn normalize_cell(value: &str) -> String {
    let nfkc: String = value.nfkc().collect();
    let lower = nfkc.to_lowercase();
    let no_quotes = APOSTROPHE_QUOTE_RE.replace_all(&lower, "");
    let alnum = NON_ALNUM_CELL_RE.replace_all(&no_quotes, " ");
    alnum.trim().to_string()
}

/// Reads `text` (the content of `filename`) into rows.
pub fn parse_playlist_file(text: &str, filename: &str) -> Vec<PlaylistFileRow> {
    let name = filename.to_ascii_lowercase();
    let all_lines: Vec<String> = text.lines().map(|l| l.trim().to_string()).collect();
    let lines: Vec<String> = all_lines
        .iter()
        .filter(|l| !l.is_empty())
        .cloned()
        .collect();
    if lines.is_empty() {
        return Vec::new();
    }

    if name.ends_with(".m3u")
        || name.ends_with(".m3u8")
        || lines[0].eq_ignore_ascii_case("#EXTM3U")
    {
        return parse_m3u(&all_lines);
    }

    if name.ends_with(".txt")
        && !lines.iter().any(|l| l.contains('\t'))
        && !lines[0]
            .split([',', ';'])
            .map(normalize_cell)
            .all(|h| {
                is_title_header(h.as_str())
                    || is_artist_header(h.as_str())
                    || URL_HEADERS.contains(&h.as_str())
            })
    {
        return lines
            .iter()
            .filter(|l| !l.starts_with('#') && !l.starts_with("//"))
            .map(|l| parse_text_track(l))
            .collect();
    }

    let cleaned_lines: Vec<String> = lines
        .into_iter()
        .filter(|l| {
            !l.starts_with('#')
                && !l.starts_with("//")
                && !l.to_ascii_lowercase().starts_with("sep=")
        })
        .collect();
    if cleaned_lines.is_empty() {
        return Vec::new();
    }
    let cleaned_text = cleaned_lines.join("\n");

    let delimiter = detect_delimiter(&cleaned_text);
    let records: Vec<Vec<String>> =
        repair_collapsed_rows(parse_records(&cleaned_text, delimiter), delimiter)
            .into_iter()
            .filter(|r| !r.is_empty() && r.iter().any(|c| !c.trim().is_empty()))
            .collect();
    if records.is_empty() {
        return Vec::new();
    }

    let first = &records[0];
    let headers: Vec<String> = first.iter().map(|c| normalize_cell(c)).collect();
    let data_rows = &records[1..];

    let is_col_mostly_numeric = |col: usize| -> bool {
        if data_rows.is_empty() {
            return false;
        }
        let non_blank: Vec<&str> = data_rows
            .iter()
            .filter_map(|r| r.get(col).map(String::as_str).map(str::trim))
            .filter(|s| !s.is_empty())
            .collect();
        if non_blank.is_empty() {
            return false;
        }
        let digits_count = non_blank.iter().filter(|s| is_digits(s)).count();
        (digits_count as f64 / non_blank.len() as f64) >= 0.7
    };

    let is_explicit_index_col = |idx: usize| -> bool {
        let h = headers.get(idx).map(String::as_str).unwrap_or("");
        INDEX_HEADERS.contains(&h)
            || (h == "track"
                && headers.iter().any(|x| {
                    PRIMARY_TITLE_HEADERS.contains(&x.as_str())
                        || SECONDARY_TITLE_HEADERS.contains(&x.as_str())
                }))
            || is_col_mostly_numeric(idx)
    };

    let mut title_index = headers
        .iter()
        .position(|h| PRIMARY_TITLE_HEADERS.contains(&h.as_str()));
    if title_index.is_none() {
        title_index = headers
            .iter()
            .position(|h| SECONDARY_TITLE_HEADERS.contains(&h.as_str()));
    }
    if title_index.is_none() {
        if let Some(t) = headers.iter().position(|h| h == "track") {
            if !is_explicit_index_col(t) {
                title_index = Some(t);
            }
        }
    }

    let mut artist_index = headers
        .iter()
        .position(|h| PRIMARY_ARTIST_HEADERS.contains(&h.as_str()));
    if artist_index.is_none() {
        artist_index = headers
            .iter()
            .position(|h| SECONDARY_ARTIST_HEADERS.contains(&h.as_str()));
    }
    let album_index = headers.iter().position(|h| ALBUM_HEADERS.contains(&h.as_str()));
    let url_index = headers.iter().position(|h| URL_HEADERS.contains(&h.as_str()));

    let has_header = first.len() >= 2
        && !first
            .iter()
            .any(|c| !c.trim().is_empty() && is_digits(c.trim()))
        && (title_index.is_some()
            || artist_index.is_some()
            || album_index.is_some()
            || url_index.is_some());

    if first.len() == 1 && !has_header {
        return cleaned_lines
            .iter()
            .map(|l| parse_text_track(l))
            .collect();
    }

    let first_column = |pred: &dyn Fn(usize) -> bool, fallback: isize| -> isize {
        for i in 0..first.len() {
            if pred(i) {
                return i as isize;
            }
        }
        fallback
    };

    let title_column: isize = if has_header {
        if let Some(ti) = title_index {
            ti as isize
        } else {
            first_column(
                &|i| {
                    artist_index != Some(i)
                        && album_index != Some(i)
                        && url_index != Some(i)
                        && !is_explicit_index_col(i)
                },
                0,
            )
        }
    } else {
        first_column(&|i| !is_explicit_index_col(i), 0)
    };

    let artist_column: isize = if has_header {
        artist_index.map(|i| i as isize).unwrap_or(-1)
    } else {
        first_column(
            &|i| i as isize != title_column && !is_col_mostly_numeric(i),
            1,
        )
    };

    let cell = |row: &[String], idx: isize| -> String {
        if idx < 0 {
            return String::new();
        }
        row.get(idx as usize)
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };

    let mut out = Vec::new();
    for row in records.iter().skip(if has_header { 1 } else { 0 }) {
        if has_header {
            let norm_row: Vec<String> = row.iter().map(|c| normalize_cell(c)).collect();
            if norm_row == headers {
                continue;
            }
        }
        let title_cell = cell(row, title_column);
        let raw_artist = cell(row, artist_column);

        let mut video_id = url_index.and_then(|ui| youtube_id_of(&cell(row, ui as isize)));
        if video_id.is_none() {
            for c in row {
                let t = c.trim().to_ascii_lowercase();
                if t.starts_with("https://") || t.starts_with("http://") {
                    if let Some(found) = youtube_id_of(c.trim()) {
                        video_id = Some(found);
                        break;
                    }
                }
            }
        }

        let title = if video_id.is_some() && youtube_id_of(&title_cell) == video_id {
            String::new()
        } else {
            title_cell
        };
        if title.is_empty() && video_id.is_none() {
            continue;
        }

        let mut final_title = title;
        let mut final_artist = raw_artist;
        if final_artist.is_empty() && !final_title.is_empty() && video_id.is_none() {
            let parsed = parse_text_track(&final_title);
            if !parsed.title.is_empty() && !parsed.artist.is_empty() {
                final_title = parsed.title;
                final_artist = parsed.artist;
            }
        }
        let album = album_index
            .map(|ai| cell(row, ai as isize))
            .unwrap_or_default();
        out.push(PlaylistFileRow {
            title: final_title,
            artist: final_artist,
            album: if album.is_empty() { None } else { Some(album) },
            video_id,
        });
    }
    out
}

fn is_digits(s: &str) -> bool {
    DIGITS_RE.is_match(s)
}

fn repair_collapsed_rows(records: Vec<Vec<String>>, delimiter: char) -> Vec<Vec<String>> {
    let Some(modal) = modal_size(&records, 1) else {
        return records;
    };
    if modal < 2 {
        return records;
    }
    let mut repaired = Vec::new();
    for record in records {
        let Some(collapsed) = record.iter().position(|c| c.contains('\n')) else {
            repaired.push(record);
            continue;
        };
        let lines: Vec<&str> = record[collapsed].split('\n').collect();
        let swallowed = &lines[1..];
        let looks_like_unclosed = !swallowed.is_empty()
            && swallowed
                .iter()
                .all(|l| cells_of(l, delimiter).len() == modal);
        if !looks_like_unclosed {
            repaired.push(record);
            continue;
        }
        let mut head: Vec<String> = record.iter().take(collapsed).cloned().collect();
        head.extend(cells_of(lines[0], delimiter));
        repaired.push(head);
        for l in swallowed {
            repaired.push(cells_of(l, delimiter));
        }
    }
    repaired
}

fn cells_of(line: &str, delimiter: char) -> Vec<String> {
    let mut r = parse_records(line, delimiter);
    if r.is_empty() {
        vec![line.trim().to_string()]
    } else {
        r.remove(0)
    }
}

fn modal_size(rows: &[Vec<String>], min_size: usize) -> Option<usize> {
    modal(rows, min_size).map(|(k, _)| k)
}

fn modal(rows: &[Vec<String>], min_size: usize) -> Option<(usize, usize)> {
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for r in rows {
        if r.len() >= min_size {
            *counts.entry(r.len()).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
}

fn detect_delimiter(text: &str) -> char {
    let mut best = DELIMITERS[0];
    let mut best_score = i64::MIN;
    for candidate in DELIMITERS {
        let rows: Vec<Vec<String>> = parse_records(text, candidate).into_iter().take(8).collect();
        if rows.is_empty() {
            continue;
        }
        let Some((_, modal_count)) = modal(&rows, 2) else {
            continue;
        };
        let col_sum: i64 = rows
            .iter()
            .map(|r| (r.len() as i64 - 1).clamp(0, 1 << 20))
            .sum();
        let score = (modal_count as i64) * 1000 + col_sum;
        if score > best_score {
            best_score = score;
            best = candidate;
        }
    }
    best
}

/// `"Artist - Title"`, `"Title by Artist"`, `"01. Artist - Title"` or a bare link.
pub fn parse_text_track(line: &str) -> PlaylistFileRow {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return PlaylistFileRow::new("");
    }
    if let Some(video_id) = youtube_id_of(trimmed) {
        return PlaylistFileRow {
            title: String::new(),
            artist: String::new(),
            album: None,
            video_id: Some(video_id),
        };
    }
    let cleaned = NUMBERING_RE.replace(trimmed, "").trim().to_string();
    if cleaned.is_empty() {
        return PlaylistFileRow::new("");
    }

    if let Some(by) = BY_SEPARATOR_RE.find(&cleaned) {
        let title = cleaned[..by.start()].trim();
        if !title.is_empty() {
            return PlaylistFileRow {
                title: title.to_string(),
                artist: cleaned[by.end()..].trim().to_string(),
                album: None,
                video_id: None,
            };
        }
    }
    if let Some(sep) = DASH_SEPARATOR_RE.find(&cleaned) {
        return PlaylistFileRow {
            title: cleaned[sep.end()..].trim().to_string(),
            artist: cleaned[..sep.start()].trim().to_string(),
            album: None,
            video_id: None,
        };
    }
    PlaylistFileRow::new(cleaned)
}

fn parse_m3u(lines: &[String]) -> Vec<PlaylistFileRow> {
    let mut tracks = Vec::new();
    let mut pending: Option<PlaylistFileRow> = None;

    for line in lines {
        if line.to_ascii_uppercase().starts_with("#EXTINF:") {
            if let Some(prev) = pending.take() {
                tracks.push(prev);
            }
            let info = &line[line.find(':').map_or(0, |i| i + 1)..];
            let artist = EXTINF_ARTIST_RE
                .captures(info)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());
            let title = EXTINF_TITLE_RE
                .captures(info)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());
            pending = match (artist, title) {
                (Some(a), Some(t)) => Some(PlaylistFileRow {
                    title: t,
                    artist: a,
                    album: None,
                    video_id: None,
                }),
                _ => {
                    let after_comma = info.find(',').map(|c| &info[c + 1..]).unwrap_or("");
                    Some(parse_text_track(after_comma))
                }
            };
        } else if line.is_empty() || line.starts_with('#') {
            continue;
        } else {
            let video_id = youtube_id_of(line);
            let mut file = line.rsplit('/').next().unwrap_or(line);
            file = file.rsplit('\\').next().unwrap_or(file);
            let stem = match file.rfind('.') {
                Some(dot) => &file[..dot],
                None => file,
            };
            let track = match pending.take() {
                Some(p) => p,
                None => {
                    if let Some(ref vid) = video_id {
                        PlaylistFileRow {
                            title: String::new(),
                            artist: String::new(),
                            album: None,
                            video_id: Some(vid.clone()),
                        }
                    } else {
                        parse_text_track(stem)
                    }
                }
            };
            tracks.push(track.with_video_id(video_id));
        }
    }
    if let Some(last) = pending {
        tracks.push(last);
    }
    tracks
}

/// YouTube video id from a bare id or a `youtube.com` / `youtu.be` /
/// `music.youtube.com` link; `None` otherwise.
pub fn youtube_id_of(value: &str) -> Option<String> {
    let v = value.trim();
    if VIDEO_ID_RE.is_match(v) {
        return Some(v.to_string());
    }
    let lower = v.to_ascii_lowercase();
    if !lower.starts_with("https://") && !lower.starts_with("http://") {
        return None;
    }
    let uri = Url::parse(v).ok()?;
    let host = uri.host_str()?.to_ascii_lowercase();
    let mut id: Option<String> = None;
    if host == "youtu.be" || host.ends_with(".youtu.be") {
        id = uri
            .path_segments()
            .and_then(|mut s| s.next())
            .filter(|s| !s.is_empty())
            .map(ToString::to_string);
    } else if host == "youtube.com" || host.ends_with(".youtube.com") {
        id = uri
            .query_pairs()
            .find(|(k, _)| k == "v")
            .map(|(_, val)| val.into_owned());
        if id.is_none() {
            if let Some(seg) = uri.path_segments() {
                let parts: Vec<&str> = seg.collect();
                const PREFIXES: &[&str] = &["shorts", "embed", "live", "v"];
                if parts.len() >= 2 && PREFIXES.contains(&parts[0]) {
                    id = Some(parts[1].to_string());
                }
            }
        }
    }
    id.filter(|candidate| VIDEO_ID_RE.is_match(candidate))
}

fn parse_records(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    let finish_field = |row: &mut Vec<String>, field: &mut String| {
        row.push(field.trim().to_string());
        field.clear();
    };
    let finish_row = |records: &mut Vec<Vec<String>>, row: &mut Vec<String>, field: &mut String| {
        finish_field(row, field);
        if row.iter().any(|c| !c.is_empty()) {
            records.push(row.clone());
        }
        row.clear();
    };

    while i < chars.len() {
        let c = chars[i];
        if c == '"' && quoted && i + 1 < chars.len() && chars[i + 1] == '"' {
            field.push('"');
            i += 2;
            continue;
        }
        if c == '"' && (quoted || field.trim().is_empty()) {
            quoted = !quoted;
        } else if c == delimiter && !quoted {
            finish_field(&mut row, &mut field);
        } else if (c == '\n' || c == '\r') && !quoted {
            finish_row(&mut records, &mut row, &mut field);
            if c == '\r' && i + 1 < chars.len() && chars[i + 1] == '\n' {
                i += 1;
            }
        } else {
            field.push(c);
        }
        i += 1;
    }
    if quoted || !field.is_empty() || !row.is_empty() {
        finish_row(&mut records, &mut row, &mut field);
    }
    records
}

fn csv_cell(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// CSV a desktop playlist can be imported back through [`parse_playlist_file`].
pub fn export_playlist_csv(rows: &[PlaylistFileRow]) -> String {
    let mut buf = String::from("Title,Artist,Album,YouTube\n");
    for row in rows {
        buf.push_str(&format!(
            "{},{},{},{}\n",
            csv_cell(&row.title),
            csv_cell(&row.artist),
            csv_cell(row.album.as_deref().unwrap_or("")),
            csv_cell(row.video_id.as_deref().unwrap_or("")),
        ));
    }
    buf
}

/// M3U with a YouTube Music watch URL when the row has a video id.
pub fn export_playlist_m3u(title: &str, rows: &[PlaylistFileRow]) -> String {
    let mut buf = format!("#EXTM3U\n#PLAYLIST:{title}\n");
    for row in rows {
        let label = if row.artist.is_empty() {
            row.title.clone()
        } else {
            format!("{} - {}", row.artist, row.title)
        };
        buf.push_str(&format!("#EXTINF:-1,{label}\n"));
        if let Some(id) = row.video_id.as_deref().filter(|s| !s.is_empty()) {
            buf.push_str(&format!("https://music.youtube.com/watch?v={id}\n"));
        } else {
            buf.push_str(&format!("{label}\n"));
        }
    }
    buf
}

/// Deduplicates parsed rows by [`PlaylistFileRow::identity`], preserving order.
pub fn deduplicate_rows(rows: Vec<PlaylistFileRow>) -> Vec<PlaylistFileRow> {
    let mut seen = HashSet::new();
    rows.into_iter()
        .filter(|r| seen.insert(r.identity()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csv_and_export_roundtrip() {
        let csv = "Track Name,Artist Name,Album Name,YouTube URL\n\
                   \"Creep\",\"Radiohead\",\"Pablo Honey\",https://www.youtube.com/watch?v=XFkzRNyygfk\n\
                   \"Everything In Its Right Place\",\"Radiohead\",\"Kid A\",\n";
        let rows = parse_playlist_file(csv, "exportify.csv");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "Creep");
        assert_eq!(rows[0].artist, "Radiohead");
        assert_eq!(rows[0].album.as_deref(), Some("Pablo Honey"));
        assert_eq!(rows[0].video_id.as_deref(), Some("XFkzRNyygfk"));
        assert_eq!(rows[1].video_id, None);

        let exported = export_playlist_csv(&rows);
        let reparsed = parse_playlist_file(&exported, "roundtrip.csv");
        assert_eq!(reparsed, rows);
    }

    #[test]
    fn parse_m3u_and_plain_text() {
        let m3u = "#EXTM3U\n#EXTINF:210,Daft Punk - Instant Crush\nhttps://youtu.be/a5uQMwRMHcs\n";
        let rows = parse_playlist_file(m3u, "mix.m3u8");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].artist, "Daft Punk");
        assert_eq!(rows[0].title, "Instant Crush");
        assert_eq!(rows[0].video_id.as_deref(), Some("a5uQMwRMHcs"));

        let txt = "01. Pink Floyd - Time\nWish You Were Here by Pink Floyd\n";
        let txt_rows = parse_playlist_file(txt, "list.txt");
        assert_eq!(txt_rows.len(), 2);
        assert_eq!(txt_rows[0].artist, "Pink Floyd");
        assert_eq!(txt_rows[0].title, "Time");
        assert_eq!(txt_rows[1].title, "Wish You Were Here");
        assert_eq!(txt_rows[1].artist, "Pink Floyd");
    }

    #[test]
    fn decode_utf16_bom_and_youtube_ids() {
        let le_bytes = [0xFF, 0xFE, b'H', 0x00, b'i', 0x00];
        assert_eq!(decode_playlist_bytes(&le_bytes), "Hi");
        assert_eq!(
            youtube_id_of("https://music.youtube.com/watch?v=dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            youtube_id_of("https://youtube.com/shorts/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(suggested_playlist_title("My_Mix-2026.csv"), "My Mix 2026");
    }
}
