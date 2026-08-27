# Dvandva v0.2 Run Protocol Hardening Design

**Status:** Approved in conversation on 2026-08-27; implementation pending

## Objective

Harden the active skill-only Dvandva run so two independently launched T3 Code
sessions can converge autonomously on one complete deliverable. The protocol
must prevent the incident in which a worker produced a second deliverable
outside its turn, published that unreviewed work separately, and a reviewer
approved the older checkpoint that remained in the only votable slot.

The normal ticket casting remains Codex as vadi and Claude as prativadi, but
semantic role and harness-specific publication duty are independent:

- `vadi` is the semantic worker;
- `prativadi` is the semantic reviewer;
- the Codex-harness participant publishes the run explainer to Codex Sites;
- the Claude-harness participant reviews the exact deployed explainer after
  every handoff.

The human starts both sessions. Neither harness invokes the other. Harness
goals are user-owned prompt context and are outside the Dvandva protocol.

## Non-goals

This release does not add a daemon, a central orchestrator, T3 wake-up, Claude
Artifacts publication, a generic publication fallback, harness-to-harness
invocation, automatic invocation of third-party skills, or goal management.
It does not revive or modify the archived v3 plugin or crate.

The kernel cannot infer deliverables from prose. V2 therefore makes the
vadi declare a non-empty list of required deliverable IDs and descriptions as
part of canonical scope. The kernel can then require a checkpoint manifest to
cover that declared list exactly, bind every decision to its digest, and leave
the reviewer to judge whether the declaration itself faithfully represents
the prose objective.

## Protocol epoch and migration

The new schema is `dvandva.run.v2`; the role-facade API is `2`. This is a
security boundary, not a cosmetic version bump. A `0.1.1` kernel ignores
unknown Serde fields and can otherwise rewrite a baton while silently dropping
the new scope, manifest, and publication gates.

The `0.2.0` kernel:

1. creates only v2 runs;
2. reads v1 runs only to identify them and perform a dedicated upgrade;
3. rejects ordinary v1 claim, heartbeat, wait, and semantic mutation paths;
4. requires facade API `2` on every role-facing command;
5. reports only v2 as probe-compatible, while advertising v1 migration as a
   separate capability;
6. accepts history with at most one monotonic `v1 -> v2` edge and never a
   downgrade.

New v2 creation and v1 upgrade both require exactly two normalized
participants: one `Codex` harness and one `Claude` harness. Missing, duplicate,
or other harness topologies are rejected before creating or upgrading a run;
the fixed publication gate must never be initialized without both actors.

An exact v1 selection returns `upgrade_required` without claiming it. The
participant invokes the dedicated atomic upgrade using its role, harness,
session, expected revision, and facade API. The upgrade refuses a terminal
run or a live same-role claim owned by another session. It then:

- preserves objective, repository/task identity, history, and the legacy
  checkpoint as migration provenance only;
- initializes one required `legacy_objective` deliverable whose description is
  the canonical objective summary, so migration is deterministic without
  pretending to infer finer-grained scope; the roles amend scope before work
  when the legacy objective requires multiple separately votable outcomes;
- clears the active checkpoint, semantic review, human decision, both
  participant claims, and every legacy publication receipt;
- installs the fixed v2 publication policy and a fresh pending
  `protocol_upgraded` explainer obligation;
- routes the semantic state to `revising/worker`;
- writes the v2 revision by CAS and forces both sessions to claim again.

Stale credential files never authorize a claim. A role start may replace its
own stale credential only after the baton proves that its stored claim is
absent or has a different epoch/token digest.

Recovery validates mixed history but never rolls semantic state backward. V1
recovery is rejected because v1 is read-only except for dedicated upgrade. If
the validated history head is v2, recovery may select only that exact v2 head
(`from_revision` must equal `high`) and writes a claim-cleared recovery
successor; it never reinstalls an earlier v2 revision. Once a v2 head exists,
recovery from v1 or any earlier v2 revision is rejected. A crash after the v2
history write but before head installation remains recoverable from that v2
head through this successor write.

## Canonical scope

Every v2 baton carries `scope_revision`, beginning at `0`, plus a non-empty
ordered list of unique `scope_deliverables` with stable IDs and non-blank
descriptions. The objective, references, and declared deliverables in the
baton are authoritative. New run creation must declare at least one
deliverable; an ordinary implementation ticket may use one `implementation`
bundle, while a multi-report review declares each separately.

An exact `--run-id` selection behaves as follows:

- no supplied objective or scope coordinate: select the run and return its
  canonical objective and scope;
- every explicitly supplied coordinate matches canonically after required
  normalization: select normally;
- a different supplied objective summary, objective reference, task reference,
  or required-deliverable declaration: return `scope_mismatch` with the run
  ID, canonical and supplied coordinates, scope revision, status, assignee,
  and next safe action; do not claim, reclaim, or mutate;
- missing exact run: return `run_missing` immediately;
- live claim owned by another session: return `busy` immediately.

Repository and participant-harness mismatches remain fail-closed. Exact run
selection never waits for discovery and never treats its ID as permission to
adopt a different objective.

Either role may request a Human Decision when new human scope conflicts with
the baton. Only the designated contact can resume it. A resume may carry a
new objective, references, and complete required-deliverable declaration. A
scope amendment atomically increments `scope_revision`, clears the checkpoint,
semantic review, and supersession, routes to `revising/worker`, and creates a
fresh explainer obligation. A plain answer without a scope update resumes the
declared state as before.

## Complete immutable checkpoints

A v2 checkpoint contains:

- `kind` and immutable `identity`;
- a non-empty `deliverables` manifest whose unique IDs exactly cover the
  canonical `scope_deliverables` and whose entries contain typed immutable
  external references;
- non-empty verification evidence;
- the kernel-stamped `scope_revision`;
- a kernel-computed SHA-256 `manifest_digest` over a canonical serialization
  of kind, identity, ordered deliverables, verification, and scope revision.

The submission action accepts the declaration but never trusts a caller-
supplied digest or scope revision. The kernel trims and validates all values,
compares the manifest ID set to canonical scope, computes the binding over a
stable ID ordering, and rejects duplicate identities, missing/extra IDs, or an
empty manifest.

A semantic review receipt binds all three checkpoint coordinates:
`identity`, `manifest_digest`, and `scope_revision`. Approval is illegal when
any coordinate is stale, a checkpoint-supersession request is pending, or the
current handoff explainer has not been approved.

The skills treat the manifest as the complete outcome for the current scope.
The vadi must not submit a partial deliverable merely because one artifact is
ready. The prativadi first reconciles the declared scope deliverables against
the prose objective, then reviews the exact manifest and requests a scope
amendment or checkpoint changes for an omission.

## Checkpoint supersession

Wrong-owner enforcement remains strict: a worker cannot replace a checkpoint
while the reviewer owns `reviewing`. Two explicit escape paths remove the
incentive to route new work around the protocol:

1. During `reviewing`, the worker may request checkpoint supersession with a
   non-blank reason. The immutable checkpoint remains reviewable, but semantic
   approval is blocked while the request is pending. The reviewer can accept
   the request, which clears checkpoint/review/supersession, routes to
   `revising/worker`, and creates a new handoff explainer obligation.
2. During `finalizing`, the worker may withdraw the approval directly with a
   non-blank reason. This clears checkpoint/review, routes to
   `revising/worker`, and creates a new obligation. Finalization cannot seal a
   newly discovered deliverable behind an old approval.

The existing expected-revision CAS decides races. If approval wins before the
supersession request, the request conflicts and the worker uses withdrawal. If
the request wins first, approval is rejected until the reviewer accepts it.

## One rolling explainer gate

Publication is an orthogonal substate, not a numeric revision counter and not
worker-owned. Every v2 run has one explainer Site under this fixed policy:

```text
publisher harness: Codex
channel: Codex Sites
access: owner-only
reviewer harness: Claude
```

Semantic role reversal does not reverse these duties. The kernel derives the
calling harness from the claimed participant; callers cannot self-assert a
harness family in an action payload.

The initial v2 baton contains a pending `run_started` obligation so the vadi
can immediately return a run ID and the Codex participant can create the Site
before the first semantic checkpoint. Each semantic handoff replaces the
pending obligation with a new exact binding:

- `run_started`;
- `worker_to_reviewer` after checkpoint submission;
- `reviewer_to_worker` after changes requested or approval;
- `scope_amended` after a human scope update;
- `checkpoint_superseded` after reviewer acceptance;
- `approval_withdrawn` after worker withdrawal;
- `protocol_upgraded` after v1 migration.

Useful domain work and checkpoint inspection may happen in parallel with
publication. The next semantic mutation that crosses a handoff boundary is
blocked until the current obligation has both a deployment and an approved
Claude review. Concretely:

- first checkpoint submission waits for `run_started` approval;
- semantic review submission waits for `worker_to_reviewer` approval;
- the next revised checkpoint waits for `reviewer_to_worker` approval;
- finalization waits for the approval handoff's explainer approval.

### Deployment binding

A Codex publication receipt must exactly echo the pending obligation's
handoff revision, handoff kind, scope revision, and optional checkpoint
binding. It also records:

- SHA-256 digest of the explainer source;
- stable Site ID (one Site per run);
- Site version/deployment ID;
- deployment URL;
- `codex_sites` channel and `owner_only` access;
- publisher harness derived by the kernel.

All strings are non-blank and digests are lowercase 64-character SHA-256
hex. A later deployment for the same obligation must preserve the Site ID.
Recording any deployment clears the prior Claude review.

### Claude review binding

A Claude explainer receipt echoes the exact obligation, source digest, Site
ID, Site version, and URL from the current deployment. It records an approved
or changes-requested verdict, findings, and the reviewer harness derived by
the kernel. Changes requested require non-empty findings; approval forbids
findings. A changes-requested explainer verdict does not reopen the semantic
checkpoint: Codex republishes and Claude reviews the new exact version.

The publication gate is satisfied only by an approved receipt whose complete
binding equals the current obligation and deployment. A mutable URL alone, a
Claude Artifact, a local HTML file, or a publication revision number cannot
satisfy it.

No in-run publication-policy override exists in v0.2. When Codex Sites is
unavailable, the role raises a Human Decision and the run remains blocked; a
Claude Artifact, generic local/hosted surface, or other alternative cannot
satisfy the v0.2 gate or finalization. An explicitly desired alternative
requires a future protocol/run version.

## Role snapshots and next actions

The kernel centrally computes `next_actions` from the whole baton, the
participant's semantic role, and its harness. `role start`, `role read`,
`role apply`, and `role wait` return the same role snapshot shape. It includes
the full baton plus:

- canonical objective and scope revision;
- status, assignee, run directory, and revision;
- zero or more legal protocol actions for this participant;
- a concise reason when a semantic action is publication-blocked;
- for vadi start, a ready-to-paste exact-run prativadi prompt.

The action vocabulary includes `upgrade_protocol`, `publish_explainer`,
`review_explainer`, `work`, `submit_checkpoint`, `review_checkpoint`,
`record_review`, `request_checkpoint_supersession`,
`accept_checkpoint_supersession`, `withdraw_approval`, `answer_human`,
`finalize`, and `wait`.

`role wait` first evaluates the current snapshot. If the caller already has a
legal mutation or assigned domain action, it returns immediately instead of
hiding role misuse behind a filesystem wait. Otherwise it retains the claim,
waits locally, and re-evaluates after every authoritative read. Exact-run
missing, mismatch, and busy outcomes never enter a discovery wait.

## Skill behavior

On vadi activation, the first user-visible protocol output—before domain work—
contains the run ID, canonical objective/scope, status/assignee, current next
actions, and this copyable prompt:

```text
Act as prativadi and join Dvandva run <run-id>.
```

Both skills obey kernel `next_actions`; they do not infer ownership from prose.
In particular:

- a vadi attached to `reviewing/reviewer` does not perform worker-domain work;
- new scope triggers a Human Decision before deliverable production;
- out-of-turn newly discovered work triggers supersession or withdrawal;
- a publication surface never substitutes for the immutable checkpoint;
- each semantic handoff is projected to the run's Codex Site by the Codex
  participant and reviewed by the Claude participant;
- the explainer contains the canonical objective, complete deliverable
  manifest, current findings/decisions, and plan as an up-to-date TODO list;
- no skill creates, replaces, completes, pauses, or clears a harness goal.

Third-party skills remain human-invoked. If the user explicitly invokes one
inside an attached role session, its output still enters the same checkpoint
manifest and handoff gates.

## Invariants and terminal gate

The kernel enforces these invariants on every v2 mutation:

1. terminal state is immutable;
2. schema never downgrades;
3. scope revision never decreases;
4. an active semantic review binds the current complete checkpoint;
5. a pending supersession forbids approval;
6. one stable Codex Sites Site ID serves the run;
7. every deployment and explainer review binds the current obligation exactly;
8. only the policy publisher harness records deployment;
9. only the policy reviewer harness records explainer review;
10. republishing invalidates explainer approval;
11. a semantic handoff cannot advance past a stale explainer gate;
12. `done` requires current scope, current checkpoint, exact semantic approval,
    exact Codex Sites deployment, and exact Claude explainer approval.

Finalization records terminal provenance only after all twelve checks pass.
Both role loops observe the terminal baton and stop; they need not exit in the
same process instant.

## Release and verification

The private kernel version becomes `0.2.0`; the skills release becomes
`skills-v0.2.0`. Setup installs the checksummed binary atomically and reports
the facade API, current schema, and v1-migration capability. It never mutates
runs during install/update.

Release verification must cover:

- old facade + new kernel rejection before run mutation;
- new facade + old kernel rejection before run mutation;
- v1 upgrade fencing a live old waiter and clearing stale credentials;
- exact-v2-head-only recovery and crash-window recovery without rollback;
- creation/upgrade rejection for missing, duplicate, or non-Codex/Claude
  normalized harness topology;
- exact-run mismatch for every explicitly supplied objective, reference, task,
  and deliverable coordinate;
- the stale-checkpoint incident as a black-box regression;
- both normal and reverse semantic casting with Codex publication and Claude
  explainer review unchanged;
- pressure tests showing the skills choose scope reconciliation and
  supersession rather than out-of-turn work;
- user-owned goal non-interference;
- format, lint, all v4 tests, archived v3 tests, shell syntax, skill package,
  and install/update/uninstall canaries.
