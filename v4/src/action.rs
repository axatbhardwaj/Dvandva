use serde::Deserialize;

use crate::model::{CheckpointSubmission, DeliverableRequirement, ExternalRef, HandoffObligation};

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
    RequestHumanDecision(HumanDecisionRequest),
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanDecisionRequest {
    pub question: String,
    pub evidence: Vec<String>,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    ChangesRequested,
    Approved,
}
