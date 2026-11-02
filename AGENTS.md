# ZionOS — instructions for coding agents

## Mission and current state
ZionOS is an experimental, bootable NAS appliance OS built on Alpine Linux 3.23 (OpenRC and an Alpine LTS kernel), a Rust/Axum `zion-core` service, and a browser-based desktop shell. Genesis 0.6 includes read-only disk and pool discovery and a **restricted QEMU-only provisioning lab**. It is **not production-ready**.

Read the actual source before modifying it. Do not rely solely on patch names, README milestones, or prior assumptions. Preserve working boot, DHCP, OpenRC and API behavior.

## Repository map
- `core/api/` — Rust HTTP server, metrics, storage and provisioning API; current web APIs are not authenticated.
- `web/desktop/` — lightweight operating-system-style browser desktop and assets.
- `platform/alpine/` — Alpine ISO profile, generated apk overlay, OpenRC startup configuration, lab utilities and branding.
- `scripts/` — staging, ISO build, QEMU and testing.
- `tests/` — smoke and storage safety tests.
- `build/` — generated outputs only; ignored by Git.

## Security and data-loss rules — mandatory
1. Never format, partition, wipe, mount read-write, repair or otherwise modify a physical block device for testing. Do not use host block-device passthrough.
2. For provisioning tests use only dedicated disposable QCOW2 images attached to QEMU; the existing lab tool targets a verified virtual `/dev/vdb`, with an explicit interactive erase confirmation.
3. Keep web storage endpoints read-only. Do not add destructive HTTP routes while authentication, authorization, audited operations and recovery guarantees are missing.
4. Do not expose the unauthenticated API to the public network; local QEMU forwarding must bind the host side to `127.0.0.1`.
5. Never commit credentials, signing keys, `.env` files, disk images, ISO outputs, backups or build logs. Never print secrets to logs.
6. Do not modify boot/init (`/etc/inittab`, OpenRC runlevels, initramfs, Alpine profile) without validating the final boot and API health in QEMU or clearly documenting that it was not verified.
7. Do not force-push, delete repository history, or publish code without explicit approval.

## Coding standards
- Rust: format with rustfmt; Clippy must pass with `-D warnings`. Fix warnings rather than globally suppressing them.
- Shell: portable `/bin/sh` for Alpine scripts; Bash only for files explicitly using `#!/usr/bin/env bash`. Use `set -eu` / `set -euo pipefail` as appropriate.
- Preserve legacy x86-64 CPU compatibility (e.g. Core 2 Duo); do not require AVX/SSE4.2 for the baseline build.
- Avoid heavy always-on services and internet-dependent UI libraries. Keep the desktop responsive on low-resource NAS hardware.
- Prefer small, reviewed changes with unit tests and informative error messages. Do not claim a function is implemented unless it is tested.
- Do not silently change architecture, kernel version, storage on-disk formats or licensing model.

## Standard checks
Run from repository root when available:
```sh
cargo fmt --manifest-path core/api/Cargo.toml --check
cargo test --manifest-path core/api/Cargo.toml
cargo clippy --manifest-path core/api/Cargo.toml --all-targets -- -D warnings
bash scripts/build-core.sh
```
Also run `bash -n` for Bash scripts, `sh -n` for portable shell files, and `node --check web/desktop/app.js` when applicable. For storage or boot changes, run `bash tests/check-safety.sh` if present and, in a properly equipped local WSL/Docker/QEMU environment, `bash scripts/build-iso.sh` and `bash scripts/test-vm.sh`.

Do not claim ISO/QEMU tests passed when those tools are unavailable. Do not execute destructive lab commands in CI.

## Working process
- First summarize what the affected code currently does, identify risks, and propose a limited change.
- Make focused edits and run available verification.
- Report changed files, tests passed/failed/not run, remaining hazards and any manual QEMU validation needed.
- Keep README and inline docs consistent with actual behavior.
- Maintain a cumulative feature checklist in README and keep the audit/roadmap and component READMEs current. Distinguish implemented features from executed verification.
- When commit/push is authorized, publish each coherent batch after its applicable tests and build pass, including the documentation updates. Keep generated artifacts out of Git; do not merge automatically. End the task with a change summary, verification limits, and exact build/run/test commands.
