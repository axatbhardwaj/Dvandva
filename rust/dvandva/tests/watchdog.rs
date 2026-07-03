//! Integration tests for `dvandva watchdog` (task WD).
//!
//! Each case spawns the real `dvandva` binary against a fixture `.dvandva`
//! tree written into a tempdir. Timestamps are built relative to "now" via
//! [`timestamp_minus`] so staleness assertions do not depend on the
//! wall-clock date, and thresholds are kept small (seconds) so the suite
//! stays fast.

use std::path::{Path, PathBuf};
use std::process::Command;

use time::OffsetDateTime;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

/// Spawn `dvandva watchdog <args>` in `cwd` with `envs` set. `watchdog` is
/// one-shot (never blocks), so this is a plain blocking `output()` call.
fn run_watchdog(cwd: Option<&Path>, envs: &[(&str, &str)], args: &[&str]) -> (i32, String) {
    let mut cmd = Command::new(bin());
    cmd.arg("watchdog").args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("spawn dvandva watchdog");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), text)
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn mkparent(file: &Path) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
}

fn write_baton(
    file: &Path,
    run_id: &str,
    status: &str,
    assignee: &str,
    checkpoint: u64,
    updated_at: &str,
) {
    mkparent(file);
    std::fs::write(
        file,
        format!(
            r#"{{
  "schema": "dvandva.baton.v2",
  "run_id": "{run_id}",
  "status": "{status}",
  "assignee": "{assignee}",
  "checkpoint": {checkpoint},
  "updated_at": "{updated_at}"
}}"#
        ),
    )
    .unwrap();
}

/// A "now minus `secs`" RFC3339 second-precision Zulu timestamp, built by
/// hand (matching the shape `wait.rs`'s `parse_rfc3339` accepts) so tests
/// stay independent of any date-parsing feature.
fn timestamp_minus(secs: i64) -> String {
    let ts = OffsetDateTime::now_utc() - time::Duration::seconds(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        ts.year(),
        ts.month() as u8,
        ts.day(),
        ts.hour(),
        ts.minute(),
        ts.second()
    )
}

fn now_rfc3339() -> String {
    timestamp_minus(0)
}

// ── classification ────────────────────────────────────────────────────────

#[test]
fn wd_stale_mid_work_emits_event() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "implementing",
        "vadi",
        5,
        &timestamp_minus(100),
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &["--stale-max", "10"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(
            "DVANDVA_WATCHDOG watchdog_stale run_id=alpha status=implementing assignee=vadi checkpoint=5"
        ),
        "{out}"
    );
    assert!(out.contains("stale=1"), "{out}");
}

#[test]
fn wd_fresh_mid_work_emits_nothing() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "implementing",
        "vadi",
        5,
        &now_rfc3339(),
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &["--stale-max", "1800"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("watchdog_stale"), "{out}");
    assert!(out.contains("stale=0"), "{out}");
}

#[test]
fn wd_paused_with_remind_paused_emits_watchdog_paused() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "human_question",
        "human",
        5,
        &timestamp_minus(50),
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &["--remind-paused", "10"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(
            "DVANDVA_WATCHDOG watchdog_paused run_id=alpha status=human_question assignee=human checkpoint=5"
        ),
        "{out}"
    );
    assert!(out.contains("paused=1"), "{out}");
}

#[test]
fn wd_paused_without_remind_flag_emits_nothing() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "human_decision",
        "human",
        5,
        &timestamp_minus(50),
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &[]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("watchdog_paused"), "{out}");
    assert!(!out.contains("watchdog_stale"), "{out}");
    // Still classified (and counted) as paused, just not reminder-emitted.
    assert!(out.contains("paused=1"), "{out}");
}

#[test]
fn wd_terminal_status_emits_nothing() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "done",
        "team",
        5,
        &timestamp_minus(1_000_000),
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &["--stale-max", "1"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("watchdog_stale"), "{out}");
    assert!(!out.contains("watchdog_paused"), "{out}");
    assert!(out.contains("batons=1"), "{out}");
    assert!(out.contains("stale=0"), "{out}");
    assert!(out.contains("paused=0"), "{out}");
}

#[test]
fn wd_garbage_baton_counts_skipped_without_crash() {
    let d = tmp();
    let garbage = d.path().join(".dvandva/runs/alpha/baton.json");
    mkparent(&garbage);
    std::fs::write(&garbage, "{ not valid json\n").unwrap();
    let (code, out) = run_watchdog(Some(d.path()), &[], &[]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("DVANDVA_WATCHDOG note skipped_unreadable"),
        "{out}"
    );
    assert!(out.contains("skipped=1"), "{out}");
}

#[test]
fn wd_unparseable_updated_at_is_stale_with_reason() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "implementing",
        "vadi",
        5,
        "not-a-timestamp",
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("DVANDVA_WATCHDOG watchdog_stale"), "{out}");
    assert!(out.contains("age_s=unparseable"), "{out}");
    assert!(out.contains("reason=unparseable_updated_at"), "{out}");
    assert!(out.contains("stale=1"), "{out}");
}

#[test]
fn wd_future_updated_at_within_tolerance_is_healthy() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "implementing",
        "vadi",
        5,
        &timestamp_minus(-30),
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &["--stale-max", "1800"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("watchdog_stale"), "{out}");
    assert!(out.contains("stale=0"), "{out}");
}

#[test]
fn wd_future_updated_at_beyond_tolerance_is_stale_with_reason() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "implementing",
        "vadi",
        5,
        &timestamp_minus(-3600),
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &["--stale-max", "1800"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("DVANDVA_WATCHDOG watchdog_stale"), "{out}");
    assert!(out.contains("reason=future_updated_at"), "{out}");
    assert!(out.contains("stale=1"), "{out}");
}

#[test]
fn wd_far_future_updated_at_is_stale_with_reason() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "implementing",
        "vadi",
        5,
        "2099-01-01T00:00:00Z",
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &["--stale-max", "1800"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("DVANDVA_WATCHDOG watchdog_stale"), "{out}");
    assert!(out.contains("reason=future_updated_at"), "{out}");
    assert!(out.contains("stale=1"), "{out}");
}

// ── discovery / roots ────────────────────────────────────────────────────

#[test]
fn wd_multi_root_scan_counts_across_roots() {
    let d1 = tmp();
    let d2 = tmp();
    write_baton(
        &d1.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "implementing",
        "vadi",
        5,
        &timestamp_minus(100),
    );
    write_baton(
        &d2.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "implementing",
        "vadi",
        5,
        &timestamp_minus(100),
    );
    let (code, out) = run_watchdog(
        None,
        &[],
        &[
            d1.path().to_str().unwrap(),
            d2.path().to_str().unwrap(),
            "--stale-max",
            "10",
        ],
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("roots=2"), "{out}");
    assert!(out.contains("batons=2"), "{out}");
    assert!(out.contains("stale=2"), "{out}");
}

#[test]
fn wd_duplicate_root_args_count_baton_once() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "implementing",
        "vadi",
        5,
        &timestamp_minus(100),
    );
    let root = d.path().to_str().unwrap();
    let (code, out) = run_watchdog(None, &[], &[root, root, "--stale-max", "10"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out.matches("DVANDVA_WATCHDOG watchdog_stale").count(),
        1,
        "{out}"
    );
    assert!(out.contains("batons=1"), "{out}");
}

#[cfg(unix)]
#[test]
fn wd_unreadable_runs_dir_notes_skip_and_counts_it() {
    use std::os::unix::fs::PermissionsExt;

    let d = tmp();
    let runs_dir = d.path().join(".dvandva/runs");
    std::fs::create_dir_all(runs_dir.join("alpha")).unwrap();
    std::fs::write(runs_dir.join("alpha/baton.json"), "{}").unwrap();

    std::fs::set_permissions(&runs_dir, std::fs::Permissions::from_mode(0o000)).unwrap();

    if std::fs::read_dir(&runs_dir).is_ok() {
        // Running as a user (e.g. root) that bypasses permission bits —
        // chmod 000 has no effect, so there is nothing to assert.
        std::fs::set_permissions(&runs_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping wd_unreadable_runs_dir_notes_skip_and_counts_it: \
             chmod 000 had no effect (running as root?)"
        );
        return;
    }

    let (code, out) = run_watchdog(Some(d.path()), &[], &[]);

    // Restore permissions unconditionally so the tempdir can be cleaned up.
    std::fs::set_permissions(&runs_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("DVANDVA_WATCHDOG note skipped_unreadable_runs_dir"),
        "{out}"
    );
    assert!(
        out.contains(&format!("root={}", d.path().display())),
        "{out}"
    );
    assert!(
        out.contains(&format!("path={}", runs_dir.display())),
        "{out}"
    );
    assert!(out.contains("skipped=1"), "{out}");
    assert!(out.contains("batons=0"), "{out}");
}

#[test]
fn wd_legacy_baton_path_is_scanned() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/baton.json"),
        "legacy",
        "implementing",
        "vadi",
        5,
        &timestamp_minus(100),
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &["--stale-max", "10"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("DVANDVA_WATCHDOG watchdog_stale run_id=legacy"),
        "{out}"
    );
}

// ── summary grammar / usage ──────────────────────────────────────────────

#[test]
fn wd_summary_line_grammar() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "implementing",
        "vadi",
        5,
        &now_rfc3339(),
    );
    let (code, out) = run_watchdog(Some(d.path()), &[], &[]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("DVANDVA_WATCHDOG summary roots=1 batons=1 stale=0 paused=0 skipped=0"),
        "{out}"
    );
}

#[test]
fn wd_unknown_flag_exits_2() {
    let (code, out) = run_watchdog(None, &[], &["--bogus"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("Usage"), "{out}");
}

#[test]
fn wd_overflowing_threshold_is_usage_error() {
    let (code, out) = run_watchdog(None, &[], &["--stale-max", "999999999999999999999"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("ERROR") || out.contains("Usage"), "{out}");
}
