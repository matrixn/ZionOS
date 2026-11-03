# Genesis 0.7 — build and use the native Package Center

Genesis 0.7 provides authenticated local graphical management of real native ZPK
applications. It is an experimental Alpine live OS, not a production NAS release.
The browser server runs as dedicated non-root `zion-core`; package effects run in
the restricted root Unix broker through the existing Manager/Runtime.

## WSL prerequisites and build

Use Ubuntu WSL/Linux with Rust/Cargo, rustfmt, Clippy, musl-tools, Node 22+, Docker
with cgroup v2, QEMU/qemu-img, xorriso, e2fsprogs/debugfs and Python 3.10+. Docker is
used for ISO/source BUILD only. Installed applications do not use containers.

From the repository root:

```sh
cd /mnt/c/Users/radut/Workspace/zionos
rustup target add x86_64-unknown-linux-musl
npm --prefix web/zion-app-sdk ci --ignore-scripts --no-audit --no-fund
(cd web/zion-app-sdk && npx playwright install chromium)
# Equipped administrator/CI machines can use: npx playwright install --with-deps chromium
bash scripts/build-iso.sh
```

Output: `build/iso/zionos-genesis-0.7.0-x86_64.iso`. The build checks Rust and stages
Core, Manager, Runtime, `zion-platform`, desktop assets and a genuine Forge-built
`zion-hello` development catalog entry. Trusted Alpine build tools are explicitly
prepared; actual Forge source compilation is isolated/offline. Existing manifest-relative
Core build commands remain valid; `build-core.sh` does not require Forge/Docker by itself.

## Start a usable persistent test desktop

```sh
bash scripts/run-qemu-package-center.sh
```

This creates a **new**, dedicated preformatted test image in `build/center-XXXXXX/`
and boots the actual ISO with that image's configured UUID. It does not use `-snapshot`.
Filesystem creation is limited to newly/exclusively created regular files in ignored
build directories; no physical disk or existing user image is formatted/overwritten.
The guarded guest verifier and deliberately failing package are test media only,
not programs installed into the production ISO.

The terminal prints the evidence directory and a private `setup-code.txt` filename,
not its secret contents. Open that file locally to obtain the one-time setup code.
Then open **http://127.0.0.1:8080**, choose **Package Center** in the dock/launcher,
enter the setup code and choose your own administrator password. There is no default
password. Setup codes expire after ten minutes; restarting an unconfigured test VM
through the local runner can enable a fresh local setup window.

The runner accepts `quit` or Ctrl+C for a clean poweroff. It retains the test image
and metadata. To reboot with **the same application/account data**:

```sh
bash scripts/run-qemu-package-center.sh --resume build/center-XXXXXX
```

Use the exact directory printed by the first run. Resume checks the test manifest,
UUID, image inode/device identity, regular-file/link/owner attributes and QCOW2 safety
header; it never formats the existing image. Do not copy/swap the image into that
directory and expect identity checks to be bypassed. Re-running without `--resume`
creates a separate empty test environment.

Port 8080 must be free. Tests do not terminate another VM/server. KVM is used when
accessible; TCG fallback is slower. No KVM user/group changes are made by these scripts.
To exercise the legacy CPU baseline, use
`ZION_CENTER_LEGACY=1 bash scripts/run-qemu-package-center.sh` (TCG/Conroe).
This is an emulated execution check, not physical hardware certification.

## Discover and install

**Discover** shows the root-curated local catalog as a responsive grid. Cards show
an icon (or an initial fallback), version, description and installed/service state.
Click a card or focus it and press Enter/Space to open its package detail page.
The ISO bundles `Zion Hello`, compiled with the actual Forge. Choose **Install**
on the detail page, read its identity,
publisher, CPU/OS compatibility and requested permissions, then approve both:

1. Explicit trust for this unsigned local development package.
2. Exact approval of the listed permission declarations.

Choose service start, autostart and optional Unix health protocol settings. Unix
health must only be required when the package implements it; the supplied hello
examples do. Static UI-only packages need no background daemon.

**Approve installation** invokes Manager through the authenticated broker. The
progress panel reports actual transaction/lifecycle states. On success, **Installed**
shows the package's real version/status; its declared desktop entry appears in the
launcher without refreshing the browser. Background-only packages remain service
cards and do not create fake desktop entries.

Hashes and provenance detect inventory changes; they **do not authenticate publishers**.
There is no remote repository or automatic unsigned package download/install.

## Build a package from source and upload it

```sh
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge build \
  --project examples/zion-hello --prepare-builder
mkdir -p build/zpk
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge package \
  --project examples/zion-hello --output build/zpk/zion-hello-0.7-demo.zpk
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge validate \
  build/zpk/zion-hello-0.7-demo.zpk
```

Choose a fresh output filename; Forge refuses overwrite. The example manifest's
package version remains 1.0.0 regardless of this illustrative filename.

In Package Center choose **Install from file** and select that `.zpk`. The browser
uploads actual bytes into bounded persistent staging. Preview validates the archive,
manifest, hashes, architecture, minimum ZionOS version and dependencies without
executing package content. Confirmation uses a session-bound, expiring snapshot.
Malformed, oversized, incompatible, permission-mismatched or stale snapshots are refused.

The full test runner also creates `media/hello-1.0.0.zpk`, `hello-1.1.0.zpk` and
`hello-1.2.0.zpk` under its evidence directory. These are all compiled by Forge.
Version 1.2.0 intentionally fails startup to test executable compensation; do not
confuse that fixture with a healthy application release.

## Operate, launch, update and remove

In **Installed**, use search/state filters and open a card to see metadata, native
PID/status, declared/effective permissions, runtime policy and available controls.
Use **Refresh** for external changes and bounded stdout logs for diagnostics.
Details show description, publisher/license/version, compatibility, package
dependencies, declared storage requirements and permissions. Primary actions are
at the top: **Install** for a catalog package that is not installed, **Uninstall**
for installed packages, **Start** for inactive services or **Stop** for running ones.
Packages without services do not show service controls. Discover also reflects
installed state; it does not offer another Install for an already installed ID.
Opening catalog details is metadata-only and does not consume preview receipts.

- **Start / Stop / Restart** use actual OpenRC lifecycle operations.
- **Launch** opens the declared entry through the existing sandboxed SDK/window manager.
- **Upgrade from file** validates/reviews the next archive before activation.
- **Rollback** switches to an available previous executable version. It does not undo
  application data or migrations; the confirmation explains this.
- **Uninstall** stops the process, removes executable versions and revokes desktop
  entries/assets. App data/config/logs remain. Data purge is not implemented.

Unavailable controls are disabled with an explanation. Read-only storage monitoring
and the disabled provisioning executor are unchanged. Package operations never run
as root inside the HTTP process and never invoke package-provided privileged scripts.

The hello backend persists its greeting/state privately. Its own browser SDK
settings/files capabilities remain unavailable until a general authenticated
application permission broker is implemented; Package Center is not that broker.

## Local console bootstrap, catalog and diagnostics

On the **target ZionOS local root console**, never the WSL/Windows developer host:

```sh
zion-platform bootstrap
zion-platform catalog-import /path/to/trusted-local-development.zpk
zpk list
zpk info org.zionos.zion-hello
zpk status org.zionos.zion-hello
zpk logs org.zionos.zion-hello
zpk repair --plan org.zionos.zion-hello
cat /run/zionos/platform.json
```

`bootstrap` writes its one-time code only to the controlling terminal. Do not put
passwords/session tokens into command lines, logs, Git, screenshots or shared artifacts.
`bootstrap-file` exists for private root-owned automation inputs. Password reset/RBAC/
MFA are not implemented; account recovery needs separate local administration design.

A missing/invalid/corrupt configured volume or registry leaves Package Center degraded;
it never installs into RAM or silently repairs storage. Production volume preparation
and immutable UUID configuration are administrator responsibilities outside this
milestone. The runner's image preparation is a disposable development facility.

## Verification

```sh
bash tests/package-center/check.sh       # auth/IPC/concurrency + source/image safety
bash tests/package-center/check.sh --browser # multi-card navigation/state controls, UI fixtures
bash tests/package-center/check.sh --vm  # actual ISO, GUI and persistent reboots
bash scripts/test-vm.sh                 # original non-persistent snapshot smoke
bash tests/apps/check.sh --vm           # existing native SDK regression on current ISO
```

The full Package Center VM test uses the production volume/OpenRC services and
authenticated Core/broker, not mocked installer state. It includes controlled local
setup, GUI file upload/review/install, service identity, launcher/window/SDK,
stop/start/restart, account/app/data persistence, upgrade/failed-start recovery,
interrupted journal recovery, manual rollback, uninstall, CSRF/auth/IPC/archive
regressions, storage reserve refusal and missing/invalid/corrupt degraded boots.

Default CI never starts package services or privileged installers on its host.
The manually dispatched `package-center-vm.yml` builds the ISO and uses only dedicated
QEMU images. [Architecture/security limits](GENESIS_07_ARCHITECTURE.md) and
[executed results](GENESIS_07_VERIFICATION.md) distinguish implemented checks from
environment-dependent coverage.

**Remote LAN/public administration is disabled.** The current development listener
uses local HTTP, with HttpOnly/SameSite cookies and Origin/CSRF enforcement. It does
not implement production TLS/Secure-cookie/proxy configuration. Never expose the
local/QEMU development listener over an unauthenticated/public network.
