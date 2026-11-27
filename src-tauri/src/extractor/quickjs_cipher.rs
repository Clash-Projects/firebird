use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use regex::Regex;
use reqwest::Client;
use rquickjs::{qjs, Context, Runtime};
use serde_json::json;
use thiserror::Error;
use tokio::sync::Mutex as AsyncMutex;
use url::{form_urlencoded, Url};

const SOLVER_LIB_JS: &str = include_str!("../../assets/ejs/yt.solver.lib.min.js");
const SOLVER_CORE_JS: &str = include_str!("../../assets/ejs/yt.solver.core.min.js");

const PLAYER_CACHE_TTL: Duration = Duration::from_secs(3600);
const QUICKJS_STACK_SIZE: usize = 64 << 20;

/// Errors raised during QuickJS cipher preprocessing, compilation, or solving.
#[derive(Debug, Error)]
pub enum QuickJsCipherError {
    #[error("QuickJS runtime error: {0}")]
    Runtime(#[from] rquickjs::Error),

    #[error("QuickJS exception: {0}")]
    JsException(String),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("solver did not return preprocessed player JS")]
    MissingPreprocessedPlayer,

    #[error("failed to serialize QuickJS bytecode")]
    BytecodeWriteFailed,

    #[error("empty QuickJS bytecode")]
    EmptyBytecode,

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
}

/// Precompiled YouTube player solver state.
#[derive(Debug, Clone)]
pub struct QuickJsPlayer {
    pub id: String,
    pub sts: i64,
    pub bytecode: Vec<u8>,
    pub preprocessed_js: Option<String>,
}

/// Deciphered `n` and `sig` challenge outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickJsSolveResult {
    pub n: Option<String>,
    pub sig: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedPlayer {
    player: QuickJsPlayer,
    loaded_at: Instant,
}

/// In-process QuickJS cipher engine embedding yt-dlp's EJS solver.
pub struct QuickJsCipher;

impl QuickJsCipher {
    /// QuickJS is compiled directly into the binary via `rquickjs`.
    pub fn is_available() -> bool {
        true
    }

    /// Preprocesses `base.js` using the embedded EJS solver (`yt.solver.lib` + `yt.solver.core`).
    pub fn preprocess(base_js: &str) -> Result<String, QuickJsCipherError> {
        let rt = new_runtime()?;
        let ctx = Context::full(&rt)?;
        ctx.with(|ctx| {
            let input = json!({
                "type": "player",
                "player": base_js,
                "output_preprocessed": true,
                "requests": []
            });
            let src = format!(
                "{SOLVER_LIB_JS}\nObject.assign(globalThis, lib);\n{SOLVER_CORE_JS}\nJSON.stringify(jsc({input}))"
            );
            let out: String = ctx.eval(src).map_err(|e| capture_js_err(&ctx, e))?;
            let parsed: serde_json::Value = serde_json::from_str(&out)?;
            parsed["preprocessed_player"]
                .as_str()
                .map(str::to_owned)
                .ok_or(QuickJsCipherError::MissingPreprocessedPlayer)
        })
    }

    /// Compiles preprocessed solver JS into stripped QuickJS bytecode.
    pub fn compile(preprocessed_js: &str) -> Result<Vec<u8>, QuickJsCipherError> {
        let rt = new_runtime()?;
        let ctx = Context::full(&rt)?;
        let src = format!(
            "var _r = {{n: null, sig: null}};\n(function (_result) {{\n{preprocessed_js}\n}})(_r);\n\0"
        );
        ctx.with(|ctx| unsafe {
            let raw_ctx = ctx.as_raw().as_ptr();
            let eval_flags = (qjs::JS_EVAL_TYPE_GLOBAL | qjs::JS_EVAL_FLAG_COMPILE_ONLY) as i32;
            let filename = c"solver.js";
            let func = qjs::JS_Eval(
                raw_ctx,
                src.as_ptr().cast(),
                (src.len() - 1) as _,
                filename.as_ptr(),
                eval_flags,
            );
            if qjs::JS_IsException(func) {
                return Err(capture_js_err(&ctx, rquickjs::Error::Exception));
            }
            let mut len = 0;
            let write_flags = (qjs::JS_WRITE_OBJ_BYTECODE
                | qjs::JS_WRITE_OBJ_STRIP_SOURCE
                | qjs::JS_WRITE_OBJ_STRIP_DEBUG) as i32;
            let buf = qjs::JS_WriteObject(raw_ctx, &mut len, func, write_flags);
            qjs::JS_FreeValue(raw_ctx, func);
            if buf.is_null() {
                return Err(QuickJsCipherError::BytecodeWriteFailed);
            }
            let out = std::slice::from_raw_parts(buf, len as usize).to_vec();
            qjs::js_free(raw_ctx, buf.cast());
            Ok(out)
        })
    }

    /// Solves `n` and/or `sig` challenges from preprocessed JS source.
    pub fn solve(
        preprocessed_js: &str,
        n: Option<&str>,
        sig: Option<&str>,
    ) -> Result<QuickJsSolveResult, QuickJsCipherError> {
        let rt = new_runtime()?;
        let ctx = Context::full(&rt)?;
        ctx.with(|ctx| {
            let setup = format!(
                "var _r = {{n: null, sig: null}};\n(function (_result) {{\n{preprocessed_js}\n}})(_r);"
            );
            let _: rquickjs::Value = ctx.eval(setup).map_err(|e| capture_js_err(&ctx, e))?;
            eval_solver_calls(&ctx, n, sig)
        })
    }

    /// Solves `n` and/or `sig` challenges from precompiled QuickJS bytecode.
    pub fn solve_bytecode(
        bytecode: &[u8],
        n: Option<&str>,
        sig: Option<&str>,
    ) -> Result<QuickJsSolveResult, QuickJsCipherError> {
        if bytecode.is_empty() {
            return Err(QuickJsCipherError::EmptyBytecode);
        }
        let rt = new_runtime()?;
        let ctx = Context::full(&rt)?;
        ctx.with(|ctx| unsafe {
            let raw_ctx = ctx.as_raw().as_ptr();
            let func = qjs::JS_ReadObject(
                raw_ctx,
                bytecode.as_ptr(),
                bytecode.len() as _,
                qjs::JS_READ_OBJ_BYTECODE as i32,
            );
            if qjs::JS_IsException(func) {
                return Err(capture_js_err(&ctx, rquickjs::Error::Exception));
            }
            let result = qjs::JS_EvalFunction(raw_ctx, func);
            if qjs::JS_IsException(result) {
                return Err(capture_js_err(&ctx, rquickjs::Error::Exception));
            }
            qjs::JS_FreeValue(raw_ctx, result);
            eval_solver_calls(&ctx, n, sig)
        })
    }
}

/// Manages player bytecode disk caching (`.qbc`/`.sts`), background preparation on
/// blocking threads, and URL deciphering.
pub struct QuickJsCipherWorker {
    client: Client,
    cache_dir: PathBuf,
    active: RwLock<Option<CachedPlayer>>,
    prepare_lock: AsyncMutex<()>,
}

impl QuickJsCipherWorker {
    pub fn new(client: Client) -> Self {
        Self::with_cache_dir(client, Self::default_cache_dir())
    }

    pub fn with_cache_dir(client: Client, cache_dir: PathBuf) -> Self {
        Self {
            client,
            cache_dir,
            active: RwLock::new(None),
            prepare_lock: AsyncMutex::new(()),
        }
    }

    pub fn default_cache_dir() -> PathBuf {
        std::env::temp_dir().join("lastwave_ejs")
    }

    /// Returns the currently loaded [`QuickJsPlayer`] if cached in memory.
    pub fn active_player(&self) -> Option<QuickJsPlayer> {
        self.active
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|entry| entry.player.clone()))
    }

    fn fresh_active_player(&self) -> Option<QuickJsPlayer> {
        let guard = self.active.read().ok()?;
        let entry = guard.as_ref()?;
        if entry.loaded_at.elapsed() < PLAYER_CACHE_TTL {
            Some(entry.player.clone())
        } else {
            None
        }
    }

    /// Returns a fresh preprocessed/compiled player, loading from disk cache or
    /// fetching and compiling `base.js` in `tokio::task::spawn_blocking`.
    pub async fn ensure_player(&self) -> Option<QuickJsPlayer> {
        if let Some(player) = self.fresh_active_player() {
            return Some(player);
        }

        let _guard = self.prepare_lock.lock().await;
        if let Some(player) = self.fresh_active_player() {
            return Some(player);
        }

        match self.load_or_prepare_player().await {
            Ok(Some(player)) => {
                if let Ok(mut slot) = self.active.write() {
                    *slot = Some(CachedPlayer {
                        player: player.clone(),
                        loaded_at: Instant::now(),
                    });
                }
                Some(player)
            }
            _ => None,
        }
    }

    async fn load_or_prepare_player(&self) -> Result<Option<QuickJsPlayer>, QuickJsCipherError> {
        if !QuickJsCipher::is_available() {
            return Ok(None);
        }
        let _ = fs::create_dir_all(&self.cache_dir);

        let iframe = self
            .client
            .get("https://www.youtube.com/iframe_api")
            .send()
            .await?
            .text()
            .await?;

        let Some(id) = extract_player_id(&iframe) else {
            return Ok(None);
        };

        let qbc_path = self.cache_dir.join(format!("{id}.qbc"));
        let sts_path = self.cache_dir.join(format!("{id}.sts"));

        if qbc_path.exists() && sts_path.exists() {
            if let (Ok(sts_raw), Ok(bc)) = (fs::read_to_string(&sts_path), fs::read(&qbc_path)) {
                if let Ok(sts) = sts_raw.trim().parse::<i64>() {
                    if sts > 0 && !bc.is_empty() {
                        return Ok(Some(QuickJsPlayer {
                            id,
                            sts,
                            bytecode: bc,
                            preprocessed_js: None,
                        }));
                    }
                }
            }
        }

        let base_js_url =
            format!("https://www.youtube.com/s/player/{id}/player_ias.vflset/en_US/base.js");
        let base_js = self
            .client
            .get(&base_js_url)
            .send()
            .await?
            .text()
            .await?;
        if base_js.is_empty() {
            return Ok(None);
        }

        let Some(sts) = extract_signature_timestamp(&base_js).filter(|&s| s > 0) else {
            return Ok(None);
        };

        let compiled = tokio::task::spawn_blocking(move || {
            let pre = QuickJsCipher::preprocess(&base_js)?;
            let bc = QuickJsCipher::compile(&pre)?;
            Ok::<_, QuickJsCipherError>((pre, bc))
        })
        .await;

        let Ok(Ok((preprocessed, bytecode))) = compiled else {
            return Ok(None);
        };
        if bytecode.is_empty() {
            return Ok(None);
        }

        prune_stale_player_cache(&self.cache_dir, &id);
        let _ = fs::write(&qbc_path, &bytecode);
        let _ = fs::write(&sts_path, sts.to_string());

        Ok(Some(QuickJsPlayer {
            id,
            sts,
            bytecode,
            preprocessed_js: Some(preprocessed),
        }))
    }

    /// Deciphers a direct stream URL or `signatureCipher` query string using QuickJS bytecode.
    pub async fn decipher_url(
        &self,
        url_or_cipher: &str,
        player_override: Option<&QuickJsPlayer>,
    ) -> Option<String> {
        let player = match player_override {
            Some(p) => p.clone(),
            None => self.ensure_player().await?,
        };
        if player.bytecode.is_empty() {
            return None;
        }
        decipher_url_with_bytecode(url_or_cipher, &player.bytecode)
    }
}

/// Deciphers a direct URL or `signatureCipher` query string synchronously against `bytecode`.
pub fn decipher_url_with_bytecode(url_or_cipher: &str, bytecode: &[u8]) -> Option<String> {
    if bytecode.is_empty() {
        return None;
    }

    let (url, s_param, sp_param) = if url_or_cipher.starts_with("http://")
        || url_or_cipher.starts_with("https://")
    {
        (url_or_cipher.to_string(), None, "signature".to_string())
    } else {
        let params = parse_query_map(url_or_cipher);
        let url = params.get("url")?.clone();
        let s = params.get("s").cloned().filter(|v| !v.is_empty());
        let sp = params
            .get("sp")
            .filter(|v| !v.is_empty())
            .cloned()
            .unwrap_or_else(|| "signature".to_string());
        (url, s, sp)
    };

    if url.is_empty() {
        return None;
    }
    if Url::parse(&url).is_err() {
        return None;
    }

    let n_param = extract_query_param(&url, "n");
    if n_param.as_deref().is_none_or(str::is_empty) && s_param.is_none() {
        return Some(url);
    }

    let solved =
        QuickJsCipher::solve_bytecode(bytecode, n_param.as_deref(), s_param.as_deref()).ok()?;

    let mut final_url = url;
    if s_param.is_some() {
        if let Some(sig) = solved.sig {
            final_url = append_query_param(&final_url, &sp_param, &sig);
        }
    }
    if n_param.is_some() {
        if let Some(n_val) = solved.n {
            final_url = replace_query_param(&final_url, "n", &n_val);
        }
    }
    Some(final_url)
}

/// Extracts the 8-hex-char YouTube player ID from `iframe_api` JS.
pub fn extract_player_id(iframe: &str) -> Option<String> {
    static PLAYER_ID_RE: OnceLock<Regex> = OnceLock::new();
    let re = PLAYER_ID_RE
        .get_or_init(|| Regex::new(r"player[\\/]+([a-fA-F0-9]{8})[\\/]+").expect("valid regex"));
    re.captures(iframe)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// Extracts `signatureTimestamp` / `sts` integer from `base.js`.
pub fn extract_signature_timestamp(base_js: &str) -> Option<i64> {
    static STS_RE: OnceLock<Regex> = OnceLock::new();
    let re = STS_RE.get_or_init(|| {
        Regex::new(r"(?:signatureTimestamp|sts)\s*[:=]\s*(\d+)").expect("valid regex")
    });
    re.captures(base_js)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<i64>().ok())
}

/// Parses `application/x-www-form-urlencoded` key/value pairs into a map.
pub fn parse_query_map(query: &str) -> HashMap<String, String> {
    form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
}

/// Reads a single query parameter value from a URL.
pub fn extract_query_param(raw_url: &str, key: &str) -> Option<String> {
    let parsed = Url::parse(raw_url).ok()?;
    parsed
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

/// Appends `name=value` to `raw_url`, preserving any `#fragment`.
pub fn append_query_param(raw_url: &str, name: &str, value: &str) -> String {
    let (base, fragment) = match raw_url.find('#') {
        Some(idx) => (&raw_url[..idx], &raw_url[idx..]),
        None => (raw_url, ""),
    };
    let sep = if base.ends_with('?') || base.ends_with('&') {
        ""
    } else if base.contains('?') {
        "&"
    } else {
        "?"
    };
    let encoded: String = form_urlencoded::Serializer::new(String::new())
        .append_pair(name, value)
        .finish();
    format!("{base}{sep}{encoded}{fragment}")
}

/// Replaces or inserts query parameter `name` with `value` in `raw_url`.
pub fn replace_query_param(raw_url: &str, name: &str, value: &str) -> String {
    let Ok(mut parsed) = Url::parse(raw_url) else {
        return raw_url.to_string();
    };
    let existing: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| k != name)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    {
        let mut pairs = parsed.query_pairs_mut();
        pairs.clear();
        for (k, v) in &existing {
            pairs.append_pair(k, v);
        }
        pairs.append_pair(name, value);
    }
    parsed.into()
}

fn new_runtime() -> Result<Runtime, QuickJsCipherError> {
    let rt = Runtime::new()?;
    rt.set_max_stack_size(QUICKJS_STACK_SIZE);
    Ok(rt)
}

fn capture_js_err(ctx: &rquickjs::Ctx, err: rquickjs::Error) -> QuickJsCipherError {
    match err {
        rquickjs::Error::Exception => {
            let exc = ctx.catch();
            let msg = exc
                .as_exception()
                .and_then(|e| e.message())
                .unwrap_or_else(|| "unknown JS exception".to_string());
            let stack = exc
                .as_exception()
                .and_then(|e| e.stack())
                .unwrap_or_default();
            if stack.is_empty() {
                QuickJsCipherError::JsException(msg)
            } else {
                QuickJsCipherError::JsException(format!("{msg}\n{stack}"))
            }
        }
        other => QuickJsCipherError::Runtime(other),
    }
}

fn eval_solver_calls(
    ctx: &rquickjs::Ctx,
    n: Option<&str>,
    sig: Option<&str>,
) -> Result<QuickJsSolveResult, QuickJsCipherError> {
    let run_kind = |kind: &str, input: Option<&str>| -> Result<Option<String>, QuickJsCipherError> {
        let Some(val) = input.filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let arg = serde_json::to_string(val)?;
        let expr = format!("_r.{kind}({arg})");
        let out: String = ctx.eval(expr).map_err(|e| capture_js_err(ctx, e))?;
        Ok(Some(out))
    };

    Ok(QuickJsSolveResult {
        n: run_kind("n", n)?,
        sig: run_kind("sig", sig)?,
    })
}

fn prune_stale_player_cache(cache_dir: &Path, current_id: &str) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let keep = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.contains(current_id));
        if !keep {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quickjs_basic_eval_and_bytecode_roundtrip() {
        let rt = new_runtime().expect("runtime");
        let ctx = Context::full(&rt).expect("context");
        ctx.with(|ctx| {
            let sum: i32 = ctx.eval("1 + 2").expect("eval");
            assert_eq!(sum, 3);
        });

        let fake_solver = "_result.n = function(v) { return v.split('').reverse().join(''); };\n\
                           _result.sig = function(v) { return 'ok_' + v; };";
        let bc = QuickJsCipher::compile(fake_solver).expect("compile");
        assert!(!bc.is_empty());

        let solved = QuickJsCipher::solve_bytecode(&bc, Some("abc"), Some("xyz")).expect("solve");
        assert_eq!(solved.n.as_deref(), Some("cba"));
        assert_eq!(solved.sig.as_deref(), Some("ok_xyz"));

        let direct = QuickJsCipher::solve(fake_solver, Some("123"), None).expect("solve direct");
        assert_eq!(direct.n.as_deref(), Some("321"));
        assert_eq!(direct.sig, None);
    }

    #[test]
    fn param_and_extraction_helpers_work() {
        assert_eq!(
            extract_player_id(r#"src="https://www.youtube.com/s/player/a1b2c3d4/player_ias.vflset/en_US/base.js""#),
            Some("a1b2c3d4".to_string())
        );
        assert_eq!(
            extract_signature_timestamp("var cfg={signatureTimestamp:20123,other:1};"),
            Some(20123)
        );
        assert_eq!(
            append_query_param("https://example.com/videoplayback?mime=audio#frag", "sig", "a b"),
            "https://example.com/videoplayback?mime=audio&sig=a+b#frag"
        );
        assert_eq!(
            replace_query_param("https://example.com/v?a=1&n=old&b=2", "n", "new"),
            "https://example.com/v?a=1&b=2&n=new"
        );
    }
}
