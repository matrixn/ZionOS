#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$ROOT/build/disks"
LAB="$ROOT/build/disks/zion-lab-vdb.qcow2"
if [[ ! -e "$LAB" ]]; then
  qemu-img create -f qcow2 "$LAB" 1G
fi
# run-qemu.sh creates vda. We add a separate disposable virtual vdb here.
# Never point this argument to a physical disk or an existing user image.
exec bash "$ROOT/scripts/run-qemu.sh" \
  -drive "file=$LAB,format=qcow2,if=virtio"
