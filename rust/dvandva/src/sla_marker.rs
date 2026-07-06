//! On-disk state for the baton-creation SLA marker (plan:
//! superpowers/plans/2026-07-06-clarifying-questions-phase-plan.html#p3,
//! scoping fix: 2026-07-06 baton-guard over-arming).
//!
//! The marker at `.dvandva/.session-baton-pending.vadi` is the SLA's arming
//! signal: it exists only after `dvandva resolve`/`preflight` returned
//! `CREATE` for the vadi — the one moment a session verifiably owes a baton.
//! The `baton-guard` hook only ever reads, stamps, or clears it; it never
//! creates one, so sessions that never engage the protocol are never armed.
//!
//! File format: first line the arming epoch (seconds); optional second line
//! the `session_id` of the session that owns the countdown (stamped by the
//! guard on first sight, since resolve cannot know the hook session id), or
//! the placeholder `-` when unknown; optional third line the last-warn
//! epoch, recorded when the guard emits a breach warning so re-warns can be
//! throttled. Older 1–2-line markers parse as never-warned.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The SLA is vadi-owned: preflight sends a batonless prativadi to
/// `wait --discover`, so only a vadi marker is ever armed.
pub fn marker_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".dvandva")
        .join(".session-baton-pending.vadi")
}

/// A parsed marker: the arming epoch, the owning session (if stamped), and
/// the last-warn epoch (once a breach warning has been emitted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub epoch: u64,
    pub session: Option<String>,
    pub last_warned: Option<u64>,
}

/// Read and parse the marker, if present and well-formed.
pub fn read(repo_root: &Path) -> Option<Marker> {
    let text = std::fs::read_to_string(marker_path(repo_root)).ok()?;
    let mut lines = text.lines();
    let epoch = lines.next()?.trim().parse().ok()?;
    let session = lines
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "-");
    let last_warned = lines.next().and_then(|s| s.trim().parse().ok());
    Some(Marker {
        epoch,
        session,
        last_warned,
    })
}

/// Arm the SLA by writing a fresh unstamped marker — but only if none
/// exists, so re-resolving can never reset a running countdown.
pub fn arm_if_absent(repo_root: &Path) {
    let path = marker_path(repo_root);
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, format!("{}\n", now_epoch()));
}

/// Stamp an unstamped marker with the session that owns the countdown,
/// preserving its arming epoch and any recorded last-warn epoch.
pub fn stamp(repo_root: &Path, marker: &Marker, session: &str) {
    let _ = std::fs::write(
        marker_path(repo_root),
        render(marker.epoch, Some(session), marker.last_warned),
    );
}

/// Record that a breach warning was emitted `now`, preserving the arming
/// epoch and session stamp so the throttle holds across guard invocations.
pub fn record_warn(repo_root: &Path, marker: &Marker, now: u64) {
    let _ = std::fs::write(
        marker_path(repo_root),
        render(marker.epoch, marker.session.as_deref(), Some(now)),
    );
}

/// Render the marker file body: epoch, session-or-`-`, optional last-warn.
fn render(epoch: u64, session: Option<&str>, last_warned: Option<u64>) -> String {
    let session_line = session.unwrap_or("-");
    match last_warned {
        Some(warned) => format!("{epoch}\n{session_line}\n{warned}\n"),
        None => format!("{epoch}\n{session_line}\n"),
    }
}

/// Remove the marker (SLA satisfied or marker dead).
pub fn clear(repo_root: &Path) {
    let _ = std::fs::remove_file(marker_path(repo_root));
}

/// Current unix time in seconds.
pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// The baton-creation SLA threshold in seconds:
/// `DVANDVA_BATON_SLA_SECONDS`, default 120.
pub fn threshold_secs() -> u64 {
    std::env::var("DVANDVA_BATON_SLA_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(120)
}

/// The `DVANDVA_SLA armed ...` stdout line for an armed marker, if one
/// exists — `resolve`/`preflight` print this on every invocation while the
/// SLA is armed so the countdown stays visible at each turn entry on any
/// engine.
pub fn deadline_line_if_armed(repo_root: &Path) -> Option<String> {
    let marker = read(repo_root)?;
    let threshold = threshold_secs();
    Some(crate::emit::dvandva_sla_armed(
        "vadi",
        marker.epoch.saturating_add(threshold),
        threshold,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_parses_all_marker_generations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join(".dvandva")).expect("mkdir");

        std::fs::write(marker_path(root), "100\n").expect("write");
        assert_eq!(
            read(root),
            Some(Marker {
                epoch: 100,
                session: None,
                last_warned: None
            })
        );

        std::fs::write(marker_path(root), "100\nsess-1\n").expect("write");
        assert_eq!(
            read(root),
            Some(Marker {
                epoch: 100,
                session: Some("sess-1".to_string()),
                last_warned: None
            })
        );

        std::fs::write(marker_path(root), "100\n-\n150\n").expect("write");
        assert_eq!(
            read(root),
            Some(Marker {
                epoch: 100,
                session: None,
                last_warned: Some(150)
            })
        );

        std::fs::write(marker_path(root), "100\nsess-1\n150\n").expect("write");
        assert_eq!(
            read(root),
            Some(Marker {
                epoch: 100,
                session: Some("sess-1".to_string()),
                last_warned: Some(150)
            })
        );
    }

    #[test]
    fn record_warn_and_stamp_preserve_each_other() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join(".dvandva")).expect("mkdir");
        std::fs::write(marker_path(root), "100\n").expect("write");

        let unwarned = read(root).expect("marker");
        record_warn(root, &unwarned, 150);
        let warned = read(root).expect("marker after warn");
        assert_eq!(warned.epoch, 100, "warn preserves the arming epoch");
        assert_eq!(warned.last_warned, Some(150));

        stamp(root, &warned, "sess-1");
        let stamped = read(root).expect("marker after stamp");
        assert_eq!(stamped.session.as_deref(), Some("sess-1"));
        assert_eq!(
            stamped.last_warned,
            Some(150),
            "stamping must preserve the last-warn epoch"
        );
        assert_eq!(stamped.epoch, 100);
    }

    #[test]
    fn read_rejects_garbage_and_missing_markers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        assert_eq!(read(root), None);

        std::fs::create_dir_all(root.join(".dvandva")).expect("mkdir");
        std::fs::write(marker_path(root), "not-a-number\n").expect("write");
        assert_eq!(read(root), None);
    }

    #[test]
    fn arm_if_absent_never_resets_an_existing_marker() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join(".dvandva")).expect("mkdir");
        std::fs::write(marker_path(root), "100\n").expect("write");

        arm_if_absent(root);
        assert_eq!(read(root).expect("marker").epoch, 100);
    }
}
