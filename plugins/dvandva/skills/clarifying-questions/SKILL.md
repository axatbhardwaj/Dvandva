---
name: dvandva:clarifying-questions
description: Use when a Dvandva run is in clarifying_questions_drafting,
  clarifying_questions_answer, clarifying_questions_followup, or
  clarifying_questions_followup_answer -- vadi and prativadi each ask the
  human at least one question (>=5 combined) about the feature/change
  before research starts.
---

# Dvandva Clarifying Questions

## Overview

Use this skill for the four states that sit between run creation and `research_drafting`: `clarifying_questions_drafting`, `clarifying_questions_answer`, `clarifying_questions_followup`, and `clarifying_questions_followup_answer`. This is a mandatory prefix on the state graph for every mode (`development` / `research` / `review`) and every profile (`fast` / `standard` / `full`) — there is no bypass. The gate exists because research and specs are only as good as the premise they start from: going straight into `research_drafting` on the literal text of `original_ask` lets scope ambiguity, unstated constraints, and unexamined assumptions ride silently into the rest of the run until an ad-hoc `human_question` surfaces them mid-research, after effort is already sunk. This phase moves that interrogation to the front and makes it mandatory rather than incidental, requiring both roles — not just whichever one happens to notice the ambiguity first — to contribute.

The phase runs as two sequential rounds, each round a vadi/prativadi turn followed by a human-answer turn:

1. `clarifying_questions_drafting` (vadi asks round 1) -> `clarifying_questions_answer` (human answers round 1)
2. `clarifying_questions_followup` (prativadi asks round 2, informed by round 1's answers) -> `clarifying_questions_followup_answer` (human answers round 2)
3. Hand off into the existing, unchanged `research_drafting`.

All four states use `phase: "clarifying"`. Everything from `research_drafting` onward is unmodified by this skill.

## Baton Field

Carry and grow this field across both rounds:

```json
"clarifying_questions": [
  {
    "round": 1,
    "asked_by": "vadi",
    "question": "...",
    "answer": null
  },
  {
    "round": 2,
    "asked_by": "prativadi",
    "question": "...",
    "answer": null
  }
]
```

Append new entries with `answer: null` when asking; fill in `answer` in place (never delete or reorder entries) when the following human-answer state records the response. The full Q&A trail rides with the baton into `research_drafting` and beyond, so later phases read it directly instead of re-deriving context from prose.

## What Makes A Good Clarifying Question

A good clarifying question does one or more of the following. A question that does none of these is not worth asking:

- **Surfaces a hidden assumption.** Name the assumption the request is quietly resting on and ask whether it holds.
- **Pins down a scope boundary.** Ask what is explicitly *out* of scope, not just what's in — scope creep usually starts at an unstated boundary, not a stated one.
- **Probes non-functional constraints.** Performance targets, security/compliance posture, backwards-compatibility requirements, and operational constraints (deployment, rollback, monitoring) rarely appear in the original ask unprompted.
- **Asks the "why" behind the request.** Understanding the motivating problem often changes which of several literal readings is correct, and can reveal that the stated solution isn't the only — or best — way to satisfy the underlying need.
- **Surfaces an edge case.** Ask about the boundary conditions, failure modes, or unusual inputs/states the stated behavior doesn't obviously cover.

Explicitly **not** acceptable: superficial yes/no confirmations that restate the request back to the human ("So you want me to add a login page, correct?"). If a question can be answered with a reflexive "yes" without the human having to think, it does not count toward the floor in spirit even if it counts toward it mechanically — draft a sharper one instead.

## Per-Role Lens

Each round has a distinct angle. Do not have prativadi simply repeat vadi's round-1 questions in different words.

**vadi (round 1) — planner / feasibility / scope lens.** Ask from the perspective of someone about to plan the work:
- What exactly are we building or changing?
- What does success look like — how will we know the result is correct/acceptable?
- What's explicitly excluded from this request?
- What non-functional constraints apply (performance, security, backwards-compatibility, dependencies)?

**prativadi (round 2) — reviewer / adversarial lens, informed by round 1's answers.** Read every round-1 `answer` before drafting round-2 questions; the point of round 2 is to press on what round 1 revealed or left thin, not to restart from scratch:
- What could go wrong with the direction round 1's answers imply?
- What did round 1 miss — a scope boundary, constraint, or assumption that surfaced in the answers but wasn't itself questioned?
- What's the riskiest assumption underlying the round-1 answers?
- What edge case remains unaddressed given what's now known?

## Review-Mode Reframe

In `review`-mode runs there is no feature being built — "the feature" reframes as **"the change under review."** Questions probe:
- Scope of the review itself (which files/behaviors/time range are in bounds; what's explicitly out of bounds).
- Acceptance criteria — what would make the review pass vs. fail.
- Priority/severity signals the human already has in mind.
- Known risk areas the human wants extra scrutiny on.

The same vadi-then-prativadi, planner-lens-then-reviewer-lens structure applies; only the subject reframes.

## Mechanics

- `dvandva state --compact --file "$BATON_FILE" --role <vadi|prativadi>` — read the current round, the pending `clarifying_questions` entries, and `next_action` before drafting or answering; do not paste the full array into a checkpoint summary.
- `dvandva next` — scaffold the validated candidate for the current legal transition, then hand-edit only the fields this skill's state requires (typically appending `clarifying_questions` entries or filling in `answer` fields) before installing.
- `dvandva write "$BATON_FILE" "$BATON_NEXT_FILE"` — install the candidate; it validates the transition and gate, and snapshots the checkpoint.
- `dvandva wait --role <vadi|prativadi> --file "$BATON_FILE" --interval 60 --max-wait 540 --stall-max 1800 --until-actionable` — the non-drafting role waits across both human-answer states (`clarifying_questions_answer`, `clarifying_questions_followup_answer`) exactly as it would wait on any other state it isn't assigned to.

## The Gating Rule

The floor is **at least 5 combined questions across both rounds, with at least 1 question from each role.** This is a hard minimum, not a target to approach:

| Leaving state | Gate |
|---|---|
| `clarifying_questions_drafting` | Every round-1 entry has a non-null `question`; `answer` is still null. |
| `clarifying_questions_answer` | Every round-1 entry has a non-null `answer`. |
| `clarifying_questions_followup` | At least 1 round-2 entry exists; round-2 entries have non-null `question` / null `answer`; **total entries across both rounds is >= 5.** |
| `clarifying_questions_followup_answer` | Every round-2 entry has a non-null `answer`. |

Do not stop at the minimum viable count and rationalize it as sufficient if the questions are thin — the floor is a lower bound on quantity, not a substitute for the quality bar above. If round 1 alone reaches 5, round 2 must still contribute at least 1 genuinely reviewer-lens question; the combined-count gate does not waive the per-role minimum.

## Human Answer Surfacing (F5 Convention)

Whichever session is Claude-Code-hosted presents **all** pending questions for the current round to the human directly — via `AskUserQuestion` where available, or plain chat otherwise — and records the answers into the corresponding `clarifying_questions` entries before writing the baton onward. This mirrors the existing F5 convention that a Claude-Code-hosted session owns surfacing `human_question`/`human_decision` to the human, applied here to a mandatory two-round gate instead of an ad-hoc pause. Codex-hosted sessions follow their own normal wait/resume path across the two human-answer states rather than surfacing the questions themselves.

## Common Mistakes

| Mistake | Correction |
|---|---|
| Writing yes/no confirmations to hit the count | Every question must surface an assumption, boundary, constraint, motivation, or edge case — not restate the request. |
| Prativadi repeating vadi's round-1 questions | Round 2 must be informed by round-1 answers and take the adversarial/reviewer angle, not re-ask the planner angle. |
| Stopping at exactly the floor with thin questions | The floor is a minimum count, not a quality waiver; keep applying the quality bar even once the count is satisfied. |
| Skipping the gate for `fast` profile or `research`/`review` mode | This phase is mandatory for every mode and every profile; there is no allowlisted bypass. |
| Codex-hosted session trying to surface questions to the human directly | Only the Claude-Code-hosted session owns direct human surfacing (F5); Codex-hosted sessions wait/resume instead. |
| Treating review-mode questions as feature questions | Reframe "the feature" as "the change under review" — scope, acceptance criteria, priority, and risk areas, not build questions. |
