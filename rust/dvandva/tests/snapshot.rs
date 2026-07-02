//! Integration tests for `dvandva snapshot`, translated from
//! `scripts/test-dvandva-snapshot.sh` (case names mirror the shell case
//! labels/comments).

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn dvandva_snapshot(baton: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("snapshot")
        .arg(baton)
        .output()
        .expect("failed to run dvandva snapshot")
}

fn write_baton(path: &Path, assignee: &str, status: &str, checkpoint: i64, branch: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let json = format!(
        "{{\n  \"schema\": \"dvandva.baton.v1\",\n  \"assignee\": \"{assignee}\",\n  \"status\": \"{status}\",\n  \"phase\": 1,\n  \"checkpoint\": {checkpoint},\n  \"branch\": \"{branch}\"\n}}\n"
    );
    fs::write(path, json).unwrap();
}

// Case 1: happy path — snapshot lands in .dvandva/history/
#[test]
fn case1_happy_path_history_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let dvandva_dir = dir.path().join("case1/.dvandva");
    let baton = dvandva_dir.join("baton.json");
    write_baton(&baton, "vadi", "implementing", 3, "main");

    let out = dvandva_snapshot(&baton);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let expected_history = dvandva_dir.join("history/3-implementing-vadi.json");
    assert!(
        expected_history.is_file(),
        "history snapshot missing at {expected_history:?}"
    );
    assert_eq!(
        fs::read(&baton).unwrap(),
        fs::read(&expected_history).unwrap(),
        "history snapshot must be byte-identical to baton"
    );
}

// Case 2: terminal status also writes named archive
#[test]
fn case2_terminal_status_writes_named_archive() {
    let dir = tempfile::tempdir().unwrap();
    let dvandva_dir = dir.path().join("case2/.dvandva");
    let baton = dvandva_dir.join("baton.json");
    write_baton(&baton, "human", "done", 10, "feature-x");

    let out = dvandva_snapshot(&baton);
    assert_eq!(out.status.code(), Some(0));

    let expected_archive = dvandva_dir.join("baton.feature-x-10-done.json");
    assert!(
        expected_archive.is_file(),
        "terminal archive missing at {expected_archive:?}"
    );
    assert_eq!(
        fs::read(&baton).unwrap(),
        fs::read(&expected_archive).unwrap()
    );
}

// Case 2a: termination_review is active, not terminal; history only, no archive.
#[test]
fn case2a_termination_review_history_only_no_archive() {
    let dir = tempfile::tempdir().unwrap();
    let dvandva_dir = dir.path().join("case2a/.dvandva");
    let baton = dvandva_dir.join("baton.json");
    write_baton(&baton, "team", "termination_review", 9, "feature-x");

    let out = dvandva_snapshot(&baton);
    assert_eq!(out.status.code(), Some(0));

    let expected_history = dvandva_dir.join("history/9-termination_review-team.json");
    let unexpected_archive = dvandva_dir.join("baton.feature-x-9-termination_review.json");
    assert!(expected_history.is_file());
    assert!(!unexpected_archive.exists());
}

// Case 2 (named runs): named-run snapshots and archives stay under each run parent
#[test]
fn case2_named_run_snapshot_isolation() {
    let dir = tempfile::tempdir().unwrap();
    let runs_root = dir.path().join("case2-runs/.dvandva/runs");
    let alpha_dir = runs_root.join("alpha");
    let beta_dir = runs_root.join("beta");
    let alpha_baton = alpha_dir.join("baton.json");
    let beta_baton = beta_dir.join("baton.json");
    write_baton(&alpha_baton, "human", "done", 12, "alpha-branch");
    write_baton(&beta_baton, "human", "done", 13, "beta-branch");

    assert_eq!(dvandva_snapshot(&alpha_baton).status.code(), Some(0));
    assert_eq!(dvandva_snapshot(&beta_baton).status.code(), Some(0));

    assert!(alpha_dir.join("history/12-done-human.json").is_file());
    assert!(alpha_dir.join("baton.alpha-branch-12-done.json").is_file());
    assert!(beta_dir.join("history/13-done-human.json").is_file());
    assert!(beta_dir.join("baton.beta-branch-13-done.json").is_file());
    assert!(!dir.path().join("case2-runs/.dvandva/history").exists());
}

// Case 2b: branch with '/' is sanitized in archive filename
#[test]
fn case2b_branch_with_slash_sanitized_in_archive_filename() {
    let dir = tempfile::tempdir().unwrap();
    let dvandva_dir = dir.path().join("case2b/.dvandva");
    let baton = dvandva_dir.join("baton.json");
    write_baton(&baton, "human", "done", 11, "feature/foo");

    assert_eq!(dvandva_snapshot(&baton).status.code(), Some(0));

    let expected_sanitized = dvandva_dir.join("baton.feature-foo-11-done.json");
    let unintended_subpath = dvandva_dir.join("baton.feature").join("foo-11-done.json");
    assert!(expected_sanitized.is_file());
    assert!(!unintended_subpath.exists());
}

// Case 3: no-clobber on collision
#[test]
fn case3_no_clobber_on_collision() {
    let dir = tempfile::tempdir().unwrap();
    let dvandva_dir = dir.path().join("case3/.dvandva");
    let baton = dvandva_dir.join("baton.json");
    write_baton(&baton, "vadi", "implementing", 4, "main");
    assert_eq!(dvandva_snapshot(&baton).status.code(), Some(0));

    let original_history = dvandva_dir.join("history/4-implementing-vadi.json");
    let original_bytes = fs::read(&original_history).unwrap();

    // Modify baton (different bytes, same checkpoint), re-run.
    fs::write(
        &baton,
        r#"{"schema":"dvandva.baton.v1","assignee":"vadi","status":"implementing","phase":1,"checkpoint":4,"branch":"main","extra":"modified"}"#,
    )
    .unwrap();
    assert_eq!(dvandva_snapshot(&baton).status.code(), Some(0));

    let post_bytes = fs::read(&original_history).unwrap();
    assert_eq!(
        original_bytes, post_bytes,
        "original history file must be untouched"
    );

    let dup_count = fs::read_dir(dvandva_dir.join("history"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("4-implementing-vadi.dup-") && name.ends_with(".json")
        })
        .count();
    assert!(dup_count >= 1, "expected a dup file to be written");
}

// Case 3b: repeated snapshot on an unchanged baton is a true no-op —
// write_with_no_clobber's identical-bytes early return produces neither a
// dup file nor a no_clobber diagnostic, and leaves the target untouched.
#[test]
fn case3b_repeated_snapshot_unchanged_baton_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    let dvandva_dir = dir.path().join("case3b/.dvandva");
    let baton = dvandva_dir.join("baton.json");
    write_baton(&baton, "vadi", "implementing", 6, "main");

    let out1 = dvandva_snapshot(&baton);
    assert_eq!(out1.status.code(), Some(0));

    let history_target = dvandva_dir.join("history/6-implementing-vadi.json");
    let bytes_after_first = fs::read(&history_target).unwrap();

    let out2 = dvandva_snapshot(&baton);
    assert_eq!(
        out2.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out2.stderr)
    );

    let history_entries: Vec<_> = fs::read_dir(dvandva_dir.join("history"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        history_entries.len(),
        1,
        "expected exactly one history file, found: {:?}",
        history_entries
            .iter()
            .map(|e| e.file_name())
            .collect::<Vec<_>>()
    );
    assert!(
        !history_entries[0]
            .file_name()
            .to_string_lossy()
            .contains(".dup-"),
        "unchanged baton must not produce a dup file"
    );

    let bytes_after_second = fs::read(&history_target).unwrap();
    assert_eq!(
        bytes_after_first, bytes_after_second,
        "target bytes must be unchanged on repeated no-op snapshot"
    );

    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    assert!(
        !stderr2.contains("no_clobber"),
        "unchanged baton must not emit a no_clobber diagnostic, got: {stderr2}"
    );
}

// Case 4: snapshot write failure exits 23
#[test]
fn case4_snapshot_write_failure_exits_23() {
    let dir = tempfile::tempdir().unwrap();
    let dvandva_dir = dir.path().join("case4/.dvandva");
    let baton = dvandva_dir.join("baton.json");
    write_baton(&baton, "vadi", "implementing", 5, "main");
    // Occupy the `history` path with a regular file so `mkdir -p` fails.
    fs::write(dvandva_dir.join("history"), b"").unwrap();

    let out = dvandva_snapshot(&baton);
    assert_eq!(out.status.code(), Some(23));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("DVANDVA_SNAPSHOT write_failed"),
        "expected write_failed diagnostic, got: {stderr}"
    );
}

// Case 5 (byte-identical vadi/prativadi shell copies) has no Rust analog: the
// port is a single `snapshot_baton` function, not two shell files that could
// drift from each other. Not translated.

// --- Additional cases beyond the shell suite: the behavioral contract
// documents exit codes 2/21/22 that scripts/test-dvandva-snapshot.sh does not
// exercise directly (it only drives the write-failure and no-clobber paths).
// Covered here since they are part of the documented public contract.

#[test]
fn wrong_arg_count_exits_2_with_usage() {
    let out = Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("snapshot")
        .output()
        .expect("failed to run dvandva snapshot");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Usage:"));

    let out2 = Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("snapshot")
        .arg("a.json")
        .arg("b.json")
        .output()
        .expect("failed to run dvandva snapshot");
    assert_eq!(out2.status.code(), Some(2));
}

#[test]
fn missing_baton_file_exits_21() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope/baton.json");
    let out = dvandva_snapshot(&missing);
    assert_eq!(out.status.code(), Some(21));
    assert!(String::from_utf8_lossy(&out.stderr).contains("DVANDVA_SNAPSHOT missing"));
}

#[test]
fn invalid_json_baton_exits_22() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    fs::write(&baton, "{not json").unwrap();
    let out = dvandva_snapshot(&baton);
    assert_eq!(out.status.code(), Some(22));
    assert!(String::from_utf8_lossy(&out.stderr).contains("DVANDVA_SNAPSHOT invalid_json"));
}

#[test]
fn non_object_baton_json_exits_22() {
    // Mirrors jq's actual behavior: `.checkpoint` on a bare number/array/string
    // top-level value fails the `jq -r` pipeline, landing in the same
    // "invalid_json" branch as unparseable text.
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    fs::write(&baton, "42").unwrap();
    let out = dvandva_snapshot(&baton);
    assert_eq!(out.status.code(), Some(22));
}
