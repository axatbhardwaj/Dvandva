use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use std::os::unix::fs::PermissionsExt;
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
    /// Canonical objective the caller asked for. A non-exact start must not
    /// adopt, or collide with, a run pursuing a different objective.
    pub objective: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
    /// Runs with no live claim whose head has not moved for this many days are
    /// abandoned rather than merely idle, and are excluded from non-exact
    /// discovery. `None` disables the horizon.
    pub stale_after_days: Option<u64>,
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
    /// Unclaimed runs older than the staleness horizon, excluded from matching
    /// but reported so they can be garbage-collected rather than silently kept.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<StaleCandidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_scope: Option<RequestedScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct StaleCandidate {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub schema: String,
    pub status: Status,
    pub idle_days: u64,
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
    let mut stale = Vec::new();
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
            if query.run_id.is_some() {
                corrupt.push(CorruptCandidate {
                    run_dir,
                    error: "named run entry is not a directory".to_owned(),
                });
            }
            continue;
        }
        match RunChannel::open(&run_dir).read() {
            Ok(baton) => {
                if query.run_id.is_none() {
                    if let Some(idle_days) = stale_idle_days(&run_dir, &baton, &query) {
                        stale.push(StaleCandidate {
                            run_id: baton.run_id,
                            run_dir,
                            schema: baton.schema,
                            status: baton.status,
                            idle_days,
                        });
                        continue;
                    }
                }
                match candidate(&run_dir, baton, &query) {
                    Ok(Some(CandidateMatch::Exact(candidate))) => candidates.push(candidate),
                    Ok(Some(CandidateMatch::TaskMismatch(candidate))) => {
                        task_mismatches.push(candidate)
                    }
                    Ok(Some(CandidateMatch::Upgrade(candidate))) => upgrades.push(candidate),
                    Ok(None) => {}
                    Err(error) => corrupt.push(CorruptCandidate { run_dir, error }),
                }
            }
            Err(crate::store::StoreError::RunMissing) => {
                if query.run_id.is_some() {
                    corrupt.push(CorruptCandidate {
                        run_dir,
                        error: "named run directory has no Baton head".to_owned(),
                    });
                }
            }
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
    stale.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    let mut result = outcome(candidates, task_mismatches, upgrades, corrupt);
    result.stale = stale;
    if query.run_id.is_some() && result.outcome == DiscoveryKind::None {
        result.outcome = DiscoveryKind::RunMissing;
    } else if query.run_id.is_some() && result.outcome == DiscoveryKind::Busy {
        result.next_action = Some("wait");
    }
    Ok(result)
}

/// Every unclaimed, non-terminal run idle for at least `older_than_days`,
/// regardless of repository, role, or objective.
pub fn stale_runs(
    runs_dir: &Path,
    older_than_days: u64,
) -> Result<Vec<StaleCandidate>, DiscoveryError> {
    if !runs_dir.exists() {
        return Ok(Vec::new());
    }
    let mut stale = Vec::new();
    for entry in fs::read_dir(runs_dir)? {
        let entry = entry?;
        let run_dir = entry.path();
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Ok(baton) = RunChannel::open(&run_dir).read() else {
            continue;
        };
        if let Some(idle_days) = collectable_idle_days(&run_dir, &baton, older_than_days) {
            stale.push(StaleCandidate {
                run_id: baton.run_id,
                run_dir,
                schema: baton.schema,
                status: baton.status,
                idle_days,
            });
        }
    }
    stale.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    Ok(stale)
}

/// Move one stale run aside, revalidating it first. Archiving is destructive to
/// discovery, so it must not act on a run that came back to life between the
/// scan and the move, must not follow a symlinked archive root, and must not
/// take its destination name from Baton content.
pub fn archive_stale_run(
    runs_dir: &Path,
    run_dir: &Path,
    older_than_days: u64,
) -> Result<Option<PathBuf>, DiscoveryError> {
    // The destination name comes from the filesystem, not the Baton: a run_id
    // read out of run state could contain separators or `..`.
    let name = match run_dir.file_name() {
        Some(name) if !name.is_empty() && name != OsStr::new(".") && name != OsStr::new("..") => {
            name.to_owned()
        }
        _ => return Ok(None),
    };
    let name: &OsStr = &name;
    if Path::new(&name).components().count() != 1 {
        return Ok(None);
    }

    // The archive root is opened once and pinned, so a symlink swapped in after
    // the check cannot redirect the move.
    let archive_root = runs_dir.join(".archived");
    match fs::symlink_metadata(&archive_root) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => return Ok(None),
        Ok(_) => {}
        Err(_) => {
            fs::create_dir_all(&archive_root)?;
            fs::set_permissions(&archive_root, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    // Both directories are opened no-follow and proven to be directories on the
    // open descriptor itself, so a symlink swapped in between the metadata
    // check and the open cannot redirect the move.
    let archive_fd = match crate::store::open_dir_nofollow(&archive_root) {
        Ok(directory) => directory,
        Err(_) => return Ok(None),
    };
    let source_parent = match crate::store::open_dir_nofollow(runs_dir) {
        Ok(directory) => directory,
        Err(_) => return Ok(None),
    };

    // Revalidate and move while holding the run's own lock, so a session that
    // reclaimed it between the scan and now keeps it.
    let channel = RunChannel::open(run_dir);
    let archived = channel.with_run_lock(|| {
        let Ok(baton) = channel.read() else {
            return Ok(None);
        };
        if collectable_idle_days(run_dir, &baton, older_than_days).is_none() {
            return Ok(None);
        }
        Ok(rename_no_replace(&source_parent, name, &archive_fd, name))
    });
    match archived {
        Ok(Some(())) => Ok(Some(archive_root.join(name))),
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}

/// Move `name` from one pinned directory to another, failing rather than
/// replacing an existing entry. Both ends are named relative to open directory
/// descriptors, so neither can be redirected by a swapped symlink.
fn rename_no_replace(
    source_dir: &fs::File,
    source_name: &OsStr,
    destination_dir: &fs::File,
    destination_name: &OsStr,
) -> Option<()> {
    use std::os::unix::ffi::OsStrExt;
    let source = std::ffi::CString::new(source_name.as_bytes()).ok()?;
    let destination = std::ffi::CString::new(destination_name.as_bytes()).ok()?;
    let moved = unsafe {
        libc::renameat2(
            std::os::fd::AsRawFd::as_raw_fd(source_dir),
            source.as_ptr(),
            std::os::fd::AsRawFd::as_raw_fd(destination_dir),
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    (moved == 0).then_some(())
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
    if query
        .run_id
        .is_some_and(|expected| baton.run_id != expected)
    {
        return Err("baton run id does not match its named directory".to_owned());
    }
    // A finished run is finished whatever schema recorded it. A terminal v1
    // sibling must not be offered for upgrade to an unrelated new start.
    let terminal = matches!(baton.status, Status::Done | Status::Abandoned);
    if terminal && query.run_id.is_none() {
        return Ok(None);
    }
    let workspace = baton
        .workspace
        .as_ref()
        .ok_or_else(|| "baton has no workspace identity".to_owned())?;
    let task = baton.task.as_ref();
    let participant = match query.role {
        Role::Worker => &baton.participants.worker,
        Role::Reviewer => &baton.participants.reviewer,
    };
    if workspace.repository_id != query.repository_id
        || !participant
            .harness
            .trim()
            .eq_ignore_ascii_case(query.participant_harness.trim())
    {
        return Ok(None);
    }
    let claim_state = if terminal {
        ClaimState::Unclaimed
    } else {
        match participant.claim.as_ref() {
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
        }
    };
    let legacy = baton.schema == LEGACY_SCHEMA;
    let task_reference = task.and_then(|identity| identity.reference.clone());
    let task_summary = task
        .map(|identity| identity.summary.clone())
        .unwrap_or_else(|| baton.objective.summary.clone());
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
        task_reference,
        task_summary,
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
    let task_reference_matches = query.task_reference.is_some_and(|expected| {
        task.and_then(|identity| identity.reference.as_deref())
            .is_some_and(|actual| actual == expected.trim())
    });
    let objective_matches = query
        .objective
        .is_none_or(|expected| candidate.objective.summary == expected.trim());
    // A run pursuing a different objective, which the caller has not pointed at
    // by task reference, is not this start's run at all. It is unrelated, not a
    // near miss, so it must not surface as a mismatch and block a new run. When
    // the caller does name this run's task, a differing objective is a real
    // scope disagreement and must still be surfaced.
    if query.run_id.is_none() && !objective_matches && !task_reference_matches {
        return Ok(None);
    }
    let task_matches = query.task_reference.is_none_or(|expected| {
        task.and_then(|identity| identity.reference.as_deref())
            .is_some_and(|actual| actual == expected.trim())
    });
    if terminal {
        // Finished runs are never offered for migration: `upgrade` would refuse
        // them as terminal, so advertising `upgrade_protocol` for one sends the
        // caller to an action that cannot succeed.
        Ok(Some(CandidateMatch::Exact(candidate)))
    } else if baton.schema == LEGACY_SCHEMA && (query.run_id.is_some() || task_matches) {
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
        let mut plausible = candidates;
        plausible.extend(upgrades);
        plausible.sort_by(|left, right| left.run_id.cmp(&right.run_id));
        if plausible.len() == 1 {
            (DiscoveryKind::UpgradeRequired, plausible)
        } else {
            (DiscoveryKind::Ambiguous, plausible)
        }
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
        stale: Vec::new(),
        requested_scope: None,
        next_action: None,
    }
}

/// How long a run has sat with no live claim and no head movement, when that
/// exceeds the caller's horizon.
fn stale_idle_days(run_dir: &Path, baton: &RunBaton, query: &DiscoveryQuery<'_>) -> Option<u64> {
    idle_days_without_live_claim(run_dir, baton, query.stale_after_days?)
}

/// Like `idle_days_without_live_claim`, but a terminal run counts too: it is
/// finished, so an explicit collection may archive it once it has gone quiet.
fn collectable_idle_days(run_dir: &Path, baton: &RunBaton, horizon: u64) -> Option<u64> {
    idle_days(run_dir, baton, horizon)
}

fn idle_days_without_live_claim(run_dir: &Path, baton: &RunBaton, horizon: u64) -> Option<u64> {
    if matches!(baton.status, Status::Done | Status::Abandoned) {
        return None;
    }
    idle_days(run_dir, baton, horizon)
}

fn idle_days(run_dir: &Path, baton: &RunBaton, horizon: u64) -> Option<u64> {
    let live_claim = [
        baton.participants.worker.claim.as_ref(),
        baton.participants.reviewer.claim.as_ref(),
    ]
    .into_iter()
    .flatten()
    .any(|claim| {
        OffsetDateTime::parse(&claim.lease_expires_at, &Rfc3339)
            .is_ok_and(|expiry| expiry > OffsetDateTime::now_utc())
    });
    if live_claim {
        return None;
    }
    let modified = std::fs::metadata(run_dir.join("baton.json"))
        .and_then(|metadata| metadata.modified())
        .ok()?;
    let idle_days = modified.elapsed().ok()?.as_secs() / 86_400;
    (idle_days >= horizon).then_some(idle_days)
}
