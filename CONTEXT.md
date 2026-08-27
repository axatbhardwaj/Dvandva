# Dvandva Coordination

Dvandva coordinates one autonomous run shared by two independently started
harness sessions. The v3 product and its distribution are a retired archive.

## Language

**Run Pair**:
One worker Role Session and one reviewer Role Session from different harness
families. Semantic roles do not determine harness-specific publication duty.

**Role Session**:
One independently started harness session participating as worker or reviewer.
Neither session launches or invokes the other.

**Run Channel**:
The isolated coordination context belonging to one Run Pair.

**Coordination Kernel**:
The authority that determines which role actions and state changes are valid.
It is distinct from a harness launcher, model router, or goal manager.

**Baton**:
The authoritative evolving record of one Run Pair.

**Canonical Scope**:
The agreed objective, references, task identity, and required deliverables for
one run.

**Checkpoint Manifest**:
The complete set of immutable deliverable references and verification evidence
covering every required deliverable in Canonical Scope exactly once.

**Checkpoint Binding**:
The exact immutable checkpoint to which findings or approval apply.

**Checkpoint Supersession**:
A pending request to replace the current immutable checkpoint after new work is
found during review.

**Approval Withdrawal**:
Retraction of approval after new required work makes the approved checkpoint
incomplete.

**Protocol Upgrade**:
One-way adoption of a newer run contract while retaining the prior run as
provenance.

**Handoff**:
An assignee change whose publication obligation binds the current scope and,
when present, the current Checkpoint Binding.

**Publication Gate**:
The requirement that the current explainer deployment and its independent
review describe the same Handoff before the run advances.

**External Reference**:
A stable identity for a deliverable or deployment outside the Baton.

**Human Decision**:
An explicit pause containing one unresolved question, evidence, options,
designated contact, and exact resume target.

**Participant Claim**:
A Role Session's exclusive authority to act for one semantic role.

**Harness Goal**:
User-owned prompt context outside Dvandva state. A role neither creates nor
changes it while joining or completing a run.

**Worker**:
The semantic role that produces complete checkpoints and finalizes an approved
Checkpoint Binding.

**Reviewer**:
The semantic role that adversarially reviews the exact Checkpoint Binding and
records findings or approval.
