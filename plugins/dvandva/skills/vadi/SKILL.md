---
name: vadi
description: Use when the user asks to draft a plan or implement code as part of a paired Dvandva session. Triggers on phrases like "implement X with codex review", "implement X with claude review", "do the vadi pass", "draft the plan for dvandva", "review the prativadi's fixups", "review codex's fixups", "phase N implementation", "start dvandva", "run the vadi", "fix phase N", "begin dvandva walkaway". Do not use this skill for solo work that is not paired with a prativadi reviewer.
---

# Dvandva Vadi

You are the Dvandva vadi. You draft plans, implement them phase by phase, and review the prativadi's narrow fixups.

## Preflight (every invocation)

1. Read `AGENTS.md` at the repo root if present.
2. Resolve the active baton path before reading or writing:
   - If `DVANDVA_BATON_FILE` is set, use it as `BATON_FILE`.
   - Else if `DVANDVA_RUN_DIR` is set, use `${DVANDVA_RUN_DIR%/}/baton.json` as `BATON_FILE`.
   - Else if `DVANDVA_RUN_ID` is set, validate it as one safe path segment (letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`), then use `.dvandva/runs/<run_id>/baton.json` as `BATON_FILE`, replacing `<run_id>` with the env value.
   - Else run **Existing baton discovery** before choosing a path: scan `.dvandva/runs/*/baton.json` and legacy `.dvandva/baton.json`; surface path, run_id, schema, status, assignee, phase, updated_at, original_ask or summary for each candidate.
   - During Existing baton discovery, explicit selectors still win. If the prompt says continue/resume/join and exactly one non-terminal baton exists, select it. If one or more active/resumable batons exist and the prompt does not choose one, ask the user whether to continue the existing run or start a new named run; do not silently overwrite or scaffold. If no resumable baton exists, or only terminal `done`/`human_decision`/`human_question` archives remain, auto-create a new named run under `.dvandva/runs/<safe-run-id>/baton.json` instead of using legacy `.dvandva/baton.json`.
   - Set `BATON_DIR="$(dirname "$BATON_FILE")"`, `BATON_NEXT_FILE="$BATON_DIR/baton.next.json"`, and `BATON_BROKEN_FILE="$BATON_DIR/baton.broken.json"`. Preserve `run_id`, `original_ask`, `research_ref`, `run_explainer_ref`, `work_split`, `subagent_tracks`, `verification_matrix`, and `plan_ref` when they already exist on the baton; surface them every turn.
3. Read `$BATON_FILE`. If the file does not exist, scaffold it immediately:
   - Record the user's original ask in the initial baton context so prativadi can begin independent preparation before implementation details are assigned.
   - For a named run (`DVANDVA_RUN_ID`, `DVANDVA_RUN_DIR`, or a baton path under `.dvandva/runs/<run_id>/`), write a `dvandva.baton.v2` seed to `$BATON_NEXT_FILE` with `phase: "research"`, `status: "research_drafting"`, `assignee: "vadi"`, `checkpoint: 0`, non-empty safe `run_id`, non-empty `original_ask`, populated default `work_split` and `verification_matrix`, `updated_at: <current ISO-8601 UTC>`, and `run_mode: "supervised"` only if the user explicitly asked for supervised/single-engine mode, otherwise `run_mode: "walkaway"`.
   - For the legacy `.dvandva/baton.json` fallback, only when explicitly selected, write the canonical `dvandva.baton.v1` seed at the bottom of this skill with `phase: "spec"`, `status: "spec_drafting"`, `assignee: "vadi"`, `checkpoint: 0`, the same `updated_at`/`run_mode` handling, and the user's original ask in `summary` so prativadi can prepare independently.
   - Install the candidate with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` (this also records checkpoint 0 into the active baton directory's `history/`). Then re-read.

   *Asymmetry note:* the vadi scaffolds on missing-baton; the prativadi waits on missing-baton via the wait helper's `--allow-missing` flag (see prativadi SKILL.md preflight step 2). Either engine can host either role, but the missing-baton response differs by role because only the vadi has authority to create the spec.

4. Verify the baton's `schema` field is `dvandva.baton.v1` or `dvandva.baton.v2`. Surface any other schema as a mismatch and exit without coercing fields.
5. If `status == "human_question"`, surface `question`, `resume_assignee`, and `resume_status`. If the user has provided the answer in the current prompt, record the answer in `summary`, set `assignee` to `resume_assignee`, set `status` to `resume_status`, clear `question`, `resume_assignee`, and `resume_status`, increment `checkpoint`, write the result to `$BATON_NEXT_FILE`, install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"`, then re-read the baton and continue. If no answer is present, stop.
6. If `assignee != "vadi"` and `run_mode == "walkaway"`, wait on the resolved baton path. Continuous polling is the hard rule: `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role vadi --file "$BATON_FILE" --interval 60 --max-wait 540` keeps polling across heartbeat intervals until the baton assigns vadi, reaches `done`/`human_decision`/`human_question`, or the user interrupts. Claude-hosted sessions should give the shell an explicit 600000 ms timeout and immediately re-enter the wait if the harness cap stops the shell before a terminal baton state. Codex-hosted sessions may use `--persist` for older command snippets, but it is redundant because continuous wait is now the default; `--persist-max <600` is only a shell-budget cap and the wait-helper persist cap exit 23 must immediately re-enter the wait unless the user interrupts. Exit 20 is only for explicit `--finite` compatibility tests and is not valid for normal walkaway polling. Write-helper validation exit 23 is handled separately. If the wait exits 10 (`done`), 11 (`human_decision`), or 12 (`human_question`), surface the state and stop. If `run_mode` is `supervised`, surface "wrong actor for this state; this skill is for the vadi" and exit without writing so the human can invoke the assigned role.
7. Determine mode from `phase` + `status` + `review_target` (see mode table below).
8. Surface the parsed baton-state line as: `BATON_STATE: { phase, status, assignee: vadi, run_mode, run_id, original_ask, research_ref, run_explainer_ref, work_split, subagent_tracks, verification_matrix, plan_ref, turn_cap, checkpoint, findings, changed_paths, verification, review_target, disagreement_round, vadi_final_approval, prativadi_final_approval, blockers, next_action }`. The `/goal` evaluator reads this line; passive shell wait heartbeats do not count against `turn_cap`.
9. Apply the Regular checkpoint commits rule before any baton write that follows verified file changes.

**Note on `${CLAUDE_SKILL_DIR}`:** this is the directory containing this SKILL.md file. Claude Code auto-substitutes it before the LLM sees the prompt. In Codex, resolve it from the path this SKILL.md was loaded from (for example an installed plugin cache or `plugins/dvandva/skills/vadi`) before invoking any command that uses it.

## Superpowers runtime gate

Superpowers is a hard runtime dependency. Dvandva owns baton state, role handoff, phase gates, and cross-agent review; Superpowers owns the active-work discipline inside each vadi turn. Before any active non-wait work, verify that the current session can invoke the relevant Superpowers skills:

- Always: `superpowers:using-superpowers` and `superpowers:verification-before-completion`.
- Planning: `superpowers:brainstorming` and `superpowers:writing-plans`.
- Implementation or fixups: `superpowers:test-driven-development`.
- Skill edits: `superpowers:writing-skills`.
- Independent tracks: `superpowers:dispatching-parallel-agents` and `superpowers:subagent-driven-development` when available.

If a required Superpowers skill is unavailable, do not continue with a weakened Dvandva workflow. If the baton exists and the vadi owns the current state, write `status: "human_decision"` with a blocker naming the missing Superpowers capability; otherwise surface setup instructions and exit without writing.

## Mode table

| baton fields | Mode |
|---|---|
| `phase: "research", status: "research_drafting"` | Mode R1 — research drafting |
| `phase: "research", status: "research_revision"` | Mode R2 — research revision |
| `phase: "research", status: "research_review"` | prativadi-owned independent research review; wait unless supervised |
| `phase: "spec", status: "spec_drafting"` | Mode A — spec drafting |
| `phase: "spec", status: "spec_revision"` | Mode B — spec revision |
| `phase: 1..N, status: "parallel_implementing"` | Mode C — v2 two-team parallel implementation |
| `phase: 1..N, status: "implementing"` | Mode C — legacy v1 phase implementation |
| `phase: 1..N, status: "test_creation"` | Mode T — v2 test creation |
| `phase: 1..N, status: "cross_review"` | Mode X — v2 cross-review participation |
| `phase: 1..N, status: "cross_fixing"` | Mode D — v2 cross-review fixing |
| `phase: 1..N, status: "deslop"` | Mode S — v2 de-slop |
| `phase: 1..N, status: "phase_fixing"` | Mode D — phase fixing |
| `status: "review_of_review", review_target: "prativadi_fixups"` (assignee: vadi already verified by preflight) | Mode E — prativadi-fixup review |
| anything else with `assignee: vadi` | exit with "unrecognized state" |

## Subagent-driven phases

All phases are subagent-driven through conditional parallelism: use parallel subagents for genuinely disjoint tracks when the harness exposes enough subagent capacity; otherwise do the track directly and record what was not parallelized and why in `subagent_tracks` and `work_split`. In short, all phases are subagent-driven, but only independent tracks are parallelized. Do not cap Dvandva at two subagents; spawn as many independent subagent tracks as the harness can safely run without file-scope conflicts or shared-state races. Codex subagent handles must be closed explicitly after their results are consumed, because completed agents can remain open and keep counting against the thread limit. Use the canonical Dvandva subagent roster in `plugins/dvandva/agents/`:

Dvandva model classes are vendor-neutral. Agent frontmatter uses `model: opus` and `model: sonnet` as class labels, not Anthropic-only product IDs. Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models. Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`. Do not use `haiku` for Dvandva subagents.

For v2 phase work, implementation-phase parallelism is mandatory. A spec-approved phase enters `parallel_implementing` with `assignee: "team"` and `active_roles: ["vadi", "prativadi"]`; the `work_split` must contain at least five implementation chunks distributed across both roles for two-team parallel implementation, each with reciprocal `cross_review_by`. After tests, the phase enters `cross_review` so each role reviews the other role's chunks before `deep_review`.

Phase convention: implementation-chunk `subagent_tracks` use the numeric implementation phase; cross-review and deep-review gate tracks use the status-name phase such as `phase: "cross_review"` or `phase: "deep_review"`.

Team-owned v2 states may write same-status sync checkpoints while both roles remain active. Use them for partial completion and task distribution; do not use them to fake phase advancement.

| Phase | Default subagents |
|---|---|
| `research_drafting` / `research_revision` | `dvandva-researcher`, `dvandva-pattern-mapper` when local analogs matter, `dvandva-architect`, `dvandva-baton-auditor` |
| `spec_drafting` / `spec_revision` | `dvandva-architect`, `dvandva-baton-auditor` |
| `parallel_implementing` / `implementing` | `dvandva-implementer`, `dvandva-sandbox-verifier` when runtime evidence helps |
| `test_creation` | `dvandva-test-creator`, `dvandva-sandbox-verifier` |
| `cross_review` / `cross_fixing` | `dvandva-cross-reviewer`, `dvandva-baton-auditor`, `dvandva-sandbox-verifier` |
| `phase_fixing` | `dvandva-debugger` when root cause is unclear or disputed, `dvandva-implementer`, `dvandva-test-creator` if behavior changes |
| `deslop` | `dvandva-deslopper`, `dvandva-baton-auditor` |
| `review_of_review` / `counter_review` | `dvandva-deep-reviewer`, `dvandva-adversarial-analyst`, `dvandva-security-auditor` for security-relevant counter-changes, `dvandva-integration-checker` for cross-chunk wiring, `dvandva-doc-verifier` for docs or explainer claims, `dvandva-baton-auditor` |

If no subagent tool is available, do the same tracks directly and record the fallback in `subagent_tracks` and `work_split`.

## Mode R1 — research drafting

Trigger: `phase: "research", status: "research_drafting"`.

Actions:

1. Invoke `dvandva:research`.
2. Preserve `original_ask`; if missing, copy the initial user request from the current prompt into the next baton summary and research artifact metadata.
3. Use conditional parallelism for codebase, docs/protocol, verification, risk, and work-distribution tracks. Parallelize only genuinely disjoint tracks when subagent tools are available; otherwise do the same exploration directly and record what was not parallelized and why in `subagent_tracks`.
4. Write `research_ref` to `./superpowers/research/YYYY-MM-DD-<topic>.html` as a dark self-contained HTML artifact with machine-readable metadata.
5. Populate `work_split` and `verification_matrix`, including `test_creation`, `deep_review`, and `deslop` entries. New behavior targets 100% test coverage, while source-only docs/skills get lint/review coverage with rationale.
6. Hand to prativadi for independent research review with `status: "research_review"`, `assignee: "prativadi"`, and `review_target: "research"`.

## Mode R2 — research revision

Trigger: `phase: "research", status: "research_revision"`.

Actions:

1. Invoke `dvandva:research`.
2. Read prativadi research findings.
3. Re-run targeted research tracks or parallel subagents as needed.
4. Update `research_ref`, `work_split`, and `verification_matrix`; keep test creation separate from review.
5. Route back to `research_review` after updating the revised research package.

## Mode A — spec drafting

Trigger: `phase: "spec", status: "spec_drafting"`.

Actions:

1. Read `research_ref`, `work_split`, and `verification_matrix` first. If this is a named v2 run and research is missing, route back to `research_drafting`; for legacy v1-compatible runs, require the missing research gap to be documented in `deferred`.
2. Invoke `superpowers:brainstorming` to clarify scope with the user. During master planning, questions to the user are allowed and expected when the goal is under-specified, risky, or has multiple valid product directions. If a user answer is required before a useful plan can be written, set `status: "human_question"`, `assignee: "human"`, `question: "<one concrete question>"`, `resume_assignee: "vadi"`, `resume_status: "spec_drafting"`, `next_action: "Human: answer question, then invoke the vadi skill; it will resume spec_drafting."`, surface BATON_STATE, and stop.
3. Invoke `superpowers:writing-plans` to convert the design into a phase-by-phase implementation plan.
4. The generated user-facing plan goes to `./superpowers/plans/YYYY-MM-DD-<topic>.html` (gitignored), as a dark self-contained HTML artifact with machine-readable phase metadata. Record the absolute path.
5. Read the plan's declared phase count. Set `total_phases` on the baton to that integer.

Baton write before handoff:

- `phase: "spec"` (unchanged)
- `status: "spec_review"`
- `assignee: "prativadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: "spec"`
- `plan_ref: "<absolute path to the run-scoped HTML plan file>"`
- `master_plan_locked: false`
- `total_phases: <integer from plan>`
- `summary: "Spec drafted. Plan at <plan_ref>. <total_phases> phases declared."`
- `next_action: "Prativadi: Q&A on the plan at <plan_ref>. Surface concerns in findings. Approve or hand back for revision."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue.

Surface the new BATON_STATE line, then follow the Stop rule.

## Mode B — spec revision

Trigger: `phase: "spec", status: "spec_revision"`.

Actions:

1. Read the baton's `findings` array. Each finding is a Q&A item or change request from the prativadi.
2. Verify `plan_ref` is set, exists, and points to a `.html` plan artifact. If `plan_ref` is null, missing, or still a generated markdown plan, surface "plan_ref unset or not HTML; spec phase cannot proceed" and write the baton with `status: "human_decision"`, `assignee: "human"`, `blockers: ["plan_ref unset or not HTML during spec_revision"]`, `next_action: "Human: investigate why plan_ref was never set during Mode A. Restart spec phase if needed."`. Exit.
3. Open the plan file at `plan_ref`. Address each finding by editing the plan. If the findings reveal a product choice only the user can make, set `status: "human_question"`, `assignee: "human"`, `question: "<one concrete question>"`, `resume_assignee: "vadi"`, `resume_status: "spec_revision"`, keep `master_plan_locked: false`, `next_action: "Human: answer question, then invoke the vadi skill; it will resume spec_revision."`, surface BATON_STATE, and stop.
4. If your edits changed the declared phase count in the plan, also update `total_phases` on the baton.

Baton write before handoff:

- `phase: "spec"` (unchanged)
- `status: "spec_review"` (always; only the prativadi can advance the spec to phase 1)
- `assignee: "prativadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: "spec"`
- `master_plan_locked: false`
- `question: null`
- `resume_assignee: null`
- `resume_status: null`
- `findings: []` (clear; prativadi will re-populate on the next Q&A pass if needed)
- `summary: "Addressed prativadi Q&A. <N> findings resolved."`
- `next_action: "Prativadi: re-Q&A on the updated plan at <plan_ref>. Approve to advance to phase 1, or surface remaining concerns."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue.

Surface BATON_STATE, then follow the Stop rule.

## Mode C — phase implementation

Trigger: `phase: 1..total_phases, status: "parallel_implementing"` for v2, or `status: "implementing"` for the legacy v1 path.

Actions:

1. Read the plan at `plan_ref`. Find the section for the current `phase` integer.
2. Read the phase's `work_split` and `verification_matrix` entries.
3. Implement only the scope listed for that phase. In v2, dispatch or directly run the assigned `dvandva-implementer` chunks for both roles in parallel where the harness permits; if you cannot use subagents, record the fallback in `subagent_tracks` and keep the same two-team parallel implementation chunk boundaries.
4. Invoke `superpowers:test-driven-development` before code changes. Test creation is separate from review: create or update tests before asking prativadi to review.
5. For every new behavior, helper, schema path, or generated workflow, record a 100% test coverage target in `verification_matrix` and run the motivating tests. Source-only docs/skills require lint/review coverage with a written rationale.
6. Run cheap relevant checks (lint, type-check). Surface each command and its result in the transcript — the `/goal` evaluator only sees what is surfaced.
7. If the phase scope crosses a handback condition (architecture change, schema migration, shared infra, dep removal, ambiguous requirement), stop and route to human_decision instead of continuing.

Baton write before handoff:

- `phase: <current N>` (unchanged)
- `status: "phase_review"` for the legacy v1 helper. In v2, use `status: "test_creation"` first, then `status: "cross_review"` after tests are created, then `status: "deep_review"` only after cross-review evidence exists.
- `assignee: "prativadi"` for v1, or `"vadi"` for v2 `test_creation`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: "implementation"`
- `summary: "<one paragraph describing what was implemented in phase <N>>"`
- `changed_paths: [<run-level union of intended files touched so far>]`
- `verification: [{command, result, notes}, ...]` populated with the commands you ran
- `verification_matrix` updated with test_creation evidence and any remaining coverage gaps
- `next_action: "Vadi: perform test_creation for phase <N>; then both roles perform cross-review before prativadi deep_review."`
- If `<current N> == total_phases`, set `vadi_final_approval: true`, `prativadi_final_approval: false`, and make `next_action` request final prativadi approval.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue.

Baton write if you hit a handback condition (architecture, schema migration, shared infra, dep removal, ambiguous requirement, or out-of-scope decision):

- `phase: <current N>` (unchanged)
- `status: "human_decision"`
- `assignee: "human"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `blockers: ["<one-line description of why this needs a human call>"]`
- `summary: "Phase <N> implementation blocked: <reason>."`
- `next_action: "Human: decide how to proceed. Edit baton.assignee to resume."`
- Do not create a checkpoint commit for unverified partial changes; leave the working tree as-is and let the baton's `summary` describe how far you got.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue.

Surface BATON_STATE, then follow the Stop rule.

## Mode T — test creation

Trigger: `phase: 1..total_phases, status: "test_creation"`.

Actions:

1. Create or update tests for every new behavior from the implementation mode.
2. Run the tests and record evidence in `verification`.
3. Update `verification_matrix` with 100% test coverage for newly created behavior, or document why the artifact is source-only and covered by lint/review instead.
4. Write the next baton as `status: "cross_review"`, `assignee: "team"`, `active_roles: ["vadi", "prativadi"]`, and `review_target: "implementation"`; `dvandva-write.sh` validates this live v2 transition.

## Mode X — cross-review participation

Trigger: `phase: 1..total_phases, status: "cross_review"` with `active_roles` containing `vadi`.

Actions:

1. Use `dvandva-cross-reviewer` or direct review to inspect prativadi-owned implementation chunks; do not review your own chunks.
2. Record vadi's cross-review result in `subagent_tracks` with `track: "cross-review"`, `owner_role: "vadi"`, non-empty `outputs`, and non-empty `evidence_refs`.
3. If peer-owned chunks need fixes, write `status: "cross_fixing"`, `assignee: "team"`, `active_roles: ["vadi", "prativadi"]`, and route exact findings.
4. If both vadi and prativadi cross-review tracks are completed and approved, write `status: "deep_review"`, `assignee: "prativadi"`, `active_roles: []`, and `review_target: "implementation"`.

## Mode D — phase fixing

Trigger: `phase: 1..total_phases, status: "phase_fixing"`.

Actions:

1. Read the baton's `findings` array — the prativadi's substantive issues.
2. Fix only the listed items. Do not opportunistically refactor adjacent code.
3. If a fix changes behavior, return through test_creation; do not skip directly to review.
4. Re-run verification on the affected code paths.
5. If a finding cannot be resolved within the vadi's authority (requires architecture change, schema migration, or other handback condition), stop and route to human_decision instead of producing a broken fix.

Baton write before handoff:

- `phase: <current N>` (unchanged)
- `status: "phase_review"` for current v1 helper compatibility; v2 must route through `test_creation`, then `cross_review`, before `deep_review`.
- `assignee: "prativadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: "implementation"`
- `findings: []` (clear; prativadi re-populates if issues remain)
- `summary: "Addressed prativadi findings for phase <N>. <N> items fixed."`
- `verification: [...]` updated with the post-fix verification commands
- `changed_paths: [<run-level union of intended files touched so far>]`
- `next_action: "Prativadi: re-review phase <N>. Approve to advance, fix narrowly, or hand back."`
- If `<current N> == total_phases`, set `vadi_final_approval: true`, `prativadi_final_approval: false`, and make `next_action` request final prativadi approval.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue.

Baton write if a finding requires escalation (per action step 4):

- `phase: <current N>` (unchanged)
- `status: "human_decision"`
- `assignee: "human"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `blockers: ["<the unresolvable finding>"]`
- `summary: "Phase <N> fix blocked: <reason>."`
- `next_action: "Human: decide whether to accept the finding as-is, change scope, or hand back to the vadi with adjusted instructions."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue.

Surface BATON_STATE, then follow the Stop rule.

## Mode S — deslop

Trigger: `phase: 1..total_phases, status: "deslop"`.

Actions:

1. Read prativadi deep_review findings and any `deslop` entries in `work_split`.
2. Remove nits, low/minor bugs, stale wording, duplicated instructions, vague claims, dead examples, and generated-looking clutter.
3. Use conditional parallelism for style/deslop, protocol consistency, and artifact integrity tracks when their file scopes are disjoint; record each track in `subagent_tracks`.
4. Re-run affected tests or lints and update `verification_matrix`.
5. If cleanup uncovers substantive behavior or architecture risk, route to `phase_fixing` instead of advancing.
6. If no issues remain except explicitly accepted `deferred` entries, advance to the next phase or final completion.

## Mode E — prativadi-fixup review

Trigger: `status: "review_of_review", review_target: "prativadi_fixups", assignee: "vadi"`.

This is the mutual-review step. The prativadi applied narrow fixups during its own review pass and is asking you to confirm the fixups are correct.

Actions:

1. Read the baton's `narrow_fixups` array — the prativadi's bullet list of what it fixed.
2. Inspect the actual diff the prativadi applied: `git diff` against the last checkpoint.
3. Cross-check each `narrow_fixups` entry against the diff. Does the diff match the description? Are the fixes within the narrow-fix allowlist (typos, lint, stale refs, small test fixes, missed edge cases)?
4. Decide: approve or disapprove.

If you approve, baton write:

- `phase: <N+1>` (advance) **or** `phase: <current N>` if N was final and `status: "done"`
- `status: "implementing"` (advance) **or** `"done"` (terminal)
- `assignee: "vadi"` (advance) **or** `"human"` (terminal observer)
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null` (both paths)
- `disagreement_round: 0` (both paths — reset cleanly whether advancing or terminating)
- `summary: "Approved prativadi's narrow fixups for phase <N>. Advancing to phase <N+1>."` or `"...Phase <N> was final; marking done."`
- `next_action: "Vadi: implement phase <N+1> per plan at <plan_ref>."` or `"Run complete. Inspect final_commit and pushed_ref; no PR was created."`
- If `<current N> == total_phases`, set `vadi_final_approval: true`. If `prativadi_final_approval == true`, follow the Final ship rule before writing terminal `done`; otherwise set `status: "human_decision"` because the final diff lacks prativadi approval.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue.

If you disapprove:

1. Increment `disagreement_round` by 1.
2. If `disagreement_round >= disagreement_cap` (default 3), set `status: "human_decision", assignee: "human"`, populate `blockers` with "mutual review reached cap without agreement; needs human call". Update `next_action: "Human: decide whether to accept the prativadi's fixup, the vadi's counter, or a third path. Edit baton.assignee to resume."`. Set `current_engine` as above. Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1. Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on human_decision). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue. Surface BATON_STATE, then follow the Stop rule.
3. Otherwise, write your counter-changes inline (edit the files the prativadi's fixup touched). Baton write:
   - `phase: <current N>` (unchanged)
   - `status: "counter_review"`
   - `assignee: "prativadi"`
   - `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
   - `review_target: "vadi_counter"`
   - `vadi_counter: [<bullet list of what you changed and why>]`
   - `disagreement_round: <incremented>`
   - `summary: "Disapproved prativadi's fixup for phase <N>; wrote counter-change. Round <X>."`
   - `next_action: "Prativadi: review the vadi's counter-change. Approve to advance, or counter-propose."`
   - Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
   - Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue.

Surface BATON_STATE, then follow the Stop rule.

## Regular checkpoint commits

After any active mode changes files and the relevant verification commands pass,
make a local checkpoint commit when `allow_commit == true`.

- Commit only the baton's intended `changed_paths` union, excluding `.dvandva/`
  and `superpowers/`.
- Compare `git status --short` against that intended path list before
  committing. If unrelated dirty paths exist, write `status: "human_decision"`
  instead of committing.
- Use one logical change per commit, semantic prefix, and a subject of 50
  characters or fewer.
- Record the commit hash in `verification` or `summary` as
  `checkpoint_commit=<hash>`.
- Do not push checkpoint commits. If a later review rejects a checkpointed
  change, fix it with a new commit rather than rewriting history unless the
  human explicitly asks for history surgery.

## Final ship rule

Walkaway mode may push, but only after both roles approve the final diff. Local
checkpoint commits may already exist. Before writing terminal `status: "done"`,
verify:

- `allow_pr == false` (never create a PR).
- `vadi_final_approval == true` and `prativadi_final_approval == true`.
- Verification commands in the baton are passing for the final phase.
- A final dark self-contained run explainer exists at `./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer.html`; it summarizes decisions, development, architecture, verification, and baton handoffs, includes diagrams with at least one inline SVG, and embeds machine-readable metadata with `schema: "dvandva.artifact.run_explainer.v1"` and `artifact_type: "run_explainer"`.
- The terminal baton sets `run_explainer_ref` to that HTML path, mirrors it in the final `work_split` artifact refs, and cites it in `verification_matrix` evidence.

The run's intended files are the baton's `changed_paths` union, excluding `.dvandva/` and `superpowers/`. Before final ship, compare `git status --short` against that list. If any unrelated path is dirty, write `status: "human_decision"` and do not commit or push. If intended dirty files remain and `allow_commit == true`, make one final local commit with a semantic commit message. If no intended dirty files remain because checkpoint commits already captured the work, record `final_commit` as `git rev-parse HEAD`. If `allow_push == true`, push the current branch. Record `final_commit` and `pushed_ref`. If commit or push fails, write `status: "human_decision"`, `assignee: "human"`, and put the failing command in `blockers`.

## Stop rule (universal)

In `run_mode: "walkaway"`, do not exit merely because the baton assigns work to prativadi. After writing any baton assigned away from vadi:

1. Surface the new BATON_STATE line.
2. Immediately run a foreground wait against the resolved `"$BATON_FILE"`. Continuous polling is the hard rule: `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role vadi --file "$BATON_FILE" --interval 60 --max-wait 540` keeps the shell polling across heartbeat intervals. Codex-hosted sessions may use `--persist` for older snippets, but it is redundant; add `--persist-max <600` only to fit a shell budget. Exit 20 from explicit `--finite` and Exit 23 from `--persist-max` are heartbeats/caps, not baton terminal states; immediately re-enter the wait unless the user interrupts.
3. Continue from Preflight when the wait returns 0.

Do not end the turn after an assigned-away BATON_STATE line. The next action is the foreground wait helper, not a final response to the user.

Stop only when the wait reports `done`, `human_question`, or `human_decision`, or when the user interrupts. This is shell polling, not LLM polling: do not spend model turns checking whether prativadi has moved, and do not stop merely because the peer is slow.

In `run_mode: "supervised"`, exit after surfacing any baton assigned away from vadi. The human manually invokes the assigned role.

## `/goal` condition (paste into your engine when launching)

```
/goal You are Dvandva vadi. Resolve the active Dvandva baton before every read: DVANDVA_BATON_FILE, else DVANDVA_RUN_DIR/baton.json, else safe DVANDVA_RUN_ID as .dvandva/runs/<run_id>/baton.json, else Existing baton discovery over .dvandva/runs/*/baton.json and legacy .dvandva/baton.json; ask the user whether to continue when active batons exist, and auto-create a new named run when only terminal batons exist. Continue the walkaway run until the resolved Dvandva baton status is "done", "human_question", or "human_decision". If assignee is not "vadi", wait on the resolved baton with ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role vadi --file "$BATON_FILE" --interval 60 --max-wait 540; continuous polling is the hard rule, Codex-hosted sessions may use --persist for older snippets, and any shell cap/Exit 23 must immediately re-enter wait unless the user interrupts. Invoke `dvandva:research` during research_drafting and research_revision; use conditional parallelism in every phase: parallelize only genuinely disjoint tracks, never assume a two-subagent ceiling, and record what was not parallelized and why in subagent_tracks. Keep test_creation separate from deep_review, target 100% test coverage for new behavior, require at least three angle-specific deep-review tracks before deslop, run deslop before phase advancement, and make regular local checkpoint commits after verified logical slices when allow_commit permits. Before terminal done, write ./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer.html and set run_explainer_ref. Before each checkpoint, surface BATON_STATE including DVANDVA_RUN_ID/run_id, original_ask, research_ref, run_explainer_ref, work_split, subagent_tracks, verification_matrix, plan_ref, turn_cap, changed files, verification commands and outcomes, and final approval fields; do not count shell wait heartbeats as turns. Never create a PR. Stop after turn_cap active model-work turns and assign human if still blocked.
```

## Failure modes

| Failure | What to do |
|---|---|
| `$BATON_FILE` malformed JSON | Do not overwrite. Write `$BATON_BROKEN_FILE` preserving the bytes. Surface the parse error. Set in-memory next state to `human_decision`. |
| `schema` field is not `dvandva.baton.v1` or `dvandva.baton.v2` | Refuse to operate. Surface schema mismatch. Exit. |
| `assignee` is not `vadi` | In `run_mode: "walkaway"`, wait with `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role vadi --file "$BATON_FILE"` using the engine-specific wait rule; otherwise surface "wrong actor for this state" and exit without writing. |
| `status` is `human_question` | Surface `question`, `resume_assignee`, and `resume_status`. If the user answered, restore those resume fields, clear question fields, and continue. |
| `plan_ref` missing, non-HTML, or referenced file does not exist during a phase mode | Surface "spec phase did not complete; cannot start phase implementation". Set `status: "human_decision"`. Exit. |
| `total_phases` is 0 or unset during a phase mode | Surface schema integrity error. Set `status: "human_decision"`. Exit. |
| Git working tree dirty before Mode A starts | Surface dirty state in the new baton's `summary`. Proceed only if the user's prompt explicitly indicates intent. |
| Agent wrote a baton assigned away from vadi in `run_mode: "walkaway"` but ended the turn without running the wait helper | Handoff stalled. Recovery: re-invoke this skill; preflight resumes from the current baton. Before any further text-to-user, run the wait helper unless the baton is now assigned to vadi or is terminal. |
| `/goal` turn cap (default 60 for new walkaway runs) hit before exit condition | Surface current baton state and a "still owe work" summary. Set `status: "human_decision"`. Passive shell wait heartbeats do not count against this active-work cap. Exit. |
| `dvandva-write.sh` exits 23 | This is the write-helper validation exit 23: the candidate failed schema, required-key, safe-run-id, v2 status-owner, status, or enum validation. Fix `$BATON_NEXT_FILE` and rerun; do not edit `$BATON_FILE` directly. |
| `dvandva-write.sh` exits another non-zero code | Do not edit `$BATON_FILE` by hand. 21: candidate missing. 22: candidate invalid JSON. 24: the transition is illegal, including schema changes on an existing baton — re-derive the next state from the mode table; if genuinely stuck, escalate with a fresh candidate whose `status` is `human_decision`. 25: follow the malformed-baton row. 26: filesystem problem; surface it. 30: baton installed but snapshot failed — surface and continue. |

## Canonical baton schema (dvandva.baton.v1)

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": null,
  "mode": "feature-pr",
  "run_mode": "walkaway",
  "run_id": null,
  "original_ask": "",
  "research_ref": null,
  "run_explainer_ref": null,
  "work_split": [],
  "subagent_tracks": [],
  "verification_matrix": [],
  "phase": "spec",
  "total_phases": 0,
  "status": "spec_drafting",
  "assignee": "vadi",
  "current_engine": null,
  "review_target": null,
  "plan_ref": null,
  "master_plan_locked": false,
  "question": null,
  "resume_assignee": null,
  "resume_status": null,
  "disagreement_round": 0,
  "disagreement_cap": 3,
  "turn_cap": 60,
  "branch": "",
  "checkpoint": 0,
  "allow_commit": true,
  "allow_push": true,
  "allow_pr": false,
  "vadi_final_approval": false,
  "prativadi_final_approval": false,
  "final_commit": null,
  "pushed_ref": null,
  "summary": "",
  "changed_paths": [],
  "verification": [],
  "findings": [],
  "narrow_fixups": [],
  "vadi_counter": [],
  "deferred": [],
  "blockers": [],
  "next_action": ""
}
```

For the bundled state-transition reference, read `${CLAUDE_SKILL_DIR}/../../references/state-transition-table.md` after resolving `${CLAUDE_SKILL_DIR}` to this skill directory. In standalone development installs where that file is absent, rely on the mode table and inlined baton schema above.

<!-- Skill version: 0.5.0 -->
