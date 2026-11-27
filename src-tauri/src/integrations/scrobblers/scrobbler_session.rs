use serde::{Deserialize, Serialize};
use url::Url;

use crate::storage::prefs::Prefs;

/// Supported scrobbler backends for read/write features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScrobblerKind {
    Lastfm,
    Listenbrainz,
    Librefm,
}

impl ScrobblerKind {
    pub const fn id(&self) -> &'static str {
        match self {
            Self::Lastfm => "lastfm",
            Self::Listenbrainz => "listenbrainz",
            Self::Librefm => "librefm",
        }
    }

    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Lastfm => "Last.fm",
            Self::Listenbrainz => "ListenBrainz",
            Self::Librefm => "Libre.fm",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "lastfm" => Some(Self::Lastfm),
            "listenbrainz" => Some(Self::Listenbrainz),
            "librefm" => Some(Self::Librefm),
            _ => None,
        }
    }
}

/// Active primary scrobbler session (or [`ScrobblerSession::none`] in guest mode).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrobblerSession {
    pub kind: Option<ScrobblerKind>,
    pub username: String,
    pub profile_url: Option<String>,
}

impl ScrobblerSession {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn connected(&self) -> bool {
        self.kind.is_some() && !self.username.is_empty()
    }

    pub fn display_name(&self) -> &'static str {
        self.kind.map(|k| k.display_name()).unwrap_or("")
    }

    /// Cache key prefix scoped by backend + username so switching primary
    /// scrobblers never collides in `snapshot_cache`.
    pub fn cache_scope(&self) -> String {
        if !self.connected() {
            return String::new();
        }
        match self.kind {
            Some(ScrobblerKind::Lastfm) => self.username.clone(),
            Some(other) => format!("{}:{}", other.id(), self.username),
            None => String::new(),
        }
    }
}

/// Connection state snapshot for all supported scrobbler backends.
#[derive(Debug, Clone, Default)]
pub struct ScrobblerBackendsState {
    pub lastfm_signed_in: bool,
    pub lastfm_username: String,
    pub listenbrainz_connected: bool,
    pub listenbrainz_username: String,
    pub listenbrainz_profile_url: Option<String>,
    pub librefm_connected: bool,
    pub librefm_username: String,
    pub librefm_profile_url: Option<String>,
}

/// All currently connected scrobbler backends in display order.
pub fn connected_scrobblers(state: &ScrobblerBackendsState) -> Vec<ScrobblerKind> {
    let mut out = Vec::new();
    if state.lastfm_signed_in && !state.lastfm_username.is_empty() {
        out.push(ScrobblerKind::Lastfm);
    }
    if state.listenbrainz_connected && !state.listenbrainz_username.is_empty() {
        out.push(ScrobblerKind::Listenbrainz);
    }
    if state.librefm_connected && !state.librefm_username.is_empty() {
        out.push(ScrobblerKind::Librefm);
    }
    out
}

/// Resolves the active primary scrobbler used for reading History, Stats,
/// Top Artists/Albums, and Home personalization.
///
/// Honors `Prefs::primary_scrobbler` if that backend is connected; otherwise
/// automatically selects the first connected scrobbler, or
/// [`ScrobblerSession::none`] in guest/local-only mode.
pub fn resolve_active_scrobbler_session(
    state: &ScrobblerBackendsState,
    prefs: &Prefs,
) -> ScrobblerSession {
    let connected = connected_scrobblers(state);
    if connected.is_empty() {
        return ScrobblerSession::none();
    }
    let pref_id = prefs.primary_scrobbler();
    let preferred = ScrobblerKind::from_id(&pref_id);
    let chosen = match preferred {
        Some(p) if connected.contains(&p) => p,
        _ => connected[0],
    };

    match chosen {
        ScrobblerKind::Lastfm => ScrobblerSession {
            kind: Some(ScrobblerKind::Lastfm),
            username: state.lastfm_username.clone(),
            profile_url: Some(lastfm_profile_url(&state.lastfm_username)),
        },
        ScrobblerKind::Listenbrainz => ScrobblerSession {
            kind: Some(ScrobblerKind::Listenbrainz),
            username: state.listenbrainz_username.clone(),
            profile_url: state.listenbrainz_profile_url.clone(),
        },
        ScrobblerKind::Librefm => ScrobblerSession {
            kind: Some(ScrobblerKind::Librefm),
            username: state.librefm_username.clone(),
            profile_url: state.librefm_profile_url.clone(),
        },
    }
}

fn lastfm_profile_url(username: &str) -> String {
    let mut url = Url::parse("https://www.last.fm/user/").expect("valid base url");
    if let Ok(mut segs) = url.path_segments_mut() {
        segs.pop_if_empty();
        segs.push(username);
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_preferred_and_fallback_sessions() {
        let prefs = Prefs::in_memory();
        let state = ScrobblerBackendsState {
            lastfm_signed_in: true,
            lastfm_username: "alice".to_string(),
            listenbrainz_connected: true,
            listenbrainz_username: "alice_lb".to_string(),
            listenbrainz_profile_url: Some("https://listenbrainz.org/user/alice_lb/".to_string()),
            librefm_connected: false,
            librefm_username: String::new(),
            librefm_profile_url: None,
        };

        let s1 = resolve_active_scrobbler_session(&state, &prefs);
        assert_eq!(s1.kind, Some(ScrobblerKind::Lastfm));
        assert_eq!(s1.cache_scope(), "alice");

        prefs.set_primary_scrobbler("listenbrainz");
        let s2 = resolve_active_scrobbler_session(&state, &prefs);
        assert_eq!(s2.kind, Some(ScrobblerKind::Listenbrainz));
        assert_eq!(s2.cache_scope(), "listenbrainz:alice_lb");

        prefs.set_primary_scrobbler("librefm");
        let s3 = resolve_active_scrobbler_session(&state, &prefs);
        assert_eq!(s3.kind, Some(ScrobblerKind::Lastfm));
    }
}
