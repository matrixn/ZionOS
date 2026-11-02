# zpk-manager

Local native package transactions and Alpine/OpenRC privileged adapter. No HTTP
installer, runtime containers or package-provided privileged scripts.

Architecture, exact commands, persistent-volume prerequisites and supported limits:
[manager guide](../../docs/zpk/MANAGER.md).

```sh
bash tests/zpk/manager-check.sh
bash tests/zpk/manager-check.sh --vm  # equipped environment; disposable QEMU only
```

Unit tests use explicit temporary roots and mock services, not the host init system.
The real CLI is restricted to root on the target Alpine/ZionOS OS and refuses ephemeral
installation targets. Unsigned archives require explicit trusted-local development policy.
