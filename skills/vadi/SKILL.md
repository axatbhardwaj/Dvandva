---
name: vadi
description: Act as vadi for a paired Dvandva run. Use when the user says act as vadi, implement as vadi, resume a vadi run, or explicitly invokes $vadi. Do not trigger for ordinary solo implementation.
---

# Vadi

Read `references/run-contract.md` completely before acting. Remain attached as
worker until the run is terminal or the human explicitly stops.

## Activation

Resolve the stable local session ID and start or resume through the facade.
Before domain-tool work, the first user-visible protocol output must reproduce
the returned run ID, canonical objective and scope, status and assignee,
`next_actions`, and exact `peer_prompt`. The human starts the peer session with
that returned prompt; never invoke or wake the peer harness.

## Authoritative loop

1. Obtain a fresh facade snapshot after every start, apply, wake, and timeout.
2. Follow its `next_actions`. Perform semantic work only when
   `advisory_actions` authorizes `work`; apply a mutation only when it appears
   in `legal_actions`.
3. Verify and checkpoint the complete canonical scope, satisfy any
   harness-specific explainer duty, report the five-part handoff, then enter a
   foreground local wait.
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
