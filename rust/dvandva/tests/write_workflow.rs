//! `dvandva write` -- transition coverage for v3 declared workflows.
//!
//! This suite covers declaration-loop seams that are deliberately outside the
//! p2-declaration happy-path file: scalar-source amendments, amendment reject /
//! revise / approve, custom graph invariants during amendment resume, and
//! custom `loop_cap_key` enforcement.

mod common;

use common::*;
use serde_json::{json, Value};
use std::path::PathBuf;

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn paths(dir: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    (
        dir.path().join("baton.json"),
        dir.path().join("baton.next.json"),
    )
}

fn locked_numeric(b: &mut Value, status_phase: i64) {
    parallel_chunks(b);
    b["phase"] = json!(status_phase);
    b["total_phases"] = json!(status_phase);
    b["master_plan_locked"] = json!(true);
}

fn workflow_review_amendment_entry(b: &mut Value) {
    parallel_chunks(b);
    b["active_roles"] = json!([]);
    b["phase"] = json!("spec");
    b["total_phases"] = json!(1);
    b["master_plan_locked"] = json!(true);
}

fn pending_amendment(resume_status: &str) -> Value {
    json!({
        "proposed_by": "vadi",
        "at_checkpoint": 5,
        "resume_status": resume_status,
        "approved_by": null,
        "approved_at_checkpoint": null
    })
}

fn approved_amendment(resume_status: &str, checkpoint: i64) -> Value {
    json!({
        "proposed_by": "vadi",
        "at_checkpoint": 5,
        "resume_status": resume_status,
        "approved_by": "prativadi",
        "approved_at_checkpoint": checkpoint
    })
}

fn invariant_states() -> Value {
    json!([
        {"name": "parallel_implementing", "owner": "team", "class": "work"},
        {"name": "deep_review", "owner": "prativadi", "class": "review_gate"},
        {"name": "phase_fixing", "owner": "vadi", "class": "work"},
        {"name": "human_question", "owner": "human", "class": "pause"},
        {"name": "human_decision", "owner": "human", "class": "pause"},
        {"name": "abandoned", "owner": "human", "class": "terminal"},
        {"name": "done", "owner": "team", "class": "terminal"}
    ])
}

fn invariant_edges() -> Value {
    json!([
        {"from": "parallel_implementing", "to": "deep_review"},
        {"from": "deep_review", "to": "phase_fixing"},
        {"from": "phase_fixing", "to": "deep_review"},
        {"from": "deep_review", "to": "done"},
        {"from": "deep_review", "to": "human_question"},
        {"from": "deep_review", "to": "human_decision"},
        {"from": "human_question", "to": "human_decision"},
        {"from": "human_question", "to": "abandoned"},
        {"from": "human_decision", "to": "human_question"},
        {"from": "human_decision", "to": "abandoned"}
    ])
}

fn set_custom_workflow(b: &mut Value, edges: Value) {
    b["run_workflow"]["source"] = json!("custom");
    b["run_workflow"]["states"] = invariant_states();
    b["run_workflow"]["edges"] = edges;
    b["run_workflow"]["declared_by"] = json!("vadi");
    b["run_workflow"]["declared_at_checkpoint"] = json!(1);
    b["run_workflow"]["approved_by"] = json!("prativadi");
    b["run_workflow"]["approved_at_checkpoint"] = json!(2);
}

fn set_custom_loop_workflow(b: &mut Value) {
    b["run_workflow"]["source"] = json!("custom");
    b["run_workflow"]["states"] = json!([
        {"name": "deep_review", "owner": "prativadi", "class": "review_gate"},
        {"name": "phase_fixing", "owner": "vadi", "class": "work"},
        {"name": "done", "owner": "team", "class": "terminal"}
    ]);
    b["run_workflow"]["edges"] = json!([
        {"from": "deep_review", "to": "phase_fixing", "loop_cap_key": "custom_fix_loop"},
        {"from": "phase_fixing", "to": "deep_review"},
        {"from": "deep_review", "to": "done"}
    ]);
    b["run_workflow"]["declared_by"] = json!("vadi");
    b["run_workflow"]["declared_at_checkpoint"] = json!(1);
    b["run_workflow"]["approved_by"] = json!("prativadi");
    b["run_workflow"]["approved_at_checkpoint"] = json!(2);
}

#[test]
fn scalar_source_status_can_raise_workflow_amendment() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "phase_fixing", "vadi", 4, |b| locked_numeric(b, 1));
    make_baton_v3(&n, "workflow_review", "prativadi", 5, |b| {
        workflow_review_amendment_entry(b);
        b["run_workflow"]["amendments"] = json!([pending_amendment("phase_fixing")]);
    });

    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert("scalar source amendment entry", 0);
}

#[test]
fn pending_amendment_can_be_rejected_revised_and_approved() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 5, |b| {
        b["phase"] = json!("spec");
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["run_workflow"]["amendments"] = json!([pending_amendment("parallel_implementing")]);
    });
    make_baton_v3(&n, "workflow_revision", "vadi", 6, |b| {
        b["phase"] = json!("spec");
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["findings"] = json!(["The amendment removes the review-gate cut."]);
        b["loop_counts"] = json!({"workflow_revision": 1});
        b["run_workflow"]["amendments"] = json!([pending_amendment("parallel_implementing")]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")]).assert("reject pending amendment", 0);

    make_baton_v3(&n, "workflow_review", "prativadi", 7, |b| {
        b["phase"] = json!("spec");
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["loop_counts"] = json!({"workflow_revision": 1});
        b["run_workflow"]["amendments"] = json!([pending_amendment("parallel_implementing")]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert("revise amendment", 0);

    make_baton_v3(&n, "parallel_implementing", "team", 8, |b| {
        locked_numeric(b, 1);
        b["loop_counts"] = json!({});
        b["run_workflow"]["amendments"] = json!([approved_amendment("parallel_implementing", 8)]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")]).assert("approve revised amendment", 0);
}

#[test]
fn custom_amendment_resume_reports_invariant_violations() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 5, |b| {
        b["phase"] = json!("spec");
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        set_custom_workflow(b, invariant_edges());
        b["run_workflow"]["amendments"] = json!([pending_amendment("parallel_implementing")]);
    });

    let mut bad_edges = invariant_edges();
    bad_edges.as_array_mut().unwrap().push(json!({
        "from": "parallel_implementing",
        "to": "done"
    }));
    make_baton_v3(&n, "parallel_implementing", "team", 6, |b| {
        locked_numeric(b, 1);
        set_custom_workflow(b, bad_edges);
        b["run_workflow"]["amendments"] = json!([approved_amendment("parallel_implementing", 6)]);
    });

    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")]).assert_contains(
        "custom amendment resume invariant violation",
        23,
        "bad_workflow_invariants",
    );
}

#[test]
fn custom_loop_cap_key_controls_declared_edge_increment() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "deep_review", "prativadi", 4, |b| {
        b["phase"] = json!(1);
        b["master_plan_locked"] = json!(true);
        set_custom_loop_workflow(b);
        b["loop_counts"] = json!({"custom_fix_loop": 0});
    });
    make_baton_v3(&n, "phase_fixing", "vadi", 5, |b| {
        b["phase"] = json!(1);
        b["master_plan_locked"] = json!(true);
        set_custom_loop_workflow(b);
        b["loop_counts"] = json!({"custom_fix_loop": 1});
    });

    run(&b, &n).assert("custom loop_cap_key increment", 0);
}

#[test]
fn custom_loop_cap_key_blocks_at_declared_cap() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "deep_review", "prativadi", 4, |b| {
        b["phase"] = json!(1);
        b["master_plan_locked"] = json!(true);
        set_custom_loop_workflow(b);
        b["loop_counts"] = json!({"custom_fix_loop": 3});
    });
    make_baton_v3(&n, "phase_fixing", "vadi", 5, |b| {
        b["phase"] = json!(1);
        b["master_plan_locked"] = json!(true);
        set_custom_loop_workflow(b);
        b["loop_counts"] = json!({"custom_fix_loop": 4});
    });

    run(&b, &n).assert_contains(
        "custom loop_cap_key cap",
        23,
        "loop_cap edge=custom_fix_loop count=3 cap=3",
    );
}
