# AGENTS.md

## Purpose

> **Active v4 skills; retired v3 archive.** The final v3 crate release remains
> `3.5.1`, and the v3 plugin is unsupported historical evidence. Do not install
> or modify that plugin. New distribution is limited to `skills/setup-dvandva`,
> `skills/vadi`, `skills/prativadi`, `skills/html-deliverables`, and versioned `skills-v*` private-kernel
> releases.

This repo preserves research into practical agent-to-agent coordination between Claude Code and Codex. The historical Dvandva implementation used a baton-passing protocol-level orchestrator: the baton coordinated roles, phases, review gates, and subagent work, with no daemon, launcher, or hidden central control loop.

The active v4 interface is skill-only. Keep the v4 kernel private, outside
`PATH`, and independent of either harness. One harness must never invoke the
other. The human starts both T3 Code sessions; each role waits locally on its
own run-scoped Baton.

Prefer concise, source-backed docs over speculative architecture. If a workflow claim depends on a tool feature, cite the relevant docs or record the local command used to verify it.

## Working Rules

- Keep coordination protocols in `docs/protocol/`.
- Keep workflow designs in `docs/workflows/`.
- Keep tool research in `docs/research/`.
- Keep case studies in `docs/case-studies/`.
- Keep public case studies sanitized and source-backed.
- Keep active skill sources under root `skills/` and v4 release automation
  isolated from `plugins/dvandva/` and `rust/dvandva/`.
- Active HTML deliverables use `skills/html-deliverables/` and its template and standalone validator. Historical HTML sources under `plugins/dvandva/skills/html-deliverables/` remain unchanged; never invoke their retired v3 lint or publication workflow.
- Do not put private project secrets, proprietary source snippets, or raw private PR exports in this repo.
- If importing a private PR history for local research, keep raw JSON and timelines outside the public tree, for example under ignored `private-artifacts/`.

## Active v4 model casting

For every active v4 run, read the role-local
`references/model-selection.md` before selecting or dispatching models. That
policy defines separate Astra/Fable planning sessions, Sol/high vadi and Opus
prativadi drivers, optional planning advisers, and parent-only mutation
boundary. The copies under `skills/vadi/` and `skills/prativadi/` are identical
so each distributed role skill remains self-contained.

## Historical workflow model

Either engine could host either role. The preferred dogfood setup was Claude Code as vadi and Codex as prativadi; Codex-as-vadi and Claude-as-prativadi were equally valid. **Dvandva never ran solo** — every recorded run used two decorrelated roles, and the reviewer was not the engine that did the work. `supervised` runs were human-gated handoffs between those same two roles, while `walkaway` sessions polled autonomously. This is preserved as historical behavior, not onboarding.

Use PR comments for archive-maintenance summaries only. Do not create new v3
baton files or restart a historical workflow. Active v4 runs must use the
released role skills and XDG run root, never the archived plugin.

## Historical model-casting evidence

Historical model-casting guidance (advisory, both engines): `docs/model-selection.md`. The
historical default casting was a repeating ring, not a one-shot pipeline: human task ->
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

Research production was Sol-owned: `research_drafting` and `research_revision` dispatched `gpt-5.6-sol` to produce and revise `research_ref`; the vadi coordinated and serialized the result. Research review was Claude-only. These rules are historical evidence only.

## Archive-maintenance handoff discipline

Each agent handoff must answer:

- What changed?
- What was verified?
- What is blocked?
- Who owns the next action?
- What exact command or prompt should the next agent run?

No silent handoffs. Archive work must identify what is historical, what was
verified, and whether it changes distribution or installation state. Do not
resume model-turn polling or an archived baton workflow.
