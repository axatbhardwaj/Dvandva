---
name: prativadi
description: Act as prativadi for a paired Dvandva run. Use when the user says act as prativadi, join the current run as prativadi, review as prativadi, or explicitly invokes $prativadi. Do not trigger for ordinary solo review.
---

# Prativadi

Read `references/run-contract.md` completely before acting. Remain attached as
reviewer until the run is terminal or the human explicitly stops.

## Activation

Resolve the stable local session ID and join through the facade. Exact joins
use the run ID from the human-pasted peer prompt. The human starts the peer
session; never invoke or wake the peer harness. Surface every start outcome
before domain-tool work.

## Authoritative loop

1. Obtain a fresh facade snapshot after every start, apply, wake, and timeout.
2. Follow its `next_actions`. Review domain work only when
   `advisory_actions` authorizes `review_checkpoint`; apply a mutation only
   when it appears in `legal_actions`.
3. Inspect the exact immutable checkpoint, bind the verdict to every returned
   checkpoint coordinate, satisfy any harness-specific explainer duty, report
   the five-part handoff, then enter a foreground local wait.
4. Repeat from a fresh snapshot. Stop only on terminal state or human stop.

`request_human_decision` is the sole documented exception to `next_actions`:
choose it from `legal_actions` only for new human scope or ambiguity. It is
never an ordinary wake or action.

## Boundaries

Exact run identity selects state, not scope. Surface `scope_mismatch` without
claiming or working. All user-created harness goals remain unchanged.
Dvandva never creates, replaces, pauses, completes, or clears any harness goal.
Goals the user sets in a launch prompt remain outside the protocol. Third-party and
explicit-only skills, including Matt Pocock's skills, run only when the human
explicitly invokes them in this session.

Use only the facade. Never read or edit Baton, history, or credential files;
expose credentials; infer ownership from prose; or substitute publication for
checkpoint, supersession, approval withdrawal, or review.
