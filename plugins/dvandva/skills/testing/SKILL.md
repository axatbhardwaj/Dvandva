---
name: testing
description: Use when a Dvandva run needs test creation, adversarial test-gap analysis, regression tests, coverage planning, or verification_matrix updates before deep_review.
---

# Dvandva Testing

## Overview

Use this skill during `test_creation` and any phase where tests or coverage are the blocking work. It absorbs the old standalone testing workflow into Dvandva: tests are planned in `verification_matrix`, created before review, and verified before `deep_review`. Do not recreate `.testing-skill/`; map the old handoff and state tracking onto `subagent_tracks`, `verification`, and `verification_matrix`.

## Contract

- Surface `BATON_STATE` before and after testing work.
- Keep test_creation separate from deep_review.
- Target 100% test coverage for every new executable behavior, helper, schema path, or generated workflow.
- For source-only docs/skills, record lint/review coverage and the reason executable coverage does not apply.
- Update `verification_matrix` with coverage owner, command, expected result, actual result, and evidence refs.
- Use `dvandva-test-creator` and `dvandva-sandbox-verifier` subagents when available.
- False positives and design limitations are filtered before tests are written.
- Runtime probes are ephemeral and deterministic: use `/tmp`, avoid `shell=True`, prefer `Docker --network none`, and mark blocked probe work as `UNVERIFIABLE`.
- Write tests only for confirmed issues.
- Do not implement production behavior during testing unless separately approved.

## Workflow

1. `detect context`: read `original_ask`, `work_split`, `verification_matrix`, changed paths, current findings, and active `subagent_tracks`.
2. `coverage analysis`: identify every new or changed behavior, including source-only skill/docs paths that need lint or scenario coverage rather than executable tests.
3. `red attack`: probe the changed surface with adversarial categories such as `Boundary`, `State/Concurrency`, `Error Handling`, and `Bypass Logic`; filter `False positives and design limitations` before turning any finding into test work.
4. `green verification`: confirm the failing test or lint check fails for the expected reason, or confirm that an existing red finding is real before treating it as a gap.
5. `sandbox validation`: keep runtime probes ephemeral and deterministic by using `/tmp`, never using `shell=True`, preferring `Docker --network none`, and recording blocked proof paths as `UNVERIFIABLE`.
6. `blue test writing`: write tests only for confirmed issues, and only the minimum lint/test coverage needed for the assigned phase.
7. `quality review`: run the focused tests and cheap relevant suite, then inspect whether each changed behavior now has evidence or an explicit non-executable rationale.
8. `final review`: update `verification` and `verification_matrix`, hand the results to `deep_review`, and do not review your own work.
9. `results`: return the exact commands, outputs, coverage rationale, remaining gaps, and any `BATON_STATE` carry-forward fields.

## Output

Return:

- Tests created or updated.
- Coverage rationale for each changed behavior.
- Commands and results.
- Remaining gaps that block `deep_review`.
- Updated `BATON_STATE` fields to carry forward.
- Any `subagent_tracks` entries or `verification_matrix` claims needed to replace the old `.testing-skill/` state machine.

## Common Mistakes

| Mistake | Fix |
|---|---|
| Treating review as coverage | Review happens after test_creation. |
| Saying 100% without evidence | Map each behavior to a command or lint/review rationale. |
| Adding broad snapshot tests | Prefer focused behavior tests. |
| Skipping RED because docs changed | For docs/skills, add lint checks; for code, watch tests fail first. |
| Writing blue-team tests for unconfirmed suspicions | Keep blue test writing for confirmed issues only. |
| Fixing production behavior while testing | Escalate it as a finding unless separate approval exists. |
