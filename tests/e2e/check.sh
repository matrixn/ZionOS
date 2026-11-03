#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
case "${1:-}" in ''|--packages|--vm|--legacy-vm|--interactive) :;; *) echo 'Usage: bash tests/e2e/check.sh [--packages|--vm|--legacy-vm|--interactive]' >&2; exit 2;; esac
if [[ $# -gt 1 ]]; then exit 2; fi
mkdir -p build
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target}"
exec 9>build/.e2e-test.lock
flock -n 9 || { echo 'Another E2E test owns the fixture inputs' >&2; exit 1; }
python3 tests/e2e/images_test.py
sh -n tests/e2e/guest.sh
node --check tests/e2e/desktop.cjs
python3 -m py_compile tests/e2e/*.py
bash tests/apps/check.sh
cargo fmt --manifest-path examples/zion-hello/Cargo.toml --check
cargo test --locked --manifest-path examples/zion-hello/Cargo.toml
cargo clippy --locked --manifest-path examples/zion-hello/Cargo.toml --all-targets -- -D warnings
if [[ -z "${1:-}" ]]; then exit 0; fi
folder="$(mktemp -d "$ROOT/build/e2e-XXXXXX")"
printf 'E2E evidence directory: %s\n' "$folder"
python3 tests/e2e/fixtures.py "$folder"
if [[ "$1" == --packages ]]; then exit 0; fi
test -s build/iso/zionos-genesis-0.6.1-x86_64.iso || { echo 'Build the final Genesis ISO first' >&2; exit 1; }
export RUSTFLAGS='-C target-cpu=x86-64'
export CFLAGS_x86_64_unknown_linux_musl='-march=x86-64 -mtune=generic'
cargo build --locked --manifest-path crates/zpk-manager/Cargo.toml --release --target x86_64-unknown-linux-musl --bin zpk --bin zpk-e2e-guest
cargo build --locked --manifest-path crates/zpk-runtime/Cargo.toml --release --target x86_64-unknown-linux-musl --bin zpk-runtime
python3 tests/e2e/vm.py "$folder" "$1"
