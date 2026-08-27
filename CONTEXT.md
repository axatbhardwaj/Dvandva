# Dvandva Coordination

Dvandva coordinates one autonomous run shared by two independently started
harness sessions. The v3 product and its distribution are a retired archive.

## Language

**Run Pair**:
One worker Role Session and one reviewer Role Session from different harness
families. Semantic roles do not determine harness-specific publication duty.

**Role Session**:
One independently started harness session holding a fenced, time-bounded
participant claim. Neither session launches or invokes the other.

**Run Channel**:
The isolated authority and history namespace for one Run Pair. Separate runs
never share a channel.

**Coordination Kernel**:
The authority for claims, transitions, bindings, recovery, and legal role
actions. It is distinct from a harness launcher, model router, or goal manager.

**Baton**:
The single `dvandva.run.v2` state for one Run Pair. V1 Baton state is legacy
input accepted only by the dedicated one-way upgrade.

**Canonical Scope**:
The Baton-owned objective, references, task identity, scope revision, and
non-empty set of required deliverables. Exact-run selection cannot amend it.

**Checkpoint Manifest**:
The complete set of immutable deliverable references and verification evidence
covering every required deliverable in Canonical Scope exactly once.

**Checkpoint Binding**:
The checkpoint identity, kernel-derived manifest digest, and scope revision
that together identify the exact object reviewed and approved.

**Checkpoint Supersession**:
A pending request to replace the current immutable checkpoint after new work is
found during review. Approval Withdrawal is the corresponding finalizing path.

**Handoff**:
An assignee change whose publication obligation binds the current scope and,
when present, the current Checkpoint Binding.

**Publication Gate**:
The fixed requirement that the Codex harness deploy one stable owner-only Codex
Site per run and the Claude harness review the exact current deployment.

**External Reference**:
A typed coordinate asserted by a participant for a Git object, artifact, or
Site deployment. The kernel binds its structure but does not resolve every
external system.

**Human Decision**:
An explicit pause containing one unresolved question, evidence, options,
designated contact, and exact resume target.

**Participant Claim**:
A role/session binding protected by an expiring lease, epoch, and secret-token
digest. Replacement fences the earlier credential.

**Harness Goal**:
User-owned prompt context outside Dvandva state. A role neither creates nor
changes it while joining or completing a run.

**Worker**:
The semantic role that produces complete checkpoints and finalizes an approved
Checkpoint Binding.

**Reviewer**:
The semantic role that adversarially reviews the exact Checkpoint Binding and
records findings or approval.
