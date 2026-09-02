# Minimal Run Baton Protocol

Issue: [#3](https://github.com/axatbhardwaj/Dvandva/issues/3)

## Boundary

One run has one directory, one `baton.json`, one worker session, and one
reviewer session. The human starts the sessions separately in T3 Code. The
kernel never invokes Claude from Codex or Codex from Claude. GitHub, Linear,
Sites, and other systems may be recorded as opaque references; they do not
coordinate or wake the pair. The v4 crate is non-publishable and independent
of archived v3.5.1. Harness goals are user-owned context outside the Baton and
remain unchanged.

Fresh runs use `dvandva.run.v2` and role API 2. V1 is readable only for
classification and a dedicated one-way upgrade. Ordinary v1 claim, wait, or
mutation returns `migration_required`; history permits one v1-to-v2 edge and
never a downgrade. Setup never migrates runs.

## Canonical scope and checkpoint binding

The Baton owns the objective, references, task identity, `scope_revision`, and
a non-empty ordered set of required deliverables. Exact-run selection compares
every supplied coordinate and returns `scope_mismatch` without claiming when
one differs. A Human Decision is one of three kinds — `scope` (what the work
covers), `intent` (which reading of the request is meant), `authority`
(permission that is the human's alone) — and never protocol approval: the kernel
refuses to park while it holds a deterministic recovery, admits a decision only
with distinct options (and, in an autonomous run, only as a choice among
concrete scope proposals), refuses to re-ask the decision just answered, and
resolves a decision only by a chosen option that changes the run. Only a
resumed Human Decision can amend scope; amendment
increments `scope_revision` and clears stale checkpoint state.

A checkpoint is complete only when its unique deliverable IDs cover canonical
scope exactly and it has non-empty verification. The kernel trims inputs,
orders the manifest deterministically, and derives `manifest_digest`. Semantic
review binds three coordinates: immutable checkpoint identity,
`manifest_digest`, and `scope_revision`. A branch name, mutable URL, or partial
artifact list is not a checkpoint.

## State graph

```text
working -> reviewing -> finalizing -> done
              |             ^
              v             |
           revising --------+

any active state -> human_decision(scope | intent | authority) -> declared active state
any active state -> abandoned
```

The worker submits a new immutable checkpoint from `working` or `revising`.
The reviewer binds findings or approval to the exact checkpoint. If new work is
found while the reviewer owns it, the worker uses
`request_checkpoint_supersession`; the pending request blocks approval until
the reviewer accepts and returns ownership. If approval already moved the run
to `finalizing`, the worker uses `withdraw_approval`. The old immutable history
remains evidence. Publication is never a side channel for replacing scope or a
checkpoint.

## Storage and authority

- `.baton.lock` serializes writers within this run only.
- `baton.json` is flushed and atomically replaced.
- `history/<revision>.json` is immutable.
- every mutation supplies an expected revision;
- role claims store only a SHA-256 token digest;
- expired claims can be replaced at a higher epoch;
- `recover` validates a complete history prefix, creates a new revision, and
  clears both claims.

The kernel derives a participant's harness from its authenticated claim. New
and upgraded runs require two distinct, non-blank normalized harness names.
`Codex` and `Claude` retain canonical title case; other names normalize to
lowercase. Credential replacement and protocol upgrade fence stale claims.

## Commands

Build with `cargo build --manifest-path v4/Cargo.toml`. `probe` must confirm
schema `dvandva.run.v2` and role API 2 before role I/O. The role facade returns
one authoritative snapshot containing the full Baton, legal and advisory
actions, a blocking reason when applicable, and the exact peer prompt.

## Starting the pair

Vadi surfaces the run ID and canonical scope before domain work, followed by
the exact returned prompt:

```text
Act as prativadi and join Dvandva run <run-id>.
```

Both roles obey the snapshot's actions and foreground-wait when the peer owns
the next mutation. The wait is `poll`: it re-enters the kernel wait on every
idle timeout and returns only on a real wake, a terminal run, or its budget, and
the role calls it again at once on an idle return rather than ending its turn. Tokens remain private to each role facade.

## Rolling explainer gate

Each work-carrying handoff opens an obligation; an approval preserves the
current obligation and its receipts, since it transfers no new work product.
For the run's current obligation,
vadi stages the explainer's bytes into the run directory and prativadi reads
those exact bytes back through the facade and reviews them. The explainer
contains canonical scope, the complete manifest, findings and decisions, and a
current plan/TODO.

A work-carrying handoff replaces the current obligation, so the gate binds
the current obligation rather than the run's whole history of them; because an
approval preserves it, an approved delivery finalizes on the explainer already
staged and reviewed for its checkpoint — one terminal handshake. The staged artifact is
content-addressed at `explainer/<source_digest>.html` and echoes the pending
handoff kind and revision, `scope_revision`, and the optional three-coordinate
checkpoint binding. Each receipt advances `receipt_seq`, which receipts declare
as `after_seq` so an out-of-order one is refused rather than applied. A review binds that digest, and staging
different bytes clears the earlier review and deployment. Prativadi's verdict
binds bytes, not a location: an unread approval, a Claude Artifact, or a mutable
URL cannot replace the local review, and finalization rehashes the staged bytes
rather than trusting either receipt.

At the run-start handoff, vadi proposes this initial status page before
continuing domain work and revises it until approved. The kernel enforces this
as the join gate: `work` is not advisory until the `run_started` approval —
prativadi's first receipt, proof the pair has formed — and the pre-join wait
rests, while a finished deliverable may still be checkpointed. Then whichever participant is Codex mechanically deploys the
same digest to one stable, owner-only ChatGPT Site per run. Later approved
handoffs update that user-facing progress URL. When Codex participates, both
receipts are required at finalization; without Codex, Sites publication is
skipped and local approval is sufficient. A policy whose reviewer cannot read
its local channel is refused at `start` and repaired with `repair-policy`; the
reviewer is never asked to authenticate to the Site.
For upgrade compatibility, a complete author/reviewer receipt pair written
before `0.3.3` remains valid against the fixed policy stored with that run. If
the upgrade lands between staging and review, current vadi restages the same
digest under the role-derived policy before current prativadi reviews it.

## Terminal checks

`done` requires one current complete checkpoint whose identity,
`manifest_digest`, and `scope_revision` match the semantic approval; no pending
supersession or Human Decision; the current handoff's staged explainer bytes,
still hashing to their recorded digest; the matching approved prativadi
explainer review; and, when the pairing contains Codex, the matching owner-only
Sites deployment. Finalization records that provenance. It is the only
transition the explainer gates: checkpoint submission, review, supersession, and
approval withdrawal never wait on it. Terminal state is immutable, and both role loops stop
only after observing the same terminal Baton identity.
