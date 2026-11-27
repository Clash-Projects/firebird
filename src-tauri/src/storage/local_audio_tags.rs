use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::library_index::well_formed_utf16_units;

fn ext_strip_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\.\w+$").expect("valid ext strip regex"))
}

fn year_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?:19|20)\d{2}").expect("valid year regex"))
}

/// Embedded title, artist, album, year, and cover art. Filename parsing is
/// the fallback when a file has no ID3v2, ID3v1, FLAC Vorbis, or MP4 tags.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalAudioTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
    pub cover_bytes: Option<Vec<u8>>,
}

impl LocalAudioTags {
    pub fn read(path: &Path, filename: &str) -> Self {
        let mut tags = Self::default();
        if let Ok(mut file) = File::open(path) {
            if let Ok(meta) = file.metadata() {
                let length = meta.len() as usize;
                if length == 0 {
                    return Self::from_filename(filename);
                }
                let head_len = length.min(262_144);
                let mut head = vec![0u8; head_len];
                if file.read_exact(&mut head).is_ok() {
                    tags = Self::id3v2(&head)
                        .or_else(|| Self::flac(&head))
                        .or_else(|| Self::mp4(&head))
                        .unwrap_or_default();
                    if tags.title.is_empty() && length > 128 {
                        if file.seek(SeekFrom::Start((length - 128) as u64)).is_ok() {
                            let mut tail = [0u8; 128];
                            if file.read_exact(&mut tail).is_ok() {
                                if let Some(v1) = Self::id3v1(&tail) {
                                    if tags.title.is_empty() {
                                        tags.title = v1.title;
                                    }
                                    if tags.artist.is_empty() {
                                        tags.artist = v1.artist;
                                    }
                                    if tags.album.is_empty() {
                                        tags.album = v1.album;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if tags.title.is_empty() {
            let named = Self::from_filename(filename);
            return Self {
                title: named.title,
                artist: if !tags.artist.is_empty() {
                    tags.artist
                } else {
                    named.artist
                },
                album: tags.album,
                year: tags.year,
                cover_bytes: tags.cover_bytes,
            };
        }
        if tags.artist.is_empty() {
            let named = Self::from_filename(filename);
            return Self {
                title: tags.title,
                artist: named.artist,
                album: tags.album,
                year: tags.year,
                cover_bytes: tags.cover_bytes,
            };
        }
        tags
    }

    /// Same rule as a dropped file: `Artist - Title`, else the whole name.
    pub fn from_filename(filename: &str) -> Self {
        let base = ext_strip_regex().replace(filename, "");
        let parts: Vec<&str> = base.split(" - ").collect();
        if parts.len() > 1 {
            return Self {
                artist: parts[0].trim().to_string(),
                title: parts[1..].join(" - ").trim().to_string(),
                ..Default::default()
            };
        }
        Self {
            title: base.trim().to_string(),
            artist: "Local file".to_string(),
            ..Default::default()
        }
    }

    /// Tags from the start of an audio payload, used before a download is
    /// written so a year folder can be chosen.
    pub fn from_head(bytes: &[u8]) -> Self {
        Self::id3v2(bytes)
            .or_else(|| Self::flac(bytes))
            .or_else(|| Self::mp4(bytes))
            .unwrap_or_default()
    }

    pub fn year_in(value: &str) -> String {
        year_of(value)
    }

    fn id3v2(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 10 {
            return None;
        }
        if &bytes[0..3] != b"ID3" {
            return None;
        }
        let version = bytes[3];
        if !(3..=4).contains(&version) {
            return None;
        }
        let tag_size = synchsafe(bytes, 6);
        let mut offset = 10usize;
        let limit = (offset + tag_size).min(bytes.len());
        let mut title = String::new();
        let mut artist = String::new();
        let mut album = String::new();
        let mut year = String::new();
        let mut cover: Option<Vec<u8>> = None;

        while offset + 10 <= limit {
            let id = latin1_str(&bytes[offset..offset + 4]);
            if id.is_empty() || bytes[offset] == 0 {
                break;
            }
            let size = if version == 4 {
                synchsafe(bytes, offset + 4)
            } else {
                u32_be(bytes, offset + 4)
            };
            offset += 10;
            if size == 0 || offset + size > bytes.len() {
                break;
            }
            let frame = &bytes[offset..offset + size];
            offset += size;
            match id.as_str() {
                "TIT2" | "TPE1" | "TALB" | "TYER" | "TDRC" | "TDRL" => {
                    let text = id3_text(frame);
                    if id == "TIT2" && title.is_empty() {
                        title = text;
                    } else if id == "TPE1" && artist.is_empty() {
                        artist = text;
                    } else if id == "TALB" && album.is_empty() {
                        album = text;
                    } else if year.is_empty()
                        && (id == "TYER" || id == "TDRC" || id == "TDRL")
                    {
                        year = year_of(&text);
                    }
                }
                "APIC" if cover.is_none() => {
                    cover = apic_bytes(frame);
                }
                _ => {}
            }
        }

        if title.is_empty() && artist.is_empty() && album.is_empty() && cover.is_none() {
            return None;
        }
        Some(Self {
            title,
            artist,
            album,
            year,
            cover_bytes: cover,
        })
    }

    fn id3v1(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 128 {
            return None;
        }
        if &bytes[0..3] != b"TAG" {
            return None;
        }
        let title = latin1_zstring(&bytes[3..33]);
        let artist = latin1_zstring(&bytes[33..63]);
        let album = latin1_zstring(&bytes[63..93]);
        if title.is_empty() && artist.is_empty() {
            return None;
        }
        Some(Self {
            title,
            artist,
            album,
            ..Default::default()
        })
    }

    fn flac(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 || &bytes[0..4] != b"fLaC" {
            return None;
        }
        let mut offset = 4usize;
        while offset + 4 <= bytes.len() {
            let header = bytes[offset];
            let last = (header & 0x80) != 0;
            let block_type = header & 0x7F;
            let size = ((bytes[offset + 1] as usize) << 16)
                | ((bytes[offset + 2] as usize) << 8)
                | (bytes[offset + 3] as usize);
            offset += 4;
            if offset + size > bytes.len() {
                break;
            }
            if block_type == 4 && size > 4 {
                if let Some(tags) = vorbis(&bytes[offset..offset + size]) {
                    return Some(tags);
                }
            }
            offset += size;
            if last {
                break;
            }
        }
        None
    }

    fn mp4(bytes: &[u8]) -> Option<Self> {
        let mut title = String::new();
        let mut artist = String::new();
        let mut album = String::new();
        let mut year = String::new();
        let mut cover: Option<Vec<u8>> = None;

        #[allow(clippy::too_many_arguments)]
        fn walk(
            bytes: &[u8],
            start: usize,
            end: usize,
            ilst: bool,
            title: &mut String,
            artist: &mut String,
            album: &mut String,
            year: &mut String,
            cover: &mut Option<Vec<u8>>,
        ) {
            let mut offset = start;
            while offset + 8 <= end {
                let size = u32_be(bytes, offset);
                if size < 8 || offset + size > end {
                    break;
                }
                let atom = &bytes[offset + 4..offset + 8];
                let next = offset + size;
                let child = offset + 8;
                if ilst && atom != b"data" {
                    if child + 8 <= next && &bytes[child + 4..child + 8] == b"data" {
                        let payload = child + 16;
                        if payload < next {
                            let raw = &bytes[payload..next];
                            let text = trim_nulls(&String::from_utf8_lossy(raw));
                            if atom == [0xA9, b'n', b'a', b'm'] && title.is_empty() {
                                *title = text;
                            } else if (atom == [0xA9, b'A', b'R', b'T'] || atom == b"aART")
                                && artist.is_empty()
                            {
                                *artist = text;
                            } else if atom == [0xA9, b'a', b'l', b'b'] && album.is_empty() {
                                *album = text;
                            } else if atom == [0xA9, b'd', b'a', b'y'] && year.is_empty() {
                                *year = year_of(&text);
                            } else if atom == b"covr" && cover.is_none() && raw.len() > 32 {
                                *cover = Some(raw.to_vec());
                            }
                        }
                    }
                } else if atom == b"moov"
                    || atom == b"udta"
                    || atom == b"ilst"
                    || atom == b"trak"
                {
                    walk(
                        bytes,
                        child,
                        next,
                        ilst || atom == b"ilst",
                        title,
                        artist,
                        album,
                        year,
                        cover,
                    );
                } else if atom == b"meta" && child + 4 <= next {
                    walk(
                        bytes,
                        child + 4,
                        next,
                        ilst,
                        title,
                        artist,
                        album,
                        year,
                        cover,
                    );
                }
                offset = next;
            }
        }

        walk(
            bytes,
            0,
            bytes.len(),
            false,
            &mut title,
            &mut artist,
            &mut album,
            &mut year,
            &mut cover,
        );
        if title.is_empty() && artist.is_empty() && album.is_empty() && cover.is_none() {
            return None;
        }
        Some(Self {
            title,
            artist,
            album,
            year,
            cover_bytes: cover,
        })
    }
}

fn vorbis(block: &[u8]) -> Option<LocalAudioTags> {
    if block.len() < 8 {
        return None;
    }
    let vendor_len = u32_le(block, 0);
    let mut offset = 4 + vendor_len;
    if offset + 4 > block.len() {
        return None;
    }
    let count = u32_le(block, offset);
    offset += 4;
    let mut title = String::new();
    let mut artist = String::new();
    let mut album = String::new();
    let mut year = String::new();

    for _ in 0..count {
        if offset + 4 > block.len() {
            break;
        }
        let len = u32_le(block, offset);
        offset += 4;
        if offset + len > block.len() {
            break;
        }
        let comment = String::from_utf8_lossy(&block[offset..offset + len]);
        offset += len;
        let Some(eq) = comment.find('=') else {
            continue;
        };
        if eq == 0 {
            continue;
        }
        let key = comment[..eq].to_uppercase();
        let value = comment[eq + 1..].trim();
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "TITLE" if title.is_empty() => title = value.to_string(),
            "ARTIST" if artist.is_empty() => artist = value.to_string(),
            "ALBUM" if album.is_empty() => album = value.to_string(),
            "DATE" | "YEAR" if year.is_empty() => year = year_of(value),
            _ => {}
        }
    }

    if title.is_empty() && artist.is_empty() {
        return None;
    }
    Some(LocalAudioTags {
        title,
        artist,
        album,
        year,
        cover_bytes: None,
    })
}

fn apic_bytes(frame: &[u8]) -> Option<Vec<u8>> {
    if frame.len() < 6 {
        return None;
    }
    let mut i = 1usize;
    while i < frame.len() && frame[i] != 0 {
        i += 1;
    }
    i += 2;
    if i >= frame.len() {
        return None;
    }
    let encoding = frame[0];
    if encoding == 1 || encoding == 2 {
        while i + 1 < frame.len() && !(frame[i] == 0 && frame[i + 1] == 0) {
            i += 2;
        }
        i += 2;
    } else {
        while i < frame.len() && frame[i] != 0 {
            i += 1;
        }
        i += 1;
    }
    if i >= frame.len().saturating_sub(8) {
        return None;
    }
    Some(frame[i..].to_vec())
}

fn id3_text(frame: &[u8]) -> String {
    if frame.is_empty() {
        return String::new();
    }
    let encoding = frame[0];
    let data = &frame[1..];
    match encoding {
        0 => trim_nulls(&latin1_str(data)),
        3 => trim_nulls(&String::from_utf8_lossy(data)),
        1 | 2 => {
            let mut start = 0usize;
            let mut le = true;
            if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xFE {
                start = 2;
                le = true;
            } else if data.len() >= 2 && data[0] == 0xFE && data[1] == 0xFF {
                start = 2;
                le = false;
            }
            let mut units = Vec::new();
            let mut i = start;
            while i + 1 < data.len() {
                let unit = if le {
                    u16::from_le_bytes([data[i], data[i + 1]])
                } else {
                    u16::from_be_bytes([data[i], data[i + 1]])
                };
                if unit == 0 {
                    break;
                }
                units.push(unit);
                i += 2;
            }
            trim_nulls(&well_formed_utf16_units(&units))
        }
        _ => String::new(),
    }
}

fn latin1_str(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn latin1_zstring(bytes: &[u8]) -> String {
    let slice = match bytes.iter().position(|&b| b == 0) {
        Some(idx) => &bytes[..idx],
        None => bytes,
    };
    latin1_str(slice).trim().to_string()
}

fn synchsafe(bytes: &[u8], offset: usize) -> usize {
    if offset + 4 > bytes.len() {
        return 0;
    }
    (((bytes[offset] & 0x7F) as usize) << 21)
        | (((bytes[offset + 1] & 0x7F) as usize) << 14)
        | (((bytes[offset + 2] & 0x7F) as usize) << 7)
        | ((bytes[offset + 3] & 0x7F) as usize)
}

fn u32_be(bytes: &[u8], offset: usize) -> usize {
    if offset + 4 > bytes.len() {
        return 0;
    }
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]) as usize
}

fn u32_le(bytes: &[u8], offset: usize) -> usize {
    if offset + 4 > bytes.len() {
        return 0;
    }
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]) as usize
}

fn year_of(value: &str) -> String {
    year_regex()
        .find(value)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

fn trim_nulls(value: &str) -> String {
    value.replace('\0', "").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_filename_and_id3v2_and_flac() {
        let named = LocalAudioTags::from_filename("Daft Punk - Instant Crush.mp3");
        assert_eq!(named.artist, "Daft Punk");
        assert_eq!(named.title, "Instant Crush");

        let bare = LocalAudioTags::from_filename("demo.wav");
        assert_eq!(bare.artist, "Local file");
        assert_eq!(bare.title, "demo");

        // Build minimal ID3v2.3 header with TIT2 + TYER frames.
        let mut id3 = Vec::new();
        id3.extend_from_slice(b"ID3\x03\x00\x00");
        let mut frames = Vec::new();
        // TIT2: encoding 3 (utf-8) + "Song"
        frames.extend_from_slice(b"TIT2");
        frames.extend_from_slice(&5u32.to_be_bytes());
        frames.extend_from_slice(&[0, 0, 3]);
        frames.extend_from_slice(b"Song");
        // TYER: encoding 0 + "2024"
        frames.extend_from_slice(b"TYER");
        frames.extend_from_slice(&5u32.to_be_bytes());
        frames.extend_from_slice(&[0, 0, 0]);
        frames.extend_from_slice(b"2024");
        let sz = frames.len() as u32;
        id3.push(((sz >> 21) & 0x7F) as u8);
        id3.push(((sz >> 14) & 0x7F) as u8);
        id3.push(((sz >> 7) & 0x7F) as u8);
        id3.push((sz & 0x7F) as u8);
        id3.extend_from_slice(&frames);

        let parsed = LocalAudioTags::from_head(&id3);
        assert_eq!(parsed.title, "Song");
        assert_eq!(parsed.year, "2024");
    }
}
