use serde::Deserialize;

use std::path::PathBuf;

use crate::model::{CheckpointSubmission, ExternalRef, HandoffObligation, ProgressPhase};

/// A human-approved replacement scope. The same shape a decision's proposals
/// use, so choosing a proposal and amending scope are one operation.
pub type ScopeAmendment = crate::model::ScopeProposal;

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
    /// Vadi stages the explainer's bytes into the run directory. The kernel
    /// hashes the file, copies it to a content-addressed location both roles can
    /// read, and binds the digest to the current obligation.
    StageExplainer {
        obligation: HandoffObligation,
        /// The `publication_binding.receipt_seq` this write was prepared
        /// against. Omitted means "whatever is current", which is only safe for
        /// a first write.
        #[serde(default)]
        after_seq: Option<u64>,
        source_path: PathBuf,
    },
    /// The Codex participant records the private status Site that renders the
    /// locally approved bytes. The digest must match vadi's artifact and
    /// prativadi's review.
    RecordExplainerPublication {
        obligation: HandoffObligation,
        /// The `publication_binding.receipt_seq` this write was prepared
        /// against. Omitted means "whatever is current", which is only safe for
        /// a first write.
        #[serde(default)]
        after_seq: Option<u64>,
        source_digest: String,
        site_id: String,
        site_version: String,
        url: String,
        channel: String,
        access: String,
    },
    RecordExplainerReview {
        obligation: HandoffObligation,
        /// The `publication_binding.receipt_seq` this write was prepared
        /// against. Omitted means "whatever is current", which is only safe for
        /// a first write.
        #[serde(default)]
        after_seq: Option<u64>,
        source_digest: String,
        verdict: ReviewVerdict,
        #[serde(default)]
        findings: Vec<String>,
    },
    /// Stage the bytes behind an `analysis` checkpoint artifact so the reviewer
    /// can materialize exactly what the manifest cites.
    StageAnalysis {
        source_path: PathBuf,
    },
    /// Publish a liveness/progress signal and renew this role's own lease.
    ReportProgress {
        phase: ProgressPhase,
        #[serde(default)]
        detail: Option<String>,
    },
    Abandon {
        reason: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanDecisionRequest {
    /// Declaring what is being asked for is what keeps a pause from becoming an
    /// open-ended approval wait: there is no kind for "please confirm". It
    /// defaults to `scope` so payloads written against the released API 2
    /// surface, which had no such field, still apply.
    #[serde(default)]
    pub kind: crate::model::HumanDecisionKind,
    pub question: String,
    pub evidence: Vec<String>,
    pub options: Vec<String>,
    /// One concrete scope per option. Makes a scope decision a choice the kernel
    /// can apply; required in autonomous runs.
    #[serde(default)]
    pub proposals: Vec<ScopeAmendment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    ChangesRequested,
    Approved,
}
