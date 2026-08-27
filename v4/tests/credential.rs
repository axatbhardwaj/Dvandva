use std::sync::{Arc, Barrier};

use assert_cmd::Command;
use dvandva_v4::{claim::Role, credential};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
}

#[test]
fn concurrent_sessions_can_create_the_shared_credential_root() {
    const SESSION_COUNT: usize = 32;

    let root = tempfile::tempdir().unwrap();
    let credentials = Arc::new(root.path().join("credentials"));
    let barrier = Arc::new(Barrier::new(SESSION_COUNT));
    let threads = (0..SESSION_COUNT)
        .map(|index| {
            let credentials = Arc::clone(&credentials);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                credential::prepare(
                    &credentials,
                    &format!("session-{index}"),
                    "run-1",
                    Role::Worker,
                )
            })
        })
        .collect::<Vec<_>>();

    for thread in threads {
        thread.join().unwrap().unwrap();
    }
}

#[test]
fn stale_migration_credential_cannot_authorize_but_can_be_replaced_after_upgrade() {
    let root = tempfile::tempdir().unwrap();
    let run_dir = root.path().join("run");
    let credentials = root.path().join("credentials");
    std::fs::create_dir_all(run_dir.join("history")).unwrap();
    let baton = serde_json::json!({
        "schema": "dvandva.run.v1", "run_id": "legacy-run",
        "objective": {"summary": "Migrate safely", "refs": []},
        "workspace": {"repository_id": "example.com/team/project", "origin": null, "worktree": null},
        "task": {"reference": "DEF-123", "summary": "Migrate safely"},
        "participants": {
            "worker": {"harness": "codex", "claim": {"session_id": "same-session", "epoch": 7,
                "token_digest": "not-the-stale-token", "lease_expires_at": "2000-01-01T00:00:00Z", "lease_seconds": 300}},
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
    credential::prepare(&credentials, "same-session", "legacy-run", Role::Worker).unwrap();
    let stale = credential::Credential {
        run_dir: std::fs::canonicalize(&run_dir).unwrap(),
        run_id: "legacy-run".to_owned(),
        role: Role::Worker,
        session_id: "same-session".to_owned(),
        epoch: 7,
        token: "stale-token".to_owned(),
    };
    let credential_path = credential::store(&credentials, &stale).unwrap();

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
            "same-session",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--expected-revision",
            "0",
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
            "same-session",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("claim"));

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
            "same-session",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "1",
            "--credentials-root",
            credentials.to_str().unwrap(),
        ])
        .assert()
        .success();
    let replacement: credential::Credential =
        serde_json::from_slice(&std::fs::read(credential_path).unwrap()).unwrap();
    assert_ne!(replacement.token, "stale-token");
    assert_eq!(replacement.epoch, 1);
}
