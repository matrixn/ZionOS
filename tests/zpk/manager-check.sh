#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target}"
case "${1:-}" in ''|--vm) :;; *) echo 'Usage: bash tests/zpk/manager-check.sh [--vm]' >&2; exit 2;; esac
if [[ $# -gt 1 ]]; then exit 2; fi
if [[ "${1:-}" == --vm ]]; then
  mkdir -p build
  exec 9>build/.manager-test.lock
  flock -n 9 || { echo 'Another manager VM test owns the fixture media' >&2; exit 1; }
fi
cargo fmt --manifest-path crates/zpk-manager/Cargo.toml --check
cargo test --locked --manifest-path crates/zpk-manager/Cargo.toml
cargo clippy --locked --manifest-path crates/zpk-manager/Cargo.toml --all-targets -- -D warnings
sh -n tests/zpk/manager-guest.sh
python3 -m py_compile tests/zpk/manager-vm.py tests/zpk/manager-fixtures.py
if [[ "${1:-}" == --vm ]]; then
  test -s build/iso/zionos-genesis-0.6.1-x86_64.iso || { echo 'Build the Genesis ISO first' >&2; exit 1; }
  # Docker is used ONLY by Forge to compile owned source examples, never to run installed applications.
  RUSTFLAGS='-C target-cpu=x86-64' CFLAGS_x86_64_unknown_linux_musl='-march=x86-64 -mtune=generic' cargo build --locked --manifest-path crates/zpk-manager/Cargo.toml --release --target x86_64-unknown-linux-musl
  RUSTFLAGS='-C target-cpu=x86-64' cargo build --locked --manifest-path crates/zpk-runtime/Cargo.toml --release --target x86_64-unknown-linux-musl --bin zpk-runtime
  python3 tests/zpk/manager-fixtures.py
  xorriso -as mkisofs -R -J -o build/manager-fixtures.iso build/manager-fixtures
  python3 tests/zpk/manager-vm.py
fi
