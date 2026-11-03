#!/bin/sh
# Trusted fixture bootstrap, not a package hook or a production persistence installer.
set -eu
test "$(cat /sys/class/dmi/id/sys_vendor)" = QEMU
test "$(cat /etc/zionos-lab)" = ZIONOS-LAB-0.6.1
modprobe virtio_pci
modprobe virtio_blk
mdev -s
test "$(cat /sys/block/vda/serial)" = ZPKE2ETEST01
for disk in /sys/block/*; do
  case "${disk##*/}" in loop*|ram*|zram*|sr*|fd*|vda) :;; *) echo 'Unexpected disk; refusing E2E mounts' >&2; exit 1;; esac
done
test -d /mnt/zion-e2e-fixtures
mkdir -p /usr/local/bin /usr/local/libexec /mnt/zion-e2e-persistence
cp /mnt/zion-e2e-fixtures/zpk /usr/local/bin/zpk
cp /mnt/zion-e2e-fixtures/zpk-runtime /usr/local/libexec/zpk-runtime
chmod 755 /usr/local/bin/zpk /usr/local/libexec/zpk-runtime
# The live RAM target must still be refused before persistent application mounts.
if zpk list; then echo 'Unsafe RAM target accepted' >&2; exit 1; fi
modprobe ext4
test "$(blkid -s LABEL -o value /dev/vda)" = ZPKE2ETEST
mount -t ext4 /dev/vda /mnt/zion-e2e-persistence
for mapping in 'apps:/opt/zionos/apps' 'config:/etc/zionos/apps' 'state:/var/lib/zionos/apps' 'logs:/var/log/zionos/apps' 'registry:/var/lib/zionos/packages'; do
  source=${mapping%%:*}; destination=${mapping#*:}
  mkdir -p "/mnt/zion-e2e-persistence/$source" "$destination"
  if test "$source" = registry; then chmod 700 "/mnt/zion-e2e-persistence/$source"; fi
  mount --bind "/mnt/zion-e2e-persistence/$source" "$destination"
done
printf '\nZION_E2E_MOUNTS_READY\n'
