# Actual Genesis 0.7 Package Center verification

Read [the developer guide](../../docs/GENESIS_07.md) and
[architecture](../../docs/GENESIS_07_ARCHITECTURE.md) first.

```sh
bash tests/package-center/check.sh
bash tests/package-center/check.sh --browser
bash scripts/build-iso.sh
bash tests/package-center/check.sh --vm
bash scripts/run-qemu-package-center.sh
bash scripts/run-qemu-package-center.sh --resume build/center-XXXXXX
```

Default checks run Rust auth/IPC/serialization tests, strict Clippy, frontend syntax
and fresh-image/resume safety tests. Four additional resume tests reject unknown
environment/UUID, changed image identity, symlinks and hardlinks before tools run.
They do not create service accounts or run a
privileged package installer/native app on the developer host. The `vm-tests` Cargo
feature only builds the separate guarded `guest.rs` fixture; it is not shipped in
the production ISO or enabled in the broker.

`--browser` uses actual UI code in Chromium with explicitly mocked API fixtures:
seven cards, grid wrapping, keyboard navigation, untrusted text rendering,
installed/not-installed and running/stopped/static controls, mobile layout and
separate permission approval. Browsing details must not allocate preview receipts.
This does not claim native execution; `--vm` supplies the real operation coverage.

The VM variant compiles three actual Forge versions and a background-only Cargo
example, then uses the production ISO, OpenRC adapter and authenticated Core/broker.
Chromium operates the real desktop: bootstrap/login, catalog preview, file upload,
permission/trust approval, install, start/stop/restart, launcher/window/SDK,
upgrade/failure/rollback, uninstall and logout. Two simultaneous real background
installs must produce one accepted operation and one BUSY response, with no fake
desktop entry. Archive mutations test traversal, architecture and hash failures.
The guest checks native UID/security controls, persistent settings/permissions,
unauthorized IPC and a controlled interrupted Manager activation. No HTTP fault
injection route exists. Root fixture actions require QEMU, the exact disk serial,
production ready state and read-only fixture media.

Six clean boot phases cover an absent configured UUID, three lifecycle boots on
one persistent disk, a deliberately corrupt registry and a separate invalid-marker
image. No persistence boot uses `-snapshot`. Existing nonpersistent Genesis smoke
tests keep snapshot mode. Do not overlap tests or another server on port 8080.

Only new exclusive regular files inside ignored `build/center-*` directories may be
formatted while constructing test images. Fresh image descriptors are pinned,
owner/link/path/size checked and never replaced. Resume verifies recorded UUID,
inode/device identity and the QCOW2 header; it never formats an existing image.
There is no physical block-device passthrough or host mount/repair operation.

Use `ZION_CENTER_LEGACY=1 bash scripts/run-qemu-package-center.sh` to force TCG/Conroe
instead of available KVM. This checks the exercised execution paths on an emulated
legacy CPU; it is not certification of every third-party instruction or real hardware.

Logs, screenshots, fixture media, random setup code/password and persistent images
stay in the ignored evidence directory. Credentials are private files: never attach
the directory wholesale to issues/CI artifacts. Screenshots omit password fields.
The interactive runner prints the code's filename, not its contents. Review evidence
for secrets before sharing it. See [executed results](../../docs/GENESIS_07_VERIFICATION.md).
