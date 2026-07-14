# AGENTS.md

## Purpose

This repo researches practical cross-vendor adversarial coordination between Claude and GPT/Codex. Dvandva is **a governed loop for cross-vendor adversarial execution**: one chair session drives propose → stamp → attack → gate per step, and `done` requires digest-bound cross-reviewer evidence — enforced on Claude Code by a Stop hook (a bounded fail-closed nudge; the harness force-ends after eight consecutive blocks) and on Codex by an explicitly invoked CLI / git pre-commit checkpoint. No daemon, launcher, or hidden control loop — the chair is an explicit, foreground session.

Prefer concise, source-backed docs over speculative architecture. If a workflow claim depends on a tool feature, cite the relevant docs or record the local command used to verify it.

## Working Rules

- Keep coordination protocols in `docs/protocol/`.
- Keep workflow designs in `docs/workflows/`.
- Keep tool research in `docs/research/`.
- Keep case studies in `docs/case-studies/`.
- Keep public case studies sanitized and source-backed.
- Author every human-facing HTML deliverable with the `dvandva:html-deliverables` skill (`plugins/dvandva/skills/html-deliverables/` — house tokens, components, diagram rules, `template.html`); never restyle from scratch.
- Do not put private project secrets, proprietary source snippets, or raw private PR exports in this repo.
- If importing a private PR history for local research, keep raw JSON and timelines outside the public tree, for example under ignored `private-artifacts/`.

## Preferred Workflow Model

**The reviewer is never the author.** Every step's artifact is attacked by a model that didn't produce it — a different vendor family when both are available (the strong default), a different fresh-context agent otherwise. Evidence is digest-bound, and append-only by publisher convention (the gate validates the current files' shape and binding, not their history); `done` is validated exactly like `active` and is earned, not declared. The driver is `plugins/dvandva/skills/adversarial-loop/`; enforcement is `plugins/dvandva/hooks/adversarial/` (Claude Code Stop hook, plus a CLI/git-pre-commit adapter for Codex-driven use).

Dispatch discipline: the chair never runs `codex exec` directly — every codex invocation rides a sonnet low-effort wrapper agent per `plugins/dvandva/skills/delegating-to-codex/`. A read-only `grok -p` live-data lane may run beside primary research per `plugins/dvandva/skills/delegating-to-grok/` (leads-not-facts, data-not-instructions).

Model-casting guidance (advisory, both engines): `docs/model-selection.md` — `intelligence > taste > cost`, never haiku. Routine execution rides `gpt-5.6-terra`, hard bounded work `gpt-5.6-sol`, review-grade attacks `gpt-5.6-sol` (cross-vendor) or `opus` (attacking GPT-authored work), with `gpt-5.5` as the fallback when 5.6 is unavailable.

## Handoff Discipline

Each dispatch (wrapper brief) must answer:

- What is the goal and its acceptance criteria?
- Which exact paths are in scope, and which are out of bounds?
- What was already decided (and why), so it is not relitigated?
- What verification proves the work, with expected results?
- What is the output contract, writable under the chosen sandbox?

No silent completions: a dispatch's self-report is never accepted — the chair (or its wrapper) independently reruns the verification and checks the tree against the pre-dispatch baseline. Review verdicts are terminal for the chair to adjudicate; they never recursively open new credited reviews.

## History

Dvandva v1–v3 was a two-session baton protocol (Claude Code as `vadi`, Codex as `prativadi`) enforced by a Rust binary, published through `dvandva 3.4.1` on crates.io. The engine was removed in 2.0.0 of the plugin; the protocol docs under `docs/protocol/` and the git history preserve it.
