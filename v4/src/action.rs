use serde::Deserialize;

use crate::model::Checkpoint;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    ChangesRequested,
    Approved,
}
