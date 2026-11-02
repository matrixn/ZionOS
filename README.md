# ZionOS Genesis 0.1 (prototype)

Read-only Alpine Linux ISO with OpenRC boot service, Rust/Axum API and static dashboard.

## Requirements
WSL Ubuntu with Rust musl target + musl-tools, Docker Desktop WSL integration, QEMU and curl.

```bash
sudo apt install -y musl-tools qemu-system-x86 qemu-utils curl python3
rustup target add x86_64-unknown-linux-musl
bash scripts/build-core.sh
bash scripts/build-iso.sh
bash scripts/run-qemu.sh
```

After boot, from second WSL terminal:
```bash
bash tests/smoke.sh
```
Browser: http://localhost:8080

## Notes
- ISO is live/diskless. No installer, persistent settings, partition changes or SMB yet.
- Port 8080 unauthenticated; run only in isolated QEMU via localhost forwarding.
- QEMU default DHCP is 10.0.2.15 and not reachable directly from Windows. Port forwarding is required.
- The initial ISO uses Alpine's kernel package, not necessarily Linux 6.12.
- Build command needs Docker with privileged mode to run Alpine image tools. Do not run untrusted build scripts.
- Image building can be sensitive to Alpine aports package/version changes. If it fails, retain the exact output for debugging.

Full automatic ISO build and QEMU smoke run:
```bash
bash scripts/build-iso.sh && bash scripts/test-vm.sh
```
