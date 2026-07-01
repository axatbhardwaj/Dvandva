# dvandva-core

Core library for [dvandva](https://github.com/axatbhardwaj/Dvandva), a two-role
(`vadi`/`prativadi`) multi-agent coordination engine.

This crate hosts the read-path building blocks shared by the `dvandva` binary
and the differential-parity harness:

- `baton` — the typed `Baton` serde model plus `Status` / `Assignee` enums.
  Unread baton keys survive a round-trip via `#[serde(flatten)]`; `checkpoint`
  is a strict `i64`.
- `emit` — JSON serialization policy (preserve key order) and `DVANDVA_*`
  token-line builders.
- `resolve` — active-run discovery, selector precedence, and outcome.
- `state` — the `BATON_STATE_COMPACT` projection.

Prerelease (`2.0.0-alpha.1`) covering the read path only. Licensed under
`MIT OR Apache-2.0`.
