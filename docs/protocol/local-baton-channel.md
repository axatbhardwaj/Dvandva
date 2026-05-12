# Local Baton Channel

## Problem

PR comments are durable, but slow and noisy. They are good for human-facing summaries, not for high-frequency agent handoff.

The local baton channel gives Claude and Codex a shared, file-based coordination contract.

## Files

The default local runtime directory is `.dvandva/`, which is gitignored.

Recommended files:

- `.dvandva/baton.json` - current state and next assignee.
- `.dvandva/events.jsonl` - append-only event log.
- `.dvandva/claude-handoff.md` - latest Claude handoff.
- `.dvandva/codex-review.md` - latest Codex review.
- `.dvandva/decisions.md` - human decisions that should survive context loss.

The shareable templates live in `templates/channel/`.

## Baton Schema

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": "2026-05-12T00:00:00Z",
  "mode": "feature-pr",
  "status": "codex_review",
  "assignee": "codex",
  "branch": "feature/example",
  "checkpoint": 1,
  "summary": "Claude implemented the first pass and tests pass.",
  "changed_paths": ["src/example.ts", "test/example.test.ts"],
  "verification": [
    {
      "command": "bun test test/example.test.ts",
      "result": "passed"
    }
  ],
  "blockers": [],
  "next_action": "Codex: review the diff and either apply narrow fixups or return blockers to Claude."
}
```

## State Machine

States:

- `claude_working`
- `codex_review`
- `claude_fixing`
- `codex_fixing`
- `human_decision`
- `done`

Allowed transitions:

- `claude_working` -> `codex_review`
- `codex_review` -> `claude_fixing`
- `codex_review` -> `codex_fixing`
- `codex_review` -> `human_decision`
- `codex_review` -> `done`
- `codex_fixing` -> `claude_fixing`
- `codex_fixing` -> `done`
- `claude_fixing` -> `codex_review`
- `human_decision` -> any state, after the human edits the baton or prompts an agent

## Handoff Rule

The active agent must stop after writing a baton that assigns the next action to another actor.

This is the core anti-polling rule:

- Claude does not wait for Codex.
- Codex does not wait for Claude.
- The human, a shell notifier, or a future orchestrator starts the next actor.

## Goal Conditions

Use `/goal` around the baton state instead of around a timer.

Example Claude goal:

```text
/goal Work until .dvandva/baton.json exists with assignee "codex" or status "human_decision" or "done". Before stopping, surface the verification commands you ran and write .dvandva/claude-handoff.md.
```

Example Codex goal:

```text
/goal Review the branch until .dvandva/baton.json has assignee "claude", assignee "human", or status "done". Apply only narrow fixups. Surface every verification command and write .dvandva/codex-review.md.
```

## Why Not Two Loops At Once

Two autonomous sessions polling the same channel recreate the PR 353 problem locally. They spend tokens checking whether the other agent has moved.

The better default is serialized autonomy:

1. One agent runs.
2. It writes a baton.
3. It exits.
4. The next actor starts.

Parallelism should be explicit and branch-scoped.

