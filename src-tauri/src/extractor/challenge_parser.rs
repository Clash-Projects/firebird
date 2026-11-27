use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde_json::{json, Value};
use thiserror::Error;

/// Errors produced when parsing YouTube `api/jnn/v1` BotGuard challenge responses.
#[derive(Debug, Error)]
pub enum ChallengeParseError {
    #[error("invalid JSON payload: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid base64 payload: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("unexpected challenge structure: {0}")]
    InvalidStructure(&'static str),

    #[error("invalid byte entry in comma-separated bytes: {0}")]
    InvalidByte(#[from] std::num::ParseIntError),
}

/// Parses a YouTube `api/jnn/v1` `create` response into the JSON payload expected
/// by the BotGuard WebView interpreter.
pub fn parse_create_challenge(raw_response: &str) -> Result<String, ChallengeParseError> {
    let outer: Value = serde_json::from_str(raw_response)?;
    let outer_arr = outer
        .as_array()
        .ok_or(ChallengeParseError::InvalidStructure("outer response is not a JSON array"))?;

    let challenge_val: Value = if outer_arr.len() > 1 {
        if let Some(scrambled) = outer_arr[1].as_str() {
            let decoded = descramble(scrambled)?;
            serde_json::from_str(&decoded)?
        } else {
            outer_arr
                .first()
                .cloned()
                .ok_or(ChallengeParseError::InvalidStructure("empty outer array"))?
        }
    } else {
        outer_arr
            .first()
            .cloned()
            .ok_or(ChallengeParseError::InvalidStructure("empty outer array"))?
    };

    let challenge = challenge_val
        .as_array()
        .ok_or(ChallengeParseError::InvalidStructure("challenge entry is not a JSON array"))?;

    let program = challenge
        .get(4)
        .and_then(Value::as_str)
        .ok_or(ChallengeParseError::InvalidStructure("missing challenge[4] program string"))?;
    let global_name = challenge
        .get(5)
        .and_then(Value::as_str)
        .ok_or(ChallengeParseError::InvalidStructure("missing challenge[5] globalName string"))?;

    let interpreter_js = first_string_in_array(challenge.get(1));
    let interpreter_url = first_string_in_array(challenge.get(2));

    let payload = json!({
        "program": program,
        "globalName": global_name,
        "interpreterJavascript": {
            "privateDoNotAccessOrElseSafeScriptWrappedValue": interpreter_js,
            "privateDoNotAccessOrElseTrustedResourceUrlWrappedValue": interpreter_url,
        }
    });
    Ok(payload.to_string())
}

/// Parses a YouTube `api/jnn/v1` `generateIT` response into `(token_u8_js_literal, lifetime_seconds)`.
pub fn parse_integrity_token(raw_response: &str) -> Result<(String, i64), ChallengeParseError> {
    let parsed: Value = serde_json::from_str(raw_response)?;
    let arr = parsed
        .as_array()
        .ok_or(ChallengeParseError::InvalidStructure("integrity response is not a JSON array"))?;

    let token_b64 = arr
        .first()
        .and_then(Value::as_str)
        .ok_or(ChallengeParseError::InvalidStructure("missing integrity token string at [0]"))?;
    let lifetime = arr
        .get(1)
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        .ok_or(ChallengeParseError::InvalidStructure("missing integrity lifetime at [1]"))?;

    let token_u8 = base64_to_js_uint8_array(token_b64)?;
    Ok((token_u8, lifetime))
}

/// Formats a UTF-8 `identifier` string as a JS `new Uint8Array([...])` literal.
pub fn string_to_js_uint8_array(identifier: &str) -> String {
    bytes_to_js_uint8_array(identifier.as_bytes())
}

/// Converts a JS `Uint8Array.toString()` comma-separated byte list into unpadded URL-safe base64.
pub fn comma_separated_bytes_to_base64(comma_bytes: &str) -> Result<String, ChallengeParseError> {
    let mut bytes = Vec::new();
    for part in comma_bytes.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            return Err(ChallengeParseError::InvalidStructure(
                "empty element in comma-separated bytes",
            ));
        }
        let val: i32 = trimmed.parse()?;
        bytes.push((val & 0xFF) as u8);
    }
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn first_string_in_array(val: Option<&Value>) -> Option<&str> {
    let arr = val?.as_array()?;
    arr.iter().find_map(Value::as_str)
}

fn descramble(base64_payload: &str) -> Result<String, ChallengeParseError> {
    let bytes = decode_lenient_base64(base64_payload)?;
    let descrambled: Vec<u8> = bytes.into_iter().map(|b| b.wrapping_add(97)).collect();
    Ok(String::from_utf8_lossy(&descrambled).into_owned())
}

fn base64_to_js_uint8_array(base64_value: &str) -> Result<String, ChallengeParseError> {
    let bytes = decode_lenient_base64(base64_value)?;
    Ok(bytes_to_js_uint8_array(&bytes))
}

fn bytes_to_js_uint8_array(bytes: &[u8]) -> String {
    let mut out = String::from("new Uint8Array([");
    for (idx, b) in bytes.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(&b.to_string());
    }
    out.push_str("])");
    out
}

fn decode_lenient_base64(base64_value: &str) -> Result<Vec<u8>, ChallengeParseError> {
    let mut normalized: String = base64_value
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            '.' => '=',
            other => other,
        })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '=')
        .collect();

    let pad = (4 - (normalized.len() % 4)) % 4;
    for _ in 0..pad {
        normalized.push('=');
    }
    Ok(STANDARD.decode(normalized.as_bytes())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_create_challenge_unscrambled_and_scrambled() {
        let unscrambled = json!([[
            null,
            ["console.log(1)"],
            ["https://example.com/interp.js"],
            null,
            "prog_abc",
            "bgGlobal"
        ]])
        .to_string();

        let parsed: Value =
            serde_json::from_str(&parse_create_challenge(&unscrambled).expect("unscrambled"))
                .expect("valid json");
        assert_eq!(parsed["program"], "prog_abc");
        assert_eq!(parsed["globalName"], "bgGlobal");
        assert_eq!(
            parsed["interpreterJavascript"]["privateDoNotAccessOrElseSafeScriptWrappedValue"],
            "console.log(1)"
        );
        assert_eq!(
            parsed["interpreterJavascript"]
                ["privateDoNotAccessOrElseTrustedResourceUrlWrappedValue"],
            "https://example.com/interp.js"
        );

        let inner = json!([
            null,
            [123, "interp_code"],
            [null, "https://example.com/bg.js"],
            null,
            "prog_xyz",
            "bgName"
        ])
        .to_string();
        let scrambled_bytes: Vec<u8> = inner.bytes().map(|b| b.wrapping_sub(97)).collect();
        let scrambled_b64 = URL_SAFE_NO_PAD.encode(scrambled_bytes);
        let scrambled_outer = json!([null, scrambled_b64]).to_string();

        let parsed_s: Value =
            serde_json::from_str(&parse_create_challenge(&scrambled_outer).expect("scrambled"))
                .expect("valid json");
        assert_eq!(parsed_s["program"], "prog_xyz");
        assert_eq!(parsed_s["globalName"], "bgName");
        assert_eq!(
            parsed_s["interpreterJavascript"]["privateDoNotAccessOrElseSafeScriptWrappedValue"],
            "interp_code"
        );
        assert_eq!(
            parsed_s["interpreterJavascript"]
                ["privateDoNotAccessOrElseTrustedResourceUrlWrappedValue"],
            "https://example.com/bg.js"
        );
    }

    #[test]
    fn parse_integrity_token_and_uint8_helpers() {
        let token_b64 = URL_SAFE_NO_PAD.encode([10u8, 255u8, 0u8]);
        let raw = json!([token_b64, 3600]).to_string();
        let (u8_js, lifetime) = parse_integrity_token(&raw).expect("token");
        assert_eq!(u8_js, "new Uint8Array([10,255,0])");
        assert_eq!(lifetime, 3600);

        assert_eq!(string_to_js_uint8_array("AB"), "new Uint8Array([65,66])");
        let b64 = comma_separated_bytes_to_base64("72, 105, 33").expect("csv b64");
        assert_eq!(b64, URL_SAFE_NO_PAD.encode(b"Hi!"));
    }
}
