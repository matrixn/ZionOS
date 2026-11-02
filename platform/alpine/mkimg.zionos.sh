profile_zionos() {
    profile_standard
    profile_abbrev="zion"
    title="ZionOS Genesis"
    desc="Read-only NAS hardware discovery demo"
    image_name="zionos-genesis"
    hostname="zionos"
    apkovl="${ZION_APKOVL_SCRIPT:?Set ZION_APKOVL_SCRIPT}"
    kernel_cmdline="console=tty0 console=ttyS0,115200"
    apks="$apks curl ca-certificates smartmontools blkid e2fsprogs"
}
