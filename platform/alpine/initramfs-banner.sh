# ZIONOS_INITRAMFS_BANNER
# Only shell builtins: /proc, /sys, /dev and the root filesystem are not ready yet.
printf '\n\033[1;36m==========================================================\033[0m\n'
printf '\033[1;36m  ZION OS\033[0m  Genesis 0.6 | Alpine Linux 3.23\n'
printf '\033[1;36m==========================================================\033[0m\n'
printf '\033[1;34m[INITRAMFS]\033[0m Starting Alpine early userspace.\n'
printf '[BOOT] Discovering boot media and preparing the live root.\n'
printf '[BOOT] OpenRC, DHCP and Zion Core start after this stage.\n\n'
