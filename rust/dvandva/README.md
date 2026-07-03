# dvandva

A two-role (`vadi`/`prativadi`) multi-agent coordination engine.

`dvandva` is a multicall binary implementing the full Dvandva runtime over a
JSON baton — the read path, the write path, foreground waiting, role preflight,
git work-gating, and the installers. It is the runtime: earlier releases bundled
these as shell helpers inside the plugin; that shell surface is now folded into
this one binary.

## Subcommands

- **Runtime** — `state`, `resolve`, `write`, `wait`, `snapshot`.
  - `dvandva state --compact --file <baton> [--role <role>]` — emit the bounded
    `BATON_STATE_COMPACT` projection.
  - `dvandva resolve --role <vadi|prativadi> [--cwd <dir>]` — resolve the active
    run selector-first, then by discovery.
  - `dvandva write "$BATON_FILE" "$BATON_NEXT_FILE"` — validate, atomically
    install, and snapshot a baton candidate.
  - `dvandva wait --role <role> [...]` — foreground continuous polling until the
    baton assigns the role or reaches a terminal state.
  - `dvandva snapshot "$BATON_FILE"` — write a history snapshot for the baton.
- **Preflight** — `preflight`, `hook-preflight`.
- **Git work-gate** — `commit-gate`, `drift-lint`, `install-hooks`, `git-hook <name>`.
- **Install** — `install`, `install-codex`, `smoke-install`, `retire-agents`.
- **Lints** — `lint <artifacts|skills|schema-parity|protocol-phase1|skill-phase3|phase4-research|run3-dynamic-agents|run4-path-gates|run4-standalone-agents>`.

The binary is a multicall executable: when invoked through a git-hook symlink
(`pre-commit`, `prepare-commit-msg`, ...) the hook name is taken from `argv[0]`.
`dvandva --version` prints the version line.

Version `2.0.0-alpha.6`. Licensed under `MIT OR Apache-2.0`.

## Install

```bash
cargo install --path rust/dvandva
# or, from crates.io (latest published, 2.0.0-alpha.5): cargo install dvandva --version 2.0.0-alpha.5
```

The binary must be on `PATH` for the Dvandva skills to run. `cargo install`
installs only the Rust binary. After it is on `PATH`, register the Dvandva
plugin into Claude Code and/or Codex with:

```bash
dvandva install
```

`dvandva install` adds the Dvandva skills, commands, agents, and references to
the engines; the plugin no longer bundles executables.

## Known limitations

- **Exponential number literals.** The read path is byte-for-value equal to the
  jq shell fallback for integer and decimal numbers, including trailing-zero
  preservation (`1.50` stays `1.50`, not `1.5`) via serde_json's
  `arbitrary_precision` feature. The one exception is numbers written in
  **exponential form**: jq normalizes them to an uppercase-`E` mantissa
  (`1e10` -> `1E+10`), while serde_json emits a lowercase `e` (`1e10` ->
  `1e+10`). This is a narrow formatting difference (the `E`/`e` case) that
  affects only synthetic batons — no real Dvandva baton carries an exponential
  number in any surfaced field. The exact jq exponential formatter is not
  reproduced.
- **Numeric `run_id`/`status`/`assignee`/`updated_at` in `resolve`.** The shell resolver
  passes these discovery fields through without `tostring`, so a *numeric*
  value would surface in the `ASK` array as a JSON number; the Rust resolver
  stringifies it. Real batons always carry these fields as strings, so this is
  an unreachable, synthetic residual (preserving the number type would change
  the `ASK` sort ordering, which must stay identical to the shell).
- **Array/object `status`/`assignee`/`branch` in `snapshot`.** `field_or`/
  `jq_tostring` render non-scalar values (arrays/objects) as their compact
  JSON text, whereas the shell's `@tsv` step fails the jq pipeline (exit 22)
  when a field isn't a scalar. Real Dvandva batons always carry `status`/
  `assignee`/`branch` as strings, so this is an unreachable, synthetic-only
  divergence.
- **Local-dir marketplace paths in `install-codex`'s app-server RPC
  fallback.** A local directory argument is canonicalized (symlinks
  resolved) before being passed to the app-server, unlike the shell's
  `cd "$dir" && pwd`, which preserves a symlinked path component.
- **Permission-denied candidate/baton files.** `read_json_lenient` maps any
  read failure — missing file or permission-denied — to the same "missing"
  outcome, so an existing-but-unreadable file surfaces as the missing-file
  exit (21) rather than the shell's invalid-JSON exit (22).
