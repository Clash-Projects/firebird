use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

/// OS keyring service identifier for LastWave Desktop.
pub const KEYRING_SERVICE: &str = "com.lastwave.lastwave_desktop";

pub const SESSION_KEY: &str = "lastwave.session_key";
pub const YT_COOKIES: &str = "lastwave.yt_cookies";
pub const YT_PROFILES: &str = "lastwave.yt_profiles";
pub const YT_ACTIVE: &str = "lastwave.yt_active";
pub const YT_SESSION_BUNDLE: &str = "yt_session_bundle";
pub const SPOTIFY_SP_DC: &str = "lastwave.spotify_sp_dc";

const KNOWN_KEYS: [&str; 6] = [
    SESSION_KEY,
    YT_COOKIES,
    YT_PROFILES,
    YT_ACTIVE,
    YT_SESSION_BUNDLE,
    SPOTIFY_SP_DC,
];

/// Secure credential storage (session keys, cookies).
///
/// Uses OS keychain/credential-manager via `keyring`, reads legacy
/// `flutter_secure_storage.dat` DPAPI vaults from official LastWave Desktop
/// installs on Windows when present, and falls back to in-memory storage.
pub struct SecureStore {
    service: String,
    use_keyring: bool,
    memory_fallback: Mutex<HashMap<String, String>>,
    legacy_vault: Mutex<HashMap<String, String>>,
    tracked_keys: Mutex<HashSet<String>>,
}

impl Default for SecureStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "windows")]
fn load_flutter_secure_storage_dat(app_support_dir: &Path) -> HashMap<String, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    let vault_path = app_support_dir.join("flutter_secure_storage.dat");
    let Ok(mut bytes) = std::fs::read(vault_path) else {
        return HashMap::new();
    };
    if bytes.is_empty() {
        return HashMap::new();
    }

    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_mut_ptr(),
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptUnprotectData(
            &in_blob,
            None,
            None,
            None,
            None,
            0,
            &mut out_blob,
        )
        .is_ok()
    };
    if !ok || out_blob.pbData.is_null() {
        return HashMap::new();
    }

    let slice = unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) };
    let text = String::from_utf8_lossy(slice).into_owned();
    unsafe {
        let _ = LocalFree(HLOCAL(out_blob.pbData.cast()));
    }

    serde_json::from_str::<HashMap<String, String>>(&text).unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
fn load_flutter_secure_storage_dat(_app_support_dir: &Path) -> HashMap<String, String> {
    HashMap::new()
}

impl SecureStore {
    /// Creates a `SecureStore` backed by the OS keyring with an in-memory fallback.
    pub fn new() -> Self {
        Self::with_service(KEYRING_SERVICE)
    }

    /// Creates a `SecureStore` backed by OS keyring plus legacy `flutter_secure_storage.dat`
    /// credentials located inside `app_support_dir` (when present on Windows).
    pub fn from_app_support_dir(app_support_dir: &Path) -> Self {
        let legacy = load_flutter_secure_storage_dat(app_support_dir);
        let mut tracked: HashSet<String> = KNOWN_KEYS.iter().map(|k| (*k).to_string()).collect();
        for k in legacy.keys() {
            tracked.insert(k.clone());
        }
        Self {
            service: KEYRING_SERVICE.to_string(),
            use_keyring: true,
            memory_fallback: Mutex::new(HashMap::new()),
            legacy_vault: Mutex::new(legacy),
            tracked_keys: Mutex::new(tracked),
        }
    }

    /// Creates a `SecureStore` using a custom keyring service name.
    pub fn with_service(service: impl Into<String>) -> Self {
        let tracked = KNOWN_KEYS.iter().map(|k| (*k).to_string()).collect();
        Self {
            service: service.into(),
            use_keyring: true,
            memory_fallback: Mutex::new(HashMap::new()),
            legacy_vault: Mutex::new(HashMap::new()),
            tracked_keys: Mutex::new(tracked),
        }
    }

    /// Creates an in-memory-only `SecureStore` (useful for tests or headless environments).
    pub fn in_memory() -> Self {
        let tracked = KNOWN_KEYS.iter().map(|k| (*k).to_string()).collect();
        Self {
            service: KEYRING_SERVICE.to_string(),
            use_keyring: false,
            memory_fallback: Mutex::new(HashMap::new()),
            legacy_vault: Mutex::new(HashMap::new()),
            tracked_keys: Mutex::new(tracked),
        }
    }

    fn track_key(&self, key: &str) {
        if let Ok(mut keys) = self.tracked_keys.lock() {
            keys.insert(key.to_string());
        }
    }

    fn read_internal(&self, key: &str) -> Option<String> {
        if let Some(mem) = self
            .memory_fallback
            .lock()
            .ok()
            .and_then(|map| map.get(key).cloned())
        {
            return Some(mem);
        }
        if self.use_keyring {
            if let Ok(entry) = keyring::Entry::new(&self.service, key) {
                if let Ok(v) = entry.get_password() {
                    return Some(v);
                }
            }
        }
        self.legacy_vault
            .lock()
            .ok()
            .and_then(|map| map.get(key).cloned())
    }

    fn write_internal(&self, key: &str, value: Option<&str>) {
        self.track_key(key);
        match value {
            None => {
                if let Ok(mut map) = self.memory_fallback.lock() {
                    map.remove(key);
                }
                if let Ok(mut map) = self.legacy_vault.lock() {
                    map.remove(key);
                }
                if self.use_keyring {
                    if let Ok(entry) = keyring::Entry::new(&self.service, key) {
                        let _ = entry.delete_credential();
                    }
                }
            }
            Some(v) => {
                if let Ok(mut map) = self.memory_fallback.lock() {
                    map.insert(key.to_string(), v.to_string());
                }
                if self.use_keyring {
                    if let Ok(entry) = keyring::Entry::new(&self.service, key) {
                        let _ = entry.set_password(v);
                    }
                }
            }
        }
    }

    pub fn read(&self, key: &str) -> Option<String> {
        self.read_internal(key)
    }

    pub fn write(&self, key: &str, value: Option<&str>) {
        self.write_internal(key, value);
    }

    pub fn delete(&self, key: &str) {
        self.write_internal(key, None);
    }

    pub fn read_session_key(&self) -> Option<String> {
        self.read_internal(SESSION_KEY)
    }

    pub fn write_session_key(&self, v: Option<&str>) {
        self.write_internal(SESSION_KEY, v);
    }

    pub fn read_yt_cookies(&self) -> Option<String> {
        self.read_internal(YT_COOKIES)
    }

    pub fn write_yt_cookies(&self, v: Option<&str>) {
        self.write_internal(YT_COOKIES, v);
    }

    /// Multi-profile roster (`YtProfile.listToJson`) + active pointer
    /// (`{"email": ..., "pageId": ...}`). The active jar itself stays in
    /// `YT_COOKIES`, so single-profile behavior is unchanged.
    pub fn read_yt_profiles(&self) -> Option<String> {
        self.read_internal(YT_PROFILES)
    }

    pub fn write_yt_profiles(&self, v: Option<&str>) {
        self.write_internal(YT_PROFILES, v);
    }

    pub fn read_yt_active(&self) -> Option<String> {
        self.read_internal(YT_ACTIVE)
    }

    pub fn write_yt_active(&self, v: Option<&str>) {
        self.write_internal(YT_ACTIVE, v);
    }

    /// Spotify `sp_dc` session cookie (WebView sign-in or pasted).
    pub fn read_spotify_sp_dc(&self) -> Option<String> {
        self.read_internal(SPOTIFY_SP_DC)
    }

    pub fn write_spotify_sp_dc(&self, v: Option<&str>) {
        self.write_internal(SPOTIFY_SP_DC, v);
    }

    pub fn clear_all(&self) {
        if let Ok(mut map) = self.memory_fallback.lock() {
            map.clear();
        }
        if let Ok(mut map) = self.legacy_vault.lock() {
            map.clear();
        }
        if self.use_keyring {
            let keys: Vec<String> = self
                .tracked_keys
                .lock()
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_else(|_| KNOWN_KEYS.iter().map(|k| (*k).to_string()).collect());
            for key in keys {
                if let Ok(entry) = keyring::Entry::new(&self.service, &key) {
                    let _ = entry.delete_credential();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_roundtrip_and_clear() {
        let store = SecureStore::in_memory();
        assert_eq!(store.read_session_key(), None);

        store.write_session_key(Some("sess-123"));
        store.write_yt_cookies(Some("SID=abc"));
        store.write_yt_profiles(Some("[{\"email\":\"a@b.com\"}]"));
        store.write_yt_active(Some("{\"email\":\"a@b.com\"}"));
        store.write_spotify_sp_dc(Some("spdc-xyz"));

        assert_eq!(store.read_session_key().as_deref(), Some("sess-123"));
        assert_eq!(store.read_yt_cookies().as_deref(), Some("SID=abc"));
        assert_eq!(
            store.read_yt_profiles().as_deref(),
            Some("[{\"email\":\"a@b.com\"}]")
        );
        assert_eq!(
            store.read_yt_active().as_deref(),
            Some("{\"email\":\"a@b.com\"}")
        );
        assert_eq!(store.read_spotify_sp_dc().as_deref(), Some("spdc-xyz"));

        store.delete(SESSION_KEY);
        assert_eq!(store.read_session_key(), None);

        store.clear_all();
        assert_eq!(store.read_yt_cookies(), None);
        assert_eq!(store.read_spotify_sp_dc(), None);
    }
}
