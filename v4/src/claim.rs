use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{
    model::{Participant, ParticipantClaim, RunBaton, Status},
    store::{require_current_schema, RunChannel, StoreError},
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
    channel.mutate_locked(expected_revision, |baton, now| {
        reject_terminal(baton)?;
        if participant(baton, role).claim.is_some() {
            return Err(ClaimError::Active);
        }
        install_claim(baton, role, session_id, lease_seconds, 1, now)
    })
}

pub fn reclaim(
    channel: &RunChannel,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
    expected_revision: u64,
) -> Result<ClaimGrant, ClaimError> {
    validate_request(session_id, lease_seconds)?;
    channel.mutate_locked(expected_revision, |baton, now| {
        reject_terminal(baton)?;
        let previous = participant(baton, role)
            .claim
            .as_ref()
            .ok_or(ClaimError::Missing)?;
        if parse_timestamp(&previous.lease_expires_at)? > now {
            return Err(ClaimError::NotExpired);
        }
        let epoch = previous
            .epoch
            .checked_add(1)
            .ok_or(ClaimError::InvalidLease)?;
        install_claim(baton, role, session_id, lease_seconds, epoch, now)
    })
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
    channel.mutate_locked(expected_revision, |baton, now| {
        if matches!(baton.status, Status::Done | Status::Abandoned) {
            return Err(ClaimError::Terminal);
        }
        let claim = participant_mut(baton, role)
            .claim
            .as_mut()
            .ok_or(ClaimError::Missing)?;
        if claim.session_id != session_id || claim.token_digest != digest(token) {
            return Err(ClaimError::Fenced);
        }
        if parse_timestamp(&claim.lease_expires_at)? <= now {
            return Err(ClaimError::Fenced);
        }
        let (started, expires) = lease_times(now, lease_seconds)?;
        claim.lease_started_at = Some(started);
        claim.lease_expires_at = expires;
        claim.lease_seconds = lease_seconds;
        baton.revision += 1;
        Ok(baton.revision)
    })
}

pub fn verify(
    baton: &RunBaton,
    role: Role,
    session_id: &str,
    token: &str,
) -> Result<(), ClaimError> {
    verify_at(baton, role, session_id, token, OffsetDateTime::now_utc())
}

pub(crate) fn verify_at(
    baton: &RunBaton,
    role: Role,
    session_id: &str,
    token: &str,
    now: OffsetDateTime,
) -> Result<(), ClaimError> {
    require_current_schema(baton)?;
    let claim = participant(baton, role)
        .claim
        .as_ref()
        .ok_or(ClaimError::Missing)?;
    if claim.session_id != session_id
        || claim.token_digest != digest(token)
        || parse_timestamp(&claim.lease_expires_at)? <= now
    {
        return Err(ClaimError::Fenced);
    }
    Ok(())
}

pub fn renewal_lease(baton: &RunBaton, role: Role) -> Result<Option<u64>, ClaimError> {
    require_current_schema(baton)?;
    let claim = participant(baton, role)
        .claim
        .as_ref()
        .ok_or(ClaimError::Missing)?;
    let remaining = parse_timestamp(&claim.lease_expires_at)? - OffsetDateTime::now_utc();
    let threshold_millis = claim
        .lease_seconds
        .saturating_mul(1_000)
        .saturating_div(3)
        .clamp(1, i64::MAX as u64) as i64;
    let threshold = Duration::milliseconds(threshold_millis);
    Ok((remaining <= threshold).then_some(claim.lease_seconds))
}

fn install_claim(
    baton: &mut RunBaton,
    role: Role,
    session_id: &str,
    lease_seconds: u64,
    epoch: u64,
    now: OffsetDateTime,
) -> Result<ClaimGrant, ClaimError> {
    let token = Uuid::new_v4().to_string();
    let (started, expires) = lease_times(now, lease_seconds)?;
    participant_mut(baton, role).claim = Some(ParticipantClaim {
        session_id: session_id.to_owned(),
        epoch,
        token_digest: digest(&token),
        lease_started_at: Some(started),
        lease_expires_at: expires,
        lease_seconds,
    });
    baton.revision += 1;
    Ok(ClaimGrant {
        token,
        epoch,
        revision: baton.revision,
    })
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

fn lease_times(
    started_at: OffsetDateTime,
    lease_seconds: u64,
) -> Result<(String, String), ClaimError> {
    let expires_at = started_at
        .checked_add(Duration::seconds(lease_seconds as i64))
        .ok_or(ClaimError::InvalidLease)?;
    let started_at = started_at
        .format(&Rfc3339)
        .map_err(|_| ClaimError::InvalidTimestamp)?;
    let expires_at = expires_at
        .format(&Rfc3339)
        .map_err(|_| ClaimError::InvalidTimestamp)?;
    Ok((started_at, expires_at))
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
