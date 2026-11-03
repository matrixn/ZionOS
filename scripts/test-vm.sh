#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "${1:-}" == --zpk-e2e ]]; then
  shift
  exec bash "$ROOT/tests/e2e/check.sh" --vm "$@"
fi
runner="$ROOT/scripts/run-qemu.sh"
lab_mode=0
if [[ "${1:-}" == --lab ]]; then runner="$ROOT/scripts/run-qemu-lab.sh"; lab_mode=1; shift; fi
started=$SECONDS
LOG="$ROOT/build/qemu-smoke.log"
mkdir -p "$ROOT/build"
# Do not accidentally accept an API from an already running VM or dev server.
python3 - <<'PY'
import socket
with socket.socket() as probe:
    # Ignore closed connections in TIME_WAIT; an active listener still refuses bind.
    probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        probe.bind(("127.0.0.1", 8080))
    except OSError as error:
        raise SystemExit(f"Port 8080 is unavailable; stop its owner before testing: {error}")
PY
# No persistence: QEMU starts from ISO and the disposable virtual disk.
bash "$runner" -display none -snapshot "$@" > "$LOG" 2>&1 &
PID=$!
trap 'kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true' EXIT
for i in $(seq 1 120); do
  if grep -q 'Kernel panic - not syncing:' "$LOG"; then
    echo 'Kernel panic before API readiness'; tail -60 "$LOG"; exit 1
  fi
  if ! kill -0 "$PID" 2>/dev/null; then
    echo 'QEMU exited before API became ready'; tail -60 "$LOG"; exit 1
  fi
  if curl -fsS --max-time 1 http://127.0.0.1:8080/api/health >/dev/null 2>&1; then
    bash "$ROOT/tests/smoke.sh"
    if (( lab_mode )); then
      curl -fsS --max-time 8 http://127.0.0.1:8080/api/provisioning | python3 -c '
import json,sys
data=json.load(sys.stdin)
assert data["target_exists"] and data["inspection_ready"],data
assert data["execution_enabled"] is False and data["eligible_for_lab"] is False,data
print("PASS: fresh lab identity/topology inspected, execution disabled")
'
    fi
    kill -0 "$PID" || { echo 'QEMU exited during smoke tests'; exit 1; }
    python3 - "$LOG" <<'PY'
from pathlib import Path
import sys
log = Path(sys.argv[1]).read_text(errors="replace")
stages = ["[INITRAMFS]", "[INIT]", "lease of", "Starting ZionOS Core"]
positions = [log.index(stage) for stage in stages]
assert positions == sorted(positions), "Unexpected initramfs/OpenRC/DHCP/API boot order"
print("PASS: initramfs branding precedes OpenRC, DHCP and Zion Core")
assert "LIVE BOOT TELEMETRY" in log, "Animated console loader did not run"
assert "[BOOT COMPLETE] ready" in log, "Loader never verified Zion Core readiness"
assert "\x1b[?25l" in log and "\x1b[?25h" in log, "Missing loader cursor lifecycle"
assert "[BOOT ERROR]" not in log, "OpenRC phase failed; inspect the boot log"
print("PASS: animated loader, API-verified completion and restored cursor")
PY
    echo 'PASS: QEMU automated smoke tests'
    printf 'QEMU boot and smoke wall time: %s seconds\n' "$((SECONDS-started))"
    exit 0
  fi
  sleep 1
done
echo 'Timeout waiting for ZionOS API'; tail -60 "$LOG"; exit 1
