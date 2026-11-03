# zpk-control — Genesis 0.7 control plane

- [x] Root-only target platform CLI; no host installation or package execution in unit tests.
- [x] Explicit UUID/preformatted ext4 validation, read-only fsck/marker inspection,
  pinned device identity and persistent application mounts; no formatter/repair command.
- [x] Shared ZionOS system owner, code-free localhost enrollment, named login,
  legacy account migration, Argon2id, persistent sessions,
  server-side CSRF validation, expiration, logout and login throttling.
- [x] Versioned bounded Unix IPC, SO_PEERCRED at both ends, exactly one validated
  preview descriptor, root-owned session-bound package snapshots and permission review.
- [x] Existing Manager/Runtime reuse, serialized Manager access, durable operation
  phases/audit, catalog imports and startup reconciliation/identity restoration.
- [ ] Production LAN/TLS administration, publisher authentication and general app capability broker.

`zion-platform` is a target administrator/bootstrap tool. Never invoke its native
volume/service functions on the Windows/WSL host. The `vm-tests` feature exposes only
a separately built guarded test coordinator; it is not installed in the ISO.

```sh
cargo fmt --manifest-path crates/zpk-control/Cargo.toml --check
cargo test --locked --manifest-path crates/zpk-control/Cargo.toml --all-features
cargo clippy --locked --manifest-path crates/zpk-control/Cargo.toml --all-targets --all-features -- -D warnings
```

Library unit tests use owned temporary files, socket pairs and a metadata-only work
closure. They do not mount devices, create service users or execute application code.
Native integration is tested separately in disposable QEMU only.

The production ISO includes this adapter, Manager/Runtime and the authenticated
Core/desktop facade. Start with the [developer guide](../../docs/GENESIS_07.md),
[architecture/security contract](../../docs/GENESIS_07_ARCHITECTURE.md) and
[executed tests](../../docs/GENESIS_07_VERIFICATION.md).
`bash tests/package-center/check.sh --vm` tests the real graphical native workflow;
`bash scripts/run-qemu-package-center.sh` exposes the same desktop interactively.
