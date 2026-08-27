use assert_cmd::Command;
use predicates::prelude::*;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
}

fn write_action(dir: &std::path::Path, name: &str, action: serde_json::Value) -> String {
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_vec_pretty(&action).unwrap()).unwrap();
    path.to_str().unwrap().to_owned()
}

fn init_pair(dir: &std::path::Path) {
    command()
        .args([
            "init",
            "--run-dir",
            dir.to_str().unwrap(),
            "--run-id",
            "run-a",
            "--objective",
            "Implement DEF-123",
            "--worker",
            "codex",
            "--reviewer",
            "claude",
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
        ])
        .assert()
        .success();
}

fn claim_role(dir: &std::path::Path, role: &str, session: &str, revision: u64) -> String {
    let output = command()
        .args([
            "claim",
            "--run-dir",
            dir.to_str().unwrap(),
            "--role",
            role,
            "--session-id",
            session,
            "--lease-seconds",
            "300",
            "--expected-revision",
            &revision.to_string(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn apply_action(
    dir: &std::path::Path,
    role: &str,
    session: &str,
    token: &str,
    revision: u64,
    name: &str,
    action: serde_json::Value,
) -> assert_cmd::assert::Assert {
    let path = write_action(dir, name, action);
    command()
        .args([
            "apply",
            "--run-dir",
            dir.to_str().unwrap(),
            "--role",
            role,
            "--session-id",
            session,
            "--token",
            token,
            "--expected-revision",
            &revision.to_string(),
            "--action",
            &path,
        ])
        .assert()
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
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
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
fn init_persists_workspace_and_task_identity() {
    let dir = tempfile::tempdir().unwrap();
    command()
        .args([
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "run-a",
            "--objective",
            "  Implement DEF-123  ",
            "--worker",
            "  codex  ",
            "--reviewer",
            "  claude  ",
            "--repository-id",
            "  github.com/axatbhardwaj/dvandva  ",
            "--origin",
            "  git@github.com:axatbhardwaj/Dvandva.git  ",
            "--worktree",
            "  /tmp/dvandva-a  ",
            "--task-reference",
            "  DEF-123  ",
        ])
        .assert()
        .success();

    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(
        baton["workspace"],
        serde_json::json!({
            "repository_id": "github.com/axatbhardwaj/dvandva",
            "origin": "git@github.com:axatbhardwaj/Dvandva.git",
            "worktree": "/tmp/dvandva-a"
        })
    );
    assert_eq!(
        baton["task"],
        serde_json::json!({
            "reference": "DEF-123",
            "summary": "Implement DEF-123"
        })
    );
    assert_eq!(baton["participants"]["worker"]["harness"], "codex");
    assert_eq!(baton["participants"]["reviewer"]["harness"], "claude");
}

#[test]
fn init_rejects_blank_repository_identity() {
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
            "--repository-id",
            "   ",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repository id must not be blank"));
}

#[test]
fn init_rejects_a_supplied_blank_task_reference() {
    let dir = tempfile::tempdir().unwrap();
    command()
        .args([
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "run-a",
            "--objective",
            "Implement the approved change",
            "--worker",
            "codex",
            "--reviewer",
            "claude",
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
            "--task-reference",
            "   ",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("task reference must not be blank"));
}

#[test]
fn init_rejects_supplied_blank_workspace_metadata() {
    for flag in ["--origin", "--worktree"] {
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
                "--repository-id",
                "github.com/axatbhardwaj/dvandva",
                flag,
                "   ",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains("must not be blank"));
    }
}

#[test]
fn pre_discovery_batons_remain_deserializable() {
    let legacy =
        dvandva_v4::model::RunBaton::new("legacy-run", "Preserved objective", "codex", "claude");
    let encoded = serde_json::to_value(legacy).unwrap();
    assert!(encoded.get("workspace").is_none());
    assert!(encoded.get("task").is_none());

    let decoded: dvandva_v4::model::RunBaton = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.run_id, "legacy-run");
    assert_eq!(decoded.workspace, None);
    assert_eq!(decoded.task, None);
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
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
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
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
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
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
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
        "--repository-id",
        "github.com/axatbhardwaj/dvandva",
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
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
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
                "--repository-id",
                "github.com/axatbhardwaj/dvandva",
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
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
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
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
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
                "--repository-id",
                "github.com/axatbhardwaj/dvandva",
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

#[test]
fn expired_claim_replacement_fences_the_old_session() {
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
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
        ])
        .assert()
        .success();

    let first = command()
        .args([
            "claim",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-1",
            "--lease-seconds",
            "1",
            "--expected-revision",
            "0",
        ])
        .output()
        .unwrap();
    assert!(first.status.success());
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    let first_token = first["token"].as_str().unwrap();

    command()
        .args([
            "claim",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-2",
            "--lease-seconds",
            "1",
            "--expected-revision",
            "1",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"claim_active""#));

    std::thread::sleep(std::time::Duration::from_millis(1_100));
    let replacement = command()
        .args([
            "reclaim",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-2",
            "--lease-seconds",
            "30",
            "--expected-revision",
            "1",
        ])
        .output()
        .unwrap();
    assert!(replacement.status.success());
    let replacement: serde_json::Value = serde_json::from_slice(&replacement.stdout).unwrap();
    let replacement_token = replacement["token"].as_str().unwrap();

    command()
        .args([
            "heartbeat",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-1",
            "--token",
            first_token,
            "--lease-seconds",
            "30",
            "--expected-revision",
            "2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"claim_fenced""#));

    let heartbeat_args = [
        "heartbeat",
        "--run-dir",
        dir.path().to_str().unwrap(),
        "--role",
        "worker",
        "--session-id",
        "worker-2",
        "--token",
        replacement_token,
        "--lease-seconds",
        "30",
        "--expected-revision",
        "2",
    ];
    command().args(heartbeat_args).assert().success();
    command()
        .args(heartbeat_args)
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"revision_conflict""#));
}

#[test]
fn worker_and_reviewer_claims_are_independent_and_tokens_are_secret() {
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
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
        ])
        .assert()
        .success();

    let worker = command()
        .args([
            "claim",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-1",
            "--lease-seconds",
            "30",
            "--expected-revision",
            "0",
        ])
        .output()
        .unwrap();
    assert!(worker.status.success());
    let worker: serde_json::Value = serde_json::from_slice(&worker.stdout).unwrap();
    let token = worker["token"].as_str().unwrap();

    command()
        .args([
            "claim",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "reviewer",
            "--session-id",
            "reviewer-1",
            "--lease-seconds",
            "30",
            "--expected-revision",
            "1",
        ])
        .assert()
        .success();
    command()
        .args([
            "reclaim",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-2",
            "--lease-seconds",
            "30",
            "--expected-revision",
            "2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"claim_not_expired""#));
    command()
        .args([
            "heartbeat",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-1",
            "--token",
            "wrong-token",
            "--lease-seconds",
            "30",
            "--expected-revision",
            "2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"claim_fenced""#));

    let baton = std::fs::read_to_string(dir.path().join("baton.json")).unwrap();
    assert!(!baton.contains(token));
    assert!(baton.contains("token_digest"));
}

#[test]
fn complete_review_fix_loop_reaches_done() {
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
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
        ])
        .assert()
        .success();
    let claim = |role: &str, session: &str, revision: &str| {
        let output = command()
            .args([
                "claim",
                "--run-dir",
                dir.path().to_str().unwrap(),
                "--role",
                role,
                "--session-id",
                session,
                "--lease-seconds",
                "300",
                "--expected-revision",
                revision,
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["token"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let worker_token = claim("worker", "worker-1", "0");
    let reviewer_token = claim("reviewer", "reviewer-1", "1");
    let apply = |role: &str, session: &str, token: &str, revision: &str, action: String| {
        command()
            .args([
                "apply",
                "--run-dir",
                dir.path().to_str().unwrap(),
                "--role",
                role,
                "--session-id",
                session,
                "--token",
                token,
                "--expected-revision",
                revision,
                "--action",
                &action,
            ])
            .assert()
            .success();
    };

    apply(
        "worker",
        "worker-1",
        &worker_token,
        "2",
        write_action(
            dir.path(),
            "first.json",
            serde_json::json!({
                "type": "submit_checkpoint", "checkpoint": {
                    "kind": "artifact", "identity": "sha256:first", "verification": ["tests passed"]
                }
            }),
        ),
    );
    apply(
        "reviewer",
        "reviewer-1",
        &reviewer_token,
        "3",
        write_action(
            dir.path(),
            "changes.json",
            serde_json::json!({
                "type": "record_review", "verdict": "changes_requested",
                "checkpoint_identity": "sha256:first", "findings": ["Handle the empty case"]
            }),
        ),
    );
    apply(
        "worker",
        "worker-1",
        &worker_token,
        "4",
        write_action(
            dir.path(),
            "second.json",
            serde_json::json!({
                "type": "submit_checkpoint", "checkpoint": {
                    "kind": "artifact", "identity": "sha256:second", "verification": ["tests passed"]
                }
            }),
        ),
    );
    apply(
        "reviewer",
        "reviewer-1",
        &reviewer_token,
        "5",
        write_action(
            dir.path(),
            "approve.json",
            serde_json::json!({
                "type": "record_review", "verdict": "approved",
                "checkpoint_identity": "sha256:second", "findings": []
            }),
        ),
    );
    apply(
        "worker",
        "worker-1",
        &worker_token,
        "6",
        write_action(
            dir.path(),
            "finalize.json",
            serde_json::json!({
                "type": "finalize"
            }),
        ),
    );

    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["status"], "done");
    assert_eq!(baton["assignee"], "none");
    assert_eq!(baton["checkpoint"]["identity"], "sha256:second");
    assert_eq!(baton["review"]["checkpoint_identity"], "sha256:second");
}

#[test]
fn human_decision_resumes_declared_owner() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);

    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "pause.json",
        serde_json::json!({
            "type": "request_human_decision", "question": "Which API should win?",
            "evidence": ["Both variants pass tests"], "options": ["Keep A", "Keep B"],
            "contact_role": "reviewer", "resume_status": "reviewing", "resume_assignee": "reviewer"
        }),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "wrong.json",
        serde_json::json!({
            "type": "resume_human_decision", "answer": "Keep A"
        }),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"wrong_contact""#));
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "resume.json",
        serde_json::json!({
            "type": "resume_human_decision", "answer": "Keep A"
        }),
    )
    .success();

    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["status"], "reviewing");
    assert_eq!(baton["assignee"], "reviewer");
    assert_eq!(baton["human_decision"]["answer"], "Keep A");
}

#[test]
fn required_publication_blocks_done_until_synchronized() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "publication.json",
        serde_json::json!({
            "type": "record_publication", "required": true, "desired_revision": 1,
            "published_revision": null, "refs": [{"kind": "site", "value": "pending"}]
        }),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "checkpoint.json",
        serde_json::json!({
            "type": "submit_checkpoint", "checkpoint": {
                "kind": "artifact", "identity": "sha256:ready", "verification": ["tests passed"]
            }
        }),
    )
    .success();
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "approval.json",
        serde_json::json!({
            "type": "record_review", "verdict": "approved",
            "checkpoint_identity": "sha256:ready", "findings": []
        }),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        5,
        "blocked.json",
        serde_json::json!({
            "type": "finalize"
        }),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"publication_stale""#));
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        5,
        "published.json",
        serde_json::json!({
            "type": "record_publication", "required": true, "desired_revision": 1,
            "published_revision": 1, "refs": [{"kind": "site", "value": "site:abc"}]
        }),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        6,
        "done.json",
        serde_json::json!({
            "type": "finalize"
        }),
    )
    .success();
}

#[test]
fn local_watcher_wakes_the_reviewer_after_a_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);

    let mut waiter = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
        .args([
            "wait",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "reviewer",
            "--session-id",
            "reviewer-1",
            "--token",
            &reviewer,
            "--after-revision",
            "2",
            "--poll-interval-ms",
            "1000",
            "--timeout-ms",
            "5000",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert!(waiter.try_wait().unwrap().is_none());

    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "wake.json",
        serde_json::json!({
            "type": "submit_checkpoint", "checkpoint": {
                "kind": "artifact", "identity": "sha256:wake", "verification": ["tests passed"]
            }
        }),
    )
    .success();
    let output = waiter.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let baton: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(baton["status"], "reviewing");
    assert_eq!(baton["assignee"], "reviewer");
    assert_eq!(baton["revision"], 3);
}

#[test]
fn recovery_fences_old_sessions_and_preserves_evidence() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let _reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "checkpoint.json",
        serde_json::json!({
            "type": "submit_checkpoint", "checkpoint": {
                "kind": "artifact", "identity": "sha256:recover", "verification": ["tests passed"]
            }
        }),
    )
    .success();
    std::fs::write(dir.path().join("baton.json"), b"corrupt\n").unwrap();
    command()
        .args(["read", "--run-dir", dir.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_baton""#));

    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "3",
        ])
        .assert()
        .success();
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["revision"], 4);
    assert_eq!(baton["checkpoint"]["identity"], "sha256:recover");
    assert!(baton["participants"]["worker"]["claim"].is_null());
    assert!(baton["participants"]["reviewer"]["claim"].is_null());
    assert_eq!(baton["recovery"]["from_revision"], 3);
}

#[test]
fn recovery_refuses_to_reopen_a_terminal_run() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        1,
        "abandon.json",
        serde_json::json!({
            "type": "abandon", "reason": "Objective was cancelled"
        }),
    )
    .success();

    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "1",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"terminal_state""#));

    std::fs::write(dir.path().join("baton.json"), b"corrupt\n").unwrap();
    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "1",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"terminal_state""#));
    let terminal: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.path().join("history/00000000000000000002.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(terminal["status"], "abandoned");
}

#[test]
fn recovery_restores_a_missing_baton_from_history() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let _worker = claim_role(dir.path(), "worker", "worker-1", 0);
    std::fs::remove_file(dir.path().join("baton.json")).unwrap();

    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "1",
        ])
        .assert()
        .success();
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["status"], "working");
    assert_eq!(baton["revision"], 2);
    assert!(baton["participants"]["worker"]["claim"].is_null());
}

#[test]
fn huge_lease_is_rejected_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    command()
        .args([
            "claim",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-1",
            "--lease-seconds",
            "9223372036854775807",
            "--expected-revision",
            "0",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(r#""error":"invalid_input""#));
}

#[test]
fn one_second_wait_renews_at_most_once_before_timeout() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let reviewer = command()
        .args([
            "claim",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "reviewer",
            "--session-id",
            "reviewer-1",
            "--lease-seconds",
            "1",
            "--expected-revision",
            "0",
        ])
        .output()
        .unwrap();
    assert!(reviewer.status.success());
    let reviewer: serde_json::Value = serde_json::from_slice(&reviewer.stdout).unwrap();
    let token = reviewer["token"].as_str().unwrap();
    let mut waiter = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
        .args([
            "wait",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "reviewer",
            "--session-id",
            "reviewer-1",
            "--token",
            token,
            "--after-revision",
            "1",
            "--poll-interval-ms",
            "25",
            "--timeout-ms",
            "250",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while waiter.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if waiter.try_wait().unwrap().is_none() {
        waiter.kill().unwrap();
    }
    let output = waiter.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains(r#""error":"timeout""#));
    assert!(
        std::fs::read_dir(dir.path().join("history"))
            .unwrap()
            .count()
            <= 3
    );
}

#[test]
fn publication_cannot_regress_to_unreported_or_optional() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        1,
        "published.json",
        serde_json::json!({
            "type": "record_publication", "required": true, "desired_revision": 7,
            "published_revision": 7, "refs": []
        }),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "regress-null.json",
        serde_json::json!({
            "type": "record_publication", "required": true, "desired_revision": 7,
            "published_revision": null, "refs": []
        }),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"publication_regression""#,
    ));
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "disarm.json",
        serde_json::json!({
            "type": "record_publication", "required": false, "desired_revision": 7,
            "published_revision": 7, "refs": []
        }),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"publication_regression""#,
    ));
}

#[test]
fn failed_history_write_does_not_advance_the_baton() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    std::fs::write(
        dir.path().join("history/00000000000000000002.json"),
        b"occupied",
    )
    .unwrap();
    command()
        .args([
            "heartbeat",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-1",
            "--token",
            &worker,
            "--lease-seconds",
            "30",
            "--expected-revision",
            "1",
        ])
        .assert()
        .failure();
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["revision"], 1);
}
