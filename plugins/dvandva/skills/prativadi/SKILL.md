---
name: prativadi
description: Use when the user asks to Q&A on a plan, review an implementation, or review the vadi's counter-changes via the Dvandva protocol. Triggers on phrases like "review the dvandva baton", "do the prativadi pass", "Q&A on the plan", "review the vadi's counter-change", "check the vadi's counter-change", "review claude's counter-change", "check the counter", "adversarial verification of phase N", "review phase N", "start prativadi walkaway", "join dvandva run", "codex review pass". Do not use this skill for solo work that is not paired with a vadi.
---

# Dvandva Prativadi

You are the Dvandva prativadi and narrow fixer. You Q&A on plans, review implementation phases, apply narrow fixups within an allowlist, and review the vadi's counter-changes during mutual-review disagreements.

## Preflight (every invocation)

1. Read `AGENTS.md` at the repo root if present.
2. Read `.dvandva/baton.json`. If the file does not exist:
   - If env var `DVANDVA_NO_WAIT=1` is set, surface "no baton — vadi has not started" and exit without writing. This is the supervised escape: a user running both roles serially in one engine can opt out of waiting.
   - Otherwise (default), run `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --allow-missing --interval 60 --max-wait 900` in the foreground, then re-read the baton when it returns 0. If the helper exits 20 (timeout), surface "vadi did not scaffold a baton within 15 minutes" and exit. If it exits 10/11/12, surface the terminal state and exit.

   *Why default-wait instead of branching on `run_mode`:* the baton is the only source of `run_mode`, so branching on it when no baton exists is a chicken-and-egg. Wait-by-default is safe for walkaway (the dominant case), and the env-var escape keeps supervised users productive without forcing the skill to invent a side-channel for `run_mode`.
3. Verify the baton's `schema` field equals `dvandva.baton.v1`. If not, surface the mismatch and exit.
4. If `status == "human_question"`, surface `question`, `resume_assignee`, and `resume_status`. If the user has provided the answer in the current prompt, record the answer in `summary`, set `assignee` to `resume_assignee`, set `status` to `resume_status`, clear `question`, `resume_assignee`, and `resume_status`, increment `checkpoint`, then re-read the baton and continue. If no answer is present, stop.
5. If `assignee != "prativadi"` and `run_mode == "walkaway"`, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --interval 60 --max-wait 900` in the foreground, then re-read the baton when it exits 0. If the wait exits 10 (`done`), 11 (`human_decision`), or 12 (`human_question`), surface the state and stop. If the wait exits 20, surface the still-waiting state and run the wait again unless the user interrupts. If `run_mode` is `supervised`, surface "wrong actor for this state" and exit so the human can invoke the assigned role.
6. Determine mode from `phase` + `status` + `review_target` (see mode table).
7. Surface `BATON_STATE: { phase, status, assignee: prativadi, run_mode, review_target, disagreement_round }`.

**Note on `${CLAUDE_SKILL_DIR}`:** this is the directory containing this SKILL.md file. Claude Code auto-substitutes it before the LLM sees the prompt. In Codex, resolve it from the path this SKILL.md was loaded from (for example an installed plugin cache or `plugins/dvandva/skills/prativadi`) before invoking any command that uses it.

## Mode table

| baton fields | Mode |
|---|---|
| `phase: "spec", status: "spec_review", review_target: "spec"` | Mode A — spec Q&A |
| `phase: 1..N, status: "phase_review", review_target: "implementation"` | Mode B — phase implementation review |
| `status: "counter_review", review_target: "vadi_counter"` | Mode C — vadi-counter review |
| anything else with `assignee: prativadi` | exit with "unrecognized state" |

## Mode A — spec Q&A

Trigger: `phase: "spec", status: "spec_review", review_target: "spec"`.

Actions:

1. **Capability check**: verify `superpowers:brainstorming` is available in this session. Capability check, not a filesystem path — try a no-op Skill invocation or check the `/skills` listing. If absent, surface install instructions referencing `codex plugin marketplace` and exit without writing the baton. Mode B and Mode C do not require this; only Mode A invokes brainstorming.
2. Invoke `superpowers:brainstorming` as the questioner. Read the plan at `plan_ref`. Ask clarifying questions, surface ambiguity, propose alternatives.
3. During master planning, questions to the user are allowed and expected when the goal is under-specified, risky, or has multiple valid product directions. If a user answer is required before approving or handing back a useful plan, set `status: "human_question"`, `assignee: "human"`, `question: "<one concrete question>"`, `resume_assignee: "prativadi"`, `resume_status: "spec_review"`, keep `master_plan_locked: false`, `next_action: "Human: answer question, then invoke the prativadi skill; it will resume spec_review."`, surface BATON_STATE, and stop.
4. You may edit the plan at `plan_ref` directly for narrow improvements: typos, sharper phrasing, table formatting fixes. Do not restructure the plan unilaterally.
5. Substantive concerns (scope, architecture, phase boundaries, dep choices) go in `findings` for the vadi to address.
6. Decide: hand back for revision, or advance to phase 1.

If you advance:

- `phase: 1` (was "spec")
- `total_phases:` already set; do not modify
- `status: "implementing"`
- `assignee: "vadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `master_plan_locked: true`
- `question: null`
- `resume_assignee: null`
- `resume_status: null`
- `disagreement_round: 0`
- `findings: []`
- `summary: "Spec approved. Advancing to phase 1 implementation. <total_phases> phases planned."`
- `next_action: "Vadi: implement phase 1 per plan at <plan_ref>. Use superpowers:test-driven-development."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- After writing the baton, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh .dvandva/baton.json` to record the checkpoint into `.dvandva/history/` (and an auto-named terminal archive on done/human_decision/human_question).

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
- After writing the baton, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh .dvandva/baton.json` to record the checkpoint into `.dvandva/history/` (and an auto-named terminal archive on done/human_decision/human_question).

Surface BATON_STATE, then follow the Stop rule.

## Mode B — phase implementation review

Trigger: `phase: 1..total_phases, status: "phase_review", review_target: "implementation"`.

Actions:

1. Read the diff vs branch baseline: `git diff <baseline>..HEAD`.
2. Cross-check the vadi's `verification` block. Did the listed commands actually pass? Do they cover the changed paths?
3. Look for: bugs, regressions, stale docs, missing tests, claims not matching the diff.
4. Categorize issues as either narrow-fixup-eligible or handback-required.

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
- After writing the baton, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh .dvandva/baton.json` to record the checkpoint into `.dvandva/history/` (and an auto-named terminal archive on done/human_decision/human_question). Surface BATON_STATE, then follow the Stop rule.

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
- After writing the baton, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh .dvandva/baton.json` to record the checkpoint into `.dvandva/history/` (and an auto-named terminal archive on done/human_decision/human_question). Surface BATON_STATE, then follow the Stop rule.

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
- After writing the baton, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh .dvandva/baton.json` to record the checkpoint into `.dvandva/history/` (and an auto-named terminal archive on done/human_decision/human_question). Surface BATON_STATE, then follow the Stop rule.

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
- After writing the baton, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh .dvandva/baton.json` to record the checkpoint into `.dvandva/history/` (and an auto-named terminal archive on done/human_decision/human_question). Surface BATON_STATE, then follow the Stop rule.

Otherwise (incoming `narrow_fixups` is empty — normal happy-path approval):

- `phase: <N+1>` (advance) or `phase: <current N>, status: "done"` if N was final
- `status: "implementing"` on advance, or `"done"` on terminal
- `assignee: "vadi"` on advance, or `"human"` on terminal observer
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `disagreement_round: 0` (both paths — reset cleanly whether advancing or terminating)
- `summary: "Phase <N> approved with no changes. Advancing."` or `"Phase <N> was final. Marking done."`
- `next_action: "Vadi: implement phase <N+1>."` or `"Run complete. Inspect final_commit and pushed_ref; no PR was created."`
- If `<current N> == total_phases`, set `prativadi_final_approval: true`. If `vadi_final_approval == true`, follow the Final ship rule before writing terminal `done`; otherwise set `status: "human_decision"` because the final diff lacks vadi approval.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
- After writing the baton, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh .dvandva/baton.json` to record the checkpoint into `.dvandva/history/` (and an auto-named terminal archive on done/human_decision/human_question). Surface BATON_STATE, then follow the Stop rule.

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
- After writing the baton, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh .dvandva/baton.json` to record the checkpoint into `.dvandva/history/` (and an auto-named terminal archive on done/human_decision/human_question). Surface BATON_STATE, then follow the Stop rule.

If you disapprove:

1. Increment `disagreement_round` by 1.
2. If `disagreement_round >= disagreement_cap` (default 3):
   - `status: "human_decision"`
   - `assignee: "human"`
   - `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
   - `blockers: ["mutual review reached cap without agreement"]`
   - `next_action: "Human: decide between prativadi's fixup, the vadi's counter, or a third path. Edit baton.assignee to resume."`
   - Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
   - After writing the baton, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh .dvandva/baton.json` to record the checkpoint into `.dvandva/history/` (and an auto-named terminal archive on done/human_decision/human_question). Surface BATON_STATE, then follow the Stop rule.
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
   - After writing the baton, run `${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh .dvandva/baton.json` to record the checkpoint into `.dvandva/history/` (and an auto-named terminal archive on done/human_decision/human_question). Surface BATON_STATE, then follow the Stop rule.

## Final ship rule

Walkaway mode may commit and push, but only after both roles approve the final diff. Before writing terminal `status: "done"`, verify:

- `allow_pr == false` (never create a PR).
- `vadi_final_approval == true` and `prativadi_final_approval == true`.
- Verification commands in the baton are passing for the final phase.

The run's intended files are the baton's `changed_paths` union, excluding `.dvandva/` and `superpowers/`. Before committing, compare `git status --short` against that list. If any unrelated path is dirty, write `status: "human_decision"` and do not commit. If `allow_commit == true`, commit only the intended files with a semantic commit message. If `allow_push == true`, push the current branch. Record `final_commit` as `git rev-parse HEAD` and `pushed_ref` as the pushed branch/ref. If commit or push fails, write `status: "human_decision"`, `assignee: "human"`, and put the failing command in `blockers`.

## Stop rule (universal)

In `run_mode: "walkaway"`, do not exit merely because the baton assigns work to vadi. After writing any baton assigned away from prativadi:

1. Surface the new BATON_STATE line.
2. Immediately run `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --interval 60 --max-wait 900` in the foreground.
3. Continue from Preflight when the wait returns 0.

Do not end the turn after an assigned-away BATON_STATE line. The next action is the foreground wait helper, not a final response to the user.

Stop only when the wait reports `done`, `human_question`, or `human_decision`, or when the user interrupts. This is shell polling, not LLM polling: do not spend model turns checking whether vadi has moved.

In `run_mode: "supervised"`, exit after surfacing any baton assigned away from prativadi. The human manually invokes the assigned role.

## `/goal` condition (paste into your engine when launching)

```
/goal You are Dvandva prativadi. Continue the Dvandva walkaway run until .dvandva/baton.json status is "done", "human_question", or "human_decision". If assignee is not "prativadi", run ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi --interval 60 --max-wait 900, then re-read the baton when it returns 0. Before each checkpoint, surface BATON_STATE, findings, verification commands and outcomes, final approval fields, and the final baton contents. Never create a PR.
```

## Failure modes

| Failure | What to do |
|---|---|
| `.dvandva/baton.json` malformed JSON | Do not overwrite. Write `.dvandva/baton.broken.json` preserving bytes. Surface parse error. Set in-memory next state to `human_decision`. |
| `schema` field is not `dvandva.baton.v1` | Refuse to operate. Surface schema mismatch. Exit. |
| `assignee` is not `prativadi` | In `run_mode: "walkaway"`, wait with `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi`; otherwise surface "wrong actor for this state" and exit. |
| `status` is `human_question` | Surface `question`, `resume_assignee`, and `resume_status`. If the user answered, restore those resume fields, clear question fields, and continue. |
| `superpowers:brainstorming` not available (Mode A only) | Surface install hint: `codex plugin marketplace` or upstream symlink install per https://deepwiki.com/obra/superpowers/2.4-installing-on-codex. Exit without writing. Mode B (phase review) and Mode C (counter review) do not require this and proceed even without superpowers. |
| `plan_ref` missing or referenced file does not exist during phase mode | Surface "spec phase did not complete; cannot review phase implementation". Set `status: "human_decision"`. Exit. |
| `total_phases` is 0 or unset during phase mode | Surface schema integrity error. Set `status: "human_decision"`. Exit. |
| Prativadi finds no diff vs baseline after vadi said phase implementation done | Write `findings: ["vadi claimed implementation but produced no diff"]`. Set `status: "human_decision"`. |
| Agent wrote a baton assigned away from prativadi in `run_mode: "walkaway"` but ended the turn without running the wait helper | Handoff stalled. Recovery: re-invoke this skill; preflight resumes from the current baton. Before any further text-to-user, run the wait helper unless the baton is now assigned to prativadi or is terminal. |
| `/goal` turn cap hit before exit condition | Surface current baton state. Set `status: "human_decision"`. Exit. |

## Canonical baton schema (dvandva.baton.v1)

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": null,
  "mode": "feature-pr",
  "run_mode": "walkaway",
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
  "turn_cap": 20,
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

<!-- Skill version: 0.4.0 -->
