---
name: testing
description: Use when a Dvandva run needs test creation, adversarial test-gap analysis, regression tests, coverage planning, or verification_matrix updates before deep_review.
---

# Dvandva Testing

## Overview

Use this skill during `test_creation` and any phase where tests or coverage are the blocking work. It absorbs the old standalone testing workflow into Dvandva: tests are planned in `verification_matrix`, created before review, and verified before `deep_review`.

## Contract

- Surface `BATON_STATE` before and after testing work.
- Keep test_creation separate from deep_review.
- Target 100% test coverage for every new executable behavior, helper, schema path, or generated workflow.
- For source-only docs/skills, record lint/review coverage and the reason executable coverage does not apply.
- Update `verification_matrix` with coverage owner, command, expected result, actual result, and evidence refs.
- Use `dvandva-test-creator` and `dvandva-sandbox-verifier` subagents when available.

## Workflow

1. Read `original_ask`, `work_split`, `verification_matrix`, changed paths, and current findings.
2. Identify every new or changed behavior.
3. Write the failing test or lint check first when executable behavior exists.
4. Watch the test fail for the expected reason.
5. Implement or request only the minimum test/code adjustment needed for the assigned phase.
6. Run the focused tests and cheap relevant suite.
7. Record exact commands in `verification` and planned/evidence coverage in `verification_matrix`.
8. Hand to `deep_review`; do not review your own work.

## Output

Return:

- Tests created or updated.
- Coverage rationale for each changed behavior.
- Commands and results.
- Remaining gaps that block `deep_review`.
- Updated `BATON_STATE` fields to carry forward.

## Common Mistakes

| Mistake | Fix |
|---|---|
| Treating review as coverage | Review happens after test_creation. |
| Saying 100% without evidence | Map each behavior to a command or lint/review rationale. |
| Adding broad snapshot tests | Prefer focused behavior tests. |
| Skipping RED because docs changed | For docs/skills, add lint checks; for code, watch tests fail first. |
