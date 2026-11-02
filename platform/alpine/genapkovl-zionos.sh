#!/bin/sh
set -eu
HOSTNAME="${1:-zionos}"
: "${ZION_STAGE:?ZION_STAGE must point to staging directory}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/usr/local/sbin" "$tmp/etc/network" "$tmp/etc/apk" "$tmp/etc/init.d" "$tmp/etc/runlevels/default" "$tmp/opt/zionos/bin" "$tmp/opt/zionos/web"
printf '%s\n' "$HOSTNAME" > "$tmp/etc/hostname"
cat > "$tmp/etc/network/interfaces" <<'NET'
auto lo
iface lo inet loopback

auto eth0
iface eth0 inet dhcp
NET
cat > "$tmp/etc/apk/world" <<'APK'
alpine-base
curl
ca-certificates
e2fsprogs
smartmontools
blkid
APK
# Keep Alpine package identity in /etc/os-release for compatibility.
# User-facing login branding is independent of the underlying package manager.
cat > "$tmp/etc/zionos-release" <<'RELEASE'
ZionOS Genesis 0.6.1 (AL 3.23; Linux LTS)
RELEASE
printf 'ZIONOS-LAB-0.6.1\n' > "$tmp/etc/zionos-lab"
printf 'Zion OS Genesis 0.6.1 (AL 3.23)\n\n' > "$tmp/etc/issue"
printf '\033[1;36mZION OS\033[0m  \033[1mGenesis 0.6.1\033[0m (AL 3.23 / Linux LTS)\n' > "$tmp/etc/motd"
cat >> "$tmp/etc/motd" <<'MOTD'
NAS Development Edition — Provisioning & Recovery Lab
Web: http://<NAS-IP>:8080  |  Backend: Alpine Linux 3.23
Storage: read-only HTTP API. Formatting disabled in Genesis 0.6.1; disposable QEMU lab only.
MOTD
cp /workspace/platform/alpine/zion-lab-provision "$tmp/usr/local/sbin/zion-lab-provision"
cp /workspace/platform/alpine/zion-lab-recover "$tmp/usr/local/sbin/zion-lab-recover"
cp /workspace/platform/alpine/zion-launch-contract "$tmp/usr/local/sbin/zion-launch-contract"
chmod 755 "$tmp/usr/local/sbin/zion-launch-contract"
chmod 755 "$tmp/usr/local/sbin/zion-lab-provision" "$tmp/usr/local/sbin/zion-lab-recover"
cp /workspace/platform/alpine/zion-boot-intro.initd "$tmp/etc/init.d/zion-boot-intro"
cp /workspace/platform/alpine/zion-network-info.initd "$tmp/etc/init.d/zion-network-info"
chmod 755 "$tmp/etc/init.d/zion-boot-intro" "$tmp/etc/init.d/zion-network-info"
mkdir -p "$tmp/etc/runlevels/boot"
ln -sf /etc/init.d/zion-boot-intro "$tmp/etc/runlevels/boot/zion-boot-intro"
ln -sf /etc/init.d/zion-network-info "$tmp/etc/runlevels/boot/zion-network-info"
cp "$ZION_STAGE/zion-core" "$tmp/opt/zionos/bin/zion-core"
cp "$ZION_STAGE/index.html" "$tmp/opt/zionos/web/index.html"
cp "$ZION_STAGE/app.js" "$tmp/opt/zionos/web/app.js"
cp "$ZION_STAGE/zion-core.initd" "$tmp/etc/init.d/zion-core"
chmod 755 "$tmp/opt/zionos/bin/zion-core" "$tmp/etc/init.d/zion-core"
ln -s /etc/init.d/zion-core "$tmp/etc/runlevels/default/zion-core"
for item in 'devfs sysinit' 'dmesg sysinit' 'mdev sysinit' 'hwdrivers sysinit' 'modloop sysinit' 'modules boot' 'sysctl boot' 'hostname boot' 'bootmisc boot' 'syslog boot' 'networking boot' 'mount-ro shutdown' 'killprocs shutdown'; do
  set -- $item
  mkdir -p "$tmp/etc/runlevels/$2"
  ln -sf "/etc/init.d/$1" "$tmp/etc/runlevels/$2/$1"
done
# The overlay is an archive of root paths, not an APK package.
# Keep one early-boot wrapper in the archived /usr/local/sbin directory.
mkdir -p "$tmp/usr/local/sbin" "$tmp/etc"
mkdir -p "$tmp/usr/local/lib/zionos"
cp /workspace/platform/alpine/boot-ui.sh "$tmp/usr/local/lib/zionos/boot-ui.sh"
cp /workspace/platform/alpine/zion-init-wrapper "$tmp/usr/local/sbin/zion-init-wrapper"
chmod 755 "$tmp/usr/local/sbin/zion-init-wrapper"

# BusyBox init executa sysinit, boot si default in ordine.
cat > "$tmp/etc/inittab" <<'ZION_EARLY_INITTAB'
::sysinit:/usr/local/sbin/zion-init-wrapper sysinit
::sysinit:/usr/local/sbin/zion-init-wrapper boot
::wait:/usr/local/sbin/zion-init-wrapper default

ttyS0::respawn:/sbin/getty 115200 ttyS0 vt100
tty1::respawn:/sbin/getty 38400 tty1 linux

::ctrlaltdel:/sbin/reboot
::shutdown:/sbin/openrc shutdown
ZION_EARLY_INITTAB

# Validare inainte de arhivare: daca lipseste wrapper-ul, build-ul trebuie sa esueze.
test -x "$tmp/usr/local/sbin/zion-init-wrapper" || {
    echo "ERROR: lipseste zion-init-wrapper" >&2
    exit 1
}
for executable in usr/local/sbin/zion-lab-provision usr/local/sbin/zion-lab-recover \
    opt/zionos/bin/zion-core etc/init.d/zion-core; do
    test -x "$tmp/$executable" || { echo "ERROR: missing executable $executable" >&2; exit 1; }
done
for asset in etc/inittab opt/zionos/web/index.html opt/zionos/web/app.js usr/local/lib/zionos/boot-ui.sh; do
    test -s "$tmp/$asset" || { echo "ERROR: missing or empty $asset" >&2; exit 1; }
done

tar -C "$tmp" -czf "$PWD/$HOSTNAME.apkovl.tar.gz" etc opt usr
