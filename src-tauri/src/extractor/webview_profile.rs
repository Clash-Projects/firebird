use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Default Tauri application support directory (`<data_dir>/com.lastwave.lastwave_desktop`).
pub fn default_app_support_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|base| base.join("com.lastwave.lastwave_desktop"))
}

/// Absolute WebView2 user-data folder for a window `slot` (`auth`, `botguard`, etc.)
/// under [`default_app_support_dir`], creating it if needed.
///
/// Distinct slots must never share a folder because two live WebView2 environments
/// pointed at the same user-data directory conflict.
pub fn webview_user_data_folder(slot: &str) -> Option<PathBuf> {
    let base = default_app_support_dir()?;
    webview_user_data_folder_in(&base, slot)
}

/// Absolute WebView2 user-data folder for `slot` rooted inside `app_support_dir`.
pub fn webview_user_data_folder_in(app_support_dir: &Path, slot: &str) -> Option<PathBuf> {
    let sanitized = sanitize_slot(slot);
    if sanitized.is_empty() {
        return None;
    }
    let dir = app_support_dir.join("webview2").join(sanitized);
    if fs::create_dir_all(&dir).is_err() {
        return None;
    }
    Some(dir)
}

/// Moves a slot's WebView profile aside (`<slot>_broken_<ms>`) so the next launch
/// starts clean without failing on still-locked files, and prunes older
/// `<slot>_broken*` copies best-effort.
pub fn reset_webview_profile(slot: &str) {
    if let Some(base) = default_app_support_dir() {
        reset_webview_profile_in(&base, slot);
    }
}

/// Rooted version of [`reset_webview_profile`] for explicit app-support directories.
pub fn reset_webview_profile_in(app_support_dir: &Path, slot: &str) {
    let sanitized = sanitize_slot(slot);
    if sanitized.is_empty() {
        return;
    }
    let parent = app_support_dir.join("webview2");
    let dir = parent.join(&sanitized);
    if !dir.exists() {
        return;
    }

    let broken_prefix = format!("{sanitized}_broken");
    if let Ok(entries) = fs::read_dir(&parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if path.is_dir() && name.starts_with(&broken_prefix) {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let target = parent.join(format!("{sanitized}_broken_{stamp}"));
    let _ = fs::rename(&dir, &target);
}

fn sanitize_slot(slot: &str) -> String {
    slot.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_reset_profile_moves_aside_and_prunes_old_broken() {
        let tmp = std::env::temp_dir().join(format!(
            "lastwave_wv_profile_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&tmp);

        let folder = webview_user_data_folder_in(&tmp, "botguard").expect("create folder");
        assert!(folder.exists());
        fs::write(folder.join("Cookies"), b"data").expect("write marker");

        // Pre-existing broken folder should be pruned during reset.
        let old_broken = tmp.join("webview2").join("botguard_broken_1");
        fs::create_dir_all(&old_broken).expect("create old broken");

        reset_webview_profile_in(&tmp, "botguard");
        assert!(!folder.exists());
        assert!(!old_broken.exists());

        let renamed_count = fs::read_dir(tmp.join("webview2"))
            .expect("read parent")
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("botguard_broken_"))
            })
            .count();
        assert_eq!(renamed_count, 1);

        let _ = fs::remove_dir_all(&tmp);
    }
}
