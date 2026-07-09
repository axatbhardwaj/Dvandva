//! Integration tests for `dvandva lint skills` — ported case-by-case from
//! `scripts/test-lint-skills.sh`, plus coverage of the usage/frontmatter/
//! length branches the shell suite exercises only implicitly via the
//! behavioral contract.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

fn dvandva() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva"))
}

fn run_lint(path: &str) -> std::process::Output {
    dvandva()
        .arg("lint")
        .arg("skills")
        .arg(path)
        .output()
        .expect("failed to run dvandva lint skills")
}

fn assert_exit(path: &str, expected: i32) -> std::process::Output {
    let out = run_lint(path);
    assert_eq!(
        out.status.code(),
        Some(expected),
        "path: {path}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn assert_exit_contains(path: &str, expected: i32, needle: &str) {
    let out = assert_exit(path, expected);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains(needle),
        "expected output to contain {needle:?}, got: {combined}"
    );
}

/// Repo root, derived at compile time from this crate's manifest directory
/// (`rust/dvandva`), two levels up.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should resolve")
}

fn real_skill(rel: &str) -> String {
    repo_root().join(rel).to_string_lossy().into_owned()
}

/// The engine's `dvandva.baton.v3` required-key list, mirrored here as the
/// self-contained fixture reference (the lint compares an inline block against
/// the crate's `v2_required_keys()` plus required `run_workflow`, which is
/// pub(crate) and not reachable from an integration test). Keep in sync with
/// that live write-path shape.
const V3_REQUIRED_KEYS: &[&str] = &[
    "schema",
    "updated_at",
    "mode",
    "run_mode",
    "phase",
    "total_phases",
    "status",
    "assignee",
    "current_engine",
    "review_target",
    "plan_ref",
    "master_plan_locked",
    "question",
    "resume_assignee",
    "resume_status",
    "disagreement_round",
    "disagreement_cap",
    "turn_cap",
    "branch",
    "checkpoint",
    "allow_commit",
    "allow_push",
    "allow_pr",
    "vadi_final_approval",
    "prativadi_final_approval",
    "final_commit",
    "pushed_ref",
    "summary",
    "changed_paths",
    "verification",
    "findings",
    "narrow_fixups",
    "vadi_counter",
    "deferred",
    "blockers",
    "next_action",
    "run_id",
    "original_ask",
    "research_ref",
    "run_explainer_ref",
    "active_roles",
    "agent_instances",
    "work_split",
    "subagent_tracks",
    "verification_matrix",
    "run_workflow",
];

/// A self-contained v3 inline contract block carrying exactly the v3 required
/// keys (schema = dvandva.baton.v3). Fixtures build from this so the lint tests
/// do not depend on any bundled schema file.
fn v3_inline_block() -> Value {
    let mut obj = serde_json::Map::new();
    for key in V3_REQUIRED_KEYS {
        let value = match *key {
            "schema" => Value::String("dvandva.baton.v3".to_string()),
            "run_workflow" => serde_json::json!({
                "source": "preset:standard",
                "declared_by": "vadi",
                "declared_at_checkpoint": 0,
                "approved_by": null,
                "approved_at_checkpoint": null,
                "revision_round": 0,
                "states": [],
                "edges": [],
                "amendments": []
            }),
            _ => Value::Null,
        };
        obj.insert((*key).to_string(), value);
    }
    Value::Object(obj)
}

fn role_skill_with_block(name: &str, description: &str, block: &Value) -> String {
    format!(
        "---\nname: {name}\ndescription: {description}\n---\n\n# Test Role\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(block).unwrap()
    )
}

// --- real repo skill files ---
//
// The docs wave swapped the vadi/prativadi SKILL.md inline contract blocks to
// the live v3 seed, so these two real-tree assertions run as active guards.
// The v3 accept path is also covered self-contained below.

#[test]
fn vadi_role_skill_passes_full_lint() {
    assert_exit(&real_skill("plugins/dvandva/skills/vadi/SKILL.md"), 0);
}

#[test]
fn prativadi_role_skill_passes_full_lint() {
    assert_exit(&real_skill("plugins/dvandva/skills/prativadi/SKILL.md"), 0);
}

#[test]
fn vadi_role_skill_with_v3_block_passes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("vadi-v3.md");
    std::fs::write(
        &file,
        role_skill_with_block(
            "vadi",
            "Use when testing that a well-formed v3 inline block passes the role lint.",
            &v3_inline_block(),
        ),
    )
    .unwrap();
    assert_exit(file.to_str().unwrap(), 0);
}

#[test]
fn prativadi_role_skill_with_v3_block_passes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prativadi-v3.md");
    std::fs::write(
        &file,
        role_skill_with_block(
            "prativadi",
            "Use when testing that a well-formed v3 inline block passes the role lint.",
            &v3_inline_block(),
        ),
    )
    .unwrap();
    assert_exit(file.to_str().unwrap(), 0);
}

#[test]
fn non_role_research_skill_passes_without_embedded_schema() {
    assert_exit(&real_skill("plugins/dvandva/skills/research/SKILL.md"), 0);
}

#[test]
fn non_role_testing_skill_passes_without_embedded_schema() {
    assert_exit(&real_skill("plugins/dvandva/skills/testing/SKILL.md"), 0);
}

// --- documented seed round-trips through the live write gate (p4-e2e-writable-seed) ---
//
// `vadi_role_skill_passes_full_lint` / `prativadi_role_skill_passes_full_lint`
// above prove the inline block is a *shape-valid* v3 contract; they do not
// prove the write path actually accepts it. Pre-migration, the documented
// seed carried `schema: "dvandva.baton.v2"` and any write attempt was
// rejected with `schema_retired` (see
// `.dvandva/runs/prod-readiness/notes/p4-red-evidence.md`) — a blocked-HIGH
// where the installed docs could not scaffold a run. This test closes that
// gap: it extracts the CURRENT inline seed straight out of the real
// SKILL.md, fills only the blanks the surrounding prose (vadi/SKILL.md line
// 24) documents as vadi-supplied at scaffold time, and pushes the result
// through the real `dvandva write` binary. A future regression that
// reintroduces a stale/rejected schema, or drifts the required-key contract
// out of sync with the write path, fails this test — not just the lint.

/// Mirror of the engine's `extract_fenced_json_block` (in
/// `dvandva/src/lint/skills.rs`, `pub(crate)` and therefore unreachable from
/// an integration test): collects the lines inside the ` ```json ` fence
/// found after the frontmatter's second `---`. Kept in lockstep with that
/// scanner so this test extracts the SAME text the live lint validates.
fn extract_json_fence(content: &str) -> String {
    let mut dashes = 0u32;
    let mut inside = false;
    let mut collected: Vec<&str> = Vec::new();
    for line in content.lines() {
        if line == "---" {
            dashes += 1;
            continue;
        }
        if dashes >= 2 && line == "```json" {
            inside = true;
            continue;
        }
        if dashes >= 2 && line == "```" {
            inside = false;
        }
        if inside {
            collected.push(line);
        }
    }
    collected.join("\n")
}

/// Fill the scaffold blanks the surrounding SKILL.md prose documents as
/// vadi-supplied (non-empty safe `run_id`, non-empty `original_ask`,
/// populated default `work_split`/`subagent_tracks`/`verification_matrix`, current
/// `updated_at`), plus the additive development `profile` block the prose
/// requires orthogonally to the required-key JSON fence (`profile`,
/// `profile_floor`, `profile_decision`, `profile_history` are NOT in the v3
/// required-key contract the lint checks, so the fence omits them, but
/// `fresh_scaffold_profile_present` in `write.rs` requires all four on a
/// fresh development-mode scaffold). The documented seed carries a minimal
/// `subagent_tracks` startup entry; keep a fallback here so older fixture
/// copies fail in the same live gate path rather than panicking. Every other
/// field in the extracted seed is left untouched.
fn fill_documented_scaffold_blanks(seed: &mut Value, run_id: &str) {
    seed["run_id"] = json!(run_id);
    seed["original_ask"] = json!(
        "lint_skills round-trip: prove the documented v3 seed is writable through the live gate."
    );
    seed["updated_at"] = json!("2026-07-09T00:00:00Z");
    seed["work_split"] = json!([{
        "id": "research-codebase",
        "phase": "research",
        "owner": "vadi",
        "scope": "Map relevant files, scripts, tests, and conventions before spec drafting.",
        "paths": [],
        "can_parallelize": true,
        "parallel_rationale": "Codebase exploration can run beside docs research and risk review when subagent tooling is available.",
        "depends_on": [],
        "status": "planned",
        "artifact_refs": []
    }]);
    seed["verification_matrix"] = json!([{
        "id": "verify-research-coverage",
        "phase": "research",
        "owner": "prativadi",
        "covers": ["original_ask", "research_ref", "work_split"],
        "command": null,
        "expected": "Independent research review confirms the artifact is source-backed and sufficient for spec drafting.",
        "result": "pending",
        "evidence_ref": null
    }]);
    if seed["subagent_tracks"]
        .as_array()
        .map(|items| items.is_empty())
        .unwrap_or(true)
    {
        seed["subagent_tracks"] = json!([{
            "id": "startup-controller",
            "phase": "research",
            "status": "planned",
            "track": "controller",
            "owner": "vadi",
            "parallelized": false,
            "rationale": "Initial run scaffold; record concrete subagent tracks as each phase begins.",
            "inputs": [],
            "outputs": [],
            "evidence_refs": [],
            "result": "pending"
        }]);
    }
    seed["profile"] = json!("standard");
    seed["profile_floor"] = json!("standard");
    seed["profile_decision"] = json!({
        "selected_profile": "standard",
        "floor": "standard",
        "reason": "lint_skills round-trip: default new development scaffold, no hard-risk paths declared.",
        "decided_by": "vadi",
        "decided_at": "2026-07-09T00:00:00Z",
        "risk_inputs": [],
        "hard_triggers": [],
        "allowlist_match": false,
        "allowlist_refs": [],
        "evidence_refs": ["lint-skills-round-trip"]
    });
    seed["profile_history"] = json!([]);
}

/// Extract the documented seed from `skill_rel`, fill its scaffold blanks,
/// and write it through the real `dvandva write` binary in a fresh named-run
/// tempdir. Asserts the write succeeds and installs a history snapshot.
fn assert_documented_seed_writes_through_gate(skill_rel: &str, run_id: &str) {
    let content = std::fs::read_to_string(repo_root().join(skill_rel))
        .unwrap_or_else(|e| panic!("{skill_rel} should be readable: {e}"));
    let fence = extract_json_fence(&content);
    let mut seed: Value = serde_json::from_str(&fence)
        .unwrap_or_else(|e| panic!("documented seed in {skill_rel} must parse as JSON: {e}"));
    fill_documented_scaffold_blanks(&mut seed, run_id);

    let dir = tempfile::tempdir().unwrap();
    let baton = dir
        .path()
        .join(".dvandva/runs")
        .join(run_id)
        .join("baton.json");
    let candidate = dir.path().join("seed-candidate.json");
    std::fs::write(&candidate, serde_json::to_string_pretty(&seed).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("write")
        .arg(&baton)
        .arg(&candidate)
        .env("DVANDVA_ROLE", "vadi")
        .env_remove("DVANDVA_LOCK_TIMEOUT")
        .env_remove("DVANDVA_WRITE_BARRIER")
        .env_remove("DVANDVA_WRITE_BARRIER_POSTFENCE")
        .output()
        .expect("failed to run dvandva write");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "documented seed from {skill_rel} should write cleanly through the live gate\noutput: {combined}"
    );
    assert!(
        combined.contains("DVANDVA_WRITE ok"),
        "expected 'DVANDVA_WRITE ok', got: {combined}"
    );
    assert!(baton.is_file(), "write should install {}", baton.display());
    let history = baton
        .parent()
        .unwrap()
        .join("history/0-clarifying_questions_drafting-vadi.json");
    assert!(
        history.is_file(),
        "write should snapshot history to {}",
        history.display()
    );
}

#[test]
fn vadi_documented_v3_seed_writes_through_live_gate() {
    assert_documented_seed_writes_through_gate(
        "plugins/dvandva/skills/vadi/SKILL.md",
        "e2e-vadi-seed",
    );
}

#[test]
fn prativadi_documented_v3_seed_writes_through_live_gate() {
    assert_documented_seed_writes_through_gate(
        "plugins/dvandva/skills/prativadi/SKILL.md",
        "e2e-prativadi-seed",
    );
}

// --- synthetic fixtures ---

#[test]
fn role_skill_without_embedded_schema_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("role-without-schema.md");
    std::fs::write(
        &file,
        "---\nname: vadi\ndescription: Use when testing the role-skill schema gate.\n---\n\n# Test Role\n\nThis role skill intentionally omits the baton schema.\n",
    )
    .unwrap();
    assert_exit(file.to_str().unwrap(), 1);
}

#[test]
fn role_inline_v3_schema_rejects_unexpected_key() {
    // The exact-key check is against the v3 required-key list, so any extra
    // top-level key is "unexpected".
    let dir = tempfile::tempdir().unwrap();
    let mut block = v3_inline_block();
    block.as_object_mut().unwrap().insert(
        "not_a_baton_key".to_string(),
        Value::String("extra".to_string()),
    );
    let file = dir.path().join("role-with-unexpected-key.md");
    std::fs::write(
        &file,
        role_skill_with_block(
            "vadi",
            "Use when testing rejection of an unexpected top-level key in the v3 inline block.",
            &block,
        ),
    )
    .unwrap();
    assert_exit_contains(file.to_str().unwrap(), 1, "unexpected key");
}

#[test]
fn role_inline_v1_schema_now_rejected() {
    // An inline block still carrying schema=dvandva.baton.v1 is rejected —
    // the lint now requires v3.
    let dir = tempfile::tempdir().unwrap();
    let mut block = v3_inline_block();
    block.as_object_mut().unwrap().insert(
        "schema".to_string(),
        Value::String("dvandva.baton.v1".to_string()),
    );
    let file = dir.path().join("role-with-v1-schema.md");
    std::fs::write(
        &file,
        role_skill_with_block(
            "prativadi",
            "Use when testing rejection of a retired v1 inline schema in a role skill.",
            &block,
        ),
    )
    .unwrap();
    assert_exit_contains(file.to_str().unwrap(), 1, "schema=dvandva.baton.v3");
}

#[test]
fn role_inline_v2_schema_now_rejected() {
    // A v2 inline block is stale: the write path rejects v2 candidates, and
    // the skill lint must forbid documenting a seed that cannot be written.
    let dir = tempfile::tempdir().unwrap();
    let mut block = v3_inline_block();
    block.as_object_mut().unwrap().insert(
        "schema".to_string(),
        Value::String("dvandva.baton.v2".to_string()),
    );
    let file = dir.path().join("role-with-v2-schema.md");
    std::fs::write(
        &file,
        role_skill_with_block(
            "prativadi",
            "Use when testing rejection of a retired v2 inline schema in a role skill.",
            &block,
        ),
    )
    .unwrap();
    assert_exit_contains(file.to_str().unwrap(), 1, "schema=dvandva.baton.v3");
}

#[test]
fn role_inline_v3_schema_missing_required_key_rejected() {
    // Dropping a required key from the v3 block is a missing-key failure.
    let dir = tempfile::tempdir().unwrap();
    let mut block = v3_inline_block();
    block.as_object_mut().unwrap().remove("run_workflow");
    let file = dir.path().join("role-with-missing-key.md");
    std::fs::write(
        &file,
        role_skill_with_block(
            "vadi",
            "Use when testing rejection of a v3 inline block missing a required key.",
            &block,
        ),
    )
    .unwrap();
    assert_exit_contains(
        file.to_str().unwrap(),
        1,
        "missing required key 'run_workflow'",
    );
}

#[test]
fn role_skill_rejects_out_of_band_final_approval_text() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("role-with-out-of-band-approval.md");
    let content = format!(
        "---\nname: prativadi\ndescription: Use when testing rejection of stale out-of-band final approval text.\n---\n\n# Test Role\n\n- If `<current N> == total_phases`, set `prativadi_final_approval: true`; the vadi must review later.\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&v3_inline_block()).unwrap()
    );
    std::fs::write(&file, content).unwrap();
    assert_exit_contains(file.to_str().unwrap(), 1, "out-of-band final approval");
}

#[test]
fn non_role_invalid_frontmatter_still_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("non-role-invalid-frontmatter.md");
    std::fs::write(
        &file,
        "---\nname: helper\n---\n\n# Invalid Helper\n\nMissing description.\n",
    )
    .unwrap();
    assert_exit(file.to_str().unwrap(), 1);
}

// --- contract branches not exercised by the shell meta-test suite ---

#[test]
fn usage_error_on_wrong_arg_count() {
    let out = dvandva()
        .arg("lint")
        .arg("skills")
        .output()
        .expect("failed to run dvandva lint skills");
    assert_eq!(out.status.code(), Some(2));

    let out_two_args = dvandva()
        .arg("lint")
        .arg("skills")
        .arg("a")
        .arg("b")
        .output()
        .expect("failed to run dvandva lint skills");
    assert_eq!(out_two_args.status.code(), Some(2));
}

#[test]
fn missing_file_fails_with_exit_1() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.md");
    assert_exit(missing.to_str().unwrap(), 1);
}

#[test]
fn unclosed_frontmatter_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("unclosed.md");
    std::fs::write(&file, "---\nname: helper\ndescription: no closing marker\n").unwrap();
    assert_exit(file.to_str().unwrap(), 1);
}

#[test]
fn description_over_length_cap_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long-description.md");
    let long_desc = "x".repeat(1537);
    std::fs::write(
        &file,
        format!("---\nname: helper\ndescription: {long_desc}\n---\n\nBody.\n"),
    )
    .unwrap();
    assert_exit(file.to_str().unwrap(), 1);
}

#[test]
fn body_over_line_cap_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("long-body.md");
    let body = "line\n".repeat(501);
    std::fs::write(
        &file,
        format!(
            "---\nname: helper\ndescription: Use when testing the body length cap.\n---\n\n{body}"
        ),
    )
    .unwrap();
    assert_exit(file.to_str().unwrap(), 1);
}
