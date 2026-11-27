use std::collections::BTreeMap;

use super::secrets;

/// Environment configuration.
///
/// Secrets are baked in at build time as XOR-obfuscated byte arrays
/// (`secrets.rs`), mirroring LastWave-native's `obfuscateSecret()` /
/// `decodeSecretBytes` scheme with the same mask. They are decoded only in
/// memory at runtime — every user of the app gets addon/lyrics access with no
/// keys of their own, and no plaintext secret ships in the binary.
///
/// Last.fm is BYOK: the user enters their own API key + secret
/// (stored in Prefs) — no bundled Last.fm keys exist.
///
/// Secrets are never logged (see [`AppEnv::configured_flags`], booleans only).
pub struct AppEnv;

impl AppEnv {
    /// Same mask as LastWave-native `SECRET_MASK`.
    pub const MASK: [u8; 8] = [0x5A, 0x3F, 0x7E, 0x1B, 0x92, 0x4C, 0xA1, 0x6D];

    /// Decodes an XOR-obfuscated UTF-8 byte slice using [`Self::MASK`].
    pub fn decode(data: &[u8]) -> String {
        if data.is_empty() {
            return String::new();
        }
        let bytes: Vec<u8> = data
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ Self::MASK[i % Self::MASK.len()])
            .collect();
        match String::from_utf8(bytes) {
            Ok(s) => s.trim().to_string(),
            Err(_) => String::new(),
        }
    }

    /// Addon one-way-lock secret (HMAC-SHA256 request proofs). Baked in
    /// like the other keys: never logged, never sent except as a keyed
    /// proof. Empty = addon calls fail closed server-side (404).
    pub fn addon_client_secret() -> String {
        Self::decode(secrets::K_ADDON_CLIENT_SECRET)
    }

    /// About-screen catalog list. Never includes URLs or keys.
    /// (Settings → Sources) are the only lossless catalog now.
    pub fn lossless_catalog_label() -> &'static str {
        "Addons"
    }

    /// Non-sensitive diagnostics only — never includes secret values.
    pub fn configured_flags() -> BTreeMap<String, bool> {
        let mut map = BTreeMap::new();
        map.insert(
            "addonKey".to_string(),
            !Self::addon_client_secret().is_empty(),
        );
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_empty_returns_empty() {
        assert_eq!(AppEnv::decode(&[]), "");
    }

    #[test]
    fn decode_roundtrip_trims_whitespace() {
        let plain = b"  test-secret-123 \n";
        let obfuscated: Vec<u8> = plain
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ AppEnv::MASK[i % AppEnv::MASK.len()])
            .collect();
        assert_eq!(AppEnv::decode(&obfuscated), "test-secret-123");
    }

    #[test]
    fn decode_invalid_utf8_returns_empty() {
        let invalid = [0xFF ^ AppEnv::MASK[0], 0xFE ^ AppEnv::MASK[1]];
        assert_eq!(AppEnv::decode(&invalid), "");
    }

    #[test]
    fn addon_secret_and_flags_consistent() {
        let secret = AppEnv::addon_client_secret();
        let flags = AppEnv::configured_flags();
        assert_eq!(flags.get("addonKey"), Some(&!secret.is_empty()));
        assert_eq!(AppEnv::lossless_catalog_label(), "Addons");
    }
}
