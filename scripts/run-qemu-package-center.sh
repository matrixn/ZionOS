#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
if [[ $# == 0 ]]; then exec bash tests/package-center/check.sh --interactive; fi
if [[ $# == 2 && "$1" == --resume ]]; then
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target}"
  exec python3 tests/package-center/vm.py "$2" --resume
fi
echo 'Usage: bash scripts/run-qemu-package-center.sh [--resume build/center-XXXXXX]' >&2
exit 2
