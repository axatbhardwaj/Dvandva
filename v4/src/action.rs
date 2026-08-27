use serde::Deserialize;

use crate::{
    claim::Role,
    model::{
        Assignee, CheckpointSubmission, DeliverableRequirement, ExternalRef, HandoffObligation,
        Status,
    },
};

#[derive(Debug, Deserialize)]
pub struct ScopeAmendment {
    pub objective: String,
    #[serde(default)]
    pub objective_refs: Vec<ExternalRef>,
    pub task_reference: Option<String>,
    pub scope_deliverables: Vec<DeliverableRequirement>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    SubmitCheckpoint {
        checkpoint: CheckpointSubmission,
    },
    RecordReview {
        verdict: ReviewVerdict,
        checkpoint_identity: String,
        manifest_digest: String,
        scope_revision: u64,
        #[serde(default)]
        findings: Vec<String>,
    },
    Finalize,
    RequestHumanDecision {
        question: String,
        evidence: Vec<String>,
        options: Vec<String>,
        contact_role: Role,
        resume_status: Status,
        resume_assignee: Assignee,
    },
    ResumeHumanDecision {
        answer: String,
        #[serde(default)]
        scope_amendment: Option<ScopeAmendment>,
    },
    RequestCheckpointSupersession {
        reason: String,
    },
    AcceptCheckpointSupersession,
    WithdrawApproval {
        reason: String,
    },
    RecordPublication {
        required: bool,
        desired_revision: u64,
        published_revision: Option<u64>,
        #[serde(default)]
        refs: Vec<ExternalRef>,
    },
    RecordExplainerPublication {
        obligation: HandoffObligation,
        source_digest: String,
        site_id: String,
        site_version: String,
        url: String,
        channel: String,
        access: String,
    },
    RecordExplainerReview {
        obligation: HandoffObligation,
        source_digest: String,
        site_id: String,
        site_version: String,
        url: String,
        verdict: ReviewVerdict,
        #[serde(default)]
        findings: Vec<String>,
    },
    Abandon {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    ChangesRequested,
    Approved,
}
