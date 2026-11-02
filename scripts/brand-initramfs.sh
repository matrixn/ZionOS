#!/bin/sh
# Build-time only: prepend a banner to Alpine's supplied init, preserving its body.
set -eu
[ "$#" -eq 3 ] || { echo "Usage: $0 upstream-init banner output-init" >&2; exit 2; }
upstream=$1
banner=$2
output=$3
[ -s "$upstream" ] && [ -s "$banner" ] || { echo 'Missing init or banner' >&2; exit 1; }
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
    cat "$banner"
    printf '\n'
    cat
} < "$upstream" > "$tmp"
sh -n "$tmp"
chmod 755 "$tmp"
mv "$tmp" "$output"
