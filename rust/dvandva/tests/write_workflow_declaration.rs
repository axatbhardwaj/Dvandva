//! `dvandva write` — v3 per-run-workflow declaration loop, approval
//! enforcement, and mid-flight amendments (P2 `p2-declaration`).
//!
//! Covers the three universal v3 fragments layered on top of the resolved
//! transition graph:
//!   * the declaration loop
//!     `research_review -> workflow_declaring -> workflow_review -> spec_drafting`
//!     with a reject/revise sub-loop capped under `"workflow_revision"`;
//!   * approval enforcement on the `research_review -> spec_drafting` exit
//!     (custom, unapproved workflows must declare first; presets stay direct);
//!   * amendments raised from any active non-terminal status into
//!     `workflow_review` and resumed to the interrupted status.

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

/// A shape-valid custom graph that passes the P2 workflow invariants (a
/// transcription of the `revision_cycle_graph` invariants fixture): a review
/// gate cuts every route to `done`, escapes stay reachable, and no
/// non-terminal is absorbing.
fn valid_custom_states() -> Value {
    json!([
        {"name": "workflow_declaring", "owner": "vadi", "class": "work"},
        {"name": "implementing", "owner": "vadi", "class": "work"},
        {"name": "phase_fixing", "owner": "vadi", "class": "work"},
        {"name": "deep_review", "owner": "prativadi", "class": "review_gate"},
        {"name": "human_question", "owner": "human", "class": "pause"},
        {"name": "human_decision", "owner": "human", "class": "pause"},
        {"name": "abandoned", "owner": "human", "class": "terminal"},
        {"name": "done", "owner": "team", "class": "terminal"}
    ])
}

fn valid_custom_edges() -> Value {
    json!([
        {"from": "workflow_declaring", "to": "implementing"},
        {"from": "implementing", "to": "deep_review"},
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

/// Set the run's phase to the declaration phase (`"spec"`) — `make_baton_v3`
/// defaults the workflow_* statuses to the wrong label.
fn spec_phase(b: &mut Value) {
    b["phase"] = json!("spec");
}

// ===========================================================================
// 1. Declaration loop edges (Deliverable 2)
// ===========================================================================

#[test]
fn research_review_to_workflow_declaring_is_legal_when_unapproved() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v3(&n, "workflow_declaring", "vadi", 5, spec_phase);
    run(&b, &n).assert("research_review->workflow_declaring (unapproved)", 0);
}

#[test]
fn research_review_to_workflow_declaring_rejected_when_already_approved() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "research_review", "prativadi", 4, |b| {
        b["run_workflow"]["approved_by"] = json!("prativadi");
        b["run_workflow"]["approved_at_checkpoint"] = json!(2);
    });
    make_baton_v3(&n, "workflow_declaring", "vadi", 5, |b| {
        spec_phase(b);
        b["run_workflow"]["approved_by"] = json!("prativadi");
        b["run_workflow"]["approved_at_checkpoint"] = json!(2);
    });
    run(&b, &n).assert_contains(
        "already-approved run has nothing to declare",
        24,
        "only legal while run_workflow is unapproved",
    );
}

#[test]
fn workflow_declaring_to_workflow_review_is_legal_with_coherent_stamp() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_declaring", "vadi", 4, spec_phase);
    make_baton_v3(&n, "workflow_review", "prativadi", 5, spec_phase);
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")])
        .assert("workflow_declaring->workflow_review submit", 0);
}

#[test]
fn workflow_declaring_submit_rejected_when_declared_by_not_writer() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_declaring", "vadi", 4, spec_phase);
    make_baton_v3(&n, "workflow_review", "prativadi", 5, |b| {
        spec_phase(b);
        b["run_workflow"]["declared_by"] = json!("prativadi");
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "incoherent declaration stamp (declared_by != writer)",
        24,
        "requires run_workflow.declared_by=vadi",
    );
}

#[test]
fn workflow_review_to_spec_drafting_approve_stamps_prativadi() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 4, spec_phase);
    make_baton_v3(&n, "spec_drafting", "vadi", 5, |b| {
        b["run_workflow"]["approved_by"] = json!("prativadi");
        b["run_workflow"]["approved_at_checkpoint"] = json!(5);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")])
        .assert("workflow_review->spec_drafting approve", 0);
    let installed: Value = serde_json::from_slice(&std::fs::read(&b).unwrap()).unwrap();
    assert_eq!(installed["run_workflow"]["approved_by"], "prativadi");
    assert_eq!(installed["run_workflow"]["approved_at_checkpoint"], 5);
}

#[test]
fn workflow_review_approve_rejected_when_writer_not_prativadi() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 4, spec_phase);
    make_baton_v3(&n, "spec_drafting", "vadi", 5, |b| {
        b["run_workflow"]["approved_by"] = json!("prativadi");
        b["run_workflow"]["approved_at_checkpoint"] = json!(5);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "declaration approval requires DVANDVA_ROLE=prativadi",
        23,
        "bad_workflow_approval requires DVANDVA_ROLE=prativadi",
    );
}

#[test]
fn workflow_review_approve_rejected_when_stamp_checkpoint_stale() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 4, spec_phase);
    make_baton_v3(&n, "spec_drafting", "vadi", 5, |b| {
        b["run_workflow"]["approved_by"] = json!("prativadi");
        b["run_workflow"]["approved_at_checkpoint"] = json!(3); // not the current checkpoint
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")]).assert_contains(
        "approval stamp must be at the current checkpoint",
        23,
        "approved_at_checkpoint must be 5",
    );
}

// ===========================================================================
// 2. Reject / revise sub-loop, capped under "workflow_revision" (Deliverable 2/5)
// ===========================================================================

#[test]
fn workflow_review_to_workflow_revision_reject_with_findings_increments_loop() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 4, spec_phase);
    make_baton_v3(&n, "workflow_revision", "vadi", 5, |b| {
        spec_phase(b);
        b["findings"] = json!(["The declared graph omits a review gate before done."]);
        b["loop_counts"] = json!({"workflow_revision": 1});
    });
    run(&b, &n).assert("workflow_review->workflow_revision reject", 0);
}

#[test]
fn workflow_review_reject_rejected_without_findings() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 4, spec_phase);
    make_baton_v3(&n, "workflow_revision", "vadi", 5, |b| {
        spec_phase(b);
        b["loop_counts"] = json!({"workflow_revision": 1});
    });
    run(&b, &n).assert_contains("reject needs findings", 24, "requires non-empty findings");
}

#[test]
fn workflow_revision_to_workflow_review_revise_is_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_revision", "vadi", 4, spec_phase);
    make_baton_v3(&n, "workflow_review", "prativadi", 5, spec_phase);
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")])
        .assert("workflow_revision->workflow_review revise", 0);
}

#[test]
fn workflow_revision_cap_exhaustion_blocks_reject_but_allows_human_decision() {
    let d = tmp();
    // At the cap (disagreement_cap=3, count already 3) the reject edge is
    // illegal; the universal human_decision escalation remains available.
    let d2 = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 4, |b| {
        spec_phase(b);
        b["loop_counts"] = json!({"workflow_revision": 3});
    });
    make_baton_v3(&n, "workflow_revision", "vadi", 5, |b| {
        spec_phase(b);
        b["findings"] = json!(["another objection"]);
        b["loop_counts"] = json!({"workflow_revision": 4});
    });
    run(&b, &n).assert_contains(
        "cap exhausted blocks another reject",
        23,
        "loop_cap edge=workflow_revision",
    );

    let (b2, n2) = paths(&d2);
    make_baton_v3(&b2, "workflow_review", "prativadi", 4, |b| {
        spec_phase(b);
        b["loop_counts"] = json!({"workflow_revision": 3});
    });
    make_baton_v3(&n2, "human_decision", "human", 5, |b| {
        spec_phase(b);
        b["loop_counts"] = json!({"workflow_revision": 3});
    });
    run(&b2, &n2).assert("human_decision escalation after cap", 0);
}

// ===========================================================================
// 3. Approval enforcement on the research_review exit (Deliverable 3)
// ===========================================================================

#[test]
fn custom_unapproved_research_review_to_spec_drafting_is_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "research_review", "prativadi", 4, |b| {
        b["run_workflow"]["source"] = json!("custom");
        b["run_workflow"]["states"] = valid_custom_states();
        b["run_workflow"]["edges"] = valid_custom_edges();
    });
    make_baton_v3(&n, "spec_drafting", "vadi", 5, |b| {
        b["run_workflow"]["source"] = json!("custom");
        b["run_workflow"]["states"] = valid_custom_states();
        b["run_workflow"]["edges"] = valid_custom_edges();
    });
    run(&b, &n).assert_contains(
        "custom unapproved must declare first",
        24,
        "requires an approved run_workflow",
    );
}

#[test]
fn preset_unapproved_research_review_to_spec_drafting_stays_legal() {
    // Regression guard: preset sources are the engine's pre-approved workflows,
    // so the direct research_review->spec_drafting edge is untouched even when
    // approved_by is null.
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v3(&n, "spec_drafting", "vadi", 5, |_| {});
    run(&b, &n).assert("preset unapproved research_review->spec_drafting", 0);
}

// ===========================================================================
// 4. Custom-graph invariants at approval (Deliverable 2)
// ===========================================================================

#[test]
fn custom_declaration_approve_passes_when_invariants_hold() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 4, |b| {
        spec_phase(b);
        b["run_workflow"]["source"] = json!("custom");
        b["run_workflow"]["states"] = valid_custom_states();
        b["run_workflow"]["edges"] = valid_custom_edges();
    });
    make_baton_v3(&n, "spec_drafting", "vadi", 5, |b| {
        b["run_workflow"]["source"] = json!("custom");
        b["run_workflow"]["states"] = valid_custom_states();
        b["run_workflow"]["edges"] = valid_custom_edges();
        b["run_workflow"]["approved_by"] = json!("prativadi");
        b["run_workflow"]["approved_at_checkpoint"] = json!(5);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")])
        .assert("custom approve with valid invariants", 0);
}

#[test]
fn custom_declaration_approve_rejected_when_invariants_violated() {
    let d = tmp();
    let (b, n) = paths(&d);
    // A happy-path edge from the seed straight to done bypasses the review gate.
    let mut bad_edges = valid_custom_edges();
    bad_edges.as_array_mut().unwrap().push(json!({
        "from": "workflow_declaring", "to": "done"
    }));
    make_baton_v3(&b, "workflow_review", "prativadi", 4, |b| {
        spec_phase(b);
        b["run_workflow"]["source"] = json!("custom");
        b["run_workflow"]["states"] = valid_custom_states();
        b["run_workflow"]["edges"] = valid_custom_edges();
    });
    make_baton_v3(&n, "spec_drafting", "vadi", 5, move |b| {
        b["run_workflow"]["source"] = json!("custom");
        b["run_workflow"]["states"] = valid_custom_states();
        b["run_workflow"]["edges"] = bad_edges.clone();
        b["run_workflow"]["approved_by"] = json!("prativadi");
        b["run_workflow"]["approved_at_checkpoint"] = json!(5);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")]).assert_contains(
        "custom approve with review-gate bypass",
        23,
        "bad_workflow_invariants",
    );
}

// ===========================================================================
// 5. Amendments (Deliverable 4)
// ===========================================================================

/// A locked, mid-flight parallel_implementing baton to raise amendments from.
fn locked_parallel(b: &mut Value) {
    parallel_chunks(b);
    b["phase"] = json!(1);
    b["total_phases"] = json!(1);
    b["master_plan_locked"] = json!(true);
}

/// A `workflow_review` amendment-entry candidate that preserves the paused
/// team's `work_split` IDs (so the S4-T4 lost-update guard is satisfied) but
/// clears `active_roles` (workflow_review is not a team-sync status).
fn amendment_entry_baton(b: &mut Value) {
    parallel_chunks(b);
    b["active_roles"] = json!([]);
    b["phase"] = json!("spec");
    b["total_phases"] = json!(1);
    b["master_plan_locked"] = json!(true);
}

fn pending_amendment() -> Value {
    json!({
        "proposed_by": "vadi",
        "at_checkpoint": 5,
        "resume_status": "parallel_implementing",
        "approved_by": null,
        "approved_at_checkpoint": null
    })
}

#[test]
fn amendment_raised_from_parallel_implementing_enters_workflow_review() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "parallel_implementing", "team", 4, locked_parallel);
    make_baton_v3(&n, "workflow_review", "prativadi", 5, |b| {
        amendment_entry_baton(b);
        b["run_workflow"]["amendments"] = json!([pending_amendment()]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")])
        .assert("amendment entry from parallel_implementing", 0);
}

#[test]
fn amendment_entry_rejected_without_new_pending_entry() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "parallel_implementing", "team", 4, locked_parallel);
    make_baton_v3(&n, "workflow_review", "prativadi", 5, |b| {
        amendment_entry_baton(b);
        // No amendments[] entry appended.
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "amendment entry needs a new pending entry",
        24,
        "amendment entry requires a new pending amendments[] entry",
    );
}

#[test]
fn amendment_approved_resumes_to_interrupted_status() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 5, |b| {
        spec_phase(b);
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["run_workflow"]["amendments"] = json!([pending_amendment()]);
    });
    make_baton_v3(&n, "parallel_implementing", "team", 6, |b| {
        locked_parallel(b);
        b["run_workflow"]["amendments"] = json!([{
            "proposed_by": "vadi",
            "at_checkpoint": 5,
            "resume_status": "parallel_implementing",
            "approved_by": "prativadi",
            "approved_at_checkpoint": 6
        }]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")])
        .assert("amendment approve resumes to parallel_implementing", 0);
}

#[test]
fn amendment_self_approval_is_rejected_by_shape() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 5, |b| {
        spec_phase(b);
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["run_workflow"]["amendments"] = json!([pending_amendment()]);
    });
    make_baton_v3(&n, "parallel_implementing", "team", 6, |b| {
        locked_parallel(b);
        b["run_workflow"]["amendments"] = json!([{
            "proposed_by": "vadi",
            "at_checkpoint": 5,
            "resume_status": "parallel_implementing",
            "approved_by": "vadi", // self-approval
            "approved_at_checkpoint": 6
        }]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "amendment self-approval",
        23,
        "bad_run_workflow",
    );
}

#[test]
fn amendment_resume_rejected_when_stamp_checkpoint_stale() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "workflow_review", "prativadi", 5, |b| {
        spec_phase(b);
        b["total_phases"] = json!(1);
        b["master_plan_locked"] = json!(true);
        b["run_workflow"]["amendments"] = json!([pending_amendment()]);
    });
    make_baton_v3(&n, "parallel_implementing", "team", 6, |b| {
        locked_parallel(b);
        b["run_workflow"]["amendments"] = json!([{
            "proposed_by": "vadi",
            "at_checkpoint": 5,
            "resume_status": "parallel_implementing",
            "approved_by": "prativadi",
            "approved_at_checkpoint": 5 // stale: not the current checkpoint (6)
        }]);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")]).assert_contains(
        "amendment resume stamp staleness",
        24,
        "illegal_transition",
    );
}

// ===========================================================================
// 6. New-token owner / phase pairing (Deliverable 1/5)
// ===========================================================================

#[test]
fn workflow_declaring_wrong_owner_is_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v3(&n, "workflow_declaring", "prativadi", 5, spec_phase);
    run(&b, &n).assert_contains(
        "workflow_declaring is vadi-owned",
        23,
        "bad_assignee_owner status=workflow_declaring",
    );
}

#[test]
fn workflow_declaring_wrong_phase_is_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v3(&n, "workflow_declaring", "vadi", 5, |b| {
        b["phase"] = json!("research"); // must be "spec"
    });
    run(&b, &n).assert_contains(
        "workflow_declaring lives in the spec phase",
        23,
        "bad_phase_status status=workflow_declaring",
    );
}
