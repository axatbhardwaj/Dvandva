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
  "updated_at": "2026-05-13T10:30:00Z",
  "mode": "feature-pr",
  "phase": 1,
  "total_phases": 3,
  "status": "phase_review",
  "assignee": "codex",
  "review_target": "implementation",
  "plan_ref": "./superpowers/plans/2026-05-13-example-feature.md",
  "disagreement_round": 0,
  "disagreement_cap": 3,
  "turn_cap": 20,
  "branch": "feature/example",
  "checkpoint": 4,
  "summary": "Claude implemented phase 1: scaffolding + tests. Awaiting Codex review.",
  "changed_paths": ["src/example.ts", "test/example.test.ts"],
  "verification": [
    { "command": "bun test test/example.test.ts", "result": "passed" }
  ],
  "findings": [],
  "narrow_fixups": [],
  "claude_counter": [],
  "deferred": [],
  "blockers": [],
  "next_action": "Codex: review phase 1 implementation. Apply narrow fixups within the allowlist, or hand back substantive findings."
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

**Implementation phase (per phase N):** `(impl)` below is shorthand for `review_target: implementation`.

- `phase: N, implementing` → `phase_review (impl)`
- `phase: N, implementing` → `human_decision`
- `phase_review (impl)` → `phase_fixing` (substantive findings)
- `phase_review (impl)` → `review_of_review (codex_fixups)` (narrow fixups applied)
- `phase_review (impl)` → `phase: N+1, implementing` (approve no changes) **or** terminal `done` if N is final
- `phase_review (impl)` → `human_decision`
- `phase_fixing` → `phase_review (impl)`
- `phase_fixing` → `human_decision`

**Mutual review and disagreement loop:**

- `review_of_review (codex_fixups)` → `phase: N+1, implementing` (Claude approves) or terminal `done`
- `review_of_review (codex_fixups)` → `counter_review (claude_counter)` (Claude disapproves; `disagreement_round += 1`)
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

The canonical v1 goal conditions are embedded in the two skill bodies (`skills/dvandva-doer/SKILL.md` and `skills/dvandva-reviewer/SKILL.md`) under their `/goal condition` sections. Always use the version from the skill file rather than copying from this doc, since the skill version is what the goal evaluator actually parses against.

Doer goal (paste into Claude):

```
/goal You are dvandva-doer. Work until .dvandva/baton.json has assignee not equal to "claude" or status is "done" or "human_decision". Before stopping, surface BATON_STATE, list changed files, list verification commands and outcomes, and do not modify files outside the requested scope. Stop after 20 turns and assign human if still blocked.
```

Reviewer goal (paste into Codex):

```
/goal You are dvandva-reviewer. Review the branch using .dvandva/baton.json as the handoff. Apply only narrow fixups within the allowlist. Stop when the baton has assignee not equal to "codex" or status is "done" or "human_decision". Before stopping, surface BATON_STATE, findings, verification commands and outcomes, and the final baton contents.
```

Both goals require the agent to surface a structured `BATON_STATE: { ... }` line at every checkpoint. The `/goal` evaluator detects exit conditions by reading that line in the transcript.

## Why Not Two Loops At Once

Two autonomous sessions polling the same channel recreate the PR 353 problem locally. They spend tokens checking whether the other agent has moved.

The better default is serialized autonomy:

1. One agent runs.
2. It writes a baton.
3. It exits.
4. The next actor starts.

Parallelism should be explicit and branch-scoped.

