use std::collections::HashSet;

use thiserror::Error;

use crate::{
    action::{Action, ReviewVerdict, ScopeAmendment},
    claim::{self, ClaimError, Role},
    model::{
        checkpoint_manifest_digest, create_bound_handoff_obligation, normalize_deliverables,
        Assignee, Checkpoint, CheckpointBinding, CheckpointSubmission, CheckpointSupersession,
        HandoffKind, HumanDecision, PublicationDeployment, PublicationReview, ReviewReceipt,
        RunBaton, Status, TerminalProvenance,
    },
    store::{require_current_schema, RunChannel, StoreError},
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
    #[error("checkpoint supersession must be resolved before semantic approval")]
    SupersessionPending,
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
    #[error("legacy numeric publication is unsupported for v2 runs")]
    LegacyPublicationUnsupported,
    #[error("only the Codex-harness participant may publish the explainer")]
    WrongPublisherHarness,
    #[error("only the Claude-harness participant may review the explainer")]
    WrongReviewerHarness,
    #[error("explainer publication metadata is invalid")]
    InvalidExplainerPublication,
    #[error("explainer receipt does not bind the current obligation and deployment")]
    StalePublicationBinding,
    #[error("explainer deployments must preserve the run Site ID")]
    SiteIdMismatch,
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
    require_current_schema(&baton)?;
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
            require_publication_gate(&baton)?;
            let checkpoint = normalize_checkpoint(checkpoint, &baton)?;
            if channel.checkpoint_identity_seen(&checkpoint.identity, expected_revision)?
                || baton
                    .checkpoint_history
                    .iter()
                    .any(|prior| prior.checkpoint_identity == checkpoint.identity)
                || baton
                    .checkpoint
                    .as_ref()
                    .is_some_and(|prior| prior.identity == checkpoint.identity)
            {
                return Err(TransitionError::InvalidCheckpoint);
            }
            let checkpoint_binding = checkpoint.binding();
            baton.checkpoint_history.push(checkpoint_binding.clone());
            baton.checkpoint = Some(checkpoint);
            baton.review = None;
            baton.pending_checkpoint_supersession = None;
            baton.status = Status::Reviewing;
            baton.assignee = Assignee::Reviewer;
            replace_handoff_obligation(
                &mut baton,
                HandoffKind::WorkerToReviewer,
                Some(checkpoint_binding),
            );
        }
        Action::RecordReview {
            verdict,
            checkpoint_identity,
            manifest_digest,
            scope_revision,
            findings,
        } => {
            require_owner(&baton, role, Role::Reviewer, Assignee::Reviewer)?;
            if baton.status != Status::Reviewing {
                return Err(TransitionError::IllegalState);
            }
            require_publication_gate(&baton)?;
            let checkpoint = baton
                .checkpoint
                .as_ref()
                .ok_or(TransitionError::InvalidCheckpoint)?;
            let submitted_binding = CheckpointBinding {
                checkpoint_identity: checkpoint_identity.trim().to_owned(),
                manifest_digest: manifest_digest.trim().to_owned(),
                scope_revision,
            };
            if checkpoint.binding() != submitted_binding {
                return Err(TransitionError::StaleReview);
            }
            let findings = findings
                .into_iter()
                .map(|finding| finding.trim().to_owned())
                .collect::<Vec<_>>();
            let verdict_name = match verdict {
                ReviewVerdict::ChangesRequested => {
                    if findings.is_empty()
                        || findings.iter().any(|finding| finding.trim().is_empty())
                    {
                        return Err(TransitionError::MissingFindings);
                    }
                    baton.status = Status::Revising;
                    baton.assignee = Assignee::Worker;
                    baton.pending_checkpoint_supersession = None;
                    "changes_requested"
                }
                ReviewVerdict::Approved => {
                    if baton.pending_checkpoint_supersession.is_some() {
                        return Err(TransitionError::SupersessionPending);
                    }
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
                checkpoint_identity: submitted_binding.checkpoint_identity.clone(),
                manifest_digest: submitted_binding.manifest_digest.clone(),
                scope_revision: submitted_binding.scope_revision,
                findings,
            });
            replace_handoff_obligation(
                &mut baton,
                HandoffKind::ReviewerToWorker,
                Some(submitted_binding),
            );
        }
        Action::Finalize => {
            require_owner(&baton, role, Role::Worker, Assignee::Worker)?;
            if baton.status != Status::Finalizing {
                return Err(TransitionError::IllegalState);
            }
            if baton.pending_checkpoint_supersession.is_some() {
                return Err(TransitionError::SupersessionPending);
            }
            let checkpoint = baton
                .checkpoint
                .as_ref()
                .ok_or(TransitionError::InvalidCheckpoint)?;
            let review = baton.review.as_ref().ok_or(TransitionError::StaleReview)?;
            if review.verdict != "approved" || review.binding() != checkpoint.binding() {
                return Err(TransitionError::StaleReview);
            }
            require_publication_gate(&baton)?;
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
        Action::ResumeHumanDecision {
            answer,
            scope_amendment,
        } => {
            if baton.status != Status::HumanDecision || answer.trim().is_empty() {
                return Err(TransitionError::InvalidHumanDecision);
            }
            let (resume_status, resume_assignee) = {
                let decision = baton
                    .human_decision
                    .as_mut()
                    .ok_or(TransitionError::InvalidHumanDecision)?;
                if decision.contact_role != role_name(role) {
                    return Err(TransitionError::WrongContact);
                }
                decision.answer = Some(answer.trim().to_owned());
                (
                    decision.resume_status.clone(),
                    decision.resume_assignee.clone(),
                )
            };
            if let Some(amendment) = scope_amendment {
                apply_scope_amendment(&mut baton, amendment)?;
            } else {
                baton.status = resume_status;
                baton.assignee = resume_assignee;
            }
        }
        Action::RequestCheckpointSupersession { reason } => {
            if role != Role::Worker {
                return Err(TransitionError::WrongOwner);
            }
            if baton.status != Status::Reviewing || baton.assignee != Assignee::Reviewer {
                return Err(TransitionError::IllegalState);
            }
            let reason = reason.trim().to_owned();
            if reason.is_empty() {
                return Err(TransitionError::MissingReason);
            }
            if baton.pending_checkpoint_supersession.is_some() {
                return Err(TransitionError::IllegalState);
            }
            let checkpoint = baton
                .checkpoint
                .as_ref()
                .ok_or(TransitionError::InvalidCheckpoint)?
                .binding();
            baton.pending_checkpoint_supersession =
                Some(CheckpointSupersession { reason, checkpoint });
        }
        Action::AcceptCheckpointSupersession => {
            require_owner(&baton, role, Role::Reviewer, Assignee::Reviewer)?;
            if baton.status != Status::Reviewing {
                return Err(TransitionError::IllegalState);
            }
            let pending = baton
                .pending_checkpoint_supersession
                .as_ref()
                .ok_or(TransitionError::IllegalState)?;
            let checkpoint = baton
                .checkpoint
                .as_ref()
                .ok_or(TransitionError::InvalidCheckpoint)?
                .binding();
            if pending.checkpoint != checkpoint {
                return Err(TransitionError::InvalidCheckpoint);
            }
            baton.checkpoint = None;
            baton.review = None;
            baton.pending_checkpoint_supersession = None;
            baton.status = Status::Revising;
            baton.assignee = Assignee::Worker;
            replace_handoff_obligation(
                &mut baton,
                HandoffKind::CheckpointSuperseded,
                Some(checkpoint),
            );
        }
        Action::WithdrawApproval { reason } => {
            require_owner(&baton, role, Role::Worker, Assignee::Worker)?;
            if baton.status != Status::Finalizing {
                return Err(TransitionError::IllegalState);
            }
            if reason.trim().is_empty() {
                return Err(TransitionError::MissingReason);
            }
            let checkpoint = baton
                .checkpoint
                .as_ref()
                .ok_or(TransitionError::InvalidCheckpoint)?
                .binding();
            let review = baton.review.as_ref().ok_or(TransitionError::StaleReview)?;
            if review.verdict != "approved" || review.binding() != checkpoint {
                return Err(TransitionError::StaleReview);
            }
            baton.checkpoint = None;
            baton.review = None;
            baton.pending_checkpoint_supersession = None;
            baton.status = Status::Revising;
            baton.assignee = Assignee::Worker;
            replace_handoff_obligation(
                &mut baton,
                HandoffKind::ApprovalWithdrawn,
                Some(checkpoint),
            );
        }
        Action::RecordPublication {
            required: _,
            desired_revision: _,
            published_revision: _,
            refs: _,
        } => {
            return Err(TransitionError::LegacyPublicationUnsupported);
        }
        Action::RecordExplainerPublication {
            obligation,
            source_digest,
            site_id,
            site_version,
            url,
            channel,
            access,
        } => {
            if caller_harness(&baton, role) != "Codex" {
                return Err(TransitionError::WrongPublisherHarness);
            }
            let binding = baton
                .publication_binding
                .as_mut()
                .ok_or(TransitionError::PublicationStale)?;
            if binding.obligation != obligation {
                return Err(TransitionError::StalePublicationBinding);
            }
            if !valid_sha256(&source_digest)
                || !valid_exact_reference(&site_id)
                || !valid_exact_reference(&site_version)
                || !valid_exact_reference(&url)
                || channel != "codex_sites"
                || access != "owner_only"
            {
                return Err(TransitionError::InvalidExplainerPublication);
            }
            if binding
                .site_id
                .as_ref()
                .is_some_and(|stable| stable != &site_id)
            {
                return Err(TransitionError::SiteIdMismatch);
            }
            binding.site_id = Some(site_id.clone());
            binding.deployment = Some(PublicationDeployment {
                obligation,
                source_digest,
                site_id,
                site_version,
                url,
                channel,
                access,
                publisher_harness: "Codex".to_owned(),
            });
            binding.review = None;
        }
        Action::RecordExplainerReview {
            obligation,
            source_digest,
            site_id,
            site_version,
            url,
            verdict,
            findings,
        } => {
            if caller_harness(&baton, role) != "Claude" {
                return Err(TransitionError::WrongReviewerHarness);
            }
            let binding = baton
                .publication_binding
                .as_mut()
                .ok_or(TransitionError::PublicationStale)?;
            let deployment = binding
                .deployment
                .as_ref()
                .ok_or(TransitionError::PublicationStale)?;
            if binding.obligation != obligation
                || deployment.obligation != obligation
                || deployment.source_digest != source_digest
                || deployment.site_id != site_id
                || deployment.site_version != site_version
                || deployment.url != url
            {
                return Err(TransitionError::StalePublicationBinding);
            }
            let findings = findings
                .into_iter()
                .map(|finding| finding.trim().to_owned())
                .collect::<Vec<_>>();
            let verdict = match verdict {
                ReviewVerdict::ChangesRequested => {
                    if findings.is_empty() || findings.iter().any(String::is_empty) {
                        return Err(TransitionError::MissingFindings);
                    }
                    "changes_requested"
                }
                ReviewVerdict::Approved => {
                    if !findings.is_empty() {
                        return Err(TransitionError::BlockingFindings);
                    }
                    "approved"
                }
            };
            binding.review = Some(PublicationReview {
                obligation,
                source_digest,
                site_id,
                site_version,
                url,
                verdict: verdict.to_owned(),
                findings,
                reviewer_harness: "Claude".to_owned(),
            });
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

fn normalize_checkpoint(
    submission: CheckpointSubmission,
    baton: &RunBaton,
) -> Result<Checkpoint, TransitionError> {
    let kind = submission.kind.trim().to_owned();
    let identity = submission.identity.trim().to_owned();
    if kind.is_empty() || identity.is_empty() || submission.deliverables.is_empty() {
        return Err(TransitionError::InvalidCheckpoint);
    }
    let verification = submission
        .verification
        .into_iter()
        .map(|evidence| evidence.trim().to_owned())
        .collect::<Vec<_>>();
    if verification.is_empty() || verification.iter().any(String::is_empty) {
        return Err(TransitionError::MissingVerification);
    }
    let mut ids = HashSet::new();
    let mut deliverables = Vec::with_capacity(submission.deliverables.len());
    for mut deliverable in submission.deliverables {
        deliverable.id = deliverable.id.trim().to_owned();
        if deliverable.id.is_empty()
            || deliverable.artifacts.is_empty()
            || !ids.insert(deliverable.id.clone())
        {
            return Err(TransitionError::InvalidCheckpoint);
        }
        for artifact in &mut deliverable.artifacts {
            artifact.kind = artifact.kind.trim().to_owned();
            artifact.value = artifact.value.trim().to_owned();
            if artifact.kind.is_empty() || artifact.value.is_empty() {
                return Err(TransitionError::InvalidCheckpoint);
            }
        }
        deliverable
            .artifacts
            .sort_by(|left, right| (&left.kind, &left.value).cmp(&(&right.kind, &right.value)));
        deliverables.push(deliverable);
    }
    let required = baton
        .scope_deliverables
        .iter()
        .map(|deliverable| deliverable.id.as_str())
        .collect::<HashSet<_>>();
    let submitted = ids.iter().map(String::as_str).collect::<HashSet<_>>();
    if submitted != required {
        return Err(TransitionError::InvalidCheckpoint);
    }
    deliverables.sort_by(|left, right| left.id.cmp(&right.id));
    let mut checkpoint = Checkpoint {
        kind,
        identity,
        deliverables,
        verification,
        scope_revision: baton.scope_revision,
        manifest_digest: String::new(),
    };
    checkpoint.manifest_digest = checkpoint_manifest_digest(&checkpoint);
    Ok(checkpoint)
}

fn apply_scope_amendment(
    baton: &mut RunBaton,
    amendment: ScopeAmendment,
) -> Result<(), TransitionError> {
    let objective = amendment.objective.trim().to_owned();
    if objective.is_empty() {
        return Err(TransitionError::InvalidHumanDecision);
    }
    let mut refs = amendment.objective_refs;
    for reference in &mut refs {
        reference.kind = reference.kind.trim().to_owned();
        reference.value = reference.value.trim().to_owned();
        if reference.kind.is_empty() || reference.value.is_empty() {
            return Err(TransitionError::InvalidHumanDecision);
        }
    }
    let task_reference = amendment
        .task_reference
        .map(|reference| reference.trim().to_owned());
    if task_reference.as_ref().is_some_and(String::is_empty) {
        return Err(TransitionError::InvalidHumanDecision);
    }
    let scope_deliverables = normalize_deliverables(amendment.scope_deliverables)
        .map_err(|_| TransitionError::InvalidHumanDecision)?;
    baton.objective.summary = objective.clone();
    baton.objective.refs = refs;
    if let Some(task) = &mut baton.task {
        task.reference = task_reference;
        task.summary = objective;
    }
    baton.scope_revision = baton
        .scope_revision
        .checked_add(1)
        .ok_or(TransitionError::InvalidHumanDecision)?;
    baton.scope_deliverables = scope_deliverables;
    baton.checkpoint = None;
    baton.review = None;
    baton.pending_checkpoint_supersession = None;
    baton.status = Status::Revising;
    baton.assignee = Assignee::Worker;
    replace_handoff_obligation(baton, HandoffKind::ScopeAmended, None);
    Ok(())
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

fn replace_handoff_obligation(
    baton: &mut RunBaton,
    kind: HandoffKind,
    checkpoint: Option<CheckpointBinding>,
) {
    let site_id = baton
        .publication_binding
        .as_ref()
        .and_then(|binding| binding.site_id.clone());
    let mut binding =
        create_bound_handoff_obligation(kind, baton.revision + 1, baton.scope_revision, checkpoint);
    binding.site_id = site_id;
    baton.publication_binding = Some(binding);
}

fn require_publication_gate(baton: &RunBaton) -> Result<(), TransitionError> {
    let binding = baton
        .publication_binding
        .as_ref()
        .ok_or(TransitionError::PublicationStale)?;
    let deployment = binding
        .deployment
        .as_ref()
        .ok_or(TransitionError::PublicationStale)?;
    let review = binding
        .review
        .as_ref()
        .ok_or(TransitionError::PublicationStale)?;
    if binding.site_id.as_ref() != Some(&deployment.site_id)
        || deployment.obligation != binding.obligation
        || review.obligation != binding.obligation
        || review.source_digest != deployment.source_digest
        || review.site_id != deployment.site_id
        || review.site_version != deployment.site_version
        || review.url != deployment.url
        || review.verdict != "approved"
        || !review.findings.is_empty()
        || deployment.channel != "codex_sites"
        || deployment.access != "owner_only"
        || deployment.publisher_harness != "Codex"
        || review.reviewer_harness != "Claude"
    {
        return Err(TransitionError::PublicationStale);
    }
    Ok(())
}

fn caller_harness(baton: &RunBaton, role: Role) -> &str {
    match role {
        Role::Worker => &baton.participants.worker.harness,
        Role::Reviewer => &baton.participants.reviewer.harness,
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_exact_reference(value: &str) -> bool {
    !value.is_empty() && value.trim() == value
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
