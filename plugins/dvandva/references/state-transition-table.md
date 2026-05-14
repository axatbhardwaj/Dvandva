# Dvandva Baton State Transitions

This is the plugin-local reference for `dvandva.baton.v1`. The skill bodies carry the operational checklist; this file is the bundled transition reference available after plugin install.

## Schema Fields

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": "ISO-8601 UTC timestamp, set by the agent that last wrote the baton",
  "mode": "feature-pr | campaign",
  "run_mode": "walkaway | supervised",
  "phase": "spec | 1 | 2 | ... | done",
  "total_phases": "integer, set during spec phase, immutable thereafter unless human edits",
  "status": "spec_drafting | spec_review | spec_revision | human_question | implementing | phase_review | phase_fixing | review_of_review | counter_review | human_decision | done",
  "assignee": "non-empty string; v1 conventions are vadi | prativadi | human",
  "current_engine": "optional; claude | codex | null. Records which CLI wrote the most recent baton; traceability only.",
  "review_target": "spec | implementation | prativadi_fixups | vadi_counter | null",
  "plan_ref": "path to gitignored Superpowers plan file under ./superpowers/plans/, set during spec phase",
  "master_plan_locked": "boolean; false during planning, true once prativadi advances to phase 1",
  "question": "string | null; one concrete user question when status is human_question",
  "resume_assignee": "vadi | prativadi | null; role to resume after a human_question answer",
  "resume_status": "spec_drafting | spec_review | spec_revision | null; status to restore after a human_question answer",
  "disagreement_round": "integer, reset to 0 by the agent that writes the first baton of each new phase",
  "disagreement_cap": "integer, default 3, optionally set during spec phase",
  "turn_cap": "integer, default 20, applied to each /goal invocation",
  "branch": "git branch name",
  "checkpoint": "integer, bumped by the writer",
  "allow_commit": "boolean; default true in walkaway mode",
  "allow_push": "boolean; default true in walkaway mode",
  "allow_pr": "boolean; default false; v1 skills must never create a PR",
  "vadi_final_approval": "boolean; true only when vadi approves final diff",
  "prativadi_final_approval": "boolean; true only when prativadi approves final diff",
  "final_commit": "git commit hash | null; set after final commit",
  "pushed_ref": "git ref | null; set after final push",
  "summary": "one-paragraph human-readable summary of this checkpoint",
  "changed_paths": ["run-level union of intended files touched so far; excludes .dvandva/ and superpowers/"],
  "verification": [
    { "command": "exact shell command run", "result": "passed | failed | skipped", "notes": "optional one-liner" }
  ],
  "findings": ["reviewer or counter-reviewer: bullets describing issues found"],
  "narrow_fixups": ["reviewer: bullets describing narrow fixes applied directly"],
  "vadi_counter": ["vadi-as-reviewer: bullets describing counter-changes proposed during mutual review"],
  "deferred": ["reviewer: items deferred with one-line rationale and next-recommended-action"],
  "blockers": ["bullets describing what is blocking forward progress"],
  "next_action": "exact one-sentence instruction for the next actor"
}
```

## Spec Phase

| From | To | Trigger |
|---|---|---|
| no baton | `phase: "spec", status: "spec_drafting"` | Vadi first run |
| `spec_drafting` | `spec_review` | Vadi hands plan to prativadi for Q&A |
| `spec_review` | `spec_revision` | Prativadi surfaces Q&A back to vadi |
| `spec_review` | `phase: 1, status: implementing` | Prativadi accepts plan and freezes `total_phases` on the baton |
| `spec_revision` | `spec_review` | Vadi answers Q&A, hands back |
| any spec state while `master_plan_locked: false` | `human_question` | Either agent needs one human answer before master plan lock |
| `human_question` | `resume_status` with `assignee: resume_assignee` | Human answers and the receiving skill clears question fields |
| any spec state | `human_decision` | Either agent escalates |

## Implementation Phase

| From | To | Trigger |
|---|---|---|
| `phase: N, implementing` | `phase: N, status: phase_review, review_target: implementation` | Vadi completes phase, hands to prativadi |
| `phase: N, implementing` | `human_decision` | Vadi blocked |
| `phase_review (impl)` | `phase_fixing` | Prativadi hands back substantive findings |
| `phase_review (impl)` | `review_of_review, review_target: prativadi_fixups` | Prativadi applied narrow fixups, mutual review owed |
| `phase_review (impl)` | `phase: N+1, status: implementing, disagreement_round: 0` | Prativadi approves, no changes |
| `phase_review (impl)` | final `done` | Final phase approved by both roles; optional commit/push complete; PR creation remains false |
| `phase_review (impl)` | `human_decision` | Prativadi escalates |
| `phase_fixing` | `phase_review (impl)` | Vadi addressed findings, re-hands |
| `phase_fixing` | `human_decision` | Vadi blocked during fix |

## Mutual Review And Disagreement

| From | To | Trigger |
|---|---|---|
| `review_of_review (prativadi_fixups)` | `phase: N+1, status: implementing, disagreement_round: 0` | Vadi approves prativadi fixups |
| `review_of_review (prativadi_fixups)` | final `done` | Final phase fixups approved by both roles; optional commit/push complete |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | Vadi disapproves, writes counter, increments `disagreement_round` |
| `review_of_review (prativadi_fixups)` | `human_decision` | `disagreement_round >= disagreement_cap` |
| `counter_review (vadi_counter)` | `phase: N+1, status: implementing, disagreement_round: 0` | Prativadi approves counter |
| `counter_review (vadi_counter)` | final `done` | Final phase counter approved by both roles; optional commit/push complete |
| `counter_review (vadi_counter)` | `review_of_review, review_target: prativadi_fixups` | Prativadi disapproves counter, applies a different fix, increments `disagreement_round` |
| `counter_review (vadi_counter)` | `human_decision` | `disagreement_round >= disagreement_cap` |

## Universal

| From | To | Trigger |
|---|---|---|
| any state | `human_decision` | Escalation, cap hit, blocker, malformed input, or unsafe dirty tree |
| `human_decision` | any state | Human edits baton or prompts an agent with a decision |

Any other transition is illegal in v1 and must be rejected by the writing agent.
