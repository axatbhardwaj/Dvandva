//! Integration tests for `dvandva drift-lint`.
//!
//! Ported from the drift-lint-specific cases in
//! `scripts/test-dvandva-commit-gate.sh`: (i) unstamped-commit detection,
//! `--warn` advisory mode, and the hook-adoption-baseline sandwich; (j) the
//! first active-baton bypass; (l) an empty repo; (m) pending-baseline
//! backfill on an unborn repo. Cases (k), (n)-(s) exercise the commit gate,
//! the hook installer, or the plugin mirror and are owned by other tasks.
//!
//! Also covers the CLI-level exit codes from the behavioral contract that
//! the shell suite does not exercise directly: an unknown option (exit 2)
//! and running outside a git repository (exit 1).

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn dvandva() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva"))
}

fn git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("failed to run git")
}

/// A repo with an initial commit, mirroring the shell suite's
/// `new_git_repo` helper.
fn new_git_repo() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    assert!(git(root, &["init", "--quiet"]).status.success());
    assert!(git(root, &["config", "user.email", "test@dvandva.test"])
        .status
        .success());
    assert!(git(root, &["config", "user.name", "Dvandva Test"])
        .status
        .success());
    std::fs::write(root.join(".gitkeep"), "").expect("write .gitkeep");
    assert!(git(root, &["add", ".gitkeep"]).status.success());
    assert!(git(root, &["commit", "--quiet", "-m", "initial"])
        .status
        .success());
    dir
}

/// A freshly `git init`ed repo with no commits at all.
fn empty_git_repo() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    assert!(git(root, &["init", "--quiet"]).status.success());
    assert!(git(root, &["config", "user.email", "test@dvandva.test"])
        .status
        .success());
    assert!(git(root, &["config", "user.name", "Dvandva Test"])
        .status
        .success());
    dir
}

/// Write `path` and commit it; each entry in `messages` becomes a `-m`
/// flag, so two entries produce a subject plus a blank-line-separated
/// trailer body (mirrors the shell suite's checkpoint-stamped commits).
/// Returns the new commit's sha.
fn commit(dir: &Path, path: &str, messages: &[&str]) -> String {
    std::fs::write(dir.join(path), "").expect("write file");
    assert!(git(dir, &["add", path]).status.success());
    let mut args = vec!["commit", "--quiet"];
    for message in messages {
        args.push("-m");
        args.push(message);
    }
    assert!(git(dir, &args).status.success());
    head_sha(dir)
}

fn head_sha(dir: &Path) -> String {
    let out = git(dir, &["rev-parse", "HEAD"]);
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn set_config(dir: &Path, key: &str, value: &str) {
    assert!(git(dir, &["config", key, value]).status.success());
}

fn get_config(dir: &Path, key: &str) -> Option<String> {
    let out = git(dir, &["config", "--get", key]);
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Write a minimal baton JSON (drift-lint only needs `status`).
fn write_baton(dir: &Path, rel_path: &str, status: &str) {
    let full = dir.join(rel_path);
    std::fs::create_dir_all(full.parent().unwrap()).expect("mkdir baton dir");
    let json = format!(
        "{{\"schema\":\"dvandva.baton.v1\",\"status\":\"{status}\",\"assignee\":\"vadi\",\"checkpoint\":1,\"active_roles\":[]}}"
    );
    std::fs::write(full, json).expect("write baton");
}

/// Run `dvandva drift-lint` in `dir` and return the exit code plus the
/// combined stdout+stderr (mirroring the shell suite's `2>&1` captures).
fn run_drift_lint(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = dvandva()
        .arg("drift-lint")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run dvandva drift-lint");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), combined)
}

// ---------------------------------------------------------------------------
// (i) Drift lint flags unstamped commits, `--warn` is advisory, and the
// hook-adoption baseline still catches the earlier bypass after a later
// stamped commit hides it from a naive "since last checkpoint" scan (the
// "sandwich").
// ---------------------------------------------------------------------------
#[test]
fn case_i_flags_unstamped_commit_and_survives_a_later_stamp() {
    let repo = new_git_repo();
    let root = repo.path();
    let initial = head_sha(root);
    // Mirrors the hook installer recording the adoption baseline at the
    // commit that was HEAD when hooks were installed.
    set_config(root, "dvandva.hooksAdoptedAt", &initial);

    commit(
        root,
        "file1.txt",
        &["feat: stamped commit", "Dvandva-Checkpoint: 3"],
    );
    let off_protocol = commit(
        root,
        "file2.txt",
        &["fix: off-protocol commit without trailer"],
    );

    let (code, out) = run_drift_lint(root, &[]);
    assert_eq!(code, 1, "output: {out}");
    assert!(out.contains("DVANDVA_DRIFT warning"), "output: {out}");
    assert!(out.contains("off-protocol"), "output: {out}");
    assert!(out.contains(&off_protocol), "output: {out}");

    let (warn_code, warn_out) = run_drift_lint(root, &["--warn"]);
    assert_eq!(warn_code, 0, "output: {warn_out}");
    assert!(warn_out.contains("DVANDVA_DRIFT"), "output: {warn_out}");

    commit(
        root,
        "file3.txt",
        &["feat: second stamped commit", "Dvandva-Checkpoint: 4"],
    );
    let (code2, out2) = run_drift_lint(root, &[]);
    assert_eq!(code2, 1, "output: {out2}");
    assert!(out2.contains("off-protocol"), "output: {out2}");
}

// ---------------------------------------------------------------------------
// (i) A repo with no checkpointed commits at all has nothing to lint.
// ---------------------------------------------------------------------------
#[test]
fn case_i_no_checkpoints_in_history_exits_0() {
    let repo = new_git_repo();
    commit(repo.path(), "file.txt", &["plain commit without trailer"]);

    let (code, out) = run_drift_lint(repo.path(), &[]);
    assert_eq!(code, 0, "output: {out}");
}

// ---------------------------------------------------------------------------
// (i) An active baton adopted after existing history means pre-adoption
// commits are not drift.
// ---------------------------------------------------------------------------
#[test]
fn case_i_adoption_baseline_ignores_pre_adoption_history() {
    let repo = new_git_repo();
    let root = repo.path();
    commit(root, "pre-adoption.txt", &["plain pre-adoption commit"]);
    write_baton(root, ".dvandva/baton.json", "implementing");
    let adoption_sha = head_sha(root);
    set_config(root, "dvandva.hooksAdoptedAt", &adoption_sha);

    let (code, out) = run_drift_lint(root, &[]);
    assert_eq!(code, 0, "output: {out}");
}

// ---------------------------------------------------------------------------
// (i) Only commits after the adoption baseline are reportable.
// ---------------------------------------------------------------------------
#[test]
fn case_i_flags_post_adoption_bypass_only() {
    let repo = new_git_repo();
    let root = repo.path();
    commit(root, "pre-adoption.txt", &["plain pre-adoption commit"]);
    write_baton(root, ".dvandva/baton.json", "implementing");
    let adoption_sha = head_sha(root);
    set_config(root, "dvandva.hooksAdoptedAt", &adoption_sha);

    commit(root, "post-adoption.txt", &["post-adoption bypass"]);

    let (code, out) = run_drift_lint(root, &[]);
    assert_eq!(code, 1, "output: {out}");
    assert!(out.contains("post-adoption bypass"), "output: {out}");
}

// ---------------------------------------------------------------------------
// (j) An active baton with no checkpointed commits yet still flags the
// first off-protocol bypass commit.
// ---------------------------------------------------------------------------
#[test]
fn case_j_flags_first_active_baton_bypass_commit() {
    let repo = new_git_repo();
    let root = repo.path();
    write_baton(root, ".dvandva/baton.json", "implementing");
    commit(root, "noverify.txt", &["bypass without role"]);

    let (code, out) = run_drift_lint(root, &[]);
    assert_eq!(code, 1, "output: {out}");
    assert!(out.contains("DVANDVA_DRIFT warning"), "output: {out}");
    assert!(out.contains("bypass without role"), "output: {out}");
}

// ---------------------------------------------------------------------------
// (l) An empty git repo (no commits at all) has no drift.
// ---------------------------------------------------------------------------
#[test]
fn case_l_empty_git_repo_exits_0() {
    let repo = empty_git_repo();
    let (code, out) = run_drift_lint(repo.path(), &[]);
    assert_eq!(code, 0, "output: {out}");
    assert!(out.contains("no checkpointed commits"), "output: {out}");
}

// ---------------------------------------------------------------------------
// (m) A pending root-commit baseline recorded before any commit existed is
// backfilled to the root sha as soon as one exists.
// ---------------------------------------------------------------------------
#[test]
fn case_m_backfills_pending_root_baseline() {
    let repo = empty_git_repo();
    let root = repo.path();
    set_config(root, "dvandva.hooksAdoptedAt", "__DVANDVA_ROOT_PENDING__");

    let root_sha = commit(
        root,
        "root.txt",
        &["feat: root checkpoint", "Dvandva-Checkpoint: 1"],
    );

    let (code, out) = run_drift_lint(root, &[]);
    assert_eq!(code, 0, "output: {out}");

    let backfilled = get_config(root, "dvandva.hooksAdoptedAt");
    assert_eq!(backfilled.as_deref(), Some(root_sha.as_str()));
}

// ---------------------------------------------------------------------------
// (m) A pending baseline still catches an unstamped root commit when an
// active baton exists, and the backfilled inclusive baseline persists
// across repeated runs.
// ---------------------------------------------------------------------------
#[test]
fn case_m_flags_unstamped_root_after_pending_baseline() {
    let repo = empty_git_repo();
    let root = repo.path();
    write_baton(root, ".dvandva/baton.json", "implementing");
    set_config(root, "dvandva.hooksAdoptedAt", "__DVANDVA_ROOT_PENDING__");

    commit(root, "root-bypass.txt", &["root bypass without trailer"]);

    let (code, out) = run_drift_lint(root, &[]);
    assert_eq!(code, 1, "output: {out}");
    assert!(out.contains("root bypass without trailer"), "output: {out}");

    let (code2, out2) = run_drift_lint(root, &[]);
    assert_eq!(code2, 1, "output: {out2}");
    assert!(
        out2.contains("root bypass without trailer"),
        "output: {out2}"
    );
}

// ---------------------------------------------------------------------------
// Behavioral contract: an unrecognized option exits 2.
// ---------------------------------------------------------------------------
#[test]
fn unknown_option_exits_2() {
    let repo = new_git_repo();
    let (code, _out) = run_drift_lint(repo.path(), &["--bogus"]);
    assert_eq!(code, 2);
}

// ---------------------------------------------------------------------------
// Behavioral contract: running outside a git repository exits 1.
// ---------------------------------------------------------------------------
#[test]
fn not_a_git_repo_exits_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, out) = run_drift_lint(dir.path(), &[]);
    assert_eq!(code, 1, "output: {out}");
    assert!(out.contains("not inside a git repository"), "output: {out}");
}
