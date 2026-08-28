use assert_cmd::Command;
use dvandva_v4::action::Action;
use dvandva_v4::claim::{self, ClaimError, Role};
use dvandva_v4::model::{DeliverableRequirement, RunBaton, TaskIdentity};
use dvandva_v4::store::{migrate_legacy_baton, RunChannel, StoreError};
use dvandva_v4::transition::{self, TransitionError};
use fs2::FileExt;
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

fn digest_of(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

/// Map a readable test label onto a valid sha256 coordinate, preserving any
/// surrounding whitespace so the kernel's trimming stays under test.
fn checkpoint_identity(label: &str) -> String {
    let leading = &label[..label.len() - label.trim_start().len()];
    let trailing = &label[label.trim_end().len()..];
    format!("{leading}{}{trailing}", digest_of(label.trim()))
}

fn checkpoint_submission(identity: &str, deliverables: serde_json::Value) -> serde_json::Value {
    let deliverables = deliverables
        .as_array()
        .expect("deliverables are an array")
        .iter()
        .map(|deliverable| {
            let artifacts = deliverable["artifacts"]
                .as_array()
                .expect("artifacts are an array")
                .iter()
                .map(|artifact| {
                    // Blank coordinates stay blank so invalid-manifest fixtures
                    // keep testing what they were written to test.
                    let kind = artifact["kind"].as_str().unwrap_or_default();
                    let value = artifact["value"].as_str().unwrap_or_default();
                    serde_json::json!({
                        "kind": if kind.trim().is_empty() { kind.to_owned() }
                                else { "analysis_digest".to_owned() },
                        "value": if value.trim().is_empty() { value.to_owned() }
                                 else { checkpoint_identity(value) }
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({"id": deliverable["id"], "artifacts": artifacts})
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "type": "submit_checkpoint",
        "checkpoint": {
            "kind": "analysis",
            "identity": checkpoint_identity(identity),
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
    artifact: &serde_json::Value,
    verdict: &str,
    findings: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "type": "record_explainer_review",
        "obligation": obligation,
        "source_digest": artifact["source_digest"],
        "verdict": verdict,
        "findings": findings
    })
}

/// Write explainer bytes outside the run directory and return the action that
/// stages them, plus their digest.
fn stage_explainer_action(
    dir: &std::path::Path,
    obligation: &serde_json::Value,
    label: &str,
) -> (serde_json::Value, String) {
    let bytes = format!("<h1>{label}</h1>").into_bytes();
    let source = dir.join(format!(".source-{label}.html"));
    std::fs::write(&source, &bytes).unwrap();
    let digest = format!("{:x}", Sha256::digest(&bytes));
    (
        serde_json::json!({
            "type": "stage_explainer",
            "obligation": obligation,
            "source_path": source.to_str().unwrap()
        }),
        digest,
    )
}

fn approved_publication_binding(
    obligation: serde_json::Value,
    site_id: &str,
    site_version: &str,
) -> serde_json::Value {
    let source_digest = "c".repeat(64);
    let artifact = serde_json::json!({
        "obligation": obligation,
        "source_digest": source_digest,
        "path": format!("explainer/{source_digest}.html"),
        "media_type": "text/html",
        "byte_length": 32,
        "channel": "run_artifact",
        "access": "run_private",
        "publisher_harness": "Codex"
    });
    let deployment = serde_json::json!({
        "obligation": obligation,
        "source_digest": source_digest,
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
        "artifact": artifact,
        "deployment": deployment,
        "review": {
            "obligation": obligation,
            "source_digest": source_digest,
            "verdict": "approved",
            "reviewer_harness": "Claude"
        }
    })
}

fn participant_claim_json(
    session_id: &str,
    epoch: u64,
    token_byte: char,
    lease_started_at: &str,
    lease_expires_at: &str,
    lease_seconds: u64,
) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "epoch": epoch,
        "token_digest": token_byte.to_string().repeat(64),
        "lease_started_at": lease_started_at,
        "lease_expires_at": lease_expires_at,
        "lease_seconds": lease_seconds
    })
}

fn shifted_rfc3339(value: &str, seconds: i64) -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::parse(value, &Rfc3339)
        .unwrap()
        .checked_add(time::Duration::seconds(seconds))
        .unwrap()
        .format(&Rfc3339)
        .unwrap()
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
    let (stage, digest) = stage_explainer_action(dir, &obligation, site_version);
    apply_action(
        dir,
        codex_role,
        codex_session,
        codex_token,
        revision,
        &format!("stage-{site_version}.json"),
        stage,
    )
    .success();
    // The Site rendering is optional and never gates; tests that exercise it
    // publish explicitly. The gate binds the staged bytes.
    let _ = &digest;
    let baton = read_baton(dir);
    let artifact = baton["publication_binding"]["artifact"].clone();
    apply_action(
        dir,
        claude_role,
        claude_session,
        claude_token,
        revision + 1,
        &format!("review-{site_version}.json"),
        explainer_review_action(&obligation, &artifact, "approved", serde_json::json!([])),
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
            "evidence": ["New requirement"], "options": ["yes", "no"]
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

fn setup_taskless_legacy_human_decision(dir: &std::path::Path) -> String {
    write_legacy_run(dir, "working", "worker", serde_json::Value::Null);
    for path in [
        dir.join("baton.json"),
        dir.join("history/00000000000000000000.json"),
    ] {
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        legacy["task"] = serde_json::Value::Null;
        std::fs::write(path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
    }

    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            dir.to_str().unwrap(),
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
            dir.join("credentials").to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(read_baton(dir)["task"].is_null());

    let worker = claim_role(dir, "worker", "worker-new", 1);
    apply_action(
        dir,
        "worker",
        "worker-new",
        &worker,
        2,
        "request-scope.json",
        serde_json::json!({
            "type": "request_human_decision",
            "question": "Which ticket now defines the run?",
            "evidence": ["The legacy run had no task identity"],
            "options": ["Adopt DEF-456", "Keep the run taskless"]
        }),
    )
    .success();
    worker
}

fn forged_migration_candidate(source: &RunBaton) -> RunBaton {
    let mut eligible = source.clone();
    eligible.status = dvandva_v4::model::Status::Working;
    eligible.assignee = dvandva_v4::model::Assignee::Worker;
    eligible.participants.worker.claim = None;
    eligible.participants.reviewer.claim = None;
    eligible.terminal = None;
    let mut next = migrate_legacy_baton(&eligible).unwrap();
    let migration = next.migration.as_mut().unwrap();
    migration.legacy_state_digest =
        format!("{:x}", Sha256::digest(serde_json::to_vec(source).unwrap()));
    migration.legacy_checkpoint = source.checkpoint.clone();
    next
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
            "publisher_harness": "Codex", "channel": "run_artifact",
            "access": "run_private", "reviewer_harness": "Claude"
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
fn scope_amendment_adds_human_approved_task_identity_after_taskless_legacy_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let worker = setup_taskless_legacy_human_decision(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-new",
        &worker,
        3,
        "amend-scope.json",
        serde_json::json!({
            "type": "resume_human_decision",
            "answer": "Adopt DEF-456",
            "scope_amendment": {
                "objective": " Implement the approved recovery ticket ",
                "objective_refs": [{"kind": "issue", "value": "DEF-456"}],
                "task_reference": " DEF-456 ",
                "scope_deliverables": [{
                    "id": "legacy_objective",
                    "description": "Implement the approved recovery ticket"
                }]
            }
        }),
    )
    .success();

    let amended = read_baton(dir.path());
    assert_eq!(amended["revision"], 4);
    assert_eq!(amended["scope_revision"], 1);
    assert_eq!(amended["task"]["reference"], "DEF-456");
    assert_eq!(
        amended["task"]["summary"],
        "Implement the approved recovery ticket"
    );
    assert_eq!(amended["status"], "revising");
    assert_eq!(amended["assignee"], "worker");
    assert_eq!(
        amended["publication_binding"]["obligation"],
        serde_json::json!({
            "handoff_revision": 4,
            "kind": "scope_amended",
            "scope_revision": 1
        })
    );

    std::fs::remove_file(dir.path().join("baton.json")).unwrap();
    command()
        .args([
            "recover",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--from-revision",
            "4",
        ])
        .assert()
        .success();
    let reopened = RunChannel::open(dir.path()).read().unwrap();
    assert_eq!(reopened.task.unwrap().reference.as_deref(), Some("DEF-456"));
}

#[test]
fn scope_amendment_without_task_reference_keeps_upgraded_taskless_run_taskless() {
    let dir = tempfile::tempdir().unwrap();
    let worker = setup_taskless_legacy_human_decision(dir.path());
    apply_action(
        dir.path(),
        "worker",
        "worker-new",
        &worker,
        3,
        "amend-scope.json",
        serde_json::json!({
            "type": "resume_human_decision",
            "answer": "Keep the run taskless",
            "scope_amendment": {
                "objective": "Continue the taskless legacy objective",
                "objective_refs": [],
                "task_reference": null,
                "scope_deliverables": [{
                    "id": "legacy_objective",
                    "description": "Continue the taskless legacy objective"
                }]
            }
        }),
    )
    .success();

    let amended = read_baton(dir.path());
    assert_eq!(amended["revision"], 4);
    assert_eq!(amended["scope_revision"], 1);
    assert!(amended["task"].is_null());
    assert_eq!(amended["status"], "revising");
    assert_eq!(amended["assignee"], "worker");
}

#[test]
fn migration_integrity_direct_cas_rejects_terminal_and_live_claim_sources() {
    for source_kind in [
        "terminal",
        "terminal_provenance",
        "worker_live",
        "reviewer_live",
        "malformed_claim",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let worker_claim = matches!(source_kind, "worker_live" | "malformed_claim").then(|| {
            serde_json::json!({
                "session_id": "live-worker", "epoch": 1, "token_digest": "digest",
                "lease_expires_at": if source_kind == "malformed_claim" { "not-a-time" } else { "2999-01-01T00:00:00Z" },
                "lease_seconds": 300
            })
        });
        write_legacy_run(
            dir.path(),
            if source_kind == "terminal" {
                "done"
            } else {
                "working"
            },
            if source_kind == "terminal" {
                "none"
            } else {
                "worker"
            },
            worker_claim.unwrap_or(serde_json::Value::Null),
        );
        if source_kind == "reviewer_live" {
            for path in [
                dir.path().join("baton.json"),
                dir.path().join("history/00000000000000000000.json"),
            ] {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                value["participants"]["reviewer"]["claim"] = serde_json::json!({
                    "session_id": "live-reviewer", "epoch": 1, "token_digest": "digest",
                    "lease_expires_at": "2999-01-01T00:00:00Z", "lease_seconds": 300
                });
                std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
            }
        }
        if source_kind == "terminal_provenance" {
            for path in [
                dir.path().join("baton.json"),
                dir.path().join("history/00000000000000000000.json"),
            ] {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                value["terminal"] = serde_json::json!({"outcome": "abandoned", "reason": "forged"});
                std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
            }
        }
        let channel = RunChannel::open(dir.path());
        let source = channel.read().unwrap();
        let forged = forged_migration_candidate(&source);
        assert!(
            matches!(
                channel.compare_and_swap(0, &forged),
                Err(StoreError::MigrationRequired)
            ),
            "illegal migration source {source_kind} was accepted"
        );
        assert_eq!(channel.read().unwrap(), source);
        assert!(!dir
            .path()
            .join("history/00000000000000000001.json")
            .exists());
    }
}

#[test]
fn migration_integrity_recovery_rejects_forged_illegal_crossings() {
    for source_kind in [
        "terminal",
        "terminal_provenance",
        "worker_live",
        "reviewer_live",
        "malformed_claim",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let worker_claim = matches!(source_kind, "worker_live" | "malformed_claim").then(|| {
            serde_json::json!({
                "session_id": "live-worker", "epoch": 1, "token_digest": "digest",
                "lease_expires_at": if source_kind == "malformed_claim" { "not-a-time" } else { "2999-01-01T00:00:00Z" },
                "lease_seconds": 300
            })
        });
        write_legacy_run(
            dir.path(),
            if source_kind == "terminal" {
                "done"
            } else {
                "working"
            },
            if source_kind == "terminal" {
                "none"
            } else {
                "worker"
            },
            worker_claim.unwrap_or(serde_json::Value::Null),
        );
        if source_kind == "reviewer_live" {
            for path in [
                dir.path().join("baton.json"),
                dir.path().join("history/00000000000000000000.json"),
            ] {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                value["participants"]["reviewer"]["claim"] = serde_json::json!({
                    "session_id": "live-reviewer", "epoch": 1, "token_digest": "digest",
                    "lease_expires_at": "2999-01-01T00:00:00Z", "lease_seconds": 300
                });
                std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
            }
        }
        if source_kind == "terminal_provenance" {
            for path in [
                dir.path().join("baton.json"),
                dir.path().join("history/00000000000000000000.json"),
            ] {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                value["terminal"] = serde_json::json!({"outcome": "abandoned", "reason": "forged"});
                std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
            }
        }
        let source = RunChannel::open(dir.path()).read().unwrap();
        let forged = forged_migration_candidate(&source);
        std::fs::write(
            dir.path().join("history/00000000000000000001.json"),
            serde_json::to_vec_pretty(&forged).unwrap(),
        )
        .unwrap();
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
            .stderr(predicate::str::contains(r#""error":"invalid_history""#));
        assert!(!dir
            .path()
            .join("history/00000000000000000002.json")
            .exists());
    }
}

#[test]
fn migration_integrity_expired_claims_upgrade_and_are_cleared() {
    let dir = tempfile::tempdir().unwrap();
    write_legacy_run(
        dir.path(),
        "working",
        "worker",
        serde_json::json!({
            "session_id": "expired-worker", "epoch": 7, "token_digest": "digest",
            "lease_expires_at": "2000-01-01T00:00:00Z", "lease_seconds": 300
        }),
    );
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
            "new-worker",
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
    let upgraded = read_baton(dir.path());
    assert!(upgraded["participants"]["worker"]["claim"].is_null());
    assert!(upgraded["participants"]["reviewer"]["claim"].is_null());
    assert!(upgraded["migration"]["migrated_at"].is_string());
}

#[test]
fn migration_integrity_generic_cas_rejects_even_an_eligible_crossing() {
    let dir = tempfile::tempdir().unwrap();
    write_legacy_run(dir.path(), "working", "worker", serde_json::Value::Null);
    let channel = RunChannel::open(dir.path());
    let source = channel.read().unwrap();
    let next = migrate_legacy_baton(&source).unwrap();
    assert!(matches!(
        channel.compare_and_swap(0, &next),
        Err(StoreError::MigrationRequired)
    ));
    assert_eq!(channel.read().unwrap(), source);
    assert!(!dir
        .path()
        .join("history/00000000000000000001.json")
        .exists());
}

#[test]
fn legacy_cas_rejects_task_identity_mutations_without_writing_history() {
    let cases = [
        (
            "create",
            None,
            Some(TaskIdentity {
                reference: Some("DEF-456".to_owned()),
                summary: "Created identity".to_owned(),
            }),
        ),
        (
            "erase",
            Some(TaskIdentity {
                reference: Some("DEF-123".to_owned()),
                summary: "Preserved objective".to_owned(),
            }),
            None,
        ),
        (
            "mutate",
            Some(TaskIdentity {
                reference: Some("DEF-123".to_owned()),
                summary: "Preserved objective".to_owned(),
            }),
            Some(TaskIdentity {
                reference: Some("DEF-789".to_owned()),
                summary: "Mutated identity".to_owned(),
            }),
        ),
        (
            "blank",
            Some(TaskIdentity {
                reference: Some("DEF-123".to_owned()),
                summary: "Preserved objective".to_owned(),
            }),
            Some(TaskIdentity {
                reference: Some(" ".to_owned()),
                summary: " ".to_owned(),
            }),
        ),
    ];

    for (case, source_task, next_task) in cases {
        let dir = tempfile::tempdir().unwrap();
        write_legacy_run(dir.path(), "working", "worker", serde_json::Value::Null);
        for path in [
            dir.path().join("baton.json"),
            dir.path().join("history/00000000000000000000.json"),
        ] {
            let mut source: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            source["task"] = serde_json::to_value(source_task.clone()).unwrap();
            std::fs::write(path, serde_json::to_vec_pretty(&source).unwrap()).unwrap();
        }

        let head_path = dir.path().join("baton.json");
        let original_bytes = std::fs::read(&head_path).unwrap();
        let channel = RunChannel::open(dir.path());
        let original = channel.read().unwrap();
        let mut next = original.clone();
        next.revision = 1;
        next.task = next_task;

        assert!(
            matches!(
                channel.compare_and_swap(0, &next),
                Err(StoreError::MigrationRequired)
            ),
            "legacy CAS accepted the {case} task-identity mutation"
        );
        assert_eq!(
            std::fs::read(&head_path).unwrap(),
            original_bytes,
            "legacy CAS rewrote the {case} source head"
        );
        assert_eq!(
            channel.read().unwrap(),
            original,
            "legacy CAS structurally changed the {case} source head"
        );
        assert!(
            !dir.path()
                .join("history/00000000000000000001.json")
                .exists(),
            "legacy CAS wrote successor history for the {case} mutation"
        );
    }
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
    assert!(!busy
        .path()
        .join("history/00000000000000000001.json")
        .exists());

    let reviewer_busy = tempfile::tempdir().unwrap();
    write_legacy_run(
        reviewer_busy.path(),
        "working",
        "worker",
        serde_json::Value::Null,
    );
    for path in [
        reviewer_busy.path().join("baton.json"),
        reviewer_busy
            .path()
            .join("history/00000000000000000000.json"),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["participants"]["reviewer"]["claim"]["lease_expires_at"] =
            serde_json::json!("2999-01-01T00:00:00Z");
        std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            reviewer_busy.path().to_str().unwrap(),
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
            reviewer_busy.path().join("credentials").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("busy"));

    let malformed = tempfile::tempdir().unwrap();
    write_legacy_run(
        malformed.path(),
        "working",
        "worker",
        serde_json::json!({
            "session_id": "broken", "epoch": 1, "token_digest": "digest",
            "lease_expires_at": "not-a-time", "lease_seconds": 300
        }),
    );
    command()
        .args([
            "role",
            "upgrade",
            "--api",
            "2",
            "--run-dir",
            malformed.path().to_str().unwrap(),
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
            malformed.path().join("credentials").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid_timestamp"));

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
    assert_eq!(
        baton["checkpoint"]["identity"],
        checkpoint_identity("sha256:second")
    );
    assert_eq!(
        baton["review"]["checkpoint_identity"],
        checkpoint_identity("sha256:second")
    );
}

#[test]
fn human_decision_resumes_authoritative_pre_request_owner() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);

    apply_action_raw(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        2,
        "pause.json",
        serde_json::json!({
            "type": "request_human_decision", "question": "Which API should win?",
            "evidence": ["Both variants pass tests"], "options": ["Keep A", "Keep B"]
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
    assert_eq!(baton["status"], "working");
    assert_eq!(baton["assignee"], "worker");
    assert_eq!(baton["human_decision"]["answer"], "Keep A");
}

#[test]
fn human_decision_derives_requester_contact_and_authoritative_resume_target() {
    let dir = tempfile::tempdir().unwrap();
    let (worker, _reviewer, _) = setup_reviewing_checkpoint(dir.path());

    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        5,
        "pause.json",
        serde_json::json!({
            "type": "request_human_decision",
            "question": "Should the new requirement enter scope?",
            "evidence": ["The ticket changed during review"],
            "options": ["Amend scope", "Keep current scope"]
        }),
    )
    .success();

    let paused: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(paused["human_decision"]["requested_by"], "worker");
    assert_eq!(paused["human_decision"]["contact_role"], "worker");
    assert_eq!(paused["human_decision"]["resume_status"], "reviewing");
    assert_eq!(paused["human_decision"]["resume_assignee"], "reviewer");

    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        6,
        "resume.json",
        serde_json::json!({"type": "resume_human_decision", "answer": "Keep current scope"}),
    )
    .success();

    let resumed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(resumed["status"], "reviewing");
    assert_eq!(resumed["assignee"], "reviewer");
}

#[test]
fn human_decision_rejects_legacy_caller_supplied_routing() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);

    for (index, legacy_field) in [
        ("contact_role", serde_json::json!("reviewer")),
        ("resume_status", serde_json::json!("reviewing")),
        ("resume_assignee", serde_json::json!("reviewer")),
    ]
    .into_iter()
    .enumerate()
    {
        let mut action = serde_json::json!({
            "type": "request_human_decision",
            "question": "Choose",
            "evidence": ["Evidence"],
            "options": ["A", "B"]
        });
        action[legacy_field.0] = legacy_field.1;
        apply_action_raw(
            dir.path(),
            "worker",
            "worker-1",
            &worker,
            1,
            &format!("legacy-{index}.json"),
            action,
        )
        .failure()
        .stderr(predicate::str::contains("unknown field"));
    }

    let baton: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("baton.json")).unwrap()).unwrap();
    assert_eq!(baton["revision"], 1);
    assert!(baton["human_decision"].is_null());
}

#[test]
fn unrelated_action_payloads_remain_forward_compatible_with_additive_fields() {
    let action: Action = serde_json::from_value(serde_json::json!({
        "type": "abandon",
        "reason": "Operator stopped the run",
        "future_metadata": {"source": "newer facade"}
    }))
    .unwrap();

    assert!(matches!(
        action,
        Action::Abandon { reason } if reason == "Operator stopped the run"
    ));
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
    assert_eq!(
        baton["checkpoint"]["identity"],
        checkpoint_identity("sha256:recover")
    );
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
    // An expected wait timeout is a normal idle outcome, reported on stdout.
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains(r#""wait_outcome": "idle_timeout""#));
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
fn leaked_history_staging_file_does_not_wedge_the_next_mutation() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    std::fs::write(
        dir.path().join("history/.00000000000000000001.leaked.tmp"),
        b"partial",
    )
    .unwrap();

    claim_role(dir.path(), "worker", "worker-1", 0);

    assert!(dir
        .path()
        .join("history/00000000000000000001.json")
        .is_file());
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
    assert_eq!(
        first["checkpoint"]["identity"],
        checkpoint_identity("checkpoint-a")
    );
    assert_eq!(first["checkpoint"]["scope_revision"], 0);
    let digest = first["checkpoint"]["manifest_digest"].as_str().unwrap();
    assert_eq!(
        digest,
        "67d908b2651b8bc97c46bf853848400533255b93b95f7c2b26837e4008373cf9"
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
        serde_json::json!({"checkpoint_identity": checkpoint_identity("checkpoint-a"), "manifest_digest": "0".repeat(64), "scope_revision": 0}),
        serde_json::json!({"checkpoint_identity": checkpoint_identity("checkpoint-a"), "manifest_digest": binding["manifest_digest"], "scope_revision": 1}),
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
            "evidence": ["New requirement"], "options": ["yes", "no"]
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
    .stderr(predicate::str::contains(r#""error":"invalid_history""#));
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
            "evidence": ["A report is required"], "options": ["yes", "no"]
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
    let before_gate = read_baton(dir.path());
    let before_gate: RunBaton = serde_json::from_value(before_gate).unwrap();
    assert!(
        dvandva_v4::next_action::classify(&before_gate, Role::Reviewer, "claude")
            .legal_actions
            .contains(&"accept_checkpoint_supersession")
    );
    let _ = &worker;
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        6,
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
        6,
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
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "race-withdrawal",
    );
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        10,
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
    let before_gate = read_baton(dir.path());
    let before_gate: RunBaton = serde_json::from_value(before_gate).unwrap();
    assert!(
        dvandva_v4::next_action::classify(&before_gate, Role::Worker, "codex")
            .legal_actions
            .contains(&"withdraw_approval")
    );
    let _ = &reviewer;
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
fn immutable_history_rejects_supersession_edges_binding_the_wrong_checkpoint() {
    use dvandva_v4::model::{
        create_bound_handoff_obligation, Assignee, HandoffKind, RunBaton, Status,
    };

    let supersession = tempfile::tempdir().unwrap();
    init_pair(supersession.path());
    let (worker, _reviewer) = claim_pair_and_approve_run_started(supersession.path());
    apply_action(
        supersession.path(),
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
        supersession.path(),
        "worker",
        "worker-1",
        &worker,
        5,
        "request.json",
        serde_json::json!({"type": "request_checkpoint_supersession", "reason": "new evidence"}),
    )
    .success();
    let current = RunChannel::open(supersession.path()).read().unwrap();
    let mut checkpoint = current.checkpoint.as_ref().unwrap().binding();
    checkpoint.checkpoint_identity = "f".repeat(64);
    let mut forged = current.clone();
    forged.revision += 1;
    forged.status = Status::Revising;
    forged.assignee = Assignee::Worker;
    forged.checkpoint = None;
    forged.review = None;
    forged.pending_checkpoint_supersession = None;
    forged.publication_binding = Some(create_bound_handoff_obligation(
        HandoffKind::CheckpointSuperseded,
        forged.revision,
        forged.scope_revision,
        Some(checkpoint),
    ));
    forged.publication_binding.as_mut().unwrap().site_id = current
        .publication_binding
        .as_ref()
        .unwrap()
        .site_id
        .clone();
    assert!(matches!(
        RunChannel::open(supersession.path()).compare_and_swap(current.revision, &forged),
        Err(StoreError::InvalidHistory | StoreError::InvalidBaton(_))
    ));
    assert_eq!(
        RunChannel::open(supersession.path()).read().unwrap(),
        current
    );

    let withdrawal = tempfile::tempdir().unwrap();
    init_pair(withdrawal.path());
    let (worker, reviewer) = claim_pair_and_approve_run_started(withdrawal.path());
    apply_action(
        withdrawal.path(),
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
    let binding = checkpoint_binding(withdrawal.path());
    approve_current_explainer(
        withdrawal.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "reviewed",
    );
    apply_action(
        withdrawal.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        7,
        "approve.json",
        review_action("approved", &binding, serde_json::json!([])),
    )
    .success();
    let current: RunBaton = RunChannel::open(withdrawal.path()).read().unwrap();
    let mut checkpoint = current.checkpoint.as_ref().unwrap().binding();
    checkpoint.checkpoint_identity = "f".repeat(64);
    let mut forged = current.clone();
    forged.revision += 1;
    forged.status = Status::Revising;
    forged.assignee = Assignee::Worker;
    forged.checkpoint = None;
    forged.review = None;
    forged.pending_checkpoint_supersession = None;
    forged.publication_binding = Some(create_bound_handoff_obligation(
        HandoffKind::ApprovalWithdrawn,
        forged.revision,
        forged.scope_revision,
        Some(checkpoint),
    ));
    forged.publication_binding.as_mut().unwrap().site_id = current
        .publication_binding
        .as_ref()
        .unwrap()
        .site_id
        .clone();
    assert!(matches!(
        RunChannel::open(withdrawal.path()).compare_and_swap(current.revision, &forged),
        Err(StoreError::InvalidHistory | StoreError::InvalidBaton(_))
    ));
    assert_eq!(RunChannel::open(withdrawal.path()).read().unwrap(), current);
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
fn publication_obligation_rolls_at_every_handoff_and_gates_only_finalize() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let worker = claim_role(dir.path(), "worker", "worker-1", 0);
    let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);

    // Every semantic handoff still rolls a fresh obligation, but none of them
    // waits on the explainer: a finished deliverable always has somewhere to land.
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "first-checkpoint.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
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
        3
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["checkpoint"],
        first_binding
    );
    assert!(baton["publication_binding"]["artifact"].is_null());

    apply_action_raw(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        3,
        "changes.json",
        review_action(
            "changes_requested",
            &first_binding,
            serde_json::json!(["Handle empty input"]),
        ),
    )
    .success();
    let baton = read_baton(dir.path());
    assert_eq!(baton["status"], "revising");
    assert_eq!(
        baton["publication_binding"]["obligation"]["kind"],
        "reviewer_to_worker"
    );
    assert_eq!(
        baton["publication_binding"]["obligation"]["handoff_revision"],
        4
    );

    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "revised-checkpoint.json",
        checkpoint_submission(
            "checkpoint-b",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "def"}]}
            ]),
        ),
    )
    .success();
    let revised_binding = checkpoint_binding(dir.path());
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        5,
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
        baton["publication_binding"]["obligation"]["checkpoint"],
        revised_binding
    );

    // Finalization is the one gate, and it binds the current obligation's bytes.
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        6,
        "blocked-finalize.json",
        serde_json::json!({"type": "finalize"}),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"explainer_not_staged""#,
    ));
    approve_current_explainer(
        dir.path(),
        ("worker", "worker-1", &worker),
        ("reviewer", "reviewer-1", &reviewer),
        "final",
    );
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        8,
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
    let (stage, digest) = stage_explainer_action(dir.path(), &obligation, "deployment-1");

    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        2,
        "wrong-publisher.json",
        stage.clone(),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"wrong_publisher_harness""#,
    ));
    for (index, source) in ["missing.html", "empty.html"].into_iter().enumerate() {
        let path = dir.path().join(source);
        if source == "empty.html" {
            std::fs::write(&path, b"").unwrap();
        }
        let mut action = stage.clone();
        action["source_path"] = serde_json::json!(path.to_str().unwrap());
        apply_action(
            dir.path(),
            "worker",
            "worker-1",
            &worker,
            2,
            &format!("invalid-source-{index}.json"),
            action,
        )
        .failure()
        .stderr(predicate::str::contains(
            r#""error":"invalid_explainer_source""#,
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
        let mut stale = stage.clone();
        stale["obligation"] = stale_obligation;
        apply_action(
            dir.path(),
            "worker",
            "worker-1",
            &worker,
            2,
            &format!("stale-stage-{index}.json"),
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
        "stage.json",
        stage,
    )
    .success();
    let baton = read_baton(dir.path());
    let artifact = baton["publication_binding"]["artifact"].clone();
    assert_eq!(artifact["obligation"], obligation);
    assert_eq!(artifact["source_digest"], digest);
    assert_eq!(artifact["channel"], "run_artifact");
    assert_eq!(artifact["access"], "run_private");
    assert_eq!(artifact["publisher_harness"], "Codex");
    assert!(dir
        .path()
        .join(format!("explainer/{digest}.html"))
        .is_file());

    // The Site is an optional rendering: it must name the staged bytes exactly.
    let valid_site = explainer_publication_action(
        &obligation,
        &digest,
        "site-run-a",
        "deployment-1",
        "https://sites.openai.test/site-run-a/deployment-1",
    );
    let mut wrong_digest = valid_site.clone();
    wrong_digest["source_digest"] = serde_json::json!("b".repeat(64));
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "site-wrong-digest.json",
        wrong_digest,
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"stale_publication_binding""#,
    ));
    for (index, coordinate) in ["site_id", "site_version", "url", "channel", "access"]
        .into_iter()
        .enumerate()
    {
        let mut action = valid_site.clone();
        action[coordinate] = serde_json::json!(" ");
        apply_action(
            dir.path(),
            "worker",
            "worker-1",
            &worker,
            3,
            &format!("invalid-site-{index}.json"),
            action,
        )
        .failure()
        .stderr(predicate::str::contains(
            r#""error":"invalid_explainer_publication""#,
        ));
    }
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "valid-site.json",
        valid_site,
    )
    .success();
    let baton = read_baton(dir.path());
    let deployment = baton["publication_binding"]["deployment"].clone();
    assert_eq!(deployment["source_digest"], digest);
    assert_eq!(deployment["channel"], "codex_sites");
    assert_eq!(deployment["access"], "owner_only");

    let approved =
        explainer_review_action(&obligation, &artifact, "approved", serde_json::json!([]));
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        4,
        "wrong-reviewer.json",
        approved.clone(),
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"wrong_reviewer_harness""#,
    ));
    let mut stale_review = approved.clone();
    stale_review["source_digest"] = serde_json::json!("b".repeat(64));
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "stale-review.json",
        stale_review,
    )
    .failure()
    .stderr(predicate::str::contains(
        r#""error":"stale_publication_binding""#,
    ));
    let mut approved_with_findings = approved.clone();
    approved_with_findings["findings"] = serde_json::json!(["blocking"]);
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "approved-with-findings.json",
        approved_with_findings,
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"blocking_findings""#));
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "changes-without-findings.json",
        explainer_review_action(
            &obligation,
            &artifact,
            "changes_requested",
            serde_json::json!([]),
        ),
    )
    .failure()
    .stderr(predicate::str::contains(r#""error":"missing_findings""#));
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "approved.json",
        approved,
    )
    .success();

    // One stable Site ID per run; a new version of the same bytes keeps the
    // approval, because the reviewer's verdict binds the bytes, not the URL.
    let mut different_site = explainer_publication_action(
        &obligation,
        &digest,
        "site-other",
        "deployment-2",
        "https://sites.openai.test/site-other/deployment-2",
    );
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        5,
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
        5,
        "republish.json",
        different_site,
    )
    .success();
    let baton = read_baton(dir.path());
    assert_eq!(
        baton["publication_binding"]["review"]["verdict"],
        "approved"
    );

    // Restaging different bytes does invalidate both the rendering and the review.
    let (restage, _) = stage_explainer_action(dir.path(), &obligation, "deployment-2");
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        6,
        "restage.json",
        restage,
    )
    .success();
    let baton = read_baton(dir.path());
    assert!(baton["publication_binding"]["deployment"].is_null());
    assert!(baton["publication_binding"]["review"].is_null());
    assert_eq!(baton["publication_binding"]["obligation"], obligation);
}

#[test]
fn publication_store_rejects_moved_receipts_and_site_identity_rewrites() {
    for (index, (pointer, value)) in [
        (
            "/publication_binding/artifact/obligation/handoff_revision",
            serde_json::json!(1),
        ),
        (
            "/publication_binding/review/obligation/kind",
            serde_json::json!("scope_amended"),
        ),
        (
            "/publication_binding/artifact/source_digest",
            serde_json::json!("b".repeat(64)),
        ),
        (
            "/publication_binding/artifact/path",
            serde_json::json!("explainer/elsewhere.html"),
        ),
        (
            "/publication_binding/review/reviewer_harness",
            serde_json::json!(" "),
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
    let obligation = current_obligation(dir.path());
    let digest = read_baton(dir.path())["publication_binding"]["artifact"]["source_digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let revision = read_baton(dir.path())["revision"].as_u64().unwrap();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        revision,
        "site.json",
        explainer_publication_action(
            &obligation,
            &digest,
            "site-run-a",
            "deployment-1",
            "https://sites.openai.test/site-run-a/deployment-1",
        ),
    )
    .success();
    let channel = RunChannel::open(dir.path());
    let current = channel.read().unwrap();
    let mut rewritten = current.clone();
    rewritten.revision += 1;
    let binding = rewritten.publication_binding.as_mut().unwrap();
    binding.site_id = Some("site-other".to_owned());
    binding.deployment.as_mut().unwrap().site_id = "site-other".to_owned();
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
fn publication_history_edge_rejects_every_unclassified_unchanged_binding_mutation() {
    for attack in [
        "scope_without_amendment",
        "participant_harnesses",
        "both_claims",
        "terminal_shortcut",
        "recovery_provenance",
        "checkpoint_history",
    ] {
        let dir = tempfile::tempdir().unwrap();
        init_pair(dir.path());
        let worker = claim_role(dir.path(), "worker", "worker-1", 0);
        let reviewer = claim_role(dir.path(), "reviewer", "reviewer-1", 1);
        approve_current_explainer(
            dir.path(),
            ("worker", "worker-1", &worker),
            ("reviewer", "reviewer-1", &reviewer),
            "unchanged-binding",
        );
        let channel = RunChannel::open(dir.path());
        let current = channel.read().unwrap();
        let mut next = serde_json::to_value(current.clone()).unwrap();
        next["revision"] = serde_json::json!(5);
        match attack {
            "scope_without_amendment" => {
                next["objective"]["summary"] = serde_json::json!("Forged objective");
                next["task"]["summary"] = serde_json::json!("Forged objective");
                next["scope_deliverables"][0]["description"] =
                    serde_json::json!("Forged deliverable");
            }
            "participant_harnesses" => {
                next["participants"]["worker"]["harness"] = serde_json::json!("Claude");
                next["participants"]["reviewer"]["harness"] = serde_json::json!("Codex");
            }
            "both_claims" => {
                next["participants"]["worker"]["claim"]["session_id"] =
                    serde_json::json!("forged-worker");
                next["participants"]["reviewer"]["claim"]["session_id"] =
                    serde_json::json!("forged-reviewer");
            }
            "terminal_shortcut" => {
                next["status"] = serde_json::json!("done");
                next["assignee"] = serde_json::json!("none");
                next["terminal"] = serde_json::json!({"outcome": "done", "reason": null});
            }
            "recovery_provenance" => {
                next["recovery"] = serde_json::json!({
                    "from_revision": 99, "previous_high_revision": 99
                });
            }
            "checkpoint_history" => {
                next["checkpoint_history"] = serde_json::json!([{
                    "checkpoint_identity": "forged-checkpoint",
                    "manifest_digest": "0".repeat(64),
                    "scope_revision": 0
                }]);
            }
            _ => unreachable!(),
        }
        let next: RunBaton = serde_json::from_value(next).unwrap();
        assert!(
            matches!(
                channel.compare_and_swap(current.revision, &next),
                Err(StoreError::InvalidHistory | StoreError::InvalidBaton(_))
            ),
            "unchanged-binding attack {attack} was accepted"
        );
        assert_eq!(channel.read().unwrap(), current);
        assert!(!dir
            .path()
            .join("history/00000000000000000005.json")
            .exists());
    }
}

#[test]
fn claim_history_direct_cas_rejects_unproved_lease_transitions() {
    for attack in [
        "active_replacement",
        "unbound_expiry",
        "duration_mismatch",
        "heartbeat_after_expiry",
    ] {
        let dir = tempfile::tempdir().unwrap();
        init_pair(dir.path());
        claim_role(dir.path(), "worker", "worker-a", 0);
        let channel = RunChannel::open(dir.path());
        let current = channel.read().unwrap();
        let mut attacked = serde_json::to_value(current.clone()).unwrap();
        let current_claim = &attacked["participants"]["worker"]["claim"];
        let current_start = current_claim["lease_started_at"].as_str().unwrap();
        let current_expiry = current_claim["lease_expires_at"].as_str().unwrap();
        let renewed_start = shifted_rfc3339(current_start, 60);
        let renewed_expiry = shifted_rfc3339(&renewed_start, 300);
        let unbound_expiry = shifted_rfc3339(&renewed_start, 360);
        let expired_start = current_expiry.to_owned();
        let expired_renewal = shifted_rfc3339(&expired_start, 300);
        attacked["revision"] = serde_json::json!(2);
        attacked["participants"]["worker"]["claim"] = match attack {
            "active_replacement" => {
                participant_claim_json("worker-b", 2, '1', &renewed_start, &renewed_expiry, 300)
            }
            "unbound_expiry" => {
                participant_claim_json("worker-a", 1, '0', &renewed_start, &unbound_expiry, 300)
            }
            "duration_mismatch" => {
                participant_claim_json("worker-a", 1, '0', &renewed_start, &renewed_expiry, 120)
            }
            "heartbeat_after_expiry" => {
                participant_claim_json("worker-a", 1, '0', &expired_start, &expired_renewal, 300)
            }
            _ => unreachable!(),
        };
        let attacked: RunBaton = serde_json::from_value(attacked).unwrap();
        assert!(
            matches!(
                channel.compare_and_swap(1, &attacked),
                Err(StoreError::InvalidHistory | StoreError::InvalidBaton(_))
            ),
            "claim attack {attack} was accepted"
        );
        assert_eq!(channel.read().unwrap(), current);
        assert!(!dir
            .path()
            .join("history/00000000000000000002.json")
            .exists());
    }
}

#[test]
fn claim_history_generic_cas_rejects_even_a_well_formed_initial_claim() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let channel = RunChannel::open(dir.path());
    let current = channel.read().unwrap();
    let mut next = serde_json::to_value(current.clone()).unwrap();
    next["revision"] = serde_json::json!(1);
    next["participants"]["worker"]["claim"] = participant_claim_json(
        "forged-future-worker",
        1,
        '0',
        "2999-01-01T00:00:00Z",
        "2999-01-01T00:05:00Z",
        300,
    );
    let next: RunBaton = serde_json::from_value(next).unwrap();
    assert!(matches!(
        channel.compare_and_swap(0, &next),
        Err(StoreError::InvalidHistory)
    ));
    assert_eq!(channel.read().unwrap(), current);
}

#[test]
fn claim_history_v2_requires_bound_start_while_v1_remains_compatible() {
    let legacy = tempfile::tempdir().unwrap();
    write_legacy_run(
        legacy.path(),
        "working",
        "worker",
        serde_json::json!({
            "session_id": "legacy", "epoch": 1, "token_digest": "digest",
            "lease_expires_at": "2000-01-01T00:00:00Z", "lease_seconds": 300
        }),
    );
    assert!(RunChannel::open(legacy.path()).read().is_ok());

    let v2 = tempfile::tempdir().unwrap();
    init_pair(v2.path());
    claim_role(v2.path(), "worker", "worker", 0);
    let mut value = read_baton(v2.path());
    value["participants"]["worker"]["claim"]
        .as_object_mut()
        .unwrap()
        .remove("lease_started_at");
    std::fs::write(
        v2.path().join("baton.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        RunChannel::open(v2.path()).read(),
        Err(StoreError::InvalidBaton(_))
    ));
}

#[test]
fn claim_history_rejects_timestamp_duration_and_epoch_overflow() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let channel = RunChannel::open(dir.path());
    let current = channel.read().unwrap();
    let mut overflow = serde_json::to_value(current).unwrap();
    overflow["participants"]["worker"]["claim"] = participant_claim_json(
        "worker",
        1,
        '0',
        "9999-12-31T23:59:59Z",
        "9999-12-31T23:59:59Z",
        u64::MAX,
    );
    std::fs::write(
        dir.path().join("baton.json"),
        serde_json::to_vec_pretty(&overflow).unwrap(),
    )
    .unwrap();
    assert!(matches!(channel.read(), Err(StoreError::InvalidBaton(_))));

    let epoch = tempfile::tempdir().unwrap();
    init_pair(epoch.path());
    let mut max_epoch = read_baton(epoch.path());
    max_epoch["participants"]["worker"]["claim"] = participant_claim_json(
        "worker",
        u64::MAX,
        '0',
        "1999-12-31T23:55:00Z",
        "2000-01-01T00:00:00Z",
        300,
    );
    std::fs::write(
        epoch.path().join("baton.json"),
        serde_json::to_vec_pretty(&max_epoch).unwrap(),
    )
    .unwrap();
    command()
        .args([
            "reclaim",
            "--run-dir",
            epoch.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker",
            "--lease-seconds",
            "300",
            "--expected-revision",
            "0",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_history""#));
}

#[test]
fn claim_history_recovery_rejects_unproved_lease_transitions() {
    for attack in [
        "active_replacement",
        "unbound_expiry",
        "duration_mismatch",
        "heartbeat_after_expiry",
    ] {
        let dir = tempfile::tempdir().unwrap();
        init_pair(dir.path());
        claim_role(dir.path(), "worker", "worker-a", 0);
        let channel = RunChannel::open(dir.path());
        let mut attacked = serde_json::to_value(channel.read().unwrap()).unwrap();
        let current_claim = &attacked["participants"]["worker"]["claim"];
        let current_start = current_claim["lease_started_at"].as_str().unwrap();
        let current_expiry = current_claim["lease_expires_at"].as_str().unwrap();
        let renewed_start = shifted_rfc3339(current_start, 60);
        let renewed_expiry = shifted_rfc3339(&renewed_start, 300);
        let unbound_expiry = shifted_rfc3339(&renewed_start, 360);
        let expired_start = current_expiry.to_owned();
        let expired_renewal = shifted_rfc3339(&expired_start, 300);
        attacked["revision"] = serde_json::json!(2);
        attacked["participants"]["worker"]["claim"] = match attack {
            "active_replacement" => {
                participant_claim_json("worker-b", 2, '1', &renewed_start, &renewed_expiry, 300)
            }
            "unbound_expiry" => {
                participant_claim_json("worker-a", 1, '0', &renewed_start, &unbound_expiry, 300)
            }
            "duration_mismatch" => {
                participant_claim_json("worker-a", 1, '0', &renewed_start, &renewed_expiry, 120)
            }
            "heartbeat_after_expiry" => {
                participant_claim_json("worker-a", 1, '0', &expired_start, &expired_renewal, 300)
            }
            _ => unreachable!(),
        };
        std::fs::write(
            dir.path().join("history/00000000000000000002.json"),
            serde_json::to_vec_pretty(&attacked).unwrap(),
        )
        .unwrap();
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
            .stderr(predicate::str::contains(r#""error":"invalid_history""#));
        assert!(!dir
            .path()
            .join("history/00000000000000000003.json")
            .exists());
    }
}

#[test]
fn claim_history_accepts_bound_claim_heartbeat_and_expired_reclaim() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let claims = [
        participant_claim_json(
            "worker-a",
            1,
            '0',
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:05:00Z",
            300,
        ),
        participant_claim_json(
            "worker-a",
            1,
            '0',
            "2026-01-01T00:04:00Z",
            "2026-01-01T00:09:00Z",
            300,
        ),
        participant_claim_json(
            "worker-b",
            2,
            '1',
            "2026-01-01T00:09:00Z",
            "2026-01-01T00:14:00Z",
            300,
        ),
    ];
    for (index, claim) in claims.into_iter().enumerate() {
        let revision = index as u64;
        let source_path = dir
            .path()
            .join("history")
            .join(format!("{revision:020}.json"));
        let mut next: serde_json::Value =
            serde_json::from_slice(&std::fs::read(source_path).unwrap()).unwrap();
        next["revision"] = serde_json::json!(revision + 1);
        next["participants"]["worker"]["claim"] = claim;
        std::fs::write(
            dir.path()
                .join("history")
                .join(format!("{:020}.json", revision + 1)),
            serde_json::to_vec_pretty(&next).unwrap(),
        )
        .unwrap();
    }
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
        .success();
    assert_eq!(read_baton(dir.path())["revision"], 4);
}

#[test]
fn claim_history_kernel_binds_one_timestamp_to_each_lease() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let token = claim_role(dir.path(), "worker", "worker-1", 0);
    let claimed = read_baton(dir.path());
    let started = claimed["participants"]["worker"]["claim"]["lease_started_at"]
        .as_str()
        .expect("v2 claim records lease start");
    let expires = claimed["participants"]["worker"]["claim"]["lease_expires_at"]
        .as_str()
        .unwrap();
    let parse = |value: &str| {
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).unwrap()
    };
    assert_eq!((parse(expires) - parse(started)).whole_seconds(), 300);

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
            &token,
            "--lease-seconds",
            "120",
            "--expected-revision",
            "1",
        ])
        .assert()
        .success();
    let renewed = read_baton(dir.path());
    let renewed_start = renewed["participants"]["worker"]["claim"]["lease_started_at"]
        .as_str()
        .unwrap();
    let renewed_expiry = renewed["participants"]["worker"]["claim"]["lease_expires_at"]
        .as_str()
        .unwrap();
    assert_eq!(
        (parse(renewed_expiry) - parse(renewed_start)).whole_seconds(),
        120
    );
}

#[test]
fn claim_history_kernel_does_not_resurrect_an_expired_heartbeat() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let output = command()
        .args([
            "claim",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker",
            "--lease-seconds",
            "1",
            "--expected-revision",
            "0",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let token = serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    std::thread::sleep(std::time::Duration::from_millis(1_100));
    command()
        .args([
            "heartbeat",
            "--run-dir",
            dir.path().to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            "worker",
            "--token",
            &token,
            "--lease-seconds",
            "300",
            "--expected-revision",
            "1",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"claim_fenced""#));
    assert_eq!(read_baton(dir.path())["revision"], 1);
}

#[test]
fn claim_linearization_heartbeat_cannot_commit_after_expiring_behind_the_store_lock() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let channel = RunChannel::open(dir.path());
    let grant = claim::claim(&channel, Role::Worker, "worker", 2, 0).unwrap();
    let expires_at = read_baton(dir.path())["participants"]["worker"]["claim"]["lease_expires_at"]
        .as_str()
        .unwrap()
        .to_owned();

    let lock_path = dir.path().join(".baton.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    lock.lock_exclusive().unwrap();

    let heartbeat_dir = dir.path().to_owned();
    let heartbeat = std::thread::spawn(move || {
        claim::heartbeat(
            &RunChannel::open(heartbeat_dir),
            Role::Worker,
            "worker",
            &grant.token,
            300,
            1,
        )
    });

    let lock_is_open_twice = || {
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| std::fs::read_link(entry.path()).ok())
            .filter(|target| target == &lock_path)
            .count()
            >= 2
    };
    let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while !lock_is_open_twice() && std::time::Instant::now() < wait_deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        lock_is_open_twice(),
        "heartbeat never reached the locked store"
    );

    let expiry =
        time::OffsetDateTime::parse(&expires_at, &time::format_description::well_known::Rfc3339)
            .unwrap();
    while time::OffsetDateTime::now_utc() < expiry {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    FileExt::unlock(&lock).unwrap();

    assert!(matches!(heartbeat.join().unwrap(), Err(ClaimError::Fenced)));
    assert_eq!(RunChannel::open(dir.path()).read().unwrap().revision, 1);
}

#[test]
fn claim_linearization_expired_claimant_cannot_commit_a_semantic_action() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let channel = RunChannel::open(dir.path());
    let grant = claim::claim(&channel, Role::Worker, "worker", 2, 0).unwrap();
    let expires_at = read_baton(dir.path())["participants"]["worker"]["claim"]["lease_expires_at"]
        .as_str()
        .unwrap()
        .to_owned();

    let lock_path = dir.path().join(".baton.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    lock.lock_exclusive().unwrap();

    let action_dir = dir.path().to_owned();
    let action = std::thread::spawn(move || {
        transition::apply(
            &RunChannel::open(action_dir),
            Role::Worker,
            "worker",
            &grant.token,
            1,
            Action::Abandon {
                reason: "linearization test".to_owned(),
            },
        )
    });

    let lock_is_open_twice = || {
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| std::fs::read_link(entry.path()).ok())
            .filter(|target| target == &lock_path)
            .count()
            >= 2
    };
    let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while !lock_is_open_twice() && std::time::Instant::now() < wait_deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        lock_is_open_twice(),
        "semantic action never reached the locked store"
    );

    let expiry =
        time::OffsetDateTime::parse(&expires_at, &time::format_description::well_known::Rfc3339)
            .unwrap();
    while time::OffsetDateTime::now_utc() < expiry {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    FileExt::unlock(&lock).unwrap();

    assert!(matches!(
        action.join().unwrap(),
        Err(TransitionError::Claim(ClaimError::Fenced))
    ));
    let current = RunChannel::open(dir.path()).read().unwrap();
    assert_eq!(current.revision, 1);
    assert_eq!(current.status, dvandva_v4::model::Status::Working);
}

#[test]
fn publication_v2_creation_root_rejects_non_initial_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut forged = RunBaton::new(
        "forged-root",
        "Forged root",
        "Codex",
        "Claude",
        vec![DeliverableRequirement {
            id: "implementation".to_owned(),
            description: "Implement it".to_owned(),
        }],
    )
    .unwrap();
    forged.status = dvandva_v4::model::Status::Reviewing;
    forged.assignee = dvandva_v4::model::Assignee::Reviewer;

    assert!(matches!(
        RunChannel::open(dir.path()).create(&forged),
        Err(StoreError::InvalidSchemaTransition | StoreError::InvalidBaton(_))
    ));
    assert!(!dir.path().join("baton.json").exists());
    assert!(!dir
        .path()
        .join("history/00000000000000000000.json")
        .exists());
}

#[test]
fn publication_recovery_rejects_a_forged_approved_v2_creation_root() {
    let dir = tempfile::tempdir().unwrap();
    init_pair(dir.path());
    let mut forged = read_baton(dir.path());
    forged["publication_binding"] = approved_publication_binding(
        forged["publication_binding"]["obligation"].clone(),
        "site-forged-root",
        "deployment-forged-root",
    );
    std::fs::write(
        dir.path().join("history/00000000000000000000.json"),
        serde_json::to_vec_pretty(&forged).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.path().join("baton.json"), b"corrupt\n").unwrap();

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
        .stderr(predicate::str::contains(r#""error":"invalid_history""#));
    assert!(!dir
        .path()
        .join("history/00000000000000000001.json")
        .exists());
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

    let legacy_null = tempfile::tempdir().unwrap();
    write_legacy_run(
        legacy_null.path(),
        "working",
        "worker",
        serde_json::Value::Null,
    );
    let legacy_null_path = legacy_null.path().join("baton.json");
    let mut null_baton = read_baton(legacy_null.path());
    null_baton["publication"] = serde_json::Value::Null;
    std::fs::write(
        &legacy_null_path,
        serde_json::to_vec_pretty(&null_baton).unwrap(),
    )
    .unwrap();
    command()
        .args(["read", "--run-dir", legacy_null.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""error":"invalid_baton""#));

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

    baton["publication"] = serde_json::Value::Null;
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
    let (stage, digest) = stage_explainer_action(dir.path(), &obligation, "deployment-1");
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        2,
        "stage.json",
        stage,
    )
    .success();
    apply_action(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        3,
        "deploy.json",
        explainer_publication_action(
            &obligation,
            &digest,
            "site-run-a",
            "deployment-1",
            "https://sites.openai.test/site-run-a/deployment-1",
        ),
    )
    .success();
    let artifact = read_baton(dir.path())["publication_binding"]["artifact"].clone();
    apply_action(
        dir.path(),
        "reviewer",
        "reviewer-1",
        &reviewer,
        4,
        "changes.json",
        explainer_review_action(
            &obligation,
            &artifact,
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
    // Checkpoint submission is deliberately independent of the explainer, so a
    // changes-requested explainer never strands a finished deliverable.
    apply_action_raw(
        dir.path(),
        "worker",
        "worker-1",
        &worker,
        5,
        "still-available.json",
        checkpoint_submission(
            "checkpoint-a",
            serde_json::json!([
                {"id": "implementation", "artifacts": [{"kind": "commit", "value": "abc"}]}
            ]),
        ),
    )
    .success();
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
    let (stage_action, _digest) = stage_explainer_action(dir.path(), &obligation, "reverse");
    apply_action(
        dir.path(),
        "worker",
        "claude-worker",
        &worker,
        2,
        "claude-cannot-publish.json",
        stage_action.clone(),
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
        stage_action,
    )
    .success();
    let artifact = read_baton(dir.path())["publication_binding"]["artifact"].clone();
    let approval =
        explainer_review_action(&obligation, &artifact, "approved", serde_json::json!([]));
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
