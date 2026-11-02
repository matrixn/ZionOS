#!/bin/sh
# Owned fixture on a dedicated disposable QEMU image. Never run on the host.
set -eu
trap 'printf "\nZION_SDK_VM_SETUP_DONE\n"' EXIT
test "$(cat /sys/class/dmi/id/sys_vendor)" = QEMU
test "$(cat /etc/zionos-lab)" = ZIONOS-LAB-0.6.1
modprobe virtio_pci
modprobe virtio_blk
mdev -s
test "$(cat /sys/block/vda/serial)" = ZPKSDKTEST01
for disk in /sys/block/*; do
  case "${disk##*/}" in loop*|ram*|zram*|sr*|fd*|vda) :;; *) echo 'Unexpected disk: refusing SDK setup' >&2; exit 1;; esac
done
test -d /mnt/zion-sdk-fixtures
mkdir -p /usr/local/bin /usr/local/libexec /mnt/zion-sdk-persistence
cp /mnt/zion-sdk-fixtures/zpk /usr/local/bin/zpk
cp /mnt/zion-sdk-fixtures/zpk-runtime /usr/local/libexec/zpk-runtime
chmod 755 /usr/local/bin/zpk /usr/local/libexec/zpk-runtime
mount -t ext4 /dev/vda /mnt/zion-sdk-persistence
for mapping in 'apps:/opt/zionos/apps' 'config:/etc/zionos/apps' 'state:/var/lib/zionos/apps' 'logs:/var/log/zionos/apps' 'registry:/var/lib/zionos/packages'; do
  source=${mapping%%:*}; destination=${mapping#*:}
  mkdir -p "/mnt/zion-sdk-persistence/$source" "$destination"
  if test "$source" = registry; then chmod 700 "/mnt/zion-sdk-persistence/$source"; fi
  mount --bind "/mnt/zion-sdk-persistence/$source" "$destination"
done
zpk install /mnt/zion-sdk-fixtures/zion-hello.zpk --trust-local-development \
  --permissions desktop-ui,notifications,background-service,persistent-data --start --unix-health
zpk status org.zionos.zion-hello > /tmp/zion-sdk-status.json
cat /tmp/zion-sdk-status.json
grep -q '"state": "running"' /tmp/zion-sdk-status.json
pid="$(sed -n 's/.*"pid": \([0-9]*\).*/\1/p' /tmp/zion-sdk-status.json)"
test -n "$pid"
uid="$(awk '/^Uid:/{print $2}' "/proc/$pid/status")"
test "$uid" -ge 10000
test "$(stat -c %u /var/lib/zionos/apps/org.zionos.zion-hello/state/starts)" = "$uid"
zpk logs org.zionos.zion-hello
printf '%s\n' "$pid" > /tmp/zion-sdk-native-pid
printf '\nZION_SDK_VM_READY\n'
