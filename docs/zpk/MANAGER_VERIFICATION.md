# TASK 03 — executed package-manager verification

2026-11-02, `feat/zpk-manager`, based on Runtime `0a13426`. The main branch, Core API,
desktop and Alpine boot/ISO profile source are unchanged. This record distinguishes
local tests from remote workflow configuration and production guarantees.

## Implemented change set

- [x] `crates/zpk-manager/{Cargo.toml,Cargo.lock,src/*,README.md}`: local CLI/helper,
  typed registry/intents, SQLite transactions, process lock, safe staging/publication,
  resolver, Runtime adapter, upgrades, executable rollback, uninstall and repair plan.
- [x] Foundation `archive.rs`: optional streaming sink through the same validator;
  existing read-only inspection API/schema behavior preserved.
- [x] Runtime `installed.rs`: verify the opened regular descriptor and nonblocking
  NOFOLLOW opens; no lifecycle/policy/HTTP behavior change.
- [x] Rust/C Forge examples: optional foreground private-socket service/counter;
  manifests declare permissions/storage. Original no-argument outputs remain.
- [x] `tests/zpk/manager*`: mock installer suite, real process-crash matrix, guarded
  persistent QEMU driver/guest and isolated Forge fixture compilation.
- [x] Ordinary CI manager job and opt-in persistent-VM workflow; previous jobs retained.
- [x] Root README cumulative checklist, component/example READMEs, manager guide,
  test guide, agent guidance and Genesis audit/roadmap references updated.

## Executed locally

- [x] **20 manager tests**, formatting and Clippy `--all-targets -- -D warnings`:
  install/list/info/start/stop/restart/status/logs/upgrade/rollback/uninstall/verify;
  data preservation and reinstall; exact unsigned trust/permission review; UI static,
  on-demand selection/ambiguity and shared backend; missing/versioned/reverse/cyclic
  dependencies and conflicts; service failure compensation and RECOVERY_REQUIRED;
  concurrent manager lock; corrupt SQLite JSON/digests/semantic records/journal;
  traversal/absolute/duplicate paths, links/devices/hash mutation and bad archives;
  CPU incompatibility; sparse oversized sources; unsafe directory modes/symlinks;
  retained snapshot tampering, foreign version preservation and data-schema refusal.
  Native identity selection reads NSS and reserves registry UIDs without modifying
  host accounts; every host service action remains mocked.
- [x] **46 actual child-process exits** across install/reinstall/upgrade/rollback/uninstall, including
  partial staging, pre/post activation, reference switch, service registration, health
  and post-commit cleanup. Fresh manager instances reopen SQLite and reconcile;
  the last confirmed version is preserved, or the committed decision is retained.
- [x] Real native manager integration in **three QEMU KVM boots** sharing one disposable
  preformatted ext4 QCOW2. Primary Genesis ISO and fixture CD read-only, no physical
  disks, no host forwarding or runtime containers. Exact guest serial/QEMU/Genesis guards.
- [x] Both actual Forge-built **Rust static and C musl-dynamic** packages installed,
  listed, verified, started/stopped under dedicated non-root identities; Unix health,
  observed process UID and application-owned counter files/logs verified.
- [x] Rust restart/upgrade 1.0.0→1.1.0/rollback; deliberately unhealthy 1.2.0 rejected
  and previous version/service restored. First boot stops services and syncs; second
  boot restores mounts, verifies registry/payload/data, recreates volatile identities/
  registrations on explicit start and confirms counter progression.
- [x] Both packages uninstalled after reboot; versions/active references removed and
  application-written data retained. Production CLI refuses the live RAM target on
  all boots before persistent fixture mounts exist. A third boot reinstalls C before
  Rust (reverse order), proving retained registry UID/GID and schema metadata are used
  rather than newly allocating the first free Unix identity; data counters advance.
  Incompatible data-schema reinstall is also refused in the unit suite. Both packages
  are uninstalled again with data and retained identity metadata preserved.
- [x] Existing Runtime **13 tests**, strict checks and full native QEMU isolation/
  lifecycle suite passed, including Landlock/seccomp, limits, shutdown/crash budget,
  autostart and stop after current removal.
- [x] Foundation **23 tests** and strict checks; Forge **16 offline + 2 isolated Docker
  tests**, five language variants, full-build reproducibility and minimal Alpine execution.
  Docker is only Forge's existing build/test infrastructure, never the manager runtime.
- [x] Genesis **33 Rust tests**, rustfmt/strict Clippy and musl release/staging build.
- [x] **16 shell/Python safety fixtures**, **3 JavaScript tests**, shell/JS syntax checks.
- [ ] Remote CI execution is separate from these local results; workflow files alone
  do not prove hosted success. Optional VM workflow is not assumed to have run remotely.

The manager's fixture ISO is a test CD, not a rebuilt Genesis release. The existing
`build/iso/zionos-genesis-0.6.1-x86_64.iso` was used for native integration; no new Genesis
ISO or automatic deployment is claimed. Generated images/binaries/provenance/logs
remain ignored under `build/` or the existing ignored Forge/cache locations.

## Remaining limits

- Trusted-local unsigned development only; no signatures/catalog, HTTP installer,
  privilege bridge, data deletion, application migration/backup or UI activation.
- Binary rollback preserves data but cannot undo writes made by a failed new binary.
- Process interruption and clean VM reboot tested; sudden power loss, failing hardware,
  ENOSPC at every fsync, XFS/Btrfs recovery and real NAS storage are not verified.
- No aggregate cgroup/disk quotas or complete sandbox claim; Runtime's documented
  restrictions/limitations remain. Actual Core 2 Duo hardware was not available.
- No atomic multi-package transaction, automatic dependency acquisition/start/restart,
  automatic replaces, journal compaction, retained-version pruning or log retention.
- No unattended live-ISO boot integration: persistence/mount ordering and durable OS
  account/OpenRC configuration remain operator prerequisites. Explicit starts after
  the second test boot are tested; automatic reboot autostart is not claimed.
- Root/kernel/administrative filesystem ownership are trusted. Unknown or corrupt
  registry/content is refused rather than deleted or silently reconstructed.

## Reproduction

```sh
bash tests/zpk/manager-check.sh
bash tests/zpk/manager-check.sh --vm
bash tests/zpk/runtime-check.sh --vm
bash tests/zpk/check.sh
bash tests/zpk/forge-check.sh --docker
bash scripts/build-core.sh
bash tests/check-safety.sh
node --check web/desktop/app.js
node --test tests/desktop.test.cjs
```

Manager VM prerequisites, target CLI commands, schema compatibility and security
contract: [MANAGER.md](MANAGER.md). Local logs: `build/manager-final.log`,
`manager-vm-{first,second,third}.log`, `manager-runtime.log`,
`manager-forge.log`, `manager-foundation.log`, `manager-core.log`, `manager-legacy.log`.
