# Zion Hello — native backend and isolated browser UI

MIT-owned Rust/Cargo example built using the real ZPK Forge. A foreground native
OpenRC service maintains its private state counter and Unix health socket. Its browser
UI imports Zion App SDK 1.0, changes its window title, reads/subscribes to theme changes,
shows a desktop notification and confirmation dialog, and demonstrates typed unavailable
file/settings capabilities. Multiple windows share the single backend service.

TASK 05 adds a real native persistent greeting (`state/greeting.txt`, mode 0600),
bounded private Unix-socket settings commands and atomic/fsynced writes. Browser
settings remain unavailable until a secure authenticated broker exists. The
[complete journey](../../docs/zpk/E2E.md) covers Forge setup, target CLI, interactive
VM, three boots, upgrade/failed-start recovery and uninstall.

```sh
bash scripts/test-vm.sh --zpk-e2e       # three boots on one new persistent QCOW2
bash tests/e2e/check.sh --interactive  # desktop + local status/settings/upgrade commands
```

Each run retains its unique `build/e2e-XXXXXX/` evidence/disk; existing images are
never overwritten. `--fail-start` is an explicit failure fixture, not normal startup.

```sh
npm --prefix web/zion-app-sdk ci --ignore-scripts --no-audit --no-fund
npm --prefix web/zion-app-sdk run build
python3 examples/zion-hello/build-assets.py
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge build \
  --project examples/zion-hello --prepare-builder
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge package \
  --project examples/zion-hello --output build/zpk/zion-hello-1.0.0.zpk
```

Choose a fresh output path for each package; Forge intentionally refuses overwrite.
Icon generation and SDK copying are deterministic and use no image/network libraries.
The application has no Node, Docker, display-server or remote asset dependency at runtime.
The UI uses standard ES modules served as registered assets; it receives no backend
socket, filesystem path, native public port or administrator credential.

For actual install/desktop integration use the prepared target's local CLI or the
guarded test VM, never the Windows/WSL host:

```sh
bash tests/apps/check.sh --browser  # mock services in private temporary roots
bash scripts/build-iso.sh
bash tests/apps/check.sh --vm       # real native Alpine/OpenRC + browser
```

[Target CLI commands, persistence/trust prerequisites and security limits](../../docs/zpk/APP_SDK.md).
