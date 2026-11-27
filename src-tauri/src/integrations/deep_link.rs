use std::sync::{LazyLock, Mutex};

use regex::Regex;
use tokio::sync::broadcast;
use url::Url;

static JAM_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Z0-9]{3,12}$").expect("valid jam code regex"));

/// Room code carried by a `lastwave://jam?code=ABC123` link, upper-cased,
/// or `None` when `link` is anything else.
pub fn jam_code_from_link(link: &str) -> Option<String> {
    let uri = Url::parse(link.trim()).ok()?;
    if !uri.scheme().eq_ignore_ascii_case("lastwave") {
        return None;
    }
    let host = uri.host_str()?;
    if !host.eq_ignore_ascii_case("jam") {
        return None;
    }
    let raw_code = uri
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();
    let code = raw_code.trim().to_ascii_uppercase();
    if JAM_CODE_RE.is_match(&code) {
        Some(code)
    } else {
        None
    }
}

/// In-app route a `lastwave://` link opens, or `None` for unknown links.
pub fn route_for_link(link: &str) -> Option<String> {
    jam_code_from_link(link).map(|code| format!("/jam?code={code}"))
}

/// `lastwave://` links: the one in the launch arguments, plus links that a
/// second launch forwards to the running window. Links are never logged.
pub struct DeepLinks {
    initial: Mutex<Option<String>>,
    sender: broadcast::Sender<String>,
}

impl Default for DeepLinks {
    fn default() -> Self {
        Self::new()
    }
}

impl DeepLinks {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(32);
        Self {
            initial: Mutex::new(None),
            sender,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.sender.subscribe()
    }

    /// Forward a `lastwave://` link received from a second launch.
    pub fn push_link(&self, link: impl Into<String>) {
        let _ = self.sender.send(link.into());
    }

    /// The link this process was launched with, once.
    pub fn take_initial(&self) -> Option<String> {
        self.initial.lock().ok().and_then(|mut g| g.take())
    }

    pub fn start(&self, args: &[String]) {
        for arg in args {
            if route_for_link(arg).is_some() {
                if let Ok(mut guard) = self.initial.lock() {
                    *guard = Some(arg.clone());
                }
                break;
            }
        }
        #[cfg(target_os = "windows")]
        {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async {
                    register_windows_scheme().await;
                });
            }
        }
    }
}

/// Points `lastwave://` at this executable (per-user, no admin rights).
/// Best-effort: a failure only means links open nothing.
#[cfg(target_os = "windows")]
async fn register_windows_scheme() {
    use tokio::process::Command;

    let Ok(exe_path) = std::env::current_exe() else {
        return;
    };
    let exe = exe_path.to_string_lossy().to_string();
    const ROOT: &str = r"HKCU\Software\Classes\lastwave";
    const COMMAND_KEY: &str = r"HKCU\Software\Classes\lastwave\shell\open\command";
    let command = format!("\"{exe}\" \"%1\"");

    if let Ok(current) = Command::new("reg")
        .args(["query", COMMAND_KEY, "/ve"])
        .output()
        .await
    {
        if current.status.success() {
            let stdout = String::from_utf8_lossy(&current.stdout);
            if stdout.contains(&command) {
                return;
            }
        }
    }

    let _ = Command::new("reg")
        .args(["add", ROOT, "/ve", "/d", "URL:LastWave", "/f"])
        .output()
        .await;
    let _ = Command::new("reg")
        .args(["add", ROOT, "/v", "URL Protocol", "/d", "", "/f"])
        .output()
        .await;
    let _ = Command::new("reg")
        .args(["add", COMMAND_KEY, "/ve", "/d", &command, "/f"])
        .output()
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jam_code_valid_links() {
        assert_eq!(
            jam_code_from_link("lastwave://jam?code=abc123"),
            Some("ABC123".to_string())
        );
        assert_eq!(
            jam_code_from_link("  LASTWAVE://JAM?code=XYZ9  "),
            Some("XYZ9".to_string())
        );
        assert_eq!(
            jam_code_from_link("lastwave://jam?foo=1&code=123456789012"),
            Some("123456789012".to_string())
        );
    }

    #[test]
    fn jam_code_invalid_links() {
        assert_eq!(jam_code_from_link(""), None);
        assert_eq!(jam_code_from_link("https://jam?code=ABC123"), None);
        assert_eq!(jam_code_from_link("lastwave://other?code=ABC123"), None);
        assert_eq!(jam_code_from_link("lastwave://jam?code=AB"), None);
        assert_eq!(jam_code_from_link("lastwave://jam?code=ABCDEFGHIJKLM"), None);
        assert_eq!(jam_code_from_link("lastwave://jam?code=ABC-123"), None);
        assert_eq!(jam_code_from_link("lastwave://jam"), None);
    }

    #[test]
    fn route_for_valid_and_invalid_links() {
        assert_eq!(
            route_for_link("lastwave://jam?code=ab12"),
            Some("/jam?code=AB12".to_string())
        );
        assert_eq!(route_for_link("lastwave://jam?code=!!"), None);
    }

    #[test]
    fn deep_links_take_initial_once() {
        let dl = DeepLinks::new();
        let args = vec![
            "lastwave.exe".to_string(),
            "lastwave://jam?code=bad!".to_string(),
            "lastwave://jam?code=room42".to_string(),
        ];
        dl.start(&args);
        assert_eq!(
            dl.take_initial(),
            Some("lastwave://jam?code=room42".to_string())
        );
        assert_eq!(dl.take_initial(), None);
    }
}
