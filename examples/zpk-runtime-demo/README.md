# Native runtime demo (owned test fixture)

Run only with `bash tests/zpk/runtime-check.sh --vm`. The fixture creates a dedicated
locked nologin user **inside the disposable VM**, seeds a root-owned versioned snapshot
and administrator runtime policy, then starts the native foreground process through
OpenRC. No Docker/OCI is used at application runtime and no host account is created.

`manifest.toml` is the shared package declaration; `runtime.toml` is administrator
policy with explicit temporary-storage acknowledgement. The manifest cannot grant
itself a UID, capabilities, NAS access or writable global paths. `main.rs` exposes a
private Unix health/report socket; CRASH/CPU commands exist only for the owned test
harness and are not Zion Core endpoints. Do not deploy this fixture as production software.

The demo exercises real security failures and records restart counters in a private
data slot. Data survives process restart in the VM; it is lost on VM disposal/reboot.
See [runtime architecture, commands and controls](../../docs/zpk/RUNTIME.md).
