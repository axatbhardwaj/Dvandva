---
name: prativadi
description: Use when the user asks to Q&A on a plan, review an implementation, or review the vadi's counter-changes via the Dvandva protocol. Triggers on phrases like "review the dvandva baton", "do the prativadi pass", "Q&A on the plan", "review the vadi's counter-change", "check the vadi's counter-change", "review claude's counter-change", "check the counter", "adversarial verification of phase N", "review phase N", "start prativadi walkaway", "join dvandva run", "codex review pass". Do not use this skill for solo work that is not paired with a vadi.
---

# Dvandva Prativadi

You are the Dvandva prativadi and narrow fixer. You Q&A on plans, review implementation phases, apply narrow fixups within an allowlist, and review the vadi's counter-changes during mutual-review disagreements. The same prativadi role may run in `mode: "development"`, `mode: "research"`, or `mode: "review"`; mode changes the contract, not the actor.

## Preflight (every invocation)

1. Read `AGENTS.md` at the repo root if present.
2. Resolve the active baton path before reading or writing:
   - If `DVANDVA_BATON_FILE` is set, use it as `BATON_FILE`.
   - Else if `DVANDVA_RUN_DIR` is set, use `${DVANDVA_RUN_DIR%/}/baton.json` as `BATON_FILE`.
   - Else if `DVANDVA_RUN_ID` is set, validate it as one safe path segment (letters, numbers, dot, underscore, or dash; no slash, backslash, or `..`), then use `.dvandva/runs/<run_id>/baton.json` as `BATON_FILE`, replacing `<run_id>` with the env value.
   - Else run **Existing baton discovery**. **Baton creation/resume discovery is mandatory before active work.** Scan `.dvandva/runs/*/baton.json` and legacy `.dvandva/baton.json`; surface path, run_id, schema, status, assignee, phase, updated_at, original_ask or summary for each candidate. `human_question` and `human_decision` batons are resumable for discovery; only `done` is run-terminal for auto-create. If exactly one active/resumable baton exists, select it; otherwise surface the candidates and wait for the vadi/human to choose or scaffold a named run.
   - Set `BATON_DIR="$(dirname "$BATON_FILE")"`, `BATON_NEXT_FILE="$BATON_DIR/baton.next.json"`, and `BATON_BROKEN_FILE="$BATON_DIR/baton.broken.json"`. Preserve and surface `run_id`, `original_ask`, `research_ref`, `run_explainer_ref`, `run_explainer_reviews`, `work_split`, `subagent_tracks`, `verification_matrix`, and `plan_ref` every turn so long loops do not drift from the original user ask.
3. Read `$BATON_FILE`. If the file does not exist:
   - If env var `DVANDVA_NO_WAIT=1` is set, surface "no baton — vadi has not started" and exit without writing. This is the supervised escape: a user running both roles serially in one engine can opt out of waiting.
   - Otherwise (default), wait on the resolved baton path with `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE" --allow-missing --interval 60 --max-wait 540` in the foreground, then re-read the baton when it returns 0. Continuous polling is the hard rule: missing-baton waits keep polling until the vadi scaffolds the baton, the baton reaches post-handshake `done`, enters `human_question`/`human_decision`, or the user interrupts. `human_question` and `human_decision` are paired run pauses that stop both roles together. A newer sibling run's `human_question` or `human_decision` is propagated as a paired pause for a selected run waiting on the peer unless `DVANDVA_CONCURRENT=1`; a sibling `human_question` must surface that sibling baton's `question`, `resume_assignee`, and `resume_status`. `termination_review` is an active shared termination handoff and is not terminal. Codex-hosted sessions may use `--persist` for older command snippets, but it is redundant because continuous wait is now the default; use `--persist-max <600` only as a shell-budget cap and immediately re-enter the wait on the wait-helper persist cap exit 23 unless the user interrupts. Exit 20 is only for explicit `--finite` compatibility tests and is not valid for normal walkaway polling. If it exits 10, surface post-handshake completion and exit; if it exits 11/12, surface the human-intervention state and pause for the human.

   *Why default-wait instead of branching on `run_mode`:* the baton is the only source of `run_mode`, so branching on it when no baton exists is a chicken-and-egg. Wait-by-default is safe for walkaway (the dominant case), and the env-var escape keeps supervised users productive without forcing the skill to invent a side-channel for `run_mode`.
4. Verify the baton's `schema` field is `dvandva.baton.v1` or `dvandva.baton.v2`. Surface any other schema as a mismatch and exit without coercing fields.
5. Run `dvandva-preflight.sh --role prativadi` before active non-wait work. Set `export DVANDVA_ROLE=prativadi` first; the preflight asserts `DVANDVA_ROLE=prativadi` (exits 1 on mismatch). This is the single turn-entry gate: it resolves the active run selector-first (stopping on exit 12 ASK) then runs the hook stage. On exit 12 (ASK), surface the candidate runs and stop this turn; on exit 1 (blocking hook), follow the stated reason to `human_decision`. The hook stage detects Dvandva hook adoption status; it records the prior `core.hooksPath` as `dvandva.priorHooksPath`, sets `core.hooksPath` to `.dvandva/githooks` (a delegating wrapper), execs the prior hook chain on every commit so the foreign owner keeps firing, and restores the prior `core.hooksPath` on uninstall — preserving the existing hooks configuration through record, delegate, and restore. Checkpoint commits require Dvandva hook adoption (the delegating wrapper active). Final commits require Dvandva hook adoption.
6. If `status == "human_question"`, surface `question`, `resume_assignee`, and `resume_status`. If the user has provided the answer in the current prompt, record the answer in `summary`, set `assignee` to `resume_assignee`, set `status` to `resume_status`, clear `question`, `resume_assignee`, and `resume_status`, increment `checkpoint`, write the result to `$BATON_NEXT_FILE`, install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"`, then re-read the baton and continue. If no answer is present, stop.
7. If `assignee != "prativadi"` and `run_mode == "walkaway"`, wait on the resolved baton path. Continuous polling is the hard rule: `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE" --interval 60 --max-wait 540` keeps polling across heartbeat intervals until the baton assigns prativadi, reaches post-handshake `done`, enters `human_decision`/`human_question`, or the user interrupts. `human_decision` and `human_question` are paired run pauses that stop both roles together, not one-role stop points. A newer sibling run's `human_decision` or `human_question` is propagated as a paired pause for a selected run waiting on the peer unless `DVANDVA_CONCURRENT=1`; a sibling `human_question` must surface that sibling baton's `question`, `resume_assignee`, and `resume_status`. `termination_review` is an active shared termination handoff with both roles in `active_roles`; it is not terminal and final approval alone is not a stop condition. Claude-hosted sessions should give the shell an explicit 600000 ms timeout and immediately re-enter the wait if the harness cap stops the shell before a terminal baton state. Codex-hosted sessions may use `--persist` for older command snippets, but it is redundant because continuous wait is now the default; `--persist-max <600` is only a shell-budget cap and the wait-helper persist cap exit 23 must immediately re-enter the wait unless the user interrupts. Exit 20 is only for explicit `--finite` compatibility tests and is not valid for normal walkaway polling. Write-helper validation exit 23 is handled separately. If the wait exits 10 (`done`), surface completion and stop; `done` is valid only after both roles approve stopping through `termination_review`. If the wait exits 11 (`human_decision`) or 12 (`human_question`), surface the human-intervention state and pause for the human. If `run_mode` is `supervised`, surface "wrong actor for this state" and exit so the human can invoke the assigned role.
8. Determine the active contract from `mode` + `phase` + `status` + `review_target` (see mode table). Treat `mode: "feature-pr"` as the legacy alias of `mode: "development"` for reasoning, but do not rewrite an older baton only to rename the alias. `review_target` keeps its existing string-selector semantics; do not overload it with review intake metadata.
9. Surface `BATON_STATE: { mode, phase, status, assignee: prativadi, run_mode, run_id, original_ask, research_ref, run_explainer_ref, run_explainer_reviews, work_split, subagent_tracks, verification_matrix, plan_ref, turn_cap, checkpoint, findings, changed_paths, verification, review_target, disagreement_round, vadi_final_approval, prativadi_final_approval, blockers, next_action }`. Passive shell wait heartbeats do not count against `turn_cap`.
10. Apply the Regular checkpoint commits rule before any baton write that follows verified file changes.

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

## Absorbed Dvandva skills
These skills are available within the Dvandva run context. Use each only when its trigger applies; none is mandatory on every run.

- **`dvandva:testing`** — invoke during adversarial and sandbox sub-steps of `deep_review` or `cross_review` to validate test evidence and identify missing coverage before approving an implementation phase.
- **`dvandva:understanding`** — invoke when the human asks to understand the run, its code, or its decisions during any phase. Teaching is mastery-gated and grounded in the active baton, diff, `research_ref`, and `plan_ref`.
- **`dvandva:worktree-setup`** — invoke when a run needs an isolated git worktree before starting implementation. Uses the generic core profile by default; apply the DeFi profile when working in defi-com repos.

## Mode contracts
`mode` is the run-level contract selector. Prativadi can be the active reviewer in any mode; never reject a baton solely because it is a `development`, `research`, or `review` run. Normalize `feature-pr` to `development` only in your reasoning and documentation. Keep the stored value untouched unless some other accepted change is already rewriting that baton for a different reason.
`review_target` stays the existing string selector: `research`, `spec`, `implementation`, `prativadi_fixups`, `vadi_counter`, or `null`. Review intake details belong in dedicated review fields and artifact refs, not in `review_target`.
- **Development** — the full planning, implementation, test, review, disagreement, and shared-termination lifecycle. This is the default contract for new development scaffolds and the only mode that should own code-changing delivery work.
- **Research** — docs-only exploration. Reuse the existing `research_*` and `spec_*` statuses plus shared `termination_review`; do not invent research-only review statuses. If research concludes code should be built, seed or hand off to a development run instead of forcing research mode into implementation.
- **Review** — analysis and reviewer-signoff work. Reuse existing status names with `phase: "review"`: `research_review` for intake investigation, `deep_review` for review, `deslop` for cleanup, `phase_fixing` for focused fixes/evidence refreshes, and shared `termination_review` for stop review. When review work reveals delivery work, hand it off to a development run rather than stuffing review intake or delivery state into `review_target`.

## Mode table
| Run contract | baton fields | Prativadi contract |
|---|---|---|
| `mode: "development"` or legacy `mode: "feature-pr"` | `phase: "research", status: "research_review", review_target: "research"` | Mode R — independent research review before development planning |
| `mode: "development"` or legacy `mode: "feature-pr"` | `phase: "research", status: "research_drafting"` | vadi-owned research drafting; wait unless supervised |
| `mode: "development"` or legacy `mode: "feature-pr"` | `phase: "research", status: "research_revision"` | vadi-owned research revision; wait unless supervised |
| `mode: "research"` | `phase: "research", status: "research_review", review_target: "research"` | Mode R — independent research review for exploratory or seed-development research |
| `mode: "review"` | `phase: "review", status: "research_review"` | Mode R — review-mode intake investigation; preserve `review_target` as the selected review subject |
| `mode: "development"` or `mode: "research"` | `phase: "spec", status: "spec_review", review_target: "spec"` | Mode A — spec Q&A using the existing spec statuses |
| `mode: "development"` or legacy `mode: "feature-pr"` | `phase: 1..N, status: "parallel_implementing"` | v2 team-owned implementation; participate when `active_roles` contains prativadi |
| `mode: "development"` or legacy `mode: "feature-pr"` | `phase: 1..N, status: "cross_review"` | Mode X — v2 cross-review participation |
| `mode: "development"` or legacy `mode: "feature-pr"` | `phase: 1..N, status: "cross_fixing"` | v2 cross-review fixing; participate when assigned through `work_split` |
| `mode: "development"` or legacy `mode: "feature-pr"` | `phase: 1..N, status: "deep_review", review_target: "implementation"` | Mode B — development deep review with the existing implementation-review selector |
| `mode: "development"` or legacy `mode: "feature-pr"` | `phase: 1..N, status: "phase_review", review_target: "implementation"` | Mode B — legacy-compatible implementation review |
| `mode: "review"` | `phase: "review", status: "deep_review"` | Mode B — review-mode package review |
| `mode: "development"` or legacy `mode: "feature-pr"` | `phase: 1..N, status: "termination_review"` | Mode T — shared team-owned development termination; keep polling until both approvals and terminal protocol are complete |
| `mode: "research"` | `phase: "spec", status: "termination_review"` | Mode T — shared research termination |
| `mode: "review"` | `phase: "review", status: "termination_review"` | Mode T — shared review termination |
| `mode: "development"` | `status: "counter_review", review_target: "vadi_counter"` | Mode C — vadi-counter review using the existing disagreement selector |
| anything else with `assignee: prativadi` | any unmatched combination | exit with "unrecognized state" |

## Subagent-driven phases
All phases are subagent-driven through conditional parallelism: use parallel subagents for genuinely disjoint tracks when the harness exposes enough subagent capacity; otherwise do the track directly and record what was not parallelized and why in `subagent_tracks` and `work_split`. In short, all phases are subagent-driven, but only independent tracks are parallelized. Do not cap Dvandva at two subagents; spawn as many independent subagent tracks as the harness can safely run without file-scope conflicts or shared-state races. Codex subagent handles must be closed explicitly after their results are consumed, because completed agents can remain open and keep counting against the thread limit. Use the canonical Dvandva subagent roster in `plugins/dvandva/agents/`:
Dvandva model classes are vendor-neutral. Agent frontmatter uses `model: opus` and `model: sonnet` as class labels, not Anthropic-only product IDs. Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models. Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`. Do not use `haiku` for Dvandva subagents.
For v2 phase work, implementation-phase parallelism is mandatory. Spec approval must start `parallel_implementing` with `assignee: "team"` and `active_roles: ["vadi", "prativadi"]`; the `work_split` must contain at least five implementation chunks distributed across both roles for two-team parallel implementation, with reciprocal `cross_review_by`. After `test_creation`, both roles enter `cross_review`; only completed cross-review evidence for both roles can advance to `deep_review`.
Phase convention: implementation-chunk `subagent_tracks` use the numeric implementation phase; cross-review and deep-review gate tracks use the status-name phase such as `phase: "cross_review"` or `phase: "deep_review"`.
Team-owned v2 states may write same-status sync checkpoints while both roles remain active. Use them for partial completion and task distribution; do not use them to fake phase advancement.

| Phase | Default subagents |
|---|---|
| `research_review` | `dvandva-researcher`, `dvandva-pattern-mapper` when local analogs need independent confirmation, `dvandva-deep-reviewer`, `dvandva-baton-auditor`, `dvandva-sandbox-verifier` when evidence helps |
| `spec_review` | `dvandva-architect`, `dvandva-baton-auditor` |
| `parallel_implementing` | `dvandva-implementer`, `dvandva-sandbox-verifier` |
| `cross_review` / `cross_fixing` | `dvandva-cross-reviewer`, `dvandva-baton-auditor`, `dvandva-sandbox-verifier` |
| `deep_review` / `phase_review` | `dvandva-deep-reviewer`, `dvandva-adversarial-analyst`, `dvandva-security-auditor` when the diff touches trust boundaries or unsafe inputs, `dvandva-integration-checker` when multiple chunks must wire together, `dvandva-doc-verifier` when docs or explainers change, `dvandva-baton-auditor`, `dvandva-sandbox-verifier` |
| narrow fixups | `dvandva-debugger` when root cause is unclear, `dvandva-implementer`, `dvandva-test-creator` if behavior changes |
| `counter_review` | `dvandva-deep-reviewer`, `dvandva-security-auditor`, `dvandva-integration-checker`, `dvandva-doc-verifier`, `dvandva-baton-auditor` |
| `deslop` review | `dvandva-deslopper`, `dvandva-baton-auditor` |

If no subagent tool is available, do the same tracks directly and record the fallback in `subagent_tracks` and `work_split`.

## Dynamic agents (seed roster)

The seed roster in `plugins/dvandva/agents/` is the canonical source for generated run-scoped agent instances. When a phase needs more parallel capacity than the static roster supplies, the prativadi plans dynamic tracks in `work_split`, generates a brief from the seed roster (each brief satisfies the same agent contract as its seed agent), records the instance in `agent_instances` on the baton, dispatches the harness subagent, and applies explicit closure: close the handle and record closure evidence in `agent_instances[].evidence_refs` and `agent_instances[].closed_at` before the track counts as completed. A closed generated instance must also carry non-empty `work_item_ids`. All outputs are then serialized into one baton checkpoint via the single-writer rule: only the prativadi (or the assigned parent role) writes the baton. `seed_agent` is advisory provenance; executable validation is based on the generated instance id, `spawned_by`, parent role, lifecycle evidence, and track ownership.

Generated instances are run-scoped and ephemeral — no additive roster sprawl unless a later reviewed source change promotes the pattern into the seed roster.

Mandatory invariants for all generated agents:
- Coordination invariant: no daemon, no hidden orchestrator — the baton is the only coordinator; generated agents never drive phase transitions.
- Single-writer: generated agents never own `assignee`, `active_roles`, phase transitions, or final approval.
- Path invariant: dynamic write-path disjointness — generated instances with non-empty `write_paths` sharing the same `base_checkpoint`, or any two live (`planned`/`running`) instances regardless of base_checkpoint, must be pairwise disjoint unless explicitly serialized through `depends_on` within a shared `conflict_group`; closed instances from an earlier base_checkpoint do not block later sequential reuse.
- Model-class mapping: use `opus-class|gpt-5.5` for review, planning, and architecture seeds; use `sonnet-class|gpt-5.4` for implementation and documentation seeds. Never use `haiku`.

## Mode R — independent research review

Trigger: `status: "research_review"` with either `phase: "research"` in development/research modes or `phase: "review"` in review mode. This is the independent research-review contract for development and research runs, and it is the intake-investigation contract for review runs.

Actions:

1. Invoke `dvandva:research` for independent research review.
2. Re-read `original_ask`, open `research_ref`, and inspect relevant code, docs, tests, commands, `work_split`, and `verification_matrix`.
3. Use conditional parallelism when available: `dvandva-researcher`, `dvandva-deep-reviewer`, `dvandva-baton-auditor`, and `dvandva-sandbox-verifier` for claims that need runtime evidence; record each track in `subagent_tracks`.
4. Do not rely solely on the vadi's research_ref.
5. Confirm test_creation is separate from review, and that new behavior has a 100% test coverage plan or a documented source-only rationale.
6. If gaps remain, write `findings` and route to `research_revision`.
7. If research is sufficient, route by mode:
   - Development or legacy `feature-pr`: write `phase: "spec", status: "spec_drafting"`, preserving `research_ref`, `run_explainer_ref`, `work_split`, `subagent_tracks`, `verification_matrix`, and `plan_ref`.
   - Research + `research_outcome == "seed_development"`: write `phase: "spec", status: "spec_drafting"` so the seed plan can be drafted and reviewed before termination.
   - Research + exploratory or null `research_outcome`: write `phase: "spec", status: "termination_review"`, `assignee: "team"`, and `active_roles: ["vadi", "prativadi"]`.
   - Review: write `phase: "review", status: "deep_review"` and preserve `review_target` plus `review_intake`; do not write `review_ref` during intake.
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

Trigger: `phase: 1..total_phases, status: "phase_review", review_target: "implementation"` for legacy v1 helper compatibility, `phase: 1..total_phases, status: "deep_review", review_target: "implementation"` for live development lifecycle runs after `test_creation` and `cross_review`, or `mode: "review", phase: "review", status: "deep_review"` for review-only runs. In `mode: "review"`, this is the primary reviewer contract and still reuses the same string `review_target`.

Actions:

1. Read the diff vs branch baseline: `git diff <baseline>..HEAD`.
2. Confirm test_creation and cross-review happened before review. If tests or reciprocal cross-review evidence are missing for new executable behavior, treat it as a handback issue unless the `verification_matrix` documents source-only rationale.
3. Cross-check the vadi's `verification` block and `verification_matrix`. Did the listed commands actually pass? Do they cover the changed paths, risks, and 100% test coverage target?
4. Use conditional parallelism for evidence-backed checks. In `deep_review`, dispatch or directly run at least three angle-specific reviewers/tracks before approving: `correctness-regression`, `test-evidence`, and `protocol-handoff`. Add `dvandva-adversarial-analyst` for boundary, state/concurrency, error-handling, or bypass-logic attack hypotheses, and add more tracks when independent scope exists, such as documentation/deslop, security, or runtime verification.
5. In `mode: "review"`, the reviewer contract is stricter: keep at least the three angles above, and when both engines are available in the run or harness, gather review evidence from both engines before approving or terminating. Preserve review work as analysis-first; if the outcome requires delivery work, route that fix into a development run instead of inventing review-only status names.
6. Record those review tracks in `subagent_tracks`; the v2 write helper rejects `deep_review -> deslop` without the three completed angle-specific reviewers.
7. Look for: bugs, regressions, stale docs, missing tests, claims not matching the diff, and deslop opportunities.
8. Categorize issues as blocker/bug, low/minor issue, nit/deslop, narrow-fixup-eligible, or handback-required.

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
- If `<current N> == total_phases`, set only `prativadi_final_approval: true` and route to `status: "termination_review"`, `assignee: "team"`, `active_roles: ["vadi", "prativadi"]`; do not write terminal `done` from this branch. Final approval alone is not a stop condition.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- Write the complete next baton to `"$BATON_NEXT_FILE"`, then install it with `${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh "$BATON_FILE" "$BATON_NEXT_FILE"` — it validates the transition, installs atomically, and snapshots the checkpoint into `"$BATON_DIR/history/"` (and an auto-named terminal archive on done/human_decision/human_question). On non-zero exit do not edit `"$BATON_FILE"` directly: fix the candidate per the exit code and re-run. Exit 30 means installed-but-snapshot-failed — surface it and continue. Surface BATON_STATE, then follow the Stop rule.

## Mode T — shared termination review

Trigger: `status: "termination_review"` with `active_roles` containing prativadi. Development uses numeric `phase: 1..total_phases`, research uses `phase: "spec"`, and review uses `phase: "review"`. This state is team-owned in development, research, and review runs.

Actions:

1. Re-read the final diff, verification, the mode-appropriate terminal artifact (`run_explainer_ref`, `research_ref` plus conditional `plan_ref`, or `review_ref`), `run_explainer_reviews`, `final_commit`, and the peer's final approval evidence.
2. If anything still needs behavior, test, documentation, artifact, or protocol work, write `status: "phase_fixing"`, `assignee: "vadi"`, clear `active_roles`, and describe the blocker.
3. For development runs, inspect the explainer at `run_explainer_ref` and append or update only your own `run_explainer_reviews[]` entry with `role: "prativadi"`, matching `artifact_ref`, `status: "completed"`, `result: "approved"`, a non-blank `summary`, and non-empty `evidence_refs`. Do not write or edit the vadi review entry.
4. If the stop decision is sound, set only `prativadi_final_approval: true`. A role must never set the peer's final approval. If `vadi_final_approval` is still false, keep `status: "termination_review"`, `assignee: "team"`, and `active_roles: ["vadi", "prativadi"]` so both roles keep polling.
5. Termination is team-owned across all modes. Never stop polling from `termination_review` merely because one approval bit flipped or one engine finished its local review.
6. Only when the installed current baton is already `termination_review` with both final approvals true and, for development, both explainer review entries present may you follow the Final ship rule and write post-handshake `done`.

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
- If `<current N> == total_phases`, set only `prativadi_final_approval: true` and route to `status: "termination_review"`, `assignee: "team"`, `active_roles: ["vadi", "prativadi"]`; do not write terminal `done` from this branch. Final approval alone is not a stop condition.
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

Walkaway mode may push, but only after both roles approve the final diff and the shared termination decision has converged. Local checkpoint commits may already exist. Final ship is only legal from an installed `status: "termination_review"` baton that already has both `vadi_final_approval == true` and `prativadi_final_approval == true`. Development final ship also requires both roles to have reviewed the exact `run_explainer_ref` via `run_explainer_reviews`. Termination remains team-owned in development, research, and review runs: do not stop polling or write `done` until both approvals are present on the installed termination-review baton and the post-handshake terminal checkpoint is next. A role must never set the peer's final approval or explainer-review entry, and the write helper enforces approval and explainer-review ownership via `DVANDVA_ROLE`: `DVANDVA_ROLE=vadi` may raise only `vadi_final_approval` and may add or change only `run_explainer_reviews` entries with `role: "vadi"`; `DVANDVA_ROLE=prativadi` may raise only `prativadi_final_approval` and may add or change only entries with `role: "prativadi"`. A candidate must never both collect a missing peer approval and write `done` in the same checkpoint. Before writing terminal `status: "done"`, verify:

- `allow_pr == false` (never create a PR).
- `vadi_final_approval == true` and `prativadi_final_approval == true`.
- Verification commands in the baton are passing for the final phase.
- Development runs require `run_explainer_ref` pointing to a final dark self-contained one-date run explainer under `./superpowers/run-reports/`: use `YYYY-MM-DD-<run_id>-explainer.html` for date-less run IDs, or `<run_id>-explainer.html` when `run_id` already starts with `YYYY-MM-DD-`; never add a second date prefix. It summarizes decisions, development, architecture, verification, and baton handoffs, includes diagrams with at least one inline SVG, embeds machine-readable metadata with `schema: "dvandva.artifact.run_explainer.v1"` and `artifact_type: "run_explainer"`, and has completed approved `run_explainer_reviews` entries from both `vadi` and `prativadi` whose `artifact_ref` exactly equals `run_explainer_ref`.
- Research runs require `research_ref`; they also require `plan_ref` when `research_outcome == "seed_development"`.
- Review runs require `review_ref` pointing to the final dark self-contained review artifact.
- The terminal baton sets the mode-appropriate artifact field, mirrors it in the final `work_split` artifact refs when applicable, and cites it in `verification_matrix` evidence.

The run's intended files are the baton's `changed_paths` union, excluding `.dvandva/` and `superpowers/`. Before final ship, compare `git status --short` against that list. If any unrelated path is dirty, write `status: "human_decision"` and do not commit or push. If intended dirty files remain and `allow_commit == true`, make one final local commit with a semantic commit message. If no intended dirty files remain because checkpoint commits already captured the work, record `final_commit` as `git rev-parse HEAD`. If `allow_push == true`, push the current branch. Record `final_commit` and `pushed_ref`. If commit or push fails, write `status: "human_decision"`, `assignee: "human"`, and put the failing command in `blockers`.

## Stop rule (universal)

In `run_mode: "walkaway"`, do not exit merely because the baton assigns work to vadi. After writing any baton assigned away from prativadi:

1. Surface the new BATON_STATE line.
2. Immediately run a foreground wait against the resolved `"$BATON_FILE"`. Continuous polling is the hard rule: `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE" --interval 60 --max-wait 540 --since-checkpoint "<checkpoint just installed>"` keeps the shell polling across heartbeat intervals until the baton changes after this handoff. Use `--since-checkpoint` after every baton write that hands work away or leaves a team-owned state active; it prevents `active_roles` from bouncing the writer immediately back to "ready" on the same checkpoint. If entering wait without a just-written checkpoint, omit `--since-checkpoint`. Codex-hosted sessions may use `--persist` for older snippets, but it is redundant; add `--persist-max <600` only to fit a shell budget. Exit 20 from explicit `--finite` and Exit 23 from `--persist-max` are heartbeats/caps, not baton terminal states; immediately re-enter the wait unless the user interrupts.
3. Continue from Preflight when the wait returns 0 (`ready` or `checkpoint_advanced`).

Do not end the turn after an assigned-away BATON_STATE line. The next action is the foreground wait helper, not a final response to the user.

Stop only when the wait reports post-handshake `done`, `human_question`, or `human_decision`, or when the user interrupts. `termination_review` is not terminal in any mode; it is a shared active handoff where both roles either keep polling or stop together only after both approve and the terminal protocol completes. This is shell polling, not LLM polling: do not spend model turns checking whether vadi has moved, and do not stop merely because the peer is slow.

In `run_mode: "supervised"`, exit after surfacing any baton assigned away from prativadi. The human manually invokes the assigned role.

## `/goal` condition (paste into your engine when launching)

```
/goal You are Dvandva prativadi. Resolve the active Dvandva baton before every read: DVANDVA_BATON_FILE, else DVANDVA_RUN_DIR/baton.json, else safe DVANDVA_RUN_ID as .dvandva/runs/<run_id>/baton.json, else scan .dvandva/runs/*/baton.json and legacy .dvandva/baton.json, selecting the single active run or waiting for vadi/human selection. Determine the active contract from mode + phase + status + review_target on every turn; prativadi can run in development, research, or review mode, and feature-pr is only the legacy alias of development. Continue the walkaway run until the resolved Dvandva baton reaches post-handshake "done" or a human-intervention state requires the user ("human_question" or "human_decision"). If assignee is not "prativadi", wait on the resolved baton with ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --file "$BATON_FILE" --interval 60 --max-wait 540; continuous polling is the hard rule, Codex-hosted sessions may use --persist for older snippets, and any shell cap/Exit 23 must immediately re-enter wait unless the user interrupts. Do not treat final approval as a stop condition; `termination_review` is a team-owned handoff state in every mode where both roles keep polling or stop together only after both approve and the terminal protocol completes. Invoke `dvandva:research` during research_review for independent research review; use conditional parallelism in every phase: parallelize only genuinely disjoint tracks, never assume a two-subagent ceiling, and record what was not parallelized and why in subagent_tracks. Keep review_target as the existing string selector, keep test_creation separate from deep_review, require 100% test coverage evidence for new behavior, require at least three angle-specific deep-review reviewers before deslop, and in review mode gather evidence from both engines when possible before approval or termination. Before post-handshake done, verify the mode-appropriate terminal artifact, including the one-date run explainer under ./superpowers/run-reports/ plus both roles' run_explainer_reviews for development runs. Before each checkpoint, surface BATON_STATE including mode, DVANDVA_RUN_ID/run_id, original_ask, research_ref, run_explainer_ref, run_explainer_reviews, work_split, subagent_tracks, verification_matrix, plan_ref, turn_cap, findings, verification commands and outcomes, final approval fields, and the final baton contents; do not count shell wait heartbeats as turns. Never create a PR. Stop after turn_cap active model-work turns and assign human if still blocked.
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
| `dvandva-write.sh` exits 23 (`bad_run_id_dir`) | The write-helper validation exit 23: the candidate failed schema, required-key, safe-run-id, v2 status-owner, status, or enum validation; for named runs this includes a `run_id` that is null, missing, empty, or mismatches the directory segment. Fix `$BATON_NEXT_FILE` and rerun; do not edit `$BATON_FILE` directly. |
| `dvandva-write.sh` exits 27 (`stale_checkpoint`) | The peer advanced — re-read the baton, re-derive the next state from the mode table, rewrite `$BATON_NEXT_FILE`, and re-run the helper; never bump past the peer's checkpoint. |
| `dvandva-write.sh` exits 2 (`bad_lock_timeout`) | `DVANDVA_LOCK_TIMEOUT` is not a canonical positive decimal (`^[1-9][0-9]*$`) — zero, negative, leading-zero forms (`08`, `09`), and non-numeric values all fail closed. Fix or unset `DVANDVA_LOCK_TIMEOUT` (default `30` seconds), then rerun. This is a `dvandva-write.sh` config error; it is distinct from `dvandva-wait.sh` exit `29` (`split_brain`). |
| `dvandva-write.sh` exits 28 (`lock_unavailable`) | A non-directory squats the baton-dir lock path `<baton-dir>/.baton.lock.d`; the critical section never runs unlocked. Investigate and remove the squatter, then rerun. This is a `dvandva-write.sh` code; it is distinct from `dvandva-wait.sh` exit `29` (`split_brain`). |
| `dvandva-write.sh` exits 29 (`lock_lost`) | The fencing token was overwritten by a peer that age-stole the lock mid-write; the install was aborted fail-closed and the baton is unchanged. Re-read the baton and re-derive the next state before retrying. This is a `dvandva-write.sh` code; it is **distinct from** `dvandva-wait.sh` exit `29` (`split_brain`). |
| `dvandva-wait.sh` exits 29 (`split_brain`) | A sibling active run is assigned to your role — reconcile selection; park the stale duplicate to `human_decision`. This is a `dvandva-wait.sh` code; it is **distinct from** `dvandva-write.sh` exit `29` (`lock_lost`). |
| `dvandva-write.sh` exits another non-zero code | Do not edit `$BATON_FILE` by hand. 21: candidate missing. 22: candidate invalid JSON. 24: the transition is illegal, including schema changes on an existing baton — re-derive the next state from the mode table; if genuinely stuck, escalate with a fresh candidate whose `status` is `human_decision`. 25: follow the malformed-baton row. 26: filesystem problem; surface it. 30: baton installed but snapshot failed — surface and continue. |

## Canonical baton schema (dvandva.baton.v1)

Use `mode: "development"` for new development scaffolds. Existing `mode: "feature-pr"` batons remain valid legacy inputs and should not be rewritten solely to normalize the alias.

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": null,
  "mode": "development",
  "run_mode": "walkaway",
  "run_id": null,
  "original_ask": "",
  "research_ref": null,
  "run_explainer_ref": null,
  "run_explainer_reviews": [],
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

<!-- Skill version: 0.5.1 -->
