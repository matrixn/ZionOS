# TASK 05 — technical implementation and verification

2026-11-03, `feat/zpk-e2e-integration`, based on SDK commit `b43f374`.
This integrates existing components; it does not redesign package format, runtime,
manager transactions, desktop windows or the Alpine boot profile.

## Implementation

- `examples/zion-hello/src/{main,settings}.rs`: real native greeting persistence,
  bounded private Unix requests, atomic writes, compile-time version reporting,
  explicit failed-start fixture and two pure first-party settings tests.
- `tests/e2e/`: three Forge-compiled versions, Foundation/hash/ELF/provenance checks,
  fresh regular-image guards, guarded native guest coordinator, three-boot driver,
  live Chromium revocation observer and optional interactive developer environment.
- `crates/zpk-manager/Cargo.toml`: a separate development-only VM test binary. No
  production Manager commands/fault injection or library lifecycle logic changed.
- `tests/apps/browser.cjs`: version-parameterized assertions and an asynchronous
  launcher-readiness check; existing SDK security/window tests are reused.
- `scripts/test-vm.sh`: explicit `--zpk-e2e` dispatch. Default smoke keeps snapshot
  mode; persistent tests do not use it. No physical-device argument is accepted.
- CI preserves existing jobs, adds package/safety/settings checks to SDK job and
  extends the manual native-VM workflow. README/AGENTS/component docs describe the
  actual development environment and remaining production prerequisites.

## Executed local checks

| Check | Result |
| --- | --- |
| Foundation | 3 projection + 23 archive/schema tests, rustfmt/strict Clippy passed |
| Forge | 16 tests + 2 actual isolated Docker build/reproducibility tests passed |
| Runtime | 13 tests, rustfmt/strict Clippy passed; native controls exercised again in E2E |
| Manager | 22 tests including 46 actual child-process crash cases; rustfmt/strict Clippy passed |
| Core | 38 tests, rustfmt/strict Clippy and release musl staging passed |
| SDK | 17 Node tests, reproducible generated SDK/types and TypeScript consumer passed |
| Settings | 2 pure tests: value validation, atomic replacement, mode, links, oversize and stale temporary files passed |
| New image safety | 4 tests: outside workspace, existing/dangling entries, symlink directories, pinned exclusive regular descriptors passed |
| Source | Bash/portable shell and Python/JavaScript syntax passed |
| Genesis safety | All 16 fixtures passed after the VM port was released |
| ISO | Final 0.6.1 ISO built; embedded init verified; no boot/profile source change |
| Existing Genesis smoke | QEMU snapshot smoke passed: boot order, console, DHCP/Core, real read-only storage/pools/provisioning and write refusal |
| Full persistent KVM | All three boots passed, same QCOW2, native UID/policy/ownership/health, settings, upgrade, failed-start compensation, interrupted journal recovery and uninstall |
| Live desktop changes | Actual Chromium old-window/asset revocation on guest CLI upgrade/uninstall passed; new-version window reopens successfully |
| Existing SDK native VM | Actual Forge backend/OpenRC and full browser regression passed |
| Interactive VM | Local settings set/get over private socket, retained disk and clean quit passed |
| Legacy x86-64 | Full three-boot lifecycle passed with `-accel tcg -cpu Conroe`, including native services, settings, SDK/UI, upgrades/recovery and uninstall |

The main persistent run is `build/e2e-Prvmuf/`; initial implementation run
`build/e2e-5qA2gx/` also completed three boots. QEMU arguments, serial/browser logs,
package hashes/provenance, packages and dedicated disk stay in each ignored directory.
Top-level logs: `build/e2e-{iso,genesis-smoke,final,sdk,manager,schema,forge,runtime,
safety,source,final-clippy,sample-tests}.log`.

Interactive evidence is `build/e2e-interactive.log`, with its disk at
`build/e2e-5BagEg/`. Final Conroe/TCG evidence is `build/e2e-legacy-final.log` and
`build/e2e-1VwOCm/` (three clean boots of the same image). No remote manual VM
workflow result is claimed here; push CI is reported separately at completion.

An initial Conroe browser attempt exposed an immediate launcher-count assertion
racing asynchronous discovery (7 built-ins observed before the installed eighth
entry arrived). The test now waits for the expected condition with a bounded timeout;
it still fails if discovery never completes. Native installation/health had already
passed in that attempt (`build/e2e-legacy.log`). The final full rerun above passed.

## Actual state assertions

1. Every package comes from Forge source compilation, not a hand-assembled archive.
   Foundation validates files/hashes; ELF is static musl x86-64 and native code reports
   the compiled Cargo version over its private socket.
2. Real `/proc` identity, zero effective capabilities, `NoNewPrivs`, seccomp, root-owned
   immutable executable and UID-owned mode-0600 state are observed in the guest.
3. A staged install process exits without destructors; CLI reconciliation removes its
   incomplete operation, then ordinary CLI installation succeeds.
4. After clean shutdown, the same image retains SQLite, active version, UID reservation,
   greeting and counter. Explicit fixture bootstrap recreates volatile NSS/OpenRC and
   invokes local Manager startup; it does not claim production automatic mounting.
5. Upgrade starts compiled 1.1.0. Failed 1.2.0 startup restores healthy 1.1.0 without
   changing the greeting. An additional process exits after switching the executable
   reference. Desktop publication is withheld until next-boot reconciliation restores
   the last committed service/version; no mock supervisor supplies this result.
6. Uninstall stops the actual PID, removes executable versions/service/desktop entries,
   returns 404 for app assets and preserves the native greeting byte-for-byte.

Chromium reuses full SDK tests for source/session/version/permission validation,
opaque iframe restrictions, title/theme/notifications, multiple windows, dock/search,
keyboard/mobile and read-only API behavior. Browser storage/metrics fixtures in the
UI suite are complemented by separate real Core smoke requests in every E2E boot.

## Known limits

- Persistent settings are native app state controlled locally through its private
  socket. Browser SDK settings/files remain `UNAVAILABLE`; there is no authenticated
  native broker, privileged HTTP write route or remote installer.
- The production live ISO does not mount app volumes or restore service registrations
  automatically. Each test boot deliberately prepares its dedicated filesystem and
  starts/reconciles via trusted local helpers. These helpers are not staged into ISO.
- Clean reboots and process interruptions are tested; sudden power loss, media errors,
  filesystem damage and production persistent boot ordering are not certified.
- Executable rollback does not reverse application data/migrations. Stale atomic-write
  temporary files can require explicit local recovery. Aggregate quotas remain pending.
- Unsigned packages and provenance do not authenticate publishers. Local trust review
  remains mandatory; iframe/CSP is not complete network/resource isolation.
- No physical NAS disk, real Core 2 hardware, Firefox/Safari or production deployment
  was tested. Emulated CPU results are not real hardware performance measurements.

Exact reproducible commands: [developer journey](E2E.md), [tests](../../tests/e2e/README.md).
