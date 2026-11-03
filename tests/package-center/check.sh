#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
case "${1:-}" in ''|--browser|--vm|--interactive) :;; *) echo 'Usage: bash tests/package-center/check.sh [--browser|--vm|--interactive]' >&2; exit 2;; esac
if [[ $# -gt 1 ]]; then exit 2; fi
mkdir -p build
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target}"
cargo fmt --manifest-path crates/zpk-control/Cargo.toml --check
cargo test --locked --manifest-path crates/zpk-control/Cargo.toml --all-features
cargo clippy --locked --manifest-path crates/zpk-control/Cargo.toml --all-targets --all-features -- -D warnings
node --check web/desktop/apps/package-center.js
node --check tests/package-center/browser.cjs
node --check tests/package-center/cards.cjs
node --check tests/package-center/session.cjs
node --check web/desktop/session.js
python3 tests/e2e/images_test.py
python3 tests/package-center/images_test.py
python3 -m py_compile tests/package-center/*.py
if [[ -z "${1:-}" ]]; then exit 0; fi
if [[ "$1" == --browser ]]; then node tests/package-center/session.cjs; node tests/package-center/cards.cjs; exit 0; fi
exec 9>build/.center-test.lock
flock -n 9 || { echo 'Another Package Center VM owns this fixture' >&2; exit 1; }
folder="$(mktemp -d "$ROOT/build/center-XXXXXX")"
python3 tests/e2e/fixtures.py "$folder"
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge build --project examples/zion-hello-rust --prepare-builder
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge package --project examples/zion-hello-rust --output "$folder/media/background.zpk"
RUSTFLAGS='-C target-cpu=x86-64' CFLAGS_x86_64_unknown_linux_musl='-march=x86-64 -mtune=generic' cargo build --locked --manifest-path crates/zpk-control/Cargo.toml --release --target x86_64-unknown-linux-musl --features vm-tests --bin zion-center-guest
python3 tests/package-center/vm.py "$folder" "$1"
