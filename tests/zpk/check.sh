#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target}"
cargo fmt --manifest-path crates/zpk-schema/Cargo.toml --check
cargo test --locked --manifest-path crates/zpk-schema/Cargo.toml
cargo clippy --locked --manifest-path crates/zpk-schema/Cargo.toml --all-targets -- -D warnings
bash -n examples/zpk-hello/build-package.sh
