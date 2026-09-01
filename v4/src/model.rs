use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use thiserror::Error;

pub const LEGACY_SCHEMA: &str = "dvandva.run.v1";
pub const SCHEMA: &str = "dvandva.run.v2";
pub const ROLE_API: u32 = 2;
pub const CODEX_HARNESS: &str = "Codex";
/// Digest-bound explainer bytes staged inside the run directory. Both harnesses
/// run locally against the same run directory, so both can read this channel.
pub const EXPLAINER_CHANNEL: &str = "run_artifact";
pub const EXPLAINER_ACCESS: &str = "run_private";
/// Human-facing rendering required only when a run contains Codex.
pub const SITES_CHANNEL: &str = "codex_sites";
pub const SITES_ACCESS: &str = "owner_only";
/// Recognized but reviewer-unreadable: an owner-only Codex Site is gated behind
/// the publisher owner's session, which the reviewing harness cannot present.
pub const LEGACY_EXPLAINER_CHANNEL: &str = SITES_CHANNEL;
pub const LEGACY_EXPLAINER_ACCESS: &str = SITES_ACCESS;
pub const EXPLAINER_ARTIFACT_DIR: &str = "explainer";
pub const ANALYSIS_ARTIFACT_DIR: &str = "analysis";
pub const EXPLAINER_MEDIA_TYPE: &str = "text/html";
pub const MAX_EXPLAINER_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelError {
    #[error("required deliverable declarations must be non-empty and non-blank")]
    InvalidDeliverables,
    #[error("duplicate required deliverable id: {0}")]
    DuplicateDeliverable(String),
    #[error("participants must name two non-blank, distinct harnesses")]
    InvalidParticipants,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Working,
    Reviewing,
    Revising,
    Finalizing,
    HumanDecision,
    Done,
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Assignee {
    Worker,
    Reviewer,
    Human,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Objective {
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<ExternalRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalRef {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participants {
    pub worker: Participant,
    pub reviewer: Participant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub harness: String,
    pub claim: Option<ParticipantClaim>,
    /// Last self-reported step. Lets a peer distinguish "slow" from "dead"
    /// without inferring liveness from lease expiry alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<ParticipantProgress>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressPhase {
    Working,
    PublishingExplainer,
    ReviewingExplainer,
    ReviewingCheckpoint,
    Waiting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantProgress {
    pub phase: ProgressPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantClaim {
    pub session_id: String,
    pub epoch: u64,
    pub token_digest: String,
    /// sha256 of a nonce held only in the claimant's private credentials root,
    /// written before the claim was installed. Recovering an orphaned claim
    /// requires presenting it, so a public session id alone proves nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_started_at: Option<String>,
    pub lease_expires_at: String,
    pub lease_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverableRequirement {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointDeliverable {
    pub id: String,
    pub artifacts: Vec<ExternalRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointSubmission {
    pub kind: String,
    pub identity: String,
    #[serde(default)]
    pub deliverables: Vec<CheckpointDeliverable>,
    pub verification: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub kind: String,
    pub identity: String,
    #[serde(default)]
    pub deliverables: Vec<CheckpointDeliverable>,
    pub verification: Vec<String>,
    #[serde(default)]
    pub scope_revision: u64,
    #[serde(default)]
    pub manifest_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointBinding {
    pub checkpoint_identity: String,
    pub manifest_digest: String,
    pub scope_revision: u64,
}

impl Checkpoint {
    pub fn binding(&self) -> CheckpointBinding {
        CheckpointBinding {
            checkpoint_identity: self.identity.clone(),
            manifest_digest: self.manifest_digest.clone(),
            scope_revision: self.scope_revision,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewReceipt {
    pub verdict: String,
    pub checkpoint_identity: String,
    #[serde(default)]
    pub manifest_digest: String,
    #[serde(default)]
    pub scope_revision: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
}

impl ReviewReceipt {
    pub fn binding(&self) -> CheckpointBinding {
        CheckpointBinding {
            checkpoint_identity: self.checkpoint_identity.clone(),
            manifest_digest: self.manifest_digest.clone(),
            scope_revision: self.scope_revision,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointSupersession {
    pub reason: String,
    pub checkpoint: CheckpointBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyPublication {
    pub required: bool,
    pub desired_revision: u64,
    pub published_revision: Option<u64>,
    #[serde(default)]
    pub refs: Vec<ExternalRef>,
}

impl Default for LegacyPublication {
    fn default() -> Self {
        Self {
            required: true,
            desired_revision: 0,
            published_revision: None,
            refs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationPolicy {
    pub publisher_harness: String,
    pub channel: String,
    pub access: String,
    pub reviewer_harness: String,
}

impl PublicationPolicy {
    /// Whether this policy names a channel/access pair the kernel recognizes at
    /// all. Unknown pairs cannot be reasoned about and are rejected on read.
    pub fn is_recognized(&self) -> bool {
        !self.publisher_harness.trim().is_empty()
            && !self.reviewer_harness.trim().is_empty()
            && matches!(
                (self.channel.as_str(), self.access.as_str()),
                (EXPLAINER_CHANNEL, EXPLAINER_ACCESS)
                    | (LEGACY_EXPLAINER_CHANNEL, LEGACY_EXPLAINER_ACCESS)
            )
    }

    pub fn fixed() -> Self {
        Self {
            publisher_harness: CODEX_HARNESS.to_owned(),
            channel: EXPLAINER_CHANNEL.to_owned(),
            access: EXPLAINER_ACCESS.to_owned(),
            reviewer_harness: "Claude".to_owned(),
        }
    }

    /// New runs bind the local artifact to the semantic roles: vadi authors it
    /// and prativadi reviews it. Sites publication is a separate Codex-only
    /// responsibility and is not encoded in this local-channel policy.
    pub fn for_participants(worker_harness: &str, reviewer_harness: &str) -> Self {
        Self {
            publisher_harness: worker_harness.to_owned(),
            channel: EXPLAINER_CHANNEL.to_owned(),
            access: EXPLAINER_ACCESS.to_owned(),
            reviewer_harness: reviewer_harness.to_owned(),
        }
    }

    /// Whether the designated reviewing harness can actually read bytes the
    /// designated publisher places on this channel. A policy that fails this
    /// check can never reach an explainer review and must be rejected at start
    /// rather than after the publisher has already deployed.
    pub fn reviewer_can_read(&self) -> bool {
        match (self.channel.as_str(), self.access.as_str()) {
            // Run-directory artifacts are local files both harnesses can open.
            (EXPLAINER_CHANNEL, EXPLAINER_ACCESS) => true,
            // An owner-only Site is readable only by the publisher's own owner
            // session, so it works only when publisher and reviewer coincide.
            (LEGACY_EXPLAINER_CHANNEL, LEGACY_EXPLAINER_ACCESS) => self
                .publisher_harness
                .trim()
                .eq_ignore_ascii_case(self.reviewer_harness.trim()),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffKind {
    RunStarted,
    WorkerToReviewer,
    ReviewerToWorker,
    ProtocolUpgraded,
    ScopeAmended,
    CheckpointSuperseded,
    ApprovalWithdrawn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffObligation {
    pub handoff_revision: u64,
    pub kind: HandoffKind,
    pub scope_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<CheckpointBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationDeployment {
    pub obligation: HandoffObligation,
    pub source_digest: String,
    pub site_id: String,
    pub site_version: String,
    pub url: String,
    pub channel: String,
    pub access: String,
    pub publisher_harness: String,
}

/// Digest-bound explainer bytes staged inside the run directory. This is the
/// artifact the reviewing harness reads. When Codex participates, finalization
/// also requires a private Sites deployment of the same digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainerArtifact {
    pub obligation: HandoffObligation,
    pub source_digest: String,
    /// Run-directory-relative path, always `explainer/<source_digest>.html`.
    pub path: String,
    pub media_type: String,
    pub byte_length: u64,
    pub channel: String,
    pub access: String,
    pub publisher_harness: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationReview {
    pub obligation: HandoffObligation,
    pub source_digest: String,
    pub verdict: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
    pub reviewer_harness: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_id: Option<String>,
    pub obligation: HandoffObligation,
    /// Counts receipts written against the current obligation. Obligation-bound
    /// writes waive the run-wide revision precondition, so this is what they
    /// concurrency-check against instead: it advances only on a receipt, so
    /// unrelated claim and progress edges never invalidate a prepared write,
    /// while a stale or out-of-order receipt is rejected.
    #[serde(default)]
    pub receipt_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ExplainerArtifact>,
    pub deployment: Option<PublicationDeployment>,
    pub review: Option<PublicationReview>,
}

/// Content-addressed, run-directory-relative location for staged explainer bytes.
pub fn explainer_artifact_path(source_digest: &str) -> String {
    format!("{EXPLAINER_ARTIFACT_DIR}/{source_digest}.html")
}

/// The identity of an `analysis` checkpoint is derived from the artifacts it
/// cites, so it cannot name one thing while carrying another. Two manifests over
/// the same bytes have the same identity, and changing any cited digest changes
/// it.
pub fn analysis_checkpoint_identity(artifact_digests: &[String]) -> String {
    let mut sorted = artifact_digests.to_vec();
    sorted.sort();
    sorted.dedup();
    format!("{:x}", Sha256::digest(sorted.join("\n").as_bytes()))
}

/// Content-addressed location for a staged analysis deliverable. An `analysis`
/// checkpoint names digests, and a reviewer has to be able to materialize the
/// exact bytes behind each one.
pub fn analysis_artifact_path(digest: &str) -> String {
    format!("{ANALYSIS_ARTIFACT_DIR}/{digest}")
}

pub fn create_handoff_obligation(
    kind: HandoffKind,
    handoff_revision: u64,
    scope_revision: u64,
) -> PublicationBinding {
    PublicationBinding {
        site_id: None,
        receipt_seq: 0,
        obligation: HandoffObligation {
            handoff_revision,
            kind,
            scope_revision,
            checkpoint: None,
        },
        artifact: None,
        deployment: None,
        review: None,
    }
}

/// What a human is being asked for. There is deliberately no "approval" kind:
/// a protocol-internal problem has a deterministic recovery and must be taken
/// autonomously, because during an autonomous run there may be nobody to ask.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanDecisionKind {
    /// What the work should cover. Only a human can widen or change scope.
    #[default]
    Scope,
    /// Which of several readings of the human's request is meant.
    Intent,
    /// Permission that is the human's alone to give, such as acting outside
    /// the workspace or on their behalf somewhere the run cannot reach.
    Authority,
}

/// How a run is allowed to interact with its human. An autonomous run admits a
/// pause only as a choice between concrete scope proposals the kernel can apply
/// itself, so there is no admissible shape for "please approve".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    #[default]
    Attended,
    Autonomous,
}

/// A concrete alternative scope a human may choose; applying it is a scope
/// amendment, so a choice among proposals is a kernel-verifiable effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeProposal {
    pub objective: String,
    #[serde(default)]
    pub objective_refs: Vec<ExternalRef>,
    pub task_reference: Option<String>,
    pub scope_deliverables: Vec<DeliverableRequirement>,
}

/// Decisions written by this kernel. Version 1 is anything recorded before
/// decisions were choices, and keeps its original resolution rules.
pub const DECISION_VERSION: u32 = 2;

fn legacy_decision_version() -> u32 {
    1
}

impl HumanDecisionKind {
    /// The objective-reference kind under which a resolved decision is recorded.
    pub fn reference_kind(self) -> &'static str {
        match self {
            HumanDecisionKind::Scope => "scope",
            HumanDecisionKind::Intent => "intent",
            HumanDecisionKind::Authority => "authority",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanDecision {
    /// Defaults to `scope` so runs created before decisions were typed load.
    #[serde(default = "default_human_decision_kind")]
    pub kind: HumanDecisionKind,
    /// Which resolution rules apply. Absent on decisions written before choices
    /// were enforced, which therefore resolve under their original rules.
    #[serde(default = "legacy_decision_version")]
    pub version: u32,
    /// One concrete scope per option, when the decision is a choice of scope
    /// the kernel applies itself. Required in autonomous runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposals: Vec<ScopeProposal>,
    pub question: String,
    pub requested_by: String,
    pub evidence: Vec<String>,
    pub options: Vec<String>,
    pub contact_role: String,
    pub resume_status: Status,
    pub resume_assignee: Assignee,
    pub answer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalProvenance {
    pub outcome: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryProvenance {
    pub from_revision: u64,
    pub previous_high_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationProvenance {
    pub from_schema: String,
    pub from_revision: u64,
    pub migrated_at: String,
    pub legacy_state_digest: String,
    pub legacy_checkpoint: Option<Checkpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceIdentity {
    pub repository_id: String,
    pub origin: Option<String>,
    pub worktree: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskIdentity {
    pub reference: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunBaton {
    pub schema: String,
    pub run_id: String,
    pub objective: Objective,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskIdentity>,
    #[serde(default)]
    pub interaction: InteractionMode,
    pub participants: Participants,
    pub status: Status,
    pub assignee: Assignee,
    pub revision: u64,
    #[serde(default)]
    pub scope_revision: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope_deliverables: Vec<DeliverableRequirement>,
    /// sha256 digests of analysis bytes staged under `analysis/`, sorted and
    /// unique. An `analysis` checkpoint may only cite digests listed here, so
    /// every non-git deliverable is materializable by the reviewer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub staged_analysis: Vec<String>,
    pub checkpoint: Option<Checkpoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checkpoint_history: Vec<CheckpointBinding>,
    pub review: Option<ReviewReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_checkpoint_supersession: Option<CheckpointSupersession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<LegacyPublication>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_policy: Option<PublicationPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_binding: Option<PublicationBinding>,
    pub human_decision: Option<HumanDecision>,
    pub predecessor_run_id: Option<String>,
    pub terminal: Option<TerminalProvenance>,
    pub recovery: Option<RecoveryProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationProvenance>,
}

impl RunBaton {
    pub fn effective_publication_policy(&self) -> PublicationPolicy {
        let stored = self
            .publication_policy
            .clone()
            .unwrap_or_else(PublicationPolicy::fixed);
        if stored.channel == EXPLAINER_CHANNEL && stored.access == EXPLAINER_ACCESS {
            PublicationPolicy::for_participants(
                &self.participants.worker.harness,
                &self.participants.reviewer.harness,
            )
        } else {
            stored
        }
    }

    pub fn has_codex_participant(&self) -> bool {
        self.participants
            .worker
            .harness
            .eq_ignore_ascii_case(CODEX_HARNESS)
            || self
                .participants
                .reviewer
                .harness
                .eq_ignore_ascii_case(CODEX_HARNESS)
    }

    /// Whether prativadi approved the exact local bytes vadi staged for this
    /// obligation. All consumers use this predicate so policy interpretation
    /// cannot drift between action routing, transitions, and history checks.
    pub fn local_explainer_approved(&self, binding: &PublicationBinding) -> bool {
        let effective = self.effective_publication_policy();
        binding.artifact.as_ref().is_some_and(|artifact| {
            binding.review.as_ref().is_some_and(|review| {
                let receipts_match = |policy: &PublicationPolicy| {
                    artifact.channel == policy.channel
                        && artifact.access == policy.access
                        && artifact.publisher_harness == policy.publisher_harness
                        && review.reviewer_harness == policy.reviewer_harness
                };
                let stored_legacy_policy_matches =
                    self.publication_policy.as_ref().is_some_and(|stored| {
                        stored.channel == EXPLAINER_CHANNEL
                            && stored.access == EXPLAINER_ACCESS
                            && receipts_match(stored)
                    });
                artifact.obligation == binding.obligation
                    && review.obligation == binding.obligation
                    && review.source_digest == artifact.source_digest
                    && review.verdict == "approved"
                    && review.findings.is_empty()
                    && (receipts_match(&effective) || stored_legacy_policy_matches)
            })
        })
    }

    /// Complete finalization predicate for one handoff. Local approval always
    /// gates; a matching owner-only Site additionally gates when Codex is one
    /// of the two participants.
    pub fn publication_gate_satisfied(
        &self,
        binding: &PublicationBinding,
        expected: Option<(&HandoffKind, &CheckpointBinding)>,
    ) -> bool {
        expected.is_none_or(|(kind, checkpoint)| {
            &binding.obligation.kind == kind
                && binding.obligation.checkpoint.as_ref() == Some(checkpoint)
        }) && self.local_explainer_approved(binding)
            && (!self.has_codex_participant()
                || binding.artifact.as_ref().is_some_and(|artifact| {
                    binding.deployment.as_ref().is_some_and(|deployment| {
                        deployment.obligation == binding.obligation
                            && deployment.source_digest == artifact.source_digest
                            && binding.site_id.as_ref() == Some(&deployment.site_id)
                            && deployment.channel == SITES_CHANNEL
                            && deployment.access == SITES_ACCESS
                            && deployment
                                .publisher_harness
                                .eq_ignore_ascii_case(CODEX_HARNESS)
                    })
                }))
    }

    pub fn new(
        run_id: impl Into<String>,
        objective: impl Into<String>,
        worker_harness: impl Into<String>,
        reviewer_harness: impl Into<String>,
        scope_deliverables: Vec<DeliverableRequirement>,
    ) -> Result<Self, ModelError> {
        let (worker_harness, reviewer_harness) =
            normalize_participants(worker_harness.into(), reviewer_harness.into())?;
        let scope_deliverables = normalize_deliverables(scope_deliverables)?;
        let publication_policy =
            PublicationPolicy::for_participants(&worker_harness, &reviewer_harness);
        Ok(Self {
            schema: SCHEMA.to_owned(),
            run_id: run_id.into(),
            objective: Objective {
                summary: objective.into(),
                refs: Vec::new(),
            },
            workspace: None,
            task: None,
            interaction: InteractionMode::Attended,
            participants: Participants {
                worker: Participant {
                    harness: worker_harness,
                    claim: None,
                    progress: None,
                },
                reviewer: Participant {
                    harness: reviewer_harness,
                    claim: None,
                    progress: None,
                },
            },
            status: Status::Working,
            assignee: Assignee::Worker,
            revision: 0,
            scope_revision: 0,
            scope_deliverables,
            staged_analysis: Vec::new(),
            checkpoint: None,
            checkpoint_history: Vec::new(),
            review: None,
            pending_checkpoint_supersession: None,
            publication: None,
            publication_policy: Some(publication_policy),
            publication_binding: Some(create_handoff_obligation(HandoffKind::RunStarted, 0, 0)),
            human_decision: None,
            predecessor_run_id: None,
            terminal: None,
            recovery: None,
            migration: None,
        })
    }

    pub fn with_discovery_identity(
        mut self,
        workspace: WorkspaceIdentity,
        task: TaskIdentity,
    ) -> Self {
        self.workspace = Some(workspace);
        self.task = Some(task);
        self
    }
}

pub fn create_bound_handoff_obligation(
    kind: HandoffKind,
    handoff_revision: u64,
    scope_revision: u64,
    checkpoint: Option<CheckpointBinding>,
) -> PublicationBinding {
    let mut binding = create_handoff_obligation(kind, handoff_revision, scope_revision);
    binding.obligation.checkpoint = checkpoint;
    binding
}

pub const CHECKPOINT_KIND_GIT: &str = "git";
/// For deliverables that produce an analysis rather than a commit — a review, an
/// audit, a research finding. Immutability comes from the content digest instead
/// of from a git object.
pub const CHECKPOINT_KIND_ANALYSIS: &str = "analysis";

/// Whether a checkpoint already on disk is acceptable to read. Kernels before
/// 0.3.0 accepted any non-blank kind, so persisted checkpoints may carry one
/// this kernel does not model. Rejecting those on read would strand every run
/// created by an earlier kernel, so an unknown kind loads unchanged; only the
/// kinds this kernel issues are held to their immutability rules.
pub fn valid_stored_checkpoint_shape(
    kind: &str,
    identity: &str,
    artifacts: &[ExternalRef],
) -> bool {
    // Deliberately permissive. Kernels before 0.3.0 accepted any non-blank
    // kind and any non-blank artifact coordinates — including `git` with
    // `identity: "HEAD"` and `analysis` with a free-form identity — and those
    // runs are still schema v2. Tightening the read path would strand them
    // while the kernel still advertises v2 and API 2. Immutability is enforced
    // where it can be without stranding anyone: at submission.
    let _ = (kind, identity, artifacts);
    true
}

/// Whether `kind`/`identity`/`artifacts` form an immutable checkpoint manifest
/// for a checkpoint of this kind. Applied to new submissions only.
pub fn valid_checkpoint_shape(kind: &str, identity: &str, artifacts: &[ExternalRef]) -> bool {
    match kind {
        CHECKPOINT_KIND_GIT => {
            valid_git_object(identity)
                && artifacts.iter().all(|artifact| {
                    matches!(artifact.kind.as_str(), "commit" | "tree" | "blob")
                        && valid_git_object(&artifact.value)
                })
        }
        CHECKPOINT_KIND_ANALYSIS => {
            valid_sha256(identity)
                && artifacts.iter().all(|artifact| {
                    artifact.kind == "analysis_digest" && valid_sha256(&artifact.value)
                })
        }
        _ => false,
    }
}

/// A full-length git object name, in either SHA-1 or SHA-256 object format.
/// Abbreviations and mutable names such as branches or `HEAD` are rejected.
pub fn valid_git_object(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn default_human_decision_kind() -> HumanDecisionKind {
    HumanDecisionKind::Scope
}

pub fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub fn valid_exact_reference(value: &str) -> bool {
    !value.is_empty() && value.trim() == value
}

pub fn checkpoint_manifest_digest(checkpoint: &Checkpoint) -> String {
    #[derive(Serialize)]
    struct CanonicalManifest<'a> {
        kind: &'a str,
        identity: &'a str,
        deliverables: &'a [CheckpointDeliverable],
        verification: &'a [String],
        scope_revision: u64,
    }
    let canonical = CanonicalManifest {
        kind: &checkpoint.kind,
        identity: &checkpoint.identity,
        deliverables: &checkpoint.deliverables,
        verification: &checkpoint.verification,
        scope_revision: checkpoint.scope_revision,
    };
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical).expect("checkpoint manifest serializes"))
    )
}

pub fn normalize_participants(
    worker_harness: String,
    reviewer_harness: String,
) -> Result<(String, String), ModelError> {
    let normalize = |value: String| {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "" => None,
            "codex" => Some("Codex".to_owned()),
            "claude" => Some("Claude".to_owned()),
            _ => Some(normalized),
        }
    };
    let worker = normalize(worker_harness).ok_or(ModelError::InvalidParticipants)?;
    let reviewer = normalize(reviewer_harness).ok_or(ModelError::InvalidParticipants)?;
    if worker.eq_ignore_ascii_case(&reviewer) {
        return Err(ModelError::InvalidParticipants);
    }
    Ok((worker, reviewer))
}

pub fn normalize_deliverables(
    deliverables: Vec<DeliverableRequirement>,
) -> Result<Vec<DeliverableRequirement>, ModelError> {
    if deliverables.is_empty() {
        return Err(ModelError::InvalidDeliverables);
    }
    let mut ids = HashSet::new();
    let mut normalized = Vec::with_capacity(deliverables.len());
    for deliverable in deliverables {
        let id = deliverable.id.trim().to_owned();
        let description = deliverable.description.trim().to_owned();
        if id.is_empty() || description.is_empty() {
            return Err(ModelError::InvalidDeliverables);
        }
        if !ids.insert(id.clone()) {
            return Err(ModelError::DuplicateDeliverable(id));
        }
        normalized.push(DeliverableRequirement { id, description });
    }
    Ok(normalized)
}
