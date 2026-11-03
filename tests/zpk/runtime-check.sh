#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target}"
if [[ "${1:-}" == --vm ]]; then
  mkdir -p build
  exec 9>build/.runtime-test.lock
  flock -n 9 || { echo 'Another runtime VM test owns the fixture media' >&2; exit 1; }
fi
cargo fmt --manifest-path crates/zpk-runtime/Cargo.toml --check
cargo fmt --manifest-path crates/zpk-common/Cargo.toml --check
cargo test --locked --manifest-path crates/zpk-runtime/Cargo.toml
cargo clippy --locked --manifest-path crates/zpk-runtime/Cargo.toml --all-targets -- -D warnings
sh -n tests/zpk/runtime-guest.sh
if [[ "${1:-}" == --vm ]]; then
  test -s "$(sh "$ROOT/scripts/iso-path.sh")" || { echo 'Build the Genesis ISO first with bash scripts/build-iso.sh' >&2; exit 1; }
  RUSTFLAGS='-C target-cpu=x86-64' cargo build --locked --manifest-path crates/zpk-runtime/Cargo.toml --release --target x86_64-unknown-linux-musl
  python3 tests/zpk/runtime-fixtures.py
  xorriso -as mkisofs -R -J -o build/runtime-fixtures.iso build/runtime-fixtures
  python3 tests/zpk/runtime-vm.py
elif [[ $# != 0 ]]; then
  echo 'Usage: bash tests/zpk/runtime-check.sh [--vm]' >&2
  exit 2
fi
