---
name: understanding
description: Use when a Dvandva user wants to understand a run, phase, review, branch, or decision deeply rather than receive a summary. Triggers: "teach me what we just did", "help me understand this phase", "walk me through this diff", "make sure I really get this change", "explain the baton", or onboarding to an unfamiliar Dvandva run.
---

# Dvandva Understanding

## Overview

Use this after or between Dvandva phases when the user wants mastery. This absorbs the teaching-deep-understanding workflow into Dvandva and uses the baton, `research_ref`, `plan_ref`, diff, and review findings as the source of truth — the learner studies the *actual run and decisions*, not a constructed hypothetical.

You already write excellent explanations. **That is the trap.** Asked to "teach," you will instinctively deliver one complete, well-organized lecture — and a lecture produces nodding, not understanding. This skill replaces the lecture with **teaching**: deliver understanding in stages, make the learner do the cognitive work, and **confirm mastery before advancing**. Drill *why* relentlessly.

Core principle: **the learner talks at least as much as you do.** If a stretch goes by where they only read, you are lecturing again.

## Ground Yourself First (no fiction)

Before teaching, read the actual artifacts:

- `BATON_STATE`: `phase`, `status`, `assignee`, `original_ask`, `run_id`, `research_ref`, `plan_ref`, `changed_paths`, `verification`, `findings`
- `git diff` and `git log` for the branch (commit-by-commit if multiple phases)
- The HTML artifact at `research_ref` (problem framing, sources, work_split)
- The HTML artifact at `plan_ref` (architecture, decisions, rejected alternatives)
- Cross-review or deep-review `findings` if teaching a completed run

Derive the checklist items from these real artifacts. **Do not teach from memory or plausible reconstruction** — every "why this decision" must be traceable to a baton field, a diff hunk, or a documented finding.

## The Iron Rule

**Never deliver the whole explanation at once. Teach ONE stage, confirm mastery (high-level *and* low-level), and only then move to the next.**

Confirmation is not "does that make sense?" — that invites a reflexive "yes." Confirmation means the learner *demonstrates* understanding: explains it back in their own words, answers a pointed question, or predicts an outcome. No demonstration → not mastered → do not advance.

### Red Flags — You Are About to Lecture

- Writing more than ~150–200 words without handing a question back
- Covering problem, solution, and impact in a single message
- "Let me give you the full picture first, then…"
- Explaining a *why* the learner could have reasoned out if you'd asked
- Accepting "makes sense" / "got it" / "yeah" as evidence of understanding
- Reaching the end with the learner never having said anything substantive

All of these mean: **stop, shrink the chunk, ask a question.**

## The Three Pillars (teach in this order)

1. **The Problem** — what the original ask was (`original_ask`), **why it existed**, what made it non-trivial (constraints surfaced in `research_ref`), and which alternatives were considered. *Do not start the solution until the learner can articulate why the problem mattered.*
2. **The Solution** — what was built (diffed files, `changed_paths`), **why that approach** (vs. rejected branches in `plan_ref`), the key design decisions, and edge cases surfaced in `verification` or `findings`.
3. **The Broader Context** — **why this matters** downstream (invariants, risk, future work, baton fields that carry the impact forward).

Each pillar maps to a section of the running HTML checklist. Each concrete thing to grasp is one checkbox item.

## The Teaching Loop (every stage)

1. **Orient** — one sentence on where we are and what this stage covers. Cite the baton field or artifact you are teaching from.
2. **Teach one chunk** — the smallest self-contained idea from the real run. Stop early.
3. **Check** — hand the work back: ask them to explain it, answer a targeted question, or predict an outcome grounded in the actual diff or baton.
4. **Drill the whys** — take their answer one or more levels deeper until you hit bedrock (a fundamental constraint, invariant, or tradeoff documented in `research_ref` or `plan_ref`).
5. **Gate on mastery** — high-level (why it matters) *and* low-level (the mechanism/edge case). Shallow → re-teach differently. Solid → tick the checklist item and advance.

The one place you will fail is the gate: you will plow ahead instead of looping back when understanding is shallow. Don't.

## Checking Mastery (ask, don't tell)

Replace assertions with questions that force retrieval and reasoning:

- **Explain-back:** "In your own words, why did the baton need `research_ref` before `plan_ref` could be written?"
- **Edge-case prediction:** "If two engines both wrote a baton field during the same phase, what would happen?"
- **Counterfactual:** "Why is the rejected approach from `plan_ref` actually *worse*, not just different?"
- **Transfer:** "Where else in the Dvandva protocol would this same constraint appear?"

**Real mastery:** they reconstruct the reasoning unprompted, catch their own gaps, handle a variation. **Shallow:** they parrot your words, hedge, or jump to mechanism with no why. "Makes sense" is not mastery — require a demonstration.

## Drilling the Whys

The single most important "why" is **why the problem existed**. If the learner doesn't feel that, everything downstream is memorization.

Go deeper instead of accepting the first answer:

> Learner: "The baton uses a helper script because direct writes aren't safe."
> You: "Right — *why* aren't direct writes safe?" → "Two engines might write at the same time." → "And *why* does that break specifically?" → "No atomic compare-and-swap — last write wins, dropping the other engine's turn data." ← bedrock.

Stop drilling when the next "why" would be a fundamental, irreducible constraint (filesystem atomics, protocol invariant, external service contract).

## The Running Checklist (HTML Artifact)

At the **start**, build a dark self-contained HTML checklist at `./superpowers/understanding/YYYY-MM-DD-<topic>.html`. Include:

- `original_ask` and `run_id` from the baton (or from context if no active run)
- BATON_STATE snapshot (phase, status, changed_paths, findings summary)
- Phase timeline (which phases happened, in order)
- Three-pillar item structure with unchecked checkboxes
- Copy-as-prompt export button (exports checked/unchecked state + user notes as a paste-back prompt)

**Tick an item only when mastery is confirmed** — never in advance. Re-surface / update the file at each checkpoint. Tell the user its path and offer to send it with SendUserFile.

## Baton Integration

Surface `BATON_STATE` at the start of the teaching session. Read `research_ref` and `plan_ref` before building the checklist — they contain the decisions worth teaching.

**Do not mutate the baton** during an understanding session. If the user asks for a reference to the understanding artifact to carry forward, add it to `deferred` or `summary` only if they explicitly request it.

## When NOT to Use

- The user wants a quick summary or TL;DR — give them the summary.
- The user is already an expert and just needs the facts — don't make them play student.
- Mid-debugging or mid-implementation — teach after, not in the middle of the work.

## Common Mistakes

| Mistake | Fix |
|---|---|
| One-message lecture covering everything | One stage per message; stop and ask |
| "Does that make sense?" | Ask them to explain it back or predict an outcome |
| Stating every *why* yourself | Let them reason; only fill the gap they can't |
| Ticking items optimistically | Tick only after a demonstration of mastery |
| Teaching the solution first | Establish *why the problem existed* (`original_ask`) before the fix |
| Quizzing only at the very end | Check incrementally, at every stage |
| Teaching from memory / reconstruction | Ground every "why" in `research_ref`, `plan_ref`, diff, or `findings` |
| Mutating the baton during teaching | Read-only; write to `deferred` only if user explicitly requests |
| Writing a Markdown checklist | Generated human-facing understanding artifacts are HTML |
