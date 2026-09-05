# Automatic Run Discovery Implementation Plan

> **For agentic workers:** Execute task-by-task in the existing isolated worktree using superpowers:executing-plans.

**Goal:** Let independently launched prativadi find the intended local run without copying a prompt.

**Architecture:** Expose bounded read-only candidate discovery through the role facade. Use the existing XDG run root and repository/harness identity, filter by workflow and explicit task coordinates, then exact-join through start. Candidate enumeration never claims or creates a run.

**Tech Stack:** Rust registry scanning, Bash facade, Python candidate filtering, Bash integration tests.

**Spec:** Approved conversation: independently start both roles; reuse ~/.local/state/dvandva/runs; one unambiguous match joins, multiple matches ask, no match waits; exact identity persists afterward.

## Constraints

- No new state store, daemon, background wake assumption or fuzzy automatic claim.
- Explicit run IDs always bypass discovery and preserve run_missing behavior.
- Preserve legacy pr_review one-shot semantics and current kernel/schema versions; release/install remains separate.
- Both independently distributed roles contain identical helper code.

## Tasks

- [x] Add tests/skills/discover.sh that initially fails on the missing facade command. Cover no registry, repository/workflow/task filtering, ambiguity, competing claims, bounded wait and exact-join continuation; compare registry bytes/modes before and after enumeration.
- [x] Make registry scans use RunChannel::peek and expose discover through both facades. Add a self-contained helper to filter candidates, preserve errors, and bound polling even when unrelated candidates exist.
- [x] Route prativadi startup without an explicit run ID through discovery. Record canonical task refs in vadi preflight, retain exact resume and describe how auto-discovery removes the join-prompt delivery dependency.
- [x] Run focused tests, the Rust discovery/role suites, and existing role/canary/poll checks. Get independent review, resolve findings and commit granular changes on the existing gh stack.

## Verification

The facade test first failed on the missing discover command. A second regression
failed when a unique task had the wrong peer harness; discovery now filters both
harnesses and exact start independently checks the peer before claiming.
The read-only kernel regression failed on the missing --read-only capability,
then passed while leaving a pending history successor and missing lock untouched.

Full Rust tests, tests/skills/discover.sh, role-skills.sh, poll.sh, poll-errors.sh
and two-role-canary.sh passed. The canary confirms both hosts install identical
discovery helpers and references. Independent review's pairing finding is fixed.
No live model sessions, T3 rendering reproduction, installed-skill update or
release publication was performed. Older kernels reject the new discovery
capability; deployment requires a release containing the updated kernel/skills.
