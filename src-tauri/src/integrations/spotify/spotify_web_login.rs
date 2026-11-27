//! In-app Spotify sign-in via the system WebView (login route #1; the
//! pasted `sp_dc` cookie is route #2).
//!
//! Ported from `lib/features/spotify/spotify_web_login.dart`.
//! `sp_dc` is issued by Spotify's authentication response, so it only counts
//! once the window has left the anonymous pages (login / sign-up / reset).

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Why the last `SpotifyWebLogin` sign-in produced no cookie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SpotifySignInFailure {
    #[default]
    None,
    Unavailable,
    WindowClosed,
    Timeout,
}

/// Bare localized login entry point: with a `continue` parameter Spotify
/// sends the browser to the web player instead of the accounts status
/// page, and the cookie is never read where it is issued.
pub const SPOTIFY_LOGIN_URL: &str = "https://accounts.spotify.com/en/login";

static STATUS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https://accounts\.spotify\.com/(?:[^/]+/)?status(?:\?.*)?$").unwrap()
});

static ANONYMOUS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https://accounts\.spotify\.com/(?:[^/]+/)?(?:login|signup|sign-up|reset-password|password-reset)(?:[/?#].*)?$",
    )
    .unwrap()
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
}

pub struct SpotifyWebLogin;

impl SpotifyWebLogin {
    /// True for a page that can only be reached once the sign-in finished.
    pub fn is_post_auth(url: Option<&str>) -> bool {
        let Some(url) = url.filter(|s| !s.is_empty()) else {
            return false;
        };
        if STATUS_RE.is_match(url) {
            return true;
        }
        if !url.starts_with("https://accounts.spotify.com/") {
            return url.starts_with("https://open.spotify.com/");
        }
        !ANONYMOUS_RE.is_match(url)
    }

    /// Strips trailing NUL bytes WebView2 appends to cookie names/values.
    pub fn strip_nul(s: &str) -> String {
        s.trim_end_matches('\0').trim().to_string()
    }

    /// Extracts `sp_dc` from a WebView cookie jar snapshot.
    pub fn extract_sp_dc(cookies: &[WebCookie]) -> Option<String> {
        for c in cookies {
            let name = Self::strip_nul(&c.name);
            let domain = Self::strip_nul(&c.domain).to_lowercase();
            if name == "sp_dc" && domain.contains("spotify.com") {
                let value = Self::strip_nul(&c.value);
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_auth_rejects_login_and_accepts_status_or_player() {
        assert!(!SpotifyWebLogin::is_post_auth(None));
        assert!(!SpotifyWebLogin::is_post_auth(Some(
            "https://accounts.spotify.com/en/login"
        )));
        assert!(!SpotifyWebLogin::is_post_auth(Some(
            "https://accounts.spotify.com/en/signup?continue=abc"
        )));
        assert!(SpotifyWebLogin::is_post_auth(Some(
            "https://accounts.spotify.com/en/status"
        )));
        assert!(SpotifyWebLogin::is_post_auth(Some(
            "https://open.spotify.com/"
        )));
    }

    #[test]
    fn extract_sp_dc_strips_webview2_trailing_nuls() {
        let cookies = vec![WebCookie {
            name: "sp_dc\0".to_string(),
            value: "AQB_secret\0\0".to_string(),
            domain: ".spotify.com\0".to_string(),
        }];
        assert_eq!(
            SpotifyWebLogin::extract_sp_dc(&cookies).as_deref(),
            Some("AQB_secret")
        );
    }
}
