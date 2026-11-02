#!/bin/sh
# Trusted disposable-VM fixture preparation, not a package-provided install hook.
set -eu
trap 'printf "\nZPK_RUNTIME_VM_DONE\n"' EXIT
test -d /mnt/runtime-fixtures
test "$(cat /sys/class/dmi/id/sys_vendor)" = QEMU
test "$(cat /etc/zionos-lab)" = ZIONOS-LAB-0.6.1
for disk in /sys/block/*; do
  case "${disk##*/}" in loop*|ram*|zram*|sr*|fd*) :;; *) echo 'Writable disk attached: refusing runtime fixture setup' >&2; exit 1;; esac
done
mkdir -p /usr/local/libexec
cp /mnt/runtime-fixtures/zpk-runtime /usr/local/libexec/zpk-runtime
chmod 755 /usr/local/libexec/zpk-runtime
/usr/local/libexec/zpk-runtime probe
# Match the example's deterministic identity; refuse arbitrary host execution.
account="$(sed -n 's/^name = "\(zp_[a-f0-9]*\)"/\1/p' /mnt/runtime-fixtures/runtime.toml)"
test -n "$account"
addgroup -g 23101 "$account"
adduser -D -H -u 23101 -G "$account" -s /sbin/nologin "$account"
app=/opt/zionos/apps/org.zionos.runtime-demo
mkdir -p "$app/versions/1.0.0/payload/bin" /etc/zionos/apps/org.zionos.runtime-demo
cp /mnt/runtime-fixtures/runtime-demo "$app/versions/1.0.0/payload/bin/runtime-demo"
chmod 755 "$app/versions/1.0.0/payload/bin/runtime-demo"
for item in manifest.toml checksums.json LICENSE; do
  cp "/mnt/runtime-fixtures/$item" "$app/versions/1.0.0/$item"
  chmod 644 "$app/versions/1.0.0/$item"
done
ln -s versions/1.0.0 "$app/current"
cp /mnt/runtime-fixtures/runtime.toml /etc/zionos/apps/org.zionos.runtime-demo/runtime.toml
chmod 600 /etc/zionos/apps/org.zionos.runtime-demo/runtime.toml
mkdir -p /var/lib/zionos/nas-fixture
echo 'fixture-must-remain-untouched' > /var/lib/zionos/nas-fixture/public.txt
chmod 666 /var/lib/zionos/nas-fixture/public.txt
/usr/local/libexec/zpk-runtime prepare --config /etc/zionos/apps/org.zionos.runtime-demo/runtime.toml
/mnt/runtime-fixtures/guest-check
echo ZPK_RUNTIME_VM_PASS
