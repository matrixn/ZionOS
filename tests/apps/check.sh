#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
case "${1:-}" in ''|--browser|--vm) :;; *) echo 'Usage: bash tests/apps/check.sh [--browser|--vm]' >&2; exit 2;; esac
if [[ $# -gt 1 ]]; then exit 2; fi
mkdir -p build
# Serialize the shared fixture package/cache paths across browser and VM runs.
exec 9>build/.sdk-test.lock
flock -n 9 || { echo 'Another SDK test owns the fixture media' >&2; exit 1; }
node tests/apps/generated.cjs
node --check web/desktop/app.js
node --check web/desktop/apps/registry.js
node --check web/desktop/apps/bridge.js
node --input-type=module --check < examples/zion-hello/assets/hello.js
node --test tests/apps/*.test.cjs tests/desktop.test.cjs
if [[ -n "${1:-}" ]]; then
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target}"
  cargo build --locked --manifest-path crates/zpk-manager/Cargo.toml --bin zpk-sdk-fixture
  cargo build --locked --manifest-path core/api/Cargo.toml --bin zion-core
  export ZION_SDK_FIXTURE_BIN="$CARGO_TARGET_DIR/debug/zpk-sdk-fixture"
  export ZION_CORE_BIN="$CARGO_TARGET_DIR/debug/zion-core"
  # Only Forge's prepared isolated builder compiles the owned native source.
  cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge build --project examples/zion-hello
  package_dir="$(mktemp -d "$ROOT/build/sdk-package-XXXXXX")"
  export ZION_SDK_PACKAGE="$package_dir/zion-hello-1.0.0.zpk"
  cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge package --project examples/zion-hello --output "$ZION_SDK_PACKAGE"
  node tests/apps/browser.cjs
  if [[ "$1" == --vm ]]; then
    test -s build/iso/zionos-genesis-0.6.1-x86_64.iso || { echo 'Build the final Genesis ISO first: bash scripts/build-iso.sh' >&2; exit 1; }
    export RUSTFLAGS='-C target-cpu=x86-64'
    export CFLAGS_x86_64_unknown_linux_musl='-march=x86-64 -mtune=generic'
    cargo build --locked --manifest-path crates/zpk-manager/Cargo.toml --release --target x86_64-unknown-linux-musl --bin zpk
    cargo build --locked --manifest-path crates/zpk-runtime/Cargo.toml --release --target x86_64-unknown-linux-musl --bin zpk-runtime
    python3 tests/apps/vm.py
  fi
fi
