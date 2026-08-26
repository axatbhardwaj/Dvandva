use thiserror::Error;

use crate::{
    action::{Action, ReviewVerdict},
    claim::{self, ClaimError, Role},
    model::{Assignee, ReviewReceipt, RunBaton, Status},
    store::{RunChannel, StoreError},
};

#[derive(Debug, Error)]
pub enum TransitionError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Claim(#[from] ClaimError),
    #[error("action belongs to the other role")]
    WrongOwner,
    #[error("action is illegal from the current state")]
    IllegalState,
    #[error("checkpoint identity is missing or was already reviewed")]
    InvalidCheckpoint,
    #[error("checkpoint verification must contain non-blank evidence")]
    MissingVerification,
    #[error("review does not bind the current checkpoint")]
    StaleReview,
    #[error("changes requested must contain actionable findings")]
    MissingFindings,
    #[error("approval cannot contain blocking findings")]
    BlockingFindings,
    #[error("required publication is not synchronized")]
    PublicationStale,
    #[error("terminal state cannot be mutated")]
    Terminal,
}

pub fn apply(
    channel: &RunChannel,
    role: Role,
    session_id: &str,
    token: &str,
    expected_revision: u64,
    action: Action,
) -> Result<RunBaton, TransitionError> {
    let mut baton = channel.read()?;
    if baton.revision != expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: expected_revision,
            actual: baton.revision,
        }
        .into());
    }
    if matches!(baton.status, Status::Done | Status::Abandoned) {
        return Err(TransitionError::Terminal);
    }
    claim::verify(&baton, role, session_id, token)?;

    match action {
        Action::SubmitCheckpoint { checkpoint } => {
            require_owner(&baton, role, Role::Worker, Assignee::Worker)?;
            if !matches!(baton.status, Status::Working | Status::Revising) {
                return Err(TransitionError::IllegalState);
            }
            if checkpoint.identity.trim().is_empty()
                || baton
                    .checkpoint
                    .as_ref()
                    .is_some_and(|prior| prior.identity == checkpoint.identity)
            {
                return Err(TransitionError::InvalidCheckpoint);
            }
            if checkpoint.verification.is_empty()
                || checkpoint
                    .verification
                    .iter()
                    .any(|evidence| evidence.trim().is_empty())
            {
                return Err(TransitionError::MissingVerification);
            }
            baton.checkpoint = Some(checkpoint);
            baton.review = None;
            baton.status = Status::Reviewing;
            baton.assignee = Assignee::Reviewer;
        }
        Action::RecordReview {
            verdict,
            checkpoint_identity,
            findings,
        } => {
            require_owner(&baton, role, Role::Reviewer, Assignee::Reviewer)?;
            if baton.status != Status::Reviewing {
                return Err(TransitionError::IllegalState);
            }
            let checkpoint = baton
                .checkpoint
                .as_ref()
                .ok_or(TransitionError::InvalidCheckpoint)?;
            if checkpoint.identity != checkpoint_identity {
                return Err(TransitionError::StaleReview);
            }
            let verdict_name = match verdict {
                ReviewVerdict::ChangesRequested => {
                    if findings.is_empty()
                        || findings.iter().any(|finding| finding.trim().is_empty())
                    {
                        return Err(TransitionError::MissingFindings);
                    }
                    baton.status = Status::Revising;
                    baton.assignee = Assignee::Worker;
                    "changes_requested"
                }
                ReviewVerdict::Approved => {
                    if !findings.is_empty() {
                        return Err(TransitionError::BlockingFindings);
                    }
                    baton.status = Status::Finalizing;
                    baton.assignee = Assignee::Worker;
                    "approved"
                }
            };
            baton.review = Some(ReviewReceipt {
                verdict: verdict_name.to_owned(),
                checkpoint_identity,
                findings,
            });
        }
        Action::Finalize => {
            require_owner(&baton, role, Role::Worker, Assignee::Worker)?;
            if baton.status != Status::Finalizing {
                return Err(TransitionError::IllegalState);
            }
            let checkpoint = baton
                .checkpoint
                .as_ref()
                .ok_or(TransitionError::InvalidCheckpoint)?;
            let review = baton.review.as_ref().ok_or(TransitionError::StaleReview)?;
            if review.verdict != "approved" || review.checkpoint_identity != checkpoint.identity {
                return Err(TransitionError::StaleReview);
            }
            if baton.publication.required
                && baton.publication.published_revision != Some(baton.publication.desired_revision)
            {
                return Err(TransitionError::PublicationStale);
            }
            baton.status = Status::Done;
            baton.assignee = Assignee::None;
        }
    }
    baton.revision += 1;
    channel.compare_and_swap(expected_revision, &baton)?;
    Ok(baton)
}

fn require_owner(
    baton: &RunBaton,
    actual_role: Role,
    required_role: Role,
    required_assignee: Assignee,
) -> Result<(), TransitionError> {
    if actual_role != required_role || baton.assignee != required_assignee {
        return Err(TransitionError::WrongOwner);
    }
    Ok(())
}
