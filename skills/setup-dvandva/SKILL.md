---
name: setup-dvandva
description: Install, update, diagnose, or uninstall the private Dvandva v4 skill kernel when the user explicitly requests Dvandva setup.
disable-model-invocation: true
---

# Setup Dvandva

Run the bundled `scripts/setup-dvandva.sh` for exactly the operation the user
requested: `install`, `update`, `doctor`, or `uninstall`.

The documented release is version `0.2.0`, tag `skills-v0.2.0`, write schema
`dvandva.run.v2`, and facade API 2. Use it unless the user names another
`skills-vX.Y.Z` release. Report the script's evidence. Do not reconstruct its
download, checksum, ownership, compatibility, or atomic-switch logic.

The kernel is private implementation for the `vadi` and `prativadi` skills. Do
not add it to `PATH`, install the archived v3 crate or plugin, start a run, or
invoke either role. Setup never migrates runs. The kernel's
v1 read support is only for explicit migration through an active role workflow.

Uninstall preserves run history. Purge it only when the user explicitly asks,
using both `--purge-runs` and `--yes-purge-runs`.

See `references/installation.md` for paths and exact operations.
