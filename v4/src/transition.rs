use thiserror::Error;

use crate::{
    action::{Action, ReviewVerdict},
    claim::{self, ClaimError, Role},
    model::{Assignee, HumanDecision, ReviewReceipt, RunBaton, Status, TerminalProvenance},
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
    #[error("human decision metadata is incomplete")]
    InvalidHumanDecision,
    #[error("only the designated contact may resume the human decision")]
    WrongContact,
    #[error("publication revisions cannot regress or exceed the desired revision")]
    PublicationRegression,
    #[error("abandonment requires a non-blank reason")]
    MissingReason,
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
            baton.terminal = Some(TerminalProvenance {
                outcome: "done".to_owned(),
                reason: None,
            });
        }
        Action::RequestHumanDecision {
            question,
            evidence,
            options,
            contact_role,
            resume_status,
            resume_assignee,
        } => {
            if baton
                .human_decision
                .as_ref()
                .is_some_and(|decision| decision.answer.is_none())
                || question.trim().is_empty()
                || evidence.is_empty()
                || evidence.iter().any(|item| item.trim().is_empty())
                || options.len() < 2
                || options.iter().any(|option| option.trim().is_empty())
                || matches!(
                    resume_status,
                    Status::HumanDecision | Status::Done | Status::Abandoned
                )
                || !resume_target_matches(&resume_status, &resume_assignee)
            {
                return Err(TransitionError::InvalidHumanDecision);
            }
            baton.human_decision = Some(HumanDecision {
                question,
                requested_by: role_name(role).to_owned(),
                evidence,
                options,
                contact_role: role_name(contact_role).to_owned(),
                resume_status,
                resume_assignee,
                answer: None,
            });
            baton.status = Status::HumanDecision;
            baton.assignee = Assignee::Human;
        }
        Action::ResumeHumanDecision { answer } => {
            if baton.status != Status::HumanDecision || answer.trim().is_empty() {
                return Err(TransitionError::InvalidHumanDecision);
            }
            let decision = baton
                .human_decision
                .as_mut()
                .ok_or(TransitionError::InvalidHumanDecision)?;
            if decision.contact_role != role_name(role) {
                return Err(TransitionError::WrongContact);
            }
            decision.answer = Some(answer);
            baton.status = decision.resume_status.clone();
            baton.assignee = decision.resume_assignee.clone();
        }
        Action::RecordPublication {
            required,
            desired_revision,
            published_revision,
            refs,
        } => {
            if role != Role::Worker {
                return Err(TransitionError::WrongOwner);
            }
            if desired_revision < baton.publication.desired_revision
                || (baton.publication.required && !required)
                || (baton.publication.published_revision.is_some() && published_revision.is_none())
                || published_revision.is_some_and(|published| {
                    published < baton.publication.published_revision.unwrap_or(0)
                        || published > desired_revision
                })
            {
                return Err(TransitionError::PublicationRegression);
            }
            baton.publication.required = required;
            baton.publication.desired_revision = desired_revision;
            baton.publication.published_revision = published_revision;
            baton.publication.refs = refs;
        }
        Action::Abandon { reason } => {
            if reason.trim().is_empty() {
                return Err(TransitionError::MissingReason);
            }
            baton.status = Status::Abandoned;
            baton.assignee = Assignee::None;
            baton.terminal = Some(TerminalProvenance {
                outcome: "abandoned".to_owned(),
                reason: Some(reason),
            });
        }
    }
    baton.revision += 1;
    channel.compare_and_swap(expected_revision, &baton)?;
    Ok(baton)
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Worker => "worker",
        Role::Reviewer => "reviewer",
    }
}

fn resume_target_matches(status: &Status, assignee: &Assignee) -> bool {
    matches!(
        (status, assignee),
        (
            Status::Working | Status::Revising | Status::Finalizing,
            Assignee::Worker
        ) | (Status::Reviewing, Assignee::Reviewer)
    )
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
