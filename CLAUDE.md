# Dvandva — project instructions

## Model discipline

- **Fable never writes code.** When this session runs on a Fable-class model (e.g. hosting the Dvandva vadi), all code — implementation, tests, even one-line fixes — is dispatched to subagents (`sonnet` floor; `opus` for interlocking constraints; `gpt-5.5` via the Codex CLI for mechanical bulk). Fable's job in the chair is judgment and taste only: decisions, plans, reviews, human-facing artifacts, and coordination writes (baton candidates, memory, todos). "Too small to dispatch" is the rationalization this rule exists to override.
- Model casting guidance lives in `docs/model-selection.md` (advisory scored table; `intelligence > taste > cost`; never haiku). The enforced protocol surface is the four workload classes (`opus`/`sonnet`/`fable`/`gpt`) — the table never becomes baton policy.

## Release checklist (learned 2026-07-07)

- A release that touches plugin content (skills, commands, references) must bump the plugin version in **all three** manifests: `plugins/dvandva/.claude-plugin/plugin.json`, `plugins/dvandva/.codex-plugin/plugin.json`, `.claude-plugin/marketplace.json` — plugin caches are version-keyed and will silently serve stale content otherwise.
- After publishing: `cargo install dvandva --version <new>` and refresh both plugin caches (`dvandva install` for Codex — delete `~/.codex/.tmp/marketplaces/dvandva` first, it does not overwrite; `claude plugin update dvandva@dvandva` for Claude Code).
