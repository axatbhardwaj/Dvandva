//! Integration tests for `dvandva lint skills` — ported case-by-case from
//! `scripts/test-lint-skills.sh`, plus coverage of the usage/frontmatter/
//! length branches the shell suite exercises only implicitly via the
//! behavioral contract.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

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

/// S5-T2: the engine's `dvandva.baton.v2` required-key list, mirrored here as the
/// self-contained fixture reference (the lint compares an inline block against
/// `crate::write::v2_required_keys`, which is pub(crate) and not reachable from an
/// integration test). Keep in sync with `write::required_keys`.
const V2_REQUIRED_KEYS: &[&str] = &[
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
];

/// A self-contained v2 inline contract block carrying exactly the v2 required
/// keys (schema = dvandva.baton.v2). Fixtures build from this so the lint tests
/// do not depend on any bundled schema file.
fn v2_inline_block() -> Value {
    let mut obj = serde_json::Map::new();
    for key in V2_REQUIRED_KEYS {
        let value = if *key == "schema" {
            Value::String("dvandva.baton.v2".to_string())
        } else {
            Value::Null
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
// S5-T2 (D5): the docs wave (ordered AFTER the WE3 engine wave) swapped the
// vadi/prativadi SKILL.md inline contract blocks to v2, so these two real-tree
// assertions run as active guards. The v2 accept path is also covered
// self-contained below.

#[test]
fn vadi_role_skill_passes_full_lint() {
    assert_exit(&real_skill("plugins/dvandva/skills/vadi/SKILL.md"), 0);
}

#[test]
fn prativadi_role_skill_passes_full_lint() {
    assert_exit(&real_skill("plugins/dvandva/skills/prativadi/SKILL.md"), 0);
}

#[test]
fn vadi_role_skill_with_v2_block_passes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("vadi-v2.md");
    std::fs::write(
        &file,
        role_skill_with_block(
            "vadi",
            "Use when testing that a well-formed v2 inline block passes the role lint.",
            &v2_inline_block(),
        ),
    )
    .unwrap();
    assert_exit(file.to_str().unwrap(), 0);
}

#[test]
fn prativadi_role_skill_with_v2_block_passes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prativadi-v2.md");
    std::fs::write(
        &file,
        role_skill_with_block(
            "prativadi",
            "Use when testing that a well-formed v2 inline block passes the role lint.",
            &v2_inline_block(),
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
fn role_inline_v2_schema_rejects_unexpected_key() {
    // S5-T2: CONVERTED from `role_inline_v1_schema_rejects_v2_only_key`. The
    // exact-key check is now against the v2 required-key list, so any extra
    // top-level key is "unexpected".
    let dir = tempfile::tempdir().unwrap();
    let mut block = v2_inline_block();
    block.as_object_mut().unwrap().insert(
        "not_a_baton_key".to_string(),
        Value::String("extra".to_string()),
    );
    let file = dir.path().join("role-with-unexpected-key.md");
    std::fs::write(
        &file,
        role_skill_with_block(
            "vadi",
            "Use when testing rejection of an unexpected top-level key in the v2 inline block.",
            &block,
        ),
    )
    .unwrap();
    assert_exit_contains(file.to_str().unwrap(), 1, "unexpected key");
}

#[test]
fn role_inline_v1_schema_now_rejected() {
    // S5-T2: an inline block still carrying schema=dvandva.baton.v1 is rejected —
    // the lint now requires v2.
    let dir = tempfile::tempdir().unwrap();
    let mut block = v2_inline_block();
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
    assert_exit_contains(file.to_str().unwrap(), 1, "schema=dvandva.baton.v2");
}

#[test]
fn role_inline_v2_schema_missing_required_key_rejected() {
    // S5-T2: dropping a required key from the v2 block is a missing-key failure.
    let dir = tempfile::tempdir().unwrap();
    let mut block = v2_inline_block();
    block.as_object_mut().unwrap().remove("run_id");
    let file = dir.path().join("role-with-missing-key.md");
    std::fs::write(
        &file,
        role_skill_with_block(
            "vadi",
            "Use when testing rejection of a v2 inline block missing a required key.",
            &block,
        ),
    )
    .unwrap();
    assert_exit_contains(file.to_str().unwrap(), 1, "missing required key 'run_id'");
}

#[test]
fn role_skill_rejects_out_of_band_final_approval_text() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("role-with-out-of-band-approval.md");
    let content = format!(
        "---\nname: prativadi\ndescription: Use when testing rejection of stale out-of-band final approval text.\n---\n\n# Test Role\n\n- If `<current N> == total_phases`, set `prativadi_final_approval: true`; the vadi must review later.\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&v2_inline_block()).unwrap()
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
