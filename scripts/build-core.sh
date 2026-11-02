#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
workspace_id="$(printf '%s' "$ROOT" | sha256sum | cut -c 1-12)"
# Keep Cargo's many small files on WSL's native filesystem; preserve explicit overrides.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/zionos/$workspace_id/cargo}"
cd "$ROOT/core/api"
build_started=$SECONDS
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release --target x86_64-unknown-linux-musl
mkdir -p "$ROOT/build/stage"
cp "$CARGO_TARGET_DIR/x86_64-unknown-linux-musl/release/zion-core" "$ROOT/build/stage/zion-core"
cp "$ROOT/web/desktop/index.html" "$ROOT/web/desktop/app.js" "$ROOT/build/stage/"
mkdir -p "$ROOT/build/stage/apps"
cp "$ROOT/web/desktop/apps/registry.js" "$ROOT/web/desktop/apps/bridge.js" "$ROOT/build/stage/apps/"
cp "$ROOT/platform/alpine/zion-core.initd" "$ROOT/build/stage/"
printf 'Core checks/build/staging: %s seconds (Cargo cache: %s)\n' "$((SECONDS-build_started))" "$CARGO_TARGET_DIR"
