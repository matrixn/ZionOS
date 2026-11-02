#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

command -v docker >/dev/null || {
  echo "ERROR: Docker CLI missing. Enable Docker Desktop WSL integration."
  exit 1
}

for file in \
  "$ROOT/platform/alpine/mkimg.zionos.sh" \
  "$ROOT/platform/alpine/genapkovl-zionos.sh" \
  "$ROOT/platform/alpine/initramfs-banner.sh" \
  "$ROOT/platform/alpine/boot-ui.sh" \
  "$ROOT/platform/alpine/zion-init-wrapper" \
  "$ROOT/scripts/brand-initramfs.sh"; do
  if [[ ! -f "$file" ]]; then
    echo "ERROR: Required file missing: $file"
    exit 1
  fi
done

bash "$ROOT/scripts/build-core.sh"
mkdir -p "$ROOT/build/iso"

docker run --rm --privileged -i \
  -v "$ROOT:/workspace" \
  -w /workspace \
  alpine:3.23 sh -e <<'ALPINE_SCRIPT'

apk add --no-cache \
  alpine-sdk abuild alpine-conf \
  syslinux xorriso squashfs-tools \
  grub grub-efi mtools git openssl mkinitfs

# update-kernel calls the builder's mkinitfs, whose default /init comes from here.
# Change only this ephemeral container; preserve the upstream init body verbatim.
sh /workspace/scripts/brand-initramfs.sh \
  /usr/share/mkinitfs/initramfs-init \
  /workspace/platform/alpine/initramfs-banner.sh \
  /usr/share/mkinitfs/initramfs-init \
  /workspace/platform/alpine/boot-ui.sh

mkdir -p /root/.abuild /workspace/build/iso
abuild-keygen -a -n
cp /root/.abuild/*.rsa.pub /etc/apk/keys/

git clone --depth=1 --branch 3.23-stable \
  https://gitlab.alpinelinux.org/alpine/aports.git /tmp/aports

# Compatibility fix for Alpine 3.23 apk when running as root.
if grep -Fq -- 'add --initdb --no-chown' /tmp/aports/scripts/mkimage.sh; then
  sed -i 's|add --initdb --no-chown|add --initdb|g' \
    /tmp/aports/scripts/mkimage.sh
  echo "Applied Alpine mkimage compatibility patch."
fi

# mkimage loads mkimg.*.sh profiles from its own scripts directory.
cp /workspace/platform/alpine/mkimg.zionos.sh \
  /tmp/aports/scripts/mkimg.zionos.sh

grep -q '^profile_zionos()' /tmp/aports/scripts/mkimg.zionos.sh || {
  echo "ERROR: profile_zionos() is missing."
  exit 1
}

# apkovl must be relative to /workspace, because mkimage checksums it
# in its current working directory before executing it with fakeroot.
chmod +x /workspace/platform/alpine/genapkovl-zionos.sh
export ZION_STAGE=/workspace/build/stage
export ZION_APKOVL_SCRIPT=platform/alpine/genapkovl-zionos.sh

test -f "$ZION_APKOVL_SCRIPT" || {
  echo "ERROR: Overlay generator missing: $ZION_APKOVL_SCRIPT"
  exit 1
}

/tmp/aports/scripts/mkimage.sh --help | grep -w zionos >/dev/null || {
  echo "ERROR: mkimage did not register the zionos profile."
  exit 1
}

echo "ZionOS profile and overlay are ready."

/tmp/aports/scripts/mkimage.sh \
  --tag 0.6.0 \
  --arch x86_64 \
  --profile zionos \
  --repository https://dl-cdn.alpinelinux.org/alpine/v3.23/main \
  --outdir /workspace/build/iso \
  --workdir /tmp/zionos-mkimage

# Inspect the initramfs that was actually placed inside the resulting ISO.
iso=/workspace/build/iso/zionos-genesis-0.6.0-x86_64.iso
test -s "$iso"
checkdir="$(mktemp -d)"
trap 'rm -rf "$checkdir"' EXIT
xorriso -osirrox on -indev "$iso" \
  -extract /boot/initramfs-lts "$checkdir/initramfs.gz"
gzip -dc "$checkdir/initramfs.gz" > "$checkdir/initramfs.cpio"
(cd "$checkdir" && cpio -i init ./init < initramfs.cpio)
test -x "$checkdir/init"
sh -n "$checkdir/init"
test "$(grep -c ZIONOS_INITRAMFS_BANNER "$checkdir/init")" -eq 1
echo 'PASS: final ISO contains executable /init with early ZionOS branding.'

ALPINE_SCRIPT

shopt -s nullglob
isos=("$ROOT"/build/iso/*.iso)
if (( ${#isos[@]} == 0 )); then
  echo "ERROR: Build finished but no ISO was generated."
  exit 1
fi
printf 'ZionOS ISO generated:\n'
ls -lh "${isos[@]}"
