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

fn baton_schema_json() -> Value {
    let bytes = std::fs::read(repo_root().join("plugins/dvandva/references/baton-schema.json"))
        .expect("baton-schema.json should exist");
    serde_json::from_slice(&bytes).expect("baton-schema.json should parse")
}

// --- real repo skill files ---

#[test]
fn vadi_role_skill_passes_full_lint() {
    assert_exit(&real_skill("plugins/dvandva/skills/vadi/SKILL.md"), 0);
}

#[test]
fn prativadi_role_skill_passes_full_lint() {
    assert_exit(&real_skill("plugins/dvandva/skills/prativadi/SKILL.md"), 0);
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
fn role_inline_v1_schema_rejects_v2_only_key() {
    let dir = tempfile::tempdir().unwrap();
    let mut schema = baton_schema_json();
    schema
        .as_object_mut()
        .unwrap()
        .insert("run_id".to_string(), Value::String("v2-only".to_string()));

    let file = dir.path().join("role-with-v2-only-key.md");
    let content = format!(
        "---\nname: vadi\ndescription: Use when testing rejection of v2-only keys in inline v1 baton schema.\n---\n\n# Test Role\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&schema).unwrap()
    );
    std::fs::write(&file, content).unwrap();
    assert_exit_contains(file.to_str().unwrap(), 1, "unexpected key");
}

#[test]
fn role_skill_rejects_out_of_band_final_approval_text() {
    let dir = tempfile::tempdir().unwrap();
    let schema = baton_schema_json();
    let file = dir.path().join("role-with-out-of-band-approval.md");
    let content = format!(
        "---\nname: prativadi\ndescription: Use when testing rejection of stale out-of-band final approval text.\n---\n\n# Test Role\n\n- If `<current N> == total_phases`, set `prativadi_final_approval: true`; the vadi must review later.\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&schema).unwrap()
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
