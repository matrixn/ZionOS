# zion-hello-c

MIT-licensed native C/CMake example. CMake builds out of source and installs to the
container's temporary DESTDIR; Forge stages only the declared executable/license.
The package is dynamically linked to musl, supplied by the Alpine/ZionOS platform;
Forge configures relative RUNPATH and reports dependencies without executing host ldd.

From repository root, after building the Forge CLI:

```bash
"$CARGO_TARGET_DIR/debug/zpk" forge build --project examples/zion-hello-c --prepare-builder
"$CARGO_TARGET_DIR/debug/zpk" forge package --project examples/zion-hello-c
"$CARGO_TARGET_DIR/debug/zpk" forge inspect examples/zion-hello-c/.zpk-forge/org.zionos.zion-hello-c-1.0.0.zpk
```

An already prepared current builder lets you omit `--prepare-builder`. Outputs and
provenance remain under ignored `.zpk-forge/`; package refuses replacement. Clean with
`forge clean --project examples/zion-hello-c` or choose a fresh package output name.
See [the Forge guide](../../docs/zpk/FORGE.md) for prerequisites, limits and all commands.

TASK 03 adds a foreground `--serve` mode with a private Runtime Unix health socket
and a counter in the declared `state` data slot. The manifest explicitly requests
background-service/persistent-data; the no-argument print-and-exit behavior remains.
The manager VM test installs/starts/stops this real CMake-built package and verifies
its data across a second boot before uninstalling. No service runs on the developer host.
See [manager commands](../../docs/zpk/MANAGER.md).
