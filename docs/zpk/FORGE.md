# ZPK Forge — TASK 01

Forge builds native local source projects in a disposable constrained Alpine container,
then creates independently validated `.zpk` archives offline. The application does not
call Docker or depend on an OCI runtime. No installer, privileged hooks, HTTP write API
or production package repository is included. Unsigned artifacts remain for trusted
local development; checksums and provenance do not authenticate their publisher.

## Supported adapters

| System | Real build command / staging contract | Verified |
|---|---|---|
| Rust/Cargo | `cargo build --offline --locked --release --target <builder-musl-target> --bin <name>`; controlled output `build/forge-app` | Native Rust example, full rebuild equality and execution in minimal Alpine |
| C and C++/CMake | Out-of-source configure/build, prefix `/usr`, `DESTDIR=/work/dest` install; explicit stage rules | C and C++, including transitive libstdc++/libgcc collection, minimal Alpine runtime |
| Go | Pure Go, `GOOS=linux GOARCH=amd64 GOAMD64=v1 CGO_ENABLED=0`, offline proxy/toolchain policy, explicit package, controlled `build/forge-app` | Actual Go build, full rebuild equality and minimal Alpine runtime |
| Make | Explicit target and declarative stage mappings; `DESTDIR=/work/dest` environment | Actual C Make project, full rebuild equality and minimal Alpine runtime |

Zig, Meson and Autotools have no adapter. Go with CGO is not supported. Only Linux
x86-64/musl is supported. Architecture descriptors exist for aarch64, i686 and ARMv7,
but resolve to explicit unsupported errors until working toolchains, ABI validation
and build tests are added. They are not aliases for x86-64 builds.

## Commands

Run on Linux/Ubuntu WSL with a local Unix Docker endpoint. The host compiles only
Forge's trusted repository code; application build scripts run exclusively in Docker.
Keep Cargo's many small files on WSL's native filesystem:

```bash
cd /mnt/c/Users/radut/Workspace/zionos
export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target"
cargo build --locked --manifest-path crates/zpk-forge/Cargo.toml
ZPK="$CARGO_TARGET_DIR/debug/zpk"

# Existing complete Rust example (clean removes only its marker-owned generated state).
"$ZPK" forge clean --project examples/zion-hello-rust
"$ZPK" forge validate --project examples/zion-hello-rust
"$ZPK" forge build --project examples/zion-hello-rust --prepare-builder
"$ZPK" forge package --project examples/zion-hello-rust
"$ZPK" forge inspect examples/zion-hello-rust/.zpk-forge/org.zionos.zion-hello-rust-1.0.0.zpk
"$ZPK" forge validate examples/zion-hello-rust/.zpk-forge/org.zionos.zion-hello-rust-1.0.0.zpk

# CMake example uses the already prepared builder.
"$ZPK" forge clean --project examples/zion-hello-c
"$ZPK" forge build --project examples/zion-hello-c
"$ZPK" forge package --project examples/zion-hello-c
"$ZPK" forge inspect examples/zion-hello-c/.zpk-forge/org.zionos.zion-hello-c-1.0.0.zpk
```

`--prepare-builder` is explicit trusted setup: it downloads the pinned Alpine base
and Alpine 3.23 toolchains using the embedded Dockerfile/driver only. Application
source is never a Docker build context. Later builds require the local image to
match the embedded builder-source hash and amd64 architecture, then address it by
immutable image ID. Missing/mismatched builders fail with preparation instructions.
No application dependencies are fetched by Forge: supply reviewed Cargo/Go vendor
trees and configuration explicitly in `inputs`. Rust requires Cargo.lock. Buildkit
and Alpine package repositories remain trusted and online during preparation; APK
versions are rolling, so preparing on another date can change toolchains/image ID.

`package` never invokes Docker, compilers, hooks or network downloads. It requires a
successful current build and unchanged source fingerprints (content plus normalized
mode), authoring manifest, resolved manifest and staging hashes. It refuses stale
or tampered state rather than packaging an earlier build accidentally. Output creation
is atomic and refuses replacement. Choose `--output /existing/directory/new.zpk` to
preserve previous packages. A `.provenance.json` companion is written beside the ZPK.

`inspect` validates archive and ELF dependency closure without executing anything.
`validate FILE.zpk` performs the same checks with a brief result. `validate --project`
checks the recipe, schema and source inventory only, without compiling or resolving
dependencies. These commands do not check installed-package state or install anything.
Use Foundation's compatibility API when evaluating an actual OS/upgrade target.

## Initializing another project

```bash
"$ZPK" forge init --project /path/to/local/source
# For multiple detected systems:
"$ZPK" forge init --project /path/to/local/source --system cmake
```

Detection reads Cargo.toml/CMakeLists.txt/go.mod/Makefile; it does not run Cargo
metadata, CMake, Make or source probes on the host. Existing zpk.toml is never
overwritten. Init reports every `UNRESOLVED` field. Fill these in before validate/build.
Reverse-domain ID and unknown author/license information are not invented. Cargo
binary selection is inferred only for a single unambiguous candidate. CMake output
paths, Go package selection, Make targets and unknown source inputs require developer
input. Library-only Cargo workspaces and multi-binary projects need explicit selection.
The generated version is a package version default when no simple source version is
available; review all generated metadata, inputs and staging mappings.

An authoring `zpk.toml` is the shared v1 manifest plus a strict `[forge]` recipe:

```toml
[forge]
version = 1
system = "cmake"
inputs = ["CMakeLists.txt", "hello.c", "LICENSE"]
license_file = "LICENSE"
jobs = 2

[[forge.stage]]
from = "dest/usr/bin/zion-hello-c"
to = "payload/bin/zion-hello-c"
```

`inputs` is an explicit allowlist of relative files/directories, at most 256 roots.
All descendants are checked: no symlinks/hardlinks, special files, unsafe paths,
credentials or known generated directories. Overlapping/duplicate inputs fail.
Known sensitive names (.ssh/.aws/.docker/.env, credential files, private-key extensions,
etc.) are rejected, not silently uploaded. This is not content-based secret scanning:
the developer must review source files for embedded secrets. Do not mutate the source
tree concurrently; a hostile local host writer/administrator is outside the boundary.

Every declared file needs one unique staging mapping. `from` begins with `source/`,
`build/` or `dest/`, relative to `/work` inside the container. No `..`, absolute path
or shell expansion is allowed. `to` exactly matches a shared-manifest file declaration.
`license_file` is a source-relative file included in inputs and becomes root LICENSE.
The authoring `[forge]` table is removed from the resolved archive manifest; Foundation
retains its strict unknown-field checks. `build.system` must agree with the adapter.

Adapter fields: Rust requires `cargo_bin`; Go requires `go_package` (`.` or `./path`);
Make requires `make_target`. These fields are forbidden on other adapters. `jobs` is
1..8 (default 2), subject to the container's fixed two-CPU quota. CMake install code
and Make recipes may execute arbitrary commands, but only as the sandbox user.

## Isolation and resource contract

- Local Unix Docker daemon only: remote/TCP/SSH contexts fail. A trusted Docker
  daemon/CLI, host kernel and pinned trusted builder are prerequisites.
- No host bind mounts, Docker socket, credential files, block devices, additional
  capabilities, host PID namespace or privileged mode. Source is transferred as a
  generated canonical regular-file tar into an empty private tmpfs before compiling.
- UID/GID 10000, read-only image root, no-new-privileges, default seccomp, network none.
  Driver checks UID/caps/seccomp/network/root-readonly and actual cgroup v2 memory,
  PID and CPU limits before executing source. Unsupported isolation fails closed.
- Two CPUs, 1 GiB memory/no swap allowance, 128 PIDs, 512 MiB executable /work tmpfs,
  64 MiB noexec /tmp, bounded file descriptors/core dumps. No writable host output mount.
- Source/staging limits: 4096 entries, 64 MiB per file, 128 MiB total; shared ZPK
  limits remain independently enforced. Build deadline 600s; bounded output capture;
  setup deadline 1200s. Containers are removed on success/error/timeout via RAII.
- Returned staging tar is untrusted: host code validates entry types/names/counts,
  creates only regular files under a new private generation directory and never
  calls tar extraction on the host. All links/extensions/special files are refused.
- Shared-kernel Docker isolation is **not a disposable VM security boundary** and
  does not defend against container/kernel/daemon vulnerabilities. For sources whose
  risk exceeds that boundary, do not build them here; use a dedicated disposable build
  machine/VM externally. There is no unsafe host fallback or implicit VM adapter.

An unexpected host interruption/SIGKILL can bypass process cleanup. Inspect leftover
containers with the `zpk-forge-` prefix; don't delete unrelated Docker resources.
`clean` affects only the exact canonical marker-owned `.zpk-forge` under the chosen
project, refuses symlink/foreign state, and leaves source, Cargo caches and images intact.

## ELF libraries, CPU baseline and provenance

ELF is parsed in Rust, never with host `ldd`. Wrong architecture, non-musl interpreter,
unsafe DT_NEEDED names, unsupported RPATH/RUNPATH and unresolved closure fail. Native
executables use `$ORIGIN/../lib`; bundled libraries use `$ORIGIN`. Trusted patchelf
runs only inside the builder to configure these relative paths. Required libraries
are copied from the read-only trusted Alpine image, recursively bounded to 64 objects,
or may be supplied explicitly through declared library files. Nothing is downloaded.
Musl libc is a reported platform dependency supplied by ZionOS/Alpine, not bundled or
silently resolved from the developer host. `dlopen` plugins and symbol-version ABI
compatibility require developer review; DT_NEEDED is not a complete runtime resolver.

Rust uses target-cpu=x86-64 and explicit musl linkage; GCC/G++ use march=x86-64 and
mtune=generic; Go uses GOAMD64=v1, CGO disabled. ELF GNU ISA-needed properties above
x86-64-v1 are rejected. **This is limited verification, not proof of every executed
instruction.** Sources can override flags, use assembly or contain runtime dispatch;
absence of a v2+ note does not prove absence of AVX/SSE4.2. Build/package output and
provenance always state this limitation. Actual Core 2 hardware is not certified.

Provenance records immutable builder image/source hashes, compiler versions and
environment, source content/mode fingerprints, authoring/resolved manifest hashes,
staged hashes, dependency closure/platform libraries, isolation and CPU limitations,
and final package SHA-256. It is an unauthenticated local record, not attestation.
Archive order, UID/GID/mode/mtime, manifest/checksum serialization and compression are
deterministic for identical staging. Full rebuild equality is tested for the owned
examples; arbitrary upstream projects may embed time, randomness or external inputs.

## Verification

```bash
bash tests/zpk/forge-check.sh          # 16 offline tests, rustfmt, strict Clippy
bash tests/zpk/forge-check.sh --docker # additionally prepare builder and run real isolated tests
bash tests/zpk/check.sh               # existing 23 Foundation tests
bash scripts/build-core.sh           # existing Genesis checks/release/staging
bash tests/check-safety.sh
```

The two Docker test cases cover five native adapters/language variants, minimal
Alpine execution, missing compiler/build/staging errors, missing library closure and
two complete builds per example with byte-for-byte equal archives. Runtime execution
is restricted to owned test examples in a second unprivileged minimal Alpine container;
Forge inspection and normal builds never launch the application. See
[task verification record](FORGE_VERIFICATION.md) for executed results and remaining limits.
