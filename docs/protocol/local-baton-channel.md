# Local Baton Channel

## Problem

PR comments are durable, but slow and noisy. They are good for human-facing summaries, not for high-frequency agent handoff.

The local baton channel gives the vadi and prativadi a shared, file-based coordination contract.

## Files

The default local runtime directory is `.dvandva/`, which is gitignored.

Recommended files:

- `.dvandva/baton.json` - current state and next assignee.
- `.dvandva/baton.next.json` - candidate the active agent writes; installed by the bundled `dvandva-write.sh`.
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
  "run_mode": "walkaway",
  "phase": 1,
  "total_phases": 3,
  "status": "phase_review",
  "assignee": "prativadi",
  "current_engine": "codex",
  "review_target": "implementation",
  "plan_ref": "./superpowers/plans/2026-05-13-example-feature.md",
  "master_plan_locked": true,
  "question": null,
  "resume_assignee": null,
  "resume_status": null,
  "disagreement_round": 0,
  "disagreement_cap": 3,
  "turn_cap": 20,
  "branch": "feature/example",
  "checkpoint": 4,
  "allow_commit": true,
  "allow_push": true,
  "allow_pr": false,
  "vadi_final_approval": false,
  "prativadi_final_approval": false,
  "final_commit": null,
  "pushed_ref": null,
  "summary": "Claude implemented phase 1: scaffolding + tests. Awaiting Codex review.",
  "changed_paths": ["src/example.ts", "test/example.test.ts"],
  "verification": [
    { "command": "bun test test/example.test.ts", "result": "passed" }
  ],
  "findings": [],
  "narrow_fixups": [],
  "vadi_counter": [],
  "deferred": [],
  "blockers": [],
  "next_action": "Prativadi: review phase 1 implementation. Apply narrow fixups within the allowlist, or hand back substantive findings."
}
```

## State Machine

> **Authority:** `product.md` Appendix A is authoritative for v1 transitions. This section is reference; if the two diverge, the spec wins. Update this section when the spec changes.

### States (v1)

- `spec_drafting` — vadi is writing the plan
- `spec_review` — prativadi is doing Q&A on the plan
- `spec_revision` — vadi is responding to prativadi Q&A
- `human_question` — planning-only user question before `master_plan_locked`
- `implementing` — vadi is doing the current phase
- `phase_review` — prativadi is reviewing the current phase
- `phase_fixing` — vadi is fixing per prativadi findings
- `review_of_review` — vadi is reviewing prativadi's narrow fixups (mutual review)
- `counter_review` — prativadi is reviewing vadi's counter-change after a disagreement
- `human_decision` — escalation pending human input
- `done` — work complete

### Allowed transitions (v1)

**Spec phase:**

- `(no baton)` → `spec_drafting`
- `spec_drafting` → `spec_review`
- `spec_review` → `spec_revision` (Codex Q&A)
- `spec_review` → `phase: 1, implementing` (Codex accepts plan; only Codex can advance the spec)
- `spec_revision` → `spec_review` (Claude answered Q&A, hands back)
- any spec state while `master_plan_locked: false` → `human_question` (human answer needed before master plan lock)
- `human_question` → `resume_status` with `assignee: resume_assignee` (human answers; skill clears question fields)

**Implementation phase (per phase N):** `(impl)` below is shorthand for `review_target: implementation`.

- `phase: N, implementing` → `phase_review (impl)`
- `phase: N, implementing` → `human_decision`
- `phase_review (impl)` → `phase_fixing` (substantive findings)
- `phase_review (impl)` → `review_of_review (prativadi_fixups)` (narrow fixups applied)
- `phase_review (impl)` → `phase: N+1, implementing` (approve no changes) **or** terminal `done` after dual final approval if N is final
- `phase_review (impl)` → `human_decision`
- `phase_fixing` → `phase_review (impl)`
- `phase_fixing` → `human_decision`

**Mutual review and disagreement loop:**

- `review_of_review (prativadi_fixups)` → `phase: N+1, implementing` (vadi approves) or terminal `done` after dual final approval
- `review_of_review (prativadi_fixups)` → `counter_review (vadi_counter)` (vadi disapproves; `disagreement_round += 1`)
- `review_of_review (prativadi_fixups)` → `human_decision` (when `disagreement_round >= cap`)
- `counter_review (vadi_counter)` → `phase: N+1, implementing` (prativadi approves counter) or terminal `done` after dual final approval
- `counter_review (vadi_counter)` → `review_of_review (prativadi_fixups)` (prativadi disapproves counter, writes new fix; `disagreement_round += 1`)
- `counter_review (vadi_counter)` → `human_decision` (when `disagreement_round >= cap`)

**Universal:**

- any state → `human_decision` (escalation)
- `human_decision` → any state (after human edits baton or prompts an agent)

Any other transition is illegal in v1. The writing agent must reject illegal transitions and route to `human_decision` instead.

## Handoff Rule

The active agent must stop doing LLM work after writing a baton that assigns the next action to another actor. In default `run_mode: "walkaway"`, it then blocks in the foreground wait helper instead of exiting the overall run.

Every baton write goes through `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh`, which validates the v1 transition, installs atomically, and snapshots the checkpoint.

This is the core anti-token-polling rule:

- The vadi does not spend model turns asking whether the prativadi moved.
- The prativadi does not spend model turns asking whether the vadi moved.
- In walkaway mode, the assigned-away agent runs `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role <vadi|prativadi> --interval 60 --max-wait 540`.
- In supervised mode, the assigned-away agent exits and the human invokes the next role manually.
- When the helper exits 0, the agent re-reads the baton and resumes.
- When the helper exits 10, 11, or 12, the agent surfaces `done`, `human_decision`, or `human_question` and stops. For `human_question`, the helper also prints `question`, `resume_assignee`, and `resume_status`.

## Goal Conditions

Use `/goal` around the baton state instead of around a timer.

The canonical v1 goal conditions are embedded in the two skill bodies (`plugins/dvandva/skills/vadi/SKILL.md` and `plugins/dvandva/skills/prativadi/SKILL.md`) under their `/goal condition` sections. Always use the version from the skill file rather than copying from this doc, since the skill version is what the goal evaluator actually parses against.

Vadi goal (paste into your engine):

```
/goal You are Dvandva vadi. Continue the Dvandva walkaway run until .dvandva/baton.json status is "done", "human_question", or "human_decision". If assignee is not "vadi", run ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role vadi --interval 60 --max-wait 540, then re-read the baton when it returns 0. Before each checkpoint, surface BATON_STATE, changed files, verification commands and outcomes, and final approval fields. Never create a PR. Stop after the baton turn_cap and assign human if still blocked.
```

Prativadi goal (paste into your engine):

```
/goal You are Dvandva prativadi. Continue the Dvandva walkaway run until .dvandva/baton.json status is "done", "human_question", or "human_decision". If assignee is not "prativadi", run ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --interval 60 --max-wait 540, then re-read the baton when it returns 0. Before each checkpoint, surface BATON_STATE, findings, verification commands and outcomes, final approval fields, and the final baton contents. Never create a PR.
```

Both goals require the agent to surface a structured `BATON_STATE: { ... }` line at every checkpoint. The `/goal` evaluator detects exit conditions by reading that line in the transcript.

## Why Not LLM Polling

Two autonomous sessions using model turns to poll the same channel recreate the PR 353 problem locally. They spend tokens checking whether the other agent has moved.

The better default is serialized model work with shell waiting:

1. One agent runs.
2. It writes a baton.
3. It blocks in the wait helper if the run is still active.
4. The already-running next actor wakes and works.

Parallelism should be explicit and branch-scoped.
