use assert_cmd::Command;
use predicates::prelude::*;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
}

#[test]
fn version_and_probe_report_the_installation_contract() {
    command()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("dvandva-v4 0.1.0"));

    let output = command()
        .args(["probe", "--expected-schema", "dvandva.run.v1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let probe: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(probe["package"], "dvandva-v4");
    assert_eq!(probe["version"], "0.1.0");
    assert_eq!(probe["schema"], "dvandva.run.v1");
    assert_eq!(probe["compatible"], true);
}

fn git(workspace: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn start_role(
    workspace: &std::path::Path,
    runs: &std::path::Path,
    credentials: &std::path::Path,
    role: &str,
    session: &str,
    current_harness: &str,
    peer_harness: &str,
) -> serde_json::Value {
    let output = command()
        .args([
            "role",
            "start",
            "--workspace",
            workspace.to_str().unwrap(),
            "--runs-dir",
            runs.to_str().unwrap(),
            "--credentials-root",
            credentials.to_str().unwrap(),
            "--role",
            role,
            "--session-id",
            session,
            "--current-harness",
            current_harness,
            "--peer-harness",
            peer_harness,
            "--objective",
            "Implement DEF-123",
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
    serde_json::from_slice(&output.stdout).unwrap()
}

struct Flow<'a> {
    root: &'a std::path::Path,
    run_dir: &'a std::path::Path,
    credentials: &'a std::path::Path,
}

impl Flow<'_> {
    fn apply(
        &self,
        role: &str,
        session: &str,
        revision: u64,
        name: &str,
        action: serde_json::Value,
    ) -> serde_json::Value {
        let action_path = self.root.join(name);
        std::fs::write(&action_path, serde_json::to_vec_pretty(&action).unwrap()).unwrap();
        let output = command()
            .args([
                "role",
                "apply",
                "--run-dir",
                self.run_dir.to_str().unwrap(),
                "--role",
                role,
                "--session-id",
                session,
                "--expected-revision",
                &revision.to_string(),
                "--credentials-root",
                self.credentials.to_str().unwrap(),
                "--action",
                action_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

#[test]
fn skill_safe_commands_complete_the_review_revision_and_publication_loop() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let runs = root.path().join("state/runs");
    let credentials = root.path().join("state/credentials");
    std::fs::create_dir(&workspace).unwrap();
    git(&workspace, &["init", "--quiet"]);
    git(
        &workspace,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:axatbhardwaj/Dvandva.git",
        ],
    );

    let worker = start_role(
        &workspace,
        &runs,
        &credentials,
        "worker",
        "worker-session",
        "codex",
        "claude",
    );
    let reviewer = start_role(
        &workspace,
        &runs,
        &credentials,
        "reviewer",
        "reviewer-session",
        "claude",
        "codex",
    );
    assert_eq!(worker["outcome"], "started");
    assert_eq!(reviewer["outcome"], "started");
    assert_eq!(worker["run_id"], reviewer["run_id"]);
    let run_dir = runs.join(worker["run_id"].as_str().unwrap());
    let flow = Flow {
        root: root.path(),
        run_dir: &run_dir,
        credentials: &credentials,
    };

    let checkpoint_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let checkpoint_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let reviewing_a = flow.apply(
        "worker",
        "worker-session",
        2,
        "checkpoint-a.json",
        serde_json::json!({
            "type": "submit_checkpoint",
            "checkpoint": {
                "kind": "git",
                "identity": checkpoint_a,
                "verification": ["cargo test"]
            }
        }),
    );
    assert_eq!(reviewing_a["status"], "reviewing");

    let revising = flow.apply(
        "reviewer",
        "reviewer-session",
        3,
        "request-changes.json",
        serde_json::json!({
            "type": "record_review",
            "verdict": "changes_requested",
            "checkpoint_identity": checkpoint_a,
            "findings": ["Add the missing contention test"]
        }),
    );
    assert_eq!(revising["status"], "revising");

    let reviewing_b = flow.apply(
        "worker",
        "worker-session",
        4,
        "checkpoint-b.json",
        serde_json::json!({
            "type": "submit_checkpoint",
            "checkpoint": {
                "kind": "git",
                "identity": checkpoint_b,
                "verification": ["cargo test", "contention test"]
            }
        }),
    );
    assert_eq!(reviewing_b["checkpoint"]["identity"], checkpoint_b);

    let approved = flow.apply(
        "reviewer",
        "reviewer-session",
        5,
        "approve.json",
        serde_json::json!({
            "type": "record_review",
            "verdict": "approved",
            "checkpoint_identity": checkpoint_b,
            "findings": []
        }),
    );
    assert_eq!(approved["status"], "finalizing");

    let published = flow.apply(
        "worker",
        "worker-session",
        6,
        "publication.json",
        serde_json::json!({
            "type": "record_publication",
            "required": true,
            "desired_revision": 6,
            "published_revision": 6,
            "refs": [{"kind": "explainer", "value": "https://example.test/run"}]
        }),
    );
    assert_eq!(published["publication"]["published_revision"], 6);

    let done = flow.apply(
        "worker",
        "worker-session",
        7,
        "finalize.json",
        serde_json::json!({"type": "finalize"}),
    );
    assert_eq!(done["status"], "done");
    assert_eq!(done["revision"], 8);
    assert_eq!(done["checkpoint"]["identity"], checkpoint_b);
    assert_eq!(done["review"]["checkpoint_identity"], checkpoint_b);

    for (role, session) in [
        ("worker", "worker-session"),
        ("reviewer", "reviewer-session"),
    ] {
        command()
            .args([
                "role",
                "wait",
                "--run-dir",
                run_dir.to_str().unwrap(),
                "--role",
                role,
                "--session-id",
                session,
                "--credentials-root",
                credentials.to_str().unwrap(),
                "--after-revision",
                "7",
                "--timeout-ms",
                "500",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains(r#""status": "done""#));
    }

    let credential_text = std::fs::read_to_string(
        credentials
            .join("worker-session")
            .join(worker["run_id"].as_str().unwrap())
            .join("worker.json"),
    )
    .unwrap();
    let token = serde_json::from_str::<serde_json::Value>(&credential_text).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!std::fs::read_to_string(run_dir.join("baton.json"))
        .unwrap()
        .contains(&token));
    for entry in std::fs::read_dir(run_dir.join("history")).unwrap() {
        assert!(!std::fs::read_to_string(entry.unwrap().path())
            .unwrap()
            .contains(&token));
    }
}

#[test]
fn explicit_role_reversal_binds_claude_as_worker_and_codex_as_reviewer() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let runs = root.path().join("state/runs");
    let credentials = root.path().join("state/credentials");
    std::fs::create_dir(&workspace).unwrap();
    git(&workspace, &["init", "--quiet"]);
    git(
        &workspace,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:axatbhardwaj/Dvandva.git",
        ],
    );

    let worker = start_role(
        &workspace,
        &runs,
        &credentials,
        "worker",
        "claude-worker",
        "claude",
        "codex",
    );
    let reviewer = start_role(
        &workspace,
        &runs,
        &credentials,
        "reviewer",
        "codex-reviewer",
        "codex",
        "claude",
    );
    assert_eq!(worker["run_id"], reviewer["run_id"]);
    let run_dir = runs.join(worker["run_id"].as_str().unwrap());
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["participants"]["worker"]["harness"], "claude");
    assert_eq!(baton["participants"]["reviewer"]["harness"], "codex");
}

#[test]
fn explicit_run_id_resolves_an_ambiguous_role_start() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let runs = root.path().join("state/runs");
    let credentials = root.path().join("state/credentials");
    std::fs::create_dir(&workspace).unwrap();
    git(&workspace, &["init", "--quiet"]);
    git(
        &workspace,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:axatbhardwaj/Dvandva.git",
        ],
    );

    let first = start_role(
        &workspace,
        &runs,
        &credentials,
        "worker",
        "worker-a",
        "codex",
        "claude",
    );
    let second = command()
        .args([
            "role",
            "start",
            "--workspace",
            workspace.to_str().unwrap(),
            "--runs-dir",
            runs.to_str().unwrap(),
            "--credentials-root",
            credentials.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-b",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Implement DEF-123",
            "--task-reference",
            "DEF-123",
            "--new-run",
        ])
        .output()
        .unwrap();
    assert!(second.status.success());

    command()
        .args([
            "role",
            "start",
            "--workspace",
            workspace.to_str().unwrap(),
            "--runs-dir",
            runs.to_str().unwrap(),
            "--credentials-root",
            credentials.to_str().unwrap(),
            "--role",
            "reviewer",
            "--session-id",
            "reviewer",
            "--current-harness",
            "claude",
            "--peer-harness",
            "codex",
            "--objective",
            "Implement DEF-123",
            "--task-reference",
            "DEF-123",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""outcome": "ambiguous""#));

    let first_run = first["run_id"].as_str().unwrap();
    command()
        .args([
            "role",
            "start",
            "--workspace",
            workspace.to_str().unwrap(),
            "--runs-dir",
            runs.to_str().unwrap(),
            "--credentials-root",
            credentials.to_str().unwrap(),
            "--role",
            "reviewer",
            "--session-id",
            "reviewer",
            "--current-harness",
            "claude",
            "--peer-harness",
            "codex",
            "--objective",
            "Implement DEF-123",
            "--task-reference",
            "DEF-123",
            "--run-id",
            first_run,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""outcome": "started""#))
        .stdout(predicate::str::contains(first_run));
}
