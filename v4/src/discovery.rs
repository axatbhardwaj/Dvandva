use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    claim::Role,
    model::{RunBaton, Status, SCHEMA},
    store::RunChannel,
};

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("cannot scan runs: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct DiscoveryQuery<'a> {
    pub repository_id: &'a str,
    pub role: Role,
    pub participant_harness: &'a str,
    pub task_reference: Option<&'a str>,
    pub session_id: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryKind {
    Match,
    None,
    Ambiguous,
    Corrupt,
}

#[derive(Debug, Serialize)]
pub struct RunCandidate {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub task_reference: Option<String>,
    pub task_summary: String,
    pub status: Status,
    pub revision: u64,
    pub claim_state: ClaimState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimState {
    Unclaimed,
    Expired,
    Owned,
}

#[derive(Debug, Serialize)]
pub struct CorruptCandidate {
    pub run_dir: PathBuf,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryOutcome {
    pub outcome: DiscoveryKind,
    pub candidates: Vec<RunCandidate>,
    pub corrupt: Vec<CorruptCandidate>,
}

pub fn discover(
    runs_dir: &Path,
    query: DiscoveryQuery<'_>,
) -> Result<DiscoveryOutcome, DiscoveryError> {
    if !runs_dir.exists() {
        return Ok(outcome(Vec::new(), Vec::new()));
    }

    let mut candidates = Vec::new();
    let mut corrupt = Vec::new();
    for entry in fs::read_dir(runs_dir)? {
        let entry = entry?;
        let run_dir = entry.path();
        if !entry.file_type()?.is_dir() {
            continue;
        }
        match RunChannel::open(&run_dir).read() {
            Ok(baton) => match candidate(&run_dir, baton, &query) {
                Ok(Some(candidate)) => candidates.push(candidate),
                Ok(None) => {}
                Err(error) => corrupt.push(CorruptCandidate { run_dir, error }),
            },
            Err(crate::store::StoreError::RunMissing) => {}
            Err(error) => corrupt.push(CorruptCandidate {
                run_dir,
                error: error.to_string(),
            }),
        }
    }
    candidates.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    corrupt.sort_by(|left, right| left.run_dir.cmp(&right.run_dir));
    Ok(outcome(candidates, corrupt))
}

pub fn wait_for_match(
    runs_dir: &Path,
    query: DiscoveryQuery<'_>,
    poll_interval: Duration,
    timeout: Duration,
    use_notifications: bool,
) -> Result<DiscoveryOutcome, DiscoveryError> {
    let (sender, receiver) = mpsc::channel();
    let mut watcher = use_notifications
        .then(|| {
            notify::recommended_watcher(move |_| {
                let _ = sender.send(());
            })
        })
        .transpose()
        .ok()
        .flatten();
    if let Some(active) = watcher.as_mut() {
        let _ = active.watch(nearest_existing(runs_dir), RecursiveMode::Recursive);
    }

    let started = Instant::now();
    loop {
        let outcome = discover(runs_dir, query)?;
        if outcome.outcome != DiscoveryKind::None {
            return Ok(outcome);
        }
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            return Ok(outcome);
        }
        let remaining = timeout.saturating_sub(elapsed);
        let _ = receiver.recv_timeout(poll_interval.max(Duration::from_millis(1)).min(remaining));
    }
}

fn nearest_existing(path: &Path) -> &Path {
    let mut candidate = path;
    while !candidate.exists() {
        match candidate.parent() {
            Some(parent) => candidate = parent,
            None => return Path::new("/"),
        }
    }
    candidate
}

fn candidate(
    run_dir: &Path,
    baton: RunBaton,
    query: &DiscoveryQuery<'_>,
) -> Result<Option<RunCandidate>, String> {
    if baton.schema != SCHEMA {
        return Err("unsupported baton schema".to_owned());
    }
    if matches!(baton.status, Status::Done | Status::Abandoned) {
        return Ok(None);
    }
    let workspace = baton
        .workspace
        .as_ref()
        .ok_or_else(|| "baton has no workspace identity".to_owned())?;
    let task = baton
        .task
        .as_ref()
        .ok_or_else(|| "baton has no task identity".to_owned())?;
    let participant = match query.role {
        Role::Worker => &baton.participants.worker,
        Role::Reviewer => &baton.participants.reviewer,
    };
    if workspace.repository_id != query.repository_id
        || !participant
            .harness
            .eq_ignore_ascii_case(query.participant_harness)
        || query.task_reference.is_some_and(|expected| {
            !task
                .reference
                .as_deref()
                .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
        })
    {
        return Ok(None);
    }
    let claim_state = match participant.claim.as_ref() {
        None => ClaimState::Unclaimed,
        Some(claim) => {
            let expiry = OffsetDateTime::parse(&claim.lease_expires_at, &Rfc3339)
                .map_err(|_| "participant claim has an invalid expiry".to_owned())?;
            if expiry <= OffsetDateTime::now_utc() {
                ClaimState::Expired
            } else if query
                .session_id
                .is_some_and(|session_id| claim.session_id == session_id)
            {
                ClaimState::Owned
            } else {
                return Ok(None);
            }
        }
    };
    Ok(Some(RunCandidate {
        run_id: baton.run_id,
        run_dir: run_dir.to_owned(),
        task_reference: task.reference.clone(),
        task_summary: task.summary.clone(),
        status: baton.status,
        revision: baton.revision,
        claim_state,
    }))
}

fn outcome(candidates: Vec<RunCandidate>, corrupt: Vec<CorruptCandidate>) -> DiscoveryOutcome {
    let outcome = if !corrupt.is_empty() {
        DiscoveryKind::Corrupt
    } else {
        match candidates.len() {
            0 => DiscoveryKind::None,
            1 => DiscoveryKind::Match,
            _ => DiscoveryKind::Ambiguous,
        }
    };
    DiscoveryOutcome {
        outcome,
        candidates,
        corrupt,
    }
}
