# Genesis 0.7 — implementation and executed verification

Date: 2026-11-03. Base inspected/fetched: `origin/main` at `d890f01` (TASK 05 merged).
Branch: `feat/genesis-07-package-center`. Changes are prepared for review; no main
merge, force push, release publication, physical disk operation or host native
application installation was performed.

## Shared ZionOS system login — 2026-11-03

The desktop now owns one shared username/password session. First-owner setup is
code-free on localhost; Package Center has no credential forms, bootstrap-code
field, sign-out control or separate account. It consumes the desktop session and
still requires administrator authorization in Core and the root broker. Global
sign-out is in the top bar; expiry returns to the system login. Existing accounts
migrate to username `admin`, keeping their password hash, sessions and applications.
New first owners choose their username. The only real account role is administrator;
the non-admin browser fixture tests refusal, not implemented multi-user/RBAC.

Changed components: shared `web/desktop/session.js`/`.css`, desktop entry/chrome,
Package Center session consumption, Core local enrollment/static assets, broker
named login/profile, authentication-database migration, ISO staging, VM onboarding,
focused session tests and documentation. The legacy root bootstrap/API remains a
compatibility tool. No ordinary user needs a setup code. Initial ownership belongs
to the first trusted local browser; exact localhost Host/Origin, peer restrictions,
singleton enrollment and existing-account refusal are enforced. Package writes
remain session/role/CSRF protected. Local read-only NAS diagnostics remain unchanged.

Executed results for this batch:

- Core: **42** unit tests, rustfmt and Clippy `-D warnings` passed. New test checks
  that first enrollment rejects non-local or mismatched Origins/Hosts.
- Control: **11** tests, all-feature checks, format and strict Clippy passed.
  New tests cover code-free named owner creation/replacement refusal and migration
  preserving the old password hash and active sessions.
- Browser: shared setup with password confirmation, exactly one login, no Package
  Center password/code fields, session expiry, global logout and UI role refusal
  passed. Existing seven-card/search/filter/keyboard/mobile tests passed.
- Actual ISO built successfully (~358 MiB; cached build **87 seconds**).
- Real full Package Center lifecycle passed across **six clean boots** in
  `build/center-7sUwpD/`, now using system setup/login without any bootstrap fixture:
  install, service/SDK, reboots, upgrade/failure/rollback, uninstall, global logout,
  authorization/security checks and missing/invalid/corrupt degraded boots.
- Old persistent `build/center-DG3IKP/` resumed under the new ISO and logged in as
  `admin` with its original password; its installed app/autostart/window still worked.
- Existing SDK: 17 Node/generated-type checks, separate Chromium regression and
  native OpenRC/SDK QEMU regression passed. Foundation 26 tests, default Forge checks
  (16 tests), eight image guards, 16 safety tests and shell/JS/YAML syntax passed.
- Original snapshot boot/API/branding smoke passed (**51 seconds** with KVM).
  Conroe and the opt-in Forge Docker test pair were not rerun for this batch;
  actual Forge builds/packaging ran in the ISO and native fixtures.

Logs: `build/system-session-{browser,iso,vm,schema,forge,sdk-browser,sdk-vm,
legacy-resume,safety,source,smoke}.log`. Real first-owner screen:
`build/center-7sUwpD/zionos-system-setup.png`; Package Center screenshots in the same
directory show no separate account/login controls. Artifacts and test passwords
remain ignored by Git. Earlier sections below retain prior milestone evidence.

## Package card/detail update — 2026-11-03

Package Center now presents responsive, fully clickable cards with an icon/initial,
name, version, description and installed/service status. Enter/Space open details.
Details put Install/Uninstall and native Start/Stop/Restart beside the package title,
followed by description, publisher/license/compatibility, dependencies, declared
storage and permissions. Static packages have no service buttons. Maintenance,
bounded logs, upgrades and executable rollback remain available below.

Discover matches stable package IDs to the actual installed registry, so an
installed catalog item offers Uninstall rather than another Install. Catalog
metadata includes inspected manifests; older summaries are enriched from hash-checked
local archives at startup. The index is bounded to 32 entries/1 MiB, below the IPC
response limit. Browsing metadata does not allocate preview receipts or execute
content. Install still requires fresh validation and explicit trust/permission review.

Executed for this update:

- Core: 41 tests, format, strict Clippy and release musl build passed.
- Control: 9 tests/default and all-feature checks, format and strict Clippy passed.
- Foundation: 26 tests and strict checks passed; Forge default checks passed
  (16 tests; its two opt-in Docker tests were not rerun in this batch). Actual
  Forge compilation/packaging was exercised by the ISO and VM fixture builds.
- SDK: 17 Node tests/generated types, separate Chromium regression and actual native
  SDK/OpenRC/QEMU regression passed.
- New focused Chromium test: seven explicit API-fixture packages; multiple columns
  and rows, search, state filtering, Enter/Space/back navigation, safe text rendering,
  state-dependent actions, no receipt allocation while browsing, explicit approval
  and mobile layout passed. This test does not run a host installer/native service.
- Actual updated ISO built successfully: ~358 MiB, cached build about 71 seconds.
- Full real Package Center workflow passed with six clean boots in
  `build/center-btJTZn/`: uninstalled catalog details/Install/review, installed catalog
  details/Uninstall/Stop, native startup and UI, Stop/Start/Restart, persistent reboots,
  upgrade/failure/rollback, uninstall back to Install, security/capacity/recovery and
  all three degraded cases. Logs: `build/package-cards-vm.log`.
- Updated ISO resumed the earlier `build/center-DG3IKP/` volume, retaining the account,
  app/autostart and working browser window; clean shutdown passed. Log:
  `build/package-cards-resume.log`. Conroe was not rerun for this UI update; the prior
  Genesis 0.7 Conroe result below remains historical evidence.
- Eight fresh-image/resume guards, 16 shell safety tests, shell/OpenRC/JavaScript
  syntax and workflow YAML parsing passed.

Other logs use the prefix `build/package-cards-{browser,iso,schema,forge,sdk-browser,
sdk-vm,safety,source}.log`. Actual updated UI screenshots are
`build/center-btJTZn/package-center-{grid,dark,light,mobile,permissions}.png`.
`build/package-center-grid-browser.png` shows seven **fixture** packages for layout
testing; those are not fake entries added to the real catalog. All generated images,
credentials and VM artifacts remain ignored by Git. Security/transport/storage
limitations documented below are unchanged.

## Delivered code

- `crates/zpk-control/`: Argon2id/local bootstrap/session/CSRF storage, bounded Unix
  IPC, root broker, curated catalog, upload snapshot receipts, real Manager phases,
  target-only identity/volume/restore CLI. Existing Manager/Runtime are reused.
- `core/api/src/packages.rs`: non-root authenticated HTTP facade with streamed
  persistent uploads, Origin/CSRF validation, typed errors and bounded concurrency.
  Core version is 0.7.0; legacy NAS/storage routes remain read-only.
- `platform/alpine/`: UUID volume validation, broker and Core OpenRC ordering,
  dedicated locked Core identity, helper/catalog staging and 0.7 boot branding.
- `web/desktop/apps/package-center.*`, desktop/index: real catalog/installed data,
  preview/trust/permissions, lifecycle, upgrade/rollback/uninstall, progress/logs,
  dark/light/mobile UI and existing native launcher/window/SDK integration.
- `scripts/`: ISO helper/catalog build, version-derived ISO lookup and safe persistent
  interactive runner/resume. Old native VM runners now resolve the current ISO.
- `tests/package-center/`: real browser/production boot/native security/recovery
  scenarios. Fresh image support extends the existing guarded E2E image builder.
- CI adds source auth/IPC checks and a manually dispatched full ISO/QEMU workflow.

One verified race required serializing **Manager reads as well as mutations**:
installed/status polling previously raced Manager's exclusive process lock during
service operations. Operation completion now releases that lock and busy state
before publishing COMMITTED. UI install completion now selects Installed correctly.
Musl/glibc ancillary length types and Linux ioctl request types are handled explicitly.
Strict IPC rejects unknown authorization claims, including status requests.

## Executed locally in WSL/Linux

| Check | Actual result |
| --- | --- |
| Core rustfmt/test/Clippy `-D warnings` | 41 tests passed; strict checks passed |
| Control rustfmt/all-feature tests/strict Clippy | 9 tests passed, including peer/framing/descriptor/auth/concurrency guards |
| Foundation/shared inspection | 26 tests passed (3 shared + 23 schema/archive) |
| Forge | 16 default tests + 2 opt-in real isolated Docker build tests passed |
| Runtime | 13 tests passed with mocked OpenRC and temporary roots |
| Manager | 22 tests passed, including child-process crash/recovery injection |
| SDK/frontend | 17 Node tests, generated JS/types and TypeScript consumer passed |
| Chromium SDK regression | Real browser tests passed; installer mocked only in that separate host-safe suite |
| Original SDK native VM regression | Real Forge service/OpenRC/browser and uninstall revocation passed on the 0.7 ISO |
| Shell safety | 16 guard/overlay/cache/console tests passed, no physical devices |
| Image helper guards | 4 tests passed, no actual host block device operations |
| Resume refusal guards | 4 additional tests passed; invalid metadata/identity/links refused before QEMU/image tools |
| Existing E2E source checks | Image/SDK checks plus 2 native hello settings tests and strict Clippy passed |
| ISO build | Release musl/baseline helpers, actual Forge catalog and final ISO passed; ~358 MiB |
| Original QEMU smoke | Boot/initramfs loader/OpenRC/DHCP/health/metrics/read-only NAS/assets passed (~44 seconds with KVM) |
| Package Center QEMU | Actual authenticated graphical lifecycle and six clean boots passed |
| Interactive runner and resume | Actual GUI install/lifecycle on TCG/Conroe, clean quit, same-disk resume with retained account/app and clean quit passed |

Detailed local logs live in ignored `build/genesis07-{schema,forge,runtime,manager,
sdk-browser,control-final,iso-final,smoke,sdk-vm,safety}.log`. Build time depends on
cache, CPU and Docker availability; the cached final ISO build took about 79 seconds.
These are local observations, not timing guarantees.
All Bash/OpenRC/active JavaScript syntax checks and workflow YAML parsing passed.
Core's lockfile preserves all 101 original external dependency versions/checksums
from inspected main; it adds 30 locked packages for the new control facade.

The interactive/legacy check used `build/center-DG3IKP/`:
`ZION_CENTER_LEGACY=1` forced TCG/Conroe for setup, actual native installation,
browser/SDK and lifecycle actions. The wrapper then shut down cleanly and
`--resume` reused the identity-checked persistent disk under KVM. A fresh browser
login observed the retained account, installed version, autostart and working app
window. Evidence: `build/genesis07-interactive-verified.log` and that directory's
`interactive-{first,resume}.log`. This tests the interactive commands, not just the
automated multi-boot driver. No physical CPU certification is implied.

All seven source CI jobs passed for integration commit `c6499d5`:
[GitHub Actions run 41623388318](https://github.com/matrixn/ZionOS/actions/runs/41623388318).
The separate manual ISO/QEMU workflow was not dispatched remotely.

## Actual Package Center VM behavior

The final complete runner passed with exit status 0 in `build/center-QxbYd7/`
(`build/genesis07-vm-verified.log`). Earlier complete production runs also passed
in `build/center-k8EDZT/` and `build/center-8BjWOe/`. Tests use real Forge archives, production volume boot and
OpenRC, HTTP/session/CSRF checks, the restricted broker and real Manager transactions.
No fake registry/installer or manual package assembly supplies this result.

- Bootstrap creates the administrator through the graphical interface; the account,
  session database and password hash survive reboot. Logout rejects cookie replay.
- Anonymous/forged-cookie package requests fail; wrong/missing CSRF and mismatched
  Origin fail. Client-supplied authorization claims fail strict request parsing.
- Root and unauthorized Core-identity IPC calls cannot bypass the session contract.
- Malicious traversal, bad hashes and incompatible architecture fail preview.
- Two concurrent real background-only installs return one accepted operation and
  one BUSY response. The accepted native service commits and is removed cleanly;
  it never creates a fake desktop launcher entry. Local catalog preview also passes.
- File upload/preview/review/install, native UID/capability/seccomp checks and actual
  launcher/icon/SDK title/theme/notification behavior pass.
- Start/stop/restart operate real native OpenRC processes. App state/settings and
  runtime grants persist after clean poweroff/reboot on the same dedicated disk.
- Upgrade activates 1.1.0 and revokes old assets/windows. Deliberately failing 1.2.0
  restores the prior executable; data migration rollback is never claimed.
- A root-only guest test kills Manager at activation; the next production boot
  reconciles its journal and retains the last committed application version.
- Manual executable rollback and graphical uninstall succeed. App data remains;
  desktop entries/assets are revoked. No data purge control is exposed.
- A configured 1 GiB reserve on the 512 MiB test filesystem returns the actual
  typed `INSUFFICIENT_STORAGE`/HTTP 507 result.
- Missing UUID, corrupt registry and invalid marker boot into explicit degraded UI;
  read-only monitoring remains healthy and no installation falls back to RAM.

Screenshots are generated by real Chromium, not mockups:
`build/center-QxbYd7/package-center-{dark,light,mobile,permissions}.png`. New runs write their
own `build/center-XXXXXX/` evidence and permission-review screenshots. These outputs
are intentionally excluded from Git. Private credentials must never be shared with
an evidence directory wholesale.

## Reproduce

Follow [the full WSL prerequisites and developer journey](GENESIS_07.md), then:

```sh
bash tests/zpk/check.sh
bash tests/zpk/forge-check.sh --docker
bash tests/zpk/runtime-check.sh
bash tests/zpk/manager-check.sh
bash tests/apps/check.sh --browser
bash tests/package-center/check.sh
bash scripts/build-iso.sh
bash scripts/test-vm.sh
bash tests/apps/check.sh --vm
bash tests/package-center/check.sh --vm
```

For actual manual use, run `bash scripts/run-qemu-package-center.sh`, open localhost
port 8080 and Package Center, read the private code file and choose your password.
Select the local catalog or a Forge-built archive, review and approve, install and
launch. Use `quit`, then resume the exact printed directory using `--resume`.

## Limits and tests not claimed

- No physical NAS/hotplug/power-cut hardware certification. Controlled process crash
  tests and clean guest reboots do not prove every power-loss/filesystem scenario.
- No production TLS/LAN/proxy administration, Secure HTTP cookie, MFA/RBAC/password
  reset, publisher signature verification or remote catalog. Development trust is
  explicit and local. Do not expose this HTTP listener to a LAN/public network.
- App data usage/quota aggregation, migration rollback, data purge, general SDK
  settings/files and complete network/resource isolation are not implemented.
- Legacy CPU flags and ELF note validation do not prove every instruction path in
  arbitrary third-party binaries. Emulated tests do not replace physical hardware.
- SQLite corruption fails closed; this milestone does not automatically repair it.
- Manual VM GitHub Actions is provided; a local run does not claim that workflow
  was dispatched remotely. Ordinary source CI results are reported separately.
