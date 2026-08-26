# Dvandva — archived project instructions

> **Retired archive — final crate release: `3.5.1`.** Preserve this tree as historical evidence. It is unsupported: do not install or update the plugin, invoke the binary for a new workflow, publish a release, or use the historical coordination rules as live operating policy.

## Historical model discipline

- Fable did not write code in a Dvandva role. Historical runs dispatched routine work to `gpt-5.6-terra`, hard bounded work to `gpt-5.6-sol`, and mechanically proven tasks to `gpt-5.6-luna`; unavailable required models routed to `human_decision`. This is archived protocol evidence, not an instruction for a new session.
- Historical Claude-hosted roles did not invoke `codex exec` directly; they used a thin wrapper-agent contract preserved on branch `loop-2.x` at `plugins/dvandva/skills/delegating-to-codex/`.
- Historical model-casting guidance lives in `docs/model-selection.md`; the recorded protocol surface used `opus`/`sonnet`/`fable`/`gpt` workload classes.

Research production was Sol-owned and research review was Claude-only. The retained details below exist only to interpret historical batons and source.

## Historical release record

- The preserved internal manifests remain at plugin version `1.7.0`; they are source history, not an installable marketplace.
- For archive-maintenance verification, run the tree-built `dvandva lint stale-version-ref .`; do not publish, upgrade, install, or update a plugin from this repository.
- `3.5.1` is the final crate release. Historical `dvandva upgrade` and installation behavior must not be used to reactivate this archive.

## Agent skills

### Issue tracker

Issues and specifications are tracked in GitHub Issues for `axatbhardwaj/Dvandva`. See `docs/agents/issue-tracker.md`.

### Triage labels

The repository uses the default Matt Pocock engineering-skill label vocabulary. See `docs/agents/triage-labels.md`.

### Domain docs

This is a single-context repository using root `CONTEXT.md` and system-wide ADRs under `docs/adr/`. See `docs/agents/domain.md`.
