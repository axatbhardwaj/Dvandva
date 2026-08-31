# Dvandva Workflow Modes Design

**Status:** Approved in conversation on 2026-09-01

## Objective

Add three explicit workflows behind the existing `vadi` and `prativadi` role
skills without adding a daemon, peer-harness invocation, or shallow wrapper
skills. Keep the complete feature change at or below 350 changed lines.

The workflow is recorded as an objective reference:

| Value | Selection | Purpose |
|---|---|---|
| `implementation` | Default when absent | Build and review a complete delivery |
| `babysit` | Human explicitly asks to babysit our PR | Repair and maintain merge readiness |
| `pr_review` | Human asks to review external PRs | Submit a formal review without patching |

The human still starts one vadi session and one prativadi session. The kernel,
claim model, exact-run join, checkpoint cycle, and foreground wait remain the
shared protocol interface.

## Default implementation

Vadi implements and verifies the complete canonical scope. Prativadi reviews
each newly authorized complete Git candidate, using Matt Pocock's `code-review`
companion when available. Requested changes return the run to vadi; approval
allows the existing finalization gate. Work in progress is never checkpointed
merely to obtain review.

## Own-PR babysit

Babysit must be explicit. Starting it authorizes routine work only on the
scoped PR branches: reproduce feedback or CI failures, patch, test, commit,
push, rerun CI, synchronize/rebase the stack, and re-request an existing
colleague reviewer. Design, security, secret-policy, and unavoidable permission
decisions escalate.

The lifecycle is:

```text
feedback or CI failure -> vadi fix_ready -> prativadi internally_cleared
-> vadi rereview_requested -> colleague decision -> merge_ready
-> maintaining_ready
```

Prativadi is an internal sanity filter, not the real reviewer. Its unresolved
findings gate re-requesting colleague review, but only the colleague's approval
can satisfy merge readiness. New feedback, a changed head, or a failed gate
reopens the fix/review loop.

Vadi replies with fix evidence but leaves colleague-owned threads for the
colleague to resolve. Merge readiness requires the exact internally reviewed
head, required CI, mergeability, required external approvals, no live requested
changes, dispositioned threads, current stack/base state, and no pending work.

After reaching merge readiness, both roles keep polling GitHub and maintain the
state. They never merge autonomously. Merge requires a fresh human instruction
after readiness; a stack merge additionally requires explicit authority for
every PR it would affect.

## External PR review

An external-review request creates one independent run per PR. The workflow is
read-only except for the final formal GitHub review; it never patches another
author's branch.

Vadi first completes a constructive review covering intent, behavior,
integration, tests, maintainability, and practical failure modes. Without
seeing that report first, prativadi performs an adversarial review covering the
diff, spec, standards, regressions, security edges, and every vadi finding.
Prativadi owns the final adjudicated `APPROVE` or `REQUEST_CHANGES`; vadi submits
that exact verdict.

Before submission, vadi rechecks PR identity, current head, actor versus author,
and permission. Self-approval, missing authority, or head drift fails closed.
A confirmed `REQUEST_CHANGES` is successful completion because submission—not
remediation—is this workflow's objective.

Both roles independently verify the GitHub review ID, PR, actor, state, reviewed
commit, and body digest. Head drift before both confirmations invalidates the
attempt and restarts review. Read-only evidence gathering may use subagents;
parents alone own Baton mutations and the formal GitHub write.

## Durable external receipts

The v2 Baton has no typed GitHub receipt or merged status. This change therefore
uses the existing digest-bound final explainer instead of expanding the kernel:

1. the acting role queries GitHub after its write and records the receipt;
2. the peer independently queries GitHub and checks the same coordinates;
3. the Codex harness stages the receipt-bearing explainer;
4. the Claude harness approves those exact bytes only after its live check;
5. finalization remains blocked until that two-harness gate is satisfied.

The kernel enforces the receipt handshake and immutable bytes, while the role
contracts own the semantic GitHub checks. No workflow may claim schema-level
GitHub validation.

## Failure and authority rules

- Recoverable CI, review, and scoped branch failures are fixed autonomously.
- Uncertainty triggers evidence exchange, another fix attempt, and an available
  local Fable adjudication lane before human escalation. Fable is advisory and
  never becomes a third Baton participant or peer-harness invocation.
- Human escalation is reserved for an irreversible action that both roles still
  cannot establish as safe, or an unavoidable external permission barrier.
- External-review submission never downgrades to a comment.
- Babysit never treats internal approval or thread resolution as colleague
  acceptance.
- No admin bypass, ruleset/secret mutation, peer wake-up, or daemon is added.
- Interrupted runs resume from the Baton and requery mutable GitHub state.

## Verification

Focused source-contract tests must prove the default, explicit workflow names,
the two distinct review lenses, the no-patch/no-automerge boundaries, fresh
merge authority, colleague re-review semantics, two-party receipt confirmation,
and parent-only mutations. Existing role-skill and v4 tests must remain green.

Tool and Fable evidence: [workflow-mode GitHub evidence](../research/2026-09-01-workflow-mode-github-evidence.md).
