//! Motion-artwork session lifecycle & state machine (no `media_kit_video`
//! widget layer — Tauri frontend renders the resolved URL/local file).
//!
//! Ported from `lib/core/artwork/animated_artwork_session.dart`.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::animated_artwork_service::CanvasDiskCache;

pub const ARTWORK_OUTPUT_PX: u32 = 1080;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkSessionState {
    pub output_width: u32,
    pub output_height: u32,
    pub url: String,
    pub fallback_url: String,
    pub opened_url: String,
    pub play_target: String,
    pub ready: bool,
    pub visible: bool,
    pub paused_for_background: bool,
    pub paused_for_playback: bool,
    pub refs: usize,
    pub generation: u64,
    pub disposed: bool,
}

impl Default for ArtworkSessionState {
    fn default() -> Self {
        Self {
            output_width: ARTWORK_OUTPUT_PX,
            output_height: ARTWORK_OUTPUT_PX,
            url: String::new(),
            fallback_url: String::new(),
            opened_url: String::new(),
            play_target: String::new(),
            ready: false,
            visible: false,
            paused_for_background: false,
            paused_for_playback: false,
            refs: 0,
            generation: 0,
            disposed: false,
        }
    }
}

impl ArtworkSessionState {
    pub fn should_pause(&self) -> bool {
        self.paused_for_background || self.paused_for_playback
    }

    pub fn is_ready_for(&self, url: &str) -> bool {
        !url.is_empty()
            && (url == self.url || url == self.opened_url)
            && self.ready
            && self.visible
    }
}

pub struct AnimatedArtworkSession {
    state: Arc<RwLock<ArtworkSessionState>>,
    http: reqwest::Client,
}

impl AnimatedArtworkSession {
    pub fn new(output_width: u32, output_height: u32, http: reqwest::Client) -> Self {
        Self {
            state: Arc::new(RwLock::new(ArtworkSessionState {
                output_width,
                output_height,
                ..Default::default()
            })),
            http,
        }
    }

    pub async fn state(&self) -> ArtworkSessionState {
        self.state.read().await.clone()
    }

    pub async fn is_ready_for(&self, url: &str) -> bool {
        self.state.read().await.is_ready_for(url)
    }

    pub async fn attach(&self, url: &str, fallback_url: &str) {
        {
            let mut st = self.state.write().await;
            if st.disposed {
                return;
            }
            st.refs += 1;
        }
        self.open(url, fallback_url).await;
    }

    pub async fn detach(&self) {
        let mut st = self.state.write().await;
        if st.refs > 0 {
            st.refs -= 1;
        }
    }

    pub async fn show_surface(&self) {
        let mut st = self.state.write().await;
        st.visible = true;
    }

    pub async fn hide_surface(&self) {
        let mut st = self.state.write().await;
        st.visible = false;
    }

    pub async fn set_backgrounded(&self, backgrounded: bool) {
        let mut st = self.state.write().await;
        if st.disposed {
            return;
        }
        st.paused_for_background = backgrounded;
    }

    pub async fn set_playback_paused(&self, paused: bool) {
        let mut st = self.state.write().await;
        if st.disposed {
            return;
        }
        st.paused_for_playback = paused;
    }

    /// Coalesces rapid skip bursts (250ms) and resolves local disk cache vs
    /// network URL before marking the target opened.
    pub async fn open(&self, url: &str, fallback_url: &str) {
        if url.is_empty() {
            return;
        }
        let gen = {
            let mut st = self.state.write().await;
            if st.disposed {
                return;
            }
            st.fallback_url = fallback_url.to_string();
            if url == st.url && !st.opened_url.is_empty() {
                if url != st.opened_url
                    && (st.fallback_url.is_empty() || st.fallback_url != st.opened_url)
                {
                    return;
                }
                return;
            }
            st.generation += 1;
            st.url = url.to_string();
            st.ready = false;
            st.generation
        };

        tokio::time::sleep(Duration::from_millis(250)).await;

        let cached_file = CanvasDiskCache::instance().peek_file_path(url);
        let play_target = cached_file.clone().unwrap_or_else(|| url.to_string());
        if cached_file.is_none() {
            let http = self.http.clone();
            let u = url.to_string();
            tokio::spawn(async move {
                let _ = CanvasDiskCache::instance().warm(&http, &u).await;
            });
        }

        let mut st = self.state.write().await;
        if st.disposed || st.generation != gen {
            return;
        }
        st.opened_url = url.to_string();
        st.play_target = play_target;
        st.ready = true;
    }

    /// Switches to the fallback URL if primary failed to decode.
    pub async fn fallback_on_error(&self) -> bool {
        let (alt, gen) = {
            let st = self.state.read().await;
            if st.disposed || st.ready || st.fallback_url.is_empty() || st.fallback_url == st.opened_url
            {
                return false;
            }
            (st.fallback_url.clone(), st.generation)
        };
        let mut st = self.state.write().await;
        if st.disposed || st.generation != gen {
            return false;
        }
        st.opened_url = alt.clone();
        st.play_target = alt;
        st.ready = true;
        true
    }

    pub async fn dispose(&self) {
        let mut st = self.state.write().await;
        st.disposed = true;
        st.generation += 1;
        st.ready = false;
        st.visible = false;
        st.refs = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lifecycle_attach_open_and_dispose() {
        let session = AnimatedArtworkSession::new(
            ARTWORK_OUTPUT_PX,
            ARTWORK_OUTPUT_PX,
            reqwest::Client::new(),
        );
        session.show_surface().await;
        session
            .attach("https://example.com/loop.mp4", "https://example.com/alt.mp4")
            .await;
        let st = session.state().await;
        assert!(st.ready);
        assert_eq!(st.refs, 1);
        assert!(session.is_ready_for("https://example.com/loop.mp4").await);

        session.set_backgrounded(true).await;
        assert!(session.state().await.should_pause());

        session.dispose().await;
        assert!(session.state().await.disposed);
        assert!(!session.is_ready_for("https://example.com/loop.mp4").await);
    }
}
