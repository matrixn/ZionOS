# App SDK verification

Run from repository root in WSL/Linux:

```sh
npm --prefix web/zion-app-sdk ci --ignore-scripts --no-audit --no-fund
bash tests/apps/check.sh
(cd web/zion-app-sdk && npx playwright install chromium)
cargo run --locked --manifest-path crates/zpk-forge/Cargo.toml -- forge build --project examples/zion-hello --prepare-builder
bash tests/apps/check.sh --browser
bash scripts/build-iso.sh
bash tests/apps/check.sh --vm
```

Default: generated distributions/types, syntax and Node fixtures. `--browser`: real
Chromium with non-root debug Core and Manager's private temporary test root, mocked
services and no native content execution on the host. `--vm`: actual local guest
Manager/Runtime, OpenRC and compiled sample inside the final Alpine ISO/QEMU.
Prepare QEMU, qemu-img, xorriso, e2fsprogs, musl and Chromium dependencies first.

The image helper formats only a newly created regular file, then converts to dedicated
QCOW2. There is no physical disk passthrough. Port 8080 must be free; tests never kill
existing VMs. Do not run `guest.sh` on the host. Artifacts stay under ignored `build/`.
See [design/limits](../../docs/zpk/APP_SDK.md) and
[executed verification](../../docs/zpk/APP_SDK_VERIFICATION.md).
