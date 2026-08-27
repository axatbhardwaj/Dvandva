use serde::Deserialize;

use crate::{
    claim::Role,
    model::{Assignee, Checkpoint, ExternalRef, Status},
};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    SubmitCheckpoint {
        checkpoint: Checkpoint,
    },
    RecordReview {
        verdict: ReviewVerdict,
        checkpoint_identity: String,
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
    },
    RecordPublication {
        required: bool,
        desired_revision: u64,
        published_revision: Option<u64>,
        #[serde(default)]
        refs: Vec<ExternalRef>,
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
