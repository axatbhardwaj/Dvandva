use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Serialize;
use thiserror::Error;

use crate::{
    action::Action,
    claim::{self, ClaimError, Role},
    credential::{self, Credential, CredentialError},
    store::{RunChannel, StoreError},
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
}

pub fn read(
    run_dir: &Path,
    credentials_root: &Path,
    role: Role,
    session_id: &str,
) -> Result<crate::model::RunBaton, RoleSessionError> {
    let channel = RunChannel::open(run_dir);
    let baton = channel.read()?;
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
    let baton = channel.read()?;
    let canonical_run = std::fs::canonicalize(run_dir).map_err(StoreError::Io)?;
    credential::prepare(credentials_root, session_id, &baton.run_id, role)?;
    let grant = claim::claim(&channel, role, session_id, lease_seconds, expected_revision)?;
    let credential = Credential {
        run_dir: canonical_run,
        run_id: baton.run_id,
        role,
        session_id: session_id.to_owned(),
        epoch: grant.epoch,
        token: grant.token,
    };
    let credential = credential::store(credentials_root, &credential)?;
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
    let baton = channel.read()?;
    let canonical_run = std::fs::canonicalize(run_dir).map_err(StoreError::Io)?;
    credential::prepare(credentials_root, session_id, &baton.run_id, role)?;
    let grant = claim::reclaim(&channel, role, session_id, lease_seconds, expected_revision)?;
    let credential = Credential {
        run_dir: canonical_run,
        run_id: baton.run_id,
        role,
        session_id: session_id.to_owned(),
        epoch: grant.epoch,
        token: grant.token,
    };
    let credential = credential::store(credentials_root, &credential)?;
    Ok(RoleClaimResult {
        revision: grant.revision,
        epoch: grant.epoch,
        credential,
    })
}
