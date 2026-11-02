# ZIONOS_INITRAMFS_BANNER
# Only shell builtins: /proc, /sys, /dev and the root filesystem are not ready yet.
if [ -t 1 ] && [ "${TERM:-linux}" != dumb ]; then
    printf '\033[2J\033[H'
    zion_boot_frame initramfs 1 0 ''
    printf '\n'
fi
printf '[INITRAMFS] ZION OS / preparing Alpine early userspace.\n'
