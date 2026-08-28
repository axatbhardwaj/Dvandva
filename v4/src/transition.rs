use std::collections::HashSet;

use sha2::Digest;

use thiserror::Error;

use crate::{
    action::{Action, ReviewVerdict, ScopeAmendment},
    claim::{self, ClaimError, Role},
    model::{
        checkpoint_manifest_digest, create_bound_handoff_obligation, explainer_artifact_path,
        normalize_deliverables, valid_exact_reference, valid_sha256, Assignee, Checkpoint,
        CheckpointBinding, CheckpointSubmission, CheckpointSupersession, ExplainerArtifact,
        HandoffKind, HumanDecision, ParticipantProgress, ProgressPhase, PublicationDeployment,
        PublicationPolicy, PublicationReview, ReviewReceipt, RunBaton, Status, TaskIdentity,
        TerminalProvenance, EXPLAINER_ACCESS, EXPLAINER_ARTIFACT_DIR, EXPLAINER_CHANNEL,
        EXPLAINER_MEDIA_TYPE, MAX_EXPLAINER_BYTES,
    },
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
    #[error("explainer bytes are missing, unreadable, empty, or above the size limit")]
    InvalidExplainerSource,
    #[error("no explainer bytes are staged for the current obligation")]
    ExplainerNotStaged,
    #[error("progress detail must be non-blank when present")]
    InvalidProgress,
}

/// Actions that carry their own idempotency token, or that only report
/// liveness, are correct against whatever head they land on. Binding them to a
/// caller-supplied revision made every unrelated peer heartbeat invalidate a
/// prepared write and forced an optimistic retry loop on the caller.
fn is_obligation_bound(action: &Action) -> bool {
    matches!(
        action,
        Action::StageExplainer { .. }
            | Action::RecordExplainerPublication { .. }
            | Action::RecordExplainerReview { .. }
            | Action::ReportProgress { .. }
    )
}

pub fn apply(
    channel: &RunChannel,
    role: Role,
    session_id: &str,
    token: &str,
    expected_revision: u64,
    action: Action,
) -> Result<RunBaton, TransitionError> {
    let mutate = |baton: &mut RunBaton, now, action| -> Result<RunBaton, TransitionError> {
        if matches!(baton.status, Status::Done | Status::Abandoned) {
            return Err(TransitionError::Terminal);
        }
        claim::verify_at(baton, role, session_id, token, now)?;
        apply_locked(channel, baton, role, expected_revision, action)?;
        baton.revision += 1;
        Ok(baton.clone())
    };
    if is_obligation_bound(&action) {
        channel.mutate_locked_untracked(|baton, now| mutate(baton, now, action))
    } else {
        channel.mutate_locked(expected_revision, |baton, now| mutate(baton, now, action))
    }
}

fn apply_locked(
    channel: &RunChannel,
    baton: &mut RunBaton,
    role: Role,
    expected_revision: u64,
    action: Action,
) -> Result<(), TransitionError> {
    match action {
        Action::SubmitCheckpoint { checkpoint } => {
            require_owner(baton, role, Role::Worker, Assignee::Worker)?;
            if !matches!(baton.status, Status::Working | Status::Revising) {
                return Err(TransitionError::IllegalState);
            }
            let checkpoint = normalize_checkpoint(checkpoint, baton)?;
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
                baton,
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
            require_owner(baton, role, Role::Reviewer, Assignee::Reviewer)?;
            if baton.status != Status::Reviewing {
                return Err(TransitionError::IllegalState);
            }
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
                baton,
                HandoffKind::ReviewerToWorker,
                Some(submitted_binding),
            );
        }
        Action::Finalize => {
            require_owner(baton, role, Role::Worker, Assignee::Worker)?;
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
            let checkpoint_binding = checkpoint.binding();
            require_publication_gate(
                baton,
                Some((&HandoffKind::ReviewerToWorker, &checkpoint_binding)),
            )?;
            baton.status = Status::Done;
            baton.assignee = Assignee::None;
            baton.terminal = Some(TerminalProvenance {
                outcome: "done".to_owned(),
                reason: None,
            });
        }
        Action::RequestHumanDecision(request) => {
            let crate::action::HumanDecisionRequest {
                question,
                evidence,
                options,
            } = request;
            if baton
                .human_decision
                .as_ref()
                .is_some_and(|decision| decision.answer.is_none())
                || question.trim().is_empty()
                || evidence.is_empty()
                || evidence.iter().any(|item| item.trim().is_empty())
                || options.len() < 2
                || options.iter().any(|option| option.trim().is_empty())
            {
                return Err(TransitionError::InvalidHumanDecision);
            }
            let resume_status = baton.status.clone();
            let resume_assignee = baton.assignee.clone();
            baton.human_decision = Some(HumanDecision {
                question,
                requested_by: role_name(role).to_owned(),
                evidence,
                options,
                contact_role: role_name(role).to_owned(),
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
                apply_scope_amendment(baton, amendment)?;
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
            require_owner(baton, role, Role::Reviewer, Assignee::Reviewer)?;
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
            replace_handoff_obligation(baton, HandoffKind::CheckpointSuperseded, Some(checkpoint));
        }
        Action::WithdrawApproval { reason } => {
            require_owner(baton, role, Role::Worker, Assignee::Worker)?;
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
            replace_handoff_obligation(baton, HandoffKind::ApprovalWithdrawn, Some(checkpoint));
        }
        Action::RecordPublication {
            required: _,
            desired_revision: _,
            published_revision: _,
            refs: _,
        } => {
            return Err(TransitionError::LegacyPublicationUnsupported);
        }
        Action::StageExplainer {
            obligation,
            source_path,
        } => {
            require_publisher(baton, role)?;
            let binding = baton
                .publication_binding
                .as_ref()
                .ok_or(TransitionError::PublicationStale)?;
            if binding.obligation != obligation {
                return Err(TransitionError::StalePublicationBinding);
            }
            let (source_digest, byte_length) =
                stage_explainer_bytes(channel.directory(), &source_path)?;
            let publisher_harness = caller_harness(baton, role).to_owned();
            let binding = baton
                .publication_binding
                .as_mut()
                .ok_or(TransitionError::PublicationStale)?;
            binding.artifact = Some(ExplainerArtifact {
                obligation,
                path: explainer_artifact_path(&source_digest),
                source_digest,
                media_type: EXPLAINER_MEDIA_TYPE.to_owned(),
                byte_length,
                channel: EXPLAINER_CHANNEL.to_owned(),
                access: EXPLAINER_ACCESS.to_owned(),
                publisher_harness,
            });
            // Fresh bytes invalidate any rendering and review of the old bytes.
            binding.deployment = None;
            binding.review = None;
        }
        Action::RecordExplainerPublication {
            obligation,
            source_digest,
            site_id,
            site_version,
            url,
            channel: site_channel,
            access,
        } => {
            require_publisher(baton, role)?;
            let publisher_harness = caller_harness(baton, role).to_owned();
            let binding = baton
                .publication_binding
                .as_mut()
                .ok_or(TransitionError::PublicationStale)?;
            if binding.obligation != obligation {
                return Err(TransitionError::StalePublicationBinding);
            }
            // A Site is a rendering of bytes that already exist locally; it can
            // never be the only copy the reviewer has.
            let artifact = binding
                .artifact
                .as_ref()
                .ok_or(TransitionError::ExplainerNotStaged)?;
            if artifact.obligation != obligation || artifact.source_digest != source_digest {
                return Err(TransitionError::StalePublicationBinding);
            }
            if !valid_sha256(&source_digest)
                || !valid_exact_reference(&site_id)
                || !valid_exact_reference(&site_version)
                || !valid_exact_reference(&url)
                || !valid_exact_reference(&site_channel)
                || !valid_exact_reference(&access)
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
                channel: site_channel,
                access,
                publisher_harness,
            });
        }
        Action::RecordExplainerReview {
            obligation,
            source_digest,
            verdict,
            findings,
        } => {
            require_reviewer(baton, role)?;
            let reviewer_harness = caller_harness(baton, role).to_owned();
            let binding = baton
                .publication_binding
                .as_mut()
                .ok_or(TransitionError::PublicationStale)?;
            let artifact = binding
                .artifact
                .as_ref()
                .ok_or(TransitionError::ExplainerNotStaged)?;
            if binding.obligation != obligation
                || artifact.obligation != obligation
                || artifact.source_digest != source_digest
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
                verdict: verdict.to_owned(),
                findings,
                reviewer_harness,
            });
        }
        Action::ReportProgress { phase, detail } => {
            let detail = match detail {
                Some(detail) if detail.trim().is_empty() => {
                    return Err(TransitionError::InvalidProgress)
                }
                Some(detail) => Some(detail.trim().to_owned()),
                None => None,
            };
            record_progress(baton, role, phase, detail)?;
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
    Ok(())
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
    } else if let Some(reference) = task_reference {
        baton.task = Some(TaskIdentity {
            reference: Some(reference),
            summary: objective,
        });
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

/// The finalization gate. It binds the reviewer's approval to the exact staged
/// bytes for the current obligation. The optional Site rendering is deliberately
/// not consulted: it is never the artifact the reviewer read.
fn require_publication_gate(
    baton: &RunBaton,
    expected: Option<(&HandoffKind, &CheckpointBinding)>,
) -> Result<(), TransitionError> {
    let policy = effective_policy(baton);
    let binding = baton
        .publication_binding
        .as_ref()
        .ok_or(TransitionError::PublicationStale)?;
    let artifact = binding
        .artifact
        .as_ref()
        .ok_or(TransitionError::ExplainerNotStaged)?;
    let review = binding
        .review
        .as_ref()
        .ok_or(TransitionError::PublicationStale)?;
    if expected.is_some_and(|(kind, checkpoint)| {
        &binding.obligation.kind != kind
            || binding.obligation.checkpoint.as_ref() != Some(checkpoint)
    }) || artifact.obligation != binding.obligation
        || review.obligation != binding.obligation
        || review.source_digest != artifact.source_digest
        || review.verdict != "approved"
        || !review.findings.is_empty()
        || artifact.channel != policy.channel
        || artifact.access != policy.access
        || artifact.publisher_harness != policy.publisher_harness
        || review.reviewer_harness != policy.reviewer_harness
    {
        return Err(TransitionError::PublicationStale);
    }
    Ok(())
}

pub(crate) fn effective_policy(baton: &RunBaton) -> PublicationPolicy {
    baton
        .publication_policy
        .clone()
        .unwrap_or_else(PublicationPolicy::fixed)
}

fn require_publisher(baton: &RunBaton, role: Role) -> Result<(), TransitionError> {
    let policy = effective_policy(baton);
    if !caller_harness(baton, role)
        .trim()
        .eq_ignore_ascii_case(policy.publisher_harness.trim())
    {
        return Err(TransitionError::WrongPublisherHarness);
    }
    Ok(())
}

fn require_reviewer(baton: &RunBaton, role: Role) -> Result<(), TransitionError> {
    let policy = effective_policy(baton);
    if !caller_harness(baton, role)
        .trim()
        .eq_ignore_ascii_case(policy.reviewer_harness.trim())
    {
        return Err(TransitionError::WrongReviewerHarness);
    }
    Ok(())
}

/// Copy the publisher's explainer bytes into a content-addressed file under the
/// run directory and return their digest and length. Content addressing makes
/// re-staging identical bytes a no-op and keeps every prior obligation's bytes
/// readable for audit.
fn stage_explainer_bytes(
    run_dir: &std::path::Path,
    source_path: &std::path::Path,
) -> Result<(String, u64), TransitionError> {
    let metadata =
        std::fs::metadata(source_path).map_err(|_| TransitionError::InvalidExplainerSource)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_EXPLAINER_BYTES {
        return Err(TransitionError::InvalidExplainerSource);
    }
    let bytes = std::fs::read(source_path).map_err(|_| TransitionError::InvalidExplainerSource)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_EXPLAINER_BYTES {
        return Err(TransitionError::InvalidExplainerSource);
    }
    let source_digest = format!("{:x}", sha2::Sha256::digest(&bytes));
    let directory = run_dir.join(EXPLAINER_ARTIFACT_DIR);
    std::fs::create_dir_all(&directory).map_err(StoreError::Io)?;
    let destination = run_dir.join(explainer_artifact_path(&source_digest));
    if !destination.exists() {
        let temporary = directory.join(format!(".{source_digest}.{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temporary, &bytes).map_err(StoreError::Io)?;
        std::fs::rename(&temporary, &destination).map_err(StoreError::Io)?;
    }
    Ok((source_digest, bytes.len() as u64))
}

fn record_progress(
    baton: &mut RunBaton,
    role: Role,
    phase: ProgressPhase,
    detail: Option<String>,
) -> Result<(), TransitionError> {
    let updated_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_| TransitionError::InvalidProgress)?;
    let participant = match role {
        Role::Worker => &mut baton.participants.worker,
        Role::Reviewer => &mut baton.participants.reviewer,
    };
    participant.progress = Some(ParticipantProgress {
        phase,
        detail,
        updated_at: updated_at.clone(),
    });
    // Reporting progress is also a liveness signal: extend this role's own lease
    // so long authorized work never looks like a dead session.
    if let Some(claim) = participant.claim.as_mut() {
        let expires = time::OffsetDateTime::now_utc()
            .checked_add(time::Duration::seconds(claim.lease_seconds as i64))
            .ok_or(TransitionError::InvalidProgress)?
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|_| TransitionError::InvalidProgress)?;
        claim.lease_started_at = Some(updated_at);
        claim.lease_expires_at = expires;
    }
    Ok(())
}

fn caller_harness(baton: &RunBaton, role: Role) -> &str {
    match role {
        Role::Worker => &baton.participants.worker.harness,
        Role::Reviewer => &baton.participants.reviewer.harness,
    }
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
