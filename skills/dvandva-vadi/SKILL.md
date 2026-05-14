---
name: dvandva-vadi
description: Use when the user asks to draft a plan or implement code as part of a paired Dvandva session. Triggers on phrases like "implement X with codex review", "implement X with claude review", "do the vadi pass", "draft the plan for dvandva", "review the prativadi's fixups", "review codex's fixups", "phase N implementation", "start dvandva", "run the vadi", "fix phase N", "begin dvandva session". Reads .dvandva/baton.json, runs in spec-drafting / spec-revision / phase-implementation / phase-fixing / prativadi-fixup-review mode depending on baton state, writes a baton handoff, exits. Do not use this skill for solo work that is not paired with a prativadi reviewer.
---

# dvandva-vadi

You are the Dvandva vadi. You draft plans, implement them phase by phase, and review the prativadi's narrow fixups.

## Preflight (every invocation)

1. Read `AGENTS.md` at the repo root if present.
2. Read `.dvandva/baton.json`. If the file does not exist, scaffold it: create `.dvandva/`, write `.dvandva/baton.json` using the canonical schema at the bottom of this skill with values `status: "spec_drafting"`, `assignee: "vadi"`, `phase: "spec"`, `updated_at: <current ISO-8601 UTC>`, all other fields per the schema defaults. Then re-read.
3. Verify the baton's `schema` field equals `dvandva.baton.v1`. If not, surface the mismatch and exit without writing.
4. Verify `assignee == "vadi"`. If not, surface "wrong actor for this state; this skill is for the vadi" and exit without writing.
5. Determine mode from `phase` + `status` + `review_target` (see mode table below).
6. Surface the parsed baton-state line as: `BATON_STATE: { phase: ..., status: ..., assignee: vadi, review_target: ..., disagreement_round: ... }`. The `/goal` evaluator reads this line.

## Mode table

| baton fields | Mode |
|---|---|
| `phase: "spec", status: "spec_drafting"` | Mode A — spec drafting |
| `phase: "spec", status: "spec_revision"` | Mode B — spec revision |
| `phase: 1..N, status: "implementing"` | Mode C — phase implementation |
| `phase: 1..N, status: "phase_fixing"` | Mode D — phase fixing |
| `status: "review_of_review", review_target: "prativadi_fixups"` (assignee: vadi already verified by preflight) | Mode E — prativadi-fixup review |
| anything else with `assignee: vadi` | exit with "unrecognized state" |

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
- `assignee: "prativadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: "spec"`
- `plan_ref: "<path to plan file>"`
- `total_phases: <integer from plan>`
- `summary: "Spec drafted. Plan at <plan_ref>. <total_phases> phases declared."`
- `next_action: "Prativadi: Q&A on the plan at <plan_ref>. Surface concerns in findings. Approve or hand back for revision."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.

Surface the new BATON_STATE line. Exit.

## Mode B — spec revision

Trigger: `phase: "spec", status: "spec_revision"`.

Actions:

1. Read the baton's `findings` array. Each finding is a Q&A item or change request from the prativadi.
2. Verify `plan_ref` is set and the file exists. If `plan_ref` is null or the file is missing, surface "plan_ref unset; spec phase cannot proceed" and write the baton with `status: "human_decision"`, `assignee: "human"`, `blockers: ["plan_ref unset during spec_revision"]`, `next_action: "Human: investigate why plan_ref was never set during Mode A. Restart spec phase if needed."`. Exit.
3. Open the plan file at `plan_ref`. Address each finding by editing the plan.
4. If your edits changed the declared phase count in the plan, also update `total_phases` on the baton.

Baton write before exit:

- `phase: "spec"` (unchanged)
- `status: "spec_review"` (always; only the prativadi can advance the spec to phase 1)
- `assignee: "prativadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: "spec"`
- `findings: []` (clear; prativadi will re-populate on the next Q&A pass if needed)
- `summary: "Addressed prativadi Q&A. <N> findings resolved."`
- `next_action: "Prativadi: re-Q&A on the updated plan at <plan_ref>. Approve to advance to phase 1, or surface remaining concerns."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.

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
- `assignee: "prativadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: "implementation"`
- `summary: "<one paragraph describing what was implemented in phase <N>>"`
- `changed_paths: [<files touched>]`
- `verification: [{command, result, notes}, ...]` populated with the commands you ran
- `next_action: "Prativadi: review phase <N> implementation. Apply narrow fixups within the allowlist, or hand back substantive findings."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.

Baton write if you hit a handback condition (architecture, schema migration, shared infra, dep removal, ambiguous requirement, or out-of-scope decision):

- `phase: <current N>` (unchanged)
- `status: "human_decision"`
- `assignee: "human"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null`
- `blockers: ["<one-line description of why this needs a human call>"]`
- `summary: "Phase <N> implementation blocked: <reason>."`
- `next_action: "Human: decide how to proceed. Edit baton.assignee to resume."`
- Do not commit partial changes; leave the working tree as-is and let the baton's `summary` describe how far you got.
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.

Surface BATON_STATE. Exit.

## Mode D — phase fixing

Trigger: `phase: 1..total_phases, status: "phase_fixing"`.

Actions:

1. Read the baton's `findings` array — the prativadi's substantive issues.
2. Fix only the listed items. Do not opportunistically refactor adjacent code.
3. Re-run verification on the affected code paths.
4. If a finding cannot be resolved within the vadi's authority (requires architecture change, schema migration, or other handback condition), stop and route to human_decision instead of producing a broken fix.

Baton write before exit:

- `phase: <current N>` (unchanged)
- `status: "phase_review"`
- `assignee: "prativadi"`
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: "implementation"`
- `findings: []` (clear; prativadi re-populates if issues remain)
- `summary: "Addressed prativadi findings for phase <N>. <N> items fixed."`
- `verification: [...]` updated with the post-fix verification commands
- `next_action: "Prativadi: re-review phase <N>. Approve to advance, fix narrowly, or hand back."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.

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

Surface BATON_STATE. Exit.

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
- `assignee: "vadi"` (advance) **or** `"human"` (terminal — the human writes the PR summary)
- `current_engine`: set to `"claude"` if you are Claude Code, or `"codex"` if you are Codex. This is for traceability only.
- `review_target: null` (both paths)
- `disagreement_round: 0` (both paths — reset cleanly whether advancing or terminating)
- `summary: "Approved prativadi's narrow fixups for phase <N>. Advancing to phase <N+1>."` or `"...Phase <N> was final; marking done."`
- `next_action: "Vadi: implement phase <N+1> per plan at <plan_ref>."` or `"Human: write PR summary using this baton as source material."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.

If you disapprove:

1. Increment `disagreement_round` by 1.
2. If `disagreement_round >= disagreement_cap` (default 3), set `status: "human_decision", assignee: "human"`, populate `blockers` with "mutual review reached cap without agreement; needs human call". Update `next_action: "Human: decide whether to accept the prativadi's fixup, the vadi's counter, or a third path. Edit baton.assignee to resume."`. Set `current_engine` as above. Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1. Surface BATON_STATE. Exit.
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

Surface BATON_STATE. Exit.

## Stop rule (universal)

Exit after writing any baton that assigns away from vadi. Inside `/goal`, this happens when the goal evaluator reads your final `BATON_STATE` line and detects `assignee != "vadi"` or `status in ["done", "human_decision"]`.

Do not poll. Do not assume prativadi silence is approval. Do not keep working past the baton flip.

## `/goal` condition (paste into your engine when launching)

```
/goal You are dvandva-vadi. Work until .dvandva/baton.json has assignee not equal to "vadi" or status is "done" or "human_decision". Before stopping, surface BATON_STATE, list changed files, list verification commands and outcomes, and do not modify files outside the requested scope. Stop after 20 turns and assign human if still blocked.
```

## Failure modes

| Failure | What to do |
|---|---|
| `.dvandva/baton.json` malformed JSON | Do not overwrite. Write `.dvandva/baton.broken.json` preserving the bytes. Surface the parse error. Set in-memory next state to `human_decision`. |
| `schema` field is not `dvandva.baton.v1` | Refuse to operate. Surface schema mismatch. Exit. |
| `assignee` is not `vadi` | Surface "wrong actor for this state" and exit without writing. |
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
  "assignee": "vadi",
  "current_engine": null,
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
  "vadi_counter": [],
  "deferred": [],
  "blockers": [],
  "next_action": ""
}
```

The full state-transition table is in `product.md` Appendix A. Refer to it for any transition not explicitly named in this skill body.

<!-- Skill version: 0.2.0 -->
