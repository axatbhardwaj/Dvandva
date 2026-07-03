# Dvandva Baton State Transitions

This is the plugin-local reference for `dvandva.baton.v1` and the v2 design
contract. The skill bodies carry the operational checklist; this file is the
bundled transition reference available after plugin install.

The `dvandva write` subcommand enforces v1 and v2 transition subsets deterministically at write time; the write port's `cargo test` suite asserts every documented v1 edge plus the v2 research/test/review/deslop edges below.

Status catalog (22): research_drafting, research_review, research_revision, spec_drafting, spec_review, spec_revision, implementing, parallel_implementing, test_creation, cross_review, cross_fixing, deep_review, review_of_review, counter_review, deslop, termination_review, phase_review, phase_fixing, human_question, human_decision, done, abandoned

`dvandva lint schema-parity` (S6-T1) holds this line equal to the engine `dvandva.baton.v2` status enum, `baton-schema-v2.json` `status_catalog`, and the `product.md` copy. `done` and `abandoned` (S2-T1) are the two terminal statuses.

## v2 Additions

`dvandva.baton.v2` is the run-scoped schema for named runs at
`.dvandva/runs/<run_id>/baton.json`. `<run_id>` must be one safe path segment:
letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`; once a
v2 baton exists, its `run_id` is immutable for that run. v2 adds:

- accepted run modes: `development`, `research`, and `review`. Older batons
  may still serialize `feature-pr`; treat it as a legacy alias for
  `development`. `campaign` is legacy wording and not the current public enum.
- development flow profiles: `fast`, `standard`, and `full`. `profile` is
  orthogonal to `mode`; new development scaffolds default to `standard`, while
  existing development/legacy `feature-pr` batons that lack `profile` are
  effective `full` for compatibility. `profile_floor` records the minimum
  allowed profile after risk analysis. `profile_decision` records selected
  profile, floor, reason, risk inputs, hard triggers, allowlist evidence, and
  evidence refs. `profile_history` records profile/floor changes.
- `run_id`: stable non-empty safe identifier shared by both role sessions.
- `original_ask`: the user's original request, surfaced in preflight so long
  goal loops do not drift.
- `research_ref`: path to the generated user-facing HTML research artifact.
- `run_explainer_ref`: path to the final run explainer HTML under `./superpowers/run-reports/`, required before terminal `done` for full-profile development runs.
- `run_explainer_reviews`: v2 array of role review records for the final run explainer. Full-profile development terminal `done` requires completed approved `vadi` and `prativadi` entries whose `artifact_ref` exactly equals `run_explainer_ref`, with non-empty `summary` and `evidence_refs`.
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
- `amendment_from_phase` (F7): additive nullable number. When non-null it records
  the numeric phase an in-progress plan-amendment loop returns to. It may become
  non-null only on an amendment entry edge (full `deslop -> spec_revision`,
  standard `phase_review -> spec_revision`), is unchangeable mid-loop, and MUST be
  nulled on exit (`dvandva write` reason `bad_amendment`; the `total_phases` freeze
  uses `bad_amendment total_phases_frozen`). While non-null the spec loop is legal
  post-lock and `total_phases`/`phase_profiles` may change. Absent = null; not a
  required key.
- `phase_profiles` (F9): additive nullable object `{"<numeric phase>": "standard" |
  "full"}`. Effective profile of numeric phase N = `phase_profiles[N]` // run
  profile; non-numeric phases and terminal-gate selection follow the run profile.
  Set or changed only in spec states (`spec_drafting`/`spec_revision`, including the
  F7 amendment loop) and never below the per-phase hard-path floor (`dvandva write`
  reason `bad_phase_profiles`). Absent = null; not a required key.
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
- `dvandva wait`: continuous polling is the hard rule. `--max-wait` is the
  heartbeat interval by default, and the helper keeps polling until role
  ownership, shared terminal `done`, the terminal `abandoned` (S2-T1; wait exits
  13), `human_question`, `human_decision`, or user interrupt. `human_question` and `human_decision` are a paired run pause that
  stops both roles together. During a selected non-terminal wait, the wait
  helper propagates a newer sibling run's `human_decision` or
  `human_question` unless `DVANDVA_CONCURRENT=1`; older sibling
  human-intervention batons are ignored so parked runs cannot hijack newer work.
  For a sibling `human_question`, output must preserve the sibling baton's
  `question`, `resume_assignee`, and `resume_status`.
  After installing a handoff checkpoint, the writer must run the helper with
  `--since-checkpoint <written_checkpoint> --until-actionable`; while the
  selected baton remains at or below that checkpoint, the helper keeps polling
  even if a team-owned `active_roles` field names the writer. `--until-actionable`
  also keeps a role asleep in team-owned states such as `parallel_implementing`
  until that role owns a ready, dependency-unblocked chunk; it does not suppress
  shared active states such as `termination_review`. It exits 0 only after the
  baton advances and the role has actionable work, so the role re-reads before
  deciding whether to work, wait again, or stop together.
  `termination_review` is not terminal; it wakes both roles.
  `--persist` is accepted for older snippets and is redundant. Optional
  `--persist-max <seconds>` is a total wall-clock cap; the wait-helper persist cap exit 23 is not a terminal baton state and must re-enter wait unless the
  user interrupts. Explicit `--finite` compatibility mode is the only path to
  exit 20 and is not valid for normal walkaway loops.
- `dvandva wait --through-human`: keeps polling THROUGH a `human_question`/
  `human_decision` pause (this role's own, or a newer paired sibling's)
  instead of exiting 11/12. Each pause episode (keyed by status and
  checkpoint, plus sibling run id for a propagated pause) prints one line to
  stderr — `DVANDVA_WAIT note human_pause status=<status>
  checkpoint=<checkpoint>` for an own pause, with ` sibling_run_id=<run_id>`
  appended for a sibling pause — and fires the same notify event the pre-flag
  exit-11/12 path used, exactly once per episode; a `.wait-pause-<role>`
  marker file next to the baton dedupes the note/notify across a wait
  re-invocation after a persist-max/shell-budget exit. The stall watchdog is
  suspended for the duration of the pause and resumes counting from a fresh
  anchor the moment the pause clears; continuous-mode `--max-wait` heartbeats
  continue uninterrupted. `done` and `abandoned` still exit immediately;
  split-brain detection is unaffected. Per F5, the Claude Code-hosted session
  (which owns surfacing `human_question`/`human_decision`) must never pass
  `--through-human`; only a non-surfacing session (Codex-hosted in a mixed
  pair, or the non-writer session in an all-Codex pair) uses it.
- `dvandva write`: the write-helper validation exit 23 means a baton
  candidate failed schema, required-key, safe-run-id, status-owner, status, or
  enum validation. Fix the candidate and rerun the helper; do not edit the
  installed baton directly.

Legacy `.dvandva/baton.json` may continue to be READ as `dvandva.baton.v1`, but
the v1 WRITE path is retired (S5-T2/D5): a `dvandva.baton.v1` write candidate —
or a current baton still carrying `schema: "dvandva.baton.v1"` — is rejected with
`schema_retired` and a migration hint to `dvandva.baton.v2`. The lenient READ
path (`state`/`resolve`/`wait`/`brief`) is untouched, so old v1 batons stay
observable; `baton-schema.json` and `templates/channel/baton.json` are kept as
HISTORICAL v1 references. The live
v2 write-helper enforcement requires safe `run_id`, `original_ask`,
`run_explainer_ref`, `active_roles`, `work_split`, `agent_instances`,
`subagent_tracks`, and `verification_matrix`; it also requires a non-empty
`research_ref` before advancing beyond the initial research draft, except that
`human_question` and `human_decision` remain legal early-escalation targets
before `research_ref` exists. Existing batons cannot change schema or v2
`run_id` mid-run. Accepted v2 docs use mode/profile-conditional terminal
artifact gates: full-profile development runs require `run_explainer_ref` plus
completed approved `run_explainer_reviews` from both roles for the exact
artifact; fast and standard development runs require `profile_decision`, passing
final verification, completed `verification_matrix` evidence, and completed
approved prativadi `phase-review` evidence but no run explainer; research runs require
`research_ref` and additionally `plan_ref` iff `research_outcome ==
seed_development`; review runs require `review_ref`. In all v2 modes, terminal
`done` still requires a coordinator assignee (`human`, `team`, `vadi`, or
`prativadi`), both final approvals, and an installed current baton already at
`termination_review`.

Full-profile implementation-phase parallelism is mandatory for v2. Spec approval enters `parallel_implementing` with `assignee: "team"` and `active_roles: ["vadi", "prativadi"]`; `work_split` must contain at least five implementation chunks split across both roles for two-team parallel implementation, each with reciprocal `cross_review_by`. After `test_creation`, the baton enters `cross_review`; `cross_review` may route to `cross_fixing`, and only completed cross-review evidence for both roles can advance to `deep_review`. Fast and standard development profiles use the compact `implementing -> phase_review -> termination_review -> done` path, still with `profile_decision`, passing final verification, completed `verification_matrix` evidence, a completed approved prativadi `phase-review` subagent track, shared termination, and role-owned final approvals. Phase convention: implementation-chunk tracks use the numeric implementation phase, while cross-review, phase-review, and deep-review gate tracks use the status-name phase such as `phase: "cross_review"`, `phase: "phase_review"`, or `phase: "deep_review"`.

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
gate `dvandva preflight --role <role>`. The preflight resolves the active run
selector-first (stopping on exit 12 ASK), then runs the hook stage in-process.
The hook stage detects Dvandva hook adoption status via a functional probe: it
materializes delegating hooks under `.dvandva/githooks/` (gitignored) — each is
a symlink to the `dvandva` binary, which dispatches on the invoking hook name in
`argv[0]` — and sets only repo-local `git config core.hooksPath .dvandva/githooks`.
When a prior `core.hooksPath` exists (e.g., set by Husky), the installer records
it as `dvandva.priorHooksPath`, the delegating hooks exec the prior hook chain on
every commit so the foreign owner keeps firing, and the prior `core.hooksPath` is
restored on uninstall — the foreign owner is preserved through record, delegate,
and restore. The installer records `dvandva.hooksAdoptedAt` as the local drift
baseline. The `pre-commit` symlink runs the in-binary commit gate then execs the
prior hook; the `prepare-commit-msg` symlink delegates first then stamps
`Dvandva-Checkpoint`. `dvandva drift-lint` detects unstamped commits from the
hook-adoption baseline floor, so a later checkpoint cannot hide an unstamped
sandwich bypass. This is local git-hook enforcement only; it does not create a
daemon, scheduler, or hidden central process.
For git work-gating, terminal `done`, `human_question`, and `human_decision`
batons are inactive: the commit gate allows them, and drift lint only reports
off-protocol commits while at least one non-terminal baton is active or when
checkpoint history gives it a scan floor.

## Concurrent advance and stale writes

Two roles can advance the baton concurrently in team-owned states. The following
exit codes guard against consistency failures.

Exit codes 2, 23, 27, 28, and 29 (`lock_lost`) are `dvandva write` codes.
Exit code 29 (`split_brain`) is a `dvandva wait` code. The two exit-29 codes
are distinct and must not be confused: `lock_lost` fires when a write-time
fencing-token mismatch aborts an install; `split_brain` fires when the wait
helper detects two concurrent active runs that both claim the same role.

| Exit code | Name | Meaning and recovery |
|---|---|---|
| `2` | `bad_lock_timeout` | `dvandva write`: `DVANDVA_LOCK_TIMEOUT` is not a canonical positive decimal (`^[1-9][0-9]*$`). Zero, negative, leading-zero forms (`08`, `09`), and non-numeric values all fail closed before the lock loop fires. Fix or unset `DVANDVA_LOCK_TIMEOUT` (default is `30` seconds), then rerun. |
| `23` | `bad_run_id_dir` | `dvandva write`: the baton at `.dvandva/runs/<seg>/baton.json` has `schema==v2` but its `run_id` field is null, missing, empty, or mismatches `<seg>`. Fix the candidate `run_id` and rerun; do not edit the installed baton directly. |
| `27` | `stale_checkpoint` | `dvandva write`: emits `DVANDVA_WRITE stale_checkpoint current=<c> candidate=<n>` when `new_checkpoint <= cur_checkpoint` — the peer advanced while this candidate was being prepared. Re-read the installed baton, re-derive the next state from the mode table, rewrite the candidate, and rerun the helper. Never bump past the peer's checkpoint. |
| `28` | `lock_unavailable` | `dvandva write`: a non-directory (e.g. a leftover file or a squatter) occupies the lock path `<baton-dir>/.baton.lock.d`; the critical section never runs unlocked. Fail closed. Investigate and remove the squatter, then rerun. |
| `29` | `lock_lost` (`dvandva write`) | `dvandva write`: the fencing token was overwritten by a peer that age-stole the lock while this writer was slow. The install is aborted fail-closed; the baton is unchanged. Re-read the baton and re-derive the next state before retrying. **Distinct from** `dvandva wait` exit `29` (`split_brain`). |
| `29` | `split_brain` (`dvandva wait`) | `dvandva wait`: a sibling active baton has `assignee == my role` while the selected baton is waiting on a peer — two simultaneous active runs both think they own the same role. Reconcile selector choice; park the stale duplicate to `human_decision`. Suppress with `DVANDVA_CONCURRENT=1` only when explicitly running two independent parallel Dvandva loops in the same worktree. **Distinct from** `dvandva write` exit `29` (`lock_lost`). |

### Test-only seam: `DVANDVA_WRITE_BARRIER`

`dvandva write` contains a synchronization seam gated on the `DVANDVA_WRITE_BARRIER`
environment variable. When the variable is unset (all production runs), the
branch is never entered — it is a no-op, a single string test. When set by a
test harness, the write port touches and stats sentinel files named
`${DVANDVA_WRITE_BARRIER}.arrived` and `${DVANDVA_WRITE_BARRIER}.release` in
order to let the write port's `cargo test` suite park a writer after the
transition check but before the atomic install, exercising the fencing-token and
stale-checkpoint guarantees deterministically. The seam never reads or executes
arbitrary input. This is an accepted test-only seam; no behavior change is required.

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
  "profile": "development-only lifecycle depth: fast | standard | full; missing existing development profiles are effective full",
  "profile_floor": "minimum allowed profile computed from risk inputs; downgrades below this require human_decision",
  "profile_decision": "object recording selected_profile, floor, reason, decided_by, decided_at, risk_inputs, hard_triggers, allowlist_match, allowlist_refs, and evidence_refs",
  "profile_history": "append-only array of profile change records: from, to, floor, checkpoint, actor_role, reason, evidence_refs",
  "run_mode": "walkaway | supervised",
  "phase": "research | spec | review | 1 | 2 | ... | done",
  "total_phases": "integer, set during spec phase; engine-frozen once master_plan_locked is true (write reason bad_amendment total_phases_frozen), changeable only inside an F7 amendment loop (amendment_from_phase non-null) or on a write into human_decision",
  "phase_profiles": "F9 additive nullable object {\"<numeric phase>\": \"standard\" | \"full\"}; effective profile of numeric phase N = phase_profiles[N] // run profile; set/changed only in spec states and never below the per-phase hard-path floor (write reason bad_phase_profiles); absent = null",
  "status": "research_drafting | research_review | research_revision | spec_drafting | spec_review | spec_revision | human_question | implementing | parallel_implementing | test_creation | cross_review | cross_fixing | deep_review | deslop | termination_review | phase_review | phase_fixing | review_of_review | counter_review | human_decision | done | abandoned (22-token v2 catalog; see the Status catalog line near the top; abandoned is the S2-T1 terminal enterable only from human_question/human_decision)",
  "assignee": "non-empty string; v1 conventions are vadi | prativadi | human; v2 status-owner pairs include team for concurrent states",
  "active_roles": "v2 concurrent roles array, usually [] or [\"vadi\", \"prativadi\"]",
  "current_engine": "optional; claude | codex | null. Records which CLI wrote the most recent baton; traceability only.",
  "review_target": "research | spec | implementation | prativadi_fixups | vadi_counter | null",
  "research_ref": "v2 path to gitignored generated HTML research file under ./superpowers/research/, set during research phase",
  "run_explainer_ref": "v2 path to gitignored final run explainer HTML under ./superpowers/run-reports/, required before terminal done for full-profile development mode",
  "run_explainer_reviews": "v2 array of role review records for the final run explainer; full-profile development done requires completed approved vadi and prativadi entries whose artifact_ref exactly equals run_explainer_ref, with non-empty summary and evidence_refs",
  "research_outcome": "nullable v2 field; accepted research result, including seed_development when a research run seeds a future development run",
  "review_ref": "nullable v2 path to gitignored generated HTML review artifact under ./superpowers/reviews/",
  "review_intake": "nullable v2 field carrying review-mode intake scope or selector",
  "plan_ref": "path to gitignored generated HTML plan file under ./superpowers/plans/, set during spec phase",
  "work_split": "v2 array/object describing planned ownership by phase, owner, scope, paths, read_paths, write_paths, conflict_group, depends_on, status, and artifact refs",
  "agent_instances": "v2 array recording generated run-scoped agent instances, provenance, model/permission class, read/write paths, work item IDs, base checkpoint, output refs, evidence refs, lifecycle status, and closure result",
  "subagent_tracks": "v2 array recording actual conditional parallelism tracks, owner, evidence refs, fallback rationale, and result",
  "verification_matrix": "v2 array/object mapping claims and risks to planned checks, owners, expected evidence, result, and evidence_ref",
  "master_plan_locked": "boolean; false during planning, true once prativadi advances to phase 1",
  "amendment_from_phase": "F7 additive nullable number; numeric phase an in-progress plan-amendment loop returns to. May become non-null only on an amendment entry edge, is unchangeable mid-loop, and MUST be nulled on exit (write reason bad_amendment); absent = null",
  "question": "string | null; one concrete user question when status is human_question",
  "resume_assignee": "vadi | prativadi | null; role to resume after a human_question answer",
  "resume_status": "spec_drafting | spec_review | spec_revision | null; status to restore after a human_question answer",
  "disagreement_round": "integer, reset to 0 by the agent that writes the first baton of each new phase",
  "disagreement_cap": "integer, default 3, optionally set during spec phase",
  "work_split_waiver": "S5-T3 additive nullable object gating the parallel/test-creation chunk floor: {reason: <non-blank string>, approved_by: \"prativadi\", checkpoint: <number>}. When valid it waives ONLY the >=5-total chunk floor; the per-role >=2 write-capable-chunk floor is never waivable. Any other present shape is rejected (write reason bad_work_split_waiver). Absent = the >=5 floor is in force.",
  "loop_counts": "v2 additive map keyed \"<kind>:<phase>\" to an integer per-cycle counter for repeated review/fix loops; the write helper mandates increment-by-one on every loop-edge write (grandfathering only the read of an absent counter to 0, so the cap cannot be bypassed by omitting loop_counts) and, at disagreement_cap, allows only a human_decision target. Absent counters read as 0; the counter resets on phase advance.",
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

`review_checkpoint` is a sub-field on a compact-profile phase-review
`subagent_tracks[]` entry (`phase: "phase_review"`, `track: "phase-review"`),
recording the checkpoint of the phase-review cycle it evidences. Two gates read
it: (1) the compact `phase_review -> termination_review` and
`termination_review -> done` gates require a completed approved prativadi
phase-review track whose `review_checkpoint` matches the current cycle; and (2)
the S4-T6 done gate reads `review_checkpoint` (coalesced with
`evidence_checkpoint`) as the numeric freshness anchor for `verification_matrix`
rows, so a row is stale unless its checkpoint is at or after the last
implementation-family checkpoint. The `agent_instances_example` entry in
`baton-schema-v2.json` is illustrative only — it is a documentation sample, not a
required field, and the write helper never reads it.

## Development Profiles

Development mode chooses one of three lifecycle profiles through `profile`.
This selector is independent from `mode`.

### Development Full Profile (v2, 28 edges)

This is the exhaustive v2 development table. Every route into
`termination_review` is explicit; there is no wildcard `* -> termination_review`.

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
| `deep_review` | `review_of_review, review_target: prativadi_fixups` | Prativadi applied narrow fixups after the required review angles; vadi must inspect the fixup diff before cleanup continues. F6: a full-effective-profile phase also needs a completed `security` angle (`dvandva-security-auditor`) when changed_paths ∪ current-phase work_split paths hit the `.env*`/secret/credential or api/client submatchers, and an `integration` angle (`dvandva-integration-checker`) when ≥2 distinct-owner write-capable chunks share a cross-owner `depends_on` or `conflict_group` (write reason `bad_deep_review_angles`). |
| `deep_review` | `deslop` | Prativadi accepts behavior and tests after the required review angles are complete, including the F6 `security`/`integration` angles when their triggers fire (write reason `bad_deep_review_angles`). |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | Vadi disapproves prativadi fixups and writes a counter-change. |
| `review_of_review (prativadi_fixups)` | `deslop` | Vadi approves prativadi fixups; v2 returns to cleanup rather than advancing or terminating directly. |
| `counter_review (vadi_counter)` | `review_of_review, review_target: prativadi_fixups` | Prativadi disapproves the counter and applies a different narrow fixup. |
| `counter_review (vadi_counter)` | `deslop` | Prativadi approves the counter; v2 returns to cleanup rather than advancing or terminating directly. |
| `phase_fixing` | `test_creation` | Vadi fixed behavior, tests, or verification gaps and must refresh test evidence. |
| `deslop` | `phase_fixing` | Cleanup finds behavior, test, or review blockers. |
| `deslop` | `phase: N+1, status: "parallel_implementing"` | A non-final phase is clean and the next development phase (effective profile full) begins. |
| `deslop` | `phase: N+1, status: "implementing"` | F9 cross-profile advance: a clean full phase advances into a next phase whose effective profile is `standard` (entry state chosen by the target phase's effective profile). |
| `deslop` | `phase: "spec", status: "spec_revision"` | F7 amendment entry: the vadi opens a capped plan-amendment episode for a post-lock scope change — sets `amendment_from_phase` to the current numeric phase, lands `phase: "spec"`, assignee `vadi`, and increments loop key `plan_amendment:<from-phase>` (cap = `disagreement_cap`; at cap only `human_decision` is legal; exit nulls the field, resets `loop_counts`, and re-enters at a numeric phase ≥ `amendment_from_phase`). |
| `deslop` | `termination_review` | The final development phase is clean; the run enters the shared stop-review gate (selected by the final phase's effective profile). |
| `termination_review` | `phase_fixing` | One role rejects final stop because behavior, tests, docs, or run artifacts still need work. |
| `termination_review` | final `done` | Both roles explicitly decide to stop, both approval bits are true, `run_explainer_ref` is set, `run_explainer_reviews` contains completed approved entries from both roles for that exact artifact, and (F10) a completed current-cycle `explainer-verification` track owned by `dvandva-doc-verifier` exists (write reason `bad_explainer_verification`). |

### Development Standard Profile (v2 compact edges)

Standard is the default for new development scaffolds that do not touch hard-risk
coordination surfaces. It keeps research/spec planning, then uses a compact
implementation and review path.

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Vadi writes `research_ref`, `profile_decision`, work split, and verification matrix. |
| `research_review` | `research_revision` | Prativadi finds research/profile gaps. |
| `research_revision` | `research_review` | Vadi addresses research/profile gaps. |
| `research_review` | `phase: "spec", status: "spec_drafting"` | Research is accepted and the compact development plan can be drafted. |
| `spec_drafting` | `spec_review` | Vadi hands the plan to prativadi for Q&A. |
| `spec_review` | `spec_revision` | Prativadi surfaces planning Q&A. |
| `spec_revision` | `spec_review` | Vadi answers Q&A and re-hands the plan. |
| `spec_review` | `phase: 1, status: "implementing"` | Prativadi accepts the plan and starts compact implementation. |
| `implementing` | `phase_review` | Vadi records implementation and verification evidence, including a profile-floor recheck. |
| `phase_review` | `phase_fixing` | Prativadi finds substantive issues. |
| `phase_review` | `implementing` | Prativadi requests additional implementation or verification without leaving compact profile. |
| `phase_review` | `review_of_review, review_target: prativadi_fixups` | S5-T1/D4: standard gains the same rarely-exercised mutual-review safety valve `full` has — prativadi applied narrow fixups and mutual review is owed (requires non-empty `narrow_fixups`). Loop-capped (`review_of_review<->counter_review`). |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | S5-T1: vadi disapproves the fixups and writes a counter-change. |
| `review_of_review (prativadi_fixups)` | `phase_review` | S5-T1: vadi approves the fixups; standard returns to compact phase review rather than advancing directly. |
| `counter_review (vadi_counter)` | `review_of_review, review_target: prativadi_fixups` | S5-T1: prativadi disapproves the counter and applies a different narrow fixup. |
| `counter_review (vadi_counter)` | `phase_review` | S5-T1: prativadi approves the counter; standard returns to compact phase review. |
| `phase_review` | `phase: N+1, status: "parallel_implementing"` | F9 cross-profile advance: a clean standard phase advances into a next phase whose effective profile is `full` (entry state chosen by the target phase's effective profile). |
| `phase_review` | `phase: "spec", status: "spec_revision"` | F7 amendment entry (standard equivalent): opens a capped plan-amendment episode for a post-lock scope change — sets `amendment_from_phase` to the current numeric phase, lands `phase: "spec"`, assignee `vadi`, increments loop key `plan_amendment:<from-phase>` (cap = `disagreement_cap`); exit via `spec_review -> implementing` nulls the field, resets `loop_counts`, and re-enters at a numeric phase ≥ `amendment_from_phase`. |
| `phase_fixing` | `phase_review` | Vadi fixes and refreshes evidence. |
| `phase_review` | `termination_review` | Prativadi approves compact implementation/review evidence. |
| `termination_review` | `phase_fixing` | Either role rejects final stop. |
| `termination_review` | final `done` | Both approvals are true, `profile_decision` is valid, final verification is passing, `verification_matrix` evidence is complete, and a completed approved prativadi `phase-review` subagent track exists. No run explainer or F10 explainer-verification track is required for standard. |

### Development Fast Profile (v2 allowlist edges)

Fast is valid only for allowlisted prose-only changes with positive allowlist
evidence, `profile_floor: "fast"`, and no hard-risk paths.

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Optional fast research prelude records `research_ref`, `profile_decision`, allowlist evidence, work split, and verification matrix before compact implementation. |
| `research_review` | `research_revision` | Prativadi requests a research/evidence correction before fast implementation. |
| `research_revision` | `research_review` | Vadi refreshes the fast research package and returns to prativadi. |
| `research_review` | `implementing` | Prativadi accepts the allowlisted fast research/evidence package; fast skips spec planning and enters compact implementation. |
| `implementing` | `phase_review` | Vadi records verification evidence and profile recheck for allowlisted paths. |
| `phase_review` | `phase_fixing` | Prativadi finds issues. |
| `phase_fixing` | `phase_review` | Vadi fixes and refreshes evidence. |
| `phase_review` | `termination_review` | Prativadi approves the compact review and the profile still satisfies the floor. |
| `termination_review` | `phase_fixing` | Either role rejects final stop. |
| `termination_review` | final `done` | Both approvals are true, `profile_decision` is valid, final verification is passing, `verification_matrix` evidence is complete, and a completed approved prativadi `phase-review` subagent track exists. No run explainer is required for fast. |

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

## Review Mode (v2, 10 edges)

Review mode reuses the existing research statuses for intake and the existing
review statuses for the review package. Set `review_intake` and `review_target`
before the first `deep_review`, and produce `review_ref` before shared
termination. Every review-mode status uses `phase: "review"`. S4-T7 adds the
`deep_review -> phase_fixing` hand-back edge (loop-capped), taking the table from
9 to 10 edges.

| From | To | Trigger |
|---|---|---|
| `research_drafting` | `research_review` | Vadi writes review scope/intake research and hands to prativadi. |
| `research_review` | `research_revision` | Prativadi finds intake gaps and routes back to vadi. |
| `research_revision` | `research_review` | Vadi fixes intake gaps and hands back. |
| `research_review` | `deep_review` | Intake is sufficient and the review package can be evaluated. |
| `deep_review` | `phase_fixing` | S4-T7: the reviewer hands back substantive fixes (bugs, missing tests, verification gaps) without lapping the stop gate. Loop-capped (`deep_review->phase_fixing`). |
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
| planning state (`research_*`/`spec_*`) with `master_plan_locked: false`, OR a post-lock working state (`implementing`/`parallel_implementing`/`test_creation`/`cross_fixing`/`phase_fixing`) | `human_question` | S4-T5/D1: route one concrete requirement question to the human instead of guessing. `resume_status`/`resume_assignee` restore the exact prior state. Not a loop edge. |
| `human_question` | `resume_status` with `assignee: resume_assignee` | Human answers and the receiving skill clears question fields. |
| `human_question` | `abandoned` (`assignee: human`, `active_roles: []`) | S2-T1: the run is abandoned. Terminal, snapshot-archived like `done`, no artifact/approval/loop gates; `dvandva wait` exits 13. |
| `human_decision` | `abandoned` (`assignee: human`, `active_roles: []`) | S2-T1: the run is abandoned. Terminal. |
| `abandoned` | — (no outgoing edge) | S2-T1: terminal; reopen only via a hand-authored `human_decision` write. The resolver/commit-gate/drift inactive set is `{done, abandoned}`. |
| any state | `human_decision` | Escalation, cap hit, blocker, malformed input, or unsafe dirty tree. |
| `human_decision` | any mode-owned state chosen by the human | Human edits baton or prompts an agent with a decision. |

**Hardening gates layered onto these edges (S4/S5):**

- **Spec-entry lock (S4-T2/D2).** `spec_review -> implementing`/`parallel_implementing` requires candidate `master_plan_locked == true` (`bad_master_plan_locked`); `master_plan_locked` `true->false` is rejected on every development edge except a write whose `new_status` is `human_decision`.
- **Done-gate refs/matrix/superset (S4-T1/T4/T6).** A `done` candidate must resolve each required ref to an existing non-empty file (`missing_artifact`), carry a complete-and-fresh `verification_matrix` (`stale_verification_matrix`, anchored at the last implementation-family checkpoint), and (team-owned) preserve installed `subagent_tracks`/`agent_instances`/`work_split` ids and `findings` as a superset (`lost_update`).
- **v1 write retirement (S5-T2/D5).** A `dvandva.baton.v1` write candidate — or a current baton still on v1 — is rejected with `schema_retired` and a migration hint. The READ path (`state`/`resolve`/`wait`/`brief`) stays lenient, so old v1 batons remain observable.
- **Parallel-chunk floor + waiver (S5-T3).** `parallel_implementing` entry and `parallel_implementing -> test_creation` require `>=2` write-capable chunks per role AND (`>=5` total OR a valid `work_split_waiver`); malformed waivers are `bad_work_split_waiver`.
- **Post-install fence (S4-T10).** After the atomic rename the write path re-verifies the lock; loss is `lock_lost_post_install` (exit 29) — the install DID happen and may be superseded, so the caller must re-read.

## Liveness and loop gates (Slice 1)

These gates keep the walkaway loop live and bound its cycles.

- **Advance-owner wake.** In `parallel_implementing` and `cross_fixing`, once no
  implementation chunk is unblocked and non-terminal for either role (all
  terminal or blocked), `dvandva wait --until-actionable` wakes the state's
  advance-owner (`vadi`) so the outbound transition is written instead of both
  roles sleeping forever. Implementation chunks are identified by the same
  definition as the `>=5` parallel-implementing gate (implementation
  `chunk_type`, reciprocal `cross_review_by`, non-empty `paths`); lifecycle gate
  chunks (`test_creation`, `deep_review`, `deslop`) are excluded. A `depends_on`
  id naming a work_split chunk must be terminal to unblock; any other id is an
  anchor (e.g. `spec-approved`), satisfied unless it equals the current status.
- **Loop caps.** `loop_counts["<kind>:<phase>"]` counts repeated review/fix
  cycles (`deep_review->phase_fixing`, `cross_review->cross_fixing`,
  `termination_review->phase_fixing`, `phase_review->phase_fixing`,
  `review_of_review<->counter_review`). The write helper mandates
  increment-by-one on every loop-edge write, even when `loop_counts` is absent
  for that edge (`bad_loop_counts` otherwise), so the cap cannot be bypassed by
  omitting the counter; once a counter reaches `disagreement_cap`, only a
  `human_decision` target is allowed (`loop_cap` otherwise). Absent counters
  read as 0 and reset on phase advance.
- **depends_on validation.** `work_split[].depends_on` ids must resolve to a
  chunk id or the fixed anchor set, and the chunk-id dependency graph must be
  acyclic (`bad_depends_on` otherwise).
- **Approval hygiene.** A `*_final_approval` may be raised only when the
  candidate status is `termination_review` (`approval_out_of_band` otherwise),
  and both approvals reset to false on `termination_review->phase_fixing`
  (`stale_approval` otherwise), so a stale pre-fix approval cannot satisfy the
  done gate.
- **Dead-peer watchdog.** `dvandva wait --stall-max <seconds>` exits 24
  (`stalled`) when the baton does not advance within the bound; the role then
  writes `human_decision`. Wait exit 24 is distinct from `dvandva write`
  exit 24 (illegal transition).
- **Out-of-band liveness monitor.** `dvandva watchdog [<root>...]` is a
  separate one-shot subcommand run from cron/systemd, not from inside a
  session — it covers the case the in-protocol dead-peer watchdog above
  cannot: both roles' sessions dying at once (VPS reboot, OOM sweep, network
  loss), leaving nothing alive to write `human_decision`. It scans every
  baton under the given roots and, for each stale (`--stale-max`, default
  1800s) or reminder-due (`--remind-paused`) baton, prints `DVANDVA_WATCHDOG
  <event> run_id=<id> status=<s> assignee=<a> checkpoint=<n> age_s=<n>
  root=<path>` (`<event>` is `watchdog_stale` or `watchdog_paused`), deduped
  per baton via a marker file keyed on status/checkpoint/age-bucket (1x/4x/24x
  the threshold, then silent), plus a `DVANDVA_WATCHDOG summary roots=<n>
  batons=<n> stale=<n> paused=<n> skipped=<n>` line at the end. Always exits
  0 — it is a monitor, not a gate.

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
- Human-owned: `human_question`, `human_decision`, and the terminal `abandoned`
  (S2-T1: `assignee: "human"`, `active_roles: []`, reachable only from
  `human_question`/`human_decision`).
- Terminal statuses are `done` and `abandoned`. `abandoned` (S2-T1) is a human
  bailout — terminal with no gates and no outgoing edge, snapshot-archived like
  `done`, and `dvandva wait` exits 13 on it.
- Terminal `done` has no status owner. It is accepted as terminal for a
  coordinator assignee (`human`, `team`, `vadi`, or `prativadi`) only from
  `termination_review` in any v2 mode. The final gate still requires both
  final approvals plus the mode/profile-conditional terminal artifact or
  evidence: `run_explainer_ref` plus both roles' completed approved
  `run_explainer_reviews` for that exact artifact in development/full,
  `profile_decision`, passing final verification, completed
  `verification_matrix` evidence, and a completed approved prativadi
  `phase-review` subagent track in development/fast and development/standard,
  `research_ref` plus `plan_ref` iff
  `research_outcome == seed_development` for research, and `review_ref` for
  review. Wait helpers stop on `done`.
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
