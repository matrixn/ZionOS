#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
workspace_id="$(printf '%s' "$ROOT" | sha256sum | cut -c 1-12)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/zionos/$workspace_id/cargo}"
cargo fmt --manifest-path crates/zpk-control/Cargo.toml --check
cargo test --locked --manifest-path crates/zpk-control/Cargo.toml
cargo clippy --locked --manifest-path crates/zpk-control/Cargo.toml --all-targets -- -D warnings
export RUSTFLAGS='-C target-cpu=x86-64'
export CFLAGS_x86_64_unknown_linux_musl='-march=x86-64 -mtune=generic'
cargo build --locked --manifest-path crates/zpk-control/Cargo.toml --release --target x86_64-unknown-linux-musl --bin zion-platform
cargo build --locked --manifest-path crates/zpk-manager/Cargo.toml --release --target x86_64-unknown-linux-musl --bin zpk
cargo build --locked --manifest-path crates/zpk-runtime/Cargo.toml --release --target x86_64-unknown-linux-musl --bin zpk-runtime
mkdir -p build/stage/catalog
for name in zion-platform zpk zpk-runtime; do cp "$CARGO_TARGET_DIR/x86_64-unknown-linux-musl/release/$name" "build/stage/$name"; done
# Explicit trusted BUILD toolchain preparation. Installed apps never need Docker.
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge build --project examples/zion-hello --prepare-builder
catalog="$(mktemp -d "$ROOT/build/catalog-XXXXXX")"
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge package --project examples/zion-hello --output "$catalog/zion-hello-1.0.0.zpk"
cp "$catalog/zion-hello-1.0.0.zpk" build/stage/catalog/
