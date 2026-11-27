/// User-facing version string: shown in Settings -> About and written into
/// the diagnostics report, so both always agree.
///
/// Bump it with the release tag. (`pubspec.yaml` / `Cargo.toml` carries the
/// build number the installers use; this is the human-readable one.)
pub const APP_VERSION: &str = "2.6.0";
