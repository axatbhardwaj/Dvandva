# Two-Mode Agent Workflow

## Goal

Create a reusable Claude + Codex workflow that keeps Claude as the vadi and Codex as prativadi/fixer, without turning PR comments into a slow agent chat room.

## Roles

### Claude: Vadi

Claude owns implementation.

Claude should:

- Create or use the feature branch.
- Read the task, repo instructions, and local baton.
- Draft the master plan with prativadi input, asking the user questions only while the plan is unlocked or when escalation is required.
- Implement the requested change phase by phase.
- Run the motivating tests and any cheap relevant checks.
- Write a checkpoint handoff to the local baton.
- Block in `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role vadi` once the baton assigns review to prativadi.

Claude should not:

- Treat Codex silence as approval.
- Push risky infra, schema, dependency removal, or force-push changes without human approval.
- Spend model turns polling after the baton says prativadi owns the next action.

### Codex: Prativadi and Narrow Fixer

Codex owns adversarial verification.

Codex should:

- Pull or inspect the latest branch state.
- Review for bugs, regressions, missing tests, stale docs, and mismatched claims.
- Run targeted checks.
- Apply narrow fixups when safe.
- Return the baton to Claude only when broader implementation is needed.
- Mark `DONE` only when the stated acceptance criteria are verified.
- Block in `${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role prativadi` once the baton assigns implementation back to vadi.

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

## Mode 1: Walkaway Feature Mode

Use this for normal feature work and bug fixes.

Target shape:

- One branch.
- Two persistent agent sessions.
- Master plan first, with voluntary human Q&A until plan lock.
- Phase implementation/review loops until `done` or `human_decision`.
- `run_mode: "walkaway"` for two sessions; `run_mode: "supervised"` for single-engine serial fallback.
- PR comments only for human-facing summaries if a human chooses to write them.

Protocol:

1. Human starts or joins the vadi session with the high-level goal.
2. Human starts or joins the prativadi session with "review the dvandva baton."
3. Vadi and prativadi converge on the master plan. Either may ask the human questions until `master_plan_locked: true`.
4. Vadi implements a phase and writes `.dvandva/baton.json` with `assignee: "prativadi"`.
5. Prativadi reviews and optionally applies narrow fixups.
6. Assigned-away agents block in the wait helper, not in model turns. In supervised mode they exit and the human invokes the next role.
7. After the final phase, both roles must set final approval. If `allow_commit` and `allow_push` are true, the active agent may commit and push. It must not create a PR.

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

1. Human defines the campaign intent. Vadi and prativadi refine it into a master plan, asking the human questions until the plan is locked.
2. Claude creates a tracked work ledger in the repo or a local `.dvandva/` ledger if the ledger should not ship.
3. Vadi completes one batch and writes the baton to prativadi.
4. Codex reviews the batch and writes:
   - accepted items,
   - blockers,
   - narrow Codex fixups,
   - deferred items,
   - next recommended batch.
5. Vadi resumes when the wait helper sees the baton assign work to vadi.
6. Stop at `DONE` or `HUMAN_DECISION`.

Campaign-specific rules:

- Never let the PR comment thread be the only source of truth.
- Keep the PR body as the human-readable status board.
- Keep raw history in local artifacts if the campaign is research material.
- Do not use model-turn polling. Use the foreground wait helper for assigned-away sessions.
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
- One active model-work loop at a time, with the other role blocked in the shell wait helper.
- Checkpoint batches instead of every tiny thought.
- Explicit `assignee` field for "ball is in your court".
- Raw PR history archived locally for offline analysis, outside any public repo.
