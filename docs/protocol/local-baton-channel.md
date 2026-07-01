# Local Baton Channel

## Problem

PR comments are durable, but slow and noisy. They are good for human-facing summaries, not for high-frequency agent handoff.

The local baton channel gives the vadi and prativadi a shared, file-based coordination contract.

Superpowers is a hard runtime dependency for Dvandva. Dvandva owns baton state, ownership, phase gates, review state, and handoff evidence; Superpowers governs the active-work discipline inside each assigned turn. A role that cannot invoke the relevant Superpowers skills must stop and surface setup instructions instead of advancing the baton as if the run were valid.

## Files

The default local runtime directory is `.dvandva/`, which is gitignored.
Legacy v1 uses `.dvandva/baton.json`. v2 named runs use
`.dvandva/runs/<run_id>/baton.json` so multiple Dvandva runs can coexist in
one git worktree or directory without sharing history, candidate files, or
terminal archives.

Recommended files:

- `.dvandva/baton.json` - legacy v1 current state and next assignee.
- `.dvandva/baton.next.json` - legacy v1 candidate the active agent writes; installed by the bundled `dvandva-write.sh`.
- `.dvandva/runs/<run_id>/baton.json` - v2 run-scoped current state and next assignee.
- `.dvandva/runs/<run_id>/baton.next.json` - v2 run-scoped candidate.
- `.dvandva/runs/<run_id>/history/*.json` - per-run checkpoint snapshots.
- `.dvandva/runs/<run_id>/events.jsonl` - optional append-only event log.

`run_id` must be one safe path segment: letters, numbers, dot, underscore, or
dash; no slash, backslash, or `..`. Once a v2 baton exists, its `run_id` is
immutable for that run. The wait helper rejects unsafe
`DVANDVA_RUN_ID` values before resolving `.dvandva/runs/<run_id>/baton.json`,
and the write helper applies the same check to v2 baton candidates.

`.dvandva/` is machine coordination state. Generated user-facing artifacts
such as research reports, implementation plans, evaluations, reviews, pilot
write-ups, and run reports live under gitignored `./superpowers/**/*.html` as
dark, self-contained HTML and are referenced from baton fields such as
`research_ref`, `plan_ref`, and `run_explainer_ref`. Source/platform Markdown files such as
`SKILL.md`, command files, README/source docs, and prompt templates remain in
their native format.

The shareable templates live in `templates/channel/`.

## Baton Schema (v2)

This shows a v2 run-scoped baton. Accepted public v2 modes are
`development`, `research`, and `review`; older batons may still serialize
`feature-pr` as a legacy alias for `development`. Legacy v1 batons use
`schema: "dvandva.baton.v1"`, omit the v2-only fields `run_id`,
`original_ask`, `research_ref`, `run_explainer_ref`, `run_explainer_reviews`, `active_roles`,
`work_split`, `agent_instances`, `subagent_tracks`, and `verification_matrix`,
and default `turn_cap` to 60. Nullable v2 additions for the accepted run modes
are `research_outcome`, `review_ref`, and `review_intake`; `review_target`
remains the existing selector field. The live v2 write-helper enforcement
covers v2-only fields, safe `run_id` values, schema continuity for existing
runs, v2 status-owner pairs, honest `agent_instances` and `subagent_tracks`,
the terminal `run_explainer_ref` and `run_explainer_reviews` invariants, and v2 lifecycle transitions.

Accepted terminal artifact gates are mode-conditional: development runs require
`run_explainer_ref` plus completed approved `run_explainer_reviews` from both
roles for that exact artifact; research runs require `research_ref` and
additionally `plan_ref` iff `research_outcome == seed_development`; review runs
require `review_ref`. In all three v2 modes, `termination_review` plus both
final approvals are shared and remain the only path to terminal `done`.

implementation-phase parallelism is mandatory for v2. Spec approval enters `parallel_implementing` with `assignee: "team"` and `active_roles: ["vadi", "prativadi"]`; the `work_split` must contain at least five implementation chunks split across both roles for two-team parallel implementation, each with reciprocal `cross_review_by`. `test_creation` routes to `cross_review` and records 100% test coverage evidence for new executable behavior or source-only rationale for docs/skills; `cross_review` may route to `cross_fixing`, and only completed cross-review evidence for both roles can advance to `deep_review`. Phase convention: implementation-chunk tracks use the numeric implementation phase, while cross-review and deep-review gate tracks use the status-name phase such as `phase: "cross_review"` or `phase: "deep_review"`.

Run 4 generalizes the path gate from dynamic `agent_instances` to `work_split`.
The write helper applies `safe_rel_path` to `work_split.paths`,
`work_split.read_paths`, and `work_split.write_paths`. For write-capable chunks
(`implementation`, `cross_fixing`, and `fix`), `write_paths` supplements rather
than narrows `paths`; the effective write set is their union, so
`write_paths: []` cannot mask the backward-compatible `paths` write surface.
`cross_review` is read-only unless explicit `write_paths` are present. Any live
overlap between write-capable chunks is rejected unless both chunks share the
same non-empty `conflict_group` and one chunk's `depends_on` serializes it after
the other. Closed or terminal historical chunks do not block later sequential
reuse because work_split has no `base_checkpoint` wave model.

Run 4 also adds local git work-gating. Role preflight exports and asserts
`DVANDVA_ROLE=<role>`, then invokes the per-role `dvandva-preflight.sh` hook
stage. The hook stage records the prior hook path, installs a delegating wrapper
at `.dvandva/githooks`, verifies repo-local `core.hooksPath=.dvandva/githooks`,
and records `dvandva.hooksAdoptedAt` as the local drift baseline. The
`.dvandva/githooks/pre-commit` wrapper delegates to the prior hook chain and
`scripts/dvandva-commit-gate.sh`; commits during an active baton require
`DVANDVA_ROLE` to match `assignee` or `active_roles`; `.dvandva/githooks/prepare-commit-msg`
stamps `Dvandva-Checkpoint`; and `scripts/dvandva-drift-lint.sh` reports
unstamped commits from the hook-adoption baseline floor when present, so a
later stamped checkpoint cannot hide a `--no-verify` bypass. This is
shell/git-hook enforcement only. There is no daemon or hidden orchestrator.
For git work-gating, completed `done` batons and human-intervention
`human_question` / `human_decision` batons are inactive: the commit gate allows
them, and drift lint only reports off-protocol commits while at least one active
baton is present or when checkpoint history gives it a scan floor.

For waiting, `human_question` and `human_decision` are a paired run pause that
stops both roles together. If a selected run is in a non-terminal wait and a newer
sibling run enters `human_decision` or `human_question`, the wait helper
propagates that sibling human-intervention state to the selected waiter unless
`DVANDVA_CONCURRENT=1`. Older sibling human-intervention batons remain parked
and ignored, and a sibling `human_question` must surface `question`,
`resume_assignee`, and `resume_status` so the human can resume the correct run.

Run 4 standalone-agent retirement is intentionally Dvandva-only: it covers only
Dvandva-covered workflows with functional parity via Runs 1-4 usage. The
allowlist is the five Claude symlink agents `adversarial-analyst`, `architect`,
`developer`, `quality-reviewer`, and `sandbox-executor`. Codex agent-axis files
are reported as no-op, skill directories are not touched, and the helper writes
a backup manifest so `--restore` can reverse an apply run.

Team-owned v2 states (`parallel_implementing`, `cross_review`, `cross_fixing`, `termination_review`) may write same-status sync checkpoints when both roles remain active. Use them to record partial completion, task distribution, peer wait state, or shared stop-review evidence without advancing the lifecycle early. Scalar-owner states still reject same-status rewrites.

```json
{
  "schema": "dvandva.baton.v2",
  "updated_at": "2026-05-13T10:30:00Z",
  "mode": "development",
  "run_mode": "walkaway",
  "run_id": "example-feature",
  "phase": 1,
  "total_phases": 3,
  "status": "parallel_implementing",
  "assignee": "team",
  "active_roles": ["vadi", "prativadi"],
  "agent_instances": [],
  "current_engine": "codex",
  "review_target": "implementation",
  "original_ask": "Implement the example feature with Dvandva review.",
  "research_ref": "./superpowers/research/2026-05-13-example-feature.html",
  "plan_ref": "./superpowers/plans/2026-05-13-example-feature.html",
  "run_explainer_ref": null,
  "run_explainer_reviews": [],
  "research_outcome": null,
  "review_ref": null,
  "review_intake": null,
  "work_split": [
    {
      "id": "implementation-chunk-1",
      "phase": 1,
      "chunk_type": "implementation",
      "owner": "vadi",
      "owner_role": "vadi",
      "scope": "Implement feature scaffolding.",
      "paths": ["src/example.ts"],
      "read_paths": ["src/example.ts"],
      "write_paths": ["src/example.ts"],
      "cross_review_by": "prativadi",
      "can_parallelize": true,
      "parallel_rationale": "Independent implementation chunk in the two-team plan.",
      "depends_on": ["research-codebase"],
      "conflict_group": "",
      "status": "complete",
      "artifact_refs": ["./superpowers/research/2026-05-13-example-feature.html"]
    },
    {
      "id": "cross-review-vadi-chunk",
      "phase": "cross_review",
      "chunk_type": "cross_review",
      "owner": "prativadi",
      "owner_role": "prativadi",
      "scope": "Read-only review of the vadi-owned implementation chunk.",
      "paths": ["src/example.ts"],
      "read_paths": ["src/example.ts"],
      "cross_review_by": null,
      "can_parallelize": true,
      "depends_on": ["implementation-chunk-1"],
      "conflict_group": "",
      "status": "planned",
      "artifact_refs": []
    }
  ],
  "subagent_tracks": [
    {
      "id": "review-correctness",
      "phase": "deep_review",
      "status": "completed",
      "track": "correctness-regression",
      "owner": "dvandva-deep-reviewer",
      "parallelized": true,
      "rationale": "Independent correctness review did not edit shared files.",
      "inputs": ["git diff"],
      "outputs": ["No correctness blocker found."],
      "evidence_refs": ["subagent:review-correctness"],
      "result": "passed"
    }
  ],
  "verification_matrix": [
    {
      "id": "verify-phase-1",
      "phase": 1,
      "owner": "prativadi",
      "covers": ["src/example.ts", "test/example.test.ts"],
      "command": "bun test test/example.test.ts",
      "expected": "Feature tests pass and cover the new behavior.",
      "result": "pending",
      "evidence_ref": null
    }
  ],
  "master_plan_locked": true,
  "question": null,
  "resume_assignee": null,
  "resume_status": null,
  "disagreement_round": 0,
  "disagreement_cap": 3,
  "turn_cap": 60,
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

## Dynamic Agent Instances (Run 3)

Run 3 turns the static 15-agent roster into a **seed roster** for run-scoped dynamic agent generation. Parent roles may generate additional named instances on demand; each is recorded in `agent_instances` on the baton — a first-class array separate from the post-hoc `subagent_tracks` record. Generated instance IDs must not collide with coordinator owners (`vadi`, `prativadi`, `team`, `human`), seed-roster owners such as `dvandva-implementer`, or legacy standalone owner names such as `adversarial-analyst`; those reserved names are protocol owners, not generated handles.

### `agent_instances` item shape

```json
{
  "id": "r3-generated-security-review-01",
  "parent_role": "vadi",
  "spawned_by": "dvandva-security-auditor",
  "spawned_at_checkpoint": 8,
  "phase": 1,
  "purpose": "Review generated-agent write-helper gates for bypasses.",
  "agent_kind": "generated",
  "seed_agent": "dvandva-security-auditor",
  "model_class": "opus-class|gpt-5.5",
  "permission_class": "verify-only",
  "status": "closed",
  "work_item_ids": ["r3-dynamic-schema-and-write-gates"],
  "read_paths": ["plugins/dvandva/skills/vadi/scripts/dvandva-write.sh"],
  "write_paths": [],
  "depends_on": [],
  "conflict_group": "run3-helper-gates",
  "base_checkpoint": 8,
  "output_refs": ["subagent_track:r3-generated-security-review-01"],
  "evidence_refs": ["subagent:<handle>", "closed:<handle>"],
  "closed_at": "2026-06-28T00:00:00Z",
  "result": "passed"
}
```

Model classes are vendor-neutral: `opus-class|gpt-5.5` for architecture/planning/review (Opus on Claude Code, gpt-5.5 on Codex); `sonnet-class|gpt-5.4` for implementation/docs (Sonnet on Claude Code, gpt-5.4 on Codex). Permission classes are `readonly`, `verify-only`, `edit-scoped`, or `write-artifact-only`.

`spawned_by` is the executable provenance used for generated-instance validation. `seed_agent` is advisory human-readable metadata that records which seed-roster contract shaped the brief; the write helper does not currently validate that `seed_agent` equals `spawned_by` or belongs to the seed roster.

### Protocol invariants for generated instances

**Single-writer merge.** Generated agents never write the baton directly and never own `assignee`, `active_roles`, phase transitions, or `vadi_final_approval`/`prativadi_final_approval`. The parent role waits for all generated handles to close, then serializes their evidence into one monotonic baton checkpoint write.

**No daemon / no hidden orchestrator.** There is no background scheduler, mailbox, or launcher outside the baton and foreground wait helper. Run 3 adds richer baton data and a validation gate; it does not add a new central process.

**Explicit closure.** Every generated agent handle must be explicitly closed after its result is consumed. Codex closure evidence must include `closed:<handle>` or equivalent harness-specific proof. A closed instance must carry non-empty `work_item_ids` — an entry with an empty array is not considered validly closed. A track whose closure record is missing is not counted as complete.

**Dynamic write-path disjointness.** Dynamic instances with non-empty `write_paths` sharing the same `base_checkpoint`, or any two live (`planned`/`running`) instances regardless of base_checkpoint, must be pairwise disjoint unless they share the same `conflict_group` and are explicitly serialized by declared dependencies. Closed historical instances from earlier base checkpoints do not block later sequential path reuse. The Run 3 write helper rejects collisions in the current merge set.

**No additive sprawl.** Generated instances are run-scoped and ephemeral. A pattern may be promoted to the seed roster only through a later reviewed source change; the seed roster is never modified at runtime.

## State Machine

> **Authority:** `product.md` Appendix A is authoritative for v1 transitions. This section is reference; if the two diverge, the spec wins. Update this section when the spec changes.

### Accepted v2 modes

- `development` — full research -> planning -> implementation -> review flow.
  The accepted development table remains the current 26-edge v2 table.
- `research` — research-only run. It may optionally emit a seed-development
  plan when `research_outcome == seed_development`, but the run still terminates
  as research.
- `review` — review-only run. It starts from `review_intake` plus
  `review_target`, produces `review_ref`, and uses the shared
  `termination_review` gate before `done`.
- `feature-pr` — legacy alias for `development` on older batons.

### Development mode (v2, 26 edges)

- v2: `deslop` → `phase: N+1, parallel_implementing` is the non-final
  phase-advance edge. Final phases route to `termination_review` instead.
- `research_drafting` -> `research_review`
- `research_review` -> `research_revision`
- `research_revision` -> `research_review`
- `research_review` -> `spec_drafting`
- `spec_drafting` -> `spec_review`
- `spec_review` -> `spec_revision`
- `spec_revision` -> `spec_review`
- `spec_review` -> `parallel_implementing`
- `parallel_implementing` -> `test_creation`
- `test_creation` -> `cross_review`
- `cross_review` -> `cross_fixing`
- `cross_fixing` -> `test_creation`
- `cross_review` -> `deep_review`
- `deep_review` -> `phase_fixing`
- `deep_review` -> `review_of_review`
- `deep_review` -> `deslop`
- `review_of_review` -> `counter_review`
- `review_of_review` -> `deslop`
- `counter_review` -> `review_of_review`
- `counter_review` -> `deslop`
- `phase_fixing` -> `test_creation`
- `deslop` -> `phase_fixing`
- `deslop` -> `parallel_implementing`
- `deslop` -> `termination_review`
- `termination_review` -> `phase_fixing`
- `termination_review` -> `done`

### Research mode (v2, 12 edges)

- `research_drafting` -> `research_review`
- `research_review` -> `research_revision`
- `research_revision` -> `research_review`
- `research_review` -> `spec_drafting`
- `spec_drafting` -> `spec_review`
- `spec_review` -> `spec_revision`
- `spec_revision` -> `spec_review`
- `spec_review` -> `termination_review`
- `research_review` -> `termination_review`
- `termination_review` -> `phase_fixing`
- `phase_fixing` -> `research_review`
- `termination_review` -> `done`

Research mode requires `research_ref` after drafting and before terminal
`done`; it additionally requires `plan_ref` iff
`research_outcome == seed_development`. Required phases are mode-specific:
`research_*` statuses use `phase: "research"`; seed-plan statuses,
`phase_fixing`, `termination_review`, and terminal `done` use `phase: "spec"`.

### Review mode (v2, 9 edges)

- `research_drafting` -> `research_review`
- `research_review` -> `research_revision`
- `research_revision` -> `research_review`
- `research_review` -> `deep_review`
- `deep_review` -> `deslop`
- `deslop` -> `termination_review`
- `termination_review` -> `phase_fixing`
- `phase_fixing` -> `deep_review`
- `termination_review` -> `done`

Review mode requires `review_ref` before terminal `done`.
Every review-mode status uses `phase: "review"`.

### Legacy v1 explicit fallback

- Legacy v1: `spec_review` → `phase: 1, implementing` remains the direct
  explicit-fallback handoff into implementation.
- `(no legacy baton)` -> `spec_drafting`
- `spec_drafting` -> `spec_review`
- `spec_review` -> `spec_revision`
- `spec_review` -> `implementing`
- `spec_revision` -> `spec_review`
- `implementing` -> `phase_review`
- `phase_review` -> `phase_fixing`
- `phase_review` -> `review_of_review`
- `phase_review` -> `implementing` for the next phase or final `done`
- `phase_fixing` -> `phase_review`
- `review_of_review` -> `implementing` for the next phase or final `done`
- `review_of_review` -> `counter_review`
- `review_of_review` -> `human_decision`
- `counter_review` -> `review_of_review`
- `counter_review` -> `implementing` for the next phase or final `done`
- `counter_review` -> `human_decision`

### Shared intervention and ownership rules

- Development or research planning states may route to `human_question` while
  `master_plan_locked: false`, then resume via `resume_status` and
  `resume_assignee`.
- Any state may route to `human_decision`; a human may then route the baton back
  to any mode-owned state.
- There is no wildcard `* -> termination_review`; every stop-review entry is an
  explicit row in the mode tables above.
- `termination_review` plus both final approvals are shared across all v2
  modes.
- Terminal `done` is valid only from `termination_review` and only with the
  mode-conditional terminal artifact: `run_explainer_ref` plus both roles'
  completed approved `run_explainer_reviews` for that exact artifact in
  development, `research_ref` plus `plan_ref` iff `research_outcome ==
  seed_development` for research, and `review_ref` for review.
- Approval ownership is role-owned: `DVANDVA_ROLE=vadi` may raise only
  `vadi_final_approval`, and `DVANDVA_ROLE=prativadi` may raise only
  `prativadi_final_approval`.

Any other transition is illegal in v1 or v2. The writing agent must reject
illegal transitions and route to `human_decision` instead.

## Handoff Rule

The active agent must stop doing LLM work after writing a baton that assigns the next action to another actor. In default `run_mode: "walkaway"`, it then blocks in the foreground wait helper instead of exiting the overall run. After installing a handoff checkpoint, call the helper with `--since-checkpoint <written_checkpoint> --until-actionable` so team-owned `active_roles` states do not return the writer as ready on the same checkpoint and do not wake a role until that role has dependency-unblocked actionable work; the helper exits 0 only after a peer write advances the baton and the selected role is actionable.

Every baton write goes through `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh`,
which validates the v1 or v2 transition, installs atomically, and snapshots the
checkpoint. The live v2 write-helper enforcement covers named-run research
transitions, v2-only fields, safe run IDs, schema continuity, status-owner
pairs, `subagent_tracks`, the three-angle `deep_review -> deslop` gate, and the
mode-conditional terminal artifact gates for development, research, and review
runs, including the two-role `run_explainer_reviews` gate for development.

## Regular checkpoint commits

The active agent should make regular local checkpoint commits after verified
logical slices when `allow_commit == true`. Commit only the baton's intended
`changed_paths` union, excluding `.dvandva/` and `superpowers/`, and only after
the motivating verification commands pass. If `git status --short` shows
unrelated dirty paths, route to `human_decision` instead of committing. Use one
logical change per commit, semantic prefix, and a subject of 50 characters or
fewer. Record the commit hash in `verification` or `summary` as
`checkpoint_commit=<hash>`.

Checkpoint commits are local. Do not push until final ship, the
`termination_review` handoff has converged with both final approvals true on the
installed baton, and `allow_push == true`. If a later review rejects a
checkpointed change, fix it with a new commit rather than rewriting history
unless the human explicitly asks for history surgery.

This is the core anti-token-polling rule:

- The vadi does not spend model turns asking whether the prativadi moved.
- The prativadi does not spend model turns asking whether the vadi moved.
- In walkaway mode, the assigned-away agent runs `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role <vadi|prativadi> --interval 60 --max-wait 540 --until-actionable`. After a baton write, add `--since-checkpoint <written_checkpoint> --until-actionable` so active team states poll until the baton advances and the role has dependency-unblocked actionable work instead of bouncing the writer back to ready immediately.
- Continuous polling is the hard rule: `--max-wait` is a heartbeat interval, not a stop condition, and the helper keeps polling until this role owns the baton, the baton reaches post-handshake `done`, the baton enters `human_question`/`human_decision`, or the user interrupts. `termination_review` is active and wakes both roles; final approval alone is not a stop condition.
- `--persist` is accepted for older call sites and is now redundant. `--persist-max <seconds>` is the optional total wall-clock cap; the wait-helper persist cap exit 23 means the cap was reached, not that the peer is done. Re-enter the wait unless the user interrupts. Explicit `--finite` compatibility mode is the only path to timeout exit 20 and is not valid for normal walkaway loops.
- The write-helper validation exit 23 means a baton candidate failed schema, required-key, safe-run-id, v2 status-owner, status, or enum validation. Fix the candidate and rerun the write helper; do not edit the installed baton directly.
- Claude Code has a Bash-tool wall-clock cap around 600 seconds, so Claude-hosted sessions must relaunch the wait if a harness cap stops the shell before a terminal baton state. Codex-hosted sessions may use unbounded default continuous polling or pass `--persist` for older snippets.
- In supervised mode, the assigned-away agent exits and the human invokes the next role manually.
- When the helper exits 0 (`ready` or `checkpoint_advanced`), the agent re-reads the baton and resumes.
- When the helper exits 10, the agent surfaces post-handshake `done` and stops. When it exits 11 or 12, the agent surfaces the human-intervention `human_decision` or `human_question` state and pauses for the human. For `human_question`, the helper also prints `question`, `resume_assignee`, and `resume_status`.

## Goal Conditions

Use `/goal` around the baton state instead of around a timer.

Do not paste goal text from this reference. Use the role skill bodies or engine command files as the canonical source, because they include current Existing baton discovery, conditional parallelism, `subagent_tracks`, `run_explainer_ref`, and terminal explainer gates. This reference intentionally avoids duplicating those long strings so it cannot drift into a stale legacy fallback.

Both goals require the agent to surface a bounded `BATON_STATE_COMPACT` line at every checkpoint — produced by `dvandva-state.sh --compact` (refs, counts, current-role work, open findings, latest verification, and `next_action`) rather than pasting the full `work_split`/`subagent_tracks`/`verification_matrix` arrays or the full baton — and to read the authoritative full `baton.json` before any state-changing decision (baton write, approval, human handoff, or validator-failure diagnosis). The `/goal` evaluator detects exit conditions by reading that line in the transcript.

## Why Not LLM Polling

Two autonomous sessions using model turns to poll the same channel recreate the PR 353 problem locally. They spend tokens checking whether the other agent has moved.

The better default is serialized model work with shell waiting:

1. One agent runs.
2. It writes a baton.
3. It blocks in the wait helper if the run is still active.
4. The already-running next actor wakes and works.

Parallelism should be explicit and branch-scoped.
