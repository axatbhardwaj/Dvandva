# Independent-Harness Dvandva V4 Design

**Status:** Superseded by issue #3 and `docs/protocol/minimal-run-baton.md`; retained as design history

## Objective

Build a small Dvandva successor for exactly two independently started T3 Code
sessions: Claude Session A and Codex Session B. They coordinate through durable
run state, never launch or impersonate one another, and keep an owner-only Run
Site current throughout the run.

## Repository boundary

The `v3.5.1` tag and every historical distribution surface remain frozen. V4
is developed under `v4/` as a non-publishable successor with independent
manifests, tests, lane instructions, and protocol contracts. It does not depend
on or register the retired crate or plugin.

Local v4 development does not authorize a real Dvandva run, plugin install,
marketplace restoration, GitHub unarchive, push, pull request, merge, release,
crate publication, or public/shared Site deployment. Those remain separate
actions.

## Runtime topology

- A run contains exactly one Claude Lane session and one Codex Lane session.
- Claude Session A creates the run. Codex Session B joins the declared run ID.
- T3 Code owns session placement, remote compatibility, and the durable
  per-run directory. T3 also owns each non-model Lane Session Adapter. Dvandva
  neither discovers nor manages transport.
- The run becomes active only after both sessions prove the same target,
  baseline, policy identity, and model pins.
- The creator initially holds the Human Contact Lease. The other session may
  reclaim it only after expiry.
- A dead lane may be replaced only by a new session from the same harness and
  model family after its participant lease expires.
- Neither lane executes a Claude or Codex CLI, SDK, process, or model on behalf
  of the other. Their only cross-harness interface is the Run Channel.
- Each Lane Session Adapter may submit a turn only to its own already-running
  session. It cannot create, resume, discover, or address the opposite session.

## Human-facing start and ticket intake

Claude Session A is the default human-facing front door because Claude owns
planning. Starting a run requires exactly two human inputs:

1. In Claude Session A, the human supplies a Run Request, grants the initial
   Skill Activation Lease, and asks Claude to create the run.
2. Claude returns a non-secret join envelope containing the run ID, target,
   baseline, and policy identity. The human submits that envelope once to Codex
   Session B and asks Codex to join.

This one copied join envelope is intentional: neither session may address the
other, and Dvandva does not pretend the second independently started session
already exists. After Codex joins, both capability manifests match, and Codex
creates the owner-only Run Site, the run becomes walkaway-capable. No further
human input is required unless a skill asks a genuine question, a Human
Decision forms, or new external-write authority is needed.

A Run Request is an opaque ticket URL or identifier plus the human's objective;
the kernel does not fetch or interpret issue trackers. Claude may use its own
authorized tools to gather the ticket and repository context before running
the planning skills. Failure to resolve the ticket becomes a human-facing
question rather than guessed scope.

Only the Role Session holding the Human Contact Lease may accept a
scope-changing Run Request. If the human sends one to the other session, that
session identifies the current contact but does not forward, duplicate, or
silently apply it. A request naming an existing plan item may change priority
without changing scope. A new ticket or changed objective creates a proposed
plan revision and requires Codex approval before it can be assigned.

## Models and native delegation

- Claude Lane: an effective Opus model at medium reasoning.
- Codex Lane: an effective Sol model at medium reasoning.
- Adjudicator: a Claude-native Fable delegate at medium reasoning.
- Effective model identifiers and effort are pinned when the Run Pair forms.
- A missing required model is retried only when the failure is demonstrably
  temporary; there is no silent substitution.
- Same-harness delegates remain private to their lane. The lane records their
  output and evidence, not their lifecycle, in shared state.

## Matt skill activation

Dvandva is the outer cross-family controller; Matt Pocock's skills provide the
inner engineering discipline within each lane. The one-time installation and
setup of those skills is a human prerequisite, not part of a run.

Matt's top-level workflow skills remain explicit-only. They are never made
implicitly model-invocable and are not considered invoked when mentioned in a
prompt, plan, handoff, or assistant response. At run start, the human may grant
one Skill Activation Lease naming the permitted skills, lanes, phases, and
scope. The lease permits deterministic host dispatch, not model choice.

For each permitted activation, the Run Channel issues one Skill Turn Directive
containing a unique dispatch identity, lane, skill name, arguments reference,
expected run revision, and lease epoch. The owning non-model Lane Session
Adapter then submits a real input turn to that same lane's existing session:

- Claude Agent SDK: `/<skill> <arguments>`;
- Codex app server: `$<skill> <arguments>` plus the exact structured `skill`
  item reported by `skills/list`.

The adapter records a Skill Dispatch Receipt only after the host accepts the
turn. That receipt proves turn start, not successful completion; a resulting
Handoff Checkpoint and evidence are still required. Dispatch identities are
idempotent, fenced to one role session and revision, and visible on the Run
Site. A repeated directive cannot start a second turn.

When a lane joins, it supplies a Skill Capability Manifest. The run fails
closed before activation if a required skill is missing, has changed content
identity, or is no longer explicit-only. There is no silent fallback to an
implicit skill, copied skill text, another model, or another harness.

The default skill sequence is:

1. Claude: `grill-with-docs`, then `to-spec`, then `to-tickets`.
2. Codex: adversarial review of the exact plan revision.
3. Claude and Codex: `implement` once per assigned independent ticket in their
   respective Track Worktrees.
4. The opposite family: `code-review` against each exact checkpoint.
5. Codex: integration, verification, and Run Site publication.

Matt's model-invocable helper skills may be chosen normally inside the owning
lane. The same-family review inside `implement` is a useful preflight but does
not satisfy Dvandva's opposite-family Adversarial Review gate. `implement-spec`,
`claude-handoff`, and in-run `handoff` are prohibited because they duplicate
the outer controller or launch/couple harnesses.

Questions that a skill genuinely directs to the human are routed by the Human
Contact Lease; the adapter may not fabricate an answer. Skill activation never
grants push, pull-request, merge, release, publication, or other external-write
authority. This delegated dispatch deliberately extends Matt's literal
human-typing convention and is recorded in ADR 0002; strict human-typed
dispatch remains the fallback.

## Run Channel

`RunChannel` is the deep coordination module. Its caller-facing interface is:

```rust
RunChannel::read() -> RunSnapshot
RunChannel::apply(actor, expected_revision, Command) -> RunSnapshot
RunChannel::wait(actor, after_revision, deadline) -> Observation
```

The module owns typed validation, compare-and-swap revisions, fenced leases,
atomic persistence, immutable history, actionability, and wakeups. Callers
submit commands; they never rewrite the state document.

Production uses the T3-provided per-run directory. Tests use an in-memory
store and fake clock through internal seams. The v4 kernel reuses the proven
concepts of atomic rename, fencing tokens, and notify-with-polling fallback,
but none of the v3 baton vocabulary, profile graph, model router, installers,
hooks, or shell output grammar.

## Generic plan and parallel work

V4 has one plan-driven workflow rather than historical modes or profiles.
Claude authors the initial Run Plan and Codex adversarially approves the exact
plan revision before implementation begins.

The tickets produced by `to-tickets` are the agreed scope and dependency input.
The Run Plan references their stable identities and exclusively owns runtime
status, lane assignment, review state, integration state, and publication
state. Ticket prose and the Baton never become competing progress ledgers.

Each plan item has a stable ID, description, dependencies, status, intended
scope, normalized intended write scopes, author family, reviewer family,
worktree reference, checkpoint content identity, evidence, and review receipt.
Claude proposes both the decomposition and the author-family split; Codex
approves or rejects that exact revision. Status and evidence may advance
without changing scope. Adding, removing, redefining, reassigning authorship,
or changing dependencies creates a new plan revision that requires
opposite-family review.

The kernel never invents a model-based work split. It computes a Ready Set from
the approved plan. An item is ready only when all dependencies are integrated,
its intended author lane has no active implementation assignment, and its write
scopes do not equal, contain, or fall beneath an active scope. Unknown or broad
scopes conflict and therefore serialize.

When both lanes have independent ready items, the Run Channel grants both
Assignment Leases in one revision so the adapters may dispatch `implement` in
parallel. Otherwise it grants only the safe item. Within a lane, blocking fixes,
opposite-family reviews, and Codex integration outrank new implementation;
items in the same class follow dependency order, then approved plan order, then
stable item ID. Each lane has at most one active skill turn, while private
same-harness delegates remain the lane's implementation detail.

An implementation assignment becomes a same-lane Skill Turn Directive. The
Lane Session Adapter submits `implement <item-and-context-reference>` to its
bound session and records only host acceptance. The author must later attach an
immutable checkpoint and verification evidence. The opposite lane then
receives the credited review assignment for that exact checkpoint. Accepted
fixes repeat author then reviewer; reviewed checkpoints enter Codex's
integration queue. Neither lane chooses its next assignment from prose.

One Dvandva run owns the Canonical Worktree. Independent items may execute in
parallel only in isolated Track Worktrees after their dependencies are
satisfied. Uncertain dependencies and overlapping integration surfaces are
serialized. Codex holds the Integration Lease and incorporates only reviewed
checkpoint commits into the Canonical Worktree.

Every work product receives credited review from the family that did not
author it. Review receipts bind to the exact content identity; self-review and
stale-checkpoint review are invalid. Claude reviews Codex-authored tracks and
integration behavior. Codex reviews Claude-authored tracks. Completion requires
opposite-family coverage for every integrated checkpoint.

## Handoffs and Git

Every source-changing handoff names an immutable Git checkpoint; artifact-only
handoffs name a content hash. Each handoff records what changed, verification,
unresolved risks, next owner, and next action.

The shared Git policy is `docs/protocol/v4-git-discipline.md`: granular semantic
commits around 200 changed lines, recoverable/cherry-pickable boundaries, and
`gh stack` for stack workflows. Local checkpoint authority never implies push,
PR, merge, release, or source-publication authority.

## Disputes and human decisions

An accepted review finding follows the fix/review loop. If the same finding
survives two genuine fix attempts, or the author explicitly contests it after
one evidence-backed exchange, it becomes a Dispute.

Claude may invoke a Claude-native Fable delegate to adjudicate. Fable may
clarify, propose a fix, or uphold an opposite-family finding. Fable cannot
unilaterally waive a Codex finding against Claude-authored work: dismissal
requires objective evidence accepted by Codex. Fable is binding for process or
interpretation disputes that do not waive credited review. Missing intent,
insufficient evidence, irreducible product/safety trade-offs, or an unresolved
opposite-family finding becomes a Human Decision.

## Run Site and publication

Every run has exactly one persistent owner-only ChatGPT Site. Codex is the sole
publisher. Claude changes desired publication state through the Run Channel and
never calls Codex or Sites.

The Run Explainer shows the plan as a live to-do list plus current ownership,
progress, verification, evidence links, decisions, adjudication summaries,
artifacts, timestamps, and blockers. It excludes prompts, transcripts, hidden
reasoning, secrets, credentials, raw private exports, and unreviewed proprietary
source excerpts.

The kernel emits immutable `RunSiteSnapshotV1` documents. The T3 Codex adapter
implements one deep interface:

```text
syncRunSite(snapshot) -> receipt
```

The adapter owns Site creation, source generation, validation, save/deploy,
polling, retry, and idempotency. It always uses owner-only access unless a human
separately authorizes broader access.

Actual Site identity and deployment outcomes live in a Codex-owned Publication
Ledger, separate from workflow state. Idempotency is keyed by snapshot content
identity. The ledger persists each acknowledged remote stage and accepts
receipts only from the Codex Lane.

Codex publishes the completed-owner snapshot before every handoff and the new
owner snapshot after it. These two identities are never coalesced. During a
confirmed Sites outage, the snapshots remain durably ordered and productive
handoffs may continue; Codex replays them after recovery. A matching deployed
terminal snapshot is always required for `done`.

The Site is created when Codex joins, before the run becomes active. It remains
owner-only after completion, cancellation, or abandonment and is never deleted
automatically. Site limits, permission failures, or an unavailable final
deployment become a Human Decision.

## Completion and cancellation

`done` requires all of the following on the same final revision:

- every plan item is terminal with evidence;
- required verification passes;
- every integrated checkpoint has opposite-family review;
- no Dispute or Human Decision remains open;
- Claude and Codex approve the same final checkpoint;
- the exact terminal Run Site snapshot is deployed owner-only.

Cancellation or abandonment releases leases, records the reason, and publishes
the terminal state when Sites is reachable. The owner-only Site and local run
record remain durable.

## Shared instruction policy

Both lane adapters point to the same v4 protocol and Git discipline. Each lane
also obeys its native T3 and repository instruction hierarchy. A higher-priority
rule that makes the workflow impossible is surfaced as a Human Decision rather
than bypassed.

The adapters also share the same pinned required-skill manifest. The default
run cannot begin until the human grants its Skill Activation Lease and both
hosts report matching required capabilities.

## Delivery decomposition

The successor is delivered as independently testable subprojects rather than
one large activation:

1. **Kernel and simulator.** A non-publishable Rust workspace under `v4/`
   implements the Run Channel, typed state transitions, deterministic Ready Set,
   skill directives and receipts, cross-family review gates, publication
   snapshots, an atomic file adapter, and an end-to-end simulator using fake
   lane and publisher adapters. It performs no real T3, Git, Sites, or skill
   actions.
2. **T3 lane adapter.** A separately reviewed adapter binds two declared T3
   threads, subscribes to their settled state, and submits exact same-thread
   turns. Current T3 internals expose client-dispatchable
   `thread.turn.start` through `orchestration.dispatchCommand`, but the proposed
   public local-app SDK is not yet an accepted dependency. This milestone must
   live in an authorized T3 integration checkout or target a released public
   SDK; Dvandva will not hand-roll private RPC framing. See T3's
   [provider architecture](https://github.com/pingdotgg/t3code/blob/main/docs/internals/providers.md)
   and the pending
   [local-app SDK RFC](https://github.com/pingdotgg/t3code/issues/6419).
3. **Workspace and Git adapter.** Track Worktrees, checkpoint validation,
   opposite-family review bindings, and Codex-only canonical integration are
   connected to the kernel behind tested adapters.
4. **Run Site adapter.** The bounded explainer projection and Codex-owned Sites
   ledger are connected and verified owner-only with fake publication first.
5. **Synthetic two-thread proof.** After separate authorization, two disposable
   T3 threads and a private fixture repository exercise one complete run with
   no proprietary data or remote source mutation.

The first implementation plan covers only subproject 1. Its approved design is
`docs/superpowers/specs/2026-08-26-v4-kernel-simulator-design.md`. Later plans
must consume the kernel's interfaces rather than bypassing its state machine.

## Acceptance criteria

- V3 archive and distribution guards still pass unchanged.
- V4 manifests are non-publishable and do not depend on the legacy crate or
  plugin.
- Exactly one Claude and one Codex session can bind; duplicates and wrong-family
  replacements fail closed.
- Stale revisions, stale lease epochs, corrupt state, overlapping active write
  scopes, self-review, and stale review receipts are rejected without mutation.
- Concurrent independent track updates survive read/retry and every successful
  revision has immutable history.
- A new Run Request enters only through the Human Contact Lease holder; it
  cannot become an assignment before an opposite-family-approved plan revision.
- The deterministic Ready Set permits two assignments only for dependency-ready,
  lane-compatible, non-overlapping write scopes and serializes unknown scopes.
- A free lane prioritizes fixes, credited review, and integration over starting
  more implementation work.
- Active v4 adapters and runtime never spawn or invoke the opposite harness.
- A Skill Turn Directive can target only its bound lane and revision; repeated
  dispatch is idempotent, and assistant prose cannot satisfy the directive.
- Required explicit-only skills are capability-checked before activation;
  missing, changed, or policy-incompatible skills fail closed without fallback.
- Skill Dispatch Receipts prove host acceptance only; completion and review
  gates still require exact content identities and evidence.
- Synthetic Claude and Codex host adapters prove the approved command shapes,
  session fencing, and separation of skill activation from external writes.
- Publication snapshots and ledger receipts are separately owned, content
  addressed, idempotent, and preserve before/after handoff identities.
- The generated mobile explainer contains only the bounded approved projection.
- A fake publisher proves completion gating; a later separately authorized
  synthetic T3 proof validates one real owner-only Site without private data.
