# Dvandva Coordination

Dvandva v4 is a minimal coordination kernel for one autonomous run shared by
exactly two independently started harness sessions. The v3.5.1 product and its
distribution remain a retired archive.

## Runtime vocabulary

**Run Pair**: one worker Role Session and one reviewer Role Session. The
default casting is Codex worker and Claude reviewer, but the protocol names
roles rather than products.

**Role Session**: one independently started harness session holding a fenced,
time-bounded participant claim. Neither session launches or invokes the other.

**Walkaway Run**: a Run Pair that waits locally and alternates until completion,
abandonment, or a Human Decision.

**Run Channel**: one run-local directory containing the authoritative Baton,
immutable history, and a lock. Two runs never share a channel or global lock.

**Coordination Kernel**: schema validation, claims, legal transitions,
compare-and-swap persistence, recovery, and local wake-up. It is not a model
router, tracker scheduler, or harness launcher.

**Baton**: the single `dvandva.run.v1` JSON state for one run.

**Handoff**: an assignee change accepted through the Baton.

**Handoff Checkpoint**: an immutable Git or artifact identity with verification
evidence. Reviews bind that exact identity.

**Adversarial Review**: evaluation by the harness family that did not author
the checkpoint.

**Human Decision**: an explicit pause containing the unresolved question,
evidence, options, designated contact, and exact resume target.

**Participant claim**: a role/session binding protected by an expiring lease,
epoch, and secret-token digest. Replacement fences the earlier token.

**Worker**: authors checkpoints, applies revisions, maintains optional
publication projections, and finalizes an approved identity.

**Reviewer**: reviews the current checkpoint and either requests actionable
changes or approves it.

Planning, grilling, specification, ticket creation, and explicit-only Matt
Pocock skills happen outside the runtime protocol. Trackers and published
explainers are optional projections; neither assigns work nor wakes a session.
