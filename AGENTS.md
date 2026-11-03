# ZionOS — instructions for coding agents

## Mission and current state
ZionOS is an experimental, bootable NAS appliance OS built on Alpine Linux 3.23 (OpenRC and an Alpine LTS kernel), a Rust/Axum `zion-core` service, and a browser-based desktop shell. Genesis 0.7 adds an authenticated local native Package Center with an explicitly configured persistent ext4 application volume. Disk/pool discovery stays read-only and the **restricted QEMU-only inspection lab** keeps provisioning `--apply` disabled pending a verified exclusive executor. It is **not production-ready**.

Read the actual source before modifying it. Do not rely solely on patch names, README milestones, or prior assumptions. Preserve working boot, DHCP, OpenRC and API behavior.

## Repository map
- `core/api/` — dedicated non-root Rust HTTP server; NAS APIs stay read-only. Package APIs enforce administrator sessions/CSRF and use a restricted Unix broker for effects.
- `crates/zpk-control/` — root package broker, private authentication/audit database, constrained IPC and target-only persistent-volume/OpenRC adapter. No formatter or arbitrary shell operations. VM test helpers are feature-gated and never shipped in the production ISO.
- `crates/zpk-schema/` — native package schema and bounded offline inspection; no extraction/execution.
- `crates/zpk-forge/` — developer build CLI; third-party compilation only inside its constrained Docker builder, never on the host. No installation or privileged hooks.
- `crates/zpk-runtime/` — native OpenRC runtime; no runtime containers. Host unit tests must use mock OpenRC operations and temporary roots. Run real identity/service/security tests only in the guarded disposable QEMU fixture.
- `crates/zpk-common/` — shared ELF inspection used by Forge and Runtime; no process execution.
- `crates/zpk-manager/` — local privileged package coordinator, SQLite registry/journal and Runtime adapter. Never run real installer/identity/service actions on the developer host. Unit tests use temporary roots and mock services; native integration uses only the guarded disposable persistent QEMU fixture.
- `web/desktop/` — lightweight operating-system-style browser desktop and assets.
- `platform/alpine/` — Alpine ISO profile, generated apk overlay, OpenRC startup configuration, lab utilities and branding.
- `scripts/` — staging, ISO build, QEMU and testing.
- `tests/` — smoke and storage safety tests.
- `build/` — generated outputs only; ignored by Git.

## Security and data-loss rules — mandatory
1. Never format, partition, wipe, mount read-write, repair or otherwise modify a physical block device for testing. Do not use host block-device passthrough.
2. For provisioning tests use only dedicated disposable QCOW2 images attached to QEMU; the lab tool inspects a session-identified virtual `/dev/vdb`. Keep `--apply` disabled in 0.6.1. Future erase execution requires a separately verified exclusive executor and explicit interactive erase confirmation.
3. Keep web storage endpoints read-only and provisioning disabled. Authenticated package mutations are a separate boundary: exact Origin/Host, administrator session, CSRF, dedicated Core peer UID and independent broker authorization are mandatory. Never add installer effects directly to the HTTP process or arbitrary privileged hooks.
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

For ZPK changes also run `bash tests/zpk/check.sh` and `bash tests/zpk/forge-check.sh`.
Use `bash tests/zpk/forge-check.sh --docker` for opt-in isolated native build/runtime tests
when a local Linux Docker daemon with cgroup v2 is available; never fall back to host compilation.

For runtime changes run `bash tests/zpk/runtime-check.sh` and, when equipped with
the Genesis ISO/QEMU/musl/xorriso, `bash tests/zpk/runtime-check.sh --vm`. Never invoke
the guest fixture shell or native runtime administration against the developer host.
Runtime grants must fail closed without required Linux controls; document rlimit vs
aggregate quota, declaration vs enforcement, and process-restart vs reboot persistence.

For manager changes run `bash tests/zpk/manager-check.sh` and, with a prepared Forge
builder/Genesis ISO/QEMU/image tools, `bash tests/zpk/manager-check.sh --vm`. Its only
writable drive is a newly created dedicated QCOW2 whose filesystem was formatted on
a regular file, never a physical block device. Keep production persistence refusal,
explicit development trust/permission review, journal reconciliation, data preservation
and the unauthenticated API boundary intact. Do not expose fault injection in the CLI.

## Working process
For App SDK/desktop changes read `docs/zpk/APP_SDK.md` and run
`bash tests/apps/check.sh`, then `--browser` with Chromium and the prepared Forge
builder. For final ISO integration run `bash scripts/build-iso.sh` followed by
`bash tests/apps/check.sh --vm`. Never invoke its guest helper on the host. Keep
opaque iframes, source/session validation and the read-only Core boundary intact.
Do not claim CSP prevents all network egress, or replace typed unavailable file/settings
APIs with fake persistence. Package Center has a dedicated authenticated package broker;
it does not grant general native file/settings capabilities to application frames.

For full native lifecycle integration read `docs/zpk/E2E.md`. Run
`bash tests/e2e/check.sh` (image guards, SDK and first-party settings tests),
`--packages` (three Forge-built versions), and `bash scripts/test-vm.sh --zpk-e2e`
(three clean boots of one persistent QCOW2). `--legacy-vm` uses TCG/Conroe.
Default Genesis smoke tests must retain `-snapshot`; persistence tests must never
use it. E2E filesystem creation only targets freshly/exclusively created regular
files inside `build/`, pinned by descriptors. Never reuse/format a user image.
The guarded E2E helper/fault injection is a development fixture, not deployed in
the production ISO or exposed through Manager's CLI/HTTP. Native sample settings
use its private Unix socket/state; SDK settings remain unavailable until an
application capability broker exists. Interactive tests accept documented local commands only.

For Genesis 0.7 read `docs/GENESIS_07_ARCHITECTURE.md` and `docs/GENESIS_07.md`.
Run `bash tests/package-center/check.sh`, the standard Core/ZPK/SDK suites, then
build the ISO and run `bash tests/package-center/check.sh --vm`. This exercises
the production boot adapter, auth, broker and GUI across persistent QEMU reboots.
Use `bash scripts/run-qemu-package-center.sh` for interactive testing; `--resume`
must reuse only its identity-checked ignored build directory/image, never format it.
`ZION_CENTER_LEGACY=1` selects TCG/Conroe for this runner. The original smoke runner
retains snapshot mode. Tests must not overlap on localhost port 8080.
Physical disks are forbidden for testing; fresh image preparation requires exclusive
regular files, descriptor pinning, build-directory containment and owner/link checks.
Production volume boot only selects the configured UUID and validates before mounting;
missing/invalid/corrupt storage must degrade, never install into RAM or auto-repair.
Keep local bootstrap secrets out of logs/screenshots/commits. No default password.
LAN/TLS/Secure-cookie/proxy administration is unsupported; do not broaden the local
network contract without implementing secure transport. Report enforced controls
separately from planned quotas, app capability brokering and publisher authentication.

- First summarize what the affected code currently does, identify risks, and propose a limited change.
- Make focused edits and run available verification.
- Report changed files, tests passed/failed/not run, remaining hazards and any manual QEMU validation needed.
- Keep README and inline docs consistent with actual behavior.
- Maintain a cumulative feature checklist in README and keep the audit/roadmap and component READMEs current. Distinguish implemented features from executed verification.
- When commit/push is authorized, publish each coherent batch after its applicable tests and build pass, including the documentation updates. Keep generated artifacts out of Git; do not merge automatically. End the task with a change summary, verification limits, and exact build/run/test commands.
