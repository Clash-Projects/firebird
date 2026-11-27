//! Central artist parsing and credit splitting utility.
//!
//! Ensures collaborations (e.g. `"Artist 1, Artist 2"`, `"Artist 1 & Artist 2"`,
//! `"Artist 1 feat. Artist 2"`, `"Artist 1 x Artist 2"`, `"Artist 1 / Artist 2"`)
//! are broken into individual distinct artists across statistics, UI,
//! search, and navigation, while preserving established band names like
//! `"Tyler, The Creator"`, `"Earth, Wind & Fire"`, `"Simon & Garfunkel"`, `"AC/DC"`.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

/// Lowercased band names containing delimiters that should not be split.
pub const KNOWN_BANDS_WITH_DELIMITERS: &[&str] = &[
    "tyler, the creator",
    "earth, wind & fire",
    "earth, wind and fire",
    "simon & garfunkel",
    "simon and garfunkel",
    "crosby, stills & nash",
    "crosby, stills, nash & young",
    "of monsters and men",
    "florence + the machine",
    "florence and the machine",
    "kool & the gang",
    "kool and the gang",
    "hall & oates",
    "daryl hall & john oates",
    "ac/dc",
];

fn is_known_band(lower: &str) -> bool {
    KNOWN_BANDS_WITH_DELIMITERS.contains(&lower)
}

fn feat_split_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\s+[(\[]?(?:feat\.?|ft\.?|featuring)\b").expect("valid feat_split regex")
    })
}

fn comma_the_and_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i),\s*(?:the|and)\b").expect("valid comma_the_and regex"))
}

fn primary_cut_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\s*[,;&/]\s*|\s+[xX]\s+|\s+and\s+").expect("valid primary_cut regex")
    })
}

fn delimiter_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:\s*[,;]\s*|\s+&\s+|\s+and\s+|\s+/\s+|\s+[xX]\s+|\s+[(\[]?(?:feat\b\.?|ft\b\.?|featuring\b|with\b)[)\]]?\s*)",
        )
        .expect("valid delimiter regex")
    })
}

fn outer_brackets_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[(\[]+|[)\]]+$").expect("valid outer_brackets regex"))
}

fn leading_feat_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(?:feat\.?|ft\.?|featuring|with)\s+").expect("valid leading_feat regex")
    })
}

/// First billed artist in a credit string.
///
/// `"Drake, Future & Metro Boomin"` → `"Drake"`
/// `"Tyler, The Creator"` stays intact.
pub fn primary_artist_name(artist: &str) -> String {
    let s = artist.trim();
    if s.is_empty() {
        return String::new();
    }
    let lower = s.to_lowercase();
    if is_known_band(&lower) {
        return s.to_string();
    }

    let s = feat_split_regex().split(s).next().unwrap_or(s).trim();
    if comma_the_and_regex().is_match(s)
        && !s.contains(" & ")
        && !s.contains(" feat")
        && !s.contains(" ft")
    {
        return s.to_string();
    }

    let cut = primary_cut_regex().split(s).next().unwrap_or(s).trim();
    if !cut.is_empty() {
        cut.to_string()
    } else {
        s.to_string()
    }
}

/// Individual credits from a collab string.
///
/// Splits on: `,`, `;`, `&`, `and`, `feat.`, `ft.`, `featuring`, `/`, `x`, `X`, `with`.
pub fn split_artist_credits(artist: &str) -> Vec<String> {
    let s = artist.trim();
    if s.is_empty() {
        return Vec::new();
    }
    let lower = s.to_lowercase();
    if lower == "unknown artist" || lower == "various artists" {
        return Vec::new();
    }
    if is_known_band(&lower) {
        return vec![s.to_string()];
    }
    if comma_the_and_regex().is_match(s)
        && !s.contains(" & ")
        && !s.contains(" feat")
        && !s.contains(" ft")
    {
        return vec![s.to_string()];
    }

    let mut result = Vec::new();
    let mut seen = HashSet::new();

    for raw in delimiter_regex().split(s) {
        let trimmed = raw.trim();
        let unbracketed = outer_brackets_regex().replace_all(trimmed, "");
        let unbracketed = unbracketed.trim();
        let stripped = leading_feat_regex().replace(unbracketed, "");
        let p = stripped.trim();
        if p.is_empty() {
            continue;
        }
        let p_low = p.to_lowercase();
        if p_low == "the" || p_low == "and" || p_low == "&" {
            continue;
        }
        if seen.insert(p_low) {
            result.push(p.to_string());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_artist_extracts_first_and_preserves_bands() {
        assert_eq!(primary_artist_name(""), "");
        assert_eq!(primary_artist_name("   "), "");
        assert_eq!(
            primary_artist_name("Drake, Future & Metro Boomin"),
            "Drake"
        );
        assert_eq!(
            primary_artist_name("Tyler, The Creator"),
            "Tyler, The Creator"
        );
        assert_eq!(
            primary_artist_name("Earth, Wind & Fire"),
            "Earth, Wind & Fire"
        );
        assert_eq!(primary_artist_name("AC/DC"), "AC/DC");
        assert_eq!(
            primary_artist_name("The Weeknd feat. Daft Punk"),
            "The Weeknd"
        );
        assert_eq!(primary_artist_name("Artist A (ft. Artist B)"), "Artist A");
    }

    #[test]
    fn split_credits_handles_collaborations_and_filters() {
        assert!(split_artist_credits("Unknown Artist").is_empty());
        assert!(split_artist_credits("Various Artists").is_empty());
        assert_eq!(
            split_artist_credits("Tyler, The Creator"),
            vec!["Tyler, The Creator"]
        );
        assert_eq!(
            split_artist_credits("Drake, Future & Metro Boomin"),
            vec!["Drake", "Future", "Metro Boomin"]
        );
        assert_eq!(
            split_artist_credits("Daft Punk feat. Pharrell Williams & Nile Rodgers"),
            vec!["Daft Punk", "Pharrell Williams", "Nile Rodgers"]
        );
        assert_eq!(
            split_artist_credits("Artist A x Artist B / Artist C; Artist A"),
            vec!["Artist A", "Artist B", "Artist C"]
        );
        assert_eq!(
            split_artist_credits("Calvin Harris with Dua Lipa"),
            vec!["Calvin Harris", "Dua Lipa"]
        );
    }
}
