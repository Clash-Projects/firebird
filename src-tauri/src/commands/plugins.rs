use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::commands::state::AppState;

const REGISTRY_PRIMARY_URL: &str =
    "https://raw.githubusercontent.com/Clash-Projects/lastwave-plugins/main/registry.json";
const REGISTRY_FALLBACK_URL: &str =
    "https://cdn.jsdelivr.net/gh/Clash-Projects/lastwave-plugins@main/registry.json";

const VALID_TYPES: [&str; 4] = ["palette", "theme", "snippet", "widget"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifestDisk {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default)]
    pub min_app_version: Option<String>,
    pub author: String,
    pub description: String,
    pub entry: String,
    #[serde(default)]
    pub swatch: Option<Vec<String>>,
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginDisk {
    pub manifest: PluginManifestDisk,
    pub enabled: bool,
    pub content: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginsRuntimeState {
    #[serde(default)]
    disabled_all: bool,
    #[serde(default)]
    enabled: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginsStateResponse {
    pub safe_mode: bool,
    pub disabled_all: bool,
    pub plugins_dir: String,
    pub plugins: Vec<InstalledPluginDisk>,
}

fn plugins_root(app_support_dir: &Path) -> PathBuf {
    app_support_dir.join("plugins")
}

fn state_file_path(root: &Path) -> PathBuf {
    root.join("state.json")
}

fn load_runtime_state(root: &Path) -> PluginsRuntimeState {
    let path = state_file_path(root);
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_runtime_state(root: &Path, state: &PluginsRuntimeState) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(state_file_path(root), json).map_err(|e| e.to_string())
}

fn is_safe_segment(s: &str) -> bool {
    !s.is_empty()
        && !s.contains("..")
        && !s.contains('/')
        && !s.contains('\\')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn validate_manifest(m: &PluginManifestDisk) -> Result<(), String> {
    if !is_safe_segment(&m.id) {
        return Err(format!("Invalid plugin id '{}'", m.id));
    }
    if !VALID_TYPES.contains(&m.kind.as_str()) {
        return Err(format!("Unsupported plugin type '{}'", m.kind));
    }
    if m.api_version != 1 {
        return Err(format!(
            "Incompatible apiVersion {} (expected 1)",
            m.api_version
        ));
    }
    if !is_safe_segment(&m.entry) {
        return Err(format!("Invalid entry filename '{}'", m.entry));
    }
    Ok(())
}

pub fn load_plugins_from_dir(root: &Path, safe_mode: bool) -> PluginsStateResponse {
    let runtime = load_runtime_state(root);
    let mut plugins = Vec::new();

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let manifest_path = dir.join("manifest.json");
            let Ok(raw_manifest) = fs::read_to_string(&manifest_path) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<PluginManifestDisk>(&raw_manifest) else {
                continue;
            };
            if validate_manifest(&manifest).is_err() {
                continue;
            }
            let entry_path = dir.join(&manifest.entry);
            let content = fs::read_to_string(&entry_path).unwrap_or_default();
            let enabled = runtime.enabled.get(&manifest.id).copied().unwrap_or(false);
            plugins.push(InstalledPluginDisk {
                manifest,
                enabled,
                content,
            });
        }
    }

    plugins.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));

    PluginsStateResponse {
        safe_mode,
        disabled_all: runtime.disabled_all,
        plugins_dir: root.to_string_lossy().into_owned(),
        plugins,
    }
}

pub fn install_plugin_to_dir(
    root: &Path,
    manifest: PluginManifestDisk,
    content: &str,
) -> Result<InstalledPluginDisk, String> {
    validate_manifest(&manifest)?;
    let plugin_dir = root.join(&manifest.id);
    fs::create_dir_all(&plugin_dir).map_err(|e| e.to_string())?;

    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    fs::write(plugin_dir.join("manifest.json"), manifest_json).map_err(|e| e.to_string())?;
    fs::write(plugin_dir.join(&manifest.entry), content).map_err(|e| e.to_string())?;

    let mut runtime = load_runtime_state(root);
    runtime.enabled.insert(manifest.id.clone(), true);
    save_runtime_state(root, &runtime)?;

    Ok(InstalledPluginDisk {
        manifest,
        enabled: true,
        content: content.to_string(),
    })
}

#[tauri::command]
pub fn plugins_is_safe_mode() -> bool {
    std::env::args().any(|arg| arg == "--safe-mode")
}

#[tauri::command]
pub fn plugins_state(state: State<'_, AppState>) -> PluginsStateResponse {
    let root = plugins_root(&state.app_support_dir);
    load_plugins_from_dir(&root, plugins_is_safe_mode())
}

#[tauri::command]
pub fn plugins_install(
    state: State<'_, AppState>,
    manifest: PluginManifestDisk,
    content: String,
) -> Result<InstalledPluginDisk, String> {
    let root = plugins_root(&state.app_support_dir);
    install_plugin_to_dir(&root, manifest, &content)
}

#[tauri::command]
pub fn plugins_set_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    if !is_safe_segment(&id) {
        return Err("Invalid plugin id".to_string());
    }
    let root = plugins_root(&state.app_support_dir);
    let mut runtime = load_runtime_state(&root);
    runtime.enabled.insert(id, enabled);
    save_runtime_state(&root, &runtime)
}

#[tauri::command]
pub fn plugins_set_all_disabled(state: State<'_, AppState>, disabled: bool) -> Result<(), String> {
    let root = plugins_root(&state.app_support_dir);
    let mut runtime = load_runtime_state(&root);
    runtime.disabled_all = disabled;
    save_runtime_state(&root, &runtime)
}

#[tauri::command]
pub fn plugins_uninstall(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if !is_safe_segment(&id) {
        return Err("Invalid plugin id".to_string());
    }
    let root = plugins_root(&state.app_support_dir);
    let plugin_dir = root.join(&id);
    if plugin_dir.exists() {
        fs::remove_dir_all(&plugin_dir).map_err(|e| e.to_string())?;
    }
    let mut runtime = load_runtime_state(&root);
    runtime.enabled.remove(&id);
    save_runtime_state(&root, &runtime)
}

#[tauri::command]
pub async fn plugins_fetch_registry(state: State<'_, AppState>) -> Result<Value, String> {
    for url in [REGISTRY_PRIMARY_URL, REGISTRY_FALLBACK_URL] {
        let Ok(resp) = state
            .http
            .get(url)
            .timeout(Duration::from_secs(12))
            .send()
            .await
        else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        if let Ok(json) = resp.json::<Value>().await {
            return Ok(json);
        }
    }
    Err("Could not fetch plugin registry from GitHub or jsDelivr CDN".to_string())
}

#[tauri::command]
pub async fn plugins_fetch_remote_text(
    state: State<'_, AppState>,
    url: String,
) -> Result<String, String> {
    if !url.starts_with("https://") {
        return Err("Only HTTPS plugin URLs are allowed".to_string());
    }
    let resp = state
        .http
        .get(&url)
        .timeout(Duration::from_secs(12))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} fetching {}", resp.status(), url));
    }
    resp.text().await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest(kind: &str, api_version: u32) -> PluginManifestDisk {
        PluginManifestDisk {
            id: "test-mod".to_string(),
            name: "Test Mod".to_string(),
            kind: kind.to_string(),
            version: "1.0.0".to_string(),
            api_version,
            min_app_version: Some("3.0.0".to_string()),
            author: "tester".to_string(),
            description: "Test plugin".to_string(),
            entry: "snippet.css".to_string(),
            swatch: None,
            source_url: None,
        }
    }

    #[test]
    fn rejects_keymaps_and_incompatible_api_version() {
        assert!(validate_manifest(&sample_manifest("keymap", 1)).is_err());
        assert!(validate_manifest(&sample_manifest("snippet", 2)).is_err());
        assert!(validate_manifest(&sample_manifest("snippet", 1)).is_ok());
    }

    #[test]
    fn install_and_load_roundtrip_on_disk() {
        let dir = std::env::temp_dir().join(format!("lw_plugins_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let installed = install_plugin_to_dir(
            &dir,
            sample_manifest("snippet", 1),
            "[data-lw='track-row'] { height: 42px; }",
        )
        .expect("install should succeed");
        assert!(installed.enabled);

        let loaded = load_plugins_from_dir(&dir, false);
        assert_eq!(loaded.plugins.len(), 1);
        assert_eq!(loaded.plugins[0].manifest.id, "test-mod");
        assert!(loaded.plugins[0].content.contains("42px"));

        let _ = fs::remove_dir_all(&dir);
    }
}
