#!/usr/bin/env bash
set -euo pipefail
BASE="${1:-http://127.0.0.1:8080}"
curl -fsS --max-time 8 "$BASE/api/health" | python3 -c '
import json,sys
j=json.load(sys.stdin)
assert j["status"]=="ok" and j["readonly"] is True
assert j["version"]=="0.6.0", j
'
curl -fsS --max-time 8 "$BASE/api/metrics" | python3 -c '
import json,sys
j=json.load(sys.stdin)
assert isinstance(j["disks"],list) and j["memory_total_bytes"]>0
'
curl -fsS --max-time 30 "$BASE/api/storage" | python3 -c '
import json,sys
j=json.load(sys.stdin)
assert j["mode"]=="read-only" and isinstance(j["disks"],list)
assert isinstance(j["mounts"],list)
'
curl -fsS --max-time 30 "$BASE/api/pools" | python3 -c '
import json,sys
j=json.load(sys.stdin)
assert j["version"]=="0.5.0" and j["mode"]=="read-only"
assert isinstance(j["pools"],list) and isinstance(j["volumes"],list) and isinstance(j["candidates"],list)
for p in j["pools"]:
 assert p["kind"] in ("mdraid","btrfs") and isinstance(p["members"],list),p
for v in j["volumes"]:
 assert v["capacity_basis"] in ("filesystem","partition"),v
for d in j["candidates"]:
 assert d["preview_only"] is True,d
'
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fsS --max-time 8 "$BASE/" -o "$TMP/index.html"
curl -fsS --max-time 8 "$BASE/app.js" -o "$TMP/app.js"
grep -q 'id="dock"' "$TMP/index.html"
grep -q 'data-pool-list' "$TMP/index.html"
grep -q 'data-plan-disks' "$TMP/index.html"
grep -q '/api/pools' "$TMP/app.js"
if command -v node >/dev/null 2>&1; then node --check "$TMP/app.js"; fi
curl -fsS --max-time 8 "$BASE/api/provisioning" | python3 -c '
import json,sys
j=json.load(sys.stdin)
assert j["version"]=="0.6.0" and j["mode"]=="read-only"
assert j["recovery"]["automatic_repairs"] is False
assert j["recovery"]["recovery_point_available"] is False
'
grep -q 'data-lab-recovery' "$TMP/index.html"
grep -q '/api/provisioning' "$TMP/app.js"
for route in health metrics storage pools provisioning; do
  status="$(curl -sS --max-time 8 -o /dev/null -w '%{http_code}' -X POST "$BASE/api/$route")"
  [ "$status" = 405 ] || { echo "Unexpected POST status for $route: $status"; exit 1; }
done
echo 'PASS: Genesis 0.6 health, metrics, storage, pools, provisioning read-only and recovery guidance' 
