//! `dvandva write` — transitions / edges / human states / same-status / loop /
//! approvals themes.
//!
//! Ported from `scripts/test-dvandva-write.sh`; each `#[test]` name mirrors the
//! shell case label (or the shell loop that generates it).

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

fn set_loop_count(b: &mut Value, edge: &str, n: i64) {
    let mut map = serde_json::Map::new();
    map.insert(edge.to_string(), json!(n));
    b["loop_counts"] = Value::Object(map);
}

// ===================== v1 edges legal =====================

#[test]
fn v1_edges_legal() {
    let edges = [
        ("spec_drafting", "spec_review"),
        ("spec_review", "spec_revision"),
        ("spec_review", "implementing"),
        ("spec_revision", "spec_review"),
        ("implementing", "phase_review"),
        ("phase_review", "phase_fixing"),
        ("phase_review", "review_of_review"),
        ("phase_review", "implementing"),
        ("phase_review", "done"),
        ("phase_fixing", "phase_review"),
        ("review_of_review", "implementing"),
        ("review_of_review", "done"),
        ("review_of_review", "counter_review"),
        ("counter_review", "implementing"),
        ("counter_review", "done"),
        ("counter_review", "review_of_review"),
    ];
    for (cur, new) in edges {
        let d = tmp();
        let (b, n) = paths(&d);
        make_baton(&b, cur, "vadi", 4, |_| {});
        make_baton(&n, new, "prativadi", 5, |v| {
            if new == "done" {
                v["vadi_final_approval"] = json!(true);
                v["prativadi_final_approval"] = json!(true);
            }
        });
        run(&b, &n).assert(&format!("edge {cur}:{new} is legal"), 0);
    }
}

// ===================== v2 edges legal =====================

const V2_EDGE_LOOP_COUNT_EDGES: &[&str] = &[
    "deep_review:phase_fixing",
    "cross_review:cross_fixing",
    "termination_review:phase_fixing",
    "review_of_review:counter_review",
    "counter_review:review_of_review",
];

#[test]
fn v2_edges_legal() {
    let edges: &[&str] = &[
        "research_drafting:research_review",
        "research_review:research_revision",
        "research_revision:research_review",
        "research_review:spec_drafting",
        "spec_drafting:spec_review",
        "spec_review:spec_revision",
        "spec_review:parallel_implementing",
        "spec_revision:spec_review",
        "parallel_implementing:test_creation",
        "test_creation:cross_review",
        "cross_review:cross_fixing",
        "cross_fixing:test_creation",
        "cross_review:deep_review",
        "deep_review:phase_fixing",
        "deep_review:review_of_review",
        "deep_review:deslop",
        "review_of_review:counter_review",
        "review_of_review:deslop",
        "counter_review:review_of_review",
        "counter_review:deslop",
        "phase_fixing:test_creation",
        "deslop:phase_fixing",
        "deslop:parallel_implementing",
        "deslop:termination_review",
        "termination_review:phase_fixing",
        "termination_review:done",
    ];
    for edge in edges.iter().copied() {
        let (cur, new) = edge.split_once(':').unwrap();
        let d = tmp();
        let (b, n) = paths(&d);
        // S4-T1: the done gate resolves required refs to real files.
        seed_done_artifacts(d.path());
        make_baton_v2(&b, cur, v2_status_owner(cur), 4, |v| {
            if cur == "review_of_review" {
                v["review_target"] = json!("prativadi_fixups");
                v["narrow_fixups"] = json!(["test fixup"]);
            }
            if cur == "counter_review" {
                v["review_target"] = json!("vadi_counter");
                v["vadi_counter"] = json!(["counter change"]);
            }
            if V2_EDGE_LOOP_COUNT_EDGES.contains(&edge) {
                set_loop_count(v, edge, 0);
            }
            if edge == "termination_review:done" {
                v["active_roles"] = json!(["vadi", "prativadi"]);
                v["run_explainer_ref"] =
                    json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
                v["vadi_final_approval"] = json!(true);
                v["prativadi_final_approval"] = json!(true);
                run_explainer_reviews(v);
            }
        });
        make_baton_v2(&n, new, v2_status_owner(new), 5, |v| {
            if edge == "deep_review:deslop" || edge == "deep_review:review_of_review" {
                review_angles(v);
            }
            if new == "review_of_review" {
                v["review_target"] = json!("prativadi_fixups");
                v["narrow_fixups"] = json!(["test fixup"]);
            }
            if new == "counter_review" {
                v["review_target"] = json!("vadi_counter");
                v["vadi_counter"] = json!(["counter change"]);
            }
            if new == "parallel_implementing" {
                parallel_chunks(v);
            }
            // S4-T2 (D2): the spec→implementation boundary must lock the plan.
            if edge == "spec_review:parallel_implementing" {
                v["master_plan_locked"] = json!(true);
            }
            if new == "cross_review" || new == "cross_fixing" || new == "test_creation" {
                v["active_roles"] = json!(["vadi", "prativadi"]);
            }
            if new == "termination_review" {
                v["active_roles"] = json!(["vadi", "prativadi"]);
                v["vadi_final_approval"] = json!(true);
            }
            if edge == "test_creation:cross_review" {
                test_creation_track(v);
            }
            if edge == "cross_review:cross_fixing" {
                cross_review_finding(v);
            }
            if V2_EDGE_LOOP_COUNT_EDGES.contains(&edge) {
                set_loop_count(v, edge, 1);
            }
            if edge == "parallel_implementing:test_creation" {
                parallel_chunks(v);
                v["active_roles"] = json!(["vadi", "prativadi"]);
                implementation_tracks(v);
            }
            if edge == "cross_review:deep_review" {
                cross_review_tracks(v);
            }
            if edge == "termination_review:done" {
                v["run_explainer_ref"] =
                    json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
                v["vadi_final_approval"] = json!(true);
                v["prativadi_final_approval"] = json!(true);
                run_explainer_reviews(v);
                explainer_verification_track(v); // F10
                done_matrix_fresh(v); // S4-T6
            }
        });
        let name = format!("v2 edge {edge} is legal");
        if edge == "deslop:termination_review" {
            run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert(&name, 0);
        } else {
            run(&b, &n).assert(&name, 0);
        }
    }
}

// ===================== schema / run_id / mode change =====================

#[test]
fn v2_current_cannot_downgrade_to_v1_during_research() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |_| {});
    make_baton(&n, "spec_drafting", "vadi", 5, |_| {});
    run(&b, &n).assert_contains(
        "v2 current cannot downgrade to v1 candidate during research",
        24,
        "schema_change",
    );
}

#[test]
fn v2_current_cannot_downgrade_to_v1_during_implementation() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |_| {});
    make_baton(&n, "phase_review", "prativadi", 5, |_| {});
    run(&b, &n).assert_contains(
        "v2 current cannot downgrade to v1 candidate during implementation",
        24,
        "schema_change",
    );
}

#[test]
fn v1_current_cannot_silently_upgrade_to_v2_candidate() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton_v2(&n, "spec_review", "prativadi", 5, |_| {});
    run(&b, &n).assert_contains(
        "v1 current cannot silently upgrade to v2 candidate",
        24,
        "schema_change",
    );
}

#[test]
fn v2_current_cannot_change_run_id_mid_run() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |v| {
        v["run_id"] = json!("alpha");
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |v| {
        v["run_id"] = json!("beta");
    });
    run(&b, &n).assert_contains(
        "v2 current cannot change run_id mid-run",
        24,
        "run_id_change",
    );
}

#[test]
fn v2_current_missing_run_id_exits_25() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |v| {
        v.as_object_mut().unwrap().remove("run_id");
    });
    make_baton_v2(&n, "research_revision", "vadi", 5, |_| {});
    run(&b, &n).assert_contains("v2 current missing run_id exits 25", 25, "bad_run_id");
}

// ===================== illegal edges =====================

#[test]
fn v2_research_drafting_to_spec_drafting_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_drafting", "vadi", 4, |_| {});
    make_baton_v2(&n, "spec_drafting", "vadi", 5, |_| {});
    run(&b, &n).assert("v2 research_drafting->spec_drafting exits 24", 24);
}

#[test]
fn v2_research_state_can_enter_human_question() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "human_question", "human", 5, |v| {
        v["question"] = json!("Which source should research use?");
        v["resume_assignee"] = json!("prativadi");
        v["resume_status"] = json!("research_review");
    });
    run(&b, &n).assert("v2 research state can enter human_question", 0);
}

#[test]
fn implementing_to_done_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "implementing", "vadi", 4, |_| {});
    make_baton(&n, "done", "human", 5, |v| {
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert("implementing->done exits 24 (no self-declared done)", 24);
}

#[test]
fn same_status_rewrite_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "implementing", "vadi", 4, |_| {});
    make_baton(&n, "implementing", "vadi", 5, |_| {});
    run(&b, &n).assert("same-status rewrite exits 24", 24);
}

#[test]
fn spec_drafting_to_implementing_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton(&n, "implementing", "vadi", 5, |_| {});
    run(&b, &n).assert("spec_drafting->implementing exits 24", 24);
}

#[test]
fn checkpoint_jump_exits_24_and_leaves_baton_unchanged() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_drafting", "vadi", 4, |_| {});
    let before: Value = serde_json::from_slice(&std::fs::read(&b).unwrap()).unwrap();
    make_baton(&n, "spec_review", "prativadi", 7, |_| {});
    run(&b, &n).assert("checkpoint jump exits 24", 24);
    let after: Value = serde_json::from_slice(&std::fs::read(&b).unwrap()).unwrap();
    assert_eq!(
        before, after,
        "rejected write should leave baton bytes unchanged"
    );
}

#[test]
fn stale_checkpoint_same_exits_27() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton(&n, "spec_review", "prativadi", 4, |_| {});
    run(&b, &n).assert_contains(
        "same checkpoint exits 27 stale_checkpoint",
        27,
        "stale_checkpoint",
    );
}

#[test]
fn stale_checkpoint_lower_exits_27() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton(&n, "spec_review", "prativadi", 3, |_| {});
    run(&b, &n).assert_contains(
        "lower checkpoint exits 27 stale_checkpoint",
        27,
        "stale_checkpoint",
    );
}

#[test]
fn checkpoint_plus_two_remains_illegal_transition() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton(&n, "spec_review", "prativadi", 6, |_| {});
    run(&b, &n).assert_contains(
        "checkpoint plus two remains illegal_transition",
        24,
        "DVANDVA_WRITE illegal_transition",
    );
}

// ===================== universal escalation and human resume =====================

#[test]
fn universal_escalation_to_human_decision_legal() {
    for src in [
        "spec_drafting",
        "implementing",
        "phase_review",
        "counter_review",
    ] {
        let d = tmp();
        let (b, n) = paths(&d);
        make_baton(&b, src, "vadi", 4, |_| {});
        make_baton(&n, "human_decision", "human", 5, |_| {});
        run(&b, &n).assert(&format!("{src}->human_decision is legal"), 0);
    }
}

#[test]
fn human_decision_to_implementing_is_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "human_decision", "human", 4, |_| {});
    make_baton(&n, "implementing", "vadi", 5, |_| {});
    run(&b, &n).assert(
        "human_decision->implementing (human-authorized) is legal",
        0,
    );
}

// ===================== human_question rules =====================

#[test]
fn spec_human_question_entry_with_fields_is_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton(&n, "human_question", "human", 5, |v| {
        v["question"] = json!("Which scope?");
        v["resume_assignee"] = json!("prativadi");
        v["resume_status"] = json!("spec_review");
    });
    run(&b, &n).assert("spec human_question entry with fields is legal", 0);
}

#[test]
fn human_question_after_plan_lock_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_review", "prativadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
    });
    make_baton(&n, "human_question", "human", 5, |v| {
        v["question"] = json!("Which scope?");
        v["resume_assignee"] = json!("prativadi");
        v["resume_status"] = json!("spec_review");
    });
    run(&b, &n).assert("human_question after plan lock exits 24", 24);
}

#[test]
fn human_question_entry_with_null_fields_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton(&n, "human_question", "human", 5, |_| {});
    run(&b, &n).assert("human_question entry with null fields exits 24", 24);
}

#[test]
fn human_question_cannot_be_created_with_resume_status_done() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton(&n, "human_question", "human", 5, |v| {
        v["question"] = json!("Stop now?");
        v["resume_assignee"] = json!("human");
        v["resume_status"] = json!("done");
    });
    run(&b, &n).assert_contains(
        "human_question cannot be created with resume_status done",
        24,
        "human_question cannot resume directly to done",
    );
}

#[test]
fn human_question_from_non_spec_state_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "implementing", "vadi", 4, |_| {});
    make_baton(&n, "human_question", "human", 5, |v| {
        v["question"] = json!("Which scope?");
        v["resume_assignee"] = json!("vadi");
        v["resume_status"] = json!("spec_review");
    });
    run(&b, &n).assert("human_question from non-spec state exits 24", 24);
}

#[test]
fn human_question_resume_matching_resume_fields_is_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "human_question", "human", 4, |v| {
        v["question"] = json!("Which scope?");
        v["resume_assignee"] = json!("prativadi");
        v["resume_status"] = json!("spec_review");
    });
    make_baton(&n, "spec_review", "prativadi", 5, |_| {});
    run(&b, &n).assert("human_question resume matching resume fields is legal", 0);
}

#[test]
fn human_question_resume_to_wrong_state_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "human_question", "human", 4, |v| {
        v["question"] = json!("Which scope?");
        v["resume_assignee"] = json!("prativadi");
        v["resume_status"] = json!("spec_review");
    });
    make_baton(&n, "implementing", "vadi", 5, |_| {});
    run(&b, &n).assert("human_question resume to wrong state exits 24", 24);
}

#[test]
fn human_question_resume_without_clearing_fields_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "human_question", "human", 4, |v| {
        v["question"] = json!("Which scope?");
        v["resume_assignee"] = json!("prativadi");
        v["resume_status"] = json!("spec_review");
    });
    make_baton(&n, "spec_review", "prativadi", 5, |v| {
        v["question"] = json!("Which scope?");
        v["resume_assignee"] = json!("prativadi");
        v["resume_status"] = json!("spec_review");
    });
    run(&b, &n).assert("human_question resume without clearing fields exits 24", 24);
}

#[test]
fn v2_human_question_cannot_resume_directly_to_done() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "human_question", "human", 4, |v| {
        v["mode"] = json!("development");
        v["phase"] = json!(1);
        v["question"] = json!("Stop now?");
        v["resume_assignee"] = json!("human");
        v["resume_status"] = json!("done");
        v["vadi_final_approval"] = json!(false);
        v["prativadi_final_approval"] = json!(false);
    });
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["mode"] = json!("development");
        v["phase"] = json!(1);
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["question"] = Value::Null;
        v["resume_assignee"] = Value::Null;
        v["resume_status"] = Value::Null;
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
    });
    run(&b, &n).assert_contains(
        "v2 human_question cannot resume directly to done",
        24,
        "human_question cannot resume directly to done",
    );
}

// ===================== reject-legacy v2 edges =====================

#[test]
fn v2_implementing_to_phase_review_rejects_legacy_direct_review() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |_| {});
    make_baton_v2(&n, "phase_review", "prativadi", 5, |_| {});
    run(&b, &n).assert_contains(
        "v2 implementing->phase_review rejects legacy direct review",
        24,
        "no legal edge implementing->phase_review",
    );
}

#[test]
fn v2_spec_review_to_implementing_rejects_sequential_implementation() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "implementing", "vadi", 5, |_| {});
    run(&b, &n).assert_contains(
        "v2 spec_review->implementing rejects sequential implementation",
        24,
        "no legal edge spec_review->implementing",
    );
}

#[test]
fn v2_test_creation_to_deep_review_requires_cross_review_first() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "test_creation", "vadi", 4, |_| {});
    make_baton_v2(&n, "deep_review", "prativadi", 5, |_| {});
    run(&b, &n).assert_contains(
        "v2 test_creation->deep_review requires cross_review first",
        24,
        "no legal edge test_creation->deep_review",
    );
}

#[test]
fn v2_phase_review_to_done_rejects_legacy_terminal_review() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
    });
    run(&b, &n).assert_contains(
        "v2 phase_review->done rejects legacy terminal review",
        24,
        "done requires current status termination_review",
    );
}

#[test]
fn v2_review_of_review_to_done_rejects_legacy_terminal_review() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "review_of_review", "vadi", 4, |_| {});
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
    });
    run(&b, &n).assert_contains(
        "v2 review_of_review->done rejects legacy terminal review",
        24,
        "done requires current status termination_review",
    );
}

#[test]
fn v2_counter_review_to_done_rejects_legacy_terminal_review() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "counter_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
    });
    run(&b, &n).assert_contains(
        "v2 counter_review->done rejects legacy terminal review",
        24,
        "done requires current status termination_review",
    );
}

// ===================== post-legality evidence gates =====================

#[test]
fn v2_test_creation_to_cross_review_rejects_missing_test_evidence() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "test_creation", "vadi", 4, |_| {});
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    run(&b, &n).assert_contains(
        "v2 test_creation->cross_review rejects missing test evidence",
        24,
        "completed test-creation subagent_track",
    );
}

#[test]
fn v2_parallel_implementing_to_test_creation_rejects_missing_impl_evidence() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "parallel_implementing", "team", 4, parallel_chunks);
    make_baton_v2(&n, "test_creation", "team", 5, |v| {
        parallel_chunks(v);
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    run(&b, &n).assert_contains(
        "v2 parallel_implementing->test_creation rejects missing implementation evidence",
        24,
        "completed implementation-chunk subagent_tracks for both roles",
    );
}

#[test]
fn v2_parallel_implementing_to_test_creation_requires_both_implementation_roles() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "parallel_implementing", "team", 4, parallel_chunks);
    make_baton_v2(&n, "test_creation", "team", 5, |v| {
        parallel_chunks(v);
        v["active_roles"] = json!(["vadi", "prativadi"]);
        implementation_tracks(v);
        if let Some(arr) = v["subagent_tracks"].as_array_mut() {
            for t in arr.iter_mut() {
                if t["track"] == "implementation-chunk" {
                    t["owner_role"] = json!("vadi");
                }
            }
        }
    });
    run(&b, &n).assert_contains(
        "v2 parallel_implementing->test_creation requires both implementation roles",
        24,
        "completed implementation-chunk subagent_tracks for both roles",
    );
}

#[test]
fn v2_cross_review_to_deep_review_rejects_missing_evidence() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "deep_review", "prativadi", 5, |_| {});
    run(&b, &n).assert_contains(
        "v2 cross_review->deep_review rejects missing cross-review evidence",
        24,
        "completed cross-review subagent_tracks for both roles",
    );
}

#[test]
fn v2_cross_review_to_cross_fixing_rejects_missing_evidence() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        set_loop_count(v, "cross_review:cross_fixing", 0);
    });
    make_baton_v2(&n, "cross_fixing", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        set_loop_count(v, "cross_review:cross_fixing", 1);
    });
    run(&b, &n).assert_contains(
        "v2 cross_review->cross_fixing rejects missing cross-review evidence",
        24,
        "completed cross-review subagent_tracks",
    );
}

#[test]
fn v2_cross_review_to_deep_review_requires_both_cross_review_roles() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "deep_review", "prativadi", 5, |v| {
        cross_review_tracks(v);
        if let Some(arr) = v["subagent_tracks"].as_array_mut() {
            for t in arr.iter_mut() {
                if t["track"] == "cross-review" {
                    t["owner_role"] = json!("vadi");
                }
            }
        }
    });
    run(&b, &n).assert_contains(
        "v2 cross_review->deep_review requires both cross-review roles",
        24,
        "completed cross-review subagent_tracks for both roles",
    );
}

#[test]
fn v2_cross_review_to_deep_review_requires_current_review_checkpoint_evidence() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "deep_review", "prativadi", 5, |v| {
        cross_review_tracks(v);
        if let Some(arr) = v["subagent_tracks"].as_array_mut() {
            for t in arr.iter_mut() {
                if t["track"] == "cross-review" {
                    t.as_object_mut().unwrap().remove("review_checkpoint");
                }
            }
        }
    });
    run(&b, &n).assert_contains(
        "v2 cross_review->deep_review requires current review checkpoint evidence",
        24,
        "current-cycle completed cross-review subagent_tracks",
    );
}

#[test]
fn v2_cross_review_to_deep_review_rejects_stale_cross_review_approvals() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "deep_review", "prativadi", 5, |v| {
        cross_review_tracks(v);
        if let Some(arr) = v["subagent_tracks"].as_array_mut() {
            for t in arr.iter_mut() {
                if t["track"] == "cross-review" {
                    t["review_checkpoint"] = json!(3);
                }
            }
        }
    });
    run(&b, &n).assert_contains(
        "v2 cross_review->deep_review rejects stale cross-review approvals",
        24,
        "current-cycle completed cross-review subagent_tracks",
    );
}

#[test]
fn v2_cross_review_deep_review_after_team_sync() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    std::fs::create_dir_all(d.path().join("history")).unwrap();
    std::fs::copy(&b, d.path().join("history/4-cross_review-team.json")).unwrap();
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        cross_review_tracks(v);
        // S4-T4: a same-status team sync must keep the installed peer data —
        // record the prativadi review role while preserving the seed track.
        if let Some(arr) = v["subagent_tracks"].as_array_mut() {
            arr.retain(|t| t["owner_role"] == "prativadi" || t["id"] == "startup-controller");
        }
    });
    run(&b, &n).assert(
        "v2 cross_review same-status sync can record first review role",
        0,
    );
    make_baton_v2(&n, "deep_review", "prativadi", 6, cross_review_tracks);
    run(&b, &n).assert(
        "v2 cross_review->deep_review accepts review-cycle checkpoint after team sync",
        0,
    );
}

#[test]
fn v2_cross_review_cross_fixing_after_team_sync() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    std::fs::create_dir_all(d.path().join("history")).unwrap();
    std::fs::copy(&b, d.path().join("history/4-cross_review-team.json")).unwrap();
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        cross_review_finding(v);
    });
    run(&b, &n).assert(
        "v2 cross_review same-status sync can record finding role",
        0,
    );
    make_baton_v2(&n, "cross_fixing", "team", 6, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        set_loop_count(v, "cross_review:cross_fixing", 1);
        cross_review_finding(v);
    });
    run(&b, &n).assert(
        "v2 cross_review->cross_fixing accepts review-cycle checkpoint after team sync",
        0,
    );
}

#[test]
fn v2_cross_review_tracks_must_use_status_name_phase() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "deep_review", "prativadi", 5, |v| {
        cross_review_tracks(v);
        if let Some(arr) = v["subagent_tracks"].as_array_mut() {
            for t in arr.iter_mut() {
                if t["track"] == "cross-review" {
                    t["phase"] = json!(1);
                }
            }
        }
    });
    run(&b, &n).assert_contains(
        "v2 cross_review tracks must use status-name phase",
        24,
        "completed cross-review subagent_tracks for both roles",
    );
}

#[test]
fn v2_deep_review_to_deslop_requires_three_review_angles() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "deslop", "vadi", 5, |_| {});
    run(&b, &n).assert_contains(
        "v2 deep_review->deslop requires three review angles",
        24,
        "three completed review-angle subagent_tracks",
    );
}

#[test]
fn v2_deep_review_to_deslop_rejects_stale_angles_from_another_phase() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "deslop", "vadi", 5, |v| {
        review_angles(v);
        if let Some(arr) = v["subagent_tracks"].as_array_mut() {
            for t in arr.iter_mut() {
                if t["track"] == "correctness-regression"
                    || t["track"] == "test-evidence"
                    || t["track"] == "protocol-handoff"
                {
                    t["phase"] = json!("research");
                }
            }
        }
    });
    run(&b, &n).assert_contains(
        "v2 deep_review->deslop rejects stale review angles from another phase",
        24,
        "three completed review-angle subagent_tracks",
    );
}

#[test]
fn v2_deep_review_to_deslop_rejects_stale_same_phase_angles() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 8, |_| {});
    make_baton_v2(&n, "deslop", "vadi", 9, |v| {
        review_angles(v);
        if let Some(arr) = v["subagent_tracks"].as_array_mut() {
            for t in arr.iter_mut() {
                if t["track"] == "correctness-regression"
                    || t["track"] == "test-evidence"
                    || t["track"] == "protocol-handoff"
                {
                    t["review_checkpoint"] = json!(4);
                }
            }
        }
    });
    run(&b, &n).assert_contains(
        "v2 deep_review->deslop rejects stale same-phase review angles",
        24,
        "current-cycle three completed review-angle subagent_tracks",
    );
}

#[test]
fn v2_deep_review_to_deslop_rejects_empty_review_evidence() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "deslop", "vadi", 5, |v| {
        review_angles(v);
        if let Some(arr) = v["subagent_tracks"].as_array_mut() {
            for t in arr.iter_mut() {
                if t["track"] == "correctness-regression"
                    || t["track"] == "test-evidence"
                    || t["track"] == "protocol-handoff"
                {
                    t["parallelized"] = json!(false);
                    t["owner"] = json!("prativadi");
                    t["inputs"] = json!([]);
                    t["outputs"] = json!([]);
                    t["evidence_refs"] = json!([]);
                }
            }
        }
    });
    run(&b, &n).assert_contains(
        "v2 deep_review->deslop rejects empty review evidence",
        24,
        "three completed review-angle subagent_tracks",
    );
}

#[test]
fn v2_deep_review_to_deslop_accepts_three_review_angles() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "deslop", "vadi", 5, review_angles);
    run(&b, &n).assert("v2 deep_review->deslop accepts three review angles", 0);
}

#[test]
fn v2_deep_review_to_review_of_review_requires_narrow_fixups() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "review_of_review", "vadi", 5, |v| {
        review_angles(v);
        v["review_target"] = json!("prativadi_fixups");
    });
    run(&b, &n).assert_contains(
        "v2 deep_review->review_of_review requires narrow fixups",
        24,
        "review_of_review requires non-empty narrow_fixups",
    );
}

#[test]
fn v2_deep_review_to_review_of_review_requires_three_review_angles() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "review_of_review", "vadi", 5, |v| {
        v["review_target"] = json!("prativadi_fixups");
        v["narrow_fixups"] = json!(["test fixup"]);
    });
    run(&b, &n).assert_contains(
        "v2 deep_review->review_of_review requires three review angles",
        24,
        "three completed review-angle subagent_tracks",
    );
}

#[test]
fn v2_deep_review_to_review_of_review_accepts_angles_and_fixups() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "review_of_review", "vadi", 5, |v| {
        review_angles(v);
        v["review_target"] = json!("prativadi_fixups");
        v["narrow_fixups"] = json!(["test fixup"]);
    });
    run(&b, &n).assert(
        "v2 deep_review->review_of_review accepts angles and fixups",
        0,
    );
}

// ===================== same-status team sync =====================

#[test]
fn v2_team_cross_fixing_accepts_same_status_sync_checkpoint() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_fixing", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_fixing", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] =
            json!("Team sync: prativadi protocol slice complete; vadi owns agent-roster slice.");
        v["next_action"] =
            json!("Vadi: complete agent-roster slice; prativadi is polling for next checkpoint.");
    });
    run(&b, &n).assert(
        "v2 team cross_fixing accepts same-status sync checkpoint",
        0,
    );
}

#[test]
fn v2_team_same_status_sync_cannot_change_phase() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["phase"] = json!(2);
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Team sync: attempted phase mutation.");
        v["next_action"] =
            json!("Team: this must be rejected because sync checkpoints cannot advance phases.");
    });
    run(&b, &n).assert_contains(
        "v2 team same-status sync cannot change phase",
        24,
        "same-status team sync cannot change phase",
    );
}

#[test]
fn v2_team_sync_rejects_whitespace_summary() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("   \t  ");
        v["next_action"] = json!("Team: valid next action.");
    });
    run(&b, &n).assert_contains(
        "v2 team sync rejects whitespace summary",
        24,
        "same-status team sync requires team assignee, both active_roles, summary, and next_action",
    );
}

#[test]
fn v2_team_sync_rejects_whitespace_next_action() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Team sync: valid summary.");
        v["next_action"] = json!("   \t  ");
    });
    run(&b, &n).assert_contains(
        "v2 team sync rejects whitespace next_action",
        24,
        "same-status team sync requires team assignee, both active_roles, summary, and next_action",
    );
}

#[test]
fn v2_non_team_same_status_rewrite_still_rejects() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |_| {});
    make_baton_v2(&n, "implementing", "vadi", 5, |_| {});
    run(&b, &n).assert_contains(
        "v2 non-team same-status rewrite still rejects",
        24,
        "same-status rewrite",
    );
}

// ===================== run-explainer review ownership + termination approval ownership =====================

#[test]
fn v2_vadi_cannot_forge_prativadi_run_explainer_review() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        v["run_explainer_reviews"][1]["summary"] = json!("FABRICATED by vadi.");
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "v2 vadi cannot forge prativadi run explainer review",
        24,
        "run explainer review ownership",
    );
}

#[test]
fn v2_termination_review_allows_vadi_own_run_explainer_review() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        if let Some(arr) = v["run_explainer_reviews"].as_array_mut() {
            arr.retain(|r| r["role"] == "prativadi");
        }
    });
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Vadi adds only its own run explainer review.");
        v["next_action"] =
            json!("Team: prativadi review was already installed; vadi review is now installed.");
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")])
        .assert("v2 vadi can add its own run explainer review", 0);
}

#[test]
fn v2_termination_review_allows_prativadi_own_run_explainer_review() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        if let Some(arr) = v["run_explainer_reviews"].as_array_mut() {
            arr.retain(|r| r["role"] == "vadi");
        }
    });
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Prativadi adds only its own run explainer review.");
        v["next_action"] =
            json!("Team: vadi review was already installed; prativadi review is now installed.");
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")])
        .assert("v2 prativadi can add its own run explainer review", 0);
}

#[test]
fn v2_vadi_cannot_edit_prativadi_run_explainer_review() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        if let Some(arr) = v["run_explainer_reviews"].as_array_mut() {
            arr.retain(|r| r["role"] == "prativadi");
        }
    });
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Vadi tries to rewrite prativadi review while adding its own.");
        v["next_action"] =
            json!("Team: this must fail because peer review entries are role-owned.");
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        v["run_explainer_reviews"][1]["summary"] = json!("Mutated by vadi.");
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "v2 vadi cannot edit prativadi run explainer review",
        24,
        "run explainer review ownership",
    );
}

#[test]
fn v2_run_explainer_review_changes_require_dvandva_role() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("A review entry change without DVANDVA_ROLE must fail.");
        v["next_action"] = json!("Team: retry with the writing role exported.");
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        if let Some(arr) = v["run_explainer_reviews"].as_array_mut() {
            arr.retain(|r| r["role"] == "vadi");
        }
    });
    run(&b, &n).assert_contains(
        "v2 run explainer review changes require DVANDVA_ROLE",
        24,
        "run explainer review ownership",
    );
}

#[test]
fn v2_done_requires_approvals_before_terminal_checkpoint() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(false);
    });
    make_baton_v2(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
    });
    run(&b, &n).assert_contains(
        "v2 done requires approvals before terminal checkpoint",
        24,
        "done requires current termination_review with both final approvals",
    );
}

#[test]
fn v2_vadi_cannot_raise_both_final_approvals_entering_termination_review() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |_| {});
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "v2 vadi cannot raise both final approvals entering termination_review",
        24,
        "final approval ownership",
    );
}

#[test]
fn v2_vadi_cannot_raise_prativadi_final_approval() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(false);
    });
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Vadi cannot approve for prativadi.");
        v["next_action"] = json!("Prativadi must make its own stop decision.");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "v2 vadi cannot raise prativadi final approval",
        24,
        "final approval ownership",
    );
}

#[test]
fn v2_prativadi_can_raise_its_own_final_approval() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(false);
    });
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Prativadi independently approves the shared stop decision.");
        v["next_action"] =
            json!("Team: final approval bits now converged; final ship may proceed.");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "prativadi")])
        .assert("v2 prativadi can raise its own final approval", 0);
}

// ===================== run-modes per-mode edges =====================

const MODE_EDGE_LOOP_COUNT_EDGES: &[&str] = &[
    "deep_review:phase_fixing",
    "cross_review:cross_fixing",
    "termination_review:phase_fixing",
    "phase_review:phase_fixing",
    "review_of_review:counter_review",
    "counter_review:review_of_review",
];

fn v2_mode_status_filter(mode: &str, status: &str, b: &mut Value) {
    match (mode, status) {
        ("research", "research_drafting")
        | ("research", "research_review")
        | ("research", "research_revision") => {
            b["mode"] = json!("research");
            b["phase"] = json!("research");
        }
        ("research", "done") => {
            b["mode"] = json!("research");
            b["phase"] = json!("spec");
            b["vadi_final_approval"] = json!(true);
            b["prativadi_final_approval"] = json!(true);
        }
        ("research", "termination_review") => {
            b["mode"] = json!("research");
            b["phase"] = json!("spec");
            b["active_roles"] = json!(["vadi", "prativadi"]);
        }
        ("research", _) => {
            b["mode"] = json!("research");
            b["phase"] = json!("spec");
        }
        ("review", "done") => {
            b["mode"] = json!("review");
            b["phase"] = json!("review");
            b["review_ref"] = json!("./superpowers/reviews/review-run-modes.html");
            b["vadi_final_approval"] = json!(true);
            b["prativadi_final_approval"] = json!(true);
        }
        ("review", "termination_review") => {
            b["mode"] = json!("review");
            b["phase"] = json!("review");
            b["active_roles"] = json!(["vadi", "prativadi"]);
        }
        ("review", _) => {
            b["mode"] = json!("review");
            b["phase"] = json!("review");
        }
        _ => {}
    }
}

fn run_mode_edge_case(mode: &str, from_status: &str, to_status: &str) {
    let edge = format!("{from_status}:{to_status}");
    let d = tmp();
    let (b, n) = paths(&d);
    // S4-T1: research/review done gates resolve required refs to real files.
    seed_done_artifacts(d.path());
    make_baton_v2(&b, from_status, v2_status_owner(from_status), 4, |v| {
        v2_mode_status_filter(mode, from_status, v);
        if from_status == "termination_review" && to_status == "done" {
            v["vadi_final_approval"] = json!(true);
            v["prativadi_final_approval"] = json!(true);
        }
        if MODE_EDGE_LOOP_COUNT_EDGES.contains(&edge.as_str()) {
            set_loop_count(v, &edge, 0);
        }
    });
    make_baton_v2(&n, to_status, v2_status_owner(to_status), 5, |v| {
        v2_mode_status_filter(mode, to_status, v);
        if MODE_EDGE_LOOP_COUNT_EDGES.contains(&edge.as_str()) {
            set_loop_count(v, &edge, 1);
        }
        if mode == "review" && from_status == "deep_review" && to_status == "deslop" {
            review_angles(v);
        }
    });
    let name = format!("{mode} mode {from_status}:{to_status} full edge table is legal");
    run(&b, &n).assert(&name, 0);
}

#[test]
fn research_mode_edges_legal() {
    for (from, to) in [
        ("research_drafting", "research_review"),
        ("research_review", "research_revision"),
        ("research_revision", "research_review"),
        ("research_review", "spec_drafting"),
        ("spec_drafting", "spec_review"),
        ("spec_review", "spec_revision"),
        ("spec_revision", "spec_review"),
        ("research_review", "termination_review"),
        ("spec_review", "termination_review"),
        ("termination_review", "phase_fixing"),
        ("phase_fixing", "research_review"),
        ("termination_review", "done"),
    ] {
        run_mode_edge_case("research", from, to);
    }
}

#[test]
fn review_mode_edges_legal() {
    for (from, to) in [
        ("research_drafting", "research_review"),
        ("research_review", "research_revision"),
        ("research_revision", "research_review"),
        ("research_review", "deep_review"),
        ("deep_review", "deslop"),
        ("deslop", "termination_review"),
        ("termination_review", "phase_fixing"),
        ("phase_fixing", "deep_review"),
        ("termination_review", "done"),
    ] {
        run_mode_edge_case("review", from, to);
    }
}

#[test]
fn research_mode_spec_review_termination_review_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        v["mode"] = json!("research");
    });
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["mode"] = json!("research");
        v["phase"] = json!("spec");
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    run(&b, &n).assert("research mode spec_review:termination_review is legal", 0);
}

#[test]
fn review_mode_research_review_deep_review_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "research_review", "prativadi", 4, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
    });
    make_baton_v2(&n, "deep_review", "prativadi", 5, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
    });
    run(&b, &n).assert("review mode research_review:deep_review is legal", 0);
}

#[test]
fn research_mode_spec_review_parallel_implementing_illegal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        v["mode"] = json!("research");
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        v["mode"] = json!("research");
        v["phase"] = json!("spec");
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    run(&b, &n).assert_contains(
        "research mode spec_review:parallel_implementing exits 24",
        24,
        "no legal edge spec_review->parallel_implementing",
    );
}

#[test]
fn review_mode_parallel_implementing_test_creation_illegal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "parallel_implementing", "team", 4, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "test_creation", "team", 5, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    run(&b, &n).assert_contains(
        "review mode parallel_implementing:test_creation exits 24",
        24,
        "no legal edge parallel_implementing->test_creation",
    );
}

#[test]
fn research_mode_cross_review_termination_review_illegal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["mode"] = json!("research");
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["mode"] = json!("research");
        v["phase"] = json!("spec");
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    run(&b, &n).assert_contains(
        "research mode cross_review:termination_review exits 24 no wildcard",
        24,
        "no legal edge cross_review->termination_review",
    );
}

#[test]
fn review_mode_deslop_termination_review_phase_review_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
    });
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    run(&b, &n).assert(
        "review mode deslop:termination_review with phase review is legal",
        0,
    );
}

#[test]
fn research_done_from_non_termination_review_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        v["mode"] = json!("research");
    });
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["mode"] = json!("research");
        v["phase"] = json!("spec");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "research done from non-termination_review exits 24",
        24,
        "done requires current status termination_review",
    );
}

#[test]
fn research_done_with_one_approval_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["mode"] = json!("research");
        v["phase"] = json!("spec");
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(false);
    });
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["mode"] = json!("research");
        v["phase"] = json!("spec");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "research done with one approval exits 24",
        24,
        "done requires current termination_review with both final approvals",
    );
}

#[test]
fn review_done_from_non_termination_review_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
    });
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["review_ref"] = json!("./superpowers/reviews/review-run-modes-PR-1.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "review done from non-termination_review exits 24",
        24,
        "done requires current status termination_review",
    );
}

#[test]
fn review_done_with_one_approval_exits_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(false);
    });
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["review_ref"] = json!("./superpowers/reviews/review-run-modes-PR-1.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "review done with one approval exits 24",
        24,
        "done requires current termination_review with both final approvals",
    );
}

// ===================== loop caps / approval hygiene =====================

#[test]
fn v2_loop_cap_rejects_fourth_cycle() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |v| {
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "deep_review:phase_fixing", 3);
    });
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |v| {
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "deep_review:phase_fixing", 4);
    });
    run(&b, &n).assert_contains(
        "v2 loop cap rejects fourth review/fix cycle",
        23,
        "loop_cap",
    );
}

#[test]
fn v2_loop_count_must_increment_by_one() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |v| {
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "deep_review:phase_fixing", 1);
    });
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |v| {
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "deep_review:phase_fixing", 1);
    });
    run(&b, &n).assert_contains("v2 loop count must increment by one", 23, "bad_loop_counts");
}

#[test]
fn v2_loop_count_first_increment_required() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |v| {
        v["disagreement_cap"] = json!(3);
        v["loop_counts"] = json!({});
    });
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |v| {
        v["disagreement_cap"] = json!(3);
        v["loop_counts"] = json!({});
    });
    run(&b, &n).assert_contains(
        "v2 loop count first increment is required",
        23,
        "bad_loop_counts",
    );
}

#[test]
fn v2_loop_count_first_increment_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |v| {
        v["disagreement_cap"] = json!(3);
        v["loop_counts"] = json!({});
    });
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |v| {
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "deep_review:phase_fixing", 1);
    });
    run(&b, &n).assert("v2 loop count first increment is accepted", 0);
}

// (B2) A malformed (non-integer) candidate loop_counts value echoes the RAW
// candidate value in the diagnostic, matching the shell's
// `jq -r ... (.loop_counts // {})[$edge] // 0`, instead of a canned "0".
#[test]
fn v2_loop_count_malformed_candidate_value_echoes_raw_value() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |v| {
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "deep_review:phase_fixing", 1);
    });
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |v| {
        v["disagreement_cap"] = json!(3);
        let mut map = serde_json::Map::new();
        map.insert("deep_review:phase_fixing".to_string(), json!("bogus"));
        v["loop_counts"] = Value::Object(map);
    });
    run(&b, &n).assert_contains(
        "v2 malformed loop count echoes the raw candidate value",
        23,
        "bad_loop_counts edge=deep_review:phase_fixing count=bogus",
    );
}

#[test]
fn v2_loop_cap_rejects_edges_table() {
    let cases = [
        ("cross_review", "cross_fixing", "team", "team"),
        ("termination_review", "phase_fixing", "team", "vadi"),
        ("phase_review", "phase_fixing", "prativadi", "vadi"),
        ("review_of_review", "counter_review", "vadi", "prativadi"),
        ("counter_review", "review_of_review", "prativadi", "vadi"),
    ];
    for (from_status, to_status, from_owner, to_owner) in cases {
        let edge = format!("{from_status}:{to_status}");
        let d = tmp();
        let (b, n) = paths(&d);
        make_baton_v2(&b, from_status, from_owner, 4, |v| {
            let roles: Vec<&str> = if from_owner == "team" {
                vec!["vadi", "prativadi"]
            } else {
                vec![]
            };
            v["active_roles"] = json!(roles);
            v["disagreement_cap"] = json!(3);
            set_loop_count(v, &edge, 3);
        });
        make_baton_v2(&n, to_status, to_owner, 5, |v| {
            let roles: Vec<&str> = if to_owner == "team" {
                vec!["vadi", "prativadi"]
            } else {
                vec![]
            };
            v["active_roles"] = json!(roles);
            v["disagreement_cap"] = json!(3);
            set_loop_count(v, &edge, 4);
        });
        run(&b, &n).assert_contains(
            &format!("v2 loop cap rejects {from_status}->{to_status}"),
            23,
            "loop_cap",
        );
    }
}

#[test]
fn v2_loop_counts_reset_on_next_phase_exits_23() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["phase"] = json!(1);
        set_loop_count(v, "deep_review:phase_fixing", 2);
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        parallel_chunks(v);
        v["phase"] = json!(2);
        v["active_roles"] = json!(["vadi", "prativadi"]);
        if let Some(arr) = v["work_split"].as_array_mut() {
            for chunk in arr.iter_mut() {
                if chunk["chunk_type"] == "implementation" {
                    chunk["phase"] = json!("2");
                }
            }
        }
        set_loop_count(v, "deep_review:phase_fixing", 2);
    });
    run(&b, &n).assert_contains("v2 loop counts reset on next phase", 23, "bad_loop_counts");
}

#[test]
fn v2_loop_counts_empty_on_next_phase_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["phase"] = json!(1);
        set_loop_count(v, "deep_review:phase_fixing", 2);
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        parallel_chunks(v);
        v["phase"] = json!(2);
        v["active_roles"] = json!(["vadi", "prativadi"]);
        if let Some(arr) = v["work_split"].as_array_mut() {
            for chunk in arr.iter_mut() {
                if chunk["chunk_type"] == "implementation" {
                    chunk["phase"] = json!("2");
                }
            }
        }
        v["loop_counts"] = json!({});
    });
    run(&b, &n).assert("v2 empty loop counts accepted on next phase", 0);
}

#[test]
fn v2_loop_cap_may_escalate_human_decision() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |v| {
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "deep_review:phase_fixing", 3);
    });
    make_baton_v2(&n, "human_decision", "human", 5, |v| {
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "deep_review:phase_fixing", 3);
    });
    run(&b, &n).assert("v2 loop cap permits human_decision escalation", 0);
}

#[test]
fn v2_approval_out_of_band_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |_| {});
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |v| {
        v["vadi_final_approval"] = json!(true);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert_contains(
        "v2 final approval outside termination_review is rejected",
        23,
        "approval_out_of_band",
    );
}

#[test]
fn v2_stale_approval_reset_required() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |v| {
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "v2 termination_review to phase_fixing resets final approvals",
        23,
        "stale_approval",
    );
}

#[test]
fn v2_termination_review_entry_can_set_own_approval() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |_| {});
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(false);
    });
    run_env(&b, &n, &[("DVANDVA_ROLE", "vadi")]).assert(
        "v2 entering termination_review may set own final approval",
        0,
    );
}

// ===================== F7: plan-amendment loop =====================

/// Full-profile amendment entry: deslop -> spec_revision sets amendment_from_phase
/// to the current numeric phase, moves to the spec phase, and increments the
/// plan_amendment loop counter.
#[test]
fn f7_amendment_enter_full_deslop_to_spec_revision() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "spec_revision", "vadi", 5, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
        set_loop_count(v, "plan_amendment:1", 1);
    });
    run(&b, &n).assert("f7 full deslop->spec_revision enters amendment loop", 0);
}

/// Standard-profile amendment entry: phase_review -> spec_revision.
#[test]
fn f7_amendment_enter_standard_phase_review_to_spec_revision() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_review", "prativadi", 4, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "spec_revision", "vadi", 5, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
        set_loop_count(v, "plan_amendment:1", 1);
    });
    run(&b, &n).assert(
        "f7 standard phase_review->spec_revision enters amendment loop",
        0,
    );
}

/// The amendment entry edge must increment plan_amendment:<from-phase> by one.
#[test]
fn f7_amendment_enter_requires_loop_increment() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "spec_revision", "vadi", 5, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
        // no plan_amendment increment
    });
    run(&b, &n).assert_contains(
        "f7 amendment enter requires loop increment",
        23,
        "bad_loop_counts edge=plan_amendment:1",
    );
}

/// At the disagreement cap the amendment entry edge is rejected.
#[test]
fn f7_amendment_enter_loop_cap_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "plan_amendment:1", 3);
    });
    make_baton_v2(&n, "spec_revision", "vadi", 5, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "plan_amendment:1", 4);
    });
    run(&b, &n).assert_contains(
        "f7 amendment enter loop cap rejects",
        23,
        "loop_cap edge=plan_amendment:1",
    );
}

/// At the cap, only human_decision remains legal from the amendment source state.
#[test]
fn f7_amendment_enter_at_cap_allows_human_decision() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "plan_amendment:1", 3);
    });
    make_baton_v2(&n, "human_decision", "human", 5, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["disagreement_cap"] = json!(3);
        set_loop_count(v, "plan_amendment:1", 3);
    });
    run(&b, &n).assert("f7 amendment at cap permits human_decision", 0);
}

/// The entry edge must set amendment_from_phase (not leave it null).
#[test]
fn f7_amendment_enter_requires_field_set() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "spec_revision", "vadi", 5, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        set_loop_count(v, "plan_amendment:1", 1);
        // amendment_from_phase left null
    });
    run(&b, &n).assert_contains("f7 amendment enter requires field set", 23, "bad_amendment");
}

/// The entry edge amendment_from_phase must equal the current numeric phase.
#[test]
fn f7_amendment_enter_wrong_from_phase_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "spec_revision", "vadi", 5, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(2);
        set_loop_count(v, "plan_amendment:1", 1);
    });
    run(&b, &n).assert_contains(
        "f7 amendment enter wrong from-phase rejected",
        23,
        "bad_amendment",
    );
}

/// Setting amendment_from_phase on a non-amendment edge is rejected.
#[test]
fn f7_amendment_set_on_wrong_edge_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton_v2(&n, "spec_review", "prativadi", 5, |v| {
        v["amendment_from_phase"] = json!(1);
    });
    run(&b, &n).assert_contains(
        "f7 amendment set on wrong edge rejected",
        23,
        "bad_amendment",
    );
}

/// While amendment_from_phase is non-null the spec loop is legal even post-lock,
/// and total_phases may change.
#[test]
fn f7_amendment_spec_loop_legal_and_total_phases_may_change() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_revision", "vadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
        set_loop_count(v, "plan_amendment:1", 1);
    });
    make_baton_v2(&n, "spec_review", "prativadi", 5, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(3);
        v["amendment_from_phase"] = json!(1);
        set_loop_count(v, "plan_amendment:1", 1);
    });
    run(&b, &n).assert(
        "f7 amendment spec loop legal and total_phases may change",
        0,
    );
}

/// total_phases is frozen once locked when amendment_from_phase is null.
#[test]
fn f7_total_phases_frozen_while_amendment_null() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(3);
        v["phase"] = json!(1);
    });
    run(&b, &n).assert_contains(
        "f7 total_phases frozen while amendment null",
        23,
        "total_phases_frozen",
    );
}

/// Full-profile amendment exit: spec_review -> parallel_implementing nulls the
/// field and re-enters at a phase >= amendment_from_phase.
#[test]
fn f7_amendment_exit_full_to_parallel_implementing() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
        parallel_chunks(v);
    });
    run(&b, &n).assert("f7 amendment exit to parallel_implementing", 0);
}

/// Standard-profile amendment exit: spec_review -> implementing.
#[test]
fn f7_amendment_exit_standard_to_implementing() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    run(&b, &n).assert("f7 amendment exit to implementing", 0);
}

/// The exit edge must null amendment_from_phase.
#[test]
fn f7_amendment_exit_must_null_field() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
        v["amendment_from_phase"] = json!(1);
    });
    run(&b, &n).assert_contains("f7 amendment exit must null field", 23, "bad_amendment");
}

/// Re-entry below amendment_from_phase is illegal.
#[test]
fn f7_amendment_reentry_below_from_phase_illegal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(3);
        v["amendment_from_phase"] = json!(2);
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(3);
        v["phase"] = json!(1);
    });
    run(&b, &n).assert_contains(
        "f7 amendment re-entry below from-phase illegal",
        24,
        "below amendment_from_phase",
    );
}

/// Fix 3: a numeric phase can never exceed total_phases. The amendment loop is
/// the one path that can LOWER total_phases, so an exit could otherwise re-enter a
/// phase above the (now smaller) ceiling ("phase 2 of 1"). Guarded with a
/// 23/bad_amendment reason.
#[test]
fn f7_amendment_exit_phase_above_total_phases_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["amendment_from_phase"] = json!(1);
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        // Re-entry at phase 2 while total_phases was lowered to 1.
        v["phase"] = json!(2);
    });
    run(&b, &n).assert_contains(
        "f7 amendment exit into phase above total_phases rejected",
        23,
        "phase_exceeds_total_phases",
    );
}

/// The amendment_from_phase value cannot be changed mid-loop.
#[test]
fn f7_amendment_loop_cannot_change_from_phase() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_revision", "vadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
        set_loop_count(v, "plan_amendment:1", 1);
    });
    make_baton_v2(&n, "spec_review", "prativadi", 5, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(2);
        set_loop_count(v, "plan_amendment:1", 1);
    });
    run(&b, &n).assert_contains(
        "f7 amendment loop cannot change from-phase",
        23,
        "bad_amendment",
    );
}

/// Post-lock human_question stays illegal during the amendment loop.
#[test]
fn f7_post_lock_human_question_illegal_during_amendment() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_revision", "vadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
        set_loop_count(v, "plan_amendment:1", 1);
    });
    make_baton_v2(&n, "human_question", "human", 5, |v| {
        v["master_plan_locked"] = json!(true);
        v["amendment_from_phase"] = json!(1);
        v["question"] = json!("Change the plan further?");
        v["resume_assignee"] = json!("vadi");
        v["resume_status"] = json!("spec_revision");
    });
    run(&b, &n).assert_contains(
        "f7 post-lock human_question illegal during amendment",
        24,
        // S4-T5 (D1): the planning-state entry stays pre-lock-only; the message
        // now spells out that post-lock questions come from working states.
        "only legal before master_plan_locked",
    );
}

// ===================== F8: team-owned test_creation =====================

/// v2 full-profile test_creation requires the team owner.
#[test]
fn f8_test_creation_requires_team_owner() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "parallel_implementing", "team", 4, parallel_chunks);
    make_baton_v2(&n, "test_creation", "vadi", 5, |v| {
        parallel_chunks(v);
        implementation_tracks(v);
    });
    run(&b, &n).assert_contains(
        "f8 test_creation requires team owner",
        23,
        "bad_assignee_owner",
    );
}

/// team-owned test_creation requires both active_roles.
#[test]
fn f8_test_creation_requires_both_active_roles() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "parallel_implementing", "team", 4, parallel_chunks);
    make_baton_v2(&n, "test_creation", "team", 5, |v| {
        parallel_chunks(v);
        v["active_roles"] = json!(["vadi"]);
        implementation_tracks(v);
    });
    run(&b, &n).assert_contains(
        "f8 test_creation requires both active_roles",
        23,
        "bad_active_roles",
    );
}

/// Entry edge parallel_implementing -> team-owned test_creation is legal.
#[test]
fn f8_parallel_implementing_to_test_creation_team_owned() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "parallel_implementing", "team", 4, parallel_chunks);
    make_baton_v2(&n, "test_creation", "team", 5, |v| {
        parallel_chunks(v);
        v["active_roles"] = json!(["vadi", "prativadi"]);
        implementation_tracks(v);
    });
    run(&b, &n).assert("f8 parallel_implementing->team test_creation", 0);
}

/// test_creation joins the same-status team-sync set.
#[test]
fn f8_test_creation_same_status_team_sync_legal() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "test_creation", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Team sync: coverage and adversarial tracks in progress.");
        v["next_action"] = json!("Team: both roles recording their test-creation tracks.");
    });
    run(&b, &n).assert("f8 test_creation same-status team sync", 0);
}

/// Exit edge test_creation -> cross_review keeps the required dvandva-test-creator
/// track under the team owner.
#[test]
fn f8_test_creation_to_cross_review_team() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        test_creation_track(v);
    });
    run(&b, &n).assert("f8 test_creation->cross_review team", 0);
}

/// The exit gate accepts an additional prativadi-owned adversarial-test track.
#[test]
fn f8_test_creation_to_cross_review_accepts_adversarial_track() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "test_creation", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        test_creation_track(v);
        adversarial_test_track(v);
    });
    run(&b, &n).assert(
        "f8 test_creation->cross_review accepts adversarial track",
        0,
    );
}

// ===================== F9: per-phase ceremony (phase_profiles) =====================

/// F9: a standard phase inside a full run is entered from spec via
/// spec_review -> implementing (the target phase's effective profile).
#[test]
fn f9_standard_phase_entry_in_full_run() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        v["phase_profiles"] = json!({"1": "standard"});
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        v["phase_profiles"] = json!({"1": "standard"});
        // S4-T2 (D2): the spec→implementation boundary locks the plan.
        v["master_plan_locked"] = json!(true);
    });
    run(&b, &n).assert("f9 standard phase entry in full run", 0);
}

/// F9: the implementing <-> phase_review loop of a standard phase is legal
/// inside a full run (edge selection by the current phase's effective profile).
#[test]
fn f9_standard_phase_impl_review_loop() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |v| {
        v["phase_profiles"] = json!({"1": "standard"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |v| {
        v["phase_profiles"] = json!({"1": "standard"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["phase"] = json!(1);
    });
    run(&b, &n).assert("f9 standard phase impl<->phase_review loop", 0);
}

/// F9 cross-profile advancement: a full phase (deslop) advances into a standard
/// next phase (implementing) via the new deslop->implementing edge.
#[test]
fn f9_advance_full_phase_to_standard_next() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["phase_profiles"] = json!({"2": "standard"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        v["phase_profiles"] = json!({"2": "standard"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(2);
    });
    run(&b, &n).assert("f9 advance full->standard next phase", 0);
}

/// F9 cross-profile advancement: a standard phase (phase_review) advances into a
/// full next phase (parallel_implementing) via the new edge.
#[test]
fn f9_advance_standard_phase_to_full_next() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_review", "prativadi", 4, |v| {
        v["phase_profiles"] = json!({"1": "standard", "2": "full"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        v["phase_profiles"] = json!({"1": "standard", "2": "full"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(2);
        parallel_chunks_phase(v, "2");
    });
    run(&b, &n).assert("f9 advance standard->full next phase", 0);
}

/// F9 final-phase termination: a standard final phase reaches termination_review
/// via phase_review->termination_review even in a full run.
#[test]
fn f9_standard_final_phase_termination() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_review", "prativadi", 4, |v| {
        v["phase_profiles"] = json!({"1": "standard"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "termination_review", "team", 5, |v| {
        v["phase_profiles"] = json!({"1": "standard"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["phase"] = json!(1);
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    run(&b, &n).assert("f9 standard final phase termination", 0);
}

/// F9 (vice versa): a full phase inside a standard run is entered from spec via
/// spec_review -> parallel_implementing.
#[test]
fn f9_full_phase_entry_in_standard_run() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        standard_profile(v);
        v["phase_profiles"] = json!({"1": "full"});
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        standard_profile(v);
        v["phase_profiles"] = json!({"1": "full"});
        v["phase"] = json!(1);
        // S4-T2 (D2): the spec→implementation boundary locks the plan.
        v["master_plan_locked"] = json!(true);
        parallel_chunks(v);
    });
    run(&b, &n).assert("f9 full phase entry in standard run", 0);
}

/// F9: phase_profiles may be SET during a spec-state write.
#[test]
fn f9_phase_profiles_spec_write_may_set() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton_v2(&n, "spec_review", "prativadi", 5, |v| {
        v["phase_profiles"] = json!({"1": "standard"});
    });
    run(&b, &n).assert("f9 spec-state write may set phase_profiles", 0);
}

/// F9: a NON-spec write that changes phase_profiles is rejected.
#[test]
fn f9_phase_profiles_non_spec_mutation_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |v| {
        standard_profile(v);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["phase"] = json!(1);
        v["phase_profiles"] = json!({"1": "standard"});
    });
    run(&b, &n).assert_contains(
        "f9 non-spec phase_profiles mutation rejected",
        23,
        "bad_phase_profiles",
    );
}

/// F9 per-phase floor: a phase declared "standard" may not carry a hard path in
/// its own work_split chunks (message names the phase + triggering path).
#[test]
fn f9_per_phase_floor_hard_path_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_drafting", "vadi", 4, |_| {});
    make_baton_v2(&n, "spec_review", "prativadi", 5, |v| {
        v["phase_profiles"] = json!({"1": "standard"});
        v["work_split"] = json!([{
            "id": "c1",
            "phase": "1",
            "chunk_type": "implementation",
            "owner": "vadi",
            "owner_role": "vadi",
            "scope": "phase-1 chunk touching a hard path",
            "paths": ["rust/dvandva/src/foo.rs"],
            "cross_review_by": "prativadi",
            "can_parallelize": false,
            "parallel_rationale": "n/a",
            "depends_on": [],
            "status": "planned",
            "artifact_refs": []
        }]);
    });
    run(&b, &n).assert_contains(
        "f9 per-phase floor rejects hard path in standard phase",
        23,
        "bad_phase_profiles phase=1",
    );
}

// ===================== F6: risk-triggered deep-review angles =====================

fn f6_deep(b: &std::path::Path) {
    make_baton_v2(b, "deep_review", "prativadi", 4, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
    });
}

fn f6_integration_work_split(v: &mut Value) {
    v["work_split"] = json!([
        {"id": "a", "phase": "1", "chunk_type": "implementation", "owner": "vadi",
         "owner_role": "vadi", "scope": "chunk a", "paths": ["src/a.ts"],
         "cross_review_by": "prativadi", "can_parallelize": true, "parallel_rationale": "x",
         "depends_on": [], "conflict_group": "shared", "status": "planned", "artifact_refs": []},
        {"id": "b", "phase": "1", "chunk_type": "implementation", "owner": "prativadi",
         "owner_role": "prativadi", "scope": "chunk b", "paths": ["src/b.ts"],
         "cross_review_by": "vadi", "can_parallelize": true, "parallel_rationale": "x",
         "depends_on": [], "conflict_group": "shared", "status": "planned", "artifact_refs": []}
    ]);
}

/// F6: a credential-touching diff requires the SECURITY angle.
#[test]
fn f6_security_angle_required_when_triggered() {
    let d = tmp();
    let (b, n) = paths(&d);
    f6_deep(&b);
    make_baton_v2(&n, "deslop", "vadi", 5, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["changed_paths"] = json!([".env"]);
        review_angles(v);
    });
    run(&b, &n).assert_contains(
        "f6 security angle required",
        23,
        "bad_deep_review_angles missing_angle=security",
    );
}

/// F6: the SECURITY angle satisfies the credential-touching trigger.
#[test]
fn f6_security_angle_satisfied() {
    let d = tmp();
    let (b, n) = paths(&d);
    f6_deep(&b);
    make_baton_v2(&n, "deslop", "vadi", 5, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["changed_paths"] = json!([".env"]);
        review_angles(v);
        security_review_track(v);
    });
    run(&b, &n).assert("f6 security angle satisfied", 0);
}

/// F6: a multi-owner shared-conflict-group phase requires the INTEGRATION angle.
#[test]
fn f6_integration_angle_required_when_triggered() {
    let d = tmp();
    let (b, n) = paths(&d);
    f6_deep(&b);
    make_baton_v2(&n, "deslop", "vadi", 5, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["changed_paths"] = json!(["src/clean.ts"]);
        review_angles(v);
        f6_integration_work_split(v);
    });
    run(&b, &n).assert_contains(
        "f6 integration angle required",
        23,
        "bad_deep_review_angles missing_angle=integration",
    );
}

/// F6: the INTEGRATION angle satisfies the multi-owner-seam trigger.
#[test]
fn f6_integration_angle_satisfied() {
    let d = tmp();
    let (b, n) = paths(&d);
    f6_deep(&b);
    make_baton_v2(&n, "deslop", "vadi", 5, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["changed_paths"] = json!(["src/clean.ts"]);
        review_angles(v);
        f6_integration_work_split(v);
        integration_review_track(v);
    });
    run(&b, &n).assert("f6 integration angle satisfied", 0);
}

/// F6: a non-triggering deep_review->deslop pays nothing (base three angles only).
#[test]
fn f6_non_triggering_no_change() {
    let d = tmp();
    let (b, n) = paths(&d);
    f6_deep(&b);
    make_baton_v2(&n, "deslop", "vadi", 5, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(1);
        v["changed_paths"] = json!(["src/clean.ts"]);
        review_angles(v);
        v["work_split"] = json!([{
            "id": "solo", "phase": "1", "chunk_type": "implementation", "owner": "vadi",
            "owner_role": "vadi", "scope": "single chunk", "paths": ["src/solo.ts"],
            "cross_review_by": "prativadi", "can_parallelize": false, "parallel_rationale": "x",
            "depends_on": [], "status": "planned", "artifact_refs": []
        }]);
    });
    run(&b, &n).assert("f6 non-triggering run pays nothing", 0);
}

// ===================== F10: explainer-verification gate =====================

fn f10_termination(b: &std::path::Path) {
    make_baton_v2(b, "termination_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
    });
}

/// F10: a full-profile terminal done without the doc-verifier track is rejected.
#[test]
fn f10_explainer_verification_missing_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    f10_termination(&b);
    make_baton_v2(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
    });
    run(&b, &n).assert_contains(
        "f10 explainer-verification missing",
        23,
        "bad_explainer_verification",
    );
}

/// F10: a completed current-cycle doc-verifier track satisfies the gate.
#[test]
fn f10_explainer_verification_present_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path()); // S4-T1
    f10_termination(&b);
    make_baton_v2(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        explainer_verification_track(v);
        done_matrix_fresh(v); // S4-T6
    });
    run(&b, &n).assert("f10 explainer-verification present", 0);
}

/// F10 (stale-track bypass): the done gate must reject an explainer-verification
/// track from a SUPERSEDED termination_review cycle. The current
/// termination_review entered at checkpoint 4, but the only doc-verifier track
/// carries review_checkpoint=1 (an older cycle) — merely naming
/// phase="termination_review" must NOT satisfy the gate.
#[test]
fn f10_stale_explainer_verification_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    f10_termination(&b); // current termination_review entered at checkpoint 4
    make_baton_v2(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        push(
            v,
            "subagent_tracks",
            json!({
                "id": "explainer-verification-stale",
                "phase": "termination_review",
                "status": "completed",
                "track": "explainer-verification",
                "owner": "dvandva-doc-verifier",
                "review_checkpoint": 1,
                "parallelized": false,
                "rationale": "Stale doc-verifier evidence from a superseded termination cycle.",
                "inputs": ["run explainer", "final diff"],
                "outputs": ["Explainer claims verified against observable behavior."],
                "evidence_refs": ["subagent:explainer-verification-stale"],
                "result": "approved"
            }),
        );
    });
    run(&b, &n).assert_contains(
        "f10 stale explainer-verification from a superseded cycle rejected",
        23,
        "bad_explainer_verification",
    );
}

// ===================== F9: advancement entry-state gating =====================

/// F9: a full phase's `deslop` advancing into a FULL next phase must use
/// `parallel_implementing`; the wrong (`implementing`) entry state is rejected.
#[test]
fn f9_full_deslop_rejects_wrong_entry_state_for_full_next_phase() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["phase_profiles"] = json!({"2": "full"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        v["phase_profiles"] = json!({"2": "full"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(2);
    });
    run(&b, &n).assert_contains(
        "f9 full deslop rejects implementing entry for a full next phase",
        24,
        "target_phase=2 effective_profile=full requires=parallel_implementing",
    );
}

/// F9 (symmetric): a standard phase's `phase_review` advancing into a STANDARD
/// next phase must use `implementing`; the wrong (`parallel_implementing`) entry
/// state is rejected.
#[test]
fn f9_standard_phase_review_rejects_wrong_entry_state_for_standard_next_phase() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_review", "prativadi", 4, |v| {
        standard_profile(v);
        v["phase_profiles"] = json!({"2": "standard"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        standard_profile(v);
        v["phase_profiles"] = json!({"2": "standard"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(2);
        parallel_chunks_phase(v, "2");
    });
    run(&b, &n).assert_contains(
        "f9 standard phase_review rejects parallel_implementing entry for a standard next phase",
        24,
        "target_phase=2 effective_profile=standard requires=implementing",
    );
}

/// F9 accept (pairs with the reject above): full deslop -> full next phase with
/// the CORRECT `parallel_implementing` entry state.
#[test]
fn f9_full_deslop_accepts_parallel_implementing_for_full_next_phase() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deslop", "vadi", 4, |v| {
        v["phase_profiles"] = json!({"2": "full"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        v["phase_profiles"] = json!({"2": "full"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(2);
        parallel_chunks_phase(v, "2");
    });
    run(&b, &n).assert(
        "f9 full deslop accepts parallel_implementing for a full next phase",
        0,
    );
}

/// F9 accept (pairs with the symmetric reject): standard phase_review -> standard
/// next phase with the CORRECT `implementing` entry state.
#[test]
fn f9_standard_phase_review_accepts_implementing_for_standard_next_phase() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "phase_review", "prativadi", 4, |v| {
        standard_profile(v);
        v["phase_profiles"] = json!({"2": "standard"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        standard_profile(v);
        v["phase_profiles"] = json!({"2": "standard"});
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["phase"] = json!(2);
    });
    run(&b, &n).assert(
        "f9 standard phase_review accepts implementing for a standard next phase",
        0,
    );
}

// ===========================================================================
// S2-T1: `abandoned` terminal status (v2 only).
// ===========================================================================

#[test]
fn s2t1_abandoned_from_human_question_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "human_question", "human", 4, |v| {
        v["phase"] = json!(1);
        v["question"] = json!("Should we keep going?");
        v["resume_assignee"] = json!("vadi");
        v["resume_status"] = json!("implementing");
    });
    make_baton_v2(&n, "abandoned", "human", 5, |v| {
        v["phase"] = json!(1);
    });
    run(&b, &n).assert("s2t1 abandoned from human_question is legal", 0);
}

#[test]
fn s2t1_abandoned_from_human_decision_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "human_decision", "human", 4, |v| {
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "abandoned", "human", 5, |v| {
        v["phase"] = json!(1);
    });
    run(&b, &n).assert("s2t1 abandoned from human_decision is legal", 0);
}

#[test]
fn s2t1_abandoned_from_working_state_illegal_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |v| {
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "abandoned", "human", 5, |v| {
        v["phase"] = json!(1);
    });
    run(&b, &n).assert("s2t1 abandoned from a working state is illegal", 24);
}

#[test]
fn s2t1_abandoned_from_spec_state_illegal_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "abandoned", "human", 5, |v| {
        v["phase"] = json!("spec");
    });
    run(&b, &n).assert("s2t1 abandoned from a spec state is illegal", 24);
}

#[test]
fn s2t1_abandoned_has_no_exit_edges_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "abandoned", "human", 4, |v| {
        v["phase"] = json!(1);
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        v["phase"] = json!(1);
    });
    run(&b, &n).assert(
        "s2t1 abandoned is terminal (no exit edge to implementing)",
        24,
    );
}

#[test]
fn s2t1_v1_abandoned_status_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton(&n, "abandoned", "human", 5, |_| {});
    run(&b, &n).assert_contains("s2t1 v1 never gains abandoned", 23, "bad_status");
}

// ===========================================================================
// S4-T2: master_plan_locked modeling (D2).
// ===========================================================================

#[test]
fn s4t2_spec_review_to_parallel_implementing_requires_locked() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(false);
        parallel_chunks(v);
    });
    run(&b, &n).assert_contains(
        "s4t2 spec_review->parallel_implementing requires master_plan_locked",
        23,
        "bad_master_plan_locked",
    );
}

#[test]
fn s4t2_spec_review_to_parallel_implementing_locked_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |_| {});
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
        parallel_chunks(v);
    });
    run(&b, &n).assert(
        "s4t2 spec_review->parallel_implementing with lock is legal",
        0,
    );
}

#[test]
fn s4t2_spec_review_to_implementing_requires_locked() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, standard_profile);
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        standard_profile(v);
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(false);
    });
    run(&b, &n).assert_contains(
        "s4t2 spec_review->implementing requires master_plan_locked",
        23,
        "bad_master_plan_locked",
    );
}

#[test]
fn s4t2_amendment_exit_requires_locked() {
    // "incl. amendment exits": a spec_review amendment exit that clears the lock
    // is rejected exactly like the fresh spec entry.
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        v["master_plan_locked"] = json!(true);
        v["total_phases"] = json!(2);
        v["amendment_from_phase"] = json!(1);
    });
    make_baton_v2(&n, "parallel_implementing", "team", 5, |v| {
        v["master_plan_locked"] = json!(false);
        v["total_phases"] = json!(2);
        v["phase"] = json!(1);
        parallel_chunks(v);
    });
    run(&b, &n).assert_contains(
        "s4t2 amendment exit cannot clear master_plan_locked",
        23,
        "bad_master_plan_locked",
    );
}

#[test]
fn s4t2_unlock_forbidden_on_a_normal_edge() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |v| {
        standard_profile(v);
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
    });
    make_baton_v2(&n, "phase_review", "prativadi", 5, |v| {
        standard_profile(v);
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(false);
    });
    run(&b, &n).assert_contains(
        "s4t2 master_plan_locked true->false is forbidden",
        23,
        "unlock_forbidden",
    );
}

#[test]
fn s4t2_unlock_allowed_into_human_decision() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, |v| {
        standard_profile(v);
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
    });
    make_baton_v2(&n, "human_decision", "human", 5, |v| {
        standard_profile(v);
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(false);
    });
    run(&b, &n).assert("s4t2 human_decision may clear master_plan_locked", 0);
}

// ===========================================================================
// S4-T5: widen human_question entry (D1).
// ===========================================================================

fn s4t5_working_baton(status: &str, b: &std::path::Path) {
    let owner = v2_status_owner(status);
    make_baton_v2(b, status, owner, 4, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
        if v2_status_owner(status) == "team" {
            v["active_roles"] = json!(["vadi", "prativadi"]);
        }
    });
}

fn s4t5_human_question_candidate(resume_status: &str, resume_assignee: &str, n: &std::path::Path) {
    let rs = resume_status.to_string();
    let ra = resume_assignee.to_string();
    make_baton_v2(n, "human_question", "human", 5, move |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
        v["question"] = json!("Human, should we keep going down this path?");
        v["resume_assignee"] = json!(ra);
        v["resume_status"] = json!(rs);
    });
}

#[test]
fn s4t5_human_question_from_implementing_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    s4t5_working_baton("implementing", &b);
    s4t5_human_question_candidate("implementing", "vadi", &n);
    run(&b, &n).assert("s4t5 post-lock human_question from implementing", 0);
}

#[test]
fn s4t5_human_question_from_parallel_implementing_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    s4t5_working_baton("parallel_implementing", &b);
    s4t5_human_question_candidate("parallel_implementing", "team", &n);
    run(&b, &n).assert(
        "s4t5 post-lock human_question from parallel_implementing",
        0,
    );
}

#[test]
fn s4t5_human_question_from_test_creation_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    s4t5_working_baton("test_creation", &b);
    s4t5_human_question_candidate("test_creation", "team", &n);
    run(&b, &n).assert("s4t5 post-lock human_question from test_creation", 0);
}

#[test]
fn s4t5_human_question_from_cross_fixing_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    s4t5_working_baton("cross_fixing", &b);
    s4t5_human_question_candidate("cross_fixing", "team", &n);
    run(&b, &n).assert("s4t5 post-lock human_question from cross_fixing", 0);
}

#[test]
fn s4t5_human_question_from_phase_fixing_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    s4t5_working_baton("phase_fixing", &b);
    s4t5_human_question_candidate("phase_fixing", "vadi", &n);
    run(&b, &n).assert("s4t5 post-lock human_question from phase_fixing", 0);
}

#[test]
fn s4t5_human_question_from_cross_review_illegal_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    s4t5_working_baton("cross_review", &b);
    s4t5_human_question_candidate("cross_review", "team", &n);
    run(&b, &n).assert("s4t5 human_question NOT allowed from cross_review", 24);
}

#[test]
fn s4t5_human_question_from_deep_review_illegal_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    s4t5_working_baton("deep_review", &b);
    s4t5_human_question_candidate("deep_review", "prativadi", &n);
    run(&b, &n).assert("s4t5 human_question NOT allowed from deep_review", 24);
}

#[test]
fn s4t5_human_question_from_deslop_illegal_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    s4t5_working_baton("deslop", &b);
    s4t5_human_question_candidate("deslop", "vadi", &n);
    run(&b, &n).assert("s4t5 human_question NOT allowed from deslop", 24);
}

#[test]
fn s4t5_human_question_from_termination_review_illegal_24() {
    let d = tmp();
    let (b, n) = paths(&d);
    s4t5_working_baton("termination_review", &b);
    s4t5_human_question_candidate("termination_review", "team", &n);
    run(&b, &n).assert(
        "s4t5 human_question NOT allowed from termination_review",
        24,
    );
}

#[test]
fn s4t5_human_question_resume_roundtrip_from_working_state() {
    // Enter human_question from implementing, then resume back to implementing —
    // the recorded resume fields restore the exact prior state.
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "human_question", "human", 4, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
        v["question"] = json!("Human, should we keep going?");
        v["resume_assignee"] = json!("vadi");
        v["resume_status"] = json!("implementing");
    });
    make_baton_v2(&n, "implementing", "vadi", 5, |v| {
        v["phase"] = json!(1);
        v["master_plan_locked"] = json!(true);
    });
    run(&b, &n).assert("s4t5 human_question resumes back to the working state", 0);
}

// ===========================================================================
// S4-T7: review-mode deep_review -> phase_fixing.
// ===========================================================================

#[test]
fn s4t7_review_mode_deep_review_to_phase_fixing_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "deep_review", "prativadi", 4, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        set_loop_count(v, "deep_review:phase_fixing", 0);
    });
    make_baton_v2(&n, "phase_fixing", "vadi", 5, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        set_loop_count(v, "deep_review:phase_fixing", 1);
    });
    run(&b, &n).assert(
        "s4t7 review-mode deep_review->phase_fixing hands fixes back to vadi",
        0,
    );
}

// ===================== S4-T1: missing_artifact done gate =====================

/// A full-profile dev `done` candidate that clears F10 + S4-T6 (fresh matrix) +
/// approvals, so the only remaining done gate under test is S4-T1
/// missing_artifact. Pairs with the `f10_termination` current baton.
fn full_done_candidate(n: &std::path::Path) {
    make_baton_v2(n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        explainer_verification_track(v); // F10
        done_matrix_fresh(v); // S4-T6
    });
}

#[test]
fn s4t1_dev_full_done_missing_research_ref_file_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    // run_explainer present, research_ref file absent -> research_ref checked first.
    write_artifact(
        d.path(),
        "superpowers/run-reports/2026-06-28-run-a-explainer.html",
    );
    f10_termination(&b);
    full_done_candidate(&n);
    run(&b, &n).assert_contains(
        "s4t1 dev full done missing research_ref file",
        23,
        "missing_artifact ref=research_ref",
    );
}

#[test]
fn s4t1_dev_full_done_missing_run_explainer_file_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    // research_ref present, run_explainer file absent.
    write_artifact(d.path(), "superpowers/research/run-a.html");
    f10_termination(&b);
    full_done_candidate(&n);
    run(&b, &n).assert_contains(
        "s4t1 dev full done missing run_explainer_ref file",
        23,
        "missing_artifact ref=run_explainer_ref",
    );
}

#[test]
fn s4t1_dev_full_done_empty_run_explainer_file_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    write_artifact(d.path(), "superpowers/research/run-a.html");
    // run_explainer exists but is EMPTY -> not a non-empty regular file.
    let empty = d
        .path()
        .join("superpowers/run-reports/2026-06-28-run-a-explainer.html");
    std::fs::create_dir_all(empty.parent().unwrap()).unwrap();
    std::fs::write(&empty, b"").unwrap();
    f10_termination(&b);
    full_done_candidate(&n);
    run(&b, &n).assert_contains(
        "s4t1 dev full done empty run_explainer file",
        23,
        "missing_artifact ref=run_explainer_ref",
    );
}

#[test]
fn s4t1_dev_full_done_all_artifacts_present_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    f10_termination(&b);
    full_done_candidate(&n);
    run(&b, &n).assert("s4t1 dev full done all artifacts present", 0);
}

#[test]
fn s4t1_done_ref_resolving_to_directory_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    // replace the research_ref target file with a directory: not a regular file.
    let p = d.path().join("superpowers/research/run-a.html");
    std::fs::remove_file(&p).unwrap();
    std::fs::create_dir_all(&p).unwrap();
    f10_termination(&b);
    full_done_candidate(&n);
    run(&b, &n).assert_contains(
        "s4t1 done ref resolving to a directory",
        23,
        "missing_artifact ref=research_ref",
    );
}

#[test]
fn s4t1_dev_compact_done_missing_research_ref_file_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    // no artifacts seeded: research_ref file absent for a compact done.
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        standard_profile(v);
        compact_terminal_evidence(v);
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "team", 5, |v| {
        standard_profile(v);
        compact_terminal_evidence(v);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "s4t1 compact done missing research_ref file",
        23,
        "missing_artifact ref=research_ref",
    );
}

#[test]
fn s4t1_research_seed_done_missing_plan_ref_file_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    // research_ref present, plan_ref file absent on the seed path.
    write_artifact(d.path(), "superpowers/research/run-a.html");
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["mode"] = json!("research");
        v["phase"] = json!("spec");
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["mode"] = json!("research");
        v["phase"] = json!("spec");
        v["research_outcome"] = json!("seed_development");
        v["plan_ref"] = json!("./superpowers/plans/2026-06-29-run-modes-plan.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "s4t1 research seed done missing plan_ref file",
        23,
        "missing_artifact ref=plan_ref",
    );
}

#[test]
fn s4t1_research_exploratory_done_needs_no_plan_ref_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    // exploratory outcome requires only research_ref (present); no plan_ref file.
    write_artifact(d.path(), "superpowers/research/run-a.html");
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["mode"] = json!("research");
        v["phase"] = json!("spec");
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["mode"] = json!("research");
        v["phase"] = json!("spec");
        v["research_outcome"] = json!("exploratory");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert("s4t1 research exploratory done needs no plan_ref", 0);
}

#[test]
fn s4t1_review_done_missing_review_ref_file_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    // research_ref present, review_ref file absent.
    write_artifact(d.path(), "superpowers/research/run-a.html");
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "human", 5, |v| {
        v["mode"] = json!("review");
        v["phase"] = json!("review");
        v["review_ref"] = json!("./superpowers/reviews/review-run-modes-PR-1.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    run(&b, &n).assert_contains(
        "s4t1 review done missing review_ref file",
        23,
        "missing_artifact ref=review_ref",
    );
}

// ===================== S4-T6: stale_verification_matrix =====================

#[test]
fn s4t6_full_done_pending_matrix_row_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    f10_termination(&b);
    // seed verification_matrix rows keep result="pending" -> incomplete -> stale.
    make_baton_v2(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        explainer_verification_track(v);
    });
    run(&b, &n).assert_contains(
        "s4t6 full done pending matrix row",
        23,
        "stale_verification_matrix row=verify-research-coverage anchor=0",
    );
}

#[test]
fn s4t6_full_done_row_missing_checkpoint_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    f10_termination(&b);
    make_baton_v2(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        explainer_verification_track(v);
        // rows complete (passed) but carry NO numeric checkpoint -> stale.
        if let Some(rows) = v["verification_matrix"].as_array_mut() {
            for r in rows {
                r["result"] = json!("passed");
                r["evidence_refs"] = json!(["command:PASS"]);
            }
        }
    });
    run(&b, &n).assert_contains(
        "s4t6 full done matrix row missing checkpoint",
        23,
        "stale_verification_matrix",
    );
}

#[test]
fn s4t6_full_done_checkpoint_below_anchor_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    // history carries a parallel_implementing at checkpoint 3 -> anchor 3.
    let hist = d.path().join("history/3-parallel_implementing.json");
    std::fs::create_dir_all(hist.parent().unwrap()).unwrap();
    make_baton_v2(&hist, "parallel_implementing", "team", 3, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
    });
    f10_termination(&b);
    make_baton_v2(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        explainer_verification_track(v);
        // rows complete but evidence_checkpoint 2 < anchor 3 -> stale.
        if let Some(rows) = v["verification_matrix"].as_array_mut() {
            for r in rows {
                r["result"] = json!("passed");
                r["evidence_refs"] = json!(["command:PASS"]);
                r["evidence_checkpoint"] = json!(2);
            }
        }
    });
    run(&b, &n).assert_contains(
        "s4t6 full done matrix checkpoint below anchor",
        23,
        "stale_verification_matrix row=verify-research-coverage anchor=3",
    );
}

#[test]
fn s4t6_full_done_review_checkpoint_coalesced_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    f10_termination(&b);
    make_baton_v2(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        explainer_verification_track(v);
        // rows carry review_checkpoint (not evidence_checkpoint) -> coalesced fresh.
        if let Some(rows) = v["verification_matrix"].as_array_mut() {
            for r in rows {
                r["result"] = json!("approved");
                r["evidence_refs"] = json!(["command:PASS"]);
                r["review_checkpoint"] = json!(4);
            }
        }
    });
    run(&b, &n).assert("s4t6 full done review_checkpoint coalesced fresh", 0);
}

#[test]
fn s4t6_full_done_fresh_matrix_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    f10_termination(&b);
    full_done_candidate(&n); // done_matrix_fresh marks rows passed + fresh
    run(&b, &n).assert("s4t6 full done fresh matrix", 0);
}

#[test]
fn s4t6_object_matrix_stale_value_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    f10_termination(&b);
    make_baton_v2(&n, "done", "team", 5, |v| {
        v["run_explainer_ref"] = json!("./superpowers/run-reports/2026-06-28-run-a-explainer.html");
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        run_explainer_reviews(v);
        explainer_verification_track(v);
        // object matrix: row-a fresh, row-b missing a checkpoint -> stale (row-b).
        v["verification_matrix"] = json!({
            "row-a": {"result": "passed", "evidence_refs": ["e"], "evidence_checkpoint": 5},
            "row-b": {"result": "passed", "evidence_refs": ["e"]}
        });
    });
    run(&b, &n).assert_contains(
        "s4t6 object matrix stale value",
        23,
        "stale_verification_matrix row=row-b",
    );
}

#[test]
fn s4t6_compact_done_row_missing_checkpoint_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    seed_done_artifacts(d.path());
    make_baton_v2(&b, "termination_review", "team", 4, |v| {
        standard_profile(v);
        compact_terminal_evidence(v);
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
    });
    make_baton_v2(&n, "done", "team", 5, |v| {
        standard_profile(v);
        compact_terminal_evidence(v);
        v["vadi_final_approval"] = json!(true);
        v["prativadi_final_approval"] = json!(true);
        // strip the evidence_checkpoint compact_terminal_evidence set -> stale.
        if let Some(rows) = v["verification_matrix"].as_array_mut() {
            for r in rows {
                r.as_object_mut().unwrap().remove("evidence_checkpoint");
            }
        }
    });
    run(&b, &n).assert_contains(
        "s4t6 compact done matrix row missing checkpoint",
        23,
        "stale_verification_matrix",
    );
}

// ===================== S4-T4: lost_update superset guard =====================

/// A completed cross-review track fixture the installed baton carries and a
/// retry must not drop.
fn peer_track(id: &str) -> Value {
    json!({
        "id": id,
        "phase": "cross_review",
        "status": "completed",
        "track": "cross-review",
        "owner": "dvandva-cross-reviewer",
        "owner_role": "vadi",
        "parallelized": true,
        "rationale": "Installed peer review evidence.",
        "inputs": [],
        "outputs": ["Peer chunk reviewed."],
        "evidence_refs": ["subagent:peer"],
        "review_checkpoint": 4,
        "result": "approved"
    })
}

#[test]
fn s4t4_lost_update_subagent_tracks_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(v, "subagent_tracks", peer_track("peer-review-x"));
    });
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Team sync that drops installed peer evidence.");
        v["next_action"] = json!("Team: this retry lost peer-review-x.");
        // candidate keeps only the seed startup-controller (drops peer-review-x).
    });
    run(&b, &n).assert_contains(
        "s4t4 lost subagent track",
        23,
        "lost_update field=subagent_tracks missing=peer-review-x",
    );
}

#[test]
fn s4t4_lost_update_work_split_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(
            v,
            "work_split",
            json!({
                "id": "extra-chunk",
                "phase": "1",
                "chunk_type": "implementation",
                "owner": "vadi",
                "owner_role": "vadi",
                "scope": "Installed chunk a retry must retain.",
                "paths": ["src/extra.ts"],
                "cross_review_by": "prativadi",
                "can_parallelize": false,
                "parallel_rationale": "x",
                "depends_on": [],
                "status": "planned",
                "artifact_refs": []
            }),
        );
    });
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Team sync that drops an installed work_split chunk.");
        v["next_action"] = json!("Team: this retry lost extra-chunk.");
        // candidate keeps only the seed work_split (drops extra-chunk).
    });
    run(&b, &n).assert_contains(
        "s4t4 lost work_split chunk",
        23,
        "lost_update field=work_split missing=extra-chunk",
    );
}

#[test]
fn s4t4_lost_update_findings_string_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["findings"] = json!(["finding-keep", {"id": "finding-obj"}]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Team sync that drops a string finding.");
        v["next_action"] = json!("Team: this retry lost finding-keep.");
        v["findings"] = json!([{"id": "finding-obj"}]);
    });
    run(&b, &n).assert_contains(
        "s4t4 lost string finding",
        23,
        "lost_update field=findings missing=finding-keep",
    );
}

#[test]
fn s4t4_lost_update_findings_object_rejected() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["findings"] = json!([{"id": "finding-obj", "note": "keep me"}]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Team sync that drops an object finding.");
        v["next_action"] = json!("Team: this retry lost finding-obj.");
        v["findings"] = json!([]);
    });
    run(&b, &n).assert_contains(
        "s4t4 lost object finding",
        23,
        "lost_update field=findings missing=finding-obj",
    );
}

#[test]
fn s4t4_superset_or_grown_accepted() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(v, "subagent_tracks", peer_track("peer-review-x"));
        v["findings"] = json!(["finding-keep"]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        v["summary"] = json!("Team sync that keeps all peer data and adds more.");
        v["next_action"] = json!("Team: superset retry is accepted.");
        push(v, "subagent_tracks", peer_track("peer-review-x"));
        push(v, "subagent_tracks", peer_track("peer-review-y"));
        v["findings"] = json!(["finding-keep", "finding-new"]);
    });
    run(&b, &n).assert("s4t4 superset/grown id-sets accepted", 0);
}

#[test]
fn s4t4_human_decision_escalation_exempt() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |v| {
        v["active_roles"] = json!(["vadi", "prativadi"]);
        push(v, "subagent_tracks", peer_track("peer-review-x"));
    });
    // an escalation to human_decision must not be blocked by array bookkeeping,
    // even though it drops the installed peer track.
    make_baton_v2(&n, "human_decision", "human", 5, |v| {
        v["question"] = json!("A blocker needs a human decision.");
    });
    run(&b, &n).assert("s4t4 human_decision escalation is exempt", 0);
}

#[test]
fn s4t4_non_team_current_not_gated() {
    let d = tmp();
    let (b, n) = paths(&d);
    // spec_review is prativadi-owned (not team): dropping installed data is fine.
    make_baton_v2(&b, "spec_review", "prativadi", 4, |v| {
        push(
            v,
            "subagent_tracks",
            json!({
                "id": "peer-x",
                "phase": "spec",
                "status": "completed",
                "track": "spec-review",
                "owner": "prativadi",
                "owner_role": "prativadi",
                "parallelized": false,
                "rationale": "x",
                "inputs": [],
                "outputs": ["o"],
                "evidence_refs": ["e"],
                "result": "approved"
            }),
        );
    });
    make_baton_v2(&n, "spec_revision", "vadi", 5, |_| {
        // candidate keeps only the seed subagent track (drops peer-x) — allowed.
    });
    run(&b, &n).assert("s4t4 non-team current not gated by lost_update", 0);
}
