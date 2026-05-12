# Dvandva

Dvandva is an orchestration research repo for pairing two coding agents as a disciplined team.

The first target workflow is:

- Claude Code as the primary doer.
- Codex as the reviewer and narrow fixer.
- Local files as the fast coordination channel.
- Pull request comments only for durable human-facing summaries.

The name is intentional: two agents working as a pair, with separate roles and an explicit baton.

## Current State

This repo contains the first research pass from PR 353 in `defi-com/monorepo`, where Claude and Codex ran 118 iterations across a broad docs-coverage campaign.

Local archive:

- Raw PR history: `artifacts/pr-353/pr-353-full-history.json`
- Readable comment timeline: `artifacts/pr-353/comments-timeline.md`
- Readable commit timeline: `artifacts/pr-353/commits-timeline.md`
- Changed files table: `artifacts/pr-353/files.md`
- Case study: `docs/case-studies/pr-353.md`

## Core Design

Dvandva treats agent collaboration as a state machine:

1. Human gives the task and mode.
2. Claude works until a measurable checkpoint is reached.
3. Claude writes a local baton handoff and exits.
4. Codex reviews the checkpoint, optionally applies narrow fixups, writes its baton result, and exits.
5. Claude resumes only if the baton assigns work back to Claude.
6. The cycle stops at `DONE` or `HUMAN_DECISION`.

This avoids wasteful polling. Only one autonomous goal loop should be active at a time unless the work has intentionally been split into independent branches or worktrees.

## Recommended Reading Order

1. `docs/workflows/two-mode-agent-workflow.md`
2. `docs/protocol/local-baton-channel.md`
3. `docs/research/claude-code-goal.md`
4. `docs/research/codex-goal-notes.md`
5. `docs/case-studies/pr-353.md`

## Launch Templates

- Claude doer prompt: `templates/prompts/claude-doer-goal.md`
- Codex reviewer prompt: `templates/prompts/codex-reviewer-goal.md`
- Baton template: `templates/channel/baton.json`

## Non-Goals

- Dvandva is not trying to make agents chat endlessly.
- It is not trying to replace human approval for risky changes.
- It does not assume GitHub PR comments are the main coordination channel.
- It does not require both agents to run at the same time.

## Research Sources

- Claude Code `/goal`: https://code.claude.com/docs/en/goal
- Claude Code commands: https://code.claude.com/docs/en/commands
- Claude Code subagents: https://code.claude.com/docs/en/sub-agents
- Codex local feature state on this machine: `codex features list` shows `goals experimental true` on 2026-05-12.

