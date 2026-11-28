use std::collections::HashSet;
use std::sync::LazyLock;

use futures_util::future::join_all;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use url::Url;

use super::{
    canvas_track_title, confirmed_track_match, normalize_confirmed_metadata, CANVAS_USER_AGENT,
};

static PAGE_DATA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<script\b[^>]*\bid="serialized-server-data"[^>]*>(.*?)</script>"#).unwrap()
});

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchItem {
    artist_id: Option<u64>,
    artist_name: Option<String>,
    track_name: Option<String>,
}

#[derive(Deserialize)]
struct SearchResponse {
    results: Vec<SearchItem>,
}

async fn search(client: &reqwest::Client, term: &str, entity: &str) -> Option<Vec<SearchItem>> {
    let response = client
        .get("https://itunes.apple.com/search")
        .header("User-Agent", CANVAS_USER_AGENT)
        .query(&[
            ("term", term),
            ("entity", entity),
            ("country", "US"),
            ("limit", "20"),
        ])
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<SearchResponse>()
        .await
        .ok()?;
    Some(response.results)
}

fn matching_ids(items: &[SearchItem], name: &str, title: Option<&str>) -> HashSet<u64> {
    items
        .iter()
        .filter_map(|item| {
            if normalize_confirmed_metadata(item.artist_name.as_deref()?) != name {
                return None;
            }
            if let Some(title) = title {
                if !confirmed_track_match(title, item.track_name.as_deref()?) {
                    return None;
                }
            }
            item.artist_id
        })
        .collect()
}

async fn resolve_artist_id(client: &reqwest::Client, artist: &str, titles: &[String]) -> Option<u64> {
    let name = normalize_confirmed_metadata(artist);
    if name.is_empty() {
        return None;
    }
    let artists = search(client, artist, "musicArtist").await?;
    let candidates = matching_ids(&artists, &name, None);
    if candidates.len() <= 1 {
        return candidates.into_iter().next();
    }

    // A shared name needs a matching catalog recording to identify the artist.
    let name = name.as_str();
    let tracks = titles.iter().take(4).map(|title| async move {
        let title = canvas_track_title(title);
        let term = format!("{artist} {title}");
        let songs = search(client, &term, "song").await?;
        Some(matching_ids(&songs, name, Some(&title)))
    });
    let confirmed: HashSet<u64> = join_all(tracks)
        .await
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    if confirmed.len() != 1 {
        return None;
    }
    confirmed.into_iter().find(|id| candidates.contains(id))
}

fn banner_from_page(html: &str, artist_id: u64) -> Option<Url> {
    let json: Value = serde_json::from_str(PAGE_DATA.captures(html)?.get(1)?.as_str()).ok()?;
    let hero = json.pointer("/data/0/data/sections/0/items/0")?;
    if hero.get("id")?.as_str()?.parse::<u64>().ok()? != artist_id {
        return None;
    }

    let assets = hero.pointer("/wideVideoArtwork/dictionary")?;
    ["motionArtistWide16x9", "motionArtistFullscreen16x9"]
        .into_iter()
        .find_map(|key| {
            let url = Url::parse(assets.get(key)?.get("video")?.as_str()?).ok()?;
            (url.scheme() == "https"
                && url.host_str() == Some("mvod.itunes.apple.com")
                && url.path().ends_with(".m3u8"))
            .then_some(url)
        })
}

pub(super) async fn lookup_banner(
    client: &reqwest::Client,
    artist: &str,
    titles: &[String],
) -> Option<Url> {
    let artist_id = resolve_artist_id(client, artist.trim(), titles).await?;
    // The public artist page carries its motion artwork alongside the hero data.
    let html = client
        .get(format!("https://music.apple.com/us/artist/{artist_id}"))
        .header("User-Agent", CANVAS_USER_AGENT)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    banner_from_page(&html, artist_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_artist_name_can_be_checked_against_a_recording() {
        let items = serde_json::from_value::<Vec<SearchItem>>(serde_json::json!([
            { "artistId": 1, "artistName": "Future", "trackName": "Low Life (feat. The Weeknd)" },
            { "artistId": 2, "artistName": "Future", "trackName": "Another Song" },
            { "artistId": 3, "artistName": "Odd Future", "trackName": "Low Life" }
        ]))
        .unwrap();
        assert_eq!(matching_ids(&items, "future", None), HashSet::from([1, 2]));
        assert_eq!(
            matching_ids(&items, "future", Some("Low Life")),
            HashSet::from([1])
        );
    }

    #[test]
    fn selects_wide_motion_only_from_the_matching_artist_hero() {
        let html = r#"<script type="application/json" id="serialized-server-data">
          {"data":[{"data":{"sections":[{"items":[{"id":"42","wideVideoArtwork":{"dictionary":{
            "motionArtistFullscreen16x9":{"video":"https://mvod.itunes.apple.com/full.m3u8"},
            "motionArtistWide16x9":{"video":"https://mvod.itunes.apple.com/wide.m3u8"}
          }}}]}]}}]}
        </script>"#;
        assert_eq!(banner_from_page(html, 42).unwrap().path(), "/wide.m3u8");
        assert!(banner_from_page(html, 41).is_none());
        assert!(banner_from_page(&html.replace("mvod.itunes.apple.com", "example.org"), 42).is_none());
    }
}
