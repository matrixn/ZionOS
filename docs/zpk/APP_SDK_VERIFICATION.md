# TASK 04 — executed verification

Verified locally on 2026-11-02 in Ubuntu WSL, branch `feat/zion-app-sdk`.
These results supplement the historical Genesis/ZPK reports.

| Check | Result |
| --- | --- |
| Foundation | 3 projection unit tests + 23 integration tests; rustfmt/strict Clippy passed |
| Core | 38 tests, rustfmt, Clippy `--all-targets -- -D warnings`, musl staging passed |
| Manager | 22 tests, 46 child-process crash cases, strict Clippy; three-boot persistent QEMU integration passed |
| Runtime | 13 tests, strict Clippy; native OpenRC/QEMU integration passed |
| Forge | 16 tests + 2 isolated Docker/reproducibility integration tests passed |
| SDK/shell | 14 SDK/registry/bridge + 3 existing desktop Node tests; generated JS/ESM/declarations/sample consistency and TypeScript consumer passed |
| Safety/source | 16 safety fixtures; Bash/portable shell, Python and JavaScript syntax passed |
| Browser | Actual Chromium desktop and 375px mobile; debug Core + temporary Manager fixture passed |
| ISO | Final `zionos-genesis-0.6.1-x86_64.iso`, about 352 MiB, built; embedded init verified |
| Final native VM | Rebuilt ISO booted under QEMU/KVM; compiled Forge app installed through guest CLI, non-root OpenRC backend and Unix health verified |
| Native browser | Discovery/icon/SDK/theme/dialog/notification, window controls, multiple windows sharing one backend PID, pin/search/keyboard/mobile, uninstall/asset revocation passed |
| Vue scaffold | Build passed with expected classic-script Vite warnings; not used in ISO |

Browser negative cases include forged source/session messages, sibling-window attacks,
malformed registries, undeclared notifications, unavailable files/settings, parent
DOM/storage access, CSP fetch/popup restrictions, second-document navigation closure,
private payload/path access, Origin-null API refusal and HTTP write refusal.
Mocks never execute native package content on the developer host. Native integration
executes only the repository's compiled sample inside the disposable VM.

Ignored local evidence: `build/sdk-schema-tests.log`, `build/sdk-core-tests.log`,
`build/sdk-core-clippy.log`, `build/sdk-manager-regression.log`,
`build/sdk-runtime-regression.log`, `build/sdk-forge-regression.log`,
`build/sdk-safety.log`, `build/sdk-generated.log`, `build/sdk-js-tests.log`,
`build/sdk-browser-navigation.log`, `build/sdk-iso-final.log`, `build/sdk-final.log`,
`build/sdk-desktop.png` and `build/sdk-mobile.png`. Media/logs/ISO are not committed.

## Limits

- Core remains unauthenticated and local-development-only. No privileged HTTP package
  operation, native permission broker, file grant or persisted SDK settings exists.
- Opaque iframe/CSP and bridge validation enforce specific boundaries, not complete
  network-egress, CPU/memory, side-channel or phishing isolation. Self-navigation can
  issue a request before the shell observes and closes it.
- Unsigned packages require trusted-local policy; hashes do not authenticate publishers.
  Dedicated origins and authenticated per-user authorization remain prerequisites.
- Live ISO does not automatically persist packages. The VM test prepares disposable
  persistent mounts/helpers; normal `run-qemu.sh` starts with built-ins only.
- Physical NAS devices, real Core 2 hardware, power-loss, Firefox/Safari and production
  persistence were not tested.
- The new manual SDK VM workflow was not remotely dispatched. Local results above
  are distinct from push CI results, reported separately at completion.

Reproduce using [APP_SDK.md](APP_SDK.md), the
[sample](../../examples/zion-hello/README.md) and [tests](../../tests/apps/README.md).
