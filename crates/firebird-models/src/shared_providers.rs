//! Shared provider contract marker module.
//!
//! In Dart (`features/search/shared_providers.dart`), this file only defines
//! Riverpod DI providers (`databaseProvider` and `prefsApiKeyProvider`) to break
//! import cycles. In the Tauri Rust backend, `AppDatabase` and `Prefs` are
//! provided via Tauri managed state (`tauri::State`), so no Riverpod bindings
//! are needed here.

use serde::{Deserialize, Serialize};

/// Pure-Rust representation of the BYOK Last.fm API key contract exposed by
/// `prefsApiKeyProvider`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedApiKeyConfig {
    #[serde(default)]
    pub last_fm_api_key: String,
}
