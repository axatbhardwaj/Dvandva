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

// ===========================================================================
// 7. Residual sweep (P2 phase-2 coverage): amendment writer-identity peer
// rule, declared-graph membership of an amendment's resume_status, and
// double-pending amendments.
// ===========================================================================

/// Sweep item 3: the peer rule must hold for amendments too — a shape-valid
/// amendment stamp (`approved_by` != `proposed_by`) is not enough; the
/// *actual writer* (`DVANDVA_ROLE`) must match the stamped `approved_by`, or
/// the proposer could impersonate the peer's approval. Here `vadi` (the
/// proposer) writes the resume transition itself, stamping `approved_by:
/// "prativadi"` without prativadi ever running the command; `amendment_
/// resume_ok`'s `newly_approved` check requires `approved_by == writer_role`,
/// so this falls through to the plain edge whitelist, which has no
/// `workflow_review->parallel_implementing` edge outside the amendment path.
#[test]
fn amendment_resume_rejected_when_writer_is_not_the_stamped_approver() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 5, |b| {
        b["phase"] = json!("spec");
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["run_workflow"]["amendments"] = json!([pending_amendment("parallel_implementing")]);
    });
    make_baton_v3(&n, "parallel_implementing", "team", 6, |b| {
        locked_numeric(b, 1);
        b["run_workflow"]["amendments"] = json!([{
            "proposed_by": "vadi",
            "at_checkpoint": 5,
            "resume_status": "parallel_implementing",
            "approved_by": "prativadi",
            "approved_at_checkpoint": 6
        }]);
    });
    // The writer is vadi (the proposer) impersonating prativadi's approval.
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "amendment resume writer must match the stamped approver",
        24,
        "illegal_transition",
    );
}

/// tc-p2-double-pending-amendment / P2 deep_review: `amendments[]` must be
/// stable across the reject (`workflow_review -> workflow_revision`) and
/// revise (`workflow_revision -> workflow_review`) edges — no new pending
/// entries may be appended (closing the double-pending gap), and the current
/// pending entry's bookkeeping (`proposed_by`, `at_checkpoint`,
/// `resume_status`, and the still-null `approved_by`/`approved_at_checkpoint`)
/// may not be mutated. This flips the formerly-pinned permissive test: a
/// second pending entry appended on the reject edge is now rejected.
#[test]
fn workflow_revision_reject_rejects_a_second_pending_amendment() {
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
        b["findings"] =
            json!(["second objection raised while the first amendment is still pending"]);
        b["loop_counts"] = json!({"workflow_revision": 1});
        b["run_workflow"]["amendments"] = json!([
            pending_amendment("parallel_implementing"),
            pending_amendment("test_creation")
        ]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")]).assert_contains(
        "double-pending amendment on reject edge is now rejected",
        24,
        "amendments immutable during reject/revise",
    );
}

/// Rule 2: the reject edge may not mutate the pending entry's `resume_status`
/// (only the entry's bookkeeping the peer/writer control — `reason`, see
/// below — is a legitimate revision surface).
#[test]
fn workflow_review_reject_rejected_when_pending_amendment_resume_status_mutated() {
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
        b["findings"] = json!(["mutating the pending amendment's resume target"]);
        b["loop_counts"] = json!({"workflow_revision": 1});
        // resume_status mutated from "parallel_implementing" to "test_creation".
        b["run_workflow"]["amendments"] = json!([pending_amendment("test_creation")]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")]).assert_contains(
        "reject edge must not mutate pending amendment resume_status",
        24,
        "amendments immutable during reject/revise",
    );
}

/// Rule 2, revise edge: same immutability, mirrored on `workflow_revision ->
/// workflow_review` — the resubmission may not mutate the pending entry's
/// `resume_status` either.
#[test]
fn workflow_revision_revise_rejected_when_pending_amendment_resume_status_mutated() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_revision", "vadi", 5, |b| {
        b["phase"] = json!("spec");
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["loop_counts"] = json!({"workflow_revision": 1});
        b["run_workflow"]["amendments"] = json!([pending_amendment("parallel_implementing")]);
    });
    make_baton_v3(&n, "workflow_review", "prativadi", 6, |b| {
        b["phase"] = json!("spec");
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["loop_counts"] = json!({"workflow_revision": 1});
        // resume_status mutated from "parallel_implementing" to "test_creation".
        b["run_workflow"]["amendments"] = json!([pending_amendment("test_creation")]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "revise edge must not mutate pending amendment resume_status",
        24,
        "amendments immutable during reject/revise",
    );
}

/// Rule 2's carve-out: an amendment entry's `reason` field (free-form, not
/// part of the bookkeeping the immutability rule pins) MAY change on the
/// revise edge — that is the legitimate revision surface for amendment
/// content itself, distinct from the bookkeeping fields above.
#[test]
fn workflow_revision_revise_accepted_when_only_amendment_reason_changes() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_revision", "vadi", 5, |b| {
        b["phase"] = json!("spec");
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["loop_counts"] = json!({"workflow_revision": 1});
        let mut a = pending_amendment("parallel_implementing");
        a["reason"] = json!("original reason");
        b["run_workflow"]["amendments"] = json!([a]);
    });
    make_baton_v3(&n, "workflow_review", "prativadi", 6, |b| {
        b["phase"] = json!("spec");
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["loop_counts"] = json!({"workflow_revision": 1});
        let mut a = pending_amendment("parallel_implementing");
        a["reason"] = json!("revised reason after addressing findings");
        b["run_workflow"]["amendments"] = json!([a]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")])
        .assert("revise edge changing only amendment reason", 0);
}

/// Sweep item 6, FINDING: an amendment's `resume_status` is only checked
/// against the *global* `V3_STATUS_CATALOG` (shape level) — never against
/// the run's own declared custom-graph `states[]`. `amendment_resume_ok`
/// grants legality from the amendment bookkeeping + `custom_invariants_ok`
/// alone, without checking that `cur_status->new_status` is even an edge (or
/// that `new_status` is a declared state) of the graph. Here the declared
/// graph (`invariant_states`/`invariant_edges`) never mentions
/// `test_creation`, yet an amendment resuming into it is accepted.
/// FINDING for deep_review/phase_fixing: amendment resume should re-check
/// declared-graph membership (state + edge), not just global catalog
/// membership + whole-graph invariants.
#[test]
fn amendment_resume_rejected_when_status_outside_declared_graph_states() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 5, |b| {
        b["phase"] = json!("spec");
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        set_custom_workflow(b, invariant_edges());
        b["run_workflow"]["amendments"] = json!([pending_amendment("test_creation")]);
    });
    make_baton_v3(&n, "test_creation", "team", 6, |b| {
        b["phase"] = json!(1);
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        set_custom_workflow(b, invariant_edges());
        b["run_workflow"]["amendments"] = json!([approved_amendment("test_creation", 6)]);
    });
    // "test_creation" is not a state in `invariant_states()` at all — the
    // declared graph never mentions it — so the resume must be rejected.
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")]).assert_contains(
        "amendment resume target must be a declared-graph state",
        24,
        "illegal_transition",
    );
}
