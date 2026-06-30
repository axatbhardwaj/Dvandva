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

- accepted run modes: `development`, `research`, and `review`. Older batons
  may still serialize `feature-pr`; treat it as a legacy alias for
  `development`. `campaign` is legacy wording and not the current public enum.
- `run_id`: stable non-empty safe identifier shared by both role sessions.
- `original_ask`: the user's original request, surfaced in preflight so long
  goal loops do not drift.
- `research_ref`: path to the generated user-facing HTML research artifact.
- `run_explainer_ref`: path to the final run explainer HTML under `./superpowers/run-reports/`, required before terminal `done`.
- `run_explainer_reviews`: v2 array of role review records for the final run explainer. Development terminal `done` requires completed approved `vadi` and `prativadi` entries whose `artifact_ref` exactly equals `run_explainer_ref`, with non-empty `summary` and `evidence_refs`.
- `research_outcome`: optional nullable v2 field recording the accepted
  research result, including `seed_development` when a research run also emits
  a seed plan for a future development run.
- `review_ref`: optional nullable v2 field pointing to the generated user-facing
  HTML review artifact for review-mode runs.
- `review_intake`: optional nullable v2 field capturing the review-mode intake
  package or selector used to scope the review.
- `active_roles`: v2 concurrent role list. Team-owned statuses use `assignee: "team"` and `active_roles: ["vadi", "prativadi"]`; scalar statuses use an empty array.
- `work_split`: planned ownership map for vadi, prativadi, human, or subagents; records phase, owner, scope, paths, read_paths, write_paths, status, artifact refs, parallelism rationale, dependencies, and optional conflict_group serialization.
- `agent_instances`: first-class registry for generated run-scoped agent instances, including provenance, model/permission class, read/write paths, work item IDs, base checkpoint, output refs, evidence refs, lifecycle status, and closure result. Generated instance IDs must not collide with coordinator owners (`vadi`, `prativadi`, `team`, `human`), seed-roster owners such as `dvandva-implementer`, or legacy standalone owner names such as `adversarial-analyst`. `spawned_by` is executable provenance; `seed_agent` is advisory metadata for humans and brief generation. Dynamic write-path disjointness is checked among generated instances sharing the same `base_checkpoint` and among any two live (`planned`/`running`) instances regardless of base_checkpoint; closed historical instances from earlier base checkpoints do not block later sequential path reuse.
- `subagent_tracks`: actual conditional parallelism record. Parallelize only genuinely disjoint tracks; record what was not parallelized and why when direct execution is safer or when subagent tooling is unavailable.
- `verification_matrix`: planned evidence map from claims and risks to checks, owners, expected results, command or inspection, result, evidence refs, and the 100% test coverage target for new behavior.
- `turn_cap`: default `60`; passive shell wait heartbeats do not count as
  active model-work turns.
- `termination_review`: v2's multipart termination state. It is team-owned
  (`assignee: "team"`, `active_roles: ["vadi", "prativadi"]`) so both roles
  keep polling and explicitly decide whether to stop. `done` is legal only from
  `termination_review` after both final approvals are true. The write helper
  enforces approval and explainer-review ownership via `DVANDVA_ROLE`:
  `DVANDVA_ROLE=vadi` may raise only `vadi_final_approval` and may add/change
  only `run_explainer_reviews` entries with `role: "vadi"`;
  `DVANDVA_ROLE=prativadi` may raise only `prativadi_final_approval` and may
  add/change only entries with `role: "prativadi"`.
- `dvandva-wait.sh`: continuous polling is the hard rule. `--max-wait` is the
  heartbeat interval by default, and the helper keeps polling until role
  ownership, shared terminal `done`, `human_question`, `human_decision`, or user
  interrupt. `human_question` and `human_decision` are a paired run pause that
  stops both roles together. When the selected run is waiting on the peer, the
  wait helper propagates a newer sibling run's `human_decision` or
  `human_question` unless `DVANDVA_CONCURRENT=1`; older sibling
  human-intervention batons are ignored so parked runs cannot hijack newer work.
  For a sibling `human_question`, output must preserve the sibling baton's
  `question`, `resume_assignee`, and `resume_status`.
  After installing a handoff checkpoint, the writer must run the helper with
  `--since-checkpoint <written_checkpoint>`; while the selected baton remains at
  or below that checkpoint, the helper keeps polling even if a team-owned
  `active_roles` field names the writer. It exits 0 with `checkpoint_advanced`
  only after a peer write advances the baton, so the role re-reads before
  deciding whether to work, wait again, or stop together.
  `termination_review` is not terminal; it wakes both roles.
  `--persist` is accepted for older snippets and is redundant. Optional
  `--persist-max <seconds>` is a total wall-clock cap; the wait-helper persist cap exit 23 is not a terminal baton state and must re-enter wait unless the
  user interrupts. Explicit `--finite` compatibility mode is the only path to
  exit 20 and is not valid for normal walkaway loops.
- `dvandva-write.sh`: the write-helper validation exit 23 means a baton
  candidate failed schema, required-key, safe-run-id, status-owner, status, or
  enum validation. Fix the candidate and rerun the helper; do not edit the
  installed baton directly.

Legacy `.dvandva/baton.json` may continue using `dvandva.baton.v1`. The live
v2 write-helper enforcement requires safe `run_id`, `original_ask`,
`run_explainer_ref`, `active_roles`, `work_split`, `agent_instances`,
`subagent_tracks`, and `verification_matrix`; it also requires a non-empty
`research_ref` before advancing beyond the initial research draft, except that
`human_question` and `human_decision` remain legal early-escalation targets
before `research_ref` exists. Existing batons cannot change schema or v2
`run_id` mid-run. Accepted v2 docs use mode-conditional terminal artifact
gates: development runs require `run_explainer_ref`; research runs require
`research_ref` and additionally `plan_ref` iff `research_outcome ==
seed_development`; review runs require `review_ref`. Development terminal
`done` additionally requires completed approved `run_explainer_reviews` from
both roles for the exact `run_explainer_ref`. In all v2 modes, terminal `done`
still requires a coordinator assignee (`human`, `team`, `vadi`, or
`prativadi`), both final approvals, and an installed current baton already at
`termination_review`.

implementation-phase parallelism is mandatory for v2. Spec approval enters `parallel_implementing` with `assignee: "team"` and `active_roles: ["vadi", "prativadi"]`; `work_split` must contain at least five implementation chunks split across both roles for two-team parallel implementation, each with reciprocal `cross_review_by`. After `test_creation`, the baton enters `cross_review`; `cross_review` may route to `cross_fixing`, and only completed cross-review evidence for both roles can advance to `deep_review`. Phase convention: implementation-chunk tracks use the numeric implementation phase, while cross-review and deep-review gate tracks use the status-name phase such as `phase: "cross_review"` or `phase: "deep_review"`.

Run 4 generalizes write gating from Run 3 generated instances to `work_split`.
The helper validates `work_split.paths`, `work_split.read_paths`, and
`work_split.write_paths` with `safe_rel_path`. For write-capable chunks
(`implementation`, `cross_fixing`, and `fix`), `write_paths` supplements rather
than narrows `paths`; the effective write set is their union, so
`write_paths: []` cannot mask the backward-compatible `paths` write surface.
`cross_review` is read-only unless explicit `write_paths` are present. Live
write overlaps are rejected unless both chunks share a non-empty `conflict_group`
and a declared `depends_on` edge serializes one chunk after the other. Closed or
terminal historical chunks do not block later sequential reuse because
work_split has no `base_checkpoint` wave model.

Run 4 work-gating uses repo-local git hooks activated by the single turn-entry
gate `dvandva-preflight.sh --role <role>`. The preflight resolves the active run
selector-first (stopping on exit 12 ASK), then runs the hook stage. The hook
stage detects Dvandva hook adoption status via a functional probe: it installs a
delegating wrapper under `.dvandva/githooks/` (gitignored) and sets only
repo-local `git config core.hooksPath .dvandva/githooks`. When a prior `core.hooksPath`
exists (e.g., set by Husky), the installer records it as `dvandva.priorHooksPath`, the
delegating wrapper execs the prior hook chain on every commit so the foreign owner keeps
firing, and the prior `core.hooksPath` is restored on uninstall — the foreign owner is
preserved through record, delegate, and restore. The installer records
`dvandva.hooksAdoptedAt` as the local drift baseline.
The delegating `pre-commit` wrapper runs the gate then execs the prior hook; the
`prepare-commit-msg` wrapper delegates first then stamps `Dvandva-Checkpoint`.
`scripts/dvandva-drift-lint.sh` detects unstamped commits from the hook-adoption
baseline floor, so a later checkpoint cannot hide an unstamped sandwich bypass.
This is shell/git-hook enforcement only; it does not create a daemon, scheduler,
or hidden central process.
For git work-gating, terminal `done`, `human_question`, and `human_decision`
batons are inactive: the commit gate allows them, and drift lint only reports
off-protocol commits while at least one non-terminal baton is active or when
checkpoint history gives it a scan floor.

## Concurrent advance and stale writes

Two roles can advance the baton concurrently in team-owned states. The following
exit codes guard against consistency failures.

Exit codes 2, 23, 27, 28, and 29 (`lock_lost`) are `dvandva-write.sh` codes.
Exit code 29 (`split_brain`) is a `dvandva-wait.sh` code. The two exit-29 codes
are distinct and must not be confused: `lock_lost` fires when a write-time
fencing-token mismatch aborts an install; `split_brain` fires when the wait
helper detects two concurrent active runs that both claim the same role.

| Exit code | Name | Meaning and recovery |
|---|---|---|
| `2` | `bad_lock_timeout` | `dvandva-write.sh`: `DVANDVA_LOCK_TIMEOUT` is not a canonical positive decimal (`^[1-9][0-9]*$`). Zero, negative, leading-zero forms (`08`, `09`), and non-numeric values all fail closed before the lock loop fires. Fix or unset `DVANDVA_LOCK_TIMEOUT` (default is `30` seconds), then rerun. |
| `23` | `bad_run_id_dir` | `dvandva-write.sh`: the baton at `.dvandva/runs/<seg>/baton.json` has `schema==v2` but its `run_id` field is null, missing, empty, or mismatches `<seg>`. Fix the candidate `run_id` and rerun; do not edit the installed baton directly. |
| `27` | `stale_checkpoint` | `dvandva-write.sh`: emits `DVANDVA_WRITE stale_checkpoint current=<c> candidate=<n>` when `new_checkpoint <= cur_checkpoint` — the peer advanced while this candidate was being prepared. Re-read the installed baton, re-derive the next state from the mode table, rewrite the candidate, and rerun the helper. Never bump past the peer's checkpoint. |
| `28` | `lock_unavailable` | `dvandva-write.sh`: a non-directory (e.g. a leftover file or a squatter) occupies the lock path `<baton-dir>/.baton.lock.d`; the critical section never runs unlocked. Fail closed. Investigate and remove the squatter, then rerun. |
| `29` | `lock_lost` (`dvandva-write.sh`) | `dvandva-write.sh`: the fencing token was overwritten by a peer that age-stole the lock while this writer was slow. The install is aborted fail-closed; the baton is unchanged. Re-read the baton and re-derive the next state before retrying. **Distinct from** `dvandva-wait.sh` exit `29` (`split_brain`). |
| `29` | `split_brain` (`dvandva-wait.sh`) | `dvandva-wait.sh`: a sibling active baton has `assignee == my role` while the selected baton is waiting on a peer — two simultaneous active runs both think they own the same role. Reconcile selector choice; park the stale duplicate to `human_decision`. Suppress with `DVANDVA_CONCURRENT=1` only when explicitly running two independent parallel Dvandva loops in the same worktree. **Distinct from** `dvandva-write.sh` exit `29` (`lock_lost`). |

### Test-only seam: `DVANDVA_WRITE_BARRIER`

`dvandva-write.sh` contains a synchronization seam gated on the `DVANDVA_WRITE_BARRIER`
environment variable. When the variable is unset (all production runs), the `if`
branch is never entered — it is a no-op, a single string test. When set by a
test harness, the helper touches and stats sentinel files named
`${DVANDVA_WRITE_BARRIER}.arrived` and `${DVANDVA_WRITE_BARRIER}.release` in
order to let `scripts/test-dvandva-write.sh` park a writer after the transition
check but before the atomic install, exercising the fencing-token and stale-
checkpoint guarantees deterministically. The seam never reads or executes arbitrary
input. This is an accepted test-only seam; no behavior change is required.

Run 4 retirement is Dvandva-only. It may retire only Dvandva-covered standalone
Claude symlink workflows with functional parity via Runs 1-4 usage:
`adversarial-analyst`, `architect`, `developer`, `quality-reviewer`, and
`sandbox-executor`. Codex agent-axis retirement is a no-op, skills are out of
scope, and backup manifest restore keeps the action reversible.

Team-owned v2 states (`parallel_implementing`, `cross_review`, `cross_fixing`,
`termination_review`) may write same-status sync checkpoints when both roles
remain active. Use them to record partial completion, task distribution, peer
wait state, or shared stop-review evidence without advancing the lifecycle
early. Scalar-owner states still reject same-status rewrites.

## Schema Fields

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": "ISO-8601 UTC timestamp, set by the agent that last wrote the baton",
  "mode": "development | research | review | feature-pr (legacy alias for development)",
  "run_mode": "walkaway | supervised",
  "phase": "research | spec | review | 1 | 2 | ... | done",
  "total_phases": "integer, set during spec phase, immutable thereafter unless human edits",
  "status": "spec_drafting | spec_review | spec_revision | human_question | implementing | parallel_implementing | test_creation | cross_review | cross_fixing | deep_review | deslop | termination_review | phase_review | phase_fixing | review_of_review | counter_review | human_decision | done",
  "assignee": "non-empty string; v1 conventions are vadi | prativadi | human; v2 status-owner pairs include team for concurrent states",
  "active_roles": "v2 concurrent roles array, usually [] or [\"vadi\", \"prativadi\"]",
  "current_engine": "optional; claude | codex | null. Records which CLI wrote the most recent baton; traceability only.",
  "review_target": "research | spec | implementation | prativadi_fixups | vadi_counter | null",
  "research_ref": "v2 path to gitignored generated HTML research file under ./superpowers/research/, set during research phase",
  "run_explainer_ref": "v2 path to gitignored final run explainer HTML under ./superpowers/run-reports/, required before terminal done for development mode",
  "run_explainer_reviews": "v2 array of role review records for the final run explainer; development done requires completed approved vadi and prativadi entries whose artifact_ref exactly equals run_explainer_ref, with non-empty summary and evidence_refs",
  "research_outcome": "nullable v2 field; accepted research result, including seed_development when a research run seeds a future development run",
  "review_ref": "nullable v2 path to gitignored generated HTML review artifact under ./superpowers/reviews/",
  "review_intake": "nullable v2 field carrying review-mode intake scope or selector",
  "plan_ref": "path to gitignored generated HTML plan file under ./superpowers/plans/, set during spec phase",
  "work_split": "v2 array/object describing planned ownership by phase, owner, scope, paths, read_paths, write_paths, conflict_group, depends_on, status, and artifact refs",
  "agent_instances": "v2 array recording generated run-scoped agent instances, provenance, model/permission class, read/write paths, work item IDs, base checkpoint, output refs, evidence refs, lifecycle status, and closure result",
  "subagent_tracks": "v2 array recording actual conditional parallelism tracks, owner, evidence refs, fallback rationale, and result",
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

## Development Mode (v2, 26 edges)

This is the currently enforced v2 development table from
`scripts/test-dvandva-write.sh` (`V2_EDGES`). It remains unchanged. Every route
into `termination_review` is explicit; there is no wildcard
`* -> termination_review`.

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Vadi writes `research_ref` and hands to prativadi. |
| `research_review` | `research_revision` | Prativadi surfaces research findings back to vadi. |
| `research_revision` | `research_review` | Vadi addresses research findings and hands back. |
| `research_review` | `phase: "spec", status: "spec_drafting"` | Prativadi accepts research and starts development planning. |
| `spec_drafting` | `spec_review` | Vadi hands the development plan to prativadi for Q&A. |
| `spec_review` | `spec_revision` | Prativadi surfaces planning Q&A back to vadi. |
| `spec_revision` | `spec_review` | Vadi answers Q&A and re-hands the plan. |
| `spec_review` | `phase: 1, status: "parallel_implementing"` | Prativadi accepts the plan, freezes `total_phases`, and activates both roles. |
| `phase: N, parallel_implementing` | `test_creation` | Both roles completed implementation chunks and recorded implementation evidence. |
| `test_creation` | `cross_review` | Vadi records tests, 100% coverage evidence for new behavior, and verification results. |
| `cross_review` | `cross_fixing` | One or both reciprocal cross-review tracks found peer-owned chunk defects. |
| `cross_fixing` | `test_creation` | Cross-review findings were fixed and tests/evidence must be refreshed. |
| `cross_review` | `deep_review` | Both roles recorded completed cross-review tracks for peer-owned chunks. |
| `deep_review` | `phase_fixing` | Prativadi finds bugs, missing tests, verification gaps, or substantive review issues. |
| `deep_review` | `review_of_review, review_target: prativadi_fixups` | Prativadi applied narrow fixups after the required review angles; vadi must inspect the fixup diff before cleanup continues. |
| `deep_review` | `deslop` | Prativadi accepts behavior and tests after the required review angles are complete. |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | Vadi disapproves prativadi fixups and writes a counter-change. |
| `review_of_review (prativadi_fixups)` | `deslop` | Vadi approves prativadi fixups; v2 returns to cleanup rather than advancing or terminating directly. |
| `counter_review (vadi_counter)` | `review_of_review, review_target: prativadi_fixups` | Prativadi disapproves the counter and applies a different narrow fixup. |
| `counter_review (vadi_counter)` | `deslop` | Prativadi approves the counter; v2 returns to cleanup rather than advancing or terminating directly. |
| `phase_fixing` | `test_creation` | Vadi fixed behavior, tests, or verification gaps and must refresh test evidence. |
| `deslop` | `phase_fixing` | Cleanup finds behavior, test, or review blockers. |
| `deslop` | `phase: N+1, status: "parallel_implementing"` | A non-final phase is clean and the next development phase begins. |
| `deslop` | `termination_review` | The final development phase is clean; the run enters the shared stop-review gate. |
| `termination_review` | `phase_fixing` | One role rejects final stop because behavior, tests, docs, or run artifacts still need work. |
| `termination_review` | final `done` | Both roles explicitly decide to stop, both approval bits are true, `run_explainer_ref` is set, and `run_explainer_reviews` contains completed approved entries from both roles for that exact artifact. |

## Research Mode (v2, 12 edges)

Research mode ends as research. It may optionally produce a seed-development
plan before termination; when it does, set `research_outcome:
"seed_development"` and populate `plan_ref`. Required phases are mode-specific:
`research_*` statuses use `phase: "research"`; seed-plan statuses,
`phase_fixing`, `termination_review`, and terminal `done` use `phase: "spec"`.

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Vadi writes `research_ref` and hands to prativadi. |
| `research_review` | `research_revision` | Prativadi surfaces research findings back to vadi. |
| `research_revision` | `research_review` | Vadi addresses research findings and hands back. |
| `research_review` | `spec_drafting` | Accepted outcome is `seed_development`; vadi must draft a seed plan before the research run can terminate. |
| `spec_drafting` | `spec_review` | Vadi hands the seed-development plan to prativadi for Q&A. |
| `spec_review` | `spec_revision` | Prativadi surfaces seed-plan Q&A back to vadi. |
| `spec_revision` | `spec_review` | Vadi answers Q&A and re-hands the seed plan. |
| `spec_review` | `termination_review` | The seed-development plan is accepted; `plan_ref` is set and the research run enters shared stop review. |
| `research_review` | `termination_review` | The accepted research outcome is not `seed_development`; `research_ref` is sufficient to enter shared stop review. |
| `termination_review` | `phase_fixing` | Shared stop review finds research or seed-plan gaps and routes to a focused fixing pass. |
| `phase_fixing` | `research_review` | The focused fix is complete and the research package must be re-reviewed. |
| `termination_review` | final `done` | Both roles explicitly decide to stop; `research_ref` exists, and `plan_ref` also exists iff `research_outcome == seed_development`. |

## Review Mode (v2, 9 edges)

Review mode reuses the existing research statuses for intake and the existing
review statuses for the review package. Set `review_intake` and `review_target`
before the first `deep_review`, and produce `review_ref` before shared
termination. Every review-mode status uses `phase: "review"`.

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Vadi writes review scope/intake research and hands to prativadi. |
| `research_review` | `research_revision` | Prativadi finds intake gaps and routes back to vadi. |
| `research_revision` | `research_review` | Vadi fixes intake gaps and hands back. |
| `research_review` | `deep_review` | Intake is sufficient and the review package can be evaluated. |
| `deep_review` | `deslop` | Substantive review passes and only cleanup or wording polish remains. |
| `deslop` | `termination_review` | Review cleanup is complete and `review_ref` is ready for the shared stop gate. |
| `termination_review` | `phase_fixing` | Shared stop review finds review-package work still owed. |
| `phase_fixing` | `deep_review` | Requested fixes or evidence refreshes are complete and the review must be rerun. |
| `termination_review` | final `done` | Both roles explicitly decide to stop and `review_ref` is set. |

## Legacy v1 explicit fallback

| From | To | Trigger |
|---|---|---|
| no legacy baton, legacy explicitly selected | `phase: "spec", status: "spec_drafting"` | Vadi first run on the legacy v1 fallback only. |
| `spec_drafting` | `spec_review` | Vadi hands plan to prativadi for Q&A. |
| `spec_review` | `spec_revision` | Prativadi surfaces Q&A back to vadi. |
| `spec_review` | `phase: 1, status: implementing` | Legacy v1 path: prativadi accepts plan and freezes `total_phases` on the baton. |
| `spec_revision` | `spec_review` | Vadi answers Q&A and re-hands the plan. |
| `phase: N, implementing` | `phase: N, status: phase_review, review_target: implementation` | Legacy v1 direct review path only. |
| `phase_review (impl)` | `phase_fixing` | Prativadi hands back substantive findings. |
| `phase_review (impl)` | `review_of_review, review_target: prativadi_fixups` | Prativadi applied narrow fixups and mutual review is owed. |
| `phase_review (impl)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1: prativadi approves with no changes. |
| `phase_review (impl)` | final `done` | Legacy v1 final phase approved by both roles; optional commit/push complete. |
| `phase_fixing` | `phase_review (impl)` | Vadi addressed findings and re-handed. |
| `review_of_review (prativadi_fixups)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1: vadi approves prativadi fixups. |
| `review_of_review (prativadi_fixups)` | final `done` | Legacy v1: final phase fixups approved by both roles; optional commit/push complete. |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | Vadi disapproves, writes counter, increments `disagreement_round`. |
| `review_of_review (prativadi_fixups)` | `human_decision` | `disagreement_round >= disagreement_cap`. |
| `counter_review (vadi_counter)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1: prativadi approves counter. |
| `counter_review (vadi_counter)` | final `done` | Legacy v1: final phase counter approved by both roles; optional commit/push complete. |
| `counter_review (vadi_counter)` | `review_of_review, review_target: prativadi_fixups` | Prativadi disapproves counter, applies a different fix, increments `disagreement_round`. |
| `counter_review (vadi_counter)` | `human_decision` | `disagreement_round >= disagreement_cap`. |

## Shared intervention edges

| From | To | Trigger |
|---|---|---|
| development or research planning state with `master_plan_locked: false` | `human_question` | Either agent needs one human answer before plan lock. |
| `human_question` | `resume_status` with `assignee: resume_assignee` | Human answers and the receiving skill clears question fields. |
| any state | `human_decision` | Escalation, cap hit, blocker, malformed input, or unsafe dirty tree. |
| `human_decision` | any mode-owned state chosen by the human | Human edits baton or prompts an agent with a decision. |

## Regular checkpoint commits

Regular checkpoint commits are local commits made after verified logical slices
when `allow_commit == true`. Checkpoint commits require Dvandva hook adoption (the
delegating wrapper active and the turn preflight exit 0 before the commit).
Commit only the baton's intended `changed_paths` union, excluding `.dvandva/` and
`superpowers/`, and only when `git status --short` has no unrelated dirty paths.
Record the hash in `verification` or `summary` as `checkpoint_commit=<hash>`. Do
not push checkpoint commits; final push remains gated by both final approvals and
`allow_push == true`.

## v2 Assignee Ownership

The v2 helper validates the next candidate's `assignee` against the status it
writes:

- Vadi-owned: `research_drafting`, `research_revision`, `spec_drafting`,
  `spec_revision`, `implementing`, `test_creation`, `deslop`, `phase_fixing`,
  `review_of_review`.
- Team-owned: `parallel_implementing`, `cross_review`, `cross_fixing`,
  `termination_review`; these require `active_roles: ["vadi", "prativadi"]`.
- Prativadi-owned: `research_review`, `spec_review`, `deep_review`,
  `phase_review`, `counter_review`.
- Human-owned: `human_question`, `human_decision`.
- Terminal `done` has no status owner. It is accepted as terminal for a
  coordinator assignee (`human`, `team`, `vadi`, or `prativadi`) only from
  `termination_review` in any v2 mode. The final gate still requires both
  final approvals plus the mode-conditional terminal artifact:
  `run_explainer_ref` plus both roles' completed approved
  `run_explainer_reviews` for that exact artifact in development,
  `research_ref` plus `plan_ref` iff `research_outcome == seed_development`
  for research, and `review_ref` for review. Wait helpers stop on `done`.
  Raising `vadi_final_approval` requires
  `DVANDVA_ROLE=vadi`; raising `prativadi_final_approval` requires
  `DVANDVA_ROLE=prativadi`.

Any other transition is illegal in v1 or v2 and must be rejected by the writing
agent.

## Dynamic Agent Instances (Run 3)

The 15-agent roster is the **seed roster**. Run 3 enables run-scoped dynamic agent generation via `agent_instances` on the baton — a first-class registry separate from `subagent_tracks`.

### Model classes (vendor-neutral)

| Class label | Use | Claude Code | Codex |
|---|---|---|---|
| `opus-class\|gpt-5.5` | Architecture, planning, review | Opus-class | gpt-5.5 |
| `sonnet-class\|gpt-5.4` | Implementation, documentation | Sonnet-class | gpt-5.4 |

Do not use `haiku` for Dvandva dynamic agents.

### Permission classes

`readonly`, `verify-only`, `edit-scoped`, `write-artifact-only`.

### Protocol invariants for generated instances

| Invariant | Rule |
|---|---|
| **Single-writer** | Generated agents never own baton `assignee`, phase transitions, or final approvals. The parent role serializes all evidence into one monotonic checkpoint write. |
| **No daemon** | No background scheduler, mailbox, or launcher. The baton and foreground wait helper remain the only coordination channel. |
| **Explicit closure** | Every generated handle must be explicitly closed before its track counts as complete. Codex closure evidence includes `closed:<handle>` or equivalent harness-specific proof. |
| **Dynamic write-path disjointness** | Write-path overlaps between generated instances sharing the same `base_checkpoint`, or between any two live (`planned`/`running`) instances regardless of base_checkpoint, are rejected unless they share a `conflict_group` with explicit dependency serialization. Closed historical instances from earlier base checkpoints are not part of the collision set. |
| **No additive sprawl** | Generated instances are run-scoped and ephemeral. The seed roster is never modified at runtime; promotion requires a later reviewed source change. |

### Run3/Run4 boundary

- **Run 3**: dynamic agent creation via `agent_instances`, single-writer merge enforcement, explicit closure gate, and dynamic write-path disjointness validation in the write helper.
- **Run 4**: generalized `work_split.write_paths` path-gate beyond the Run 3 dynamic disjointness check, repo-local git work-gating, and Dvandva-only standalone-agent retirement once the seed roster covers the same scope with functional parity via Runs 1-4 usage.
