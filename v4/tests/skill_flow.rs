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
        .stdout(predicate::str::contains("dvandva-v4 0.1.1"));

    let output = command()
        .args([
            "probe",
            "--expected-schema",
            "dvandva.run.v2",
            "--expected-role-api",
            "2",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let probe: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(probe["package"], "dvandva-v4");
    assert_eq!(probe["version"], "0.1.1");
    assert_eq!(probe["write_schema"], "dvandva.run.v2");
    assert_eq!(probe["role_api"], 2);
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
            "--api",
            "2",
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
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
                "--api",
                "2",
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

    fn approve_explainer(&self, revision: u64, site_version: &str) -> serde_json::Value {
        let baton: serde_json::Value =
            serde_json::from_slice(&std::fs::read(self.run_dir.join("baton.json")).unwrap())
                .unwrap();
        let obligation = baton["publication_binding"]["obligation"].clone();
        let published = self.apply(
            "worker",
            "worker-session",
            revision,
            &format!("publish-{site_version}.json"),
            serde_json::json!({
                "type": "record_explainer_publication", "obligation": obligation,
                "source_digest": "a".repeat(64), "site_id": "site-run",
                "site_version": site_version,
                "url": format!("https://sites.openai.test/site-run/{site_version}"),
                "channel": "codex_sites", "access": "owner_only"
            }),
        );
        let deployment = published["publication_binding"]["deployment"].clone();
        self.apply(
            "reviewer",
            "reviewer-session",
            revision + 1,
            &format!("review-{site_version}.json"),
            serde_json::json!({
                "type": "record_explainer_review", "obligation": obligation,
                "source_digest": deployment["source_digest"],
                "site_id": deployment["site_id"],
                "site_version": deployment["site_version"], "url": deployment["url"],
                "verdict": "approved", "findings": []
            }),
        )
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
    flow.approve_explainer(2, "deployment-1");
    let reviewing_a = flow.apply(
        "worker",
        "worker-session",
        4,
        "checkpoint-a.json",
        serde_json::json!({
            "type": "submit_checkpoint",
            "checkpoint": {
                "kind": "git",
                "identity": checkpoint_a,
                "deliverables": [{
                    "id": "implementation",
                    "artifacts": [{"kind": "commit", "value": checkpoint_a}]
                }],
                "verification": ["cargo test"]
            }
        }),
    );
    assert_eq!(reviewing_a["status"], "reviewing");
    assert!(reviewing_a["next_actions"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("request_checkpoint_supersession")));
    assert!(reviewing_a["next_actions"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("publish_explainer")));

    flow.approve_explainer(5, "deployment-2");

    let revising = flow.apply(
        "reviewer",
        "reviewer-session",
        7,
        "request-changes.json",
        serde_json::json!({
            "type": "record_review",
            "verdict": "changes_requested",
            "checkpoint_identity": checkpoint_a,
            "manifest_digest": reviewing_a["checkpoint"]["manifest_digest"],
            "scope_revision": reviewing_a["checkpoint"]["scope_revision"],
            "findings": ["Add the missing contention test"]
        }),
    );
    assert_eq!(revising["status"], "revising");

    flow.approve_explainer(8, "deployment-3");

    let reviewing_b = flow.apply(
        "worker",
        "worker-session",
        10,
        "checkpoint-b.json",
        serde_json::json!({
            "type": "submit_checkpoint",
            "checkpoint": {
                "kind": "git",
                "identity": checkpoint_b,
                "deliverables": [{
                    "id": "implementation",
                    "artifacts": [{"kind": "commit", "value": checkpoint_b}]
                }],
                "verification": ["cargo test", "contention test"]
            }
        }),
    );
    assert_eq!(reviewing_b["checkpoint"]["identity"], checkpoint_b);

    flow.approve_explainer(11, "deployment-4");

    let approved = flow.apply(
        "reviewer",
        "reviewer-session",
        13,
        "approve.json",
        serde_json::json!({
            "type": "record_review",
            "verdict": "approved",
            "checkpoint_identity": checkpoint_b,
            "manifest_digest": reviewing_b["checkpoint"]["manifest_digest"],
            "scope_revision": reviewing_b["checkpoint"]["scope_revision"],
            "findings": []
        }),
    );
    assert_eq!(approved["status"], "finalizing");

    flow.approve_explainer(14, "deployment-5");

    let done = flow.apply(
        "worker",
        "worker-session",
        16,
        "finalize.json",
        serde_json::json!({"type": "finalize"}),
    );
    assert_eq!(done["status"], "done");
    assert_eq!(done["next_actions"], serde_json::json!(["stop"]));
    assert_eq!(done["revision"], 17);
    assert_eq!(done["checkpoint"]["identity"], checkpoint_b);
    assert_eq!(done["review"]["checkpoint_identity"], checkpoint_b);

    for (role, session) in [
        ("worker", "worker-session"),
        ("reviewer", "reviewer-session"),
    ] {
        let output = command()
            .args([
                "role",
                "wait",
                "--api",
                "2",
                "--run-dir",
                run_dir.to_str().unwrap(),
                "--role",
                role,
                "--session-id",
                session,
                "--credentials-root",
                credentials.to_str().unwrap(),
                "--after-revision",
                "16",
                "--timeout-ms",
                "500",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        let waited: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(waited["status"], "done");
        assert_eq!(waited["next_actions"], serde_json::json!(["stop"]));
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
    assert_eq!(baton["participants"]["worker"]["harness"], "Claude");
    assert_eq!(baton["participants"]["reviewer"]["harness"], "Codex");
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
            "worker-b",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Implement DEF-123",
            "--task-reference",
            "DEF-123",
            "--required-deliverable",
            "implementation=Implement DEF-123",
            "--new-run",
        ])
        .output()
        .unwrap();
    assert!(second.status.success());

    command()
        .args([
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""outcome": "ambiguous""#));

    let first_run = first["run_id"].as_str().unwrap();
    let mismatch = command()
        .args([
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
            "reviewer",
            "--session-id",
            "reviewer",
            "--current-harness",
            "claude",
            "--peer-harness",
            "codex",
            "--objective",
            "Review the mobile app tech spec",
            "--task-reference",
            "https://app.notion.com/p/Mobile-App-Tech-Spec",
            "--required-deliverable",
            "implementation=Implement DEF-123",
            "--run-id",
            first_run,
        ])
        .output()
        .unwrap();
    assert!(mismatch.status.success());
    let mismatch: serde_json::Value = serde_json::from_slice(&mismatch.stdout).unwrap();
    assert_eq!(mismatch["outcome"], "scope_mismatch");
    assert_eq!(mismatch["candidates"][0]["run_id"], first_run);
    assert_eq!(
        mismatch["candidates"][0]["objective"]["summary"],
        "Implement DEF-123"
    );
    assert_eq!(mismatch["candidates"][0]["scope_revision"], 0);
}

#[test]
fn a_live_worker_run_blocks_silent_duplicate_creation() {
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
            "worker-b",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Implement DEF-123",
            "--task-reference",
            "DEF-123",
            "--required-deliverable",
            "implementation=Implement DEF-123",
        ])
        .output()
        .unwrap();
    assert!(second.status.success());
    let result: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(result["outcome"], "busy");
    assert_eq!(result["candidates"][0]["run_id"], first["run_id"]);
    assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 1);
}
