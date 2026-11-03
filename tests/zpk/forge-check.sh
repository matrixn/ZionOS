#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target}"
cargo fmt --manifest-path crates/zpk-forge/Cargo.toml --check
cargo fmt --manifest-path examples/zion-hello-rust/Cargo.toml --check
cargo test --locked --manifest-path crates/zpk-forge/Cargo.toml
cargo clippy --locked --manifest-path crates/zpk-forge/Cargo.toml --all-targets -- -D warnings
if [[ "${1:-}" == --docker ]]; then
  cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge build \
    --project examples/zion-hello-rust --prepare-builder
  cargo test --locked --manifest-path crates/zpk-forge/Cargo.toml --test forge -- --ignored --test-threads=1
elif [[ $# != 0 ]]; then
  echo 'Usage: bash tests/zpk/forge-check.sh [--docker]' >&2
  exit 2
fi
