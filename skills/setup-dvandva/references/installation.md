# Installation contract

The setup script supports:

```bash
bash scripts/setup-dvandva.sh install --version 0.1.1
bash scripts/setup-dvandva.sh update --version 0.1.1
bash scripts/setup-dvandva.sh doctor --version 0.1.1
bash scripts/setup-dvandva.sh uninstall --version 0.1.1
```

It installs the checksummed Linux release asset under
`${XDG_DATA_HOME:-$HOME/.local/share}/dvandva/bin/<version>/`, then atomically
switches `bin/current`. The helper remains outside `PATH`.

Installation ownership is recorded in `dvandva/installation.json`. Run Batons
and private role credentials live under
`${XDG_STATE_HOME:-$HOME/.local/state}/dvandva/`, with private directory modes.

`doctor` verifies the ownership manifest, selected version, asset digest, and
schema compatibility probe. `uninstall` refuses unowned data and preserves the
runs directory. A destructive purge requires the user's explicit request and:

```bash
bash scripts/setup-dvandva.sh uninstall --version 0.1.1 \
  --purge-runs --yes-purge-runs
```
