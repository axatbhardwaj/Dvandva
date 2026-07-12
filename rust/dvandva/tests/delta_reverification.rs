//! Integration coverage for Option B delta re-verification (VM-1 through VM-17).

mod common;

use common::*;
use dvandva::{provenance, reverify};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn paths(dir: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    (
        dir.path().join("baton.json"),
        dir.path().join("baton.next.json"),
    )
}

fn write_json(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn load(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn git_ok(repo: &Path, args: &[&str]) {
    let out = dvandva::gitcfg::git(repo, args).unwrap();
    assert!(
        out.status.success(),
        "git {} failed:\n{}{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn head(repo: &Path) -> String {
    dvandva::gitcfg::git_stdout(repo, &["rev-parse", "HEAD"]).unwrap()
}

fn committed_repo_at(dir: &Path, files: &[(&str, &str)]) -> String {
    git_ok(dir, &["init", "-q"]);
    git_ok(dir, &["config", "user.name", "Dvandva Test"]);
    git_ok(dir, &["config", "user.email", "dvandva@example.test"]);
    for (name, contents) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }
    git_ok(dir, &["add", "--all"]);
    git_ok(dir, &["commit", "-q", "-m", "origin"]);
    head(dir)
}

fn write_snapshot(dir: &Path, checkpoint: i64, status: &str, phase: &str, extra: Value) {
    let mut snapshot = json!({
        "schema": "dvandva.baton.v3",
        "checkpoint": checkpoint,
        "status": status,
        "assignee": "team",
        "phase": phase,
        "verification_matrix": [],
        "subagent_tracks": [],
        "work_split": [],
        "findings": []
    });
    if let (Some(target), Some(fields)) = (snapshot.as_object_mut(), extra.as_object()) {
        target.extend(fields.clone());
    }
    write_json(
        &dir.join(format!("history/{checkpoint}-{status}-team.json")),
        &snapshot,
    );
}

fn bounded_chunk(id: &str, write_paths: &[&str], read_paths: &[&str]) -> Value {
    json!({
        "id": id,
        "phase": "1",
        "chunk_type": "implementation",
        "owner": "vadi",
        "owner_role": "vadi",
        "scope": "Bounded delta re-verification fixture.",
        "paths": write_paths,
        "write_paths": write_paths,
        "read_paths": read_paths,
        "cross_review_by": "prativadi",
        "can_parallelize": false,
        "parallel_rationale": "Single deterministic fixture chunk.",
        "depends_on": [],
        "status": "completed",
        "artifact_refs": []
    })
}

fn origin_track(id: &str, digest: &str, covered_paths: &[&str]) -> Value {
    json!({
        "id": id,
        "phase": "test_creation",
        "status": "completed",
        "track": "test-creation",
        "owner": "dvandva-test-creator",
        "owner_role": "vadi",
        "parallelized": false,
        "rationale": "Direct origin execution.",
        "inputs": ["bounded inputs"],
        "outputs": ["tests passed"],
        "evidence_refs": ["test:origin"],
        "result": "passed",
        "covered_input_digest": digest,
        "digest_algo": "git-covers-diff-v1",
        "covered_paths": covered_paths
    })
}

fn carried_track(
    id: &str,
    origin: i64,
    digest: &str,
    covered_paths: &[&str],
    covers_chunks: &[&str],
) -> Value {
    let mut track = origin_track(id, digest, covered_paths);
    track["carried_from_checkpoint"] = json!(origin);
    track["carry_reason"] = json!("Covered inputs are unchanged from the origin pass.");
    track["covers_chunks"] = json!(covers_chunks);
    track
}

fn direct_bounded_track(id: &str, digest: &str, path: &str) -> Value {
    let mut track = origin_track(id, digest, &[path]);
    track["covers_chunks"] = json!(["X"]);
    track
}

fn legacy_or_unbounded_track(id: &str, opted_in: bool) -> Value {
    let mut track = json!({
        "id": id,
        "phase": "test_creation",
        "status": "completed",
        "track": "test-creation",
        "owner": "dvandva-test-creator",
        "owner_role": "vadi",
        "parallelized": false,
        "rationale": "Deterministic test-creation evidence.",
        "inputs": ["implementation evidence"],
        "outputs": ["tests passed"],
        "evidence_refs": ["test:evidence"],
        "result": "passed"
    });
    if opted_in {
        track["covers_chunks"] = json!([]);
    }
    track
}

fn decide_baton(unit: Value, chunks: Value) -> Value {
    json!({
        "checkpoint": 10,
        "status": "test_creation",
        "phase": "1",
        "work_split": chunks,
        "findings": [],
        "subagent_tracks": [unit]
    })
}

fn write_origin_track_snapshot(
    dir: &Path,
    checkpoint: i64,
    phase: &str,
    track: Value,
    chunks: Value,
) {
    write_snapshot(
        dir,
        checkpoint,
        "test_creation",
        phase,
        json!({"subagent_tracks": [track], "work_split": chunks}),
    );
}

fn configure_bounded_work_split(v: &mut Value, path: &str) {
    v["work_split"] = json!([bounded_chunk("X", &[path], &[])]);
    v["findings"] = json!([]);
}

fn configure_terminal_current(v: &mut Value) {
    v["active_roles"] = json!(["vadi", "prativadi"]);
    v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
    v["vadi_final_approval"] = json!(true);
    v["prativadi_final_approval"] = json!(true);
    run_explainer_reviews(v);
}

fn configure_done_candidate(v: &mut Value) {
    v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
    v["vadi_final_approval"] = json!(true);
    v["prativadi_final_approval"] = json!(true);
    run_explainer_reviews(v);
    explainer_verification_track(v);
    done_matrix_fresh(v);
}

fn write_impl_anchor(dir: &Path, checkpoint: i64, phase: &str) {
    write_snapshot(dir, checkpoint, "phase_fixing", phase, json!({}));
}

// ===== VM-1: covered diff forces replay ==================================

/// Proves that dirty bytes inside the engine-derived closure force a re-run.
#[test]
fn vm01_diff_intersects_rerun() {
    let d = tmp();
    let anchor = committed_repo_at(d.path(), &[("src/covered.rs", "origin\n")]);
    let origin = origin_track("delta-test", &anchor, &["src/covered.rs"]);
    let candidate = carried_track("delta-test", 5, &anchor, &["src/covered.rs"], &["X"]);
    let chunks = json!([bounded_chunk("X", &["src/covered.rs"], &[])]);
    write_origin_track_snapshot(d.path(), 5, "1", origin, chunks.clone());
    let baton = decide_baton(candidate.clone(), chunks);

    std::fs::write(d.path().join("src/covered.rs"), "dirty working tree\n").unwrap();

    assert_eq!(
        reverify::decide(
            &baton,
            d.path(),
            &candidate,
            "subagent_track",
            10,
            "1",
            d.path(),
        ),
        reverify::Decision::ReRun
    );
}

// ===== VM-2: disjoint, valid carry =======================================

/// Proves a bounded provenance-valid carry is decided and honored at the sole carry gate.
#[test]
fn vm02_disjoint_valid_carry() {
    let d = tmp();
    let (b, n) = paths(&d);
    let anchor = committed_repo_at(
        d.path(),
        &[("src/covered.rs", "covered\n"), ("src/fixed.rs", "fixed\n")],
    );
    let origin = origin_track("delta-E-test-creation", &anchor, &["src/covered.rs"]);
    let candidate_track = carried_track(
        "delta-E-test-creation",
        4,
        &anchor,
        &["src/covered.rs"],
        &["X"],
    );
    write_origin_track_snapshot(
        d.path(),
        4,
        "1",
        origin,
        json!([bounded_chunk("X", &["src/covered.rs"], &[])]),
    );
    write_impl_anchor(d.path(), 5, "1");
    make_baton_v3(&b, "test_creation", "team", 6, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        configure_bounded_work_split(v, "src/covered.rs");
    });
    make_baton_v3(&n, "cross_review", "team", 7, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        configure_bounded_work_split(v, "src/covered.rs");
        push(v, "subagent_tracks", candidate_track.clone());
    });

    let candidate_baton = load(&n);
    let unit = candidate_baton["subagent_tracks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "delta-E-test-creation")
        .unwrap();
    assert_eq!(
        reverify::decide(
            &candidate_baton,
            d.path(),
            unit,
            "subagent_track",
            6,
            "1",
            d.path(),
        ),
        reverify::Decision::Carry
    );
    run(&b, &n).assert("vm02 valid carried test_creation track", 0);
}

// ===== VM-3: findings block carry ========================================

/// Proves both overlapping and pathless open findings block carry before provenance or Git.
#[test]
fn vm03_open_finding_blocks_carry() {
    let candidate = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);

    let mut overlapping = decide_baton(candidate.clone(), chunks.clone());
    overlapping["findings"] = json!([{"id": "f-overlap", "status": "open", "paths": ["src/a.rs"]}]);
    assert_eq!(
        reverify::decide(
            &overlapping,
            Path::new("/no-fs-needed"),
            &candidate,
            "subagent_track",
            10,
            "1",
            Path::new("/no-git-needed"),
        ),
        reverify::Decision::ReRun
    );

    let mut pathless = decide_baton(candidate.clone(), chunks);
    pathless["findings"] = json!([{"id": "f-global", "status": "open"}]);
    assert_eq!(
        reverify::decide(
            &pathless,
            Path::new("/no-fs-needed"),
            &candidate,
            "subagent_track",
            10,
            "1",
            Path::new("/no-git-needed"),
        ),
        reverify::Decision::ReRun
    );
}

// ===== VM-4: global/unbounded units ======================================

/// Proves explicit-global, wildcard, and dangling-closure claims never carry.
#[test]
fn vm04_global_unbounded_never_carries() {
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    let mut global = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    global["global"] = json!(true);
    let mut wildcard = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    wildcard["covers"] = json!(["*"]);
    let dangling = carried_track("t", 5, "anchor", &["src/a.rs"], &["missing"]);

    for unit in [global, wildcard, dangling] {
        let baton = decide_baton(unit.clone(), chunks.clone());
        assert_eq!(
            reverify::decide(
                &baton,
                Path::new("/no-fs-needed"),
                &unit,
                "subagent_track",
                10,
                "1",
                Path::new("/no-git-needed"),
            ),
            reverify::Decision::ReRun
        );
    }
}

// ===== VM-5: provenance fails closed =====================================

/// Proves self/future, out-of-cycle, missing-origin, and carry-of-carry claims all re-run.
#[test]
fn vm05_provenance_invalid_failclosed() {
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);

    for invalid_origin in [10, 11] {
        let d = tmp();
        let unit = carried_track("t", invalid_origin, "anchor", &["src/a.rs"], &["X"]);
        let baton = decide_baton(unit.clone(), chunks.clone());
        assert_eq!(
            reverify::decide(&baton, d.path(), &unit, "subagent_track", 10, "1", d.path(),),
            reverify::Decision::ReRun
        );
    }

    let out_of_cycle = tmp();
    write_origin_track_snapshot(
        out_of_cycle.path(),
        2,
        "1",
        origin_track("t", "anchor", &["src/a.rs"]),
        chunks.clone(),
    );
    write_snapshot(out_of_cycle.path(), 3, "phase_fixing", "2", json!({}));
    write_snapshot(out_of_cycle.path(), 6, "phase_fixing", "1", json!({}));
    let stale = carried_track("t", 2, "anchor", &["src/a.rs"], &["X"]);
    let baton = decide_baton(stale.clone(), chunks.clone());
    assert_eq!(
        reverify::decide(
            &baton,
            out_of_cycle.path(),
            &stale,
            "subagent_track",
            10,
            "1",
            out_of_cycle.path(),
        ),
        reverify::Decision::ReRun
    );

    let missing = tmp();
    let absent = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    let baton = decide_baton(absent.clone(), chunks.clone());
    assert_eq!(
        reverify::decide(
            &baton,
            missing.path(),
            &absent,
            "subagent_track",
            10,
            "1",
            missing.path(),
        ),
        reverify::Decision::ReRun
    );

    let carried_origin = tmp();
    let mut origin = origin_track("t", "anchor", &["src/a.rs"]);
    origin["carried_from_checkpoint"] = json!(2);
    write_origin_track_snapshot(carried_origin.path(), 5, "1", origin, chunks.clone());
    let laundering = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    let baton = decide_baton(laundering.clone(), chunks);
    assert_eq!(
        reverify::decide(
            &baton,
            carried_origin.path(),
            &laundering,
            "subagent_track",
            10,
            "1",
            carried_origin.path(),
        ),
        reverify::Decision::ReRun
    );
}

// ===== VM-6: committed Git drift =========================================

/// Proves a later commit touching a covered path invalidates the stored commit anchor.
#[test]
fn vm06_git_diff_drift_failclosed() {
    let d = tmp();
    let anchor = committed_repo_at(d.path(), &[("src/covered.rs", "origin\n")]);
    let chunks = json!([bounded_chunk("X", &["src/covered.rs"], &[])]);
    write_origin_track_snapshot(
        d.path(),
        5,
        "1",
        origin_track("t", &anchor, &["src/covered.rs"]),
        chunks.clone(),
    );
    std::fs::write(d.path().join("src/covered.rs"), "second commit\n").unwrap();
    git_ok(d.path(), &["add", "src/covered.rs"]);
    git_ok(d.path(), &["commit", "-q", "-m", "covered drift"]);
    let unit = carried_track("t", 5, &anchor, &["src/covered.rs"], &["X"]);
    let baton = decide_baton(unit.clone(), chunks);

    assert_eq!(
        reverify::decide(&baton, d.path(), &unit, "subagent_track", 10, "1", d.path(),),
        reverify::Decision::ReRun
    );
}

// ===== VM-7: legacy first pass ===========================================

/// Proves legacy test evidence remains byte-identical while the terminal matrix gate stays strict.
#[test]
fn vm07_first_pass_legacy_byte_equal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v3(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(
            v,
            "subagent_tracks",
            legacy_or_unbounded_track("legacy-test", false),
        );
    });
    let candidate_bytes = std::fs::read(&n).unwrap();
    run(&b, &n).assert("vm07 legacy test_creation track", 0);
    assert_eq!(std::fs::read(&b).unwrap(), candidate_bytes);

    seed_done_artifacts(d.path());
    write_impl_anchor(d.path(), 3, "1");
    make_baton_v3(
        &b,
        "termination_review",
        "team",
        6,
        configure_terminal_current,
    );
    make_baton_v3(&n, "done", "team", 7, |v| {
        configure_done_candidate(v);
        let explainer = v["subagent_tracks"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|t| t["id"] == "explainer-verification-evidence")
            .unwrap();
        explainer["review_checkpoint"] = json!(6);
        let row = &mut v["verification_matrix"].as_array_mut().unwrap()[0];
        row["evidence_checkpoint"] = json!(0);
    });
    run(&b, &n).assert_contains(
        "vm07 terminal matrix remains strict",
        23,
        "stale_verification_matrix",
    );
}

// ===== VM-8: transitive closure ==========================================

/// Proves depends-on and conflict-group peers contribute their own declared paths.
#[test]
fn vm08_transitive_closure_completeness() {
    let baton = json!({
        "work_split": [
            {
                "id": "X", "write_paths": ["src/x.rs"], "read_paths": ["src/shared.rs"],
                "depends_on": ["Y"], "conflict_group": "g"
            },
            {
                "id": "Y", "write_paths": ["src/y.rs"], "read_paths": ["src/y-input.rs"],
                "depends_on": [], "conflict_group": ""
            },
            {
                "id": "Z", "write_paths": ["src/z.rs"], "read_paths": [],
                "depends_on": [], "conflict_group": "g"
            }
        ]
    });
    let unit = json!({"id": "t", "covers_chunks": ["X"]});
    let actual = reverify::derive_covered_closure(&baton, &unit).unwrap();
    let expected: BTreeSet<String> = [
        "src/x.rs",
        "src/shared.rs",
        "src/y.rs",
        "src/y-input.rs",
        "src/z.rs",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(actual, expected);
}

// ===== VM-9: engine-derived covers =======================================

/// Proves every non-derivable closure shape fails closed through both derivation and decide.
#[test]
fn vm09_engine_derived_covers_failclosed() {
    let cases = [
        (json!([]), json!([])),
        (
            json!([bounded_chunk("X", &["src/x.rs"], &[])]),
            json!(["missing"]),
        ),
        (json!([{"id": "X", "paths": ["src/x.rs"]}]), json!(["X"])),
        (
            json!([{"id": "X", "write_paths": ["/abs/x"], "read_paths": []}]),
            json!(["X"]),
        ),
        (
            json!([{"id": "X", "write_paths": ["../up.rs"], "read_paths": []}]),
            json!(["X"]),
        ),
        (
            json!([{"id": "X", "write_paths": ["src/*.rs"], "read_paths": []}]),
            json!(["X"]),
        ),
    ];

    for (chunks, covers) in cases {
        let mut unit = carried_track("t", 5, "anchor", &["src/x.rs"], &[]);
        unit["covers_chunks"] = covers;
        let baton = decide_baton(unit.clone(), chunks);
        assert_eq!(reverify::derive_covered_closure(&baton, &unit), None);
        assert_eq!(
            reverify::decide(
                &baton,
                Path::new("/no-fs-needed"),
                &unit,
                "subagent_track",
                10,
                "1",
                Path::new("/no-git-needed"),
            ),
            reverify::Decision::ReRun
        );
    }
}

// ===== VM-10: tracked regular-file binding ===============================

/// Proves untracked files and tracked symlinks cannot satisfy Git closure binding.
#[test]
fn vm10_git_covers_diff_binding() {
    let untracked = tmp();
    let anchor = committed_repo_at(untracked.path(), &[("seed.txt", "seed\n")]);
    std::fs::write(untracked.path().join("untracked.rs"), "untracked\n").unwrap();
    let chunks = json!([bounded_chunk("X", &["untracked.rs"], &[])]);
    write_origin_track_snapshot(
        untracked.path(),
        5,
        "1",
        origin_track("t", &anchor, &["untracked.rs"]),
        chunks.clone(),
    );
    let unit = carried_track("t", 5, &anchor, &["untracked.rs"], &["X"]);
    let baton = decide_baton(unit.clone(), chunks);
    assert_eq!(
        reverify::decide(
            &baton,
            untracked.path(),
            &unit,
            "subagent_track",
            10,
            "1",
            untracked.path(),
        ),
        reverify::Decision::ReRun
    );

    #[cfg(unix)]
    {
        let symlink = tmp();
        committed_repo_at(symlink.path(), &[("target.rs", "target\n")]);
        std::os::unix::fs::symlink("target.rs", symlink.path().join("link.rs")).unwrap();
        git_ok(symlink.path(), &["add", "link.rs"]);
        git_ok(symlink.path(), &["commit", "-q", "-m", "tracked symlink"]);
        let anchor = head(symlink.path());
        let chunks = json!([bounded_chunk("X", &["link.rs"], &[])]);
        write_origin_track_snapshot(
            symlink.path(),
            5,
            "1",
            origin_track("t", &anchor, &["link.rs"]),
            chunks.clone(),
        );
        let unit = carried_track("t", 5, &anchor, &["link.rs"], &["X"]);
        let baton = decide_baton(unit.clone(), chunks);
        assert_eq!(
            reverify::decide(
                &baton,
                symlink.path(),
                &unit,
                "subagent_track",
                10,
                "1",
                symlink.path(),
            ),
            reverify::Decision::ReRun
        );
    }
}

// ===== VM-11: engine stamp lifecycle =====================================

/// Proves direct stamps bind to HEAD, carried stamps bind to origin, and both valid modes pass.
#[test]
fn vm11_engine_stamped_direct_vs_carry() {
    fn direct_case(digest_override: Option<&str>) -> Out {
        let d = tmp();
        let (b, n) = paths(&d);
        let anchor = committed_repo_at(d.path(), &[("src/x.rs", "x\n")]);
        let current = direct_bounded_track("direct", &anchor, "src/x.rs");
        let candidate_digest = digest_override.unwrap_or(&anchor).to_string();
        let candidate = direct_bounded_track("direct", &candidate_digest, "src/x.rs");
        make_baton_v3(&b, "test_creation", "team", 5, |v| {
            v["active_roles"] = json!(["vadi", "prativadi"]);
            configure_bounded_work_split(v, "src/x.rs");
            push(v, "subagent_tracks", current);
        });
        write_json(
            &d.path().join("history/5-test_creation-team.json"),
            &load(&b),
        );
        make_baton_v3(&n, "cross_review", "team", 6, |v| {
            v["active_roles"] = json!(["vadi", "prativadi"]);
            configure_bounded_work_split(v, "src/x.rs");
            push(v, "subagent_tracks", candidate);
        });
        run(&b, &n)
    }

    fn carried_case(forged: bool) -> Out {
        let d = tmp();
        let (b, n) = paths(&d);
        let anchor = committed_repo_at(d.path(), &[("src/x.rs", "x\n")]);
        let origin = origin_track("carried", &anchor, &["src/x.rs"]);
        write_origin_track_snapshot(
            d.path(),
            4,
            "1",
            origin,
            json!([bounded_chunk("X", &["src/x.rs"], &[])]),
        );
        write_impl_anchor(d.path(), 5, "1");
        make_baton_v3(&b, "test_creation", "team", 6, |v| {
            v["active_roles"] = json!(["vadi", "prativadi"]);
            configure_bounded_work_split(v, "src/x.rs");
        });
        let digest = if forged {
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
        } else {
            &anchor
        };
        make_baton_v3(&n, "cross_review", "team", 7, |v| {
            v["active_roles"] = json!(["vadi", "prativadi"]);
            configure_bounded_work_split(v, "src/x.rs");
            push(
                v,
                "subagent_tracks",
                carried_track("carried", 4, digest, &["src/x.rs"], &["X"]),
            );
        });
        run(&b, &n)
    }

    direct_case(Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")).assert_contains(
        "vm11 forged direct stamp",
        23,
        "forged_test_creation_stamp id=direct mode=direct reason=triple_mismatch",
    );
    carried_case(true).assert_contains(
        "vm11 forged carried stamp",
        23,
        "forged_test_creation_stamp id=carried mode=carried reason=triple_mismatch",
    );
    direct_case(None).assert("vm11 correct direct stamp", 0);
    carried_case(false).assert("vm11 correct carried stamp", 0);
}

// ===== VM-12: re-lap ancestry ============================================

/// Proves phase lineage, not status continuity, defines valid re-lap ancestry.
#[test]
fn vm12_relap_ancestry_validity() {
    let d = tmp();
    write_snapshot(d.path(), 2, "test_creation", "1", json!({}));
    write_snapshot(d.path(), 3, "parallel_implementing", "2", json!({}));
    write_snapshot(d.path(), 4, "phase_fixing", "1", json!({}));
    write_snapshot(d.path(), 5, "human_question", "1", json!({}));
    write_snapshot(d.path(), 6, "test_creation", "1", json!({}));
    let cur = json!({"checkpoint": 8, "status": "cross_review", "phase": "1"});

    assert!(!provenance::on_current_cycle_ancestry(
        d.path(),
        &cur,
        8,
        "1",
        8
    ));
    assert!(!provenance::on_current_cycle_ancestry(
        d.path(),
        &cur,
        8,
        "1",
        9
    ));
    assert!(!provenance::on_current_cycle_ancestry(
        d.path(),
        &cur,
        8,
        "1",
        2
    ));
    assert!(!provenance::on_current_cycle_ancestry(
        d.path(),
        &cur,
        8,
        "1",
        3
    ));
    assert!(!provenance::on_current_cycle_ancestry(
        d.path(),
        &cur,
        8,
        "1",
        7
    ));
    assert_eq!(
        provenance::current_phase_cycle_start(d.path(), &cur, 8, "1"),
        4
    );
    assert!(provenance::on_current_cycle_ancestry(
        d.path(),
        &cur,
        8,
        "1",
        4
    ));
}

// ===== VM-13: matrix lost-update protection ==============================

/// Proves team-sync writes must retain every installed array-matrix row id.
#[test]
fn vm13_matrix_lost_update_protection() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(
            v,
            "verification_matrix",
            json!({"id": "vm-preserve", "current": "pending"}),
        );
    });
    make_baton_v3(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Retry that drops an installed matrix row.");
        v["next_action"] = json!("Team must retain vm-preserve.");
    });
    run(&b, &n).assert_contains(
        "vm13 dropped verification matrix row",
        23,
        "lost_update field=verification_matrix missing=vm-preserve",
    );

    make_baton_v3(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Retry that retains and grows the matrix.");
        v["next_action"] = json!("Continue test creation.");
        push(
            v,
            "verification_matrix",
            json!({"id": "vm-preserve", "current": "pending"}),
        );
        push(
            v,
            "verification_matrix",
            json!({"id": "vm-grown", "current": "pending"}),
        );
    });
    run(&b, &n).assert("vm13 matrix id superset accepted", 0);
}

// ===== VM-14: test-creation cycle scoping ================================

/// Proves an opted-in test track first completed before the implementation anchor is stale.
#[test]
fn vm14_test_creation_cycle_scoping() {
    let d = tmp();
    let (b, n) = paths(&d);
    let stale = legacy_or_unbounded_track("stale-test", true);
    write_snapshot(
        d.path(),
        3,
        "test_creation",
        "1",
        json!({"subagent_tracks": [stale.clone()]}),
    );
    write_impl_anchor(d.path(), 5, "1");
    make_baton_v3(&b, "test_creation", "team", 6, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v3(&n, "cross_review", "team", 7, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(v, "subagent_tracks", stale);
    });
    run(&b, &n).assert_contains(
        "vm14 pre-anchor test track",
        24,
        "test_creation->cross_review requires completed test-creation subagent_track",
    );
}

/// Proves a rewritten same id cannot launder freshness, while a genuinely new post-fix id can.
#[test]
fn vm14_dr2r5_laundering_regression_first_completed_wins() {
    let stale_dir = tmp();
    let (b, n) = paths(&stale_dir);
    let mut stale = legacy_or_unbounded_track("same-id", true);
    stale["evidence_checkpoint"] = json!(3);
    write_snapshot(
        stale_dir.path(),
        3,
        "test_creation",
        "1",
        json!({"subagent_tracks": [stale.clone()]}),
    );
    let mut rewritten = stale.clone();
    rewritten["evidence_checkpoint"] = json!(5);
    write_snapshot(
        stale_dir.path(),
        5,
        "phase_fixing",
        "1",
        json!({"subagent_tracks": [rewritten.clone()]}),
    );
    make_baton_v3(&b, "test_creation", "team", 6, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    let cur = load(&b);
    assert_eq!(
        provenance::first_completed_checkpoint(
            stale_dir.path(),
            &cur,
            6,
            "subagent_track",
            "same-id",
        ),
        Some(3)
    );
    make_baton_v3(&n, "cross_review", "team", 7, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(v, "subagent_tracks", rewritten);
    });
    run(&b, &n).assert_contains(
        "vm14 rewritten same-id remains stale",
        24,
        "test_creation->cross_review requires completed test-creation subagent_track",
    );

    let honest_dir = tmp();
    let (b, n) = paths(&honest_dir);
    write_impl_anchor(honest_dir.path(), 5, "1");
    let honest = legacy_or_unbounded_track("new-post-fix-id", true);
    make_baton_v3(&b, "test_creation", "team", 6, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(v, "subagent_tracks", honest.clone());
    });
    write_json(
        &honest_dir.path().join("history/6-test_creation-team.json"),
        &load(&b),
    );
    let cur = load(&b);
    assert_eq!(
        provenance::first_completed_checkpoint(
            honest_dir.path(),
            &cur,
            6,
            "subagent_track",
            "new-post-fix-id",
        ),
        Some(6)
    );
    make_baton_v3(&n, "cross_review", "team", 7, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(v, "subagent_tracks", honest);
    });
    run(&b, &n).assert("vm14 honest new post-fix id", 0);
}

// ===== VM-15: kind collisions and stripped provenance ====================

/// Proves origin lookup is kind-qualified/unique and stripped stamp triples fail closed.
#[test]
fn vm15_kind_collision() {
    let snapshot = json!({
        "verification_matrix": [{"id": "same", "current": "passed"}],
        "subagent_tracks": [{"id": "same", "status": "completed", "result": "approved"}]
    });
    assert_eq!(
        provenance::find_unit(&snapshot, "verification_matrix_row", "same").unwrap()["current"],
        "passed"
    );
    assert_eq!(
        provenance::find_unit(&snapshot, "subagent_track", "same").unwrap()["result"],
        "approved"
    );
    let duplicate = json!({"subagent_tracks": [{"id": "same"}, {"id": "same"}]});
    assert!(provenance::find_unit(&duplicate, "subagent_track", "same").is_none());

    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    let stripped_candidate_dir = tmp();
    write_origin_track_snapshot(
        stripped_candidate_dir.path(),
        5,
        "1",
        origin_track("t", "anchor", &["src/a.rs"]),
        chunks.clone(),
    );
    let mut candidate = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    for key in ["covered_input_digest", "digest_algo", "covered_paths"] {
        candidate.as_object_mut().unwrap().remove(key);
    }
    let baton = decide_baton(candidate.clone(), chunks.clone());
    assert_eq!(
        reverify::decide(
            &baton,
            stripped_candidate_dir.path(),
            &candidate,
            "subagent_track",
            10,
            "1",
            stripped_candidate_dir.path(),
        ),
        reverify::Decision::ReRun
    );

    let stripped_origin_dir = tmp();
    let mut origin = origin_track("t", "anchor", &["src/a.rs"]);
    for key in ["covered_input_digest", "digest_algo", "covered_paths"] {
        origin.as_object_mut().unwrap().remove(key);
    }
    write_origin_track_snapshot(stripped_origin_dir.path(), 5, "1", origin, chunks.clone());
    let candidate = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    let baton = decide_baton(candidate.clone(), chunks);
    assert_eq!(
        reverify::decide(
            &baton,
            stripped_origin_dir.path(),
            &candidate,
            "subagent_track",
            10,
            "1",
            stripped_origin_dir.path(),
        ),
        reverify::Decision::ReRun
    );
}

// ===== VM-16: three-way closure membership ===============================

/// Proves current closure drift defeats a stale origin/candidate two-way agreement.
#[test]
fn vm16_closure_membership_drift() {
    let d = tmp();
    let stale_chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    write_origin_track_snapshot(
        d.path(),
        5,
        "1",
        origin_track("t", "anchor", &["src/a.rs"]),
        stale_chunks,
    );
    let current_chunks = json!([
        {
            "id": "X", "write_paths": ["src/a.rs"], "read_paths": [],
            "depends_on": ["Y"]
        },
        {
            "id": "Y", "write_paths": ["src/new-dependency.rs"], "read_paths": [],
            "depends_on": []
        }
    ]);
    let candidate = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    let baton = decide_baton(candidate.clone(), current_chunks);
    assert_eq!(
        reverify::decide(
            &baton,
            d.path(),
            &candidate,
            "subagent_track",
            10,
            "1",
            d.path(),
        ),
        reverify::Decision::ReRun
    );
}

// ===== VM-17: reviews never carry ========================================

/// Proves terminal matrix and cross-review gates ignore even valid-looking carry provenance.
#[test]
fn vm17_reviews_never_carry() {
    let done_dir = tmp();
    let (b, n) = paths(&done_dir);
    let anchor = committed_repo_at(done_dir.path(), &[("src/review-input.rs", "stable\n")]);
    seed_done_artifacts(done_dir.path());
    make_baton_v3(&b, "termination_review", "team", 4, |v| {
        configure_terminal_current(v);
        configure_bounded_work_split(v, "src/review-input.rs");
    });
    make_baton_v3(&n, "done", "team", 5, |v| {
        configure_done_candidate(v);
        configure_bounded_work_split(v, "src/review-input.rs");
        let row = &mut v["verification_matrix"].as_array_mut().unwrap()[0];
        row["current"] = json!("passed");
        row["result"] = json!("passed");
        row["evidence_checkpoint"] = json!(2);
        row["carried_from_checkpoint"] = json!(3);
        row["carry_reason"] = json!("Inputs unchanged, but terminal evidence must still be fresh.");
        row["covers_chunks"] = json!(["X"]);
        row["covered_input_digest"] = json!(anchor.clone());
        row["digest_algo"] = json!("git-covers-diff-v1");
        row["covered_paths"] = json!(["src/review-input.rs"]);
    });
    let candidate = load(&n);
    let row = candidate["verification_matrix"].as_array().unwrap()[0].clone();
    let row_id = row["id"].as_str().unwrap();
    let mut origin_row = row.clone();
    origin_row
        .as_object_mut()
        .unwrap()
        .remove("carried_from_checkpoint");
    origin_row.as_object_mut().unwrap().remove("carry_reason");
    write_snapshot(
        done_dir.path(),
        3,
        "parallel_implementing",
        "1",
        json!({
            "verification_matrix": [origin_row],
            "work_split": [bounded_chunk("X", &["src/review-input.rs"], &[])]
        }),
    );
    assert_eq!(
        provenance::find_unit(
            &provenance::read_origin_snapshot(done_dir.path(), 3).unwrap(),
            "verification_matrix_row",
            row_id,
        )
        .unwrap()["current"],
        "passed"
    );
    assert_eq!(
        reverify::decide(
            &candidate,
            done_dir.path(),
            &row,
            "verification_matrix_row",
            4,
            "1",
            done_dir.path(),
        ),
        reverify::Decision::Carry,
        "the claim would carry if this generic decision were honored"
    );
    run(&b, &n).assert_contains(
        "vm17 done matrix ignores carry",
        23,
        "stale_verification_matrix",
    );

    let review_dir = tmp();
    let (b, n) = paths(&review_dir);
    make_baton_v3(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v3(&n, "deep_review", "prativadi", 5, |v| {
        cross_review_tracks(v);
        dispatch_request_open_vadi(v);
        let invalid = v["subagent_tracks"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|t| t["id"] == "cross-prativadi")
            .unwrap();
        invalid["status"] = json!("running");
        invalid["carried_from_checkpoint"] = json!(3);
        invalid["carry_reason"] = json!("Review inputs appear unchanged.");
        invalid["covers_chunks"] = json!(["X"]);
        invalid["covered_input_digest"] = json!("valid-looking-origin-anchor");
        invalid["digest_algo"] = json!("git-covers-diff-v1");
        invalid["covered_paths"] = json!(["src/review-input.rs"]);
    });
    run(&b, &n).assert_contains(
        "vm17 cross-review track ignores carry",
        24,
        "completed cross-review subagent_tracks for both roles",
    );
}

// ===========================================================================
// Coverage completion (VM-18): the VM-1..VM-17 narrative above proves every
// behavioral claim in the verification matrix; the tests below additionally
// close specific reverify.rs / provenance.rs / write.rs branches the VM
// narrative does not naturally reach, so `--test delta_reverification` alone
// clears 100% changed-line coverage of the three delta-reverification files.
// ===========================================================================

// ---- reverify.rs: decide() guard branches not reached by VM-1..VM-17 -----

/// Closes reverify.rs's guard-(d) `unit_id.is_empty()` branch.
#[test]
fn reverify_empty_unit_id_never_carries() {
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    let mut unit = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    unit["id"] = json!("");
    let baton = decide_baton(unit.clone(), chunks);
    assert_eq!(
        reverify::decide(
            &baton,
            Path::new("/no-fs-needed"),
            &unit,
            "subagent_track",
            10,
            "1",
            Path::new("/no-git-needed"),
        ),
        reverify::Decision::ReRun
    );
}

/// Closes reverify.rs's guard-(d) `find_unit` "no matching id in an
/// on-cycle, readable snapshot" branch (distinct from VM-15's ambiguous-id
/// and VM-5's missing-snapshot cases).
#[test]
fn reverify_origin_snapshot_missing_matching_unit_is_rerun() {
    let d = tmp();
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    write_origin_track_snapshot(
        d.path(),
        5,
        "1",
        origin_track("someone-else", "anchor", &["src/a.rs"]),
        chunks.clone(),
    );
    let unit = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    let baton = decide_baton(unit.clone(), chunks);
    assert_eq!(
        reverify::decide(&baton, d.path(), &unit, "subagent_track", 10, "1", d.path()),
        reverify::Decision::ReRun
    );
}

/// Closes reverify.rs's guard-(d) `was_pass(&orig_unit)` false branch: the
/// origin exists, is on-cycle, and is not itself a carry, but did not pass.
#[test]
fn reverify_origin_unit_not_passing_is_rerun() {
    let d = tmp();
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    let mut origin = origin_track("t", "anchor", &["src/a.rs"]);
    origin["result"] = json!("failed");
    write_origin_track_snapshot(d.path(), 5, "1", origin, chunks.clone());
    let unit = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    let baton = decide_baton(unit.clone(), chunks);
    assert_eq!(
        reverify::decide(&baton, d.path(), &unit, "subagent_track", 10, "1", d.path()),
        reverify::Decision::ReRun
    );
}

/// Closes reverify.rs's guard-(e) SP-1 anti-substitution digest-mismatch
/// branch reached with the closure three-way check already agreeing (so it
/// is the digest comparison itself, not closure membership, that rejects) —
/// no Git call is needed since the mismatch is caught before
/// `commit_anchor_valid` runs.
#[test]
fn reverify_digest_mismatch_with_agreeing_closure_is_rerun() {
    let d = tmp();
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    write_origin_track_snapshot(
        d.path(),
        5,
        "1",
        origin_track("t", "origin-anchor-sha", &["src/a.rs"]),
        chunks.clone(),
    );
    let unit = carried_track("t", 5, "impostor-anchor-sha", &["src/a.rs"], &["X"]);
    let baton = decide_baton(unit.clone(), chunks);
    assert_eq!(
        reverify::decide(&baton, d.path(), &unit, "subagent_track", 10, "1", d.path()),
        reverify::Decision::ReRun
    );
}

/// Closes reverify.rs's `is_terminal_approval_unit`'s explicit `terminal:
/// true` marker branch.
#[test]
fn reverify_explicit_terminal_marker_never_carries() {
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    let mut unit = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    unit["terminal"] = json!(true);
    let baton = decide_baton(unit.clone(), chunks);
    assert_eq!(
        reverify::decide(
            &baton,
            Path::new("/no-fs-needed"),
            &unit,
            "subagent_track",
            10,
            "1",
            Path::new("/no-git-needed"),
        ),
        reverify::Decision::ReRun
    );
}

/// Closes `open_finding_touches_closure`'s bare-string-finding branch, its
/// "finding not open -> continue" branch, its "open finding whose paths do
/// not overlap -> falls through" branch, and both disjuncts of
/// `path_overlap`'s directory-prefix check.
#[test]
fn reverify_finding_edge_shapes() {
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);

    // A bare-string finding is open + pathless -> global block.
    let candidate_noop = carried_track("t", 5, "anchor", &["src/a.rs"], &["X"]);
    let mut bare = decide_baton(candidate_noop.clone(), chunks.clone());
    bare["findings"] = json!(["unstructured note"]);
    assert_eq!(
        reverify::decide(
            &bare,
            Path::new("/no-fs-needed"),
            &candidate_noop,
            "subagent_track",
            10,
            "1",
            Path::new("/no-git-needed"),
        ),
        reverify::Decision::ReRun
    );

    let d = tmp();
    let anchor = committed_repo_at(d.path(), &[("src/a.rs", "a\n")]);
    write_origin_track_snapshot(
        d.path(),
        5,
        "1",
        origin_track("t", &anchor, &["src/a.rs"]),
        chunks.clone(),
    );
    let candidate = carried_track("t", 5, &anchor, &["src/a.rs"], &["X"]);

    // A CLOSED finding never blocks, even overlapping the closure.
    let mut closed_ok = decide_baton(candidate.clone(), chunks.clone());
    closed_ok["findings"] =
        json!([{"id": "f-closed", "status": "resolved", "paths": ["src/a.rs"]}]);
    assert_eq!(
        reverify::decide(
            &closed_ok,
            d.path(),
            &candidate,
            "subagent_track",
            10,
            "1",
            d.path()
        ),
        reverify::Decision::Carry
    );

    // An OPEN finding whose paths do NOT overlap the closure never blocks.
    let mut disjoint_ok = decide_baton(candidate.clone(), chunks.clone());
    disjoint_ok["findings"] =
        json!([{"id": "f-disjoint", "status": "open", "paths": ["src/unrelated.rs"]}]);
    assert_eq!(
        reverify::decide(
            &disjoint_ok,
            d.path(),
            &candidate,
            "subagent_track",
            10,
            "1",
            d.path()
        ),
        reverify::Decision::Carry
    );

    // A directory-prefix overlap (closure path is a deeper child of the
    // finding path) still blocks: `path_overlap`'s second disjunct.
    let mut prefix_block = decide_baton(candidate.clone(), chunks);
    prefix_block["findings"] = json!([{"id": "f-prefix", "status": "open", "paths": ["src"]}]);
    assert_eq!(
        reverify::decide(
            &prefix_block,
            d.path(),
            &candidate,
            "subagent_track",
            10,
            "1",
            d.path()
        ),
        reverify::Decision::ReRun
    );

    // The mirrored direction (finding path is a deeper child of a closure
    // entry): `path_overlap`'s first disjunct.
    let d2 = tmp();
    let anchor2 = committed_repo_at(d2.path(), &[("src", "a directory-token fixture\n")]);
    let dir_chunks = json!([bounded_chunk("X", &["src"], &[])]);
    write_origin_track_snapshot(
        d2.path(),
        5,
        "1",
        origin_track("t", &anchor2, &["src"]),
        dir_chunks.clone(),
    );
    let dir_candidate = carried_track("t", 5, &anchor2, &["src"], &["X"]);
    let mut nested_block = decide_baton(dir_candidate.clone(), dir_chunks);
    nested_block["findings"] = json!([{"id": "f-nested", "status": "open", "paths": ["src/a.rs"]}]);
    assert_eq!(
        reverify::decide(
            &nested_block,
            d2.path(),
            &dir_candidate,
            "subagent_track",
            10,
            "1",
            d2.path(),
        ),
        reverify::Decision::ReRun
    );
}

/// Closes `str_field`'s non-string-scalar `to_string()` branch: a
/// non-string, non-null/false `carry_reason` still renders non-blank (jq
/// `tostring` semantics) and does not block guard (f).
#[test]
fn reverify_non_string_scalar_field_renders_via_tostring() {
    let d = tmp();
    let anchor = committed_repo_at(d.path(), &[("src/a.rs", "a\n")]);
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    write_origin_track_snapshot(
        d.path(),
        5,
        "1",
        origin_track("t", &anchor, &["src/a.rs"]),
        chunks.clone(),
    );
    let mut unit = carried_track("t", 5, &anchor, &["src/a.rs"], &["X"]);
    unit["carry_reason"] = json!(12345);
    let baton = decide_baton(unit.clone(), chunks);
    assert_eq!(
        reverify::decide(&baton, d.path(), &unit, "subagent_track", 10, "1", d.path()),
        reverify::Decision::Carry
    );
}

/// Closes guard-(f)'s blank-`carry_reason` branch: every OTHER guard passes
/// (real Git anchor, matching closure, matching digest) but the reason is
/// whitespace-only, so `decide` still rejects the claim as an unaudited
/// carry.
#[test]
fn reverify_blank_carry_reason_never_carries() {
    let d = tmp();
    let anchor = committed_repo_at(d.path(), &[("src/a.rs", "a\n")]);
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    write_origin_track_snapshot(
        d.path(),
        5,
        "1",
        origin_track("t", &anchor, &["src/a.rs"]),
        chunks.clone(),
    );
    let mut unit = carried_track("t", 5, &anchor, &["src/a.rs"], &["X"]);
    unit["carry_reason"] = json!("   ");
    let baton = decide_baton(unit.clone(), chunks);
    assert_eq!(
        reverify::decide(&baton, d.path(), &unit, "subagent_track", 10, "1", d.path()),
        reverify::Decision::ReRun
    );
}

/// Closes `str_vec_field`'s null-drop and non-string-`tostring` filter-map
/// arms on a chunk's declared `write_paths`.
#[test]
fn reverify_declared_paths_tolerates_null_and_nonstring_entries() {
    let baton = json!({
        "work_split": [
            {
                "id": "X",
                "write_paths": [null, 42, "src/x.rs"],
                "read_paths": []
            }
        ]
    });
    let unit = json!({"id": "t", "covers_chunks": ["X"]});
    let closure = reverify::derive_covered_closure(&baton, &unit).unwrap();
    let expected: BTreeSet<String> = ["42", "src/x.rs"].into_iter().map(String::from).collect();
    assert_eq!(closure, expected);
}

/// Closes `arr_field`'s "absent/non-array" fallback branch: a baton with no
/// `findings` key at all is treated as having no findings.
#[test]
fn reverify_missing_findings_field_is_treated_as_no_findings() {
    let d = tmp();
    let anchor = committed_repo_at(d.path(), &[("src/a.rs", "a\n")]);
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    write_origin_track_snapshot(
        d.path(),
        5,
        "1",
        origin_track("t", &anchor, &["src/a.rs"]),
        chunks.clone(),
    );
    let candidate = carried_track("t", 5, &anchor, &["src/a.rs"], &["X"]);
    let mut baton = decide_baton(candidate.clone(), chunks);
    baton.as_object_mut().unwrap().remove("findings");
    assert_eq!(
        reverify::decide(
            &baton,
            d.path(),
            &candidate,
            "subagent_track",
            10,
            "1",
            d.path()
        ),
        reverify::Decision::Carry
    );
}

/// Closes `work_split_chunks`'s object-shaped (id-keyed) branch and its
/// "absent/non-array/non-object" fallback branch.
#[test]
fn reverify_work_split_accepts_object_shape_and_rejects_absent_shape() {
    let object_baton = json!({
        "work_split": {
            "X": {"id": "X", "write_paths": ["src/obj.rs"], "read_paths": []}
        }
    });
    let unit = json!({"id": "t", "covers_chunks": ["X"]});
    let closure = reverify::derive_covered_closure(&object_baton, &unit).unwrap();
    assert_eq!(
        closure,
        ["src/obj.rs".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );

    let no_split_baton = json!({});
    assert_eq!(
        reverify::derive_covered_closure(&no_split_baton, &unit),
        None
    );
}

// ---- provenance.rs: branches not reached by VM-1..VM-17 ------------------

/// Closes `read_origin_snapshot`'s ambiguous-duplicate-checkpoint-file
/// branch (VM-1..VM-17 never write two files sharing one checkpoint prefix).
#[test]
fn provenance_read_origin_snapshot_rejects_ambiguous_checkpoint_files() {
    let d = tmp();
    write_snapshot(d.path(), 5, "test_creation", "1", json!({}));
    write_json(
        &d.path().join("history/5-test_creation-team.dup-1.json"),
        &json!({"checkpoint": 5, "status": "test_creation", "phase": "1"}),
    );
    assert!(provenance::read_origin_snapshot(d.path(), 5).is_none());
}

/// Closes `current_phase_cycle_start`'s "current doc's own phase field is
/// absent" branch (renders as `""`, mismatches, returns `current_ckpt`).
#[test]
fn provenance_cycle_start_handles_missing_phase_on_current_doc() {
    let d = tmp();
    let cur = json!({"checkpoint": 4, "status": "test_creation"});
    assert_eq!(
        provenance::current_phase_cycle_start(d.path(), &cur, 4, "1"),
        4
    );
}

/// Closes `commit_anchor_valid`'s empty-anchor and empty-covered-list
/// fail-closed branches.
#[test]
fn provenance_commit_anchor_valid_rejects_empty_anchor_or_empty_paths() {
    let d = tmp();
    committed_repo_at(d.path(), &[("tracked.rs", "x\n")]);
    let anchor = head(d.path());
    assert!(!provenance::commit_anchor_valid(
        d.path(),
        "",
        &["tracked.rs".to_string()]
    ));
    assert!(!provenance::commit_anchor_valid(d.path(), &anchor, &[]));
}

/// Closes `find_unit`'s unknown-`kind` branch.
#[test]
fn provenance_find_unit_rejects_unknown_kind() {
    let snap = json!({"verification_matrix": [], "subagent_tracks": []});
    assert!(provenance::find_unit(&snap, "unknown-kind", "id").is_none());
}

/// Closes `first_completed_checkpoint`'s "current doc carries no numeric
/// `checkpoint` field" tolerance branch.
#[test]
fn provenance_first_completed_tolerates_current_doc_without_checkpoint() {
    let d = tmp();
    write_snapshot(
        d.path(),
        5,
        "test_creation",
        "1",
        json!({"subagent_tracks": [{"id": "t", "status": "completed", "result": "passed"}]}),
    );
    let cur = json!({"status": "cross_review"});
    assert_eq!(
        provenance::first_completed_checkpoint(d.path(), &cur, 9, "subagent_track", "t"),
        Some(5)
    );
}

// ---- write.rs: delta branches not reached by VM-1..VM-17 -----------------

/// Closes `test_track_stamp_violation`'s DIRECT `head_unresolved` branch:
/// the fixture directory is never `git init`-ed, so `rev-parse HEAD` fails.
#[test]
fn write_direct_stamp_head_unresolved_is_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    let mut unit = direct_bounded_track("no-git-direct", "unused-digest", "src/x.rs");
    unit.as_object_mut().unwrap().remove("covered_input_digest");
    unit.as_object_mut().unwrap().remove("digest_algo");
    unit.as_object_mut().unwrap().remove("covered_paths");
    make_baton_v3(&b, "test_creation", "team", 6, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        configure_bounded_work_split(v, "src/x.rs");
    });
    make_baton_v3(&n, "cross_review", "team", 7, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        configure_bounded_work_split(v, "src/x.rs");
        push(v, "subagent_tracks", unit);
    });
    run(&b, &n).assert_contains(
        "write direct stamp head unresolved",
        23,
        "forged_test_creation_stamp id=no-git-direct mode=direct reason=head_unresolved",
    );
}

/// Closes `test_track_stamp_violation`'s CARRIED `origin_unreadable` branch:
/// `carried_from_checkpoint` names a checkpoint with no history snapshot.
#[test]
fn write_carried_stamp_origin_unreadable_is_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 6, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        configure_bounded_work_split(v, "src/x.rs");
    });
    let track = carried_track("carried-missing-origin", 3, "anchor", &["src/x.rs"], &["X"]);
    make_baton_v3(&n, "cross_review", "team", 7, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        configure_bounded_work_split(v, "src/x.rs");
        push(v, "subagent_tracks", track);
    });
    run(&b, &n).assert_contains(
        "write carried stamp origin unreadable",
        23,
        "forged_test_creation_stamp id=carried-missing-origin mode=carried reason=origin_unreadable",
    );
}

/// Closes `test_track_stamp_violation`'s CARRIED `origin_unit_missing`
/// branch: the origin snapshot is readable but does not contain the
/// candidate's own id.
#[test]
fn write_carried_stamp_origin_unit_missing_is_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    write_origin_track_snapshot(
        d.path(),
        3,
        "1",
        origin_track("someone-else", "anchor", &["src/x.rs"]),
        json!([bounded_chunk("X", &["src/x.rs"], &[])]),
    );
    make_baton_v3(&b, "test_creation", "team", 6, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        configure_bounded_work_split(v, "src/x.rs");
    });
    let track = carried_track("carried-x", 3, "anchor", &["src/x.rs"], &["X"]);
    make_baton_v3(&n, "cross_review", "team", 7, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        configure_bounded_work_split(v, "src/x.rs");
        push(v, "subagent_tracks", track);
    });
    run(&b, &n).assert_contains(
        "write carried stamp origin unit missing",
        23,
        "forged_test_creation_stamp id=carried-x mode=carried reason=origin_unit_missing",
    );
}

/// Closes `track_is_fresh`'s `first_completed_checkpoint -> None`
/// (never-completed-anywhere) arm. `track_is_fresh`'s own empty-id
/// short-circuit is NOT independently reachable through the public `write`
/// pipeline: the earlier `bad_subagent_tracks` shape gate already rejects an
/// empty subagent_track id (exit 23) before the transition-legality carry
/// gate runs, confirmed by inspection above — `reverify::decide`'s own
/// equivalent empty-id guard (SP-2 identity) is covered directly at the
/// library level by `reverify_empty_unit_id_never_carries`.
#[test]
fn write_track_is_fresh_rejects_never_completed_id() {
    let d2 = tmp();
    let (b2, n2) = paths(&d2);
    let anchor2 = committed_repo_at(d2.path(), &[("src/x.rs", "x\n")]);
    let never_completed = direct_bounded_track("never-completed-anywhere", &anchor2, "src/x.rs");
    make_baton_v3(&b2, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        configure_bounded_work_split(v, "src/x.rs");
    });
    make_baton_v3(&n2, "cross_review", "team", 6, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        configure_bounded_work_split(v, "src/x.rs");
        push(v, "subagent_tracks", never_completed);
    });
    run(&b2, &n2).assert_contains(
        "write track_is_fresh never completed",
        24,
        "test_creation->cross_review requires completed test-creation subagent_track",
    );
}

/// Closes `lost_update_violation`'s RR-6a shape-mismatch fallback arm: an
/// object-shaped `verification_matrix` (not array-shaped on both sides)
/// skips the id-superset check instead of panicking or false-rejecting.
#[test]
fn write_lost_update_matrix_check_skips_non_array_shapes() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!({"vm-1": {"id": "vm-1", "current": "pending"}});
    });
    make_baton_v3(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!({"vm-1": {"id": "vm-1", "current": "pending"}});
        v["summary"] = json!("Team sync with an object-shaped verification_matrix.");
        v["next_action"] = json!("Continue test creation.");
    });
    run(&b, &n).assert(
        "write lost_update tolerates non-array verification_matrix",
        0,
    );
}

// ===========================================================================
// Known-unreachable lines (documented, not test gaps): four lines remain
// uncovered under `--test delta_reverification` after the above additions.
// Each is defensive redundancy the surrounding function's own invariants
// make unreachable through any valid input to the PUBLIC API — not a missed
// behavioral branch:
//
//   * reverify.rs:74  `if closure.is_empty() { return Decision::ReRun; }` —
//     `derive_covered_closure` never returns `Some` of an empty set (its own
//     `if paths.is_empty() { return None; }` guard fires first), so this
//     branch cannot be reached by any `unit`/`baton` pair.
//   * reverify.rs:96  the second `read_origin_snapshot(dir, origin)` call's
//     `else` arm — `on_current_cycle_ancestry` (already required to return
//     `true` to reach this line) performs its own internal
//     `read_origin_snapshot(dir, origin)` read of the identical, immutable
//     file first; a single-threaded test cannot make the second read of the
//     same path fail after the first succeeded.
//   * reverify.rs:192 `if paths.is_empty() { return None; }` inside
//     `derive_covered_closure` — `covers_chunks` is checked non-empty at
//     entry, and every loop iteration either returns `None` early (dangling
//     seed / no declared paths / unnormalizable path) or inserts at least
//     one path before continuing, so the accumulated set can never be empty
//     by the time this guard runs.
//   * write.rs:5020   `track_is_fresh`'s `if id.is_empty() { return false; }`
//     — an empty `subagent_track.id` is already rejected by the earlier
//     `bad_subagent_tracks` shape gate (exit 23) before the transition
//     reaches the carry gate at all (confirmed empirically: see the removed
//     first half of `write_track_is_fresh_rejects_never_completed_id`'s
//     history in this file's authoring). The equivalent identity guard
//     inside `reverify::decide` IS reachable and IS covered directly by
//     `reverify_empty_unit_id_never_carries`.
// ===========================================================================
