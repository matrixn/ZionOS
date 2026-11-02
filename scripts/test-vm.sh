#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG="$ROOT/build/qemu-smoke.log"
mkdir -p "$ROOT/build"
# No persistence: QEMU starts from ISO and the disposable virtual disk.
"$ROOT/scripts/run-qemu.sh" -display none > "$LOG" 2>&1 &
PID=$!
trap 'kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true' EXIT
for i in $(seq 1 120); do
  if curl -fsS --max-time 1 http://127.0.0.1:8080/api/health >/dev/null 2>&1; then
    "$ROOT/tests/smoke.sh"
    echo 'PASS: QEMU automated smoke tests'
    exit 0
  fi
  if ! kill -0 "$PID" 2>/dev/null; then
    echo 'QEMU exited before API became ready'; tail -60 "$LOG"; exit 1
  fi
  sleep 1
done
echo 'Timeout waiting for ZionOS API'; tail -60 "$LOG"; exit 1
