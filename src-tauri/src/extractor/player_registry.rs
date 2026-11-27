use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;

const RECIPE_SOURCES: [&str; 2] = [
    "https://raw.githubusercontent.com/ZemerTeam/zemer-cipher/master/library/src/main/assets/player_configs.json",
    "https://raw.githubusercontent.com/MetrolistGroup/faraday/master/registry/player_configs.json",
];

const REFRESH_MIN_INTERVAL: Duration = Duration::from_secs(300);
const PLAYER_IIFE_CLOSE: &str = "})(_yt_player);";

/// Minimal browser-like global environment so player scripts initialize inside QuickJS.
const QUICKJS_BROWSER_SHIM: &str = r#"
var _yt_player = {};
var window = globalThis;
var self = globalThis;
var navigator = { userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36" };
var location = { href: "https://music.youtube.com/", hostname: "music.youtube.com", protocol: "https:", origin: "https://music.youtube.com", search: "" };
var document = {
  URL: location.href,
  location: location,
  cookie: "",
  createElement: function() { return { style: {}, setAttribute: function(){}, appendChild: function(){} }; },
  getElementsByTagName: function() { return []; },
  querySelector: function() { return null; }
};
var performance = { now: function() { return Date.now(); } };
"#;

/// Validated cipher invocation recipe for a specific YouTube player revision hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerRecipe {
    pub sts: Option<i64>,
    pub sig_call: String,
    pub n_ctor: String,
}

#[derive(Debug, Deserialize)]
struct RemoteRegistryPayload {
    #[serde(default)]
    players: HashMap<String, RemotePlayerEntry>,
}

#[derive(Debug, Deserialize)]
struct RemotePlayerEntry {
    sig: String,
    #[serde(rename = "nClass")]
    n_class: String,
    #[serde(default)]
    sts: Option<i64>,
    #[serde(default)]
    aliases: Vec<String>,
}

/// Manages per-player-hash cipher recipes fetched from community registries and cached on disk.
pub struct PlayerRegistry {
    client: Client,
    cache_file: PathBuf,
    entries: RwLock<HashMap<String, PlayerRecipe>>,
    last_refresh: RwLock<Option<Instant>>,
    refresh_guard: AsyncMutex<()>,
}

impl PlayerRegistry {
    pub fn new(client: Client, cache_dir: PathBuf) -> Self {
        let cache_file = cache_dir.join("player_recipes.json");
        let initial = load_cached_recipes(&cache_file).unwrap_or_default();
        Self {
            client,
            cache_file,
            entries: RwLock::new(initial),
            last_refresh: RwLock::new(None),
            refresh_guard: AsyncMutex::new(()),
        }
    }

    /// Looks up a recipe by player hash in memory.
    pub fn get(&self, player_id: &str) -> Option<PlayerRecipe> {
        self.entries
            .read()
            .ok()
            .and_then(|map| map.get(player_id).cloned())
    }

    /// Looks up a recipe by player hash, refreshing from remote registries if absent and
    /// outside the cooldown window.
    pub async fn resolve(&self, player_id: &str) -> Option<PlayerRecipe> {
        if let Some(found) = self.get(player_id) {
            return Some(found);
        }
        self.refresh_if_due(false).await;
        self.get(player_id)
    }

    /// Forces a refresh when cooldown permits (e.g. after a stream rejection).
    pub async fn refresh_if_due(&self, force: bool) {
        if !force && !self.cooldown_elapsed() {
            return;
        }
        let _lock = self.refresh_guard.lock().await;
        if !force && !self.cooldown_elapsed() {
            return;
        }

        let mut merged = self
            .entries
            .read()
            .map(|m| m.clone())
            .unwrap_or_default();

        let mut updated = false;
        for url in RECIPE_SOURCES {
            let Ok(resp) = self
                .client
                .get(url)
                .timeout(Duration::from_secs(10))
                .send()
                .await
            else {
                continue;
            };
            if !resp.status().is_success() {
                continue;
            }
            let Ok(body) = resp.text().await else {
                continue;
            };
            if merge_registry_json(&body, &mut merged) > 0 {
                updated = true;
            }
        }

        if let Ok(mut ts) = self.last_refresh.write() {
            *ts = Some(Instant::now());
        }

        if updated {
            if let Ok(mut guard) = self.entries.write() {
                *guard = merged.clone();
            }
            save_cached_recipes(&self.cache_file, &merged);
        }
    }

    fn cooldown_elapsed(&self) -> bool {
        self.last_refresh
            .read()
            .ok()
            .and_then(|g| *g)
            .map(|t| t.elapsed() >= REFRESH_MIN_INTERVAL)
            .unwrap_or(true)
    }
}

/// Validates and merges a remote registry JSON document into `out`.
/// Returns the number of valid player keys inserted or updated.
pub fn merge_registry_json(raw_json: &str, out: &mut HashMap<String, PlayerRecipe>) -> usize {
    let Ok(payload) = serde_json::from_str::<RemoteRegistryPayload>(raw_json) else {
        return 0;
    };

    let mut count = 0usize;
    for (hash, entry) in payload.players {
        if !is_valid_player_hash(&hash) || !is_valid_recipe(&entry.sig, &entry.n_class) {
            continue;
        }
        let recipe = PlayerRecipe {
            sts: entry.sts,
            sig_call: entry.sig,
            n_ctor: entry.n_class,
        };
        out.insert(hash, recipe.clone());
        count += 1;
        for alias in entry.aliases {
            if is_valid_player_hash(&alias) {
                out.insert(alias, recipe.clone());
                count += 1;
            }
        }
    }
    count
}

/// Validates that `sig_call` has the exact shape `ident(int,int,INPUT)` (or 1+ int args
/// followed by `INPUT`) and `n_ctor` is a short bare JS identifier.
pub fn is_valid_recipe(sig_call: &str, n_ctor: &str) -> bool {
    is_valid_js_ident(n_ctor) && is_valid_sig_template(sig_call)
}

fn is_valid_player_hash(hash: &str) -> bool {
    (6..=16).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_valid_js_ident(ident: &str) -> bool {
    let bytes = ident.as_bytes();
    if bytes.is_empty() || bytes.len() > 12 {
        return false;
    }
    let first_ok = bytes[0].is_ascii_alphabetic() || bytes[0] == b'_' || bytes[0] == b'$';
    first_ok
        && bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
}

fn is_valid_sig_template(sig: &str) -> bool {
    let Some((fn_name, rest)) = sig.split_once('(') else {
        return false;
    };
    let Some(arg_list) = rest.strip_suffix(')') else {
        return false;
    };
    if !is_valid_js_ident(fn_name) {
        return false;
    }
    let parts: Vec<&str> = arg_list.split(',').map(str::trim).collect();
    if parts.is_empty() || parts.len() > 4 {
        return false;
    }
    let mut saw_input = false;
    for part in parts {
        if part == "INPUT" {
            if saw_input {
                return false;
            }
            saw_input = true;
        } else if part.parse::<i64>().is_err() {
            return false;
        }
    }
    saw_input
}

/// Instruments a raw YouTube `base.js` script with QuickJS browser shims and closure exports
/// (`_result.sig` and `_result.n`) driven by a validated [`PlayerRecipe`].
pub fn instrument_player_script(base_js: &str, recipe: &PlayerRecipe) -> String {
    let sig_expr = recipe.sig_call.replace("INPUT", "v");
    let n_ctor = &recipe.n_ctor;

    let hook = format!(
        ";try{{_result.sig=function(v){{return {sig_expr};}};}}catch(_e){{}}\
         try{{_result.n=function(v){{\
           var u=new g.{n_ctor}('https://x.googlevideo.com/videoplayback?n='+v,true);\
           var t=u.get('n');return (t&&t!==v)?t:v;\
         }};}}catch(_e){{}}{PLAYER_IIFE_CLOSE}"
    );

    let body = match base_js.rfind(PLAYER_IIFE_CLOSE) {
        Some(idx) => {
            let mut spliced = String::with_capacity(base_js.len() + hook.len());
            spliced.push_str(&base_js[..idx]);
            spliced.push_str(&hook);
            spliced.push_str(&base_js[idx + PLAYER_IIFE_CLOSE.len()..]);
            spliced
        }
        None => format!("{base_js}\n{hook}"),
    };

    format!("{QUICKJS_BROWSER_SHIM}\n{body}")
}

fn load_cached_recipes(path: &PathBuf) -> Option<HashMap<String, PlayerRecipe>> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_cached_recipes(path: &PathBuf, recipes: &HashMap<String, PlayerRecipe>) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(recipes) {
        let _ = fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_validation_accepts_safe_shapes_and_rejects_injection() {
        assert!(is_valid_recipe("Ii(25,558,INPUT)", "as"));
        assert!(is_valid_recipe("$a(INPUT)", "_n1"));
        assert!(is_valid_recipe("fn1(10, INPUT, -3)", "URL"));

        assert!(!is_valid_recipe("eval(INPUT);alert(1)", "as"));
        assert!(!is_valid_recipe("Ii(25,558,INPUT)", "as;alert(1)"));
        assert!(!is_valid_recipe("Ii(25,558)", "as"));
        assert!(!is_valid_recipe("Ii(INPUT,INPUT)", "as"));
        assert!(!is_valid_recipe("1bad(INPUT)", "as"));
    }

    #[test]
    fn merge_registry_resolves_aliases_and_skips_invalid_entries() {
        let json = r#"{
            "players": {
                "341562bc": {
                    "sig": "Ii(25,558,INPUT)",
                    "nClass": "as",
                    "sts": 20670,
                    "aliases": ["a3fe4c92", "not-hex!"]
                },
                "deadbeef": {
                    "sig": "alert(1)",
                    "nClass": "as"
                }
            }
        }"#;
        let mut map = HashMap::new();
        let inserted = merge_registry_json(json, &mut map);
        assert_eq!(inserted, 2);
        assert_eq!(map.get("341562bc").and_then(|r| r.sts), Some(20670));
        assert_eq!(
            map.get("a3fe4c92").map(|r| r.sig_call.as_str()),
            Some("Ii(25,558,INPUT)")
        );
        assert!(!map.contains_key("deadbeef"));
        assert!(!map.contains_key("not-hex!"));
    }

    #[test]
    fn instrument_player_script_splices_inside_iife_closure() {
        let base_js = "var _yt_player={};(function(g){\
            function Ii(a,b,s){return s.split('').reverse().join('');}\
            g.as=function(url){this._n='solved_'+url.split('n=')[1];};\
            g.as.prototype.get=function(){return this._n;};\
        })(_yt_player);";
        let recipe = PlayerRecipe {
            sts: Some(20670),
            sig_call: "Ii(1,2,INPUT)".to_string(),
            n_ctor: "as".to_string(),
        };
        let out = instrument_player_script(base_js, &recipe);
        assert!(out.contains("_result.sig=function(v){return Ii(1,2,v);}"));
        assert!(out.contains("new g.as('https://x.googlevideo.com/videoplayback?n='+v,true)"));
        assert!(out.ends_with("})(_yt_player);"));
    }
}
