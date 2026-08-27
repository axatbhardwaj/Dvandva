use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use thiserror::Error;

pub const LEGACY_SCHEMA: &str = "dvandva.run.v1";
pub const SCHEMA: &str = "dvandva.run.v2";
pub const ROLE_API: u32 = 2;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelError {
    #[error("required deliverable declarations must be non-empty and non-blank")]
    InvalidDeliverables,
    #[error("duplicate required deliverable id: {0}")]
    DuplicateDeliverable(String),
    #[error("participants must contain exactly one Codex and one Claude harness")]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantClaim {
    pub session_id: String,
    pub epoch: u64,
    pub token_digest: String,
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
pub struct Publication {
    pub required: bool,
    pub desired_revision: u64,
    pub published_revision: Option<u64>,
    #[serde(default)]
    pub refs: Vec<ExternalRef>,
}

impl Default for Publication {
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
    pub fn fixed() -> Self {
        Self {
            publisher_harness: "Codex".to_owned(),
            channel: "codex_sites".to_owned(),
            access: "owner_only".to_owned(),
            reviewer_harness: "Claude".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffKind {
    RunStarted,
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
    pub source_digest: String,
    pub site_id: String,
    pub site_version: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationReview {
    pub verdict: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationBinding {
    pub obligation: HandoffObligation,
    pub deployment: Option<PublicationDeployment>,
    pub review: Option<PublicationReview>,
}

pub fn create_handoff_obligation(
    kind: HandoffKind,
    handoff_revision: u64,
    scope_revision: u64,
) -> PublicationBinding {
    PublicationBinding {
        obligation: HandoffObligation {
            handoff_revision,
            kind,
            scope_revision,
            checkpoint: None,
        },
        deployment: None,
        review: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanDecision {
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
    pub participants: Participants,
    pub status: Status,
    pub assignee: Assignee,
    pub revision: u64,
    #[serde(default)]
    pub scope_revision: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope_deliverables: Vec<DeliverableRequirement>,
    pub checkpoint: Option<Checkpoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checkpoint_history: Vec<CheckpointBinding>,
    pub review: Option<ReviewReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_checkpoint_supersession: Option<CheckpointSupersession>,
    #[serde(default)]
    pub publication: Publication,
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
        Ok(Self {
            schema: SCHEMA.to_owned(),
            run_id: run_id.into(),
            objective: Objective {
                summary: objective.into(),
                refs: Vec::new(),
            },
            workspace: None,
            task: None,
            participants: Participants {
                worker: Participant {
                    harness: worker_harness,
                    claim: None,
                },
                reviewer: Participant {
                    harness: reviewer_harness,
                    claim: None,
                },
            },
            status: Status::Working,
            assignee: Assignee::Worker,
            revision: 0,
            scope_revision: 0,
            scope_deliverables,
            checkpoint: None,
            checkpoint_history: Vec::new(),
            review: None,
            pending_checkpoint_supersession: None,
            publication: Publication {
                required: true,
                desired_revision: 0,
                published_revision: None,
                refs: Vec::new(),
            },
            publication_policy: Some(PublicationPolicy::fixed()),
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
    let normalize = |value: String| match value.trim().to_ascii_lowercase().as_str() {
        "codex" => Some("Codex".to_owned()),
        "claude" => Some("Claude".to_owned()),
        _ => None,
    };
    let worker = normalize(worker_harness).ok_or(ModelError::InvalidParticipants)?;
    let reviewer = normalize(reviewer_harness).ok_or(ModelError::InvalidParticipants)?;
    if worker == reviewer {
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
