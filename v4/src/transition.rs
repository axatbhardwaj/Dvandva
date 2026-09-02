use std::collections::HashSet;

use sha2::Digest;
use std::os::unix::fs::PermissionsExt;

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
        TerminalProvenance, CODEX_HARNESS, EXPLAINER_ACCESS, EXPLAINER_ARTIFACT_DIR,
        EXPLAINER_CHANNEL, EXPLAINER_MEDIA_TYPE, MAX_EXPLAINER_BYTES, SITES_ACCESS, SITES_CHANNEL,
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
    #[error("only a Codex participant may publish the approved explainer to Sites")]
    WrongPublisherHarness,
    #[error("only vadi may author the local explainer")]
    WrongExplainerAuthor,
    #[error("only prativadi may review the local explainer")]
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
    #[error("the local explainer bytes are not approved for publication")]
    ExplainerNotApproved,
    #[error("the approved explainer bytes have not been published to a private Site")]
    ExplainerNotPublished,
    #[error("progress detail must be non-blank when present")]
    InvalidProgress,
    #[error("checkpoint artifacts are not immutable for this checkpoint kind")]
    InvalidCheckpointArtifact,
    #[error("staged explainer bytes are missing or no longer match their digest")]
    ExplainerBytesMissing,
    #[error("analysis checkpoint cites a digest that has not been staged for this run")]
    AnalysisNotStaged,
    #[error("a protocol-internal recovery is available; take it instead of parking the run")]
    AutonomousRecoveryAvailable,
    #[error("a cited analysis artifact is missing or no longer matches its digest")]
    AnalysisBytesMissing,
    #[error("this receipt was prepared against an older state of the obligation")]
    StaleReceiptSequence,
    #[error("a human decision is answered by choosing one of its options")]
    AnswerNotAnOption,
    #[error("a scope decision resolves only through a scope amendment; a pause that changes nothing was an approval wait")]
    DecisionWithoutChange,
    #[error("an autonomous run admits a pause only as a choice among concrete scope proposals")]
    NotAnAutonomousDecision,
    #[error("the decision just answered cannot be asked again")]
    RepeatedDecision,
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
        channel.mutate_locked_untracked(|baton, now| mutate(baton, now, action), |current| current)
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
                    // Requested changes hand real work back: the next delivery
                    // will carry different bytes, so the current explainer
                    // receipts stop describing it and the obligation turns over.
                    replace_handoff_obligation(
                        baton,
                        HandoffKind::ReviewerToWorker,
                        Some(submitted_binding.clone()),
                    );
                    "changes_requested"
                }
                ReviewVerdict::Approved => {
                    if baton.pending_checkpoint_supersession.is_some() {
                        return Err(TransitionError::SupersessionPending);
                    }
                    if !findings.is_empty() {
                        return Err(TransitionError::BlockingFindings);
                    }
                    let checkpoint = baton
                        .checkpoint
                        .as_ref()
                        .ok_or(TransitionError::InvalidCheckpoint)?;
                    require_checkpoint_artifacts_intact(channel.directory(), checkpoint)?;
                    baton.status = Status::Finalizing;
                    baton.assignee = Assignee::Worker;
                    "approved"
                }
            };
            // An approval transfers no new work product, so it opens no fresh
            // explainer obligation: the receipts staged for the approved
            // checkpoint stay valid and `finalize` needs one handshake, not two.
            baton.review = Some(ReviewReceipt {
                verdict: verdict_name.to_owned(),
                checkpoint_identity: submitted_binding.checkpoint_identity.clone(),
                manifest_digest: submitted_binding.manifest_digest.clone(),
                scope_revision: submitted_binding.scope_revision,
                findings,
            });
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
            // The current kernel leaves the worker_to_reviewer obligation (and
            // its receipts) in place at approval; a run wedged by an older
            // kernel carries a reviewer_to_worker obligation instead. Either is
            // finalizable when bound to the approved checkpoint.
            require_publication_gate(
                baton,
                Some((
                    &[HandoffKind::WorkerToReviewer, HandoffKind::ReviewerToWorker][..],
                    &checkpoint_binding,
                )),
            )?;
            let artifact = baton
                .publication_binding
                .as_ref()
                .and_then(|binding| binding.artifact.as_ref())
                .ok_or(TransitionError::ExplainerNotStaged)?;
            require_staged_bytes_intact(channel.directory(), artifact)?;
            let checkpoint = baton
                .checkpoint
                .as_ref()
                .ok_or(TransitionError::InvalidCheckpoint)?;
            require_checkpoint_artifacts_intact(channel.directory(), checkpoint)?;
            baton.status = Status::Done;
            baton.assignee = Assignee::None;
            baton.terminal = Some(TerminalProvenance {
                outcome: "done".to_owned(),
                reason: None,
            });
        }
        Action::RequestHumanDecision(request) => {
            let crate::action::HumanDecisionRequest {
                kind,
                question,
                evidence,
                options,
                proposals,
            } = request;
            // Enforceable autonomy, not just a label on the request: while the
            // kernel itself offers a deterministic recovery, parking the run is
            // refused whatever the request claims to be about. This is exactly
            // the PR-914 escalation — an unreadable publication policy is
            // repairable, so it is never a question for a human.
            if !effective_policy(baton).reviewer_can_read() {
                return Err(TransitionError::AutonomousRecoveryAvailable);
            }
            // Likewise a changes-requested explainer: vadi's recovery is to
            // restage, and asking a human to bless a rewrite instead is
            // the approval wait the protocol exists to avoid.
            let author_owes_restage = baton
                .publication_binding
                .as_ref()
                .and_then(|binding| binding.review.as_ref())
                .is_some_and(|review| review.verdict == "changes_requested")
                && role == Role::Worker;
            if author_owes_restage {
                return Err(TransitionError::AutonomousRecoveryAvailable);
            }
            let options = options
                .into_iter()
                .map(|option| option.trim().to_owned())
                .collect::<Vec<_>>();
            let distinct = options.iter().collect::<HashSet<_>>().len() == options.len();
            let question = question.trim().to_owned();
            // Admission is where an approval wait is made unrepresentable. An
            // autonomous run admits a pause only as a choice among concrete
            // scope proposals the kernel applies itself: no proposals, or a
            // question of any other kind, has no admissible shape when the
            // human may be absent. In every mode, proposals are one per option
            // and distinct, and the decision just answered cannot be re-asked.
            if baton.interaction == crate::model::InteractionMode::Autonomous
                && (kind != crate::model::HumanDecisionKind::Scope || proposals.is_empty())
            {
                return Err(TransitionError::NotAnAutonomousDecision);
            }
            // Every proposal is canonicalized here with exactly the rules that
            // will apply it, so an admitted option can never fail to resolve.
            let proposals = proposals
                .into_iter()
                .map(normalize_scope_proposal)
                .collect::<Result<Vec<_>, _>>()?;
            if !proposals.is_empty() {
                let normalized = proposals
                    .iter()
                    .map(|proposal| serde_json::to_string(proposal).unwrap_or_default())
                    .collect::<HashSet<_>>();
                if proposals.len() != options.len() || normalized.len() != proposals.len() {
                    return Err(TransitionError::InvalidHumanDecision);
                }
            }
            if baton.human_decision.as_ref().is_some_and(|previous| {
                previous.answer.is_some()
                    && previous.kind == kind
                    && previous.question == question
                    && previous.options == options
            }) {
                return Err(TransitionError::RepeatedDecision);
            }
            if baton
                .human_decision
                .as_ref()
                .is_some_and(|decision| decision.answer.is_none())
                || question.trim().is_empty()
                || evidence.is_empty()
                || evidence.iter().any(|item| item.trim().is_empty())
                || options.len() < 2
                || options.iter().any(String::is_empty)
                || !distinct
            {
                return Err(TransitionError::InvalidHumanDecision);
            }
            let resume_status = baton.status.clone();
            let resume_assignee = baton.assignee.clone();
            baton.human_decision = Some(HumanDecision {
                kind,
                version: crate::model::DECISION_VERSION,
                proposals,
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
            let answer = answer.trim().to_owned();
            let (kind, version, chosen_proposal, resume_status, resume_assignee) = {
                let decision = baton
                    .human_decision
                    .as_mut()
                    .ok_or(TransitionError::InvalidHumanDecision)?;
                if decision.contact_role != role_name(role) {
                    return Err(TransitionError::WrongContact);
                }
                let current_version = decision.version >= crate::model::DECISION_VERSION;
                // A decision is a choice among the options that were put to the
                // human. Free-form prose — "yes", "approved", "go ahead" — is
                // not a choice unless it was one of the options. Decisions
                // recorded before this rule keep their original resolution, so
                // a released client never creates a pause it cannot clear.
                let chosen = decision
                    .options
                    .iter()
                    .position(|option| option.trim() == answer);
                if current_version && chosen.is_none() {
                    return Err(TransitionError::AnswerNotAnOption);
                }
                decision.answer = Some(answer.clone());
                (
                    decision.kind,
                    decision.version,
                    chosen.and_then(|index| decision.proposals.get(index).cloned()),
                    decision.resume_status.clone(),
                    decision.resume_assignee.clone(),
                )
            };
            // A pause that resolves into no change to the run was an approval
            // wait, whatever it was called. A scope decision resolves through
            // the proposal that was chosen or an explicit amendment; an intent
            // or authority decision is recorded on the canonical objective.
            if let Some(amendment) = scope_amendment {
                apply_scope_amendment(baton, amendment)?;
            } else if let Some(proposal) = chosen_proposal {
                apply_scope_amendment(baton, proposal)?;
            } else if version >= crate::model::DECISION_VERSION {
                match kind {
                    crate::model::HumanDecisionKind::Scope => {
                        return Err(TransitionError::DecisionWithoutChange);
                    }
                    crate::model::HumanDecisionKind::Intent
                    | crate::model::HumanDecisionKind::Authority => {
                        baton.objective.refs.push(crate::model::ExternalRef {
                            kind: kind.reference_kind().to_owned(),
                            value: answer,
                        });
                    }
                }
                baton.status = resume_status;
                baton.assignee = resume_assignee;
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
            after_seq,
            source_path,
        } => {
            require_explainer_author(role)?;
            let binding = baton
                .publication_binding
                .as_ref()
                .ok_or(TransitionError::PublicationStale)?;
            if binding.obligation != obligation {
                return Err(TransitionError::StalePublicationBinding);
            }
            // An exact replay by the same author changes nothing and is a no-op
            // whatever sequence it was prepared against. If an upgraded run's
            // worker restages bytes authored under the old fixed harness policy,
            // the digest stays stable but the author receipt must be rewritten.
            let incoming_digest = source_digest(&source_path)?;
            let publisher_harness = caller_harness(baton, role).to_owned();
            if binding.artifact.as_ref().is_some_and(|artifact| {
                artifact.obligation == obligation
                    && artifact.source_digest == incoming_digest
                    && artifact
                        .publisher_harness
                        .trim()
                        .eq_ignore_ascii_case(publisher_harness.trim())
            }) {
                return Ok(());
            }
            require_receipt_seq(binding, after_seq)?;
            let (source_digest, byte_length) =
                stage_explainer_bytes(channel.directory(), &source_path)?;
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
            binding.receipt_seq += 1;
        }
        Action::RecordExplainerPublication {
            obligation,
            after_seq,
            source_digest,
            site_id,
            site_version,
            url,
            channel: site_channel,
            access,
        } => {
            require_sites_publisher(baton, role)?;
            let publisher_harness = caller_harness(baton, role).to_owned();
            let binding = baton
                .publication_binding
                .as_ref()
                .ok_or(TransitionError::PublicationStale)?;
            if binding.obligation != obligation {
                return Err(TransitionError::StalePublicationBinding);
            }
            // An exact replay of the recorded rendering is a no-op.
            if binding.deployment.as_ref().is_some_and(|deployment| {
                deployment.obligation == obligation
                    && deployment.source_digest == source_digest
                    && deployment.site_id == site_id
                    && deployment.site_version == site_version
                    && deployment.url == url
                    && deployment.channel == site_channel
                    && deployment.access == access
            }) {
                return Ok(());
            }
            require_receipt_seq(binding, after_seq)?;
            // A Site is a rendering of bytes that already exist locally; it can
            // never be the only copy the reviewer has.
            let artifact = binding
                .artifact
                .as_ref()
                .ok_or(TransitionError::ExplainerNotStaged)?;
            if artifact.obligation != obligation || artifact.source_digest != source_digest {
                return Err(TransitionError::StalePublicationBinding);
            }
            if !baton.local_explainer_approved(binding) {
                return Err(TransitionError::ExplainerNotApproved);
            }
            if !valid_sha256(&source_digest)
                || !valid_exact_reference(&site_id)
                || !valid_exact_reference(&site_version)
                || !valid_exact_reference(&url)
                || !valid_exact_reference(&site_channel)
                || !valid_exact_reference(&access)
                || site_channel != SITES_CHANNEL
                || access != SITES_ACCESS
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
            let binding = baton
                .publication_binding
                .as_mut()
                .ok_or(TransitionError::PublicationStale)?;
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
            binding.receipt_seq += 1;
        }
        Action::RecordExplainerReview {
            obligation,
            after_seq,
            source_digest,
            verdict,
            findings,
        } => {
            require_explainer_reviewer(role)?;
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
            // An exact replay of the recorded verdict is a no-op.
            let incoming_findings = findings
                .iter()
                .map(|finding| finding.trim().to_owned())
                .collect::<Vec<_>>();
            let incoming_verdict = match verdict {
                ReviewVerdict::Approved => "approved",
                ReviewVerdict::ChangesRequested => "changes_requested",
            };
            if binding.review.as_ref().is_some_and(|review| {
                review.obligation == obligation
                    && review.source_digest == source_digest
                    && review.verdict == incoming_verdict
                    && review.findings == incoming_findings
            }) {
                return Ok(());
            }
            require_receipt_seq(binding, after_seq)?;
            require_staged_bytes_intact(channel.directory(), artifact)?;
            // Ordering: once these exact bytes carry a verdict, a later delivery
            // of a different verdict for the same bytes is stale, not an
            // update. Restaging is how a new verdict is legitimately obtained.
            if let Some(existing) = binding.review.as_ref() {
                let same_bytes =
                    existing.obligation == obligation && existing.source_digest == source_digest;
                let incoming_verdict = match verdict {
                    ReviewVerdict::Approved => "approved",
                    ReviewVerdict::ChangesRequested => "changes_requested",
                };
                if same_bytes && existing.verdict != incoming_verdict {
                    return Err(TransitionError::StalePublicationBinding);
                }
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
            binding.receipt_seq += 1;
        }
        Action::StageAnalysis { source_path } => {
            require_owner(baton, role, Role::Worker, Assignee::Worker)?;
            if !matches!(baton.status, Status::Working | Status::Revising) {
                return Err(TransitionError::IllegalState);
            }
            let digest = stage_content_addressed(
                channel.directory(),
                &source_path,
                crate::model::ANALYSIS_ARTIFACT_DIR,
                crate::model::analysis_artifact_path,
            )?;
            if let Err(index) = baton.staged_analysis.binary_search(&digest) {
                baton.staged_analysis.insert(index, digest);
            }
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
        if !crate::model::valid_checkpoint_shape(&kind, &identity, &deliverable.artifacts) {
            return Err(TransitionError::InvalidCheckpointArtifact);
        }
        // An analysis deliverable is only reviewable if its bytes are actually
        // present, so a manifest may only cite digests this run has staged.
        if kind == crate::model::CHECKPOINT_KIND_ANALYSIS
            && !deliverable
                .artifacts
                .iter()
                .all(|artifact| baton.staged_analysis.contains(&artifact.value))
        {
            return Err(TransitionError::AnalysisNotStaged);
        }
        deliverable
            .artifacts
            .sort_by(|left, right| (&left.kind, &left.value).cmp(&(&right.kind, &right.value)));
        deliverables.push(deliverable);
    }
    // An analysis identity must be derived from the bytes it cites, so a
    // manifest cannot claim an identity unrelated to its own content.
    if kind == crate::model::CHECKPOINT_KIND_ANALYSIS {
        let cited = deliverables
            .iter()
            .flat_map(|deliverable| deliverable.artifacts.iter())
            .map(|artifact| artifact.value.clone())
            .collect::<Vec<_>>();
        if identity != crate::model::analysis_checkpoint_identity(&cited) {
            return Err(TransitionError::InvalidCheckpointArtifact);
        }
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

/// The one canonical form for a scope change, whether proposed or applied.
/// A proposal that does not canonicalize is refused at admission, so every
/// option a human is offered is one the kernel can actually apply.
pub fn normalize_scope_proposal(
    proposal: ScopeAmendment,
) -> Result<ScopeAmendment, TransitionError> {
    let objective = proposal.objective.trim().to_owned();
    if objective.is_empty() {
        return Err(TransitionError::InvalidHumanDecision);
    }
    let mut refs = proposal.objective_refs;
    for reference in &mut refs {
        reference.kind = reference.kind.trim().to_owned();
        reference.value = reference.value.trim().to_owned();
        if reference.kind.is_empty() || reference.value.is_empty() {
            return Err(TransitionError::InvalidHumanDecision);
        }
    }
    let task_reference = proposal
        .task_reference
        .map(|reference| reference.trim().to_owned());
    if task_reference.as_ref().is_some_and(String::is_empty) {
        return Err(TransitionError::InvalidHumanDecision);
    }
    let scope_deliverables = normalize_deliverables(proposal.scope_deliverables)
        .map_err(|_| TransitionError::InvalidHumanDecision)?;
    Ok(ScopeAmendment {
        objective,
        objective_refs: refs,
        task_reference,
        scope_deliverables,
    })
}

fn apply_scope_amendment(
    baton: &mut RunBaton,
    amendment: ScopeAmendment,
) -> Result<(), TransitionError> {
    let ScopeAmendment {
        objective,
        objective_refs: refs,
        task_reference,
        scope_deliverables,
    } = normalize_scope_proposal(amendment)?;
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

/// The finalization gate. Prativadi approves vadi's exact local bytes. When the
/// pairing contains Codex, that participant must also publish the same digest
/// to the run's private status Site.
fn require_publication_gate(
    baton: &RunBaton,
    expected: Option<(&[HandoffKind], &CheckpointBinding)>,
) -> Result<(), TransitionError> {
    let binding = baton
        .publication_binding
        .as_ref()
        .ok_or(TransitionError::PublicationStale)?;
    if binding.artifact.is_none() {
        return Err(TransitionError::ExplainerNotStaged);
    }
    if !baton.local_explainer_approved(binding) {
        return Err(TransitionError::PublicationStale);
    }
    if baton.has_codex_participant() && binding.deployment.is_none() {
        return Err(TransitionError::ExplainerNotPublished);
    }
    if !baton.publication_gate_satisfied(binding, expected) {
        return Err(TransitionError::PublicationStale);
    }
    Ok(())
}

/// Re-hash every analysis artifact a checkpoint cites. A checkpoint is only
/// immutable if the bytes behind it are still there and still themselves, so a
/// deleted or tampered deliverable must not reach an approval or a terminal
/// state.
fn require_checkpoint_artifacts_intact(
    run_dir: &std::path::Path,
    checkpoint: &Checkpoint,
) -> Result<(), TransitionError> {
    if checkpoint.kind != crate::model::CHECKPOINT_KIND_ANALYSIS {
        return Ok(());
    }
    for deliverable in &checkpoint.deliverables {
        for artifact in &deliverable.artifacts {
            let relative = crate::model::analysis_artifact_path(&artifact.value);
            let bytes = crate::store::read_private_file_beneath(run_dir, &relative)
                .map_err(|_| TransitionError::AnalysisBytesMissing)?;
            if format!("{:x}", sha2::Sha256::digest(&bytes)) != artifact.value {
                return Err(TransitionError::AnalysisBytesMissing);
            }
        }
    }
    Ok(())
}

/// Re-hash the staged bytes named by an approved artifact. Baton metadata alone
/// cannot prove the file still holds what the reviewer read, so deletion or
/// tampering after approval must not be able to reach a terminal state.
fn require_staged_bytes_intact(
    run_dir: &std::path::Path,
    artifact: &ExplainerArtifact,
) -> Result<(), TransitionError> {
    let bytes = crate::store::read_private_file_beneath(run_dir, &artifact.path)
        .map_err(|_| TransitionError::ExplainerBytesMissing)?;
    if bytes.len() as u64 != artifact.byte_length
        || format!("{:x}", sha2::Sha256::digest(&bytes)) != artifact.source_digest
    {
        return Err(TransitionError::ExplainerBytesMissing);
    }
    Ok(())
}

/// Targeted concurrency control for obligation-bound writes: a caller that saw
/// receipt N may only write receipt N. Claim and progress edges do not advance
/// this, so unrelated peer activity never invalidates a prepared receipt, while
/// a delayed or out-of-order one is refused instead of overwriting newer state.
fn require_receipt_seq(
    binding: &crate::model::PublicationBinding,
    after_seq: Option<u64>,
) -> Result<(), TransitionError> {
    // An omitted sequence is the released API-2 shape, and is honoured only
    // where it is unambiguous: as the first receipt of an obligation. Once any
    // receipt exists, a write must say what it was prepared against, or a
    // delayed payload could silently regress newer state.
    let prepared_against = after_seq.unwrap_or(0);
    if prepared_against != binding.receipt_seq {
        return Err(TransitionError::StaleReceiptSequence);
    }
    Ok(())
}

/// The sha256 of the bytes at `path`, subject to the staging size limits.
fn source_digest(path: &std::path::Path) -> Result<String, TransitionError> {
    let metadata = std::fs::metadata(path).map_err(|_| TransitionError::InvalidExplainerSource)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_EXPLAINER_BYTES {
        return Err(TransitionError::InvalidExplainerSource);
    }
    let bytes = std::fs::read(path).map_err(|_| TransitionError::InvalidExplainerSource)?;
    Ok(format!("{:x}", sha2::Sha256::digest(&bytes)))
}

pub(crate) fn effective_policy(baton: &RunBaton) -> PublicationPolicy {
    baton.effective_publication_policy()
}

fn require_explainer_author(role: Role) -> Result<(), TransitionError> {
    if role != Role::Worker {
        return Err(TransitionError::WrongExplainerAuthor);
    }
    Ok(())
}

fn require_sites_publisher(baton: &RunBaton, role: Role) -> Result<(), TransitionError> {
    if !caller_harness(baton, role).eq_ignore_ascii_case(CODEX_HARNESS) {
        return Err(TransitionError::WrongPublisherHarness);
    }
    Ok(())
}

fn require_explainer_reviewer(role: Role) -> Result<(), TransitionError> {
    if role != Role::Reviewer {
        return Err(TransitionError::WrongReviewerHarness);
    }
    Ok(())
}

/// Copy caller-supplied bytes into a content-addressed, mode-restricted file
/// under the run directory and return their digest.
fn stage_content_addressed(
    run_dir: &std::path::Path,
    source_path: &std::path::Path,
    directory_name: &str,
    relative: impl Fn(&str) -> String,
) -> Result<String, TransitionError> {
    let metadata =
        std::fs::metadata(source_path).map_err(|_| TransitionError::InvalidExplainerSource)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_EXPLAINER_BYTES {
        return Err(TransitionError::InvalidExplainerSource);
    }
    let bytes = std::fs::read(source_path).map_err(|_| TransitionError::InvalidExplainerSource)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_EXPLAINER_BYTES {
        return Err(TransitionError::InvalidExplainerSource);
    }
    let digest = format!("{:x}", sha2::Sha256::digest(&bytes));
    let directory = run_dir.join(directory_name);
    crate::store::create_private_dir(&directory).map_err(StoreError::Io)?;
    let destination = run_dir.join(relative(&digest));
    // Reuse only a private regular file inside the run whose bytes really hash
    // to its name. A symlink, a group-readable file, or tampered content is
    // replaced rather than trusted.
    let reusable = crate::store::is_private_regular_file(&destination)
        && crate::store::read_private_file_beneath(run_dir, &relative(&digest))
            .is_ok_and(|existing| format!("{:x}", sha2::Sha256::digest(&existing)) == digest);
    if !reusable && std::fs::symlink_metadata(&destination).is_ok() {
        std::fs::remove_file(&destination).map_err(StoreError::Io)?;
    }
    if !reusable {
        let temporary = directory.join(format!(".{digest}.{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temporary, &bytes).map_err(StoreError::Io)?;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))
            .map_err(StoreError::Io)?;
        std::fs::rename(&temporary, &destination).map_err(StoreError::Io)?;
    }
    Ok(digest)
}

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
    // `access: run_private` is a promise about the bytes on disk, so the modes
    // are set explicitly rather than inherited from the caller's umask.
    let directory = run_dir.join(EXPLAINER_ARTIFACT_DIR);
    crate::store::create_private_dir(&directory).map_err(StoreError::Io)?;
    let destination = run_dir.join(explainer_artifact_path(&source_digest));
    // Content addressing only holds if the existing file really is those bytes;
    // a tampered or truncated file is replaced rather than trusted.
    let reusable = std::fs::read(&destination)
        .is_ok_and(|existing| format!("{:x}", sha2::Sha256::digest(&existing)) == source_digest);
    if !reusable {
        let temporary = directory.join(format!(".{source_digest}.{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temporary, &bytes).map_err(StoreError::Io)?;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))
            .map_err(StoreError::Io)?;
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
    // One clock reading: the lease invariant is an exact
    // `started_at + lease_seconds == expires_at`.
    let now = time::OffsetDateTime::now_utc();
    let rfc3339 = &time::format_description::well_known::Rfc3339;
    let updated_at = now
        .format(rfc3339)
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
        let expires = now
            .checked_add(time::Duration::seconds(claim.lease_seconds as i64))
            .ok_or(TransitionError::InvalidProgress)?
            .format(rfc3339)
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
