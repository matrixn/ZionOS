# ZPK regression suite

Run from the repository root:

```sh
bash tests/zpk/check.sh
```

`validation.rs` is a Cargo integration test target of `crates/zpk-schema`.
`fixtures/invalid/*.toml` are all expected to fail; architecture/CPU/schema fixtures
also assert the corresponding explicit error variants. Valid service, single UI and
multiple UI manifests are in `fixtures/valid/`; the headless application manifest is
`examples/zpk-hello/zpk.toml`.

Malicious binary archives are generated in memory by test helpers rather than stored
as opaque compressed blobs. Test names describe the attack class; archive checksums
are regenerated only when the test intends to reach a later validation layer. No
test extracts payloads, executes a package, formats a disk or needs privileges.

Real-ELF authoring/inspection instructions: [hello example](../../examples/zpk-hello/README.md).

## Forge

`forge.rs` is a separate integration target of `crates/zpk-forge`. Run
`bash tests/zpk/forge-check.sh` for 16 offline tests and strict checks, or
`bash tests/zpk/forge-check.sh --docker` to prepare the trusted builder and execute
the two opt-in Docker cases as well. These cases compile only inside the constrained
builder, run owned hello fixtures in a second unprivileged minimal Alpine environment,
and compare two complete builds each of Rust/CMake/Go/Make/C++.

Source/manifest/stage errors, missing compilers, failed builds, dependency closure,
architecture/ISA-note mismatches, bounds, symlink/credential rejection and safe cleanup
are covered. Offline tests use only generated regular-file fixtures and fixed owned
process helpers; they never run third-party application build scripts on the host.

## Native runtime

`runtime.rs` contains 13 tests with mock OpenRC operations and regular-file temporary
roots. `bash tests/zpk/runtime-check.sh` runs these plus strict formatting/Clippy.
`bash tests/zpk/runtime-check.sh --vm` compiles the owned native demo and administrator
harness, creates a read-only fixture CD and boots a disposable QEMU guest. It does
not create host users, install host services, mount host NAS volumes or attach a
writable disk. Never run `runtime-guest.sh` directly on the host.

Real VM checks include dedicated non-root identity, capability/descriptor clearing,
Landlock/seccomp enforcement, memory/FD/process/file/CPU limits, native Unix health,
logs, clean shutdown, state across restarts, crash respawn/exhaustion, autostart,
stop with broken current reference, and refusal of persistent-required on live RAM.
