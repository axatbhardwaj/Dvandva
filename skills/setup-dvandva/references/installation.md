# Installation contract

The source and planned release target is `0.3.0` / `skills-v0.3.0`. Remote
installation is available only after the tag and release asset exist. The
installer resolves both at invocation and fails closed until both exist; this
reference does not claim current availability. These are target commands:

```bash
bash scripts/setup-dvandva.sh install --version 0.3.0
bash scripts/setup-dvandva.sh update --version 0.3.0
bash scripts/setup-dvandva.sh doctor --version 0.3.0
bash scripts/setup-dvandva.sh uninstall --version 0.3.0
```

Once published, these operations consume release `skills-v0.3.0`.
Compatibility requires
write schema `dvandva.run.v2` and facade API 2. Kernel v1 read support is only
for explicit migration by a role; setup never migrates runs during install,
update, doctor, or uninstall.

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
bash scripts/setup-dvandva.sh uninstall --version 0.3.0 \
  --purge-runs --yes-purge-runs
```
