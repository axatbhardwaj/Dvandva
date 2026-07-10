//! `dvandva stop-guard` — Claude Code Stop hook that keeps a walkaway Dvandva
//! role from silently ending its turn while it still holds a live baton.
//!
//! Spawns the compiled binary (`CARGO_BIN_EXE_dvandva`) and pipes each test's
//! Stop-hook payload to its stdin, mirroring how Claude Code invokes a
//! command-type hook.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};

/// Spawn `dvandva stop-guard` with `cwd` as its working directory and `envs`
/// applied, write `stdin_bytes` to its stdin, and return the completed `Output`.
fn run_guard_in(cwd: &Path, envs: &[(&str, &str)], stdin_bytes: &[u8]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dvandva"));
    cmd.arg("stop-guard")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().expect("failed to spawn dvandva stop-guard");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(stdin_bytes)
        .expect("failed to write stdin to dvandva stop-guard");
    child
        .wait_with_output()
        .expect("failed to wait on dvandva stop-guard")
}

fn init_git_repo(dir: &Path) {
    let ok = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .status()
        .expect("git init")
        .success();
    assert!(ok, "git init failed for {dir:?}");
}

/// Write a run baton at `.dvandva/runs/<run_id>/baton.json`.
fn seed_baton(dir: &Path, run_id: &str, baton: &Value) {
    let run_dir = dir.join(".dvandva/runs").join(run_id);
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    std::fs::write(run_dir.join("baton.json"), baton.to_string()).expect("write baton.json");
}

/// A Stop-hook payload with the given `stop_hook_active` flag.
fn stop_payload(active: bool) -> String {
    json!({ "hook_event_name": "Stop", "stop_hook_active": active }).to_string()
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
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should resolve")
}

// ---------------------------------------------------------------------------
// Plugin hook wiring
// ---------------------------------------------------------------------------

#[test]
fn hooks_json_registers_stop_command_without_matcher() {
    let bytes = std::fs::read(repo_root().join("plugins/dvandva/hooks/hooks.json"))
        .expect("plugins/dvandva/hooks/hooks.json should exist");
    let value: Value = serde_json::from_slice(&bytes)
        .expect("plugins/dvandva/hooks/hooks.json should parse as JSON");

    let stop = value["hooks"]["Stop"]
        .as_array()
        .expect("hooks.Stop should be an array");
    assert!(
        !stop.is_empty(),
        "hooks.Stop should have at least one entry"
    );

    let entry = &stop[0];
    // Stop hooks do not support a matcher (Claude Code ignores one if present).
    assert!(
        entry.get("matcher").is_none(),
        "Stop entry should carry no matcher, got: {entry}"
    );
    let handlers = entry["hooks"]
        .as_array()
        .expect("Stop entry should have a hooks array");
    assert!(
        handlers
            .iter()
            .any(|h| h["type"] == "command" && h["command"] == "dvandva stop-guard"),
        "expected a command-type hook running `dvandva stop-guard`, got: {handlers:?}"
    );
}

// ---------------------------------------------------------------------------
// End-to-end decisions
// ---------------------------------------------------------------------------

#[test]
fn allows_outside_a_git_repo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = run_guard_in(dir.path(), &[], stop_payload(false).as_bytes());
    assert_allowed(&out);
}

#[test]
fn blocks_live_walkaway_baton_for_active_role() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    seed_baton(
        dir.path(),
        "live",
        &json!({
            "schema": "dvandva.baton.v3",
            "run_id": "live",
            "run_mode": "walkaway",
            "status": "cross_review",
            "assignee": "team",
            "active_roles": ["vadi", "prativadi"],
            "checkpoint": 12
        }),
    );

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload(false).as_bytes(),
    );
    assert_blocked(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dvandva wait"),
        "block message should name `dvandva wait`, got: {stderr}"
    );
}

#[test]
fn allows_live_walkaway_baton_on_hook_continuation() {
    // stop_hook_active=true: the one-shot nudge already fired; never loop.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    seed_baton(
        dir.path(),
        "live",
        &json!({
            "schema": "dvandva.baton.v3",
            "run_id": "live",
            "run_mode": "walkaway",
            "status": "cross_review",
            "assignee": "team",
            "active_roles": ["vadi", "prativadi"],
            "checkpoint": 12
        }),
    );

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload(true).as_bytes(),
    );
    assert_allowed(&out);
}

#[test]
fn allows_terminal_baton() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    seed_baton(
        dir.path(),
        "done-run",
        &json!({
            "schema": "dvandva.baton.v3",
            "run_id": "done-run",
            "run_mode": "walkaway",
            "status": "done",
            "assignee": "team",
            "active_roles": [],
            "checkpoint": 40
        }),
    );

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload(false).as_bytes(),
    );
    assert_allowed(&out);
}

#[test]
fn allows_human_paused_baton() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    seed_baton(
        dir.path(),
        "paused",
        &json!({
            "schema": "dvandva.baton.v3",
            "run_id": "paused",
            "run_mode": "walkaway",
            "status": "human_question",
            "assignee": "human",
            "active_roles": [],
            "checkpoint": 9
        }),
    );

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload(false).as_bytes(),
    );
    assert_allowed(&out);
}

#[test]
fn empty_stdin_still_evaluates_batons() {
    // An empty stdin reads successfully (0 bytes), so it is not a read failure:
    // it parses to no `stop_hook_active` flag (false), and the live walkaway
    // baton still blocks. This asserts the guard reads the real baton rather
    // than short-circuiting on an empty payload.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    seed_baton(
        dir.path(),
        "live",
        &json!({
            "schema": "dvandva.baton.v3",
            "run_id": "live",
            "run_mode": "walkaway",
            "status": "cross_review",
            "assignee": "team",
            "active_roles": ["vadi", "prativadi"],
            "checkpoint": 12
        }),
    );
    let out = run_guard_in(dir.path(), &[("DVANDVA_ROLE", "vadi")], b"");
    assert_blocked(&out);
}
