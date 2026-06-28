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
- `run_explainer_ref`: path to the final run explainer HTML under `./superpowers/run-reports/`, required before terminal `done`.
- `active_roles`: v2 concurrent role list. Team-owned statuses use `assignee: "team"` and `active_roles: ["vadi", "prativadi"]`; scalar statuses use an empty array.
- `work_split`: planned ownership map for vadi, prativadi, human, or subagents; records phase, owner, scope, paths, read_paths, write_paths, status, artifact refs, parallelism rationale, dependencies, and optional conflict_group serialization.
- `agent_instances`: first-class registry for generated run-scoped agent instances, including provenance, model/permission class, read/write paths, work item IDs, base checkpoint, output refs, evidence refs, lifecycle status, and closure result. Generated instance IDs must not collide with coordinator owners (`vadi`, `prativadi`, `team`, `human`), seed-roster owners such as `dvandva-implementer`, or legacy standalone owner names such as `adversarial-analyst`. `spawned_by` is executable provenance; `seed_agent` is advisory metadata for humans and brief generation. Dynamic write-path disjointness is checked among generated instances sharing the same `base_checkpoint` and among any two live (`planned`/`running`) instances regardless of base_checkpoint; closed historical instances from earlier base checkpoints do not block later sequential path reuse.
- `subagent_tracks`: actual conditional parallelism record. Parallelize only genuinely disjoint tracks; record what was not parallelized and why when direct execution is safer or when subagent tooling is unavailable.
- `verification_matrix`: planned evidence map from claims and risks to checks, owners, expected results, command or inspection, result, evidence refs, and the 100% test coverage target for new behavior.
- `turn_cap`: default `60`; passive shell wait heartbeats do not count as
  active model-work turns.
- `dvandva-wait.sh`: continuous polling is the hard rule. `--max-wait` is the
  heartbeat interval by default, and the helper keeps polling until role
  ownership, `done`, `human_question`, `human_decision`, or user interrupt.
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
`run_explainer_ref`, `active_roles`, `work_split`, `agent_instances`, `subagent_tracks`, and `verification_matrix`; it also requires a
non-empty `research_ref` before advancing beyond the initial research draft,
except that `human_question` and `human_decision` remain legal early-escalation
targets before `research_ref` exists. Existing batons cannot change schema or v2
`run_id` mid-run. Terminal `done` requires a coordinator assignee (`human`, `team`, `vadi`, or `prativadi`), `vadi_final_approval == true`, `prativadi_final_approval == true`, and `run_explainer_ref` pointing to `./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer.html`.

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

Run 4 work-gating uses repo-local git hooks activated by role preflight:
the active skill exports and asserts `DVANDVA_ROLE=<role>`,
`scripts/install-dvandva-hooks.sh` sets and verifies `core.hooksPath=.githooks`,
and the installer records `dvandva.hooksAdoptedAt` as the local drift baseline.
`.githooks/pre-commit` delegates to `scripts/dvandva-commit-gate.sh`, the gate
checks `DVANDVA_ROLE` against baton `assignee` / `active_roles`,
`.githooks/prepare-commit-msg` stamps `Dvandva-Checkpoint`, and
`scripts/dvandva-drift-lint.sh` detects unstamped commits from the
hook-adoption baseline floor when present, so a later checkpoint cannot hide an
unstamped sandwich bypass. This is shell/git-hook enforcement only; it does not
create a daemon, scheduler, or hidden central process.

Run 4 retirement is Dvandva-only. It may retire only Dvandva-covered standalone
Claude symlink workflows with functional parity via Runs 1-4 usage:
`adversarial-analyst`, `architect`, `developer`, `quality-reviewer`, and
`sandbox-executor`. Codex agent-axis retirement is a no-op, skills are out of
scope, and backup manifest restore keeps the action reversible.

Team-owned v2 states (`parallel_implementing`, `cross_review`, `cross_fixing`) may write same-status sync checkpoints when both roles remain active. Use them to record partial completion, task distribution, or peer wait state without advancing the lifecycle early. Scalar-owner states still reject same-status rewrites.

## Schema Fields

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": "ISO-8601 UTC timestamp, set by the agent that last wrote the baton",
  "mode": "feature-pr | campaign",
  "run_mode": "walkaway | supervised",
  "phase": "spec | 1 | 2 | ... | done",
  "total_phases": "integer, set during spec phase, immutable thereafter unless human edits",
  "status": "spec_drafting | spec_review | spec_revision | human_question | implementing | parallel_implementing | test_creation | cross_review | cross_fixing | deep_review | deslop | phase_review | phase_fixing | review_of_review | counter_review | human_decision | done",
  "assignee": "non-empty string; v1 conventions are vadi | prativadi | human; v2 status-owner pairs include team for concurrent states",
  "active_roles": "v2 concurrent roles array, usually [] or [\"vadi\", \"prativadi\"]",
  "current_engine": "optional; claude | codex | null. Records which CLI wrote the most recent baton; traceability only.",
  "review_target": "spec | implementation | prativadi_fixups | vadi_counter | null",
  "research_ref": "v2 path to gitignored generated HTML research file under ./superpowers/research/, set during research phase",
  "run_explainer_ref": "v2 path to gitignored final run explainer HTML under ./superpowers/run-reports/, required before terminal done",
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

## Spec Phase

| From | To | Trigger |
|---|---|---|
| no legacy baton, legacy explicitly selected | `phase: "spec", status: "spec_drafting"` | Vadi first run on the legacy v1 fallback only |
| `spec_drafting` | `spec_review` | Vadi hands plan to prativadi for Q&A |
| `spec_review` | `spec_revision` | Prativadi surfaces Q&A back to vadi |
| `spec_review` | `phase: 1, status: implementing` | Legacy v1 path: prativadi accepts plan and freezes `total_phases` on the baton |
| `spec_review` | `phase: 1, status: parallel_implementing` | v2 path: prativadi accepts plan, freezes `total_phases`, and activates both roles |
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
| `phase: N, implementing` | `phase: N, status: phase_review, review_target: implementation` | Legacy v1 direct review path only |
| `phase: N, parallel_implementing` | `test_creation` | v2: both roles completed implementation chunks and recorded implementation-chunk subagent evidence |
| `test_creation` | `cross_review` | v2: Vadi records tests, 100% test coverage evidence for new behavior, and verification results |
| `cross_review` | `cross_fixing` | v2: one or both reciprocal cross-review tracks found peer-owned chunk defects |
| `cross_fixing` | `test_creation` | v2: cross-review findings were fixed and tests must be refreshed |
| `cross_review` | `deep_review` | v2: both roles recorded completed cross-review tracks for peer-owned chunks |
| `deep_review` | `phase_fixing` | v2: Prativadi finds bugs, missing tests, verification gaps, or substantive review issues |
| `deep_review` | `deslop` | v2: Prativadi accepts behavior and tests after at least three angle-specific reviewers/tracks in `subagent_tracks` (`correctness-regression`, `test-evidence`, `protocol-handoff`), then routes cleanup |
| `phase_fixing` | `test_creation` | v2: Vadi fixed behavior, tests, or verification gaps and must refresh test evidence before review |
| `deslop` | `phase_fixing` | v2: Cleanup finds behavior, test, or review blockers |
| `deslop` | `phase: N+1, status: parallel_implementing, disagreement_round: 0` | v2: no nits, low/minor bugs, stale wording, or unclear instructions remain except explicitly accepted `deferred` items |
| `deslop` | final `done` | v2: final phase passed implementation, test_creation, deep_review, deslop, uses a coordinator assignee (`human`, `team`, `vadi`, or `prativadi`), has both final approvals true, and `run_explainer_ref` points to `./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer.html` |
| `phase: N, implementing` | `human_decision` | Vadi blocked |
| `phase_review (impl)` | `phase_fixing` | Prativadi hands back substantive findings |
| `phase_review (impl)` | `review_of_review, review_target: prativadi_fixups` | Prativadi applied narrow fixups, mutual review owed |
| `phase_review (impl)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1: Prativadi approves, no changes |
| `phase_review (impl)` | final `done` | Legacy v1 final phase approved by both roles; optional commit/push complete; PR creation remains false |
| `phase_review (impl)` | `human_decision` | Prativadi escalates |
| `phase_fixing` | `phase_review (impl)` | Vadi addressed findings, re-hands |
| `phase_fixing` | `human_decision` | Vadi blocked during fix |

## Mutual Review And Disagreement

| From | To | Trigger |
|---|---|---|
| `review_of_review (prativadi_fixups)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1: Vadi approves prativadi fixups |
| `review_of_review (prativadi_fixups)` | final `done` | Legacy v1: final phase fixups approved by both roles; optional commit/push complete |
| `review_of_review (prativadi_fixups)` | `counter_review, review_target: vadi_counter` | Vadi disapproves, writes counter, increments `disagreement_round` |
| `review_of_review (prativadi_fixups)` | `human_decision` | `disagreement_round >= disagreement_cap` |
| `counter_review (vadi_counter)` | `phase: N+1, status: implementing, disagreement_round: 0` | Legacy v1: Prativadi approves counter |
| `counter_review (vadi_counter)` | final `done` | Legacy v1: final phase counter approved by both roles; optional commit/push complete |
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
- Team-owned: `parallel_implementing`, `cross_review`, `cross_fixing`; these
  require `active_roles: ["vadi", "prativadi"]`.
- Prativadi-owned: `research_review`, `spec_review`, `deep_review`,
  `phase_review`, `counter_review`.
- Human-owned: `human_question`, `human_decision`.
- Terminal `done` has no status owner. It is accepted as terminal for a
  coordinator assignee (`human`, `team`, `vadi`, or `prativadi`) while the final
  gate still requires both final approvals and the run explainer; wait helpers
  stop on `done`.

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
