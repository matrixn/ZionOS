#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/qemu-common.sh"
mkdir -p "$ROOT/build/disks"
lab_directory="$(mktemp -d "$ROOT/build/disks/lab.XXXXXXXX")"
lab_serial="$(python3 -c 'import secrets; print("zl-"+secrets.token_hex(8))')"
lab_path="$lab_directory/vdb.qcow2"
qemu-img create -f qcow2 "$lab_path" 1G
printf 'Fresh disposable lab image: %s (never reused automatically)\n' "$lab_path"
launch_qemu "$lab_path" "$lab_serial" "$@"
