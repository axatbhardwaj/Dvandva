---
name: worktree-setup
description: Use when preparing a Dvandva worktree, branch, PR-review workspace, ticket workspace, or repo-local setup before a paired run.
---

# Dvandva Worktree Setup

## Overview

Use this to prepare isolated Dvandva work. It absorbs the worktree setup workflow into Dvandva while keeping repo-specific conventions as profiles rather than hard dependencies.

Invoke `superpowers:using-git-worktrees` first. If the current harness cannot load that skill, record the capability gap in `BATON_STATE`, run the equivalent native git worktree preflight, and do not silently skip isolation.

## Contract

- Surface `BATON_STATE` if a baton already exists.
- Create or select a worktree before starting a risky multi-phase run.
- Copy local env files only when the repo convention allows it.
- Confirm `.gitignore` covers `.dvandva/`, `/superpowers/`, and `agent-os/`.
- Record branch status in `BRANCH-NOTES.md`.
- Update `~/ACTIVE-WORK.md` with one concise active-work line.
- Do not reset, delete branches, or remove dependencies without explicit user approval.

## Generic Workflow

1. Read repo instructions and current branch/worktree state:
   - `git status --short --branch`
   - `git worktree list --porcelain`
   - `git branch --list`
2. Choose the worktree path and branch name from the task ID, PR, or run ID using repo-local naming conventions.
3. Create or reuse the worktree from the correct base branch or fetched review head. Never reset user work.
4. Copy environment files from the source worktree when appropriate. Preserve relative paths and exclude dependency directories such as `node_modules`.
5. Install dependencies only when needed and remove setup-only lockfile noise when the repo convention calls for it.
6. Run a bounded baseline command and record exact status, including whether validation is blocked by known repo-specific caveats.
7. Always verify no leftover repo-specific baseline or test-runner process remains after the baseline command finishes or times out.
8. Start or update Dvandva baton fields: `run_id`, `original_ask`, `work_split`, and `verification_matrix`.
9. Record handoff in `BRANCH-NOTES.md` and `~/ACTIVE-WORK.md`.

## DeFi Profile

For `/home/xzat/defi/monorepo`, preserve the existing conventions instead of inventing new names or metadata:

1. Start from the default DeFi root: `/home/xzat/defi/monorepo` unless the user explicitly names another repo.
2. Gather context before creating anything:
   - PR review: fetch and read the GitHub PR metadata plus the PR head SHA.
   - Monday item: read the Monday item using the board ID and pulse ID from the URL, then pull its **EDEF/TDEF/STDEF item key** from the `custom-key` column.
   - The raw `pulse ID` is only for fetching the item. It is not the branch or worktree key.
   - If the key column shows a `bare number` instead of an `EDEF/TDEF/STDEF-<n>` key, stop and flag the board configuration issue rather than naming from the raw pulse ID.
   - Monday has no git-branch field. Derive the branch name from the repo convention.
3. Use the local naming convention:
   - Single PR review path: `/home/xzat/defi/monorepo-pr-<num>-review`
   - Multi-PR review path: `/home/xzat/defi/monorepo-prs-<nums>-review`
   - Monday item path: `/home/xzat/defi/monorepo-<key>` where `<key>` is lowercase, for example `monorepo-edef-12`
   - PR review branches: `review/pr-<num>` or `review/prs-<nums>`
   - Ticket branches: `feature/<key>`
4. Copy local env files from the source worktree, preserving relative paths:
   - `.env`
   - `.env.local`
   - `.envrc`
   - `.env.keys`
   - search below the repo while excluding `.git` and `node_modules`
5. Run `bun install`.
   - If Bun only adds `"configVersion": 0` to `bun.lock`, remove that setup-only noise.
6. Run the bounded baseline command exactly:
   - `timeout --kill-after=10s 180s bash -lc 'TURBO_UI=false bun run test'`
   - Known caveat: live E2E or Vitest packages may hang or time out after other packages pass, so do not claim success if the bounded command times out.
   - Always verify no leftover `turbo`, `vitest`, or `bun run test` process remains for the worktree after the bounded run; record the exact status as no leftover turbo, vitest, or bun run test process remains.
7. Preserve handoff and review guardrails:
   - Update `BRANCH-NOTES.md` in the worktree root.
   - Update `~/ACTIVE-WORK.md` with one concise line for the worktree and branch.
   - PR review deliverables are dark self-contained HTML, stored locally unless the user says otherwise.
   - Never post GitHub review bodies or comments without explicit per-session approval.
8. Preserve Git identity and reporting rules:
   - Verified GitHub emails: `axatbhardwaj@outlook.com` and `axatbhardwaj@gmail.com`.
   - Report exact SHAs for the base branch and any fetched PR heads.

## Output

Return:

- Worktree path, branch, base SHA, and dirty state.
- Env/dependency/baseline actions taken.
- `BRANCH-NOTES.md` and `~/ACTIVE-WORK.md` update status.
- Suggested initial `work_split` and `verification_matrix`.
