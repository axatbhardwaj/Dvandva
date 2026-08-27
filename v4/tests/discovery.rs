use assert_cmd::Command;
use dvandva_v4::{
    claim::{self, Role},
    model::{ParticipantClaim, RunBaton, Status, TaskIdentity, WorkspaceIdentity},
    store::RunChannel,
};

const REPOSITORY_ID: &str = "github.com/axatbhardwaj/dvandva";

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
}

fn create_run(
    runs: &std::path::Path,
    run_id: &str,
    repository_id: &str,
    task_reference: Option<&str>,
    reviewer: &str,
) -> std::path::PathBuf {
    let run_dir = runs.join(run_id);
    let baton = RunBaton::new(run_id, "Implement the ticket", "codex", reviewer)
        .with_discovery_identity(
            WorkspaceIdentity {
                repository_id: repository_id.to_owned(),
                origin: Some("git@github.com:axatbhardwaj/Dvandva.git".to_owned()),
                worktree: Some("/tmp/worker".to_owned()),
            },
            TaskIdentity {
                reference: task_reference.map(str::to_owned),
                summary: "Implement the ticket".to_owned(),
            },
        );
    RunChannel::open(&run_dir).create(&baton).unwrap();
    run_dir
}

fn discover(runs: &std::path::Path, task_reference: Option<&str>) -> serde_json::Value {
    let mut args = vec![
        "discover",
        "--runs-dir",
        runs.to_str().unwrap(),
        "--repository-id",
        REPOSITORY_ID,
        "--reviewer-harness",
        "claude",
    ];
    if let Some(reference) = task_reference {
        args.extend(["--task-reference", reference]);
    }
    let output = command().args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn one_matching_active_run_is_returned() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = create_run(
        root.path(),
        "def-123-a1",
        REPOSITORY_ID,
        Some("DEF-123"),
        "claude",
    );

    let outcome = discover(root.path(), Some("DEF-123"));

    assert_eq!(outcome["outcome"], "match");
    assert_eq!(outcome["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(outcome["candidates"][0]["run_id"], "def-123-a1");
    assert_eq!(
        outcome["candidates"][0]["run_dir"],
        run_dir.to_str().unwrap()
    );
    assert_eq!(outcome["candidates"][0]["task_reference"], "DEF-123");
    assert_eq!(outcome["candidates"][0]["status"], "working");
    assert_eq!(outcome["corrupt"], serde_json::json!([]));
}

#[test]
fn a_live_reviewer_claim_is_not_joinable() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = create_run(
        root.path(),
        "def-123-claimed",
        REPOSITORY_ID,
        Some("DEF-123"),
        "claude",
    );
    claim::claim(
        &RunChannel::open(&run_dir),
        Role::Reviewer,
        "existing-reviewer",
        300,
        0,
    )
    .unwrap();

    let outcome = discover(root.path(), Some("DEF-123"));

    assert_eq!(outcome["outcome"], "none");
    assert_eq!(outcome["candidates"], serde_json::json!([]));
}

#[test]
fn an_expired_reviewer_claim_is_reclaimable() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("def-123-expired");
    let mut baton = RunBaton::new("def-123-expired", "Implement", "codex", "claude")
        .with_discovery_identity(
            WorkspaceIdentity {
                repository_id: REPOSITORY_ID.to_owned(),
                origin: None,
                worktree: Some("/tmp/worker".to_owned()),
            },
            TaskIdentity {
                reference: Some("DEF-123".to_owned()),
                summary: "Implement".to_owned(),
            },
        );
    baton.participants.reviewer.claim = Some(ParticipantClaim {
        session_id: "gone-reviewer".to_owned(),
        epoch: 3,
        token_digest: "old-digest".to_owned(),
        lease_expires_at: "2000-01-01T00:00:00Z".to_owned(),
        lease_seconds: 300,
    });
    RunChannel::open(run_dir).create(&baton).unwrap();

    let outcome = discover(root.path(), Some("DEF-123"));
    assert_eq!(outcome["outcome"], "match");
    assert_eq!(outcome["candidates"][0]["run_id"], "def-123-expired");
}

#[test]
fn explicit_task_reference_narrows_otherwise_ambiguous_runs() {
    let root = tempfile::tempdir().unwrap();
    create_run(
        root.path(),
        "def-123-a",
        REPOSITORY_ID,
        Some("DEF-123"),
        "claude",
    );
    create_run(
        root.path(),
        "def-456-b",
        REPOSITORY_ID,
        Some("DEF-456"),
        "claude",
    );

    let broad = discover(root.path(), None);
    assert_eq!(broad["outcome"], "ambiguous");
    assert_eq!(
        broad["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|candidate| candidate["run_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["def-123-a", "def-456-b"]
    );

    let narrow = discover(root.path(), Some("def-456"));
    assert_eq!(narrow["outcome"], "match");
    assert_eq!(narrow["candidates"][0]["run_id"], "def-456-b");
}

#[test]
fn wrong_repository_reviewer_and_terminal_runs_are_ignored() {
    let root = tempfile::tempdir().unwrap();
    create_run(
        root.path(),
        "wrong-repo",
        "github.com/example/other",
        Some("DEF-123"),
        "claude",
    );
    create_run(
        root.path(),
        "wrong-reviewer",
        REPOSITORY_ID,
        Some("DEF-123"),
        "codex",
    );
    let terminal_dir = create_run(
        root.path(),
        "terminal",
        REPOSITORY_ID,
        Some("DEF-123"),
        "claude",
    );
    let channel = RunChannel::open(&terminal_dir);
    let mut terminal = channel.read().unwrap();
    terminal.status = Status::Done;
    terminal.revision = 1;
    channel.compare_and_swap(0, &terminal).unwrap();

    let outcome = discover(root.path(), Some("DEF-123"));
    assert_eq!(outcome["outcome"], "none");
    assert_eq!(outcome["candidates"], serde_json::json!([]));
}

#[test]
fn corrupt_candidates_fail_closed_and_are_reported_separately() {
    let root = tempfile::tempdir().unwrap();
    create_run(
        root.path(),
        "valid",
        REPOSITORY_ID,
        Some("DEF-123"),
        "claude",
    );
    let corrupt = root.path().join("corrupt");
    std::fs::create_dir(&corrupt).unwrap();
    std::fs::write(corrupt.join("baton.json"), b"not json\n").unwrap();

    let outcome = discover(root.path(), Some("DEF-123"));
    assert_eq!(outcome["outcome"], "corrupt");
    assert_eq!(outcome["candidates"][0]["run_id"], "valid");
    assert_eq!(outcome["corrupt"].as_array().unwrap().len(), 1);
    assert_eq!(outcome["corrupt"][0]["run_dir"], corrupt.to_str().unwrap());
}

#[test]
fn a_missing_runs_root_has_no_match() {
    let root = tempfile::tempdir().unwrap().path().join("not-created");
    let outcome = discover(&root, Some("DEF-123"));
    assert_eq!(outcome["outcome"], "none");
    assert_eq!(outcome["candidates"], serde_json::json!([]));
    assert_eq!(outcome["corrupt"], serde_json::json!([]));
}

#[test]
fn discovery_wait_started_first_wakes_when_vadi_creates_a_run() {
    let root = tempfile::tempdir().unwrap();
    let runs = root.path().join("state/dvandva/runs");
    let waiter = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
        .args([
            "discover-wait",
            "--runs-dir",
            runs.to_str().unwrap(),
            "--repository-id",
            REPOSITORY_ID,
            "--reviewer-harness",
            "claude",
            "--task-reference",
            "DEF-123",
            "--poll-interval-ms",
            "25",
            "--timeout-ms",
            "3000",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));
    std::fs::create_dir_all(&runs).unwrap();
    create_run(
        &runs,
        "def-123-later",
        REPOSITORY_ID,
        Some("DEF-123"),
        "claude",
    );

    let output = waiter.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["outcome"], "match");
    assert_eq!(outcome["candidates"][0]["run_id"], "def-123-later");
}

#[test]
fn polling_only_wait_finds_a_later_run() {
    let root = tempfile::tempdir().unwrap();
    let runs = root.path().join("runs");
    let waiter = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
        .args([
            "discover-wait",
            "--runs-dir",
            runs.to_str().unwrap(),
            "--repository-id",
            REPOSITORY_ID,
            "--reviewer-harness",
            "claude",
            "--task-reference",
            "DEF-123",
            "--poll-interval-ms",
            "20",
            "--timeout-ms",
            "3000",
            "--poll-only",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));
    create_run(
        &runs,
        "polling-run",
        REPOSITORY_ID,
        Some("DEF-123"),
        "claude",
    );

    let output = waiter.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["outcome"], "match");
    assert_eq!(outcome["candidates"][0]["run_id"], "polling-run");
}

#[test]
fn unrelated_events_do_not_turn_a_timed_wait_into_a_match() {
    let root = tempfile::tempdir().unwrap();
    let runs = root.path().join("runs");
    std::fs::create_dir(&runs).unwrap();
    let waiter = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
        .args([
            "discover-wait",
            "--runs-dir",
            runs.to_str().unwrap(),
            "--repository-id",
            REPOSITORY_ID,
            "--reviewer-harness",
            "claude",
            "--poll-interval-ms",
            "25",
            "--timeout-ms",
            "200",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(50));
    std::fs::write(runs.join("unrelated-event"), b"wake hint only\n").unwrap();

    let output = waiter.wait_with_output().unwrap();
    assert!(output.status.success());
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["outcome"], "none");
    assert_eq!(outcome["candidates"], serde_json::json!([]));
}

#[test]
fn discovery_wait_surfaces_ambiguity_without_selecting_by_recency() {
    let root = tempfile::tempdir().unwrap();
    create_run(root.path(), "z-older", REPOSITORY_ID, None, "claude");
    std::thread::sleep(std::time::Duration::from_millis(5));
    create_run(root.path(), "a-newer", REPOSITORY_ID, None, "claude");

    let output = command()
        .args([
            "discover-wait",
            "--runs-dir",
            root.path().to_str().unwrap(),
            "--repository-id",
            REPOSITORY_ID,
            "--reviewer-harness",
            "claude",
            "--timeout-ms",
            "500",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["outcome"], "ambiguous");
    assert_eq!(
        outcome["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|candidate| candidate["run_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["a-newer", "z-older"]
    );
}

#[test]
fn discovery_wait_fails_closed_on_a_corrupt_baton() {
    let root = tempfile::tempdir().unwrap();
    let corrupt = root.path().join("corrupt");
    std::fs::create_dir(&corrupt).unwrap();
    std::fs::write(corrupt.join("baton.json"), b"{\n").unwrap();

    let output = command()
        .args([
            "discover-wait",
            "--runs-dir",
            root.path().to_str().unwrap(),
            "--repository-id",
            REPOSITORY_ID,
            "--reviewer-harness",
            "claude",
            "--timeout-ms",
            "500",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["outcome"], "corrupt");
    assert_eq!(outcome["corrupt"].as_array().unwrap().len(), 1);
}

#[test]
fn worker_discovery_is_independent_of_a_live_reviewer_claim() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = create_run(
        root.path(),
        "def-123-worker",
        REPOSITORY_ID,
        Some("DEF-123"),
        "claude",
    );
    claim::claim(
        &RunChannel::open(&run_dir),
        Role::Reviewer,
        "reviewer-session",
        300,
        0,
    )
    .unwrap();

    let output = command()
        .args([
            "discover",
            "--runs-dir",
            root.path().to_str().unwrap(),
            "--repository-id",
            REPOSITORY_ID,
            "--role",
            "worker",
            "--harness",
            "codex",
            "--task-reference",
            "DEF-123",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["outcome"], "match");
    assert_eq!(outcome["candidates"][0]["run_id"], "def-123-worker");
}
