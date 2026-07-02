//! Integration tests for the `dvandva hook-preflight` subcommand, porting
//! `scripts/test-dvandva-hook-preflight.sh`.
//!
//! RE-KEY (whole-file): the shell suite staged a full copy of the installer
//! and plugin hook sources into each fixture repo (`stage_hook_sources`) so
//! the helper's `resolve_installer` (plugin-co-located script, falling back
//! to the target repo's `scripts/`) had something to find. Post-port there
//! is no installer script to resolve at all: `hook-preflight` calls
//! `dvandva::install_hooks::run_install` in-process, and that function
//! materializes symlinks to the CURRENTLY RUNNING BINARY, so a target repo
//! with zero staged Dvandva source already works. `stage_hook_sources` and
//! its `INSTALLER`/`ROOT_DIR` plumbing are dropped; every case below runs
//! against a bare fixture repo.
//!
//! DROPPED:
//! - "vadi and prativadi hook preflight helpers are byte-identical" (lines
//!   114-119) — one compiled binary now serves both roles via `--role`.
//! - "plugin-only target succeeds via plugin installer" (lines 161-182) —
//!   its entire premise (a target repo with no `scripts/`/`plugins/` proving
//!   the PLUGIN-co-located installer path resolves) is subsumed by every
//!   other case here: the Rust installer never resolves an installer PATH at
//!   all, so "zero staged source" is simply the normal case, not a distinct
//!   one. `foreign_auto_adopts_and_probes_reachable` below already runs
//!   against a repo with nothing staged and asserts the same outcome
//!   (`result=ok`, delegated wrapper adopted, prior recorded).
//!
//! RE-KEYED, PARTIALLY WITHOUT A DEDICATED TEST (documented, not silently
//! dropped): `broken_chain` / `probe_failed` are implemented for
//! shell-output fidelity (exact reason tokens preserved) but are DEFENSIVE,
//! UNREACHABLE branches through the CLI's normal flow post-port:
//! `install_hooks::run_install`'s own internal functional probe (with full
//! rollback on failure) already guarantees that whenever it reports success,
//! the resulting `core.hooksPath` + materialized `pre-commit` are
//! self-consistent and selfcheck-reachable — there is no longer a separate
//! "installer resolved but produced a non-functional chain" failure mode to
//! reach from outside adversarial config tampering that install itself would
//! immediately re-heal on the very next call. The shell's "broken chain"
//! fixture (lines 149-159) relied on stubbing a non-selfcheck-aware
//! `pre-commit` INTO the installer's own source tree before install ever
//! ran; there is no equivalent staged source to corrupt anymore. See
//! `rust/dvandva/tests/install_hooks.rs` for the adjacent installer-level
//! probe/rollback coverage this now depends on.
//!
//! `install_failed`, by contrast, IS reachable through ordinary filesystem
//! failure, not just adversarial tampering: `run_install` calls
//! `fs::create_dir_all(".dvandva/githooks")` before any probing happens, and
//! that call fails whenever `.dvandva` already exists as a non-directory
//! (a stray file, for example). See
//! `dvandva_path_is_a_file_reports_install_failed` below for the dedicated
//! regression.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

fn base_cmd<P: AsRef<std::ffi::OsStr>>(program: P) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_HOOK_SELFCHECK")
        .env_remove("DVANDVA_HOOK_PREFLIGHT");
    cmd
}

fn git(repo: &Path, args: &[&str]) -> Output {
    base_cmd("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git invocation")
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    assert!(git(dir, &["init", "-q"]).status.success(), "git init");
    git(dir, &["config", "user.email", "test@dvandva.test"]);
    git(dir, &["config", "user.name", "Dvandva Test"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    fs::write(dir.join(".gitkeep"), "").unwrap();
    git(dir, &["add", ".gitkeep"]);
    assert!(
        git(dir, &["commit", "-q", "-m", "initial"])
            .status
            .success(),
        "initial commit"
    );
}

fn write_hook(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}

/// Seed a foreign prior hooks dir (`rel`) with one executable hook named
/// `name` and point `core.hooksPath` at it (mirrors a Husky-like adoption).
fn seed_prior(repo: &Path, rel: &str, name: &str, body: &str) {
    write_hook(&repo.join(rel).join(name), body);
    git(repo, &["config", "core.hooksPath", rel]);
}

/// A repo with a Husky-like foreign `core.hooksPath` already set.
fn new_husky_repo(dir: &Path) {
    init_repo(dir);
    seed_prior(
        dir,
        ".husky/_",
        "pre-commit",
        "#!/usr/bin/env bash\nexit 0\n",
    );
}

fn cfg_read(repo: &Path, key: &str) -> String {
    let wt = git(repo, &["config", "--bool", "extensions.worktreeConfig"]);
    if String::from_utf8_lossy(&wt.stdout).trim() == "true" {
        let out = git(repo, &["config", "--worktree", "--get", key]);
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    let out = git(repo, &["config", "--local", "--get", key]);
    if out.status.success() {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        String::new()
    }
}

fn local_cfg(repo: &Path, key: &str) -> String {
    let out = git(repo, &["config", "--local", "--get", key]);
    if out.status.success() {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        String::new()
    }
}

fn hook_preflight(role: &str, repo: &Path, env_role: Option<&str>, extra: &[&str]) -> Output {
    let mut cmd = base_cmd(bin());
    cmd.args(["hook-preflight", "--role", role, "--repo"])
        .arg(repo)
        .args(extra);
    if let Some(env_role) = env_role {
        cmd.env("DVANDVA_ROLE", env_role);
    }
    cmd.output().expect("dvandva hook-preflight")
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

const HOOK_REL: &str = ".dvandva/githooks";

// ===========================================================================
// auto mode over a foreign (Husky-like) hooksPath: adopts, records the
// prior, and the active pre-commit selfcheck probe is reachable. (ports
// lines 121-133; also subsumes the dropped "plugin-only" case since no
// Dvandva source is staged into the fixture at all.)
// ===========================================================================
#[test]
fn foreign_auto_adopts_and_probes_reachable() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    new_husky_repo(repo);

    let out = hook_preflight("prativadi", repo, Some("prativadi"), &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("DVANDVA_HOOK_PREFLIGHT"), "stdout: {text}");
    assert!(text.contains("result=ok"), "stdout: {text}");
    assert!(
        text.contains("sentinel=DVANDVA_GATE_WIRED"),
        "stdout: {text}"
    );

    assert_eq!(cfg_read(repo, "core.hooksPath"), HOOK_REL);
    assert_eq!(cfg_read(repo, "dvandva.priorHooksPath"), ".husky/_");

    let probe = base_cmd(repo.join(HOOK_REL).join("pre-commit"))
        .current_dir(repo)
        .env("DVANDVA_HOOK_SELFCHECK", "1")
        .output()
        .unwrap();
    assert_eq!(code(&probe), 0);
    assert!(
        stdout(&probe).contains("DVANDVA_GATE_WIRED"),
        "stdout: {}",
        stdout(&probe)
    );
}

// ===========================================================================
// off mode: no install, no hooksPath change. (ports lines 135-141)
// ===========================================================================
#[test]
fn off_mode_skips_install_and_leaves_hookspath_unset() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);

    let out = hook_preflight("vadi", repo, Some("vadi"), &["--mode", "off"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("mode=off"),
        "stdout: {}",
        stdout(&out)
    );
    assert_eq!(local_cfg(repo, "core.hooksPath"), "");
    assert!(!repo.join(HOOK_REL).exists());
}

// ===========================================================================
// role mismatch: DVANDVA_ROLE set and different from --role. (ports lines
// 143-147)
// ===========================================================================
#[test]
fn role_mismatch_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);

    let out = hook_preflight("prativadi", repo, Some("vadi"), &[]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("reason=role_mismatch"),
        "stdout: {}",
        stdout(&out)
    );
    assert!(!repo.join(HOOK_REL).exists());
}

// ===========================================================================
// (B7) install_failed is reachable through ordinary filesystem failure: a
// `.dvandva` path that already exists as a FILE (not a directory) makes
// `run_install`'s `fs::create_dir_all(".dvandva/githooks")` fail before any
// probing happens.
// ===========================================================================
#[test]
fn dvandva_path_is_a_file_reports_install_failed() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    fs::write(repo.join(".dvandva"), "not a directory").unwrap();

    let out = hook_preflight("vadi", repo, Some("vadi"), &[]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("reason=install_failed"),
        "stdout: {}",
        stdout(&out)
    );
}

// A role matching (or absent) DVANDVA_ROLE proceeds normally.
#[test]
fn role_matches_absent_env_role_proceeds() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);

    let out = hook_preflight("vadi", repo, None, &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("result=ok"),
        "stdout: {}",
        stdout(&out)
    );
}

// ===========================================================================
// not_git: --repo does not resolve to a git worktree.
// ===========================================================================
#[test]
fn not_git_repo_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    let out = hook_preflight("vadi", tmp.path(), Some("vadi"), &[]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("reason=not_git"),
        "stdout: {}",
        stdout(&out)
    );
}

// ===========================================================================
// Idempotent second call: refreshes rather than re-recording the prior.
// ===========================================================================
#[test]
fn second_call_is_idempotent_and_stays_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    new_husky_repo(repo);

    assert_eq!(code(&hook_preflight("vadi", repo, Some("vadi"), &[])), 0);
    let out = hook_preflight("vadi", repo, Some("vadi"), &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("result=ok"),
        "stdout: {}",
        stdout(&out)
    );
    assert_eq!(cfg_read(repo, "dvandva.priorHooksPath"), ".husky/_");
}

// ===========================================================================
// CLI / usage contract (new: the shell suite never exercised the argument
// parser directly; mirrors the convention established in
// tests/install_hooks.rs).
// ===========================================================================

#[test]
fn missing_role_exits_2() {
    let out = base_cmd(bin()).args(["hook-preflight"]).output().unwrap();
    assert_eq!(code(&out), 2);
}

#[test]
fn invalid_role_exits_2() {
    let out = base_cmd(bin())
        .args(["hook-preflight", "--role", "team"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
}

#[test]
fn invalid_mode_exits_2() {
    let out = base_cmd(bin())
        .args(["hook-preflight", "--role", "vadi", "--mode", "bogus"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
}

#[test]
fn unknown_flag_exits_2() {
    let out = base_cmd(bin())
        .args(["hook-preflight", "--bogus"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
}

#[test]
fn help_flag_exits_0() {
    let out = base_cmd(bin())
        .args(["hook-preflight", "--help"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("Usage"), "stdout: {}", stdout(&out));
}
