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

> **Authority:** `product.md` Appendix A is authoritative for v1 transitions. This section is reference; if the two diverge, the spec wins. Update this section when the spec changes.

### States (v1)

- `spec_drafting` — Claude is writing the plan
- `spec_review` — Codex is doing Q&A on the plan
- `spec_revision` — Claude is responding to Codex Q&A
- `implementing` — Claude is doing the current phase
- `phase_review` — Codex is reviewing the current phase
- `phase_fixing` — Claude is fixing per Codex findings
- `review_of_review` — Claude is reviewing Codex's narrow fixups (mutual review)
- `counter_review` — Codex is reviewing Claude's counter-change after a disagreement
- `human_decision` — escalation pending human input
- `done` — work complete

### Allowed transitions (v1)

**Spec phase:**

- `(no baton)` → `spec_drafting`
- `spec_drafting` → `spec_review`
- `spec_review` → `spec_revision` (Codex Q&A)
- `spec_review` → `phase: 1, implementing` (Codex accepts plan; only Codex can advance the spec)
- `spec_revision` → `spec_review` (Claude answered Q&A, hands back)

**Implementation phase (per phase N):**

- `phase: N, implementing` → `phase_review (review_target: implementation)`
- `phase: N, implementing` → `human_decision`
- `phase_review (impl)` → `phase_fixing` (substantive findings)
- `phase_review (impl)` → `review_of_review (review_target: codex_fixups)` (narrow fixups applied)
- `phase_review (impl)` → `phase: N+1, implementing` (approve no changes) **or** terminal `done` if N is final
- `phase_review (impl)` → `human_decision`
- `phase_fixing` → `phase_review (impl)`
- `phase_fixing` → `human_decision`

**Mutual review and disagreement loop:**

- `review_of_review (codex_fixups)` → `phase: N+1, implementing` (Claude approves) or terminal `done`
- `review_of_review (codex_fixups)` → `counter_review (review_target: claude_counter)` (Claude disapproves; `disagreement_round += 1`)
- `review_of_review (codex_fixups)` → `human_decision` (when `disagreement_round >= cap`)
- `counter_review (claude_counter)` → `phase: N+1, implementing` (Codex approves counter) or terminal `done`
- `counter_review (claude_counter)` → `review_of_review (codex_fixups)` (Codex disapproves counter, writes new fix; `disagreement_round += 1`)
- `counter_review (claude_counter)` → `human_decision` (when `disagreement_round >= cap`)

**Universal:**

- any state → `human_decision` (escalation)
- `human_decision` → any state (after human edits baton or prompts an agent)

Any other transition is illegal in v1. The writing agent must reject illegal transitions and route to `human_decision` instead.

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

