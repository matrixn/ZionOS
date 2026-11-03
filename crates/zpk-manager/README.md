# zpk-manager

TASK 05 integrates the real Manager with Forge, SDK and persistent native app state:
[developer journey](../../docs/zpk/E2E.md). `bash scripts/test-vm.sh --zpk-e2e`
tests three boots, upgrades and interrupted activation. The additional `zpk-e2e-guest`
binary is a guarded trusted development fixture, not a production command/helper.

Local native package transactions and Alpine/OpenRC privileged adapter. No HTTP
server, runtime containers or package-provided privileged scripts in this crate.
Genesis 0.7 reuses this library behind the restricted authenticated Unix broker;
the HTTP process remains non-root. See [Package Center](../../docs/GENESIS_07.md)
and its [authorization contract](../../docs/GENESIS_07_ARCHITECTURE.md).

Architecture, exact commands, persistent-volume prerequisites and supported limits:
[manager guide](../../docs/zpk/MANAGER.md).

```sh
bash tests/zpk/manager-check.sh
bash tests/zpk/manager-check.sh --vm  # equipped environment; disposable QEMU only
```

Unit tests use explicit temporary roots and mock services, not the host init system.
The real CLI is restricted to root on the target Alpine/ZionOS OS and refuses ephemeral
installation targets. Unsigned archives require explicit trusted-local development policy.
