//! `dvandva write` — PROFILE / PATHS / AGENTS theme: agent_instances
//! write-path collisions, work_split write-path collisions, the depends_on
//! DAG, and the development profile matrix (fast/standard/full).
//!
//! Ported from `scripts/test-dvandva-write.sh`; each `#[test]` name mirrors
//! the shell case label. One deliberate change (design D6): the hard-path
//! floor set now keys the Rust source/test trees (`rust/dvandva/src/**`,
//! `rust/dvandva/tests/**`) instead of the three retired shell-script glob
//! patterns; cases that exercised those retired patterns are re-keyed to
//! Rust paths (see the report for the shell-path -> rust-path table) and two
//! new negative cases prove the old patterns are no longer hard.

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

/// Find a work_split chunk by id for in-place mutation (mirrors the shell's
/// `.work_split |= map(if .id == "..." then ... else . end)` idiom).
fn chunk_by_id<'a>(b: &'a mut Value, id: &str) -> &'a mut Value {
    b["work_split"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("chunk {id} not found in work_split"))
}

// ===========================================================================
// agent_instances write-path collisions
// ===========================================================================

#[test]
fn v2_agent_instance_write_path_collision_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        b["agent_instances"][0]["status"] = json!("running");
        b["agent_instances"][0]["write_paths"] = json!(["scripts/test-dvandva-write.sh"]);
        let mut second = b["agent_instances"][0].clone();
        second["id"] = json!("r3-generated-dynamic-review-b");
        second["status"] = json!("running");
        second["write_paths"] = json!(["scripts/test-dvandva-write.sh"]);
        second["evidence_refs"] = json!(["subagent:r3-generated-dynamic-review-b"]);
        second["output_refs"] = json!(["subagent_track:r3-generated-dynamic-review-b"]);
        b["agent_instances"].as_array_mut().unwrap().push(second);
    });
    run(&b, &n).assert_contains(
        "v2 agent_instance write path collision exits 23",
        23,
        "DVANDVA_WRITE bad_agent_instances_write_paths",
    );
}

#[test]
fn v2_agent_instance_unsafe_write_path_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        b["agent_instances"][0]["write_paths"] = json!(["../escape"]);
    });
    run(&b, &n).assert_contains(
        "v2 agent_instance unsafe write path exits 23",
        23,
        "DVANDVA_WRITE bad_agent_instances",
    );
}

#[test]
fn v2_agent_instance_write_path_prefix_collision_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        b["agent_instances"][0]["status"] = json!("running");
        b["agent_instances"][0]["write_paths"] = json!(["src/a"]);
        let mut second = b["agent_instances"][0].clone();
        second["id"] = json!("r3-generated-dynamic-review-b");
        second["status"] = json!("running");
        second["write_paths"] = json!(["src/a/b"]);
        second["evidence_refs"] = json!(["subagent:r3-generated-dynamic-review-b"]);
        second["output_refs"] = json!(["subagent_track:r3-generated-dynamic-review-b"]);
        b["agent_instances"].as_array_mut().unwrap().push(second);
    });
    run(&b, &n).assert_contains(
        "v2 agent_instance write path prefix collision exits 23",
        23,
        "DVANDVA_WRITE bad_agent_instances_write_paths",
    );
}

#[test]
fn v2_agent_instance_sibling_prefix_paths_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        b["agent_instances"][0]["status"] = json!("running");
        b["agent_instances"][0]["write_paths"] = json!(["src/a"]);
        let mut second = b["agent_instances"][0].clone();
        second["id"] = json!("r3-generated-dynamic-review-b");
        second["status"] = json!("running");
        second["write_paths"] = json!(["src/ab"]);
        second["evidence_refs"] = json!(["subagent:r3-generated-dynamic-review-b"]);
        second["output_refs"] = json!(["subagent_track:r3-generated-dynamic-review-b"]);
        b["agent_instances"].as_array_mut().unwrap().push(second);
    });
    run(&b, &n).assert("v2 agent_instance sibling prefix paths are accepted", 0);
}

#[test]
fn v2_six_agent_instances_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, many_agent_instances);
    run(&b, &n).assert(
        "v2 six generated agent_instances with collapsed mix are accepted",
        0,
    );
}

#[test]
fn v2_six_agent_instances_late_collision_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        many_agent_instances(b);
        b["agent_instances"][4]["status"] = json!("running");
        b["agent_instances"][5]["status"] = json!("running");
        b["agent_instances"][4]["write_paths"] = json!(["src/late"]);
        b["agent_instances"][5]["write_paths"] = json!(["src/late/sub"]);
    });
    run(&b, &n).assert_contains(
        "v2 six generated agent_instances catch late path collision",
        23,
        "DVANDVA_WRITE bad_agent_instances_write_paths",
    );
}

#[test]
fn v2_closed_agent_instances_same_base_collision_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        b["agent_instances"][0]["write_paths"] = json!(["scripts/test-dvandva-write.sh"]);
        let mut second = b["agent_instances"][0].clone();
        second["id"] = json!("r3-generated-dynamic-review-b");
        second["write_paths"] = json!(["scripts/test-dvandva-write.sh"]);
        second["evidence_refs"] = json!([
            "subagent:r3-generated-dynamic-review-b",
            "closed:r3-generated-dynamic-review-b"
        ]);
        second["output_refs"] = json!(["subagent_track:r3-generated-dynamic-review-b"]);
        b["agent_instances"].as_array_mut().unwrap().push(second);
    });
    run(&b, &n).assert_contains(
        "v2 closed agent_instances sharing base checkpoint still collide",
        23,
        "DVANDVA_WRITE bad_agent_instances_write_paths",
    );
}

#[test]
fn v2_running_agent_instances_prior_base_collision_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        b["agent_instances"][0]["status"] = json!("running");
        b["agent_instances"][0]["base_checkpoint"] = json!(5);
        b["agent_instances"][0]["spawned_at_checkpoint"] = json!(5);
        b["agent_instances"][0]["write_paths"] = json!(["scripts/test-dvandva-write.sh"]);
        b["agent_instances"][0]["evidence_refs"] = json!(["subagent:r3-generated-dynamic-review"]);
        let mut second = b["agent_instances"][0].clone();
        second["id"] = json!("r3-generated-dynamic-review-b");
        second["status"] = json!("running");
        second["base_checkpoint"] = json!(12);
        second["spawned_at_checkpoint"] = json!(12);
        second["write_paths"] = json!(["scripts/test-dvandva-write.sh"]);
        second["evidence_refs"] = json!(["subagent:r3-generated-dynamic-review-b"]);
        second["output_refs"] = json!(["subagent_track:r3-generated-dynamic-review-b"]);
        b["agent_instances"].as_array_mut().unwrap().push(second);
    });
    run(&b, &n).assert_contains(
        "v2 running historical agent_instances sharing write paths still collide",
        23,
        "DVANDVA_WRITE bad_agent_instances_write_paths",
    );
}

#[test]
fn v2_closed_agent_instances_prior_base_reuse_paths_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        b["agent_instances"][0]["base_checkpoint"] = json!(5);
        b["agent_instances"][0]["spawned_at_checkpoint"] = json!(5);
        b["agent_instances"][0]["write_paths"] = json!(["scripts/test-dvandva-write.sh"]);
        let mut second = b["agent_instances"][0].clone();
        second["id"] = json!("r3-generated-dynamic-review-b");
        second["base_checkpoint"] = json!(12);
        second["spawned_at_checkpoint"] = json!(12);
        second["write_paths"] = json!(["scripts/test-dvandva-write.sh"]);
        second["evidence_refs"] = json!([
            "subagent:r3-generated-dynamic-review-b",
            "closed:r3-generated-dynamic-review-b"
        ]);
        second["output_refs"] = json!(["subagent_track:r3-generated-dynamic-review-b"]);
        b["agent_instances"].as_array_mut().unwrap().push(second);
    });
    run(&b, &n).assert(
        "v2 closed historical agent_instances may reuse write paths",
        0,
    );
}

#[test]
fn v2_agent_instance_serialized_conflict_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        dynamic_agent_instances(b);
        b["agent_instances"][0]["status"] = json!("running");
        b["agent_instances"][0]["write_paths"] = json!(["scripts/test-dvandva-write.sh"]);
        let mut second = b["agent_instances"][0].clone();
        second["id"] = json!("r3-generated-dynamic-review-b");
        second["status"] = json!("running");
        second["depends_on"] = json!(["r3-generated-dynamic-review"]);
        second["write_paths"] = json!(["scripts/test-dvandva-write.sh"]);
        second["evidence_refs"] = json!(["subagent:r3-generated-dynamic-review-b"]);
        second["output_refs"] = json!(["subagent_track:r3-generated-dynamic-review-b"]);
        b["agent_instances"].as_array_mut().unwrap().push(second);
    });
    run(&b, &n).assert("v2 serialized agent_instance conflict is accepted", 0);
}

// ===========================================================================
// work_split write-path collisions
// ===========================================================================

#[test]
fn v2_work_split_bare_path_collision_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "parallel_implementing", "team", 4, |b| {
        parallel_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |b| {
        parallel_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] =
            json!("Team sync: candidate introduces a parallel implementation collision.");
        b["next_action"] = json!("Team: reject overlapping write intent before continuing.");
        chunk_by_id(b, "implementation-chunk-a")["paths"] = json!(["src/shared"]);
        chunk_by_id(b, "implementation-chunk-b")["paths"] = json!(["src/shared"]);
    });
    run(&b, &n).assert_contains(
        "v2 parallel work_split bare path collision exits 23",
        23,
        "DVANDVA_WRITE bad_work_split_write_paths",
    );
}

#[test]
fn v2_work_split_default_implementation_collision_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "parallel_implementing", "team", 4, |b| {
        parallel_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |b| {
        parallel_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] =
            json!("Team sync: candidate uses default implementation chunks with colliding paths.");
        b["next_action"] =
            json!("Team: reject missing chunk_type chunks as implementation write intent.");
        for id in ["implementation-chunk-a", "implementation-chunk-b"] {
            let c = chunk_by_id(b, id);
            c.as_object_mut().unwrap().remove("chunk_type");
            c["paths"] = json!(["src/default-impl.ts"]);
        }
    });
    run(&b, &n).assert_contains(
        "v2 default implementation chunks collide on bare paths",
        23,
        "DVANDVA_WRITE bad_work_split_write_paths",
    );
}

#[test]
fn v2_work_split_prefix_collision_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_fixing", "team", 4, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_fixing", "team", 5, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] =
            json!("Team sync: candidate introduces an ancestor-descendant write collision.");
        b["next_action"] = json!("Team: reject prefix-overlapping fix chunks.");
        b["work_split"][0]["paths"] = json!(["src/tree"]);
        b["work_split"][1]["paths"] = json!(["src/tree/child"]);
    });
    run(&b, &n).assert_contains(
        "v2 work_split prefix collision exits 23",
        23,
        "DVANDVA_WRITE bad_work_split_write_paths",
    );
}

#[test]
fn v2_work_split_empty_write_paths_cannot_mask_paths_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_fixing", "team", 4, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_fixing", "team", 5, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Team sync: empty write_paths must not mask write-capable paths.");
        b["next_action"] =
            json!("Team: reject colliding paths even when one chunk declares empty write_paths.");
        b["work_split"][0]["paths"] = json!(["src/masked.ts"]);
        b["work_split"][0]["write_paths"] = json!([]);
        b["work_split"][1]["paths"] = json!(["src/masked.ts"]);
    });
    run(&b, &n).assert_contains(
        "v2 work_split empty write_paths cannot mask paths collision",
        23,
        "DVANDVA_WRITE bad_work_split_write_paths",
    );
}

#[test]
fn v2_work_split_sibling_prefix_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_fixing", "team", 4, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_fixing", "team", 5, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Team sync: sibling prefixes should remain disjoint.");
        b["next_action"] = json!("Team: continue with non-overlapping sibling write paths.");
        b["work_split"][0]["paths"] = json!(["src/a"]);
        b["work_split"][1]["paths"] = json!(["src/ab"]);
    });
    run(&b, &n).assert("v2 work_split sibling prefix paths are accepted", 0);
}

#[test]
fn v2_work_split_serialized_conflict_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_fixing", "team", 4, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_fixing", "team", 5, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Team sync: serialized write conflict is intentional.");
        b["next_action"] =
            json!("Team: allow the dependent fix chunk to reuse the path after its dependency.");
        b["work_split"][0]["paths"] = json!(["src/shared-fix.ts"]);
        b["work_split"][1]["paths"] = json!(["src/shared-fix.ts"]);
        b["work_split"][0]["conflict_group"] = json!("fix-shared");
        b["work_split"][1]["conflict_group"] = json!("fix-shared");
        b["work_split"][1]["depends_on"] = json!(["cross-fixing-a"]);
    });
    run(&b, &n).assert("v2 serialized work_split conflict is accepted", 0);
}

#[test]
fn v2_work_split_conflict_group_without_depends_on_rejects() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_fixing", "team", 4, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_fixing", "team", 5, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Team sync: conflict_group alone must not serialize writers.");
        b["next_action"] =
            json!("Team: reject overlapping write chunks without an explicit dependency edge.");
        b["work_split"][0]["paths"] = json!(["src/shared-fix.ts"]);
        b["work_split"][1]["paths"] = json!(["src/shared-fix.ts"]);
        b["work_split"][0]["conflict_group"] = json!("fix-shared");
        b["work_split"][1]["conflict_group"] = json!("fix-shared");
    });
    run(&b, &n).assert_contains(
        "v2 work_split conflict_group without depends_on rejects",
        23,
        "DVANDVA_WRITE bad_work_split_write_paths",
    );
}

#[test]
fn v2_work_split_depends_on_without_conflict_group_rejects() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_fixing", "team", 4, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_fixing", "team", 5, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Team sync: depends_on alone must not serialize writers.");
        b["next_action"] =
            json!("Team: reject overlapping write chunks without a shared conflict group.");
        b["work_split"][0]["paths"] = json!(["src/shared-fix.ts"]);
        b["work_split"][1]["paths"] = json!(["src/shared-fix.ts"]);
        b["work_split"][1]["depends_on"] = json!(["cross-fixing-a"]);
    });
    run(&b, &n).assert_contains(
        "v2 work_split depends_on without conflict_group rejects",
        23,
        "DVANDVA_WRITE bad_work_split_write_paths",
    );
}

#[test]
fn v2_cross_review_read_overlap_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Team sync: cross-review overlaps are read-only by default.");
        b["next_action"] = json!("Team: continue with read-only cross-review coverage.");
        b["work_split"][0]["paths"] = json!(["src/shared-review.ts"]);
        b["work_split"][1]["paths"] = json!(["src/shared-review.ts"]);
    });
    run(&b, &n).assert("v2 cross_review overlapping read paths are accepted", 0);
}

#[test]
fn v2_cross_review_explicit_write_collision_rejects() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] =
            json!("Team sync: explicit write_paths should make cross-review collisions fail.");
        b["next_action"] = json!("Team: reject cross-review write collisions unless serialized.");
        b["work_split"][0]["write_paths"] = json!(["src/shared-review.ts"]);
        b["work_split"][1]["write_paths"] = json!(["src/shared-review.ts"]);
    });
    run(&b, &n).assert_contains(
        "v2 cross_review explicit write_paths collision rejects",
        23,
        "DVANDVA_WRITE bad_work_split_write_paths",
    );
}

#[test]
fn v2_team_sync_new_collision_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_fixing", "team", 4, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        let first = b["work_split"][0].clone();
        b["work_split"] = json!([first]);
    });
    make_baton_v2(&n, "cross_fixing", "team", 5, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Team sync: candidate adds a new colliding live fix chunk.");
        b["next_action"] =
            json!("Team: reject the sync because it introduces overlapping write ownership.");
        b["work_split"][0]["paths"] = json!(["src/live.ts"]);
        b["work_split"][1]["paths"] = json!(["src/live.ts"]);
    });
    run(&b, &n).assert_contains(
        "v2 team sync rejects newly introduced live work_split collision",
        23,
        "DVANDVA_WRITE bad_work_split_write_paths",
    );
}

#[test]
fn v2_work_split_terminal_reuse_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_fixing", "team", 4, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_fixing", "team", 5, |b| {
        cross_fixing_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Team sync: terminal chunks should not block later path reuse.");
        b["next_action"] =
            json!("Team: continue because the live fix chunk is reusing a completed path.");
        b["work_split"][0]["paths"] = json!(["src/reuse.ts"]);
        b["work_split"][0]["status"] = json!("completed");
        b["work_split"][1]["paths"] = json!(["src/reuse.ts"]);
        b["work_split"][1]["status"] = json!("planned");
    });
    run(&b, &n).assert("v2 terminal-aware work_split path reuse is accepted", 0);
}

#[test]
fn v2_work_split_empty_explicit_write_paths_keeps_intent_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "parallel_implementing", "team", 4, |b| {
        parallel_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |b| {
        parallel_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!(
            "Team sync: implementation paths still carry write intent with empty write_paths."
        );
        b["next_action"] = json!(
            "Team: continue because write_paths does not narrow paths for write-capable chunks."
        );
        chunk_by_id(b, "implementation-chunk-a")["write_paths"] = json!([]);
    });
    run(&b, &n).assert(
        "v2 implementation chunk with explicit empty write_paths keeps paths write intent",
        0,
    );
}

// ===========================================================================
// depends_on DAG
// ===========================================================================

#[test]
fn v2_work_split_dangling_depends_on_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "parallel_implementing", "team", 5, |b| {
        parallel_chunks(b);
        chunk_by_id(b, "implementation-chunk-a")["depends_on"] = json!(["missing-anchor"]);
    });
    run(&b, &n).assert_contains(
        "v2 work_split dangling depends_on exits 23",
        23,
        "bad_depends_on",
    );
}

#[test]
fn v2_work_split_depends_on_cycle_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "parallel_implementing", "team", 5, |b| {
        parallel_chunks(b);
        chunk_by_id(b, "implementation-chunk-a")["depends_on"] = json!(["implementation-chunk-b"]);
        chunk_by_id(b, "implementation-chunk-b")["depends_on"] = json!(["implementation-chunk-a"]);
    });
    run(&b, &n).assert_contains(
        "v2 work_split depends_on cycle exits 23",
        23,
        "bad_depends_on",
    );
}

#[test]
fn v2_work_split_anchor_depends_on_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "parallel_implementing", "team", 5, |b| {
        // S4-T2 (D2): the spec→implementation boundary locks the plan.
        b["master_plan_locked"] = json!(true);
        parallel_chunks(b);
        for item in b["work_split"].as_array_mut().unwrap() {
            if item["chunk_type"].as_str().unwrap_or("") == "implementation" {
                item["depends_on"] = json!(["spec-approved"]);
            }
        }
        push(
            b,
            "work_split",
            json!({
                "id": "test_creation",
                "phase": "1",
                "chunk_type": "test",
                "owner": "vadi",
                "owner_role": "vadi",
                "suggested_agent": "dvandva-test-creator",
                "scope": "Test gate follows parallel implementation.",
                "paths": ["scripts/test-dvandva-write.sh"],
                "write_paths": ["scripts/test-dvandva-write.sh"],
                "can_parallelize": false,
                "parallel_rationale": "Gate runs after implementation.",
                "depends_on": ["parallel_implementing"],
                "status": "planned",
                "artifact_refs": []
            }),
        );
    });
    run(&b, &n).assert("v2 work_split accepts fixed anchor depends_on refs", 0);
}

// ===========================================================================
// development profile matrix (fast / standard / full)
// ===========================================================================

/// Direct v2-seed scaffold candidate for the "new development default"
/// case: only the fields the shell's raw `jq` pipeline touches are
/// overridden; profile/profile_floor/profile_decision/profile_history stay
/// at the schema seed's defaults (standard/standard).
fn profile_default_scaffold_candidate() -> Value {
    let mut b = v2_seed();
    b["updated_at"] = json!("2026-07-01T00:00:00Z");
    b["mode"] = json!("development");
    b["run_id"] = json!("run-a");
    b["original_ask"] = json!("Profile default test");
    b["research_ref"] = json!("./superpowers/research/run-a.html");
    b["current_engine"] = json!("codex");
    b["branch"] = json!("test-branch");
    b["status"] = json!("research_drafting");
    b["assignee"] = json!("vadi");
    b["checkpoint"] = json!(0);
    b
}

#[test]
fn profile_new_development_scaffold_defaults_standard() {
    let d = tmp();
    let (b, n) = paths(&d);
    let cand = profile_default_scaffold_candidate();
    std::fs::create_dir_all(n.parent().unwrap()).unwrap();
    std::fs::write(&n, serde_json::to_string_pretty(&cand).unwrap()).unwrap();
    run(&b, &n).assert("profile new development scaffold defaults standard", 0);
    let installed: Value = serde_json::from_slice(&std::fs::read(&b).unwrap()).unwrap();
    assert_eq!(installed["profile"], "standard");
    assert_eq!(installed["profile_floor"], "standard");
    assert!(installed["profile_decision"].is_object());
    assert!(installed["profile_history"].is_array());
}

#[test]
fn profile_new_development_requires_metadata_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |b| {
        let o = b.as_object_mut().unwrap();
        o.remove("profile");
        o.remove("profile_floor");
        o.remove("profile_decision");
        o.remove("profile_history");
    });
    run(&b, &n).assert_contains(
        "new development scaffold missing profile metadata exits 23",
        23,
        "DVANDVA_WRITE bad_profile",
    );
}

#[test]
fn profile_bad_enum_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["profile"] = json!("turbo");
    });
    run(&b, &n).assert_contains("profile bad enum exits 23", 23, "DVANDVA_WRITE bad_profile");
}

#[test]
fn profile_decision_decided_by_blank_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["profile_decision"]["decided_by"] = json!("   ");
    });
    run(&b, &n).assert_contains(
        "profile_decision decided_by blank exits 23",
        23,
        "DVANDVA_WRITE bad_profile",
    );
}

#[test]
fn profile_floor_bad_enum_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["profile_floor"] = json!("turbo");
    });
    run(&b, &n).assert_contains(
        "profile_floor bad enum exits 23",
        23,
        "DVANDVA_WRITE bad_profile",
    );
}

#[test]
fn profile_decision_missing_key_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["profile_decision"]
            .as_object_mut()
            .unwrap()
            .remove("evidence_refs");
    });
    run(&b, &n).assert_contains(
        "profile_decision missing required key exits 23",
        23,
        "DVANDVA_WRITE bad_profile",
    );
}

#[test]
fn profile_decision_selected_profile_mismatch_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["profile_decision"]["selected_profile"] = json!("standard");
    });
    run(&b, &n).assert_contains(
        "profile_decision selected_profile mismatch exits 23",
        23,
        "DVANDVA_WRITE bad_profile",
    );
}

#[test]
fn profile_decision_floor_mismatch_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["profile_decision"]["floor"] = json!("standard");
    });
    run(&b, &n).assert_contains(
        "profile_decision floor mismatch exits 23",
        23,
        "DVANDVA_WRITE bad_profile",
    );
}

#[test]
fn profile_history_bad_entry_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["profile_history"] = json!([{
            "from": "fast", "to": "standard", "floor": "standard", "checkpoint": 5,
            "actor_role": "bot", "reason": "invalid actor", "evidence_refs": []
        }]);
    });
    run(&b, &n).assert_contains(
        "profile_history malformed entry exits 23",
        23,
        "DVANDVA_WRITE bad_profile",
    );
}

#[test]
fn profile_missing_existing_dev_effective_full_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    let strip = |b: &mut Value| {
        let o = b.as_object_mut().unwrap();
        o.remove("profile");
        o.remove("profile_floor");
        o.remove("profile_decision");
        o.remove("profile_history");
    };
    make_baton_v2(&b, "research_review", "prativadi", 4, strip);
    make_baton_v2(&n, "spec_drafting", "vadi", 5, strip);
    run(&b, &n).assert(
        "missing profile on existing development baton keeps full-compatible edge legal",
        0,
    );
}

#[test]
fn profile_feature_pr_missing_effective_full_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    let strip = |b: &mut Value| {
        b["mode"] = json!("feature-pr");
        let o = b.as_object_mut().unwrap();
        o.remove("profile");
        o.remove("profile_floor");
        o.remove("profile_decision");
        o.remove("profile_history");
    };
    make_baton_v2(&b, "research_review", "prativadi", 4, strip);
    make_baton_v2(&n, "spec_drafting", "vadi", 5, strip);
    run(&b, &n).assert(
        "missing profile on existing feature-pr baton keeps full-compatible edge legal",
        0,
    );
}

#[test]
fn profile_missing_existing_dev_full_only_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    let strip = |b: &mut Value| {
        let o = b.as_object_mut().unwrap();
        o.remove("profile");
        o.remove("profile_floor");
        o.remove("profile_decision");
        o.remove("profile_history");
    };
    make_baton_v2(&b, "spec_review", "prativadi", 4, strip);
    make_baton_v2(&n, "parallel_implementing", "team", 5, |b| {
        strip(b);
        // S4-T2 (D2): the spec→implementation boundary locks the plan.
        b["master_plan_locked"] = json!(true);
        parallel_chunks(b);
    });
    run(&b, &n).assert(
        "missing profile on existing development baton permits full-only edge",
        0,
    );
}

#[test]
fn profile_missing_existing_dev_standard_edge_rejected_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    let strip = |b: &mut Value| {
        let o = b.as_object_mut().unwrap();
        o.remove("profile");
        o.remove("profile_floor");
        o.remove("profile_decision");
        o.remove("profile_history");
    };
    make_baton_v2(&b, "spec_review", "prativadi", 4, strip);
    make_baton_v2(&n, "implementing", "vadi", 5, |b| {
        strip(b);
        b["phase"] = json!(1);
        b["master_plan_locked"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "missing profile on existing development baton rejects standard-only edge",
        24,
        "no legal edge spec_review->implementing",
    );
}

#[test]
fn profile_fast_allowlist_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
        push(
            b,
            "verification",
            json!({"command": "bash scripts/test-dvandva-write.sh", "result": "passed", "notes": "fast verification evidence"}),
        );
    });
    run(&b, &n).assert(
        "profile fast allowlist implementing:phase_review is legal",
        0,
    );
}

#[test]
fn profile_fast_research_drafting_review_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_drafting", "vadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "research_review", "prativadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    run(&b, &n).assert("profile fast research_drafting:research_review is legal", 0);
}

#[test]
fn profile_fast_research_review_implementing_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
        b["phase"] = json!(1);
        b["master_plan_locked"] = json!(true);
    });
    run(&b, &n).assert("profile fast research_review:implementing is legal", 0);
}

#[test]
fn profile_fast_phase_review_fixing_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_review", "prativadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
        b["loop_counts"] = json!({"phase_review:phase_fixing": 0});
    });
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
        b["loop_counts"] = json!({"phase_review:phase_fixing": 1});
    });
    run(&b, &n).assert("profile fast phase_review:phase_fixing is legal", 0);
}

#[test]
fn profile_fast_phase_fixing_review_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_fixing", "vadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    run(&b, &n).assert("profile fast phase_fixing:phase_review is legal", 0);
}

#[test]
fn profile_fast_phase_review_termination_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_review", "prativadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "termination_review", "team", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")])
        .assert("profile fast phase_review:termination_review is legal", 0);
}

#[test]
fn profile_fast_termination_fixing_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["loop_counts"] = json!({"termination_review:phase_fixing": 0});
    });
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
        b["loop_counts"] = json!({"termination_review:phase_fixing": 1});
    });
    run(&b, &n).assert("profile fast termination_review:phase_fixing is legal", 0);
}

#[test]
fn profile_fast_done_without_explainer_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        compact_terminal_evidence(b);
        b["changed_paths"] = json!(["README.md"]);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        compact_terminal_evidence(b);
        b["changed_paths"] = json!(["README.md"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert(
        "profile fast termination_review:done does not require run_explainer_ref",
        0,
    );
}

#[test]
fn profile_fast_done_requires_verification_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        compact_terminal_evidence(b);
        b["changed_paths"] = json!(["README.md"]);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        compact_terminal_evidence(b);
        b["verification"] = json!([]);
        b["changed_paths"] = json!(["README.md"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "profile fast done requires final verification evidence",
        23,
        "DVANDVA_WRITE bad_compact_terminal_evidence",
    );
}

#[test]
fn profile_fast_done_requires_review_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        compact_terminal_evidence(b);
        b["changed_paths"] = json!(["README.md"]);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        compact_terminal_evidence(b);
        if let Some(tracks) = b["subagent_tracks"].as_array_mut() {
            tracks.retain(|t| t["id"] != "compact-phase-review");
        }
        b["changed_paths"] = json!(["README.md"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "profile fast done requires independent review evidence",
        23,
        "DVANDVA_WRITE bad_compact_terminal_evidence",
    );
}

#[test]
fn profile_fast_done_rejects_missing_phase_review_checkpoint_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        compact_terminal_evidence(b);
        b["changed_paths"] = json!(["README.md"]);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        compact_terminal_evidence(b);
        if let Some(tracks) = b["subagent_tracks"].as_array_mut() {
            for t in tracks.iter_mut() {
                if t["track"] == "phase-review" {
                    t.as_object_mut().unwrap().remove("review_checkpoint");
                }
            }
        }
        b["changed_paths"] = json!(["README.md"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "profile fast done requires current phase-review checkpoint evidence",
        23,
        "DVANDVA_WRITE bad_compact_terminal_evidence",
    );
}

#[test]
fn profile_fast_done_rejects_stale_phase_review_checkpoint_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        compact_terminal_evidence(b);
        b["changed_paths"] = json!(["README.md"]);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        compact_terminal_evidence(b);
        if let Some(tracks) = b["subagent_tracks"].as_array_mut() {
            for t in tracks.iter_mut() {
                if t["track"] == "phase-review" {
                    t["review_checkpoint"] = json!(3);
                }
            }
        }
        b["changed_paths"] = json!(["README.md"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "profile fast done rejects stale phase-review checkpoint evidence",
        23,
        "DVANDVA_WRITE bad_compact_terminal_evidence",
    );
}

#[test]
fn profile_fast_without_allowlist_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
        b["profile_decision"]["allowlist_match"] = json!(false);
    });
    run(&b, &n).assert_contains(
        "profile fast without allowlist match exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

// ---- D6 re-key: retired shell-script glob patterns -> Rust source/test trees ----

#[test]
fn profile_fast_hard_risk_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |b| {
        fast_profile(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |b| {
        fast_profile(b);
        b["changed_paths"] = json!(["rust/dvandva/src/write.rs"]);
    });
    run(&b, &n).assert_contains(
        "profile fast hard-risk path exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_fast_protocol_doc_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["docs/protocol/local-baton-channel.md"]);
    });
    run(&b, &n).assert_contains(
        "profile fast protocol doc hard-risk path exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_fast_agent_write_not_allowlisted_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        dynamic_agent_instances(b);
        b["changed_paths"] = json!(["README.md"]);
        b["agent_instances"][0]["read_paths"] = json!(["README.md"]);
        b["agent_instances"][0]["write_paths"] = json!(["docs/workflows/probe.md"]);
    });
    run(&b, &n).assert_contains(
        "profile fast agent write path outside allowlist exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_fast_work_split_read_not_allowlisted_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
        b["work_split"][0]["read_paths"] = json!(["docs/workflows/probe.md"]);
    });
    run(&b, &n).assert_contains(
        "profile fast work_split read path outside allowlist exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_fast_agent_read_not_allowlisted_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |b| {
        fast_profile(b);
        fast_allowlist_work_split(b);
        dynamic_agent_instances(b);
        b["changed_paths"] = json!(["README.md"]);
        b["agent_instances"][0]["read_paths"] = json!(["docs/workflows/probe.md"]);
        b["agent_instances"][0]["write_paths"] = json!(["README.md"]);
    });
    run(&b, &n).assert_contains(
        "profile fast agent read path outside allowlist exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_standard_role_skill_hard_risk_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["changed_paths"] = json!(["plugins/dvandva/skills/vadi/SKILL.md"]);
    });
    run(&b, &n).assert_contains(
        "profile standard role skill hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_standard_product_spec_hard_risk_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["changed_paths"] = json!(["product.md"]);
    });
    run(&b, &n).assert_contains(
        "profile standard product spec hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_full_hard_risk_low_floor_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, fast_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        full_low_floor_profile(b);
        b["changed_paths"] = json!(["plugins/dvandva/skills/vadi/SKILL.md"]);
    });
    run(&b, &n).assert_contains(
        "profile full hard-risk path still requires full floor",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_full_hard_risk_decision_low_floor_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["profile"] = json!("full");
        b["profile_floor"] = json!("full");
        b["profile_decision"] = json!({
            "selected_profile": "full",
            "floor": "standard",
            "reason": "test hard-risk path with decision floor left too low",
            "decided_by": "test-suite",
            "decided_at": "2026-07-01T00:00:00Z",
            "risk_inputs": ["changed_paths"],
            "hard_triggers": ["plugins/dvandva/skills/vadi/SKILL.md"],
            "allowlist_match": false,
            "allowlist_refs": [],
            "evidence_refs": ["test:hard-risk-decision-low-floor"]
        });
        b["profile_history"] = json!([{
            "from": "standard", "to": "full", "floor": "full", "checkpoint": 5,
            "actor_role": "vadi", "reason": "hard-risk path detected",
            "evidence_refs": ["test:hard-risk-decision-low-floor"]
        }]);
        b["changed_paths"] = json!(["plugins/dvandva/skills/vadi/SKILL.md"]);
    });
    run(&b, &n).assert_contains(
        "profile full hard-risk path requires profile_decision floor consistency",
        23,
        "DVANDVA_WRITE bad_profile",
    );
}

#[test]
fn profile_standard_env_hard_risk_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["changed_paths"] = json!([".env"]);
    });
    run(&b, &n).assert_contains(
        "profile standard env hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_standard_hard_risk_work_split_path_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["work_split"][0]["paths"] = json!(["plugins/dvandva/references/baton-schema-v2.json"]);
    });
    run(&b, &n).assert_contains(
        "profile standard work_split paths hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_standard_hard_risk_work_split_read_path_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["work_split"][0]["read_paths"] =
            json!(["plugins/dvandva/references/state-transition-table.md"]);
    });
    run(&b, &n).assert_contains(
        "profile standard work_split read_paths hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_standard_hard_risk_work_split_write_path_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["work_split"][0]["write_paths"] = json!(["templates/channel/baton.json"]);
    });
    run(&b, &n).assert_contains(
        "profile standard work_split write_paths hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

/// D6 re-key: shell used `read_paths: ["scripts/test-dvandva-write.sh"]` ->
/// rust path `rust/dvandva/tests/write_lock.rs`.
#[test]
fn profile_standard_hard_risk_agent_read_path_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["agent_instances"] = json!([{
            "id": "profile-risk-probe",
            "read_paths": ["rust/dvandva/tests/write_lock.rs"],
            "write_paths": []
        }]);
    });
    run(&b, &n).assert_contains(
        "profile standard agent read_paths hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

/// D6 re-key: shell used
/// `write_paths: ["plugins/dvandva/skills/vadi/scripts/dvandva-state.sh"]`
/// -> rust path `rust/dvandva/src/state.rs`.
#[test]
fn profile_standard_hard_risk_agent_write_path_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["agent_instances"] = json!([{
            "id": "profile-risk-probe",
            "read_paths": [],
            "write_paths": ["rust/dvandva/src/state.rs"]
        }]);
    });
    run(&b, &n).assert_contains(
        "profile standard agent write_paths hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

// Replaces the shell's role-helper loop and top-level-script loop (which
// enumerated retired `dvandva-*.sh` / `scripts/*.sh` helpers) with concrete
// cases proving the new Rust source/test trees are hard.

#[test]
fn profile_standard_rust_src_lock_hard_risk_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["changed_paths"] = json!(["rust/dvandva/src/lock.rs"]);
    });
    run(&b, &n).assert_contains(
        "profile standard rust src lock.rs hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_standard_rust_cmd_write_hard_risk_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["changed_paths"] = json!(["rust/dvandva/src/cmd/write.rs"]);
    });
    run(&b, &n).assert_contains(
        "profile standard rust src cmd write.rs hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

#[test]
fn profile_standard_rust_tests_write_paths_hard_risk_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["changed_paths"] = json!(["rust/dvandva/tests/write_paths.rs"]);
    });
    run(&b, &n).assert_contains(
        "profile standard rust tests write_paths.rs hard-risk exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

/// New positive case: `rust/dvandva/tests/**` is hard under the fast profile
/// too (companion to `profile_fast_hard_risk_rejected_exits_23`, which
/// already proves `rust/dvandva/src/**` is hard under fast).
#[test]
fn profile_fast_rust_tests_hard_risk_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |b| {
        fast_profile(b);
        b["changed_paths"] = json!(["README.md"]);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |b| {
        fast_profile(b);
        b["changed_paths"] = json!(["rust/dvandva/tests/write_paths.rs"]);
    });
    run(&b, &n).assert_contains(
        "profile fast rust tests hard-risk path exits 23",
        23,
        "DVANDVA_WRITE bad_profile_floor",
    );
}

/// New negative case: the retired top-level `scripts/*.sh` glob is no longer
/// hard under D6.
#[test]
fn profile_standard_old_top_level_script_pattern_not_hard_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["changed_paths"] = json!(["scripts/test-install.sh"]);
    });
    run(&b, &n).assert(
        "profile standard old top-level script pattern is no longer hard",
        0,
    );
}

/// New negative case: the retired `plugins/dvandva/skills/*/scripts/dvandva-*.sh`
/// glob is no longer hard under D6.
#[test]
fn profile_standard_old_role_skill_script_pattern_not_hard_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["changed_paths"] = json!(["plugins/dvandva/skills/vadi/scripts/dvandva-write.sh"]);
    });
    run(&b, &n).assert(
        "profile standard old role skill script pattern is no longer hard",
        0,
    );
}

// ---- standard-profile compact edges ----

#[test]
fn profile_standard_compact_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "implementing", "vadi", 5, |b| {
        standard_profile(b);
        b["phase"] = json!(1);
        b["master_plan_locked"] = json!(true);
    });
    run(&b, &n).assert("profile standard spec_review:implementing is legal", 0);
}

#[test]
fn profile_standard_research_spec_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "spec_drafting", "vadi", 5, standard_profile);
    run(&b, &n).assert("profile standard research_review:spec_drafting is legal", 0);
}

#[test]
fn profile_standard_spec_revision_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "spec_revision", "vadi", 5, standard_profile);
    run(&b, &n).assert("profile standard spec_review:spec_revision is legal", 0);
}

#[test]
fn profile_standard_implementing_review_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |b| {
        standard_profile(b);
        b["phase"] = json!(1);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |b| {
        standard_profile(b);
        b["phase"] = json!(1);
    });
    run(&b, &n).assert("profile standard implementing:phase_review is legal", 0);
}

#[test]
fn profile_standard_review_fixing_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_review", "prativadi", 4, |b| {
        standard_profile(b);
        b["phase"] = json!(1);
        b["loop_counts"] = json!({"phase_review:phase_fixing": 0});
    });
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |b| {
        standard_profile(b);
        b["phase"] = json!(1);
        b["loop_counts"] = json!({"phase_review:phase_fixing": 1});
    });
    run(&b, &n).assert("profile standard phase_review:phase_fixing is legal", 0);
}

#[test]
fn profile_standard_review_termination_edge_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_review", "prativadi", 4, |b| {
        standard_profile(b);
        b["phase"] = json!(1);
    });
    make_baton_v2(&n, "termination_review", "team", 5, |b| {
        standard_profile(b);
        b["phase"] = json!(1);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert(
        "profile standard phase_review:termination_review is legal",
        0,
    );
}

#[test]
fn profile_standard_done_without_explainer_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert(
        "profile standard done does not require run_explainer_ref",
        0,
    );
}

#[test]
fn profile_standard_done_requires_verification_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        if let Some(matrix) = b["verification_matrix"].as_array_mut() {
            for m in matrix {
                m["current"] = json!("pending");
                m["evidence_refs"] = json!([]);
            }
        }
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "profile standard done requires completed verification matrix",
        23,
        "DVANDVA_WRITE bad_compact_terminal_evidence",
    );
}

#[test]
fn profile_standard_done_requires_review_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        if let Some(tracks) = b["subagent_tracks"].as_array_mut() {
            tracks.retain(|t| t["track"] != "phase-review");
        }
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "profile standard done requires independent review evidence",
        23,
        "DVANDVA_WRITE bad_compact_terminal_evidence",
    );
}

#[test]
fn profile_standard_done_rejects_missing_phase_review_checkpoint_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        if let Some(tracks) = b["subagent_tracks"].as_array_mut() {
            for t in tracks.iter_mut() {
                if t["track"] == "phase-review" {
                    t.as_object_mut().unwrap().remove("review_checkpoint");
                }
            }
        }
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "profile standard done requires current phase-review checkpoint evidence",
        23,
        "DVANDVA_WRITE bad_compact_terminal_evidence",
    );
}

#[test]
fn profile_standard_done_rejects_stale_phase_review_checkpoint_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        if let Some(tracks) = b["subagent_tracks"].as_array_mut() {
            for t in tracks.iter_mut() {
                if t["track"] == "phase-review" {
                    t["review_checkpoint"] = json!(3);
                }
            }
        }
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "profile standard done rejects stale phase-review checkpoint evidence",
        23,
        "DVANDVA_WRITE bad_compact_terminal_evidence",
    );
}

#[test]
fn profile_standard_done_rejects_generic_cross_review_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |b| {
        standard_profile(b);
        compact_terminal_evidence(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |b| {
        standard_profile(b);
        b["verification"] = json!([
            {"command": "bash scripts/test-dvandva-write.sh", "result": "passed", "notes": "compact terminal verification"}
        ]);
        if let Some(matrix) = b["verification_matrix"].as_array_mut() {
            for m in matrix {
                m["current"] = json!("passed");
                m["evidence_refs"] = json!(["command:bash scripts/test-dvandva-write.sh"]);
            }
        }
        push(
            b,
            "subagent_tracks",
            json!({
                "id": "generic-cross-review-not-compact-phase-review",
                "phase": "phase_review",
                "track": "cross-review",
                "owner": "dvandva-cross-reviewer",
                "status": "completed",
                "result": "approved",
                "parallelized": false,
                "rationale": "Generic cross-review evidence must not satisfy compact terminal phase-review gate.",
                "inputs": ["profile compact implementation"],
                "outputs": ["Generic cross-review approved something."],
                "evidence_refs": ["test:generic-cross-review"]
            }),
        );
        b["vadi_final_approval"] = json!(true);
        b["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "profile standard done requires prativadi phase-review evidence, not generic cross-review",
        23,
        "DVANDVA_WRITE bad_compact_terminal_evidence",
    );
}

// ---- escalation / downgrade / history ----

#[test]
fn profile_escalation_history_required_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, fast_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["profile_history"] = json!([]);
    });
    run(&b, &n).assert_contains(
        "profile escalation requires profile_history entry",
        23,
        "DVANDVA_WRITE bad_profile_history",
    );
}

#[test]
fn profile_history_append_only_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        fast_profile(b);
        b["profile_history"] = json!([{
            "from": null, "to": "fast", "floor": "fast", "checkpoint": 2,
            "actor_role": "vadi", "reason": "initial fast selection",
            "evidence_refs": ["test:initial-fast"]
        }]);
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["profile_history"] = json!([{
            "from": "fast", "to": "standard", "floor": "standard", "checkpoint": 5,
            "actor_role": "vadi", "reason": "risk increased after review",
            "evidence_refs": ["test:profile-escalation"]
        }]);
    });
    run(&b, &n).assert_contains(
        "profile_history preserves prior entries during escalation",
        23,
        "DVANDVA_WRITE bad_profile_history",
    );
}

#[test]
fn profile_escalation_history_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, fast_profile);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["profile_history"] = json!([{
            "from": "fast", "to": "standard", "floor": "standard", "checkpoint": 5,
            "actor_role": "vadi", "reason": "risk increased after review",
            "evidence_refs": ["test:profile-escalation"]
        }]);
    });
    run(&b, &n).assert("profile escalation with profile_history entry is legal", 0);
}

#[test]
fn profile_downgrade_below_floor_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        b["profile"] = json!("full");
        b["profile_floor"] = json!("full");
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        standard_profile(b);
        b["profile_floor"] = json!("full");
        b["profile_decision"]["floor"] = json!("full");
    });
    run(&b, &n).assert_contains(
        "profile downgrade below floor exits 23",
        23,
        "DVANDVA_WRITE bad_profile_downgrade",
    );
}

#[test]
fn profile_floor_lowering_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        b["profile"] = json!("full");
        b["profile_floor"] = json!("full");
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["profile"] = json!("full");
        b["profile_floor"] = json!("standard");
        b["profile_decision"] = json!({
            "selected_profile": "full",
            "floor": "standard",
            "reason": "test illegal floor lowering",
            "decided_by": "test-suite",
            "decided_at": "2026-07-01T00:00:00Z",
            "risk_inputs": [],
            "hard_triggers": [],
            "allowlist_match": false,
            "allowlist_refs": [],
            "evidence_refs": ["test:floor-lowering"]
        });
        b["profile_history"] = json!([{
            "from": "full", "to": "full", "floor": "standard", "checkpoint": 5,
            "actor_role": "vadi", "reason": "attempted floor lowering",
            "evidence_refs": ["test:floor-lowering"]
        }]);
    });
    run(&b, &n).assert_contains(
        "profile_floor lowering below current floor exits 23",
        23,
        "DVANDVA_WRITE bad_profile_downgrade",
    );
}

#[test]
fn profile_history_only_floor_lowering_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        b["profile"] = json!("full");
        b["profile_floor"] = json!("full");
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        b["profile"] = json!("full");
        b["profile_floor"] = json!("full");
        b["profile_decision"] = json!({
            "selected_profile": "full",
            "floor": "full",
            "reason": "test history-only floor lowering",
            "decided_by": "test-suite",
            "decided_at": "2026-07-01T00:00:00Z",
            "risk_inputs": [],
            "hard_triggers": [],
            "allowlist_match": false,
            "allowlist_refs": [],
            "evidence_refs": ["test:history-only-floor-lowering"]
        });
        b["profile_history"] = json!([{
            "from": "full", "to": "full", "floor": "standard", "checkpoint": 5,
            "actor_role": "vadi", "reason": "attempted hidden floor lowering",
            "evidence_refs": ["test:history-only-floor-lowering"]
        }]);
    });
    run(&b, &n).assert_contains(
        "profile_history cannot append floor below current floor",
        23,
        "DVANDVA_WRITE bad_profile_downgrade",
    );
}

#[test]
fn profile_history_old_lower_floor_compatible() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, old_low_floor_history);
    make_baton_v2(&n, "research_revision", "vadi", 5, old_low_floor_history);
    run(&b, &n).assert(
        "existing profile_history entry below current floor remains compatible",
        0,
    );
}

#[test]
fn profile_history_duplicate_old_lower_floor_rejected_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, old_low_floor_history);
    make_baton_v2(&n, "research_revision", "vadi", 5, |b| {
        old_low_floor_history(b);
        let first = b["profile_history"][0].clone();
        b["profile_history"].as_array_mut().unwrap().push(first);
    });
    run(&b, &n).assert_contains(
        "profile_history cannot append duplicate old lower-floor entry",
        23,
        "DVANDVA_WRITE bad_profile_downgrade",
    );
}

#[test]
fn profile_downgrade_human_decision_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |b| {
        b["profile"] = json!("full");
        b["profile_floor"] = json!("full");
    });
    make_baton_v2(&n, "human_decision", "human", 5, |b| {
        standard_profile(b);
        b["profile_floor"] = json!("full");
        b["profile_decision"]["floor"] = json!("full");
    });
    run(&b, &n).assert(
        "profile downgrade below floor may route to human_decision",
        0,
    );
}
