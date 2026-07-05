//! `dvandva baton-guard` — Claude Code PreToolUse hook that blocks direct
//! edits to the Dvandva baton and its history (design §F4).
//!
//! Spawns the compiled binary (`CARGO_BIN_EXE_dvandva`) and pipes each test's
//! hook payload to its stdin, mirroring how Claude Code itself invokes a
//! command-type hook.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// Spawn `dvandva baton-guard` in a fresh, non-git-repo temp directory (so
/// the SLA lookup fails open — "can't tell" — and the direct-edit-guard
/// tests below stay deterministic regardless of this repo's own real
/// baton), write `stdin_bytes` to its stdin, and return the completed
/// `Output`.
fn run_guard(stdin_bytes: &[u8]) -> Output {
    let dir = tempfile::tempdir().expect("tempdir");
    run_guard_in(dir.path(), &[], stdin_bytes)
}

/// Spawn `dvandva baton-guard` with `cwd` as its working directory and
/// `envs` applied, write `stdin_bytes` to its stdin, and return the
/// completed `Output`.
fn run_guard_in(cwd: &Path, envs: &[(&str, &str)], stdin_bytes: &[u8]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dvandva"));
    cmd.arg("baton-guard")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().expect("failed to spawn dvandva baton-guard");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(stdin_bytes)
        .expect("failed to write stdin to dvandva baton-guard");
    child
        .wait_with_output()
        .expect("failed to wait on dvandva baton-guard")
}

/// Initialize an empty git repo at `dir` (so `git rev-parse --show-toplevel`
/// resolves and the SLA lookup can actually run against it).
fn init_git_repo(dir: &Path) {
    let ok = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .status()
        .expect("git init")
        .success();
    assert!(ok, "git init failed for {dir:?}");
}

/// Write the per-role SLA-pending marker at `dir`/.dvandva/.session-baton-pending.<role>
/// with a stored timestamp `age_secs` in the past, so the marker reads as
/// already aged without any real sleep.
fn write_aged_marker(dir: &Path, role: &str, age_secs: u64) {
    let marker_dir = dir.join(".dvandva");
    std::fs::create_dir_all(&marker_dir).expect("create .dvandva");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_secs();
    std::fs::write(
        marker_dir.join(format!(".session-baton-pending.{role}")),
        (now.saturating_sub(age_secs)).to_string(),
    )
    .expect("write marker");
}

/// A minimal PreToolUse payload for a file-path-taking tool.
fn payload(tool_name: &str, file_path: &str) -> String {
    json!({
        "tool_name": tool_name,
        "tool_input": { "file_path": file_path }
    })
    .to_string()
}

fn assert_blocked(out: &Output) {
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit 2, stdout: {}, stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn assert_allowed(out: &Output) {
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0, stdout: {}, stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stdout.is_empty() && out.stderr.is_empty(),
        "expected no output on allow, stdout: {}, stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---------------------------------------------------------------------------
// Block cases
// ---------------------------------------------------------------------------

#[test]
fn blocks_edit_on_baton_json_under_dot_dvandva() {
    let out = run_guard(payload("Edit", "/repo/.dvandva/baton.json").as_bytes());
    assert_blocked(&out);
}

#[test]
fn blocks_write_on_baton_json_in_dvandva_runs() {
    let out = run_guard(payload("Write", ".dvandva/runs/x/baton.json").as_bytes());
    assert_blocked(&out);
}

#[test]
fn blocks_edit_on_history_file() {
    let out = run_guard(payload("Edit", ".dvandva/runs/x/history/3-foo-vadi.json").as_bytes());
    assert_blocked(&out);
}

#[test]
fn blocks_relative_baton_json_path() {
    let out = run_guard(payload("Edit", ".dvandva/baton.json").as_bytes());
    assert_blocked(&out);
}

#[test]
fn blocks_multiedit_on_baton_json() {
    let out = run_guard(payload("MultiEdit", "/repo/.dvandva/baton.json").as_bytes());
    assert_blocked(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dvandva next"),
        "stderr should name `dvandva next`, got: {stderr}"
    );
    assert!(
        stderr.contains("dvandva write"),
        "stderr should name `dvandva write`, got: {stderr}"
    );
}

#[test]
fn blocks_notebookedit_via_notebook_path() {
    let out = run_guard(
        json!({
            "tool_name": "NotebookEdit",
            "tool_input": { "notebook_path": "/repo/.dvandva/baton.json" }
        })
        .to_string()
        .as_bytes(),
    );
    assert_blocked(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dvandva next"),
        "stderr should name `dvandva next`, got: {stderr}"
    );
    assert!(
        stderr.contains("dvandva write"),
        "stderr should name `dvandva write`, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Allow cases
// ---------------------------------------------------------------------------

#[test]
fn allows_baton_next_json_candidate() {
    let out = run_guard(payload("Write", ".dvandva/baton.next.json").as_bytes());
    assert_allowed(&out);
}

#[test]
fn allows_baton_json_outside_dot_dvandva() {
    let out = run_guard(payload("Edit", "/repo/other/baton.json").as_bytes());
    assert_allowed(&out);
}

#[test]
fn allows_read_tool_on_baton() {
    let out = run_guard(payload("Read", "/repo/.dvandva/baton.json").as_bytes());
    assert_allowed(&out);
}

#[test]
fn allows_bash_tool_on_baton() {
    let out = run_guard(
        json!({
            "tool_name": "Bash",
            "tool_input": { "command": "cat /repo/.dvandva/baton.json" }
        })
        .to_string()
        .as_bytes(),
    );
    assert_allowed(&out);
}

// ---------------------------------------------------------------------------
// Fail-open cases
// ---------------------------------------------------------------------------

#[test]
fn fail_open_on_empty_stdin() {
    let out = run_guard(b"");
    assert_allowed(&out);
}

#[test]
fn fail_open_on_garbage_stdin() {
    let out = run_guard(b"not json{{{");
    assert_allowed(&out);
}

#[test]
fn fail_open_on_missing_tool_input() {
    let out = run_guard(json!({ "tool_name": "Edit" }).to_string().as_bytes());
    assert_allowed(&out);
}

// ---------------------------------------------------------------------------
// Block message contract
// ---------------------------------------------------------------------------

#[test]
fn block_message_names_next_and_write() {
    let out = run_guard(payload("Edit", "/repo/.dvandva/baton.json").as_bytes());
    assert_blocked(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dvandva next"),
        "stderr should name `dvandva next`, got: {stderr}"
    );
    assert!(
        stderr.contains("dvandva write"),
        "stderr should name `dvandva write`, got: {stderr}"
    );
}

#[test]
fn block_message_documents_candidate_and_human_resume() {
    // Fix 2a: the block guidance is complete — it names the candidate file to edit
    // (baton.next.json, never baton.json) and the human_question/human_decision
    // resume path that requires hand-authoring that candidate.
    let out = run_guard(payload("Edit", "/repo/.dvandva/baton.json").as_bytes());
    assert_blocked(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("baton.next.json"),
        "stderr should name the candidate file to edit, got: {stderr}"
    );
    assert!(
        stderr.contains("human_question") || stderr.contains("human_decision"),
        "stderr should document the human resume path, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Plugin hook wiring
// ---------------------------------------------------------------------------

/// Repo root, derived at compile time from this crate's manifest directory
/// (`rust/dvandva`), two levels up.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should resolve")
}

#[test]
fn hooks_json_registers_pretooluse_matcher_and_command() {
    let bytes = std::fs::read(repo_root().join("plugins/dvandva/hooks/hooks.json"))
        .expect("plugins/dvandva/hooks/hooks.json should exist");
    let value: Value = serde_json::from_slice(&bytes)
        .expect("plugins/dvandva/hooks/hooks.json should parse as JSON");

    // Fix 2c: the top-level object carries no non-schema `description` key.
    assert!(
        value.get("description").is_none(),
        "hooks.json should not carry a non-schema top-level description key, got: {value}"
    );

    let pre_tool_use = value["hooks"]["PreToolUse"]
        .as_array()
        .expect("hooks.PreToolUse should be an array");
    assert!(
        !pre_tool_use.is_empty(),
        "hooks.PreToolUse should have at least one matcher entry"
    );

    let entry = &pre_tool_use[0];
    let matcher = entry["matcher"]
        .as_str()
        .expect("matcher should be a string");
    assert_eq!(
        matcher, "*",
        "matcher should widen to all tools for the baton-creation SLA"
    );

    let hook_handlers = entry["hooks"]
        .as_array()
        .expect("matcher entry should have a hooks array");
    assert!(
        hook_handlers
            .iter()
            .any(|h| h["type"] == "command" && h["command"] == "dvandva baton-guard"),
        "expected a command-type hook running `dvandva baton-guard`, got: {hook_handlers:?}"
    );
}

// ---------------------------------------------------------------------------
// Baton-creation SLA (real I/O: marker file + active-run resolution)
// ---------------------------------------------------------------------------

#[test]
fn sla_blocks_a_tool_call_once_marker_ages_past_threshold_with_no_baton() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_blocked(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dvandva resolve") || stderr.contains("dvandva write"),
        "SLA block message should name the way out, got: {stderr}"
    );
}

#[test]
fn sla_allows_dvandva_write_command_even_past_threshold() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let out = run_guard_in(
        dir.path(),
        &[],
        json!({
            "tool_name": "Bash",
            "tool_input": { "command": "dvandva write --candidate x" }
        })
        .to_string()
        .as_bytes(),
    );
    assert_allowed(&out);
}

#[test]
fn sla_marker_is_removed_once_a_real_baton_exists() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let run_dir = dir.path().join(".dvandva/runs/r1");
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    std::fs::write(
        run_dir.join("baton.json"),
        json!({
            "run_id": "r1",
            "status": "research_drafting",
            "assignee": "vadi",
            "updated_at": "2026-01-01T00:00:00Z"
        })
        .to_string(),
    )
    .expect("write baton.json");

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_allowed(&out);
    assert!(
        !dir.path()
            .join(".dvandva/.session-baton-pending.vadi")
            .exists(),
        "marker should be removed once a real baton is resolvable"
    );
}

#[test]
fn sla_not_yet_breached_below_threshold_allows() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 5);

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_BATON_SLA_SECONDS", "120")],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_allowed(&out);
}
