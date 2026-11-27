use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_PIPE: usize = 10;
const MAX_FRAME_LEN: usize = 1024 * 1024;
const HEADER_SIZE: usize = 8;

pub const OP_HANDSHAKE: i32 = 0;
pub const OP_FRAME: i32 = 1;

#[cfg(target_os = "windows")]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
type Stream = tokio::net::UnixStream;

/// Minimal Discord IPC (Rich Presence) client for desktop.
///
/// Talks to the local Discord client directly: named pipes
/// (`\\.\pipe\discord-ipc-N`) on Windows, Unix domain sockets
/// (`discord-ipc-N` under `$XDG_RUNTIME_DIR`, `/run/user/*`, `/tmp`, ...) on
/// Linux/macOS. Every entry point is error-safe: failures return `false`/`None`
/// so callers degrade to silence.
pub struct DiscordIpc {
    #[cfg(any(target_os = "windows", unix))]
    stream: Option<Stream>,
    open: AtomicBool,
}

impl DiscordIpc {
    /// Named pipes (Windows) or Unix sockets (Linux/macOS) only.
    pub fn is_supported() -> bool {
        cfg!(any(
            target_os = "windows",
            target_os = "linux",
            target_os = "macos"
        ))
    }

    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    /// Unix socket search order per Discord spec: env runtime dirs,
    /// `/run/user/<uid>/*`, then `/tmp`. Never panics.
    pub fn socket_candidates() -> Vec<String> {
        let mut dirs: Vec<String> = Vec::new();
        let mut add = |dir: Option<String>| {
            if let Some(d) = dir {
                if !d.is_empty() && !dirs.contains(&d) {
                    dirs.push(d);
                }
            }
        };

        for key in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
            add(std::env::var(key).ok());
        }

        if let Ok(entries) = std::fs::read_dir("/run/user") {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        add(Some(entry.path().to_string_lossy().into_owned()));
                    }
                }
            }
        }
        add(Some("/tmp".to_string()));

        let mut out = Vec::with_capacity(dirs.len() * MAX_PIPE);
        for dir in &dirs {
            for i in 0..MAX_PIPE {
                out.push(format!("{dir}/discord-ipc-{i}"));
            }
        }
        out
    }

    /// Shared frame codec: int32LE opcode + int32LE length + JSON.
    pub fn encode_frame(opcode: i32, payload: &Value) -> Vec<u8> {
        let data = serde_json::to_vec(payload).unwrap_or_default();
        let len = data.len() as i32;
        let mut bytes = Vec::with_capacity(HEADER_SIZE + data.len());
        bytes.extend_from_slice(&opcode.to_le_bytes());
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(&data);
        bytes
    }

    pub fn decode_frame(body: &[u8]) -> Option<Value> {
        let decoded: Value = serde_json::from_slice(body).ok()?;
        if decoded.is_object() {
            Some(decoded)
        } else {
            None
        }
    }

    /// Connects to the first responding Discord endpoint and handshakes.
    /// Returns `None` when Discord isn't running (or vanished mid-handshake).
    pub async fn connect(client_id: &str) -> Option<Self> {
        #[cfg(target_os = "windows")]
        {
            for i in 0..MAX_PIPE {
                let name = format!(r"\\.\pipe\discord-ipc-{i}");
                if let Some(ipc) = Self::try_windows_pipe(&name, client_id).await {
                    return Some(ipc);
                }
            }
            None
        }
        #[cfg(all(not(target_os = "windows"), unix))]
        {
            for path in Self::socket_candidates() {
                if let Some(ipc) = Self::try_unix_socket(&path, client_id).await {
                    return Some(ipc);
                }
            }
            None
        }
        #[cfg(not(any(target_os = "windows", unix)))]
        {
            let _ = client_id;
            None
        }
    }

    #[cfg(target_os = "windows")]
    async fn try_windows_pipe(name: &str, client_id: &str) -> Option<Self> {
        use tokio::net::windows::named_pipe::ClientOptions;

        let mut client = ClientOptions::new().open(name).ok()?;
        let handshake = json!({ "v": 1, "client_id": client_id });
        if !write_frame(&mut client, OP_HANDSHAKE, &handshake, Duration::from_secs(2)).await {
            return None;
        }
        let reply = read_frame(&mut client, Duration::from_millis(2000)).await?;
        if reply.get("evt").and_then(Value::as_str) == Some("READY") {
            Some(Self {
                stream: Some(client),
                open: AtomicBool::new(true),
            })
        } else {
            None
        }
    }

    #[cfg(unix)]
    async fn try_unix_socket(path: &str, client_id: &str) -> Option<Self> {
        use tokio::net::UnixStream;

        let mut stream = tokio::time::timeout(Duration::from_secs(2), UnixStream::connect(path))
            .await
            .ok()?
            .ok()?;
        let handshake = json!({ "v": 1, "client_id": client_id });
        if !write_frame(&mut stream, OP_HANDSHAKE, &handshake, Duration::from_secs(2)).await {
            return None;
        }
        let reply = read_frame(&mut stream, Duration::from_secs(5)).await?;
        if reply.get("evt").and_then(Value::as_str) == Some("READY") {
            Some(Self {
                stream: Some(stream),
                open: AtomicBool::new(true),
            })
        } else {
            None
        }
    }

    /// Sends the activity. Returns `false` on any transport failure (caller
    /// should drop and reconnect).
    pub async fn set_activity(&mut self, activity: Value) -> bool {
        self.send_activity_payload(activity).await
    }

    /// Clears the activity. Best-effort.
    pub async fn clear_activity(&mut self) -> bool {
        self.send_activity_payload(Value::Null).await
    }

    async fn send_activity_payload(&mut self, activity: Value) -> bool {
        if !self.is_open() {
            return false;
        }
        #[cfg(any(target_os = "windows", unix))]
        {
            let Some(stream) = self.stream.as_mut() else {
                self.open.store(false, Ordering::Release);
                return false;
            };
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_micros())
                .unwrap_or(0)
                .to_string();
            let payload = json!({
                "cmd": "SET_ACTIVITY",
                "args": {
                    "pid": std::process::id(),
                    "activity": activity,
                },
                "nonce": nonce,
            });
            let ok = write_frame(stream, OP_FRAME, &payload, Duration::from_secs(2)).await;
            if !ok {
                self.close();
                return false;
            }
            drain_replies(stream, Duration::from_millis(300)).await;
            true
        }
        #[cfg(not(any(target_os = "windows", unix)))]
        {
            let _ = activity;
            false
        }
    }

    pub fn close(&mut self) {
        self.open.store(false, Ordering::Release);
        #[cfg(any(target_os = "windows", unix))]
        {
            self.stream.take();
        }
    }
}

async fn write_frame<S: AsyncWrite + Unpin>(
    stream: &mut S,
    opcode: i32,
    payload: &Value,
    budget: Duration,
) -> bool {
    let frame = DiscordIpc::encode_frame(opcode, payload);
    tokio::time::timeout(budget, async {
        stream.write_all(&frame).await?;
        stream.flush().await
    })
    .await
    .map(|res| res.is_ok())
    .unwrap_or(false)
}

async fn read_frame<S: AsyncRead + Unpin>(stream: &mut S, budget: Duration) -> Option<Value> {
    tokio::time::timeout(budget, async {
        let mut header = [0u8; HEADER_SIZE];
        stream.read_exact(&mut header).await.ok()?;
        let len = i32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if len < 0 || len as usize > MAX_FRAME_LEN {
            return None;
        }
        let mut body = vec![0u8; len as usize];
        stream.read_exact(&mut body).await.ok()?;
        DiscordIpc::decode_frame(&body)
    })
    .await
    .ok()
    .flatten()
}

/// Drains unread replies so the pipe/socket buffer never fills over time.
async fn drain_replies<S: AsyncRead + Unpin>(stream: &mut S, budget: Duration) {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        let step = (deadline - now).min(Duration::from_millis(50));
        if read_frame(stream, step).await.is_none() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_codec_roundtrip() {
        let payload = json!({ "v": 1, "client_id": "12345" });
        let frame = DiscordIpc::encode_frame(OP_HANDSHAKE, &payload);
        assert!(frame.len() > HEADER_SIZE);
        let op = i32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]);
        let len = i32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]) as usize;
        assert_eq!(op, OP_HANDSHAKE);
        assert_eq!(len, frame.len() - HEADER_SIZE);
        let decoded = DiscordIpc::decode_frame(&frame[HEADER_SIZE..]);
        assert_eq!(decoded, Some(payload));
        assert_eq!(DiscordIpc::decode_frame(b"[1,2,3]"), None);
        assert_eq!(DiscordIpc::decode_frame(b"not-json"), None);
    }

    #[test]
    fn socket_candidates_include_tmp_slots() {
        let candidates = DiscordIpc::socket_candidates();
        assert!(candidates.iter().any(|c| c == "/tmp/discord-ipc-0"));
        assert!(candidates.iter().any(|c| c == "/tmp/discord-ipc-9"));
    }
}
