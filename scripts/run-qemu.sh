#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO="${ZION_ISO:-$ROOT/build/iso/zionos-genesis-0.6.0-x86_64.iso}"
[ -f "$ISO" ] || { echo 'ISO missing. Run scripts/build-iso.sh or set ZION_ISO.'; exit 1; }
mkdir -p "$ROOT/build/disks"
[ -e "$ROOT/build/disks/test.qcow2" ] || qemu-img create -f qcow2 "$ROOT/build/disks/test.qcow2" 2G
exec qemu-system-x86_64 -m 2048 -smp 2 -cdrom "$ISO" \
  -drive "file=$ROOT/build/disks/test.qcow2,format=qcow2,if=virtio" \
  -nic user,model=e1000,hostfwd=tcp:127.0.0.1:8080-:8080 \
  -boot d -serial mon:stdio "${@}"
