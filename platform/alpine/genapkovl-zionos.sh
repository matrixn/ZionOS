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
ZionOS Genesis 0.6 (AL 3.23; Linux LTS)
RELEASE
printf 'ZionOS lab preview; never use on valuable physical disks.\n' > "$tmp/etc/zionos-lab"
printf 'Zion OS Genesis 0.6 (AL 3.23)\n\n' > "$tmp/etc/issue"
printf '\033[1;36mZION OS\033[0m  \033[1mGenesis 0.6\033[0m (AL 3.23 / Linux LTS)\n' > "$tmp/etc/motd"
cat >> "$tmp/etc/motd" <<'MOTD'
NAS Development Edition — Provisioning & Recovery Lab
Web: http://<NAS-IP>:8080  |  Backend: Alpine Linux 3.23
Storage: read-only HTTP API. Destructive CLI restricted to disposable QEMU vdb.
MOTD
cp /workspace/platform/alpine/zion-lab-provision "$tmp/usr/local/sbin/zion-lab-provision"
cp /workspace/platform/alpine/zion-lab-recover "$tmp/usr/local/sbin/zion-lab-recover"
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
# ZIONOS_EARLY_BOOT_FIXED_061
# Include wrapper-ul in /usr/local/sbin (directorul usr este deja in apkovl).
# Nu modificam /sbin din baza Alpine si nu avem nevoie de directorul sbin in tar.
mkdir -p "$tmp/usr/local/sbin" "$tmp/etc"
cat > "$tmp/usr/local/sbin/zion-init-wrapper" <<'ZION_EARLY_INIT'
#!/bin/sh
# Executat de BusyBox init inainte de OpenRC sysinit.
CYAN="$(printf '\033[1;36m')"
BLUE="$(printf '\033[1;34m')"
GREEN="$(printf '\033[1;32m')"
GRAY="$(printf '\033[0;37m')"
RESET="$(printf '\033[0m')"
printf '\n%s\n' "${CYAN}==========================================================${RESET}"
printf '%s\n' "${CYAN}  ZION OS  ${RESET}Genesis 0.6 (AL 3.23 / Linux $(uname -r))"
printf '%s\n' "${CYAN}==========================================================${RESET}"
printf '%s\n' "${BLUE}[INIT]${RESET} Pornim OpenRC: coordoneaza serviciile sistemului."
printf '%s\n' "${BLUE}[FILESYSTEM]${RESET} Pregatim directoarele si sistemele de fisiere."
printf '%s\n' "${BLUE}[HARDWARE]${RESET} Detectam dispozitivele si incarcam driverele."
printf '%s\n' "${BLUE}[NETWORK]${RESET} Activam reteaua si solicitam IP prin DHCP."
printf '%s\n' "${BLUE}[STORAGE]${RESET} Pregatim serviciile NAS si Zion Core."
printf '%s\n\n' "${GREEN}[BOOT]${RESET} Continuam initializarea standard Alpine/OpenRC."
exec /sbin/openrc sysinit
ZION_EARLY_INIT
chmod 755 "$tmp/usr/local/sbin/zion-init-wrapper"

# BusyBox init executa sysinit, boot si default in ordine.
cat > "$tmp/etc/inittab" <<'ZION_EARLY_INITTAB'
::sysinit:/usr/local/sbin/zion-init-wrapper
::sysinit:/sbin/openrc boot
::wait:/sbin/openrc default

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
# END_ZIONOS_EARLY_BOOT_FIXED_061

tar -C "$tmp" -czf "$PWD/$HOSTNAME.apkovl.tar.gz" etc opt usr


# ==========================================
# ZION OS - EARLY BOOT CONSOLE
# ==========================================

mkdir -p "$tmp/sbin"

cat > "$tmp/sbin/zion-init-wrapper" <<'ZION_INIT'
#!/bin/sh

CYAN="$(printf '\033[1;36m')"
GREEN="$(printf '\033[1;32m')"
YELLOW="$(printf '\033[1;33m')"
WHITE="$(printf '\033[1;37m')"
RESET="$(printf '\033[0m')"

printf '\n'
printf '%s\n' "${CYAN}============================================================${RESET}"
printf '%s\n' "${WHITE}  ZION OS - GENESIS 0.6${RESET}"
printf '%s\n' "  Alpine Linux 3.23 | Kernel $(uname -r) | $(uname -m)"
printf '%s\n' "${CYAN}============================================================${RESET}"
printf '\n'

printf '%s\n' "${GREEN}[SYSTEM]${RESET} Initializam ZionOS Boot Environment."
printf '%s\n' "${YELLOW}[INIT]${RESET} Pornim OpenRC pentru gestionarea serviciilor."
printf '%s\n' "${YELLOW}[FILESYSTEM]${RESET} Pregatim memoria si sistemele de fisiere."
printf '%s\n' "${YELLOW}[HARDWARE]${RESET} Initializam dispozitivele si driverele."
printf '%s\n' "${YELLOW}[NETWORK]${RESET} Configuram reteaua si conectivitatea."
printf '%s\n' "${YELLOW}[STORAGE]${RESET} Pregatim serviciile de stocare NAS."
printf '\n'

exec /sbin/openrc sysinit
ZION_INIT

chmod 755 "$tmp/sbin/zion-init-wrapper"

# BusyBox init: pastram toate cele trei etape
# OpenRC: sysinit -> boot -> default.

cat > "$tmp/etc/inittab" <<'ZION_INITTAB'
::sysinit:/sbin/zion-init-wrapper
::sysinit:/sbin/openrc boot
::wait:/sbin/openrc default

ttyS0::respawn:/sbin/getty 115200 ttyS0 vt100
tty1::respawn:/sbin/getty 38400 tty1 linux

::ctrlaltdel:/sbin/reboot
::shutdown:/sbin/openrc shutdown
ZION_INITTAB

