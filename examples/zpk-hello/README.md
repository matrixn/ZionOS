# Native ZPK hello

This is a small owned Rust source example, a CLI-only manifest and its MIT license.
There is no installer, daemon, Docker image or container runtime. Building the sample
compiles source and writes a local artifact; inspection never runs the executable.

In Linux/Ubuntu WSL, with Rust's musl target and musl-tools installed:

```sh
rustup target add x86_64-unknown-linux-musl
bash examples/zpk-hello/build-package.sh
cargo run --locked --manifest-path crates/zpk-schema/Cargo.toml --bin zpk-inspect -- \
  build/zpk/hello-1.0.0.zpk 0.6.1
```

The script compiles with `-C target-cpu=x86-64`, packages the resulting native ELF,
and validates it before creating the output. Output creation is exclusive: repeat
with `bash examples/zpk-hello/build-package.sh build/zpk/hello-second.zpk` if the
default artifact already exists. Generated files remain under ignored `build/`.

The developer-only `build-hello` Cargo example writes USTAR using the tar crate
(dev dependency) and compresses it with zstd. It is intentionally a fixed example
builder, not a general-purpose production package publisher. It writes no installation
paths and needs no elevated privileges.

For optional services/storage or desktop windows, see the complete valid manifests
under `tests/zpk/fixtures/valid/`. Desktop manifests require supplying their declared
HTML and PNG files; they are schema fixtures, not completed graphical applications.
See [format reference](../../docs/zpk/FORMAT_V1.md) for every field and restriction.
