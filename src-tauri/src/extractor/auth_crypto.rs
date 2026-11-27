use std::time::{SystemTime, UNIX_EPOCH};

use md5::{Digest as Md5Digest, Md5};
use sha1::Sha1;

/// Last.fm API signing and YouTube InnerTube `SAPISIDHASH` cryptographic helpers.
pub struct AuthCrypto;

impl AuthCrypto {
    /// Strips whitespace and lowercases a Last.fm API key or secret.
    pub fn normalize_key(raw: &str) -> String {
        raw.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect()
    }

    /// Computes Last.fm `api_sig`: `md5(sorted(k + v) + secret)` as lowercase hex,
    /// excluding `format`, `callback`, `api_sig`, and empty values.
    pub fn api_sig<'a, I>(params: I, secret: &str) -> String
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut entries: Vec<(&str, &str)> = params
            .into_iter()
            .filter(|(k, v)| {
                *k != "format" && *k != "callback" && *k != "api_sig" && !v.is_empty()
            })
            .collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));

        let mut payload = String::new();
        for (k, v) in entries {
            payload.push_str(k);
            payload.push_str(v);
        }
        payload.push_str(secret);
        Self::md5_hex(&payload)
    }

    /// Computes lowercase hex MD5 of UTF-8 `input`.
    pub fn md5_hex(input: &str) -> String {
        let mut hasher = Md5::new();
        hasher.update(input.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Computes lowercase hex SHA-1 of UTF-8 `input`.
    pub fn sha1_hex(input: &str) -> String {
        let mut hasher = Sha1::new();
        hasher.update(input.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Computes `SAPISIDHASH <unix_ts>_<sha1(ts + " " + sapisid + " " + origin)>`.
    pub fn sapisid_hash(sapisid: &str, origin: &str) -> String {
        Self::sapisid_hash_with_scheme("SAPISIDHASH", sapisid, origin, current_unix_seconds())
    }

    /// Computes `SAPISID1PHASH <unix_ts>_<sha1(ts + " " + sapisid_1p + " " + origin)>`.
    pub fn sapisid_1p_hash(sapisid_1p: &str, origin: &str) -> String {
        Self::sapisid_hash_with_scheme("SAPISID1PHASH", sapisid_1p, origin, current_unix_seconds())
    }

    /// Computes `SAPISID3PHASH <unix_ts>_<sha1(ts + " " + sapisid_3p + " " + origin)>`.
    pub fn sapisid_3p_hash(sapisid_3p: &str, origin: &str) -> String {
        Self::sapisid_hash_with_scheme("SAPISID3PHASH", sapisid_3p, origin, current_unix_seconds())
    }

    /// Deterministic `SAPISID*HASH` builder for a caller-supplied UNIX timestamp.
    pub fn sapisid_hash_with_scheme(
        scheme: &str,
        sapisid: &str,
        origin: &str,
        unix_ts: u64,
    ) -> String {
        let hash = Self::sha1_hex(&format!("{unix_ts} {sapisid} {origin}"));
        format!("{scheme} {unix_ts}_{hash}")
    }

    /// Builds a combined `Authorization` header value from available `SAPISID`,
    /// `__Secure-1PAPISID`, and `__Secure-3PAPISID` cookie values.
    pub fn sapisid_authorization_header(
        sapisid: Option<&str>,
        sapisid_1p: Option<&str>,
        sapisid_3p: Option<&str>,
        origin: &str,
    ) -> Option<String> {
        Self::sapisid_authorization_header_at(
            sapisid,
            sapisid_1p,
            sapisid_3p,
            origin,
            current_unix_seconds(),
        )
    }

    /// Deterministic variant of [`Self::sapisid_authorization_header`] using `unix_ts`.
    pub fn sapisid_authorization_header_at(
        sapisid: Option<&str>,
        sapisid_1p: Option<&str>,
        sapisid_3p: Option<&str>,
        origin: &str,
        unix_ts: u64,
    ) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(val) = sapisid.filter(|v| !v.is_empty()) {
            parts.push(Self::sapisid_hash_with_scheme(
                "SAPISIDHASH",
                val,
                origin,
                unix_ts,
            ));
        }
        if let Some(val) = sapisid_1p.filter(|v| !v.is_empty()) {
            parts.push(Self::sapisid_hash_with_scheme(
                "SAPISID1PHASH",
                val,
                origin,
                unix_ts,
            ));
        }
        if let Some(val) = sapisid_3p.filter(|v| !v.is_empty()) {
            parts.push(Self::sapisid_hash_with_scheme(
                "SAPISID3PHASH",
                val,
                origin,
                unix_ts,
            ));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }
}

fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_key_strips_whitespace_and_lowercases() {
        assert_eq!(AuthCrypto::normalize_key("  ABc 12\t3\nF "), "abc123f");
    }

    #[test]
    fn api_sig_sorts_skips_reserved_and_hashes() {
        let params = [
            ("method", "track.scrobble"),
            ("api_key", "mykey"),
            ("format", "json"),
            ("callback", "cb"),
            ("api_sig", "old"),
            ("empty", ""),
            ("track", "Song"),
        ];
        // Sorted included pairs: api_keymykeymethodtrack.scrobbletrackSong + secret
        let expected = AuthCrypto::md5_hex("api_keymykeymethodtrack.scrobbletrackSongmysecret");
        assert_eq!(AuthCrypto::api_sig(params, "mysecret"), expected);
    }

    #[test]
    fn sapisid_hashes_match_expected_sha1_format() {
        let ts = 1_700_000_000u64;
        let origin = "https://music.youtube.com";
        let expected_sha = AuthCrypto::sha1_hex("1700000000 tok123 https://music.youtube.com");
        assert_eq!(
            AuthCrypto::sapisid_hash_with_scheme("SAPISIDHASH", "tok123", origin, ts),
            format!("SAPISIDHASH {ts}_{expected_sha}")
        );
        assert_eq!(
            AuthCrypto::sapisid_authorization_header_at(
                Some("tok"),
                Some("tok1p"),
                Some("tok3p"),
                origin,
                ts,
            ),
            Some(format!(
                "SAPISIDHASH {ts}_{} SAPISID1PHASH {ts}_{} SAPISID3PHASH {ts}_{}",
                AuthCrypto::sha1_hex("1700000000 tok https://music.youtube.com"),
                AuthCrypto::sha1_hex("1700000000 tok1p https://music.youtube.com"),
                AuthCrypto::sha1_hex("1700000000 tok3p https://music.youtube.com"),
            ))
        );
    }
}
