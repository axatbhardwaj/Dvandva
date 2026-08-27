use std::{sync::mpsc, time::Duration};

use notify::{RecursiveMode, Watcher};
use thiserror::Error;

use crate::{
    claim::{self, ClaimError, Role},
    model::{RunBaton, Status},
    next_action,
    store::{require_current_schema, RunChannel, StoreError},
};

#[derive(Debug, Error)]
pub enum WaitError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Claim(#[from] ClaimError),
    #[error("wait timed out")]
    Timeout,
}

pub fn wait(
    channel: &RunChannel,
    role: Role,
    session_id: &str,
    token: &str,
    after_revision: u64,
    poll_interval: Duration,
    timeout: Duration,
) -> Result<RunBaton, WaitError> {
    let (sender, receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |_| {
        let _ = sender.send(());
    })
    .ok();
    let watch_failed = if let Some(active_watcher) = watcher.as_mut() {
        active_watcher
            .watch(channel.directory(), RecursiveMode::NonRecursive)
            .is_err()
    } else {
        false
    };
    if watch_failed {
        watcher.take();
    }

    let deadline = std::time::Instant::now() + timeout;
    let mut seen_revision = after_revision;
    loop {
        let baton = channel.read()?;
        require_current_schema(&baton)?;
        if matches!(baton.status, Status::Done | Status::Abandoned) {
            return Ok(baton);
        }
        claim::verify(&baton, role, session_id, token)?;
        let harness = match role {
            Role::Worker => &baton.participants.worker.harness,
            Role::Reviewer => &baton.participants.reviewer.harness,
        };
        if next_action::classify(&baton, role, harness).actionable {
            return Ok(baton);
        }
        if let Some(lease_seconds) = claim::renewal_lease(&baton, role)? {
            match claim::heartbeat(
                channel,
                role,
                session_id,
                token,
                lease_seconds,
                baton.revision,
            ) {
                Ok(revision) => seen_revision = revision,
                Err(ClaimError::Store(StoreError::RevisionConflict { .. })) => continue,
                Err(error) => return Err(error.into()),
            }
            continue;
        }
        if baton.revision > seen_revision {
            seen_revision = baton.revision;
        }

        let now = std::time::Instant::now();
        if now >= deadline {
            return Err(WaitError::Timeout);
        }
        let remaining = deadline.saturating_duration_since(now);
        let interval = poll_interval.min(remaining);
        let _ = receiver.recv_timeout(interval);
    }
}
