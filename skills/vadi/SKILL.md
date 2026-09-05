---
name: vadi
description: Act as vadi for a paired Dvandva run, including discovery, implementation, babysitting, or review. Use when the user says act as vadi, implement as vadi, resume a vadi run, or explicitly invokes $vadi. Do not trigger for ordinary solo implementation.
---

# Vadi

Read `references/run-contract.md` and `references/model-selection.md` completely
before acting. Remain attached as worker until the run is terminal or the human
explicitly stops.

## Activation

Read `references/initiation.md` before starting or joining. For discovery,
also read `references/discovery.md`. Its bounded startup source verification
and intentional human-input waits are explicit exceptions to the ordinary
work/poll loop below; all mutations still require facade authorization.

Resolve the stable local session ID and start or resume through the facade.
Before domain-tool work, the first user-visible protocol output must reproduce
the returned run ID, canonical objective and scope, status and assignee,
`next_actions`, and exact `peer_prompt`. Show the `peer_prompt` verbatim in
its own fenced code block as the last line of the activation block, so the
human can copy it from a phone. The human starts the peer session with that
returned prompt; never invoke or wake the peer harness. The first `poll` is
illegal until that activation block, with the exact `peer_prompt`, has been
shown to the human. At run start the kernel withholds `work` until prativadi
approves the `run_started` explainer, so the only work between the activation
block and the first `poll` is staging that explainer and reporting the handoff.

## Authoritative loop

1. Obtain a fresh facade snapshot after every start, apply, wake, and timeout.
2. Follow its `next_actions`. Perform semantic work only when
   `advisory_actions` authorizes `work`; apply a mutation only when it appears
   in `legal_actions`.
3. Verify and checkpoint the complete canonical scope, satisfy any
   harness-specific explainer duty, and report the five-part handoff.
4. Before the first `poll`, after all semantic work and the handoff, repeat
   the activation block: its exact `peer_prompt` is the final protocol output
   immediately before that first wait. Then, in the same turn, enter a
   foreground local wait with `dvandva-role.sh poll`.
5. When `poll` returns `wait_outcome: idle_timeout`, call it again at once.
   On every other successful JSON outcome, repeat from a fresh snapshot.
   A nonzero exit or missing JSON alone does not establish a human interrupt.
   On failure, retain the exit status and non-secret error, take a fresh
   read-only `observe` snapshot, and follow the recovery in the run contract.
   An explicit human stop or host-reported cancellation interrupts the loop;
   otherwise stop only on terminal state or report an unrecoverable environment
   blocker explicitly.

Ending the turn is not a wait. It stops the poll, lets the lease lapse, and
stalls the protocol for the peer. While the local channel remains usable, stay
in the loop until the snapshot is terminal or the human says stop; a successful
handoff report is followed by a poll, never by the end of the turn. After
bounded recovery, report a persistent environment blocker with the active run,
owner, and next action before yielding; it is not a human interrupt or terminal
handoff.
After an interrupt force-ends the turn, the next turn, unless its message is
an explicit human stop, first reads a fresh snapshot, answers anything the
human asked, and re-enters `poll`. A bare continue or an empty resume is never
a stop and never a no-op.

On the `run_started` obligation, propose the initial HTML explainer before
continuing domain work. Prativadi reviews those local bytes; incorporate every
requested change and restage until approved. If this participant is Codex,
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
this session.

Use only the facade. Never read or edit Baton, history, or credential files;
expose credentials; infer ownership from prose; or substitute publication for
checkpoint, supersession, approval withdrawal, or review.
