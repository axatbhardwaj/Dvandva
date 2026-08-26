use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "dvandva.run.v1";

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
pub struct Checkpoint {
    pub kind: String,
    pub identity: String,
    pub verification: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewReceipt {
    pub verdict: String,
    pub checkpoint_identity: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Publication {
    pub required: bool,
    pub desired_revision: u64,
    pub published_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<ExternalRef>,
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
pub struct RunBaton {
    pub schema: String,
    pub run_id: String,
    pub objective: Objective,
    pub participants: Participants,
    pub status: Status,
    pub assignee: Assignee,
    pub revision: u64,
    pub checkpoint: Option<Checkpoint>,
    pub review: Option<ReviewReceipt>,
    pub publication: Publication,
    pub human_decision: Option<HumanDecision>,
    pub predecessor_run_id: Option<String>,
    pub terminal: Option<TerminalProvenance>,
    pub recovery: Option<RecoveryProvenance>,
}

impl RunBaton {
    pub fn new(
        run_id: impl Into<String>,
        objective: impl Into<String>,
        worker_harness: impl Into<String>,
        reviewer_harness: impl Into<String>,
    ) -> Self {
        Self {
            schema: SCHEMA.to_owned(),
            run_id: run_id.into(),
            objective: Objective {
                summary: objective.into(),
                refs: Vec::new(),
            },
            participants: Participants {
                worker: Participant {
                    harness: worker_harness.into(),
                    claim: None,
                },
                reviewer: Participant {
                    harness: reviewer_harness.into(),
                    claim: None,
                },
            },
            status: Status::Working,
            assignee: Assignee::Worker,
            revision: 0,
            checkpoint: None,
            review: None,
            publication: Publication {
                required: false,
                desired_revision: 0,
                published_revision: None,
                refs: Vec::new(),
            },
            human_decision: None,
            predecessor_run_id: None,
            terminal: None,
            recovery: None,
        }
    }
}
