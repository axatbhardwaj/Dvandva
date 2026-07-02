//! `dvandva baton-guard` — Claude Code PreToolUse hook that blocks direct
//! edits to the Dvandva baton and its history (design §F4).
//!
//! Spawns the compiled binary (`CARGO_BIN_EXE_dvandva`) and pipes each test's
//! hook payload to its stdin, mirroring how Claude Code itself invokes a
//! command-type hook.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};

/// Spawn `dvandva baton-guard`, write `stdin_bytes` to its stdin, and return
/// the completed `Output`.
fn run_guard(stdin_bytes: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("baton-guard")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn dvandva baton-guard");
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
    for tool in ["Write", "Edit", "MultiEdit", "NotebookEdit"] {
        assert!(
            matcher.contains(tool),
            "matcher {matcher:?} should cover {tool}"
        );
    }

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
