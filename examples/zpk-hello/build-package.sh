#!/usr/bin/env bash
# Compile owned example source and package it; the inspector never executes it.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
mkdir -p build/zpk
rustc --edition 2021 --target x86_64-unknown-linux-musl -C target-cpu=x86-64 \
  -C opt-level=s -C strip=symbols -C panic=abort examples/zpk-hello/hello.rs -o build/zpk/hello
cargo run --locked --manifest-path crates/zpk-schema/Cargo.toml --example build-hello -- \
  build/zpk/hello "${1:-build/zpk/hello-1.0.0.zpk}"
