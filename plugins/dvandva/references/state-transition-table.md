# Dvandva Baton State Transitions

This is the plugin-local reference for `dvandva.baton.v1` and the v2 design
contract. The skill bodies carry the operational checklist; this file is the
bundled transition reference available after plugin install.

The bundled `dvandva-write.sh` helper enforces v1 and v2 transition subsets deterministically at write time; `scripts/test-dvandva-write.sh` asserts every documented v1 edge plus the v2 research/test/review/deslop edges below.

## v2 Additions

`dvandva.baton.v2` is the run-scoped schema for named runs at
`.dvandva/runs/<run_id>/baton.json`. `<run_id>` must be one safe path segment:
letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`; once a
v2 baton exists, its `run_id` is immutable for that run. v2 adds:

- `run_id`: stable non-empty safe identifier shared by both role sessions.
- `original_ask`: the user's original request, surfaced in preflight so long
  goal loops do not drift.
- `research_ref`: path to the generated user-facing HTML research artifact.
- `work_split`: planned ownership map for vadi, prativadi, human, or subagents; records phase, owner, scope, paths, status, and artifact refs.
- `verification_matrix`: planned evidence map from claims and risks to checks, owners, expected results, command or inspection, result, evidence refs, and the 100% test coverage target for new behavior.
- `turn_cap`: default `60`; passive shell wait heartbeats do not count as
  active model-work turns.
- `dvandva-wait.sh --persist`: treats `--max-wait` as the heartbeat interval.
  Optional `--persist-max <seconds>` is a total wall-clock cap. The
  wait-helper persist cap exit 23 means that cap was reached. Claude-hosted
  sessions should keep the cap below the Bash-tool limit or use finite
  540-second re-loops; Codex-hosted sessions can use unbounded persistent waits
  when the shell budget supports it.
- `dvandva-write.sh`: the write-helper validation exit 23 means a baton
  candidate failed schema, required-key, safe-run-id, status-owner, status, or
  enum validation. Fix the candidate and rerun the helper; do not edit the
  installed baton directly.

Legacy `.dvandva/baton.json` may continue using `dvandva.baton.v1`. Phase 6
adds live v2 write-helper enforcement: v2 writers require safe `run_id`,
`original_ask`, `work_split`, and `verification_matrix`; they also require a
non-empty `research_ref` before advancing beyond the initial research draft,
except that `human_question` and `human_decision` remain legal early-escalation
targets before `research_ref` exists. Existing batons cannot change schema or v2
`run_id` mid-run.

## Schema Fields

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": "ISO-8601 UTC timestamp, set by the agent that last wrote the baton",
  "mode": "feature-pr | campaign",
  "run_mode": "walkaway | supervised",
  "phase": "spec | 1 | 2 | ... | done",
  "total_phases": "integer, set during spec phase, immutable thereafter unless human edits",
  "status": "spec_drafting | spec_review | spec_revision | human_question | implementing | test_creation | deep_review | deslop | phase_review | phase_fixing | review_of_review | counter_review | human_decision | done",
  "assignee": "non-empty string; v1 conventions are vadi | prativadi | human; v2 status-owner pairs are enforced",
  "current_engine": "optional; claude | codex | null. Records which CLI wrote the most recent baton; traceability only.",
  "review_target": "spec | implementation | prativadi_fixups | vadi_counter | null",
  "research_ref": "v2 path to gitignored generated HTML research file under ./superpowers/research/, set during research phase",
  "plan_ref": "path to gitignored generated HTML plan file under ./superpowers/plans/, set during spec phase",
  "work_split": "v2 array/object describing planned ownership by phase, owner, scope, paths, status, and artifact refs",
  "verification_matrix": "v2 array/object mapping claims and risks to planned checks, owners, expected evidence, result, and evidence_ref",
  "master_plan_locked": "boolean; false during planning, true once prativadi advances to phase 1",
  "question": "string | null; one concrete user question when status is human_question",
  "resume_assignee": "vadi | prativadi | null; role to resume after a human_question answer",
  "resume_status": "spec_drafting | spec_review | spec_revision | null; status to restore after a human_question answer",
  "disagreement_round": "integer, reset to 0 by the agent that writes the first baton of each new phase",
  "disagreement_cap": "integer, default 3, optionally set during spec phase",
  "turn_cap": "integer, default 60; passive shell wait heartbeats do not count",
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

## Research Phase (v2)

| From | To | Trigger |
|---|---|---|
| no named-run baton | `phase: "research", status: "research_drafting"` | Vadi first run for a v2 named run |
| `research_drafting` | `research_review` | Vadi writes `research_ref` and hands to prativadi |
| `research_review` | `research_revision` | Prativadi surfaces research findings back to vadi |
| `research_revision` | `research_review` | Vadi addresses research findings and hands back |
| `research_review` | `phase: "spec", status: "spec_drafting"` | Prativadi accepts research and starts spec drafting |
| any research state while `master_plan_locked: false` | `human_question` | Either agent needs one human answer before spec lock |
| any research state | `human_decision` | Either agent escalates |

## Implementation Phase

| From | To | Trigger |
|---|---|---|
| `phase: N, implementing` | `phase: N, status: phase_review, review_target: implementation` | Vadi completes phase, hands to prativadi |
| `phase: N, implementing` | `test_creation` | v2: Vadi completes implementation and starts separate test creation |
| `test_creation` | `deep_review` | v2: Vadi records tests, 100% test coverage evidence for new behavior, and verification results |
| `deep_review` | `phase_fixing` | v2: Prativadi finds bugs, missing tests, verification gaps, or substantive review issues |
| `deep_review` | `deslop` | v2: Prativadi accepts behavior and tests, then routes cleanup |
| `phase_fixing` | `test_creation` | v2: Vadi fixed behavior, tests, or verification gaps and must refresh test evidence before review |
| `deslop` | `phase_fixing` | v2: Cleanup finds behavior, test, or review blockers |
| `deslop` | `phase: N+1, status: implementing, disagreement_round: 0` | v2: no nits, low/minor bugs, stale wording, or unclear instructions remain except explicitly accepted `deferred` items |
| `deslop` | final `done` | v2: final phase passed implementation, test_creation, deep_review, deslop, and dual approval |
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

## Regular checkpoint commits

Regular checkpoint commits are local commits made after verified logical slices
when `allow_commit == true`. Commit only the baton's intended `changed_paths`
union, excluding `.dvandva/` and `superpowers/`, and only when `git status
--short` has no unrelated dirty paths. Record the hash in `verification` or
`summary` as `checkpoint_commit=<hash>`. Do not push checkpoint commits; final
push remains gated by both final approvals and `allow_push == true`.

## v2 Assignee Ownership

The v2 helper validates the next candidate's `assignee` against the status it
writes:

- Vadi-owned: `research_drafting`, `research_revision`, `spec_drafting`,
  `spec_revision`, `implementing`, `test_creation`, `deslop`, `phase_fixing`,
  `review_of_review`.
- Prativadi-owned: `research_review`, `spec_review`, `deep_review`,
  `phase_review`, `counter_review`.
- Human-owned: `human_question`, `human_decision`.
- Terminal `done` is accepted as terminal regardless of assignee; wait helpers
  stop on `done`.

Any other transition is illegal in v1 or v2 and must be rejected by the writing
agent.
