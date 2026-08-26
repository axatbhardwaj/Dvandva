# Dvandva Coordination

Dvandva coordinates two independently running AI coding harnesses through a governed, durable handoff loop. The revived product is the coordination kernel, not the archived 3.5.1 workflow and distribution surface.

## Runtime

**Harness**:
A long-lived host capable of running a Dvandva role session. A running harness process is not, by itself, an active Dvandva participant.
_Avoid_: Engine, model, agent

**Role Session**:
One active Dvandva participant hosted by exactly one harness for the lifetime of a run.
_Avoid_: Process, subagent

**Run Pair**:
The exactly two independently started T3 Code role sessions that constitute a run: one Claude Lane session and one Codex Lane session.
_Avoid_: Harness launcher, interchangeable worker pool

**Run Channel**:
The durable coordination surface shared by the Run Pair. T3 Code owns where the sessions run and how they reach that surface.
_Avoid_: Remote protocol, harness-to-harness call

**Walkaway Run**:
A run in which both role sessions are started once, wait while unassigned, and continue autonomously until completion, cancellation, or a human decision.
_Avoid_: Background job, daemon

**Run Request**:
The human-authorized ticket reference or objective that enters through the Role Session holding the Human Contact Lease. It proposes run scope but never directly assigns implementation work.
_Avoid_: Assignment, model prompt

## Coordination

**Coordination Kernel**:
The small governed loop comprising durable run state, legal handoffs, evidence gates, bounded disagreement, and termination rules.
_Avoid_: Model router, workflow ring, orchestrator

**Baton**:
The single authoritative record of a run's coordination state. It communicates assignments and evidence but does not launch or manage either harness.
_Avoid_: Message, prompt, scheduler

**Handoff**:
An explicit transfer of actionable responsibility from one role session to the other through the baton.
_Avoid_: Invocation, dispatch

**Assignment Lease**:
Time-bounded, exclusive ownership of one cross-lane assignment, recoverable after its holder stops renewing it.
_Avoid_: Permanent acknowledgement, duplicate claim

**Ready Set**:
The plan items whose dependencies are integrated, whose intended write scopes do not conflict with active work, and whose approved author lane is free to accept an Assignment Lease.
_Avoid_: Backlog, model-selected task list

**Workspace Write Lease**:
The exclusive authority held by one role session at a time to modify the run's shared source workspace.
_Avoid_: Shared write access

**Canonical Worktree**:
The run's authoritative integration workspace, owned by only one Dvandva run at a time.
_Avoid_: Parallel scratch checkout

**Track Worktree**:
An isolated workspace in which one bounded plan item may produce a recoverable checkpoint without concurrently editing the Canonical Worktree.
_Avoid_: Shared target worktree

**Integration Lease**:
Exclusive authority to incorporate reviewed Track Worktree checkpoints into the Canonical Worktree.
_Avoid_: Parallel merge

**Handoff Checkpoint**:
An immutable, evidence-bearing state from which the next role session can verify and recover a handoff.
_Avoid_: Uncommitted working tree, verbal completion claim

**Dispute**:
An explicit conflict between the two role sessions after each has stated its position and supporting evidence. Requested fixes are not disputes unless the responsible role contests them.
_Avoid_: Review finding, revision request

**Adjudication**:
A third judgment produced by a Claude-hosted Fable delegate to resolve a dispute before involving the human.
_Avoid_: Majority vote, ordinary review

**Adversarial Review**:
Credited evaluation of a work product by the model family that did not author it.
_Avoid_: Self-review, same-family approval

**Human Decision**:
A request for intent or judgment that remains genuinely unresolved after adjudication.
_Avoid_: Routine approval, progress update

**Human Contact Lease**:
Exclusive responsibility for presenting a Human Decision through T3 Code. It initially belongs to the role session that created the run and may transfer after expiry.
_Avoid_: Fixed harness role, duplicate notification

## Artifacts

**Artifact Revision**:
An immutable, peer-approved content identity that remains the canonical record even when presented through a hosted viewer.
_Avoid_: Latest file, Site version

**Run Site**:
A required, persistent, owner-restricted Sites project created with exactly one Dvandva run and presenting that run's artifacts and progress through successive deployments.
_Avoid_: Repository report hub, Site per artifact

**Run Plan**:
The peer-agreed list of work items displayed as the run's to-do list. Work-item status may advance without changing the agreed scope; changing scope creates a new plan revision.
_Avoid_: Static specification, activity log

**Run Explainer**:
The live Run Site view of the Run Plan, current ownership, progress, evidence, decisions, and blockers.
_Avoid_: Final report, manually maintained dashboard

**Publication Ledger**:
The Codex-owned record of which desired Run Explainer snapshots were saved and deployed, including their Site versions and URLs.
_Avoid_: Workflow baton, second plan

**Publishable Artifact**:
An explicitly classified, peer-approved artifact revision that contains no secrets, credentials, raw private exports, or unreviewed proprietary source excerpts.
_Avoid_: Every generated file, raw evidence dump

## Lanes

**Codex Lane**:
The Sol-hosted role session responsible for adversarial review of Claude-authored work, eligible parallel work tracks, integration, verification, and Sites publication.
_Avoid_: Production Lane

**Claude Lane**:
The Opus-hosted role session responsible for adversarial review of Codex-authored work, eligible parallel work tracks, planning, Fable adjudication, and completion judgment.
_Avoid_: Review Lane

**Lane Delegate**:
A same-harness helper whose lifecycle remains private to its lane; the lane remains accountable for any outcome entered into the baton.
_Avoid_: Third Dvandva participant

**Lane Session Adapter**:
The non-model T3 integration that submits input turns only to its own existing Role Session and reports host receipts. It cannot create, resume, or address the opposite harness.
_Avoid_: Harness launcher, cross-lane invoker

## Skill activation

**Skill Activation Lease**:
A human-granted, run-scoped allowlist permitting Lane Session Adapters to dispatch named explicit-only skills at agreed phases. It never grants external-write authority or permits model-initiated invocation.
_Avoid_: Blanket model permission, skill auto-invocation

**Skill Turn Directive**:
A Baton instruction authorizing one named skill, lane, arguments reference, dispatch identity, and expected run revision as the next host input turn.
_Avoid_: Prompt suggestion, implicit invocation

**Skill Dispatch Receipt**:
Evidence that the correct host accepted a Skill Turn Directive for the intended Role Session. It proves that the skill turn started, not that the skill completed successfully.
_Avoid_: Handoff Checkpoint, skill success

**Skill Capability Manifest**:
The host-reported names, invocation policies, and content identities of the skills available to one Role Session when it joins a run.
_Avoid_: Assumed installation, mutable skill catalog
