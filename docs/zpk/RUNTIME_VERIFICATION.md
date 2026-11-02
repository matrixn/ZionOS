# TASK 02 — native runtime verification

2026-11-02. `feat/zpk-runtime`, based on Forge `288edd2`; main remains Genesis 0.6.1.
This is an experimental runtime implementation, not a production sandbox certification.

## Implemented changes

- [x] `crates/zpk-runtime`: administrator policy model, installed snapshot verification,
  native launcher, OpenRC adapter, extensible lifecycle interface, health/status/logs,
  explicit autostart and resource/permission enforcement.
- [x] Background/shared backend, on-demand and static desktop models; no daemon is
  created for a static desktop. UI activation/broker bridge itself remains planned.
- [x] Root-owned version/current layout and separate persistent/config/log/run/control
  directories; locked dedicated nologin identities and explicit writable slot grants.
- [x] Required Landlock ABI>=6, Linux x86-64 seccomp, zero capability sets, no_new_privs,
  no supplementary groups, hard rlimits, sanitized environment and CLOEXEC descriptor cleanup.
- [x] `zpk-common` now shares real ELF checks between Forge and Runtime. Forge keeps
  its re-export; its lockfile adds the local shared crate without upgrading registries.
- [x] Foundation exposes its existing strict checksum/content checks for installed
  inventory validation. Package schemas and accepted archive behavior are preserved.
- [x] `examples/zpk-runtime-demo`, mock/temp-root unit tests, guarded QEMU fixture media,
  ordinary runtime CI job and separately dispatchable VM workflow configuration.
- [x] Root README, component/developer docs and agent instructions updated. No Core,
  OpenRC boot profile, kernel, storage endpoints or ISO staging changes.

## Executed evidence

- [x] **13 runtime tests**: valid/invalid service/runtime models, identity/limits/grants,
  multiple UI entries/shared service/static models, mocked lifecycle/autostart/status,
  crash state, shell-fragment exclusion, inventory/checksum/current reference, ELF and
  package dependency failures, path/symlink/mode refusal, bounded logs and peer-checked
  Unix health. CLI test roots remain pure; no host services/accounts are created.
- [x] Runtime/Common rustfmt, Runtime strict Clippy `--all-targets -- -D warnings`,
  portable guest-script syntax and native baseline musl release build.
- [x] **Real disposable QEMU KVM integration** on the existing Alpine/Genesis ISO:
  Landlock **ABI 7** available; primary service UID/GID **23101**, no supplementary
  groups/capabilities/inherited extra FDs, no_new_privs/seccomp enabled.
- [x] Actual OS denials for a synthetic world-readable/writable NAS file, passwd,
  devices, host shell, IP sockets, outbound Unix connections, SysV IPC and namespace creation.
- [x] Actual allocation/open-file/fork/file-size failures and CPU-limit termination;
  defaults are rlimits, not cgroup aggregate quotas or filesystem quotas.
- [x] Actual OpenRC start/stop/restart, clean SIGTERM marker, observed process UID/exe,
  Unix health, logs, SIGKILL crash respawn/restart count and exhausted respawn budget
  reported crashed. Stop also works after the current symlink is removed.
- [x] Actual autostart default-runlevel link on/off and configuration save; live RAM
  refuses persistent-required. State counter survives process restarts/crashes only.
- [x] Existing Foundation **23 tests**, strict Clippy and formatting.
- [x] Existing Forge **16 offline + 2 Docker build tests**, including five language
  variants, complete rebuild equality, dependency closure and minimal Alpine execution.
  These are Forge's separate legacy tests, not the ZPK Runtime implementation/backend.
- [x] Existing Genesis **33 Rust tests**, formatting/strict Clippy and musl release/staging.
- [x] Existing **16 shell/Python fixtures**, **3 JavaScript tests**, Bash/Alpine/OpenRC syntax.
- [ ] Remote CI results are reported separately after publication, not assumed from
  local results. The native VM workflow is opt-in; local VM execution above is verified.

The VM had two read-only CDs, no writable/physical disks, no host forwarding and no
runtime container. The trusted guest fixture refuses writable block devices. NAS
test paths were synthetic files in guest RAM. The guest process was terminated after
test completion; host account database/OpenRC/NAS storage were not touched.

Local ignored artifacts: `build/runtime-fixtures.iso`, `build/runtime-fixtures/`,
`runtime-final.log`, `runtime-vm.log`, `runtime-forge-checks.log`,
`runtime-foundation-checks.log`, `runtime-core-checks.log`, `runtime-legacy-checks.log`.

## Remaining limitations

- Shared-kernel policy layers are not a VM-equivalent/complete sandbox. No PID/mount
  namespace, aggregate cgroup control, aggregate disk quota, metadata/side-channel privacy
  or comprehensive syscall allowlist is claimed. Root/kernel remain trusted.
- No actual durable volume/reboot/power-loss validation, package installer, upgrade
  execution/rollback, signature trust, authenticated permission broker or UI-origin bridge.
- No automatic public HTTP port or network grant. Declaring network permissions does
  not bypass the server-only Unix policy. FD passing is deliberately unavailable.
- Log retention/rotation and tamper-proof audit are not implemented; logs are untrusted
  application output. Runlevel/config persistence is not a transactional installer.
- Linux x86-64 and a foreground service contract only. Other architectures, unsupported
  kernels/controls and arbitrary application compatibility remain unverified/unsupported.

Full contract, filesystem model, exact commands and enforcement table: [RUNTIME.md](RUNTIME.md).
