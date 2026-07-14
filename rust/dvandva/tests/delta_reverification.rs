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

/// Proves SP-3 closure-membership drift is LOAD-BEARING (DR53-F2). Every other
/// guard passes: a real committed anchor over BOTH closure paths (so
/// `commit_anchor_valid` would succeed), origin and candidate `covered_paths`
/// byte-equal, valid provenance + on-cycle ancestry, and a non-blank
/// `carry_reason` — a would-be Carry. The ONLY thing forcing ReRun is that the
/// CURRENT `work_split`'s `depends_on` expanded (or shrank) the engine-derived
/// closure past the stale origin/candidate two-way agreement. Commenting the
/// SP-3 three-way check in reverify.rs guard (e) flips this test to Carry
/// (mutation-verified), which the old fixture — lacking any committed repo, so
/// `commit_anchor_valid` fail-closed regardless — could not detect.
#[test]
fn vm16_closure_membership_drift() {
    // --- expand direction: current closure GAINS src/new-dependency.rs ------
    let d = tmp();
    let anchor = committed_repo_at(
        d.path(),
        &[("src/a.rs", "a\n"), ("src/new-dependency.rs", "dep\n")],
    );
    // Origin snapshot + candidate agree on the STALE two-way closure ["src/a.rs"].
    write_origin_track_snapshot(
        d.path(),
        5,
        "1",
        origin_track("t", &anchor, &["src/a.rs"]),
        json!([bounded_chunk("X", &["src/a.rs"], &[])]),
    );
    let candidate = carried_track("t", 5, &anchor, &["src/a.rs"], &["X"]);
    // CURRENT work_split: X now depends_on Y, expanding the derived closure to
    // {src/a.rs, src/new-dependency.rs} — the drift the two-way agreement misses.
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

    // --- shrink direction: current closure LOSES src/b.rs -------------------
    let d2 = tmp();
    let anchor2 = committed_repo_at(d2.path(), &[("src/a.rs", "a\n"), ("src/b.rs", "b\n")]);
    write_origin_track_snapshot(
        d2.path(),
        5,
        "1",
        origin_track("t", &anchor2, &["src/a.rs", "src/b.rs"]),
        json!([bounded_chunk("X", &["src/a.rs", "src/b.rs"], &[])]),
    );
    let candidate2 = carried_track("t", 5, &anchor2, &["src/a.rs", "src/b.rs"], &["X"]);
    // CURRENT work_split shrinks X's footprint to just src/a.rs.
    let shrunk_chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    let baton2 = decide_baton(candidate2.clone(), shrunk_chunks);
    assert_eq!(
        reverify::decide(
            &baton2,
            d2.path(),
            &candidate2,
            "subagent_track",
            10,
            "1",
            d2.path(),
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

/// CR56-F1: closure resolution must treat a BLANK or DUPLICATED `work_split`
/// chunk id as ambiguity ⇒ `None` (unbounded, never carries), consistent with
/// the SP-2 `find_unit` / DR53-F4 duplicate-poison conventions — never a silent
/// first-match. Exercises the three `find_chunk` fail-closed arms directly.
#[test]
fn reverify_derive_closure_poisons_on_blank_or_duplicate_chunk_ids() {
    // (a) blank seed id reaches find_chunk unaddressable ⇒ None.
    let baton = json!({"work_split": [bounded_chunk("X", &["src/x.rs"], &[])]});
    assert_eq!(
        reverify::derive_covered_closure(&baton, &json!({"id": "t", "covers_chunks": [""]})),
        None
    );

    // (b) duplicate SEED id on DISTINCT paths — first-match would omit the
    // second chunk's src/second.rs; the whole derivation must poison instead.
    let dup_seed = json!({
        "work_split": [
            {"id": "X", "write_paths": ["src/first.rs"], "read_paths": []},
            {"id": "X", "write_paths": ["src/second.rs"], "read_paths": []}
        ]
    });
    assert_eq!(
        reverify::derive_covered_closure(&dup_seed, &json!({"id": "t", "covers_chunks": ["X"]})),
        None
    );

    // (b) duplicate id reached transitively via a UNIQUE seed's depends_on.
    let dup_dep = json!({
        "work_split": [
            {"id": "A", "write_paths": ["src/a.rs"], "read_paths": [], "depends_on": ["B"]},
            {"id": "B", "write_paths": ["src/b1.rs"], "read_paths": []},
            {"id": "B", "write_paths": ["src/b2.rs"], "read_paths": []}
        ]
    });
    assert_eq!(
        reverify::derive_covered_closure(&dup_dep, &json!({"id": "t", "covers_chunks": ["A"]})),
        None
    );
}

/// CR56-F1 end-to-end regression: two `work_split` chunks share id "X" on
/// DISTINCT `write_paths`; a carried test track covers "X"; the SECOND X-chunk's
/// path (`src/second.rs`) has drifted since the origin anchor while the first
/// (`src/first.rs`) is untouched. A silent first-match derives only
/// {src/first.rs} — clean under the anchor — and CARRIES, laundering the change
/// on src/second.rs. Fail-closed poisoning of the ambiguous id forces ReRun.
/// (Pre-fix `find_chunk` first-matched ⇒ this asserted Carry: RED.)
#[test]
fn reverify_duplicate_chunk_id_masking_changed_path_forces_rerun() {
    let d = tmp();
    let anchor = committed_repo_at(
        d.path(),
        &[("src/first.rs", "first\n"), ("src/second.rs", "second\n")],
    );
    // Origin + candidate agree on the stale first-match closure ["src/first.rs"].
    write_origin_track_snapshot(
        d.path(),
        5,
        "1",
        origin_track("t", &anchor, &["src/first.rs"]),
        json!([bounded_chunk("X", &["src/first.rs"], &[])]),
    );
    let candidate = carried_track("t", 5, &anchor, &["src/first.rs"], &["X"]);
    // The SECOND X-chunk's path drifts since the anchor — invisible to a
    // first-matched {src/first.rs} closure.
    std::fs::write(d.path().join("src/second.rs"), "DRIFTED\n").unwrap();
    // CURRENT work_split: a DUPLICATE "X" on distinct paths.
    let dup_chunks = json!([
        {"id": "X", "write_paths": ["src/first.rs"], "read_paths": []},
        {"id": "X", "write_paths": ["src/second.rs"], "read_paths": []}
    ]);
    let baton = decide_baton(candidate.clone(), dup_chunks);
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

/// DR64-F1: a duplicate `work_split` chunk id reached ONLY via a
/// `conflict_group` peer edge (never a seed nor a `depends_on` target) must
/// poison the closure exactly like the seed/depends_on cases above —
/// `find_chunk`'s ambiguity guard has no special-case for which edge type
/// reached it. `derive_covered_closure` returns `None`, and `decide`'s guard
/// (a) turns that straight into `ReRun`.
#[test]
fn reverify_derive_closure_poisons_on_duplicate_conflict_group_peer() {
    let chunks = json!([
        {"id": "A", "write_paths": ["src/a.rs"], "read_paths": [], "depends_on": [], "conflict_group": "grp"},
        {"id": "B", "write_paths": ["src/b1.rs"], "read_paths": [], "depends_on": [], "conflict_group": "grp"},
        {"id": "B", "write_paths": ["src/b2.rs"], "read_paths": [], "depends_on": [], "conflict_group": "grp"}
    ]);
    let baton = json!({"work_split": chunks.clone()});
    assert_eq!(
        reverify::derive_covered_closure(&baton, &json!({"id": "t", "covers_chunks": ["A"]})),
        None
    );

    let candidate = carried_track("t", 5, "anchor", &["src/a.rs"], &["A"]);
    let decide_input = decide_baton(candidate.clone(), chunks);
    assert_eq!(
        reverify::decide(
            &decide_input,
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

/// Closes `current_phase_cycle_start`'s empty-history `return 0` arm: the
/// current doc's phase matches but no earlier history snapshot exists. (CR21-F4
/// reordered `reverify::decide` to read the origin snapshot BEFORE the ancestry
/// check, so VM-5's missing-origin case no longer reaches this arm transitively;
/// it is now covered here directly.)
#[test]
fn provenance_cycle_start_returns_zero_on_empty_history() {
    let d = tmp();
    assert_eq!(
        provenance::current_phase_cycle_start(d.path(), &json!({"phase": "1"}), 10, "1"),
        0
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

/// TR73-F1 regression: a `git init --object-format=sha256` repo stamps 64-hex
/// object ids. `commit_anchor_valid`'s defense-in-depth shape guard must accept
/// a full 64-lowercase-hex anchor (the SHA-256 length arm), not only 40-hex
/// SHA-1 — otherwise carry is permanently impossible on such repos. An unchanged
/// tracked file validates against the 64-hex anchor; a working-tree change to a
/// covered path fails it. SKIPS (never fakes) when the installed git lacks
/// SHA-256 object-format support.
#[test]
fn provenance_commit_anchor_valid_accepts_sha256_object_ids() {
    let d = tmp();
    let init = dvandva::gitcfg::git(d.path(), &["init", "--object-format=sha256", "-q"]).unwrap();
    if !init.status.success() {
        eprintln!(
            "SKIP provenance_commit_anchor_valid_accepts_sha256_object_ids: installed git \
             lacks --object-format=sha256 support"
        );
        return;
    }
    git_ok(d.path(), &["config", "user.name", "Dvandva Test"]);
    git_ok(d.path(), &["config", "user.email", "dvandva@example.test"]);
    std::fs::write(d.path().join("tracked.rs"), "origin\n").unwrap();
    git_ok(d.path(), &["add", "tracked.rs"]);
    git_ok(d.path(), &["commit", "-q", "-m", "origin"]);
    let anchor = head(d.path());
    assert_eq!(
        anchor.len(),
        64,
        "SHA-256 object-format HEAD must be 64 hex chars, got {anchor:?}"
    );

    // Unchanged tracked file: the 64-hex anchor validates (RED before the fix).
    assert!(provenance::commit_anchor_valid(
        d.path(),
        &anchor,
        &["tracked.rs".to_string()]
    ));

    // A working-tree change to the covered path fails the same 64-hex anchor.
    std::fs::write(d.path().join("tracked.rs"), "changed\n").unwrap();
    assert!(!provenance::commit_anchor_valid(
        d.path(),
        &anchor,
        &["tracked.rs".to_string()]
    ));
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

/// DR64-F1: closes `first_completed_checkpoint`'s (private `lookup_unit`)
/// unknown-`kind` branch — an unrecognized `kind` never resolves to a
/// snapshot field, so the one history document scanned is treated ABSENT and
/// the scan exhausts to `None` rather than matching anything.
#[test]
fn provenance_first_completed_checkpoint_unknown_kind_is_none() {
    let d = tmp();
    write_snapshot(
        d.path(),
        5,
        "test_creation",
        "1",
        json!({"subagent_tracks": [{"id": "t", "status": "completed", "result": "passed"}]}),
    );
    assert_eq!(
        provenance::first_completed_checkpoint(d.path(), &json!({}), 9, "unknown-kind", "t"),
        None
    );
}

/// DR64-F1: closes `first_completed_checkpoint`'s (private `lookup_unit`)
/// non-array-field branch — a snapshot whose `subagent_tracks` is present but
/// not an array is treated ABSENT (not poisoned), so the scan continues past
/// it to a later genuinely-completed checkpoint.
#[test]
fn provenance_first_completed_checkpoint_non_array_field_is_absent() {
    let d = tmp();
    write_snapshot(
        d.path(),
        3,
        "test_creation",
        "1",
        json!({"subagent_tracks": "not-an-array"}),
    );
    write_snapshot(
        d.path(),
        5,
        "test_creation",
        "1",
        json!({"subagent_tracks": [{"id": "t", "status": "completed", "result": "passed"}]}),
    );
    assert_eq!(
        provenance::first_completed_checkpoint(d.path(), &json!({}), 9, "subagent_track", "t"),
        Some(5)
    );
}

/// DR64-F1: closes `first_completed_checkpoint`'s (private `lookup_unit`)
/// duplicate-id branch with a REAL history file — the `--lib` unit test in
/// `provenance.rs` exercises the same branch directly on an in-memory
/// snapshot. A snapshot with two same-id `subagent_tracks` entries poisons the
/// whole scan (`None`) even though a later checkpoint is a clean
/// completed+passing match.
#[test]
fn provenance_first_completed_checkpoint_duplicate_id_snapshot_poisons() {
    let d = tmp();
    write_snapshot(
        d.path(),
        3,
        "test_creation",
        "1",
        json!({"subagent_tracks": [
            {"id": "t", "status": "completed", "result": "passed"},
            {"id": "t", "status": "completed", "result": "passed"}
        ]}),
    );
    write_snapshot(
        d.path(),
        5,
        "test_creation",
        "1",
        json!({"subagent_tracks": [{"id": "t", "status": "completed", "result": "passed"}]}),
    );
    assert_eq!(
        provenance::first_completed_checkpoint(d.path(), &json!({}), 9, "subagent_track", "t"),
        None
    );
}

/// DR64-F1: an early snapshot's same-id track exists but is NOT
/// completed+passing — the `Found` arm's guard falls through without
/// returning, so the scan continues to a later checkpoint that IS a genuine
/// completed+passing match.
#[test]
fn provenance_first_completed_checkpoint_skips_found_not_passing() {
    let d = tmp();
    write_snapshot(
        d.path(),
        3,
        "test_creation",
        "1",
        json!({"subagent_tracks": [{"id": "t", "status": "completed", "result": "failed"}]}),
    );
    write_snapshot(
        d.path(),
        5,
        "test_creation",
        "1",
        json!({"subagent_tracks": [{"id": "t", "status": "completed", "result": "passed"}]}),
    );
    assert_eq!(
        provenance::first_completed_checkpoint(d.path(), &json!({}), 9, "subagent_track", "t"),
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
/// (never-completed-anywhere) arm. (CR21-F4 removed `track_is_fresh`'s former
/// empty-id short-circuit — an empty id already fails closed through the same
/// `first_completed_checkpoint -> None` path, and `reverify::decide`'s own
/// SP-2 identity guard stays covered by `reverify_empty_unit_id_never_carries`.)
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

// (CR21-F1 retired `write_lost_update_matrix_check_skips_non_array_shapes` — the
// old array-only guard no longer skips object matrices; object-key superset and
// same/cross-status shape-change behavior are covered by the cr21_f1_* tests
// below.)

// ===========================================================================
// CR21 cross-review regressions (F1 / F2 / F3). CR21-F4 removed the four
// former "documented-unreachable" lines (reverify.rs closure/second-read/paths
// guards + write.rs's track_is_fresh empty-id guard) by restructuring, so the
// `--test delta_reverification` changed-line map now clears a TRUE 100%.
// ===========================================================================

// ---- CR21-F3: the carry origin must be a complete qualifying test-creation --

/// Proves a same-id carry fails closed unless the ORIGIN unit is a COMPLETE
/// qualifying DIRECT test-creation execution — not merely `was_pass`. The repo,
/// closure, digest, and carry_reason are all VALID (so guards (e)/(f) would
/// otherwise carry); only the origin-shape defect re-runs it. A control case
/// with an untouched origin proves the fixture would carry absent the defect.
#[test]
fn cr21_f3_origin_direct_test_creation_shape_required() {
    let chunks = json!([bounded_chunk("X", &["src/a.rs"], &[])]);
    // control: an untouched, fully-qualified origin DOES carry (proves the
    // fixture reaches guards (e)/(f) and would carry but for the shape defect).
    {
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
        let baton = decide_baton(candidate.clone(), chunks.clone());
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
            reverify::Decision::Carry,
            "control: a fully-qualified origin carries"
        );
    }
    // CR29-F3: named tuple type keeps the table off clippy::type_complexity.
    type OriginCase = (&'static str, fn(&mut Value));
    let cases: [OriginCase; 7] = [
        ("running_not_completed", |o| o["status"] = json!("running")),
        ("wrong_track_subtype", |o| {
            o["track"] = json!("cross-review")
        }),
        ("owner_absent", |o| {
            o.as_object_mut().unwrap().remove("owner");
        }),
        // CR29-F2: a wrong (but non-empty) owner must not back a carry — the
        // gate requires the exact dvandva-test-creator identity.
        ("wrong_nonempty_owner", |o| o["owner"] = json!("vadi")),
        // CR29-F2: a same-id completed/passing track from another phase must not
        // back a carry — the gate requires phase == test_creation exactly.
        ("wrong_phase", |o| o["phase"] = json!("1")),
        ("evidence_refs_empty", |o| o["evidence_refs"] = json!([])),
        ("wrong_digest_algo", |o| o["digest_algo"] = json!("sha256")),
    ];
    for (label, mutate) in cases {
        let d = tmp();
        let anchor = committed_repo_at(d.path(), &[("src/a.rs", "a\n")]);
        let mut origin = origin_track("t", &anchor, &["src/a.rs"]);
        mutate(&mut origin);
        write_origin_track_snapshot(d.path(), 5, "1", origin, chunks.clone());
        let candidate = carried_track("t", 5, &anchor, &["src/a.rs"], &["X"]);
        let baton = decide_baton(candidate.clone(), chunks.clone());
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
            reverify::Decision::ReRun,
            "origin defect '{label}' must fail closed to ReRun"
        );
    }
}

// ---- CR21-F1: object matrices and shape flips in the lost_update guard ------

/// Proves an OBJECT-shaped verification_matrix is protected by object KEY: a
/// same-status team write dropping a key is a lost_update.
#[test]
fn cr21_f1_object_matrix_key_deletion_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!({
            "vm-1": {"id": "vm-1", "current": "pending"},
            "vm-2": {"id": "vm-2", "current": "pending"}
        });
    });
    make_baton_v3(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!({"vm-1": {"id": "vm-1", "current": "pending"}});
        v["summary"] = json!("Team sync dropping an installed object-matrix key.");
        v["next_action"] = json!("Team must retain vm-2.");
    });
    run(&b, &n).assert_contains(
        "cr21-f1 object matrix key deletion",
        23,
        "lost_update field=verification_matrix missing=vm-2",
    );
}

/// Proves an object-matrix key SUPERSET (retain + grow) is accepted (the
/// replacement for the retired array-only shape-mismatch acceptance test).
#[test]
fn cr21_f1_object_matrix_superset_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!({"vm-1": {"id": "vm-1", "current": "pending"}});
    });
    make_baton_v3(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!({
            "vm-1": {"id": "vm-1", "current": "pending"},
            "vm-2": {"id": "vm-2", "current": "pending"}
        });
        v["summary"] = json!("Team sync with an object-shaped verification_matrix superset.");
        v["next_action"] = json!("Continue test creation.");
    });
    run(&b, &n).assert("cr21-f1 object matrix superset accepted", 0);
}

/// Proves a SAME-STATUS team write flipping the matrix array<->object erases
/// the identity basis and is rejected as `shape_change` (the evasion vector).
#[test]
fn cr21_f1_same_status_shape_change_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        // default seed array matrix (non-empty identity basis).
    });
    make_baton_v3(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!({"row-a": {"result": "passed"}});
        v["summary"] = json!("Team sync flipping the matrix to object shape.");
        v["next_action"] = json!("Reshape evasion attempt.");
    });
    run(&b, &n).assert_contains(
        "cr21-f1 same-status shape change",
        23,
        "lost_update field=verification_matrix shape_change",
    );
}

/// CR29-F1 regression: a NON-terminal cross-status team write
/// (`test_creation`->`cross_review`) that reshapes the matrix array->object and
/// drops an installed row must fail `lost_update` as `shape_change`. The reshape
/// allowlist is the terminal `termination_review`->`done` edge ONLY — any other
/// cross-status edge that flips shape erases the identity basis and can silently
/// delete a row past the id-superset guard.
#[test]
fn cr29_f1_non_terminal_cross_status_reshape_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!([
            {"id": "vm-1", "result": "passed"},
            {"id": "vm-2", "result": "passed"}
        ]);
    });
    make_baton_v3(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        // array->object reshape dropping vm-2: without the terminal-edge-only
        // allowlist this slips the deletion past the id-superset guard.
        v["verification_matrix"] = json!({"vm-1": {"id": "vm-1", "result": "passed"}});
        v["summary"] = json!("Cross-status reshape dropping an installed matrix row.");
        v["next_action"] = json!("Reshape evasion across a status transition.");
    });
    run(&b, &n).assert_contains(
        "cr29-f1 non-terminal cross-status reshape",
        23,
        "lost_update field=verification_matrix shape_change",
    );
}

/// Proves the terminal termination_review->done matrix rebuild legitimately
/// reshapes array->object: lost_update ALLOWS the cross-status reshape when the
/// candidate PRESERVES the installed identity set (CR40-F1: each re-keyed row
/// carries its installed inner `id`), and the stale_verification_matrix_row sweep
/// still re-verifies every row fresh (row-b, id verify-100-percent-test-coverage,
/// is stale here, so terminal integrity holds and the stale gate — not
/// lost_update — is what fires).
#[test]
fn cr21_f1_cross_status_done_reshape_allowed_then_stale_swept() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    make_baton_v3(
        &b,
        "termination_review",
        "team",
        4,
        configure_terminal_current,
    );
    make_baton_v3(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        explainer_verification_track(v);
        // CR40-F1: re-key the installed two-row seed array into an object while
        // preserving both installed identities via inner `id`, so lost_update
        // passes and the stale sweep (not the identity guard) is exercised. row-b
        // (verify-100-percent-test-coverage) is missing a checkpoint -> stale.
        v["verification_matrix"] = json!({
            "row-a": {"id": "verify-research-coverage", "result": "passed", "evidence_refs": ["e"], "evidence_checkpoint": 5},
            "row-b": {"id": "verify-100-percent-test-coverage", "result": "passed", "evidence_refs": ["e"]}
        });
    });
    run(&b, &n).assert_contains(
        "cr21-f1 cross-status done reshape allowed then stale swept",
        23,
        "stale_verification_matrix row=verify-100-percent-test-coverage",
    );
}

/// CR34-F1 / CR40-F1 regression: the terminal `termination_review`->`done`
/// reshape must not let a candidate PERMANENTLY DROP an installed row. Here the
/// installed baton carries the default two-row array matrix; the done candidate
/// rebuilds it as a SINGLE fresh+complete object row that PRESERVES
/// verify-research-coverage (inner `id`) but drops verify-100-percent-test-coverage.
/// The survivor is fresh, so the stale_verification_matrix_row sweep is satisfied
/// — yet an installed identity was silently omitted. It must fail lost_update.
/// (CR40-F1 replaced the CR34-F1 cardinality floor with the unified
/// identity-superset `missing=<id>` vocabulary, which also catches equal-count
/// substitution, not only shrinkage.)
#[test]
fn cr34_f1_terminal_reshape_dropping_installed_row_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    make_baton_v3(
        &b,
        "termination_review",
        "team",
        4,
        configure_terminal_current,
    );
    make_baton_v3(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        explainer_verification_track(v);
        // Rebuild the installed two-row array into ONE fresh+complete object row
        // that preserves verify-research-coverage (inner id) but drops
        // verify-100-percent-test-coverage: the survivor passes the stale sweep,
        // but the installed identity set shrank 2 -> 1.
        v["verification_matrix"] = json!({
            "row-a": {
                "id": "verify-research-coverage",
                "result": "passed",
                "current": "passed",
                "evidence_refs": ["e"],
                "evidence_checkpoint": 5
            }
        });
    });
    run(&b, &n).assert_contains(
        "cr34-f1 terminal reshape dropping installed row",
        23,
        "lost_update field=verification_matrix missing=verify-100-percent-test-coverage",
    );
}

/// CR40-F1 regression (red-first): the terminal `termination_review`->`done`
/// reshape must preserve the INSTALLED row IDENTITY SET, not merely the row
/// COUNT. The installed baton carries the default two-row array matrix
/// {verify-research-coverage, verify-100-percent-test-coverage}; the done
/// candidate rebuilds it as an EQUAL-COUNT (2 -> 2) object that PRESERVES
/// verify-research-coverage but SUBSTITUTES verify-100-percent-test-coverage with
/// an unrelated fresh row. Every candidate row is fresh+complete (stale sweep
/// satisfied) and the count is equal (retired cardinality floor satisfied), yet
/// an installed identity was silently dropped. The identity-superset guard must
/// reject it as `missing=verify-100-percent-test-coverage`.
#[test]
fn cr40_f1_terminal_reshape_equal_count_substitution_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    make_baton_v3(
        &b,
        "termination_review",
        "team",
        4,
        configure_terminal_current,
    );
    make_baton_v3(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        explainer_verification_track(v);
        // Equal-count (2 -> 2) reshape, ALL rows fresh+complete, but the second
        // installed identity is SUBSTITUTED with an unrelated fresh row: the count
        // floor and the stale sweep both pass, yet an installed identity is gone.
        v["verification_matrix"] = json!({
            "row-a": {
                "id": "verify-research-coverage",
                "result": "passed",
                "current": "passed",
                "evidence_refs": ["e"],
                "evidence_checkpoint": 5
            },
            "row-x": {
                "id": "substitute-fresh-row",
                "result": "passed",
                "current": "passed",
                "evidence_refs": ["e"],
                "evidence_checkpoint": 5
            }
        });
    });
    run(&b, &n).assert_contains(
        "cr40-f1 terminal reshape equal-count substitution",
        23,
        "lost_update field=verification_matrix missing=verify-100-percent-test-coverage",
    );
}

/// Proves an EMPTY installed matrix has no identity to protect: a same-status
/// reshape from `[]` to a fresh object matrix is not a lost_update.
#[test]
fn cr21_f1_empty_current_matrix_is_clean() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!([]);
    });
    make_baton_v3(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!({"row-a": {"result": "passed"}});
        v["summary"] = json!("Team sync from an empty matrix to a new object matrix.");
        v["next_action"] = json!("Continue test creation.");
    });
    run(&b, &n).assert("cr21-f1 empty current matrix clean", 0);
}

// ---- CR21-F2: a metadata-free legacy track cannot ride a re-lap ------------

/// Proves a metadata-free legacy test-creation track first completed BEFORE the
/// implementation-family anchor is REJECTED when reused unchanged on a re-lap
/// (fixing-family status in cycle history), while the first-pass legacy case
/// (no fixing history) stays accepted byte-identically.
#[test]
fn cr21_f2_relap_legacy_track_requires_fresh_or_carry() {
    // RE-LAP: history shows a phase_fixing (anchor 5, re-lap signal); the legacy
    // track first completed at checkpoint 3 (< anchor) -> rejected.
    let relap = tmp();
    let (b, n) = paths(&relap);
    let legacy = legacy_or_unbounded_track("relap-legacy-test", false);
    write_snapshot(
        relap.path(),
        3,
        "test_creation",
        "1",
        json!({"subagent_tracks": [legacy.clone()]}),
    );
    write_impl_anchor(relap.path(), 5, "1"); // phase_fixing@5: re-lap + anchor 5
    make_baton_v3(&b, "test_creation", "team", 6, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v3(&n, "cross_review", "team", 7, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(v, "subagent_tracks", legacy.clone());
    });
    run(&b, &n).assert_contains(
        "cr21-f2 re-lap legacy track rejected",
        24,
        "test_creation->cross_review requires completed test-creation subagent_track",
    );

    // FIRST PASS: no fixing history -> the same metadata-free legacy track keeps
    // its byte-identical bypass and is accepted.
    let first = tmp();
    let (b2, n2) = paths(&first);
    make_baton_v3(&b2, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v3(&n2, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(
            v,
            "subagent_tracks",
            legacy_or_unbounded_track("first-pass-legacy-test", false),
        );
    });
    run(&b2, &n2).assert("cr21-f2 first-pass legacy track accepted", 0);
}

// ===== DR53-F1: type-tagged, count-aware matrix identity guard ============

/// DR53-F1(a) end-to-end through the write gate: a same-shape retry that drops
/// an ID-LESS installed array row is rejected. Exercises the positional
/// (`idx:<n>`) label path — the id-less row is now protected — and the
/// count-aware multiset comparison. The reported `missing=1` is the dropped
/// row's array position.
#[test]
fn dr53_f1_idless_array_row_drop_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!([
            { "id": "vm-a", "current": "pending" },
            { "note": "id-less stale row" }
        ]);
    });
    make_baton_v3(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Retry that drops the id-less matrix row.");
        v["next_action"] = json!("Team must retain the id-less row.");
        v["verification_matrix"] = json!([{ "id": "vm-a", "current": "pending" }]);
    });
    run(&b, &n).assert_contains(
        "dr53-f1 dropped id-less array row",
        23,
        "lost_update field=verification_matrix missing=1",
    );
}

/// DR53-F1 end-to-end: a same-shape OBJECT matrix retry that drops a stable KEY
/// is rejected. Exercises the same-shape object label path (`key:<k>`) and its
/// tag-stripped `missing=<key>` reason.
#[test]
fn dr53_f1_object_key_drop_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["verification_matrix"] = json!({
            "row-x": { "current": "pending" },
            "row-y": { "current": "pending" }
        });
    });
    make_baton_v3(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Retry that drops an installed object matrix key.");
        v["next_action"] = json!("Team must retain row-y.");
        v["verification_matrix"] = json!({ "row-x": { "current": "pending" } });
    });
    run(&b, &n).assert_contains(
        "dr53-f1 dropped object matrix key",
        23,
        "lost_update field=verification_matrix missing=row-y",
    );
}

/// DR53-F3 end-to-end: an installed baton with NO verification_matrix reaches
/// the team-sync lost_update guard with nothing to protect and is accepted —
/// exercising the absent/scalar (`_`) label arm of the shared labeller.
#[test]
fn dr53_f3_absent_installed_matrix_tolerated() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v.as_object_mut().unwrap().remove("verification_matrix");
    });
    make_baton_v3(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Retry from a baton that never installed a matrix.");
        v["next_action"] = json!("Continue test creation.");
    });
    run(&b, &n).assert("dr53-f3 absent installed matrix tolerated", 0);
}

// ---- VF75-F1: agent_instances pairwise write-path disjointness -------------

/// VF75-F1: two live (`planned`/`running`) generated `agent_instances`
/// sharing a `base_checkpoint`, with OVERLAPPING `write_paths` and no
/// `depends_on` serialization, fail the pairwise write-path disjointness gate
/// (`write_paths_overlap`, write.rs:3651). Differs from
/// `vf75_f1_agent_instances_disjoint_write_paths_accepted` only in the
/// `write_paths` values: both instances share `"src/vf75-shared.rs"` here,
/// making the pair OVERLAPPING, vs. pairwise-DISJOINT paths there — that
/// sibling test proves this fixture is otherwise valid, isolating the
/// rejection to the overlap itself.
#[test]
fn vf75_f1_agent_instances_overlapping_write_paths_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&n, "clarifying_questions_drafting", "vadi", 0, |v| {
        dynamic_agent_instances(v);
        v["agent_instances"][0]["status"] = json!("planned");
        v["agent_instances"][0]["write_paths"] = json!(["src/vf75-shared.rs"]);
        let mut second = v["agent_instances"][0].clone();
        second["id"] = json!("r3-generated-vf75-b");
        second["status"] = json!("running");
        second["write_paths"] = json!(["src/vf75-shared.rs"]);
        second["evidence_refs"] = json!(["subagent:r3-generated-vf75-b"]);
        second["output_refs"] = json!(["subagent_track:r3-generated-vf75-b"]);
        v["agent_instances"].as_array_mut().unwrap().push(second);
    });
    run(&b, &n).assert_contains(
        "vf75-f1 overlapping agent_instances write_paths rejected",
        23,
        "DVANDVA_WRITE bad_agent_instances_write_paths",
    );
}

/// VF75-F1 accept arm: the same fixture shape as above, but the two
/// instances' `write_paths` values are pairwise disjoint instead of
/// overlapping, so the same scaffold write succeeds — proving the reject
/// arm's failure is caused specifically by the overlap.
#[test]
fn vf75_f1_agent_instances_disjoint_write_paths_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v3(&n, "clarifying_questions_drafting", "vadi", 0, |v| {
        dynamic_agent_instances(v);
        v["agent_instances"][0]["status"] = json!("planned");
        v["agent_instances"][0]["write_paths"] = json!(["src/vf75-a.rs"]);
        let mut second = v["agent_instances"][0].clone();
        second["id"] = json!("r3-generated-vf75-b");
        second["status"] = json!("running");
        second["write_paths"] = json!(["src/vf75-b.rs"]);
        second["evidence_refs"] = json!(["subagent:r3-generated-vf75-b"]);
        second["output_refs"] = json!(["subagent_track:r3-generated-vf75-b"]);
        v["agent_instances"].as_array_mut().unwrap().push(second);
    });
    run(&b, &n).assert("vf75-f1 disjoint agent_instances write_paths accepted", 0);
}
