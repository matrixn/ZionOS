#!/bin/sh
# Runs only inside the pinned, cached builder image.
set -eu
cd /workspace
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' core/api/Cargo.toml)"
mkdir -p /cache/abuild
chmod 700 /cache/abuild
if [ ! -L /root/.abuild ]; then
    rmdir /root/.abuild 2>/dev/null || :
    test ! -e /root/.abuild || { echo 'Unexpected builder signing directory' >&2; exit 1; }
    ln -s /cache/abuild /root/.abuild
fi
if [ ! -s /root/.abuild/abuild.conf ]; then
    abuild-keygen -a -n >/dev/null 2>&1
fi
cp /root/.abuild/*.rsa.pub /etc/apk/keys/
# Kernel/initramfs cache includes every custom init input, builder and public key.
kernel_hash="$(python3 scripts/build-inputs.py kernel /workspace /root/.abuild "$ZION_BUILDER_ID")"
workdir="/cache/work/$kernel_hash"
outdir="/cache/iso-output/$kernel_hash"
mkdir -p "$workdir" "$outdir"
sh scripts/brand-initramfs.sh /usr/share/mkinitfs/initramfs-init \
    platform/alpine/initramfs-banner.sh /usr/share/mkinitfs/initramfs-init platform/alpine/boot-ui.sh
cp platform/alpine/mkimg.zionos.sh /opt/aports/scripts/mkimg.zionos.sh
export ZION_STAGE=/workspace/build/stage
mkdir -p build/cache-input build/iso
# mkimage's default apkovl cache hashes only the generator, not its copied inputs.
overlay_digest="$(python3 scripts/build-inputs.py overlay /workspace)"
cp platform/alpine/genapkovl-zionos.sh build/cache-input/genapkovl-zionos.sh
printf '\n# Staged input digest: %s\n' "$overlay_digest" >> build/cache-input/genapkovl-zionos.sh
chmod 755 build/cache-input/genapkovl-zionos.sh
export ZION_APKOVL_SCRIPT=build/cache-input/genapkovl-zionos.sh
echo "Builder cache: $kernel_hash"
/opt/aports/scripts/mkimage.sh --tag "$version" --arch x86_64 --profile zionos \
    --repository https://dl-cdn.alpinelinux.org/alpine/v3.23/main \
    --outdir "$outdir" --workdir "$workdir"
iso="$outdir/zionos-genesis-$version-x86_64.iso"
test -s "$iso"
checkdir="$(mktemp -d)"
trap 'rm -rf "$checkdir"' EXIT
xorriso -osirrox on -indev "$iso" -extract /boot/initramfs-lts "$checkdir/initramfs.gz"
gzip -dc "$checkdir/initramfs.gz" > "$checkdir/initramfs.cpio"
(cd "$checkdir" && cpio -i init ./init < initramfs.cpio)
test -x "$checkdir/init"
sh -n "$checkdir/init"
test "$(grep -c ZIONOS_INITRAMFS_BANNER "$checkdir/init")" -eq 1
# Publish only the validated image. Existing QEMU processes retain the old inode.
published="/workspace/build/iso/$(basename "$iso")"
python3 scripts/publish-iso.py "$iso" "$published"
iso="$published"
python3 - "$iso" "$version" "$kernel_hash" "$overlay_digest" "$ZION_BUILDER_ID" <<'PY'
import hashlib,json,pathlib,subprocess,sys
iso,version,kernel,overlay,builder=sys.argv[1:]
path=pathlib.Path(iso)
manifest={"version":version,"iso":path.name,"sha256":hashlib.file_digest(path.open('rb'),'sha256').hexdigest(),
          "kernel_cache_input":kernel,"overlay_input":overlay,"builder_image":builder,
          "aports_commit":subprocess.check_output(['git','-C','/opt/aports','rev-parse','HEAD'],text=True).strip(),
          "builder_packages":subprocess.check_output(['apk','info','-v'],text=True).splitlines(),
          "limits":"Package repositories are not immutable snapshots; this is provenance, not byte-for-byte reproducibility."}
path.with_suffix('.manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
PY
echo "PASS: final ISO /init verified; manifest written: $iso"
