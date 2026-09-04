# Installation contract

The release target is `0.3.6` / `skills-v0.3.6`. The installer resolves the
tag and release asset at invocation and fails closed if either is missing.
The operations:

```bash
bash scripts/setup-dvandva.sh install --version 0.3.6
bash scripts/setup-dvandva.sh update --version 0.3.6
bash scripts/setup-dvandva.sh doctor --version 0.3.6
bash scripts/setup-dvandva.sh uninstall --version 0.3.6
```

These operations consume release `skills-v0.3.6`. Compatibility requires
write schema `dvandva.run.v2` and facade API 2. Kernel v1 read support is only
for explicit migration by a role; setup never migrates runs during install,
update, doctor, or uninstall.

Supported host: Linux x86_64 only, for now. The release carries a single
asset, `dvandva-kernel-linux-x86_64`; on any other operating system the script
exits with `only Linux x86_64 is supported for now`, and on another architecture
with `unsupported architecture`, before downloading anything. macOS, native
Windows, and arm64 are not supported; Windows users run the sessions in WSL2.

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
bash scripts/setup-dvandva.sh uninstall --version 0.3.6 \
  --purge-runs --yes-purge-runs
```
