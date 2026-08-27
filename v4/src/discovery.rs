use std::{
    ffi::OsStr,
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
    model::{Assignee, DeliverableRequirement, Objective, RunBaton, Status, LEGACY_SCHEMA, SCHEMA},
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
    pub run_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryKind {
    Match,
    UpgradeRequired,
    Busy,
    RunMissing,
    ScopeMismatch,
    None,
    Ambiguous,
    TaskMismatch,
    Corrupt,
}

#[derive(Debug, Serialize)]
pub struct RunCandidate {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub task_reference: Option<String>,
    pub task_summary: String,
    pub objective: Objective,
    pub scope_revision: u64,
    pub scope_deliverables: Vec<DeliverableRequirement>,
    pub status: Status,
    pub assignee: Assignee,
    pub revision: u64,
    pub claim_state: ClaimState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationMetadata>,
}

#[derive(Debug, Serialize)]
pub struct MigrationMetadata {
    pub from_schema: String,
    pub next_action: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimState {
    Unclaimed,
    Expired,
    Owned,
    Busy,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_scope: Option<RequestedScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct RequestedScope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objective_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objective_refs: Option<Vec<crate::model::ExternalRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_deliverables: Option<Vec<DeliverableRequirement>>,
}

pub fn discover(
    runs_dir: &Path,
    query: DiscoveryQuery<'_>,
) -> Result<DiscoveryOutcome, DiscoveryError> {
    if !runs_dir.exists() {
        let mut result = outcome(Vec::new(), Vec::new(), Vec::new(), Vec::new());
        if query.run_id.is_some() {
            result.outcome = DiscoveryKind::RunMissing;
        }
        return Ok(result);
    }

    let mut candidates = Vec::new();
    let mut task_mismatches = Vec::new();
    let mut upgrades = Vec::new();
    let mut corrupt = Vec::new();
    for entry in fs::read_dir(runs_dir)? {
        let entry = entry?;
        if query
            .run_id
            .is_some_and(|run_id| entry.file_name() != OsStr::new(run_id))
        {
            continue;
        }
        let run_dir = entry.path();
        if !entry.file_type()?.is_dir() {
            continue;
        }
        match RunChannel::open(&run_dir).read() {
            Ok(baton) => match candidate(&run_dir, baton, &query) {
                Ok(Some(CandidateMatch::Exact(candidate))) => candidates.push(candidate),
                Ok(Some(CandidateMatch::TaskMismatch(candidate))) => {
                    task_mismatches.push(candidate)
                }
                Ok(Some(CandidateMatch::Upgrade(candidate))) => upgrades.push(candidate),
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
    task_mismatches.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    upgrades.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    corrupt.sort_by(|left, right| left.run_dir.cmp(&right.run_dir));
    let mut result = outcome(candidates, task_mismatches, upgrades, corrupt);
    if query.run_id.is_some() && result.outcome == DiscoveryKind::None {
        result.outcome = DiscoveryKind::RunMissing;
    } else if query.run_id.is_some() && result.outcome == DiscoveryKind::Busy {
        result.next_action = Some("wait");
    }
    Ok(result)
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
) -> Result<Option<CandidateMatch>, String> {
    if !matches!(baton.schema.as_str(), SCHEMA | LEGACY_SCHEMA) {
        return Err("unsupported baton schema".to_owned());
    }
    if baton.schema == SCHEMA && matches!(baton.status, Status::Done | Status::Abandoned) {
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
    if query
        .run_id
        .is_some_and(|expected| baton.run_id != expected)
    {
        return Err("baton run id does not match its named directory".to_owned());
    }
    if workspace.repository_id != query.repository_id
        || !participant
            .harness
            .trim()
            .eq_ignore_ascii_case(query.participant_harness.trim())
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
            } else if query.role == Role::Worker || query.run_id.is_some() {
                ClaimState::Busy
            } else {
                return Ok(None);
            }
        }
    };
    let legacy = baton.schema == LEGACY_SCHEMA;
    let objective = baton.objective;
    let scope_deliverables = if legacy {
        vec![DeliverableRequirement {
            id: "legacy_objective".to_owned(),
            description: objective.summary.trim().to_owned(),
        }]
    } else {
        baton.scope_deliverables
    };
    let candidate = RunCandidate {
        run_id: baton.run_id,
        run_dir: run_dir.to_owned(),
        task_reference: task.reference.clone(),
        task_summary: task.summary.clone(),
        objective,
        scope_revision: baton.scope_revision,
        scope_deliverables,
        status: baton.status,
        assignee: baton.assignee,
        revision: baton.revision,
        claim_state,
        migration: legacy.then(|| MigrationMetadata {
            from_schema: LEGACY_SCHEMA.to_owned(),
            next_action: "upgrade_protocol",
        }),
    };
    let task_matches = query.task_reference.is_none_or(|expected| {
        task.reference
            .as_deref()
            .is_some_and(|actual| actual == expected.trim())
    });
    if baton.schema == LEGACY_SCHEMA && (query.run_id.is_some() || task_matches) {
        Ok(Some(CandidateMatch::Upgrade(candidate)))
    } else if query.run_id.is_none() && !task_matches {
        if candidate.claim_state == ClaimState::Busy {
            Ok(None)
        } else {
            Ok(Some(CandidateMatch::TaskMismatch(candidate)))
        }
    } else {
        Ok(Some(CandidateMatch::Exact(candidate)))
    }
}

enum CandidateMatch {
    Exact(RunCandidate),
    TaskMismatch(RunCandidate),
    Upgrade(RunCandidate),
}

fn outcome(
    candidates: Vec<RunCandidate>,
    task_mismatches: Vec<RunCandidate>,
    upgrades: Vec<RunCandidate>,
    corrupt: Vec<CorruptCandidate>,
) -> DiscoveryOutcome {
    let (outcome, candidates) = if !corrupt.is_empty() {
        (DiscoveryKind::Corrupt, candidates)
    } else if !upgrades.is_empty() {
        (DiscoveryKind::UpgradeRequired, upgrades)
    } else if candidates.is_empty() && !task_mismatches.is_empty() {
        (DiscoveryKind::TaskMismatch, task_mismatches)
    } else {
        match candidates.len() {
            0 => (DiscoveryKind::None, candidates),
            1 if candidates[0].claim_state == ClaimState::Busy => (DiscoveryKind::Busy, candidates),
            1 => (DiscoveryKind::Match, candidates),
            _ => (DiscoveryKind::Ambiguous, candidates),
        }
    };
    DiscoveryOutcome {
        outcome,
        candidates,
        corrupt,
        requested_scope: None,
        next_action: None,
    }
}
