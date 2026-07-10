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

/// Stamp `session_id` as bound to `run_id` at `.dvandva/runs/<run_id>/.sessions/<session_id>`,
/// mirroring what the `baton-guard` PreToolUse hook writes when a session
/// touches the run.
fn stamp_session(dir: &Path, run_id: &str, session_id: &str) {
    let sessions = dir.join(".dvandva/runs").join(run_id).join(".sessions");
    std::fs::create_dir_all(&sessions).expect("create .sessions dir");
    std::fs::write(sessions.join(session_id), b"").expect("write session marker");
}

/// A Stop-hook payload with the given `stop_hook_active` flag.
fn stop_payload(active: bool) -> String {
    json!({ "hook_event_name": "Stop", "stop_hook_active": active }).to_string()
}

/// A Stop-hook payload carrying a `session_id`, as Claude Code sends.
fn stop_payload_with_session(active: bool, session_id: &str) -> String {
    json!({
        "hook_event_name": "Stop",
        "stop_hook_active": active,
        "session_id": session_id,
    })
    .to_string()
}

/// A live (walkaway, non-terminal, non-paused) run baton with the given
/// assignment shape.
fn live_baton(run_id: &str, assignee: &str, active_roles: Value) -> Value {
    json!({
        "schema": "dvandva.baton.v3",
        "run_id": run_id,
        "run_mode": "walkaway",
        "status": "implementing",
        "assignee": assignee,
        "active_roles": active_roles,
        "checkpoint": 7
    })
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
fn blocks_live_walkaway_baton_for_bound_session() {
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
    stamp_session(dir.path(), "live", "sess-live");

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload_with_session(false, "sess-live").as_bytes(),
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
    // stop_hook_active=true: the one-shot nudge already fired; never loop —
    // even for a session bound to the live run.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    seed_baton(
        dir.path(),
        "live",
        &live_baton("live", "team", json!(["vadi", "prativadi"])),
    );
    stamp_session(dir.path(), "live", "sess-live");

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload_with_session(true, "sess-live").as_bytes(),
    );
    assert_allowed(&out);
}

#[test]
fn allows_terminal_baton_even_when_bound() {
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
    stamp_session(dir.path(), "done-run", "sess-done");

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload_with_session(false, "sess-done").as_bytes(),
    );
    assert_allowed(&out);
}

#[test]
fn allows_human_paused_baton_even_when_bound() {
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
    stamp_session(dir.path(), "paused", "sess-paused");

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload_with_session(false, "sess-paused").as_bytes(),
    );
    assert_allowed(&out);
}

#[test]
fn empty_stdin_fails_open() {
    // An empty stdin reads successfully (0 bytes) but cannot be parsed for a
    // `session_id`, so the session cannot be bound to any run — fail open and
    // allow the stop, like the malformed-payload path (P3).
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    seed_baton(
        dir.path(),
        "live",
        &live_baton("live", "team", json!(["vadi", "prativadi"])),
    );
    stamp_session(dir.path(), "live", "sess-live");
    let out = run_guard_in(dir.path(), &[("DVANDVA_ROLE", "vadi")], b"");
    assert_allowed(&out);
}

// ---------------------------------------------------------------------------
// Session/run binding probes (ckpt-96): the guard blocks a session BOUND to a
// live walkaway run regardless of assignee, and fails open for strangers,
// malformed payloads, and missing session ids.
// ---------------------------------------------------------------------------

/// P1: a session bound to the run must be blocked from stopping while the run
/// is live, even when its role is not the current assignee (waiting IS the job).
#[test]
fn p1_assigned_away_bound_session_blocks() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    // assignee/active_roles name only the peer (prativadi); our role is vadi.
    seed_baton(
        dir.path(),
        "p1",
        &live_baton("p1", "prativadi", json!(["prativadi"])),
    );
    stamp_session(dir.path(), "p1", "sess-P1");

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload_with_session(false, "sess-P1").as_bytes(),
    );
    assert_blocked(&out);
}

/// P2: a session that never participated in the run (no marker) must be free to
/// stop, even though the baton names its role as active — fail-open for strangers.
#[test]
fn p2_unbound_unrelated_session_allows() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    seed_baton(
        dir.path(),
        "p2",
        &live_baton("p2", "team", json!(["vadi", "prativadi"])),
    );
    // No stamp_session: this session is a stranger to the run.

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload_with_session(false, "sess-STRANGER").as_bytes(),
    );
    assert_allowed(&out);
}

/// P3: an unparseable/malformed stdin payload must fail OPEN (allow), like the
/// other guard failure paths — a guard defect must never strand a session.
#[test]
fn p3_malformed_payload_allows() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    seed_baton(
        dir.path(),
        "p3",
        &live_baton("p3", "team", json!(["vadi", "prativadi"])),
    );

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        b"{ this is : not valid json",
    );
    assert_allowed(&out);
}

/// P4: the block reason must include the canonical `dvandva wait` resume
/// command with its full flag set so the nudged session can re-enter the loop.
#[test]
fn p4_block_reason_names_canonical_resume_command() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    seed_baton(
        dir.path(),
        "p4",
        &live_baton("p4", "team", json!(["vadi", "prativadi"])),
    );
    stamp_session(dir.path(), "p4", "sess-P4");

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "vadi")],
        stop_payload_with_session(false, "sess-P4").as_bytes(),
    );
    assert_blocked(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dvandva wait --role vadi"),
        "reason should name the role in the resume command, got: {stderr}"
    );
    assert!(
        stderr.contains("--interval 60 --max-wait 540 --stall-max 1800 --until-actionable"),
        "reason should carry the canonical wait flags, got: {stderr}"
    );
    assert!(
        stderr.contains("--through-human"),
        "reason should mention --through-human for Codex-hosted, got: {stderr}"
    );
}
