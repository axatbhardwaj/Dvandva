//! Integration tests for `dvandva next` — the candidate scaffolder (design §F1,
//! superpowers/specs/2026-07-02-flow-patches-design.html).
//!
//! Each test spawns the real `dvandva` binary against a fixture baton written
//! into a tempdir. Current batons are built with the shared `make_baton_v2`
//! fixture so a generated candidate inherits a fully-valid v2 field set; the
//! strongest property — `dvandva write` ACCEPTS the generated candidate — is
//! proven by spawning `write` (via `common::run`) on the emitted file.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::{make_baton_v2, run};
use serde_json::Value;

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

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read candidate")).expect("parse candidate")
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

// ===================== LIST mode =====================

#[test]
fn list_research_drafting_option_set() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    make_baton_v2(&baton, "research_drafting", "vadi", 0, |_| {});

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
    make_baton_v2(&baton, "research_drafting", "vadi", 0, |_| {});

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

// ===================== GENERATE mode =====================

#[test]
fn generate_research_review_happy_path_and_write_accepts_it() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v2(&baton, "research_drafting", "vadi", 0, |_| {});

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

    // Strongest property: the SAME binary's `write` accepts the generated file.
    run(&baton, &candidate).assert("write accepts the generated research_review candidate", 0);
    // ...and the baton is now installed at checkpoint 1.
    assert_eq!(read_json(&baton)["checkpoint"], 1);
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
    make_baton_v2(&baton, "research_drafting", "vadi", 0, |b| {
        b["mode"] = Value::from("review");
        b["phase"] = Value::from("review");
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
    make_baton_v2(&baton, "research_drafting", "vadi", 0, |b| {
        b["mode"] = Value::from("research");
        b["phase"] = Value::from("research");
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

    run(&baton, &candidate).assert(
        "write accepts the research-mode research_review candidate",
        0,
    );
    assert_eq!(read_json(&baton)["checkpoint"], 1);
}

#[test]
fn generate_loop_edge_increments_loop_counts() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v2(&baton, "deep_review", "prativadi", 4, |b| {
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
    // Full-profile deslop fixture (make_baton_v2 defaults to full profile).
    make_baton_v2(&baton, "deslop", "vadi", 6, |b| {
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
    make_baton_v2(&baton, "research_drafting", "vadi", 0, |_| {});

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
    make_baton_v2(&baton, "research_drafting", "vadi", 0, |_| {});

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
    make_baton_v2(&baton, "research_drafting", "vadi", 0, |_| {});

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
    make_baton_v2(&baton, "research_drafting", "vadi", 0, |_| {});

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
    make_baton_v2(&baton, "research_drafting", "vadi", 0, |_| {});

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
