//! Shared fixtures for the `dvandva write` integration tests, ported from the
//! helper functions in `scripts/test-dvandva-write.sh` (`make_baton`,
//! `make_baton_v2`, and the `v2_*_filter` / profile blob builders).
//!
//! The shell suite drives the helper as a subprocess with `DVANDVA_ROLE`
//! unset and per-case env overrides; [`run`] / [`run_env`] reproduce that by
//! spawning `CARGO_BIN_EXE_dvandva write <baton> <candidate>` with the three
//! `DVANDVA_*` env vars cleared unless a case sets them.
//!
//! jq override filters become `impl FnOnce(&mut Value)` closures; the reusable
//! blob filters become the `fn(&mut Value)` mutators below.

#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Schema seeds (embedded from the repo references at compile time).
// ---------------------------------------------------------------------------
const V1_SEED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../plugins/dvandva/references/baton-schema.json"
));
const V2_SEED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../plugins/dvandva/references/baton-schema-v2.json"
));

pub fn v1_seed() -> Value {
    serde_json::from_str(V1_SEED).expect("v1 seed parses")
}
pub fn v2_seed() -> Value {
    serde_json::from_str(V2_SEED).expect("v2 seed parses")
}

// ---------------------------------------------------------------------------
// Baton builders
// ---------------------------------------------------------------------------

/// `make_baton <file> <status> <assignee> <checkpoint> [mutate]` (v1).
pub fn make_baton(
    path: &Path,
    status: &str,
    assignee: &str,
    checkpoint: i64,
    mutate: impl FnOnce(&mut Value),
) {
    let mut b = v1_seed();
    b["status"] = json!(status);
    b["assignee"] = json!(assignee);
    b["checkpoint"] = json!(checkpoint);
    b["master_plan_locked"] = json!(false);
    b["question"] = Value::Null;
    b["resume_assignee"] = Value::Null;
    b["resume_status"] = Value::Null;
    mutate(&mut b);
    write_json(path, &b);
}

/// `make_baton_v2 <file> <status> <assignee> <checkpoint> [mutate]`.
pub fn make_baton_v2(
    path: &Path,
    status: &str,
    assignee: &str,
    checkpoint: i64,
    mutate: impl FnOnce(&mut Value),
) {
    let phase: Value = match status {
        "clarifying_questions_drafting"
        | "clarifying_questions_answer"
        | "clarifying_questions_followup"
        | "clarifying_questions_followup_answer" => json!("clarifying"),
        "spec_drafting" | "spec_review" | "spec_revision" => json!("spec"),
        "implementing"
        | "parallel_implementing"
        | "test_creation"
        | "cross_review"
        | "cross_fixing"
        | "deep_review"
        | "deslop"
        | "termination_review"
        | "phase_review"
        | "phase_fixing"
        | "review_of_review"
        | "counter_review"
        | "done" => json!(1),
        _ => json!("research"),
    };
    let mut b = v2_seed();
    b["updated_at"] = json!("2026-06-27T00:00:00Z");
    b["status"] = json!(status);
    b["assignee"] = json!(assignee);
    b["checkpoint"] = json!(checkpoint);
    b["phase"] = phase;
    b["run_id"] = json!("run-a");
    b["original_ask"] = json!("Original user ask for v2 enforcement");
    b["research_ref"] = json!("./superpowers/research/run-a.html");
    b["profile"] = json!("full");
    b["profile_floor"] = json!("full");
    b["profile_decision"] = json!({
        "selected_profile": "full",
        "floor": "full",
        "reason": "test helper default preserves the existing full v2 development graph unless a case overrides it",
        "decided_by": "test-suite",
        "decided_at": "2026-07-01T00:00:00Z",
        "risk_inputs": [],
        "hard_triggers": [],
        "allowlist_match": false,
        "allowlist_refs": [],
        "evidence_refs": ["test-helper"]
    });
    b["profile_history"] = json!([]);
    b["current_engine"] = json!("codex");
    b["branch"] = json!("test-branch");
    b["master_plan_locked"] = json!(false);
    b["question"] = Value::Null;
    b["resume_assignee"] = Value::Null;
    b["resume_status"] = Value::Null;
    mutate(&mut b);
    write_json(path, &b);
}

/// `make_baton_v3 <file> <status> <assignee> <checkpoint> [mutate]`.
pub fn make_baton_v3(
    path: &Path,
    status: &str,
    assignee: &str,
    checkpoint: i64,
    mutate: impl FnOnce(&mut Value),
) {
    let phase: Value = match status {
        "clarifying_questions_drafting"
        | "clarifying_questions_answer"
        | "clarifying_questions_followup"
        | "clarifying_questions_followup_answer" => json!("clarifying"),
        "spec_drafting" | "spec_review" | "spec_revision" => json!("spec"),
        "implementing"
        | "parallel_implementing"
        | "test_creation"
        | "cross_review"
        | "cross_fixing"
        | "deep_review"
        | "deslop"
        | "termination_review"
        | "phase_review"
        | "phase_fixing"
        | "review_of_review"
        | "counter_review"
        | "done" => json!(1),
        _ => json!("research"),
    };
    let mut b = v2_seed();
    b["schema"] = json!("dvandva.baton.v3");
    b["updated_at"] = json!("2026-07-06T00:00:00Z");
    b["status"] = json!(status);
    b["assignee"] = json!(assignee);
    b["checkpoint"] = json!(checkpoint);
    b["phase"] = phase;
    b["run_id"] = json!("run-a");
    b["original_ask"] = json!("Original user ask for v3 enforcement");
    b["research_ref"] = json!("./superpowers/research/run-a.html");
    b["profile"] = json!("full");
    b["profile_floor"] = json!("full");
    b["profile_decision"] = json!({
        "selected_profile": "full",
        "floor": "full",
        "reason": "test helper default preserves the existing full v3 development graph unless a case overrides it",
        "decided_by": "test-suite",
        "decided_at": "2026-07-06T00:00:00Z",
        "risk_inputs": [],
        "hard_triggers": [],
        "allowlist_match": false,
        "allowlist_refs": [],
        "evidence_refs": ["test-helper"]
    });
    b["run_workflow"] = json!({
        "source": "preset:full",
        "declared_by": "vadi",
        "declared_at_checkpoint": 0,
        "approved_by": null,
        "approved_at_checkpoint": null,
        "revision_round": 0,
        "states": [],
        "edges": [],
        "amendments": []
    });
    b["profile_history"] = json!([]);
    b["current_engine"] = json!("codex");
    b["branch"] = json!("test-branch");
    b["master_plan_locked"] = json!(false);
    b["question"] = Value::Null;
    b["resume_assignee"] = Value::Null;
    b["resume_status"] = Value::Null;
    mutate(&mut b);
    write_json(path, &b);
}

fn write_json(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

/// The v2 owner the shell suite assigns to a from/to status (`v2_status_owner`).
pub fn v2_status_owner(status: &str) -> &'static str {
    match status {
        "research_drafting" | "research_revision" | "spec_drafting" | "spec_revision"
        | "implementing" | "deslop" | "phase_fixing" | "review_of_review" => "vadi",
        "parallel_implementing"
        | "test_creation"
        | "cross_review"
        | "cross_fixing"
        | "termination_review" => "team",
        "research_review" | "spec_review" | "deep_review" | "phase_review" | "counter_review" => {
            "prativadi"
        }
        "human_question" | "human_decision" => "human",
        "done" => "team",
        _ => "vadi",
    }
}

// ---------------------------------------------------------------------------
// Subprocess runner
// ---------------------------------------------------------------------------
pub struct Out {
    pub code: i32,
    pub text: String,
}

impl Out {
    pub fn assert(&self, name: &str, expected: i32) {
        assert_eq!(
            self.code, expected,
            "case '{name}': expected exit {expected}, got {}\noutput:\n{}",
            self.code, self.text
        );
    }
    pub fn assert_contains(&self, name: &str, expected: i32, needle: &str) {
        self.assert(name, expected);
        assert!(
            self.text.contains(needle),
            "case '{name}': output missing '{needle}'\noutput:\n{}",
            self.text
        );
    }
}

fn spawn(baton: &Path, candidate: &Path, envs: &[(&str, &str)]) -> Out {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dvandva"));
    cmd.arg("write").arg(baton).arg(candidate);
    cmd.env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_LOCK_TIMEOUT")
        .env_remove("DVANDVA_WRITE_BARRIER")
        .env_remove("DVANDVA_WRITE_BARRIER_POSTFENCE");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("failed to run dvandva write");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Out {
        code: output.status.code().unwrap_or(-1),
        text,
    }
}

pub fn run(baton: &Path, candidate: &Path) -> Out {
    spawn(baton, candidate, &[])
}
pub fn run_env(baton: &Path, candidate: &Path, envs: &[(&str, &str)]) -> Out {
    spawn(baton, candidate, envs)
}

/// Append `value` to the array at `key` (creating it if absent).
pub fn push(b: &mut Value, key: &str, value: Value) {
    if !b[key].is_array() {
        b[key] = json!([]);
    }
    b[key].as_array_mut().unwrap().push(value);
}

// ===========================================================================
// jq filter blob ports (each mutates the baton Value in place).
// ===========================================================================

/// `v2_review_angles_filter` — three completed deep-review angle tracks.
pub fn review_angles(b: &mut Value) {
    for (id, track, subagent) in [
        (
            "review-correctness",
            "correctness-regression",
            "review-correctness",
        ),
        ("review-tests", "test-evidence", "review-tests"),
        ("review-protocol", "protocol-handoff", "review-protocol"),
    ] {
        let owner = if track == "protocol-handoff" {
            "dvandva-baton-auditor"
        } else {
            "dvandva-deep-reviewer"
        };
        push(
            b,
            "subagent_tracks",
            json!({
                "id": id,
                "phase": "deep_review",
                "status": "completed",
                "track": track,
                "review_checkpoint": 4,
                "owner": owner,
                "parallelized": true,
                "rationale": "Independent review angle can run without editing shared files.",
                "inputs": ["candidate diff"],
                "outputs": ["No blockers found."],
                "evidence_refs": [format!("subagent:{subagent}")],
                "result": "passed"
            }),
        );
    }
}

/// `v2_parallel_chunks_filter` — active_roles + five two-team impl chunks.
pub fn parallel_chunks(b: &mut Value) {
    b["active_roles"] = json!(["vadi", "prativadi"]);
    for (id, owner, path) in [
        ("implementation-chunk-a", "vadi", "src/a.ts"),
        ("implementation-chunk-b", "vadi", "src/b.ts"),
        ("implementation-chunk-c", "prativadi", "src/c.ts"),
        ("implementation-chunk-d", "prativadi", "src/d.ts"),
        ("implementation-chunk-e", "vadi", "src/e.ts"),
    ] {
        let reviewer = if owner == "vadi" { "prativadi" } else { "vadi" };
        push(
            b,
            "work_split",
            json!({
                "id": id,
                "phase": "1",
                "chunk_type": "implementation",
                "owner": owner,
                "owner_role": owner,
                "suggested_agent": "dvandva-implementer",
                "scope": "Two-team implementation chunk.",
                "paths": [path],
                "cross_review_by": reviewer,
                "can_parallelize": true,
                "parallel_rationale": "Independent file.",
                "depends_on": [],
                "status": "planned",
                "artifact_refs": []
            }),
        );
    }
}

/// Like [`parallel_chunks`] but for an arbitrary numeric `phase` (F9 mixed
/// profile: entering a full phase N > 1).
pub fn parallel_chunks_phase(b: &mut Value, phase: &str) {
    b["active_roles"] = json!(["vadi", "prativadi"]);
    b["work_split"] = json!([]);
    for (id, owner, path) in [
        ("impl-chunk-a", "vadi", "src/pa.ts"),
        ("impl-chunk-b", "vadi", "src/pb.ts"),
        ("impl-chunk-c", "prativadi", "src/pc.ts"),
        ("impl-chunk-d", "prativadi", "src/pd.ts"),
        ("impl-chunk-e", "vadi", "src/pe.ts"),
    ] {
        let reviewer = if owner == "vadi" { "prativadi" } else { "vadi" };
        push(
            b,
            "work_split",
            json!({
                "id": id,
                "phase": phase,
                "chunk_type": "implementation",
                "owner": owner,
                "owner_role": owner,
                "suggested_agent": "dvandva-implementer",
                "scope": "Two-team implementation chunk.",
                "paths": [path],
                "cross_review_by": reviewer,
                "can_parallelize": true,
                "parallel_rationale": "Independent file.",
                "depends_on": [],
                "status": "planned",
                "artifact_refs": []
            }),
        );
    }
}

/// F6: a completed current-cycle SECURITY-angle track (review_checkpoint 4).
pub fn security_review_track(b: &mut Value) {
    push(
        b,
        "subagent_tracks",
        json!({
            "id": "security-angle",
            "phase": "deep_review",
            "status": "completed",
            "track": "security-audit",
            "review_checkpoint": 4,
            "owner": "dvandva-security-auditor",
            "parallelized": true,
            "rationale": "Security auditor reviewed the credential-touching diff.",
            "inputs": ["candidate diff"],
            "outputs": ["No secret leakage or injection vector found."],
            "evidence_refs": ["subagent:security-angle"],
            "result": "passed"
        }),
    );
}

/// F6: a completed current-cycle INTEGRATION-angle track (review_checkpoint 4).
pub fn integration_review_track(b: &mut Value) {
    push(
        b,
        "subagent_tracks",
        json!({
            "id": "integration-angle",
            "phase": "deep_review",
            "status": "completed",
            "track": "integration-check",
            "review_checkpoint": 4,
            "owner": "dvandva-integration-checker",
            "parallelized": true,
            "rationale": "Integration checker verified cross-chunk wiring across owners.",
            "inputs": ["candidate diff"],
            "outputs": ["Cross-chunk seams wire end to end."],
            "evidence_refs": ["subagent:integration-angle"],
            "result": "passed"
        }),
    );
}

/// `v2_dynamic_agent_instances_filter` — one closed generated instance.
pub fn dynamic_agent_instances(b: &mut Value) {
    b["agent_instances"] = json!([{
        "id": "r3-generated-dynamic-review",
        "parent_role": "vadi",
        "spawned_by": "dvandva-implementer",
        "spawned_at_checkpoint": 0,
        "phase": "research",
        "purpose": "Run-scoped generated agent for dynamic-agent gate coverage.",
        "agent_kind": "generated",
        "seed_agent": "dvandva-implementer",
        "model_class": "sonnet-class|gpt-5.5-high",
        "permission_class": "verify-only",
        "status": "closed",
        "work_item_ids": ["implementation-chunk-1"],
        "read_paths": ["rust/dvandva/src/write.rs"],
        "write_paths": [],
        "depends_on": [],
        "conflict_group": "r3-dynamic-review",
        "base_checkpoint": 0,
        "output_refs": ["subagent_track:r3-generated-dynamic-review"],
        "evidence_refs": ["subagent:r3-generated-dynamic-review", "closed:r3-generated-dynamic-review"],
        "closed_at": "2026-06-28T00:00:00Z",
        "result": "passed"
    }]);
}

/// `v2_dynamic_parallel_track_filter` — point subagent_tracks[0] at the
/// generated instance.
pub fn dynamic_parallel_track(b: &mut Value) {
    b["subagent_tracks"][0]["parallelized"] = json!(true);
    b["subagent_tracks"][0]["owner"] = json!("r3-generated-dynamic-review");
    b["subagent_tracks"][0]["owner_role"] = json!("vadi");
    b["subagent_tracks"][0]["outputs"] = json!(["Generated dynamic review completed."]);
    b["subagent_tracks"][0]["evidence_refs"] = json!([
        "subagent:r3-generated-dynamic-review",
        "closed:r3-generated-dynamic-review"
    ]);
}

/// `v2_many_agent_instances_filter` — one collapsed + five closed instances.
pub fn many_agent_instances(b: &mut Value) {
    let mut arr = vec![json!({
        "id": "r3-gen-0",
        "parent_role": "vadi",
        "spawned_by": "dvandva-implementer",
        "spawned_at_checkpoint": 0,
        "phase": 1,
        "purpose": "Collapsed generated instance for large dynamic registries.",
        "agent_kind": "generated",
        "seed_agent": "dvandva-implementer",
        "model_class": "sonnet-class|gpt-5.5-high",
        "permission_class": "edit-scoped",
        "status": "collapsed",
        "work_item_ids": [],
        "read_paths": ["src/gen-0"],
        "write_paths": [],
        "depends_on": [],
        "conflict_group": "many-0",
        "base_checkpoint": 0,
        "output_refs": [],
        "evidence_refs": [],
        "result": "collapsed"
    })];
    for i in 1..=5 {
        let parent = if i % 2 == 0 { "prativadi" } else { "vadi" };
        arr.push(json!({
            "id": format!("r3-gen-{i}"),
            "parent_role": parent,
            "spawned_by": "dvandva-implementer",
            "spawned_at_checkpoint": 0,
            "phase": 1,
            "purpose": format!("Closed generated instance {i} for large dynamic registry coverage."),
            "agent_kind": "generated",
            "seed_agent": "dvandva-implementer",
            "model_class": "sonnet-class|gpt-5.5-high",
            "permission_class": "edit-scoped",
            "status": "closed",
            "work_item_ids": [format!("chunk-{i}")],
            "read_paths": [format!("src/gen-{i}")],
            "write_paths": [format!("src/gen-{i}")],
            "depends_on": [],
            "conflict_group": format!("many-{i}"),
            "base_checkpoint": 0,
            "output_refs": [format!("subagent_track:r3-gen-{i}")],
            "evidence_refs": [format!("subagent:r3-gen-{i}"), format!("closed:r3-gen-{i}")],
            "closed_at": "2026-06-28T00:00:00Z",
            "result": "passed"
        }));
    }
    b["agent_instances"] = Value::Array(arr);
}

/// `v2_implementation_tracks_filter` — five completed implementation-chunk tracks.
pub fn implementation_tracks(b: &mut Value) {
    for (id, owner_role, chunk) in [
        ("impl-a", "vadi", "a"),
        ("impl-b", "vadi", "b"),
        ("impl-c", "prativadi", "c"),
        ("impl-d", "prativadi", "d"),
        ("impl-e", "vadi", "e"),
    ] {
        push(
            b,
            "subagent_tracks",
            json!({
                "id": id,
                "phase": 1,
                "status": "completed",
                "track": "implementation-chunk",
                "owner": "dvandva-implementer",
                "owner_role": owner_role,
                "parallelized": true,
                "rationale": "Implementation chunk completed in parallel.",
                "inputs": [format!("implementation-chunk-{chunk}")],
                "outputs": [format!("Chunk {chunk} implemented.")],
                "evidence_refs": [format!("subagent:{id}")],
                "result": "passed"
            }),
        );
    }
}

/// `v2_test_creation_track_filter`.
pub fn test_creation_track(b: &mut Value) {
    push(
        b,
        "subagent_tracks",
        json!({
            "id": "test-creation-evidence",
            "phase": "test_creation",
            "status": "completed",
            "track": "test-creation",
            "owner": "dvandva-test-creator",
            "owner_role": "vadi",
            "parallelized": false,
            "rationale": "Vadi test_creation recorded coverage evidence before cross-review.",
            "inputs": ["implementation evidence"],
            "outputs": ["Motivating tests and coverage evidence recorded."],
            "evidence_refs": ["bash scripts/test PASS"],
            "result": "passed"
        }),
    );
}

/// A prativadi-owned adversarial-test track (F8: accepted, not required).
pub fn adversarial_test_track(b: &mut Value) {
    push(
        b,
        "subagent_tracks",
        json!({
            "id": "adversarial-test-evidence",
            "phase": "test_creation",
            "status": "completed",
            "track": "adversarial-test",
            "owner": "dvandva-adversarial-analyst",
            "owner_role": "prativadi",
            "parallelized": true,
            "rationale": "Prativadi authored decorrelated adversarial tests for the phase.",
            "inputs": ["implementation evidence"],
            "outputs": ["Adversarial edge-case tests recorded."],
            "evidence_refs": ["bash scripts/test PASS"],
            "result": "passed"
        }),
    );
}

fn run_explainer_reviews_for(b: &mut Value, artifact: &str) {
    b["run_explainer_reviews"] = json!([
        {
            "id": "run-explainer-review-vadi",
            "role": "vadi",
            "artifact_ref": artifact,
            "status": "completed",
            "result": "approved",
            "summary": "Vadi reviewed the final run explainer.",
            "evidence_refs": ["vadi:run-explainer-review"]
        },
        {
            "id": "run-explainer-review-prativadi",
            "role": "prativadi",
            "artifact_ref": artifact,
            "status": "completed",
            "result": "approved",
            "summary": "Prativadi reviewed the final run explainer.",
            "evidence_refs": ["prativadi:run-explainer-review"]
        }
    ]);
}

/// F10: a completed current-cycle explainer-verification track owned by
/// `dvandva-doc-verifier`, required on a full-profile terminal `done`.
pub fn explainer_verification_track(b: &mut Value) {
    push(
        b,
        "subagent_tracks",
        json!({
            "id": "explainer-verification-evidence",
            "phase": "termination_review",
            "status": "completed",
            "track": "explainer-verification",
            "owner": "dvandva-doc-verifier",
            "review_checkpoint": 4,
            "parallelized": false,
            "rationale": "Doc-verifier checked the run explainer's claims against the code.",
            "inputs": ["run explainer", "final diff"],
            "outputs": ["Explainer claims verified against observable behavior."],
            "evidence_refs": ["subagent:explainer-verification-evidence"],
            "result": "approved"
        }),
    );
}

/// `v2_run_explainer_reviews_filter`.
pub fn run_explainer_reviews(b: &mut Value) {
    run_explainer_reviews_for(
        b,
        "./superpowers/run-reports/2026-06-28-run-a-explainer.html",
    );
}
/// `v2_date_prefixed_run_explainer_reviews_filter`.
pub fn date_prefixed_run_explainer_reviews(b: &mut Value) {
    run_explainer_reviews_for(
        b,
        "./superpowers/run-reports/2026-06-29-baton-accuracy-hook-coexist-explainer.html",
    );
}
/// `v2_double_date_run_explainer_reviews_filter`.
pub fn double_date_run_explainer_reviews(b: &mut Value) {
    run_explainer_reviews_for(
        b,
        "./superpowers/run-reports/2026-06-30-2026-06-29-baton-accuracy-hook-coexist-explainer.html",
    );
}

/// `v2_cross_review_tracks_filter` — two approved cross-review tracks.
pub fn cross_review_tracks(b: &mut Value) {
    push(
        b,
        "subagent_tracks",
        json!({
            "id": "cross-vadi",
            "phase": "cross_review",
            "status": "completed",
            "track": "cross-review",
            "owner": "dvandva-cross-reviewer",
            "owner_role": "vadi",
            "parallelized": true,
            "rationale": "Vadi cross-reviewed prativadi-owned chunks.",
            "inputs": ["implementation-chunk-c", "implementation-chunk-d"],
            "outputs": ["Peer chunks accepted."],
            "evidence_refs": ["subagent:cross-vadi"],
            "review_checkpoint": 4,
            "result": "approved"
        }),
    );
    push(
        b,
        "subagent_tracks",
        json!({
            "id": "cross-prativadi",
            "phase": "cross_review",
            "status": "completed",
            "track": "cross-review",
            "owner": "dvandva-cross-reviewer",
            "owner_role": "prativadi",
            "parallelized": true,
            "rationale": "Prativadi cross-reviewed vadi-owned chunks.",
            "inputs": ["implementation-chunk-a", "implementation-chunk-b", "implementation-chunk-e"],
            "outputs": ["Peer chunks accepted."],
            "evidence_refs": ["subagent:cross-prativadi"],
            "review_checkpoint": 4,
            "result": "approved"
        }),
    );
}

/// `v2_cross_review_finding_filter` — one changes-requested cross-review track.
pub fn cross_review_finding(b: &mut Value) {
    push(
        b,
        "subagent_tracks",
        json!({
            "id": "cross-prativadi-finding",
            "phase": "cross_review",
            "status": "completed",
            "track": "cross-review",
            "owner": "dvandva-cross-reviewer",
            "owner_role": "prativadi",
            "parallelized": true,
            "rationale": "Prativadi found fix-required evidence.",
            "inputs": ["implementation-chunk-a"],
            "outputs": ["changes-requested: vadi-owned chunk needs a fix."],
            "evidence_refs": ["subagent:cross-prativadi-finding"],
            "review_checkpoint": 4,
            "result": "changes-requested"
        }),
    );
}

/// `v2_cross_fixing_chunks_filter` — two cross_fixing chunks.
pub fn cross_fixing_chunks(b: &mut Value) {
    b["work_split"] = json!([
        {
            "id": "cross-fixing-a",
            "phase": "1",
            "chunk_type": "cross_fixing",
            "owner": "vadi",
            "owner_role": "vadi",
            "suggested_agent": "dvandva-implementer",
            "scope": "Vadi-owned cross-fixing chunk A.",
            "paths": ["src/fix/a.ts"],
            "can_parallelize": true,
            "parallel_rationale": "Independent fix slice.",
            "depends_on": [],
            "status": "planned",
            "artifact_refs": []
        },
        {
            "id": "cross-fixing-b",
            "phase": "1",
            "chunk_type": "cross_fixing",
            "owner": "prativadi",
            "owner_role": "prativadi",
            "suggested_agent": "dvandva-implementer",
            "scope": "Prativadi-owned cross-fixing chunk B.",
            "paths": ["src/fix/b.ts"],
            "can_parallelize": true,
            "parallel_rationale": "Independent fix slice.",
            "depends_on": [],
            "status": "planned",
            "artifact_refs": []
        }
    ]);
}

/// `v2_cross_review_chunks_filter` — two cross_review chunks.
pub fn cross_review_chunks(b: &mut Value) {
    b["work_split"] = json!([
        {
            "id": "cross-review-a",
            "phase": "1",
            "chunk_type": "cross_review",
            "owner": "vadi",
            "owner_role": "vadi",
            "suggested_agent": "dvandva-cross-reviewer",
            "scope": "Vadi cross-reviews prativadi-owned code.",
            "paths": ["src/shared-review.ts"],
            "can_parallelize": true,
            "parallel_rationale": "Cross-review is read-only by default.",
            "depends_on": [],
            "status": "planned",
            "artifact_refs": []
        },
        {
            "id": "cross-review-b",
            "phase": "1",
            "chunk_type": "cross_review",
            "owner": "prativadi",
            "owner_role": "prativadi",
            "suggested_agent": "dvandva-cross-reviewer",
            "scope": "Prativadi cross-reviews vadi-owned code.",
            "paths": ["src/shared-review.ts"],
            "can_parallelize": true,
            "parallel_rationale": "Cross-review is read-only by default.",
            "depends_on": [],
            "status": "planned",
            "artifact_refs": []
        }
    ]);
}

// ---- profile blobs --------------------------------------------------------

/// `fast_profile_filter`.
pub fn fast_profile(b: &mut Value) {
    b["profile"] = json!("fast");
    b["profile_floor"] = json!("fast");
    b["profile_decision"] = json!({
        "selected_profile": "fast",
        "floor": "fast",
        "reason": "test fast allowlist",
        "decided_by": "test-suite",
        "decided_at": "2026-07-01T00:00:00Z",
        "risk_inputs": ["changed_paths"],
        "hard_triggers": [],
        "allowlist_match": true,
        "allowlist_refs": ["README.md"],
        "evidence_refs": ["test:fast-allowlist"]
    });
    b["profile_history"] = json!([]);
}

/// `standard_profile_filter`.
pub fn standard_profile(b: &mut Value) {
    b["profile"] = json!("standard");
    b["profile_floor"] = json!("standard");
    b["profile_decision"] = json!({
        "selected_profile": "standard",
        "floor": "standard",
        "reason": "test standard profile",
        "decided_by": "test-suite",
        "decided_at": "2026-07-01T00:00:00Z",
        "risk_inputs": [],
        "hard_triggers": [],
        "allowlist_match": false,
        "allowlist_refs": [],
        "evidence_refs": ["test:standard"]
    });
    b["profile_history"] = json!([]);
}

/// `full_low_floor_profile_filter` — hard-risk full with an incorrectly low floor.
pub fn full_low_floor_profile(b: &mut Value) {
    b["profile"] = json!("full");
    b["profile_floor"] = json!("fast");
    b["profile_decision"] = json!({
        "selected_profile": "full",
        "floor": "fast",
        "reason": "test hard-risk path with incorrectly low floor",
        "decided_by": "test-suite",
        "decided_at": "2026-07-01T00:00:00Z",
        "risk_inputs": ["changed_paths"],
        "hard_triggers": ["plugins/dvandva/skills/vadi/SKILL.md"],
        "allowlist_match": false,
        "allowlist_refs": [],
        "evidence_refs": ["test:hard-risk-low-floor"]
    });
    b["profile_history"] = json!([{
        "from": "fast", "to": "full", "floor": "fast", "checkpoint": 5,
        "actor_role": "vadi", "reason": "hard-risk path detected but floor left too low",
        "evidence_refs": ["test:hard-risk-low-floor"]
    }]);
}

/// `fast_allowlist_work_split` — a single README-only fast chunk.
pub fn fast_allowlist_work_split(b: &mut Value) {
    b["work_split"] = json!([{
        "id": "fast-readme-doc",
        "phase": "1",
        "chunk_type": "implementation",
        "owner": "vadi",
        "owner_role": "vadi",
        "scope": "README-only fast allowlist fixture.",
        "paths": ["README.md"],
        "write_paths": ["README.md"],
        "cross_review_by": "prativadi",
        "can_parallelize": false,
        "parallel_rationale": "Single allowlisted prose file.",
        "depends_on": [],
        "status": "planned",
        "artifact_refs": []
    }]);
}

/// `compact_terminal_evidence_filter` — verification + matrix + phase-review evidence.
pub fn compact_terminal_evidence(b: &mut Value) {
    b["verification"] = json!([
        {"command": "bash scripts/test-dvandva-write.sh", "result": "passed", "notes": "compact terminal verification"}
    ]);
    if let Some(matrix) = b["verification_matrix"].as_array_mut() {
        for m in matrix {
            m["current"] = json!("passed");
            m["evidence_refs"] = json!(["command:bash scripts/test-dvandva-write.sh"]);
            // S4-T6: a compact-profile done requires every matrix row to carry a
            // numeric evidence checkpoint at/after the implementation-family
            // freshness anchor. These fixtures have no impl-family history, so the
            // anchor is 0 and any non-negative checkpoint keeps the row fresh.
            m["evidence_checkpoint"] = json!(4);
        }
    }
    push(
        b,
        "subagent_tracks",
        json!({
            "id": "compact-phase-review",
            "phase": "phase_review",
            "track": "phase-review",
            "owner": "prativadi",
            "owner_role": "prativadi",
            "status": "completed",
            "result": "approved",
            "parallelized": false,
            "rationale": "Compact profile independent prativadi review evidence.",
            "review_checkpoint": 4,
            "inputs": ["profile compact implementation"],
            "outputs": ["Prativadi approved compact profile implementation and verification evidence."],
            "evidence_refs": ["test:compact-phase-review"]
        }),
    );
}

/// `old_low_floor_history` — full profile carrying a historical lower-floor entry.
pub fn old_low_floor_history(b: &mut Value) {
    b["profile"] = json!("full");
    b["profile_floor"] = json!("full");
    b["profile_decision"] = json!({
        "selected_profile": "full",
        "floor": "full",
        "reason": "test old low-floor history compatibility",
        "decided_by": "test-suite",
        "decided_at": "2026-07-01T00:00:00Z",
        "risk_inputs": [],
        "hard_triggers": [],
        "allowlist_match": false,
        "allowlist_refs": [],
        "evidence_refs": ["test:old-low-floor-history"]
    });
    b["profile_history"] = json!([{
        "from": "fast", "to": "standard", "floor": "standard", "checkpoint": 3,
        "actor_role": "vadi", "reason": "historical lower floor before later escalation",
        "evidence_refs": ["test:old-low-floor-history"]
    }]);
}

// ---------------------------------------------------------------------------
// S4-T1 / S4-T6 done-gate fixtures (WE2).
// ---------------------------------------------------------------------------

/// Write a single non-empty artifact file at `rel` (repo-root-relative, a
/// leading `./` is stripped) under `baton_dir`. The write test temp dirs are
/// never git repos, so the engine's repo-root resolution (git toplevel of the
/// baton dir, else the baton dir) collapses to `baton_dir` — the same root used
/// here, so `missing_artifact` sees the file the engine resolves.
pub fn write_artifact(baton_dir: &Path, rel: &str) {
    let rel = rel.strip_prefix("./").unwrap_or(rel);
    let p = baton_dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, b"<html>artifact</html>").unwrap();
}

/// S4-T1: create every standard done-gate artifact file used across the
/// success-path done tests (research/run-explainer/plan/review refs), so a
/// `missing_artifact` check resolves each required ref to a non-empty file.
pub fn seed_done_artifacts(baton_dir: &Path) {
    for rel in [
        "superpowers/research/run-a.html",
        "superpowers/run-reports/2026-06-28-run-a-explainer.html",
        "superpowers/run-reports/2026-06-29-baton-accuracy-hook-coexist-explainer.html",
        "superpowers/plans/2026-06-29-run-modes-plan.html",
        "superpowers/reviews/review-run-modes-PR-1.html",
        "superpowers/reviews/review-run-modes.html",
    ] {
        write_artifact(baton_dir, rel);
    }
}

// ---------------------------------------------------------------------------
// S5-T3 chunk-floor waiver fixtures (WE3).
// ---------------------------------------------------------------------------

/// S5-T3: `vadi_n` vadi-owned + `prativadi_n` prativadi-owned two-team
/// implementation chunks (phase "1"). Total = `vadi_n + prativadi_n`, so a caller
/// can build a sub-5 split to exercise the chunk-floor waiver.
pub fn parallel_chunks_split(b: &mut Value, vadi_n: usize, prativadi_n: usize) {
    b["active_roles"] = json!(["vadi", "prativadi"]);
    b["work_split"] = json!([]);
    let mut idx = 0;
    for (role, count) in [("vadi", vadi_n), ("prativadi", prativadi_n)] {
        let reviewer = if role == "vadi" { "prativadi" } else { "vadi" };
        for _ in 0..count {
            push(
                b,
                "work_split",
                json!({
                    "id": format!("impl-chunk-{idx}"),
                    "phase": "1",
                    "chunk_type": "implementation",
                    "owner": role,
                    "owner_role": role,
                    "suggested_agent": "dvandva-implementer",
                    "scope": "Two-team implementation chunk.",
                    "paths": [format!("src/chunk-{idx}.ts")],
                    "cross_review_by": reviewer,
                    "can_parallelize": true,
                    "parallel_rationale": "Independent file.",
                    "depends_on": [],
                    "status": "planned",
                    "artifact_refs": []
                }),
            );
            idx += 1;
        }
    }
}

/// S5-T3: `vadi_n` + `prativadi_n` completed implementation-chunk subagent_tracks
/// (phase 1), for the parallel_implementing->test_creation floor.
pub fn implementation_tracks_split(b: &mut Value, vadi_n: usize, prativadi_n: usize) {
    let mut idx = 0;
    for (role, count) in [("vadi", vadi_n), ("prativadi", prativadi_n)] {
        for _ in 0..count {
            push(
                b,
                "subagent_tracks",
                json!({
                    "id": format!("impl-track-{idx}"),
                    "phase": 1,
                    "status": "completed",
                    "track": "implementation-chunk",
                    "owner": "dvandva-implementer",
                    "owner_role": role,
                    "parallelized": true,
                    "rationale": "Implementation chunk completed in parallel.",
                    "inputs": [format!("implementation-chunk-{idx}")],
                    "outputs": [format!("Chunk {idx} implemented.")],
                    "evidence_refs": [format!("subagent:impl-track-{idx}")],
                    "result": "passed"
                }),
            );
            idx += 1;
        }
    }
}

/// S5-T3: a well-formed `work_split_waiver` — prativadi-approved, nonblank
/// reason, numeric checkpoint.
pub fn work_split_waiver(b: &mut Value) {
    b["work_split_waiver"] = json!({
        "reason": "Two-file phase; a further split would fabricate chunks.",
        "approved_by": "prativadi",
        "checkpoint": 5
    });
}

/// S4-T6: mark every `verification_matrix` row complete (result passed) and
/// fresh (numeric `evidence_checkpoint` = the candidate's checkpoint, which is
/// at/after the freshness anchor in these no-impl-history fixtures) so a
/// full-profile `done` clears the `stale_verification_matrix` gate.
pub fn done_matrix_fresh(b: &mut Value) {
    let cp = b["checkpoint"].clone();
    if let Some(rows) = b["verification_matrix"].as_array_mut() {
        for r in rows {
            r["result"] = json!("passed");
            r["current"] = json!("passed");
            r["evidence_refs"] = json!(["command:bash scripts/test PASS"]);
            r["evidence_checkpoint"] = cp.clone();
        }
    }
}
