#!/bin/sh
# Trusted fixture only, never an install hook. The image is preformatted on a REGULAR FILE.
set -eu
trap 'printf "\nZPK_MANAGER_VM_DONE\n"' EXIT
test "$(cat /sys/class/dmi/id/sys_vendor)" = QEMU
test "$(cat /etc/zionos-lab)" = ZIONOS-LAB-0.6.1
modprobe virtio_pci
modprobe virtio_blk
mdev -s
test "$(cat /sys/block/vda/serial)" = ZPKMGRTEST01
for disk in /sys/block/*; do
  case "${disk##*/}" in loop*|ram*|zram*|sr*|fd*|vda) :;; *) echo 'Unexpected block device; refusing test setup' >&2; exit 1;; esac
done
test -d /mnt/zpk-manager-fixtures
mkdir -p /usr/local/bin /usr/local/libexec
cp /mnt/zpk-manager-fixtures/zpk /usr/local/bin/zpk
cp /mnt/zpk-manager-fixtures/zpk-runtime /usr/local/libexec/zpk-runtime
chmod 755 /usr/local/bin/zpk /usr/local/libexec/zpk-runtime
# The production CLI must refuse the live RAM target, before fixture mounts exist.
if /usr/local/bin/zpk list; then echo 'Ephemeral installation target was not refused' >&2; exit 1; fi
mkdir -p /mnt/zpk-manager-persistence
modprobe ext4
mount -t ext4 /dev/vda /mnt/zpk-manager-persistence
for mapping in 'apps:/opt/zionos/apps' 'config:/etc/zionos/apps' 'state:/var/lib/zionos/apps' 'logs:/var/log/zionos/apps' 'registry:/var/lib/zionos/packages'; do
  source=${mapping%%:*}; destination=${mapping#*:}
  mkdir -p "/mnt/zpk-manager-persistence/$source" "$destination"
  if test "$source" = registry; then chmod 700 "/mnt/zpk-manager-persistence/$source"; fi
  mount --bind "/mnt/zpk-manager-persistence/$source" "$destination"
done
phase=first
if test -f /mnt/zpk-manager-persistence/phase-one-done; then phase=second; fi
if test -f /mnt/zpk-manager-persistence/phase-two-done; then phase=third; fi
/mnt/zpk-manager-fixtures/guest-check "$phase"
sync
printf '\nZPK_MANAGER_VM_PASS_%s\n' "$phase"
