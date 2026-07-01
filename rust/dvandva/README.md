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
