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

#[test]
fn concurrent_initialization_has_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let spawn = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
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
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let first = spawn();
    let second = spawn();
    let outputs = [
        first.wait_with_output().unwrap(),
        second.wait_with_output().unwrap(),
    ];

    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1
    );
    let loser = outputs
        .iter()
        .find(|output| !output.status.success())
        .unwrap();
    assert!(String::from_utf8_lossy(&loser.stderr).contains(r#""error":"run_exists""#));
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["revision"], 0);
    assert_eq!(
        std::fs::read_dir(dir.path().join("history"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn corrupt_current_state_is_reported_without_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("baton.json"), b"{not json}\n").unwrap();

    command()
        .args(["read", "--run-dir", dir.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_baton""#));
    assert_eq!(
        std::fs::read(dir.path().join("baton.json")).unwrap(),
        b"{not json}\n"
    );
}

#[test]
fn initialization_history_is_immutable() {
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
    let history = dir.path().join("history/00000000000000000000.json");
    let original = std::fs::read(&history).unwrap();

    command()
        .args([
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "run-b",
            "--objective",
            "Replace the run",
            "--worker",
            "codex",
            "--reviewer",
            "claude",
        ])
        .assert()
        .failure();
    assert_eq!(std::fs::read(history).unwrap(), original);
}

#[test]
fn separate_run_directories_are_independent() {
    let parent = tempfile::tempdir().unwrap();
    for run_id in ["run-a", "run-b"] {
        command()
            .args([
                "init",
                "--run-dir",
                parent.path().join(run_id).to_str().unwrap(),
                "--run-id",
                run_id,
                "--objective",
                "Independent objective",
                "--worker",
                "codex",
                "--reviewer",
                "claude",
            ])
            .assert()
            .success();
    }
    for run_id in ["run-a", "run-b"] {
        let baton: serde_json::Value = serde_json::from_slice(
            &std::fs::read(parent.path().join(run_id).join("baton.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(baton["run_id"], run_id);
    }
}
