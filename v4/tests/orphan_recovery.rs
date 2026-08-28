//! An install a writer died in the middle of — history one revision ahead of
//! the head — must be finished by the next reader, through the role facade,
//! with no human and no recovery command. Built by hand rather than by a
//! failpoint, so this runs in optimized builds too.

use assert_cmd::Command;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
}

fn run_json(command: &mut Command) -> serde_json::Value {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

struct Started {
    _root: tempfile::TempDir,
    workspace: std::path::PathBuf,
    runs: std::path::PathBuf,
    credentials: std::path::PathBuf,
    run_dir: std::path::PathBuf,
    run_id: String,
}

fn started() -> Started {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec![
            "remote",
            "add",
            "origin",
            "git@github.com:axatbhardwaj/Dvandva.git",
        ],
    ] {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&workspace)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    let runs = root.path().join("runs");
    let credentials = root.path().join("credentials");
    let created = run_json(command().args([
        "role",
        "start",
        "--api",
        "2",
        "--workspace",
        workspace.to_str().unwrap(),
        "--runs-dir",
        runs.to_str().unwrap(),
        "--credentials-root",
        credentials.to_str().unwrap(),
        "--role",
        "worker",
        "--session-id",
        "claude-session",
        "--current-harness",
        "claude",
        "--peer-harness",
        "codex",
        "--objective",
        "Orphan recovery",
        "--task-reference",
        "TASK-ORPHAN",
        "--required-deliverable",
        "kernel=Fix the kernel",
        "--lease-seconds",
        "1800",
    ]));
    let run_id = created["run_id"].as_str().unwrap().to_owned();
    let run_dir = std::path::PathBuf::from(created["run_dir"].as_str().unwrap());
    Started {
        _root: root,
        workspace,
        runs,
        credentials,
        run_dir,
        run_id,
    }
}

/// Leave the run exactly as a writer that died after linking revision N+1 but
/// before installing the head would: a progress report as the orphan.
fn orphan_progress_report(run_dir: &std::path::Path) -> u64 {
    let head: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    let revision = head["revision"].as_u64().unwrap();
    let mut orphan = head.clone();
    orphan["revision"] = serde_json::json!(revision + 1);
    orphan["participants"]["worker"]["progress"] = serde_json::json!({
        "phase": "working",
        "detail": "interrupted before install",
        "updated_at": head["participants"]["worker"]["claim"]["lease_started_at"],
    });
    let mut bytes = serde_json::to_vec_pretty(&orphan).unwrap();
    bytes.push(b'\n');
    std::fs::write(
        run_dir.join(format!("history/{:020}.json", revision + 1)),
        &bytes,
    )
    .unwrap();
    // The root temporary such a death leaves behind.
    std::fs::write(run_dir.join(".baton.dead-writer.tmp"), b"{ partial").unwrap();
    revision + 1
}

#[test]
fn an_exact_start_with_a_valid_credential_finishes_an_interrupted_install() {
    let fixture = started();
    let expected = orphan_progress_report(&fixture.run_dir);

    let resumed = run_json(command().args([
        "role",
        "start",
        "--api",
        "2",
        "--workspace",
        fixture.workspace.to_str().unwrap(),
        "--runs-dir",
        fixture.runs.to_str().unwrap(),
        "--credentials-root",
        fixture.credentials.to_str().unwrap(),
        "--role",
        "worker",
        "--session-id",
        "claude-session",
        "--current-harness",
        "claude",
        "--peer-harness",
        "codex",
        "--run-id",
        &fixture.run_id,
    ]));
    assert_eq!(resumed["outcome"], "started");
    assert_eq!(resumed["disposition"], "resumed");
    assert_eq!(
        resumed["revision"].as_u64().unwrap(),
        expected,
        "the exact start must return the reconciled head, not the stale one"
    );
    assert_eq!(
        resumed["participants"]["worker"]["progress"]["phase"],
        "working"
    );

    let head: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.run_dir.join("baton.json")).unwrap())
            .unwrap();
    assert_eq!(head["revision"].as_u64().unwrap(), expected);
    let leftovers = [fixture.run_dir.clone(), fixture.run_dir.join("history")]
        .iter()
        .flat_map(|directory| std::fs::read_dir(directory).unwrap().flatten())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with('.') && name.ends_with(".tmp")
        })
        .count();
    assert_eq!(
        leftovers, 0,
        "reconciliation must scavenge the dead writer's temporary"
    );
}

#[test]
fn a_facade_read_also_finishes_an_interrupted_install() {
    let fixture = started();
    let expected = orphan_progress_report(&fixture.run_dir);
    let snapshot = run_json(command().args([
        "role",
        "read",
        "--api",
        "2",
        "--run-dir",
        fixture.run_dir.to_str().unwrap(),
        "--role",
        "worker",
        "--session-id",
        "claude-session",
        "--credentials-root",
        fixture.credentials.to_str().unwrap(),
    ]));
    assert_eq!(snapshot["revision"].as_u64().unwrap(), expected);
}

#[test]
fn an_orphan_over_a_tampered_prefix_is_refused_not_installed() {
    let fixture = started();
    orphan_progress_report(&fixture.run_dir);
    let revision_zero = fixture.run_dir.join("history/00000000000000000000.json");
    let mut tampered: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&revision_zero).unwrap()).unwrap();
    tampered["objective"]["summary"] = serde_json::json!("something else entirely");
    std::fs::write(
        &revision_zero,
        serde_json::to_vec_pretty(&tampered).unwrap(),
    )
    .unwrap();

    let head_before = std::fs::read(fixture.run_dir.join("baton.json")).unwrap();
    let output = command()
        .args([
            "role",
            "start",
            "--api",
            "2",
            "--workspace",
            fixture.workspace.to_str().unwrap(),
            "--runs-dir",
            fixture.runs.to_str().unwrap(),
            "--credentials-root",
            fixture.credentials.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "claude-session",
            "--current-harness",
            "claude",
            "--peer-harness",
            "codex",
            "--run-id",
            &fixture.run_id,
        ])
        .output()
        .unwrap();
    // Discovery reports the run as corrupt rather than joining it; either way
    // the start must not proceed and nothing may be installed.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let refused = !output.status.success()
        || serde_json::from_str::<serde_json::Value>(&stdout)
            .is_ok_and(|value| value["outcome"] == "corrupt");
    assert!(
        refused,
        "a corrupt chain must not be advanced: {stdout}{stderr}"
    );
    assert!(
        stdout.contains("invalid_history")
            || stderr.contains("invalid_history")
            || stdout.contains("corrupt"),
        "the corruption must be named: {stdout}{stderr}"
    );
    assert_eq!(
        std::fs::read(fixture.run_dir.join("baton.json")).unwrap(),
        head_before,
        "the head must be untouched"
    );
}
