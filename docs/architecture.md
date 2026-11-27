# Firebird Architecture

Firebird is structured as a Cargo workspace separating pure domain models from the Tauri v2 desktop runtime:

## Workspace Layout

- `crates/firebird-models/` — Pure serializable domain models (`app_env`, `app_info`, `app_language`, `artist_parser`, `canvas_clip`, `dac_device`, `equalizer_models`, `lastfm_home_models`, `lyrics_models`, `match_vocab`, `output_path_status`, `pcm_format`, `performance_controller`, `player_state`, `podcast_models`, `secrets`, `shared_providers`, `stream_models`) with zero runtime/platform dependencies.
- `src-tauri/` — Tauri v2 backend shell (`lastwave_lib`) organized into:
  - `src/storage/` — SQLite persistence, preferences, secure keyring store, local music library scanner, audio tagger, download manager, backup/export, diagnostics, snapshots.
  - `src/extractor/` — InnerTube API client, in-process `rquickjs` EJS cipher solver, ephemeral WebView2 BotGuard/PoToken broker, cookie auth store & session keeper, two-way YouTube Music library sync, local HTTP loopback streaming proxy.
  - `src/audio/` — Dual-deck `libmpv` playback engine, native Windows COM/WASAPI exclusive engine, SMTC/MPRIS OS media controls, 15-band EQ/AutoEQ/DSP chain, beat-aware AutoMix/DJ mix planner.
  - `src/integrations/` — 17-provider concurrent lyrics racer, Last.fm/ListenBrainz/Libre.fm scrobblers, Spotify integration, Jam WebSocket sync, Discord Rich Presence, Apple/Tidal/Community animated canvas, playlist importers, feed/home/search/stats, podcasts, updates.
  - `src/commands/` — `AppState` and typed `#[tauri::command]` IPC handlers wired into `src-tauri/src/lib.rs`.
