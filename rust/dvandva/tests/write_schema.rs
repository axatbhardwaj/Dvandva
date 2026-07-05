//! `dvandva write` — schema / required-keys / status / assignee / checkpoint /
//! phase-status / mode / done-evidence / run-explainer / backcompat themes.
//!
//! Ported from `scripts/test-dvandva-write.sh`; each `#[test]` name mirrors the
//! shell case label.

mod common;

use common::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

/// A named test-case mutator paired with its label, for tables of
/// otherwise-identical cases that each apply a different override.
type NamedMutator = (&'static str, Box<dyn Fn(&mut Value)>);

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}
fn paths(dir: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    (
        dir.path().join("baton.json"),
        dir.path().join("baton.next.json"),
    )
}

// ===================== scaffold =====================
// S5-T2 (D5): `scaffold_installs_and_snapshots` (v1 happy-path scaffold) was
// REMOVED — v1 is retired from the write path. The v2 scaffold happy path is
// `v2_scaffold_clarifying_questions_drafting_installs`; retirement is probed
// by `s5t2_*`.

#[test]
fn run_isolation_two_named_runs() {
    let d = tmp();
    let alpha = d.path().join(".dvandva/runs/alpha");
    let beta = d.path().join(".dvandva/runs/beta");
    make_baton_v2(
        &alpha.join("baton.next.json"),
        "clarifying_questions_drafting",
        "vadi",
        0,
        |b| {
            b["branch"] = json!("alpha-branch");
            b["run_id"] = json!("alpha");
            b["phase"] = json!("clarifying");
        },
    );
    make_baton_v2(
        &beta.join("baton.next.json"),
        "clarifying_questions_drafting",
        "vadi",
        0,
        |b| {
            b["branch"] = json!("beta-branch");
            b["run_id"] = json!("beta");
            b["phase"] = json!("clarifying");
        },
    );
    run(&alpha.join("baton.json"), &alpha.join("baton.next.json"))
        .assert("run alpha v2 scaffold", 0);
    run(&beta.join("baton.json"), &beta.join("baton.next.json")).assert("run beta v2 scaffold", 0);
    assert!(alpha
        .join("history/0-clarifying_questions_drafting-vadi.json")
        .is_file());
    assert!(beta
        .join("history/0-clarifying_questions_drafting-vadi.json")
        .is_file());
    assert!(!d.path().join(".dvandva/history").exists());
}

// S5-T2 (D5): `legacy_dot_dvandva_v1_scaffold_allowed` was REMOVED — the legacy
// v1 `.dvandva/baton.json` scaffold is retired from the write path.

#[test]
fn named_run_v1_scaffold_now_schema_retired() {
    // S5-T2: a v1 scaffold under a named run dir is now retired with the
    // migration hint (the `schema_retired` gate fires ahead of `bad_run_id_dir`).
    let d = tmp();
    let rd = d.path().join(".dvandva/runs/alpha");
    make_baton(
        &rd.join("baton.next.json"),
        "spec_drafting",
        "vadi",
        0,
        |_| {},
    );
    run(&rd.join("baton.json"), &rd.join("baton.next.json")).assert_contains(
        "named run v1 scaffold is retired",
        23,
        "DVANDVA_WRITE schema_retired",
    );
}

#[test]
fn named_run_v2_run_id_mismatch_exits_23() {
    let d = tmp();
    let rd = d.path().join(".dvandva/runs/alpha");
    make_baton_v2(
        &rd.join("baton.next.json"),
        "research_drafting",
        "vadi",
        0,
        |b| {
            b["run_id"] = json!("beta");
        },
    );
    run(&rd.join("baton.json"), &rd.join("baton.next.json")).assert_contains(
        "run_id mismatch",
        23,
        "DVANDVA_WRITE bad_run_id_dir",
    );
}

#[test]
fn named_run_v2_run_id_null_exits_23() {
    let d = tmp();
    let rd = d.path().join(".dvandva/runs/alpha");
    make_baton_v2(
        &rd.join("baton.next.json"),
        "research_drafting",
        "vadi",
        0,
        |b| {
            b["run_id"] = Value::Null;
        },
    );
    run(&rd.join("baton.json"), &rd.join("baton.next.json")).assert_contains(
        "null run_id",
        23,
        "DVANDVA_WRITE bad_run_id_dir",
    );
}

#[test]
fn named_run_v2_run_id_missing_exits_23() {
    let d = tmp();
    let rd = d.path().join(".dvandva/runs/alpha");
    make_baton_v2(
        &rd.join("baton.next.json"),
        "research_drafting",
        "vadi",
        0,
        |b| {
            b.as_object_mut().unwrap().remove("run_id");
        },
    );
    run(&rd.join("baton.json"), &rd.join("baton.next.json")).assert_contains(
        "missing run_id",
        23,
        "DVANDVA_WRITE bad_run_id_dir",
    );
}

#[test]
fn named_run_v2_run_id_empty_exits_23() {
    let d = tmp();
    let rd = d.path().join(".dvandva/runs/alpha");
    make_baton_v2(
        &rd.join("baton.next.json"),
        "research_drafting",
        "vadi",
        0,
        |b| {
            b["run_id"] = json!("");
        },
    );
    run(&rd.join("baton.json"), &rd.join("baton.next.json")).assert_contains(
        "empty run_id",
        23,
        "DVANDVA_WRITE bad_run_id_dir",
    );
}

// S5-T2 (D5): `scaffold_wrong_initial_status_exits_24` was REMOVED — it probed
// the v1 scaffold-status gate, which is unreachable now that v1 candidates are
// rejected upstream with `schema_retired` (see `s5t2_*`).

// ===================== candidate-level validation =====================

#[test]
fn missing_candidate_exits_21() {
    let d = tmp();
    let (b, n) = paths(&d);
    run(&b, &n).assert("missing candidate", 21);
}

#[test]
fn invalid_candidate_json_exits_22() {
    let d = tmp();
    let (b, n) = paths(&d);
    std::fs::write(&n, "{\"schema\": ").unwrap();
    run(&b, &n).assert("invalid json", 22);
}

#[test]
fn wrong_schema_string_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&n, "spec_drafting", "vadi", 0, |b| {
        b["schema"] = json!("dvandva.baton.v3");
    });
    run(&b, &n).assert_contains("wrong schema", 23, "DVANDVA_WRITE schema_mismatch");
}

// S5-T2 (D5): the generic shape tests below were CONVERTED from v1 to v2
// (make_baton -> make_baton_v2). The checks they exercise (missing key, status
// enum, empty assignee, checkpoint type) are engine-wide and were only vehicled
// on v1 before; a v1 baton now short-circuits to `schema_retired`, so these
// carry the coverage forward on a v2 baton.

#[test]
fn missing_required_key_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b.as_object_mut().unwrap().remove("branch");
    });
    run(&b, &n).assert_contains("missing key", 23, "DVANDVA_WRITE missing_key");
}

#[test]
fn unknown_status_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["status"] = json!("doing_stuff");
    });
    run(&b, &n).assert("unknown status", 23);
}

#[test]
fn empty_assignee_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["assignee"] = json!("");
    });
    run(&b, &n).assert("empty assignee", 23);
}

#[test]
fn string_checkpoint_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["checkpoint"] = json!("5");
    });
    run(&b, &n).assert("string checkpoint", 23);
}

#[test]
fn octal_string_checkpoint_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_drafting", "vadi", 7, |_| {});
    make_baton_v2(&n, "research_review", "prativadi", 8, |b| {
        b["checkpoint"] = json!("08");
    });
    run(&b, &n).assert("octal string checkpoint", 23);
}

#[test]
fn string_checkpoint_in_current_baton_exits_25() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_drafting", "vadi", 7, |b| {
        b["checkpoint"] = json!("7");
    });
    make_baton_v2(&n, "research_review", "prativadi", 8, |_| {});
    run(&b, &n).assert("string checkpoint current", 25);
}

// ===================== v2 candidate-level validation =====================

#[test]
fn v2_scaffold_clarifying_questions_drafting_installs() {
    // P1/Task 1.2: the checkpoint-0 scaffold seed now requires
    // clarifying_questions_drafting, the mandatory pre-research gate.
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "clarifying_questions_drafting", "vadi", 0, |b| {
        b["phase"] = json!("clarifying");
    });
    run(&b, &n).assert("v2 scaffold", 0);
    let installed: Value = serde_json::from_slice(&std::fs::read(&b).unwrap()).unwrap();
    assert_eq!(installed["run_id"], "run-a");
    assert!(d
        .path()
        .join("history/0-clarifying_questions_drafting-vadi.json")
        .is_file());
}

#[test]
fn v2_scaffold_research_drafting_rejected_as_seed() {
    // P1/Task 1.2 regression: the OLD scaffold seed (research_drafting at
    // checkpoint 0) is no longer legal now that clarifying_questions_drafting
    // is the mandatory first state.
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |_| {});
    run(&b, &n).assert_contains(
        "research_drafting is no longer a legal scaffold seed",
        24,
        "DVANDVA_WRITE illegal_transition",
    );
    assert!(!b.is_file(), "a rejected scaffold must not install a baton");
}

#[test]
fn v2_empty_run_id_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["run_id"] = json!("")
    });
    run(&b, &n).assert_contains("empty run_id", 23, "DVANDVA_WRITE bad_run_id");
}

#[test]
fn v2_unsafe_parent_run_id_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["run_id"] = json!("../escape")
    });
    run(&b, &n).assert_contains("unsafe parent run_id", 23, "DVANDVA_WRITE bad_run_id");
}

#[test]
fn v2_unsafe_slash_run_id_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["run_id"] = json!("alpha/beta")
    });
    run(&b, &n).assert_contains("unsafe slash run_id", 23, "DVANDVA_WRITE bad_run_id");
}

#[test]
fn v2_empty_original_ask_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["original_ask"] = json!("")
    });
    run(&b, &n).assert_contains("empty original_ask", 23, "DVANDVA_WRITE bad_original_ask");
}

#[test]
fn v2_missing_work_split_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b.as_object_mut().unwrap().remove("work_split");
    });
    run(&b, &n).assert_contains(
        "missing work_split",
        23,
        "DVANDVA_WRITE missing_key key=work_split",
    );
}

#[test]
fn v2_empty_work_split_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["work_split"] = json!([])
    });
    run(&b, &n).assert_contains("empty work_split", 23, "DVANDVA_WRITE bad_work_split");
}

#[test]
fn v2_unsafe_work_split_path_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["work_split"][0]["paths"] = json!(["../escape"]);
    });
    run(&b, &n).assert_contains("unsafe work_split path", 23, "DVANDVA_WRITE bad_work_split");
}

#[test]
fn v2_empty_verification_matrix_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["verification_matrix"] = json!([])
    });
    run(&b, &n).assert_contains(
        "empty verification_matrix",
        23,
        "DVANDVA_WRITE bad_verification_matrix",
    );
}

#[test]
fn v2_missing_run_explainer_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b.as_object_mut().unwrap().remove("run_explainer_ref");
    });
    run(&b, &n).assert_contains(
        "missing run_explainer_ref",
        23,
        "DVANDVA_WRITE missing_key key=run_explainer_ref",
    );
}

#[test]
fn v2_missing_active_roles_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b.as_object_mut().unwrap().remove("active_roles");
    });
    run(&b, &n).assert_contains(
        "missing active_roles",
        23,
        "DVANDVA_WRITE missing_key key=active_roles",
    );
}

#[test]
fn v2_arbitrary_review_target_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["review_target"] = json!("DR6-DR7-profile-matrix-fixups")
    });
    run(&b, &n).assert_contains(
        "arbitrary review_target",
        23,
        "DVANDVA_WRITE bad_review_target",
    );
}

#[test]
fn v2_missing_agent_instances_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b.as_object_mut().unwrap().remove("agent_instances");
    });
    run(&b, &n).assert_contains(
        "missing agent_instances",
        23,
        "DVANDVA_WRITE missing_key key=agent_instances",
    );
}

#[test]
fn v2_non_array_agent_instances_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["agent_instances"] = json!({})
    });
    run(&b, &n).assert_contains(
        "non-array agent_instances",
        23,
        "DVANDVA_WRITE bad_agent_instances",
    );
}

#[test]
fn v2_duplicate_active_roles_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["active_roles"] = json!(["vadi", "vadi"])
    });
    run(&b, &n).assert_contains(
        "duplicate active_roles",
        23,
        "DVANDVA_WRITE bad_active_roles",
    );
}

#[test]
fn v2_empty_subagent_tracks_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["subagent_tracks"] = json!([])
    });
    run(&b, &n).assert_contains(
        "empty subagent_tracks",
        23,
        "DVANDVA_WRITE bad_subagent_tracks",
    );
}

#[test]
fn v2_malformed_subagent_tracks_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["subagent_tracks"][0]
            .as_object_mut()
            .unwrap()
            .remove("owner");
    });
    run(&b, &n).assert_contains(
        "malformed subagent_tracks",
        23,
        "DVANDVA_WRITE bad_subagent_tracks",
    );
}

#[test]
fn v2_null_subagent_track_phase_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["subagent_tracks"][0]["phase"] = Value::Null;
    });
    run(&b, &n).assert_contains("null track phase", 23, "DVANDVA_WRITE bad_subagent_tracks");
}

#[test]
fn v2_fake_parallel_subagent_track_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["subagent_tracks"][0]["parallelized"] = json!(true);
        b["subagent_tracks"][0]["owner"] = json!("vadi");
        b["subagent_tracks"][0]["outputs"] = json!([]);
        b["subagent_tracks"][0]["evidence_refs"] = json!([]);
    });
    run(&b, &n).assert_contains(
        "fake parallel track",
        23,
        "DVANDVA_WRITE bad_subagent_tracks",
    );
}

#[test]
fn v2_standalone_parallel_subagent_owner_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "clarifying_questions_drafting", "vadi", 0, |b| {
        b["phase"] = json!("clarifying");
        b["subagent_tracks"][0]["parallelized"] = json!(true);
        b["subagent_tracks"][0]["owner"] = json!("adversarial-analyst");
        b["subagent_tracks"][0]["outputs"] = json!(["Independent review completed."]);
        b["subagent_tracks"][0]["evidence_refs"] = json!(["subagent:adversarial-analyst"]);
    });
    run(&b, &n).assert("standalone parallel owner", 0);
}

#[test]
fn v2_bundled_adversarial_parallel_owner_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "clarifying_questions_drafting", "vadi", 0, |b| {
        b["phase"] = json!("clarifying");
        b["subagent_tracks"][0]["parallelized"] = json!(true);
        b["subagent_tracks"][0]["owner"] = json!("dvandva-adversarial-analyst");
        b["subagent_tracks"][0]["outputs"] = json!(["Bundled adversarial review completed."]);
        b["subagent_tracks"][0]["evidence_refs"] = json!(["subagent:dvandva-adversarial-analyst"]);
    });
    run(&b, &n).assert("bundled adversarial owner", 0);
}

#[test]
fn v2_new_bundled_owners_accepted() {
    for owner in [
        "dvandva-security-auditor",
        "dvandva-integration-checker",
        "dvandva-debugger",
        "dvandva-doc-verifier",
        "dvandva-pattern-mapper",
    ] {
        let d = tmp();
        let (b, n) = paths(&d);
        make_baton_v2(&n, "clarifying_questions_drafting", "vadi", 0, |b| {
            b["phase"] = json!("clarifying");
            b["subagent_tracks"][0]["parallelized"] = json!(true);
            b["subagent_tracks"][0]["owner"] = json!(owner);
            b["subagent_tracks"][0]["outputs"] =
                json!([format!("New bundled owner accepted: {owner}")]);
            b["subagent_tracks"][0]["evidence_refs"] = json!([format!("subagent:{owner}")]);
        });
        run(&b, &n).assert(&format!("bundled owner {owner}"), 0);
    }
}

// ---- dynamic agent instances ----

#[test]
fn v2_dynamic_owner_requires_agent_instance() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_parallel_track(b);
        b["agent_instances"] = json!([]);
    });
    run(&b, &n).assert_contains(
        "dynamic owner needs instance",
        23,
        "DVANDVA_WRITE bad_agent_instances",
    );
}

#[test]
fn v2_nonparallel_dynamic_owner_requires_agent_instance() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_parallel_track(b);
        b["subagent_tracks"][0]["parallelized"] = json!(false);
        b["agent_instances"] = json!([]);
    });
    run(&b, &n).assert_contains(
        "nonparallel dynamic owner",
        23,
        "DVANDVA_WRITE bad_agent_instances",
    );
}

#[test]
fn v2_dynamic_owner_requires_closure_evidence() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        dynamic_parallel_track(b);
        b["agent_instances"][0]["evidence_refs"] = json!(["subagent:r3-generated-dynamic-review"]);
        b["agent_instances"][0]["closed_at"] = Value::Null;
    });
    run(&b, &n).assert_contains(
        "dynamic closure evidence",
        23,
        "DVANDVA_WRITE bad_agent_instances",
    );
}

#[test]
fn v2_dynamic_owner_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "clarifying_questions_drafting", "vadi", 0, |b| {
        b["phase"] = json!("clarifying");
        dynamic_agent_instances(b);
        dynamic_parallel_track(b);
    });
    run(&b, &n).assert("dynamic owner accepted", 0);
}

#[test]
fn v2_nonparallel_dynamic_owner_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "clarifying_questions_drafting", "vadi", 0, |b| {
        b["phase"] = json!("clarifying");
        dynamic_agent_instances(b);
        dynamic_parallel_track(b);
        b["subagent_tracks"][0]["parallelized"] = json!(false);
    });
    run(&b, &n).assert("nonparallel dynamic owner accepted", 0);
}

#[test]
fn v2_dynamic_owner_role_must_match_parent_role() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        dynamic_parallel_track(b);
        b["subagent_tracks"][0]["owner_role"] = json!("prativadi");
    });
    run(&b, &n).assert_contains("owner_role match", 23, "DVANDVA_WRITE bad_agent_instances");
}

#[test]
fn v2_agent_instance_field_validation() {
    // parent_role, spawned_by, spawned_at_checkpoint, phase, purpose, kind,
    // status, read_paths, depends_on, output_refs, base_checkpoint, result,
    // work_item_ids, permission, model, unsafe id, reserved id.
    let cases: Vec<NamedMutator> = vec![
        (
            "bad parent_role",
            Box::new(|b: &mut Value| b["agent_instances"][0]["parent_role"] = json!("team")),
        ),
        (
            "blank spawned_by",
            Box::new(|b: &mut Value| b["agent_instances"][0]["spawned_by"] = json!("   ")),
        ),
        (
            "bad spawn checkpoint",
            Box::new(|b: &mut Value| b["agent_instances"][0]["spawned_at_checkpoint"] = json!("0")),
        ),
        (
            "empty phase",
            Box::new(|b: &mut Value| b["agent_instances"][0]["phase"] = json!("")),
        ),
        (
            "blank purpose",
            Box::new(|b: &mut Value| b["agent_instances"][0]["purpose"] = json!("   ")),
        ),
        (
            "wrong kind",
            Box::new(|b: &mut Value| b["agent_instances"][0]["agent_kind"] = json!("static")),
        ),
        (
            "bad status",
            Box::new(|b: &mut Value| b["agent_instances"][0]["status"] = json!("done")),
        ),
        (
            "unsafe read path",
            Box::new(|b: &mut Value| b["agent_instances"][0]["read_paths"] = json!(["/absolute"])),
        ),
        (
            "bad depends_on",
            Box::new(|b: &mut Value| b["agent_instances"][0]["depends_on"] = json!("r3-other")),
        ),
        (
            "bad output_refs",
            Box::new(|b: &mut Value| b["agent_instances"][0]["output_refs"] = json!("x")),
        ),
        (
            "bad base_checkpoint",
            Box::new(|b: &mut Value| b["agent_instances"][0]["base_checkpoint"] = json!("0")),
        ),
        (
            "closed missing result",
            Box::new(|b: &mut Value| b["agent_instances"][0]["result"] = json!("")),
        ),
        (
            "closed empty work_items",
            Box::new(|b: &mut Value| b["agent_instances"][0]["work_item_ids"] = json!([])),
        ),
        (
            "unsafe id",
            Box::new(|b: &mut Value| b["agent_instances"][0]["id"] = json!("../escape")),
        ),
        (
            "bad permission",
            Box::new(|b: &mut Value| {
                b["agent_instances"][0]["permission_class"] = json!("full-write")
            }),
        ),
        (
            "bad model",
            Box::new(|b: &mut Value| b["agent_instances"][0]["model_class"] = json!("haiku")),
        ),
    ];
    for (name, mutate) in cases {
        let d = tmp();
        let (b, n) = paths(&d);
        make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
            dynamic_agent_instances(b);
            mutate(b);
        });
        run(&b, &n).assert_contains(name, 23, "DVANDVA_WRITE bad_agent_instances");
    }
}

#[test]
fn v2_agent_instance_accepts_canonical_and_legacy_model_aliases() {
    for model_class in [
        "opus-class|gpt-5.5-xhigh",
        "sonnet-class|gpt-5.5-high",
        "opus",
        "sonnet",
        "opus-class|gpt-5.5",
        "sonnet-class|gpt-5.4",
        "gpt-5.5",
        "gpt-5.4",
    ] {
        let d = tmp();
        let (b, n) = paths(&d);
        make_baton_v2(&n, "clarifying_questions_drafting", "vadi", 0, |b| {
            b["phase"] = json!("clarifying");
            dynamic_agent_instances(b);
            b["agent_instances"][0]["model_class"] = json!(model_class);
        });
        run(&b, &n).assert(model_class, 0);
    }
}

#[test]
fn v2_dynamic_owner_requires_output_refs() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        dynamic_parallel_track(b);
        b["agent_instances"][0]["output_refs"] = json!([]);
    });
    run(&b, &n).assert_contains(
        "dynamic output_refs",
        23,
        "DVANDVA_WRITE bad_agent_instances",
    );
}

#[test]
fn v2_duplicate_agent_instance_ids_exit_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        let first = b["agent_instances"][0].clone();
        b["agent_instances"].as_array_mut().unwrap().push(first);
    });
    run(&b, &n).assert_contains("duplicate ids", 23, "DVANDVA_WRITE bad_agent_instances");
}

#[test]
fn v2_reserved_agent_instance_ids_rejected() {
    for reserved in ["dvandva-implementer", "adversarial-analyst", "vadi"] {
        let d = tmp();
        let (b, n) = paths(&d);
        make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
            dynamic_agent_instances(b);
            b["agent_instances"][0]["id"] = json!(reserved);
        });
        run(&b, &n).assert_contains(
            &format!("reserved {reserved}"),
            23,
            "DVANDVA_WRITE bad_agent_instances",
        );
    }
}

// ===================== v2 phase↔status =====================

#[test]
fn v2_implementation_status_rejects_research_phase() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "implementing", "vadi", 0, |b| {
        b["phase"] = json!("research")
    });
    run(&b, &n).assert_contains(
        "phase status mismatch",
        23,
        "DVANDVA_WRITE bad_phase_status",
    );
}

#[test]
fn dev_deep_review_string_phase_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "deep_review", "prativadi", 0, |b| {
        b["phase"] = json!("deep-review-string")
    });
    run(&b, &n).assert_contains(
        "dev deep_review string phase",
        23,
        "DVANDVA_WRITE bad_phase_status",
    );
}

// ===================== research_ref after draft =====================

#[test]
fn v2_missing_research_ref_after_draft_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_review", "prativadi", 0, |b| {
        b["research_ref"] = Value::Null
    });
    run(&b, &n).assert_contains("missing research_ref", 23, "DVANDVA_WRITE bad_research_ref");
}

#[test]
fn v2_research_drafting_without_research_ref_can_enter_human_question() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_drafting", "vadi", 0, |b| {
        b["research_ref"] = Value::Null
    });
    make_baton_v2(&n, "human_question", "human", 1, |b| {
        b["research_ref"] = Value::Null;
        b["question"] = json!("Which source should research use?");
        b["resume_assignee"] = json!("vadi");
        b["resume_status"] = json!("research_drafting");
    });
    run(&b, &n).assert("research_drafting -> human_question", 0);
}

#[test]
fn v2_research_drafting_without_research_ref_can_escalate_human_decision() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_drafting", "vadi", 0, |b| {
        b["research_ref"] = Value::Null
    });
    make_baton_v2(&n, "human_decision", "human", 1, |b| {
        b["research_ref"] = Value::Null
    });
    run(&b, &n).assert("research_drafting -> human_decision", 0);
}

// ===================== assignee-owner =====================

#[test]
fn v2_research_revision_requires_vadi() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "research_revision", "prativadi", 5, |_| {});
    run(&b, &n).assert_contains("research_revision owner", 23, "bad_assignee_owner");
}

#[test]
fn v2_deep_review_requires_prativadi() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "test_creation", "vadi", 4, |_| {});
    make_baton_v2(&n, "deep_review", "vadi", 5, |_| {});
    run(&b, &n).assert_contains("deep_review owner", 23, "bad_assignee_owner");
}

#[test]
fn v2_deslop_requires_vadi() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "deslop", "prativadi", 5, |_| {});
    run(&b, &n).assert_contains("deslop owner", 23, "bad_assignee_owner");
}

#[test]
fn v2_parallel_implementing_requires_team() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "parallel_implementing", "vadi", 5, parallel_chunks);
    run(&b, &n).assert_contains("parallel owner", 23, "bad_assignee_owner");
}

#[test]
fn v2_parallel_implementing_requires_both_roles() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "parallel_implementing", "team", 5, |b| {
        parallel_chunks(b);
        b["active_roles"] = json!(["vadi"]);
    });
    run(&b, &n).assert_contains("parallel roles", 23, "bad_active_roles");
}

#[test]
fn v2_termination_review_missing_active_roles() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |_| {});
    make_baton_v2(&n, "termination_review", "team", 5, |b| {
        b["vadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains("termination roles", 23, "bad_active_roles");
}

// ===================== mode alias / enum / immutability =====================

#[test]
fn mode_feature_pr_on_development_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        b["mode"] = json!("feature-pr")
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["mode"] = json!("feature-pr")
    });
    run(&b, &n).assert("feature-pr dev edge", 0);
}

#[test]
fn mode_bogus_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["mode"] = json!("bogus")
    });
    run(&b, &n).assert_contains("bogus mode", 23, "DVANDVA_WRITE bad_mode");
}

#[test]
fn mode_fast_still_invalid() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        b["mode"] = json!("fast")
    });
    run(&b, &n).assert_contains("fast mode invalid", 23, "DVANDVA_WRITE bad_mode");
}

#[test]
fn mode_dev_to_research_mutation_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        b["mode"] = json!("development")
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["mode"] = json!("research")
    });
    run(&b, &n).assert_contains("mode mutation", 24, "mode_change");
}

#[test]
fn mode_feature_pr_to_development_immutable_equal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        b["mode"] = json!("feature-pr")
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["mode"] = json!("development")
    });
    run(&b, &n).assert("feature-pr == development", 0);
}

#[test]
fn mode_development_to_feature_pr_immutable_equal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        b["mode"] = json!("development")
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["mode"] = json!("feature-pr")
    });
    run(&b, &n).assert("development == feature-pr", 0);
}

// ===================== phase-type for research/review modes =====================

#[test]
fn research_review_numeric_phase_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_drafting", "vadi", 4, |b| {
        b["mode"] = json!("research")
    });
    make_baton_v2(&n, "research_review", "prativadi", 5, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!(1);
    });
    run(&b, &n).assert_contains(
        "research numeric phase",
        23,
        "DVANDVA_WRITE bad_phase_status",
    );
}

#[test]
fn research_spec_review_research_phase_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_drafting", "vadi", 4, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!("spec");
    });
    make_baton_v2(&n, "spec_review", "prativadi", 5, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!("research");
    });
    run(&b, &n).assert_contains("research spec phase", 23, "DVANDVA_WRITE bad_phase_status");
}

#[test]
fn review_research_review_research_phase_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_drafting", "vadi", 4, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!("review");
    });
    make_baton_v2(&n, "research_review", "prativadi", 5, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!("research");
    });
    run(&b, &n).assert_contains(
        "review research phase",
        23,
        "DVANDVA_WRITE bad_phase_status",
    );
}

#[test]
fn review_deep_review_numeric_phase_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!("review");
    });
    make_baton_v2(&n, "deep_review", "prativadi", 5, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!(1);
    });
    run(&b, &n).assert_contains("review numeric phase", 23, "DVANDVA_WRITE bad_phase_status");
}

// ===================== done evidence by mode =====================

#[test]
fn v2_done_requires_run_explainer_ref() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["active_roles"] = json!(["vadi", "prativadi"])
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["run_explainer_ref"] = Value::Null
    });
    run(&b, &n).assert_contains("done needs explainer", 23, "bad_run_explainer_ref");
}

#[test]
fn v2_done_rejects_invalid_run_explainer_path() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["active_roles"] = json!(["vadi", "prativadi"])
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["run_explainer_ref"] = json!("../run-a-explainer.html")
    });
    run(&b, &n).assert_contains("bad explainer path", 23, "bad_run_explainer_ref");
}

#[test]
fn v2_done_rejects_mismatched_run_explainer() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["run_id"] = json!("alpha");
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["run_id"] = json!("alpha");
        b["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-beta-explainer.html");
    });
    run(&b, &n).assert_contains("mismatched explainer", 23, "bad_run_explainer_ref");
}

#[test]
fn v2_done_valid_run_explainer() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path()); // S4-T1
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(b);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(b);
        explainer_verification_track(b); // F10
        done_matrix_fresh(b); // S4-T6
    });
    run(&b, &n).assert("done valid explainer", 0);
}

#[test]
fn v2_done_accepts_date_prefixed_run_id_explainer() {
    let d = tmp();
    let (b, n) = paths(&d);
    let run_id = "2026-06-29-baton-accuracy-hook-coexist";
    let refp = "./superpowers/run-reports/2026-06-29-baton-accuracy-hook-coexist-explainer.html";
    seed_done_artifacts(d.path()); // S4-T1
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["run_id"] = json!(run_id);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["run_explainer_ref"] = json!(refp);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
        date_prefixed_run_explainer_reviews(b);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["run_id"] = json!(run_id);
        b["run_explainer_ref"] = json!(refp);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
        date_prefixed_run_explainer_reviews(b);
        explainer_verification_track(b); // F10
        done_matrix_fresh(b); // S4-T6
    });
    run(&b, &n).assert("date-prefixed explainer", 0);
}

#[test]
fn v2_done_rejects_double_date_run_id_explainer() {
    let d = tmp();
    let (b, n) = paths(&d);
    let run_id = "2026-06-29-baton-accuracy-hook-coexist";
    let refp =
        "./superpowers/run-reports/2026-06-30-2026-06-29-baton-accuracy-hook-coexist-explainer.html";
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["run_id"] = json!(run_id);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["run_explainer_ref"] = json!(refp);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
        double_date_run_explainer_reviews(b);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["run_id"] = json!(run_id);
        b["run_explainer_ref"] = json!(refp);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
        double_date_run_explainer_reviews(b);
    });
    run(&b, &n).assert_contains("double-date explainer", 23, "bad_run_explainer_ref");
}

#[test]
fn v2_done_accepts_coordinator_assignees() {
    for owner in ["human", "team", "vadi", "prativadi"] {
        let d = tmp();
        let (b, n) = paths(&d);
        seed_done_artifacts(d.path()); // S4-T1
        make_baton_v2(&b, "termination_review", "team", 4, |b| {
            b["active_roles"] = json!(["vadi", "prativadi"]);
            b["run_explainer_ref"] =
                json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
            b["vadi_final_approval"] = json!(true);
            b["prativadi_final_approval"] = json!(true);
            run_explainer_reviews(b);
        });
        make_baton_v2(&n, "done", owner, 5, |b| {
            b["run_explainer_ref"] =
                json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
            b["vadi_final_approval"] = json!(true);
            b["prativadi_final_approval"] = json!(true);
            run_explainer_reviews(b);
            explainer_verification_track(b); // F10
            done_matrix_fresh(b); // S4-T6
        });
        run(&b, &n).assert(&format!("done owner {owner}"), 0);
    }
}

#[test]
fn v2_done_rejects_generated_assignee() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["active_roles"] = json!(["vadi", "prativadi"])
    });
    make_baton_v2(&n, "done", "r3-generated-dynamic-review", 5, |b| {
        b["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(b);
    });
    run(&b, &n).assert_contains(
        "done generated assignee",
        23,
        "DVANDVA_WRITE bad_done_state",
    );
}

#[test]
fn v2_done_rejects_missing_final_approval() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["active_roles"] = json!(["vadi", "prativadi"])
    });
    make_baton_v2(&n, "done", "team", 5, |b| {
        b["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(false);
        run_explainer_reviews(b);
    });
    run(&b, &n).assert_contains("done both approvals", 23, "DVANDVA_WRITE bad_done_state");
}

#[test]
fn v2_done_run_explainer_review_variants_rejected() {
    // missing reviews, one review, mismatched ref, incomplete, unapproved,
    // blank summary, empty evidence.
    let variants: Vec<NamedMutator> = vec![
        ("missing reviews", Box::new(|_b: &mut Value| {})),
        (
            "one review",
            Box::new(|b: &mut Value| {
                run_explainer_reviews(b);
                let keep: Vec<Value> = b["run_explainer_reviews"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|r| r["role"] == "vadi")
                    .cloned()
                    .collect();
                b["run_explainer_reviews"] = Value::Array(keep);
            }),
        ),
        (
            "mismatched ref",
            Box::new(|b: &mut Value| {
                run_explainer_reviews(b);
                b["run_explainer_reviews"][1]["artifact_ref"] =
                    json!("./superpowers/run-reports/2026-06-28-other-explainer.html");
            }),
        ),
        (
            "incomplete",
            Box::new(|b: &mut Value| {
                run_explainer_reviews(b);
                b["run_explainer_reviews"][1]["status"] = json!("pending");
            }),
        ),
        (
            "unapproved",
            Box::new(|b: &mut Value| {
                run_explainer_reviews(b);
                b["run_explainer_reviews"][1]["result"] = json!("rejected");
            }),
        ),
        (
            "blank summary",
            Box::new(|b: &mut Value| {
                run_explainer_reviews(b);
                b["run_explainer_reviews"][1]["summary"] = json!("   ");
            }),
        ),
        (
            "empty evidence",
            Box::new(|b: &mut Value| {
                run_explainer_reviews(b);
                b["run_explainer_reviews"][1]["evidence_refs"] = json!([]);
            }),
        ),
    ];
    for (name, mutate) in variants {
        let d = tmp();
        let (b, n) = paths(&d);
        make_baton_v2(&b, "termination_review", "team", 4, |b| {
            b["active_roles"] = json!(["vadi", "prativadi"]);
            b["vadi_final_approval"] = json!(true);
            b["prativadi_final_approval"] = json!(true);
        });
        make_baton_v2(&n, "done", "team", 5, |b| {
            b["run_explainer_ref"] =
                json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
            b["vadi_final_approval"] = json!(true);
            b["prativadi_final_approval"] = json!(true);
            mutate(b);
        });
        run(&b, &n).assert_contains(name, 23, "DVANDVA_WRITE bad_run_explainer_reviews");
    }
}

// ---- research / review done evidence ----

#[test]
fn research_done_seed_development_with_plan_ref_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path()); // S4-T1
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!("spec");
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!("spec");
        b["research_outcome"] = json!("seed_development");
        b["plan_ref"] = json!("./superpowers/plans/2026-06-29-run-modes-plan.html");
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert("research done seed_development", 0);
}

#[test]
fn research_done_exploratory_needs_only_research_ref() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path()); // S4-T1
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!("spec");
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["mode"] = json!("research");
        // S5-T5: exploratory done carries phase "research".
        b["phase"] = json!("research");
        b["research_outcome"] = json!("exploratory");
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert("research done exploratory", 0);
}

#[test]
fn research_done_seed_development_missing_plan_ref_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!("spec");
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!("spec");
        b["research_outcome"] = json!("seed_development");
        b["plan_ref"] = Value::Null;
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "research done no plan_ref",
        23,
        "DVANDVA_WRITE bad_research_done_ref",
    );
}

#[test]
fn review_done_with_review_ref_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path()); // S4-T1
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!("review");
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!("review");
        b["review_ref"] = json!("./superpowers/reviews/review-run-modes-PR-1.html");
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert("review done review_ref", 0);
}

#[test]
fn review_done_bad_review_refs_rejected() {
    for bad in [
        "../escape.html",
        "https://example.com/review.html",
        "./superpowers/review/review.html",
        "./superpowers/reviews/review.md",
        "./superpowers/reviews/../escape.html",
        "./superpowers/reviews//review.html",
    ] {
        let d = tmp();
        let (b, n) = paths(&d);
        make_baton_v2(&b, "termination_review", "team", 4, |b| {
            b["mode"] = json!("review");
            b["phase"] = json!("review");
            b["active_roles"] = json!(["vadi", "prativadi"]);
            b["vadi_final_approval"] = json!(true);
            b["prativadi_final_approval"] = json!(true);
        });
        make_baton_v2(&n, "done", "human", 5, |b| {
            b["mode"] = json!("review");
            b["phase"] = json!("review");
            b["review_ref"] = json!(bad);
            b["vadi_final_approval"] = json!(true);
            b["prativadi_final_approval"] = json!(true);
        });
        run(&b, &n).assert_contains(
            &format!("bad review_ref {bad}"),
            23,
            "DVANDVA_WRITE bad_review_ref",
        );
    }
}

#[test]
fn review_done_missing_review_ref_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!("review");
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!("review");
        b["review_ref"] = Value::Null;
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains("review done no ref", 23, "DVANDVA_WRITE bad_review_ref");
}

// ---- v1 done ----
// S5-T2 (D5): the v1 done tests (`v1_done_accepts_coordinator_assignees`,
// `v1_done_requires_both_final_approvals`, `v1_done_rejects_generated_assignee`)
// were REMOVED — v1 is retired from the write path, and their behaviour is
// covered by the v2 equivalents `v2_done_accepts_coordinator_assignees`,
// `v2_done_rejects_missing_final_approval`, and `v2_done_rejects_generated_assignee`.
// Retirement itself is probed by `s5t2_*` in write_transitions.rs.

// ===================== schema backcompat =====================

#[test]
fn v2_backcompat_scaffold_missing_optional_fields() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "clarifying_questions_drafting", "vadi", 0, |b| {
        b["phase"] = json!("clarifying");
        let o = b.as_object_mut().unwrap();
        o.remove("research_outcome");
        o.remove("review_ref");
        o.remove("review_intake");
    });
    run(&b, &n).assert("backcompat scaffold", 0);
}

#[test]
fn v2_backcompat_transition_missing_optional_fields() {
    let d = tmp();
    let (b, n) = paths(&d);
    let strip = |b: &mut Value| {
        let o = b.as_object_mut().unwrap();
        o.remove("research_outcome");
        o.remove("review_ref");
        o.remove("review_intake");
    };
    make_baton_v2(&b, "research_review", "prativadi", 4, strip);
    make_baton_v2(&n, "research_revision", "vadi", 5, strip);
    run(&b, &n).assert("backcompat transition", 0);
}

// ===================== F7: amendment_from_phase schema =====================

/// amendment_from_phase is additive and NOT a required key: a transition with it
/// stripped entirely still validates.
#[test]
fn f7_amendment_from_phase_absent_is_null() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |v| {
        v.as_object_mut().unwrap().remove("amendment_from_phase");
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |v| {
        v.as_object_mut().unwrap().remove("amendment_from_phase");
    });
    run(&b, &n).assert("f7 amendment_from_phase absent is null", 0);
}

/// A non-numeric amendment_from_phase fails the shape check.
#[test]
fn f7_amendment_from_phase_bad_shape_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |v| {
        v["amendment_from_phase"] = json!("foo");
    });
    run(&b, &n).assert_contains(
        "f7 amendment_from_phase bad shape rejected",
        23,
        "bad_amendment",
    );
}

// ===================== usage =====================

#[test]
fn usage_error_without_args_exits_2() {
    let out = write_output(&[]);
    assert_eq!(out.status.code(), Some(2));
}

fn write_output(args: &[&str]) -> std::process::Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_dvandva"));
    cmd.arg("write");
    for a in args {
        cmd.arg(a);
    }
    cmd.output().unwrap()
}

// ===================== F9: phase_profiles shape =====================

/// F9: phase_profiles values are restricted to "standard"|"full".
#[test]
fn f9_phase_profiles_bad_value_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton_v2(&n, "spec_review", "prativadi", 5, |v| {
        v["phase_profiles"] = json!({"1": "fast"});
    });
    run(&b, &n).assert_contains("f9 phase_profiles bad value", 23, "bad_phase_profiles");
}

/// F9: a non-object phase_profiles is rejected.
#[test]
fn f9_phase_profiles_non_object_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton_v2(&n, "spec_review", "prativadi", 5, |v| {
        v["phase_profiles"] = json!(["1"]);
    });
    run(&b, &n).assert_contains("f9 phase_profiles non-object", 23, "bad_phase_profiles");
}

/// F9: phase_profiles keys must be stringified numeric phases.
#[test]
fn f9_phase_profiles_non_numeric_key_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton_v2(&n, "spec_review", "prativadi", 5, |v| {
        v["phase_profiles"] = json!({"one": "standard"});
    });
    run(&b, &n).assert_contains(
        "f9 phase_profiles non-numeric key",
        23,
        "bad_phase_profiles",
    );
}

/// F9: an absent/null phase_profiles is accepted (additive, old batons unchanged).
#[test]
fn f9_phase_profiles_absent_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton_v2(&n, "spec_review", "prativadi", 5, |v| {
        v["phase_profiles"] = Value::Null;
    });
    run(&b, &n).assert("f9 phase_profiles null accepted", 0);
}

// ===========================================================================
// S2-T1: `abandoned` owner / active_roles / phase shape (v2 only).
// ===========================================================================

#[test]
fn s2t1_abandoned_owner_must_be_human() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "human_question", "human", 4, |v| {
        v["phase"] = json!(1);
        v["question"] = json!("Continue?");
        v["resume_assignee"] = json!("vadi");
        v["resume_status"] = json!("implementing");
    });
    make_baton_v2(&n, "abandoned", "vadi", 5, |v| {
        v["phase"] = json!(1);
    });
    run(&b, &n).assert_contains(
        "s2t1 abandoned candidate must be human-owned",
        23,
        "bad_assignee_owner",
    );
}

#[test]
fn s2t1_abandoned_rejects_nonempty_active_roles() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "human_decision", "human", 4, |v| {
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "abandoned", "human", 5, |v| {
        v["phase"] = json!(1);
        v["active_roles"] = json!(["vadi"]);
    });
    run(&b, &n).assert_contains(
        "s2t1 abandoned carries no active_roles",
        23,
        "bad_active_roles",
    );
}

#[test]
fn s2t1_abandoned_accepts_the_carried_human_phase() {
    // The human state carried a numeric phase; abandoned must accept it unchanged.
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "human_question", "human", 4, |v| {
        v["phase"] = json!(2);
        v["question"] = json!("Stop the run?");
        v["resume_assignee"] = json!("vadi");
        v["resume_status"] = json!("implementing");
    });
    make_baton_v2(&n, "abandoned", "human", 5, |v| {
        v["phase"] = json!(2);
    });
    run(&b, &n).assert("s2t1 abandoned accepts the carried numeric phase", 0);
}

// ===========================================================================
// P1: clarifying-questions phase (mandatory pre-research gate)
// ===========================================================================

/// `n` `clarifying_questions` entries for `round`, authored by `asked_by`;
/// `answered` controls whether each entry already carries a non-empty answer.
fn cq_entries(round: i64, n: usize, answered: bool, asked_by: &str) -> Vec<Value> {
    (0..n)
        .map(|i| {
            json!({
                "round": round,
                "asked_by": asked_by,
                "question": format!("Clarifying question r{round}.{i}?"),
                "answer": if answered {
                    json!(format!("Answer r{round}.{i}"))
                } else {
                    Value::Null
                }
            })
        })
        .collect()
}

/// Spawn `dvandva next --file <baton>`, clearing the selector env vars so the
/// resolved baton is fully controlled by the fixture. Returns stdout.
fn run_next_list(baton: &std::path::Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("next")
        .arg("--file")
        .arg(baton)
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_BATON_FILE")
        .env_remove("DVANDVA_RUN_DIR")
        .env_remove("DVANDVA_RUN_ID")
        .output()
        .expect("spawn dvandva next");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A minimal current-baton JSON carrying only the fields `legal_transitions`
/// consults (schema/status/mode/profile/phase/checkpoint/master_plan_locked).
fn minimal_current_baton(mode: &str, profile: Option<&str>, status: &str, phase: &str) -> Value {
    let mut b = json!({
        "schema": "dvandva.baton.v2",
        "status": status,
        "mode": mode,
        "phase": phase,
        "checkpoint": 4,
        "master_plan_locked": false,
    });
    if let Some(p) = profile {
        b["profile"] = json!(p);
    }
    b
}

fn write_value(path: &std::path::Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

// ----- Task 1.3: research_ref exemption ------------------------------------

#[test]
fn v2_clarifying_questions_drafting_research_ref_null_accepted() {
    // clarifying_questions_drafting runs before research exists, so a null
    // research_ref must not trip bad_research_ref (unlike every other status).
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "clarifying_questions_drafting", "vadi", 0, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
    });
    run(&b, &n).assert("clarifying_questions_drafting exempt from research_ref", 0);
}

// ----- Task 1.4: phase mapping ----------------------------------------------

#[test]
fn v2_clarifying_questions_scaffold_wrong_phase_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "clarifying_questions_drafting", "vadi", 0, |b| {
        b["phase"] = json!("research");
    });
    run(&b, &n).assert_contains(
        "clarifying_questions_drafting requires phase=clarifying, not research",
        23,
        "DVANDVA_WRITE bad_phase_status",
    );
}

#[test]
fn v2_clarifying_questions_research_mode_phase_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "clarifying_questions_drafting", "vadi", 4, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
    });
    make_baton_v2(&n, "clarifying_questions_answer", "human", 5, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 1, false, "vadi"));
    });
    run(&b, &n).assert("research-mode clarifying phase is accepted", 0);
}

#[test]
fn v2_clarifying_questions_research_mode_phase_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "clarifying_questions_drafting", "vadi", 4, |b| {
        b["mode"] = json!("research");
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
    });
    make_baton_v2(&n, "clarifying_questions_answer", "human", 5, |b| {
        b["mode"] = json!("research");
        // Pre-P1, research mode's catch-all would have demanded "spec" here.
        b["phase"] = json!("spec");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 1, false, "vadi"));
    });
    run(&b, &n).assert_contains(
        "research-mode clarifying_questions_answer requires phase=clarifying, not spec",
        23,
        "DVANDVA_WRITE bad_phase_status",
    );
}

#[test]
fn v2_clarifying_questions_review_mode_phase_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "clarifying_questions_drafting", "vadi", 4, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
    });
    make_baton_v2(&n, "clarifying_questions_answer", "human", 5, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 1, false, "vadi"));
    });
    run(&b, &n).assert("review-mode clarifying phase is accepted", 0);
}

#[test]
fn v2_clarifying_questions_review_mode_phase_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "clarifying_questions_drafting", "vadi", 4, |b| {
        b["mode"] = json!("review");
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
    });
    make_baton_v2(&n, "clarifying_questions_answer", "human", 5, |b| {
        b["mode"] = json!("review");
        // Pre-P1, review mode's catch-all would have demanded "review" here.
        b["phase"] = json!("review");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 1, false, "vadi"));
    });
    run(&b, &n).assert_contains(
        "review-mode clarifying_questions_answer requires phase=clarifying, not review",
        23,
        "DVANDVA_WRITE bad_phase_status",
    );
}

// ----- Task 1.5: per-state non-null gates -----------------------------------

#[test]
fn v2_clarifying_questions_round1_zero_entries_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "clarifying_questions_drafting", "vadi", 4, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
    });
    make_baton_v2(&n, "clarifying_questions_answer", "human", 5, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!([]);
    });
    run(&b, &n).assert_contains(
        "zero round-1 questions is rejected",
        23,
        "DVANDVA_WRITE bad_clarifying_questions_round1",
    );
}

#[test]
fn v2_clarifying_questions_round1_unanswered_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "clarifying_questions_answer", "human", 4, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 3, false, "vadi"));
    });
    make_baton_v2(&n, "clarifying_questions_followup", "prativadi", 5, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        // Round 1 questions asked but still unanswered -- must not skip ahead.
        b["clarifying_questions"] = json!(cq_entries(1, 3, false, "vadi"));
    });
    run(&b, &n).assert_contains(
        "unanswered round-1 questions is rejected",
        23,
        "DVANDVA_WRITE bad_clarifying_questions_round1_answer",
    );
}

#[test]
fn v2_clarifying_questions_round2_zero_entries_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "clarifying_questions_followup", "prativadi", 4, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 3, true, "vadi"));
    });
    make_baton_v2(
        &n,
        "clarifying_questions_followup_answer",
        "human",
        5,
        |b| {
            b["phase"] = json!("clarifying");
            b["research_ref"] = Value::Null;
            // No prativadi (round-2) contribution at all.
            b["clarifying_questions"] = json!(cq_entries(1, 3, true, "vadi"));
        },
    );
    run(&b, &n).assert_contains(
        "zero round-2 questions (no prativadi contribution) is rejected",
        23,
        "DVANDVA_WRITE bad_clarifying_questions_round2",
    );
}

#[test]
fn v2_clarifying_questions_combined_below_five_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "clarifying_questions_followup", "prativadi", 4, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 3, true, "vadi"));
    });
    make_baton_v2(
        &n,
        "clarifying_questions_followup_answer",
        "human",
        5,
        |b| {
            b["phase"] = json!("clarifying");
            b["research_ref"] = Value::Null;
            // 3 round-1 + 1 round-2 = 4 combined, below the >=5 floor.
            let mut qs = cq_entries(1, 3, true, "vadi");
            qs.extend(cq_entries(2, 1, false, "prativadi"));
            b["clarifying_questions"] = json!(qs);
        },
    );
    run(&b, &n).assert_contains(
        "4 combined questions is below the >=5 floor",
        23,
        "DVANDVA_WRITE bad_clarifying_questions_round2",
    );
}

#[test]
fn v2_clarifying_questions_round2_unanswered_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(
        &b,
        "clarifying_questions_followup_answer",
        "human",
        4,
        |b| {
            b["phase"] = json!("clarifying");
            b["research_ref"] = Value::Null;
            let mut qs = cq_entries(1, 3, true, "vadi");
            qs.extend(cq_entries(2, 2, false, "prativadi"));
            b["clarifying_questions"] = json!(qs);
        },
    );
    make_baton_v2(&n, "research_drafting", "vadi", 5, |b| {
        b["phase"] = json!("research");
        b["research_ref"] = json!("./superpowers/research/run-a.html");
        // Round 2 still unanswered -- must not hand off to research_drafting.
        let mut qs = cq_entries(1, 3, true, "vadi");
        qs.extend(cq_entries(2, 2, false, "prativadi"));
        b["clarifying_questions"] = json!(qs);
    });
    run(&b, &n).assert_contains(
        "unanswered round-2 questions is rejected",
        23,
        "DVANDVA_WRITE bad_clarifying_questions_round2_answer",
    );
}

#[test]
fn v2_clarifying_questions_round1_wrong_asked_by_rejected() {
    // Round-1 questions must be asked_by vadi -- prativadi asking in round 1
    // must not satisfy the drafting-round gate, even though question/answer
    // shape is otherwise valid.
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "clarifying_questions_drafting", "vadi", 0, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
    });
    make_baton_v2(&n, "clarifying_questions_answer", "human", 1, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 3, false, "prativadi"));
    });
    run(&b, &n).assert_contains(
        "round-1 questions asked by prativadi is rejected",
        23,
        "DVANDVA_WRITE bad_clarifying_questions_round1",
    );
}

#[test]
fn v2_clarifying_questions_round2_wrong_asked_by_rejected() {
    // Round-2 questions must be asked_by prativadi -- vadi asking in round 2
    // must not satisfy the followup gate, even with a combined total >=5.
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "clarifying_questions_followup", "prativadi", 2, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 3, true, "vadi"));
    });
    make_baton_v2(
        &n,
        "clarifying_questions_followup_answer",
        "human",
        3,
        |b| {
            b["phase"] = json!("clarifying");
            b["research_ref"] = Value::Null;
            let mut qs = cq_entries(1, 3, true, "vadi");
            qs.extend(cq_entries(2, 2, false, "vadi"));
            b["clarifying_questions"] = json!(qs);
        },
    );
    run(&b, &n).assert_contains(
        "round-2 questions asked by vadi is rejected",
        23,
        "DVANDVA_WRITE bad_clarifying_questions_round2",
    );
}

#[test]
fn v2_clarifying_questions_full_sequence_accepted() {
    // The valid 3 round-1 + 2 round-2 (5 combined, >=1 per role) sequence
    // reaches research_drafting end to end.
    let d = tmp();
    let (b0, c1) = paths(&d);

    make_baton_v2(&c1, "clarifying_questions_drafting", "vadi", 0, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
    });
    run(&b0, &c1).assert("scaffold clarifying_questions_drafting", 0);

    let c2 = d.path().join("c2.json");
    make_baton_v2(&c2, "clarifying_questions_answer", "human", 1, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 3, false, "vadi"));
    });
    run(&b0, &c2).assert("clarifying_questions_drafting -> answer", 0);

    let c3 = d.path().join("c3.json");
    make_baton_v2(&c3, "clarifying_questions_followup", "prativadi", 2, |b| {
        b["phase"] = json!("clarifying");
        b["research_ref"] = Value::Null;
        b["clarifying_questions"] = json!(cq_entries(1, 3, true, "vadi"));
    });
    run(&b0, &c3).assert("clarifying_questions_answer -> followup", 0);

    let c4 = d.path().join("c4.json");
    make_baton_v2(
        &c4,
        "clarifying_questions_followup_answer",
        "human",
        3,
        |b| {
            b["phase"] = json!("clarifying");
            b["research_ref"] = Value::Null;
            let mut qs = cq_entries(1, 3, true, "vadi");
            qs.extend(cq_entries(2, 2, false, "prativadi"));
            b["clarifying_questions"] = json!(qs);
        },
    );
    run(&b0, &c4).assert("clarifying_questions_followup -> followup_answer", 0);

    let c5 = d.path().join("c5.json");
    make_baton_v2(&c5, "research_drafting", "vadi", 4, |b| {
        b["phase"] = json!("research");
        b["research_ref"] = json!("./superpowers/research/run-a.html");
        let mut qs = cq_entries(1, 3, true, "vadi");
        qs.extend(cq_entries(2, 2, true, "prativadi"));
        b["clarifying_questions"] = json!(qs);
    });
    run(&b0, &c5).assert(
        "clarifying_questions_followup_answer -> research_drafting",
        0,
    );

    let installed: Value = serde_json::from_slice(&std::fs::read(&b0).unwrap()).unwrap();
    assert_eq!(installed["status"], "research_drafting");
    assert_eq!(
        installed["clarifying_questions"].as_array().unwrap().len(),
        5
    );
}

// ----- Task 1.6: edge whitelist across every mode/profile arm --------------

#[test]
fn v2_clarifying_questions_edges_legal_development_fast() {
    assert_clarifying_edges_legal("development", Some("fast"));
}

#[test]
fn v2_clarifying_questions_edges_legal_development_standard() {
    assert_clarifying_edges_legal("development", Some("standard"));
}

#[test]
fn v2_clarifying_questions_edges_legal_development_full() {
    assert_clarifying_edges_legal("development", Some("full"));
}

#[test]
fn v2_clarifying_questions_edges_legal_research_mode() {
    assert_clarifying_edges_legal("research", None);
}

#[test]
fn v2_clarifying_questions_edges_legal_review_mode() {
    assert_clarifying_edges_legal("review", None);
}

/// For a given (mode, profile) arm, assert `dvandva next` LIST surfaces all 4
/// new clarifying-questions edges from their respective source statuses.
fn assert_clarifying_edges_legal(mode: &str, profile: Option<&str>) {
    let d = tmp();
    let hops: [(&str, &str); 4] = [
        (
            "clarifying_questions_drafting",
            "clarifying_questions_answer",
        ),
        (
            "clarifying_questions_answer",
            "clarifying_questions_followup",
        ),
        (
            "clarifying_questions_followup",
            "clarifying_questions_followup_answer",
        ),
        ("clarifying_questions_followup_answer", "research_drafting"),
    ];
    for (from, to) in hops {
        let baton = d.path().join(format!("{from}.json"));
        let current = minimal_current_baton(mode, profile, from, "clarifying");
        write_value(&baton, &current);
        let out = run_next_list(&baton);
        assert!(
            out.lines()
                .any(|l| l.starts_with(&format!("DVANDVA_NEXT {to} "))),
            "mode={mode} profile={profile:?}: expected {from}->{to} to be legal, got:\n{out}"
        );
    }
}
