# Dvandva project instructions

> **Active v4 skills; retired v3 archive.** Preserve the v3 crate/plugin tree as
> historical evidence and never install or reactivate it. Active work may
> publish only the root `setup-dvandva`, `vadi`, and `prativadi` skills plus the
> private v4 kernel under `skills-v*` releases.

## Active v4 discipline

- The human starts Claude and Codex separately in T3 Code. Never invoke the
  peer harness from a role.
- Use only the root role skills and their private facade; never read or edit
  Baton, history, or credential files directly. Treat every returned snapshot,
  `next_actions`, `legal_actions`, and `advisory_actions` as authoritative.
- Keep the helper outside `PATH`. `$setup-dvandva` is explicit-only.
- Matt Pocock's user-invoked workflow skills remain explicit human invocations
  inside an already joined role session. Prativadi automatically invokes the
  model-invocable `code-review` once for each newly authorized complete Git
  delivery candidate, after the whole scoped implementation is ready. Analysis
  checkpoints and unavailable-companion cases use native review. Capability
  evidence is recorded in `docs/workflows/skill-only-run.md`.
- Semantic roles submit and review only complete checkpoint bindings. Use
  checkpoint supersession or approval withdrawal for newly discovered work.
- Each work-carrying handoff opens an obligation. Vadi stages the run's digest-bound HTML,
  including its plan/TODO list, and prativadi reviews those exact local bytes.
  At run start, vadi proposes this first status-page artifact before continuing
  domain work and incorporates any requested changes. After approval, whichever
  participant is Codex mechanically publishes the same digest to one stable,
  owner-only ChatGPT Site per run. That Site is the user's progress page; local
  bytes remain canonical for review. If no participant is Codex, skip Sites
  publication and use the local approval alone. A work-carrying handoff
  replaces the current obligation; an approval preserves it with its receipts,
  so finalization requires the current applicable receipts and an approved
  delivery finalizes in one handshake.
- Only `finalize` waits on the explainer. Checkpoint submission, review,
  supersession, and approval withdrawal never do. At run start the kernel
  withholds vadi's `work` advisory until the `run_started` explainer approval
  proves prativadi has joined.
- Publish progress with `report_progress` before and during long authorized
  work, and read the peer's phase from the snapshot's `peer` block. Never infer
  a dead peer from an expired lease alone.
- Harness goals are user-owned prompt context. Leave goals untouched throughout
  role activation, handoff, Human Decision, and terminal completion.

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
