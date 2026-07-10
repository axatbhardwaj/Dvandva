//! Integration tests for `dvandva next` — the candidate scaffolder (design §F1,
//! superpowers/specs/2026-07-02-flow-patches-design.html).
//!
//! Each test spawns the real `dvandva` binary against a fixture baton written
//! into a tempdir. Current batons are built with the shared `make_baton_v3`
//! fixture so a generated candidate inherits a fully-valid v3 field set; the
//! strongest property — `dvandva write` ACCEPTS the generated candidate — is
//! proven by spawning `write` (via `common::run`) on the emitted file.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::{cross_review_tracks, make_baton_v3, run, run_env};
use serde_json::{json, Value};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

/// Spawn `dvandva next <args>`, clearing the selector env vars so the resolved
/// baton path is fully controlled by the test. Returns (exit, stdout, stderr).
fn run_next(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(bin())
        .arg("next")
        .args(args)
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_BATON_FILE")
        .env_remove("DVANDVA_RUN_DIR")
        .env_remove("DVANDVA_RUN_ID")
        .output()
        .expect("spawn dvandva next");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Spawn `dvandva next <args>` with explicit env overrides (e.g. `DVANDVA_ROLE`),
/// otherwise clearing the selector env vars like [`run_next`].
fn run_next_env(args: &[&str], envs: &[(&str, &str)]) -> (i32, String, String) {
    let mut cmd = Command::new(bin());
    cmd.arg("next")
        .args(args)
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_BATON_FILE")
        .env_remove("DVANDVA_RUN_DIR")
        .env_remove("DVANDVA_RUN_ID");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("spawn dvandva next");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read candidate")).expect("parse candidate")
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn assert_v3_candidate(cand: &Value, workflow_source: &str) {
    assert_eq!(cand["schema"], "dvandva.baton.v3");
    assert_eq!(cand["run_workflow"]["source"], workflow_source);
    assert!(cand["run_workflow"]["states"].is_array());
    assert!(cand["run_workflow"]["edges"].is_array());
    assert!(cand["run_workflow"]["amendments"].is_array());
}

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

// ===================== LIST mode =====================

#[test]
fn list_research_drafting_option_set() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 0, |_| {});

    let (code, stdout, stderr) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0, "list exits 0\nstderr:\n{stderr}");
    assert_eq!(
        lines(&stdout),
        vec![
            "DVANDVA_NEXT research_review owner=prativadi phase=spec review_target=research",
            "DVANDVA_NEXT human_decision owner=human phase=same",
            "DVANDVA_NEXT human_question owner=human phase=same",
            "DVANDVA_NEXT note content_gates_not_reflected",
        ],
        "research_drafting legal option set + fixed over-approximation note"
    );
}

#[test]
fn list_role_filter_keeps_only_that_role() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 0, |_| {});

    let (code, stdout, _stderr) =
        run_next(&["--file", baton.to_str().unwrap(), "--role", "prativadi"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("DVANDVA_NEXT research_review owner=prativadi"),
        "prativadi owns research_review\n{stdout}"
    );
    assert!(
        !stdout.contains("human_decision") && !stdout.contains("human_question"),
        "human-owned transitions are filtered out under --role\n{stdout}"
    );
    assert!(
        stdout.contains("DVANDVA_NEXT note content_gates_not_reflected"),
        "the note is still printed after a filtered list\n{stdout}"
    );
}

#[test]
fn custom_graph_list_uses_declared_edges_not_selected_preset() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 4, |b| {
        b["mode"] = Value::from("review");
        b["phase"] = Value::from("review");
        b["run_workflow"] = custom_rewired_workflow();
    });

    let (code, stdout, stderr) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0, "custom graph list exits 0\nstderr:\n{stderr}");
    assert!(
        stdout.contains(
            "DVANDVA_NEXT deep_review owner=prativadi phase=same review_target=implementation"
        ),
        "LIST follows the declared custom edge research_drafting->deep_review\n{stdout}"
    );
    assert!(
        !stdout.contains("DVANDVA_NEXT research_review "),
        "LIST must not leak the review preset's omitted research_drafting->research_review edge\n{stdout}"
    );
}

#[test]
fn list_research_review_offers_workflow_declaring_gate() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "research_review", "prativadi", 4, |_| {});

    let (code, stdout, stderr) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0, "research_review list exits 0\nstderr:\n{stderr}");
    assert!(
        stdout.contains("DVANDVA_NEXT workflow_declaring owner=vadi phase=spec"),
        "LIST includes the declaration gate after research_review\n{stdout}"
    );
}

// ===================== GENERATE mode =====================

#[test]
fn generate_research_review_happy_path_and_write_accepts_it() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 0, |_| {});

    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "research_review",
        "--summary",
        "Research drafted; handing to prativadi for review.",
        "--next-action",
        "prativadi: run research_review against the artifact.",
    ]);
    assert_eq!(code, 0, "generate exits 0\nstderr:\n{stderr}");
    assert!(
        stdout.contains(&format!(
            "DVANDVA_NEXT ok wrote={} to=research_review checkpoint=1",
            candidate.display()
        )),
        "ok line names the default candidate path + target + checkpoint\n{stdout}"
    );

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "research_review");
    assert_eq!(cand["assignee"], "prativadi");
    assert_eq!(cand["phase"], "research");
    assert_eq!(cand["checkpoint"], 1);
    assert_eq!(cand["review_target"], "research");
    assert_v3_candidate(&cand, "preset:full");

    // Strongest property: the SAME binary's `write` accepts the generated file.
    run(&baton, &candidate).assert("write accepts the generated research_review candidate", 0);
    // ...and the baton is now installed at checkpoint 1.
    assert_eq!(read_json(&baton)["checkpoint"], 1);
}

#[test]
fn custom_graph_generate_declared_edge_and_write_accepts_it() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 4, |b| {
        b["mode"] = Value::from("review");
        b["phase"] = Value::from("review");
        b["run_workflow"] = custom_rewired_workflow();
    });

    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "deep_review",
        "--summary",
        "Custom graph skips research_review and sends the work straight to deep_review.",
        "--next-action",
        "prativadi: run deep_review on the custom declared graph path.",
    ]);
    assert_eq!(code, 0, "custom graph generate exits 0\nstderr:\n{stderr}");
    assert!(
        stdout.contains("to=deep_review checkpoint=5"),
        "ok line names the custom target + checkpoint\n{stdout}"
    );

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "deep_review");
    assert_eq!(cand["assignee"], "prativadi");
    assert_eq!(cand["phase"], "review");
    assert_eq!(cand["review_target"], "implementation");
    assert_v3_candidate(&cand, "custom");

    run(&baton, &candidate).assert("write accepts the generated custom-graph candidate", 0);
    assert_eq!(read_json(&baton)["checkpoint"], 5);
}

// dr-dispatch-request-not-produced (round 2, GAP 1): the canonical tooling path
// into a development-mode prativadi-owned deep_review must produce a scaffold
// that `dvandva write` ACCEPTS. `next` re-validates its own candidate in-process
// through the same write pipeline, so before the scaffold learned to record the
// vadi dispatch request this GENERATE failed 23 (`missing_dispatch_request`) and
// never emitted a file — the one command a walkaway loop is supposed to use was
// broken. Post-fix: next scaffolds the canonical open entry, self-validation
// passes, and the emitted candidate round-trips through `write` at exit 0.
#[test]
fn generate_cross_review_to_deep_review_scaffolds_dispatch_request_and_write_accepts_it() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    // Development-mode cross_review, ready to advance: both cross-review tracks
    // are recorded (required review_checkpoint == current checkpoint 4).
    make_baton_v3(&baton, "cross_review", "team", 4, |b| {
        b["active_roles"] = serde_json::json!(["vadi", "prativadi"]);
        cross_review_tracks(b);
    });

    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "deep_review",
        "--summary",
        "Cross-review approved by both roles; entering deep_review under prativadi.",
        "--next-action",
        "prativadi: run deep_review; vadi: dispatch the credited cross-vendor opus reviewers.",
    ]);
    assert_eq!(code, 0, "deep_review generate exits 0\nstderr:\n{stderr}");
    assert!(
        stdout.contains("to=deep_review checkpoint=5"),
        "ok line names the deep_review target + checkpoint\n{stdout}"
    );

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "deep_review");
    assert_eq!(cand["assignee"], "prativadi");
    // The scaffold recorded exactly one OPEN dispatch request naming the vadi,
    // id keyed to the new checkpoint.
    let reqs = cand["dispatch_requests"]
        .as_array()
        .expect("dispatch_requests array");
    assert_eq!(reqs.len(), 1, "one scaffolded dispatch request\n{cand}");
    assert_eq!(reqs[0]["id"], "credited-opus-dispatch-5");
    assert_eq!(reqs[0]["role"], "vadi");
    assert_eq!(reqs[0]["status"], "open");
    assert!(
        reqs[0]["purpose"].as_str().is_some_and(|p| !p.is_empty()),
        "non-empty purpose\n{cand}"
    );

    // Strongest property: the SAME binary's `write` accepts the generated file.
    run(&baton, &candidate).assert("write accepts the generated deep_review candidate", 0);
    assert_eq!(read_json(&baton)["checkpoint"], 5);
}

// tc-dispatch-request-ack-producer-wiring-r4 (FIX 2a): on a deep_review baton
// where the addressed role's own dispatch request is OPEN, `dvandva next --to
// deep_review` emits the same-status ack candidate (open->acknowledged) that the
// write-side ack carve accepts. Before wiring, next had no same-status deep_review
// target and exited 2 (illegal_target), so the addressed role had no tooling path
// to the ack write and the wait surface kept re-firing.
#[test]
fn generate_deep_review_ack_candidate_and_write_accepts_it() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "deep_review", "prativadi", 4, |b| {
        b["review_target"] = json!("implementation");
        // The vadi's own dispatch request is open -- the wake it just answered.
        b["dispatch_requests"] = json!([
            {"id": "dr-opus", "role": "vadi", "purpose": "credited cross-vendor Anthropic-Opus dispatch", "status": "open"}
        ]);
    });

    // LIST surfaces the same-status ack target (owned by the addressed vadi).
    let (lcode, lout, _lerr) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(lcode, 0);
    assert!(
        lout.contains("DVANDVA_NEXT deep_review owner=prativadi phase=same"),
        "LIST offers the same-status deep_review ack\n{lout}"
    );

    let (code, stdout, stderr) = run_next_env(
        &[
            "--file",
            baton.to_str().unwrap(),
            "--to",
            "deep_review",
            "--summary",
            "Vadi claims the paid cross-vendor Opus dispatch before spawning reviewers.",
            "--next-action",
            "vadi: dispatch the credited cross-vendor Opus reviewers; prativadi holds deep_review.",
        ],
        &[("DVANDVA_ROLE", "vadi")],
    );
    assert_eq!(
        code, 0,
        "deep_review ack generate exits 0\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("to=deep_review checkpoint=5"),
        "ok line names the deep_review target + checkpoint\n{stdout}"
    );

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "deep_review");
    // The ack does not change ownership -- deep_review stays prativadi-owned.
    assert_eq!(cand["assignee"], "prativadi");
    let reqs = cand["dispatch_requests"]
        .as_array()
        .expect("dispatch_requests array");
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0]["id"], "dr-opus");
    assert_eq!(reqs[0]["status"], "acknowledged");

    // The generated ack candidate round-trips through `write` under the vadi role.
    run_env(&baton, &candidate, &[("DVANDVA_ROLE", "vadi")])
        .assert("write accepts the generated deep_review ack candidate", 0);
    assert_eq!(read_json(&baton)["checkpoint"], 5);
}

// tc-dispatch-request-ack-producer-wiring-r4 (FIX 2b): scaffold idempotence keys
// on the CANONICAL vadi request specifically, not "any open vadi request". An
// UNRELATED open vadi request must not suppress scaffolding of the credited
// dispatch entry -- before the fix, scaffold skipped, the exact-purpose entry gate
// then rejected the candidate, and next self-failed 23 with no file emitted.
#[test]
fn generate_deep_review_scaffolds_canonical_alongside_unrelated_open_vadi_request() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "cross_review", "team", 4, |b| {
        b["active_roles"] = json!(["vadi", "prativadi"]);
        cross_review_tracks(b);
        // An unrelated open vadi request already rides the baton.
        b["dispatch_requests"] = json!([
            {"id": "unrelated-1", "role": "vadi", "purpose": "unrelated maintenance sweep", "status": "open"}
        ]);
    });

    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "deep_review",
        "--summary",
        "Cross-review approved; entering deep_review with the canonical dispatch scaffolded.",
        "--next-action",
        "prativadi: run deep_review; vadi: dispatch the credited cross-vendor opus reviewers.",
    ]);
    assert_eq!(
        code, 0,
        "deep_review generate exits 0 despite an unrelated open vadi request\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("to=deep_review checkpoint=5"),
        "ok line\n{stdout}"
    );

    let cand = read_json(&candidate);
    let reqs = cand["dispatch_requests"]
        .as_array()
        .expect("dispatch_requests array");
    // Both survive: the unrelated one untouched, the canonical scaffolded.
    assert_eq!(
        reqs.len(),
        2,
        "canonical scaffolded alongside the unrelated request\n{cand}"
    );
    assert!(
        reqs.iter().any(|r| r["id"] == "credited-opus-dispatch-5"
            && r["purpose"] == "credited cross-vendor Anthropic-Opus dispatch"
            && r["status"] == "open"),
        "the canonical open vadi request was scaffolded\n{cand}"
    );

    run(&baton, &candidate).assert("write accepts the candidate with both requests", 0);
}

// tc-dispatch-request-composition-r5 (P2): an ack write flips EXACTLY ONE request.
// With two OPEN vadi requests on a deep_review baton, `dvandva next --to deep_review`
// acks only ONE (the deterministic lowest id), leaving the other open — one wake,
// one ack. Before the fix the scaffold flipped EVERY open vadi request in a single
// write, claiming both paid dispatches at once.
#[test]
fn generate_deep_review_ack_flips_exactly_one_of_two_open_vadi_requests() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "deep_review", "prativadi", 4, |b| {
        b["review_target"] = json!("implementation");
        b["dispatch_requests"] = json!([
            {"id": "dr-1-canonical", "role": "vadi", "purpose": "credited cross-vendor Anthropic-Opus dispatch", "status": "open"},
            {"id": "dr-2-unrelated", "role": "vadi", "purpose": "unrelated maintenance sweep", "status": "open"}
        ]);
    });

    let (code, stdout, stderr) = run_next_env(
        &[
            "--file",
            baton.to_str().unwrap(),
            "--to",
            "deep_review",
            "--summary",
            "Vadi claims exactly one paid cross-vendor Opus dispatch.",
            "--next-action",
            "vadi: dispatch the credited reviewers for the acked request; prativadi holds deep_review.",
        ],
        &[("DVANDVA_ROLE", "vadi")],
    );
    assert_eq!(code, 0, "single-ack generate exits 0\nstderr:\n{stderr}");
    assert!(
        stdout.contains("to=deep_review checkpoint=5"),
        "ok line names the target + checkpoint\n{stdout}"
    );

    let cand = read_json(&candidate);
    let reqs = cand["dispatch_requests"]
        .as_array()
        .expect("dispatch_requests array");
    assert_eq!(reqs.len(), 2, "both requests survive\n{cand}");
    let acked: Vec<&str> = reqs
        .iter()
        .filter(|r| r["status"] == "acknowledged")
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        acked,
        vec!["dr-1-canonical"],
        "exactly one request (the lowest id) is acked, the other stays open\n{cand}"
    );

    // The single-flip candidate round-trips through `write` under the vadi role.
    run_env(&baton, &candidate, &[("DVANDVA_ROLE", "vadi")])
        .assert("write accepts the single-request ack candidate", 0);
    assert_eq!(read_json(&baton)["checkpoint"], 5);
}

// tc-dispatch-request-composition-r5 (P3): GENERATE selection honors --role. With a
// prativadi-addressed request FIRST in the list, `next --role vadi --to deep_review`
// acks the VADI's request, not the first-listed prativadi one. Before the fix,
// selection matched on to_status alone, picked the prativadi ack, and self-failed 23
// (a vadi cannot ack a prativadi request).
#[test]
fn generate_deep_review_ack_honors_role_with_prativadi_request_first() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "deep_review", "prativadi", 4, |b| {
        b["review_target"] = json!("implementation");
        b["dispatch_requests"] = json!([
            {"id": "dr-prativadi", "role": "prativadi", "purpose": "prativadi-side maintenance dispatch", "status": "open"},
            {"id": "dr-vadi", "role": "vadi", "purpose": "credited cross-vendor Anthropic-Opus dispatch", "status": "open"}
        ]);
    });

    let (code, stdout, stderr) = run_next_env(
        &[
            "--file",
            baton.to_str().unwrap(),
            "--role",
            "vadi",
            "--to",
            "deep_review",
            "--summary",
            "Vadi acks its own request despite a prativadi request listed first.",
            "--next-action",
            "vadi: dispatch the credited reviewers; prativadi holds its own open request.",
        ],
        &[("DVANDVA_ROLE", "vadi")],
    );
    assert_eq!(code, 0, "role-scoped generate exits 0\nstderr:\n{stderr}");
    assert!(
        stdout.contains("to=deep_review checkpoint=5"),
        "ok line\n{stdout}"
    );

    let cand = read_json(&candidate);
    let reqs = cand["dispatch_requests"]
        .as_array()
        .expect("dispatch_requests array");
    let by_id = |id: &str| {
        reqs.iter()
            .find(|r| r["id"] == id)
            .expect("request present")
            .clone()
    };
    assert_eq!(
        by_id("dr-vadi")["status"],
        "acknowledged",
        "the vadi request is acked\n{cand}"
    );
    assert_eq!(
        by_id("dr-prativadi")["status"],
        "open",
        "the prativadi request is untouched\n{cand}"
    );

    run_env(&baton, &candidate, &[("DVANDVA_ROLE", "vadi")])
        .assert("write accepts the role-scoped vadi ack candidate", 0);
    assert_eq!(read_json(&baton)["checkpoint"], 5);
}

// The exact canonical credited-Opus dispatch purpose (mirrors write.rs's
// CANONICAL_OPUS_DISPATCH_PURPOSE, which is pub(crate) and unreachable from this
// integration crate). A request whose purpose is EXACTLY this string is the
// canonical wake the protocol produces on deep_review entry.
const CANONICAL_PURPOSE: &str = "credited cross-vendor Anthropic-Opus dispatch";

/// A `dispatch_requests` entry: canonical=true stamps the exact canonical purpose,
/// false a distinct unrelated purpose.
fn ack_req(id: &str, role: &str, canonical: bool, status: &str) -> Value {
    let purpose = if canonical {
        CANONICAL_PURPOSE
    } else {
        "unrelated maintenance sweep"
    };
    json!({"id": id, "role": role, "purpose": purpose, "status": status})
}

// tc-dispatch-request-selection-order-r6 (probe a): with two OPEN vadi requests
// where a NON-canonical id sorts lexicographically BEFORE the canonical one, the
// role-scoped `next --role vadi --to deep_review` must ack the CANONICAL wake, not
// the lowest id. Before the fix, selection sorted by id and acked `aaa-unrelated`,
// leaving the credited-Opus wake open — the wait surface kept re-firing.
#[test]
fn generate_deep_review_ack_prefers_canonical_over_lower_sorting_id() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "deep_review", "prativadi", 4, |b| {
        b["review_target"] = json!("implementation");
        b["dispatch_requests"] = json!([
            ack_req("aaa-unrelated", "vadi", false, "open"),
            ack_req("credited-opus-dispatch-49", "vadi", true, "open"),
        ]);
    });

    let (code, stdout, stderr) = run_next_env(
        &[
            "--file",
            baton.to_str().unwrap(),
            "--role",
            "vadi",
            "--to",
            "deep_review",
            "--summary",
            "Vadi claims the canonical credited-Opus dispatch despite a lower-sorting id.",
            "--next-action",
            "vadi: dispatch the credited reviewers for the canonical wake.",
        ],
        &[("DVANDVA_ROLE", "vadi")],
    );
    assert_eq!(
        code, 0,
        "canonical-first generate exits 0\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("dispatch_ack=credited-opus-dispatch-49"),
        "ok line names the canonical acked id\n{stdout}"
    );

    let cand = read_json(&candidate);
    let reqs = cand["dispatch_requests"].as_array().unwrap();
    let by_id = |id: &str| reqs.iter().find(|r| r["id"] == id).unwrap().clone();
    assert_eq!(
        by_id("credited-opus-dispatch-49")["status"],
        "acknowledged",
        "the canonical request is acked\n{cand}"
    );
    assert_eq!(
        by_id("aaa-unrelated")["status"],
        "open",
        "the lower-sorting unrelated request stays open\n{cand}"
    );
    run_env(&baton, &candidate, &[("DVANDVA_ROLE", "vadi")])
        .assert("write accepts the canonical-first ack candidate", 0);
}

// tc-dispatch-request-selection-order-r6 (probe b): the BARE documented command
// `DVANDVA_ROLE=vadi dvandva next --to deep_review` (no --role flag) must honor the
// environment role for selection and ack the VADI's canonical wake, even when a
// prativadi request sorts first. Before the fix, the bare command ignored
// DVANDVA_ROLE, sorted by id, selected the prativadi request, and self-failed 23
// (a vadi cannot ack a prativadi request).
#[test]
fn generate_deep_review_ack_bare_command_honors_env_role() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "deep_review", "prativadi", 4, |b| {
        b["review_target"] = json!("implementation");
        b["dispatch_requests"] = json!([
            ack_req("aaa-prativadi", "prativadi", false, "open"),
            ack_req("zzz-vadi-canonical", "vadi", true, "open"),
        ]);
    });

    // Bare command: no --role flag; DVANDVA_ROLE=vadi is the only role source.
    let (code, stdout, stderr) = run_next_env(
        &[
            "--file",
            baton.to_str().unwrap(),
            "--to",
            "deep_review",
            "--summary",
            "Bare command acks the vadi wake using the environment role.",
            "--next-action",
            "vadi: dispatch the credited reviewers for the env-role wake.",
        ],
        &[("DVANDVA_ROLE", "vadi")],
    );
    assert_eq!(code, 0, "bare env-role generate exits 0\nstderr:\n{stderr}");
    assert!(
        stdout.contains("dispatch_ack=zzz-vadi-canonical"),
        "ok line names the vadi acked id\n{stdout}"
    );

    let cand = read_json(&candidate);
    let reqs = cand["dispatch_requests"].as_array().unwrap();
    let by_id = |id: &str| reqs.iter().find(|r| r["id"] == id).unwrap().clone();
    assert_eq!(
        by_id("zzz-vadi-canonical")["status"],
        "acknowledged",
        "the vadi request is acked via the env role\n{cand}"
    );
    assert_eq!(
        by_id("aaa-prativadi")["status"],
        "open",
        "the first-sorting prativadi request stays open\n{cand}"
    );
    run_env(&baton, &candidate, &[("DVANDVA_ROLE", "vadi")])
        .assert("write accepts the env-role ack candidate", 0);
}

// --dispatch-request only selects a deep_review ack; on any other --to it errors.
#[test]
fn generate_dispatch_request_flag_rejected_on_non_deep_review_target() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "deep_review", "prativadi", 4, |b| {
        b["review_target"] = json!("implementation");
        b["dispatch_requests"] = json!([ack_req("d1", "vadi", true, "open")]);
    });
    let (code, _stdout, stderr) = run_next_env(
        &[
            "--file",
            baton.to_str().unwrap(),
            "--to",
            "phase_fixing",
            "--dispatch-request",
            "d1",
            "--summary",
            "Route findings to phase_fixing.",
            "--next-action",
            "vadi: address the deep_review findings.",
        ],
        &[("DVANDVA_ROLE", "vadi")],
    );
    assert_eq!(
        code, 2,
        "flag on a non-ack target exits 2\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("applies only to"),
        "error explains the flag is deep_review-ack only\n{stderr}"
    );
}

// tc-dispatch-request-selection-order-r6 (P6): the exhaustive composition matrix.
// Sweeps {1..3 requests} x {vadi/prativadi owners} x {open/acknowledged/completed}
// x {bare / --role / --dispatch-request selection}, asserting for every row either
// the ack targets EXACTLY the expected request (exit 0, single open->acknowledged
// flip, others preserved, and write accepts the candidate) or the command errors
// with the expected reason. This enumerated table is the completeness proof future
// rounds check against.
#[test]
fn dispatch_ack_selection_composition_matrix() {
    // (id, owner-role, canonical?, status)
    type Req = (&'static str, &'static str, bool, &'static str);
    enum Expect {
        // exit 0; this id flips open->acknowledged, every other entry preserved.
        Ack(&'static str),
        // exit code + a stderr needle.
        Err(i32, &'static str),
    }
    struct Case {
        name: &'static str,
        requests: &'static [Req],
        env_role: &'static str,
        role_flag: Option<&'static str>,
        dispatch_request: Option<&'static str>,
        expect: Expect,
    }
    use Expect::{Ack, Err};

    let cases: &[Case] = &[
        // ---- single request, bare command --------------------------------
        Case {
            name: "1req-open-canonical-vadi-bare",
            requests: &[("d1", "vadi", true, "open")],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: None,
            expect: Ack("d1"),
        },
        Case {
            name: "1req-open-noncanonical-vadi-bare",
            requests: &[("d1", "vadi", false, "open")],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: None,
            expect: Ack("d1"),
        },
        Case {
            name: "1req-open-canonical-prativadi-bare",
            requests: &[("d1", "prativadi", true, "open")],
            env_role: "prativadi",
            role_flag: None,
            dispatch_request: None,
            expect: Ack("d1"),
        },
        // ---- two same-role requests, canonical-first ---------------------
        Case {
            name: "2req-vadi-canonical-sorts-last-bare",
            requests: &[
                ("aaa", "vadi", false, "open"),
                ("zzz-canon", "vadi", true, "open"),
            ],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: None,
            expect: Ack("zzz-canon"),
        },
        Case {
            name: "2req-vadi-canonical-sorts-last-role-flag",
            requests: &[
                ("aaa", "vadi", false, "open"),
                ("zzz-canon", "vadi", true, "open"),
            ],
            env_role: "vadi",
            role_flag: Some("vadi"),
            dispatch_request: None,
            expect: Ack("zzz-canon"),
        },
        // ---- two same-role non-canonical: ambiguous without a selector ---
        Case {
            name: "2req-vadi-noncanonical-ambiguous-bare",
            requests: &[("d1", "vadi", false, "open"), ("d2", "vadi", false, "open")],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: None,
            expect: Err(2, "ambiguous dispatch ack"),
        },
        Case {
            name: "2req-vadi-noncanonical-explicit-d2",
            requests: &[("d1", "vadi", false, "open"), ("d2", "vadi", false, "open")],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: Some("d2"),
            expect: Ack("d2"),
        },
        Case {
            name: "2req-vadi-noncanonical-explicit-d1",
            requests: &[("d1", "vadi", false, "open"), ("d2", "vadi", false, "open")],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: Some("d1"),
            expect: Ack("d1"),
        },
        // ---- mixed-role: env/flag role binds the selection ---------------
        Case {
            name: "2req-mixed-prativadi-first-vadi-canonical-bare",
            requests: &[
                ("aaa-prati", "prativadi", false, "open"),
                ("zzz-vadi", "vadi", true, "open"),
            ],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: None,
            expect: Ack("zzz-vadi"),
        },
        Case {
            name: "2req-mixed-invoking-prativadi-bare",
            requests: &[
                ("aaa-prati", "prativadi", false, "open"),
                ("zzz-vadi", "vadi", true, "open"),
            ],
            env_role: "prativadi",
            role_flag: None,
            dispatch_request: None,
            expect: Ack("aaa-prati"),
        },
        Case {
            name: "2req-mixed-explicit-cross-role-rejected",
            requests: &[
                ("aaa-prati", "prativadi", false, "open"),
                ("zzz-vadi", "vadi", true, "open"),
            ],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: Some("aaa-prati"),
            expect: Err(2, "not the invoking role"),
        },
        // ---- explicit selector error surfaces ----------------------------
        Case {
            name: "explicit-nonexistent-id",
            requests: &[("d1", "vadi", true, "open")],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: Some("nope"),
            expect: Err(2, "no dispatch request with id"),
        },
        Case {
            name: "explicit-acknowledged-not-open",
            requests: &[
                ("d1", "vadi", true, "open"),
                ("d2", "vadi", false, "acknowledged"),
            ],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: Some("d2"),
            expect: Err(2, "not open"),
        },
        Case {
            name: "explicit-completed-not-open",
            requests: &[
                ("d1", "vadi", true, "open"),
                ("d2", "vadi", false, "completed"),
            ],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: Some("d2"),
            expect: Err(2, "not open"),
        },
        // ---- role has no open request ------------------------------------
        Case {
            name: "role-has-no-open-request",
            requests: &[("p1", "prativadi", false, "open")],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: None,
            expect: Err(2, "no open dispatch request for role"),
        },
        // ---- three requests ----------------------------------------------
        Case {
            name: "3req-canonical-among-vadi-role-flag",
            requests: &[
                ("aaa-vadi", "vadi", false, "open"),
                ("mmm-vadi-canon", "vadi", true, "open"),
                ("prati", "prativadi", false, "open"),
            ],
            env_role: "vadi",
            role_flag: Some("vadi"),
            dispatch_request: None,
            expect: Ack("mmm-vadi-canon"),
        },
        Case {
            name: "3req-prativadi-single-candidate",
            requests: &[
                ("aaa-vadi", "vadi", false, "open"),
                ("mmm-vadi-canon", "vadi", true, "open"),
                ("prati", "prativadi", false, "open"),
            ],
            env_role: "prativadi",
            role_flag: Some("prativadi"),
            dispatch_request: None,
            expect: Ack("prati"),
        },
        // ---- closed companions do not block the open ack -----------------
        Case {
            name: "open-canonical-with-acknowledged-companion-bare",
            requests: &[
                ("d1", "vadi", true, "open"),
                ("d2", "vadi", false, "acknowledged"),
            ],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: None,
            expect: Ack("d1"),
        },
        // ---- no open request at all: same-status ack is not a legal edge --
        Case {
            name: "all-acknowledged-no-open-illegal-target",
            requests: &[("d1", "vadi", true, "acknowledged")],
            env_role: "vadi",
            role_flag: None,
            dispatch_request: None,
            expect: Err(2, "illegal_target"),
        },
    ];

    for case in cases {
        let dir = tempfile::tempdir().unwrap();
        let baton = dir.path().join("baton.json");
        let candidate = dir.path().join("baton.next.json");
        let reqs: Vec<Value> = case
            .requests
            .iter()
            .map(|(id, role, canon, status)| ack_req(id, role, *canon, status))
            .collect();
        make_baton_v3(&baton, "deep_review", "prativadi", 4, |b| {
            b["review_target"] = json!("implementation");
            b["dispatch_requests"] = Value::Array(reqs);
        });

        let mut args: Vec<String> = vec![
            "--file".into(),
            baton.to_str().unwrap().into(),
            "--to".into(),
            "deep_review".into(),
            "--summary".into(),
            format!("matrix case {}", case.name),
            "--next-action".into(),
            "vadi/prativadi acks its dispatch wake.".into(),
        ];
        if let Some(role) = case.role_flag {
            args.push("--role".into());
            args.push(role.into());
        }
        if let Some(id) = case.dispatch_request {
            args.push("--dispatch-request".into());
            args.push(id.into());
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, stdout, stderr) = run_next_env(&arg_refs, &[("DVANDVA_ROLE", case.env_role)]);

        match case.expect {
            Ack(id) => {
                assert_eq!(
                    code, 0,
                    "case '{}': expected ack exit 0, got {code}\nstderr:\n{stderr}",
                    case.name
                );
                assert!(
                    stdout.contains(&format!("dispatch_ack={id}")),
                    "case '{}': ok line must name acked id {id}\nstdout:\n{stdout}",
                    case.name
                );
                let cand = read_json(&candidate);
                let creqs = cand["dispatch_requests"].as_array().unwrap();
                for r in creqs {
                    let rid = r["id"].as_str().unwrap();
                    let want = if rid == id {
                        "acknowledged"
                    } else {
                        case.requests
                            .iter()
                            .find(|(cid, ..)| *cid == rid)
                            .unwrap()
                            .3
                    };
                    assert_eq!(
                        r["status"], want,
                        "case '{}': request {rid} status\n{cand}",
                        case.name
                    );
                }
                // End-to-end: `write` accepts the ack candidate under the same role.
                run_env(&baton, &candidate, &[("DVANDVA_ROLE", case.env_role)])
                    .assert(case.name, 0);
            }
            Err(expected_code, needle) => {
                assert_eq!(
                    code, expected_code,
                    "case '{}': expected err exit {expected_code}, got {code}\nstderr:\n{stderr}",
                    case.name
                );
                assert!(
                    stderr.contains(needle),
                    "case '{}': stderr must contain '{needle}'\nstderr:\n{stderr}",
                    case.name
                );
                assert!(
                    !candidate.exists(),
                    "case '{}': no candidate must be written on error",
                    case.name
                );
            }
        }
    }
}

#[test]
fn generate_workflow_declaring_candidate_and_write_accepts_it() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "research_review", "prativadi", 4, |_| {});

    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "workflow_declaring",
        "--summary",
        "Research accepted; vadi will declare the per-run workflow before spec drafting.",
        "--next-action",
        "vadi: declare the run workflow and hand to prativadi for workflow_review.",
    ]);
    assert_eq!(
        code, 0,
        "workflow_declaring generate exits 0\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("to=workflow_declaring checkpoint=5"),
        "ok line names the declaration target + checkpoint\n{stdout}"
    );

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "workflow_declaring");
    assert_eq!(cand["assignee"], "vadi");
    assert_eq!(cand["phase"], "spec");
    assert_eq!(cand["checkpoint"], 5);
    assert_v3_candidate(&cand, "preset:full");

    run(&baton, &candidate).assert("write accepts the workflow_declaring candidate", 0);
    assert_eq!(read_json(&baton)["checkpoint"], 5);
}

#[test]
fn review_mode_list_and_generate_roundtrip() {
    // H1 regression pin: a review-mode baton pins EVERY status to phase="review"
    // (the engine's phase_status_ok demands is_str("review") for ("review", _)).
    // LIST must show research_review as legal, and GENERATE must emit a candidate
    // `dvandva write` ACCEPTS — i.e. its phase is "review", not "research".
    // Before the fix, generate reconstructed phase="research" mode-blindly and the
    // engine rejected the candidate with exit 23 (bad_phase_status).
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 0, |b| {
        b["mode"] = Value::from("review");
        b["phase"] = Value::from("review");
        b["run_workflow"]["source"] = Value::from("preset:review");
    });

    // LIST shows research_review is legal from a review-mode research_drafting.
    let (list_code, list_out, list_err) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(list_code, 0, "list exits 0\nstderr:\n{list_err}");
    assert!(
        list_out.contains("DVANDVA_NEXT research_review owner=prativadi"),
        "review-mode list shows research_review\n{list_out}"
    );

    // GENERATE the research_review candidate.
    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "research_review",
        "--summary",
        "Research drafted; handing to prativadi for review.",
        "--next-action",
        "prativadi: run research_review against the artifact.",
    ]);
    assert_eq!(code, 0, "review-mode generate exits 0\nstderr:\n{stderr}");
    assert!(
        stdout.contains("to=research_review checkpoint=1"),
        "ok line names the target + checkpoint\n{stdout}"
    );

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "research_review");
    assert_eq!(cand["assignee"], "prativadi");
    // The crux: review mode pins the phase to "review", NOT "research".
    assert_eq!(
        cand["phase"], "review",
        "review-mode candidate carries phase=review, matching phase_status_ok"
    );
    assert_v3_candidate(&cand, "preset:review");

    // Strongest property: the SAME binary's `write` accepts the generated file.
    run(&baton, &candidate).assert("write accepts the review-mode research_review candidate", 0);
    assert_eq!(read_json(&baton)["checkpoint"], 1);
}

#[test]
fn research_mode_generate_keeps_research_phase() {
    // Research-mode guard: a research-mode baton keeps its (correct) behavior of
    // pinning research_* statuses to phase="research". Locks in that the H1 fix
    // does not regress research mode.
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 0, |b| {
        b["mode"] = Value::from("research");
        b["phase"] = Value::from("research");
        b["run_workflow"]["source"] = Value::from("preset:research");
    });

    let (code, _stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "research_review",
        "--summary",
        "Research drafted; handing to prativadi for review.",
        "--next-action",
        "prativadi: run research_review against the artifact.",
    ]);
    assert_eq!(code, 0, "research-mode generate exits 0\nstderr:\n{stderr}");

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "research_review");
    assert_eq!(
        cand["phase"], "research",
        "research-mode research_review keeps phase=research"
    );
    assert_v3_candidate(&cand, "preset:research");

    run(&baton, &candidate).assert(
        "write accepts the research-mode research_review candidate",
        0,
    );
    assert_eq!(read_json(&baton)["checkpoint"], 1);
}

#[test]
fn research_mode_termination_review_roundtrip() {
    // H1' regression pin: research_review->termination_review is a PhaseMove::Same
    // edge (classify_phase_move only special-cases the research_*/spec_* planning
    // statuses and the implementing/parallel_implementing entry). GENERATE
    // consults expected_phase_for for PhaseMove::Same edges too, so the candidate
    // phase can never desync from phase_status_ok. S5-T5: on the EXPLORATORY
    // research path (no seed markers) the terminal carries phase "research".
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "research_review", "prativadi", 0, |b| {
        b["mode"] = Value::from("research");
        b["phase"] = Value::from("research");
        b["run_workflow"]["source"] = Value::from("preset:research");
    });

    // LIST shows termination_review is legal from a research-mode research_review.
    let (list_code, list_out, list_err) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(list_code, 0, "list exits 0\nstderr:\n{list_err}");
    assert!(
        list_out.contains("DVANDVA_NEXT termination_review owner=team"),
        "research-mode list shows termination_review\n{list_out}"
    );

    // GENERATE the termination_review candidate.
    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "termination_review",
        "--summary",
        "Research approved; entering termination review.",
        "--next-action",
        "team: run termination_review against the research artifact.",
    ]);
    assert_eq!(code, 0, "research-mode generate exits 0\nstderr:\n{stderr}");
    assert!(
        stdout.contains("to=termination_review checkpoint=1"),
        "ok line names the target + checkpoint\n{stdout}"
    );

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "termination_review");
    // The crux: S5-T5 labels the exploratory research terminal "research" (the
    // current phase), matching phase_status_ok's exploratory-path expectation.
    assert_eq!(
        cand["phase"], "research",
        "research-mode exploratory candidate carries phase=research, matching phase_status_ok"
    );

    // Strongest property: the SAME binary's `write` accepts the generated file.
    run(&baton, &candidate).assert(
        "write accepts the research-mode termination_review candidate",
        0,
    );
    assert_eq!(read_json(&baton)["checkpoint"], 1);
}

#[test]
fn generate_loop_edge_increments_loop_counts() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "deep_review", "prativadi", 4, |b| {
        b["master_plan_locked"] = Value::Bool(true);
        b["loop_counts"] = serde_json::json!({"deep_review:phase_fixing": 1});
    });

    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "phase_fixing",
        "--summary",
        "Deep review found blocking issues; entering the fixing loop.",
        "--next-action",
        "vadi: address deep_review findings in phase_fixing.",
    ]);
    assert_eq!(code, 0, "loop-edge generate exits 0\nstderr:\n{stderr}");
    assert!(stdout.contains("to=phase_fixing checkpoint=5"), "{stdout}");

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "phase_fixing");
    assert_eq!(
        cand["loop_counts"]["deep_review:phase_fixing"], 2,
        "the loop edge's count is incremented by exactly one"
    );
    // Loop edge stays in the same numeric phase.
    assert_eq!(cand["phase"], 1);

    run(&baton, &candidate).assert("write accepts the generated phase_fixing candidate", 0);
}

#[test]
fn generate_amendment_entry_sets_amendment_from_phase() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    // Full-profile deslop fixture (make_baton_v3 defaults to full profile).
    make_baton_v3(&baton, "deslop", "vadi", 6, |b| {
        b["master_plan_locked"] = Value::Bool(true);
    });

    let (code, _stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "spec_revision",
        "--summary",
        "Reopening the spec to amend the plan before final approval.",
        "--next-action",
        "vadi: revise the master plan, then re-enter implementation.",
    ]);
    assert_eq!(
        code, 0,
        "amendment-entry generate exits 0\nstderr:\n{stderr}"
    );

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "spec_revision");
    assert_eq!(cand["assignee"], "vadi");
    assert_eq!(cand["phase"], "spec");
    assert_eq!(
        cand["amendment_from_phase"], 1,
        "amendment entry records the current numeric phase"
    );
    assert_eq!(
        cand["loop_counts"]["plan_amendment:1"], 1,
        "the amendment entry increments plan_amendment:<from-phase>"
    );

    run(&baton, &candidate).assert("write accepts the generated amendment-entry candidate", 0);
}

#[test]
fn generate_human_question_requires_resume_flags() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 0, |_| {});

    // Missing the resume triple -> usage error (2).
    let (missing, _out, _err) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "human_question",
        "--summary",
        "Need a human decision on scope.",
        "--next-action",
        "human: answer the question.",
    ]);
    assert_eq!(
        missing, 2,
        "human_question without resume flags is a usage error"
    );

    // With the full triple -> generated candidate.
    let (code, _out, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "human_question",
        "--summary",
        "Need a human decision on scope.",
        "--next-action",
        "human: answer the question.",
        "--question",
        "Should feature X be in scope?",
        "--resume-assignee",
        "vadi",
        "--resume-status",
        "research_drafting",
    ]);
    assert_eq!(
        code, 0,
        "human_question with resume flags generates\nstderr:\n{stderr}"
    );

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "human_question");
    assert_eq!(cand["assignee"], "human");
    assert_eq!(cand["question"], "Should feature X be in scope?");
    assert_eq!(cand["resume_assignee"], "vadi");
    assert_eq!(cand["resume_status"], "research_drafting");

    // M1: prove `dvandva write` ACCEPTS the emitted human_question candidate
    // (parity with the other three edge classes), and installs it at checkpoint 1.
    run(&baton, &candidate).assert("write accepts the generated human_question candidate", 0);
    assert_eq!(read_json(&baton)["checkpoint"], 1);
}

#[test]
fn generate_illegal_target_exits_2_and_lists_legal() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 0, |_| {});

    let (code, _stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "done",
        "--summary",
        "x",
        "--next-action",
        "y",
    ]);
    assert_eq!(code, 2, "an illegal --to is a usage error");
    assert!(
        stderr.contains("illegal_target") && stderr.contains("research_review"),
        "stderr lists the legal targets\n{stderr}"
    );
}

#[test]
fn generate_missing_summary_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 0, |_| {});

    let (code, _stdout, _stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "research_review",
        "--next-action",
        "y",
    ]);
    assert_eq!(code, 2, "missing --summary is a usage error");
}

#[test]
fn generate_default_out_path_is_baton_next_json() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 0, |_| {});

    let (code, _stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "research_review",
        "--summary",
        "s",
        "--next-action",
        "n",
    ]);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        dir.path().join("baton.next.json").is_file(),
        "the default --out is <baton-dir>/baton.next.json"
    );
}

#[test]
fn generate_never_overwrites_the_baton() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "research_drafting", "vadi", 0, |_| {});

    let (code, _stdout, _stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "research_review",
        "--summary",
        "s",
        "--next-action",
        "n",
    ]);
    assert_eq!(code, 0);

    // The baton is untouched: still checkpoint 0, still research_drafting.
    let after = read_json(&baton);
    assert_eq!(after["checkpoint"], 0, "generate must not write the baton");
    assert_eq!(after["status"], "research_drafting");
    // The candidate is a separate file at the next checkpoint.
    assert_eq!(read_json(&candidate)["checkpoint"], 1);
}

// ===================== Fix 1: F1×F7×F9 amendment re-profile seam =====================

#[test]
fn f1_amendment_reprofile_exit_roundtrip() {
    // F1×F7×F9 seam: a plan amendment opened from a STANDARD phase 2, whose spec
    // revision re-profiles phase 3 to `full`, must be able to re-enter phase 3 as
    // `parallel_implementing`. Before the fix, legal_transitions pinned the
    // spec_review entry to amendment_from_phase's (standard) profile, so LIST
    // offered only `implementing` and `next --to parallel_implementing` failed with
    // illegal_target — while `next --to implementing --phase 3` self-failed 23
    // bad_amendment. The fix offers BOTH entry states across the reachable
    // re-entry phases, so the full re-entry roundtrips end to end.
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "spec_review", "prativadi", 4, |b| {
        common::standard_profile(b);
        b["run_workflow"]["source"] = Value::from("preset:standard");
        b["master_plan_locked"] = Value::Bool(true);
        b["total_phases"] = Value::from(3);
        b["amendment_from_phase"] = Value::from(2);
        b["phase_profiles"] = serde_json::json!({"3": "full"});
        // Seed the phase-3 parallel work_split so the generated candidate carries
        // the five two-team chunks parallel_implementing requires.
        common::parallel_chunks_phase(b, "3");
    });

    // LIST must offer BOTH entry states: `implementing` (phase 2 is standard) and
    // `parallel_implementing` (phase 3 is full).
    let (list_code, list_out, list_err) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(list_code, 0, "list exits 0\nstderr:\n{list_err}");
    assert!(
        list_out.contains("DVANDVA_NEXT parallel_implementing owner=team phase=advance"),
        "list offers the full re-entry state\n{list_out}"
    );
    assert!(
        list_out.contains("DVANDVA_NEXT implementing owner=vadi phase=advance"),
        "list still offers the standard re-entry state\n{list_out}"
    );

    // GENERATE the full re-entry at phase 3.
    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "parallel_implementing",
        "--phase",
        "3",
        "--summary",
        "Amended plan re-profiles phase 3 to full; re-entering parallel implementation.",
        "--next-action",
        "team: run the phase-3 parallel implementation chunks.",
    ]);
    assert_eq!(
        code, 0,
        "amendment re-profile exit generate exits 0\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("to=parallel_implementing checkpoint=5"),
        "{stdout}"
    );

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "parallel_implementing");
    assert_eq!(cand["assignee"], "team");
    assert_eq!(cand["phase"], 3);
    assert_eq!(
        cand["amendment_from_phase"],
        Value::Null,
        "the amendment exit nulls amendment_from_phase"
    );

    // Strongest property: the SAME binary's `write` ACCEPTS the generated file.
    run(&baton, &candidate).assert(
        "write accepts the amendment re-profile parallel_implementing candidate",
        0,
    );
    assert_eq!(read_json(&baton)["checkpoint"], 5);
}

#[test]
fn spec_entry_non_amendment_phase1_single_state() {
    // Guard (pairs with the seam fix): a non-amendment spec_review entry still
    // resolves to the SINGLE phase-1 entry state. A standard run offers only
    // `implementing`; the full entry state must not leak in.
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "spec_review", "prativadi", 4, |b| {
        common::standard_profile(b);
        b["run_workflow"]["source"] = Value::from("preset:standard");
        b["master_plan_locked"] = Value::Bool(true);
        b["total_phases"] = Value::from(1);
    });

    let (list_code, list_out, list_err) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(list_code, 0, "list exits 0\nstderr:\n{list_err}");
    assert!(
        list_out.contains("DVANDVA_NEXT implementing owner=vadi phase=advance"),
        "standard phase-1 entry offers implementing\n{list_out}"
    );
    assert!(
        !list_out.contains("parallel_implementing"),
        "no full entry state surfaces for a standard phase-1 run\n{list_out}"
    );

    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "implementing",
        "--phase",
        "1",
        "--summary",
        "Spec approved; entering phase-1 implementation.",
        "--next-action",
        "vadi: implement phase 1.",
    ]);
    assert_eq!(
        code, 0,
        "non-amendment entry generate exits 0\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("to=implementing checkpoint=5"), "{stdout}");
    run(&baton, &candidate).assert("write accepts the non-amendment implementing candidate", 0);
    assert_eq!(read_json(&baton)["checkpoint"], 5);
}

// ===================== Fix 2: human-resume scaffolding =====================

#[test]
fn human_question_resume_roundtrip() {
    // Fix 2b: from human_question, LIST offers the recorded resume edge (restore
    // resume_status/resume_assignee, clear the question/resume fields) and GENERATE
    // produces a candidate `dvandva write` ACCEPTS. Red before the fix:
    // legal_transitions offered no resume edge, so `next --to spec_review` was an
    // illegal_target.
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v3(&baton, "human_question", "human", 4, |b| {
        b["phase"] = Value::from("spec");
        b["question"] = Value::from("Should feature X be in scope?");
        b["resume_assignee"] = Value::from("prativadi");
        b["resume_status"] = Value::from("spec_review");
    });

    // LIST offers the recorded resume edge back to (spec_review, prativadi).
    let (list_code, list_out, list_err) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(list_code, 0, "list exits 0\nstderr:\n{list_err}");
    assert!(
        list_out.contains("DVANDVA_NEXT spec_review owner=prativadi phase=spec review_target=spec"),
        "human_question list offers the recorded resume edge\n{list_out}"
    );

    // GENERATE the resume.
    let (code, stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "spec_review",
        "--summary",
        "Human answered the scope question; resuming spec review.",
        "--next-action",
        "prativadi: resume the spec review with the human's answer.",
    ]);
    assert_eq!(
        code, 0,
        "human_question resume generate exits 0\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("to=spec_review checkpoint=5"), "{stdout}");

    let cand = read_json(&candidate);
    assert_eq!(cand["status"], "spec_review");
    assert_eq!(cand["assignee"], "prativadi");
    assert_eq!(cand["phase"], "spec");
    assert_eq!(cand["question"], Value::Null, "resume clears the question");
    assert_eq!(
        cand["resume_assignee"],
        Value::Null,
        "resume clears resume_assignee"
    );
    assert_eq!(
        cand["resume_status"],
        Value::Null,
        "resume clears resume_status"
    );

    // Strongest property: the SAME binary's `write` ACCEPTS the resume candidate.
    run(&baton, &candidate).assert("write accepts the human_question resume candidate", 0);
    assert_eq!(read_json(&baton)["checkpoint"], 5);
}

#[test]
fn human_decision_lists_human_resume_marker_hand_authored() {
    // Fix 2b: human_decision authorises ANY non-terminal resume, which `next`
    // cannot scaffold mechanically. LIST surfaces a `human_resume` marker so the
    // edge is honest, and generate rejects it with hand-authoring guidance.
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "human_decision", "human", 4, |b| {
        b["phase"] = Value::from("spec");
    });

    let (list_code, list_out, list_err) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(list_code, 0, "list exits 0\nstderr:\n{list_err}");
    assert!(
        list_out.contains("DVANDVA_NEXT human_resume owner=human phase=same"),
        "human_decision list surfaces the hand-authored resume marker\n{list_out}"
    );

    let (code, _stdout, stderr) = run_next(&[
        "--file",
        baton.to_str().unwrap(),
        "--to",
        "human_resume",
        "--summary",
        "x",
        "--next-action",
        "y",
    ]);
    assert_eq!(code, 2, "human_resume is not machine-generatable");
    assert!(
        stderr.contains("hand-authored") && stderr.contains("baton.next.json"),
        "the rejection guides toward hand-authoring the candidate file\n{stderr}"
    );
}

// ===================== S2-T1: abandoned surfaced by legal_transitions =====================

#[test]
fn s2t1_list_human_states_offer_abandoned() {
    // legal_transitions (the `next` LIST surface) must offer `abandoned` from both
    // human states with a human owner — the human declaring the run dead.
    let dir = tempfile::tempdir().unwrap();

    let hq = dir.path().join("hq.json");
    make_baton_v3(&hq, "human_question", "human", 4, |b| {
        b["phase"] = Value::from(1);
        b["question"] = Value::from("Continue?");
        b["resume_assignee"] = Value::from("vadi");
        b["resume_status"] = Value::from("implementing");
    });
    let (code, out, err) = run_next(&["--file", hq.to_str().unwrap()]);
    assert_eq!(code, 0, "list exits 0\n{err}");
    assert!(
        out.contains("DVANDVA_NEXT abandoned owner=human phase=same"),
        "human_question LIST offers abandoned\n{out}"
    );

    let hd = dir.path().join("hd.json");
    make_baton_v3(&hd, "human_decision", "human", 4, |b| {
        b["phase"] = Value::from(1);
    });
    let (code2, out2, err2) = run_next(&["--file", hd.to_str().unwrap()]);
    assert_eq!(code2, 0, "list exits 0\n{err2}");
    assert!(
        out2.contains("DVANDVA_NEXT abandoned owner=human phase=same"),
        "human_decision LIST offers abandoned\n{out2}"
    );
}

#[test]
fn s2t1_list_abandoned_is_terminal_no_options() {
    // From `abandoned`, the LIST surface offers no protocol transitions (only the
    // fixed over-approximation note) — abandoned is terminal like done.
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "abandoned", "human", 4, |b| {
        b["phase"] = Value::from(1);
    });
    let (code, out, err) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0, "list exits 0\n{err}");
    assert!(
        !out.contains("owner="),
        "abandoned offers no transitions\n{out}"
    );
    assert!(
        out.contains("DVANDVA_NEXT note content_gates_not_reflected"),
        "the fixed note still prints\n{out}"
    );
}

#[test]
fn s4t5_list_working_state_offers_human_question() {
    // S4-T5 (D1): the LIST surface stays coherent with the widened write path —
    // a post-lock working state offers human_question (owner human, same phase).
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v3(&baton, "implementing", "vadi", 4, |b| {
        b["phase"] = Value::from(1);
        b["master_plan_locked"] = Value::Bool(true);
    });
    let (code, out, err) = run_next(&["--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0, "list exits 0\n{err}");
    assert!(
        out.contains("DVANDVA_NEXT human_question owner=human phase=same"),
        "implementing LIST offers post-lock human_question\n{out}"
    );
}
