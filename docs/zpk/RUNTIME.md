# Native ZPK Runtime — TASK 02

`crates/zpk-runtime` runs installed ZPK executables as native Linux processes under
OpenRC. There is no Docker/OCI/Kubernetes backend or runtime dependency. The runtime
does not install packages, unpack archives, run package-provided privileged scripts,
or add HTTP routes. Genesis Core remains unauthenticated and read-only.

## Components and application classes

- `RuntimeConfig` is a separate, versioned administrator-owned policy model. The ZPK
  manifest remains init-system independent. Runtime grants are explicit subsets of
  declared persistent directories; declarations do not grant OS privileges.
- `Plan` validates installed version, identity, activation, resource limits and
  directory grants. `ServiceBackend` is the lifecycle boundary; `OpenRc<Operations>`
  delegates start/stop/restart/status/autostart to OpenRC. Unit tests supply mock
  operations, not host service commands. Alternative init adapters can implement the
  interface later; none are advertised now.
- The trusted launcher applies security controls and directly `fexecve`s the pinned
  executable. It does not remain as another parent supervisor. OpenRC's
  `supervise-daemon` performs bounded respawn, process-group termination and shutdown.
- A background service runs its manifest executable/arguments in the foreground.
  Multiple desktop entries can resolve to the same shared service. On-demand backends
  are not added to a boot runlevel; the caller explicitly starts/stops them. A static
  desktop without a native backend has state `not-required` and no process/service.
- [TASK 04](APP_SDK.md) implements UI asset publication and desktop windows without
  privileged HTTP activation. Native lifecycle leases and authenticated activation
  remain future work. No permanent daemon is created just to represent UI.
  One-shot CLI tasks are not modeled as long-running supervised services.

The shared ELF reader moved from Forge into `zpk-common` because Forge and Runtime
now both consume it. Forge retains its public re-export. Runtime does not depend on
Forge, build adapters, a Docker CLI or container libraries. Core's independent Cargo
manifest/lockfile and ISO staging are preserved.

## Filesystem and trust model

| Path | Owner / purpose |
|---|---|
| `/opt/zionos/apps/<id>/versions/<version>/` | Root-owned application snapshot, read-only to applications; manifest, checksums, LICENSE, native payload/UI/assets |
| `/opt/zionos/apps/<id>/current` | Only permitted installed symlink: exact relative `versions/<configured-version>` |
| `/var/lib/zionos/apps/<id>/<data-slot>/` | Dedicated UID/GID, mode 0700; only explicitly granted declared slots are writable |
| `/etc/zionos/apps/<id>/runtime.toml` | Administrator policy, outside writable configuration slots |
| `/etc/zionos/apps/<id>/<config-slot>/` | Private application configuration; declared grant required for writing |
| `/var/log/zionos/apps/<id>/` | Root-owned 0700, private 0600 stdout/stderr files; application receives only stdio descriptors |
| `/run/zionos/apps/<identity>/` | Dedicated UID/GID, 0700, ephemeral private Unix socket/tmp directory |
| `/run/zionos/zpk-supervisors/` | Root-owned 0700 supervisor pidfiles and package identity binding; application cannot tamper with OpenRC control files |

"Immutable" means root-controlled and not application-writable, not a filesystem
immutable flag protecting against an administrator. All installed ancestors and
regular files are checked for root ownership/non-writable group/other modes. Inventory,
strict checksums, ELF/UI/icon content, package/OS versions and dependency presence
are validated before execution. Path traversal, unlisted files and symlinks other
than the bounded current reference fail. Native dependencies must be bundled or
the supported musl platform library. Cross-ZPK dependency declarations check identity
and version availability, not sibling service readiness or implicit shared-library access.

Applications use a deterministic `zp_<20 hex>` account name, dedicated UID/GID in
10000..60000, locked password, nologin shell and no foreign primary/supplementary group
members. The administrator must provision this account; Runtime never invokes adduser
or package installation hooks. The process drops all supplementary groups and real,
effective and saved IDs before exec. Different applications must not share identities.

The live Genesis ISO has **no assumed persistence**. `ephemeral-development` explicitly
permits RAM-backed VM fixtures. `persistent-required` refuses tmpfs/overlay and accepts
only ext4/XFS/Btrfs for application version, data, configuration and logs. This checks
filesystem type, not storage durability, backups or power-loss safety. Operators must
provision persistent volumes/mounts externally. Runtime does not mount, format or repair
anything. Restart persistence in the test is not reboot/power-loss persistence.

## Lifecycle and CLI

Build/check the trusted runtime as an ordinary developer in Linux/WSL:

```bash
export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target"
cargo build --locked --manifest-path crates/zpk-runtime/Cargo.toml
bash tests/zpk/runtime-check.sh

# Disposable VM integration: existing Genesis ISO plus a second read-only fixture CD.
# If the base image is missing: bash scripts/build-iso.sh
rustup target add x86_64-unknown-linux-musl
bash tests/zpk/runtime-check.sh --vm
```

VM prerequisites: QEMU, xorriso, musl toolchain, existing Genesis 0.6.1 ISO. KVM is
selected if usable; otherwise TCG is available (slower). Set `ZPK_RUNTIME_ACCEL=kvm`
or `tcg` to choose. No disk image, physical passthrough or host forwarding is attached.
The guarded guest fixture refuses writable disks and runs only on QEMU/Genesis. Host
accounts, services and NAS directories are not changed. Build media/binaries/logs are
under ignored `build/`. Runtime VM tests use a lock to prevent fixture-media overlap.

On the **target Alpine system only**, after trusted installation/account/volume setup:

```sh
zpk-runtime prepare --config /etc/zionos/apps/org.example.app/runtime.toml
zpk-runtime start --config /etc/zionos/apps/org.example.app/runtime.toml
zpk-runtime status --config /etc/zionos/apps/org.example.app/runtime.toml
zpk-runtime healthcheck --config /etc/zionos/apps/org.example.app/runtime.toml
zpk-runtime logs --config /etc/zionos/apps/org.example.app/runtime.toml
zpk-runtime logs --stderr --config /etc/zionos/apps/org.example.app/runtime.toml
zpk-runtime restart --config /etc/zionos/apps/org.example.app/runtime.toml
zpk-runtime autostart --config /etc/zionos/apps/org.example.app/runtime.toml on
zpk-runtime autostart --config /etc/zionos/apps/org.example.app/runtime.toml off
zpk-runtime stop --config /etc/zionos/apps/org.example.app/runtime.toml
```

Administrative lifecycle commands require root and canonical per-package configuration.
`prepare` creates controlled directories and a trusted generated OpenRC definition,
not a package installer. Autostart adds/removes only that service in the default
runlevel and saves configuration. Stop/disable remain available if current/payload
validation is broken. Static desktops do not create a service. A temporary `--root`
is supported only by the pure `plan` command; unit lifecycle tests use mocks.

Status distinguishes stopped/starting/running/stopping/inactive/crashed/unknown and
no-process-required. A started OpenRC service is not called running until its recorded
child UID/executable are observed in `/proc`. Restart counts and exhausted-budget
failure come from OpenRC, not a second runtime supervisor. Status is an observation,
not a guarantee the process will remain alive. Start acknowledges supervision setup,
not functional readiness. Foreground daemons must not double-fork.

Health modes: `process` (default) checks primary process liveness only;
`unix-socket` uses the private `service.sock`, verifies SO_PEERCRED against the dedicated
UID/GID, and exchanges `ZPK_HEALTH_V1\n` → `OK\n\n`, with bounded connect/read/write.
Static UI health is not applicable and reports no process required. No package-supplied
health executable is ever run as root. Manual OpenRC healthcheck invokes this same
trusted client; periodic health monitoring is not enabled by default.

Logs are bounded tail reads, serialized as JSON by the CLI to avoid rendering terminal
escape sequences. They are application-controlled output, **not tamper-proof audit
records**. No log rotation/retention service is installed; per-file limits can terminate
a verbose process. Operational rotation and transactional runlevel/config updates
need further production work. Control commands revalidate installed content and may
be expensive for large packages; there is no always-on polling service in this task.

## Enforced policy versus declarations/plans

| Property | Implementation and evidence |
|---|---|
| Non-root identities | Real/effective/saved UID/GID match, no supplementary groups, locked nologin account; verified in disposable VM |
| Linux capabilities | Effective/permitted/inheritable, ambient and bounding sets cleared; no_new_privs required; VM verifies resulting sets |
| File content/access | Required Landlock ABI >=6, deny by default; read-only version, executable allowlist, declared private data/config grants and private run directory; VM denies world-readable/writable synthetic NAS file, passwd, devices and host shell |
| Network and IPC | x86-64 seccomp denies IP/non-stream sockets, outbound connect, addressed sendto, FD passing, SysV IPC, tracing, namespace creation, io_uring and privileged syscall families; ordinary private Unix stream server remains usable |
| Process signals | Landlock signal/abstract-Unix scopes plus dedicated UID; no PID/mount/network namespace is created |
| Resource limits | Hard RLIMIT_AS/CPU/NOFILE/NPROC/FSIZE/CORE/MEMLOCK; actual allocation, descriptor, fork, file and CPU limit failures tested |
| Directory permissions | Root-controlled ancestors/control/logs; application-owned 0700 slots; no general NAS volume path grant |
| Declared network/file permissions | Metadata alone grants nothing. Runtime has no network-client/listener or NAS-volume override; unsupported capabilities need a future broker |
| Not enforced | Aggregate cgroup limits, filesystem/project quotas corresponding to storage.max_bytes, mount/PID namespaces, complete syscall allowlist, metadata/side-channel secrecy, publisher authentication, UI-origin isolation, persistent audit/rotation, reboot durability |

The policy fails closed when Landlock, seccomp or identity setup is unavailable; it
does not silently downgrade to DAC-only operation. Linux x86-64 is the only tested
profile. This is a tested set of restrictions, **not a claim of VM-equivalent or
complete sandbox security**. Kernel/root administrators are trusted. RLIMIT_AS bounds
virtual address space, not resident service-wide memory; CPU/FSIZE are per process/file
and NPROC is per UID. An application can consume aggregate resources across children
within those limits; production hostile-package deployment needs aggregate controls.
Landlock does not hide all stat/readlink metadata. Existing stdio descriptors are
explicit capabilities; the environment and all other descriptors are cleared at exec.

## Future authenticated broker

The current unauthenticated Core API receives no lifecycle or write routes. A future
small broker must authenticate the ZionOS user/session, bind the process to a registered
package identity using Unix peer credentials, authorize each operation and emit an audit
record. Its transport should use private filesystem Unix sockets and bounded, versioned
messages. Under this server-only profile, the trusted broker connects into the package's
private socket; the package cannot connect to arbitrary system sockets.

Proposed capabilities are scoped file reads/writes, notifications and typed system
queries, with explicit user grants, quotas, revocation and denied-by-default policy.
No raw device handles, arbitrary shell commands or automatic NAS mounts should be
exposed. Current seccomp denies SCM_RIGHTS sendmsg/recvmsg; future FD grants require
a reviewed profile change and a revocation/lifetime design. No broker daemon, tokens,
user authorization or native notification broker is implemented here. TASK 04's
browser-only desktop bridge and permission-checked notifications are separate from
this proposed privileged native transport; see [APP_SDK.md](APP_SDK.md).

Primary design references: [OpenRC supervision](https://github.com/OpenRC/openrc/blob/master/man/supervise-daemon.8),
[OpenRC service model](https://github.com/OpenRC/openrc/blob/master/man/openrc-run.8),
[Linux Landlock ABI](https://docs.kernel.org/userspace-api/landlock.html).
See [runtime verification record](RUNTIME_VERIFICATION.md) for executed tests and limits.
