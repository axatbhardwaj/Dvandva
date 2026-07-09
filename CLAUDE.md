# Dvandva — project instructions

## Model discipline

- **Fable never writes code.** When this session runs on a Fable-class model (e.g. hosting the Dvandva vadi), all code — implementation, tests, even one-line fixes — is dispatched to subagents (`sonnet` floor; `opus` for interlocking constraints; `gpt-5.5` via the Codex CLI for mechanical bulk). Fable's job in the chair is judgment and taste only: decisions, plans, reviews, human-facing artifacts, and coordination writes (baton candidates, memory, todos). "Too small to dispatch" is the rationalization this rule exists to override.
- Model casting guidance lives in `docs/model-selection.md` (advisory scored table; `intelligence > taste > cost`; never haiku). The enforced protocol surface is the four workload classes (`opus`/`sonnet`/`fable`/`gpt`) — the table never becomes baton policy.

## Release checklist (learned 2026-07-07)

- A release that touches plugin content (skills, commands, references) must bump the plugin version in **all three** manifests: `plugins/dvandva/.claude-plugin/plugin.json`, `plugins/dvandva/.codex-plugin/plugin.json`, `.claude-plugin/marketplace.json` — plugin caches are version-keyed and will silently serve stale content otherwise.
- Before publishing, run `dvandva lint stale-version-ref .` (from a fresh tree-built binary, never the globally-installed one) — it fail-closes on any user-facing version reference (READMEs, SKILL install hints, explainer HTML, manifests, help-text defaults) that drifted from Cargo.toml / the shared plugin version.
- After publishing: bring the whole stack current with `dvandva upgrade` (3.1.0+; runs cargo install + refreshes both plugin caches + prints a version table). On binaries older than 3.1.0 the manual sequence is `cargo install dvandva --version <new>`, `dvandva install`, `claude plugin update dvandva@dvandva` (the delete-the-codex-marketplace-first workaround died with the 3.0.0-alpha.2 installer fix).
