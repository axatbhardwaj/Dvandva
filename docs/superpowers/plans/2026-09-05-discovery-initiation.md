# Discovery Initiation Implementation Plan

> **For agentic workers:** Execute this approved plan task-by-task in the existing isolated worktree. Use superpowers:executing-plans for inline execution.

**Goal:** Add the four-workflow startup ceremony and paired discovery using unchanged user-invoked Matt skills.

**Architecture:** Role-local references own initiation and discovery policy. Reuse the v2 explainer join receipt, analysis checkpoints and explicit human-decision mechanism; do not add kernel state.

**Tech Stack:** Markdown role skills, Rust source-contract tests, Bash integration suites.

**Spec:** docs/workflows/2026-09-05-discovery-initiation-design.md

## Global Constraints

- Keep kernel/schema and archived v3 unchanged.
- Preserve explicit invocation and approval requirements of upstream skills.
- Preserve exact-run identity, immutable checkpoints and merge authority.
- Keep each distributed role self-contained with parity-checked shared references.

## Tasks

- [x] Add failing source-contract tests in v4/tests/skill_flow.rs for startup reference routing, shared-reference parity, source discovery, independent research, skill waits, linked discovery runs and persistent Review semantics. Run `cargo test --locked --manifest-path v4/Cargo.toml --test skill_flow discovery` and confirm the missing-reference failure.
- [x] Add skills/{vadi,prativadi}/references/initiation.md and discovery.md; wire both entry points and run contracts. Define exact preflight and research permissions, source manifest, invocation wait/resume, source-bound analysis review, and canonical workflow values with legacy compatibility.
- [x] Update both model-selection references, README.md and docs/workflows/skill-only-run.md for discovery casting, explicit skill commands and four workflows. Run the focused tests and fix contradictory old instructions.
- [x] Run Rust tests, role-skills.sh, poll.sh, poll-errors.sh and two-role-canary.sh. Review the complete diff, record limitations and deliver the source change without modifying installed skills or publishing a release.

## Verification evidence

The new source-contract test failed first on the missing initiation route.
After implementation, the full Rust suite passed; focused skill_flow now has
18 passing tests. Role wrappers, poll, poll-errors, HTML companion and two-role
canary passed. The canary exercises discovery research incorporation through
real facade receipts, startup work gating and exact skill-wait resumption.
Independent review identified and prompted fixes for approval findings, external
wait liveness, discovery HTML casting and the final CI readiness handoff.
No live model planning session, real tracker publication or Sites deployment was
performed; canary resources use an isolated temporary installation and receipts.
