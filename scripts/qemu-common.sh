#!/usr/bin/env bash
# Shared fixed networking/drive configuration. Caller creates any lab disk itself.
launch_qemu() {
  local lab_path="$1" lab_serial="$2"
  shift 2
  local accel="${ZION_QEMU_ACCEL:-auto}" cpu='' display='' snapshot=0
  while (( $# )); do
    case "$1" in
      -display) (( $# >= 2 )) || return 2; display="$2"; shift 2;;
      -cpu) (( $# >= 2 )) || return 2; cpu="$2"; shift 2;;
      -accel) (( $# >= 2 )) || return 2; accel="$2"; shift 2;;
      -snapshot) snapshot=1; shift;;
      *) echo "Unsupported QEMU argument: $1 (network and drive overrides are forbidden)" >&2; return 2;;
    esac
  done
  [[ "$ROOT" != *,* ]] || { echo 'Comma in workspace path is unsupported' >&2; return 2; }
  [[ -z "$display" || "$display" =~ ^(none|gtk|sdl|curses)$ ]] || return 2
  [[ -z "$cpu" || "$cpu" =~ ^[a-zA-Z0-9_-]+$ ]] || return 2
  case "$accel" in auto|kvm|tcg) ;; *) echo 'ZION_QEMU_ACCEL must be auto, kvm or tcg' >&2; return 2;; esac
  local kvm=0
  if python3 - <<'PY'
import os,fcntl
try:
    fd=os.open('/dev/kvm',os.O_RDWR)
    try:
        if fcntl.ioctl(fd,0xAE00,0)!=12: raise OSError('unsupported KVM API')
        vm=fcntl.ioctl(fd,0xAE01,0)
        os.close(vm)
    finally: os.close(fd)
except OSError: raise SystemExit(1)
PY
  then kvm=1; fi
  if [[ "$accel" == auto ]]; then
    if (( kvm )); then accel=kvm; else accel=tcg; fi
  fi
  if [[ "$accel" == kvm ]] && (( ! kvm )); then
    echo 'KVM unavailable: check virtualization support and /dev/kvm group permissions, or select tcg' >&2
    return 1
  fi
  if [[ -z "$cpu" ]]; then
    if [[ "$accel" == kvm ]]; then cpu=host; else cpu=qemu64; fi
  fi
  local version iso disk
  version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/core/api/Cargo.toml")"
  iso="${ZION_ISO:-$ROOT/build/iso/zionos-genesis-$version-x86_64.iso}"
  [[ -f "$iso" ]] || { echo 'ISO missing; run scripts/build-iso.sh' >&2; return 1; }
  mkdir -p "$ROOT/build/disks"
  disk="$ROOT/build/disks/test.qcow2"
  [[ -e "$disk" ]] || qemu-img create -f qcow2 "$disk" 2G
  python3 "$ROOT/scripts/check-qcow2.py" "$disk" || return
  local -a args=(-accel "$accel" -cpu "$cpu" -m 2048 -smp 2 -cdrom "$iso"
    -drive "file=$disk,format=qcow2,if=none,id=zion-system"
    -device virtio-blk-pci,drive=zion-system,addr=0x4
    -nic user,model=e1000,hostfwd=tcp:127.0.0.1:8080-:8080
    -fw_cfg name=opt/zionos/network,string=QEMU-USERNET-LOCALHOST-V1
    -boot d -serial mon:stdio)
  if [[ -n "$lab_path" ]]; then
    [[ "$lab_serial" =~ ^zl-[a-f0-9]{16}$ ]] || return 2
    python3 "$ROOT/scripts/check-qcow2.py" "$lab_path" || return
    args+=(-drive "file=$lab_path,format=qcow2,if=none,id=zion-lab"
           -device "virtio-blk-pci,drive=zion-lab,serial=$lab_serial,addr=0x5"
           -fw_cfg "name=opt/zionos/lab,string=$lab_serial")
  fi
  [[ -z "$display" ]] || args+=(-display "$display")
  (( ! snapshot )) || args+=(-snapshot)
  printf 'ZionOS QEMU: accelerator=%s cpu=%s (host forwarding 127.0.0.1 only)\n' "$accel" "$cpu"
  exec qemu-system-x86_64 "${args[@]}"
}
