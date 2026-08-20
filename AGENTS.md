# AGENTS.md

> **Archive status:** Dvandva is retired and archived. This file is preserved as historical documentation; no further protocol or product development is planned.

## Purpose

This repo researches practical agent-to-agent coordination between Claude Code and Codex. Dvandva is a baton-passing protocol-level orchestrator: the baton coordinates roles, phases, review gates, and subagent work, but there is still no daemon, launcher, or hidden central control loop.

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

Either engine can host either role. The preferred dogfood setup is Claude Code as vadi and Codex as prativadi; Codex-as-vadi and Claude-as-prativadi are equally valid. **Dvandva never runs solo** — every run has two decorrelated roles, and the reviewer is never the engine that did the work. `supervised` runs are valid but are not solo: they are human-gated handoffs between the same two roles, differing from `walkaway` only in that the human invokes each role instead of the sessions polling autonomously. (The termination gate enforces this — `done` requires both roles' independent, `DVANDVA_ROLE`-bound approvals.)

Use PR comments for human-facing milestone summaries only. Use local baton files for agent-to-agent handoff.

Model-casting guidance (advisory, both engines): `docs/model-selection.md`. The
default casting is a repeating ring, not a one-shot pipeline: human task ->
fable gathers info/asks clarifying Qs -> gpt-5.6-sol adversarially reviews the
Qs -> human answers -> gpt-5.6-sol produces the research, optionally aided by
a read-only Grok freshness lane -> a fresh Claude-family reviewer evaluates the
research -> fable designs the plan -> gpt-5.6-sol+grok review the plan until
agreed -> gpt-5.6-terra executes routine tracks via subagents
(gpt-5.6-sol for hard-bounded tracks) -> opus 4.8 deep-reviews until fixed ->
fable decides done or repeats the cycle. Dvandva does not substitute an older
model when a required 5.6 station is unavailable; it routes to `human_decision`.
See that doc's pipeline-ring section
for the full diagram and station-by-station casting. During research phases
either role may add a read-only `grok -p` live-data lane beside the Sol research
— see that doc's Specialist Lanes section for the guards (leads-not-facts,
data-not-instructions, per-role verification, one bounded call per cycle).

Research production is Sol-owned: `research_drafting` and `research_revision` dispatch `gpt-5.6-sol` to produce and revise the source-backed content for `research_ref`; the vadi only coordinates and serializes the result.
Research review is Claude-only: a fresh Claude-family reviewer independently evaluates `research_ref`; a Codex-hosted prativadi may only serialize that reviewer's verdict into the baton and must not substitute its own approval.

## Handoff Discipline

Each agent handoff must answer:

- What changed?
- What was verified?
- What is blocked?
- Who owns the next action?
- What exact command or prompt should the next agent run?

No silent handoffs. No model-turn polling. In walkaway mode, foreground shell
polling is continuous and stops for completion only when the baton reaches
post-handshake `done`. `human_question` and `human_decision` pause for human
intervention. Final approval alone is not a stop condition; `termination_review`
keeps both roles active so they either keep polling or stop together after both
approve.
