use assert_cmd::Command;
use predicates::prelude::*;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
}

#[test]
fn init_creates_a_run_centric_baton() {
    let dir = tempfile::tempdir().unwrap();
    command()
        .args([
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "run-a",
            "--objective",
            "Implement DEF-123",
            "--worker",
            "codex",
            "--reviewer",
            "claude",
        ])
        .assert()
        .success();

    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["schema"], "dvandva.run.v1");
    assert_eq!(baton["run_id"], "run-a");
    assert_eq!(baton["status"], "working");
    assert_eq!(baton["assignee"], "worker");
    assert_eq!(baton["participants"]["worker"]["harness"], "codex");
    assert_eq!(baton["participants"]["reviewer"]["harness"], "claude");
}

#[test]
fn init_rejects_unsafe_run_ids() {
    let dir = tempfile::tempdir().unwrap();
    command()
        .args([
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "../another-run",
            "--objective",
            "Implement DEF-123",
            "--worker",
            "codex",
            "--reviewer",
            "claude",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_input""#));
}

#[test]
fn init_rejects_blank_objectives() {
    let dir = tempfile::tempdir().unwrap();
    command()
        .args([
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "run-a",
            "--objective",
            "   ",
            "--worker",
            "codex",
            "--reviewer",
            "claude",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("objective must not be blank"));
}

#[test]
fn init_rejects_same_harness_family() {
    let dir = tempfile::tempdir().unwrap();
    command()
        .args([
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "run-a",
            "--objective",
            "Implement DEF-123",
            "--worker",
            "Codex",
            "--reviewer",
            "codex",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("different harness families"));
}

#[test]
fn init_rejects_duplicate_initialization() {
    let dir = tempfile::tempdir().unwrap();
    let args = [
        "init",
        "--run-dir",
        dir.path().to_str().unwrap(),
        "--run-id",
        "run-a",
        "--objective",
        "Implement DEF-123",
        "--worker",
        "codex",
        "--reviewer",
        "claude",
    ];
    command().args(args).assert().success();
    command()
        .args(args)
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"run_exists""#));
}

#[test]
fn read_emits_canonical_baton_json() {
    let dir = tempfile::tempdir().unwrap();
    command()
        .args([
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "run-a",
            "--objective",
            "Implement DEF-123",
            "--worker",
            "codex",
            "--reviewer",
            "claude",
        ])
        .assert()
        .success();

    let output = command()
        .args(["read", "--run-dir", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let from_stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let from_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(from_stdout, from_disk);
}
