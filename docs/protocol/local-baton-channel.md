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
dash; no slash, backslash, or `..`. The wait helper rejects unsafe
`DVANDVA_RUN_ID` values before resolving `.dvandva/runs/<run_id>/baton.json`,
and the write helper applies the same check to v2 baton candidates.

`.dvandva/` is machine coordination state. Generated user-facing artifacts
such as research reports, implementation plans, evaluations, reviews, pilot
write-ups, and run reports live under gitignored `./superpowers/**/*.html` as
dark, self-contained HTML and are referenced from baton fields such as
`research_ref` and `plan_ref`. Source/platform Markdown files such as
`SKILL.md`, command files, README/source docs, and prompt templates remain in
their native format.

The shareable templates live in `templates/channel/`.

## Baton Schema (v2)

This shows a v2 run-scoped baton. Legacy v1 batons use `schema: "dvandva.baton.v1"`, omit the v2-only fields `run_id`, `original_ask`, `research_ref`, `work_split`, and `verification_matrix`, and default `turn_cap` to 60. The live v2 write-helper enforcement covers v2-only fields, safe `run_id` values, schema continuity for existing runs, v2 status-owner pairs, and v2 lifecycle transitions.

```json
{
  "schema": "dvandva.baton.v2",
  "updated_at": "2026-05-13T10:30:00Z",
  "mode": "feature-pr",
  "run_mode": "walkaway",
  "run_id": "example-feature",
  "phase": 1,
  "total_phases": 3,
  "status": "phase_review",
  "assignee": "prativadi",
  "current_engine": "codex",
  "review_target": "implementation",
  "original_ask": "Implement the example feature with Dvandva review.",
  "research_ref": "./superpowers/research/2026-05-13-example-feature.html",
  "plan_ref": "./superpowers/plans/2026-05-13-example-feature.html",
  "work_split": [
    {
      "id": "phase-1-code",
      "phase": 1,
      "owner": "vadi",
      "scope": "Implement feature scaffolding and tests.",
      "paths": ["src/example.ts", "test/example.test.ts"],
      "status": "complete",
      "artifact_refs": ["./superpowers/research/2026-05-13-example-feature.html"]
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

### States (v2 research)

- `research_drafting` — vadi invokes `dvandva:research`, uses parallel subagents when available, writes `research_ref`, and records `work_split` plus `verification_matrix`
- `research_review` — prativadi performs independent research review and does not rely solely on the vadi artifact
- `research_revision` — vadi responds to research findings and updates the generated HTML artifact plus baton fields
- `test_creation` — vadi creates or updates tests after implementation; new behavior targets 100% test coverage or records a source-only rationale
- `deep_review` — prativadi performs review after test creation; review is separate from test creation
- `deslop` — cleanup loop for nits, low/minor bugs, stale wording, vague instructions, duplication, and generated-looking clutter

### Allowed transitions (v1)

**Spec phase:**

- `(no baton)` → `spec_drafting`
- `spec_drafting` → `spec_review`
- `spec_review` → `spec_revision` (Codex Q&A)
- `spec_review` → `phase: 1, implementing` (Codex accepts plan; only Codex can advance the spec)
- `spec_revision` → `spec_review` (Claude answered Q&A, hands back)
- any spec state while `master_plan_locked: false` → `human_question` (human answer needed before master plan lock)
- `human_question` → `resume_status` with `assignee: resume_assignee` (human answers; skill clears question fields)

**Research phase (v2):**

- `(no named-run baton)` → `phase: "research", status: "research_drafting"`
- `research_drafting` → `research_review` after vadi writes `research_ref`, `work_split`, and `verification_matrix`
- `research_review` → `research_revision` when prativadi finds source, coverage, or work-distribution gaps
- `research_revision` → `research_review` after vadi updates the research artifact and baton fields
- `research_review` → `phase: "spec", status: "spec_drafting"` when prativadi approves the research package
- any research state while `master_plan_locked: false` → `human_question`
- any research state → `human_decision`

**Implementation phase (per phase N):** `(impl)` below is shorthand for `review_target: implementation`.

- `phase: N, implementing` → `phase_review (impl)`
- v2: `phase: N, implementing` → `test_creation`
- v2: `test_creation` → `deep_review (impl)` after tests and coverage evidence are recorded
- v2: `deep_review (impl)` → `deslop` when implementation and tests are substantively accepted
- v2: `deep_review (impl)` → `phase_fixing` when bugs, missing tests, or verification gaps remain
- v2: `deslop` → `phase: N+1, implementing` or terminal `done` when no nits, low/minor bugs, stale wording, or unclear instructions remain except explicitly accepted `deferred` items
- v2: `deslop` → `phase_fixing` when cleanup finds behavior, test, or review blockers
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

The helper permits `human_question` and `human_decision` from early research
states even before `research_ref` exists, so agents can ask for missing setup or
escalate before the first research artifact is available. Other v2 states after
`research_drafting` require a non-empty `research_ref`.

For v2 candidates, `assignee` is status-owned: vadi owns
`research_drafting`, `research_revision`, `spec_drafting`, `spec_revision`,
`implementing`, `test_creation`, `deslop`, `phase_fixing`, and
`review_of_review`; prativadi owns `research_review`, `spec_review`,
`deep_review`, `phase_review`, and `counter_review`; human owns
`human_question` and `human_decision`. Terminal `done` is terminal regardless of
assignee. Existing batons cannot change schema mid-run.

Any other transition is illegal in v1 or v2. The writing agent must reject
illegal transitions and route to `human_decision` instead.

## Handoff Rule

The active agent must stop doing LLM work after writing a baton that assigns the next action to another actor. In default `run_mode: "walkaway"`, it then blocks in the foreground wait helper instead of exiting the overall run.

Every baton write goes through `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh`, which validates the v1 or v2 transition, installs atomically, and snapshots the checkpoint. The live v2 write-helper enforcement covers named-run research transitions, v2-only fields, safe run IDs, schema continuity, and status-owner pairs.

## Regular checkpoint commits

The active agent should make regular local checkpoint commits after verified
logical slices when `allow_commit == true`. Commit only the baton's intended
`changed_paths` union, excluding `.dvandva/` and `superpowers/`, and only after
the motivating verification commands pass. If `git status --short` shows
unrelated dirty paths, route to `human_decision` instead of committing. Use one
logical change per commit, semantic prefix, and a subject of 50 characters or
fewer. Record the commit hash in `verification` or `summary` as
`checkpoint_commit=<hash>`.

Checkpoint commits are local. Do not push until final ship, both final approvals
are true, and `allow_push == true`. If a later review rejects a checkpointed
change, fix it with a new commit rather than rewriting history unless the human
explicitly asks for history surgery.

This is the core anti-token-polling rule:

- The vadi does not spend model turns asking whether the prativadi moved.
- The prativadi does not spend model turns asking whether the vadi moved.
- In walkaway mode, the assigned-away agent runs `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role <vadi|prativadi> --interval 60 --max-wait 540`.
- In v2 persistent mode, the assigned-away agent may run the same helper with `--persist`. In that mode `--max-wait` is the heartbeat interval, and `--persist-max <seconds>` is the optional total wall-clock cap. The wait-helper persist cap exit 23 means the persistent cap was reached; it is a controlled wait stop, not proof the peer is done.
- The write-helper validation exit 23 means a baton candidate failed schema, required-key, safe-run-id, v2 status-owner, status, or enum validation. Fix the candidate and rerun the write helper; do not edit the installed baton directly.
- Claude Code has a Bash-tool wall-clock cap around 600 seconds, so Claude-hosted sessions must use finite `--max-wait 540` re-loops or `--persist --persist-max <600`. Codex-hosted sessions may use unbounded `--persist` when the shell budget supports it.
- In supervised mode, the assigned-away agent exits and the human invokes the next role manually.
- When the helper exits 0, the agent re-reads the baton and resumes.
- When the helper exits 10, 11, or 12, the agent surfaces `done`, `human_decision`, or `human_question` and stops. For `human_question`, the helper also prints `question`, `resume_assignee`, and `resume_status`.

## Goal Conditions

Use `/goal` around the baton state instead of around a timer.

The canonical v1 goal conditions are embedded in the two skill bodies (`plugins/dvandva/skills/vadi/SKILL.md` and `plugins/dvandva/skills/prativadi/SKILL.md`) under their `/goal condition` sections. Always use the version from the skill file rather than copying from this doc, since the skill version is what the goal evaluator actually parses against.

Vadi goal (paste into your engine):

```
/goal You are Dvandva vadi. Resolve the active Dvandva baton before every read: DVANDVA_BATON_FILE, else DVANDVA_RUN_DIR/baton.json, else safe DVANDVA_RUN_ID as .dvandva/runs/<run_id>/baton.json, else legacy .dvandva/baton.json. Continue the walkaway run until the resolved Dvandva baton status is "done", "human_question", or "human_decision". If assignee is not "vadi", wait on the resolved baton with ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role vadi --file "$BATON_FILE" --interval 60 --max-wait 540; Claude uses finite wait re-loops, while Codex may add --persist when the shell budget supports it. Before each checkpoint, surface BATON_STATE including DVANDVA_RUN_ID/run_id, original_ask, research_ref, plan_ref, turn_cap, changed files, verification commands and outcomes, and final approval fields; do not count shell wait heartbeats as turns. Never create a PR. Stop after turn_cap active model-work turns and assign human if still blocked.
```

Prativadi goal (paste into your engine):

```
/goal You are Dvandva prativadi. Resolve the active Dvandva baton before every read: DVANDVA_BATON_FILE, else DVANDVA_RUN_DIR/baton.json, else safe DVANDVA_RUN_ID as .dvandva/runs/<run_id>/baton.json, else legacy .dvandva/baton.json. Continue the walkaway run until the resolved Dvandva baton status is "done", "human_question", or "human_decision". If assignee is not "prativadi", wait on the resolved baton with ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE" --interval 60 --max-wait 540; Claude uses finite wait re-loops, while Codex may add --persist when the shell budget supports it. Before each checkpoint, surface BATON_STATE including DVANDVA_RUN_ID/run_id, original_ask, research_ref, plan_ref, turn_cap, findings, verification commands and outcomes, final approval fields, and the final baton contents; do not count shell wait heartbeats as turns. Never create a PR. Stop after turn_cap active model-work turns and assign human if still blocked.
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
