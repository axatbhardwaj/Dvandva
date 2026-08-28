use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Serialize;
use sha2::Digest;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    action::Action,
    claim::{self, ClaimError, Role},
    credential::{self, Credential, CredentialError},
    discovery::{
        self, ClaimState, DiscoveryError, DiscoveryKind, DiscoveryOutcome, DiscoveryQuery,
        RequestedScope,
    },
    identity::{self, IdentityError},
    model::{
        normalize_participants, DeliverableRequirement, ExternalRef, ModelError,
        ParticipantProgress, RunBaton, Status, TaskIdentity, LEGACY_SCHEMA,
    },
    next_action::{self, NextActions},
    store::{require_current_schema, RunChannel, StoreError},
    transition::{self, TransitionError},
    wait::{self, WaitError, WaitOutcome},
};

#[derive(Debug, Error)]
pub enum RoleSessionError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Claim(#[from] ClaimError),
    #[error(transparent)]
    Credential(#[from] CredentialError),
    #[error(transparent)]
    Transition(#[from] TransitionError),
    #[error(transparent)]
    Wait(#[from] WaitError),
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Upgrade(#[from] UpgradeError),
}

#[derive(Debug, Error)]
pub enum UpgradeError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("terminal v1 runs cannot be upgraded")]
    Terminal,
    #[error("same-role claim is busy in another live session")]
    Busy,
    #[error(transparent)]
    Credential(#[from] CredentialError),
    #[error(transparent)]
    Claim(#[from] ClaimError),
    #[error("upgrade session id must not be blank")]
    InvalidSession,
    #[error("legacy objective must not be blank")]
    InvalidObjective,
    #[error("upgrade caller does not match the stored participant topology")]
    InvalidTopology,
    #[error("upgrade requires a v1 baton")]
    InvalidSchema,
    #[error("invalid stored lease timestamp")]
    InvalidTimestamp,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RoleStartResult {
    Started(Box<StartedRole>),
    Discovery(DiscoveryOutcome),
    Upgrade(UpgradeRequiredRole),
    PublicationUnreadable(UnreadablePublicationRole),
}

/// A run whose publication policy names a reviewer that cannot read the
/// publisher's channel can never reach an explainer review. Surface it before
/// any claim, rather than after the publisher has already spent a deployment.
#[derive(Debug, Serialize)]
pub struct UnreadablePublicationRole {
    pub outcome: &'static str,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub revision: u64,
    pub publication_policy: crate::model::PublicationPolicy,
    pub reason: &'static str,
    pub next_action: &'static str,
    pub next_actions: [&'static str; 1],
    pub actionable: bool,
}

#[derive(Debug, Serialize)]
pub struct UpgradeRequiredRole {
    pub outcome: &'static str,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub revision: u64,
    pub from_schema: &'static str,
    pub next_action: &'static str,
    pub next_actions: [&'static str; 1],
    pub actionable: bool,
    pub objective: crate::model::Objective,
    pub task_reference: Option<String>,
    pub task_summary: String,
    pub scope_revision: u64,
    pub scope_deliverables: Vec<DeliverableRequirement>,
    pub status: Status,
    pub assignee: crate::model::Assignee,
}

#[derive(Debug, Serialize)]
pub struct StartedRole {
    pub outcome: &'static str,
    pub disposition: &'static str,
    #[serde(flatten)]
    pub snapshot: RoleSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_credential_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_prompt: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoleSnapshot {
    #[serde(flatten)]
    pub baton: RunBaton,
    pub run_dir: PathBuf,
    pub peer: PeerStatus,
    /// Present only on the result of a foreground wait.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_outcome: Option<WaitOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explainer: Option<StagedExplainer>,
    #[serde(flatten)]
    pub actions: NextActions,
}

/// What the other role is doing right now. Without this a worker can only see
/// an unfulfilled obligation and a lease timestamp, and cannot tell a peer that
/// is mid-way through a long publication from one that died.
#[derive(Debug, Serialize)]
pub struct PeerStatus {
    pub role: &'static str,
    pub harness: String,
    pub claim_state: ClaimState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ParticipantProgress>,
}

/// Absolute location of the digest-bound explainer bytes for the current
/// obligation, so the reviewing role can read exactly what was staged.
#[derive(Debug, Serialize)]
pub struct StagedExplainer {
    pub source_digest: String,
    pub path: PathBuf,
    pub media_type: String,
    pub byte_length: u64,
    pub channel: String,
    pub access: String,
}

#[derive(Clone, Copy)]
pub struct RoleStartRequest<'a> {
    pub workspace: &'a Path,
    pub runs_dir: &'a Path,
    pub credentials_root: &'a Path,
    pub role: Role,
    pub session_id: &'a str,
    pub current_harness: &'a str,
    pub peer_harness: &'a str,
    pub objective: Option<&'a str>,
    pub objective_refs: &'a [ExternalRef],
    pub task_reference: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub lease_seconds: u64,
    pub wait: bool,
    pub poll_interval: Duration,
    pub timeout: Duration,
    pub new_run: bool,
    pub required_deliverables: &'a [DeliverableRequirement],
    pub interaction: crate::model::InteractionMode,
}

/// A run with no live claim whose head has not moved for this long is treated as
/// abandoned by non-exact discovery. Exact `--run-id` joins still reach it.
pub const STALE_RUN_DAYS: u64 = 14;

pub fn start(request: RoleStartRequest<'_>) -> Result<RoleStartResult, RoleSessionError> {
    start_with_retries(request, 8)
}

fn start_with_retries(
    request: RoleStartRequest<'_>,
    remaining_conflicts: u8,
) -> Result<RoleStartResult, RoleSessionError> {
    validate_start(
        request.session_id,
        request.current_harness,
        request.peer_harness,
        request.objective,
        request.objective_refs,
        request.task_reference,
    )?;
    if request.run_id.is_none() && request.objective.is_none() {
        return Err(RoleSessionError::Invalid(
            "non-exact role start requires an objective".to_owned(),
        ));
    }
    let normalized = match request.role {
        Role::Worker => normalize_participants(
            request.current_harness.to_owned(),
            request.peer_harness.to_owned(),
        ),
        Role::Reviewer => normalize_participants(
            request.peer_harness.to_owned(),
            request.current_harness.to_owned(),
        ),
    }?;
    let participant_harness = match request.role {
        Role::Worker => normalized.0.as_str(),
        Role::Reviewer => normalized.1.as_str(),
    };
    credential::path(
        request.credentials_root,
        request.session_id,
        "validation",
        request.role,
    )?;
    if request.lease_seconds == 0 || request.lease_seconds > i64::MAX as u64 {
        return Err(RoleSessionError::Invalid(
            "lease seconds are outside the supported range".to_owned(),
        ));
    }
    let workspace_identity = identity::identify(request.workspace)?;
    let query = DiscoveryQuery {
        repository_id: &workspace_identity.repository_id,
        role: request.role,
        participant_harness,
        task_reference: request.task_reference,
        objective: request.objective,
        run_id: request.run_id,
        session_id: Some(request.session_id),
        stale_after_days: Some(STALE_RUN_DAYS),
    };
    let mut outcome = discovery::discover(request.runs_dir, query)?;
    if request.new_run && request.run_id.is_none() {
        if request.role != Role::Worker {
            return Err(RoleSessionError::Invalid(
                "only a worker may create a separate run".to_owned(),
            ));
        }
        if outcome.outcome != DiscoveryKind::Corrupt {
            outcome.outcome = DiscoveryKind::None;
            outcome.candidates.clear();
        }
    } else {
        classify_scope_mismatch(&mut outcome, &request);
    }
    if outcome.outcome == DiscoveryKind::None && request.role == Role::Reviewer && request.wait {
        outcome = discovery::wait_for_match(
            request.runs_dir,
            query,
            request.poll_interval,
            request.timeout,
            true,
        )?;
        classify_scope_mismatch(&mut outcome, &request);
    }
    match outcome.outcome {
        DiscoveryKind::Match => {
            let candidate = outcome.candidates.remove(0);
            let result = start_candidate(
                candidate,
                request.credentials_root,
                request.role,
                request.session_id,
                request.lease_seconds,
            );
            retry_start_conflict(result, request, remaining_conflicts)
        }
        DiscoveryKind::UpgradeRequired => {
            let candidate = outcome.candidates.remove(0);
            Ok(RoleStartResult::Upgrade(UpgradeRequiredRole {
                outcome: "upgrade_required",
                run_id: candidate.run_id,
                run_dir: candidate.run_dir,
                revision: candidate.revision,
                from_schema: LEGACY_SCHEMA,
                next_action: "upgrade_protocol",
                next_actions: ["upgrade_protocol"],
                actionable: true,
                objective: candidate.objective,
                task_reference: candidate.task_reference,
                task_summary: candidate.task_summary,
                scope_revision: candidate.scope_revision,
                scope_deliverables: candidate.scope_deliverables,
                status: candidate.status,
                assignee: candidate.assignee,
            }))
        }
        DiscoveryKind::None if request.role == Role::Worker => {
            std::fs::create_dir_all(request.runs_dir).map_err(StoreError::Io)?;
            let objective = request.objective.ok_or_else(|| {
                RoleSessionError::Invalid("new worker creation requires an objective".to_owned())
            })?;
            let run_id = new_run_id(request.task_reference.unwrap_or(objective));
            let run_dir = request.runs_dir.join(&run_id);
            let (worker, reviewer) = match request.role {
                Role::Worker => (request.current_harness, request.peer_harness),
                Role::Reviewer => unreachable!(),
            };
            let task = TaskIdentity {
                reference: request.task_reference.map(|value| value.trim().to_owned()),
                summary: objective.trim().to_owned(),
            };
            let mut baton = RunBaton::new(
                &run_id,
                objective.trim(),
                worker,
                reviewer,
                request.required_deliverables.to_vec(),
            )?
            .with_discovery_identity(workspace_identity, task);
            baton.objective.refs = request.objective_refs.to_vec();
            baton.interaction = request.interaction;
            RunChannel::open(&run_dir).create(&baton)?;
            start_created_run(&run_dir, &baton, request, remaining_conflicts)
        }
        _ => Ok(RoleStartResult::Discovery(outcome)),
    }
}

fn start_created_run(
    run_dir: &Path,
    created: &RunBaton,
    request: RoleStartRequest<'_>,
    mut remaining_conflicts: u8,
) -> Result<RoleStartResult, RoleSessionError> {
    let mut expected_revision = created.revision;
    let grant = loop {
        match claim(
            run_dir,
            request.credentials_root,
            request.role,
            request.session_id,
            request.lease_seconds,
            expected_revision,
        ) {
            Ok(grant) => break grant,
            Err(error) if is_revision_conflict(&error) && remaining_conflicts > 0 => {
                let current = RunChannel::open(run_dir).read()?;
                validate_created_retry(created, &current)?;
                expected_revision = current.revision;
                remaining_conflicts -= 1;
            }
            Err(error) => return Err(error),
        }
    };
    finish_created_start(
        run_dir,
        request.credentials_root,
        request.role,
        request.session_id,
        grant,
    )
}

fn validate_created_retry(created: &RunBaton, current: &RunBaton) -> Result<(), RoleSessionError> {
    if matches!(current.status, Status::Done | Status::Abandoned) {
        return Err(ClaimError::Terminal.into());
    }
    let mut expected = created.clone();
    let mut actual = current.clone();
    actual.revision = expected.revision;
    actual.participants.worker.claim = expected.participants.worker.claim.take();
    actual.participants.reviewer.claim = expected.participants.reviewer.claim.take();
    if actual != expected {
        return Err(RoleSessionError::Invalid(
            "newly created run changed before the worker claim completed".to_owned(),
        ));
    }
    if current.participants.worker.claim.is_some() {
        return Err(ClaimError::Active.into());
    }
    Ok(())
}

fn retry_start_conflict(
    result: Result<RoleStartResult, RoleSessionError>,
    request: RoleStartRequest<'_>,
    remaining_conflicts: u8,
) -> Result<RoleStartResult, RoleSessionError> {
    match result {
        Err(error) if is_revision_conflict(&error) && remaining_conflicts > 0 => {
            start_with_retries(request, remaining_conflicts - 1)
        }
        result => result,
    }
}

fn is_revision_conflict(error: &RoleSessionError) -> bool {
    matches!(
        error,
        RoleSessionError::Store(StoreError::RevisionConflict { .. })
            | RoleSessionError::Claim(ClaimError::Store(StoreError::RevisionConflict { .. }))
    )
}

/// Capability preflight: refuse to join a run whose reviewer cannot read the
/// channel its publisher must use.
fn publication_preflight(candidate: &crate::discovery::RunCandidate) -> Option<RoleStartResult> {
    let baton = RunChannel::open(&candidate.run_dir).read().ok()?;
    let policy = baton
        .publication_policy
        .clone()
        .unwrap_or_else(crate::model::PublicationPolicy::fixed);
    if policy.reviewer_can_read() {
        return None;
    }
    Some(RoleStartResult::PublicationUnreadable(
        UnreadablePublicationRole {
            outcome: "publication_unreadable",
            run_id: baton.run_id,
            run_dir: candidate.run_dir.clone(),
            revision: baton.revision,
            publication_policy: policy,
            reason:
                "the reviewing harness cannot read the publisher's channel at this access level",
            next_action: "repair_publication_policy",
            next_actions: ["repair_publication_policy"],
            actionable: true,
        },
    ))
}

fn start_candidate(
    candidate: crate::discovery::RunCandidate,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
) -> Result<RoleStartResult, RoleSessionError> {
    if !matches!(candidate.status, Status::Done | Status::Abandoned) {
        if let Some(unreadable) = publication_preflight(&candidate) {
            return Ok(unreadable);
        }
    }
    if matches!(candidate.status, Status::Done | Status::Abandoned) {
        // A terminal run is reported as terminal on any supported schema. It is
        // finished, so there is nothing to migrate and nothing to claim.
        let baton = RunChannel::open(&candidate.run_dir).read()?;
        if baton.revision != candidate.revision {
            return Err(StoreError::RevisionConflict {
                expected: candidate.revision,
                actual: baton.revision,
            }
            .into());
        }
        if !matches!(baton.status, Status::Done | Status::Abandoned) {
            return Err(RoleSessionError::Invalid(
                "selected terminal run became active".to_owned(),
            ));
        }
        return Ok(RoleStartResult::Started(Box::new(StartedRole {
            outcome: "started",
            disposition: "terminal",
            snapshot: snapshot(baton, &candidate.run_dir, role),
            credential: None,
            private_credential_path: None,
            peer_prompt: None,
        })));
    }
    match candidate.claim_state {
        ClaimState::Unclaimed => {
            let grant = claim(
                &candidate.run_dir,
                credentials_root,
                role,
                session_id,
                lease_seconds,
                candidate.revision,
            )?;
            finish_candidate_claim(
                "claimed",
                &candidate.run_dir,
                credentials_root,
                role,
                session_id,
                grant,
            )
        }
        ClaimState::Expired => {
            let grant = reclaim(
                &candidate.run_dir,
                credentials_root,
                role,
                session_id,
                lease_seconds,
                candidate.revision,
            )?;
            finish_candidate_claim(
                "reclaimed",
                &candidate.run_dir,
                credentials_root,
                role,
                session_id,
                grant,
            )
        }
        ClaimState::Owned => {
            let credential =
                credential::path(credentials_root, session_id, &candidate.run_id, role)?;
            match read_snapshot_at_revision(
                &candidate.run_dir,
                credentials_root,
                role,
                session_id,
                candidate.revision,
            ) {
                Ok(snapshot) => Ok(started_role(
                    "resumed",
                    snapshot,
                    credential,
                    role == Role::Worker,
                )),
                // The baton names this session but no credential exists: the
                // process died after installing the claim and before storing
                // its token. Nobody else can act on that claim, so the session
                // replaces it with one it can prove — no human, no recovery
                // command.
                Err(RoleSessionError::Credential(CredentialError::Missing)) => {
                    let grant = reclaim_own(
                        &candidate.run_dir,
                        credentials_root,
                        role,
                        session_id,
                        lease_seconds,
                        candidate.revision,
                    )?;
                    finish_candidate_claim(
                        "reclaimed",
                        &candidate.run_dir,
                        credentials_root,
                        role,
                        session_id,
                        grant,
                    )
                }
                Err(error) => Err(error),
            }
        }
        ClaimState::Busy => Err(RoleSessionError::Invalid(
            "selected run is owned by another live session".to_owned(),
        )),
    }
}

fn finish_candidate_claim(
    disposition: &'static str,
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    grant: RoleClaimResult,
) -> Result<RoleStartResult, RoleSessionError> {
    let private = load_for_run(
        run_dir,
        credentials_root,
        role,
        session_id,
        &grant.committed_baton.run_id,
    )?;
    claim::verify(&grant.committed_baton, role, session_id, &private.token)?;
    let snapshot = snapshot(grant.committed_baton, run_dir, role);
    Ok(started_role(
        disposition,
        snapshot,
        grant.credential,
        role == Role::Worker,
    ))
}

fn started_role(
    disposition: &'static str,
    snapshot: RoleSnapshot,
    credential: PathBuf,
    include_peer_prompt: bool,
) -> RoleStartResult {
    let peer_prompt = include_peer_prompt.then(|| {
        format!(
            "Act as prativadi and join Dvandva run {}.",
            snapshot.baton.run_id
        )
    });
    RoleStartResult::Started(Box::new(StartedRole {
        outcome: "started",
        disposition,
        snapshot,
        private_credential_path: Some(credential.clone()),
        credential: Some(credential),
        peer_prompt,
    }))
}

fn finish_created_start(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    grant: RoleClaimResult,
) -> Result<RoleStartResult, RoleSessionError> {
    finish_candidate_claim(
        "created",
        run_dir,
        credentials_root,
        role,
        session_id,
        grant,
    )
}

fn read_snapshot_at_revision(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    expected_revision: u64,
) -> Result<RoleSnapshot, RoleSessionError> {
    let baton = RunChannel::open(run_dir).read()?;
    require_current_schema(&baton)?;
    if baton.revision != expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: expected_revision,
            actual: baton.revision,
        }
        .into());
    }
    let credential = load_for_run(run_dir, credentials_root, role, session_id, &baton.run_id)?;
    claim::verify(&baton, role, session_id, &credential.token)?;
    Ok(snapshot(baton, run_dir, role))
}

fn scope_matches(
    candidate: &crate::discovery::RunCandidate,
    request: &RoleStartRequest<'_>,
) -> bool {
    request
        .objective
        .is_none_or(|value| candidate.objective.summary == value.trim())
        && (request.objective_refs.is_empty() || candidate.objective.refs == request.objective_refs)
        && request
            .task_reference
            .is_none_or(|value| candidate.task_reference.as_deref() == Some(value.trim()))
        && (request.required_deliverables.is_empty()
            || candidate.scope_deliverables == request.required_deliverables)
}

fn classify_scope_mismatch(outcome: &mut DiscoveryOutcome, request: &RoleStartRequest<'_>) {
    let comparable = matches!(
        outcome.outcome,
        DiscoveryKind::Match
            | DiscoveryKind::Busy
            | DiscoveryKind::UpgradeRequired
            | DiscoveryKind::TaskMismatch
    ) && outcome.candidates.len() == 1;
    if comparable && !scope_matches(&outcome.candidates[0], request) {
        outcome.outcome = DiscoveryKind::ScopeMismatch;
        outcome.next_action = Some("retry_with_canonical_scope");
        outcome.requested_scope = Some(RequestedScope {
            objective_summary: request.objective.map(|value| value.trim().to_owned()),
            objective_refs: (!request.objective_refs.is_empty())
                .then(|| request.objective_refs.to_vec()),
            task_reference: request.task_reference.map(|value| value.trim().to_owned()),
            required_deliverables: (!request.required_deliverables.is_empty())
                .then(|| request.required_deliverables.to_vec()),
        });
    }
}

fn validate_start(
    session_id: &str,
    current_harness: &str,
    peer_harness: &str,
    objective: Option<&str>,
    objective_refs: &[ExternalRef],
    task_reference: Option<&str>,
) -> Result<(), RoleSessionError> {
    if session_id.trim().is_empty()
        || current_harness.trim().is_empty()
        || peer_harness.trim().is_empty()
        || objective.is_some_and(|value| value.trim().is_empty())
        || task_reference.is_some_and(|reference| reference.trim().is_empty())
        || objective_refs
            .iter()
            .any(|reference| reference.kind.trim().is_empty() || reference.value.trim().is_empty())
    {
        return Err(RoleSessionError::Invalid(
            "role start fields must not be blank".to_owned(),
        ));
    }
    normalize_participants(current_harness.to_owned(), peer_harness.to_owned())?;
    Ok(())
}

fn new_run_id(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character);
            separator = false;
        } else {
            separator = true;
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    let slug = if slug.is_empty() { "run" } else { slug };
    format!("{slug}-{}", &Uuid::new_v4().simple().to_string()[..8])
}

pub fn read(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
) -> Result<RoleSnapshot, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    let credential = load_for_run(run_dir, credentials_root, role, session_id, &baton.run_id)?;
    claim::verify(&baton, role, session_id, &credential.token)?;
    Ok(snapshot(baton, run_dir, role))
}

pub fn heartbeat(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
    expected_revision: u64,
) -> Result<u64, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    let credential = load_for_run(run_dir, credentials_root, role, session_id, &baton.run_id)?;
    Ok(claim::heartbeat(
        &channel,
        role,
        session_id,
        &credential.token,
        lease_seconds,
        expected_revision,
    )?)
}

pub fn wait(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    after_revision: u64,
    poll_interval: Duration,
    timeout: Duration,
) -> Result<RoleSnapshot, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    let credential = load_for_run(run_dir, credentials_root, role, session_id, &baton.run_id)?;
    let (baton, outcome) = wait::wait(
        &channel,
        role,
        session_id,
        &credential.token,
        after_revision,
        poll_interval,
        timeout,
    )?;
    let mut snapshot = snapshot(baton, run_dir, role);
    snapshot.wait_outcome = Some(outcome);
    Ok(snapshot)
}

pub fn apply(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    expected_revision: u64,
    action: Action,
) -> Result<RoleSnapshot, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    let credential = load_for_run(run_dir, credentials_root, role, session_id, &baton.run_id)?;
    let baton = transition::apply(
        &channel,
        role,
        session_id,
        &credential.token,
        expected_revision,
        action,
    )?;
    Ok(snapshot(baton, run_dir, role))
}

fn snapshot(baton: RunBaton, run_dir: &Path, role: Role) -> RoleSnapshot {
    let harness = match role {
        Role::Worker => &baton.participants.worker.harness,
        Role::Reviewer => &baton.participants.reviewer.harness,
    };
    let actions = next_action::classify(&baton, role, harness);
    let run_dir = std::fs::canonicalize(run_dir).unwrap_or_else(|_| run_dir.to_owned());
    let peer = peer_status(&baton, role);
    let explainer = baton
        .publication_binding
        .as_ref()
        .and_then(|binding| binding.artifact.as_ref())
        .map(|artifact| StagedExplainer {
            source_digest: artifact.source_digest.clone(),
            path: run_dir.join(&artifact.path),
            media_type: artifact.media_type.clone(),
            byte_length: artifact.byte_length,
            channel: artifact.channel.clone(),
            access: artifact.access.clone(),
        });
    RoleSnapshot {
        baton,
        run_dir,
        peer,
        wait_outcome: None,
        explainer,
        actions,
    }
}

fn peer_status(baton: &RunBaton, role: Role) -> PeerStatus {
    let (peer_role, participant) = match role {
        Role::Worker => ("reviewer", &baton.participants.reviewer),
        Role::Reviewer => ("worker", &baton.participants.worker),
    };
    let claim_state = match participant.claim.as_ref() {
        None => ClaimState::Unclaimed,
        Some(claim) => {
            match time::OffsetDateTime::parse(
                &claim.lease_expires_at,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(expiry) if expiry > time::OffsetDateTime::now_utc() => ClaimState::Busy,
                _ => ClaimState::Expired,
            }
        }
    };
    PeerStatus {
        role: peer_role,
        harness: participant.harness.clone(),
        claim_state,
        lease_expires_at: participant
            .claim
            .as_ref()
            .map(|claim| claim.lease_expires_at.clone()),
        progress: participant.progress.clone(),
    }
}

/// Broker the staged explainer bytes through the facade so a role never reads
/// run-directory files directly, and always verifies the digest it was handed.
pub fn read_explainer(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
) -> Result<StagedExplainerContents, RoleSessionError> {
    let snapshot = read(run_dir, credentials_root, role, session_id)?;
    let staged = snapshot
        .explainer
        .ok_or_else(|| RoleSessionError::Invalid("no explainer bytes are staged".to_owned()))?;
    // Read beneath the pinned run root, never following a symlink at any
    // component, so the bytes handed to the reviewer are the run's own.
    let relative = snapshot
        .baton
        .publication_binding
        .as_ref()
        .and_then(|binding| binding.artifact.as_ref())
        .map(|artifact| artifact.path.clone())
        .ok_or_else(|| RoleSessionError::Invalid("no explainer bytes are staged".to_owned()))?;
    let bytes = crate::store::read_private_file_beneath(&snapshot.run_dir, &relative)
        .map_err(StoreError::Io)?;
    let digest = format!("{:x}", sha2::Sha256::digest(&bytes));
    if digest != staged.source_digest {
        return Err(RoleSessionError::Invalid(
            "staged explainer bytes do not match their recorded digest".to_owned(),
        ));
    }
    let contents = String::from_utf8(bytes).map_err(|_| {
        RoleSessionError::Invalid("staged explainer bytes are not valid UTF-8".to_owned())
    })?;
    Ok(StagedExplainerContents {
        obligation: snapshot
            .baton
            .publication_binding
            .as_ref()
            .map(|binding| binding.obligation.clone()),
        source_digest: staged.source_digest,
        path: staged.path,
        media_type: staged.media_type,
        byte_length: staged.byte_length,
        contents,
    })
}

/// Materialize the bytes behind an `analysis` checkpoint artifact, verifying the
/// digest, so a reviewer reads exactly what the manifest cites.
pub fn read_analysis(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    digest: &str,
) -> Result<StagedAnalysisContents, RoleSessionError> {
    let snapshot = read(run_dir, credentials_root, role, session_id)?;
    if !snapshot.baton.staged_analysis.iter().any(|d| d == digest) {
        return Err(RoleSessionError::Invalid(
            "that digest is not staged for this run".to_owned(),
        ));
    }
    let relative = crate::model::analysis_artifact_path(digest);
    let path = snapshot.run_dir.join(&relative);
    let bytes = crate::store::read_private_file_beneath(&snapshot.run_dir, &relative)
        .map_err(StoreError::Io)?;
    if format!("{:x}", sha2::Sha256::digest(&bytes)) != digest {
        return Err(RoleSessionError::Invalid(
            "staged analysis bytes do not match their recorded digest".to_owned(),
        ));
    }
    let contents = String::from_utf8(bytes.clone()).ok();
    Ok(StagedAnalysisContents {
        digest: digest.to_owned(),
        path,
        byte_length: bytes.len() as u64,
        contents,
    })
}

#[derive(Debug, Serialize)]
pub struct StagedAnalysisContents {
    pub digest: String,
    pub path: PathBuf,
    pub byte_length: u64,
    /// Absent when the staged bytes are not valid UTF-8; the path is still exact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StagedExplainerContents {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obligation: Option<crate::model::HandoffObligation>,
    pub source_digest: String,
    pub path: PathBuf,
    pub media_type: String,
    pub byte_length: u64,
    pub contents: String,
}

fn load_for_run(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    run_id: &str,
) -> Result<Credential, RoleSessionError> {
    let credential = credential::load(credentials_root, session_id, run_id, role)?;
    let credential_run = std::fs::canonicalize(&credential.run_dir).map_err(StoreError::Io)?;
    let requested_run = std::fs::canonicalize(run_dir).map_err(StoreError::Io)?;
    if credential_run != requested_run {
        return Err(CredentialError::Mismatch.into());
    }
    Ok(credential)
}

#[derive(Debug, Serialize)]
pub struct RoleClaimResult {
    pub revision: u64,
    pub epoch: u64,
    pub credential: PathBuf,
    #[serde(skip)]
    committed_baton: RunBaton,
}

pub fn claim(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
    expected_revision: u64,
) -> Result<RoleClaimResult, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let initial = channel.read()?;
    require_current_schema(&initial)?;
    let credential_lock =
        credential::lock_for_claim(credentials_root, session_id, &initial.run_id, role)?;
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    let canonical_run = std::fs::canonicalize(run_dir).map_err(StoreError::Io)?;
    credential_lock.prepare(&baton)?;
    // A recovery nonce lives only in this private root, written before the
    // claim exists. Its digest travels with the claim, so if the process dies
    // before the token is stored, only this root can replace the claim.
    let nonce = write_recovery_nonce(credentials_root, session_id, &baton.run_id, role)?;
    let grant = claim::claim_with_recovery(
        &channel,
        role,
        session_id,
        lease_seconds,
        expected_revision,
        Some(claim::digest(&nonce)),
    )?;
    let credential = Credential {
        run_dir: canonical_run,
        run_id: baton.run_id,
        role,
        session_id: session_id.to_owned(),
        epoch: grant.epoch,
        token: grant.token,
    };
    let credential = credential_lock.store(&credential)?;
    Ok(RoleClaimResult {
        revision: grant.revision,
        epoch: grant.epoch,
        credential,
        committed_baton: grant.committed_baton,
    })
}

pub fn reclaim(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
    expected_revision: u64,
) -> Result<RoleClaimResult, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let initial = channel.read()?;
    require_current_schema(&initial)?;
    let credential_lock =
        credential::lock_for_claim(credentials_root, session_id, &initial.run_id, role)?;
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    let canonical_run = std::fs::canonicalize(run_dir).map_err(StoreError::Io)?;
    credential_lock.prepare(&baton)?;
    let grant = claim::reclaim(&channel, role, session_id, lease_seconds, expected_revision)?;
    let credential = Credential {
        run_dir: canonical_run,
        run_id: baton.run_id,
        role,
        session_id: session_id.to_owned(),
        epoch: grant.epoch,
        token: grant.token,
    };
    let credential = credential_lock.store(&credential)?;
    Ok(RoleClaimResult {
        revision: grant.revision,
        epoch: grant.epoch,
        credential,
        committed_baton: grant.committed_baton,
    })
}

/// Swap an unreadable publication policy for the canonical channel both
/// harnesses can read, and reset the current obligation's receipts so the
/// publisher restages against the new channel. Never touches semantic scope.
pub fn repair_publication_policy(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    current_harness: &str,
    peer_harness: &str,
    expected_revision: u64,
) -> Result<RunBaton, RoleSessionError> {
    if session_id.trim().is_empty() {
        return Err(RoleSessionError::Invalid(
            "repair session id must not be blank".to_owned(),
        ));
    }
    let channel = RunChannel::open(run_dir);
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    if baton.revision != expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: expected_revision,
            actual: baton.revision,
        }
        .into());
    }
    if matches!(baton.status, Status::Done | Status::Abandoned) {
        return Err(ClaimError::Terminal.into());
    }
    // Scoped like `upgrade`, the other control-plane operation: a claim cannot
    // be required, because `start` refuses an unreadable policy before minting
    // one. Instead the caller must name the run's actual participant topology,
    // so a repair cannot be aimed at a run the caller is not part of.
    let requested = match role {
        Role::Worker => normalize_participants(current_harness.to_owned(), peer_harness.to_owned()),
        Role::Reviewer => {
            normalize_participants(peer_harness.to_owned(), current_harness.to_owned())
        }
    }
    .map_err(|_| RoleSessionError::Invalid("repair harnesses are invalid".to_owned()))?;
    let stored = normalize_participants(
        baton.participants.worker.harness.clone(),
        baton.participants.reviewer.harness.clone(),
    )
    .map_err(|_| RoleSessionError::Invalid("stored harnesses are invalid".to_owned()))?;
    if requested != stored {
        return Err(RoleSessionError::Invalid(
            "repair caller does not match the stored participant topology".to_owned(),
        ));
    }
    // Topology is only enough for the pre-claim path. If this role currently
    // holds a live claim, the caller must be that claim's session and prove it
    // with the private credential; otherwise any local process could repair a
    // run out from under the session that owns it.
    let participant = match role {
        Role::Worker => &baton.participants.worker,
        Role::Reviewer => &baton.participants.reviewer,
    };
    let live_claim = participant.claim.as_ref().is_some_and(|claim| {
        time::OffsetDateTime::parse(
            &claim.lease_expires_at,
            &time::format_description::well_known::Rfc3339,
        )
        .is_ok_and(|expiry| expiry > time::OffsetDateTime::now_utc())
    });
    if live_claim {
        let credential = load_for_run(run_dir, credentials_root, role, session_id, &baton.run_id)?;
        claim::verify(&baton, role, session_id, &credential.token)?;
    }
    let policy = baton
        .publication_policy
        .clone()
        .unwrap_or_else(crate::model::PublicationPolicy::fixed);
    if policy.reviewer_can_read() {
        return Err(RoleSessionError::Invalid(
            "publication policy is already reviewer-readable".to_owned(),
        ));
    }
    Ok(channel.repair_publication_policy(expected_revision)?)
}

fn reclaim_own(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
    expected_revision: u64,
) -> Result<RoleClaimResult, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let initial = channel.read()?;
    require_current_schema(&initial)?;
    let credential_lock =
        credential::lock_for_claim(credentials_root, session_id, &initial.run_id, role)?;
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    let canonical_run = std::fs::canonicalize(run_dir).map_err(StoreError::Io)?;
    credential_lock.prepare(&baton)?;
    let nonce = read_recovery_nonce(credentials_root, session_id, &baton.run_id, role)?;
    let grant = claim::reclaim_own(
        &channel,
        role,
        session_id,
        lease_seconds,
        expected_revision,
        &nonce,
    )?;
    let credential = Credential {
        run_dir: canonical_run,
        run_id: baton.run_id,
        role,
        session_id: session_id.to_owned(),
        epoch: grant.epoch,
        token: grant.token,
    };
    let credential = credential_lock.store(&credential)?;
    Ok(RoleClaimResult {
        revision: grant.revision,
        epoch: grant.epoch,
        credential,
        committed_baton: grant.committed_baton,
    })
}

fn recovery_nonce_path(
    credentials_root: &Path,
    session_id: &str,
    run_id: &str,
    role: Role,
) -> Result<PathBuf, RoleSessionError> {
    let credential = credential::path(credentials_root, session_id, run_id, role)?;
    let directory = credential
        .parent()
        .ok_or(CredentialError::UnsafeDirectory)?;
    Ok(directory.join(format!(
        ".{}.recovery",
        match role {
            Role::Worker => "worker",
            Role::Reviewer => "reviewer",
        }
    )))
}

fn write_recovery_nonce(
    credentials_root: &Path,
    session_id: &str,
    run_id: &str,
    role: Role,
) -> Result<String, RoleSessionError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = recovery_nonce_path(credentials_root, session_id, run_id, role)?;
    let nonce = Uuid::new_v4().to_string();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(StoreError::Io)?;
    file.write_all(nonce.as_bytes()).map_err(StoreError::Io)?;
    file.sync_all().map_err(StoreError::Io)?;
    Ok(nonce)
}

fn read_recovery_nonce(
    credentials_root: &Path,
    session_id: &str,
    run_id: &str,
    role: Role,
) -> Result<String, RoleSessionError> {
    let path = recovery_nonce_path(credentials_root, session_id, run_id, role)?;
    let nonce = crate::store::read_private_file(&path).map_err(|_| CredentialError::Missing)?;
    String::from_utf8(nonce).map_err(|_| CredentialError::Missing.into())
}

pub fn upgrade(
    run_dir: &Path,
    _credentials_root: &Path,
    role: Role,
    session_id: &str,
    current_harness: &str,
    peer_harness: &str,
    expected_revision: u64,
) -> Result<RunBaton, UpgradeError> {
    if session_id.trim().is_empty() {
        return Err(UpgradeError::InvalidSession);
    }
    let channel = RunChannel::open(run_dir);
    let baton = channel.read()?;
    if baton.revision != expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: expected_revision,
            actual: baton.revision,
        }
        .into());
    }
    if baton.schema != LEGACY_SCHEMA {
        return Err(UpgradeError::InvalidSchema);
    }
    if baton.objective.summary.trim().is_empty() {
        return Err(UpgradeError::InvalidObjective);
    }
    if matches!(baton.status, Status::Done | Status::Abandoned) {
        return Err(UpgradeError::Terminal);
    }

    let requested = match role {
        Role::Worker => normalize_participants(current_harness.to_owned(), peer_harness.to_owned()),
        Role::Reviewer => {
            normalize_participants(peer_harness.to_owned(), current_harness.to_owned())
        }
    }
    .map_err(|_| UpgradeError::InvalidTopology)?;
    let stored = normalize_participants(
        baton.participants.worker.harness.clone(),
        baton.participants.reviewer.harness.clone(),
    )
    .map_err(|_| UpgradeError::InvalidTopology)?;
    if requested != stored {
        return Err(UpgradeError::InvalidTopology);
    }
    channel
        .upgrade_legacy(expected_revision)
        .map_err(|error| match error {
            StoreError::TerminalState => UpgradeError::Terminal,
            StoreError::LegacyClaimLive => UpgradeError::Busy,
            StoreError::InvalidLeaseTimestamp => UpgradeError::InvalidTimestamp,
            other => UpgradeError::Store(other),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        action::{HumanDecisionRequest, ScopeAmendment},
        model::WorkspaceIdentity,
    };

    fn fixture_workspace(root: &Path) -> (PathBuf, WorkspaceIdentity) {
        let workspace = root.join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        for args in [
            &["init", "--quiet"][..],
            &[
                "remote",
                "add",
                "origin",
                "git@github.com:axatbhardwaj/Dvandva.git",
            ],
        ] {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&workspace)
                .args(args)
                .status()
                .unwrap()
                .success());
        }
        let identity = identity::identify(&workspace).unwrap();
        (workspace, identity)
    }

    fn fixture_scope() -> [DeliverableRequirement; 1] {
        [DeliverableRequirement {
            id: "implementation".to_owned(),
            description: "Original implementation".to_owned(),
        }]
    }

    fn fixture_baton(workspace: &WorkspaceIdentity, scope: &[DeliverableRequirement]) -> RunBaton {
        RunBaton::new(
            "run-a",
            "Original objective",
            "codex",
            "claude",
            scope.to_vec(),
        )
        .unwrap()
        .with_discovery_identity(
            workspace.clone(),
            TaskIdentity {
                reference: Some("DEF-123".to_owned()),
                summary: "Original objective".to_owned(),
            },
        )
    }

    fn fixture_request<'a>(
        workspace: &'a Path,
        runs: &'a Path,
        credentials: &'a Path,
        scope: &'a [DeliverableRequirement],
        run_id: Option<&'a str>,
        new_run: bool,
    ) -> RoleStartRequest<'a> {
        RoleStartRequest {
            workspace,
            runs_dir: runs,
            credentials_root: credentials,
            role: Role::Worker,
            session_id: "worker",
            current_harness: "codex",
            peer_harness: "claude",
            objective: Some("Original objective"),
            objective_refs: &[],
            task_reference: Some("DEF-123"),
            run_id,
            lease_seconds: 300,
            wait: false,
            poll_interval: Duration::from_millis(1),
            timeout: Duration::from_millis(1),
            new_run,
            required_deliverables: scope,
            interaction: crate::model::InteractionMode::Attended,
        }
    }

    fn amend_scope(
        channel: &RunChannel,
        credentials: &Path,
        session_id: &str,
        expected_revision: u64,
    ) -> String {
        let private = credential::load(credentials, session_id, "run-a", Role::Worker).unwrap();
        transition::apply(
            channel,
            Role::Worker,
            session_id,
            &private.token,
            expected_revision,
            Action::RequestHumanDecision(HumanDecisionRequest {
                kind: crate::model::HumanDecisionKind::Scope,
                question: "Use amended scope?".to_owned(),
                evidence: vec!["new requirement".to_owned()],
                options: vec!["yes".to_owned(), "no".to_owned()],
                proposals: Vec::new(),
            }),
        )
        .unwrap();
        transition::apply(
            channel,
            Role::Worker,
            session_id,
            &private.token,
            expected_revision + 1,
            Action::ResumeHumanDecision {
                answer: "yes".to_owned(),
                scope_amendment: Some(ScopeAmendment {
                    objective: "Amended objective".to_owned(),
                    objective_refs: Vec::new(),
                    task_reference: Some("DEF-123".to_owned()),
                    scope_deliverables: vec![DeliverableRequirement {
                        id: "implementation".to_owned(),
                        description: "Amended implementation".to_owned(),
                    }],
                }),
            },
        )
        .unwrap();
        private.token
    }

    #[test]
    fn new_run_creation_conflict_retries_worker_claim_on_the_same_run() {
        let root = tempfile::tempdir().unwrap();
        let (workspace, workspace_identity) = fixture_workspace(root.path());
        let runs = root.path().join("runs");
        let run_dir = runs.join("run-a");
        let credentials = root.path().join("credentials");
        let scope = fixture_scope();
        let baton = fixture_baton(&workspace_identity, &scope);
        RunChannel::open(&run_dir).create(&baton).unwrap();
        claim(&run_dir, &credentials, Role::Reviewer, "reviewer", 300, 0).unwrap();
        let request = fixture_request(&workspace, &runs, &credentials, &scope, None, true);

        let result = start_created_run(&run_dir, &baton, request, 8).unwrap();
        let RoleStartResult::Started(started) = result else {
            panic!("created run did not return started");
        };
        assert_eq!(started.disposition, "created");
        assert_eq!(started.snapshot.baton.run_id, "run-a");
        assert_eq!(started.snapshot.baton.revision, 2);
        assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 1);

        let installed = RunChannel::open(&run_dir).read().unwrap();
        let worker = credential::load(&credentials, "worker", "run-a", Role::Worker).unwrap();
        let reviewer = credential::load(&credentials, "reviewer", "run-a", Role::Reviewer).unwrap();
        claim::verify(&installed, Role::Worker, "worker", &worker.token).unwrap();
        claim::verify(&installed, Role::Reviewer, "reviewer", &reviewer.token).unwrap();
    }

    #[test]
    fn new_run_creation_retry_rejects_a_changed_canonical_scope() {
        let root = tempfile::tempdir().unwrap();
        let (workspace, workspace_identity) = fixture_workspace(root.path());
        let runs = root.path().join("runs");
        let run_dir = runs.join("run-a");
        let credentials = root.path().join("credentials");
        let scope = fixture_scope();
        let baton = fixture_baton(&workspace_identity, &scope);
        let channel = RunChannel::open(&run_dir);
        channel.create(&baton).unwrap();
        claim(&run_dir, &credentials, Role::Reviewer, "reviewer", 300, 0).unwrap();
        let amender = claim(&run_dir, &credentials, Role::Worker, "amender", 300, 1).unwrap();
        amend_scope(&channel, &credentials, "amender", 2);
        channel.recover(4).unwrap();
        let request = fixture_request(&workspace, &runs, &credentials, &scope, None, true);

        let result = start_created_run(&run_dir, &baton, request, 8);
        assert!(result.is_err(), "changed scope was silently adopted");
        let current = channel.read().unwrap();
        assert_eq!(current.revision, 5);
        assert_eq!(current.objective.summary, "Amended objective");
        assert!(current.participants.worker.claim.is_none());
        assert!(!credentials.join("worker/run-a/worker.json").exists());
        assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 1);
        assert_eq!(amender.revision, 2);
    }

    #[test]
    fn claimed_and_reclaimed_completion_keep_the_committed_scope() {
        for disposition in ["claimed", "reclaimed"] {
            let root = tempfile::tempdir().unwrap();
            let (workspace, workspace_identity) = fixture_workspace(root.path());
            let runs = root.path().join("runs");
            let run_dir = runs.join("run-a");
            let credentials = root.path().join("credentials");
            let scope = fixture_scope();
            let baton = fixture_baton(&workspace_identity, &scope);
            RunChannel::open(&run_dir).create(&baton).unwrap();
            let grant = claim(&run_dir, &credentials, Role::Worker, "worker", 300, 0).unwrap();
            let channel = RunChannel::open(&run_dir);
            let token = amend_scope(&channel, &credentials, "worker", 1);
            let request = fixture_request(
                &workspace,
                &runs,
                &credentials,
                &scope,
                Some("run-a"),
                false,
            );

            let first = finish_candidate_claim(
                disposition,
                &run_dir,
                &credentials,
                Role::Worker,
                "worker",
                grant,
            );
            let result = retry_start_conflict(first, request, 2).unwrap();
            let RoleStartResult::Started(started) = result else {
                panic!("{disposition} completion was reclassified after its committed claim");
            };
            assert_eq!(started.disposition, disposition);
            assert_eq!(started.snapshot.baton.revision, 1);
            assert_eq!(
                started.snapshot.baton.objective.summary,
                "Original objective"
            );

            let current = channel.read().unwrap();
            assert_eq!(current.revision, 3);
            assert_eq!(current.objective.summary, "Amended objective");
            claim::verify(&current, Role::Worker, "worker", &token).unwrap();
        }
    }

    #[test]
    fn created_start_accepts_an_immediate_peer_claim_without_recreating() {
        let root = tempfile::tempdir().unwrap();
        let runs = root.path().join("runs");
        let run_dir = runs.join("run-a");
        let credentials = root.path().join("credentials");
        let baton = RunBaton::new(
            "run-a",
            "Original objective",
            "codex",
            "claude",
            vec![DeliverableRequirement {
                id: "implementation".to_owned(),
                description: "Original implementation".to_owned(),
            }],
        )
        .unwrap();
        RunChannel::open(&run_dir).create(&baton).unwrap();
        let worker = claim(&run_dir, &credentials, Role::Worker, "worker", 300, 0).unwrap();
        claim(&run_dir, &credentials, Role::Reviewer, "reviewer", 300, 1).unwrap();

        let result =
            finish_created_start(&run_dir, &credentials, Role::Worker, "worker", worker).unwrap();
        let RoleStartResult::Started(started) = result else {
            panic!("created run did not return started");
        };
        assert_eq!(started.disposition, "created");
        assert_eq!(started.snapshot.baton.revision, 1);
        assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 1);
        let current = RunChannel::open(&run_dir).read().unwrap();
        assert_eq!(current.revision, 2);
        let worker = credential::load(&credentials, "worker", "run-a", Role::Worker).unwrap();
        let reviewer = credential::load(&credentials, "reviewer", "run-a", Role::Reviewer).unwrap();
        claim::verify(&current, Role::Worker, "worker", &worker.token).unwrap();
        claim::verify(&current, Role::Reviewer, "reviewer", &reviewer.token).unwrap();
    }

    #[test]
    fn created_start_keeps_the_worker_claim_scope_after_a_later_amendment() {
        let root = tempfile::tempdir().unwrap();
        let runs = root.path().join("runs");
        let run_dir = runs.join("run-a");
        let credentials = root.path().join("credentials");
        let scope = fixture_scope();
        let baton = RunBaton::new(
            "run-a",
            "Original objective",
            "codex",
            "claude",
            scope.to_vec(),
        )
        .unwrap();
        let channel = RunChannel::open(&run_dir);
        channel.create(&baton).unwrap();
        let worker = claim(&run_dir, &credentials, Role::Worker, "worker", 300, 0).unwrap();
        let token = amend_scope(&channel, &credentials, "worker", 1);

        let result =
            finish_created_start(&run_dir, &credentials, Role::Worker, "worker", worker).unwrap();
        let RoleStartResult::Started(started) = result else {
            panic!("created run did not return started");
        };
        assert_eq!(started.disposition, "created");
        assert_eq!(started.snapshot.baton.revision, 1);
        assert_eq!(
            started.snapshot.baton.objective.summary,
            "Original objective"
        );
        assert_eq!(
            started.peer_prompt.as_deref(),
            Some("Act as prativadi and join Dvandva run run-a.")
        );

        let current = channel.read().unwrap();
        assert_eq!(current.revision, 3);
        assert_eq!(current.objective.summary, "Amended objective");
        claim::verify(&current, Role::Worker, "worker", &token).unwrap();
        assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 1);
    }

    #[test]
    fn resumed_candidate_revision_drift_is_rediscovered_before_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        for args in [
            vec!["init", "--quiet"],
            vec![
                "remote",
                "add",
                "origin",
                "git@github.com:axatbhardwaj/Dvandva.git",
            ],
        ] {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&workspace)
                .args(args)
                .status()
                .unwrap()
                .success());
        }
        let workspace_identity = identity::identify(&workspace).unwrap();
        let runs = root.path().join("runs");
        let run_dir = runs.join("run-a");
        let credentials = root.path().join("credentials");
        let original_scope = [DeliverableRequirement {
            id: "implementation".to_owned(),
            description: "Original implementation".to_owned(),
        }];
        let baton = RunBaton::new(
            "run-a",
            "Original objective",
            "codex",
            "claude",
            original_scope.to_vec(),
        )
        .unwrap()
        .with_discovery_identity(
            WorkspaceIdentity {
                repository_id: workspace_identity.repository_id.clone(),
                origin: workspace_identity.origin.clone(),
                worktree: workspace_identity.worktree.clone(),
            },
            TaskIdentity {
                reference: Some("DEF-123".to_owned()),
                summary: "Original objective".to_owned(),
            },
        );
        RunChannel::open(&run_dir).create(&baton).unwrap();
        claim(&run_dir, &credentials, Role::Worker, "worker", 300, 0).unwrap();
        let mut discovered = discovery::discover(
            &runs,
            DiscoveryQuery {
                repository_id: &workspace_identity.repository_id,
                role: Role::Worker,
                participant_harness: "Codex",
                task_reference: Some("DEF-123"),
                objective: None,
                run_id: Some("run-a"),
                session_id: Some("worker"),
                stale_after_days: None,
            },
        )
        .unwrap();
        let candidate = discovered.candidates.remove(0);
        assert_eq!(candidate.revision, 1);

        let private = credential::load(&credentials, "worker", "run-a", Role::Worker).unwrap();
        let channel = RunChannel::open(&run_dir);
        transition::apply(
            &channel,
            Role::Worker,
            "worker",
            &private.token,
            1,
            Action::RequestHumanDecision(HumanDecisionRequest {
                kind: crate::model::HumanDecisionKind::Scope,
                question: "Use amended scope?".to_owned(),
                evidence: vec!["new requirement".to_owned()],
                options: vec!["yes".to_owned(), "no".to_owned()],
                proposals: Vec::new(),
            }),
        )
        .unwrap();
        transition::apply(
            &channel,
            Role::Worker,
            "worker",
            &private.token,
            2,
            Action::ResumeHumanDecision {
                answer: "yes".to_owned(),
                scope_amendment: Some(ScopeAmendment {
                    objective: "Amended objective".to_owned(),
                    objective_refs: Vec::new(),
                    task_reference: Some("DEF-123".to_owned()),
                    scope_deliverables: vec![DeliverableRequirement {
                        id: "implementation".to_owned(),
                        description: "Amended implementation".to_owned(),
                    }],
                }),
            },
        )
        .unwrap();

        let request = RoleStartRequest {
            workspace: &workspace,
            runs_dir: &runs,
            credentials_root: &credentials,
            role: Role::Worker,
            session_id: "worker",
            current_harness: "codex",
            peer_harness: "claude",
            objective: Some("Original objective"),
            objective_refs: &[],
            task_reference: Some("DEF-123"),
            run_id: Some("run-a"),
            lease_seconds: 300,
            wait: false,
            poll_interval: Duration::from_millis(1),
            timeout: Duration::from_millis(1),
            new_run: false,
            required_deliverables: &original_scope,
            interaction: crate::model::InteractionMode::Attended,
        };
        let first = start_candidate(candidate, &credentials, Role::Worker, "worker", 300);
        let result = retry_start_conflict(first, request, 2).unwrap();
        match result {
            RoleStartResult::Discovery(outcome) => {
                assert_eq!(outcome.outcome, DiscoveryKind::ScopeMismatch);
                assert_eq!(outcome.candidates[0].revision, 3);
            }
            RoleStartResult::Started(started) => panic!(
                "stale candidate returned resumed snapshot at revision {}",
                started.snapshot.baton.revision
            ),
            RoleStartResult::Upgrade(_) => panic!("unexpected upgrade"),
            RoleStartResult::PublicationUnreadable(_) => {
                panic!("unexpected publication preflight failure")
            }
        }
        assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 1);
    }
}
