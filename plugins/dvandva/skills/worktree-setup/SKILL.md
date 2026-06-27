---
name: worktree-setup
description: Use when preparing a Dvandva worktree, branch, PR-review workspace, ticket workspace, or repo-local setup before a paired run.
---

# Dvandva Worktree Setup

## Overview

Use this to prepare isolated Dvandva work. It absorbs the worktree setup workflow into Dvandva while keeping repo-specific conventions as profiles rather than hard dependencies.

## Contract

- Surface `BATON_STATE` if a baton already exists.
- Create or select a worktree before starting a risky multi-phase run.
- Copy local env files only when the repo convention allows it.
- Confirm `.gitignore` covers `.dvandva/`, `/superpowers/`, and `agent-os/`.
- Record branch status in `BRANCH-NOTES.md`.
- Update `~/ACTIVE-WORK.md` with one concise active-work line.
- Do not reset, delete branches, or remove dependencies without explicit user approval.

## Generic Workflow

1. Read repo instructions and current branch/worktree state.
2. Choose worktree path and branch name from the task ID, PR, or run ID.
3. Create or reuse the worktree.
4. Copy environment files from the source worktree when appropriate.
5. Install dependencies only when needed.
6. Run a bounded baseline command and record exact status.
7. Start or update Dvandva baton fields: `run_id`, `original_ask`, `work_split`, and `verification_matrix`.
8. Record handoff in `BRANCH-NOTES.md` and `~/ACTIVE-WORK.md`.

## DeFi Profile

For `/home/xzat/defi/monorepo`, preserve the existing conventions:

- PR review path: `/home/xzat/defi/monorepo-pr-<num>-review`
- Multi-PR path: `/home/xzat/defi/monorepo-prs-<nums>-review`
- Ticket path: `/home/xzat/defi/monorepo-<lowercase-key>`
- Branches: `review/pr-<num>`, `review/prs-<nums>`, or `feature/<key>`
- Baseline: bounded `bun run test`, with known live E2E timeout caveats recorded plainly
- Git identity: use a verified GitHub email

## Output

Return:

- Worktree path, branch, base SHA, and dirty state.
- Env/dependency/baseline actions taken.
- `BRANCH-NOTES.md` and `~/ACTIVE-WORK.md` update status.
- Suggested initial `work_split` and `verification_matrix`.
