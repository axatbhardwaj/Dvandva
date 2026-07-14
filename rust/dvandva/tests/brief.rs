//! Integration tests for `dvandva brief` (design §F2,
//! superpowers/specs/2026-07-02-flow-patches-design.html).
//!
//! Each test spawns the real `dvandva` binary against a fixture baton (plus,
//! where relevant, a sibling `history/` directory) written into a tempdir.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

/// Spawn `dvandva brief <args>`, clearing the selector env vars so the
/// resolved baton path is fully controlled by the test. Returns (exit code,
/// stdout, stderr).
fn run_brief(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(bin())
        .arg("brief")
        .args(args)
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_BATON_FILE")
        .env_remove("DVANDVA_RUN_DIR")
        .env_remove("DVANDVA_RUN_ID")
        .output()
        .expect("spawn dvandva brief");
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

fn base_baton() -> Value {
    json!({
        "schema": "dvandva.baton.v2",
        "run_id": "run-1",
        "mode": "development",
        "profile": "full",
        "phase": 2,
        "status": "parallel_implementing",
        "assignee": "team",
        "active_roles": ["vadi", "prativadi"],
        "checkpoint": 12,
        "disagreement_cap": 3,
        "loop_counts": {"phase_review:phase_fixing": 1},
        "plan_ref": "./superpowers/plans/x.html",
        "research_ref": "./superpowers/research/y.html",
        "review_ref": null,
        "run_explainer_ref": null,
        "work_split": [
            {"id": "vadi-2a", "phase": 2, "chunk_type": "implementation", "owner_role": "vadi",
             "status": "ready", "paths": ["a.rs"], "write_paths": ["a.rs"], "depends_on": ["root"]},
            {"id": "prativadi-2a", "phase": 2, "chunk_type": "implementation", "owner_role": "prativadi",
             "status": "ready", "paths": ["b.rs"]},
            {"id": "vadi-1a", "phase": 1, "chunk_type": "implementation", "owner_role": "vadi",
             "status": "completed", "paths": ["c.rs"]}
        ],
        "findings": [
            {"id": "F-1", "severity": "medium", "status": "open", "summary": "Open finding one."},
            {"id": "F-2", "severity": "low", "status": "Resolved", "summary": "Closed finding, case-insensitive."},
            "Bare string finding still open."
        ],
        "verification_matrix": [
            {"id": "verify-phase-2", "phase": 2, "command": "cargo test foo", "result": "pending"},
            {"id": "verify-phase-1", "phase": 1, "command": "cargo test bar", "result": "passed"}
        ],
        "next_action": "Write the outbound transition."
    })
}

// ── usage errors ──────────────────────────────────────────────────────────

#[test]
fn missing_role_is_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    write_json(&baton, &base_baton());

    let (code, _out, err) = run_brief(&["--file", baton.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(err.contains("Usage"));
}

#[test]
fn unknown_flag_is_usage_error() {
    let (code, _out, err) = run_brief(&["--role", "vadi", "--bogus", "x"]);
    assert_eq!(code, 2);
    assert!(err.contains("Usage"));
}

#[test]
fn invalid_role_value_is_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    write_json(&baton, &base_baton());

    let (code, _out, err) = run_brief(&["--role", "team", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(err.contains("--role must be vadi or prativadi"));
}

// ── read errors ───────────────────────────────────────────────────────────

#[test]
fn missing_baton_exits_21() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("nope.json");

    let (code, _out, _err) = run_brief(&["--role", "vadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 21);
}

#[test]
fn invalid_json_baton_exits_22() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    fs::write(&baton, "{ not json").unwrap();

    let (code, _out, _err) = run_brief(&["--role", "vadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 22);
}

// ── happy path ────────────────────────────────────────────────────────────

#[test]
fn happy_path_renders_all_sections_with_filtering() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    write_json(&baton, &base_baton());

    let (code, out, _err) = run_brief(&["--role", "vadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0, "stderr should be empty on success");

    // Title + header.
    assert!(out.contains("# Dvandva brief — run-1 (vadi)"));
    assert!(out.contains("mode: development"));
    assert!(out.contains("run profile: full"));
    // phase_profiles absent -> effective profile falls back to run profile.
    assert!(out.contains("effective profile (phase 2): full"));
    assert!(out.contains("phase: 2"));
    assert!(out.contains("status: parallel_implementing"));
    assert!(out.contains("assignee: team"));
    assert!(out.contains("active_roles: vadi,prativadi"));
    assert!(out.contains("checkpoint: 12"));
    assert!(out.contains("disagreement_cap: 3"));
    assert!(out.contains("loop phase_review:phase_fixing: 1/3"));

    // Artifacts: only non-null refs listed, in the fixed order.
    assert!(out.contains("## Read these artifacts"));
    assert!(out.contains("plan_ref: ./superpowers/plans/x.html"));
    assert!(out.contains("research_ref: ./superpowers/research/y.html"));
    assert!(!out.contains("- review_ref:"));
    assert!(!out.contains("- run_explainer_ref:"));

    // Work: role filter (vadi only) AND phase filter (phase 2 only).
    assert!(out.contains("## Your work (phase 2)"));
    assert!(out.contains("vadi-2a"));
    assert!(!out.contains("prativadi-2a"));
    assert!(!out.contains("vadi-1a"));

    // Findings: open + bare-string included; case-insensitive resolved excluded.
    assert!(out.contains("## Open findings"));
    assert!(out.contains("F-1"));
    assert!(out.contains("Open finding one."));
    assert!(!out.contains("F-2"));
    assert!(!out.contains("Closed finding"));
    assert!(out.contains("Bare string finding still open."));

    // Verification matrix: phase filter, id fallback label, command fallback planned check.
    assert!(out.contains("## Verification matrix (phase 2)"));
    assert!(out.contains("[pending] verify-phase-2: cargo test foo"));
    assert!(!out.contains("verify-phase-1"));

    // Next action.
    assert!(out.contains("## Next action"));
    assert!(out.contains("Write the outbound transition."));
}

#[test]
fn effective_profile_uses_phase_profiles_entry_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let mut fixture = base_baton();
    fixture["phase_profiles"] = json!({"2": "standard"});
    write_json(&baton, &fixture);

    let (code, out, _err) = run_brief(&["--role", "vadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains("run profile: full"));
    assert!(out.contains("effective profile (phase 2): standard"));
}

#[test]
fn effective_profile_falls_back_when_phase_profiles_entry_absent() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let mut fixture = base_baton();
    // Entry exists only for phase 3; current phase is 2.
    fixture["phase_profiles"] = json!({"3": "standard"});
    write_json(&baton, &fixture);

    let (code, out, _err) = run_brief(&["--role", "vadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains("run profile: full"));
    assert!(out.contains("effective profile (phase 2): full"));
}

#[test]
fn effective_profile_falls_back_when_phase_profiles_is_falsy() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let mut fixture = base_baton();
    fixture["phase_profiles"] = json!(false);
    write_json(&baton, &fixture);

    let (code, out, _err) = run_brief(&["--role", "vadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains("run profile: full"));
    assert!(out.contains("effective profile (phase 2): full"));
}

#[test]
fn role_filter_prativadi_sees_only_its_own_chunk() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    write_json(&baton, &base_baton());

    let (code, out, _err) = run_brief(&["--role", "prativadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains("- prativadi-2a:"));
    // "vadi-2a" is a substring of "prativadi-2a", so anchor on the bullet
    // prefix to assert the vadi-owned chunk itself is absent.
    assert!(!out.contains("- vadi-2a:"));
}

// ── history ───────────────────────────────────────────────────────────────

#[test]
fn history_shows_last_five_sorted_by_checkpoint_with_truncated_summary() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    write_json(&baton, &base_baton());

    let history_dir = dir.path().join("history");
    // Checkpoints run through two digits (0..=10) so the ascending sort is
    // pinned as numeric, not lexicographic (which would sort "10" before
    // "6").
    for checkpoint in 0..=10 {
        let long_summary = if checkpoint == 10 {
            "x".repeat(200)
        } else {
            format!("checkpoint {checkpoint} summary")
        };
        write_json(
            &history_dir.join(format!("{checkpoint}-implementing-vadi.json")),
            &json!({"status": "implementing", "assignee": "vadi", "summary": long_summary}),
        );
    }

    let (code, out, _err) = run_brief(&["--role", "vadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0);

    assert!(out.contains("## Recent checkpoints"));
    // Only the last 5 (checkpoints 6..10), oldest-of-the-five first.
    assert!(!out.contains("cp0 "));
    assert!(!out.contains("cp5 "));
    assert!(out.contains("cp6 implementing vadi"));
    assert!(out.contains("cp9 implementing vadi"));
    assert!(out.contains("cp10 implementing vadi"));

    let cp6_pos = out.find("cp6 implementing vadi").unwrap();
    let cp9_pos = out.find("cp9 implementing vadi").unwrap();
    let cp10_pos = out.find("cp10 implementing vadi").unwrap();
    assert!(
        cp6_pos < cp9_pos && cp9_pos < cp10_pos,
        "checkpoints should render in ascending numeric order (cp9 before cp10, not lexicographic)"
    );

    // Summary truncated to 160 chars with a "..." marker.
    assert!(out.contains(&"x".repeat(160)));
    assert!(!out.contains(&"x".repeat(161)));
    assert!(out.contains("..."));
}

#[test]
fn history_skips_no_clobber_dup_files() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    write_json(&baton, &base_baton());

    let history_dir = dir.path().join("history");
    for checkpoint in 7..=12 {
        write_json(
            &history_dir.join(format!("{checkpoint}-implementing-vadi.json")),
            &json!({
                "status": "implementing",
                "assignee": "vadi",
                "summary": format!("checkpoint {checkpoint} summary")
            }),
        );
    }
    // A snapshot no-clobber duplicate for cp12 (`src/snapshot.rs` writes
    // `<target-stem>.dup-<epoch-ns>.json` on a byte-differing collision) —
    // must not be treated as a second canonical history entry.
    write_json(
        &history_dir.join("12-implementing-vadi.dup-172837465923847.json"),
        &json!({
            "status": "implementing",
            "assignee": "vadi",
            "summary": "garbled dup content"
        }),
    );

    let (code, out, _err) = run_brief(&["--role", "vadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0);

    // Exactly one cp12 row, carrying the real file's summary.
    assert_eq!(out.matches("cp12 ").count(), 1);
    assert!(out.contains("cp12 implementing vadi — checkpoint 12 summary"));
    assert!(!out.contains("garbled dup content"));
    assert!(
        !out.contains("dup-"),
        "dup marker must not leak into rendered history"
    );

    // The dup file must not evict the true oldest-of-last-5 (cp8).
    assert!(out.contains("cp8 implementing vadi"));
    assert!(!out.contains("cp7 "));
}

// ── verification_matrix fallback shapes ─────────────────────────────────

#[test]
fn verification_matrix_object_shape_includes_all_rows_with_note() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let mut fixture = base_baton();
    fixture["verification_matrix"] = json!({
        "row-a": {"id": "row-a", "phase": 1, "command": "echo a", "result": "passed"}
    });
    write_json(&baton, &fixture);

    let (code, out, _err) = run_brief(&["--role", "vadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0);
    // Included despite phase 1 != current phase 2, because the matrix is an
    // object rather than an array of phase-tagged rows.
    assert!(out.contains("row-a"));
    assert!(out.contains("_note:"));
}

// ── next_action ───────────────────────────────────────────────────────────

#[test]
fn next_action_object_with_all_values_falsy_renders_none() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    let mut fixture = base_baton();
    fixture["next_action"] = json!({"a": null, "b": false});
    write_json(&baton, &fixture);

    let (code, out, _err) = run_brief(&["--role", "vadi", "--file", baton.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(
        out.ends_with("## Next action\n\n_none_\n"),
        "all-falsy next_action object should render _none_, got: {out:?}"
    );
}

// ── --out ─────────────────────────────────────────────────────────────────

#[test]
fn out_flag_writes_file() {
    let dir = tempfile::tempdir().unwrap();
    let baton = dir.path().join("baton.json");
    write_json(&baton, &base_baton());
    let out_file = dir.path().join("brief.md");

    let (code, stdout, _err) = run_brief(&[
        "--role",
        "vadi",
        "--file",
        baton.to_str().unwrap(),
        "--out",
        out_file.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    assert!(
        stdout.is_empty(),
        "markdown should go to the file, not stdout"
    );

    let written = fs::read_to_string(&out_file).unwrap();
    assert!(written.contains("# Dvandva brief — run-1 (vadi)"));
    assert!(written.contains("## Next action"));
}
