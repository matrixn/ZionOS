#!/bin/sh
# Build-time only: prepend a banner to Alpine's supplied init, preserving its body.
set -eu
[ "$#" -eq 4 ] || { echo "Usage: $0 upstream-init banner output-init boot-ui" >&2; exit 2; }
upstream=$1
banner=$2
output=$3
ui=$4
[ -s "$upstream" ] && [ -s "$banner" ] && [ -s "$ui" ] || { echo 'Missing init, banner or UI' >&2; exit 1; }
if grep -q ZIONOS_INITRAMFS_BANNER "$upstream"; then
    echo 'Init is already branded; refusing duplicate injection' >&2
    exit 1
fi
tmp="$(mktemp "${output}.XXXXXX")"
trap 'rm -f "$tmp"' EXIT HUP INT TERM
{
    IFS= read -r shebang
    [ "$shebang" = '#!/bin/sh' ] || { echo 'Unexpected Alpine init shebang' >&2; exit 1; }
    printf '%s\n' "$shebang"
    cat "$ui"
    printf '\n'
    cat "$banner"
    printf '\n'
    cat
} < "$upstream" > "$tmp"
sh -n "$tmp"
chmod 755 "$tmp"
mv "$tmp" "$output"
