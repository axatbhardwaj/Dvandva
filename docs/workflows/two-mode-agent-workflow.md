# Two-Mode Agent Workflow

## Goal

Create a reusable Claude + Codex workflow that keeps Claude as the vadi and Codex as prativadi/fixer, without turning PR comments into a slow agent chat room.

## Roles

### Claude: Vadi

Claude owns implementation.

Claude should:

- Create or use the feature branch.
- Read the task, repo instructions, and local baton.
- Implement the requested change.
- Run the motivating tests and any cheap relevant checks.
- Write a checkpoint handoff to the local baton.
- Exit once the baton assigns review to Codex.

Claude should not:

- Treat Codex silence as approval.
- Push risky infra, schema, dependency removal, or force-push changes without human approval.
- Keep looping after the baton says Codex owns the next action.

### Codex: Prativadi and Narrow Fixer

Codex owns adversarial verification.

Codex should:

- Pull or inspect the latest branch state.
- Review for bugs, regressions, missing tests, stale docs, and mismatched claims.
- Run targeted checks.
- Apply narrow fixups when safe.
- Return the baton to Claude only when broader implementation is needed.
- Mark `DONE` only when the stated acceptance criteria are verified.

Codex may directly fix:

- Typographical docs mistakes.
- Stale references in docs or audit rows.
- Small test expectation updates.
- Lint, formatting, or type errors with obvious fixes.
- Small missed edge cases that do not change architecture.

Codex should hand back to Claude for:

- Product behavior changes.
- Architecture changes.
- Schema migrations.
- Shared infra changes.
- Dependency removals or major dependency additions.
- Ambiguous requirements.

## Mode 1: Feature PR Mode

Use this for normal feature work and bug fixes.

Target shape:

- One branch.
- One Claude implementation pass.
- One Codex review/fix pass.
- One final Claude response or human review.
- PR comments only at important checkpoints.

Protocol:

1. Human starts Claude with the vadi goal.
2. Claude implements until tests pass or it hits a blocker.
3. Claude writes `.dvandva/baton.json` with `assignee: "codex"`.
4. Human starts Codex with the prativadi goal.
5. Codex reviews and optionally commits narrow fixups.
6. Codex writes `.dvandva/baton.json` with one of:
   - `assignee: "claude"` for required implementation follow-up.
   - `assignee: "human"` for decision needed.
   - `status: "done"` when finished.
7. Human starts the assigned next agent only if needed.

Noise limits:

- Claude writes one checkpoint summary.
- Codex writes one review summary.
- Local baton carries the detailed handoff.
- PR comments are optional unless there is an open PR and the summary is useful to humans.

## Mode 2: Campaign Mode

Use this for broad repo audits, documentation coverage, migration sweeps, or repeated mechanical work.

Target shape:

- Prefer several smaller PRs by area.
- If one PR is necessary, use explicit batch checkpoints.
- Each batch is 3-5 surfaces or roughly 500 changed lines.
- Codex reviews each batch before Claude advances.

Protocol:

1. Human defines the campaign scope and stop condition.
2. Claude creates a tracked work ledger in the repo or a local `.dvandva/` ledger if the ledger should not ship.
3. Claude completes one batch and writes the baton to Codex.
4. Codex reviews the batch and writes:
   - accepted items,
   - blockers,
   - narrow Codex fixups,
   - deferred items,
   - next recommended batch.
5. Claude resumes only if the baton assigns work to Claude.
6. Stop at `DONE` or `HUMAN_DECISION`.

Campaign-specific rules:

- Never let the PR comment thread be the only source of truth.
- Keep the PR body as the human-readable status board.
- Keep raw history in local artifacts if the campaign is research material.
- Do not use timer polling when `/goal` can stop on a baton condition.
- Require human approval before expanding scope from one dimension to another.

## Why This Improves PR 353

PR 353 proved the driver/reviewer pattern works, but it also exposed costs:

- Too many comments for routine use.
- Too many tiny commits in one PR.
- Same GitHub identity made agent attribution fuzzy.
- Timer/polling cadence wasted turns.
- PR comments became both coordination protocol and archival record.

Dvandva changes the defaults:

- Local baton first, PR comments second.
- One active autonomous loop at a time.
- Checkpoint batches instead of every tiny thought.
- Explicit `assignee` field for "ball is in your court".
- Raw PR history archived locally for offline analysis, outside any public repo.
