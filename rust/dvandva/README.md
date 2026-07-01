# dvandva

A two-role (`vadi`/`prativadi`) multi-agent coordination engine.

`dvandva` is a multicall binary implementing the **read path** over a JSON
baton:

- `dvandva state --compact --file <baton> --role <r>` — emit the bounded
  `BATON_STATE_COMPACT` projection.
- `dvandva resolve --role <r> [--cwd <dir>]` — resolve the active run
  selector-first, then by discovery.

When invoked through the delegating shims `dvandva-state.sh` /
`dvandva-resolve.sh`, the subcommand and role are derived from `argv[0]`.
`dvandva --version` prints the version line.

Prerelease (`2.0.0-alpha.1`) covering the read path only. Licensed under
`MIT OR Apache-2.0`.

## Install

```bash
cargo install dvandva --version 2.0.0-alpha.1
```

This alpha intentionally covers only the read path (`state` and `resolve`).
The repository's shell shims remain the compatibility boundary and delegate to
the Rust binary when `DVANDVA_BIN`, a co-located binary, or `PATH` exposes
`dvandva`.

This installs only the Rust binary. To install the Dvandva skills, commands,
agents, references, and shell helpers into Claude Code and Codex, run the
repository installer instead:

```bash
bash scripts/install.sh
```

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
