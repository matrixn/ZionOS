#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/core/api"
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release --target x86_64-unknown-linux-musl
mkdir -p "$ROOT/build/stage"
cp target/x86_64-unknown-linux-musl/release/zion-core "$ROOT/build/stage/zion-core"
cp "$ROOT/web/desktop/index.html" "$ROOT/web/desktop/app.js" "$ROOT/build/stage/"
cp "$ROOT/platform/alpine/zion-core.initd" "$ROOT/build/stage/"
echo "Staged in $ROOT/build/stage"
