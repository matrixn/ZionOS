# TASK 00 verification record

2026-11-02. Prepared locally on `feat/zpk-foundation`, starting from main commit
`fbe4d99816729ddc5b445684e111b24d78bce747`. The remote main reference still matched
that commit during this task. Commit and push were subsequently authorized for
`feat/zpk-foundation`; no merge into main or tag is included.

## Implemented

- [x] `crates/zpk-schema`: typed versioned manifest, Serde roundtrip, strict TOML parsing,
  target and capability validation, bounded USTAR/zstd reader, SHA-256 inventory and CLI.
- [x] No root Cargo workspace; separate ZPK lockfile, no `zpk-common` without shared code.
  Core's source, Cargo.toml/Cargo.lock, staging, OpenRC, Alpine profile and launchers are unchanged.
- [x] `tests/zpk`: 23 regression tests, three complete valid feature manifests,
  eleven invalid manifest fixtures, adversarial archives constructed in memory.
- [x] `examples/zpk-hello`: real native musl/x86-64 source, manifest, license and builder.
- [x] `docs/zpk`: format, field reference, limits, trust boundaries and validation commands.
- [x] CI configuration adds a separate ZPK job, locked tests/Clippy and native example
  build/inspection; existing Genesis CI job remains present. README checklist updated.

## Executed locally

- [x] ZPK rustfmt and 23 tests, including deterministic header mutations, every compressed
  truncation of a valid test archive, malformed ELF/PNG/UI and byte/window/count limits.
- [x] ZPK Clippy `--all-targets -- -D warnings` with no warning suppressions.
- [x] Actual hello musl compilation, USTAR/zstd authoring and full CLI inspection,
  using caller-provided ZionOS 0.6.1. Payload was not executed.
- [x] Genesis rustfmt, all 33 Rust tests, strict Clippy and musl release/staging
  through the existing `scripts/build-core.sh`.
- [x] Existing shell syntax checks, all Alpine/OpenRC portable shell checks,
  `node --check` and all 3 existing JavaScript tests.
- [x] Existing Python safety suite: all 16 passed in WSL and all 16 passed in an
  ephemeral Alpine container with Bash added there. An earlier WSL run skipped
  the port fixture while a VM was using 8080; the final rerun had no skips.
- [x] Existing ISO build command completed in 22s using its cache, with final ISO
  `/init` extracted and validated. Image remains approximately 352 MiB.
- [x] Read-only API/assets smoke tests passed against the already running Genesis VM.
- [x] After the user stopped that VM, a fresh boot of the rebuilt ISO passed in
  42s with KVM: initramfs → OpenRC → DHCP → Core, loader ready/cursor lifecycle,
  health/metrics/storage/pools/provisioning, assets and POST refusal. Test VM stopped normally.
- [x] CI YAML parsed locally; both ZPK and existing Genesis jobs are present.

## Not executed / limitations

- [ ] Remote GitHub Actions: outside this pre-publication verification record;
  inspect the branch workflow result after push.
- [ ] Physical hardware, new Conroe/UEFI checks, graphical ZPK integration or execution.
- [ ] Installation, privileged operations, package execution, signature trust, sandbox
  enforcement, dependency solving, upgrades/migrations or rollback. None are implemented.

Generated packages, binaries, ISO and logs remain ignored under `build/`. Logs include
`zpk-final-checks.log`, `zpk-genesis-checks.log`, `zpk-example.log`, `zpk-inspection.json`,
`zpk-core-compat.log`, `zpk-iso-compat.log`, `zpk-qemu-compat.log`, `zpk-safety-wsl.log`
and `zpk-existing-vm.log`.
Library dependency outputs can be kept off NTFS using `CARGO_TARGET_DIR`; the ZPK
test runner defaults to `~/.cache/zionos/zpk-target` while preserving explicit overrides.

To repeat the boot check, run `bash scripts/test-vm.sh` with no other listener on 8080.
No ZPK code is included in the Genesis ISO by this task.
