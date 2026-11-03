# Native end-to-end integration

See [the complete guide](../../docs/zpk/E2E.md) for prerequisites and safety limits.

```sh
bash tests/e2e/check.sh
bash tests/e2e/check.sh --packages
bash scripts/test-vm.sh --zpk-e2e
bash tests/e2e/check.sh --legacy-vm
bash tests/e2e/check.sh --interactive
```

- `images.py` exclusively creates regular files under a fresh ignored build directory.
  Filesystem creation uses pinned descriptors; existing entries and links are refused.
  Unit tests mock those external tools and never touch devices.
- `fixtures.py` builds three sample versions through actual Forge isolation, validates
  archive/ELF/CPU metadata and package hashes, and records provenance.
- `guest.sh` guards the one disposable virtual disk and mounts the same persistent
  app/registry directories on each boot. Never run it on the host.
- `guest.rs` is a separately compiled trusted VM fixture: real CLI installation,
  health/UID/ownership/capability checks, settings, lifecycle, upgrade/rollback,
  process-crash injection and uninstall preservation. It is not staged into the ISO.
- `vm.py` runs three clean boots without snapshot mode, runs real Genesis smoke
  routes and reuses the SDK Chromium suite. Interactive mode accepts documented local
  commands only and cleanly powers off its owned guest on quit/Ctrl+C.
- `desktop.cjs` observes live old-window/asset revocation during actual CLI changes.

Default tests do not execute installed application services on the host. Pure Rust
settings unit tests concern this audited first-party example's temporary regular files.
Produced packages are always compiled by Forge. VM tests require prepared tools and
exclusive localhost:8080. Logs, images and package outputs stay in ignored `build/`;
all runs retain separate evidence directories and never reuse a user image.
