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
