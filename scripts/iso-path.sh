#!/bin/sh
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/core/api/Cargo.toml")"
test -n "$version"
printf '%s\n' "${ZION_ISO:-$root/build/iso/zionos-genesis-$version-x86_64.iso}"
