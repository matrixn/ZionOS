FROM alpine@sha256:ff7f0c727dd796aa36787bfcd9a64bf35a6ce72a73451b3359fde849b6703d63
RUN apk add --no-cache alpine-sdk abuild alpine-conf syslinux xorriso squashfs-tools \
    grub grub-efi mtools git openssl mkinitfs python3 \
 && git init /opt/aports \
 && git -C /opt/aports remote add origin https://gitlab.alpinelinux.org/alpine/aports.git \
 && git -C /opt/aports fetch --depth=1 origin 66c6fed56afee22bdf91a067c8129bfd6b946788 \
 && git -C /opt/aports checkout --detach FETCH_HEAD \
 && grep -Fq 'add --initdb --no-chown' /opt/aports/scripts/mkimage.sh \
 && sed -i 's|add --initdb --no-chown|add --initdb|g' /opt/aports/scripts/mkimage.sh
