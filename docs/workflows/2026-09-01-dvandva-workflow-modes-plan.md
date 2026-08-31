# Dvandva Workflow Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add default implementation, own-PR babysit, and external PR-review workflows to the active role skills.

**Architecture:** Keep `vadi` and `prativadi` as the sole role interfaces. Select behavior from the Baton objective's `workflow` reference, defaulting to `implementation`; keep GitHub semantics in role contracts and bind external attestations through the existing final explainer gate.

**Tech Stack:** Markdown agent skills, Rust source-contract tests, Bash packaging tests

**Spec:** `docs/workflows/2026-09-01-dvandva-workflow-modes-design.md`

## Global Constraints

- Final feature diff is at most 350 changed lines.
- No kernel schema, daemon, peer-harness invocation, admin bypass, or autonomous merge.
- One independent run and formal GitHub receipt per external PR.
- Prativadi is an internal babysit filter; the colleague reviewer owns real approval.
- Evidence exchange and available local Fable adjudication precede escalation.

---

### Task 1: Workflow contracts

**Files:**
- Modify: `skills/vadi/SKILL.md`
- Modify: `skills/vadi/references/run-contract.md`
- Modify: `skills/prativadi/references/run-contract.md`
- Test: `v4/tests/skill_flow.rs`

**Interfaces:**
- Consumes: objective reference `workflow=<implementation|babysit|pr_review>`
- Produces: deterministic role behavior and digest-bound external receipt evidence

- [ ] **Step 1: Establish RED behavioral baselines**

Run fresh read-only subagents against the unmodified role contracts for a
babysit regression and an external-review submission. Record whether they
preserve colleague authority, refuse autonomous merge, avoid patching external
PRs, pin the head, and require two live receipt checks.

- [ ] **Step 2: Write and verify the failing contract test**

Add one focused test requiring all workflow values, the babysit authority
rules, independent external-review lenses, the receipt handshake, and the
Fable-before-escalation rule. Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test skill_flow workflow_modes
```

Require failure because those workflow contracts are absent.

- [ ] **Step 3: Add minimal workflow guidance**

Update vadi discovery metadata for explicit babysit/external-review requests.
Add compact workflow-selection and role-specific lifecycle sections to both run
contracts. Reuse existing checkpoints, explainer receipts, human decisions,
polling, and Matt `code-review`; add no kernel state.

- [ ] **Step 4: Verify GREEN and behavioral compliance**

Run the focused test and the same fresh subagent scenarios. Require correct
workflow selection, authority, review independence, and receipt confirmation.

- [ ] **Step 5: Verify the complete candidate**

```bash
cargo test --manifest-path v4/Cargo.toml --test skill_flow
bash tests/skills/role-skills.sh
git diff --check
git diff --numstat
```

Require every command to pass and at most 350 changed lines before committing
the immutable candidate for prativadi.
