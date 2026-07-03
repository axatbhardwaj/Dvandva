//! Integration tests for `dvandva watchdog` (task WD).
//!
//! Each case spawns the real `dvandva` binary against a fixture `.dvandva`
//! tree written into a tempdir. Timestamps are built relative to "now" via
//! [`timestamp_minus`] so staleness/bucket assertions do not depend on the
//! wall-clock date, and thresholds are kept small (seconds) so the suite
//! stays fast.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use time::OffsetDateTime;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

/// Spawn `dvandva watchdog <args>` in `cwd` with `DVANDVA_NOTIFY_URL`
/// cleared unless `envs` sets it. `watchdog` is one-shot (never blocks), so
/// this is a plain blocking `output()` call.
fn run_watchdog(cwd: Option<&Path>, envs: &[(&str, &str)], args: &[&str]) -> (i32, String) {
    let mut cmd = Command::new(bin());
    cmd.arg("watchdog").args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.env_remove("DVANDVA_NOTIFY_URL");
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

// ── notify TCP fixture (mirrored from tests/wait.rs — not importable across
// separate integration-test binaries; see that file's equivalent helpers). ──

fn start_notify_listener() -> (u16, mpsc::Receiver<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind notify listener");
    let port = listener.local_addr().expect("local_addr").port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let request = read_full_http_request(&mut stream);
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = tx.send(request);
        }
    });
    (port, rx)
}

fn read_full_http_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok();
    let mut raw: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if let Some(header_end) = find_subslice(&raw, b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&raw[..header_end]).to_lowercase();
                    let want_body = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if raw.len() >= header_end + 4 + want_body {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&raw).into_owned()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A TCP port with nothing listening on it: bind then immediately drop, so
/// a POST to it fails fast (connection refused) instead of timing out.
fn dead_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind dead port");
    listener.local_addr().expect("local_addr").port()
}

// ── classification ────────────────────────────────────────────────────────

#[test]
fn wd_stale_mid_work_emits_event_and_posts_notify() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "implementing",
        "vadi",
        5,
        &timestamp_minus(100),
    );
    let (port, rx) = start_notify_listener();
    let url = format!("http://127.0.0.1:{port}/");
    let (code, out) = run_watchdog(
        Some(d.path()),
        &[],
        &["--stale-max", "10", "--notify", &url],
    );
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(
            "DVANDVA_WATCHDOG watchdog_stale run_id=alpha status=implementing assignee=vadi checkpoint=5"
        ),
        "{out}"
    );
    assert!(out.contains("stale=1"), "{out}");

    let request = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("notify request");
    assert!(request.starts_with("POST"), "{request}");
    assert!(
        request
            .to_lowercase()
            .contains("title: dvandva alpha: watchdog_stale"),
        "{request}"
    );
    assert!(request.contains("event=watchdog_stale"), "{request}");
    assert!(request.contains("status=implementing"), "{request}");
    assert!(request.contains("assignee=vadi"), "{request}");
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

// ── dedupe / bucketing ───────────────────────────────────────────────────

#[test]
fn wd_failed_post_leaves_marker_unset_then_live_listener_delivers_once() {
    let d = tmp();
    let baton = d.path().join(".dvandva/runs/alpha/baton.json");
    write_baton(
        &baton,
        "alpha",
        "implementing",
        "vadi",
        5,
        "2024-01-01T00:00:00Z",
    );

    let dead_url = format!("http://127.0.0.1:{}/", dead_port());
    let (code1, out1) = run_watchdog(
        Some(d.path()),
        &[],
        &["--stale-max", "10", "--notify", &dead_url],
    );
    assert_eq!(code1, 0, "{out1}");
    assert!(out1.contains("watchdog_stale"), "{out1}");

    let (port2, rx2) = start_notify_listener();
    let url2 = format!("http://127.0.0.1:{port2}/");
    let (code2, out2) = run_watchdog(
        Some(d.path()),
        &[],
        &["--stale-max", "10", "--notify", &url2],
    );
    assert_eq!(code2, 0, "{out2}");
    assert!(
        rx2.recv_timeout(Duration::from_secs(3)).is_ok(),
        "a failed POST must not mark the finding delivered — the next scan \
         against a live listener must still deliver exactly one POST"
    );
}

#[test]
fn wd_no_url_writes_no_marker_then_configuring_url_delivers_next_scan() {
    let d = tmp();
    let baton = d.path().join(".dvandva/runs/alpha/baton.json");
    write_baton(
        &baton,
        "alpha",
        "implementing",
        "vadi",
        5,
        "2024-01-01T00:00:00Z",
    );

    for _ in 0..2 {
        let (code, out) = run_watchdog(Some(d.path()), &[], &["--stale-max", "10"]);
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("watchdog_stale"), "{out}");
    }

    let (port, rx) = start_notify_listener();
    let url = format!("http://127.0.0.1:{port}/");
    let (code, out) = run_watchdog(
        Some(d.path()),
        &[],
        &["--stale-max", "10", "--notify", &url],
    );
    assert_eq!(code, 0, "{out}");
    assert!(
        rx.recv_timeout(Duration::from_secs(3)).is_ok(),
        "no marker should have accrued while the URL was unset — the scan \
         that first configures it must deliver immediately"
    );
}

#[test]
fn wd_dedupe_suppresses_second_identical_post() {
    let d = tmp();
    let baton = d.path().join(".dvandva/runs/alpha/baton.json");
    write_baton(
        &baton,
        "alpha",
        "implementing",
        "vadi",
        5,
        "2024-01-01T00:00:00Z",
    );

    let (port1, rx1) = start_notify_listener();
    let url1 = format!("http://127.0.0.1:{port1}/");
    let (code1, out1) = run_watchdog(
        Some(d.path()),
        &[],
        &["--stale-max", "10", "--notify", &url1],
    );
    assert_eq!(code1, 0, "{out1}");
    assert!(out1.contains("watchdog_stale"), "{out1}");
    assert!(
        rx1.recv_timeout(Duration::from_secs(3)).is_ok(),
        "first run should POST"
    );

    let (port2, rx2) = start_notify_listener();
    let url2 = format!("http://127.0.0.1:{port2}/");
    let (code2, out2) = run_watchdog(
        Some(d.path()),
        &[],
        &["--stale-max", "10", "--notify", &url2],
    );
    assert_eq!(code2, 0, "{out2}");
    assert!(
        out2.contains("watchdog_stale"),
        "the finding line is always printed, even when the POST is deduped: {out2}"
    );
    assert!(
        rx2.recv_timeout(Duration::from_millis(300)).is_err(),
        "second run with identical (status, checkpoint, bucket) must not re-POST"
    );
}

#[test]
fn wd_checkpoint_bump_renotifies() {
    let d = tmp();
    let baton = d.path().join(".dvandva/runs/alpha/baton.json");
    write_baton(
        &baton,
        "alpha",
        "implementing",
        "vadi",
        5,
        "2024-01-01T00:00:00Z",
    );

    let (port1, rx1) = start_notify_listener();
    let url1 = format!("http://127.0.0.1:{port1}/");
    run_watchdog(
        Some(d.path()),
        &[],
        &["--stale-max", "10", "--notify", &url1],
    );
    assert!(rx1.recv_timeout(Duration::from_secs(3)).is_ok());

    write_baton(
        &baton,
        "alpha",
        "implementing",
        "vadi",
        6,
        "2024-01-01T00:00:00Z",
    );
    let (port2, rx2) = start_notify_listener();
    let url2 = format!("http://127.0.0.1:{port2}/");
    run_watchdog(
        Some(d.path()),
        &[],
        &["--stale-max", "10", "--notify", &url2],
    );
    assert!(
        rx2.recv_timeout(Duration::from_secs(3)).is_ok(),
        "checkpoint bump must re-notify"
    );
}

#[test]
fn wd_bucket_crossing_renotifies() {
    let d = tmp();
    let baton = d.path().join(".dvandva/runs/alpha/baton.json");
    // stale-max=2s: age 5s -> bucket "1x".
    write_baton(
        &baton,
        "alpha",
        "implementing",
        "vadi",
        5,
        &timestamp_minus(5),
    );

    let (port1, rx1) = start_notify_listener();
    let url1 = format!("http://127.0.0.1:{port1}/");
    run_watchdog(
        Some(d.path()),
        &[],
        &["--stale-max", "2", "--notify", &url1],
    );
    assert!(rx1.recv_timeout(Duration::from_secs(3)).is_ok());

    // Same status/checkpoint, but age now crosses into bucket "4x" (>= 8s).
    write_baton(
        &baton,
        "alpha",
        "implementing",
        "vadi",
        5,
        &timestamp_minus(20),
    );
    let (port2, rx2) = start_notify_listener();
    let url2 = format!("http://127.0.0.1:{port2}/");
    run_watchdog(
        Some(d.path()),
        &[],
        &["--stale-max", "2", "--notify", &url2],
    );
    assert!(
        rx2.recv_timeout(Duration::from_secs(3)).is_ok(),
        "crossing into the next age bucket must re-notify"
    );
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

// ── notify configuration / summary grammar / usage ──────────────────────

#[test]
fn wd_no_url_prints_note_without_crash() {
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
    assert_eq!(
        out.matches("DVANDVA_WATCHDOG note notify_unconfigured")
            .count(),
        1,
        "{out}"
    );
}

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
