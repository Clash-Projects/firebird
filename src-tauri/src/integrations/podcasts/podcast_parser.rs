use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::models::podcast_models::{
    PodcastEpisode, PodcastEpisodePage, PodcastShelf, PodcastShow, PodcastShowPage,
};

static MONTH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)\b")
        .expect("valid month regex")
});
static DAY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d{1,2}[/.]\d{1,2}[/.]\d{2,4}").expect("valid day regex"));
static HOURS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(\d+)\s*(?:h|hr|hrs|hour|hours)\s*(?:(\d+)\s*(?:m|min|mins|minute|minutes))?$",
    )
    .expect("valid hours regex")
});
static MINUTES_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(\d+)\s*(?:m|min|mins|minute|minutes)$").expect("valid minutes regex")
});

/// Parsers for WEB_REMIX podcast browse and search payloads.
pub struct PodcastParser;

impl PodcastParser {
    pub const EPISODE_SEARCH_FILTER: &'static str = "EgWKAQJIAWoSEBEQAxAQEAUQChAOEAkQBBAV";

    pub fn parse_home(root: &Value) -> Vec<PodcastShelf> {
        let mut carousels = Vec::new();
        collect(root, "musicCarouselShelfRenderer", &mut carousels);

        let mut shelves = Vec::new();
        for carousel in carousels {
            let Some(title) = shelf_title(carousel) else {
                continue;
            };
            if title.is_empty() {
                continue;
            }
            let carousel_val = Value::Object(carousel.clone());
            let mut show_renderers = Vec::new();
            collect(&carousel_val, "musicTwoRowItemRenderer", &mut show_renderers);
            let mut shows: Vec<PodcastShow> = Vec::new();
            for renderer in show_renderers {
                if let Some(show) = Self::parse_show_card(renderer) {
                    if shows.iter().all(|s| s.show_id != show.show_id) {
                        shows.push(show);
                    }
                }
            }
            let episodes = parse_episode_renderers(&carousel_val);
            if shows.is_empty() && episodes.is_empty() {
                continue;
            }
            shelves.push(PodcastShelf {
                title,
                shows,
                episodes,
            });
        }
        if !shelves.is_empty() {
            return shelves;
        }

        let mut fallback_shows = Vec::new();
        collect(root, "musicTwoRowItemRenderer", &mut fallback_shows);
        let mut parsed: Vec<PodcastShow> = Vec::new();
        for renderer in fallback_shows {
            if let Some(show) = Self::parse_show_card(renderer) {
                if parsed.iter().all(|s| s.show_id != show.show_id) {
                    parsed.push(show);
                }
            }
        }
        if !parsed.is_empty() {
            return vec![PodcastShelf {
                title: "Podcasts".to_string(),
                shows: parsed,
                episodes: Vec::new(),
            }];
        }

        let episodes = parse_episode_renderers(root);
        if episodes.is_empty() {
            return Vec::new();
        }
        vec![PodcastShelf {
            title: "Podcasts".to_string(),
            shows: Vec::new(),
            episodes,
        }]
    }

    pub fn parse_show(root: &Value, show_id: &str, fallback_title: &str) -> PodcastShowPage {
        let mut headers = Vec::new();
        collect(root, "musicResponsiveHeaderRenderer", &mut headers);
        if headers.is_empty() {
            collect(root, "musicDetailHeaderRenderer", &mut headers);
        }
        if headers.is_empty() {
            collect(root, "musicImmersiveHeaderRenderer", &mut headers);
        }
        let header = headers.first().copied();
        let title = header
            .and_then(title_text)
            .unwrap_or_default()
            .trim()
            .to_string();
        let resolved_title = if !title.is_empty() {
            title
        } else if !fallback_title.is_empty() {
            fallback_title.to_string()
        } else {
            "Podcast".to_string()
        };

        let episodes = parse_episode_renderers(root)
            .into_iter()
            .map(|ep| {
                let next_show_id = if ep.show_id.is_empty() {
                    Some(show_id.to_string())
                } else {
                    None
                };
                let next_show_title = if ep.show_title.is_empty() {
                    Some(resolved_title.clone())
                } else {
                    None
                };
                ep.with_show(next_show_id, next_show_title)
            })
            .collect();

        let show = PodcastShow {
            show_id: show_id.to_string(),
            title: resolved_title,
            publisher: header
                .and_then(|h| text_of(h.get("subtitle")))
                .unwrap_or_default(),
            description: header
                .and_then(|h| {
                    text_of(h.get("description")).or_else(|| text_of(h.get("descriptionText")))
                })
                .unwrap_or_default(),
            artwork_url: header
                .and_then(|h| artwork_url_in_map(h))
                .unwrap_or_default(),
        };

        PodcastShowPage {
            show,
            episodes,
            continuation: episode_continuation(root),
        }
    }

    pub fn parse_episode_continuation(
        root: &Value,
        show_id: Option<&str>,
        show_title: Option<&str>,
    ) -> PodcastEpisodePage {
        let episodes = parse_episode_renderers(root)
            .into_iter()
            .map(|ep| {
                let next_show_id = if ep.show_id.is_empty() {
                    show_id.map(ToString::to_string)
                } else {
                    None
                };
                let next_show_title = if ep.show_title.is_empty() {
                    show_title.map(ToString::to_string)
                } else {
                    None
                };
                ep.with_show(next_show_id, next_show_title)
            })
            .collect();

        PodcastEpisodePage {
            episodes,
            continuation: episode_continuation(root),
        }
    }

    pub fn parse_episode_search(
        root: &Value,
        require_podcast_marker: bool,
    ) -> Vec<PodcastEpisode> {
        let mut renderers = Vec::new();
        collect(root, "musicResponsiveListItemRenderer", &mut renderers);
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for renderer in renderers {
            if !is_episode_search_row(renderer, require_podcast_marker) {
                continue;
            }
            let Some(ep) = parse_responsive_episode(renderer) else {
                continue;
            };
            if seen.insert(ep.video_id.clone()) {
                out.push(ep);
            }
        }
        out
    }

    /// Search `params` on the Episodes chip, used to retry a filtered query.
    pub fn episode_search_params(root: &Value) -> Option<String> {
        let mut chips = Vec::new();
        collect(root, "chipCloudChipRenderer", &mut chips);
        for chip in chips {
            let label = text_of(chip.get("text"))
                .unwrap_or_default()
                .to_lowercase();
            if !label.contains("episode") {
                continue;
            }
            if let Some(params) = find_string_in_map(chip, "params") {
                if !params.is_empty() {
                    return Some(params);
                }
            }
        }
        None
    }

    pub fn parse_show_card(renderer: &Map<String, Value>) -> Option<PodcastShow> {
        let ids = browse_ids_in_map(renderer);
        let show_id = ids
            .into_iter()
            .find(|id| id.starts_with("MPSPPL") || id.starts_with("MPED"))?;
        let title = title_text(renderer)?.trim().to_string();
        if title.is_empty() {
            return None;
        }
        Some(PodcastShow {
            show_id,
            title,
            publisher: text_of(renderer.get("subtitle")).unwrap_or_default(),
            description: String::new(),
            artwork_url: artwork_url_in_map(renderer).unwrap_or_default(),
        })
    }
}

fn parse_episode_renderers(root: &Value) -> Vec<PodcastEpisode> {
    let mut multi = Vec::new();
    collect(root, "musicMultiRowListItemRenderer", &mut multi);
    let mut episodes = Vec::new();
    let mut seen = HashSet::new();
    for renderer in multi {
        if let Some(ep) = parse_multi_row_episode(renderer) {
            if seen.insert(ep.video_id.clone()) {
                episodes.push(ep);
            }
        }
    }
    if episodes.is_empty() {
        let mut responsive = Vec::new();
        collect(root, "musicResponsiveListItemRenderer", &mut responsive);
        for renderer in responsive {
            if !is_episode_search_row(renderer, false) {
                continue;
            }
            if let Some(ep) = parse_responsive_episode(renderer) {
                if seen.insert(ep.video_id.clone()) {
                    episodes.push(ep);
                }
            }
        }
    }
    episodes
        .into_iter()
        .filter(|e| !e.video_id.is_empty())
        .collect()
}

fn parse_multi_row_episode(renderer: &Map<String, Value>) -> Option<PodcastEpisode> {
    let video_id = episode_video_id(renderer)?;
    let title = title_text(renderer)?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    let subtitle = text_of(renderer.get("subtitle")).unwrap_or_default();
    let (published_label, show_title) = episode_meta(&subtitle);
    let show_id = browse_ids_in_map(renderer)
        .into_iter()
        .find(|id| id.starts_with("MPSPPL"))
        .unwrap_or_default();

    Some(PodcastEpisode {
        video_id,
        title,
        show_id,
        show_title: show_title.unwrap_or_default(),
        description: text_of(renderer.get("description")).unwrap_or_default(),
        artwork_url: artwork_url_in_map(renderer).unwrap_or_default(),
        duration_ms: duration_ms(renderer).unwrap_or(0),
        published_label: published_label.unwrap_or_default(),
    })
}

fn parse_responsive_episode(renderer: &Map<String, Value>) -> Option<PodcastEpisode> {
    let video_id = episode_video_id(renderer)?;
    let columns = renderer.get("flexColumns");
    let title = column_text(columns, 0)?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    let subtitle = column_text(columns, 1).unwrap_or_default();
    let (published_label, show_title) = episode_meta(&subtitle);
    let show_id = browse_ids_in_map(renderer)
        .into_iter()
        .find(|id| id.starts_with("MPSPPL"))
        .unwrap_or_default();

    Some(PodcastEpisode {
        video_id,
        title,
        show_id,
        show_title: show_title.unwrap_or_default(),
        description: String::new(),
        artwork_url: artwork_url_in_map(renderer).unwrap_or_default(),
        duration_ms: duration_ms(renderer).unwrap_or(0),
        published_label: published_label.unwrap_or_default(),
    })
}

fn is_episode_search_row(renderer: &Map<String, Value>, require_podcast_marker: bool) -> bool {
    if has_podcast_marker(renderer) {
        return true;
    }
    if require_podcast_marker {
        return false;
    }
    let page_type = find_string_in_map(renderer, "pageType");
    match page_type {
        Some(pt) => !pt.to_ascii_uppercase().contains("PODCAST"),
        None => true,
    }
}

fn has_podcast_marker(renderer: &Map<String, Value>) -> bool {
    if let Some(pt) = find_string_in_map(renderer, "pageType") {
        if pt.to_ascii_uppercase().contains("PODCAST") {
            return true;
        }
    }
    if let Some(vt) = find_string_in_map(renderer, "videoType") {
        if vt.to_ascii_uppercase().contains("PODCAST") {
            return true;
        }
    }
    if browse_ids_in_map(renderer)
        .iter()
        .any(|id| id.starts_with("MPED") || id.starts_with("MPSPPL"))
    {
        return true;
    }
    if let Some(subtitle) = column_text(renderer.get("flexColumns"), 1) {
        if subtitle.to_ascii_lowercase().contains("podcast") {
            return true;
        }
    }
    contains_podcast_episode_token(renderer)
}

fn contains_podcast_episode_token(map: &Map<String, Value>) -> bool {
    fn walk(val: &Value) -> bool {
        match val {
            Value::String(s) => s.to_ascii_uppercase().contains("PODCAST_EPISODE"),
            Value::Array(arr) => arr.iter().any(walk),
            Value::Object(obj) => obj
                .iter()
                .any(|(k, v)| k.to_ascii_uppercase().contains("PODCAST_EPISODE") || walk(v)),
            _ => false,
        }
    }
    map.iter()
        .any(|(k, v)| k.to_ascii_uppercase().contains("PODCAST_EPISODE") || walk(v))
}

fn episode_video_id(renderer: &Map<String, Value>) -> Option<String> {
    fn from_endpoint(node: Option<&Value>) -> Option<String> {
        let obj = node?.as_object()?;
        let watch = obj.get("watchEndpoint")?.as_object()?;
        let id = watch.get("videoId")?.as_str()?;
        if !id.is_empty() {
            Some(id.to_string())
        } else {
            None
        }
    }

    let playlist_item = renderer
        .get("playlistItemData")
        .and_then(Value::as_object)
        .and_then(|m| m.get("videoId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let direct = from_endpoint(renderer.get("onTap"))
        .or_else(|| from_endpoint(renderer.get("navigationEndpoint")))
        .or(playlist_item)
        .or_else(|| from_endpoint(renderer.get("doubleTapCommand")));
    if direct.is_some() {
        return direct;
    }

    if let Some(Value::Array(columns)) = renderer.get("flexColumns") {
        for column in columns {
            let Some(runs) = column
                .as_object()
                .and_then(|c| c.get("musicResponsiveListItemFlexColumnRenderer"))
                .and_then(Value::as_object)
                .and_then(|f| f.get("text"))
                .and_then(Value::as_object)
                .and_then(|t| t.get("runs"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for run in runs {
                if let Some(id) =
                    from_endpoint(run.as_object().and_then(|r| r.get("navigationEndpoint")))
                {
                    return Some(id);
                }
            }
        }
    }

    if let Some(found) = find_string_in_map(renderer, "videoId") {
        if found.len() == 11 {
            return Some(found);
        }
    }
    None
}

fn duration_ms(renderer: &Map<String, Value>) -> Option<i64> {
    if let Some(inner) = renderer
        .get("playbackProgress")
        .and_then(Value::as_object)
        .and_then(|p| p.get("musicPlaybackProgressRenderer"))
        .and_then(Value::as_object)
    {
        if let Some(ms) = parse_clock(text_of(inner.get("durationText")).as_deref()) {
            return Some(ms);
        }
    }
    if let Some(ms) = parse_clock(text_of(renderer.get("lengthText")).as_deref()) {
        return Some(ms);
    }
    if let Some(Value::Array(fixed)) = renderer.get("fixedColumns") {
        for column in fixed {
            let Some(runs) = column
                .as_object()
                .and_then(|c| c.get("musicResponsiveListItemFixedColumnRenderer"))
                .and_then(Value::as_object)
                .and_then(|f| f.get("text"))
                .and_then(Value::as_object)
                .and_then(|t| t.get("runs"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for run in runs {
                if let Some(ms) = parse_clock(text_of(Some(run)).as_deref()) {
                    return Some(ms);
                }
            }
        }
    }
    None
}

fn episode_meta(subtitle: &str) -> (Option<String>, Option<String>) {
    let segments: Vec<String> = subtitle
        .split(['•', '·'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();
    if segments.is_empty() {
        return (None, None);
    }
    let mut published = segments.iter().find(|s| is_date_label(s)).cloned();
    if published.is_none() {
        published = segments.iter().find(|s| !is_stat_segment(s)).cloned();
    }
    let show = segments
        .iter()
        .rev()
        .find(|s| {
            if published.as_deref() == Some(s.as_str()) {
                return false;
            }
            !is_stat_segment(s) && !is_date_label(s)
        })
        .cloned();
    (published, show)
}

fn is_date_label(value: &str) -> bool {
    value.to_ascii_lowercase().contains("ago") || MONTH_RE.is_match(value) || DAY_RE.is_match(value)
}

fn is_stat_segment(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("view")
        || lower.contains("play")
        || lower.contains("listen")
        || lower.contains("episode")
        || lower.contains("min")
        || lower.contains("hr")
        || lower.contains("season")
        || lower.contains("podcast")
}

fn parse_clock(value: Option<&str>) -> Option<i64> {
    let cleaned = value?.trim();
    if cleaned.is_empty() {
        return None;
    }
    let colon: Vec<&str> = cleaned.split(':').collect();
    if (2..=3).contains(&colon.len()) && colon.iter().all(|p| p.trim().parse::<i64>().is_ok()) {
        let mut seconds = 0i64;
        for part in colon {
            seconds = seconds * 60 + part.trim().parse::<i64>().ok()?;
        }
        if seconds > 0 {
            return Some(seconds * 1000);
        }
    }
    if let Some(caps) = HOURS_RE.captures(cleaned) {
        let h = caps
            .get(1)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0);
        let m = caps
            .get(2)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0);
        let total = h * 3600 + m * 60;
        if total > 0 {
            return Some(total * 1000);
        }
    }
    if let Some(caps) = MINUTES_RE.captures(cleaned) {
        let m = caps
            .get(1)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0);
        if m > 0 {
            return Some(m * 60_000);
        }
    }
    None
}

fn episode_continuation(root: &Value) -> Option<String> {
    let mut shelves = Vec::new();
    collect(root, "musicShelfRenderer", &mut shelves);
    for shelf in shelves {
        let shelf_val = Value::Object(shelf.clone());
        let mut multi = Vec::new();
        collect(&shelf_val, "musicMultiRowListItemRenderer", &mut multi);
        if multi.is_empty() {
            continue;
        }
        if let Some(token) = continuation_in(&shelf_val) {
            return Some(token);
        }
    }
    continuation_in(root)
}

fn continuation_in(element: &Value) -> Option<String> {
    let mut commands = Vec::new();
    collect(element, "continuationCommand", &mut commands);
    for command in commands {
        if let Some(token) = command.get("token").and_then(Value::as_str) {
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    let mut legacy = Vec::new();
    collect(element, "nextContinuationData", &mut legacy);
    for item in legacy {
        if let Some(token) = item.get("continuation").and_then(Value::as_str) {
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

fn column_text(columns: Option<&Value>, index: usize) -> Option<String> {
    let arr = columns?.as_array()?;
    let col = arr.get(index)?.as_object()?;
    let flex = col
        .get("musicResponsiveListItemFlexColumnRenderer")?
        .as_object()?;
    text_of(flex.get("text"))
}

fn shelf_title(element: &Map<String, Value>) -> Option<String> {
    if let Some(direct) = text_of(element.get("title")) {
        if !direct.is_empty() {
            return Some(direct);
        }
    }
    let header = element.get("header")?.as_object()?;
    for key in [
        "musicCarouselShelfBasicHeaderRenderer",
        "musicShelfHeaderRenderer",
        "musicCardShelfHeaderBasicRenderer",
        "musicHeaderRenderer",
    ] {
        if let Some(node) = header.get(key).and_then(Value::as_object) {
            if let Some(text) = text_of(node.get("title")) {
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

fn title_text(element: &Map<String, Value>) -> Option<String> {
    text_of(element.get("title")).or_else(|| text_of(element.get("name")))
}

fn text_of(element: Option<&Value>) -> Option<String> {
    let el = element?;
    match el {
        Value::String(s) => {
            if s.trim().is_empty() {
                None
            } else {
                Some(s.clone())
            }
        }
        Value::Object(map) => {
            if let Some(Value::Array(runs)) = map.get("runs") {
                let mut text = String::new();
                for run in runs {
                    match run {
                        Value::Object(r) => {
                            if let Some(s) = r.get("text").and_then(Value::as_str) {
                                text.push_str(s);
                            }
                        }
                        Value::String(s) => text.push_str(s),
                        _ => {}
                    }
                }
                if !text.trim().is_empty() {
                    return Some(text);
                }
            }
            if let Some(simple) = map.get("simpleText").and_then(Value::as_str) {
                if !simple.trim().is_empty() {
                    return Some(simple.to_string());
                }
            }
            None
        }
        _ => None,
    }
}

fn artwork_url_in_map(map: &Map<String, Value>) -> Option<String> {
    fn walk<'a>(node: &'a Value, arrays: &mut Vec<&'a Vec<Value>>) {
        match node {
            Value::Object(obj) => {
                if let Some(Value::Array(thumbs)) = obj.get("thumbnails") {
                    if !thumbs.is_empty() {
                        arrays.push(thumbs);
                    }
                }
                for child in obj.values() {
                    walk(child, arrays);
                }
            }
            Value::Array(arr) => {
                for child in arr {
                    walk(child, arrays);
                }
            }
            _ => {}
        }
    }

    let mut arrays = Vec::new();
    if let Some(Value::Array(thumbs)) = map.get("thumbnails") {
        if !thumbs.is_empty() {
            arrays.push(thumbs);
        }
    }
    for child in map.values() {
        walk(child, &mut arrays);
    }
    let best = arrays.first()?;
    let mut url: Option<String> = None;
    for item in best.iter().rev() {
        if let Some(u) = item
            .as_object()
            .and_then(|o| o.get("url"))
            .and_then(Value::as_str)
        {
            if !u.is_empty() {
                url = Some(u.to_string());
                break;
            }
        }
    }
    let raw = url?;
    Some(crate::extractor::innertube_api::InnerTubeMusicApi::high_resolution_artwork(&raw))
}

fn browse_ids_in_map(map: &Map<String, Value>) -> Vec<String> {
    fn walk(node: &Value, ids: &mut Vec<String>) {
        match node {
            Value::Object(obj) => {
                if let Some(id) = obj.get("browseId").and_then(Value::as_str) {
                    if !id.is_empty() {
                        ids.push(id.to_string());
                    }
                }
                for child in obj.values() {
                    walk(child, ids);
                }
            }
            Value::Array(arr) => {
                for child in arr {
                    walk(child, ids);
                }
            }
            _ => {}
        }
    }

    let mut ids = Vec::new();
    if let Some(id) = map.get("browseId").and_then(Value::as_str) {
        if !id.is_empty() {
            ids.push(id.to_string());
        }
    }
    for child in map.values() {
        walk(child, &mut ids);
    }
    ids
}

fn find_string_in_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    if let Some(direct) = map.get(key).and_then(Value::as_str) {
        if !direct.is_empty() {
            return Some(direct.to_string());
        }
    }
    for child in map.values() {
        if let Some(found) = find_string(child, key) {
            return Some(found);
        }
    }
    None
}

fn find_string(element: &Value, key: &str) -> Option<String> {
    match element {
        Value::Object(obj) => find_string_in_map(obj, key),
        Value::Array(arr) => {
            for child in arr {
                if let Some(found) = find_string(child, key) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn collect<'a>(element: &'a Value, key: &str, out: &mut Vec<&'a Map<String, Value>>) {
    match element {
        Value::Object(map) => {
            for (name, child) in map {
                if name == key {
                    if let Value::Object(child_map) = child {
                        out.push(child_map);
                    }
                }
                collect(child, key, out);
            }
        }
        Value::Array(arr) => {
            for child in arr {
                collect(child, key, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_home_show_and_episodes() {
        let payload = json!({
            "contents": {
                "musicCarouselShelfRenderer": {
                    "header": {
                        "musicCarouselShelfBasicHeaderRenderer": {
                            "title": { "runs": [{ "text": "Featured Podcasts" }] }
                        }
                    },
                    "contents": [
                        {
                            "musicTwoRowItemRenderer": {
                                "title": { "runs": [{ "text": "Waveform" }] },
                                "subtitle": { "runs": [{ "text": "Vox Media" }] },
                                "navigationEndpoint": {
                                    "browseEndpoint": { "browseId": "MPSPPL12345" }
                                },
                                "thumbnailRenderer": {
                                    "musicThumbnailRenderer": {
                                        "thumbnail": {
                                            "thumbnails": [
                                                { "url": "https://lh3.googleusercontent.com/abc=w120-h120" }
                                            ]
                                        }
                                    }
                                }
                            }
                        },
                        {
                            "musicMultiRowListItemRenderer": {
                                "title": { "runs": [{ "text": "Episode 1" }] },
                                "subtitle": { "runs": [{ "text": "Oct 12 • 45 min • Waveform" }] },
                                "onTap": {
                                    "watchEndpoint": { "videoId": "dQw4w9WgXcQ" }
                                },
                                "lengthText": { "simpleText": "45:00" }
                            }
                        }
                    ]
                }
            }
        });

        let shelves = PodcastParser::parse_home(&payload);
        assert_eq!(shelves.len(), 1);
        assert_eq!(shelves[0].title, "Featured Podcasts");
        assert_eq!(shelves[0].shows.len(), 1);
        assert_eq!(shelves[0].shows[0].show_id, "MPSPPL12345");
        assert_eq!(
            shelves[0].shows[0].artwork_url,
            "https://lh3.googleusercontent.com/abc=w512-h512-l90-rj"
        );
        assert_eq!(shelves[0].episodes.len(), 1);
        assert_eq!(shelves[0].episodes[0].video_id, "dQw4w9WgXcQ");
        assert_eq!(shelves[0].episodes[0].duration_ms, 45 * 60 * 1000);
        assert_eq!(shelves[0].episodes[0].published_label, "Oct 12");
        assert_eq!(shelves[0].episodes[0].show_title, "Waveform");
    }

    #[test]
    fn parse_clock_variants() {
        assert_eq!(parse_clock(Some("1:02:03")), Some(3723_000));
        assert_eq!(parse_clock(Some("12:30")), Some(750_000));
        assert_eq!(parse_clock(Some("1 hr 15 min")), Some(4500_000));
        assert_eq!(parse_clock(Some("22 mins")), Some(1320_000));
        assert_eq!(parse_clock(Some("invalid")), None);
    }
}
