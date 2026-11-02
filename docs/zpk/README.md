# Zion Package Format — v1 foundation

ZPK is an independently designed, native Linux application archive. This foundation
defines declarations and offline inspection only. It does not install applications,
start services, grant permissions, modify storage, or extend the HTTP API. No Docker
images or container runtime are part of ZPK. Genesis remains at 0.6.1.

[ZPK Forge (TASK 01)](FORGE.md) adds isolated source builds and offline native packaging.
Docker is used only for development builds/tests, not by produced applications at runtime.

[Native ZPK Runtime (TASK 02)](RUNTIME.md) adds dedicated identities, native process
lifecycle through OpenRC and tested Linux policy layers. Real integration uses a
disposable QEMU VM, not a runtime container. No package installer or authenticated
Core API bridge is included.

[Native Package Manager (TASK 03)](MANAGER.md) adds the local administrator installer,
SQLite registry, durable operation intents, dependency checks, upgrades and executable
rollback. It reuses Foundation/Runtime; no write routes are added to Zion Core. Persistent
target filesystems and explicit trusted-local development policy are mandatory.

[Zion App SDK (TASK 04)](APP_SDK.md) adds Manager's derived desktop registry,
read-only discovery/asset routes, shared desktop windows and an opaque-iframe SDK.
The [native Zion Hello example](../../examples/zion-hello/README.md) is tested in
Alpine/QEMU. Files/settings and an authenticated native permission broker remain
unavailable; the current Core API still has no authentication or write routes.

**Unsigned ZPK files are limited to trusted local development. SHA-256 detects
corruption and inventory changes; it does not authenticate publishers. An attacker
can replace both content and checksums.** Valid inspection is not permission to run
arbitrary binaries or display arbitrary package HTML in the ZionOS origin.

- [Format v1 and manifest reference](FORMAT_V1.md)
- [Validation, threat boundaries and review checklist](VALIDATION.md)
- [Native hello example](../../examples/zpk-hello/README.md)
- [Fixtures and regression tests](../../tests/zpk/README.md)

## Code and maintenance decisions

`crates/zpk-schema` contains the typed Serde schema, strict manifest validation,
bounded archive inspector and `zpk-inspect` CLI. `zpk-common` is deliberately absent:
there is one consumer and no established shared functionality. A second consumer
can use this library directly; split shared utilities only when the boundary is useful.

A root Cargo workspace was evaluated and deferred. The existing core uses
`core/api/Cargo.lock`, manifest-relative commands, a per-workspace Cargo target cache
and a musl staging script. Migrating to a root workspace would relocate dependency
resolution/lockfile ownership and affect those commands for no runtime benefit in
TASK 00. ZPK has its own committed `Cargo.lock`; core's manifest, lockfile, source,
staging and ISO inputs are unchanged. Use `--locked` for reproducible dependency
selection. This does not claim byte-for-byte reproducible compiler output.

The inspector and Forge are not staged in the ISO. Their zstd C build needs a C toolchain;
Rust, Cargo, rustfmt and Clippy are needed for development. PNG validation is bounded
and runs only on requested inspection. There are no always-on ZPK services.

## Build and validate

From the repository root in Ubuntu WSL/Linux:

```sh
cargo fmt --manifest-path crates/zpk-schema/Cargo.toml --check
cargo test --locked --manifest-path crates/zpk-schema/Cargo.toml
cargo clippy --locked --manifest-path crates/zpk-schema/Cargo.toml --all-targets -- -D warnings

rustup target add x86_64-unknown-linux-musl
# Install musl-tools through your development environment if musl-gcc is missing.
bash examples/zpk-hello/build-package.sh
cargo run --locked --manifest-path crates/zpk-schema/Cargo.toml --bin zpk-inspect -- \
  build/zpk/hello-1.0.0.zpk 0.6.1
```

The example builder refuses to overwrite its output. Supply a new output path as
its first argument for another run. Inspection prints JSON and exits 0 only after
archive validation and the requested OS/upgrade compatibility checks pass. It exits
nonzero with a diagnostic on any failure. An optional third CLI argument is the
installed version of the **same package**, used to check `upgrades_from` and prohibit
downgrades/reinstall. CLI callers are responsible for providing the actual OS version
and installed-package identity; this inspector does not consult Manager's database.

Library use:

```rust,no_run
use std::fs::File;
use zpk_schema::{inspect, Limits, Manifest};
let manifest = Manifest::parse(&std::fs::read_to_string("zpk.toml")?)?;
let report = inspect(File::open("hello.zpk")?, Limits::default())?;
report.manifest.check_compatibility(&"0.6.1".parse()?, None)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Manifest::parse` checks structure and declarations; it cannot verify payload files.
`inspect` checks payloads and hashes but does not assume a host version.
`check_compatibility` performs the separate OS/upgrade version check. Public structs
support Serde round-tripping; call `validate` after constructing or mutating one.

Existing Genesis commands remain valid:

```sh
cargo fmt --manifest-path core/api/Cargo.toml --check
cargo test --locked --manifest-path core/api/Cargo.toml
cargo clippy --locked --manifest-path core/api/Cargo.toml --all-targets -- -D warnings
bash tests/check-safety.sh
node --test tests/desktop.test.cjs
bash scripts/build-core.sh
```

## Next boundaries

Before production distribution: authenticated publisher policy, trusted repository/index
design, aggregate resource quotas, application-aware data migrations/backups,
desktop origin isolation and an authenticated permission broker. Local development
installation, dedicated identities, dependency checks and executable transactions are
implemented separately in Manager/Runtime, with their documented limitations.
Do not turn the current declaration parser into privileged execution of package
scripts. There are no lifecycle scripts, root services or HTTP write endpoints here.
