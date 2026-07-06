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

/// Like [`write_aged_marker`], but with a second line stamping the session
/// that owns the marker.
fn write_stamped_marker(dir: &Path, role: &str, age_secs: u64, session: &str) {
    let marker_dir = dir.join(".dvandva");
    std::fs::create_dir_all(&marker_dir).expect("create .dvandva");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_secs();
    std::fs::write(
        marker_dir.join(format!(".session-baton-pending.{role}")),
        format!("{}\n{session}\n", now.saturating_sub(age_secs)),
    )
    .expect("write stamped marker");
}

/// Write a v3 (three-line) marker: arming epoch, session (or "-"), and a
/// last-warn epoch `warn_age_secs` in the past.
fn write_warned_marker(dir: &Path, role: &str, age_secs: u64, session: &str, warn_age_secs: u64) {
    let marker_dir = dir.join(".dvandva");
    std::fs::create_dir_all(&marker_dir).expect("create .dvandva");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_secs();
    std::fs::write(
        marker_dir.join(format!(".session-baton-pending.{role}")),
        format!(
            "{}\n{session}\n{}\n",
            now.saturating_sub(age_secs),
            now.saturating_sub(warn_age_secs)
        ),
    )
    .expect("write warned marker");
}

/// Assert the guard allowed the call (exit 0) while emitting the warn JSON:
/// stdout parses as JSON carrying a PreToolUse `additionalContext` (the
/// model-visible warning) and a top-level `systemMessage` (the user-visible
/// one).
fn assert_warned(out: &Output) {
    assert_eq!(
        out.status.code(),
        Some(0),
        "warn must not block; stdout: {}, stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "warn stdout should be JSON ({e}); stdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert_eq!(
        value["hookSpecificOutput"]["hookEventName"], "PreToolUse",
        "hookEventName must be PreToolUse"
    );
    let context = value["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("warn JSON should carry hookSpecificOutput.additionalContext");
    assert!(
        context.contains("dvandva resolve") && context.contains("dvandva write"),
        "additionalContext should spell the recovery sequence, got: {context}"
    );
    assert!(
        context.to_lowercase().contains("warning"),
        "additionalContext should state this is a warning, got: {context}"
    );
    assert!(
        value["systemMessage"].as_str().is_some(),
        "warn JSON should carry a user-visible systemMessage"
    );
}

/// Read the raw marker content for `role`, if the marker exists.
fn read_marker(dir: &Path, role: &str) -> Option<String> {
    std::fs::read_to_string(
        dir.join(".dvandva")
            .join(format!(".session-baton-pending.{role}")),
    )
    .ok()
}

/// Spawn an arbitrary `dvandva <args...>` subcommand with `cwd` as its
/// working directory (no stdin), returning the completed `Output`.
fn run_cli(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run dvandva subcommand")
}

/// Like [`run_cli`], with explicit environment variables applied.
fn run_cli_with_env(cwd: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dvandva"));
    cmd.args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().expect("failed to run dvandva subcommand")
}

/// A minimal PreToolUse payload for a file-path-taking tool.
fn payload(tool_name: &str, file_path: &str) -> String {
    json!({
        "tool_name": tool_name,
        "tool_input": { "file_path": file_path }
    })
    .to_string()
}

/// A PreToolUse payload carrying the `session_id` Claude Code includes on
/// every hook invocation.
fn payload_with_session(tool_name: &str, file_path: &str, session_id: &str) -> String {
    json!({
        "session_id": session_id,
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
fn sla_breach_warns_without_blocking_and_records_the_warn() {
    // Q1 (human): a breach is a loud warning, never a block. The first
    // breached call is allowed with warn JSON on stdout, and the marker
    // gains a third line so the throttle can hold.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_warned(&out);
    let content = read_marker(dir.path(), "vadi").expect("marker should survive a warn");
    assert_eq!(
        content.lines().count(),
        3,
        "warn should record a last-warn epoch as the third line, got: {content}"
    );
}

#[test]
fn sla_breach_stays_silent_within_the_warn_throttle() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_warned_marker(dir.path(), "vadi", 500, "-", 10);

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_allowed(&out);
}

#[test]
fn sla_breach_rewarns_after_the_throttle_interval() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_warned_marker(dir.path(), "vadi", 900, "-", 350);

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_warned(&out);
}

#[test]
fn direct_edit_guard_still_blocks_during_breach() {
    // Q5 (human): warn-only applies ONLY to the creation SLA — baton
    // integrity keeps its hard block in every SLA state.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Edit", ".dvandva/runs/x/baton.json").as_bytes(),
    );
    assert_blocked(&out);
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
    assert_eq!(
        out.status.code(),
        Some(0),
        "recovery command must run; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
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
            "schema": "dvandva.baton.v2",
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
fn schema_less_stub_does_not_satisfy_the_sla() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let run_dir = dir.path().join(".dvandva/runs/run");
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    std::fs::write(
        run_dir.join("baton.json"),
        json!({ "stub": "session-unblock" }).to_string(),
    )
    .expect("write stub baton.json");

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_warned(&out);
    assert!(
        dir.path()
            .join(".dvandva/.session-baton-pending.vadi")
            .exists(),
        "a schema-less stub must not be treated as a real baton"
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

#[test]
fn guard_never_self_arms_in_a_repo_without_a_marker() {
    // The over-arming bug: the guard used to CREATE the pending marker (and a
    // .dvandva/ directory) on the first tool call in ANY git repo, turning
    // every non-Dvandva session into an SLA countdown. The guard must be a
    // pure reader: arming belongs to `dvandva resolve`/`preflight`.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_allowed(&out);
    assert!(
        !dir.path().join(".dvandva").exists(),
        "guard must not create markers or a .dvandva directory in repos it merely observes"
    );
}

#[test]
fn sla_enforces_marker_stamped_by_own_session() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_stamped_marker(dir.path(), "vadi", 500, "sess-1");

    let out = run_guard_in(
        dir.path(),
        &[],
        payload_with_session("Write", "some/other/file.txt", "sess-1").as_bytes(),
    );
    assert_warned(&out);
}

#[test]
fn sla_ignores_marker_stamped_by_a_different_session() {
    // A marker armed and stamped by a dead (or concurrent) session must not
    // brick an unrelated session in the same repo — this is the "next
    // session in the repo is blocked from its first tool call" leak.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_stamped_marker(dir.path(), "vadi", 500, "sess-1");

    let out = run_guard_in(
        dir.path(),
        &[],
        payload_with_session("Write", "some/other/file.txt", "sess-2").as_bytes(),
    );
    assert_allowed(&out);
    let content = read_marker(dir.path(), "vadi").expect("foreign marker must be preserved");
    assert!(
        content.contains("sess-1"),
        "foreign-session marker must not be rewritten or deleted, got: {content}"
    );
}

#[test]
fn sla_stamps_unstamped_marker_with_payload_session() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 5);

    let out = run_guard_in(
        dir.path(),
        &[],
        payload_with_session("Write", "some/other/file.txt", "sess-1").as_bytes(),
    );
    assert_allowed(&out);
    let content = read_marker(dir.path(), "vadi").expect("marker should still exist");
    assert!(
        content.contains("sess-1"),
        "first session to see an unstamped marker should stamp it, got: {content}"
    );
}

#[test]
fn sla_deletes_marker_past_dead_run_ceiling() {
    // A marker 30x older than the threshold belongs to a run that died long
    // ago; enforcing it would block sessions hours later (observed: 28688s).
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 4000);

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_allowed(&out);
    assert!(
        read_marker(dir.path(), "vadi").is_none(),
        "a dead-run marker past the ceiling should be removed"
    );
}

#[test]
fn sla_breach_allows_writing_a_candidate_baton() {
    // Recovery is resolve -> Write the candidate -> `dvandva write`; under
    // warn-only semantics the candidate write runs (possibly carrying the
    // warn JSON), never blocks.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Write", ".dvandva/runs/run/baton.next.json").as_bytes(),
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "candidate write must run; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn sla_breach_never_blocks_arbitrary_bash() {
    // Warn-only semantics: with nothing blocked there is no exemption gate
    // left to bypass — chained commands run like everything else (the old
    // first-token allowlist and its chain-bypass defect are both gone).
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let out = run_guard_in(
        dir.path(),
        &[],
        json!({
            "tool_name": "Bash",
            "tool_input": { "command": "dvandva --version && touch pwned" }
        })
        .to_string()
        .as_bytes(),
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "warn-only breach must not block any Bash command; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn sla_old_single_line_marker_upgrades_on_first_warn() {
    // Upgrade smoke (Q7): a beta.3-era single-line marker parses as
    // never-warned; the first warn upgrades it to the three-line format
    // while preserving the arming epoch.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);
    let before = read_marker(dir.path(), "vadi").expect("marker pre-exists");

    let out = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_warned(&out);
    let after = read_marker(dir.path(), "vadi").expect("marker survives");
    assert_eq!(
        before.lines().next(),
        after.lines().next(),
        "warn upgrade must preserve the arming epoch"
    );
}

#[test]
fn sla_inert_for_explicit_prativadi_sessions() {
    // The baton-creation SLA is vadi-owned: preflight tells a batonless
    // prativadi to `wait --discover`, so a prativadi session must never be
    // blocked by a pending vadi marker.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_ROLE", "prativadi")],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_allowed(&out);
}

// ---------------------------------------------------------------------------
// SLA arming (owned by resolve/preflight, never by the guard)
// ---------------------------------------------------------------------------

#[test]
fn resolve_create_arms_the_vadi_sla_marker() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());

    let out = run_cli(dir.path(), &["resolve", "--role", "vadi"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("CREATE "),
        "expected CREATE, got: {stdout}"
    );

    let content =
        read_marker(dir.path(), "vadi").expect("resolve CREATE should arm the vadi SLA marker");
    let epoch: u64 = content
        .lines()
        .next()
        .expect("marker first line")
        .trim()
        .parse()
        .expect("marker holds an epoch timestamp");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_secs();
    assert!(
        now.saturating_sub(epoch) < 5,
        "marker epoch should be freshly written"
    );
}

#[test]
fn resolve_create_does_not_arm_for_prativadi() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());

    let out = run_cli(dir.path(), &["resolve", "--role", "prativadi"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        read_marker(dir.path(), "vadi").is_none() && read_marker(dir.path(), "prativadi").is_none(),
        "prativadi never owes the baton, so resolve must not arm any marker"
    );
}

#[test]
fn resolve_run_id_selector_missing_arms_for_vadi() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    std::fs::create_dir_all(dir.path().join(".dvandva/runs/fresh")).expect("create run dir");

    let out = run_cli_with_env(
        dir.path(),
        &["resolve", "--role", "vadi"],
        &[("DVANDVA_RUN_ID", "fresh")],
    );
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("CREATE .dvandva/runs/fresh/baton.json"),
        "expected CREATE for missing selected baton, got: {stdout}"
    );
    assert!(
        read_marker(dir.path(), "vadi").is_some(),
        "missing selected baton should arm the vadi SLA marker"
    );
}

#[test]
fn resolve_run_id_selector_missing_does_not_arm_for_prativadi() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    std::fs::create_dir_all(dir.path().join(".dvandva/runs/fresh")).expect("create run dir");

    let out = run_cli_with_env(
        dir.path(),
        &["resolve", "--role", "prativadi"],
        &[("DVANDVA_RUN_ID", "fresh")],
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(
        read_marker(dir.path(), "vadi").is_none(),
        "prativadi selector bootstrap must not arm the vadi SLA marker"
    );
}

#[test]
fn resolve_run_id_selector_invalid_candidate_stops_as_stale_run_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    let run_dir = dir.path().join(".dvandva/runs/fresh");
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    std::fs::write(run_dir.join("baton.next.json"), "{ not json\n").expect("write candidate");

    let out = run_cli_with_env(
        dir.path(),
        &["resolve", "--role", "vadi"],
        &[("DVANDVA_RUN_ID", "fresh")],
    );
    assert_eq!(out.status.code(), Some(12));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("DVANDVA_RESOLVE stale_run_dir"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("reason=invalid_candidate"),
        "stderr: {stderr}"
    );
    assert!(
        read_marker(dir.path(), "vadi").is_none(),
        "stale run-dir gate must not arm a fresh SLA marker"
    );
    assert!(
        run_dir.join("baton.next.json").exists(),
        "invalid leftover candidate should remain for human inspection"
    );
}

#[test]
fn resolve_run_id_selector_garbage_marker_stops_as_stale_run_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    std::fs::create_dir_all(dir.path().join(".dvandva/runs/fresh")).expect("create run dir");
    std::fs::write(
        dir.path().join(".dvandva/.session-baton-pending.vadi"),
        "not-a-number\n",
    )
    .expect("write marker");

    let out = run_cli_with_env(
        dir.path(),
        &["resolve", "--role", "vadi"],
        &[("DVANDVA_RUN_ID", "fresh")],
    );
    assert_eq!(out.status.code(), Some(12));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("DVANDVA_RESOLVE stale_run_dir"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("reason=garbage_marker"), "stderr: {stderr}");
    assert_eq!(
        read_marker(dir.path(), "vadi").as_deref(),
        Some("not-a-number\n"),
        "garbage marker should stay in place for human inspection"
    );
}

#[test]
fn resolve_run_id_selector_invalid_baton_stops_as_stale_run_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    let baton = dir.path().join(".dvandva/runs/fresh/baton.json");
    std::fs::create_dir_all(baton.parent().expect("parent")).expect("create run dir");
    std::fs::write(&baton, "{ not json\n").expect("write baton");

    let out = run_cli_with_env(
        dir.path(),
        &["resolve", "--role", "vadi"],
        &[("DVANDVA_RUN_ID", "fresh")],
    );
    assert_eq!(out.status.code(), Some(12));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("DVANDVA_RESOLVE stale_run_dir"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("reason=invalid_baton"), "stderr: {stderr}");
}

#[test]
fn resolve_run_dir_selector_missing_arms_for_vadi() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    let run_dir = dir.path().join(".dvandva/runs/from-dir");
    std::fs::create_dir_all(&run_dir).expect("create run dir");

    let out = run_cli_with_env(
        dir.path(),
        &["resolve", "--role", "vadi"],
        &[("DVANDVA_RUN_DIR", run_dir.to_str().expect("utf8 path"))],
    );
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with(&format!("CREATE {}/baton.json", run_dir.display())),
        "expected CREATE for missing run-dir selector baton, got: {stdout}"
    );
    assert!(
        read_marker(dir.path(), "vadi").is_some(),
        "missing selected run-dir baton should arm the vadi SLA marker"
    );
}

#[test]
fn resolve_baton_file_selector_invalid_baton_stops_as_stale_run_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    let baton = dir.path().join(".dvandva/runs/from-file/baton.json");
    std::fs::create_dir_all(baton.parent().expect("parent")).expect("create run dir");
    std::fs::write(&baton, "{ not json\n").expect("write baton");

    let out = run_cli_with_env(
        dir.path(),
        &["resolve", "--role", "vadi"],
        &[("DVANDVA_BATON_FILE", baton.to_str().expect("utf8 path"))],
    );
    assert_eq!(out.status.code(), Some(12));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("DVANDVA_RESOLVE stale_run_dir"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("reason=invalid_baton"), "stderr: {stderr}");
}

#[test]
fn resolve_does_not_reset_an_existing_marker_epoch() {
    // Re-running resolve must not restart the SLA clock — otherwise a
    // drifting agent could reset its own deadline indefinitely.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);
    let before = read_marker(dir.path(), "vadi").expect("marker pre-exists");

    let out = run_cli(dir.path(), &["resolve", "--role", "vadi"]);
    assert_eq!(out.status.code(), Some(0));
    let after = read_marker(dir.path(), "vadi").expect("marker should survive re-resolve");
    assert_eq!(
        before.lines().next(),
        after.lines().next(),
        "re-resolving must preserve the original SLA epoch"
    );
}

#[test]
fn resolve_resolved_clears_the_marker() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let run_dir = dir.path().join(".dvandva/runs/r1");
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    std::fs::write(
        run_dir.join("baton.json"),
        json!({
            "schema": "dvandva.baton.v2",
            "run_id": "r1",
            "status": "research_drafting",
            "assignee": "vadi",
            "updated_at": "2026-01-01T00:00:00Z"
        })
        .to_string(),
    )
    .expect("write baton.json");

    let out = run_cli(dir.path(), &["resolve", "--role", "vadi"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("RESOLVED "),
        "expected RESOLVED, got: {stdout}"
    );
    assert!(
        read_marker(dir.path(), "vadi").is_none(),
        "a resolved run means the SLA is satisfied; the marker must be cleared"
    );
}

#[test]
fn preflight_create_arms_the_vadi_sla_marker() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());

    let out = run_cli(
        dir.path(),
        &["preflight", "--role", "vadi", "--mode", "off"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {}, stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        read_marker(dir.path(), "vadi").is_some(),
        "preflight result=create should arm the vadi SLA marker"
    );
}

#[test]
fn preflight_create_does_not_arm_for_prativadi() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());

    let out = run_cli(
        dir.path(),
        &["preflight", "--role", "prativadi", "--mode", "off"],
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(
        read_marker(dir.path(), "vadi").is_none() && read_marker(dir.path(), "prativadi").is_none(),
        "preflight result=wait must not arm any marker for prativadi"
    );
}

// ---------------------------------------------------------------------------
// SLA deadline line (C3): resolve/preflight surface the countdown on stdout
// ---------------------------------------------------------------------------

/// Extract the first `DVANDVA_SLA ` line from a command's stdout.
fn sla_line(out: &Output) -> Option<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.starts_with("DVANDVA_SLA "))
        .map(str::to_string)
}

#[test]
fn resolve_create_prints_the_sla_deadline_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());

    let out = run_cli(dir.path(), &["resolve", "--role", "vadi"]);
    assert_eq!(out.status.code(), Some(0));
    let line = sla_line(&out).expect("resolve CREATE should print a DVANDVA_SLA line");
    let epoch: u64 = read_marker(dir.path(), "vadi")
        .expect("marker armed")
        .lines()
        .next()
        .expect("epoch line")
        .trim()
        .parse()
        .expect("epoch");
    assert_eq!(
        line,
        format!(
            "DVANDVA_SLA armed role=vadi deadline={} threshold_s=120",
            epoch + 120
        ),
        "deadline must be the arming epoch plus the threshold"
    );
}

#[test]
fn resolve_reprints_the_same_deadline_while_armed() {
    // Re-resolving must re-surface the countdown (turn-entry visibility on
    // any engine) without resetting it.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 50);
    let epoch: u64 = read_marker(dir.path(), "vadi")
        .expect("marker")
        .trim()
        .parse()
        .expect("epoch");

    let out = run_cli(dir.path(), &["resolve", "--role", "vadi"]);
    assert_eq!(out.status.code(), Some(0));
    let line = sla_line(&out).expect("armed resolve should re-print the DVANDVA_SLA line");
    assert!(
        line.contains(&format!("deadline={}", epoch + 120)),
        "re-resolve must keep the original deadline, got: {line}"
    );
}

#[test]
fn preflight_create_prints_the_sla_deadline_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());

    let out = run_cli(
        dir.path(),
        &["preflight", "--role", "vadi", "--mode", "off"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {}, stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        sla_line(&out).is_some(),
        "preflight result=create should print a DVANDVA_SLA line, got: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn no_sla_line_when_a_valid_baton_resolves() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 50);

    let run_dir = dir.path().join(".dvandva/runs/r1");
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    std::fs::write(
        run_dir.join("baton.json"),
        json!({
            "schema": "dvandva.baton.v2",
            "run_id": "r1",
            "status": "research_drafting",
            "assignee": "vadi",
            "updated_at": "2026-01-01T00:00:00Z"
        })
        .to_string(),
    )
    .expect("write baton.json");

    let out = run_cli(dir.path(), &["resolve", "--role", "vadi"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        sla_line(&out).is_none(),
        "a resolved run satisfies the SLA; no deadline line, got: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn warn_sequence_composes_warn_silent_rewarn() {
    // The composed lifecycle in one repo: first breached call warns, the
    // next is silent under the throttle, and once the recorded warn ages
    // past the interval the guard warns again.
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 500);

    let first = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_warned(&first);

    let second = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_allowed(&second);

    // Age the recorded warn past the 300s throttle, preserving epoch/session.
    write_warned_marker(dir.path(), "vadi", 900, "-", 350);
    let third = run_guard_in(
        dir.path(),
        &[],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_warned(&third);
}

#[test]
fn preflight_run_id_selector_missing_arms_and_prints_create() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());

    let out = run_cli_with_env(
        dir.path(),
        &["preflight", "--role", "vadi", "--mode", "off"],
        &[("DVANDVA_RUN_ID", "fresh-run")],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {}, stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("result=create"),
        "selector bootstrap should report create, got: {stdout}"
    );
    assert!(
        read_marker(dir.path(), "vadi").is_some(),
        "selector bootstrap must arm the vadi SLA marker"
    );
    assert!(
        sla_line(&out).is_some(),
        "selector bootstrap should print the deadline line, got: {stdout}"
    );
}

#[test]
fn preflight_run_id_selector_stale_candidate_stops_as_stale_run_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    let run_dir = dir.path().join(".dvandva/runs/fresh-run");
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    std::fs::write(run_dir.join("baton.next.json"), "not-json{{{").expect("write stale candidate");

    let out = run_cli_with_env(
        dir.path(),
        &["preflight", "--role", "vadi", "--mode", "off"],
        &[("DVANDVA_RUN_ID", "fresh-run")],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "stale bootstrap must stop for the human"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("result=error") && stdout.contains("reason=stale_run_dir"),
        "expected the stale_run_dir error token, got: {stdout}"
    );
    assert!(
        read_marker(dir.path(), "vadi").is_none(),
        "a stale-stopped bootstrap must not arm the SLA"
    );
}

#[test]
fn sla_custom_threshold_env_var_actually_breaches_at_the_cli_level() {
    let dir = tempfile::tempdir().expect("tempdir");
    init_git_repo(dir.path());
    write_aged_marker(dir.path(), "vadi", 6);

    let out = run_guard_in(
        dir.path(),
        &[("DVANDVA_BATON_SLA_SECONDS", "5")],
        payload("Write", "some/other/file.txt").as_bytes(),
    );
    assert_warned(&out);
}
