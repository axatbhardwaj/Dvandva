---
name: understanding
description: Use when a Dvandva user wants to understand a run, phase, review, branch, or decision deeply rather than receive a summary.
---

# Dvandva Understanding

## Overview

Use this after or between Dvandva phases when the user wants mastery. This absorbs the teaching-deep-understanding workflow into Dvandva and uses the baton, `research_ref`, `plan_ref`, diff, and review findings as the source of truth.

## Artifact

Create a dark self-contained HTML checklist at `./superpowers/understanding/YYYY-MM-DD-<topic>.html`. Include:

- `original_ask`
- BATON_STATE snapshot
- phase timeline
- problem, solution, and broader-context items
- checkboxes for mastery gates
- copy-as-prompt export state

## Teaching Loop

1. Ground in real artifacts: baton, diff, `research_ref`, `plan_ref`, verification, and findings.
2. Teach one small chunk.
3. Ask the user to explain, predict, or compare.
4. Do not advance until the user demonstrates high-level and low-level understanding.
5. Update the HTML checklist only after mastery is demonstrated.

## Baton Integration

Surface `BATON_STATE` when the teaching topic is tied to an active run. Do not mutate the baton unless the active role explicitly asks for an understanding artifact reference in `deferred` or `summary`.

## Common Mistakes

| Mistake | Fix |
|---|---|
| Giving a complete lecture | Teach one chunk, then ask. |
| Asking "does that make sense?" | Require explain-back or prediction. |
| Teaching the solution first | Establish why the problem existed first. |
| Writing Markdown checklist | Generated human-facing understanding artifacts are HTML. |
