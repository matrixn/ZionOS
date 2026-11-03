# Native ZPK developer journey — TASK 05

This integrates existing Foundation, Forge, Runtime, Manager and App SDK without
redesigning their package layout, storage API, boot profile or native runtime.
Installed apps are Alpine/OpenRC processes. Docker is only a BUILD tool.
Genesis 0.6.1 remains experimental, live/diskless and unauthenticated.

## 1. Prepare Forge

Use Ubuntu WSL/Linux, Rust/Cargo/rustfmt/Clippy, Node 22+ and Docker with cgroup v2
and [Forge isolation prerequisites](FORGE.md). Never run third-party build scripts
directly on Windows/WSL. There is no unsafe host fallback.

From repository root:

```sh
# Optional developer CLI installation; never install the target Manager on this host.
cargo install --locked --path crates/zpk-forge --root "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
zpk forge --help
npm --prefix web/zion-app-sdk ci --ignore-scripts --no-audit --no-fund
(cd web/zion-app-sdk && npx playwright install chromium)
rustup target add x86_64-unknown-linux-musl
```

VM testing also needs musl-tools, QEMU/qemu-img, xorriso, e2fsprogs and Chromium's
system dependencies. Playwright's `install --with-deps chromium` is available on
equipped administrator/CI machines. KVM accelerates tests when available; TCG is the
fallback. No host account/service changes are performed. Port 8080 must be free.

## 2. Create an application

Run `zpk forge init --project /path/to/project` in a supported source project and
resolve every reported ambiguous metadata/executable field. Forge supports Cargo,
CMake, Go and declaratively staged Make. It does not invent executable paths.

For a working complete example use `examples/zion-hello/`, or copy its source,
manifest, license, icon and SDK assets into a new project. Set your reverse-domain
package ID/version, native executable staging, permissions, storage and desktop entries.
References: [manifest](FORMAT_V1.md), [Forge](FORGE.md), [SDK](APP_SDK.md).

Zion Hello declares static musl/x86-64-v1, one non-root foreground service, a private
data slot and a desktop entry with notifications. Multiple windows share one backend.
Packages declaring only static UI do not require a permanent daemon.

## 3. Build and validate

```sh
# Explicit trusted toolchain preparation; actual source compilation has no network.
zpk forge build --project examples/zion-hello --prepare-builder
mkdir -p build/zpk
zpk forge package --project examples/zion-hello --output build/zpk/zion-hello-1.0.0.zpk
zpk forge validate build/zpk/zion-hello-1.0.0.zpk
zpk forge inspect build/zpk/zion-hello-1.0.0.zpk
cargo run --locked --manifest-path crates/zpk-schema/Cargo.toml --bin zpk-inspect -- \
  build/zpk/zion-hello-1.0.0.zpk 0.6.1
```

Choose a fresh output path on each run; Forge refuses overwrite. It stages outputs,
validates ELF dependencies, generates checksums/provenance and uses Foundation to
validate archives. Packaging downloads nothing. Hashes/provenance do not authenticate
publishers: unsigned packages are trusted-local development only. Legacy build flags
and ELF notes do not prove every possible instruction or execution path.

## 4. Boot a safe persistent development target

```sh
bash scripts/build-iso.sh
bash tests/e2e/check.sh --interactive
```

This creates a fresh `build/e2e-XXXXXX/` containing three Forge-built packages,
read-only fixture media, logs/evidence and a dedicated 512 MiB QCOW2. Filesystem tools
receive pinned descriptors for newly created exclusive regular files only. Existing
images/symlinks are refused. No physical device is passed through or formatted.
The guest checks QEMU, its Genesis marker, the expected virtio serial, filesystem
label and absence of other writable disks. This test never uses QEMU `-snapshot`.

The fixture mounts the app disk and bind-mounts `apps`, `config`, `state`, `logs`
and `registry` at their canonical ZPK locations. It copies trusted Manager/Runtime
helpers from read-only media. The CLI first proves RAM-only targets are refused.
It tests interrupted installation recovery, then installs through the real local CLI.

Open **http://127.0.0.1:8080** and choose **Zion Hello** in the launcher. The terminal
accepts only these local guest test commands:

```text
status
settings-get
settings-set My persistent greeting
upgrade
failure
uninstall
quit
```

`upgrade` installs compiled 1.1.0. `failure` requires that upgrade first and attempts
the failing 1.2.0 candidate, checking compensation. `quit`/Ctrl+C stops the service
and powers off the owned VM cleanly. Artifacts stay at the printed path. Every run
uses a new directory and never overwrites the previous disk. These are trusted VM
fixtures, not package-provided root hooks, production commands or remote APIs.

## 5. Local target installation and desktop launch

On a prepared persistent Alpine/ZionOS target only, as its local administrator:

```sh
zpk install /path/to/zion-hello-1.0.0.zpk --trust-local-development \
  --permissions desktop-ui,notifications,background-service,persistent-data --start --unix-health
zpk list
zpk info org.zionos.zion-hello
zpk status org.zionos.zion-hello
zpk verify org.zionos.zion-hello
zpk logs org.zionos.zion-hello
zpk stop org.zionos.zion-hello
zpk start org.zionos.zion-hello
zpk restart org.zionos.zion-hello
```

Manager enforces persistent mounts, exact permission review and dedicated native
identity; OpenRC manages lifecycle. Core reads only the public derived desktop
projection, not private SQLite. Installed/enabled entries appear automatically.
Open the launcher entry to exercise SDK title, theme, notification and window controls.
Opening a UI window never invokes a privileged service-start operation.

## 6. Native application settings

The backend owns `state/greeting.txt` in its private persistent data slot. It initializes
once and changes it through create-new temporary files, mode 0600, fsync, atomic rename
and directory fsync. Values are nonempty, control-free UTF-8, at most 160 bytes.
Existing symlinks/hardlinks and oversize files are refused on reads. Stale temporary
files are not overwritten; explicit local recovery may be required after unusual crashes.

Its private `service.sock` accepts newline-terminated `ZPK_HEALTH_V1`, `ZPK_INFO_V1`,
`ZPK_SETTINGS_V1 GET`, and `ZPK_SETTINGS_V1 SET <greeting>`. Requests are bounded to
1024 bytes; reads/writes time out after one second. Its 0700 directory limits access
to the package identity and local administrator. The guarded guest fixture implements
`settings-get`/`settings-set` above as a client without running package code as root.

This is **native application persistence**, not an authenticated browser broker.
The UI receives no private socket path, public service port or credential.
SDK `settings.get/set` and file selection remain typed `UNAVAILABLE`. Browser editing
of native settings is deferred until authentication and a reviewed broker exist.
Browser localStorage is not presented as persistent app-disk storage.

## 7. Upgrade, rollback and uninstall

Increment Cargo and manifest versions, update Cargo.lock, and rebuild/package/validate
through Forge. The fixture does this for 1.1.0; 1.2.0 uses the explicit `--fail-start`
development argument to exercise failed candidate startup.

On the prepared target:

```sh
zpk upgrade /path/to/zion-hello-1.1.0.zpk --trust-local-development \
  --permissions desktop-ui,notifications,background-service,persistent-data --start --unix-health
zpk rollback org.zionos.zion-hello
zpk uninstall org.zionos.zion-hello
```

Upgrades preserve state and verify health. Failed startup restores the prior executable
and service when compensation succeeds. Manual rollback selects an available previous
version. Neither operation rolls back application data/migrations. Uninstall stops the
native process and removes executable versions/desktop registration; data/config/logs
and reserved identity metadata remain. Data deletion is not implemented.

## Automated full journey

```sh
bash tests/e2e/check.sh                  # image safety, SDK and settings unit tests
bash tests/e2e/check.sh --packages       # three Forge-built/validated packages
bash scripts/test-vm.sh --zpk-e2e        # complete three-boot persistent journey
bash tests/e2e/check.sh --legacy-vm      # same journey on emulated Conroe/TCG
bash scripts/test-vm.sh                 # existing Genesis smoke, still -snapshot
```

Boot one installs, checks UID/ownership/health, changes a native setting, launches
the actual UI and shuts down. Boot two mounts the same disk, recreates volatile
identities/OpenRC through explicit Manager startup, checks settings/counters,
upgrades, tests failed-start compensation, then interrupts activation and shuts down.
Boot three reconciles the durable journal, verifies the prior healthy version,
launches the UI and uninstalls while retaining data. Real Chromium also watches an
open old window during guest CLI upgrade/uninstall and checks live revocation.
Genesis smoke uses real metrics/storage/provisioning routes rather than browser mocks.

Default CI adds safety/settings/package checks and preserves previous jobs. The
manual native-VM workflow adds this persistent scenario. Evidence stays in ignored
`build/`: package hashes/provenance, QEMU arguments, serial/browser logs and disk image.

## Remaining boundaries

The live ISO has no automatic package persistence/autostart from this fixture. Each
test boot explicitly mounts the same disk and invokes a trusted local bootstrap.
Production mounts, boot ordering and recovery policy need separate design/verification.
No mount/format command is added to Core or Manager. HTTP remains read-only,
provisioning apply stays disabled, and installed apps use no runtime container/CDN.
Remote installation, publisher signatures, aggregate quotas, power-loss durability
and an authenticated native broker remain outside this milestone. Read the
[technical verification report](E2E_VERIFICATION.md) for executed vs untested results.
