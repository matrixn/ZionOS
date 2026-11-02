#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG="$ROOT/build/qemu-smoke.log"
mkdir -p "$ROOT/build"
# Do not accidentally accept an API from an already running VM or dev server.
python3 - <<'PY'
import socket
with socket.socket() as probe:
    try:
        probe.bind(("127.0.0.1", 8080))
    except OSError as error:
        raise SystemExit(f"Port 8080 is unavailable; stop its owner before testing: {error}")
PY
# No persistence: QEMU starts from ISO and the disposable virtual disk.
bash "$ROOT/scripts/run-qemu.sh" -display none -snapshot "$@" > "$LOG" 2>&1 &
PID=$!
trap 'kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true' EXIT
for i in $(seq 1 120); do
  if ! kill -0 "$PID" 2>/dev/null; then
    echo 'QEMU exited before API became ready'; tail -60 "$LOG"; exit 1
  fi
  if curl -fsS --max-time 1 http://127.0.0.1:8080/api/health >/dev/null 2>&1; then
    bash "$ROOT/tests/smoke.sh"
    kill -0 "$PID" || { echo 'QEMU exited during smoke tests'; exit 1; }
    python3 - "$LOG" <<'PY'
from pathlib import Path
import sys
log = Path(sys.argv[1]).read_text(errors="replace")
stages = ["[INITRAMFS]", "[INIT]", "lease of", "Starting ZionOS Core"]
positions = [log.index(stage) for stage in stages]
assert positions == sorted(positions), "Unexpected initramfs/OpenRC/DHCP/API boot order"
print("PASS: initramfs branding precedes OpenRC, DHCP and Zion Core")
PY
    echo 'PASS: QEMU automated smoke tests'
    exit 0
  fi
  sleep 1
done
echo 'Timeout waiting for ZionOS API'; tail -60 "$LOG"; exit 1
