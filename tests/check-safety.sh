#!/usr/bin/env bash
# Offline regression checks: regular files and temporary overlay fixtures only.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 "$ROOT/tests/test-safety.py"
