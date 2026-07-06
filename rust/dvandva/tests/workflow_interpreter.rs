//! `dvandva write` — the declared-graph interpreter (v3 `run_workflow`).
//!
//! Transition legality for a v3 baton is drawn from the baton's OWN
//! `run_workflow` graph, not the hardcoded profile match. A `source: "custom"`
//! run_workflow legalizes exactly its declared edges: an edge no preset has is
//! accepted when the custom graph declares it, and an edge the preset would
//! have allowed is rejected when the custom graph omits it.

mod common;

use common::{make_baton_v3, run, v2_status_owner};
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

/// A shape-valid `source: "custom"` run_workflow that REWIRES the research
/// phase: it declares `research_drafting -> deep_review` (an edge no preset
/// graph has) and deliberately OMITS `research_drafting -> research_review`
/// (an edge the `review`/`research` presets DO have). Every state token is in
/// the v2 catalog; every edge endpoint is a declared state; the stamps are a
/// valid peer-approved pair.
fn custom_rewired_workflow() -> Value {
    json!({
        "source": "custom",
        "declared_by": "vadi",
        "declared_at_checkpoint": 0,
        "approved_by": "prativadi",
        "approved_at_checkpoint": 1,
        "revision_round": 0,
        "states": [
            {"name": "research_drafting", "owner": "vadi", "class": "work"},
            {"name": "research_review", "owner": "prativadi", "class": "review_gate"},
            {"name": "deep_review", "owner": "prativadi", "class": "review_gate"},
            {"name": "termination_review", "owner": "team", "class": "review_gate"},
            {"name": "done", "owner": "team", "class": "terminal"}
        ],
        "edges": [
            {"from": "research_drafting", "to": "deep_review"},
            {"from": "deep_review", "to": "termination_review"},
            {"from": "termination_review", "to": "done"}
        ],
        "amendments": []
    })
}

/// A v3 CUSTOM graph declaring `research_drafting -> deep_review` (which NO
/// preset has) legalizes that edge: the write is not rejected with the
/// edge-legality diagnostic. Pre-cutover the interpreter ignored `run_workflow`
/// and fell back to the preset match, which has no such edge, so this failed
/// with `illegal_transition no legal edge research_drafting->deep_review`.
#[test]
fn custom_graph_legalizes_a_non_preset_edge() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "research_drafting", "vadi", 4, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["run_workflow"] = custom_rewired_workflow();
    });
    make_baton_v3(&n, "deep_review", v2_status_owner("deep_review"), 5, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["run_workflow"] = custom_rewired_workflow();
    });
    let out = run(&b, &n);
    assert!(
        !out.text
            .contains("no legal edge research_drafting->deep_review"),
        "custom graph declares research_drafting->deep_review, so it must not be \
         rejected as an illegal edge; got exit {} output:\n{}",
        out.code,
        out.text
    );
}

/// A v3 CUSTOM graph that OMITS `research_drafting -> research_review` rejects
/// that edge even though the `review` preset (selected by mode) would have
/// allowed it. Pre-cutover the interpreter used the preset match and accepted
/// this transition (exit 0); post-cutover the baton's own graph is authority,
/// so it is an illegal edge (exit 24).
#[test]
fn custom_graph_rejects_an_omitted_preset_edge() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "research_drafting", "vadi", 4, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["run_workflow"] = custom_rewired_workflow();
    });
    make_baton_v3(
        &n,
        "research_review",
        v2_status_owner("research_review"),
        5,
        |v| {
            v["mode"] = json!("review");
            v["phase"] = json!("review");
            v["run_workflow"] = custom_rewired_workflow();
        },
    );
    run(&b, &n).assert_contains(
        "custom graph omits research_drafting->research_review",
        24,
        "no legal edge research_drafting->research_review",
    );
}
