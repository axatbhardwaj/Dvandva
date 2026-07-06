//! v3 read-path compatibility checks.
//!
//! v3 retires v2 on the write path, but state/resolve/wait/brief must keep
//! reading both v2 and v3 JSON as data during the migration window.

#![recursion_limit = "256"]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn run(args: &[&str], cwd: Option<&Path>) -> (i32, String, String) {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_BATON_FILE")
        .env_remove("DVANDVA_RUN_DIR")
        .env_remove("DVANDVA_RUN_ID")
        .env_remove("DVANDVA_CONCURRENT");
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let output = cmd.output().expect("spawn dvandva");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

fn read_path_baton(schema: &str, run_id: &str) -> Value {
    let mut baton = json!({
        "schema": schema,
        "updated_at": "2026-07-06T00:00:00Z",
        "mode": "development",
        "profile": "full",
        "profile_floor": "full",
        "profile_decision": {
            "selected_profile": "full",
            "floor": "full",
            "reason": "read-path compatibility fixture",
            "decided_by": "test-suite",
            "decided_at": "2026-07-06T00:00:00Z",
            "risk_inputs": [],
            "hard_triggers": [],
            "allowlist_match": false,
            "allowlist_refs": [],
            "evidence_refs": ["test:v3-read-path"]
        },
        "profile_history": [],
        "run_mode": "walkaway",
        "run_id": run_id,
        "original_ask": "Read both v2 and v3 batons.",
        "phase": 1,
        "total_phases": 1,
        "phase_profiles": null,
        "status": "implementing",
        "assignee": "vadi",
        "active_roles": [],
        "agent_instances": [],
        "current_engine": "codex",
        "review_target": null,
        "research_ref": "./superpowers/research/read-path.html",
        "plan_ref": "./superpowers/plans/read-path.html",
        "run_explainer_ref": null,
        "run_explainer_reviews": [],
        "research_outcome": null,
        "review_ref": null,
        "review_intake": null,
        "work_split": [{
            "id": "read-work",
            "phase": "1",
            "chunk_type": "implementation",
            "owner": "vadi",
            "owner_role": "vadi",
            "paths": ["rust/dvandva/src/state.rs"],
            "write_paths": ["rust/dvandva/src/state.rs"],
            "depends_on": [],
            "status": "planned",
            "artifact_refs": []
        }],
        "subagent_tracks": [],
        "verification_matrix": [{
            "id": "verify-read-path",
            "phase": "1",
            "command": "cargo test --test v3_read_path",
            "result": "pending"
        }],
        "master_plan_locked": true,
        "amendment_from_phase": null,
        "question": null,
        "resume_assignee": null,
        "resume_status": null,
        "disagreement_round": 0,
        "disagreement_cap": 3,
        "loop_counts": {},
        "turn_cap": 60,
        "branch": "test-branch",
        "checkpoint": 7,
        "allow_commit": true,
        "allow_push": true,
        "allow_pr": false,
        "vadi_final_approval": false,
        "prativadi_final_approval": false,
        "final_commit": null,
        "pushed_ref": null,
        "summary": "Read-path compatibility fixture.",
        "changed_paths": [],
        "verification": [],
        "findings": [],
        "narrow_fixups": [],
        "vadi_counter": [],
        "deferred": [],
        "blockers": [],
        "next_action": "vadi: continue."
    });
    if schema == "dvandva.baton.v3" {
        baton["run_workflow"] = json!({
            "source": "preset:full",
            "declared_by": "vadi",
            "declared_at_checkpoint": 0,
            "approved_by": null,
            "approved_at_checkpoint": null,
            "revision_round": 0,
            "states": [],
            "edges": [],
            "amendments": []
        });
    }
    baton
}

fn assert_read_subcommands(schema: &str) {
    let dir = tempfile::tempdir().unwrap();
    let run_id = schema.replace("dvandva.baton.", "read-");
    let baton = dir
        .path()
        .join(format!(".dvandva/runs/{run_id}/baton.json"));
    write_json(&baton, &read_path_baton(schema, &run_id));

    let baton_arg = baton.to_str().unwrap();
    let (state_code, state_out, state_err) = run(
        &["state", "--compact", "--file", baton_arg, "--role", "vadi"],
        None,
    );
    assert_eq!(state_code, 0, "state stderr:\n{state_err}");
    let state: Value = serde_json::from_str(&state_out).expect("state JSON");
    assert_eq!(state["schema"], schema);
    assert_eq!(state["status"], "implementing");
    assert_eq!(state["current_role_work"].as_array().unwrap().len(), 1);

    let (brief_code, brief_out, brief_err) =
        run(&["brief", "--role", "vadi", "--file", baton_arg], None);
    assert_eq!(brief_code, 0, "brief stderr:\n{brief_err}");
    assert!(brief_out.contains(&format!("# Dvandva brief — {run_id} (vadi)")));
    assert!(brief_out.contains("- status: implementing"));
    assert!(brief_out.contains("- read-work:"));

    let (wait_code, wait_out, wait_err) = run(
        &[
            "wait",
            "--role",
            "vadi",
            "--file",
            baton_arg,
            "--finite",
            "--max-wait",
            "0",
        ],
        None,
    );
    assert_eq!(wait_code, 0, "wait stderr:\n{wait_err}");
    assert!(
        wait_out.contains("DVANDVA_WAIT ready role=vadi phase=1 status=implementing checkpoint=7"),
        "wait output:\n{wait_out}"
    );

    let (resolve_code, resolve_out, resolve_err) = run(
        &[
            "resolve",
            "--role",
            "prativadi",
            "--cwd",
            dir.path().to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(resolve_code, 0, "resolve stderr:\n{resolve_err}");
    assert!(
        resolve_out.contains(&format!("RESOLVED .dvandva/runs/{run_id}/baton.json")),
        "resolve output:\n{resolve_out}"
    );
}

#[test]
fn read_subcommands_accept_v2_and_v3_batons() {
    assert_read_subcommands("dvandva.baton.v2");
    assert_read_subcommands("dvandva.baton.v3");
}

#[test]
fn live_references_expose_v3_and_mark_v2_historical() {
    let root = repo_root();

    let v2_text = fs::read_to_string(root.join("plugins/dvandva/references/baton-schema-v2.json"))
        .expect("read baton-schema-v2.json");
    assert!(
        v2_text.contains("HISTORICAL: dvandva.baton.v2"),
        "baton-schema-v2.json must be marked historical after v3 becomes the live write schema"
    );

    let v3_path = root.join("plugins/dvandva/references/baton-schema-v3.json");
    let v3_text = fs::read_to_string(&v3_path).expect("read baton-schema-v3.json");
    let v3: Value = serde_json::from_str(&v3_text).expect("parse baton-schema-v3.json");
    assert_eq!(v3["schema"], "dvandva.baton.v3");
    assert!(v3.get("run_workflow").is_some());
    let catalog = v3["status_catalog"]
        .as_array()
        .expect("v3 status_catalog array");
    assert_eq!(catalog.len(), 26);
    assert!(catalog.iter().any(|v| v == "abandoned"));

    let report = dvandva::lint::schema_parity::report(&root);
    assert!(
        report.passed(),
        "schema parity failures: {}",
        report.failures()
    );
}
