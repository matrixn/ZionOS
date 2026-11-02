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
