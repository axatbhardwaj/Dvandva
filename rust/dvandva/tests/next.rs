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
fn research_mode_termination_review_roundtrip() {
    // H1' regression pin: research_review->termination_review is a PhaseMove::Same
    // edge (classify_phase_move only special-cases the research_*/spec_* planning
    // statuses and the implementing/parallel_implementing entry), but under
    // research mode phase_status_ok demands phase "spec" for termination_review
    // regardless of move class. Before the fix, GENERATE only consulted
    // expected_phase_for in the PhaseMove::Spec arm and PhaseMove::Same preserved
    // the current phase ("research"), so `dvandva write` rejected the candidate
    // with exit 23 bad_phase_status.
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let candidate = dir.path().join("baton.next.json");
    make_baton_v2(&baton, "research_review", "prativadi", 0, |b| {
        b["mode"] = Value::from("research");
        b["phase"] = Value::from("research");
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
    // The crux: research mode pins the candidate phase to "spec" for
    // termination_review, NOT the preserved "research".
    assert_eq!(
        cand["phase"], "spec",
        "research-mode candidate carries phase=spec, matching phase_status_ok"
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
    make_baton_v2(&baton, "spec_review", "prativadi", 4, |b| {
        common::standard_profile(b);
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
    make_baton_v2(&baton, "spec_review", "prativadi", 4, |b| {
        common::standard_profile(b);
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
    make_baton_v2(&baton, "human_question", "human", 4, |b| {
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
    make_baton_v2(&baton, "human_decision", "human", 4, |b| {
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
