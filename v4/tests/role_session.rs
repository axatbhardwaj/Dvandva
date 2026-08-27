use std::os::unix::fs::PermissionsExt;

use assert_cmd::Command;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
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
    let action = root.path().join("checkpoint.json");
    std::fs::write(
        &action,
        serde_json::to_vec_pretty(&serde_json::json!({
            "type": "submit_checkpoint",
            "checkpoint": {
                "kind": "git",
                "identity": "0123456789abcdef0123456789abcdef01234567",
                "verification": ["cargo test"]
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let output = command()
        .args([
            "role",
            "apply",
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
    assert_eq!(baton["status"], "reviewing");
    assert_eq!(baton["assignee"], "reviewer");
    assert_eq!(baton["revision"], 2);

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
    let channel = dvandva_v4::store::RunChannel::open(&run_dir);
    let mut baton = channel.read().unwrap();
    baton.revision = 2;
    channel.compare_and_swap(1, &baton).unwrap();

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
    let mut baton =
        dvandva_v4::model::RunBaton::new("run-a", "Implement DEF-123", "codex", "claude");
    baton.participants.worker.claim = Some(dvandva_v4::model::ParticipantClaim {
        session_id: "expired-session".to_owned(),
        epoch: 7,
        token_digest: "expired-digest".to_owned(),
        lease_expires_at: "2000-01-01T00:00:00Z".to_owned(),
        lease_seconds: 300,
    });
    dvandva_v4::store::RunChannel::open(&run_dir)
        .create(&baton)
        .unwrap();

    let output = command()
        .args([
            "role",
            "reclaim",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "replacement-session",
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
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["revision"], 1);
    assert_eq!(result["epoch"], 8);
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
    assert_eq!(current["participants"]["worker"]["claim"]["epoch"], 8);
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
            "0",
        ])
        .assert()
        .success();

    command()
        .args([
            "role",
            "read",
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
