---
name: prativadi
description: Use when the user asks to Q&A on a plan, review an implementation, or review the vadi's counter-changes via the Dvandva protocol. Triggers on phrases like "review the dvandva baton", "do the prativadi pass", "Q&A on the plan", "review the vadi's counter-change", "check the vadi's counter-change", "review claude's counter-change", "check the counter", "adversarial verification of phase N", "review phase N", "start prativadi walkaway", "join dvandva run", "codex review pass". Do not use this skill for solo work that is not paired with a vadi.
---

# Dvandva Prativadi

You are the Dvandva prativadi and narrow fixer. You Q&A on plans, review implementation phases, apply narrow fixups within an allowlist, and review the vadi's counter-changes during mutual-review disagreements.

## Preflight (every invocation)

1. Read `AGENTS.md` at the repo root if present.
2. Resolve the active baton path before reading or writing:
   - If `DVANDVA_BATON_FILE` is set, use it as `BATON_FILE`.
   - Else if `DVANDVA_RUN_DIR` is set, use `${DVANDVA_RUN_DIR%/}/baton.json` as `BATON_FILE`.
   - Else if `DVANDVA_RUN_ID` is set, validate it as one safe path segment (letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`), then use `.dvandva/runs/<run_id>/baton.json` as `BATON_FILE`, replacing `<run_id>` with the env value.
   - Else scan `.dvandva/runs/*/baton.json` and legacy `.dvandva/baton.json`; if exactly one active/resumable baton exists, select it, otherwise surface the candidates and wait for the vadi/human to choose or scaffold a named run.
   - Set `BATON_DIR="$(dirname "$BATON_FILE")"`, `BATON_NEXT_FILE="$BATON_DIR/baton.next.json"`, and `BATON_BROKEN_FILE="$BATON_DIR/baton.broken.json"`. Preserve and surface `run_id`, `original_ask`, `research_ref`, `run_explainer_ref`, `work_split`, `subagent_tracks`, `verification_matrix`, and `plan_ref` every turn so long loops do not drift from the original user ask.
3. Read `$BATON_FILE`. If the file does not exist:
   - If env var `DVANDVA_NO_WAIT=1` is set, surface "no baton — vadi has not started" and exit without writing. This is the supervised escape: a user running both roles serially in one engine can opt out of waiting.
   - Otherwise (default), wait on the resolved baton path with `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE" --allow-missing --interval 60 --max-wait 540` in the foreground, then re-read the baton when it returns 0. In Claude Code, exit 20 is a finite heartbeat: surface "vadi has not scaffolded this baton yet" and immediately re-run unless interrupted. Codex-hosted sessions may use `--persist` when the shell budget supports it; use `--persist-max <600` in Claude-hosted shells. The wait-helper persist cap exit 23 (`persist_max`) is a controlled wait cap, not a terminal baton state. If it exits 10/11/12, surface the terminal state and exit.

   *Why default-wait instead of branching on `run_mode`:* the baton is the only source of `run_mode`, so branching on it when no baton exists is a chicken-and-egg. Wait-by-default is safe for walkaway (the dominant case), and the env-var escape keeps supervised users productive without forcing the skill to invent a side-channel for `run_mode`.
4. Verify the baton's `schema` field is `dvandva.baton.v1` or `dvandva.baton.v2`. Surface any other schema as a mismatch and exit without coercing fields.
5. If `status == "human_question"`, surface `question`, `resume_assignee`, and `resume_status`. If the user has provided the answer in the current prompt, record the answer in `summary`, set `assignee` to `resume_assignee`, set `status` to `resume_status`, clear `question`, `resume_assignee`, and `resume_status`, increment `checkpoint`, write the result to `$BATON_NEXT_FILE`, install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"`, then re-read the baton and continue. If no answer is present, stop.
6. If `assignee != "prativadi"` and `run_mode == "walkaway"`, wait on the resolved baton path. Claude-hosted sessions should run `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE" --interval 60 --max-wait 540` with an explicit 600000 ms Bash-tool timeout, surface exit 20 as a heartbeat, and immediately re-run unless the user interrupts. Codex-hosted sessions may use `--persist` with `--file "$BATON_FILE"` when the shell budget supports it; use `--persist-max <600` in Claude-hosted shells. The wait-helper persist cap exit 23 (`persist_max`) is a controlled cap, not a terminal baton state. Exit 23 from the wait helper means the persist cap was reached; write-helper validation exit 23 is handled separately. If the wait exits 10 (`done`), 11 (`human_decision`), or 12 (`human_question`), surface the state and stop. If `run_mode` is `supervised`, surface "wrong actor for this state" and exit so the human can invoke the assigned role.
7. Determine mode from `phase` + `status` + `review_target` (see mode table).
8. Surface `BATON_STATE: { phase, status, assignee: prativadi, run_mode, run_id, original_ask, research_ref, run_explainer_ref, work_split, subagent_tracks, verification_matrix, plan_ref, turn_cap, checkpoint, findings, changed_paths, verification, review_target, disagreement_round, vadi_final_approval, prativadi_final_approval, blockers, next_action }`. Passive shell wait heartbeats do not count against `turn_cap`.
9. Apply the Regular checkpoint commits rule before any baton write that follows verified file changes.

**Note on `${CLAUDE_SKILL_DIR}`:** this is the directory containing this SKILL.md file. Claude Code auto-substitutes it before the LLM sees the prompt. In Codex, resolve it from the path this SKILL.md was loaded from (for example an installed plugin cache or `plugins/dvandva/skills/prativadi`) before invoking any command that uses it.

## Superpowers runtime gate

Superpowers is a hard runtime dependency. Dvandva owns baton state, role handoff, phase gates, and cross-agent review; Superpowers owns the active-work discipline inside each prativadi turn. Before any active non-wait work, verify that the current session can invoke the relevant Superpowers skills:

- Always: `superpowers:using-superpowers` and `superpowers:verification-before-completion`.
- Spec Q&A: `superpowers:brainstorming`.
- Review work: `superpowers:requesting-code-review` or the applicable review discipline available in the installed Superpowers set.
- Narrow behavior-changing fixups: `superpowers:test-driven-development`.
- Skill review or edits: `superpowers:writing-skills`.
- Independent review tracks: `superpowers:dispatching-parallel-agents` and `superpowers:subagent-driven-development` when available.

If a required Superpowers skill is unavailable, do not continue with a weakened Dvandva workflow. If the baton exists and prativadi owns the current state, write `status: "human_decision"` with a blocker naming the missing Superpowers capability; otherwise surface setup instructions and exit without writing.

## Mode table

| baton fields | Mode |
|---|---|
| `phase: "research", status: "research_review", review_target: "research"` | Mode R — independent research review |
| `phase: "research", status: "research_drafting"` | vadi-owned research drafting; wait unless supervised |
| `phase: "research", status: "research_revision"` | vadi-owned research revision; wait unless supervised |
| `phase: "spec", status: "spec_review", review_target: "spec"` | Mode A — spec Q&A |
| `phase: 1..N, status: "parallel_implementing"` | v2 team-owned implementation; participate when `active_roles` contains prativadi |
| `phase: 1..N, status: "cross_review"` | Mode X — v2 cross-review participation |
| `phase: 1..N, status: "cross_fixing"` | v2 cross-review fixing; participate when assigned through `work_split` |
| `phase: 1..N, status: "deep_review", review_target: "implementation"` | Mode B — v2 deep review after test_creation |
| `phase: 1..N, status: "phase_review", review_target: "implementation"` | Mode B — v1-compatible implementation review |
| `status: "counter_review", review_target: "vadi_counter"` | Mode C — vadi-counter review |
| anything else with `assignee: prativadi` | exit with "unrecognized state" |

## Subagent-driven phases

All phases are subagent-driven through conditional parallelism: use parallel subagents for genuinely disjoint tracks when the harness exposes enough subagent capacity; otherwise do the track directly and record what was not parallelized and why in `subagent_tracks` and `work_split`. In short, all phases are subagent-driven, but only independent tracks are parallelized. Do not cap Dvandva at two subagents; spawn as many independent subagent tracks as the harness can safely run without file-scope conflicts or shared-state races. Codex subagent handles must be closed explicitly after their results are consumed, because completed agents can remain open and keep counting against the thread limit. Use the canonical Dvandva subagent roster in `plugins/dvandva/agents/`:

For v2 phase work, implementation-phase parallelism is mandatory. Spec approval must start `parallel_implementing` with `assignee: "team"` and `active_roles: ["vadi", "prativadi"]`; the `work_split` must contain at least five implementation chunks distributed across both roles for two-team parallel implementation, with reciprocal `cross_review_by`. After `test_creation`, both roles enter `cross_review`; only completed cross-review evidence for both roles can advance to `deep_review`.

| Phase | Default subagents |
|---|---|
| `research_review` | `dvandva-researcher`, `dvandva-deep-reviewer`, `dvandva-baton-auditor`, `dvandva-sandbox-verifier` when evidence helps |
| `spec_review` | `dvandva-architect`, `dvandva-baton-auditor` |
| `parallel_implementing` | `dvandva-implementer`, `dvandva-sandbox-verifier` |
| `cross_review` / `cross_fixing` | `dvandva-cross-reviewer`, `dvandva-baton-auditor`, `dvandva-sandbox-verifier` |
| `deep_review` / `phase_review` | `dvandva-deep-reviewer`, `dvandva-baton-auditor`, `dvandva-sandbox-verifier` |
| narrow fixups | `dvandva-implementer`, `dvandva-test-creator` if behavior changes |
| `counter_review` | `dvandva-deep-reviewer`, `dvandva-baton-auditor` |
| `deslop` review | `dvandva-deslopper`, `dvandva-baton-auditor` |

If no subagent tool is available, do the same tracks directly and record the fallback in `subagent_tracks` and `work_split`.

## Mode R — independent research review

Trigger: `phase: "research", status: "research_review", review_target: "research"`.

Actions:

1. Invoke `dvandva:research` for independent research review.
2. Re-read `original_ask`, open `research_ref`, and inspect relevant code, docs, tests, commands, `work_split`, and `verification_matrix`.
3. Use conditional parallelism when available: `dvandva-researcher`, `dvandva-deep-reviewer`, `dvandva-baton-auditor`, and `dvandva-sandbox-verifier` for claims that need runtime evidence; record each track in `subagent_tracks`.
4. Do not rely solely on the vadi's research_ref.
5. Confirm test_creation is separate from review, and that new behavior has a 100% test coverage plan or a documented source-only rationale.
6. If gaps remain, write `findings` and route to `research_revision`.
7. If research is sufficient, route to `phase: "spec", status: "spec_drafting"`, preserving `research_ref`, `run_explainer_ref`, `work_split`, `subagent_tracks`, `verification_matrix`, and `plan_ref`.
8. Install the next research baton through `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh`; the helper validates live v2 research transitions and fields.

## Mode A — spec Q&A

Trigger: `phase: "spec", status: "spec_review", review_target: "spec"`.

Actions:

1. **Capability check**: verify `superpowers:brainstorming` is available in this session. Capability check, not a filesystem path — try a no-op Skill invocation or check the `/skills` listing. If absent, follow the Superpowers runtime gate and do not approve or advance the baton.
2. Invoke `superpowers:brainstorming` as the questioner. Read the run-scoped HTML plan at `plan_ref`; reject missing, non-`.html`, generated markdown, or cross-run/shared plan refs with `human_decision`. Ask clarifying questions, surface ambiguity, propose alternatives.
3. During master planning, questions to the user are allowed and expected when the goal is under-specified, risky, or has multiple valid product directions. If a user answer is required before approving or handing back a useful plan, set `status: "human_question"`, `assignee: "human"`, `question: "<one concrete question>"`, `resume_assignee: "prativadi"`, `resume_status: "spec_review"`, keep `master_plan_locked: false`, `next_action: "Human: answer question, then invoke the prativadi skill; it will resume spec_review."`, surface BATON_STATE, and stop.
4. You may edit the HTML plan at `plan_ref` directly for narrow improvements: typos, sharper phrasing, table formatting fixes, or embedded metadata corrections. Do not restructure the plan unilaterally.
5. Substantive concerns (scope, architecture, phase boundaries, dep choices) go in `findings` for the vadi to address.
6. Decide: hand back for revision, or advance to phase 1.

If you advance:

- `phase: 1` (was "spec")
- `total_phases:` already set; do not modify
- `status: "parallel_implementing"` for v2 named runs; legacy v1 uses `"implementing"` only on the explicit legacy path
- `assignee: "team"` for v2, with `active_roles: ["vadi", "prativadi"]`; legacy v1 uses `"vadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `master_plan_locked: true`
- `question: null`
- `resume_assignee: null`
- `resume_status: null`
- `disagreement_round: 0`
- `findings: []`
- `summary: "Spec approved. Advancing to phase 1 parallel implementation. <total_phases> phases planned."`
- `next_action: "Vadi and prativadi: execute assigned parallel_implementing chunks from work_split, then route through test_creation and cross-review before deep_review."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue.

If you hand back for revision:

- `phase: "spec"` (unchanged)
- `status: "spec_revision"`
- `assignee: "vadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `master_plan_locked: false`
- `question: null`
- `resume_assignee: null`
- `resume_status: null`
- `findings: [<your Q&A items, one bullet each>]`
- `summary: "Spec needs revision. <N> findings raised."`
- `next_action: "Vadi: address findings in <plan_ref>, then hand back to prativadi for re-Q&A."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue.

Surface BATON_STATE, then follow the Stop rule.

## Mode X — cross-review participation

Trigger: `phase: 1..total_phases, status: "cross_review"` with `active_roles` containing `prativadi`.

Actions:

1. Use `dvandva-cross-reviewer` or direct review to inspect vadi-owned implementation chunks; do not review your own chunks.
2. Record prativadi's cross-review result in `subagent_tracks` with `track: "cross-review"`, `owner_role: "prativadi"`, non-empty `outputs`, and non-empty `evidence_refs`.
3. If peer-owned chunks need fixes, write `status: "cross_fixing"`, `assignee: "team"`, `active_roles: ["vadi", "prativadi"]`, and route exact findings.
4. If both vadi and prativadi cross-review tracks are completed and approved, hand the baton to `deep_review` with `assignee: "prativadi"` and `active_roles: []`.

## Mode B — phase implementation review / deep_review

Trigger: `phase: 1..total_phases, status: "phase_review", review_target: "implementation"` for legacy v1 helper compatibility, or `phase: 1..total_phases, status: "deep_review", review_target: "implementation"` for live v2 lifecycle runs after `test_creation` and `cross_review`.

Actions:

1. Read the diff vs branch baseline: `git diff <baseline>..HEAD`.
2. Confirm test_creation and cross-review happened before review. If tests or reciprocal cross-review evidence are missing for new executable behavior, treat it as a handback issue unless the `verification_matrix` documents source-only rationale.
3. Cross-check the vadi's `verification` block and `verification_matrix`. Did the listed commands actually pass? Do they cover the changed paths, risks, and 100% test coverage target?
4. Use conditional parallelism for evidence-backed checks. In `deep_review`, dispatch or directly run at least three angle-specific reviewers/tracks before approving: `correctness-regression`, `test-evidence`, and `protocol-handoff`. Add more tracks when independent scope exists, such as documentation/deslop, security, or runtime verification.
5. Record those review tracks in `subagent_tracks`; the v2 write helper rejects `deep_review -> deslop` without the three completed angle-specific reviewers.
6. Look for: bugs, regressions, stale docs, missing tests, claims not matching the diff, and deslop opportunities.
7. Categorize issues as blocker/bug, low/minor issue, nit/deslop, narrow-fixup-eligible, or handback-required.

### Narrow-fix allowlist

You MAY directly fix:

- Typographical and docs mistakes
- Stale references in docs or audit rows
- Small test expectation updates
- Lint, formatting, or type errors with obvious fixes
- Small missed edge cases that do not change architecture

### Handback conditions

You MUST hand back (not fix) for:

- Product behavior changes
- Architecture changes
- Schema migrations
- Shared infra changes
- Dependency removals or major dependency additions
- Ambiguous requirements

### Decision branching

**If only handback issues:**

- `phase: <current N>` (unchanged)
- `status: "phase_fixing"`
- `assignee: "vadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `findings: [<one bullet per substantive issue>]`
- `summary: "Phase <N> needs implementation work before re-review."`
- `next_action: "Vadi: address findings, then hand back to prativadi for re-review."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue. Surface BATON_STATE, then follow the Stop rule.

**If narrow fixups apply AND no handback issues:** apply the fixups inline (edit the affected files), re-run verification, then:

- `phase: <current N>` (unchanged)
- `status: "review_of_review"`
- `assignee: "vadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: "prativadi_fixups"`
- `narrow_fixups: [<one bullet per fix you applied>]`
- `changed_paths: [<run-level union of intended files touched so far>]`
- `verification: [<post-fixup commands and results>]`
- `summary: "Phase <N> reviewed. Applied <N> narrow fixups. Mutual review owed."`
- `next_action: "Vadi: review prativadi's narrow fixups for phase <N>. Approve to advance, or counter."`
- If `<current N> == total_phases`, set `prativadi_final_approval: true`; the vadi must review the final fixups before commit/push.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue. Surface BATON_STATE, then follow the Stop rule.

**If narrow fixups apply AND handback issues:** apply the narrow fixups inline first (edit affected files), re-run verification, then route to `phase_fixing` for the vadi to address handback issues. Mutual review of the narrow fixups happens on the next prativadi pass after the vadi's fix.

- `phase: <current N>` (unchanged)
- `status: "phase_fixing"`
- `assignee: "vadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `findings: [<one bullet per substantive handback issue>]`
- `narrow_fixups: [<one bullet per narrow fix you applied — carry these forward so mutual review fires after the vadi's fix>]`
- `changed_paths: [<run-level union of intended files touched so far>]`
- `verification: [<post-fixup commands and results>]`
- `summary: "Phase <N> has handback issues; <N> narrow fixups applied inline. Routing to fix first; mutual review of the fixups deferred to the next prativadi pass."`
- `next_action: "Vadi: address findings. After re-implementation, prativadi will also review the narrow fixups already applied."`
- If `<current N> == total_phases`, keep `prativadi_final_approval: false`; final approval is not valid while handback findings remain.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue. Surface BATON_STATE, then follow the Stop rule.

**If approve with no changes:**

First check the incoming baton's `narrow_fixups` array. If it is **non-empty**, that means an earlier Mode B pass applied fixups during a "fixups + handback" branch and the mutual review for those fixups is still owed — the vadi only addressed the handback findings, not the fixups. In that case, do NOT advance the phase; route to mutual review instead:

- `phase: <current N>` (unchanged)
- `status: "review_of_review"`
- `assignee: "vadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: "prativadi_fixups"`
- `narrow_fixups: [<existing array, carried forward unchanged>]`
- `summary: "Phase <N> handback addressed by vadi. Mutual review of carried-forward narrow fixups now owed."`
- `next_action: "Vadi: review prativadi's narrow fixups for phase <N> (carried forward from the earlier review pass). Approve to advance, or counter."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue. Surface BATON_STATE, then follow the Stop rule.

Otherwise (incoming `narrow_fixups` is empty — normal happy-path approval), route through deslop before advancement when v2 states are available. The deslop pass is mandatory until nits, low/minor bugs, stale wording, unclear instructions, and generated-looking residue are fixed or explicitly accepted in `deferred`.

- `phase: <N+1>` (advance) or `phase: <current N>, status: "done"` if N was final
- `status: "implementing"` on advance, or `"done"` on terminal. In v2, prefer `status: "deslop"` before either path.
- `assignee: "vadi"` on advance, or `"human"` on terminal observer
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `disagreement_round: 0` (both paths — reset cleanly whether advancing or terminating)
- `summary: "Phase <N> approved with no changes. Advancing."` or `"Phase <N> was final. Marking done."`
- `next_action: "Vadi: implement phase <N+1>."` or `"Run complete. Inspect final_commit and pushed_ref; no PR was created."`
- If `<current N> == total_phases`, set `prativadi_final_approval: true`. If `vadi_final_approval == true`, follow the Final ship rule before writing terminal `done`; otherwise set `status: "human_decision"` because the final diff lacks vadi approval.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue. Surface BATON_STATE, then follow the Stop rule.

## Mode C — vadi-counter review

Trigger: `status: "counter_review", review_target: "vadi_counter", assignee: "prativadi"`.

This is the mutual-review disagreement step. The vadi disapproved your earlier narrow fixup and wrote a counter-change. Decide whether the counter is correct.

Actions:

1. Read the baton's `vadi_counter` array — the vadi's bullet list of what they changed and why.
2. Inspect the actual diff the vadi applied since the previous checkpoint.
3. Cross-check: does the counter address the original issue your fixup was trying to fix? Or did the vadi introduce a different problem?
4. Decide: approve or disapprove.

If you approve:

- `phase: <N+1>` (advance) or `phase: <current N>, status: "done"` if N was final
- `status: "implementing"` on advance, or `"done"` on terminal
- `assignee: "vadi"` on advance, or `"human"` on terminal
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `disagreement_round: 0` (both paths — reset cleanly whether advancing or terminating)
- `summary: "Approved vadi's counter-change for phase <N>. Advancing to phase <N+1>."` or `"...Phase <N> was final."`
- `next_action: "Vadi: implement phase <N+1>."` or `"Run complete. Inspect final_commit and pushed_ref; no PR was created."`
- If `<current N> == total_phases`, set `prativadi_final_approval: true`. If `vadi_final_approval == true`, follow the Final ship rule before writing terminal `done`; otherwise set `status: "human_decision"` because the final diff lacks vadi approval.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue. Surface BATON_STATE, then follow the Stop rule.

If you disapprove:

1. Increment `disagreement_round` by 1.
2. If `disagreement_round >= disagreement_cap` (default 3):
   - `status: "human_decision"`
   - `assignee: "human"`
   - `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
   - `blockers: ["mutual review reached cap without agreement"]`
   - `next_action: "Human: decide between prativadi's fixup, the vadi's counter, or a third path. Edit baton.assignee to resume."`
   - Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
   - Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on human_decision). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue. Surface BATON_STATE, then follow the Stop rule.
3. Otherwise, write a new narrow fixup (edit the affected files):
   - `phase: <current N>` (unchanged)
   - `status: "review_of_review"`
   - `assignee: "vadi"`
   - `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
   - `review_target: "prativadi_fixups"`
   - `narrow_fixups: [<your new fix description>]`
   - `disagreement_round: <incremented>`
   - `summary: "Disapproved vadi's counter; wrote a different fix. Round <X>."`
   - `next_action: "Vadi: review prativadi's new fixup. Approve to advance, or counter again."`
   - Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
   - Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue. Surface BATON_STATE, then follow the Stop rule.

## Regular checkpoint commits

After any active mode applies narrow fixups or counter-fixups and the relevant
verification commands pass, make a local checkpoint commit when
`allow_commit == true`.

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

In `run_mode: "walkaway"`, do not exit merely because the baton assigns work to vadi. After writing any baton assigned away from prativadi:

1. Surface the new BATON_STATE line.
2. Immediately run a foreground wait against the resolved `"$BATON_FILE"`. Claude Code uses finite waits: `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE" --interval 60 --max-wait 540` with an explicit 600000 ms Bash-tool timeout; on exit 20, surface the heartbeat and run it again unless interrupted. Codex-hosted sessions may use `--persist`: `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE" --interval 60 --max-wait 540 --persist`; add `--persist-max <600` when running inside a Claude-hosted shell. The wait-helper persist cap exit 23 is a persistent wait cap, not a terminal baton state.
3. Continue from Preflight when the wait returns 0.

Do not end the turn after an assigned-away BATON_STATE line. The next action is the foreground wait helper, not a final response to the user.

Stop only when the wait reports `done`, `human_question`, or `human_decision`, or when the user interrupts. This is shell polling, not LLM polling: do not spend model turns checking whether vadi has moved.

In `run_mode: "supervised"`, exit after surfacing any baton assigned away from prativadi. The human manually invokes the assigned role.

## `/goal` condition (paste into your engine when launching)

```
/goal You are Dvandva prativadi. Resolve the active Dvandva baton before every read: DVANDVA_BATON_FILE, else DVANDVA_RUN_DIR/baton.json, else safe DVANDVA_RUN_ID as .dvandva/runs/<run_id>/baton.json, else scan .dvandva/runs/*/baton.json and legacy .dvandva/baton.json, selecting the single active run or waiting for vadi/human selection. Continue the walkaway run until the resolved Dvandva baton status is "done", "human_question", or "human_decision". If assignee is not "prativadi", wait on the resolved baton with ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE" --interval 60 --max-wait 540; Claude uses finite wait re-loops, while Codex may add --persist when the shell budget supports it. Invoke `dvandva:research` during research_review for independent research review; use conditional parallelism in every phase: parallelize only genuinely disjoint tracks, never assume a two-subagent ceiling, and record what was not parallelized and why in subagent_tracks. Keep test_creation separate from deep_review, require 100% test coverage evidence for new behavior, require at least three angle-specific deep-review reviewers before deslop, route deslop findings until only explicitly deferred nits remain, and make regular local checkpoint commits after verified fixup slices when allow_commit permits. Before terminal done, verify ./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer.html exists and run_explainer_ref is set. Before each checkpoint, surface BATON_STATE including DVANDVA_RUN_ID/run_id, original_ask, research_ref, run_explainer_ref, work_split, subagent_tracks, verification_matrix, plan_ref, turn_cap, findings, verification commands and outcomes, final approval fields, and the final baton contents; do not count shell wait heartbeats as turns. Never create a PR. Stop after turn_cap active model-work turns and assign human if still blocked.
```

## Failure modes

| Failure | What to do |
|---|---|
| `$BATON_FILE` malformed JSON | Do not overwrite. Write `$BATON_BROKEN_FILE` preserving bytes. Surface parse error. Set in-memory next state to `human_decision`. |
| `schema` field is not `dvandva.baton.v1` or `dvandva.baton.v2` | Refuse to operate. Surface schema mismatch. Exit. |
| `assignee` is not `prativadi` | In `run_mode: "walkaway"`, wait with `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE"` using the engine-specific wait rule; otherwise surface "wrong actor for this state" and exit. |
| `status` is `human_question` | Surface `question`, `resume_assignee`, and `resume_status`. If the user answered, restore those resume fields, clear question fields, and continue. |
| Required Superpowers skill unavailable | Surface install hint: `codex plugin marketplace` or upstream symlink install per https://deepwiki.com/obra/superpowers/2.4-installing-on-codex. Do not continue with a weakened Dvandva workflow; if prativadi owns the current baton state, route to `human_decision` with the missing capability in `blockers`. |
| `plan_ref` missing, non-HTML, or referenced file does not exist during phase mode | Surface "spec phase did not complete; cannot review phase implementation". Set `status: "human_decision"`. Exit. |
| `total_phases` is 0 or unset during phase mode | Surface schema integrity error. Set `status: "human_decision"`. Exit. |
| Prativadi finds no diff vs baseline after vadi said phase implementation done | Write `findings: ["vadi claimed implementation but produced no diff"]`. Set `status: "human_decision"`. |
| Agent wrote a baton assigned away from prativadi in `run_mode: "walkaway"` but ended the turn without running the wait helper | Handoff stalled. Recovery: re-invoke this skill; preflight resumes from the current baton. Before any further text-to-user, run the wait helper unless the baton is now assigned to prativadi or is terminal. |
| `/goal` turn cap hit before exit condition | Surface current baton state. Set `status: "human_decision"`. Passive shell wait heartbeats do not count against this active-work cap. Exit. |
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
  "status": "spec_review",
  "assignee": "prativadi",
  "current_engine": null,
  "review_target": "spec",
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
