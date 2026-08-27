use assert_cmd::Command;
use dvandva_v4::model::{DeliverableRequirement, RunBaton};
use dvandva_v4::store::{RunChannel, StoreError};
use predicates::prelude::*;
use sha2::{Digest, Sha256};

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
            "--required-deliverable",
            "implementation=Implement DEF-123",
        ])
        .assert()
        .success();
}

fn init_pair_with_scope(dir: &std::path::Path, deliverables: &[(&str, &str)]) {
    let mut args = vec![
        "init".to_owned(),
        "--run-dir".to_owned(),
        dir.to_str().unwrap().to_owned(),
        "--run-id".to_owned(),
        "run-a".to_owned(),
        "--objective".to_owned(),
        "Implement DEF-123".to_owned(),
        "--worker".to_owned(),
        "codex".to_owned(),
        "--reviewer".to_owned(),
        "claude".to_owned(),
        "--repository-id".to_owned(),
        "github.com/axatbhardwaj/dvandva".to_owned(),
    ];
    for (id, description) in deliverables {
        args.push("--required-deliverable".to_owned());
        args.push(format!("{id}={description}"));
    }
    command().args(args).assert().success();
}

fn checkpoint_submission(identity: &str, deliverables: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "submit_checkpoint",
        "checkpoint": {
            "kind": "artifact",
            "identity": identity,
            "deliverables": deliverables,
            "verification": ["tests passed"]
        }
    })
}

fn checkpoint_binding(dir: &std::path::Path) -> serde_json::Value {
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("baton.json")).unwrap()).unwrap();
    serde_json::json!({
        "checkpoint_identity": baton["checkpoint"]["identity"],
        "manifest_digest": baton["checkpoint"]["manifest_digest"],
        "scope_revision": baton["checkpoint"]["scope_revision"]
    })
}

fn review_action(
    verdict: &str,
    binding: &serde_json::Value,
    findings: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "type": "record_review",
        "verdict": verdict,
        "checkpoint_identity": binding["checkpoint_identity"],
        "manifest_digest": binding["manifest_digest"],
        "scope_revision": binding["scope_revision"],
        "findings": findings
    })
}

fn setup_reviewing_checkpoint(dir: &std::path::Path) -> (String, String, serde_json::Value) {
    init_pair(dir);
    let worker = claim_role(dir, "worker", "worker-1", 0);
    let reviewer = claim_role(dir, "reviewer", "reviewer-1", 1);
    apply_action(
        dir,
        "worker",
        "worker-1",
        &worker,
        2,
        "checkpoint.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .success();
    let binding = checkpoint_binding(dir);
    (worker, reviewer, binding)
}

fn setup_scope_amended_checkpoint(dir: &std::path::Path) -> (String, String) {
    let (worker, reviewer, _) = setup_reviewing_checkpoint(dir);
    apply_action(
        dir,
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "human.json",
        serde_json::json!({
            "type": "request_human_decision", "question": "Replace scope?",
            "evidence": ["New requirement"], "options": ["yes", "no"],
            "contact_role": "reviewer", "resume_status": "reviewing", "resume_assignee": "reviewer"
        }),
    )
    .success();
    apply_action(
        dir,
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "amend.json",
        serde_json::json!({
            "type": "resume_human_decision", "answer": "yes",
            "scope_amendment": {
                "objective": "New objective", "objective_refs": [], "task_reference": null,
                "scope_deliverables": [{"id": "implementation", "description": "New code"}]
            }
        }),
    )
    .success();
    (worker, reviewer)
}

fn write_legacy_run(
    dir: &std::path::Path,
    status: &str,
    assignee: &str,
    worker_claim: serde_json::Value,
) -> Vec<u8> {
    std::fs::create_dir_all(dir.join("history")).unwrap();
    let baton = serde_json::json!({
        "schema": "dvandva.run.v1",
        "run_id": "legacy-run",
        "objective": {"summary": "Preserved objective", "refs": [{"kind": "issue", "value": "DEF-123"}]},
        "workspace": {"repository_id": "github.com/axatbhardwaj/dvandva", "origin": null, "worktree": null},
        "task": {"reference": "DEF-123", "summary": "Preserved objective"},
        "participants": {
            "worker": {"harness": " codex ", "claim": worker_claim},
            "reviewer": {"harness": "CLAUDE", "claim": {
                "session_id": "reviewer-old", "epoch": 2, "token_digest": "reviewer-digest",
                "lease_expires_at": "2000-01-01T00:00:00Z", "lease_seconds": 300
            }}
        },
        "status": status,
        "assignee": assignee,
        "revision": 0,
        "checkpoint": {"kind": "artifact", "identity": "sha256:legacy", "verification": ["legacy tests"]},
        "review": {"verdict": "approved", "checkpoint_identity": "sha256:legacy", "findings": []},
        "publication": {"required": true, "desired_revision": 7, "published_revision": 7,
            "refs": [{"kind": "explainer", "value": "https://legacy.invalid"}]},
        "human_decision": {"question": "Legacy?", "requested_by": "worker", "evidence": ["old"],
            "options": ["yes", "no"], "contact_role": "worker", "resume_status": "working",
            "resume_assignee": "worker", "answer": "yes"},
        "predecessor_run_id": null,
        "terminal": if status == "done" { serde_json::json!({"outcome": "done", "reason": null}) } else { serde_json::Value::Null },
        "recovery": null
    });
    let mut bytes = serde_json::to_vec_pretty(&baton).unwrap();
    bytes.push(b'\n');
    std::fs::write(dir.join("history/00000000000000000000.json"), &bytes).unwrap();
    std::fs::write(dir.join("baton.json"), &bytes).unwrap();
    bytes
}

#[test]
fn migration_init_writes_v2_scope_and_run_started_obligation() {
    let dir = tempfile::tempdir().unwrap();
    command()
        .args([
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "run-v2",
            "--objective",
            " Ship the protocol ",
            "--worker",
            " CoDeX ",
            "--reviewer",
            " claude ",
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
            "--required-deliverable",
            " kernel = Harden the kernel ",
        ])
        .assert()
        .success();

    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["schema"], "dvandva.run.v2");
    assert_eq!(baton["participants"]["worker"]["harness"], "Codex");
    assert_eq!(baton["participants"]["reviewer"]["harness"], "Claude");
    assert_eq!(baton["scope_revision"], 0);
    assert_eq!(
        baton["scope_deliverables"],
        serde_json::json!([
            {"id": "kernel", "description": "Harden the kernel"}
        ])
    );
    assert_eq!(
        baton["publication_policy"],
        serde_json::json!({
            "publisher_harness": "Codex", "channel": "codex_sites",
            "access": "owner_only", "reviewer_harness": "Claude"
        })
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["kind"],
        "run_started"
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["handoff_revision"],
        0
    );
    assert!(baton["publication_binding"]["deployment"].is_null());
    assert!(baton["publication_binding"]["review"].is_null());
}

#[test]
fn migration_init_rejects_invalid_scope_and_participant_topology() {
    let cases = [
        (vec![], "required deliverable"),
        (
            vec!["--required-deliverable", " = blank"],
            "required deliverable",
        ),
        (
            vec![
                "--required-deliverable",
                "same=one",
                "--required-deliverable",
                "same=two",
            ],
            "duplicate required deliverable",
        ),
        (vec!["--required-deliverable", "one=One"], ""),
    ];
    for (extra, expected) in cases {
        let dir = tempfile::tempdir().unwrap();
        let mut args = vec![
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "run-v2",
            "--objective",
            "Ship",
            "--worker",
            "codex",
            "--reviewer",
            "claude",
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
        ];
        if expected.is_empty() {
            args[8] = "cursor";
        }
        args.extend(extra);
        let expected = if expected.is_empty() {
            "exactly one Codex"
        } else {
            expected
        };
        command()
            .args(args)
            .assert()
            .failure()
            .stderr(predicate::str::contains(expected));
        assert!(!dir.path().join("baton.json").exists());
    }
}

#[test]
fn migration_probe_reports_epoch_and_rejects_mismatched_expectations() {
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
    assert_eq!(probe["write_schema"], "dvandva.run.v2");
    assert_eq!(
        probe["read_schemas"],
        serde_json::json!(["dvandva.run.v2", "dvandva.run.v1"])
    );
    assert_eq!(probe["role_api"], 2);
    assert_eq!(probe["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(probe["capabilities"]["upgrade_from_v1"], true);

    for args in [
        [
            "probe",
            "--expected-schema",
            "dvandva.run.v1",
            "--expected-role-api",
            "2",
        ],
        [
            "probe",
            "--expected-schema",
            "dvandva.run.v2",
            "--expected-role-api",
            "1",
        ],
    ] {
        command().args(args).assert().failure();
    }
}

#[test]
fn migration_upgrade_is_one_way_and_clears_active_legacy_state() {
    let dir = tempfile::tempdir().unwrap();
    let original = write_legacy_run(dir.path(), "reviewing", "reviewer", serde_json::Value::Null);
    let legacy: RunBaton = serde_json::from_slice(&original).unwrap();
    let expected_digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&legacy).unwrap()));
    let mut changed = legacy.clone();
    changed.objective.summary.push('!');
    let changed_digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&changed).unwrap())
    );
    let credentials = dir.path().join("credentials");

    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-new",
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

    assert_eq!(
        std::fs::read(dir.path().join("history/00000000000000000000.json")).unwrap(),
        original
    );
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["schema"], "dvandva.run.v2");
    assert_eq!(baton["revision"], 1);
    assert_eq!(baton["status"], "revising");
    assert_eq!(baton["assignee"], "worker");
    assert_eq!(
        baton["scope_deliverables"],
        serde_json::json!([
            {"id": "legacy_objective", "description": "Preserved objective"}
        ])
    );
    for pointer in [
        "/checkpoint",
        "/review",
        "/human_decision",
        "/participants/worker/claim",
        "/participants/reviewer/claim",
    ] {
        assert!(baton.pointer(pointer).unwrap().is_null(), "{pointer}");
    }
    assert_eq!(
        baton["publication"]["published_revision"],
        serde_json::Value::Null
    );
    assert_eq!(baton["publication"]["refs"], serde_json::json!([]));
    assert_eq!(
        baton["publication_binding"]["obligation"]["kind"],
        "protocol_upgraded"
    );
    assert_eq!(baton["migration"]["from_schema"], "dvandva.run.v1");
    assert_eq!(baton["migration"]["from_revision"], 0);
    assert_eq!(
        baton["migration"]["legacy_checkpoint"]["identity"],
        "sha256:legacy"
    );
    assert_eq!(baton["migration"]["legacy_state_digest"], expected_digest);
    assert_ne!(expected_digest, changed_digest);
}

#[test]
fn epoch_validation_rejects_malformed_v2_and_relabelled_v1() {
    for mutation in ["missing_scope", "relabelled_v1"] {
        let dir = tempfile::tempdir().unwrap();
        let legacy = write_legacy_run(dir.path(), "working", "worker", serde_json::Value::Null);
        let mut value: serde_json::Value = serde_json::from_slice(&legacy).unwrap();
        value["schema"] = serde_json::json!("dvandva.run.v2");
        if mutation == "missing_scope" {
            value["publication_policy"] = serde_json::json!({
                "publisher_harness": "Codex", "channel": "codex_sites",
                "access": "owner_only", "reviewer_harness": "Claude"
            });
        }
        std::fs::write(
            dir.path().join("baton.json"),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        command()
            .args(["read", "--run-dir", dir.path().to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid_baton"));
    }
}

#[test]
fn epoch_validation_rejects_unknown_schema_before_full_decoding() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(
        dir.path().join("baton.json"),
        br#"{"schema":"dvandva.run.v99","structurally":"unrelated"}"#,
    )
    .unwrap();

    command()
        .args(["read", "--run-dir", dir.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported_schema"));
}

#[test]
fn epoch_validation_fences_create_and_binds_the_v1_to_v2_crossing() {
    let invalid_create = tempfile::tempdir().unwrap();
    let mut malformed = RunBaton::new(
        "bad-v2",
        "Ship",
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "implementation".to_owned(),
            description: "Ship".to_owned(),
        }],
    )
    .unwrap();
    malformed.scope_deliverables.clear();
    assert!(dvandva_v4::store::RunChannel::open(invalid_create.path())
        .create(&malformed)
        .is_err());

    let crossing = tempfile::tempdir().unwrap();
    let legacy_bytes = write_legacy_run(
        crossing.path(),
        "working",
        "worker",
        serde_json::Value::Null,
    );
    let legacy: RunBaton = serde_json::from_slice(&legacy_bytes).unwrap();
    let mut arbitrary = RunBaton::new(
        legacy.run_id.clone(),
        legacy.objective.summary.clone(),
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "legacy_objective".to_owned(),
            description: legacy.objective.summary.clone(),
        }],
    )
    .unwrap();
    arbitrary.workspace = legacy.workspace.clone();
    arbitrary.task = legacy.task.clone();
    arbitrary.revision = 1;
    assert!(dvandva_v4::store::RunChannel::open(crossing.path())
        .compare_and_swap(0, &arbitrary)
        .is_err());
}

#[test]
fn epoch_validation_rejects_an_arbitrary_crossing_in_recovery_history() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_bytes = write_legacy_run(dir.path(), "working", "worker", serde_json::Value::Null);
    let legacy: RunBaton = serde_json::from_slice(&legacy_bytes).unwrap();
    let mut arbitrary = RunBaton::new(
        legacy.run_id.clone(),
        legacy.objective.summary.clone(),
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "legacy_objective".to_owned(),
            description: legacy.objective.summary,
        }],
    )
    .unwrap();
    arbitrary.workspace = legacy.workspace;
    arbitrary.task = legacy.task;
    arbitrary.revision = 1;
    std::fs::write(
        dir.path().join("history/00000000000000000001.json"),
        serde_json::to_vec_pretty(&arbitrary).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("baton.json"),
        serde_json::to_vec_pretty(&arbitrary).unwrap(),
    )
    .unwrap();

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
        .stderr(predicate::str::contains("invalid_history"));
}

#[test]
fn migration_upgrade_rejects_terminal_busy_and_invalid_topology() {
    let terminal = tempfile::tempdir().unwrap();
    write_legacy_run(terminal.path(), "done", "none", serde_json::Value::Null);
    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            terminal.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "new",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--expected-revision",
            "0",
            "--credentials-root",
            terminal.path().join("credentials").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("terminal_state"));

    let busy = tempfile::tempdir().unwrap();
    write_legacy_run(
        busy.path(),
        "working",
        "worker",
        serde_json::json!({
            "session_id": "other", "epoch": 4, "token_digest": "digest",
            "lease_expires_at": "2999-01-01T00:00:00Z", "lease_seconds": 300
        }),
    );
    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            busy.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "new",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--expected-revision",
            "0",
            "--credentials-root",
            busy.path().join("credentials").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("busy"));

    let invalid = tempfile::tempdir().unwrap();
    write_legacy_run(invalid.path(), "working", "worker", serde_json::Value::Null);
    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            invalid.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "new",
            "--current-harness",
            "codex",
            "--peer-harness",
            "cursor",
            "--expected-revision",
            "0",
            "--credentials-root",
            invalid.path().join("credentials").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid_participants"));
}

#[test]
fn migration_recovery_uses_only_the_exact_validated_v2_history_head() {
    let dir = tempfile::tempdir().unwrap();
    write_legacy_run(dir.path(), "working", "worker", serde_json::Value::Null);
    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "0",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("migration_required"));

    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-new",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--expected-revision",
            "0",
            "--credentials-root",
            dir.path().join("credentials").to_str().unwrap(),
        ])
        .assert()
        .success();
    std::fs::write(dir.path().join("baton.json"), b"corrupt").unwrap();

    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "0",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid_history"));
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

    let recovered: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(recovered["schema"], "dvandva.run.v2");
    assert_eq!(recovered["revision"], 2);
    assert_eq!(recovered["recovery"]["from_revision"], 1);
    assert!(recovered["participants"]["worker"]["claim"].is_null());
    assert!(recovered["participants"]["reviewer"]["claim"].is_null());

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
        .stderr(predicate::str::contains("invalid_history"));
    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "2",
        ])
        .assert()
        .success();
}

#[test]
fn migration_history_rejects_v2_downgrades_and_multiple_crossings() {
    let dir = tempfile::tempdir().unwrap();
    write_legacy_run(dir.path(), "working", "worker", serde_json::Value::Null);
    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker-new",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--expected-revision",
            "0",
            "--credentials-root",
            dir.path().join("credentials").to_str().unwrap(),
        ])
        .assert()
        .success();

    let mut downgrade: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.path().join("history/00000000000000000000.json")).unwrap(),
    )
    .unwrap();
    downgrade["revision"] = serde_json::json!(2);
    std::fs::write(
        dir.path().join("history/00000000000000000002.json"),
        serde_json::to_vec_pretty(&downgrade).unwrap(),
    )
    .unwrap();
    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid_history"));
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
        ])
        .assert()
        .success();

    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["schema"], "dvandva.run.v2");
    assert_eq!(baton["run_id"], "run-a");
    assert_eq!(baton["status"], "working");
    assert_eq!(baton["assignee"], "worker");
    assert_eq!(baton["participants"]["worker"]["harness"], "Codex");
    assert_eq!(baton["participants"]["reviewer"]["harness"], "Claude");
}

#[test]
fn new_runs_require_a_synchronized_publication() {
    let baton = RunBaton::new(
        "run-publication",
        "Implement DEF-123",
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "implementation".to_owned(),
            description: "Implement DEF-123".to_owned(),
        }],
    )
    .unwrap();
    assert!(baton.publication.required);
    assert_eq!(baton.publication.published_revision, None);
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
            "--required-deliverable",
            " implementation = Implement DEF-123 ",
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
    assert_eq!(baton["participants"]["worker"]["harness"], "Codex");
    assert_eq!(baton["participants"]["reviewer"]["harness"], "Claude");
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
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
                "--required-deliverable",
                "implementation=Implement DEF-123",
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
    let legacy = dvandva_v4::model::RunBaton::new(
        "legacy-run",
        "Preserved objective",
        "codex",
        "claude",
        vec![DeliverableRequirement {
            id: "implementation".to_owned(),
            description: "Preserved objective".to_owned(),
        }],
    )
    .unwrap();
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("exactly one Codex and one Claude"));
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
        "--required-deliverable",
        "implementation=Implement DEF-123",
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
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
                "--required-deliverable",
                "implementation=Implement DEF-123",
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
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
                "--required-deliverable",
                "implementation=Implement DEF-123",
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
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
            "--required-deliverable",
            "implementation=Implement DEF-123",
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
            checkpoint_submission(
                "sha256:first",
                serde_json::json!([
                    {"id": "implementation", "artifacts": [{"kind": "commit", "value": "first"}]}
                ]),
            ),
        ),
    );
    let first_binding = checkpoint_binding(dir.path());
    apply(
        "reviewer",
        "reviewer-1",
        &reviewer_token,
        "3",
        write_action(
            dir.path(),
            "changes.json",
            review_action(
                "changes_requested",
                &first_binding,
                serde_json::json!(["Handle the empty case"]),
            ),
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
            checkpoint_submission(
                "sha256:second",
                serde_json::json!([
                    {"id": "implementation", "artifacts": [{"kind": "commit", "value": "second"}]}
                ]),
            ),
        ),
    );
    let second_binding = checkpoint_binding(dir.path());
    apply(
        "reviewer",
        "reviewer-1",
        &reviewer_token,
        "5",
        write_action(
            dir.path(),
            "approve.json",
            review_action("approved", &second_binding, serde_json::json!([])),
        ),
    );
    apply(
        "worker",
        "worker-1",
        &worker_token,
        "6",
        write_action(
            dir.path(),
            "publication.json",
            serde_json::json!({
                "type": "record_publication", "required": true,
                "desired_revision": 6, "published_revision": 6,
                "refs": [{"kind": "explainer", "value": "https://example.test/run-a"}]
            }),
        ),
    );
    apply(
        "worker",
        "worker-1",
        &worker_token,
        "7",
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
        checkpoint_submission(
            "sha256:ready",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "ready"}]}
            ]),
        ),
    )
    .success();
    let ready_binding = checkpoint_binding(dir.path());
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "approval.json",
        review_action("approved", &ready_binding, serde_json::json!([])),
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
            "published_revision": 1, "refs": []
        }),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        6,
        "missing-explainer.json",
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
        6,
        "explainer.json",
        serde_json::json!({
            "type": "record_publication", "required": true, "desired_revision": 1,
            "published_revision": 1,
            "refs": [{"kind": "explainer", "value": "https://example.test/run-a"}]
        }),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        7,
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
        checkpoint_submission(
            "sha256:wake",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "wake"}]}
            ]),
        ),
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
        checkpoint_submission(
            "sha256:recover",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "recover"}]}
            ]),
        ),
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
            "2",
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
            "2",
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

#[test]
fn checkpoint_manifest_must_exactly_cover_canonical_scope() {
    let dir = tempfile::tempdir().unwrap();
    init_pair_with_scope(
        dir.path(),
        &[("implementation", "Code"), ("report", "Report")],
    );
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let _reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
    let invalid = [
        serde_json::json!([]),
        serde_json::json!([
            {"id": " ", "artifacts": [{"kind": "commit", "value": "abc"}]},
            {"id": "report", "artifacts": [{"kind": "file", "value": "report.md"}]}
        ]),
        serde_json::json!([
            {"id": "implementation", "artifacts": [{"kind": " ", "value": "abc"}]},
            {"id": "report", "artifacts": [{"kind": "file", "value": "report.md"}]}
        ]),
        serde_json::json!([
            {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
        ]),
        serde_json::json!([
            {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]},
            {"id": "report", "artifacts": [{"kind": "file", "value": "report.md"}]},
            {"id": "extra", "artifacts": [{"kind": "file", "value": "extra.md"}]}
        ]),
        serde_json::json!([
            {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]},
            {"id": "implementation", "artifacts": [{"kind": "file", "value": "report.md"}]}
        ]),
    ];
    for (index, manifest) in invalid.into_iter().enumerate() {
        apply_action(
            dir.path(),
            "worker",
            "worker-1",
            &worker,
            2,
            &format!("invalid-checkpoint-{index}.json"),
            checkpoint_submission("checkpoint-a", manifest),
        )
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_checkpoint""#));
    }
}

#[test]
fn checkpoint_digest_is_deterministic_and_scope_stamped() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for dir in [&first, &second] {
        init_pair_with_scope(
            dir.path(),
            &[("implementation", "Code"), ("report", "Report")],
        );
        let worker = claim_role(dir.path(), "worker", "worker-1", 0);
        let _reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
        let manifest = if dir.path() == first.path() {
            serde_json::json!([
                {"id": " report ", "artifacts": [{"kind": " file ", "value": " report.md "}]},
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": " abc "}]}
            ])
        } else {
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]},
                {"id": "report", "artifacts": [{"kind": "file", "value": "report.md"}]}
            ])
        };
        apply_action(
            dir.path(),
            "worker",
            "worker-1",
            &worker,
            2,
            "checkpoint.json",
            checkpoint_submission(" checkpoint-a ", manifest),
        )
        .success();
    }
    let read = |dir: &tempfile::TempDir| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap()
    };
    let first = read(&first);
    let second = read(&second);
    assert_eq!(first["checkpoint"], second["checkpoint"]);
    assert_eq!(first["checkpoint"]["identity"], "checkpoint-a");
    assert_eq!(first["checkpoint"]["scope_revision"], 0);
    let digest = first["checkpoint"]["manifest_digest"].as_str().unwrap();
    assert_eq!(
        digest,
        "4e5a7ed7606fb766f9139460ec6e6f1fb0e5d73a92ee818f6d472e70e8527bc1"
    );
}

#[test]
fn checkpoint_review_binds_all_coordinates_and_identity_history() {
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
        "checkpoint.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .success();
    let binding = checkpoint_binding(dir.path());
    for (index, stale) in [
        serde_json::json!({"checkpoint_identity": "other", "manifest_digest": binding["manifest_digest"], "scope_revision": 0}),
        serde_json::json!({"checkpoint_identity": "checkpoint-a", "manifest_digest": "0".repeat(64), "scope_revision": 0}),
        serde_json::json!({"checkpoint_identity": "checkpoint-a", "manifest_digest": binding["manifest_digest"], "scope_revision": 1}),
    ].into_iter().enumerate() {
        apply_action(
            dir.path(), "reviewer", "reviewer-1", &reviewer, 3,
            &format!("stale-review-{index}.json"),
            review_action("changes_requested", &stale, serde_json::json!(["fix it"])),
        ).failure().stderr(predicate::str::contains(r#""error":"stale_review""#));
    }
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "changes.json",
        review_action("changes_requested", &binding, serde_json::json!(["fix it"])),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "duplicate.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "def"}]}
            ]),
        ),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"invalid_checkpoint""#));
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["checkpoint_history"].as_array().unwrap().len(), 1);
    assert_eq!(
        baton["review"]["manifest_digest"],
        binding["manifest_digest"]
    );
    assert_eq!(baton["review"]["scope_revision"], 0);
}

#[test]
fn scope_checkpoint_identity_history_on_disk_remains_authoritative() {
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
        "checkpoint.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .success();
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "human.json",
        serde_json::json!({
            "type": "request_human_decision", "question": "Replace scope?",
            "evidence": ["New requirement"], "options": ["yes", "no"],
            "contact_role": "reviewer", "resume_status": "reviewing", "resume_assignee": "reviewer"
        }),
    )
    .success();
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "amend.json",
        serde_json::json!({
            "type": "resume_human_decision", "answer": "yes",
            "scope_amendment": {
                "objective": "New objective", "objective_refs": [], "task_reference": null,
                "scope_deliverables": [{"id": "implementation", "description": "New code"}]
            }
        }),
    )
    .success();
    let baton_path = dir.path().join("baton.json");
    let mut baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baton_path).unwrap()).unwrap();
    baton["checkpoint_history"] = serde_json::json!([]);
    std::fs::write(&baton_path, serde_json::to_vec_pretty(&baton).unwrap()).unwrap();

    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        5,
        "duplicate.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "def"}]}
            ]),
        ),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"invalid_checkpoint""#));
}

#[test]
fn scope_amendment_replaces_scope_and_pending_handoff() {
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
        "checkpoint.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "supersede.json",
        serde_json::json!({"type": "request_checkpoint_supersession", "reason": "scope changed"}),
    )
    .success();
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "human.json",
        serde_json::json!({
            "type": "request_human_decision", "question": "Expand scope?",
            "evidence": ["A report is required"], "options": ["yes", "no"],
            "contact_role": "reviewer", "resume_status": "reviewing", "resume_assignee": "reviewer"
        }),
    )
    .success();
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        5,
        "amend.json",
        serde_json::json!({
            "type": "resume_human_decision", "answer": "Include both",
            "scope_amendment": {
                "objective": " Ship code and report ",
                "objective_refs": [{"kind": " issue ", "value": " DEF-456 "}],
                "task_reference": " DEF-456 ",
                "scope_deliverables": [
                    {"id": " implementation ", "description": " Code "},
                    {"id": "report", "description": " Report "}
                ]
            }
        }),
    )
    .success();
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["scope_revision"], 1);
    assert_eq!(baton["objective"]["summary"], "Ship code and report");
    assert_eq!(
        baton["objective"]["refs"],
        serde_json::json!([{"kind": "issue", "value": "DEF-456"}])
    );
    assert_eq!(baton["task"]["reference"], "DEF-456");
    assert_eq!(baton["status"], "revising");
    assert_eq!(baton["assignee"], "worker");
    assert!(baton["checkpoint"].is_null());
    assert!(baton["review"].is_null());
    assert!(baton["pending_checkpoint_supersession"].is_null());
    assert_eq!(baton["checkpoint_history"].as_array().unwrap().len(), 1);
    assert_eq!(
        baton["publication_binding"]["obligation"]["kind"],
        "scope_amended"
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["handoff_revision"],
        6
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["scope_revision"],
        1
    );
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        6,
        "duplicate-after-scope.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "def"}]},
                {"id": "report", "artifacts": [{"kind": "file", "value": "report.md"}]}
            ]),
        ),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"invalid_checkpoint""#));
}

#[test]
fn supersession_request_blocks_approval_and_acceptance_reopens_revision() {
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
        "checkpoint.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .success();
    let binding = checkpoint_binding(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "blank.json",
        serde_json::json!({"type": "request_checkpoint_supersession", "reason": " "}),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"missing_reason""#));
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "request.json",
        serde_json::json!({"type": "request_checkpoint_supersession", "reason": "new evidence"}),
    )
    .success();
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "blocked.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"supersession_pending""#,
    ));
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "accept.json",
        serde_json::json!({"type": "accept_checkpoint_supersession"}),
    )
    .success();
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["status"], "revising");
    assert_eq!(baton["assignee"], "worker");
    assert!(baton["checkpoint"].is_null());
    assert!(baton["review"].is_null());
    assert!(baton["pending_checkpoint_supersession"].is_null());
    assert_eq!(
        baton["checkpoint_history"],
        serde_json::json!([binding.clone()])
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["kind"],
        "checkpoint_superseded"
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["checkpoint"],
        binding
    );
}

#[test]
fn checkpoint_supersession_cas_races_are_fail_closed() {
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
        "checkpoint.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .success();
    let binding = checkpoint_binding(dir.path());
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "approve-first.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "stale-request.json",
        serde_json::json!({"type": "request_checkpoint_supersession", "reason": "too late"}),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"revision_conflict""#));
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "withdraw.json",
        serde_json::json!({"type": "withdraw_approval", "reason": "new evidence"}),
    )
    .success();

    let second = tempfile::tempdir().unwrap();
    init_pair(second.path());
    let worker = claim_role(second.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(second.path(), "reviewer", "reviewer-1", 1);
    apply_action(
        second.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "checkpoint.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .success();
    let binding = checkpoint_binding(second.path());
    apply_action(
        second.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "request-first.json",
        serde_json::json!({"type": "request_checkpoint_supersession", "reason": "new evidence"}),
    )
    .success();
    apply_action(
        second.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "stale-approval.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"revision_conflict""#));
    apply_action(
        second.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "blocked-approval.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"supersession_pending""#,
    ));
}

#[test]
fn supersession_approval_withdrawal_reopens_revision_and_replaces_handoff() {
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
        "checkpoint.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .success();
    let binding = checkpoint_binding(dir.path());
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "approve.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "blank.json",
        serde_json::json!({"type": "withdraw_approval", "reason": " "}),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"missing_reason""#));
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "withdraw.json",
        serde_json::json!({"type": "withdraw_approval", "reason": "new deliverable"}),
    )
    .success();
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["status"], "revising");
    assert_eq!(baton["assignee"], "worker");
    assert!(baton["checkpoint"].is_null());
    assert!(baton["review"].is_null());
    assert_eq!(baton["checkpoint_history"].as_array().unwrap().len(), 1);
    assert_eq!(
        baton["publication_binding"]["obligation"]["kind"],
        "approval_withdrawn"
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["checkpoint"],
        binding
    );
}

#[test]
fn supersession_mutations_reject_terminal_runs() {
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
        serde_json::json!({"type": "abandon", "reason": "cancelled"}),
    )
    .success();
    for (index, action) in [
        serde_json::json!({"type": "request_checkpoint_supersession", "reason": "new"}),
        serde_json::json!({"type": "accept_checkpoint_supersession"}),
        serde_json::json!({"type": "withdraw_approval", "reason": "new"}),
        serde_json::json!({
            "type": "resume_human_decision", "answer": "new",
            "scope_amendment": {
                "objective": "new", "objective_refs": [], "task_reference": null,
                "scope_deliverables": [{"id": "implementation", "description": "new"}]
            }
        }),
    ]
    .into_iter()
    .enumerate()
    {
        apply_action(
            dir.path(),
            "worker",
            "worker-1",
            &worker,
            2,
            &format!("terminal-{index}.json"),
            action,
        )
        .failure()
        .stderr(predicate::str::contains(r#""error":"terminal_state""#));
    }
}

#[test]
fn store_edge_rejects_a_truncated_installed_head_without_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let (worker, _) = setup_scope_amended_checkpoint(dir.path());
    let baton_path = dir.path().join("baton.json");
    let mut baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baton_path).unwrap()).unwrap();
    baton["checkpoint_history"] = serde_json::json!([]);
    std::fs::write(&baton_path, serde_json::to_vec_pretty(&baton).unwrap()).unwrap();
    command()
        .args(["read", "--run-dir", dir.path().to_str().unwrap()])
        .assert()
        .success();
    let before = std::fs::read(&baton_path).unwrap();

    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        5,
        "mutation.json",
        serde_json::json!({
            "type": "record_publication", "required": true,
            "desired_revision": 0, "published_revision": null, "refs": []
        }),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"invalid_history""#));
    assert_eq!(std::fs::read(&baton_path).unwrap(), before);
    assert!(!dir
        .path()
        .join("history/00000000000000000006.json")
        .exists());
}

#[test]
fn store_edge_rejects_scope_rollback_without_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let (worker, _) = setup_scope_amended_checkpoint(dir.path());
    let baton_path = dir.path().join("baton.json");
    let mut baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baton_path).unwrap()).unwrap();
    baton["scope_revision"] = serde_json::json!(0);
    baton["publication_binding"]["obligation"]["scope_revision"] = serde_json::json!(0);
    std::fs::write(&baton_path, serde_json::to_vec_pretty(&baton).unwrap()).unwrap();
    command()
        .args(["read", "--run-dir", dir.path().to_str().unwrap()])
        .assert()
        .success();
    let before = std::fs::read(&baton_path).unwrap();

    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        5,
        "mutation.json",
        serde_json::json!({
            "type": "record_publication", "required": true,
            "desired_revision": 0, "published_revision": null, "refs": []
        }),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"invalid_history""#));
    assert_eq!(std::fs::read(&baton_path).unwrap(), before);
}

#[test]
fn store_edge_direct_cas_cannot_replace_checkpoint_history() {
    let dir = tempfile::tempdir().unwrap();
    setup_scope_amended_checkpoint(dir.path());
    let channel = RunChannel::open(dir.path());
    let current = channel.read().unwrap();
    let mut next = current.clone();
    next.revision += 1;
    next.checkpoint_history[0].manifest_digest = "0".repeat(64);

    let error = channel.compare_and_swap(5, &next).unwrap_err();
    assert!(matches!(error, StoreError::InvalidHistory));
    assert_eq!(channel.read().unwrap(), current);
    assert!(!dir
        .path()
        .join("history/00000000000000000006.json")
        .exists());
}

#[test]
fn store_edge_recovery_rejects_non_monotonic_v2_history() {
    let dir = tempfile::tempdir().unwrap();
    setup_scope_amended_checkpoint(dir.path());
    let channel = RunChannel::open(dir.path());
    let mut bad = channel.read().unwrap();
    bad.revision += 1;
    bad.scope_revision = 0;
    bad.checkpoint_history.clear();
    bad.publication_binding
        .as_mut()
        .unwrap()
        .obligation
        .scope_revision = 0;
    std::fs::write(
        dir.path().join("history/00000000000000000006.json"),
        serde_json::to_vec_pretty(&bad).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.path().join("baton.json"), b"corrupt\n").unwrap();

    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "6",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_history""#));
    assert!(!dir
        .path()
        .join("history/00000000000000000007.json")
        .exists());
}

#[test]
fn store_edge_missing_expected_history_is_typed_and_non_mutating() {
    let dir = tempfile::tempdir().unwrap();
    let (worker, _) = setup_scope_amended_checkpoint(dir.path());
    let baton_path = dir.path().join("baton.json");
    let before = std::fs::read(&baton_path).unwrap();
    std::fs::remove_file(dir.path().join("history/00000000000000000005.json")).unwrap();

    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        5,
        "mutation.json",
        serde_json::json!({
            "type": "record_publication", "required": true,
            "desired_revision": 0, "published_revision": null, "refs": []
        }),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"invalid_history""#));
    assert_eq!(std::fs::read(&baton_path).unwrap(), before);
    assert!(!dir
        .path()
        .join("history/00000000000000000006.json")
        .exists());
}

#[test]
fn supersession_changes_requested_resolves_the_pending_request() {
    let dir = tempfile::tempdir().unwrap();
    let (worker, reviewer, binding) = setup_reviewing_checkpoint(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "supersede.json",
        serde_json::json!({
            "type": "request_checkpoint_supersession", "reason": "new evidence"
        }),
    )
    .success();

    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "changes.json",
        review_action(
            "changes_requested",
            &binding,
            serde_json::json!(["Include the new evidence"]),
        ),
    )
    .success();
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["status"], "revising");
    assert_eq!(baton["assignee"], "worker");
    assert!(baton["pending_checkpoint_supersession"].is_null());
    assert_eq!(baton["review"]["verdict"], "changes_requested");
}

#[test]
fn store_rejects_approved_review_with_blocking_findings() {
    let dir = tempfile::tempdir().unwrap();
    let (_, reviewer, binding) = setup_reviewing_checkpoint(dir.path());
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "approve.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .success();
    let baton_path = dir.path().join("baton.json");
    let mut baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baton_path).unwrap()).unwrap();
    baton["review"]["findings"] = serde_json::json!(["blocking"]);
    std::fs::write(&baton_path, serde_json::to_vec_pretty(&baton).unwrap()).unwrap();

    command()
        .args(["read", "--run-dir", dir.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_baton""#));
}

#[test]
fn supersession_finalize_rejects_approved_review_with_pending_request() {
    let dir = tempfile::tempdir().unwrap();
    let (worker, reviewer, binding) = setup_reviewing_checkpoint(dir.path());
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "approve.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "publication.json",
        serde_json::json!({
            "type": "record_publication", "required": true,
            "desired_revision": 0, "published_revision": 0,
            "refs": [{"kind": "explainer", "value": "https://example.test/run"}]
        }),
    )
    .success();
    let baton_path = dir.path().join("baton.json");
    let mut baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baton_path).unwrap()).unwrap();
    baton["pending_checkpoint_supersession"] = serde_json::json!({
        "reason": "late evidence", "checkpoint": binding
    });
    std::fs::write(&baton_path, serde_json::to_vec_pretty(&baton).unwrap()).unwrap();

    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        5,
        "finalize.json",
        serde_json::json!({"type": "finalize"}),
    )
    .failure();
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baton_path).unwrap()).unwrap();
    assert_ne!(persisted["status"], "done");
}
