---
name: dvandva-doer
description: Use when the user asks Claude to draft a plan or implement code as part of a Claude+Codex pair via the Dvandva protocol. Triggers on phrases like "implement X with codex review", "do the doer pass", "draft the plan for dvandva", "review codex's fixups", "phase N implementation". Reads .dvandva/baton.json, runs in spec-drafting / spec-revision / phase-implementation / phase-fixing / codex-fixup-review mode depending on baton state, writes a baton handoff, exits. Do not use this skill for solo Claude work that is not paired with a Codex review.
---

# dvandva-doer

You are the Dvandva doer. You draft plans, implement them phase by phase, and review Codex's narrow fixups.

## Preflight (every invocation)

1. Read `AGENTS.md` at the repo root if present.
2. Read `.dvandva/baton.json`. If the file does not exist, scaffold it: create `.dvandva/`, copy `templates/channel/baton.json` into it, then re-read.
3. Verify the baton's `schema` field equals `dvandva.baton.v1`. If not, surface the mismatch and exit without writing.
4. Verify `assignee == "claude"`. If not, surface "wrong actor for this state; this skill is for the doer" and exit without writing.
5. Determine mode from `phase` + `status` + `review_target` (see mode table below).
6. Surface the parsed baton-state line as: `BATON_STATE: { phase: ..., status: ..., assignee: claude, review_target: ..., disagreement_round: ... }`. The `/goal` evaluator reads this line.

## Mode table

| baton fields | Mode |
|---|---|
| `phase: "spec", status: "spec_drafting"` | Mode A — spec drafting |
| `phase: "spec", status: "spec_revision"` | Mode B — spec revision |
| `phase: 1..N, status: "implementing"` | Mode C — phase implementation |
| `phase: 1..N, status: "phase_fixing"` | Mode D — phase fixing |
| `status: "review_of_review", review_target: "codex_fixups"` | Mode E — codex-fixup review |
| anything else with `assignee: claude` | exit with "unrecognized state" |

## Mode A — spec drafting

Trigger: `phase: "spec", status: "spec_drafting"`.

Actions:

1. Invoke `superpowers:brainstorming` to clarify scope with the user. The brainstorming skill produces a refined design.
2. Invoke `superpowers:writing-plans` to convert the design into a phase-by-phase implementation plan.
3. The plan goes to `./superpowers/plans/YYYY-MM-DD-<topic>.md` (gitignored). Record the absolute path.
4. Read the plan's declared phase count. Set `total_phases` on the baton to that integer.

Baton write before exit:

- `phase: "spec"` (unchanged)
- `status: "spec_review"`
- `assignee: "codex"`
- `review_target: "spec"`
- `plan_ref: "<path to plan file>"`
- `total_phases: <integer from plan>`
- `summary: "Spec drafted. Plan at <plan_ref>. <total_phases> phases declared."`
- `next_action: "Codex: Q&A on the plan at <plan_ref>. Surface concerns in findings. Approve or hand back for revision."`
- Update `updated_at` and bump `checkpoint`.

Surface the new BATON_STATE line. Exit.

## Mode B — spec revision

Trigger: `phase: "spec", status: "spec_revision"`.

Actions:

1. Read the baton's `findings` array. Each finding is a Q&A item or change request from Codex.
2. Open the plan file at `plan_ref`. Address each finding by editing the plan.
3. If your edits changed the declared phase count in the plan, also update `total_phases` on the baton.

Baton write before exit:

- `phase: "spec"` (unchanged)
- `status: "spec_review"` (always; only Codex can advance the spec to phase 1)
- `assignee: "codex"`
- `review_target: "spec"`
- `findings: []` (clear; Codex will re-populate on the next Q&A pass if needed)
- `summary: "Addressed Codex Q&A. <N> findings resolved."`
- `next_action: "Codex: re-Q&A on the updated plan at <plan_ref>. Approve to advance to phase 1, or surface remaining concerns."`
- Update `updated_at` and bump `checkpoint`.

Surface BATON_STATE. Exit.

## Mode C — phase implementation

Trigger: `phase: 1..total_phases, status: "implementing"`.

Actions:

1. Read the plan at `plan_ref`. Find the section for the current `phase` integer.
2. Implement only the scope listed for that phase. Do not bleed into adjacent phases.
3. Invoke `superpowers:test-driven-development` if the phase involves writing code with test coverage.
4. Run motivating tests and cheap relevant checks (lint, type-check). Surface each command and its result in the transcript — the `/goal` evaluator only sees what is surfaced.
5. If the phase scope crosses a handback condition (architecture change, schema migration, shared infra, dep removal, ambiguous requirement), stop and route to human_decision instead of continuing.

Baton write before exit:

- `phase: <current N>` (unchanged)
- `status: "phase_review"`
- `assignee: "codex"`
- `review_target: "implementation"`
- `summary: "<one paragraph describing what was implemented in phase <N>>"`
- `changed_paths: [<files touched>]`
- `verification: [{command, result, notes}, ...]` populated with the commands you ran
- `next_action: "Codex: review phase <N> implementation. Apply narrow fixups within the allowlist, or hand back substantive findings."`
- Update `updated_at` and bump `checkpoint`.

Surface BATON_STATE. Exit.

## Mode D — phase fixing

Trigger: `phase: 1..total_phases, status: "phase_fixing"`.

Actions:

1. Read the baton's `findings` array — Codex's substantive issues.
2. Fix only the listed items. Do not opportunistically refactor adjacent code.
3. Re-run verification on the affected code paths.

Baton write before exit:

- `phase: <current N>` (unchanged)
- `status: "phase_review"`
- `assignee: "codex"`
- `review_target: "implementation"`
- `findings: []` (clear; Codex re-populates if issues remain)
- `summary: "Addressed Codex findings for phase <N>. <N> items fixed."`
- `verification: [...]` updated with the post-fix verification commands
- `next_action: "Codex: re-review phase <N>. Approve to advance, fix narrowly, or hand back."`
- Update `updated_at` and bump `checkpoint`.

Surface BATON_STATE. Exit.

## Mode E — codex-fixup review

Trigger: `status: "review_of_review", review_target: "codex_fixups", assignee: "claude"`.

This is the mutual-review step. Codex applied narrow fixups during its own review pass and is asking you to confirm the fixups are correct.

Actions:

1. Read the baton's `narrow_fixups` array — Codex's bullet list of what it fixed.
2. Inspect the actual diff Codex applied: `git diff` against the last checkpoint Claude committed.
3. Cross-check each `narrow_fixups` entry against the diff. Does the diff match the description? Are the fixes within the narrow-fix allowlist (typos, lint, stale refs, small test fixes, missed edge cases)?
4. Decide: approve or disapprove.

If you approve, baton write:

- `phase: <N+1>` (advance) **or** `phase: <current N>` if N was final and `status: "done"`
- `status: "implementing"` (advance) **or** `"done"` (terminal)
- `assignee: "claude"` (advance) **or** unchanged (terminal)
- `review_target: null` on advance, or null on terminal
- `disagreement_round: 0` (reset on advance)
- `summary: "Approved Codex's narrow fixups for phase <N>. Advancing to phase <N+1>."` or `"...Phase <N> was final; marking done."`
- `next_action: "Claude: implement phase <N+1> per plan at <plan_ref>."` or `"Human: write PR summary using this baton as source material."`
- Update `updated_at` and bump `checkpoint`.

If you disapprove:

1. Increment `disagreement_round` by 1.
2. If `disagreement_round >= disagreement_cap` (default 3), set `status: "human_decision", assignee: "human"`, populate `blockers` with "mutual review reached cap without agreement; needs human call". Update `next_action: "Human: decide whether to accept Codex's fixup, Claude's counter, or a third path. Edit baton.assignee to resume."`. Exit.
3. Otherwise, write your counter-changes inline (edit the files Codex's fixup touched). Baton write:
   - `phase: <current N>` (unchanged)
   - `status: "counter_review"`
   - `assignee: "codex"`
   - `review_target: "claude_counter"`
   - `claude_counter: [<bullet list of what you changed and why>]`
   - `disagreement_round: <incremented>`
   - `summary: "Disapproved Codex's fixup for phase <N>; wrote counter-change. Round <X>."`
   - `next_action: "Codex: review Claude's counter-change. Approve to advance, or counter-propose."`
   - Update `updated_at` and bump `checkpoint`.

Surface BATON_STATE. Exit.

## Stop rule (universal)

Exit after writing any baton that assigns away from Claude. Inside `/goal`, this happens when the goal evaluator reads your final `BATON_STATE` line and detects `assignee != "claude"` or `status in ["done", "human_decision"]`.

Do not poll. Do not assume Codex silence is approval. Do not keep working past the baton flip.

## `/goal` condition (paste into Claude when launching)

```
/goal You are dvandva-doer. Work until .dvandva/baton.json has assignee not equal to "claude" or status is "done" or "human_decision". Before stopping, surface BATON_STATE, list changed files, list verification commands and outcomes, and do not modify files outside the requested scope. Stop after 20 turns and assign human if still blocked.
```

## Failure modes

| Failure | What to do |
|---|---|
| `.dvandva/baton.json` malformed JSON | Do not overwrite. Write `.dvandva/baton.broken.json` preserving the bytes. Surface the parse error. Set in-memory next state to `human_decision`. |
| `schema` field is not `dvandva.baton.v1` | Refuse to operate. Surface schema mismatch. Exit. |
| `assignee` is not `claude` | Surface "wrong actor for this state" and exit without writing. |
| `plan_ref` missing or referenced file does not exist during a phase mode | Surface "spec phase did not complete; cannot start phase implementation". Set `status: "human_decision"`. Exit. |
| `total_phases` is 0 or unset during a phase mode | Surface schema integrity error. Set `status: "human_decision"`. Exit. |
| Git working tree dirty before Mode A starts | Surface dirty state in the new baton's `summary`. Proceed only if the user's prompt explicitly indicates intent. |
| `/goal` turn cap (default 20) hit before exit condition | Surface current baton state and a "still owe work" summary. Set `status: "human_decision"`. Exit. |

## Canonical baton schema (dvandva.baton.v1)

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": null,
  "mode": "feature-pr",
  "phase": "spec",
  "total_phases": 0,
  "status": "spec_drafting",
  "assignee": "claude",
  "review_target": null,
  "plan_ref": null,
  "disagreement_round": 0,
  "disagreement_cap": 3,
  "turn_cap": 20,
  "branch": "",
  "checkpoint": 0,
  "summary": "",
  "changed_paths": [],
  "verification": [],
  "findings": [],
  "narrow_fixups": [],
  "claude_counter": [],
  "deferred": [],
  "blockers": [],
  "next_action": ""
}
```

The full state-transition table is in `product.md` Appendix A. Refer to it for any transition not explicitly named in this skill body.

<!-- Skill version: 0.1.0 -->
