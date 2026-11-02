#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO="$(find "$ROOT/build/iso" -maxdepth 1 -name '*.iso' -print -quit)"
[ -n "$ISO" ] || { echo 'No ISO found. Run scripts/build-iso.sh'; exit 1; }
mkdir -p "$ROOT/build/disks"
[ -e "$ROOT/build/disks/test.qcow2" ] || qemu-img create -f qcow2 "$ROOT/build/disks/test.qcow2" 2G
qemu-system-x86_64 -m 2048 -smp 2 -cdrom "$ISO" \
  -drive "file=$ROOT/build/disks/test.qcow2,format=qcow2,if=virtio" \
  -nic user,model=e1000,hostfwd=tcp:127.0.0.1:8080-:8080 \
  -boot d -serial mon:stdio "${@}"
