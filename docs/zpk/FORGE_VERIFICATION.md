# TASK 01 — Forge verification

2026-11-02. Branch `feat/zpk-forge`, based on Foundation `0ccd091` (main remains
Genesis `fbe4d99`). Commit/push authorized after validation; no merge or force push.

## Changes

- [x] `crates/zpk-forge`: separate locked Rust CLI/library, strict authoring recipe,
  pure project detection, source allowlist/snapshot, constrained Alpine builder,
  safe staging reader, offline deterministic packaging, ELF dependency closure,
  provenance, inspection/validation and guarded cleanup.
- [x] Rust/Cargo, C/C++ CMake, pure Go and declarative Make adapters are implemented
  and tested with actual compilers. No Zig/Meson/Autotools or CGO placeholder support.
- [x] `examples/zion-hello-rust` and `examples/zion-hello-c`: MIT source, complete
  zpk.toml recipes, native payloads produced in ignored `.zpk-forge` state.
- [x] `tests/zpk/forge.rs`, `forge-check.sh`, separate Forge CI job; existing Foundation
  and Genesis jobs retained. No root workspace or Core lockfile migration.
- [x] Shared schema adds `build.system=go` and `+` in canonical file names to represent
  actual C++ libraries (`libstdc++.so.6`). Existing manifests/archives remain accepted;
  strict unknown fields, target limits and hash validation remain enforced.
- [x] README checklist, package format, agent map and developer/component docs updated.

## Executed

- [x] Forge rustfmt, **16 offline tests**, strict Clippy `--all-targets -- -D warnings`.
- [x] **2 Docker integration cases**, actually executed after explicit trusted setup:
  five language/adapter variants, two complete builds each, byte-for-byte archive
  equality, native execution in separate minimal Alpine without build toolchains,
  transitive C++ libraries, missing-library refusal, missing compiler, failed source
  build and missing staging output. No application was executed on the host.
- [x] All four supported adapters use the same tested unprivileged/networkless
  resource-constrained environment. Driver checks seccomp, UID/caps, root mount,
  network and cgroup v2 limits before source execution.
- [x] Real Rust and C example build/package commands completed; final artifacts and
  provenance exist locally. Source fingerprints include content and normalized mode.
- [x] Foundation: **23 tests**, rustfmt and strict Clippy.
- [x] Genesis: **33 Rust tests**, rustfmt, strict Clippy, release musl/staging.
- [x] Bash/Alpine/OpenRC syntax, **16 shell/Python fixtures**, **3 JavaScript tests**,
  active desktop syntax. No skipped legacy fixture in the final run.
- [x] Existing ISO builder completed in **22s** using cache; final `/init` validated.
- [x] Fresh **KVM QEMU boot/smoke in 39s**: early branding/OpenRC/DHCP/Core order,
  loader completion/cursor restoration, API/assets and POST refusal. No test VM left running.
- [x] CI YAML parsed locally, with Forge/Foundation/Genesis jobs present.
- [ ] Remote GitHub Actions is not part of this pre-publication record; report the
  run after branch publication separately.

Local logs: `build/forge-final-checks.log`, `forge-foundation-checks.log`,
`forge-core-checks.log`, `forge-genesis-checks.log`, `forge-iso-compat.log`,
`forge-qemu-compat.log`. Generated logs, packages, provenance and images are excluded
from Git. Forge is not staged in the ISO and changes no OpenRC, API or storage behavior.

## Limits and unsupported cases

- No package installation, root hooks, HTTP writes, runtime Docker dependency,
  trusted publisher signing, package database, upgrade execution or rollback.
- Only x86-64 musl. aarch64/i686/ARMv7 adapters are explicit unsupported descriptors;
  no tested cross toolchains. Go CGO, Zig/Meson/Autotools are unsupported.
- Complete AVX/SSE4.2/control-flow verification and physical Core 2 Duo certification
  are not claimed. Build flags/ELF notes are enforced/checked where available and the
  limitation is printed at every build/package and recorded in provenance.
- Shared-kernel Docker is not equivalent to a disposable VM. Docker daemon/kernel,
  CLI and reviewed local source snapshot are trusted host prerequisites. Container
  escape vulnerabilities, a hostile local host writer, content-embedded secrets or
  compromised trusted toolchains are outside this boundary. There is no host fallback.
- Source dependencies must be supplied offline; no automatic vendor/download support.
  APK toolchain repositories are rolling; image ID and compiler versions are recorded,
  but cross-date toolchain/byte reproducibility and SBOM/attestation are not promised.
- DT_NEEDED closure is checked; dlopen plugins and symbol-version ABI compatibility
  need developer review. Symlink/hardlink sources/staging and unsupported ELF shapes
  fail explicitly, rather than being followed or silently repackaged.
- SIGKILL/host interruption can prevent RAII container cleanup. Inspect only Forge's
  named containers if cleanup is needed; don't delete unrelated Docker resources.

Commands, manifest recipe, quotas and trust boundaries: [Forge guide](FORGE.md).
