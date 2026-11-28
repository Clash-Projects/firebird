use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use regex::Regex;
use reqwest::header::{ACCEPT_LANGUAGE, USER_AGENT};
use reqwest::Client;
use thiserror::Error;

use super::quickjs_cipher::{
    append_query_param, decipher_url_with_bytecode, extract_query_param, parse_query_map,
    replace_query_param, QuickJsCipher, QuickJsCipherWorker,
};

const WEB_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0";

/// Errors produced when parsing or executing fallback signature decipher operations.
#[derive(Debug, Error)]
pub enum SignatureDecipherError {
    #[error("invalid player script structure: {0}")]
    Parse(&'static str),

    #[error("unknown decipher statement: {0}")]
    UnknownStatement(String),

    #[error("missing helper object or member: {0}")]
    MissingHelper(String),

    #[error("cannot decipher empty input")]
    EmptyInput,
}

/// Operation kinds supported by the static fallback decipher interpreter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecipherOpType {
    Reverse,
    Splice,
    Slice,
    Swap,
    Shift,
    Unshift,
    Pop,
    Push,
}

/// Single decipher operation with integer arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecipherOp {
    pub op_type: DecipherOpType,
    pub args: Vec<i64>,
}

impl DecipherOp {
    pub fn new(op_type: DecipherOpType, args: Vec<i64>) -> Self {
        Self { op_type, args }
    }
}

/// Parsed YouTube `base.js` signature and throttling (`n`) transformation script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerScript {
    pub sts: Option<i64>,
    pub js_url: String,
    pub sig_name: String,
    pub sig_ops: Vec<DecipherOp>,
    pub sig_env: HashMap<String, i64>,
    pub sig_array_var: String,
    pub n_name: Option<String>,
    pub n_ops: Option<Vec<DecipherOp>>,
    pub n_env: HashMap<String, i64>,
    pub n_array_var: String,
}

/// YouTube player signature and `n`-parameter decipherer.
///
/// Uses in-process QuickJS EJS bytecode as the fast path and falls back to
/// regex-extracted op sequences when QuickJS bytecode is unavailable.
pub struct SignatureDecipher {
    client: Client,
    pub quick_js: Arc<QuickJsCipherWorker>,
    by_js_url: RwLock<HashMap<String, PlayerScript>>,
    n_cache: RwLock<HashMap<String, HashMap<String, String>>>,
}

impl SignatureDecipher {
    pub fn new(client: Client, quick_js: Option<Arc<QuickJsCipherWorker>>) -> Self {
        let qjs = quick_js.unwrap_or_else(|| Arc::new(QuickJsCipherWorker::new(client.clone())));
        Self {
            client,
            quick_js: qjs,
            by_js_url: RwLock::new(HashMap::new()),
            n_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Discovers `base.js` URL from YouTube embed/watch pages for `video_id`.
    pub async fn player_info(&self, video_id: &str) -> (Option<i64>, Option<String>) {
        let pages = [
            format!("https://www.youtube.com/embed/{video_id}?hl=en"),
            format!("https://www.youtube.com/watch?v={video_id}&hl=en"),
        ];

        let mut found_url: Option<String> = None;
        for page in pages {
            let Ok(req) = self
                .client
                .get(&page)
                .header(USER_AGENT, WEB_UA)
                .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
                .timeout(Duration::from_secs(15))
                .send()
                .await
            else {
                continue;
            };
            let Ok(html) = req.text().await else {
                continue;
            };
            if let Some(js_url) = extract_base_js_url(&html) {
                found_url = Some(js_url);
                break;
            }
        }

        if let Some(mut js_url) = found_url {
            js_url = js_url.replace("\\/", "/");
            if js_url.starts_with('/') {
                js_url = format!("https://www.youtube.com{js_url}");
            }
            return (None, Some(js_url));
        }

        (None, None)
    }

    /// Resolves the active player `signatureTimestamp` (`sts`), preferring QuickJS.
    pub async fn get_signature_timestamp(&self, video_id: &str) -> Option<i64> {
        if QuickJsCipher::is_available() {
            if let Ok(Some(player)) =
                tokio::time::timeout(Duration::from_secs(8), self.quick_js.ensure_player()).await
            {
                if player.sts > 0 {
                    return Some(player.sts);
                }
            }
        }

        match tokio::time::timeout(Duration::from_secs(25), self.script_for(video_id)).await {
            Ok(Some(script)) => script.sts,
            _ => None,
        }
    }

    /// Fetches and parses `base.js` into a cached [`PlayerScript`].
    pub async fn script_for(&self, video_id: &str) -> Option<PlayerScript> {
        if QuickJsCipher::is_available() && self.quick_js.active_player().is_none() {
            let _ = tokio::time::timeout(Duration::from_secs(6), self.quick_js.ensure_player()).await;
        }

        let (_, Some(js_url)) = self.player_info(video_id).await else {
            return None;
        };
        if js_url.is_empty() {
            return None;
        }

        if let Ok(guard) = self.by_js_url.read() {
            if let Some(cached) = guard.get(&js_url) {
                return Some(cached.clone());
            }
        }

        let res = self
            .client
            .get(&js_url)
            .header(USER_AGENT, WEB_UA)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .ok()?;
        let js = res.text().await.ok()?;
        if js.is_empty() {
            return None;
        }

        let url_for_parse = js_url.clone();
        let parsed = tokio::task::spawn_blocking(move || parse_player_script(&url_for_parse, &js))
            .await
            .ok()??;

        if let Ok(mut guard) = self.by_js_url.write() {
            if guard.len() > 8 {
                guard.clear();
                if let Ok(mut n_guard) = self.n_cache.write() {
                    n_guard.clear();
                }
            }
            guard.insert(js_url, parsed.clone());
        }
        Some(parsed)
    }

    /// Deciphers a `signatureCipher` (or `cipher`) query string into a playable URL.
    pub fn decipher_url(
        &self,
        signature_cipher: &str,
        script: Option<&PlayerScript>,
    ) -> Option<String> {
        if QuickJsCipher::is_available() {
            if let Some(player) = self.quick_js.active_player() {
                if !player.bytecode.is_empty() {
                    if let Some(fast) = decipher_url_with_bytecode(signature_cipher, &player.bytecode) {
                        return Some(fast);
                    }
                }
            }
        }

        let script = script?;
        let params = parse_query_map(signature_cipher);
        let mut url = params.get("url").filter(|u| !u.is_empty())?.clone();

        if let Some(encrypted) = params.get("s").filter(|s| !s.is_empty()) {
            let signature = apply_ops(&script.sig_ops, encrypted).ok()?;
            let sp = params
                .get("sp")
                .filter(|v| !v.is_empty())
                .map(String::as_str)
                .unwrap_or("signature");
            url = append_query_param(&url, sp, &signature);
        }

        if let Some(n) = extract_query_param(&url, "n").filter(|v| !v.is_empty()) {
            if let Some(n_ops) = &script.n_ops {
                let cached = self
                    .n_cache
                    .read()
                    .ok()
                    .and_then(|map| map.get(&script.js_url).and_then(|inner| inner.get(&n).cloned()));

                if let Some(fixed) = cached {
                    url = replace_query_param(&url, "n", &fixed);
                } else if let Ok(fixed) = apply_ops(n_ops, &n) {
                    if fixed.is_empty() {
                        return Some(url);
                    }
                    if let Ok(mut map) = self.n_cache.write() {
                        map.entry(script.js_url.clone())
                            .or_default()
                            .insert(n, fixed.clone());
                    }
                    url = replace_query_param(&url, "n", &fixed);
                }
            }
        }

        Some(url)
    }

    /// Solves throttling (`n`) on an already-direct stream URL, returning the
    /// original URL unchanged if no `n` transform is available or needed.
    pub fn decipher_direct_url(&self, url: &str, script: Option<&PlayerScript>) -> String {
        if url.is_empty() || url.starts_with("sabr:") {
            return url.to_string();
        }
        if QuickJsCipher::is_available() {
            if let Some(player) = self.quick_js.active_player() {
                if !player.bytecode.is_empty() {
                    if let Some(fast) = decipher_url_with_bytecode(url, &player.bytecode) {
                        return fast;
                    }
                }
            }
        }
        let Some(script) = script else {
            return url.to_string();
        };
        let Some(n) = extract_query_param(url, "n").filter(|v| !v.is_empty()) else {
            return url.to_string();
        };
        let Some(n_ops) = &script.n_ops else {
            return url.to_string();
        };
        let cached = self
            .n_cache
            .read()
            .ok()
            .and_then(|map| map.get(&script.js_url).and_then(|inner| inner.get(&n).cloned()));
        if let Some(fixed) = cached {
            return replace_query_param(url, "n", &fixed);
        }
        if let Ok(fixed) = apply_ops(n_ops, &n) {
            if !fixed.is_empty() {
                if let Ok(mut map) = self.n_cache.write() {
                    map.entry(script.js_url.clone())
                        .or_default()
                        .insert(n, fixed.clone());
                }
                return replace_query_param(url, "n", &fixed);
            }
        }
        url.to_string()
    }
}

/// Parses `base.js` source into a [`PlayerScript`] without performing network I/O.
pub fn parse_player_script(js_url: &str, js: &str) -> Option<PlayerScript> {
    let sts = extract_sts(js);
    let (sig_name, sig_extra) = find_sig_function(js)?;
    let (sig_body, sig_params) = extract_function(js, &sig_name).ok()?;
    let prepared = prepare_ops(js, &sig_body, &sig_params, sig_extra.as_deref()).ok()?;
    if prepared.ops.is_empty() {
        return None;
    }

    let mut n_name = None;
    let mut n_ops = None;
    let mut n_env = HashMap::new();
    let mut n_array_var = String::new();

    if let Some(found_n) = find_n_function(js) {
        if let Ok((n_body, n_params)) = extract_function(js, &found_n) {
            if let Ok(n_prep) = prepare_ops(js, &n_body, &n_params, None) {
                if !n_prep.ops.is_empty() {
                    n_name = Some(found_n);
                    n_ops = Some(n_prep.ops);
                    n_env = n_prep.env;
                    n_array_var = n_prep.array_var;
                }
            }
        }
    }

    Some(PlayerScript {
        sts,
        js_url: js_url.to_string(),
        sig_name,
        sig_ops: prepared.ops,
        sig_env: prepared.env,
        sig_array_var: prepared.array_var,
        n_name,
        n_ops,
        n_env,
        n_array_var,
    })
}

/// Applies a sequence of [`DecipherOp`] transformations to `input`.
pub fn apply_ops(ops: &[DecipherOp], input: &str) -> Result<String, SignatureDecipherError> {
    if input.is_empty() {
        return Err(SignatureDecipherError::EmptyInput);
    }
    let mut chars: Vec<char> = input.chars().collect();
    for op in ops {
        match op.op_type {
            DecipherOpType::Reverse => chars.reverse(),
            DecipherOpType::Splice => {
                let len = chars.len() as i64;
                let start = op.args.first().copied().unwrap_or(0).clamp(0, len) as usize;
                let end = if op.args.len() > 1 {
                    (start as i64 + op.args[1]).clamp(0, len) as usize
                } else {
                    chars.len()
                };
                if end > start {
                    chars.drain(start..end);
                }
            }
            DecipherOpType::Slice => {
                let start = op
                    .args
                    .first()
                    .copied()
                    .unwrap_or(0)
                    .clamp(0, chars.len() as i64) as usize;
                chars.drain(0..start);
            }
            DecipherOpType::Swap => {
                if !chars.is_empty() {
                    let arg = op.args.first().copied().unwrap_or(0);
                    let idx = arg.rem_euclid(chars.len() as i64) as usize;
                    chars.swap(0, idx);
                }
            }
            DecipherOpType::Shift => {
                if !chars.is_empty() {
                    chars.remove(0);
                }
            }
            DecipherOpType::Unshift => {
                if let Some(ch) = op.args.first().and_then(|&v| u32::try_from(v).ok()).and_then(char::from_u32) {
                    chars.insert(0, ch);
                }
            }
            DecipherOpType::Pop => {
                chars.pop();
            }
            DecipherOpType::Push => {
                if let Some(ch) = op.args.first().and_then(|&v| u32::try_from(v).ok()).and_then(char::from_u32) {
                    chars.push(ch);
                }
            }
        }
    }
    Ok(chars.into_iter().collect())
}

fn extract_base_js_url(html: &str) -> Option<String> {
    static DIRECT_RE: OnceLock<Regex> = OnceLock::new();
    static JSON_RE: OnceLock<Regex> = OnceLock::new();
    static ESCAPED_RE: OnceLock<Regex> = OnceLock::new();

    let direct_re = DIRECT_RE.get_or_init(|| {
        Regex::new(r"(/s/player/[A-Za-z0-9_./-]+/base\.js)").expect("valid regex")
    });
    if let Some(m) = direct_re.captures(html).and_then(|c| c.get(1)) {
        return Some(m.as_str().to_string());
    }

    let json_re = JSON_RE.get_or_init(|| {
        Regex::new(r#""jsUrl"\s*:\s*"((?:\\.|[^"\\])*)""#).expect("valid regex")
    });
    if let Some(m) = json_re.captures(html).and_then(|c| c.get(1)) {
        let quoted = format!("\"{}\"", m.as_str());
        if let Ok(decoded) = serde_json::from_str::<String>(&quoted) {
            return Some(decoded);
        }
        return Some(m.as_str().replace("\\/", "/"));
    }

    let escaped_re = ESCAPED_RE.get_or_init(|| {
        Regex::new(r"((?:\\?/s\\?/player\\?/[A-Za-z0-9_./\\-]+/base\.js))").expect("valid regex")
    });
    escaped_re
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().replace("\\/", "/"))
}

fn extract_sts(js: &str) -> Option<i64> {
    static STS_RE: OnceLock<Regex> = OnceLock::new();
    let re = STS_RE.get_or_init(|| Regex::new(r"signatureTimestamp[=:](\d+)").expect("valid regex"));
    re.captures(js)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<i64>().ok())
}

fn find_sig_function(js: &str) -> Option<(String, Option<String>)> {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    static SPLIT_FALLBACK_RE: OnceLock<Regex> = OnceLock::new();

    let patterns = PATTERNS.get_or_init(|| {
        vec![
            Regex::new(r"\b(?:[a-zA-Z0-9_$]+)&&\((?:[a-zA-Z0-9_$]+)=([a-zA-Z0-9_$]{2,})\((\d+),decodeURIComponent\((?:[a-zA-Z0-9_$]+)\)\)").expect("valid regex"),
            Regex::new(r"\b(?:[a-zA-Z0-9_$]+)&&\((?:[a-zA-Z0-9_$]+)=([a-zA-Z0-9_$]{2,})\(decodeURIComponent\((?:[a-zA-Z0-9_$]+)\)\)").expect("valid regex"),
            Regex::new(r"\bm=([a-zA-Z0-9$]{2,})\(decodeURIComponent\(h\.s\)\)").expect("valid regex"),
            Regex::new(r"\bc&&\(c=([a-zA-Z0-9$]{2,})\(decodeURIComponent\(c\)\)").expect("valid regex"),
            Regex::new(r#"(?:\b|[^a-zA-Z0-9$])([a-zA-Z0-9$]{2,})\s*=\s*function\(\s*a\s*\)\s*\{\s*a\s*=\s*a\.split\(\s*""\s*\)"#).expect("valid regex"),
        ]
    });

    for re in patterns {
        if let Some(caps) = re.captures(js) {
            if let Some(name) = caps.get(1).map(|m| m.as_str()).filter(|s| !s.is_empty()) {
                let extra = caps.get(2).map(|m| m.as_str().to_string());
                return Some((name.to_string(), extra));
            }
        }
    }

    // Rust `regex` lacks backreferences, so match `name=function(p1){p2=p3.split("");`
    // and verify `p1 == p2 && p2 == p3`.
    let fallback = SPLIT_FALLBACK_RE.get_or_init(|| {
        Regex::new(r#"([\w$]+)\s*=\s*function\((\w+)\)\{\s*(\w+)\s*=\s*(\w+)\.split\(""\)\s*;"#)
            .expect("valid regex")
    });
    for caps in fallback.captures_iter(js) {
        let p1 = caps.get(2).map(|m| m.as_str());
        let p2 = caps.get(3).map(|m| m.as_str());
        let p3 = caps.get(4).map(|m| m.as_str());
        if p1.is_some() && p1 == p2 && p2 == p3 {
            if let Some(name) = caps.get(1).map(|m| m.as_str()).filter(|s| !s.is_empty()) {
                return Some((name.to_string(), None));
            }
        }
    }
    None
}

fn find_n_function(js: &str) -> Option<String> {
    static N_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = N_PATTERNS.get_or_init(|| {
        let sc = r"[a-zA-Z0-9$_]";
        let mc = r"[a-zA-Z0-9$_]+";
        vec![
            Regex::new(r"(?s)([A-Za-z0-9_$]{2,})=function.*return [A-Z]\[\d+\]").expect("valid regex"),
            Regex::new(&format!(r#"{sc}="nn"\[\+{mc}\.{mc}\],{mc}\({mc}\),{mc}={mc}\.{mc}\[{mc}\]\|\|null\)&&\({mc}=({mc})\[(\d+)\]"#)).expect("valid regex"),
            Regex::new(&format!(r#"{sc}="nn"\[\+{mc}\.{mc}\],{mc}\({mc}\),{mc}={mc}\.{mc}\[{mc}\]\|\|null\).+\|\|({mc})\(""\)"#)).expect("valid regex"),
            Regex::new(&format!(r#",{mc}\({mc}\),{mc}={mc}\.{mc}\[{mc}\]\|\|null\)&&\(\b{mc}=({mc})\[(\d+)\]\({sc}\),{mc}\.set\((?:"n+"|{sc}),{mc}\)"#)).expect("valid regex"),
            Regex::new(&format!(r#"{sc}="nn"\[\+{mc}\.{mc}\],{mc}={mc}\.get\({mc}\)\).+\|\|({mc})\(""\)"#)).expect("valid regex"),
            Regex::new(&format!(r#"{sc}="nn"\[\+{mc}\.{mc}\],{mc}={mc}\.get\({mc}\)\)&&\({mc}=({mc})\[(\d+)\]"#)).expect("valid regex"),
            Regex::new(&format!(r"\({sc}=String\.fromCharCode\(110\),{sc}={sc}\.get\({sc}\)\)&&\({sc}=({mc})(?:\[(\d+)\])?\({sc}\)")).expect("valid regex"),
            Regex::new(&format!(r#"\.get\("n"\)\)&&\({sc}=({mc})(?:\[(\d+)\])?\({sc}\)"#)).expect("valid regex"),
        ]
    });

    for re in patterns {
        let Some(caps) = re.captures(js) else {
            continue;
        };
        let Some(mut name) = caps.get(1).map(|m| m.as_str().to_string()).filter(|s| !s.is_empty()) else {
            continue;
        };
        if let Some(idx_str) = caps.get(2).map(|m| m.as_str()) {
            let arr_re = Regex::new(&format!(r"{}\s*=\s*\[(.+?)\][;,]", regex::escape(&name))).ok()?;
            let Some(arr) = arr_re.captures(js).and_then(|c| c.get(1)).map(|m| m.as_str()) else {
                continue;
            };
            let names: Vec<&str> = arr.split(',').collect();
            let Ok(idx) = idx_str.parse::<usize>() else {
                continue;
            };
            let Some(resolved) = names.get(idx).map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                continue;
            };
            name = resolved.to_string();
        }
        return Some(name);
    }
    None
}

struct PreparedOps {
    ops: Vec<DecipherOp>,
    env: HashMap<String, i64>,
    array_var: String,
}

fn extract_function(js: &str, name: &str) -> Result<(String, Vec<String>), SignatureDecipherError> {
    let needle = format!("{name}=function");
    let idx = js
        .find(&needle)
        .ok_or(SignatureDecipherError::Parse("missing function definition"))?;
    let rel_open = js[idx..]
        .find('{')
        .ok_or(SignatureDecipherError::Parse("missing opening brace"))?;
    let open_idx = idx + rel_open;

    let sig_re = Regex::new(&format!(r"{}\s*=\s*function\s*\(([^)]*)\)", regex::escape(name)))
        .map_err(|_| SignatureDecipherError::Parse("invalid function regex"))?;
    let header = &js[idx..=open_idx];
    let params = sig_re
        .captures(header)
        .and_then(|c| c.get(1))
        .map(|m| {
            m.as_str()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let body = extract_balanced(js, open_idx)?;
    Ok((body.to_string(), params))
}

fn prepare_ops(
    js: &str,
    body: &str,
    params: &[String],
    extra_literal: Option<&str>,
) -> Result<PreparedOps, SignatureDecipherError> {
    let mut env = HashMap::new();
    let array_var = if let Some(extra_raw) = extra_literal.filter(|s| !s.is_empty()) {
        if params.len() > 1 {
            let extra = extra_raw
                .parse::<i64>()
                .map_err(|_| SignatureDecipherError::Parse("extra argument is not numeric"))?;
            env.insert(params[0].clone(), extra);
            params[1].clone()
        } else {
            params
                .first()
                .cloned()
                .ok_or(SignatureDecipherError::Parse("no function parameters"))?
        }
    } else {
        params
            .first()
            .cloned()
            .ok_or(SignatureDecipherError::Parse("no function parameters"))?
    };

    let split_marker = ".split(\"\")";
    let split_idx = body
        .find(split_marker)
        .ok_or(SignatureDecipherError::Parse("missing .split(\"\")"))?;
    let mut after_split = body[split_idx + split_marker.len()..].to_string();

    if let Some(first) = params.first() {
        let guard_re = Regex::new(&format!(
            r#";\s*if\s*\(\s*typeof\s+[A-Za-z0-9_$]+\s*===?\s*(?:"undefined"|'undefined')\s*\)\s*return\s+{}\s*;"#,
            regex::escape(first)
        ))
        .map_err(|_| SignatureDecipherError::Parse("invalid guard regex"))?;
        after_split = guard_re.replace_all(&after_split, ";").into_owned();
    }

    let stmts = match after_split.find("return") {
        Some(ret_idx) => &after_split[..ret_idx],
        None => after_split.as_str(),
    };

    let call_re = Regex::new(&format!(
        r"^([A-Za-z0-9_$]+)\.([A-Za-z0-9_$]+)\(\s*{}\s*(?:,\s*(.+?))?\)$",
        regex::escape(&array_var)
    ))
    .map_err(|_| SignatureDecipherError::Parse("invalid call regex"))?;

    let mut ops = Vec::new();
    for raw in stmts.split(';') {
        let stmt = raw.trim();
        if stmt.is_empty()
            || stmt.starts_with("return")
            || stmt.starts_with("var ")
            || stmt.starts_with("let ")
            || stmt.starts_with("const ")
        {
            continue;
        }
        if let Some(caps) = call_re.captures(stmt) {
            let obj = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let method = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let arg_src = caps.get(3).map(|m| m.as_str());
            ops.push(resolve_op(js, obj, method, arg_src, &env)?);
            continue;
        }
        if stmt == format!("{array_var}.reverse()") {
            ops.push(DecipherOp::new(DecipherOpType::Reverse, Vec::new()));
            continue;
        }
        return Err(SignatureDecipherError::UnknownStatement(stmt.to_string()));
    }

    if ops.is_empty() {
        return Err(SignatureDecipherError::Parse("no decipher ops extracted"));
    }

    Ok(PreparedOps {
        ops,
        env,
        array_var,
    })
}

fn resolve_op(
    js: &str,
    obj: &str,
    method: &str,
    arg_src: Option<&str>,
    outer_env: &HashMap<String, i64>,
) -> Result<DecipherOp, SignatureDecipherError> {
    let helper = helper_body(js, obj)?;
    let member = member_body(helper, method)?;
    let params = member_params(helper, method);
    let call_args = match arg_src.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => split_args(s),
        None => Vec::new(),
    };

    let mut env = outer_env.clone();
    for (i, arg) in call_args.iter().enumerate() {
        if i + 1 < params.len() {
            if let Some(v) = eval_int(arg, &env, 0) {
                env.insert(params[i + 1].clone(), v);
            }
        }
    }
    classify_member(member, &env)
}

fn helper_body<'a>(js: &'a str, obj: &str) -> Result<&'a str, SignatureDecipherError> {
    let re = Regex::new(&format!(r"(?:var\s+)?{}\s*=\s*\{{", regex::escape(obj)))
        .map_err(|_| SignatureDecipherError::MissingHelper(obj.to_string()))?;
    let m = re
        .find(js)
        .ok_or_else(|| SignatureDecipherError::MissingHelper(obj.to_string()))?;
    let rel_open = js[m.start()..]
        .find('{')
        .ok_or_else(|| SignatureDecipherError::MissingHelper(obj.to_string()))?;
    extract_balanced(js, m.start() + rel_open)
}

fn member_body<'a>(helper: &'a str, method: &str) -> Result<&'a str, SignatureDecipherError> {
    let colon_re = Regex::new(&format!(
        r"{}\s*:\s*function\s*\(([^)]*)\)\s*\{{",
        regex::escape(method)
    ))
    .map_err(|_| SignatureDecipherError::MissingHelper(method.to_string()))?;
    if let Some(m) = colon_re.find(helper) {
        return extract_balanced(helper, m.end() - 1);
    }

    let shorthand_re = Regex::new(&format!(r"{}\s*\(([^)]*)\)\s*\{{", regex::escape(method)))
        .map_err(|_| SignatureDecipherError::MissingHelper(method.to_string()))?;
    if let Some(m) = shorthand_re.find(helper) {
        return extract_balanced(helper, m.end() - 1);
    }

    Err(SignatureDecipherError::MissingHelper(method.to_string()))
}

fn member_params(helper: &str, method: &str) -> Vec<String> {
    let Ok(re) = Regex::new(&format!(
        r"{}\s*(?::\s*function)?\s*\(([^)]*)\)\s*\{{",
        regex::escape(method)
    )) else {
        return Vec::new();
    };
    re.captures(helper)
        .and_then(|c| c.get(1))
        .map(|m| {
            m.as_str()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn classify_member(
    body: &str,
    env: &HashMap<String, i64>,
) -> Result<DecipherOp, SignatureDecipherError> {
    let b: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    if b.contains(".reverse(") {
        return Ok(DecipherOp::new(DecipherOpType::Reverse, Vec::new()));
    }
    if b.contains(".splice(") {
        let args = Regex::new(r"\.splice\(([^)]*)\)")
            .ok()
            .and_then(|re| re.captures(&b))
            .and_then(|c| c.get(1))
            .map(|m| split_args(m.as_str()))
            .unwrap_or_default();
        return Ok(splice_op(&args, env));
    }
    if b.contains(".slice(") {
        let arg = Regex::new(r"\.slice\(([^),]*)")
            .ok()
            .and_then(|re| re.captures(&b))
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or("0");
        let n = eval_int(arg, env, 0).unwrap_or(0);
        return Ok(DecipherOp::new(DecipherOpType::Slice, vec![n]));
    }
    if b.contains(".push(") {
        let arg = Regex::new(r"\.push\(([^)]*)\)")
            .ok()
            .and_then(|re| re.captures(&b))
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or("");
        let n = eval_int(arg, env, -1).unwrap_or(-1);
        if n < 0 {
            return Err(SignatureDecipherError::Parse("invalid push arg"));
        }
        return Ok(DecipherOp::new(DecipherOpType::Push, vec![n]));
    }
    if b.contains(".unshift(") {
        let arg = Regex::new(r"\.unshift\(([^)]*)\)")
            .ok()
            .and_then(|re| re.captures(&b))
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .unwrap_or("");
        let n = eval_int(arg, env, -1).unwrap_or(-1);
        if n < 0 {
            return Err(SignatureDecipherError::Parse("invalid unshift arg"));
        }
        return Ok(DecipherOp::new(DecipherOpType::Unshift, vec![n]));
    }
    if b.contains(".shift(") {
        return Ok(DecipherOp::new(DecipherOpType::Shift, Vec::new()));
    }
    if b.contains(".pop(") {
        return Ok(DecipherOp::new(DecipherOpType::Pop, Vec::new()));
    }
    if b.contains("[0]") {
        if let Some(caps) = Regex::new(r"\[([^\]]+)%").ok().and_then(|re| re.captures(&b)) {
            if let Some(expr) = caps.get(1).map(|m| m.as_str()) {
                let n = eval_int(expr, env, 0).unwrap_or(0);
                return Ok(DecipherOp::new(DecipherOpType::Swap, vec![n]));
            }
        }
    }
    Err(SignatureDecipherError::Parse("unknown helper op"))
}

fn splice_op(args: &[String], env: &HashMap<String, i64>) -> DecipherOp {
    if args.len() >= 2 {
        let start = eval_int(&args[0], env, 0).unwrap_or(0);
        let count = eval_int(&args[1], env, 0).unwrap_or(0);
        DecipherOp::new(DecipherOpType::Splice, vec![start, count])
    } else {
        let start = args
            .first()
            .and_then(|a| eval_int(a, env, 0))
            .unwrap_or(0);
        DecipherOp::new(DecipherOpType::Splice, vec![start])
    }
}

fn split_args(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut buf = String::new();
    for ch in src.chars() {
        if ch == '(' || ch == '[' {
            depth += 1;
        } else if ch == ')' || ch == ']' {
            depth -= 1;
        }
        if ch == ',' && depth == 0 {
            out.push(buf.trim().to_string());
            buf.clear();
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

fn eval_int(expr: &str, env: &HashMap<String, i64>, len: i64) -> Option<i64> {
    static MOD_RE: OnceLock<Regex> = OnceLock::new();
    let e = expr.trim();
    if let Ok(lit) = e.parse::<i64>() {
        return Some(lit);
    }
    if let Some(&val) = env.get(e) {
        return Some(val);
    }
    let mod_re = MOD_RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z0-9_$]+)\s*%\s*[A-Za-z0-9_$.]+\.length$").expect("valid regex")
    });
    if let Some(caps) = mod_re.captures(e) {
        let inner = caps.get(1)?.as_str();
        let val = eval_int(inner, env, len)?;
        if len > 0 {
            return Some(val.rem_euclid(len));
        }
        return Some(val);
    }
    None
}

fn extract_balanced(src: &str, open_idx: usize) -> Result<&str, SignatureDecipherError> {
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let mut escaped = false;

    for (rel_idx, ch) in src[open_idx..].char_indices() {
        if let Some(quote) = in_str {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_str = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            in_str = Some(ch);
            continue;
        }
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return Ok(&src[open_idx + 1..open_idx + rel_idx]);
            }
        }
    }
    Err(SignatureDecipherError::Parse("unbalanced braces"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::http_client::HttpClientFactory;

    #[test]
    fn op_interpreter_handles_all_ops() {
        let ops = vec![
            DecipherOp::new(DecipherOpType::Swap, vec![2]),      // abcdef -> cbadef
            DecipherOp::new(DecipherOpType::Reverse, Vec::new()), // cbadef -> fedabc
            DecipherOp::new(DecipherOpType::Splice, vec![1, 2]), // fedabc -> fabc
            DecipherOp::new(DecipherOpType::Slice, vec![1]),     // fabc -> abc
            DecipherOp::new(DecipherOpType::Shift, Vec::new()),  // abc -> bc
            DecipherOp::new(DecipherOpType::Pop, Vec::new()),    // bc -> b
            DecipherOp::new(DecipherOpType::Unshift, vec![b'X' as i64]), // b -> Xb
            DecipherOp::new(DecipherOpType::Push, vec![b'Z' as i64]),    // Xb -> XbZ
        ];
        assert_eq!(apply_ops(&ops, "abcdef").expect("apply"), "XbZ");
    }

    #[test]
    fn parse_player_script_and_decipher_fallback() {
        let fake_js = r#"
var cfg={signatureTimestamp:19999};
var Xy={
  rv:function(a){a.reverse()},
  sw:function(a,b){var c=a[0];a[0]=a[b%a.length];a[b%a.length]=c},
  sp:function(a,b){a.splice(0,b)}
};
var decSig=function(a){a=a.split("");if(typeof zz==="undefined")return a;Xy.sw(a,1);Xy.rv(a);Xy.sp(a,1);return a.join("")};
a&&b&&(b=decSig(decodeURIComponent(c)));
"#;
        let script = parse_player_script("https://www.youtube.com/s/player/12345678/base.js", fake_js)
            .expect("parse script");
        assert_eq!(script.sts, Some(19999));
        assert_eq!(script.sig_name, "decSig");
        assert_eq!(script.sig_ops.len(), 3);

        let client = HttpClientFactory::create().expect("client");
        let decipher = SignatureDecipher::new(client, None);
        // "abcd" -> swap(1) -> "bacd" -> reverse -> "dcab" -> splice(0,1) -> "cab"
        let out = decipher
            .decipher_url(
                "s=abcd&sp=sig&url=https%3A%2F%2Fexample.com%2Fvideoplayback%3Fmime%3Daudio",
                Some(&script),
            )
            .expect("decipher");
        assert_eq!(
            out,
            "https://example.com/videoplayback?mime=audio&sig=cab"
        );
    }
}
