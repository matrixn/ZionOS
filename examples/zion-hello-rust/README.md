# zion-hello-rust

MIT-licensed native Rust/Cargo example, compiled offline inside Forge's constrained
Alpine builder. There are no external Rust crates, runtime Docker calls or installation
hooks. The supplied Cargo.lock is required. The package is musl-static and targets
the generic x86-64 baseline; complete ISA verification is not claimed.

From repository root, after building the Forge CLI:

```bash
"$CARGO_TARGET_DIR/debug/zpk" forge build --project examples/zion-hello-rust --prepare-builder
"$CARGO_TARGET_DIR/debug/zpk" forge package --project examples/zion-hello-rust
"$CARGO_TARGET_DIR/debug/zpk" forge inspect examples/zion-hello-rust/.zpk-forge/org.zionos.zion-hello-rust-1.0.0.zpk
```

Set CARGO_TARGET_DIR and build Forge as described in [the Forge guide](../../docs/zpk/FORGE.md).
To rebuild without replacing an old package, use a new `package --output` filename.
`forge clean --project examples/zion-hello-rust` removes its marker-owned generated
state and packages only. Outputs/provenance under `.zpk-forge/` are ignored by Git.
