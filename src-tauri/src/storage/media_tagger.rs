use std::sync::OnceLock;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::Rng;

/// Embedded-metadata writer for downloaded audio.
///
/// Pure Rust, matching `media_tagger.dart` 1:1:
/// - YouTube Opus (WebM) -> Ogg Opus `.opus`
/// - FLAC -> rebuilt `VORBIS_COMMENT` + `PICTURE`
/// - FLAC-in-MP4 -> transmuxed native `.flac`
/// - MP3 -> ID3v2.3 (`TIT2`/`TPE1`/`TALB`/`USLT`/`APIC`)
/// - M4A -> `moov/udta/meta/ilst` with `stco`/`co64` offset fixup
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DownloadTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub lyrics: String,
    pub cover_bytes: Option<Vec<u8>>,
    pub cover_mime: String,
}

impl DownloadTags {
    pub fn has_cover(&self) -> bool {
        self.cover_bytes
            .as_ref()
            .is_some_and(|b| !b.is_empty())
            && !self.cover_mime.is_empty()
    }
}

/// Returned when tagging is impossible for an input (wrong container,
/// truncated file, etc.). Not fatal — callers keep raw bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggerSkip {
    pub reason: String,
}

impl TaggerSkip {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for TaggerSkip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TaggerSkip: {}", self.reason)
    }
}

impl std::error::Error for TaggerSkip {}

pub struct MediaTagger;

impl MediaTagger {
    /// Container of an audio buffer from magic bytes: `webm` (EBML),
    /// `mp4` (`ftyp`), `flac`, `mp3` (ID3 or frame sync), else `unknown`.
    pub fn detect_container(bytes: &[u8]) -> &'static str {
        if bytes.len() >= 4 && bytes[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
            return "webm";
        }
        if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
            return "mp4";
        }
        if bytes.len() >= 4 && &bytes[0..4] == b"fLaC" {
            return "flac";
        }
        if bytes.len() >= 3 && &bytes[0..3] == b"ID3" {
            return "mp3";
        }
        if bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
            return "mp3";
        }
        "unknown"
    }

    /// Image MIME from magic bytes. `None` when unrecognized.
    pub fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
        if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
            return Some("image/jpeg");
        }
        if bytes.len() >= 8 && bytes[0..4] == [0x89, 0x50, 0x4E, 0x47] {
            return Some("image/png");
        }
        if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            return Some("image/webp");
        }
        if bytes.len() >= 6 && &bytes[0..3] == b"GIF" {
            return Some("image/gif");
        }
        None
    }

    // -- WebM Opus -> Ogg Opus -----------------------------------------------

    pub fn remux_webm_opus_to_ogg(
        webm: &[u8],
        tags: &DownloadTags,
    ) -> Result<Vec<u8>, TaggerSkip> {
        let parsed = parse_webm_audio(webm)?;
        if parsed.packets.is_empty() {
            return Err(TaggerSkip::new("no audio packets"));
        }
        let serial: u32 = rand::thread_rng().gen_range(0..=0x7fff_ffff);
        let mut out: Vec<Vec<u8>> = Vec::new();
        let mut seq = 0u32;

        // OpusHead (identification header, BOS).
        out.push(ogg_page(serial, seq, 0x02, 0, &[&parsed.opus_head])?);
        seq += 1;

        // OpusTags (comment header).
        let tags_pkt = opus_tags_packet(tags);
        out.push(ogg_page(serial, seq, 0x00, 0, &[&tags_pkt])?);
        seq += 1;

        let mut granule = parsed.preskip as u64;
        let mut granules = Vec::with_capacity(parsed.packets.len());
        for p in &parsed.packets {
            granule += opus_packet_samples(&p.data)? as u64;
            granules.push(granule);
        }

        let mut idx = 0usize;
        while idx < parsed.packets.len() {
            let mut page_packets: Vec<&[u8]> = Vec::new();
            let mut last_granule = 0u64;
            let mut seg_count = 0usize;
            while idx < parsed.packets.len() {
                let need = ogg_seg_count(parsed.packets[idx].data.len());
                if !page_packets.is_empty() && seg_count + need > 255 {
                    break;
                }
                page_packets.push(&parsed.packets[idx].data);
                last_granule = granules[idx];
                seg_count += need;
                idx += 1;
            }
            let last = idx >= parsed.packets.len();
            out.push(ogg_page(
                serial,
                seq,
                if last { 0x04 } else { 0x00 },
                last_granule,
                &page_packets,
            )?);
            seq += 1;
        }

        Ok(concat_slices(&out))
    }

    // -- FLAC ----------------------------------------------------------------

    pub fn tag_flac(flac: &[u8], tags: &DownloadTags) -> Result<Vec<u8>, TaggerSkip> {
        if flac.len() < 42 || &flac[0..4] != b"fLaC" {
            return Err(TaggerSkip::new("not flac"));
        }
        let mut pos = 4usize;
        let mut streaminfo: Option<Vec<u8>> = None;
        let mut others: Vec<Vec<u8>> = Vec::new();
        let mut ended = false;

        while !ended {
            if pos + 4 > flac.len() {
                return Err(TaggerSkip::new("flac truncated"));
            }
            let hb = flac[pos];
            let block_type = hb & 0x7F;
            let last = (hb & 0x80) != 0;
            let len = ((flac[pos + 1] as usize) << 16)
                | ((flac[pos + 2] as usize) << 8)
                | (flac[pos + 3] as usize);
            if pos + 4 + len > flac.len() {
                return Err(TaggerSkip::new("flac block overrun"));
            }
            let body = &flac[pos + 4..pos + 4 + len];
            if block_type == 0 {
                if streaminfo.is_some() || body.len() != 34 {
                    return Err(TaggerSkip::new("bad streaminfo"));
                }
                streaminfo = Some(body.to_vec());
            } else if block_type == 3 || block_type == 4 || block_type == 6 {
                // Drop seektable, old comments, old pictures.
            } else {
                others.push(flac[pos..pos + 4 + len].to_vec());
            }
            pos += 4 + len;
            if last {
                ended = true;
            }
        }

        let streaminfo = streaminfo.ok_or_else(|| TaggerSkip::new("no streaminfo"))?;
        let audio = &flac[pos..];

        let mut blocks: Vec<Vec<u8>> = Vec::new();
        blocks.push(flac_block(0, &streaminfo, false));
        blocks.push(flac_block(4, &vorbis_comment_block(tags), false));
        if tags.has_cover() {
            if let Some(cover) = tags.cover_bytes.as_deref() {
                blocks.push(flac_block(
                    6,
                    &flac_picture_block(&tags.cover_mime, cover, 3),
                    false,
                ));
            }
        }
        blocks.extend(others);

        let block_count = blocks.len();
        for (i, b) in blocks.iter_mut().enumerate() {
            if i == block_count - 1 {
                b[0] |= 0x80;
            } else {
                b[0] &= 0x7F;
            }
        }

        let mut out = Vec::with_capacity(
            4 + blocks.iter().map(Vec::len).sum::<usize>() + audio.len(),
        );
        out.extend_from_slice(b"fLaC");
        for b in blocks {
            out.extend_from_slice(&b);
        }
        out.extend_from_slice(audio);
        Ok(out)
    }

    // -- MP3 (ID3v2.3) -------------------------------------------------------

    pub fn tag_mp3(mp3: &[u8], tags: &DownloadTags) -> Result<Vec<u8>, TaggerSkip> {
        let mut start = 0usize;
        let mut end = mp3.len();
        if end >= 10 && &mp3[0..3] == b"ID3" {
            let size = syncsafe(mp3, 6);
            start = 10 + size;
            if start > end {
                return Err(TaggerSkip::new("bad id3"));
            }
        }
        if end - start >= 128 && &mp3[end - 128..end - 125] == b"TAG" {
            end -= 128;
        }
        if end <= start {
            return Err(TaggerSkip::new("empty mp3"));
        }
        let audio = &mp3[start..end];
        let tag = id3v23_tag(tags);
        let mut out = Vec::with_capacity(tag.len() + audio.len());
        out.extend_from_slice(&tag);
        out.extend_from_slice(audio);
        Ok(out)
    }

    // -- M4A (moov/udta/meta/ilst) -------------------------------------------

    pub fn tag_m4a(m4a: &[u8], tags: &DownloadTags) -> Result<Vec<u8>, TaggerSkip> {
        let top = mp4_read_boxes(m4a, 0, m4a.len())?;
        let moov = top
            .iter()
            .find(|b| &b.kind == b"moov")
            .cloned()
            .ok_or_else(|| TaggerSkip::new("no moov"))?;
        let mut kids = mp4_read_boxes(m4a, moov.content_start, moov.content_end)?;
        let ilst_payload = mp4_box_raw(*b"ilst", &concat_slices(&mp4_ilst_items(tags)), 8);

        if let Some(udta_idx) = kids.iter().position(|k| &k.kind == b"udta") {
            let udta = &kids[udta_idx];
            let ukids = mp4_read_boxes(m4a, udta.content_start, udta.content_end)?;
            let new_udta_payload =
                if let Some(meta_idx) = ukids.iter().position(|k| &k.kind == b"meta") {
                    let meta = &ukids[meta_idx];
                    let mkids = mp4_read_boxes(m4a, meta.content_start + 4, meta.content_end)?;
                    let mut rebuilt = vec![0u8; 4];
                    let mut ilst_replaced = false;
                    for k in &mkids {
                        if &k.kind == b"ilst" && !ilst_replaced {
                            rebuilt.extend_from_slice(&ilst_payload);
                            ilst_replaced = true;
                        } else if &k.kind != b"ilst" {
                            rebuilt.extend_from_slice(&mp4_copy_box(m4a, k));
                        }
                    }
                    if !ilst_replaced {
                        rebuilt.extend_from_slice(&ilst_payload);
                    }
                    let mut urebuilt = Vec::new();
                    for (i, k) in ukids.iter().enumerate() {
                        if i == meta_idx {
                            urebuilt.extend_from_slice(&mp4_box_raw(*b"meta", &rebuilt, 8));
                        } else {
                            urebuilt.extend_from_slice(&mp4_copy_box(m4a, k));
                        }
                    }
                    urebuilt
                } else {
                    let hdlr = mp4_meta_hdlr();
                    let meta_inner = concat_slices(&[vec![0u8; 4], hdlr, ilst_payload]);
                    let meta = mp4_box_raw(*b"meta", &meta_inner, 8);
                    let mut rebuilt = Vec::new();
                    for k in &ukids {
                        rebuilt.extend_from_slice(&mp4_copy_box(m4a, k));
                    }
                    rebuilt.extend_from_slice(&meta);
                    rebuilt
                };
            kids[udta_idx].fresh_payload = Some(new_udta_payload);
        } else {
            let hdlr = mp4_meta_hdlr();
            let meta_inner = concat_slices(&[vec![0u8; 4], hdlr, ilst_payload]);
            let meta = mp4_box_raw(*b"meta", &meta_inner, 8);
            kids.push(Mp4Box {
                kind: *b"udta",
                header_len: 8,
                box_start: 0,
                content_start: 0,
                content_end: 0,
                fresh_payload: Some(meta),
            });
        }

        let old_moov_content_len = (moov.content_end - moov.content_start) as isize;
        let mut new_children = mp4_serialize_kids(m4a, &kids);
        let delta = new_children.len() as isize - old_moov_content_len;
        if delta != 0 {
            mp4_shift_chunk_offsets(&mut kids, m4a, delta)?;
            new_children = mp4_serialize_kids(m4a, &kids);
        }
        let new_moov = mp4_box_raw(moov.kind, &new_children, moov.header_len);
        let old_total = moov.content_end - moov.box_start;
        let mut out = Vec::with_capacity(m4a.len() + new_moov.len() - old_total);
        out.extend_from_slice(&m4a[..moov.box_start]);
        out.extend_from_slice(&new_moov);
        out.extend_from_slice(&m4a[moov.box_start + old_total..]);
        Ok(out)
    }

    // -- FLAC-in-MP4 ---------------------------------------------------------

    pub fn mp4_audio_sample_entry(mp4: &[u8]) -> Result<String, TaggerSkip> {
        let (fourcc, _) = mp4_first_audio_sample_entry(mp4)?;
        Ok(fourcc)
    }

    pub fn is_flac_in_mp4(mp4: &[u8]) -> bool {
        mp4_first_audio_sample_entry(mp4)
            .map(|(fourcc, _)| fourcc == "fLaC" || fourcc == "flac")
            .unwrap_or(false)
    }

    pub fn remux_flac_in_mp4_to_flac(
        mp4: &[u8],
        tags: &DownloadTags,
    ) -> Result<Vec<u8>, TaggerSkip> {
        let (fourcc, entry) = mp4_first_audio_sample_entry(mp4)?;
        if fourcc != "fLaC" && fourcc != "flac" {
            return Err(TaggerSkip::new(format!("mp4 entry {fourcc}")));
        }
        let streaminfo = mp4_flac_streaminfo(&entry)?;
        let frames = mp4_collect_flac_frames(mp4)?;
        if frames.is_empty() {
            return Err(TaggerSkip::new("no flac frames"));
        }
        let mut head = Vec::with_capacity(8 + streaminfo.len());
        head.extend_from_slice(b"fLaC");
        head.push(0x80);
        head.push(0);
        head.push(0);
        head.push(34);
        head.extend_from_slice(&streaminfo);

        let total = head.len() + frames.len();
        if total > 512 * 1024 * 1024 {
            return Err(TaggerSkip::new("flac too large"));
        }
        let mut minimal = Vec::with_capacity(total);
        minimal.extend_from_slice(&head);
        minimal.extend_from_slice(&frames);
        Self::tag_flac(&minimal, tags)
    }
}

// == MP4 helpers =============================================================

#[derive(Clone)]
struct Mp4Box {
    kind: [u8; 4],
    header_len: usize,
    box_start: usize,
    content_start: usize,
    content_end: usize,
    fresh_payload: Option<Vec<u8>>,
}

fn mp4_first_audio_sample_entry(d: &[u8]) -> Result<(String, Vec<u8>), TaggerSkip> {
    let top = mp4_read_boxes(d, 0, d.len())?;
    let moov = top
        .iter()
        .find(|b| &b.kind == b"moov")
        .ok_or_else(|| TaggerSkip::new("no moov"))?;
    for trak in mp4_find_children(d, moov, b"trak")? {
        for mdia in mp4_find_children(d, &trak, b"mdia")? {
            let Some(minf) = mp4_find_child(d, &mdia, b"minf")? else {
                continue;
            };
            let Some(stbl) = mp4_find_child(d, &minf, b"stbl")? else {
                continue;
            };
            let Some(stsd) = mp4_find_child(d, &stbl, b"stsd")? else {
                continue;
            };
            let payload = &d[stsd.content_start..stsd.content_end];
            if payload.len() < 8 {
                return Err(TaggerSkip::new("bad stsd"));
            }
            let entry_count = read_u32_be(payload, 4) as usize;
            let mut pos = 8usize;
            for _ in 0..entry_count {
                if pos + 8 > payload.len() {
                    return Err(TaggerSkip::new("stsd overrun"));
                }
                let size = read_u32_be(payload, pos) as usize;
                let fourcc = String::from_utf8_lossy(&payload[pos + 4..pos + 8]).into_owned();
                if size < 8 || pos + size > payload.len() {
                    return Err(TaggerSkip::new("stsd entry overrun"));
                }
                if mp4_track_is_audio(d, &trak) {
                    return Ok((fourcc, payload[pos..pos + size].to_vec()));
                }
                pos += size;
            }
        }
    }
    Err(TaggerSkip::new("no audio stsd"))
}

fn mp4_track_is_audio(d: &[u8], trak: &Mp4Box) -> bool {
    let Ok(mdias) = mp4_find_children(d, trak, b"mdia") else {
        return false;
    };
    for mdia in mdias {
        if let Ok(Some(hdlr)) = mp4_find_child(d, &mdia, b"hdlr") {
            let payload = &d[hdlr.content_start..hdlr.content_end];
            if payload.len() >= 12 && &payload[8..12] == b"soun" {
                return true;
            }
        }
    }
    false
}

fn mp4_flac_streaminfo(entry: &[u8]) -> Result<Vec<u8>, TaggerSkip> {
    if entry.len() < 8 + 28 + 8 {
        return Err(TaggerSkip::new("short flac entry"));
    }
    let payload = &entry[8..];
    let mut pos = 28usize;
    while pos + 8 <= payload.len() {
        let size = read_u32_be(payload, pos) as usize;
        if size < 8 || pos + size > payload.len() {
            return Err(TaggerSkip::new("flac box overrun"));
        }
        if &payload[pos + 4..pos + 8] == b"dfLa" {
            let body = &payload[pos + 8..pos + size];
            if body.len() < 4 + 4 + 34 {
                return Err(TaggerSkip::new("short dfLa"));
            }
            let block_header = &body[4..8];
            let streaminfo = &body[8..42];
            if (block_header[0] & 0x7F) != 0
                || block_header[1] != 0
                || block_header[2] != 0
                || block_header[3] != 34
            {
                return Err(TaggerSkip::new("bad dfLa header"));
            }
            return Ok(streaminfo.to_vec());
        }
        pos += size;
    }
    Err(TaggerSkip::new("no dfLa"))
}

fn mp4_collect_flac_frames(d: &[u8]) -> Result<Vec<u8>, TaggerSkip> {
    let top = mp4_read_boxes(d, 0, d.len())?;
    if !top.iter().any(|b| &b.kind == b"moof") {
        return Err(TaggerSkip::new("progressive flac-in-mp4 unsupported"));
    }
    let mut sizes: Vec<usize> = Vec::new();
    let mut mdat_payloads: Vec<&[u8]> = Vec::new();
    let mut total_mdat = 0usize;

    for b in &top {
        if &b.kind == b"moof" {
            sizes.extend(mp4_trun_sample_sizes(d, b)?);
        } else if &b.kind == b"mdat" {
            let payload = &d[b.content_start..b.content_end];
            mdat_payloads.push(payload);
            total_mdat += payload.len();
            if total_mdat > 512 * 1024 * 1024 {
                return Err(TaggerSkip::new("mdat too large"));
            }
        }
    }

    if sizes.is_empty() {
        return Err(TaggerSkip::new("no trun sizes"));
    }
    let mut total_sizes = 0usize;
    for &s in &sizes {
        if s == 0 || s > 16 * 1024 * 1024 {
            return Err(TaggerSkip::new("bad sample size"));
        }
        total_sizes += s;
    }
    if total_sizes != total_mdat {
        return Err(TaggerSkip::new("size mismatch"));
    }

    let mut out = Vec::with_capacity(total_mdat);
    for p in mdat_payloads {
        out.extend_from_slice(p);
    }

    let mut pos = 0usize;
    for s in sizes {
        if pos + 2 > out.len() || pos + s > out.len() {
            return Err(TaggerSkip::new("frame overrun"));
        }
        if out[pos] != 0xFF || (out[pos + 1] & 0xF8) != 0xF8 {
            return Err(TaggerSkip::new("bad flac sync"));
        }
        pos += s;
    }
    Ok(out)
}

fn mp4_trun_sample_sizes(d: &[u8], moof: &Mp4Box) -> Result<Vec<usize>, TaggerSkip> {
    let mut sizes = Vec::new();
    for traf in mp4_find_children(d, moof, b"traf")? {
        for trun in mp4_find_children(d, &traf, b"trun")? {
            let raw = &d[trun.content_start..trun.content_end];
            if raw.len() < 8 {
                return Err(TaggerSkip::new("short trun"));
            }
            let flags = ((raw[1] as u32) << 16) | ((raw[2] as u32) << 8) | (raw[3] as u32);
            let sample_count = read_u32_be(raw, 4) as usize;
            if sample_count == 0 || sample_count > 100_000 {
                return Err(TaggerSkip::new("bad sample count"));
            }
            let mut pos = 8usize;
            if (flags & 0x1) != 0 {
                pos += 4;
            }
            if (flags & 0x4) != 0 {
                pos += 4;
            }
            let mut per_sample = 0usize;
            let mut size_offset = 0usize;
            if (flags & 0x100) != 0 {
                size_offset = per_sample;
                per_sample += 4;
            }
            let mut has_size = false;
            if (flags & 0x200) != 0 {
                size_offset = per_sample;
                per_sample += 4;
                has_size = true;
            }
            if (flags & 0x400) != 0 {
                per_sample += 4;
            }
            if (flags & 0x800) != 0 {
                per_sample += 4;
            }
            if !has_size {
                return Err(TaggerSkip::new("trun without sizes"));
            }
            if raw.len() < pos + per_sample * sample_count {
                return Err(TaggerSkip::new("trun overrun"));
            }
            for i in 0..sample_count {
                sizes.push(read_u32_be(raw, pos + i * per_sample + size_offset) as usize);
            }
        }
    }
    Ok(sizes)
}

fn mp4_find_child(d: &[u8], parent: &Mp4Box, kind: &[u8; 4]) -> Result<Option<Mp4Box>, TaggerSkip> {
    let kids = mp4_read_boxes(d, parent.content_start, parent.content_end)?;
    Ok(kids.into_iter().find(|k| &k.kind == kind))
}

fn mp4_find_children(d: &[u8], parent: &Mp4Box, kind: &[u8; 4]) -> Result<Vec<Mp4Box>, TaggerSkip> {
    let kids = mp4_read_boxes(d, parent.content_start, parent.content_end)?;
    Ok(kids.into_iter().filter(|k| &k.kind == kind).collect())
}

fn mp4_read_boxes(d: &[u8], start: usize, end: usize) -> Result<Vec<Mp4Box>, TaggerSkip> {
    let mut boxes = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let size32 = read_u32_be(d, pos) as u64;
        let kind: [u8; 4] = d[pos + 4..pos + 8].try_into().unwrap();
        let mut header_len = 8usize;
        let mut size = size32 as usize;
        if size32 == 1 {
            if pos + 16 > end {
                return Err(TaggerSkip::new("mp4 largesize"));
            }
            let size64 = read_u64_be(d, pos + 8);
            size = usize::try_from(size64).map_err(|_| TaggerSkip::new("mp4 box overrun"))?;
            header_len = 16;
        } else if size32 == 0 {
            size = end - pos;
        }
        if size < header_len || pos + size > end {
            return Err(TaggerSkip::new("mp4 box overrun"));
        }
        boxes.push(Mp4Box {
            kind,
            header_len,
            box_start: pos,
            content_start: pos + header_len,
            content_end: pos + size,
            fresh_payload: None,
        });
        pos += size;
    }
    if pos != end {
        return Err(TaggerSkip::new("mp4 trailing bytes"));
    }
    Ok(boxes)
}

fn mp4_box_raw(kind: [u8; 4], payload: &[u8], header_len: usize) -> Vec<u8> {
    let total = header_len + payload.len();
    let mut out = Vec::with_capacity(total);
    if header_len == 8 {
        out.extend_from_slice(&(total as u32).to_be_bytes());
        out.extend_from_slice(&kind);
    } else {
        out.extend_from_slice(&1u32.to_be_bytes());
        out.extend_from_slice(&kind);
        out.extend_from_slice(&(total as u64).to_be_bytes());
    }
    out.extend_from_slice(payload);
    out
}

fn mp4_meta_hdlr() -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&[0u8; 4]); // version/flags
    payload.extend_from_slice(&[0u8; 4]); // pre_defined
    payload.extend_from_slice(b"mdirappl");
    payload.extend_from_slice(&[0u8; 12]); // reserved
    payload.push(0x00);
    mp4_box_raw(*b"hdlr", &payload, 8)
}

fn mp4_data_box(kind: u32, payload: &[u8]) -> Vec<u8> {
    let mut inner = Vec::with_capacity(8 + payload.len());
    inner.extend_from_slice(&kind.to_be_bytes());
    inner.extend_from_slice(&0u32.to_be_bytes());
    inner.extend_from_slice(payload);
    mp4_box_raw(*b"data", &inner, 8)
}

fn mp4_text_item(fourcc: [u8; 4], text: &str) -> Vec<u8> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    mp4_box_raw(fourcc, &mp4_data_box(1, trimmed.as_bytes()), 8)
}

fn mp4_ilst_items(tags: &DownloadTags) -> Vec<Vec<u8>> {
    let mut items = vec![
        mp4_text_item([0xA9, b'n', b'a', b'm'], &tags.title),
        mp4_text_item([0xA9, b'A', b'R', b'T'], &tags.artist),
        mp4_text_item([0xA9, b'a', b'l', b'b'], &tags.album),
    ];
    let lyrics = tags.lyrics.replace("\r\n", "\n").trim().to_string();
    if !lyrics.is_empty() {
        items.push(mp4_text_item([0xA9, b'l', b'y', b'r'], &lyrics));
    }
    if tags.has_cover() {
        if let Some(cover) = tags.cover_bytes.as_deref() {
            let kind = if tags.cover_mime.to_lowercase().contains("png") {
                14
            } else {
                13
            };
            items.push(mp4_box_raw(*b"covr", &mp4_data_box(kind, cover), 8));
        }
    }
    items.into_iter().filter(|b| !b.is_empty()).collect()
}

fn mp4_copy_box(d: &[u8], b: &Mp4Box) -> Vec<u8> {
    match &b.fresh_payload {
        Some(payload) => mp4_box_raw(b.kind, payload, b.header_len),
        None => d[b.box_start..b.content_end].to_vec(),
    }
}

fn mp4_serialize_kids(d: &[u8], kids: &[Mp4Box]) -> Vec<u8> {
    let mut out = Vec::new();
    for k in kids {
        out.extend_from_slice(&mp4_copy_box(d, k));
    }
    out
}

fn is_mp4_container(kind: &[u8; 4]) -> bool {
    matches!(
        kind,
        b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"edts" | b"dinf" | b"udta"
    )
}

fn mp4_shift_chunk_offsets(
    kids: &mut [Mp4Box],
    d: &[u8],
    delta: isize,
) -> Result<(), TaggerSkip> {
    for k in kids.iter_mut() {
        if k.fresh_payload.is_some() {
            continue;
        }
        if &k.kind == b"stco" || &k.kind == b"co64" {
            let mut raw = d[k.content_start..k.content_end].to_vec();
            if raw.len() < 8 {
                return Err(TaggerSkip::new("bad stco"));
            }
            let count = read_u32_be(&raw, 4) as usize;
            if &k.kind == b"stco" {
                if raw.len() < 8 + count * 4 {
                    return Err(TaggerSkip::new("bad stco entries"));
                }
                for i in 0..count {
                    let off = 8 + i * 4;
                    let val = read_u32_be(&raw, off) as isize + delta;
                    raw[off..off + 4].copy_from_slice(&(val as u32).to_be_bytes());
                }
            } else {
                if raw.len() < 8 + count * 8 {
                    return Err(TaggerSkip::new("bad co64 entries"));
                }
                for i in 0..count {
                    let off = 8 + i * 8;
                    let val = read_u64_be(&raw, off) as i64 + delta as i64;
                    raw[off..off + 8].copy_from_slice(&(val as u64).to_be_bytes());
                }
            }
            k.fresh_payload = Some(raw);
        } else if is_mp4_container(&k.kind) {
            let mut sub = mp4_read_boxes(d, k.content_start, k.content_end)?;
            mp4_shift_chunk_offsets(&mut sub, d, delta)?;
            k.fresh_payload = Some(mp4_serialize_kids(d, &sub));
        } else if &k.kind == b"meta" {
            let mut sub = mp4_read_boxes(d, k.content_start + 4, k.content_end)?;
            mp4_shift_chunk_offsets(&mut sub, d, delta)?;
            let mut payload = d[k.content_start..k.content_start + 4].to_vec();
            payload.extend_from_slice(&mp4_serialize_kids(d, &sub));
            k.fresh_payload = Some(payload);
        }
    }
    Ok(())
}

// == Ogg / Opus / Vorbis helpers =============================================

fn ogg_crc_table() -> &'static [u32; 256] {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (i, slot) in table.iter_mut().enumerate() {
            let mut r = (i as u32) << 24;
            for _ in 0..8 {
                if (r & 0x8000_0000) != 0 {
                    r = (r << 1) ^ 0x04C1_1DB7;
                } else {
                    r <<= 1;
                }
            }
            *slot = r;
        }
        table
    })
}

fn ogg_crc(page: &[u8]) -> u32 {
    let table = ogg_crc_table();
    let mut crc = 0u32;
    for &b in page {
        let idx = ((crc >> 24) ^ (b as u32)) & 0xFF;
        crc = table[idx as usize] ^ (crc << 8);
    }
    crc
}

fn ogg_seg_count(packet_len: usize) -> usize {
    packet_len / 255 + 1
}

fn ogg_page(
    serial: u32,
    seq: u32,
    flags: u8,
    granule: u64,
    packets: &[&[u8]],
) -> Result<Vec<u8>, TaggerSkip> {
    let mut segs: Vec<u8> = Vec::new();
    let mut body_len = 0usize;
    for p in packets {
        let mut rem = p.len();
        while rem >= 255 {
            segs.push(255);
            rem -= 255;
        }
        segs.push(rem as u8);
        body_len += p.len();
    }
    if segs.len() > 255 {
        return Err(TaggerSkip::new("ogg page overflow"));
    }
    let mut out = vec![0u8; 27 + segs.len() + body_len];
    out[0..4].copy_from_slice(b"OggS");
    out[4] = 0;
    out[5] = flags;
    out[6..14].copy_from_slice(&granule.to_le_bytes());
    out[14..18].copy_from_slice(&serial.to_le_bytes());
    out[18..22].copy_from_slice(&seq.to_le_bytes());
    out[26] = segs.len() as u8;
    out[27..27 + segs.len()].copy_from_slice(&segs);

    let mut w = 27 + segs.len();
    for p in packets {
        out[w..w + p.len()].copy_from_slice(p);
        w += p.len();
    }
    let crc = ogg_crc(&out);
    out[22..26].copy_from_slice(&crc.to_le_bytes());
    Ok(out)
}

const OPUS_FRAME_SAMPLES: [usize; 32] = [
    480, 960, 1920, 2880, 480, 960, 1920, 2880, 480, 960, 1920, 2880, 480, 960, 480, 960, 120,
    240, 480, 960, 120, 240, 480, 960, 120, 240, 480, 960, 120, 240, 480, 960,
];

fn opus_packet_samples(p: &[u8]) -> Result<usize, TaggerSkip> {
    if p.is_empty() {
        return Err(TaggerSkip::new("empty opus packet"));
    }
    let toc = p[0];
    let frame = OPUS_FRAME_SAMPLES[((toc >> 3) & 31) as usize];
    let code = toc & 3;
    if code == 0 {
        return Ok(frame);
    }
    if code == 3 {
        if p.len() < 2 {
            return Err(TaggerSkip::new("bad opus toc"));
        }
        let count = (p[1] & 63) as usize;
        if count == 0 {
            return Err(TaggerSkip::new("bad opus count"));
        }
        return Ok(frame * count);
    }
    Ok(frame * 2)
}

fn opus_tags_packet(tags: &DownloadTags) -> Vec<u8> {
    let mut comments = Vec::new();
    if !tags.title.trim().is_empty() {
        comments.push(format!("TITLE={}", tags.title.trim()));
    }
    if !tags.artist.trim().is_empty() {
        comments.push(format!("ARTIST={}", tags.artist.trim()));
    }
    if !tags.album.trim().is_empty() {
        comments.push(format!("ALBUM={}", tags.album.trim()));
    }
    let lyrics = tags.lyrics.replace("\r\n", "\n").trim().to_string();
    if !lyrics.is_empty() {
        comments.push(format!("LYRICS={lyrics}"));
    }
    if tags.has_cover() {
        if let Some(cover) = tags.cover_bytes.as_deref() {
            let pic = BASE64.encode(flac_picture_block(&tags.cover_mime, cover, 3));
            comments.push(format!("METADATA_BLOCK_PICTURE={pic}"));
        }
    }
    let vendor = b"LastWave Desktop";
    let mut out = Vec::new();
    out.extend_from_slice(b"OpusTags");
    out.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    out.extend_from_slice(vendor);
    out.extend_from_slice(&(comments.len() as u32).to_le_bytes());
    for c in comments {
        let b = c.as_bytes();
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    }
    out
}

fn flac_picture_block(mime: &str, data: &[u8], pic_type: u32) -> Vec<u8> {
    let mime_bytes = mime.as_bytes();
    let (w, h) = image_dims(data);
    let mut out = Vec::with_capacity(32 + mime_bytes.len() + data.len());
    out.extend_from_slice(&pic_type.to_be_bytes());
    out.extend_from_slice(&(mime_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(mime_bytes);
    out.extend_from_slice(&0u32.to_be_bytes()); // description length
    out.extend_from_slice(&w.to_be_bytes());
    out.extend_from_slice(&h.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes()); // depth
    out.extend_from_slice(&0u32.to_be_bytes()); // colors
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(data);
    out
}

fn image_dims(d: &[u8]) -> (u32, u32) {
    if d.len() >= 24 && d[0..4] == [0x89, 0x50, 0x4E, 0x47] {
        let w = read_u32_be(d, 16);
        let h = read_u32_be(d, 20);
        if w > 0 && h > 0 && w <= 16000 && h <= 16000 {
            return (w, h);
        }
        return (0, 0);
    }
    if d.len() >= 4 && d[0] == 0xFF && d[1] == 0xD8 {
        let mut pos = 2usize;
        while pos + 9 < d.len() {
            if d[pos] != 0xFF {
                break;
            }
            let marker = d[pos + 1];
            if marker == 0xD8 || (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
                pos += 2;
                continue;
            }
            if pos + 4 > d.len() {
                break;
            }
            let seg_len = (((d[pos + 2] as usize) << 8) | (d[pos + 3] as usize)).max(0);
            if seg_len < 2 {
                break;
            }
            if (0xC0..=0xC3).contains(&marker) {
                if pos + 9 >= d.len() {
                    break;
                }
                let h = ((d[pos + 5] as u32) << 8) | (d[pos + 6] as u32);
                let w = ((d[pos + 7] as u32) << 8) | (d[pos + 8] as u32);
                if w > 0 && h > 0 && w <= 16000 && h <= 16000 {
                    return (w, h);
                }
                return (0, 0);
            }
            pos += 2 + seg_len;
        }
    }
    (0, 0)
}

fn vorbis_comment_block(tags: &DownloadTags) -> Vec<u8> {
    let mut comments = Vec::new();
    if !tags.title.trim().is_empty() {
        comments.push(format!("TITLE={}", tags.title.trim()));
    }
    if !tags.artist.trim().is_empty() {
        comments.push(format!("ARTIST={}", tags.artist.trim()));
    }
    if !tags.album.trim().is_empty() {
        comments.push(format!("ALBUM={}", tags.album.trim()));
    }
    let lyrics = tags.lyrics.replace("\r\n", "\n").trim().to_string();
    if !lyrics.is_empty() {
        comments.push(format!("LYRICS={lyrics}"));
    }
    comments.push("ENCODER=LastWave Desktop".to_string());
    let vendor = b"LastWave Desktop";

    let mut out = Vec::new();
    out.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    out.extend_from_slice(vendor);
    out.extend_from_slice(&(comments.len() as u32).to_le_bytes());
    for c in comments {
        let b = c.as_bytes();
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    }
    out
}

fn flac_block(block_type: u8, body: &[u8], last: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + body.len());
    out.push((if last { 0x80 } else { 0 }) | (block_type & 0x7F));
    out.push(((body.len() >> 16) & 0xFF) as u8);
    out.push(((body.len() >> 8) & 0xFF) as u8);
    out.push((body.len() & 0xFF) as u8);
    out.extend_from_slice(body);
    out
}

// == ID3v2.3 helpers =========================================================

fn syncsafe(d: &[u8], off: usize) -> usize {
    (((d[off] & 0x7F) as usize) << 21)
        | (((d[off + 1] & 0x7F) as usize) << 14)
        | (((d[off + 2] & 0x7F) as usize) << 7)
        | ((d[off + 3] & 0x7F) as usize)
}

fn id3_frame(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(10 + payload.len());
    out.extend_from_slice(id);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(payload);
    out
}

fn utf16le_bom(s: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in s.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

fn id3_text(id: &[u8; 4], text: &str) -> Vec<u8> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut payload = vec![0x01];
    payload.extend_from_slice(&utf16le_bom(text));
    id3_frame(id, &payload)
}

fn id3v23_tag(tags: &DownloadTags) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&id3_text(b"TIT2", &tags.title));
    body.extend_from_slice(&id3_text(b"TPE1", &tags.artist));
    body.extend_from_slice(&id3_text(b"TALB", &tags.album));

    let lyrics = tags.lyrics.replace("\r\n", "\n").trim().to_string();
    if !lyrics.is_empty() {
        let mut payload = vec![0x01];
        payload.extend_from_slice(b"eng");
        payload.push(0x00);
        payload.extend_from_slice(&utf16le_bom(&lyrics));
        body.extend_from_slice(&id3_frame(b"USLT", &payload));
    }

    if tags.has_cover() {
        if let Some(cover) = tags.cover_bytes.as_deref() {
            let mut payload = vec![0x00];
            payload.extend_from_slice(tags.cover_mime.as_bytes());
            payload.extend_from_slice(&[0x00, 0x03, 0x00]);
            payload.extend_from_slice(cover);
            body.extend_from_slice(&id3_frame(b"APIC", &payload));
        }
    }

    let size = body.len();
    let mut head = [0u8; 10];
    head[0..3].copy_from_slice(b"ID3");
    head[3] = 0x03;
    head[6] = ((size >> 21) & 0x7F) as u8;
    head[7] = ((size >> 14) & 0x7F) as u8;
    head[8] = ((size >> 7) & 0x7F) as u8;
    head[9] = (size & 0x7F) as u8;

    let mut out = Vec::with_capacity(10 + body.len());
    out.extend_from_slice(&head);
    out.extend_from_slice(&body);
    out
}

// == EBML / WebM helpers =====================================================

struct WebmPacket {
    #[allow(dead_code)]
    time_ms: i64,
    data: Vec<u8>,
}

struct WebmAudio {
    opus_head: Vec<u8>,
    preskip: u16,
    #[allow(dead_code)]
    time_scale: u64,
    packets: Vec<WebmPacket>,
}

struct EbmlCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> EbmlCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_byte(&mut self) -> Result<u8, TaggerSkip> {
        if self.pos >= self.data.len() {
            return Err(TaggerSkip::new("truncated"));
        }
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn skip(&mut self, n: i64) -> Result<(), TaggerSkip> {
        if n < 0 {
            return Err(TaggerSkip::new("skip overrun"));
        }
        let n_usize = usize::try_from(n).map_err(|_| TaggerSkip::new("skip overrun"))?;
        if self.pos + n_usize > self.data.len() {
            return Err(TaggerSkip::new("skip overrun"));
        }
        self.pos += n_usize;
        Ok(())
    }

    fn read_id(&mut self) -> Result<(u32, usize), TaggerSkip> {
        let b = self.read_byte()?;
        let len = if (b & 0x80) != 0 {
            1
        } else if (b & 0x40) != 0 {
            2
        } else if (b & 0x20) != 0 {
            3
        } else if (b & 0x10) != 0 {
            4
        } else {
            return Err(TaggerSkip::new("bad EBML id"));
        };
        let mut id = b as u32;
        for _ in 1..len {
            id = (id << 8) | (self.read_byte()? as u32);
        }
        Ok((id, len))
    }

    /// `(size, length)`; `-1` = unknown (all value bits set).
    fn read_size(&mut self) -> Result<(i64, usize), TaggerSkip> {
        let b = self.read_byte()?;
        let (len, mask) = if (b & 0x80) != 0 {
            (1, 0x7F)
        } else if (b & 0x40) != 0 {
            (2, 0x3F)
        } else if (b & 0x20) != 0 {
            (3, 0x1F)
        } else if (b & 0x10) != 0 {
            (4, 0x0F)
        } else if (b & 0x08) != 0 {
            (5, 0x07)
        } else if (b & 0x04) != 0 {
            (6, 0x03)
        } else if (b & 0x02) != 0 {
            (7, 0x01)
        } else if b == 0x01 {
            (8, 0x00)
        } else {
            return Err(TaggerSkip::new("bad EBML size"));
        };
        let mut v = (b & mask) as u64;
        for _ in 1..len {
            v = (v << 8) | (self.read_byte()? as u64);
        }
        let all_ones = if len == 8 {
            (1u64 << 56) - 1
        } else {
            (1u64 << (7 * len)) - 1
        };
        if v == all_ones {
            Ok((-1, len))
        } else {
            Ok((v as i64, len))
        }
    }
}

fn parse_webm_audio(webm: &[u8]) -> Result<WebmAudio, TaggerSkip> {
    let mut c = EbmlCursor::new(webm);
    let (hid, _) = c.read_id()?;
    if hid != 0x1A45_DFA3 {
        return Err(TaggerSkip::new("not ebml"));
    }
    let (hsize, _) = c.read_size()?;
    if hsize < 0 {
        return Err(TaggerSkip::new("bad ebml header"));
    }
    c.skip(hsize)?;

    let (sid, _) = c.read_id()?;
    if sid != 0x1853_8067 {
        return Err(TaggerSkip::new("no segment"));
    }
    let (seg_size, _) = c.read_size()?;
    let seg_end = if seg_size < 0 {
        webm.len()
    } else {
        c.pos + seg_size as usize
    };
    if seg_end > webm.len() {
        return Err(TaggerSkip::new("segment overrun"));
    }

    let mut time_scale = 1_000_000u64;
    let mut track_no: i64 = -1;
    let mut codec_private: Option<Vec<u8>> = None;
    let mut cluster_ranges: Vec<(usize, usize)> = Vec::new();

    while c.pos < seg_end {
        let (eid, _) = c.read_id()?;
        let (esize, _) = c.read_size()?;
        let e_end = if esize < 0 {
            seg_end
        } else {
            c.pos + esize as usize
        };
        if e_end > seg_end {
            return Err(TaggerSkip::new("element overrun"));
        }
        if eid == 0x1654_AE6B {
            let (tno, cp) = parse_tracks(webm, c.pos, e_end)?;
            track_no = tno;
            codec_private = cp;
        } else if eid == 0x1549_A966 {
            time_scale = parse_info_time_scale(webm, c.pos, e_end)?;
        } else if eid == 0x1F43_B675 {
            cluster_ranges.push((c.pos, e_end));
        }
        c.pos = e_end;
    }

    let mut packets = Vec::new();
    for (start, end) in cluster_ranges {
        parse_cluster(webm, start, end, track_no, &mut packets)?;
    }
    if track_no < 0 {
        return Err(TaggerSkip::new("no audio track"));
    }
    let Some(cp) = codec_private else {
        return Err(TaggerSkip::new("not opus"));
    };
    if cp.len() < 19 || &cp[0..8] != b"OpusHead" {
        return Err(TaggerSkip::new("not opus"));
    }
    let preskip = u16::from_le_bytes([cp[10], cp[11]]);
    Ok(WebmAudio {
        opus_head: cp[0..19].to_vec(),
        preskip,
        time_scale,
        packets,
    })
}

fn parse_tracks(d: &[u8], start: usize, end: usize) -> Result<(i64, Option<Vec<u8>>), TaggerSkip> {
    let mut c = EbmlCursor { data: d, pos: start };
    let mut track_no: i64 = -1;
    let mut priv_bytes: Option<Vec<u8>> = None;
    let mut codec = String::new();

    while c.pos < end {
        let (eid, _) = c.read_id()?;
        let (esize, _) = c.read_size()?;
        let e_end = if esize < 0 {
            end
        } else {
            c.pos + esize as usize
        };
        if e_end > end {
            return Err(TaggerSkip::new("tracks overrun"));
        }
        if eid == 0xAE {
            let mut cc = EbmlCursor { data: d, pos: c.pos };
            let mut no: i64 = -1;
            let mut kind: i64 = -1;
            let mut cid = String::new();
            let mut cp: Option<Vec<u8>> = None;
            while cc.pos < e_end {
                let (fid, _) = cc.read_id()?;
                let (fsize, _) = cc.read_size()?;
                let f_end = if fsize < 0 {
                    e_end
                } else {
                    cc.pos + fsize as usize
                };
                if f_end > e_end {
                    return Err(TaggerSkip::new("track overrun"));
                }
                if fid == 0xD7 {
                    no = ebml_uint(d, cc.pos, f_end) as i64;
                } else if fid == 0x83 {
                    kind = ebml_uint(d, cc.pos, f_end) as i64;
                } else if fid == 0x86 {
                    cid = String::from_utf8_lossy(&d[cc.pos..f_end])
                        .replace('\0', "")
                        .trim()
                        .to_string();
                } else if fid == 0x63A2 {
                    cp = Some(d[cc.pos..f_end].to_vec());
                }
                cc.pos = f_end;
            }
            if kind == 2 && track_no < 0 {
                track_no = no;
                priv_bytes = cp;
                codec = cid;
            }
        }
        c.pos = e_end;
    }

    if !codec.is_empty() && codec != "A_OPUS" {
        return Err(TaggerSkip::new(format!("codec {codec}")));
    }
    Ok((track_no, priv_bytes))
}

fn parse_info_time_scale(d: &[u8], start: usize, end: usize) -> Result<u64, TaggerSkip> {
    let mut c = EbmlCursor { data: d, pos: start };
    while c.pos < end {
        let (eid, _) = c.read_id()?;
        let (esize, _) = c.read_size()?;
        let e_end = if esize < 0 {
            end
        } else {
            c.pos + esize as usize
        };
        if e_end > end {
            return Err(TaggerSkip::new("info overrun"));
        }
        if eid == 0x2AD7B1 {
            let v = ebml_uint(d, c.pos, e_end);
            if v > 0 {
                return Ok(v);
            }
        }
        c.pos = e_end;
    }
    Ok(1_000_000)
}

fn parse_cluster(
    d: &[u8],
    start: usize,
    end: usize,
    want_track: i64,
    out: &mut Vec<WebmPacket>,
) -> Result<(), TaggerSkip> {
    let mut c = EbmlCursor { data: d, pos: start };
    let mut cluster_time: i64 = 0;
    while c.pos < end {
        let (eid, _) = c.read_id()?;
        let (esize, _) = c.read_size()?;
        let e_end = if esize < 0 {
            end
        } else {
            c.pos + esize as usize
        };
        if e_end > end {
            return Err(TaggerSkip::new("cluster overrun"));
        }
        if eid == 0xE7 {
            cluster_time = ebml_uint(d, c.pos, e_end) as i64;
        } else if eid == 0xA3 {
            let _ = collect_block(d, c.pos, e_end, cluster_time, want_track, out);
        } else if eid == 0xA0 {
            let mut cc = EbmlCursor { data: d, pos: c.pos };
            while cc.pos < e_end {
                let (bid, _) = cc.read_id()?;
                let (bsize, _) = cc.read_size()?;
                let b_end = if bsize < 0 {
                    e_end
                } else {
                    cc.pos + bsize as usize
                };
                if b_end > e_end {
                    return Err(TaggerSkip::new("blockgroup overrun"));
                }
                if bid == 0xA1 {
                    let _ = collect_block(d, cc.pos, b_end, cluster_time, want_track, out);
                }
                cc.pos = b_end;
            }
        }
        c.pos = e_end;
    }
    Ok(())
}

fn collect_block(
    d: &[u8],
    start: usize,
    end: usize,
    cluster_time: i64,
    want_track: i64,
    out: &mut Vec<WebmPacket>,
) -> Result<(), TaggerSkip> {
    if want_track < 0 {
        return Ok(());
    }
    let mut pos = start;
    let (track, track_len) = read_vint(d, pos, end)?;
    pos += track_len;
    if pos + 3 > end {
        return Ok(());
    }
    let tc = i16::from_be_bytes([d[pos], d[pos + 1]]) as i64;
    let flags = d[pos + 2];
    pos += 3;
    if track != want_track {
        return Ok(());
    }
    let lace = (flags >> 1) & 3;
    let mut frames: Vec<Vec<u8>> = Vec::new();
    if lace == 0 {
        if pos >= end {
            return Ok(());
        }
        frames.push(d[pos..end].to_vec());
    } else {
        if pos >= end {
            return Ok(());
        }
        let count = d[pos] as usize + 1;
        pos += 1;
        let mut sizes: Vec<usize> = Vec::new();
        if lace == 1 {
            for _ in 0..count - 1 {
                let mut s = 0usize;
                loop {
                    if pos >= end {
                        return Ok(());
                    }
                    let b = d[pos];
                    pos += 1;
                    s += b as usize;
                    if b != 255 {
                        break;
                    }
                }
                sizes.push(s);
            }
        } else if lace == 2 {
            let each = (end - pos) / count;
            if each * count != end - pos {
                return Ok(());
            }
            for _ in 0..count - 1 {
                sizes.push(each);
            }
        } else {
            let (first, flen) = read_vint(d, pos, end)?;
            pos += flen;
            sizes.push(first as usize);
            for i in 1..count - 1 {
                let (v, vl) = read_vint(d, pos, end)?;
                pos += vl;
                let bias = (1i64 << (7 * vl - 1)) - 1;
                let next_sz = sizes[i - 1] as i64 + v - bias;
                if next_sz < 0 {
                    return Ok(());
                }
                sizes.push(next_sz as usize);
            }
        }
        for s in sizes {
            if pos + s > end {
                return Ok(());
            }
            frames.push(d[pos..pos + s].to_vec());
            pos += s;
        }
        if pos < end {
            frames.push(d[pos..end].to_vec());
        } else if frames.len() < count - 1 {
            return Ok(());
        }
    }

    let t = (cluster_time + tc).max(0);
    for f in frames {
        if f.is_empty() {
            continue;
        }
        out.push(WebmPacket { time_ms: t, data: f });
    }
    Ok(())
}

fn ebml_uint(d: &[u8], start: usize, end: usize) -> u64 {
    let mut v = 0u64;
    for &b in &d[start..end] {
        v = (v << 8) | (b as u64);
    }
    v
}

fn read_vint(d: &[u8], pos: usize, end: usize) -> Result<(i64, usize), TaggerSkip> {
    if pos >= end {
        return Err(TaggerSkip::new("vint overrun"));
    }
    let b = d[pos];
    let (len, mask) = if (b & 0x80) != 0 {
        (1, 0x7F)
    } else if (b & 0x40) != 0 {
        (2, 0x3F)
    } else if (b & 0x20) != 0 {
        (3, 0x1F)
    } else if (b & 0x10) != 0 {
        (4, 0x0F)
    } else if (b & 0x08) != 0 {
        (5, 0x07)
    } else if (b & 0x04) != 0 {
        (6, 0x03)
    } else if (b & 0x02) != 0 {
        (7, 0x01)
    } else if b == 0x01 {
        (8, 0x00)
    } else {
        return Err(TaggerSkip::new("bad vint"));
    };
    if pos + len > end {
        return Err(TaggerSkip::new("vint overrun"));
    }
    let mut v = (b & mask) as i64;
    for i in 1..len {
        v = (v << 8) | (d[pos + i] as i64);
    }
    Ok((v, len))
}

fn read_u32_be(d: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

fn read_u64_be(d: &[u8], off: usize) -> u64 {
    u64::from_be_bytes([
        d[off],
        d[off + 1],
        d[off + 2],
        d[off + 3],
        d[off + 4],
        d[off + 5],
        d[off + 6],
        d[off + 7],
    ])
}

fn concat_slices(parts: &[Vec<u8>]) -> Vec<u8> {
    let total = parts.iter().map(Vec::len).sum();
    let mut out = Vec::with_capacity(total);
    for p in parts {
        out.extend_from_slice(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_containers_and_image_mimes() {
        assert_eq!(MediaTagger::detect_container(b"\x1A\x45\xDF\xA3rest"), "webm");
        assert_eq!(MediaTagger::detect_container(b"\x00\x00\x00\x18ftypM4A "), "mp4");
        assert_eq!(MediaTagger::detect_container(b"fLaC\x00\x00"), "flac");
        assert_eq!(MediaTagger::detect_container(b"ID3\x03\x00"), "mp3");
        assert_eq!(MediaTagger::detect_container(b"\xFF\xFB\x90\x00"), "mp3");
        assert_eq!(MediaTagger::detect_container(b"junk"), "unknown");

        assert_eq!(
            MediaTagger::sniff_image_mime(b"\xFF\xD8\xFF\xE0"),
            Some("image/jpeg")
        );
        assert_eq!(
            MediaTagger::sniff_image_mime(b"\x89PNG\r\n\x1A\n"),
            Some("image/png")
        );
        assert_eq!(
            MediaTagger::sniff_image_mime(b"RIFF1234WEBP"),
            Some("image/webp")
        );
        assert_eq!(MediaTagger::sniff_image_mime(b"GIF89a"), Some("image/gif"));
        assert_eq!(MediaTagger::sniff_image_mime(b"nope"), None);
    }

    #[test]
    fn tags_flac_and_mp3_cleanly() {
        let mut raw_flac = Vec::new();
        raw_flac.extend_from_slice(b"fLaC");
        raw_flac.push(0x80);
        raw_flac.extend_from_slice(&[0, 0, 34]);
        raw_flac.extend_from_slice(&[0x12; 34]);
        raw_flac.extend_from_slice(&[0xFF, 0xF8, 0x01, 0x02]);

        let tags = DownloadTags {
            title: "Track".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            lyrics: "Line 1".to_string(),
            cover_bytes: None,
            cover_mime: String::new(),
        };
        let tagged_flac = MediaTagger::tag_flac(&raw_flac, &tags).unwrap();
        assert_eq!(&tagged_flac[0..4], b"fLaC");
        assert!(String::from_utf8_lossy(&tagged_flac).contains("TITLE=Track"));

        let raw_mp3 = b"\xFF\xFB\x90\x00\x01\x02\x03\x04";
        let tagged_mp3 = MediaTagger::tag_mp3(raw_mp3, &tags).unwrap();
        assert_eq!(&tagged_mp3[0..3], b"ID3");
    }
}
