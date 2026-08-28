//! Regressions for the PR #914 run, in which a completed review never reached a
//! checkpoint because the mandated explainer channel was unreadable by the
//! harness mandated to review it.
//!
//! One test per failure named in `docs/dvandva-pr-914-run-incident-report.html`.

use assert_cmd::Command;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
}

struct Fixture {
    _root: tempfile::TempDir,
    workspace: PathBuf,
    runs: PathBuf,
    credentials: PathBuf,
    run_dir: PathBuf,
    run_id: String,
    lease_seconds: u64,
}

impl Fixture {
    /// A run created the way a vadi session creates one, with the reviewer
    /// joined. Returns the fixture at the revision after both claims.
    fn started(deliverables: &[(&str, &str)]) -> Self {
        Self::started_with_lease(deliverables, 1800)
    }

    /// The reviewer takes `lease_seconds`; the worker keeps a long lease so it
    /// can still observe its peer after the peer's original interval lapses.
    fn started_with_lease(deliverables: &[(&str, &str)], lease_seconds: u64) -> Self {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        for args in [
            vec!["init", "--quiet"],
            vec![
                "remote",
                "add",
                "origin",
                "git@github.com:axatbhardwaj/Dvandva.git",
            ],
        ] {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&workspace)
                .args(args)
                .status()
                .unwrap()
                .success());
        }
        let runs = root.path().join("runs");
        let credentials = root.path().join("credentials");
        let mut args = vec![
            "role".to_owned(),
            "start".to_owned(),
            "--api".to_owned(),
            "2".to_owned(),
            "--workspace".to_owned(),
            workspace.to_str().unwrap().to_owned(),
            "--runs-dir".to_owned(),
            runs.to_str().unwrap().to_owned(),
            "--credentials-root".to_owned(),
            credentials.to_str().unwrap().to_owned(),
            "--role".to_owned(),
            "worker".to_owned(),
            "--session-id".to_owned(),
            "claude-session".to_owned(),
            "--current-harness".to_owned(),
            "claude".to_owned(),
            "--peer-harness".to_owned(),
            "codex".to_owned(),
            "--objective".to_owned(),
            "Fix the protocol defects".to_owned(),
            "--lease-seconds".to_owned(),
            "1800".to_owned(),
        ];
        for (id, description) in deliverables {
            args.push("--required-deliverable".to_owned());
            args.push(format!("{id}={description}"));
        }
        let created = run_json(command().args(&args));
        assert_eq!(created["outcome"], "started");
        let run_id = created["run_id"].as_str().unwrap().to_owned();
        let run_dir = PathBuf::from(created["run_dir"].as_str().unwrap());

        let fixture = Self {
            _root: root,
            workspace,
            runs,
            credentials,
            run_dir,
            run_id,
            lease_seconds,
        };
        fixture.join_reviewer();
        fixture
    }

    /// A run at revision 0 with no claims and the incident's unreadable policy,
    /// written consistently to both the head and its history revision.
    fn unclaimed_with_owner_only_site() -> Self {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        for args in [
            vec!["init", "--quiet"],
            vec![
                "remote",
                "add",
                "origin",
                "git@github.com:axatbhardwaj/Dvandva.git",
            ],
        ] {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&workspace)
                .args(args)
                .status()
                .unwrap()
                .success());
        }
        let runs = root.path().join("runs");
        let root_credentials = root.path().join("credentials");
        let run_id = "unreadable-policy-run".to_owned();
        let run_dir = runs.join(&run_id);
        command()
            .args([
                "init",
                "--run-dir",
                run_dir.to_str().unwrap(),
                "--run-id",
                &run_id,
                "--objective",
                "Fix the protocol defects",
                "--worker",
                "claude",
                "--reviewer",
                "codex",
                "--repository-id",
                "github.com/axatbhardwaj/dvandva",
                "--required-deliverable",
                "kernel=Fix the kernel",
            ])
            .assert()
            .success();
        rewrite_policy_to_owner_only_site(&run_dir);
        Self {
            _root: root,
            workspace,
            runs,
            credentials: root_credentials,
            run_dir,
            run_id,
            lease_seconds: 1800,
        }
    }

    fn join_reviewer(&self) {
        let joined = run_json(command().args([
            "role",
            "start",
            "--api",
            "2",
            "--workspace",
            self.workspace.to_str().unwrap(),
            "--runs-dir",
            self.runs.to_str().unwrap(),
            "--credentials-root",
            self.credentials.to_str().unwrap(),
            "--role",
            "reviewer",
            "--session-id",
            "codex-session",
            "--current-harness",
            "codex",
            "--peer-harness",
            "claude",
            "--run-id",
            &self.run_id,
            "--lease-seconds",
            &self.lease_seconds.to_string(),
        ]));
        assert_eq!(joined["outcome"], "started");
    }

    fn read(&self, role: &str) -> serde_json::Value {
        run_json(command().args([
            "role",
            "read",
            "--api",
            "2",
            "--run-dir",
            self.run_dir.to_str().unwrap(),
            "--role",
            role,
            "--session-id",
            session_for(role),
            "--credentials-root",
            self.credentials.to_str().unwrap(),
        ]))
    }

    fn revision(&self) -> u64 {
        self.read("worker")["revision"].as_u64().unwrap()
    }

    fn apply(&self, role: &str, revision: u64, action: serde_json::Value) -> serde_json::Value {
        run_json(&mut self.apply_command(role, revision, action))
    }

    fn apply_command(&self, role: &str, revision: u64, action: serde_json::Value) -> Command {
        let path = self
            ._root
            .path()
            .join(format!("action-{role}-{revision}-{}.json", uuid()));
        std::fs::write(&path, serde_json::to_vec_pretty(&action).unwrap()).unwrap();
        let mut command = command();
        command.args([
            "role",
            "apply",
            "--api",
            "2",
            "--run-dir",
            self.run_dir.to_str().unwrap(),
            "--role",
            role,
            "--session-id",
            session_for(role),
            "--expected-revision",
            &revision.to_string(),
            "--credentials-root",
            self.credentials.to_str().unwrap(),
            "--action",
            path.to_str().unwrap(),
        ]);
        command
    }

    /// Stage the bytes behind an analysis deliverable and return their digest,
    /// so the manifest cites something the reviewer can materialize.
    fn stage_analysis(&self, label: &str) -> String {
        let source = self._root.path().join(format!("analysis-{label}.md"));
        let bytes = format!("# {label}\nanalysis deliverable\n").into_bytes();
        std::fs::write(&source, &bytes).unwrap();
        // `staged_analysis` is a sorted set, so the digest is computed here
        // rather than read back positionally.
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let staged = self.apply(
            "worker",
            self.revision(),
            serde_json::json!({
                "type": "stage_analysis",
                "source_path": source.to_str().unwrap()
            }),
        );
        assert!(staged["staged_analysis"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(digest)));
        digest
    }

    /// The publisher stages bytes and the reviewer approves exactly those bytes.
    fn approve_explainer(&self, label: &str) {
        let baton = self.read("worker");
        let obligation = baton["publication_binding"]["obligation"].clone();
        let source = self._root.path().join(format!("explainer-{label}.html"));
        std::fs::write(&source, format!("<h1>{label}</h1>")).unwrap();
        let staged = self.apply(
            publisher_role(&baton),
            baton["revision"].as_u64().unwrap(),
            serde_json::json!({
                "type": "stage_explainer",
                "obligation": obligation,
                "source_path": source.to_str().unwrap()
            }),
        );
        let digest = staged["publication_binding"]["artifact"]["source_digest"].clone();
        self.apply(
            reviewer_role(&baton),
            staged["revision"].as_u64().unwrap(),
            serde_json::json!({
                "type": "record_explainer_review",
                "obligation": obligation,
                "source_digest": digest,
                "verdict": "approved",
                "findings": []
            }),
        );
    }
}

fn uuid() -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{:?}", std::time::Instant::now()).as_bytes())
    )
}

fn session_for(role: &str) -> &'static str {
    match role {
        "worker" => "claude-session",
        _ => "codex-session",
    }
}

/// Which protocol role currently sits on the publishing harness. Casting is the
/// human's choice; the publication policy is not.
fn publisher_role(baton: &serde_json::Value) -> &'static str {
    let publisher = baton["publication_policy"]["publisher_harness"]
        .as_str()
        .unwrap();
    if baton["participants"]["worker"]["harness"] == publisher {
        "worker"
    } else {
        "reviewer"
    }
}

fn reviewer_role(baton: &serde_json::Value) -> &'static str {
    match publisher_role(baton) {
        "worker" => "reviewer",
        _ => "worker",
    }
}

/// An `analysis` identity is derived from the digests the manifest cites.
fn analysis_identity(digests: &[String]) -> String {
    let mut sorted = digests.to_vec();
    sorted.sort();
    sorted.dedup();
    format!("{:x}", Sha256::digest(sorted.join("\n").as_bytes()))
}

fn run_json(command: &mut Command) -> serde_json::Value {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn rewrite_policy_to_owner_only_site(run_dir: &Path) {
    for name in ["baton.json", "history/00000000000000000000.json"] {
        let path = run_dir.join(name);
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let mut baton: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        baton["publication_policy"]["channel"] = serde_json::json!("codex_sites");
        baton["publication_policy"]["access"] = serde_json::json!("owner_only");
        std::fs::write(&path, serde_json::to_vec_pretty(&baton).unwrap()).unwrap();
    }
}

/// Incident P0: an owner-only Codex Site is readable only through the publisher
/// owner's session, so a Claude reviewer can never open it. The run must be
/// refused before any claim rather than after a deployment is already spent.
#[test]
fn an_unreadable_publication_policy_is_refused_at_start_and_can_be_repaired() {
    // A genuine unclaimed revision-0 run, so the preflight is exercised from the
    // state a joining session actually meets rather than from an edited head.
    let fixture = Fixture::unclaimed_with_owner_only_site();

    let start = |session: &str| {
        let mut command = command();
        command.args([
            "role",
            "start",
            "--api",
            "2",
            "--workspace",
            fixture.workspace.to_str().unwrap(),
            "--runs-dir",
            fixture.runs.to_str().unwrap(),
            "--credentials-root",
            fixture.credentials.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            session,
            "--current-harness",
            "claude",
            "--peer-harness",
            "codex",
            "--run-id",
            &fixture.run_id,
        ]);
        command
    };

    let refused = run_json(&mut start("claude-session"));
    assert_eq!(refused["outcome"], "publication_unreadable");
    assert_eq!(refused["next_action"], "repair_publication_policy");
    assert_eq!(refused["publication_policy"]["channel"], "codex_sites");
    assert_eq!(refused["publication_policy"]["access"], "owner_only");
    assert_eq!(refused["revision"], 0);

    // The refusal precedes every side effect: nothing claimed, nothing minted.
    let untouched: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.run_dir.join("baton.json")).unwrap())
            .unwrap();
    assert_eq!(untouched["revision"], 0);
    assert!(untouched["participants"]["worker"]["claim"].is_null());
    assert!(untouched["participants"]["reviewer"]["claim"].is_null());
    assert!(
        !fixture.credentials.join("claude-session").exists(),
        "a refused start must not mint a credential"
    );

    let repair = |session: &str, current: &str, peer: &str, revision: u64| {
        let mut command = command();
        command.args([
            "role",
            "repair-policy",
            "--api",
            "2",
            "--run-dir",
            fixture.run_dir.to_str().unwrap(),
            "--role",
            "worker",
            "--session-id",
            session,
            "--current-harness",
            current,
            "--peer-harness",
            peer,
            "--expected-revision",
            &revision.to_string(),
        ]);
        command
    };

    // Repair is scoped to the run's actual topology.
    repair("claude-session", "codex", "claude", 0)
        .assert()
        .failure();

    let repaired = run_json(&mut repair("claude-session", "claude", "codex", 0));
    assert_eq!(repaired["publication_policy"]["channel"], "run_artifact");
    assert_eq!(repaired["publication_policy"]["access"], "run_private");
    assert!(repaired["publication_binding"]["artifact"].is_null());
    assert!(repaired["publication_binding"]["deployment"].is_null());

    // The repaired chain is consistent: the head is exactly its own revision.
    let head: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.run_dir.join("baton.json")).unwrap())
            .unwrap();
    let recorded: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixture.run_dir.join("history/00000000000000000001.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(head, recorded);

    // And the run is joinable now that the policy is readable.
    let resumed = run_json(&mut start("claude-session"));
    assert_eq!(resumed["outcome"], "started");
}

/// Incident P1: the peer held a claim across a long Site build without
/// heartbeating, and the worker read the lapsed lease as a dead session. A
/// publisher that takes longer than one lease interval must stay visibly alive.
#[test]
fn a_long_publication_keeps_the_claim_live_and_the_phase_visible() {
    // A short lease, observed after it would have lapsed: the incident's
    // publisher held a claim for roughly 45 minutes across a Site build without
    // heartbeating, and the worker read the lapsed lease as a dead session.
    let fixture = Fixture::started_with_lease(&[("kernel", "Fix the kernel")], 4);
    let before = fixture.read("worker");
    let peer_lease_before = before["peer"]["lease_expires_at"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(before["peer"]["progress"].is_null());

    // Report late inside the long work, just before the lease it started with
    // lapses, which is when a real publisher notices it is still going.
    let original_expiry = time::OffsetDateTime::parse(
        before["peer"]["lease_expires_at"].as_str().unwrap(),
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap();
    let report_at = original_expiry - time::Duration::seconds(2);
    while time::OffsetDateTime::now_utc() < report_at {
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    fixture.apply(
        "reviewer",
        fixture.revision(),
        serde_json::json!({
            "type": "report_progress",
            "phase": "publishing_explainer",
            "detail": "rendering the manifest"
        }),
    );

    // Now let the original interval lapse. The publisher is still working, and
    // the peer must read it as alive rather than as the dead session the
    // incident inferred from an expired lease.
    while time::OffsetDateTime::now_utc() <= original_expiry {
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let after = fixture.read("worker");
    let renewed = time::OffsetDateTime::parse(
        after["peer"]["lease_expires_at"].as_str().unwrap(),
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap();
    assert!(
        renewed > original_expiry,
        "long authorized work must extend the reporting role's own lease"
    );
    assert_eq!(after["peer"]["role"], "reviewer");
    assert_eq!(after["peer"]["claim_state"], "busy");
    assert_eq!(after["peer"]["progress"]["phase"], "publishing_explainer");
    assert_eq!(
        after["peer"]["progress"]["detail"],
        "rendering the manifest"
    );
    assert!(
        after["peer"]["lease_expires_at"].as_str().unwrap() > peer_lease_before.as_str(),
        "reporting progress must renew the reporting role's own lease"
    );
    assert_eq!(
        after["peer"]["claim_state"], "busy",
        "a peer that reported progress must not read as expired"
    );
    // Liveness is always available and is never advised work.
    for role in ["worker", "reviewer"] {
        let snapshot = fixture.read(role);
        assert!(snapshot["legal_actions"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("report_progress")));
        assert!(!snapshot["advisory_actions"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("report_progress")));
    }
}

/// Incident P1: two publication records lost optimistic-concurrency races to
/// unrelated worker heartbeats. An obligation-bound write is correct against any
/// head, so it must not fail merely because the revision moved.
#[test]
fn an_obligation_bound_write_survives_an_unrelated_heartbeat() {
    let fixture = Fixture::started(&[("kernel", "Fix the kernel")]);
    let baton = fixture.read("worker");
    let obligation = baton["publication_binding"]["obligation"].clone();
    let prepared_revision = baton["revision"].as_u64().unwrap();

    // The peer heartbeats between preparing the action and applying it.
    run_json(command().args([
        "role",
        "heartbeat",
        "--api",
        "2",
        "--run-dir",
        fixture.run_dir.to_str().unwrap(),
        "--role",
        "worker",
        "--session-id",
        "claude-session",
        "--lease-seconds",
        "1800",
        "--expected-revision",
        &prepared_revision.to_string(),
        "--credentials-root",
        fixture.credentials.to_str().unwrap(),
    ]));
    assert_eq!(fixture.revision(), prepared_revision + 1);

    let source = fixture._root.path().join("explainer.html");
    std::fs::write(&source, b"<h1>explainer</h1>").unwrap();
    let staged = fixture.apply(
        "reviewer",
        prepared_revision,
        serde_json::json!({
            "type": "stage_explainer",
            "obligation": obligation,
            "source_path": source.to_str().unwrap()
        }),
    );
    assert_eq!(staged["revision"], prepared_revision + 2);
    assert_eq!(
        staged["publication_binding"]["artifact"]["source_digest"],
        format!("{:x}", Sha256::digest(b"<h1>explainer</h1>"))
    );

    // The incident's races were on publication and review records, not staging,
    // so both must also survive an intervening heartbeat at a stale revision.
    let digest = staged["publication_binding"]["artifact"]["source_digest"]
        .as_str()
        .unwrap()
        .to_owned();
    run_json(command().args([
        "role",
        "heartbeat",
        "--api",
        "2",
        "--run-dir",
        fixture.run_dir.to_str().unwrap(),
        "--role",
        "worker",
        "--session-id",
        "claude-session",
        "--lease-seconds",
        "1800",
        "--expected-revision",
        &fixture.revision().to_string(),
        "--credentials-root",
        fixture.credentials.to_str().unwrap(),
    ]));
    let published = fixture.apply(
        "reviewer",
        prepared_revision,
        serde_json::json!({
            "type": "record_explainer_publication",
            "obligation": obligation,
            "source_digest": digest,
            "site_id": "site-run",
            "site_version": "1",
            "url": "https://example.chatgpt.site",
            "channel": "codex_sites",
            "access": "owner_only"
        }),
    );
    assert_eq!(
        published["publication_binding"]["deployment"]["source_digest"],
        digest
    );

    run_json(command().args([
        "role",
        "heartbeat",
        "--api",
        "2",
        "--run-dir",
        fixture.run_dir.to_str().unwrap(),
        "--role",
        "worker",
        "--session-id",
        "claude-session",
        "--lease-seconds",
        "1800",
        "--expected-revision",
        &fixture.revision().to_string(),
        "--credentials-root",
        fixture.credentials.to_str().unwrap(),
    ]));
    let reviewed = fixture.apply(
        "worker",
        prepared_revision,
        serde_json::json!({
            "type": "record_explainer_review",
            "obligation": obligation,
            "source_digest": digest,
            "verdict": "approved",
            "findings": []
        }),
    );
    assert_eq!(
        reviewed["publication_binding"]["review"]["verdict"],
        "approved"
    );

    // An ordinary semantic mutation still takes its revision precondition.
    fixture
        .apply_command(
            "worker",
            prepared_revision,
            serde_json::json!({
                "type": "request_human_decision", "kind": "scope",
                "question": "Which scope?",
                "evidence": ["stale revision"],
                "options": ["a", "b"]
            }),
        )
        .assert()
        .failure()
        .stderr(predicates::str::contains(r#""error":"revision_conflict""#));
}

/// Incident P1: `submit_checkpoint awaits current explainer approval` was
/// asserted on the very first snapshot, before any work existed to checkpoint.
/// A finished deliverable must always have somewhere to land.
#[test]
fn a_checkpoint_is_submittable_before_any_explainer_exists() {
    let fixture = Fixture::started(&[("kernel", "Fix the kernel")]);
    let baton = fixture.read("worker");
    assert!(baton["publication_binding"]["artifact"].is_null());
    assert!(baton["blocking_reason"].is_null());
    assert!(baton["legal_actions"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("submit_checkpoint")));

    let digest = fixture.stage_analysis("early");
    let identity = analysis_identity(std::slice::from_ref(&digest));
    let submitted = fixture.apply(
        "worker",
        fixture.revision(),
        serde_json::json!({
            "type": "submit_checkpoint",
            "checkpoint": {
                "kind": "analysis",
                "identity": identity,
                "deliverables": [
                    {"id": "kernel", "artifacts": [{"kind": "analysis_digest", "value": digest}]}
                ],
                "verification": ["cargo test --offline: 177 passed"]
            }
        }),
    );
    assert_eq!(submitted["status"], "reviewing");
    assert_eq!(submitted["assignee"], "reviewer");

    // Only finalization waits on the explainer.
    let approved = fixture.apply(
        "reviewer",
        submitted["revision"].as_u64().unwrap(),
        serde_json::json!({
            "type": "record_review",
            "verdict": "approved",
            "checkpoint_identity": identity,
            "manifest_digest": submitted["checkpoint"]["manifest_digest"],
            "scope_revision": 0,
            "findings": []
        }),
    );
    assert_eq!(approved["status"], "finalizing");
    // The blocker appears only now, and only for the role that owns finalizing.
    let finalizing = fixture.read("worker");
    assert_eq!(
        finalizing["blocking_reason"],
        "finalize awaits current explainer approval"
    );
}

/// Incident P2: a human pasting rendered text does not prove the bytes are the
/// bytes the deployment recorded. The facade hands over digest-bound bytes and
/// refuses to serve them if the digest no longer matches.
#[test]
fn relayed_explainer_bytes_are_verified_against_the_recorded_digest() {
    let fixture = Fixture::started(&[("kernel", "Fix the kernel")]);
    let baton = fixture.read("worker");
    let source = fixture._root.path().join("explainer.html");
    std::fs::write(&source, b"<h1>canonical explainer</h1>").unwrap();
    let staged = fixture.apply(
        "reviewer",
        baton["revision"].as_u64().unwrap(),
        serde_json::json!({
            "type": "stage_explainer",
            "obligation": baton["publication_binding"]["obligation"],
            "source_path": source.to_str().unwrap()
        }),
    );
    let digest = staged["publication_binding"]["artifact"]["source_digest"]
        .as_str()
        .unwrap()
        .to_owned();

    let read = |role: &str| {
        let mut command = command();
        command.args([
            "role",
            "explainer",
            "--api",
            "2",
            "--run-dir",
            fixture.run_dir.to_str().unwrap(),
            "--role",
            role,
            "--session-id",
            session_for(role),
            "--credentials-root",
            fixture.credentials.to_str().unwrap(),
        ]);
        command
    };

    // Both harnesses can read the same bytes; no session, no network, no 401.
    for role in ["worker", "reviewer"] {
        let relayed = run_json(&mut read(role));
        assert_eq!(relayed["source_digest"], digest);
        assert_eq!(relayed["contents"], "<h1>canonical explainer</h1>");
        assert_eq!(relayed["media_type"], "text/html");
    }

    std::fs::write(
        fixture.run_dir.join(format!("explainer/{digest}.html")),
        b"<h1>tampered</h1>",
    )
    .unwrap();
    read("worker")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "do not match their recorded digest",
        ));
}

/// Incident regression: the whole lifecycle must reach a terminal state with two
/// independently started sessions, neither invoking the other. The human relays
/// the returned `peer_prompt`; the kernel never spawns a harness.
#[test]
fn two_independent_harnesses_reach_a_terminal_state_without_invoking_each_other() {
    let fixture = Fixture::started(&[("kernel", "Fix the kernel")]);
    let started = fixture.read("worker");
    assert_eq!(started["participants"]["worker"]["harness"], "Claude");
    assert_eq!(started["participants"]["reviewer"]["harness"], "Codex");

    let digest = fixture.stage_analysis("lifecycle");
    let identity = analysis_identity(std::slice::from_ref(&digest));
    let submitted = fixture.apply(
        "worker",
        fixture.revision(),
        serde_json::json!({
            "type": "submit_checkpoint",
            "checkpoint": {
                "kind": "analysis",
                "identity": identity,
                "deliverables": [
                    {"id": "kernel", "artifacts": [{"kind": "analysis_digest", "value": digest}]}
                ],
                "verification": ["cargo test --offline"]
            }
        }),
    );
    let approved = fixture.apply(
        "reviewer",
        submitted["revision"].as_u64().unwrap(),
        serde_json::json!({
            "type": "record_review",
            "verdict": "approved",
            "checkpoint_identity": identity,
            "manifest_digest": submitted["checkpoint"]["manifest_digest"],
            "scope_revision": 0,
            "findings": []
        }),
    );
    assert_eq!(approved["status"], "finalizing");

    fixture.approve_explainer("finalizing");
    let finalized = fixture.apply(
        "worker",
        fixture.revision(),
        serde_json::json!({"type": "finalize"}),
    );
    assert_eq!(finalized["status"], "done");
    assert_eq!(finalized["terminal"]["outcome"], "done");
    assert_eq!(finalized["next_actions"], serde_json::json!(["stop"]));

    // The peer session was addressed by prompt, never invoked.
    let restarted = run_json(command().args([
        "role",
        "start",
        "--api",
        "2",
        "--workspace",
        fixture.workspace.to_str().unwrap(),
        "--runs-dir",
        fixture.runs.to_str().unwrap(),
        "--credentials-root",
        fixture.credentials.to_str().unwrap(),
        "--role",
        "worker",
        "--session-id",
        "claude-session",
        "--current-harness",
        "claude",
        "--peer-harness",
        "codex",
        "--run-id",
        &fixture.run_id,
    ]));
    assert_eq!(restarted["disposition"], "terminal");
    assert!(restarted["peer_prompt"].is_null());
}

/// Incident timeline, first entry: `start` returned `scope_mismatch` against a
/// stale unrelated run — a parked Notion spec review on the old schema — and the
/// worker had to retry with `--new-run`. An unrelated run must not block a fresh
/// objective, whatever schema it is on and however long it has been parked.
#[test]
fn an_unrelated_parked_run_does_not_block_a_fresh_objective() {
    let fixture = Fixture::started(&[("kernel", "Fix the kernel")]);

    // A sibling on the legacy schema, parked in human_decision, pursuing an
    // entirely different objective — exactly the incident's blocker.
    let sibling = fixture
        .runs
        .join("notion-mobile-app-tech-spec-review-90cfffcc");
    std::fs::create_dir_all(sibling.join("history")).unwrap();
    let legacy = serde_json::json!({
        "schema": "dvandva.run.v1",
        "run_id": "notion-mobile-app-tech-spec-review-90cfffcc",
        "objective": {"summary": "Review the Notion mobile app technical spec", "refs": []},
        "workspace": {
            "repository_id": "github.com/axatbhardwaj/dvandva",
            "origin": "https://github.com/axatbhardwaj/Dvandva",
            "worktree": null
        },
        "task": {"reference": null, "summary": "Review the Notion mobile app technical spec"},
        "participants": {
            "worker": {"harness": "claude", "claim": null},
            "reviewer": {"harness": "codex", "claim": null}
        },
        "status": "human_decision", "assignee": "human", "revision": 0,
        "checkpoint": null, "review": null,
        "publication": {"required": true, "desired_revision": 0, "published_revision": null, "refs": []},
        "human_decision": {
            "question": "Which sections?", "requested_by": "worker",
            "evidence": ["parked"], "options": ["a", "b"],
            "contact_role": "worker", "resume_status": "working",
            "resume_assignee": "worker", "answer": null
        },
        "predecessor_run_id": null, "terminal": null, "recovery": null
    });
    let bytes = serde_json::to_vec_pretty(&legacy).unwrap();
    std::fs::write(sibling.join("history/00000000000000000000.json"), &bytes).unwrap();
    std::fs::write(sibling.join("baton.json"), &bytes).unwrap();

    // A fresh, unrelated objective must create its own run without --new-run.
    let started = run_json(command().args([
        "role",
        "start",
        "--api",
        "2",
        "--workspace",
        fixture.workspace.to_str().unwrap(),
        "--runs-dir",
        fixture.runs.to_str().unwrap(),
        "--credentials-root",
        fixture.credentials.to_str().unwrap(),
        "--role",
        "worker",
        "--session-id",
        "fresh-session",
        "--current-harness",
        "claude",
        "--peer-harness",
        "codex",
        "--objective",
        "Something else entirely",
        "--required-deliverable",
        "other=Other work",
    ]));
    assert_eq!(started["outcome"], "started");
    assert_eq!(started["disposition"], "created");
    assert_eq!(started["objective"]["summary"], "Something else entirely");
    assert_ne!(started["run_id"], fixture.run_id.as_str());

    // The unrelated sibling is untouched: not upgraded, not claimed, not moved.
    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sibling.join("baton.json")).unwrap()).unwrap();
    assert_eq!(after["schema"], "dvandva.run.v1");
    assert_eq!(after["revision"], 0);

    // Naming this run's own task reference, with a different objective, is still
    // a real scope disagreement rather than an unrelated run.
    let mismatch = run_json(command().args([
        "role",
        "start",
        "--api",
        "2",
        "--workspace",
        fixture.workspace.to_str().unwrap(),
        "--runs-dir",
        fixture.runs.to_str().unwrap(),
        "--credentials-root",
        fixture.credentials.to_str().unwrap(),
        "--role",
        "worker",
        "--session-id",
        "claude-session",
        "--current-harness",
        "claude",
        "--peer-harness",
        "codex",
        "--run-id",
        &fixture.run_id,
        "--objective",
        "A different objective for the same run",
    ]));
    assert_eq!(mismatch["outcome"], "scope_mismatch");
}
