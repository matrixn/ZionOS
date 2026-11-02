#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
command -v docker >/dev/null || { echo 'Docker CLI required' >&2; exit 1; }
mkdir -p build/iso
exec 9>build/.iso-build.lock
flock -n 9 || { echo 'Another ISO build owns this workspace' >&2; exit 1; }
build_started=$SECONDS
bash scripts/build-core.sh
BUILDER=zionos-builder:0.6.1
docker build -t "$BUILDER" - < platform/alpine/Dockerfile.builder
builder_id="$(docker image inspect "$BUILDER" --format '{{.Id}}')"
workspace_id="$(printf '%s' "$ROOT" | sha256sum | cut -c 1-12)"
cache_volume="zionos-build-cache-$workspace_id"
docker volume create "$cache_volume" >/dev/null
docker run --rm --privileged -i \
  -v "$ROOT:/workspace" -v "$cache_volume:/cache" -w /workspace \
  -e "ZION_BUILDER_ID=$builder_id" "$BUILDER" sh scripts/build-iso-inner.sh
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' core/api/Cargo.toml)"
ls -lh "build/iso/zionos-genesis-$version-x86_64.iso"
printf 'ISO build wall time: %s seconds\n' "$((SECONDS-build_started))"
