# Native Zion Package Manager — TASK 03

`crates/zpk-manager` provides the local `zpk` administrator CLI and a constrained
privileged adapter for Alpine/OpenRC. Installed applications are native Linux
processes under the existing Runtime. No Docker/OCI runtime, network installer,
catalog, HTTP route, package shell hook or Alpine `apk` modification is added.
The current unauthenticated Zion Core API remains read-only.

Unsigned packages are **trusted local development only**. SHA-256 verifies content
and inventory consistency, **not publisher authenticity**. Do not use this milestone
to deploy hostile packages or unreviewed remote downloads.

## Components

- `model.rs`: typed package/version records, permission review and journal intents.
- `database.rs`: SQLite registry schema v1, DELETE rollback journal, synchronous FULL,
  transactional registry/COMMITTED update and diagnostic history. Bundled SQLite
  avoids a database server or Alpine development-library prerequisite at runtime.
- `filesystem.rs`: trusted path ancestry, exclusive nonblocking `flock`, bounded
  regular-file sources, durable writes, same-filesystem publication using Linux
  `renameat2(RENAME_NOREPLACE)` and directory fsync. There is no cross-device copy fallback.
- `resolver.rs`: installed dependency graph plus one candidate, SemVer ranges,
  cycles, bidirectional conflicts and reverse-dependency checks; no network resolver.
- `host.rs`: dedicated identity allocation, fixed Alpine account helpers and Runtime
  registration/lifecycle/health/logs. It extends Runtime's `ServiceBackend`; it does
  not introduce another supervisor or execute package commands while privileged.
- `manager.rs`: staging, activation, compensation and startup reconciliation.
  The local CLI itself is the privileged helper; it is not setuid and exposes no socket.

Foundation's strict USTAR/zstd parser now offers a streaming callback. The manager
first inspects a bounded private in-memory compressed snapshot, then streams through
the **same validator** into private staging. Callback data is not considered valid
until inspection finishes; failed staging is discarded. No general tar extractor,
package-provided paths outside the fixed tree, links, devices or special files are accepted.
Runtime's bounded file reader also verifies the opened descriptor and uses nonblocking
NOFOLLOW opens so replacing a regular source with a FIFO cannot block that open.

## Filesystem and persistence prerequisites

| Path | Purpose |
|---|---|
| `/opt/zionos/apps/<id>/.manager-owner` | Manager-owned namespace marker; foreign directories are refused |
| `/opt/zionos/apps/<id>/versions/<version>` | Versioned executable snapshot, root owned, files 0755/0644; not writable by applications |
| `/opt/zionos/apps/<id>/versions/.stage-<operation>` | Private staging on the publication filesystem |
| `/opt/zionos/apps/<id>/current` | Atomic relative reference to exactly one retained version |
| `/opt/zionos/apps/<id>/.removed-<operation>` | Uninstall quarantine, cleaned after registry commit |
| `/var/lib/zionos/packages/registry.sqlite3` | Root-only registry/journal; separate from application data |
| `/var/lib/zionos/apps/<id>/<slot>` | Dedicated application's persistent data |
| `/etc/zionos/apps/<id>/runtime.toml` | Root-owned administrator configuration; writable slots are separate children |
| `/var/log/zionos/apps/<id>` | Runtime's root-controlled log files |

Production administration is limited to root on the target Alpine/ZionOS system.
All five application/registry base paths must be on writable local ext4, XFS or
Btrfs, with trusted non-symlink, non-group/world-writable ancestry. tmpfs, live
overlay, network and read-only targets are refused. This is a filesystem prerequisite,
not proof of power-loss durability or correctly provisioned NAS storage. The manager
does not format devices, mount volumes, configure boot mounts or add itself to the ISO.

Operators must provision persistence and restore mounts **before** running the CLI.
OpenRC definitions/runlevels and Unix account databases also require persistent OS
configuration for unattended boot autostart. On a live ISO those are volatile:
`start` reconstructs the owned registration/identity after the application's persistent
mounts return, but `list` is not a boot-time service activation hook. No automatic
manager boot hook or persistent-volume provisioning is implemented.

## Durable transaction and recovery contract

```text
PREPARED -> STAGED -> ACTIVATING -> COMMITTED
    \         \           \----> FAILED / RECOVERY_REQUIRED
     \---------\---------------> FAILED / RECOVERY_REQUIRED
```

1. Inspect archive, verify hashes/target/OS/upgrade compatibility and validate dependencies.
2. Require explicit development trust and an exact approval of declared permissions.
3. Persist PREPARED intent containing old/new records and observed prior service state.
4. Extract privately, normalize modes, fsync files and directories, persist STAGED.
5. Persist ACTIVATING **before** changing service registrations or live references.
6. Stop/unregister the old service. Publish a new immutable version without overwriting
   an existing path, or quarantine the tracked version tree for uninstall.
7. Atomically write administrator configuration and switch `current`; revalidate with
   Runtime, including full ELF dependencies and stored manifest/inventory digests.
8. Create/validate a locked dedicated nologin UID/GID and register through Runtime.
   Start only when requested or when replacing a previously running service. If started,
   require Runtime health within a bounded wait. Otherwise report that health was not run.
9. Atomically commit the registry and COMMITTED outcome in SQLite. Perform cleanup
   afterward; its durable cleanup flag permits retry after a crash.

Normal errors compensate to the previous executable/configuration/registration and
restart it if it was running. If compensation cannot complete, RECOVERY_REQUIRED
blocks activation and lifecycle operations. Staging errors are FAILED after cleanup;
unremovable staging requires recovery. Journal states and record digests are validated
before use; a corrupt registry is never silently reinitialized.

Every ordinary CLI startup reconciles incomplete intents under the global lock.
PREPARED/STAGED are discarded without changing the confirmed version. ACTIVATING
is compensated, including partially switched config/current or an uninstall quarantine.
COMMITTED cleanup is resumed without reverting its registry decision. Interrupted
operations are not silently promoted to success. `repair --plan` deliberately skips
filesystem/service reconciliation and emits a diagnostic plan only, including for a
failed initial install with no installed record. SQLite may perform its own hot-journal
recovery when opening the database; this is separate from package repair.

Application data is **never** included in executable rollback. Data/configuration
schema changes, slot-kind changes and removal of old storage declarations are refused.
There are no migration hooks. A failed new binary may already have changed data in a
declared writable slot; executable compensation does not undo those writes. Backups,
application-aware migrations and power-loss guarantees remain separate requirements.

## Registry and bounded operation

Records retain active/previous and up to 64 immutable versions, original manifest,
archive/manifest/checksum-inventory digests, source path, dependencies, desktop entries,
approved declarations, Runtime policy description and effective writable slot grants.
`service_state` is the **last observed** state; `status` queries Runtime for current state.
The stored policy describes requirements; it does not assert a stopped process is sandboxed.

Uninstall transactionally moves the last record into a separate `retained_data` table.
It is not an installed dependency or executable version, but reserves its UID/GID and
data/config schema even when a live OS loses its volatile Unix accounts after reboot.
Reinstall reuses that identity and checks preserved-data compatibility before staging;
it never silently allocates a new owner or accepts an incompatible data schema. The
retained-data record is cleared only in the transaction committing a successful reinstall.

The manager uses a nonblocking exclusive lock for reads and mutations, preventing
concurrent installation/recovery. A second caller gets an explicit busy error. Local
root administrators and the kernel are trusted; do not modify managed references or
definitions concurrently outside this coordinator.

Input is at most 64 MiB compressed; Foundation expansion/file/entry limits and Runtime
inventory/ELF limits still apply. The compressed snapshot stays in memory to prevent
source replacement between inspection and extraction; extracted content streams to disk.
Registry size is capped at 64 MiB, individual records/intents at 8 MiB, installed packages
at 1024 and retained-data identities at 1024. Successful cleanup is recorded so every CLI call need not replay historical
snapshots. Journal compaction, version pruning and log retention are not implemented;
budget/space exhaustion fails explicitly and may require administrator maintenance.
Only ext4 persistence is integration-tested here; XFS/Btrfs are allowed prerequisites
but their crash behavior has not been exercised by this suite.

## Permission, dependency and lifecycle limits

`--permissions` must match the manifest declarations exactly. It grants only declared
private data/config slots through Runtime's existing Linux policy; it grants no root,
devices, arbitrary shell, NAS mounts or public HTTP ports. Network client/listener
declarations are rejected because the tested Runtime profile cannot grant them.
Desktop declarations do not publish HTML into Zion Core's unauthenticated origin.
See [Runtime enforcement versus planned controls](RUNTIME.md).

The resolver accepts dependencies already installed at compatible versions. Missing
packages, cycles, conflicts, incompatible reverse dependencies and automatic `replaces`
are rejected. Uninstall refuses installed dependents. Before upgrading/rolling back a
dependency, stop its transitive dependent services explicitly; multi-package activation
and dependent restarts are not automatic. Dependencies are not automatically downloaded,
started or upgraded, and do not grant cross-package filesystem access. No ordinary ZPK
package can request changes to Alpine system packages.

Services run foreground processes. Multiple desktop entries can share one service.
An on-demand package with one native desktop entry selects it; ambiguous native entries
require `--desktop-entry ID`. Static desktops install assets without a daemon. UI-origin
isolation, window activation and broker integration remain unimplemented. Archives with
neither a service nor a desktop activation declaration are explicitly unsupported by
the current Runtime model; one-shot jobs are not silently treated as healthy daemons.

`stop`, `status` and bounded `logs` remain available from trusted registry metadata
when payload verification fails. Start/restart/upgrade require valid installed content.
An uninstall currently requires a verifiable active snapshot; corrupt content needs
diagnosis/backup restoration rather than unchecked recursive removal. Unknown version
directories and foreign service definitions are preserved and reported as errors.

Uninstall removes tracked executable versions, the active reference, runtime configuration
and owned service registration. Data, configuration slots, logs and locked identities
are preserved. **Data deletion is not implemented**: deletion flags are rejected; no
package-manager command can silently purge data or reuse the retained identity.

## Commands

From WSL/Linux repository root, ordinary developer checks do not install host services:

```sh
bash tests/zpk/manager-check.sh

# Build the target CLI; keep this output separate from Forge's own zpk binary.
CARGO_TARGET_DIR="$HOME/.cache/zionos/zpk-manager-target" \
  cargo build --locked --manifest-path crates/zpk-manager/Cargo.toml --release \
  --target x86_64-unknown-linux-musl

# Equipped local environment only: existing Genesis ISO, QEMU, musl-tools,
# xorriso, qemu-img, e2fsprogs and a PREPARED Forge builder.
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge build \
  --project examples/zion-hello-rust --prepare-builder
bash tests/zpk/manager-check.sh --vm
```

The VM test formats only a newly created regular image file, converts it to dedicated
disposable QCOW2, attaches no physical disks and accepts only its exact QEMU serial.
It mounts that test filesystem, installs the owned Forge packages, shuts down, boots
again, verifies persistence and uninstalls. A third boot reinstalls them in reverse
order to verify retained identities/data, then uninstalls again. Never run the guest scripts on the host.
Docker is used only for Forge's compilation, never for installed application execution.

On the **prepared target Alpine system only**, with the trusted Runtime helper at
`/usr/local/libexec/zpk-runtime` and the manager CLI on PATH:

```sh
zpk install ./rust-1.0.0.zpk --trust-local-development \
  --permissions background-service,persistent-data --start --unix-health
zpk list
zpk info org.zionos.zion-hello-rust
zpk status org.zionos.zion-hello-rust
zpk stop org.zionos.zion-hello-rust
zpk start org.zionos.zion-hello-rust
zpk restart org.zionos.zion-hello-rust
zpk logs org.zionos.zion-hello-rust
zpk logs org.zionos.zion-hello-rust --stderr
zpk verify org.zionos.zion-hello-rust
zpk upgrade ./rust-1.1.0.zpk --trust-local-development \
  --permissions background-service,persistent-data --start --unix-health
zpk rollback org.zionos.zion-hello-rust
zpk repair --plan org.zionos.zion-hello-rust
zpk uninstall org.zionos.zion-hello-rust
```

An upgrade manifest must explicitly accept the installed version via `upgrades_from`.
The VM fixture generates 1.1.0 with `>=1.0.0, <2.0.0`; changing only a version number
does not bypass compatibility. Empty permission lists use `--permissions none`.
`--autostart` explicitly enables runlevel registration; an existing autostart/Unix-health
selection is preserved by default on upgrade. Manager does not add an autostart toggle
command; Runtime's separate administration interface owns that capability, and manual
policy changes require coordinated registry maintenance rather than silently overriding it.

Forge's original Cargo commands remain valid. Both crates name their standalone CLI
`zpk`; do not overwrite one with the other. The optional `zpk forge` dispatcher uses only
a separately installed trusted `/usr/local/libexec/zpk-forge` binary and refuses root
execution. Missing helpers fail explicitly. Forge is not a manager runtime dependency.
Use separate Cargo target directories if developing both CLI binaries concurrently.

See [executed verification and limitations](MANAGER_VERIFICATION.md).
