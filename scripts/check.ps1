$ErrorActionPreference = "Stop"

cargo check --workspace
cargo test --workspace --lib
