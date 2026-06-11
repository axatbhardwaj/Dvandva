# AGENTS.md

## Purpose

This repo researches practical agent-to-agent coordination between Claude Code and Codex. Dvandva is a baton-passing protocol, deliberately not an orchestrator: no daemon, launcher, or central control loop.

Prefer concise, source-backed docs over speculative architecture. If a workflow claim depends on a tool feature, cite the relevant docs or record the local command used to verify it.

## Working Rules

- Keep coordination protocols in `docs/protocol/`.
- Keep workflow designs in `docs/workflows/`.
- Keep tool research in `docs/research/`.
- Keep case studies in `docs/case-studies/`.
- Keep public case studies sanitized and source-backed.
- Do not put private project secrets, proprietary source snippets, or raw private PR exports in this repo.
- If importing a private PR history for local research, keep raw JSON and timelines outside the public tree, for example under ignored `private-artifacts/`.

## Preferred Workflow Model

Either engine can host either role. The preferred dogfood setup is Claude Code as vadi and Codex as prativadi, but Codex-as-vadi, Claude-as-prativadi, and single-engine supervised runs are valid.

Use PR comments for human-facing milestone summaries only. Use local baton files for agent-to-agent handoff.

## Handoff Discipline

Each agent handoff must answer:

- What changed?
- What was verified?
- What is blocked?
- Who owns the next action?
- What exact command or prompt should the next agent run?

No silent handoffs. No indefinite polling.
