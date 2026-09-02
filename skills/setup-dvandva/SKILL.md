---
name: setup-dvandva
description: Install, update, diagnose, or uninstall the private Dvandva v4 skill kernel when the user explicitly requests Dvandva setup.
disable-model-invocation: true
---

# Setup Dvandva

Run the bundled `scripts/setup-dvandva.sh` for exactly the operation the user
requested: `install`, `update`, `doctor`, or `uninstall`.

Version `0.3.4` is the release target, tag `skills-v0.3.4`, write schema
`dvandva.run.v2`, facade API 2. The installer resolves the tag and asset at
invocation and fails closed if either is missing. Report the script's evidence
and do not reconstruct its download, checksum, ownership, compatibility, or
atomic-switch logic.

The kernel is Linux x86_64 only, for now. On any other operating system or
architecture the script refuses before downloading; report that outcome as the
answer. Do not build the kernel from source, fetch a different asset, or
substitute another binary to work around it.

The kernel is private implementation for the `vadi` and `prativadi` skills. Do
not add it to `PATH`, install the archived v3 crate or plugin, start a run, or
invoke either role. Setup never migrates runs. The kernel's
v1 read support is only for explicit migration through an active role workflow.

Uninstall preserves run history. Purge it only when the user explicitly asks,
using both `--purge-runs` and `--yes-purge-runs`.

See `references/installation.md` for paths and exact operations.
