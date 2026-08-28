use std::os::unix::fs::PermissionsExt;

use assert_cmd::Command;
use dvandva_v4::{claim::Role, credential};
use sha2::{Digest, Sha256};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
}

fn initialize_workspace(root: &std::path::Path, origin: &str) -> std::path::PathBuf {
    let workspace = root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec!["remote", "add", "origin", origin],
    ] {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&workspace)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    workspace
}

fn create_worker_run(
    workspace: &std::path::Path,
    runs: &std::path::Path,
    credentials: &std::path::Path,
    objective: &str,
    task_reference: &str,
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
            "worker",
            "--session-id",
            "worker-session",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            objective,
            "--task-reference",
            task_reference,
            "--required-deliverable",
            "implementation=Canonical implementation",
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

fn write_legacy_run(run_dir: &std::path::Path) {
    std::fs::create_dir_all(run_dir.join("history")).unwrap();
    let baton = serde_json::json!({
        "schema": "dvandva.run.v1", "run_id": "legacy-run",
        "objective": {"summary": "Migrate safely", "refs": []},
        "workspace": {"repository_id": "example.com/team/project", "origin": "https://example.com/team/project.git", "worktree": null},
        "task": {"reference": "DEF-123", "summary": "Migrate safely"},
        "participants": {
            "worker": {"harness": "codex", "claim": null},
            "reviewer": {"harness": "claude", "claim": null}
        },
        "status": "working", "assignee": "worker", "revision": 0,
        "checkpoint": null, "review": null,
        "publication": {"required": true, "desired_revision": 0, "published_revision": null, "refs": []},
        "human_decision": null, "predecessor_run_id": null, "terminal": null, "recovery": null
    });
    let bytes = serde_json::to_vec_pretty(&baton).unwrap();
    std::fs::write(run_dir.join("history/00000000000000000000.json"), &bytes).unwrap();
    std::fs::write(run_dir.join("baton.json"), &bytes).unwrap();
}

#[test]
fn migration_fences_every_ordinary_v1_role_mutation() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("legacy-run");
    write_legacy_run(&run_dir);
    let action = root.path().join("action.json");
    std::fs::write(&action, br#"{"type":"abandon","reason":"no"}"#).unwrap();

    let cases = [
        vec![
            "claim",
            "--role",
            "worker",
            "--session-id",
            "s",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
        ],
        vec![
            "reclaim",
            "--role",
            "worker",
            "--session-id",
            "s",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
        ],
        vec![
            "heartbeat",
            "--role",
            "worker",
            "--session-id",
            "s",
            "--token",
            "stale",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
        ],
        vec![
            "apply",
            "--role",
            "worker",
            "--session-id",
            "s",
            "--token",
            "stale",
            "--expected-revision",
            "0",
            "--action",
            action.to_str().unwrap(),
        ],
        vec![
            "wait",
            "--role",
            "worker",
            "--session-id",
            "s",
            "--token",
            "stale",
            "--after-revision",
            "0",
            "--poll-interval-ms",
            "1",
            "--timeout-ms",
            "1",
        ],
    ];
    for case in cases {
        let mut args = case;
        args.extend(["--run-dir", run_dir.to_str().unwrap()]);
        command()
            .args(args)
            .assert()
            .failure()
            .stderr(predicates::str::contains("migration_required"));
    }
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["revision"], 0);
}

#[test]
fn migration_role_api_is_checked_before_any_run_io() {
    let root = tempfile::tempdir().unwrap();
    command()
        .args([
            "role",
            "read",
            "--api",
            "1",
            "--run-dir",
            root.path().join("missing").to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "session",
            "--credentials-root",
            root.path().join("credentials").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("role_api_mismatch"));
    assert!(!root.path().join("credentials").exists());
}

#[test]
fn migration_exact_v1_start_returns_upgrade_metadata_without_a_credential() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&workspace)
            .status()
            .unwrap();
        assert!(status.success());
    };
    git(&["init", "--quiet"]);
    git(&[
        "remote",
        "add",
        "origin",
        "https://example.com/team/project.git",
    ]);
    let runs = root.path().join("runs");
    let run_dir = runs.join("legacy-run");
    write_legacy_run(&run_dir);
    let corrupt = runs.join("unrelated-corrupt");
    std::fs::create_dir_all(&corrupt).unwrap();
    std::fs::write(corrupt.join("baton.json"), b"not json").unwrap();
    let credentials = root.path().join("credentials");

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
            "worker",
            "--session-id",
            "worker-new",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Migrate safely",
            "--task-reference",
            "DEF-123",
            "--run-id",
            "legacy-run",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["outcome"], "upgrade_required");
    assert_eq!(result["run_id"], "legacy-run");
    assert_eq!(result["from_schema"], "dvandva.run.v1");
    assert_eq!(result["next_action"], "upgrade_protocol");
    assert_eq!(result["task_reference"], "DEF-123");
    assert_eq!(result["task_summary"], "Migrate safely");
    assert!(result.get("credential").is_none());
    assert!(!credentials.exists());
}

#[test]
fn migration_exact_v1_scope_mismatch_precedes_upgrade() {
    let root = tempfile::tempdir().unwrap();
    let workspace = initialize_workspace(root.path(), "https://example.com/team/project.git");
    let runs = root.path().join("runs");
    let run_dir = runs.join("legacy-run");
    write_legacy_run(&run_dir);
    let corrupt = runs.join("unrelated-corrupt");
    std::fs::create_dir_all(&corrupt).unwrap();
    std::fs::write(corrupt.join("baton.json"), b"not json").unwrap();
    let credentials = root.path().join("credentials");

    let mismatches = [
        vec!["--objective", "Different objective"],
        vec!["--objective-ref", "ticket=https://tracker.test/DEF-999"],
        vec!["--task-reference", "DEF-999"],
        vec!["--required-deliverable", "implementation=New output"],
    ];
    for (index, mismatch) in mismatches.into_iter().enumerate() {
        let mut process = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"));
        process
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
                &format!("worker-{index}"),
                "--current-harness",
                "codex",
                "--peer-harness",
                "claude",
                "--run-id",
                "legacy-run",
            ])
            .args(mismatch);
        let output = process.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["outcome"], "scope_mismatch");
        assert_eq!(result["candidates"][0]["task_reference"], "DEF-123");
        assert!(result.get("requested_scope").is_some());
    }
    let installed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    assert_eq!(installed["revision"], 0);
    assert!(!credentials.exists());
}

#[test]
fn migration_exact_v1_accepts_the_deterministic_default_scope() {
    let root = tempfile::tempdir().unwrap();
    let workspace = initialize_workspace(root.path(), "https://example.com/team/project.git");
    let runs = root.path().join("runs");
    write_legacy_run(&runs.join("legacy-run"));
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
            root.path().join("credentials").to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--run-id",
            "legacy-run",
            "--objective",
            "Migrate safely",
            "--task-reference",
            "DEF-123",
            "--required-deliverable",
            "legacy_objective=Migrate safely",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["outcome"], "upgrade_required");
    assert_eq!(result["scope_deliverables"][0]["id"], "legacy_objective");
    assert_eq!(
        result["scope_deliverables"][0]["description"],
        "Migrate safely"
    );
}

#[test]
fn migration_broad_start_is_ambiguous_across_multiple_upgrade_candidates() {
    let root = tempfile::tempdir().unwrap();
    let workspace = initialize_workspace(root.path(), "https://example.com/team/project.git");
    let runs = root.path().join("runs");
    write_legacy_run(&runs.join("legacy-run"));
    let second = runs.join("legacy-second");
    write_legacy_run(&second);
    for path in [
        second.join("baton.json"),
        second.join("history/00000000000000000000.json"),
    ] {
        let mut baton: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        baton["run_id"] = serde_json::json!("legacy-second");
        std::fs::write(path, serde_json::to_vec_pretty(&baton).unwrap()).unwrap();
    }
    let credentials = root.path().join("credentials");
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
            "worker",
            "--session-id",
            "worker",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Migrate safely",
            "--task-reference",
            "DEF-123",
            "--required-deliverable",
            "legacy_objective=Migrate safely",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["outcome"], "ambiguous");
    assert_eq!(result["candidates"].as_array().unwrap().len(), 2);
    assert!(!credentials.exists());
    for run_id in ["legacy-run", "legacy-second"] {
        let baton: serde_json::Value =
            serde_json::from_slice(&std::fs::read(runs.join(run_id).join("baton.json")).unwrap())
                .unwrap();
        assert_eq!(baton["revision"], 0);
    }
}

#[test]
fn migration_discovery_normalizes_v1_storage_and_v2_resume_harnesses() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let git = |args: &[&str]| {
        assert!(std::process::Command::new("git")
            .args(args)
            .current_dir(&workspace)
            .status()
            .unwrap()
            .success());
    };
    git(&["init", "--quiet"]);
    git(&[
        "remote",
        "add",
        "origin",
        "https://example.com/team/project.git",
    ]);
    let runs = root.path().join("runs");
    let legacy_dir = runs.join("legacy-run");
    write_legacy_run(&legacy_dir);
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(legacy_dir.join("baton.json")).unwrap()).unwrap();
    legacy["participants"]["worker"]["harness"] = serde_json::json!(" CoDeX ");
    let legacy_bytes = serde_json::to_vec_pretty(&legacy).unwrap();
    std::fs::write(legacy_dir.join("baton.json"), &legacy_bytes).unwrap();
    std::fs::write(
        legacy_dir.join("history/00000000000000000000.json"),
        &legacy_bytes,
    )
    .unwrap();
    let credentials = root.path().join("credentials");

    let legacy_result = command()
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
            "legacy-s",
            "--current-harness",
            " cOdEx ",
            "--peer-harness",
            " CLAUDE ",
            "--objective",
            "Migrate safely",
            "--task-reference",
            "DEF-123",
            "--run-id",
            "legacy-run",
        ])
        .output()
        .unwrap();
    assert!(
        legacy_result.status.success(),
        "{}",
        String::from_utf8_lossy(&legacy_result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&legacy_result.stdout).unwrap()["outcome"],
        "upgrade_required"
    );

    let v2_dir = runs.join("run-v2");
    command()
        .args([
            "init",
            "--run-dir",
            v2_dir.to_str().unwrap(),
            "--run-id",
            "run-v2",
            "--objective",
            "Resume safely",
            "--worker",
            "codex",
            "--reviewer",
            "claude",
            "--repository-id",
            "example.com/team/project",
            "--task-reference",
            "DEF-456",
            "--required-deliverable",
            "implementation=Resume safely",
        ])
        .assert()
        .success();
    let v2_result = command()
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
            "v2-s",
            "--current-harness",
            " CODEX ",
            "--peer-harness",
            " claude ",
            "--objective",
            "Resume safely",
            "--task-reference",
            "DEF-456",
            "--run-id",
            "run-v2",
        ])
        .output()
        .unwrap();
    assert!(
        v2_result.status.success(),
        "{}",
        String::from_utf8_lossy(&v2_result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&v2_result.stdout).unwrap()["outcome"],
        "started"
    );
}

#[test]
fn taskless_legacy_upgrade_is_discoverable_and_exactly_joinable() {
    let root = tempfile::tempdir().unwrap();
    let workspace = initialize_workspace(root.path(), "https://example.com/team/project.git");
    let runs = root.path().join("runs");
    let run_dir = runs.join("legacy-run");
    write_legacy_run(&run_dir);
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    legacy["task"] = serde_json::Value::Null;
    let bytes = serde_json::to_vec_pretty(&legacy).unwrap();
    std::fs::write(run_dir.join("baton.json"), &bytes).unwrap();
    std::fs::write(run_dir.join("history/00000000000000000000.json"), &bytes).unwrap();
    let credentials = root.path().join("credentials");

    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "upgrade-session",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();

    let joined = command()
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
            "worker-session",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Migrate safely",
            "--run-id",
            "legacy-run",
        ])
        .output()
        .unwrap();
    assert!(
        joined.status.success(),
        "{}",
        String::from_utf8_lossy(&joined.stderr)
    );
    let joined: serde_json::Value = serde_json::from_slice(&joined.stdout).unwrap();
    assert_eq!(joined["outcome"], "started");
    assert_eq!(joined["run_id"], "legacy-run");
    assert!(joined["task"].is_null());

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
            "worker",
            "--session-id",
            "other-session",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--run-id",
            "legacy-run",
            "--task-reference",
            "DEF-123",
        ])
        .output()
        .unwrap();
    assert!(mismatch.status.success());
    let mismatch: serde_json::Value = serde_json::from_slice(&mismatch.stdout).unwrap();
    assert_eq!(mismatch["outcome"], "scope_mismatch");
    assert_eq!(
        mismatch["candidates"][0]["task_reference"],
        serde_json::Value::Null
    );
    assert_eq!(mismatch["candidates"][0]["task_summary"], "Migrate safely");
}

#[test]
fn migration_upgrade_live_own_claim_is_busy_even_with_matching_private_credential() {
    for credential_token in [None, Some("wrong-token"), Some("valid-token")] {
        let root = tempfile::tempdir().unwrap();
        let run_dir = root.path().join("run");
        write_legacy_run(&run_dir);
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
        legacy["participants"]["worker"]["claim"] = serde_json::json!({
            "session_id": "same-session", "epoch": 7,
            "token_digest": format!("{:x}", Sha256::digest(b"valid-token")),
            "lease_expires_at": "2999-01-01T00:00:00Z", "lease_seconds": 300
        });
        let bytes = serde_json::to_vec_pretty(&legacy).unwrap();
        std::fs::write(run_dir.join("baton.json"), &bytes).unwrap();
        std::fs::write(run_dir.join("history/00000000000000000000.json"), &bytes).unwrap();
        let credentials = root.path().join("credentials");
        if let Some(token) = credential_token {
            let stored = credential::Credential {
                run_dir: std::fs::canonicalize(&run_dir).unwrap(),
                run_id: "legacy-run".to_owned(),
                role: Role::Worker,
                session_id: "same-session".to_owned(),
                epoch: 7,
                token: token.to_owned(),
            };
            credential::store(&credentials, &stored).unwrap();
        }

        let result = command()
            .args([
                "role",
                "upgrade",
                "--api",
                "2",
                "--run-dir",
                run_dir.to_str().unwrap(),
                "--role",
                "worker",
                "--session-id",
                "same-session",
                "--current-harness",
                "codex",
                "--peer-harness",
                "claude",
                "--expected-revision",
                "0",
                "--credentials-root",
                credentials.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(r#""error":"busy""#),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let current: serde_json::Value =
            serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
        assert_eq!(current["schema"], "dvandva.run.v1");
        assert_eq!(current["revision"], 0);
        assert!(!run_dir.join("history/00000000000000000001.json").exists());
    }
}

#[test]
fn migration_upgrade_reports_blank_session_and_objective_separately() {
    let root = tempfile::tempdir().unwrap();
    let session_dir = root.path().join("blank-session");
    write_legacy_run(&session_dir);
    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            session_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            " ",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--expected-revision",
            "0",
            "--credentials-root",
            root.path().join("credentials").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid_session"));

    let objective_dir = root.path().join("blank-objective");
    write_legacy_run(&objective_dir);
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(objective_dir.join("baton.json")).unwrap()).unwrap();
    legacy["objective"]["summary"] = serde_json::json!(" ");
    let bytes = serde_json::to_vec_pretty(&legacy).unwrap();
    std::fs::write(objective_dir.join("baton.json"), &bytes).unwrap();
    std::fs::write(
        objective_dir.join("history/00000000000000000000.json"),
        &bytes,
    )
    .unwrap();
    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            objective_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "session",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--expected-revision",
            "0",
            "--credentials-root",
            root.path().join("credentials").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid_objective"));
}

fn init_run(run_dir: &std::path::Path) {
    command()
        .args([
            "init",
            "--run-dir",
            run_dir.to_str().unwrap(),
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
            "--task-reference",
            "DEF-123",
            "--required-deliverable",
            "implementation=Implement DEF-123",
        ])
        .assert()
        .success();
}

fn mode(path: &std::path::Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn role_claim_keeps_the_raw_token_in_a_private_credential() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    init_run(&run_dir);

    let output = command()
        .args([
            "role",
            "claim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());

    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let credential = credentials.join("worker-session/run-a/worker.json");
    assert_eq!(result["revision"], 1);
    assert_eq!(result["epoch"], 1);
    assert_eq!(result["credential"], credential.to_str().unwrap());

    let private: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&credential).unwrap()).unwrap();
    let token = private["token"].as_str().unwrap();
    assert!(!token.is_empty());
    assert_eq!(private["session_id"], "worker-session");
    assert_eq!(private["run_id"], "run-a");
    assert_eq!(private["role"], "worker");
    assert_eq!(private["epoch"], 1);

    assert_eq!(mode(&credentials), 0o700);
    assert_eq!(mode(&credentials.join("worker-session")), 0o700);
    assert_eq!(mode(&credentials.join("worker-session/run-a")), 0o700);
    assert_eq!(mode(&credential), 0o600);

    assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(token));
    assert!(
        !String::from_utf8_lossy(&std::fs::read(run_dir.join("baton.json")).unwrap())
            .contains(token)
    );
    for entry in std::fs::read_dir(run_dir.join("history")).unwrap() {
        assert!(
            !String::from_utf8_lossy(&std::fs::read(entry.unwrap().path()).unwrap())
                .contains(token)
        );
    }
}

#[test]
fn role_apply_loads_the_private_token_without_a_cli_argument() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    init_run(&run_dir);
    command()
        .args([
            "role",
            "claim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();
    let explainer = root.path().join("explainer.html");
    std::fs::write(&explainer, b"<h1>run-a explainer</h1>").unwrap();
    let action = root.path().join("publication.json");
    std::fs::write(
        &action,
        serde_json::to_vec_pretty(&serde_json::json!({
            "type": "stage_explainer",
            "obligation": {
                "handoff_revision": 0, "kind": "run_started", "scope_revision": 0
            },
            "source_path": explainer.to_str().unwrap()
        }))
        .unwrap(),
    )
    .unwrap();

    let output = command()
        .args([
            "role",
            "apply",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--expected-revision",
            "1",
            "--credentials-root",
            credentials.to_str().unwrap(),
            "--action",
            action.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let baton: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(baton["status"], "working");
    assert_eq!(baton["assignee"], "worker");
    assert_eq!(baton["revision"], 2);
    let digest = format!("{:x}", Sha256::digest(b"<h1>run-a explainer</h1>"));
    assert_eq!(
        baton["publication_binding"]["artifact"]["source_digest"],
        digest
    );
    assert_eq!(
        baton["publication_binding"]["artifact"]["path"],
        format!("explainer/{digest}.html")
    );
    assert!(run_dir.join(format!("explainer/{digest}.html")).is_file());

    let private: serde_json::Value = serde_json::from_slice(
        &std::fs::read(credentials.join("worker-session/run-a/worker.json")).unwrap(),
    )
    .unwrap();
    let token = private["token"].as_str().unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(token));
}

#[test]
fn role_read_verifies_the_private_credential_against_the_baton() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    init_run(&run_dir);
    command()
        .args([
            "role",
            "claim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();

    let output = command()
        .args([
            "role",
            "read",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let baton: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(baton["revision"], 1);
    assert!(baton["next_actions"].is_array());
    assert!(baton["actionable"].is_boolean());
    assert!(baton["role_state"].is_string());
    assert_eq!(
        baton["participants"]["worker"]["claim"]["session_id"],
        "worker-session"
    );
}

#[test]
fn role_heartbeat_renews_without_exposing_the_token() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    init_run(&run_dir);
    command()
        .args([
            "role",
            "claim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();
    let credential = credentials.join("worker-session/run-a/worker.json");
    let private: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&credential).unwrap()).unwrap();
    let token = private["token"].as_str().unwrap();

    let output = command()
        .args([
            "role",
            "heartbeat",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "1",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["revision"],
        2
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(token));
}

#[test]
fn role_wait_returns_immediately_when_the_current_role_is_actionable() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    init_run(&run_dir);
    command()
        .args([
            "role",
            "claim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();

    let waiter = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
        .args([
            "role",
            "wait",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--credentials-root",
            credentials.to_str().unwrap(),
            "--after-revision",
            "1",
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
    command()
        .args([
            "role",
            "heartbeat",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "1",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();

    let output = waiter.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let baton: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(baton["revision"], 1);
    assert_eq!(baton["assignee"], "worker");
    assert_eq!(baton["actionable"], true);
}

#[test]
fn role_wait_keeps_blocking_when_human_escape_is_the_only_extra_legal_action() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    init_run(&run_dir);
    command()
        .args([
            "role",
            "claim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "reviewer",
            "--session-id",
            "reviewer-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();

    command()
        .args([
            "role",
            "wait",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "reviewer",
            "--session-id",
            "reviewer-session",
            "--credentials-root",
            credentials.to_str().unwrap(),
            "--after-revision",
            "1",
            "--poll-interval-ms",
            "10",
            "--timeout-ms",
            "100",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(r#""error":"timeout""#));
}

#[test]
fn role_reclaim_fences_an_expired_session_with_a_new_private_credential() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    let baton = dvandva_v4::model::RunBaton::new(
        "run-a",
        "Implement DEF-123",
        "codex",
        "claude",
        vec![dvandva_v4::model::DeliverableRequirement {
            id: "implementation".to_owned(),
            description: "Implement DEF-123".to_owned(),
        }],
    )
    .unwrap();
    let channel = dvandva_v4::store::RunChannel::open(&run_dir);
    channel.create(&baton).unwrap();
    dvandva_v4::claim::claim(&channel, Role::Worker, "expired-session", 1, 0).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1_100));

    let output = command()
        .args([
            "role",
            "reclaim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "replacement-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "1",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["revision"], 2);
    assert_eq!(result["epoch"], 2);
    let credential = credentials.join("replacement-session/run-a/worker.json");
    assert_eq!(result["credential"], credential.to_str().unwrap());
    assert_eq!(mode(&credential), 0o600);

    let private: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&credential).unwrap()).unwrap();
    let token = private["token"].as_str().unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
    let current: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    assert_eq!(
        current["participants"]["worker"]["claim"]["session_id"],
        "replacement-session"
    );
    assert_eq!(current["participants"]["worker"]["claim"]["epoch"], 2);
}

#[test]
fn unsafe_credential_roots_are_rejected_before_claim_mutation() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    let redirect = tempfile::tempdir().unwrap();
    init_run(&run_dir);
    std::os::unix::fs::symlink(redirect.path(), &credentials).unwrap();

    command()
        .args([
            "role",
            "claim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .failure();

    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["revision"], 0);
    assert_eq!(
        baton["participants"]["worker"]["claim"],
        serde_json::Value::Null
    );
}

#[test]
fn racing_role_claims_create_exactly_one_reviewer_credential() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    init_run(&run_dir);

    let spawn = |session: &str| {
        std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
            .args([
                "role",
                "claim",
                "--api",
                "2",
                "--run-dir",
                run_dir.to_str().unwrap(),
                "--role",
                "reviewer",
                "--session-id",
                session,
                "--lease-seconds",
                "300",
                "--expected-revision",
                "0",
                "--credentials-root",
                credentials.to_str().unwrap(),
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let first = spawn("reviewer-a");
    let second = spawn("reviewer-b");
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
    assert_eq!(
        ["reviewer-a", "reviewer-b"]
            .iter()
            .filter(|session| {
                credentials
                    .join(session)
                    .join("run-a/reviewer.json")
                    .exists()
            })
            .count(),
        1
    );
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["revision"], 1);
    assert!(matches!(
        baton["participants"]["reviewer"]["claim"]["session_id"].as_str(),
        Some("reviewer-a" | "reviewer-b")
    ));
}

#[test]
fn recovery_clears_claims_and_fences_the_preserved_credential() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    init_run(&run_dir);
    command()
        .args([
            "role",
            "claim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();
    let credential = credentials.join("worker-session/run-a/worker.json");
    assert!(credential.exists());

    command()
        .args([
            "recover",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--from-revision",
            "1",
        ])
        .assert()
        .success();

    command()
        .args([
            "role",
            "read",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .failure();
    assert!(credential.exists());
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["revision"], 2);
    assert_eq!(
        baton["participants"]["worker"]["claim"],
        serde_json::Value::Null
    );
}

#[test]
fn relative_run_paths_are_canonicalized_in_credentials() {
    let current = std::env::current_dir().unwrap();
    let root = tempfile::Builder::new()
        .prefix("dvandva-relative-")
        .tempdir_in(&current)
        .unwrap();
    let relative_root = root.path().strip_prefix(&current).unwrap();
    let run_dir = relative_root.join("runs/run-a");
    let credentials = root.path().join("credentials");
    init_run(&run_dir);

    command()
        .args([
            "role",
            "claim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();

    let private: serde_json::Value = serde_json::from_slice(
        &std::fs::read(credentials.join("worker-session/run-a/worker.json")).unwrap(),
    )
    .unwrap();
    assert!(std::path::Path::new(private["run_dir"].as_str().unwrap()).is_absolute());
}

#[test]
fn worker_start_creates_claims_and_idempotently_resumes_one_run() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let runs = root.path().join("state/runs");
    let credentials = root.path().join("state/credentials");
    std::fs::create_dir(&workspace).unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&workspace)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--quiet"]);
    git(&[
        "remote",
        "add",
        "origin",
        "git@github.com:axatbhardwaj/Dvandva.git",
    ]);

    let start = || {
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
                "worker",
                "--session-id",
                "worker-session",
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
                "--lease-seconds",
                "300",
            ])
            .output()
            .unwrap()
    };

    let created = start();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    assert_eq!(created["outcome"], "started");
    assert_eq!(created["disposition"], "created");
    assert_eq!(created["revision"], 1);
    assert_eq!(created["objective"]["summary"], "Implement DEF-123");
    assert_eq!(created["scope_revision"], 0);
    assert_eq!(created["status"], "working");
    assert_eq!(created["assignee"], "worker");
    assert_eq!(created["role_state"], "assigned");
    assert_eq!(created["advisory_actions"], serde_json::json!(["work"]));
    assert_eq!(
        created["legal_actions"],
        serde_json::json!([
            "submit_checkpoint",
            "stage_explainer",
            "report_progress",
            "request_human_decision"
        ])
    );
    assert_eq!(
        created["next_actions"],
        serde_json::json!(["work", "submit_checkpoint", "stage_explainer", "report_progress"])
    );
    // A completed deliverable always has somewhere to land, from revision 1 on.
    assert_eq!(created["blocking_reason"], serde_json::Value::Null);
    assert_eq!(created["actionable"], true);
    assert_eq!(
        created["peer_prompt"],
        format!(
            "Act as prativadi and join Dvandva run {}.",
            created["run_id"].as_str().unwrap()
        )
    );
    assert!(created.get("token").is_none());
    assert_eq!(created["private_credential_path"], created["credential"]);
    let run_id = created["run_id"].as_str().unwrap();
    let run_dir = runs.join(run_id);
    assert_eq!(created["run_dir"], run_dir.to_str().unwrap());

    let resumed = start();
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let resumed: serde_json::Value = serde_json::from_slice(&resumed.stdout).unwrap();
    assert_eq!(resumed["outcome"], "started");
    assert_eq!(resumed["disposition"], "resumed");
    assert_eq!(resumed["run_id"], run_id);
    assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 1);

    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    assert_eq!(
        baton["workspace"]["repository_id"],
        "github.com/axatbhardwaj/dvandva"
    );
    assert_eq!(baton["task"]["reference"], "DEF-123");
    assert_eq!(baton["participants"]["worker"]["harness"], "Codex");
    assert_eq!(baton["participants"]["reviewer"]["harness"], "Claude");
    assert_eq!(
        baton["participants"]["worker"]["claim"]["session_id"],
        "worker-session"
    );

    let separate = command()
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
            "worker-session",
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
    assert!(
        separate.status.success(),
        "{}",
        String::from_utf8_lossy(&separate.stderr)
    );
    let separate: serde_json::Value = serde_json::from_slice(&separate.stdout).unwrap();
    assert_eq!(separate["disposition"], "created");
    assert_ne!(separate["run_id"], run_id);
    assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 2);
}

#[test]
fn snapshot_non_exact_start_requires_objective_before_discovery() {
    let root = tempfile::tempdir().unwrap();
    let output = command()
        .args([
            "role",
            "start",
            "--api",
            "2",
            "--workspace",
            root.path().join("missing-workspace").to_str().unwrap(),
            "--runs-dir",
            root.path().join("runs").to_str().unwrap(),
            "--credentials-root",
            root.path().join("credentials").to_str().unwrap(),
            "--role",
            "reviewer",
            "--session-id",
            "reviewer",
            "--current-harness",
            "claude",
            "--peer-harness",
            "codex",
            "--wait",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["error"], "invalid_input");
    assert!(error["message"].as_str().unwrap().contains("objective"));
    assert!(!root.path().join("runs").exists());
    assert!(!root.path().join("credentials").exists());
}

#[test]
fn snapshot_broad_scope_mismatch_does_not_claim_immediate_match() {
    let root = tempfile::tempdir().unwrap();
    let workspace = initialize_workspace(root.path(), "git@github.com:axatbhardwaj/Dvandva.git");
    let runs = root.path().join("runs");
    let credentials = root.path().join("credentials");
    let created = create_worker_run(
        &workspace,
        &runs,
        &credentials,
        "Canonical objective",
        "DEF-123",
    );
    let run_id = created["run_id"].as_str().unwrap();

    let mismatches = [
        vec![
            "--objective",
            "Different objective",
            "--task-reference",
            "DEF-123",
        ],
        vec![
            "--objective",
            "Canonical objective",
            "--objective-ref",
            "ticket=https://tracker.test/OTHER",
            "--task-reference",
            "DEF-123",
        ],
        vec![
            "--objective",
            "Canonical objective",
            "--task-reference",
            "DEF-999",
        ],
        vec![
            "--objective",
            "Canonical objective",
            "--task-reference",
            "DEF-123",
            "--required-deliverable",
            "implementation=Different output",
        ],
    ];
    for (index, coordinates) in mismatches.into_iter().enumerate() {
        let mut process = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"));
        process
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
                &format!("reviewer-{index}"),
                "--current-harness",
                "claude",
                "--peer-harness",
                "codex",
            ])
            .args(coordinates);
        let output = process.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mismatch: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(mismatch["outcome"], "scope_mismatch");
        assert_eq!(mismatch["candidates"][0]["run_id"], run_id);
        assert!(mismatch.get("requested_scope").is_some());
    }
    let installed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(runs.join(run_id).join("baton.json")).unwrap())
            .unwrap();
    assert_eq!(installed["revision"], 1);
    assert!(installed["participants"]["reviewer"]["claim"].is_null());
    assert!(!credentials.join("reviewer-0").exists());

    let separate = command()
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
            "separate-worker",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Separate objective",
            "--task-reference",
            "DEF-123",
            "--required-deliverable",
            "implementation=Separate implementation",
            "--new-run",
        ])
        .output()
        .unwrap();
    assert!(separate.status.success());
    let separate: serde_json::Value = serde_json::from_slice(&separate.stdout).unwrap();
    assert_eq!(separate["disposition"], "created");
    assert_ne!(separate["run_id"], run_id);
    assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 2);
}

#[test]
fn snapshot_broad_scope_mismatch_after_wait_does_not_claim() {
    let root = tempfile::tempdir().unwrap();
    let workspace = initialize_workspace(root.path(), "git@github.com:axatbhardwaj/Dvandva.git");
    let runs = root.path().join("runs");
    let credentials = root.path().join("credentials");
    let waiter = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
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
            "Requested objective",
            "--task-reference",
            "DEF-123",
            "--wait",
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
    let created = create_worker_run(
        &workspace,
        &runs,
        &credentials,
        "Canonical objective",
        "DEF-123",
    );
    let output = waiter.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mismatch: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(mismatch["outcome"], "scope_mismatch");
    let run_id = created["run_id"].as_str().unwrap();
    let installed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(runs.join(run_id).join("baton.json")).unwrap())
            .unwrap();
    assert_eq!(installed["revision"], 1);
    assert!(installed["participants"]["reviewer"]["claim"].is_null());
}

#[test]
fn snapshot_exact_start_ignores_unrelated_corrupt_sibling() {
    let root = tempfile::tempdir().unwrap();
    let workspace = initialize_workspace(root.path(), "git@github.com:axatbhardwaj/Dvandva.git");
    let runs = root.path().join("runs");
    let credentials = root.path().join("credentials");
    let created = create_worker_run(&workspace, &runs, &credentials, "Objective", "DEF-123");
    let corrupt = runs.join("unrelated-corrupt");
    std::fs::create_dir_all(&corrupt).unwrap();
    std::fs::write(corrupt.join("baton.json"), b"not json").unwrap();

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
            "reviewer",
            "--session-id",
            "reviewer",
            "--current-harness",
            "claude",
            "--peer-harness",
            "codex",
            "--run-id",
            created["run_id"].as_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let joined: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(joined["outcome"], "started");
    assert_eq!(joined["run_id"], created["run_id"]);
}

#[test]
fn snapshot_exact_join_needs_no_objective_and_returns_canonical_scope() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let runs = root.path().join("state/runs");
    let credentials = root.path().join("state/credentials");
    std::fs::create_dir(&workspace).unwrap();
    let git = |args: &[&str]| {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&workspace)
            .args(args)
            .status()
            .unwrap()
            .success());
    };
    git(&["init", "--quiet"]);
    git(&[
        "remote",
        "add",
        "origin",
        "git@github.com:axatbhardwaj/Dvandva.git",
    ]);
    let created = command()
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
            "worker",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Canonical objective",
            "--objective-ref",
            "ticket=https://tracker.test/DEF-123",
            "--task-reference",
            "DEF-123",
            "--required-deliverable",
            "implementation=Canonical implementation",
        ])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let created: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    let run_id = created["run_id"].as_str().unwrap();

    let joined = command()
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
            "--run-id",
            run_id,
        ])
        .output()
        .unwrap();
    assert!(
        joined.status.success(),
        "{}",
        String::from_utf8_lossy(&joined.stderr)
    );
    let joined: serde_json::Value = serde_json::from_slice(&joined.stdout).unwrap();
    assert_eq!(joined["objective"]["summary"], "Canonical objective");
    assert_eq!(joined["objective"]["refs"][0]["kind"], "ticket");
    assert_eq!(joined["task"]["reference"], "DEF-123");
    assert_eq!(joined["scope_deliverables"][0]["id"], "implementation");

    let canonical_revision = joined["revision"].as_u64().unwrap();
    let mismatches = [
        vec!["--objective", "Different objective", "--new-run"],
        vec!["--objective-ref", "ticket=https://tracker.test/OTHER"],
        vec!["--task-reference", "DEF-999"],
        vec!["--required-deliverable", "implementation=Different output"],
    ];
    for (index, extra) in mismatches.into_iter().enumerate() {
        let mut process = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"));
        process
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
                &format!("mismatch-{index}"),
                "--current-harness",
                "claude",
                "--peer-harness",
                "codex",
                "--run-id",
                run_id,
            ])
            .args(extra);
        let output = process.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mismatch: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(mismatch["outcome"], "scope_mismatch");
        assert_eq!(
            mismatch["candidates"][0]["objective"]["summary"],
            "Canonical objective"
        );
        assert!(mismatch.get("requested_scope").is_some());
    }
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(runs.join(run_id).join("baton.json")).unwrap())
            .unwrap();
    assert_eq!(baton["revision"], canonical_revision);
    assert!(!credentials.join("mismatch-0").exists());
}

#[test]
fn snapshot_exact_join_normalizes_explicit_deliverable_coordinates() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let runs = root.path().join("state/runs");
    let credentials = root.path().join("state/credentials");
    std::fs::create_dir(&workspace).unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .args(["init", "--quiet"])
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .args([
            "remote",
            "add",
            "origin",
            "git@github.com:axatbhardwaj/Dvandva.git"
        ])
        .status()
        .unwrap()
        .success());
    let created = command()
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
            "worker",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Canonical objective",
            "--required-deliverable",
            "implementation=Canonical implementation",
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    let created: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    let joined = command()
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
            "--run-id",
            created["run_id"].as_str().unwrap(),
            "--required-deliverable",
            " implementation = Canonical implementation ",
        ])
        .output()
        .unwrap();
    assert!(
        joined.status.success(),
        "{}",
        String::from_utf8_lossy(&joined.stderr)
    );
    let joined: serde_json::Value = serde_json::from_slice(&joined.stdout).unwrap();
    assert_eq!(joined["outcome"], "started");
}

#[test]
fn snapshot_near_expiry_actionable_wait_returns_before_heartbeat() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    init_run(&run_dir);
    command()
        .args([
            "role",
            "claim",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker",
            "--lease-seconds",
            "1",
            "--expected-revision",
            "0",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();
    std::thread::sleep(std::time::Duration::from_millis(650));
    let started = std::time::Instant::now();
    let output = command()
        .args([
            "role",
            "wait",
            "--api",
            "2",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker",
            "--credentials-root",
            credentials.to_str().unwrap(),
            "--after-revision",
            "1",
            "--poll-interval-ms",
            "25",
            "--timeout-ms",
            "1500",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(started.elapsed() < std::time::Duration::from_millis(300));
    let snapshot: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(snapshot["revision"], 1);
    assert_eq!(snapshot["next_actions"][0], "work");
}

#[test]
fn snapshot_exact_missing_and_busy_ignore_discovery_wait() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let runs = root.path().join("state/runs");
    let credentials = root.path().join("state/credentials");
    std::fs::create_dir(&workspace).unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .args(["init", "--quiet"])
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .args([
            "remote",
            "add",
            "origin",
            "git@github.com:axatbhardwaj/Dvandva.git"
        ])
        .status()
        .unwrap()
        .success());
    let base = [
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
        "--current-harness",
        "codex",
        "--peer-harness",
        "claude",
    ];
    let started = command()
        .args(base)
        .args([
            "--session-id",
            "owner",
            "--objective",
            "Canonical objective",
            "--required-deliverable",
            "implementation=Canonical output",
        ])
        .output()
        .unwrap();
    assert!(started.status.success());
    let started: serde_json::Value = serde_json::from_slice(&started.stdout).unwrap();
    for (run_id, expected) in [
        (started["run_id"].as_str().unwrap(), "busy"),
        ("missing-run", "run_missing"),
    ] {
        let before = std::time::Instant::now();
        let output = command()
            .args(base)
            .args([
                "--session-id",
                "other",
                "--run-id",
                run_id,
                "--wait",
                "--timeout-ms",
                "2000",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(before.elapsed() < std::time::Duration::from_millis(500));
        let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(outcome["outcome"], expected);
    }
}

#[test]
fn exact_terminal_start_returns_stop_snapshot_without_a_new_credential() {
    use dvandva_v4::model::{
        Assignee, DeliverableRequirement, RunBaton, Status, TaskIdentity, TerminalProvenance,
        WorkspaceIdentity,
    };

    let root = tempfile::tempdir().unwrap();
    let workspace = initialize_workspace(root.path(), "git@github.com:axatbhardwaj/Dvandva.git");
    let runs = root.path().join("runs");
    let credentials = root.path().join("credentials");
    let started = create_worker_run(
        &workspace,
        &runs,
        &credentials,
        "Terminal objective",
        "DEF-123",
    );
    let abandoned_id = started["run_id"].as_str().unwrap();
    let abandoned_dir = runs.join(abandoned_id);
    let action = root.path().join("abandon.json");
    std::fs::write(&action, br#"{"type":"abandon","reason":"cancelled"}"#).unwrap();
    command()
        .args([
            "role",
            "apply",
            "--api",
            "2",
            "--run-dir",
            abandoned_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-session",
            "--expected-revision",
            "1",
            "--credentials-root",
            credentials.to_str().unwrap(),
            "--action",
            action.to_str().unwrap(),
        ])
        .assert()
        .success();

    let done_id = "done-run";
    let done_dir = runs.join(done_id);
    std::fs::create_dir_all(&done_dir).unwrap();
    let mut done = RunBaton::new(
        done_id,
        "Terminal objective",
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "implementation".into(),
            description: "Canonical implementation".into(),
        }],
    )
    .unwrap()
    .with_discovery_identity(
        WorkspaceIdentity {
            repository_id: "github.com/axatbhardwaj/dvandva".into(),
            origin: Some("git@github.com:axatbhardwaj/Dvandva.git".into()),
            worktree: None,
        },
        TaskIdentity {
            reference: Some("DEF-123".into()),
            summary: "Terminal objective".into(),
        },
    );
    done.status = Status::Done;
    done.assignee = Assignee::None;
    done.terminal = Some(TerminalProvenance {
        outcome: "done".into(),
        reason: None,
    });
    std::fs::write(
        done_dir.join("baton.json"),
        serde_json::to_vec_pretty(&done).unwrap(),
    )
    .unwrap();

    for (run_id, session_id) in [
        (abandoned_id, "worker-session"),
        (abandoned_id, "late-session"),
        (done_id, "done-late-session"),
    ] {
        let private = credentials
            .join(session_id)
            .join(run_id)
            .join("worker.json");
        let existed = private.exists();
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
                "worker",
                "--session-id",
                session_id,
                "--current-harness",
                "codex",
                "--peer-harness",
                "claude",
                "--objective",
                "Terminal objective",
                "--task-reference",
                "DEF-123",
                "--run-id",
                run_id,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let snapshot: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(snapshot["outcome"], "started");
        assert_eq!(snapshot["disposition"], "terminal");
        assert_eq!(snapshot["next_actions"], serde_json::json!(["stop"]));
        assert!(snapshot.get("credential").is_none());
        assert_eq!(private.exists(), existed);
    }

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
            "worker",
            "--session-id",
            "mismatch",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Different objective",
            "--run-id",
            done_id,
        ])
        .output()
        .unwrap();
    assert!(mismatch.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&mismatch.stdout).unwrap()["outcome"],
        "scope_mismatch"
    );
}

#[test]
fn snapshot_classifier_keeps_semantic_and_harness_duties_independent() {
    use dvandva_v4::{claim::Role, model::*, next_action};

    let normal = RunBaton::new(
        "normal",
        "Objective",
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "output".into(),
            description: "Output".into(),
        }],
    )
    .unwrap();
    let worker = next_action::classify(&normal, Role::Worker, "Codex");
    assert_eq!(worker.advisory_actions, vec!["work"]);
    assert_eq!(
        worker.legal_actions,
        vec![
            "submit_checkpoint",
            "stage_explainer",
            "report_progress",
            "request_human_decision"
        ]
    );
    assert_eq!(worker.blocking_reason, None);
    assert_eq!(
        next_action::classify(&normal, Role::Reviewer, "Claude").next_actions,
        vec!["wait", "report_progress"]
    );

    let reverse = RunBaton::new(
        "reverse",
        "Objective",
        "claude",
        "codex",
        vec![DeliverableRequirement {
            id: "output".into(),
            description: "Output".into(),
        }],
    )
    .unwrap();
    assert_eq!(
        next_action::classify(&reverse, Role::Worker, "Claude").next_actions,
        vec!["work", "submit_checkpoint", "report_progress"]
    );
    assert_eq!(
        next_action::classify(&reverse, Role::Reviewer, "Codex").next_actions,
        vec!["stage_explainer", "report_progress"]
    );

    let mut stale = normal;
    stale.status = Status::Reviewing;
    stale.assignee = Assignee::Reviewer;
    stale.checkpoint = Some(Checkpoint {
        kind: "git".into(),
        identity: "checkpoint-a".into(),
        deliverables: vec![],
        verification: vec!["test".into()],
        scope_revision: 0,
        manifest_digest: "a".repeat(64),
    });
    stale.pending_checkpoint_supersession = Some(CheckpointSupersession {
        reason: "new complete checkpoint".into(),
        checkpoint: stale.checkpoint.as_ref().unwrap().binding(),
    });
    let reviewer = next_action::classify(&stale, Role::Reviewer, "Claude");
    assert!(reviewer
        .next_actions
        .contains(&"accept_checkpoint_supersession"));
    assert!(!reviewer.next_actions.contains(&"record_review"));
    assert_eq!(reviewer.blocking_reason, None);
}

#[test]
fn claimed_active_snapshots_advertise_human_escape_without_making_wait_actionable() {
    use dvandva_v4::{claim::Role, model::*, next_action};

    for (status, assignee) in [
        (Status::Working, Assignee::Worker),
        (Status::Revising, Assignee::Worker),
        (Status::Reviewing, Assignee::Reviewer),
        (Status::Finalizing, Assignee::Worker),
    ] {
        for role in [Role::Worker, Role::Reviewer] {
            let mut baton = RunBaton::new(
                "active",
                "Objective",
                "codex",
                "claude",
                vec![DeliverableRequirement {
                    id: "output".into(),
                    description: "Output".into(),
                }],
            )
            .unwrap();
            baton.status = status.clone();
            baton.assignee = assignee.clone();

            let snapshot = next_action::classify(&baton, role, "Other");
            assert!(
                snapshot.legal_actions.contains(&"request_human_decision"),
                "{status:?}/{assignee:?}/{role:?} omitted the escape action"
            );
            assert!(!snapshot.next_actions.contains(&"request_human_decision"));
        }
    }

    let waiting = RunBaton::new(
        "waiting",
        "Objective",
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "output".into(),
            description: "Output".into(),
        }],
    )
    .unwrap();
    let waiting = next_action::classify(&waiting, Role::Reviewer, "Other");
    assert_eq!(waiting.next_actions, vec!["wait", "report_progress"]);
    assert_eq!(
        waiting.legal_actions,
        vec!["wait", "report_progress", "request_human_decision"]
    );
    assert!(!waiting.actionable);
}

#[test]
fn human_decision_and_terminal_snapshots_do_not_advertise_another_request() {
    use dvandva_v4::{claim::Role, model::*, next_action};

    let mut baton = RunBaton::new(
        "inactive",
        "Objective",
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "output".into(),
            description: "Output".into(),
        }],
    )
    .unwrap();
    baton.status = Status::HumanDecision;
    baton.assignee = Assignee::Human;
    baton.human_decision = Some(HumanDecision {
        question: "Choose".into(),
        requested_by: "worker".into(),
        evidence: vec!["Evidence".into()],
        options: vec!["A".into(), "B".into()],
        contact_role: "worker".into(),
        resume_status: Status::Working,
        resume_assignee: Assignee::Worker,
        answer: None,
    });
    for role in [Role::Worker, Role::Reviewer] {
        assert!(!next_action::classify(&baton, role, "Other")
            .legal_actions
            .contains(&"request_human_decision"));
    }

    for status in [Status::Done, Status::Abandoned] {
        baton.status = status;
        baton.assignee = Assignee::None;
        assert!(!next_action::classify(&baton, Role::Worker, "Other")
            .legal_actions
            .contains(&"request_human_decision"));
    }
}

#[test]
fn reviewer_start_waits_without_model_polling_and_joins_the_created_run() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let runs = root.path().join("state/runs");
    let credentials = root.path().join("state/credentials");
    std::fs::create_dir(&workspace).unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&workspace)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
    };
    git(&["init", "--quiet"]);
    git(&[
        "remote",
        "add",
        "origin",
        "git@github.com:axatbhardwaj/Dvandva.git",
    ]);

    let reviewer = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
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
            "reviewer-session",
            "--current-harness",
            "claude",
            "--peer-harness",
            "codex",
            "--objective",
            "Implement DEF-123",
            "--task-reference",
            "DEF-123",
            "--wait",
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
    let worker = command()
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
            "worker-session",
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
    assert!(
        worker.status.success(),
        "{}",
        String::from_utf8_lossy(&worker.stderr)
    );
    let worker: serde_json::Value = serde_json::from_slice(&worker.stdout).unwrap();

    let reviewer = reviewer.wait_with_output().unwrap();
    assert!(
        reviewer.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewer.stderr)
    );
    let reviewer: serde_json::Value = serde_json::from_slice(&reviewer.stdout).unwrap();
    assert_eq!(reviewer["outcome"], "started");
    assert_eq!(reviewer["disposition"], "claimed");
    assert_eq!(reviewer["run_id"], worker["run_id"]);
    assert!(matches!(reviewer["revision"].as_u64(), Some(1 | 2)));
    assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 1);
    let run_dir = runs.join(worker["run_id"].as_str().unwrap());
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["revision"], 2);
    assert_eq!(
        baton["participants"]["worker"]["claim"]["session_id"],
        "worker-session"
    );
    assert_eq!(
        baton["participants"]["reviewer"]["claim"]["session_id"],
        "reviewer-session"
    );
}

#[test]
fn worker_start_rejects_unsafe_session_ids_before_creating_a_run() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let runs = root.path().join("state/runs");
    let credentials = root.path().join("state/credentials");
    std::fs::create_dir(&workspace).unwrap();
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .args(["init", "--quiet"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .args([
            "remote",
            "add",
            "origin",
            "git@github.com:axatbhardwaj/Dvandva.git",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

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
            "worker",
            "--session-id",
            "../escape",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Implement DEF-123",
            "--task-reference",
            "DEF-123",
        ])
        .assert()
        .failure();
    assert!(!runs.exists() || std::fs::read_dir(&runs).unwrap().next().is_none());

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
            "worker",
            "--session-id",
            "worker-session",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--objective",
            "Implement DEF-123",
            "--task-reference",
            "DEF-123",
            "--lease-seconds",
            "0",
        ])
        .assert()
        .failure();
    assert!(!runs.exists() || std::fs::read_dir(&runs).unwrap().next().is_none());
}
