# Dvandva V4 Kernel and Simulator Design

**Status:** Superseded by issue #3 and `docs/protocol/minimal-run-baton.md`; the simulator/controller described below was not implemented

## Goal

Build the first working v4 subproject: an independent, non-publishable Rust
workspace that can execute and persist a complete two-lane Dvandva lifecycle
against fake lane and publication adapters. It proves ticket distribution and
all governance gates without starting a harness, running a Matt skill, changing
a Git worktree, calling Sites, or reviving the v3 product.

The parent design is
`docs/superpowers/specs/2026-08-26-independent-harness-v4-design.md`.

## Repository and package boundary

- Create a separate `v4/` Rust workspace. It is not a member of `rust/`, does
  not depend on the `dvandva` 3.5.1 crate, and sets `publish = false`.
- Use the package name `dvandva-v4-kernel` for the library and
  `dvandva-v4-sim` for the executable. The names are development identities,
  not a new release line.
- Leave `rust/`, `plugins/dvandva/`, the historical HTML, installers, and all
  v3 tests unchanged.
- Use stable Rust and the smallest dependency set needed for serialization,
  content identities, temporary test directories, and portable file locking.

## Deep module and seams

`RunChannel` is the only caller-facing workflow module:

```rust
pub trait RunChannel {
    fn read(&self) -> Result<RunSnapshot, ChannelError>;
    fn apply(
        &self,
        actor: ParticipantId,
        expected_revision: Revision,
        command: Command,
    ) -> Result<RunSnapshot, ChannelError>;
    fn wait(
        &self,
        actor: ParticipantId,
        after_revision: Revision,
        deadline: Timestamp,
    ) -> Result<Observation, ChannelError>;
}
```

The module owns transition legality, compare-and-swap revisions, lease fencing,
Ready Set calculation, next-action priority, content-identity checks,
idempotency, immutable event history, publication gating, and terminal-state
rules. Callers cannot edit serialized state or manufacture actions.

Two internal seams are real because each has two adapters:

- `RunStore`: `MemoryStore` for deterministic tests and `FileStore` for two
  local processes sharing a T3-provided run directory.
- `Clock`: `FakeClock` for lease and wait tests and `SystemClock` for the
  simulator executable.

Host dispatch, Git, and Sites are represented as typed requested actions and
receipts in this subproject. They are not adapter implementations yet.

## Bootstrap state

The simulator accepts a JSON fixture containing:

- a Run Request with opaque ticket reference and objective;
- target repository identity and exact baseline commit;
- Claude and Codex participant/session identities and pinned model selections;
- the human-granted Skill Activation Lease;
- both Skill Capability Manifests;
- a proposed Run Plan; and
- scripted fake host, checkpoint, review, integration, and publication results.

Claude creates revision 0 and receives a join envelope. Codex may join exactly
once when target, baseline, policy identity, model family, and required skill
capabilities match. Duplicate roles, wrong families, mismatched policy, changed
skill identities, or absent capabilities fail without mutation.

The simulated run becomes active only after both participants bind and the fake
publisher acknowledges creation of one owner-only Run Site identity.

## Plan and deterministic distribution

Each plan item records its stable ID, approved order, description, dependencies,
normalized intended write scopes, author family, reviewer family, status,
assignment lease, checkpoint identity, evidence, review receipt, and integration
identity. Scope paths are repository-relative, normalized, and may not contain
parent traversal.

Claude proposes a plan revision; Codex alone approves it. A scope, dependency,
order, or author-family change creates a new revision. Runtime status updates do
not.

The Ready Set contains an item only when:

- the exact plan revision is approved;
- every dependency is integrated;
- its approved author lane has no active implementation action; and
- its write scopes do not equal, contain, or fall beneath an active write
  scope. An absent, repository-wide, or otherwise uncertain scope conflicts
  with every active implementation scope.

The kernel grants at most one active skill action per lane. It may grant one
Claude and one Codex implementation Assignment Lease in the same revision when
both items are independent. It never dynamically changes an item's approved
author family.

For a free lane, action classes have this order:

1. fix an accepted finding on that lane's checkpoint;
2. review an opposite-family checkpoint;
3. for Codex, integrate a reviewed checkpoint;
4. implement the earliest Ready Set item.

Within one class, dependency order wins, then approved plan order, then stable
item ID. This priority prevents a lane from starting more work while existing
work is waiting on it.

## Skill directives and checkpoints

An implementation or review action creates exactly one Skill Turn Directive
with dispatch ID, lane, skill name, arguments reference, expected revision,
participant identity, and lease epoch. The simulator's fake host accepts or
rejects it and returns a Skill Dispatch Receipt.

The same dispatch ID is idempotent: retrying it can return the recorded receipt
but cannot create another requested turn. A receipt proves only host acceptance.
The author must separately record an immutable checkpoint content identity and
verification evidence before the item becomes reviewable.

Review receipts bind the plan item, checkpoint identity, author family, reviewer
family, verdict, and evidence. Self-review, same-family review, stale checkpoint
review, or approval with unresolved blocking findings fails without mutation.
An accepted finding creates a fix action for the author. A new checkpoint
invalidates the prior review.

## Integration, publication, and completion

Only Codex may record integration of an opposite-family-reviewed checkpoint.
The integration command binds the item, approved checkpoint, resulting canonical
identity, and verification evidence. Dependencies become satisfied only after
integration.

Every accepted state transition derives an immutable `RunSiteSnapshotV1`
content identity. The fake publisher records owner-only creation and ordered
deployment receipts. Before- and after-handoff identities remain distinct, and
replaying an acknowledged snapshot is idempotent.

`done` requires all parent-spec gates, including a publication receipt matching
the exact terminal snapshot. The simulator must refuse premature completion and
print the unsatisfied gates as typed data.

## Persistence and recovery

`FileStore` keeps the current state and append-only event records inside one
explicit run directory. Each successful command acquires a portable exclusive
lock, rereads the current revision, validates the transition, writes the new
state and event to temporary files, flushes them, and atomically renames them
before releasing the lock.

The state includes a schema version and a digest over canonical serialized
content. Corrupt JSON, a digest mismatch, a missing event revision, or a state
revision that disagrees with history fails closed. Recovery never guesses or
silently truncates history.

`wait` uses filesystem notifications when available and bounded polling as a
fallback. A fake clock and memory store make timeout and lease behavior
deterministic in tests.

## Simulator interface

The executable exposes one initial command:

```text
dvandva-v4-sim run <fixture.json> --run-dir <directory>
```

It executes the scripted actors only through `RunChannel`, writes the durable
state and history, and emits newline-delimited JSON observations. Human-readable
diagnostics go to stderr. Exit `0` means the scripted run reached its expected
terminal state; fixture, transition, persistence, or expectation failures use
nonzero exits and typed JSON errors.

This is a protocol simulator, not a hidden controller or daemon. It never
spawns Claude, Codex, Git, a browser, or a network client.

## Tests

The implementation plan must cover at least:

- exact creator/join invariants and capability-manifest failures;
- stale revision and lease-epoch rejection without mutation;
- two independent tickets assigned concurrently to different lanes;
- dependency, same-path, parent-path, unknown-scope, and same-lane
  serialization;
- action priority: fix, review, integration, then new implementation;
- dispatch retry idempotency and receipt-versus-completion separation;
- opposite-family exact-checkpoint review and invalidation after a fix;
- Codex-only integration and dependency readiness after integration;
- ordered, idempotent before/after publication receipts;
- premature done rejection and exact terminal-snapshot completion;
- concurrent file-backed compare-and-swap updates;
- corrupt state/history detection; and
- a golden end-to-end fixture whose two independent tickets fan out, cross for
  review, integrate, publish, and finish.

Every behavioral test follows red-green-refactor. Existing v3 archive and
distribution-guard tests run unchanged as the final regression suite.

## Deferred subprojects

This subproject deliberately does not implement live T3 orchestration, actual
Matt skill dispatch, repository worktrees, Git commits, Fable, or Sites. Its
typed actions and receipts are the stable interfaces those later adapters must
satisfy. No fake receipt may be accepted outside the simulator/test adapters.
