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

fn read_baton(dir: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(dir.join("baton.json")).unwrap()).unwrap()
}

fn current_obligation(dir: &std::path::Path) -> serde_json::Value {
    read_baton(dir)["publication_binding"]["obligation"].clone()
}

fn explainer_publication_action(
    obligation: &serde_json::Value,
    source_digest: &str,
    site_id: &str,
    site_version: &str,
    url: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "record_explainer_publication",
        "obligation": obligation,
        "source_digest": source_digest,
        "site_id": site_id,
        "site_version": site_version,
        "url": url,
        "channel": "codex_sites",
        "access": "owner_only"
    })
}

fn explainer_review_action(
    obligation: &serde_json::Value,
    deployment: &serde_json::Value,
    verdict: &str,
    findings: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "type": "record_explainer_review",
        "obligation": obligation,
        "source_digest": deployment["source_digest"],
        "site_id": deployment["site_id"],
        "site_version": deployment["site_version"],
        "url": deployment["url"],
        "verdict": verdict,
        "findings": findings
    })
}

fn approved_publication_binding(
    obligation: serde_json::Value,
    site_id: &str,
    site_version: &str,
) -> serde_json::Value {
    let deployment = serde_json::json!({
        "obligation": obligation,
        "source_digest": "c".repeat(64),
        "site_id": site_id,
        "site_version": site_version,
        "url": format!("https://sites.openai.test/{site_id}/{site_version}"),
        "channel": "codex_sites",
        "access": "owner_only",
        "publisher_harness": "Codex"
    });
    serde_json::json!({
        "site_id": site_id,
        "obligation": obligation,
        "deployment": deployment,
        "review": {
            "obligation": obligation,
            "source_digest": deployment["source_digest"],
            "site_id": deployment["site_id"],
            "site_version": deployment["site_version"],
            "url": deployment["url"],
            "verdict": "approved",
            "reviewer_harness": "Claude"
        }
    })
}

fn approve_current_explainer(
    dir: &std::path::Path,
    codex: (&str, &str, &str),
    claude: (&str, &str, &str),
    site_version: &str,
) {
    let (codex_role, codex_session, codex_token) = codex;
    let (claude_role, claude_session, claude_token) = claude;
    let baton = read_baton(dir);
    let revision = baton["revision"].as_u64().unwrap();
    let obligation = baton["publication_binding"]["obligation"].clone();
    apply_action(
        dir,
        codex_role,
        codex_session,
        codex_token,
        revision,
        &format!("publish-{site_version}.json"),
        explainer_publication_action(
            &obligation,
            &"a".repeat(64),
            "site-run-a",
            site_version,
            &format!("https://sites.openai.test/site-run-a/{site_version}"),
        ),
    )
    .success();
    let baton = read_baton(dir);
    let deployment = baton["publication_binding"]["deployment"].clone();
    apply_action(
        dir,
        claude_role,
        claude_session,
        claude_token,
        revision + 1,
        &format!("review-{site_version}.json"),
        explainer_review_action(&obligation, &deployment, "approved", serde_json::json!([])),
    )
    .success();
}

fn claim_pair_and_approve_run_started(dir: &std::path::Path) -> (String, String) {
    let worker = claim_role(dir, "worker", "worker-1", 0);
    let reviewer = claim_role(dir, "reviewer", "reviewer-1", 1);
    approve_current_explainer(
        dir,
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "run-started",
    );
    (worker, reviewer)
}

fn setup_reviewing_checkpoint(dir: &std::path::Path) -> (String, String, serde_json::Value) {
    init_pair(dir);
    let worker = claim_role(dir, "worker", "worker-1", 0);
    let reviewer = claim_role(dir, "reviewer", "reviewer-1", 1);
    approve_current_explainer(
        dir,
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "setup-run-started",
    );
    apply_action(
        dir,
        "worker",
        "worker-1",
        &worker,
        4,
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
        5,
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
        6,
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
    assert!(baton["publication"].is_null());
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
    apply_action_raw(dir, role, session, token, revision, name, action)
}

fn apply_action_raw(
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
    assert!(baton.publication.is_none());
    let binding = baton.publication_binding.unwrap();
    assert_eq!(
        binding.obligation.kind,
        dvandva_v4::model::HandoffKind::RunStarted
    );
    assert!(binding.deployment.is_none());
    assert!(binding.review.is_none());
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

    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker_token),
        ("reviewer", "reviewer-1", &reviewer_token),
        "complete-run-started",
    );
    apply(
        "worker",
        "worker-1",
        &worker_token,
        "4",
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
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker_token),
        ("reviewer", "reviewer-1", &reviewer_token),
        "complete-checkpoint-a",
    );
    apply(
        "reviewer",
        "reviewer-1",
        &reviewer_token,
        "7",
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
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker_token),
        ("reviewer", "reviewer-1", &reviewer_token),
        "complete-revise-a",
    );
    apply(
        "worker",
        "worker-1",
        &worker_token,
        "10",
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
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker_token),
        ("reviewer", "reviewer-1", &reviewer_token),
        "complete-checkpoint-b",
    );
    apply(
        "reviewer",
        "reviewer-1",
        &reviewer_token,
        "13",
        write_action(
            dir.path(),
            "approve.json",
            review_action("approved", &second_binding, serde_json::json!([])),
        ),
    );
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker_token),
        ("reviewer", "reviewer-1", &reviewer_token),
        "complete-approved",
    );
    apply(
        "worker",
        "worker-1",
        &worker_token,
        "16",
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

    apply_action_raw(
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
    apply_action_raw(
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
fn local_watcher_wakes_the_reviewer_after_a_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let (worker, reviewer) = claim_pair_and_approve_run_started(dir.path());

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
            "4",
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
        4,
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
    assert_eq!(baton["revision"], 5);
}

#[test]
fn recovery_fences_old_sessions_and_preserves_evidence() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let (worker, _reviewer) = claim_pair_and_approve_run_started(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
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
            "5",
        ])
        .assert()
        .success();
    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["revision"], 6);
    assert_eq!(baton["checkpoint"]["identity"], "sha256:recover");
    assert!(baton["participants"]["worker"]["claim"].is_null());
    assert!(baton["participants"]["reviewer"]["claim"].is_null());
    assert_eq!(baton["recovery"]["from_revision"], 5);
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
    let (worker, _reviewer) = claim_pair_and_approve_run_started(dir.path());
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
            4,
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
        let (worker, _reviewer) = claim_pair_and_approve_run_started(dir.path());
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
            4,
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
    let (worker, reviewer) = claim_pair_and_approve_run_started(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
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
            dir.path(), "reviewer", "reviewer-1", &reviewer, 5,
            &format!("stale-review-{index}.json"),
            review_action("changes_requested", &stale, serde_json::json!(["fix it"])),
        ).failure().stderr(predicate::str::contains(r#""error":"stale_review""#));
    }
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "checkpoint-a",
    );
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        7,
        "changes.json",
        review_action("changes_requested", &binding, serde_json::json!(["fix it"])),
    )
    .success();
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "checkpoint-a-revision",
    );
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        10,
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
    let (worker, reviewer) = claim_pair_and_approve_run_started(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
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
        5,
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
        6,
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
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "scope-history",
    );
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
        9,
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
    let (worker, reviewer) = claim_pair_and_approve_run_started(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
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
        5,
        "supersede.json",
        serde_json::json!({"type": "request_checkpoint_supersession", "reason": "scope changed"}),
    )
    .success();
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        6,
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
        7,
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
        8
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["scope_revision"],
        1
    );
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "scope-amended",
    );
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        10,
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
    let (worker, reviewer) = claim_pair_and_approve_run_started(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
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
        5,
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
        5,
        "request.json",
        serde_json::json!({"type": "request_checkpoint_supersession", "reason": "new evidence"}),
    )
    .success();
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "supersession-pending",
    );
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        8,
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
        8,
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
    let (worker, reviewer) = claim_pair_and_approve_run_started(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
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
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "race-approval",
    );
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        7,
        "approve-first.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        7,
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
        8,
        "withdraw.json",
        serde_json::json!({"type": "withdraw_approval", "reason": "new evidence"}),
    )
    .success();

    let second = tempfile::tempdir().unwrap();
    init_pair(second.path());
    let (worker, reviewer) = claim_pair_and_approve_run_started(second.path());
    apply_action(
        second.path(),
        "worker",
        "worker-1",
        &worker,
        4,
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
        5,
        "request-first.json",
        serde_json::json!({"type": "request_checkpoint_supersession", "reason": "new evidence"}),
    )
    .success();
    apply_action(
        second.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        5,
        "stale-approval.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"revision_conflict""#));
    approve_current_explainer(
        second.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "race-pending",
    );
    apply_action(
        second.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        8,
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
    let (worker, reviewer) = claim_pair_and_approve_run_started(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
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
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "withdraw-approval",
    );
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        7,
        "approve.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        8,
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
        8,
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
        7,
        "mutation.json",
        explainer_publication_action(
            &current_obligation(dir.path()),
            &"e".repeat(64),
            "site-run-a",
            "edge-truncated",
            "https://sites.openai.test/site-run-a/edge-truncated",
        ),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"invalid_history""#));
    assert_eq!(std::fs::read(&baton_path).unwrap(), before);
    assert!(!dir
        .path()
        .join("history/00000000000000000008.json")
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
        7,
        "mutation.json",
        explainer_publication_action(
            &current_obligation(dir.path()),
            &"e".repeat(64),
            "site-run-a",
            "edge-scope",
            "https://sites.openai.test/site-run-a/edge-scope",
        ),
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

    let error = channel.compare_and_swap(7, &next).unwrap_err();
    assert!(matches!(error, StoreError::InvalidHistory));
    assert_eq!(channel.read().unwrap(), current);
    assert!(!dir
        .path()
        .join("history/00000000000000000008.json")
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
        dir.path().join("history/00000000000000000008.json"),
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
            "8",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_history""#));
    assert!(!dir
        .path()
        .join("history/00000000000000000009.json")
        .exists());
}

#[test]
fn store_edge_missing_expected_history_is_typed_and_non_mutating() {
    let dir = tempfile::tempdir().unwrap();
    let (worker, _) = setup_scope_amended_checkpoint(dir.path());
    let baton_path = dir.path().join("baton.json");
    let before = std::fs::read(&baton_path).unwrap();
    std::fs::remove_file(dir.path().join("history/00000000000000000007.json")).unwrap();

    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        7,
        "mutation.json",
        explainer_publication_action(
            &current_obligation(dir.path()),
            &"e".repeat(64),
            "site-run-a",
            "edge-missing",
            "https://sites.openai.test/site-run-a/edge-missing",
        ),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"invalid_history""#));
    assert_eq!(std::fs::read(&baton_path).unwrap(), before);
    assert!(!dir
        .path()
        .join("history/00000000000000000008.json")
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
        5,
        "supersede.json",
        serde_json::json!({
            "type": "request_checkpoint_supersession", "reason": "new evidence"
        }),
    )
    .success();

    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "supersession-changes",
    );

    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        8,
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
    let (worker, reviewer, binding) = setup_reviewing_checkpoint(dir.path());
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "approved-review-validation",
    );
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        7,
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
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "pending-finalize-review",
    );
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        7,
        "approve.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .success();
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "pending-finalize-gate",
    );
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
        10,
        "finalize.json",
        serde_json::json!({"type": "finalize"}),
    )
    .failure();
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baton_path).unwrap()).unwrap();
    assert_ne!(persisted["status"], "done");
}

#[test]
fn publication_rolling_gate_binds_every_semantic_handoff_without_recursive_staleness() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
    let first_checkpoint = checkpoint_submission(
        "checkpoint-a",
        serde_json::json!([
            {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
        ]),
    );

    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "blocked-first-checkpoint.json",
        first_checkpoint.clone(),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"publication_stale""#));
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "deploy-1",
    );
    let approved_run_started = read_baton(dir.path());
    assert_eq!(approved_run_started["revision"], 4);
    assert_eq!(
        approved_run_started["publication_binding"]["obligation"]["handoff_revision"],
        0
    );
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "first-checkpoint.json",
        first_checkpoint,
    )
    .success();
    let first_binding = checkpoint_binding(dir.path());
    let baton = read_baton(dir.path());
    assert_eq!(
        baton["publication_binding"]["obligation"]["kind"],
        "worker_to_reviewer"
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["handoff_revision"],
        5
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["scope_revision"],
        0
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["checkpoint"],
        first_binding
    );
    assert!(baton["publication_binding"]["deployment"].is_null());

    let changes = review_action(
        "changes_requested",
        &first_binding,
        serde_json::json!(["Handle empty input"]),
    );
    apply_action_raw(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        5,
        "blocked-review.json",
        changes.clone(),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"publication_stale""#));
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "deploy-2",
    );
    apply_action_raw(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        7,
        "changes.json",
        changes,
    )
    .success();
    let baton = read_baton(dir.path());
    assert_eq!(baton["status"], "revising");
    assert_eq!(baton["assignee"], "worker");
    assert_eq!(
        baton["publication_binding"]["obligation"]["kind"],
        "reviewer_to_worker"
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["handoff_revision"],
        8
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["checkpoint"],
        first_binding
    );

    let revised_checkpoint = checkpoint_submission(
        "checkpoint-b",
        serde_json::json!([
            {"id": "implementation", "artifacts": [{"kind": "commit", "value": "def"}]}
        ]),
    );
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        8,
        "blocked-revision.json",
        revised_checkpoint.clone(),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"publication_stale""#));
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "deploy-3",
    );
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        10,
        "revised-checkpoint.json",
        revised_checkpoint,
    )
    .success();
    let revised_binding = checkpoint_binding(dir.path());
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "deploy-4",
    );
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        13,
        "approve-checkpoint.json",
        review_action("approved", &revised_binding, serde_json::json!([])),
    )
    .success();
    let baton = read_baton(dir.path());
    assert_eq!(baton["status"], "finalizing");
    assert_eq!(
        baton["publication_binding"]["obligation"]["kind"],
        "reviewer_to_worker"
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["handoff_revision"],
        14
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["checkpoint"],
        revised_binding
    );
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        14,
        "blocked-finalize.json",
        serde_json::json!({"type": "finalize"}),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"publication_stale""#));
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "deploy-5",
    );
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        16,
        "finalize.json",
        serde_json::json!({"type": "finalize"}),
    )
    .success();
    assert_eq!(read_baton(dir.path())["status"], "done");
}

#[test]
fn publication_receipts_are_exact_authorized_and_keep_one_stable_site() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
    let obligation = current_obligation(dir.path());
    let valid = explainer_publication_action(
        &obligation,
        &"a".repeat(64),
        "site-run-a",
        "deployment-1",
        "https://sites.openai.test/site-run-a/deployment-1",
    );

    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        2,
        "wrong-publisher.json",
        valid.clone(),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"wrong_publisher_harness""#,
    ));
    for (index, mutate) in [
        ("source_digest", serde_json::json!("A".repeat(64))),
        ("source_digest", serde_json::json!("a".repeat(63))),
        ("site_id", serde_json::json!(" ")),
        ("site_version", serde_json::json!(" ")),
        ("url", serde_json::json!(" ")),
        ("channel", serde_json::json!("local")),
        ("access", serde_json::json!("public")),
    ]
    .into_iter()
    .enumerate()
    {
        let mut action = valid.clone();
        action[mutate.0] = mutate.1;
        apply_action(
            dir.path(),
            "worker",
            "worker-1",
            &worker,
            2,
            &format!("invalid-deployment-{index}.json"),
            action,
        )
        .failure()
        .stderr(predicate::str::contains(
            r#""error":"invalid_explainer_publication""#,
        ));
    }
    for (index, stale_obligation) in [
        serde_json::json!({"handoff_revision": 99, "kind": "run_started", "scope_revision": 0}),
        serde_json::json!({"handoff_revision": 0, "kind": "scope_amended", "scope_revision": 0}),
        serde_json::json!({"handoff_revision": 0, "kind": "run_started", "scope_revision": 1}),
        serde_json::json!({
            "handoff_revision": 0, "kind": "run_started", "scope_revision": 0,
            "checkpoint": {
                "checkpoint_identity": "stale", "manifest_digest": "0".repeat(64),
                "scope_revision": 0
            }
        }),
    ]
    .into_iter()
    .enumerate()
    {
        let mut stale = valid.clone();
        stale["obligation"] = stale_obligation;
        apply_action(
            dir.path(),
            "worker",
            "worker-1",
            &worker,
            2,
            &format!("stale-deployment-{index}.json"),
            stale,
        )
        .failure()
        .stderr(predicate::str::contains(
            r#""error":"stale_publication_binding""#,
        ));
    }
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "valid-deployment.json",
        valid,
    )
    .success();
    let baton = read_baton(dir.path());
    let deployment = baton["publication_binding"]["deployment"].clone();
    assert_eq!(deployment["obligation"], obligation);
    assert_eq!(deployment["publisher_harness"], "Codex");
    assert_eq!(deployment["channel"], "codex_sites");
    assert_eq!(deployment["access"], "owner_only");

    let approved =
        explainer_review_action(&obligation, &deployment, "approved", serde_json::json!([]));
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "wrong-reviewer.json",
        approved.clone(),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"wrong_reviewer_harness""#,
    ));
    for (index, (coordinate, stale_value)) in [
        ("source_digest", serde_json::json!("b".repeat(64))),
        ("site_id", serde_json::json!("site-old")),
        ("site_version", serde_json::json!("deployment-old")),
        ("url", serde_json::json!("https://sites.openai.test/stale")),
    ]
    .into_iter()
    .enumerate()
    {
        let mut stale = approved.clone();
        stale[coordinate] = stale_value;
        apply_action(
            dir.path(),
            "reviewer",
            "reviewer-1",
            &reviewer,
            3,
            &format!("stale-review-{index}.json"),
            stale,
        )
        .failure()
        .stderr(predicate::str::contains(
            r#""error":"stale_publication_binding""#,
        ));
    }
    let mut approved_with_findings = approved.clone();
    approved_with_findings["findings"] = serde_json::json!(["blocking"]);
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "approved-with-findings.json",
        approved_with_findings,
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"blocking_findings""#));
    let changes_without_findings = explainer_review_action(
        &obligation,
        &deployment,
        "changes_requested",
        serde_json::json!([]),
    );
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "changes-without-findings.json",
        changes_without_findings,
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"missing_findings""#));
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "approved.json",
        approved,
    )
    .success();

    let mut different_site = explainer_publication_action(
        &obligation,
        &"b".repeat(64),
        "site-other",
        "deployment-2",
        "https://sites.openai.test/site-other/deployment-2",
    );
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "different-site.json",
        different_site.clone(),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"site_id_mismatch""#));
    different_site["site_id"] = serde_json::json!("site-run-a");
    different_site["url"] = serde_json::json!("https://sites.openai.test/site-run-a/deployment-2");
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "republish.json",
        different_site,
    )
    .success();
    let baton = read_baton(dir.path());
    assert!(baton["publication_binding"]["review"].is_null());
    assert_eq!(baton["publication_binding"]["obligation"], obligation);
}

#[test]
fn publication_store_rejects_moved_receipts_and_site_identity_rewrites() {
    for (index, (pointer, value)) in [
        (
            "/publication_binding/deployment/obligation/handoff_revision",
            serde_json::json!(1),
        ),
        (
            "/publication_binding/review/obligation/kind",
            serde_json::json!("scope_amended"),
        ),
        (
            "/publication_binding/deployment/publisher_harness",
            serde_json::json!("Claude"),
        ),
        (
            "/publication_binding/review/reviewer_harness",
            serde_json::json!("Codex"),
        ),
        ("/publication_policy/channel", serde_json::json!("local")),
    ]
    .into_iter()
    .enumerate()
    {
        let dir = tempfile::tempdir().unwrap();
        init_pair(dir.path());
        let worker = claim_role(dir.path(), "worker", "worker-1", 0);
        let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
        approve_current_explainer(
            dir.path(),
            ("worker", "worker-1", &worker),
            ("reviewer", "reviewer-1", &reviewer),
            "deployment-1",
        );
        let baton_path = dir.path().join("baton.json");
        let mut baton = read_baton(dir.path());
        *baton.pointer_mut(pointer).unwrap() = value;
        std::fs::write(&baton_path, serde_json::to_vec_pretty(&baton).unwrap()).unwrap();
        command()
            .args(["read", "--run-dir", dir.path().to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains(r#""error":"invalid_baton""#));
        assert!(
            !dir.path()
                .join("history/00000000000000000005.json")
                .exists(),
            "tamper case {index} mutated history"
        );
    }

    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "deployment-1",
    );
    let channel = RunChannel::open(dir.path());
    let current = channel.read().unwrap();
    let mut rewritten = current.clone();
    rewritten.revision += 1;
    let binding = rewritten.publication_binding.as_mut().unwrap();
    binding.site_id = Some("site-other".to_owned());
    binding.deployment.as_mut().unwrap().site_id = "site-other".to_owned();
    binding.review.as_mut().unwrap().site_id = "site-other".to_owned();
    let error = channel
        .compare_and_swap(current.revision, &rewritten)
        .unwrap_err();
    assert!(matches!(error, StoreError::InvalidHistory));
    assert_eq!(channel.read().unwrap(), current);
}

#[test]
fn publication_history_edge_rejects_unjustified_obligation_and_dual_receipt_write() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    claim_role(dir.path(), "worker", "worker-1", 0);
    claim_role(dir.path(), "reviewer", "reviewer-1", 1);
    let channel = RunChannel::open(dir.path());
    let current = channel.read().unwrap();

    let mut unjustified = serde_json::to_value(current.clone()).unwrap();
    unjustified["revision"] = serde_json::json!(3);
    unjustified["publication_binding"] = serde_json::json!({
        "obligation": {
            "handoff_revision": 3, "kind": "scope_amended", "scope_revision": 0
        },
        "deployment": null,
        "review": null
    });
    let unjustified: RunBaton = serde_json::from_value(unjustified).unwrap();
    assert!(matches!(
        channel.compare_and_swap(2, &unjustified),
        Err(StoreError::InvalidHistory)
    ));

    let mut dual = serde_json::to_value(current.clone()).unwrap();
    dual["revision"] = serde_json::json!(3);
    dual["publication_binding"] = approved_publication_binding(
        dual["publication_binding"]["obligation"].clone(),
        "site-run-a",
        "deployment-dual",
    );
    let dual: RunBaton = serde_json::from_value(dual).unwrap();
    assert!(matches!(
        channel.compare_and_swap(2, &dual),
        Err(StoreError::InvalidHistory)
    ));
    assert_eq!(channel.read().unwrap(), current);
}

#[test]
fn publication_recovery_rejects_an_illegal_v2_successor_edge() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    claim_role(dir.path(), "worker", "worker-1", 0);
    claim_role(dir.path(), "reviewer", "reviewer-1", 1);
    let channel = RunChannel::open(dir.path());
    let current = channel.read().unwrap();
    let mut illegal = serde_json::to_value(current).unwrap();
    illegal["revision"] = serde_json::json!(3);
    illegal["publication_binding"] = serde_json::json!({
        "obligation": {
            "handoff_revision": 3, "kind": "scope_amended", "scope_revision": 0
        },
        "deployment": null,
        "review": null
    });
    std::fs::write(
        dir.path().join("history/00000000000000000003.json"),
        serde_json::to_vec_pretty(&illegal).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.path().join("baton.json"), b"corrupt\n").unwrap();

    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "3",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_history""#));
    assert!(!dir
        .path()
        .join("history/00000000000000000004.json")
        .exists());
}

#[test]
fn publication_old_checkpoint_approval_cannot_finalize_the_current_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "deployment-1",
    );
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "checkpoint-old.json",
        checkpoint_submission(
            "checkpoint-old",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "old"}]}
            ]),
        ),
    )
    .success();
    let old_checkpoint = checkpoint_binding(dir.path());
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "deployment-2",
    );
    apply_action_raw(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        7,
        "changes-old.json",
        review_action(
            "changes_requested",
            &old_checkpoint,
            serde_json::json!(["Revise it"]),
        ),
    )
    .success();
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "deployment-old-approved",
    );
    let old_approved_binding = read_baton(dir.path())["publication_binding"].clone();
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        10,
        "checkpoint-current.json",
        checkpoint_submission(
            "checkpoint-current",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "current"}]}
            ]),
        ),
    )
    .success();
    let current_checkpoint = checkpoint_binding(dir.path());
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "deployment-current-review",
    );
    apply_action_raw(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        13,
        "approve-current.json",
        review_action("approved", &current_checkpoint, serde_json::json!([])),
    )
    .success();

    let baton_path = dir.path().join("baton.json");
    let mut attacked = read_baton(dir.path());
    attacked["publication_binding"] = old_approved_binding;
    let bytes = serde_json::to_vec_pretty(&attacked).unwrap();
    std::fs::write(&baton_path, &bytes).unwrap();
    std::fs::write(dir.path().join("history/00000000000000000014.json"), bytes).unwrap();
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        14,
        "finalize-with-old-explainer.json",
        serde_json::json!({"type": "finalize"}),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"publication_stale""#));
    assert_eq!(read_baton(dir.path())["status"], "finalizing");
}

#[test]
fn publication_schema_decode_preserves_v1_default_and_rejects_v2_numeric_key() {
    let legacy = tempfile::tempdir().unwrap();
    write_legacy_run(legacy.path(), "working", "worker", serde_json::Value::Null);
    for path in [
        legacy.path().join("baton.json"),
        legacy.path().join("history/00000000000000000000.json"),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value.as_object_mut().unwrap().remove("publication");
        std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
    let output = command()
        .args(["read", "--run-dir", legacy.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let decoded: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(decoded["publication"]["required"], true);
    assert_eq!(decoded["publication"]["desired_revision"], 0);
    assert!(decoded["publication"]["published_revision"].is_null());

    let v2 = tempfile::tempdir().unwrap();
    init_pair(v2.path());
    let baton_path = v2.path().join("baton.json");
    let mut baton = read_baton(v2.path());
    baton["publication"] = serde_json::json!({
        "required": false, "desired_revision": 0,
        "published_revision": null, "refs": []
    });
    std::fs::write(&baton_path, serde_json::to_vec_pretty(&baton).unwrap()).unwrap();
    command()
        .args(["read", "--run-dir", v2.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_baton""#));
}

#[test]
fn publication_changes_requested_is_substate_only_and_legacy_numeric_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
    let obligation = current_obligation(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "legacy.json",
        serde_json::json!({
            "type": "record_publication", "required": true,
            "desired_revision": 99, "published_revision": 99,
            "refs": [{"kind": "explainer", "value": "https://mutable.invalid"}]
        }),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"legacy_publication_unsupported""#,
    ));
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "deploy.json",
        explainer_publication_action(
            &obligation,
            &"a".repeat(64),
            "site-run-a",
            "deployment-1",
            "https://sites.openai.test/site-run-a/deployment-1",
        ),
    )
    .success();
    let deployment = read_baton(dir.path())["publication_binding"]["deployment"].clone();
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "changes.json",
        explainer_review_action(
            &obligation,
            &deployment,
            "changes_requested",
            serde_json::json!(["Explain the current TODO"]),
        ),
    )
    .success();
    let baton = read_baton(dir.path());
    assert_eq!(baton["status"], "working");
    assert_eq!(baton["assignee"], "worker");
    assert!(baton["checkpoint"].is_null());
    assert!(baton["review"].is_null());
    assert_eq!(
        baton["publication_binding"]["review"]["verdict"],
        "changes_requested"
    );
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "still-blocked.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"publication_stale""#));
}

#[test]
fn reverse_casting_keeps_codex_publisher_and_claude_explainer_reviewer() {
    let dir = tempfile::tempdir().unwrap();
    command()
        .args([
            "init",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--run-id",
            "run-reverse",
            "--objective",
            "Implement DEF-123",
            "--worker",
            "claude",
            "--reviewer",
            "codex",
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
            "--required-deliverable",
            "implementation=Implement DEF-123",
        ])
        .assert()
        .success();
    let worker = claim_role(dir.path(), "worker", "claude-worker", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "codex-reviewer", 1);
    let obligation = current_obligation(dir.path());
    let deployment_action = explainer_publication_action(
        &obligation,
        &"a".repeat(64),
        "site-reverse",
        "deployment-1",
        "https://sites.openai.test/site-reverse/deployment-1",
    );
    apply_action(
        dir.path(),
        "worker",
        "claude-worker",
        &worker,
        2,
        "claude-cannot-publish.json",
        deployment_action.clone(),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"wrong_publisher_harness""#,
    ));
    apply_action(
        dir.path(),
        "reviewer",
        "codex-reviewer",
        &reviewer,
        2,
        "codex-publishes.json",
        deployment_action,
    )
    .success();
    let deployment = read_baton(dir.path())["publication_binding"]["deployment"].clone();
    let approval =
        explainer_review_action(&obligation, &deployment, "approved", serde_json::json!([]));
    apply_action(
        dir.path(),
        "reviewer",
        "codex-reviewer",
        &reviewer,
        3,
        "codex-cannot-review.json",
        approval.clone(),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"wrong_reviewer_harness""#,
    ));
    apply_action(
        dir.path(),
        "worker",
        "claude-worker",
        &worker,
        3,
        "claude-reviews.json",
        approval,
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "claude-worker",
        &worker,
        4,
        "checkpoint.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .success();
    let baton = read_baton(dir.path());
    assert_eq!(
        baton["publication_binding"]["obligation"]["kind"],
        "worker_to_reviewer"
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["checkpoint"],
        checkpoint_binding(dir.path())
    );
}
