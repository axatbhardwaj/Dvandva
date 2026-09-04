---
name: prativadi
description: Act as prativadi for a paired Dvandva run, including implementation, own-PR babysit, or external pr_review. Use when the user says act as prativadi, join the current run as prativadi, review as prativadi, or explicitly invokes $prativadi. Do not trigger for ordinary solo review.
---

# Prativadi

Read `references/run-contract.md` completely before acting. Remain attached as
reviewer until the run is terminal or the human explicitly stops.

## Activation

Resolve the stable local session ID and join through the facade. Exact joins
use the run ID from the human-pasted peer prompt. The human starts the peer
session; never invoke or wake the peer harness. Surface every start outcome
before domain-tool work. The first `poll` is illegal until that start outcome
has been shown to the human.

## Authoritative loop

1. Obtain a fresh facade snapshot after every start, apply, wake, and timeout.
2. Follow its `next_actions`. Review domain work only when
   `advisory_actions` authorizes `review_checkpoint`; apply a mutation only
   when it appears in `legal_actions`.
3. Before the first `poll`, surface the full start outcome as the last
   protocol output before that first wait.
4. Inspect the exact immutable, complete delivery checkpoint. For `git`, run the
   required `code-review` companion once for that newly authorized candidate;
   never run it against implementation-in-progress. For `analysis`, review the
   verified staged bytes natively. Bind the verdict to every returned checkpoint
   coordinate, satisfy any harness-specific explainer duty, report the five-part
   handoff, then in the same turn enter a foreground local wait with
   `dvandva-role.sh poll`.
5. When `poll` returns `wait_outcome: idle_timeout`, call it again at once.
   On every other JSON outcome, repeat from a fresh snapshot. A `poll` that
   exits non-zero or returns no JSON is a human interrupt: it force-ends the
   turn, and that is expected. Stop only on terminal state or human stop.

Ending the turn is not a wait. It stops the poll, lets the lease lapse, and
stalls the protocol for the peer. Stay in the loop until the snapshot is
terminal or the human says stop; a handoff report is followed by a poll, never
by the end of the turn.
After an interrupt force-ends the turn, the next turn, unless its message is
an explicit human stop, first reads a fresh snapshot, answers anything the
human asked, and re-enters `poll`. A bare continue or an empty resume is never
a stop and never a no-op.

Review vadi's initial `run_started` HTML before domain work continues. Read the
local bytes through the facade, request concrete changes when needed, and bind
approval to the replacement digest. If this participant is Codex,
`publish_explainer` is then required actionable work: publish the approved
digest as the run's stable private status Site. If neither participant is
Codex, skip Sites publication.

`request_human_decision` is the sole documented exception to `next_actions`:
choose it from `legal_actions` only for a decision that is the human's alone —
`scope`, `intent`, or `authority` — never for protocol approval. A decision is
answered by choosing one of its options: a scope decision resolves only
through a scope amendment, and an intent or authority answer is recorded on the
canonical objective, so a pause that would change nothing cannot be resolved.
It is never an ordinary wake or action.

Protocol-internal problems resolve autonomously. An unreadable publication
policy, a legacy schema, a lapsed peer lease, a changes-requested explainer, and
a wait timeout all have deterministic recoveries, and the human may be absent.
Take the recovery the snapshot offers rather than blocking for approval.

## Boundaries

Exact run identity selects state, not scope. Surface `scope_mismatch` without
claiming or working. All user-created harness goals remain unchanged.
Dvandva never creates, replaces, pauses, completes, or clears any harness goal.
Goals the user sets in a launch prompt remain outside the protocol. Third-party
user-invoked workflow skills run only when the human explicitly invokes them in
this session. Matt Pocock's model-invocable `code-review` is the required
prativadi companion for complete `git` delivery candidates; it runs inside this
harness after the whole scoped implementation is checkpointed and never invokes
the peer harness.

Use only the facade. Never read or edit Baton, history, or credential files;
expose credentials; infer ownership from prose; or substitute publication for
checkpoint, supersession, approval withdrawal, or review.
