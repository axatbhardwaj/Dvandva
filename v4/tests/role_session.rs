use std::os::unix::fs::PermissionsExt;

use assert_cmd::Command;
use dvandva_v4::{claim::Role, credential};
use sha2::{Digest, Sha256};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
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
    assert!(result.get("credential").is_none());
    assert!(!credentials.exists());
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
fn migration_upgrade_requires_the_matching_private_v1_credential_for_a_live_claim() {
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
        assert_eq!(
            result.status.success(),
            credential_token == Some("valid-token")
        );
        let current: serde_json::Value =
            serde_json::from_slice(&std::fs::read(run_dir.join("baton.json")).unwrap()).unwrap();
        assert_eq!(
            current["schema"] == "dvandva.run.v2",
            credential_token == Some("valid-token")
        );
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
    let action = root.path().join("publication.json");
    std::fs::write(
        &action,
        serde_json::to_vec_pretty(&serde_json::json!({
            "type": "record_explainer_publication",
            "obligation": {
                "handoff_revision": 0, "kind": "run_started", "scope_revision": 0
            },
            "source_digest": "a".repeat(64),
            "site_id": "site-run-a", "site_version": "deployment-1",
            "url": "https://sites.openai.test/site-run-a/deployment-1",
            "channel": "codex_sites", "access": "owner_only"
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
    assert_eq!(
        baton["publication_binding"]["deployment"]["site_id"],
        "site-run-a"
    );

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
fn role_wait_blocks_in_the_foreground_until_the_role_is_actionable() {
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
    assert_eq!(baton["revision"], 2);
    assert_eq!(baton["assignee"], "worker");
}

#[test]
fn role_reclaim_fences_an_expired_session_with_a_new_private_credential() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("runs/run-a");
    let credentials = root.path().join("credentials");
    let mut baton = dvandva_v4::model::RunBaton::new(
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
    baton.participants.worker.claim = Some(dvandva_v4::model::ParticipantClaim {
        session_id: "expired-session".to_owned(),
        epoch: 1,
        token_digest: "0".repeat(64),
        lease_expires_at: "2000-01-01T00:00:00Z".to_owned(),
        lease_seconds: 300,
    });
    baton.revision = 1;
    channel.compare_and_swap(0, &baton).unwrap();

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
