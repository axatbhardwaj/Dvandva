use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    action::Action,
    claim::{self, ClaimError, Role},
    credential::{self, Credential, CredentialError},
    discovery::{
        self, ClaimState, DiscoveryError, DiscoveryKind, DiscoveryOutcome, DiscoveryQuery,
    },
    identity::{self, IdentityError},
    model::{
        normalize_participants, DeliverableRequirement, ModelError, RunBaton, Status, TaskIdentity,
        LEGACY_SCHEMA,
    },
    store::{require_current_schema, RunChannel, StoreError},
    transition::{self, TransitionError},
    wait::{self, WaitError},
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
    Started(StartedRole),
    Discovery(DiscoveryOutcome),
    Upgrade(UpgradeRequiredRole),
}

#[derive(Debug, Serialize)]
pub struct UpgradeRequiredRole {
    pub outcome: &'static str,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub revision: u64,
    pub from_schema: &'static str,
    pub next_action: &'static str,
}

#[derive(Debug, Serialize)]
pub struct StartedRole {
    pub outcome: &'static str,
    pub disposition: &'static str,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub revision: u64,
    pub credential: PathBuf,
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
    pub objective: &'a str,
    pub task_reference: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub lease_seconds: u64,
    pub wait: bool,
    pub poll_interval: Duration,
    pub timeout: Duration,
    pub new_run: bool,
    pub required_deliverables: &'a [DeliverableRequirement],
}

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
        request.task_reference,
    )?;
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
        run_id: request.run_id,
        session_id: Some(request.session_id),
    };
    let mut outcome = discovery::discover(request.runs_dir, query)?;
    if request.new_run {
        if request.role != Role::Worker {
            return Err(RoleSessionError::Invalid(
                "only a worker may create a separate run".to_owned(),
            ));
        }
        if outcome.outcome != DiscoveryKind::Corrupt {
            outcome.outcome = DiscoveryKind::None;
            outcome.candidates.clear();
        }
    }
    if outcome.outcome == DiscoveryKind::None && request.role == Role::Reviewer && request.wait {
        outcome = discovery::wait_for_match(
            request.runs_dir,
            query,
            request.poll_interval,
            request.timeout,
            true,
        )?;
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
            }))
        }
        DiscoveryKind::None if request.role == Role::Worker => {
            std::fs::create_dir_all(request.runs_dir).map_err(StoreError::Io)?;
            let run_id = new_run_id(request.task_reference.unwrap_or(request.objective));
            let run_dir = request.runs_dir.join(&run_id);
            let (worker, reviewer) = match request.role {
                Role::Worker => (request.current_harness, request.peer_harness),
                Role::Reviewer => unreachable!(),
            };
            let task = TaskIdentity {
                reference: request.task_reference.map(|value| value.trim().to_owned()),
                summary: request.objective.trim().to_owned(),
            };
            let baton = RunBaton::new(
                &run_id,
                request.objective.trim(),
                worker,
                reviewer,
                request.required_deliverables.to_vec(),
            )?
            .with_discovery_identity(workspace_identity, task);
            RunChannel::open(&run_dir).create(&baton)?;
            let grant = claim(
                &run_dir,
                request.credentials_root,
                request.role,
                request.session_id,
                request.lease_seconds,
                0,
            );
            let grant = match grant {
                Ok(grant) => grant,
                Err(error) if is_revision_conflict(&error) && remaining_conflicts > 0 => {
                    return start_with_retries(request, remaining_conflicts - 1)
                }
                Err(error) => return Err(error),
            };
            Ok(RoleStartResult::Started(StartedRole {
                outcome: "started",
                disposition: "created",
                run_id,
                run_dir,
                revision: grant.revision,
                credential: grant.credential,
            }))
        }
        _ => Ok(RoleStartResult::Discovery(outcome)),
    }
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
        RoleSessionError::Claim(ClaimError::Store(StoreError::RevisionConflict { .. }))
    )
}

fn start_candidate(
    candidate: crate::discovery::RunCandidate,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
) -> Result<RoleStartResult, RoleSessionError> {
    let (disposition, revision, credential_path) = match candidate.claim_state {
        ClaimState::Unclaimed => {
            let grant = claim(
                &candidate.run_dir,
                credentials_root,
                role,
                session_id,
                lease_seconds,
                candidate.revision,
            )?;
            ("claimed", grant.revision, grant.credential)
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
            ("reclaimed", grant.revision, grant.credential)
        }
        ClaimState::Owned => {
            let baton = read(&candidate.run_dir, credentials_root, role, session_id)?;
            let path = credential::path(credentials_root, session_id, &candidate.run_id, role)?;
            ("resumed", baton.revision, path)
        }
        ClaimState::Busy => {
            return Err(RoleSessionError::Invalid(
                "selected run is owned by another live session".to_owned(),
            ));
        }
    };
    Ok(RoleStartResult::Started(StartedRole {
        outcome: "started",
        disposition,
        run_id: candidate.run_id,
        run_dir: candidate.run_dir,
        revision,
        credential: credential_path,
    }))
}

fn validate_start(
    session_id: &str,
    current_harness: &str,
    peer_harness: &str,
    objective: &str,
    task_reference: Option<&str>,
) -> Result<(), RoleSessionError> {
    if session_id.trim().is_empty()
        || current_harness.trim().is_empty()
        || peer_harness.trim().is_empty()
        || objective.trim().is_empty()
        || task_reference.is_some_and(|reference| reference.trim().is_empty())
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
) -> Result<crate::model::RunBaton, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    let credential = load_for_run(run_dir, credentials_root, role, session_id, &baton.run_id)?;
    claim::verify(&baton, role, session_id, &credential.token)?;
    Ok(baton)
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
) -> Result<crate::model::RunBaton, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    let credential = load_for_run(run_dir, credentials_root, role, session_id, &baton.run_id)?;
    Ok(wait::wait(
        &channel,
        role,
        session_id,
        &credential.token,
        after_revision,
        poll_interval,
        timeout,
    )?)
}

pub fn apply(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
    expected_revision: u64,
    action: Action,
) -> Result<crate::model::RunBaton, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let baton = channel.read()?;
    require_current_schema(&baton)?;
    let credential = load_for_run(run_dir, credentials_root, role, session_id, &baton.run_id)?;
    Ok(transition::apply(
        &channel,
        role,
        session_id,
        &credential.token,
        expected_revision,
        action,
    )?)
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
    let grant = claim::claim(&channel, role, session_id, lease_seconds, expected_revision)?;
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
    })
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
