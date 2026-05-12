# AGENTS.md

## Purpose

This repo researches practical orchestration between Claude Code and Codex.

Prefer concise, source-backed docs over speculative architecture. If a workflow claim depends on a tool feature, cite the relevant docs or record the local command used to verify it.

## Working Rules

- Keep coordination protocols in `docs/protocol/`.
- Keep workflow designs in `docs/workflows/`.
- Keep tool research in `docs/research/`.
- Keep case studies in `docs/case-studies/`.
- Keep raw imported artifacts in `artifacts/`.
- Do not put private project secrets or proprietary source snippets in this repo.
- If importing a PR history, store raw JSON plus a readable Markdown index.

## Preferred Workflow Model

Claude is the primary doer. Codex is the reviewer and narrow fixer.

Use PR comments for human-facing milestone summaries only. Use local baton files for agent-to-agent handoff.

## Handoff Discipline

Each agent handoff must answer:

- What changed?
- What was verified?
- What is blocked?
- Who owns the next action?
- What exact command or prompt should the next agent run?

No silent handoffs. No indefinite polling.

