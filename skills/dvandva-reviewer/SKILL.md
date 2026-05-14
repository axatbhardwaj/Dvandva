---
name: dvandva-reviewer
description: Use when the user asks Codex to Q&A on a plan, review a Claude implementation, or review Claude's counter-changes via the Dvandva protocol. Triggers on phrases like "review the dvandva baton", "do the reviewer pass", "Q&A on the plan", "review codex's fixups", "check claude's counter-change", "adversarial verification of phase N". Reads .dvandva/baton.json, runs in spec-Q&A / phase-review / claude-counter-review mode depending on baton state, applies only narrow fixups within the allowlist, writes a baton handoff, exits. Do not use this skill for solo Codex work that is not paired with Claude as the doer.
---

# dvandva-reviewer

You are the Dvandva reviewer and narrow fixer. You Q&A on plans, review implementation phases, apply narrow fixups within an allowlist, and review Claude's counter-changes during mutual-review disagreements.

## Preflight (every invocation)

1. Read `AGENTS.md` at the repo root if present.
2. Read `.dvandva/baton.json`. If the file does not exist, surface "no baton — doer has not started" and exit without writing.
3. Verify the baton's `schema` field equals `dvandva.baton.v1`. If not, surface the mismatch and exit.
4. Verify `assignee == "codex"`. If not, surface "wrong actor for this state" and exit.
5. **Capability check**: verify `superpowers:brainstorming` is available in this Codex session. Capability check, not a filesystem path — try a no-op Skill invocation or check the `/skills` listing. If absent, surface install instructions referencing `codex plugin marketplace` and exit.
6. Determine mode from `phase` + `status` + `review_target` (see mode table).
7. Surface `BATON_STATE: { phase, status, assignee: codex, review_target, disagreement_round }`.

## Mode table

| baton fields | Mode |
|---|---|
| `phase: "spec", status: "spec_review", review_target: "spec"` | Mode A — spec Q&A |
| `phase: 1..N, status: "phase_review", review_target: "implementation"` | Mode B — phase implementation review |
| `status: "counter_review", review_target: "claude_counter"` | Mode C — claude-counter review |
| anything else with `assignee: codex` | exit with "unrecognized state" |

## Mode A — spec Q&A

Trigger: `phase: "spec", status: "spec_review", review_target: "spec"`.

Actions:

1. Invoke `superpowers:brainstorming` as the questioner. Read the plan at `plan_ref`. Ask clarifying questions, surface ambiguity, propose alternatives.
2. You may edit the plan at `plan_ref` directly for narrow improvements: typos, sharper phrasing, table formatting fixes. Do not restructure the plan unilaterally.
3. Substantive concerns (scope, architecture, phase boundaries, dep choices) go in `findings` for the doer to address.
4. Decide: hand back for revision, or advance to phase 1.

If you advance:

- `phase: 1` (was "spec")
- `total_phases:` already set; do not modify
- `status: "implementing"`
- `assignee: "claude"`
- `review_target: null`
- `disagreement_round: 0`
- `findings: []`
- `summary: "Spec approved. Advancing to phase 1 implementation. <total_phases> phases planned."`
- `next_action: "Claude: implement phase 1 per plan at <plan_ref>. Use superpowers:test-driven-development."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.

If you hand back for revision:

- `phase: "spec"` (unchanged)
- `status: "spec_revision"`
- `assignee: "claude"`
- `review_target: null`
- `findings: [<your Q&A items, one bullet each>]`
- `summary: "Spec needs revision. <N> findings raised."`
- `next_action: "Claude: address findings in <plan_ref>, then hand back to Codex for re-Q&A."`
- Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.

Surface BATON_STATE. Exit.

## Mode B — phase implementation review

Trigger: `phase: 1..total_phases, status: "phase_review", review_target: "implementation"`.

Actions:

1. Read the diff vs branch baseline: `git diff <baseline>..HEAD`.
2. Cross-check the doer's `verification` block. Did the listed commands actually pass? Do they cover the changed paths?
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
- `assignee: "claude"`
- `review_target: null`
- `findings: [<one bullet per substantive issue>]`
- `summary: "Phase <N> needs implementation work before re-review."`
- `next_action: "Claude: address findings, then hand back to Codex for re-review."`

**If narrow fixups apply AND no handback issues:** apply the fixups inline (edit the affected files), re-run verification, then:

- `phase: <current N>` (unchanged)
- `status: "review_of_review"`
- `assignee: "claude"`
- `review_target: "codex_fixups"`
- `narrow_fixups: [<one bullet per fix you applied>]`
- `verification: [<post-fixup commands and results>]`
- `summary: "Phase <N> reviewed. Applied <N> narrow fixups. Mutual review owed."`
- `next_action: "Claude: review Codex's narrow fixups for phase <N>. Approve to advance, or counter."`

**If narrow fixups apply AND handback issues:** populate both `findings` and `narrow_fixups`, but route to `phase_fixing` first. Mutual review of the narrow fixups happens on the next Codex pass after Claude's fix.

**If approve with no changes:**

- `phase: <N+1>` (advance) or `phase: <current N>, status: "done"` if N was final
- `status: "implementing"` on advance, or `"done"` on terminal
- `assignee: "claude"` on advance, or `"human"` on terminal (so the human knows to write the PR summary)
- `review_target: null`
- `disagreement_round: 0` (both paths — reset cleanly whether advancing or terminating)
- `summary: "Phase <N> approved with no changes. Advancing."` or `"Phase <N> was final. Marking done."`
- `next_action: "Claude: implement phase <N+1>."` or `"Human: write PR summary using baton as source."`

Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1. Surface BATON_STATE. Exit.

## Mode C — claude-counter review

Trigger: `status: "counter_review", review_target: "claude_counter", assignee: "codex"`.

This is the mutual-review disagreement step. Claude disapproved your earlier narrow fixup and wrote a counter-change. Decide whether the counter is correct.

Actions:

1. Read the baton's `claude_counter` array — Claude's bullet list of what they changed and why.
2. Inspect the actual diff Claude applied since the previous checkpoint.
3. Cross-check: does the counter address the original issue your fixup was trying to fix? Or did Claude introduce a different problem?
4. Decide: approve or disapprove.

If you approve:

- `phase: <N+1>` (advance) or `phase: <current N>, status: "done"` if N was final
- `status: "implementing"` on advance, or `"done"` on terminal
- `assignee: "claude"` on advance, or `"human"` on terminal
- `review_target: null`
- `disagreement_round: 0` (both paths — reset cleanly whether advancing or terminating)
- `summary: "Approved Claude's counter-change for phase <N>. Advancing to phase <N+1>."` or `"...Phase <N> was final."`
- `next_action: "Claude: implement phase <N+1>."` or `"Human: write PR summary."`

If you disapprove:

1. Increment `disagreement_round` by 1.
2. If `disagreement_round >= disagreement_cap` (default 3):
   - `status: "human_decision"`
   - `assignee: "human"`
   - `blockers: ["mutual review reached cap without agreement"]`
   - `next_action: "Human: decide between Codex's fixup, Claude's counter, or a third path. Edit baton.assignee to resume."`
   - Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1.
   - Exit.
3. Otherwise, write a new narrow fixup (edit the affected files):
   - `phase: <current N>` (unchanged)
   - `status: "review_of_review"`
   - `assignee: "claude"`
   - `review_target: "codex_fixups"`
   - `narrow_fixups: [<your new fix description>]`
   - `disagreement_round: <incremented>`
   - `summary: "Disapproved Claude's counter; wrote a different fix. Round <X>."`
   - `next_action: "Claude: review Codex's new fixup. Approve to advance, or counter again."`

Set `updated_at` to the current UTC time in ISO-8601 format (e.g., `2026-05-13T10:30:00Z`). Increment `checkpoint` by 1. Surface BATON_STATE. Exit.

## Stop rule (universal)

Exit after writing any baton. Inside `/goal`, the goal evaluator stops the loop when the BATON_STATE line shows `assignee != "codex"` or `status in ["done", "human_decision"]`.

Do not poll. Do not stay running waiting for Claude.

## `/goal` condition (paste into Codex when launching)

```
/goal You are dvandva-reviewer. Review the branch using .dvandva/baton.json as the handoff. Apply only narrow fixups within the allowlist. Stop when the baton has assignee not equal to "codex" or status is "done" or "human_decision". Before stopping, surface BATON_STATE, findings, verification commands and outcomes, and the final baton contents.
```

## Failure modes

| Failure | What to do |
|---|---|
| `.dvandva/baton.json` malformed JSON | Do not overwrite. Write `.dvandva/baton.broken.json` preserving bytes. Surface parse error. Set in-memory next state to `human_decision`. |
| `schema` field is not `dvandva.baton.v1` | Refuse to operate. Surface schema mismatch. Exit. |
| `assignee` is not `codex` | Surface "wrong actor for this state" and exit. |
| `superpowers:brainstorming` not available in this Codex session | Surface install hint: `codex plugin marketplace` or upstream symlink install per https://deepwiki.com/obra/superpowers/2.4-installing-on-codex. Exit without writing. |
| `plan_ref` missing or referenced file does not exist during phase mode | Surface "spec phase did not complete; cannot review phase implementation". Set `status: "human_decision"`. Exit. |
| `total_phases` is 0 or unset during phase mode | Surface schema integrity error. Set `status: "human_decision"`. Exit. |
| Reviewer finds no diff vs baseline after Claude said phase implementation done | Write `findings: ["doer claimed implementation but produced no diff"]`. Set `status: "human_decision"`. |
| `/goal` turn cap hit before exit condition | Surface current baton state. Set `status: "human_decision"`. Exit. |

## Canonical baton schema (dvandva.baton.v1)

```json
{
  "schema": "dvandva.baton.v1",
  "updated_at": null,
  "mode": "feature-pr",
  "phase": "spec",
  "total_phases": 0,
  "status": "spec_review",
  "assignee": "codex",
  "review_target": "spec",
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
