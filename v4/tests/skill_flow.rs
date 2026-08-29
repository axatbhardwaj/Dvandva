use assert_cmd::Command;
use dvandva_v4::action::Action;
use predicates::prelude::*;
use std::collections::BTreeSet;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva-v4"))
}

fn repository_file(path: &str) -> String {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    std::fs::read_to_string(repository.join(path)).unwrap()
}

fn role_sources(role: &str) -> (String, String) {
    (
        repository_file(&format!("skills/{role}/SKILL.md")),
        repository_file(&format!("skills/{role}/references/run-contract.md")),
    )
}

fn assert_role_source_contract(role: &str) {
    let (skill, contract) = role_sources(role);
    let source = format!("{skill}\n{contract}");
    for required in [
        "fresh facade snapshot",
        "next_actions",
        "advisory_actions",
        "legal_actions",
        "never an ordinary wake or action",
        "scope_mismatch",
        "complete deliverable manifest",
        "canonical deliverable IDs exactly once",
        "request_checkpoint_supersession",
        "accept_checkpoint_supersession",
        "withdraw_approval",
        "Codex harness stages",
        "Claude harness reviews",
        "regardless of semantic casting",
        "the gate binds a digest, not a URL",
        "canonical scope, complete manifest, findings and decisions, and a current plan/TODO",
        "stage_explainer",
        "explainer/<source_digest>.html",
        "stable Site ID",
        "new Site version",
        "never gates the run",
        "Never record a verdict on bytes you did not read",
        "Claude Artifact",
        "generic publisher",
        "silent fallback",
        "publication_unreadable",
        "repair-policy",
        "report_progress",
        "slow from dead",
        "user-created harness goals remain unchanged",
        "human starts the peer session",
        "explicitly invokes them in this session",
        "What changed",
        "What was verified",
        "What is blocked",
        "Who owns the next action",
        "Exact command or prompt",
        "foreground local wait",
        "Ending the turn is not a wait",
        "poll  SESSION RUN_DIR AFTER_REVISION [MAX_MS]",
        "upgrade_required",
        "upgrade SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION",
        "repair-policy SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION",
        "explainer SESSION RUN_DIR",
        "claim SESSION RUN_DIR EXPECTED_REVISION",
        "reclaim SESSION RUN_DIR EXPECTED_REVISION",
        "exact `start --run-id` automatically reclaims",
        "ACTION_FILE",
    ] {
        assert!(
            source.contains(required),
            "{role} contract omitted {required:?}"
        );
    }

    assert!(source.contains("Exact joins pass only `--run-id`"));
    assert!(source.contains("mode 0600"));
    assert!(source.contains("private temporary file"));
    assert!(source.contains("deletes it after"));
    assert!(!source.contains("ACTION_JSON"));
    assert!(source.contains("Publication never substitutes for supersession or withdrawal."));
    let allowed_exception = "a decision that is the human's alone";
    assert!(
        skill
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains(allowed_exception),
        "{role} skill omits the complete Human Decision exception"
    );
    assert!(
        contract
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains(allowed_exception),
        "{role} reference omits the complete Human Decision exception"
    );
    assert!(skill.contains(
        "Dvandva never creates, replaces, pauses, completes, or clears any harness goal."
    ));
    assert!(skill.contains("Goals the user sets in a launch prompt remain outside the protocol."));
    for forbidden in [
        "create_goal",
        "update_goal",
        "get_goal",
        "pause_goal",
        "complete_goal",
        "clear_goal",
    ] {
        assert!(
            !source.contains(forbidden),
            "{role} skill names goal tool {forbidden}"
        );
    }

    assert!(
        !contract.contains(r#""scope_revision":1"#),
        "{role} hardcodes a facade-derived scope revision"
    );
    assert!(
        !contract.contains(r#""handoff_revision":12"#),
        "{role} hardcodes a facade-derived handoff revision"
    );
    assert!(
        !contract.contains(r#""identity":"<checkpoint.identity>""#),
        "{role} uses identity where CheckpointBinding requires checkpoint_identity"
    );
    assert!(contract.contains("copy `publication_binding.obligation` unchanged"));
    for contradictory in [
        "only for new human scope or ambiguity",
        "solely for new human scope or ambiguity",
    ] {
        assert!(
            !skill.contains(contradictory) && !contract.contains(contradictory),
            "{role} retains contradictory Human Decision restriction {contradictory:?}"
        );
    }
}

#[test]
fn vadi_skill_sources_define_the_complete_v2_contract() {
    assert_role_source_contract("vadi");
    let (skill, contract) = role_sources("vadi");
    let source = format!("{skill}\n{contract}");
    for required in [
        "first user-visible protocol output",
        "canonical objective and scope",
        "status and assignee",
        "peer_prompt",
    ] {
        assert!(source.contains(required), "vadi omitted {required:?}");
    }
}

#[test]
fn prativadi_skill_sources_define_the_complete_v2_contract() {
    assert_role_source_contract("prativadi");
    let (_, contract) = role_sources("prativadi");
    assert!(contract.contains("Prativadi never creates a run."));
    assert!(contract.contains("copy the exact current `checkpoint` coordinates"));
    assert!(contract.contains("manifest_digest"));
    assert!(contract.contains("scope_revision"));
    let synopsis = contract.split("```").nth(1).unwrap();
    assert!(!synopsis.contains("--new-run"));
}

fn documented_json_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = None::<String>;
    for line in markdown.lines() {
        if line == "```json" {
            current = Some(String::new());
        } else if line == "```" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
        } else if let Some(block) = current.as_mut() {
            block.push_str(line);
            block.push('\n');
        }
    }
    blocks
}

fn normalize_documented_action(template: &str) -> serde_json::Value {
    let obligation = serde_json::json!({
        "handoff_revision": 5,
        "kind": "worker_to_reviewer",
        "scope_revision": 0,
        "checkpoint": {
            "checkpoint_identity": "checkpoint-a",
            "manifest_digest": "b".repeat(64),
            "scope_revision": 0
        }
    });
    let normalized = template
        .replace("<snapshot.publication_binding.receipt_seq>", "0")
        .replace(
            "<absolute path to the explainer HTML>",
            "/nonexistent/explainer.html",
        )
        .replace(
            "<absolute path to the analysis bytes>",
            "/nonexistent/analysis.md",
        )
        .replace(
            r#""<snapshot.publication_binding.obligation>""#,
            &serde_json::to_string(&obligation).unwrap(),
        )
        .replace("<snapshot.checkpoint.identity>", "checkpoint-a")
        .replace("<snapshot.checkpoint.manifest_digest>", &"b".repeat(64))
        .replace("<snapshot.checkpoint.scope_revision>", "0")
        .replace(
            "<snapshot.publication_binding.artifact.source_digest>",
            &"a".repeat(64),
        )
        .replace(
            "<sha256 of the cited digests, sorted, deduplicated, newline-joined>",
            &"c".repeat(64),
        )
        .replace("<sha256 of the artifact>", &"d".repeat(64))
        .replace("<full-length commit object name>", &"e".repeat(40))
        .replace("<current step>", "building the explainer")
        .replace(
            "<snapshot.publication_binding.deployment.site_id>",
            "site-run",
        )
        .replace(
            "<snapshot.publication_binding.deployment.site_version>",
            "site-version",
        )
        .replace(
            "<snapshot.publication_binding.deployment.url>",
            "https://sites.openai.test/site-run/site-version",
        )
        .replace("<human-approved answer>", "Include report")
        .replace("<one of the recorded options>", "Include report")
        .replace("<concrete option A>", "Include report")
        .replace("<concrete option B>", "Leave it out")
        .replace("<human-approved objective>", "Ship approved scope")
        .replace("<human-approved ref kind>", "issue")
        .replace("<human-approved ref value>", "DEF-456")
        .replace("<human-approved task reference>", "DEF-456")
        .replace("<human-approved deliverable ID>", "report")
        .replace(
            "<human-approved deliverable description>",
            "Approved report",
        );
    serde_json::from_str(&normalized).unwrap_or_else(|error| {
        panic!("documented action is not normalizable JSON: {error}\n{template}")
    })
}

fn documented_actions(role: &str) -> Vec<serde_json::Value> {
    let (_, contract) = role_sources(role);
    documented_json_blocks(&contract)
        .iter()
        .flat_map(|block| block.lines())
        .filter(|template| !template.trim().is_empty())
        .map(normalize_documented_action)
        .collect()
}

fn documented_action(role: &str, action_type: &str) -> serde_json::Value {
    documented_actions(role)
        .into_iter()
        .find(|action| action["type"] == action_type)
        .unwrap_or_else(|| panic!("{role} omitted documented {action_type} payload"))
}

#[test]
fn every_documented_role_action_deserializes_against_the_v2_schema() {
    let expected = [
        (
            "vadi",
            BTreeSet::from([
                "finalize",
                "record_explainer_publication",
                "record_explainer_review",
                "report_progress",
                "request_checkpoint_supersession",
                "request_human_decision",
                "resume_human_decision",
                "stage_analysis",
                "stage_explainer",
                "submit_checkpoint",
                "withdraw_approval",
            ]),
        ),
        (
            "prativadi",
            BTreeSet::from([
                "accept_checkpoint_supersession",
                "record_explainer_publication",
                "record_explainer_review",
                "record_review",
                "report_progress",
                "request_human_decision",
                "resume_human_decision",
                "stage_explainer",
            ]),
        ),
    ];
    for (role, expected) in expected {
        let actions = documented_actions(role);
        let actual = actions
            .iter()
            .map(|action| action["type"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected, "{role} action map is not role-specific");
        for action in actions {
            serde_json::from_value::<Action>(action.clone()).unwrap_or_else(|error| {
                panic!("{role} documented invalid action {action}: {error}")
            });
        }
    }
}

fn documented_scope_amendment(role: &str) -> serde_json::Value {
    documented_actions(role)
        .into_iter()
        .find(|action| !action["scope_amendment"].is_null())
        .unwrap_or_else(|| panic!("{role} omitted documented scope-amending resume payload"))
}

#[test]
fn documented_human_decision_payload_transitions_for_each_role() {
    for role in ["vadi", "prativadi"] {
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
        let run_dir = runs.join(worker["run_id"].as_str().unwrap());
        let flow = Flow {
            root: root.path(),
            run_dir: &run_dir,
            credentials: &credentials,
        };
        let (kernel_role, session) = if role == "vadi" {
            ("worker", "worker-session")
        } else {
            ("reviewer", "reviewer-session")
        };
        let action = documented_action(role, "request_human_decision");
        assert!(action["options"].as_array().unwrap().len() >= 2);
        let human = flow.apply(kernel_role, session, 2, "human.json", action);
        assert_eq!(human["status"], "human_decision");
        assert_eq!(
            human["human_decision"]["options"].as_array().unwrap().len(),
            2
        );
        assert_eq!(reviewer["run_id"], worker["run_id"]);
    }
}

#[test]
fn documented_scope_amendment_transitions_for_each_role() {
    for role in ["vadi", "prativadi"] {
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
        let run_dir = runs.join(worker["run_id"].as_str().unwrap());
        let flow = Flow {
            root: root.path(),
            run_dir: &run_dir,
            credentials: &credentials,
        };
        let (kernel_role, session) = if role == "vadi" {
            ("worker", "worker-session")
        } else {
            ("reviewer", "reviewer-session")
        };
        let human = flow.apply(
            kernel_role,
            session,
            2,
            "request-scope.json",
            documented_action(role, "request_human_decision"),
        );
        assert_eq!(human["status"], "human_decision");

        let amended = flow.apply(
            kernel_role,
            session,
            3,
            "amend-scope.json",
            documented_scope_amendment(role),
        );
        assert_eq!(amended["scope_revision"], 1);
        assert_eq!(amended["objective"]["summary"], "Ship approved scope");
        assert_eq!(
            amended["objective"]["refs"],
            serde_json::json!([{"kind":"issue","value":"DEF-456"}])
        );
        assert_eq!(amended["task"]["reference"], "DEF-456");
        assert_eq!(
            amended["scope_deliverables"],
            serde_json::json!([{"id":"report","description":"Approved report"}])
        );
        assert_eq!(amended["status"], "revising");
        assert_eq!(amended["assignee"], "worker");
        assert_eq!(
            amended["publication_binding"]["obligation"]["kind"],
            "scope_amended"
        );
        assert_eq!(
            amended["publication_binding"]["obligation"]["scope_revision"],
            1
        );
        assert_eq!(
            amended["publication_binding"]["obligation"]["handoff_revision"],
            4
        );
        assert_eq!(reviewer["run_id"], worker["run_id"]);
    }
}

#[test]
fn setup_skill_sources_pin_v2_without_implicit_run_migration() {
    let setup = format!(
        "{}\n{}",
        repository_file("skills/setup-dvandva/SKILL.md"),
        repository_file("skills/setup-dvandva/references/installation.md")
    );
    for required in [
        "0.3.0",
        "skills-v0.3.0",
        "release target",
        "fails closed if either is missing",
        "Linux x86_64 only",
        "only Linux x86_64 is supported for now",
        "dvandva.run.v2",
        "facade API 2",
        "v1 read support is only for explicit migration",
        "setup never migrates runs",
    ] {
        assert!(
            setup.contains(required),
            "setup contract omitted {required:?}"
        );
    }
}

#[test]
fn role_entry_prompts_remain_concise_pointers_not_duplicate_contracts() {
    for path in [
        "skills/vadi/agents/openai.yaml",
        "skills/prativadi/agents/openai.yaml",
    ] {
        let prompt = repository_file(path);
        assert!(prompt.lines().count() <= 7, "{path} is no longer concise");
        for duplicate in [
            "next_actions",
            "legal_actions",
            "submit_checkpoint",
            "record_review",
        ] {
            assert!(
                !prompt.contains(duplicate),
                "{path} duplicates contract term {duplicate}"
            );
        }
    }
}

#[test]
fn version_and_probe_report_the_installation_contract() {
    command()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("dvandva-v4 0.3.0"));

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
    assert_eq!(probe["version"], "0.3.0");
    assert_eq!(probe["write_schema"], "dvandva.run.v2");
    assert_eq!(probe["role_api"], 2);
    assert_eq!(probe["publish"], false);
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
        let source = self.root.join(format!("explainer-{site_version}.html"));
        std::fs::write(&source, format!("<h1>{site_version}</h1>")).unwrap();
        let mut stage = documented_action("vadi", "stage_explainer");
        stage["obligation"] = obligation.clone();
        stage["after_seq"] = baton["publication_binding"]["receipt_seq"].clone();
        stage["source_path"] = serde_json::json!(source.to_str().unwrap());
        let staged = self.apply(
            "worker",
            "worker-session",
            revision,
            &format!("stage-{site_version}.json"),
            stage,
        );
        let artifact = staged["publication_binding"]["artifact"].clone();
        let mut review = documented_action("prativadi", "record_explainer_review");
        review["obligation"] = obligation;
        review["after_seq"] = staged["publication_binding"]["receipt_seq"].clone();
        review["source_digest"] = artifact["source_digest"].clone();
        self.apply(
            "reviewer",
            "reviewer-session",
            revision + 1,
            &format!("review-{site_version}.json"),
            review,
        )
    }
}

fn documented_checkpoint(identity: &str, verification: Vec<&str>) -> serde_json::Value {
    let mut action = documented_action("vadi", "submit_checkpoint");
    action["checkpoint"]["identity"] = serde_json::json!(identity);
    action["checkpoint"]["deliverables"] = serde_json::json!([{
        "id": "implementation",
        "artifacts": [{"kind": "commit", "value": identity}]
    }]);
    action["checkpoint"]["verification"] = serde_json::json!(verification);
    action
}

fn documented_review(
    verdict: &str,
    checkpoint: &serde_json::Value,
    findings: Vec<&str>,
) -> serde_json::Value {
    let mut action = documented_action("prativadi", "record_review");
    action["verdict"] = serde_json::json!(verdict);
    action["checkpoint_identity"] = checkpoint["identity"].clone();
    action["manifest_digest"] = checkpoint["manifest_digest"].clone();
    action["scope_revision"] = checkpoint["scope_revision"].clone();
    action["findings"] = serde_json::json!(findings);
    action
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
        documented_checkpoint(checkpoint_a, vec!["cargo test"]),
    );
    assert_eq!(reviewing_a["status"], "reviewing");
    assert!(reviewing_a["next_actions"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("request_checkpoint_supersession")));
    assert!(reviewing_a["next_actions"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("stage_explainer")));

    flow.approve_explainer(5, "deployment-2");

    let revising = flow.apply(
        "reviewer",
        "reviewer-session",
        7,
        "request-changes.json",
        documented_review(
            "changes_requested",
            &reviewing_a["checkpoint"],
            vec!["Add the missing contention test"],
        ),
    );
    assert_eq!(revising["status"], "revising");

    flow.approve_explainer(8, "deployment-3");

    let reviewing_b = flow.apply(
        "worker",
        "worker-session",
        10,
        "checkpoint-b.json",
        documented_checkpoint(checkpoint_b, vec!["cargo test", "contention test"]),
    );
    assert_eq!(reviewing_b["checkpoint"]["identity"], checkpoint_b);

    flow.approve_explainer(11, "deployment-4");

    let approved = flow.apply(
        "reviewer",
        "reviewer-session",
        13,
        "approve.json",
        documented_review("approved", &reviewing_b["checkpoint"], vec![]),
    );
    assert_eq!(approved["status"], "finalizing");

    flow.approve_explainer(14, "deployment-5");

    let done = flow.apply(
        "worker",
        "worker-session",
        16,
        "finalize.json",
        documented_action("vadi", "finalize"),
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
