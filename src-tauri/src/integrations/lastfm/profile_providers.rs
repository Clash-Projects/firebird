//! Profile stats & display-name / avatar resolvers.
//!
//! Ported from `lib/features/lastfm/profile_providers.dart`.

use std::sync::Arc;

use crate::integrations::lastfm::home_repository::HomeRepository;
use crate::integrations::lastfm::lastfm_api::LastFmApiError;
use crate::integrations::scrobblers::scrobbler_session::ScrobblerSession;
use crate::models::lastfm_home_models::HomeStats;
use crate::storage::prefs::Prefs;

pub const CACHE_GROUP_LASTFM: &str = "lastfm";
pub const LASTFM_CACHE_MAX_AGE_SECS: u64 = 15 * 60;

pub fn last_fm_cache_key(kind: &str, scope: &str, viewing: Option<&str>) -> String {
    match viewing.filter(|v| !v.is_empty()) {
        Some(v) => format!("lastfm:{kind}:{scope}:view:{v}"),
        None => format!("lastfm:{kind}:{scope}"),
    }
}

/// Lightweight YouTube account summary for avatar/name fallback without
/// coupling `integrations::lastfm` to `extractor::yt_library_providers`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct YtAccountProfile {
    pub name: String,
    pub photo_url: String,
}

/// Fetches scrobbler profile stats for a user (or empty when guest/unconfigured).
pub async fn fetch_user_profile_stats(
    home: &HomeRepository,
    session: &ScrobblerSession,
    viewing: Option<&str>,
) -> Result<HomeStats, LastFmApiError> {
    let scope = session.cache_scope();
    if scope.is_empty() && viewing.is_none() {
        return Ok(HomeStats::default());
    }
    home.fetch_stats(viewing).await
}

/// Reactive user avatar URL resolver.
///
/// Priority:
/// 1. Active scrobbler profile avatar URL (if connected and has avatar)
/// 2. YouTube Music channel photo URL (if connected to YouTube and has photo)
/// 3. Empty string (UI falls back to user initial)
pub fn resolve_user_avatar_url(
    session: &ScrobblerSession,
    stats: Option<&HomeStats>,
    yt_account: Option<&YtAccountProfile>,
) -> String {
    if session.connected() {
        if let Some(s) = stats {
            if !s.avatar_url.is_empty() {
                return s.avatar_url.clone();
            }
        }
    }
    if let Some(yt) = yt_account {
        if !yt.photo_url.is_empty() {
            return yt.photo_url.clone();
        }
    }
    String::new()
}

/// Title-bar avatar source: local picture first, then connected YouTube
/// channel photo. Never tied to Last.fm or other scrobblers.
pub fn resolve_title_bar_avatar_url(
    prefs: &Prefs,
    yt_account: Option<&YtAccountProfile>,
) -> String {
    let local_path = prefs.profile_picture_path().trim().to_string();
    if !local_path.is_empty() {
        return local_path;
    }
    if let Some(yt) = yt_account {
        if !yt.photo_url.is_empty() {
            return yt.photo_url.clone();
        }
    }
    String::new()
}

/// Title-bar display name: local display name first, then connected YouTube
/// channel name. Never tied to Last.fm or other scrobblers.
pub fn resolve_title_bar_display_name(
    prefs: &Arc<Prefs>,
    yt_account: Option<&YtAccountProfile>,
) -> String {
    let custom = prefs.display_name().trim().to_string();
    if !custom.is_empty() {
        return custom;
    }
    if let Some(yt) = yt_account {
        if !yt.name.is_empty() {
            return yt.name.clone();
        }
    }
    String::new()
}

/// Greeting name on Home: custom display name first, then YouTube channel
/// name, then active scrobbler username.
pub fn resolve_home_display_name(
    prefs: &Arc<Prefs>,
    yt_account: Option<&YtAccountProfile>,
    session: &ScrobblerSession,
) -> String {
    let title_bar_name = resolve_title_bar_display_name(prefs, yt_account);
    if !title_bar_name.is_empty() {
        return title_bar_name;
    }
    if session.connected() {
        session.username.clone()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::scrobblers::scrobbler_session::ScrobblerKind;

    #[test]
    fn resolves_avatar_and_display_name_priorities() {
        let prefs = Arc::new(Prefs::in_memory());
        let yt = YtAccountProfile {
            name: "YT Name".to_string(),
            photo_url: "https://yt.example/photo.jpg".to_string(),
        };
        let session = ScrobblerSession {
            kind: Some(ScrobblerKind::Lastfm),
            username: "lfm_user".to_string(),
            profile_url: None,
        };
        let stats = HomeStats {
            avatar_url: "https://lfm.example/avatar.jpg".to_string(),
            ..Default::default()
        };

        assert_eq!(
            resolve_user_avatar_url(&session, Some(&stats), Some(&yt)),
            "https://lfm.example/avatar.jpg"
        );
        assert_eq!(
            resolve_title_bar_avatar_url(&prefs, Some(&yt)),
            "https://yt.example/photo.jpg"
        );
        assert_eq!(
            resolve_home_display_name(&prefs, Some(&yt), &session),
            "YT Name"
        );
        assert_eq!(
            resolve_home_display_name(&prefs, None, &session),
            "lfm_user"
        );
    }
}
