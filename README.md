# Firebird

Tauri v2 Rust backend for LastWave Desktop.

## Structure

- [`crates/firebird-models/`](crates/firebird-models) — Pure Rust domain models (`firebird-models` crate)
- [`src-tauri/`](src-tauri) — Tauri v2 backend (`storage`, `extractor`, `audio`, `integrations`, `commands`)
- [`docs/architecture.md`](docs/architecture.md) — Module & workspace overview
- [`scripts/`](scripts) — Workspace check & test scripts
- [`.github/workflows/ci.yml`](.github/workflows/ci.yml) — Cross-platform CI checks

## Development

```powershell
# Check & run all workspace unit tests
.\scripts\check.ps1
```
