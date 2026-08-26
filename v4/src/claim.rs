use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{
    model::{Participant, ParticipantClaim, RunBaton, Status},
    store::{RunChannel, StoreError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Worker,
    Reviewer,
}

#[derive(Debug, Serialize)]
pub struct ClaimGrant {
    pub token: String,
    pub epoch: u64,
    pub revision: u64,
}

#[derive(Debug, Error)]
pub enum ClaimError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("role already has a live claim")]
    Active,
    #[error("role claim has not expired")]
    NotExpired,
    #[error("role is unclaimed")]
    Missing,
    #[error("claim is fenced by a different session or token")]
    Fenced,
    #[error("lease seconds must be greater than zero")]
    InvalidLease,
    #[error("session id must not be blank")]
    InvalidSession,
    #[error("terminal runs cannot renew claims")]
    Terminal,
    #[error("invalid stored lease timestamp")]
    InvalidTimestamp,
}

pub fn claim(
    channel: &RunChannel,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
    expected_revision: u64,
) -> Result<ClaimGrant, ClaimError> {
    validate_request(session_id, lease_seconds)?;
    let mut baton = read_expected(channel, expected_revision)?;
    reject_terminal(&baton)?;
    if participant(&baton, role).claim.is_some() {
        return Err(ClaimError::Active);
    }
    install_claim(
        channel,
        &mut baton,
        role,
        session_id,
        lease_seconds,
        expected_revision,
        1,
    )
}

pub fn reclaim(
    channel: &RunChannel,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
    expected_revision: u64,
) -> Result<ClaimGrant, ClaimError> {
    validate_request(session_id, lease_seconds)?;
    let mut baton = read_expected(channel, expected_revision)?;
    reject_terminal(&baton)?;
    let previous = participant(&baton, role)
        .claim
        .as_ref()
        .ok_or(ClaimError::Missing)?;
    if parse_timestamp(&previous.lease_expires_at)? > OffsetDateTime::now_utc() {
        return Err(ClaimError::NotExpired);
    }
    let epoch = previous.epoch + 1;
    install_claim(
        channel,
        &mut baton,
        role,
        session_id,
        lease_seconds,
        expected_revision,
        epoch,
    )
}

pub fn heartbeat(
    channel: &RunChannel,
    role: Role,
    session_id: &str,
    token: &str,
    lease_seconds: u64,
    expected_revision: u64,
) -> Result<u64, ClaimError> {
    validate_request(session_id, lease_seconds)?;
    let mut baton = read_expected(channel, expected_revision)?;
    if matches!(baton.status, Status::Done | Status::Abandoned) {
        return Err(ClaimError::Terminal);
    }
    let claim = participant_mut(&mut baton, role)
        .claim
        .as_mut()
        .ok_or(ClaimError::Missing)?;
    if claim.session_id != session_id || claim.token_digest != digest(token) {
        return Err(ClaimError::Fenced);
    }
    claim.lease_expires_at = expiry(lease_seconds)?;
    claim.lease_seconds = lease_seconds;
    baton.revision += 1;
    channel.compare_and_swap(expected_revision, &baton)?;
    Ok(baton.revision)
}

pub fn verify(
    baton: &RunBaton,
    role: Role,
    session_id: &str,
    token: &str,
) -> Result<(), ClaimError> {
    let claim = participant(baton, role)
        .claim
        .as_ref()
        .ok_or(ClaimError::Missing)?;
    if claim.session_id != session_id
        || claim.token_digest != digest(token)
        || parse_timestamp(&claim.lease_expires_at)? <= OffsetDateTime::now_utc()
    {
        return Err(ClaimError::Fenced);
    }
    Ok(())
}

pub fn renewal_lease(baton: &RunBaton, role: Role) -> Result<Option<u64>, ClaimError> {
    let claim = participant(baton, role)
        .claim
        .as_ref()
        .ok_or(ClaimError::Missing)?;
    let remaining = parse_timestamp(&claim.lease_expires_at)? - OffsetDateTime::now_utc();
    let threshold = Duration::seconds((claim.lease_seconds / 3).max(1) as i64);
    Ok((remaining <= threshold).then_some(claim.lease_seconds))
}

fn install_claim(
    channel: &RunChannel,
    baton: &mut RunBaton,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
    expected_revision: u64,
    epoch: u64,
) -> Result<ClaimGrant, ClaimError> {
    let token = Uuid::new_v4().to_string();
    participant_mut(baton, role).claim = Some(ParticipantClaim {
        session_id: session_id.to_owned(),
        epoch,
        token_digest: digest(&token),
        lease_expires_at: expiry(lease_seconds)?,
        lease_seconds,
    });
    baton.revision += 1;
    channel.compare_and_swap(expected_revision, baton)?;
    Ok(ClaimGrant {
        token,
        epoch,
        revision: baton.revision,
    })
}

fn read_expected(channel: &RunChannel, expected: u64) -> Result<RunBaton, ClaimError> {
    let baton = channel.read()?;
    if baton.revision != expected {
        return Err(StoreError::RevisionConflict {
            expected,
            actual: baton.revision,
        }
        .into());
    }
    Ok(baton)
}

fn participant(baton: &RunBaton, role: Role) -> &Participant {
    match role {
        Role::Worker => &baton.participants.worker,
        Role::Reviewer => &baton.participants.reviewer,
    }
}

fn participant_mut(baton: &mut RunBaton, role: Role) -> &mut Participant {
    match role {
        Role::Worker => &mut baton.participants.worker,
        Role::Reviewer => &mut baton.participants.reviewer,
    }
}

fn validate_request(session_id: &str, lease_seconds: u64) -> Result<(), ClaimError> {
    if session_id.trim().is_empty() {
        return Err(ClaimError::InvalidSession);
    }
    if lease_seconds == 0 || lease_seconds > i64::MAX as u64 {
        return Err(ClaimError::InvalidLease);
    }
    Ok(())
}

fn reject_terminal(baton: &RunBaton) -> Result<(), ClaimError> {
    if matches!(baton.status, Status::Done | Status::Abandoned) {
        return Err(ClaimError::Terminal);
    }
    Ok(())
}

fn expiry(lease_seconds: u64) -> Result<String, ClaimError> {
    (OffsetDateTime::now_utc() + Duration::seconds(lease_seconds as i64))
        .format(&Rfc3339)
        .map_err(|_| ClaimError::InvalidTimestamp)
}

fn parse_timestamp(value: &str) -> Result<OffsetDateTime, ClaimError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|_| ClaimError::InvalidTimestamp)
}

fn digest(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
