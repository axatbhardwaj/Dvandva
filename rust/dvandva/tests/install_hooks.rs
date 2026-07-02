//! Integration tests for the `dvandva install-hooks` installer, porting the
//! installer-focused cases of `scripts/test-dvandva-commit-gate.sh` (config
//! adoption, materialization, idempotent refresh, uninstall/restore, foreign
//! hooksPath wrapping, absolute-prior round-trip, unborn-repo baseline, and
//! linked-worktree hooksPath isolation) plus the materialization contract the
//! hook-preflight suite depends on.
//!
//! DESIGN DECISION D2 re-key: post-port there are NO shell files. The installer
//! materializes `.dvandva/githooks/pre-commit` and `prepare-commit-msg` as
//! SYMLINKS to the running binary (copy fallback if symlink(2) fails), plus a
//! symlink pass-through stub for every OTHER canonical hook name that exists
//! executable in the prior hooks dir. Shell assertions about the presence of
//! `dvandva-hook-lib.sh` / `dvandva-commit-gate.sh` / `dvandva-drift-lint.sh`
//! are re-keyed here to symlink presence + `DVANDVA_*_WIRED` selfcheck probes.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

/// A command with an isolated git environment and no ambient Dvandva vars, so
/// the developer's real global `core.hooksPath` cannot leak into a fixture.
fn base_cmd<P: AsRef<std::ffi::OsStr>>(program: P) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_HOOK_SELFCHECK");
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

/// A repo with no initial commit (unborn HEAD).
fn init_repo_unborn(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    assert!(git(dir, &["init", "-q"]).status.success(), "git init");
    git(dir, &["config", "user.email", "test@dvandva.test"]);
    git(dir, &["config", "user.name", "Dvandva Test"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

fn write_hook(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}

/// Seed a foreign prior hooks dir (`rel`) with one executable hook named `name`
/// and point `core.hooksPath` at it (mirrors a Husky/lefthook adoption).
fn seed_prior(repo: &Path, rel: &str, name: &str, body: &str) {
    write_hook(&repo.join(rel).join(name), body);
    git(repo, &["config", "core.hooksPath", rel]);
}

/// Read a config key with the installer's worktree-then-local fallback.
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

fn head_sha(repo: &Path) -> String {
    String::from_utf8_lossy(&git(repo, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string()
}

fn head_subject(repo: &Path) -> String {
    String::from_utf8_lossy(&git(repo, &["log", "-1", "--format=%s"]).stdout)
        .trim()
        .to_string()
}

/// Run `dvandva install-hooks <repo> [extra...]`.
fn install(repo: &Path, extra: &[&str]) -> Output {
    base_cmd(bin())
        .arg("install-hooks")
        .arg(repo)
        .args(extra)
        .output()
        .expect("install-hooks")
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

/// A `git commit` with an optional role; stages nothing (caller stages first).
fn commit(repo: &Path, role: Option<&str>, msg: &str) -> Output {
    let mut cmd = base_cmd("git");
    cmd.arg("-C").arg(repo).args(["commit", "-m", msg]);
    if let Some(role) = role {
        cmd.env("DVANDVA_ROLE", role);
    }
    cmd.output().expect("git commit")
}

const HOOK_REL: &str = ".dvandva/githooks";
const SENTINEL_DEFAULT: &str = "__DVANDVA_DEFAULT__";
const PENDING_ROOT: &str = "__DVANDVA_ROOT_PENDING__";

// ===========================================================================
// (h) Fresh install in a default (unset hooksPath) repo.
// ===========================================================================

// (h) Fresh adopt: exits 0, repoints core.hooksPath, records the default
// sentinel, and records the HEAD adoption baseline at --local scope.
#[test]
fn fresh_install_adopts_default_sentinel() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);

    let out = install(repo, &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert_eq!(cfg_read(repo, "core.hooksPath"), HOOK_REL);
    assert_eq!(cfg_read(repo, "dvandva.priorHooksPath"), SENTINEL_DEFAULT);
    assert_eq!(cfg_read(repo, "dvandva.hooksAdopted"), "true");
    assert_eq!(cfg_read(repo, "dvandva.hookDir"), HOOK_REL);
    assert_eq!(local_cfg(repo, "dvandva.hooksAdoptedAt"), head_sha(repo));
}

// (h RE-KEY) The materialized hooks are SYMLINKS (was: copied .sh files); the
// old shell artifacts (lib/gate/drift-lint) are NOT materialized any more.
#[test]
fn fresh_install_materializes_symlink_wrappers_only() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    assert_eq!(code(&install(repo, &[])), 0);

    let hookdir = repo.join(HOOK_REL);
    for name in ["pre-commit", "prepare-commit-msg"] {
        let link = hookdir.join(name);
        let meta = fs::symlink_metadata(&link).unwrap_or_else(|e| panic!("missing {name}: {e}"));
        assert!(meta.file_type().is_symlink(), "{name} must be a symlink");
    }
    // The removed shell artifacts must NOT be materialized.
    for gone in [
        "dvandva-hook-lib.sh",
        "dvandva-commit-gate.sh",
        "dvandva-drift-lint.sh",
    ] {
        assert!(
            !hookdir.join(gone).exists(),
            "post-port must not materialize {gone}"
        );
    }
}

// (h RE-KEY) selfcheck probe: the materialized wrappers self-identify with the
// positive wiring sentinels — the re-key anchor for the removed .sh presence
// assertions and proof the symlink resolves to the multicall binary.
#[test]
fn fresh_install_selfcheck_probes_report_wired() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    assert_eq!(code(&install(repo, &[])), 0);

    let hookdir = repo.join(HOOK_REL);
    let pre = base_cmd(hookdir.join("pre-commit"))
        .current_dir(repo)
        .env("DVANDVA_HOOK_SELFCHECK", "1")
        .output()
        .unwrap();
    assert_eq!(code(&pre), 0);
    assert!(
        stdout(&pre).contains("DVANDVA_GATE_WIRED"),
        "stdout: {}",
        stdout(&pre)
    );

    let prep = base_cmd(hookdir.join("prepare-commit-msg"))
        .current_dir(repo)
        .env("DVANDVA_HOOK_SELFCHECK", "1")
        .output()
        .unwrap();
    assert_eq!(code(&prep), 0);
    assert!(
        stdout(&prep).contains("DVANDVA_PREPARE_WIRED"),
        "stdout: {}",
        stdout(&prep)
    );
}

// (h) The narration records the materialization mode (symlink in a normal env).
#[test]
fn fresh_install_reports_symlink_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let out = install(repo, &[]);
    assert_eq!(code(&out), 0);
    assert!(
        stdout(&out).contains("symlink"),
        "narration must record the materialization mode: {}",
        stdout(&out)
    );
}

// (h) Idempotent: a second install leaves prior + hooksPath unchanged.
#[test]
fn second_install_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    assert_eq!(code(&install(repo, &[])), 0);

    let out = install(repo, &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert_eq!(cfg_read(repo, "dvandva.priorHooksPath"), SENTINEL_DEFAULT);
    assert_eq!(cfg_read(repo, "core.hooksPath"), HOOK_REL);
    assert!(
        stdout(&out).contains("already adopted"),
        "stdout: {}",
        stdout(&out)
    );
}

// (h) --uninstall restores the default (unset), removes the dir, clears keys.
#[test]
fn uninstall_restores_default_and_clears_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    assert_eq!(code(&install(repo, &[])), 0);

    let out = install(repo, &["--uninstall"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert_eq!(cfg_read(repo, "core.hooksPath"), "");
    assert!(!repo.join(HOOK_REL).exists(), "hook dir must be removed");
    // Keys cleared from BOTH scopes.
    for key in ["dvandva.priorHooksPath", "dvandva.hooksAdopted"] {
        assert_eq!(cfg_read(repo, key), "", "{key} must be cleared");
        assert_eq!(local_cfg(repo, key), "", "{key} --local must be cleared");
    }
}

// (h) A second --uninstall is a safe no-op.
#[test]
fn second_uninstall_is_nothing_to_do() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    assert_eq!(code(&install(repo, &[])), 0);
    assert_eq!(code(&install(repo, &["--uninstall"])), 0);

    let out = install(repo, &["--uninstall"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("nothing to uninstall"),
        "stdout: {}",
        stdout(&out)
    );
}

// ===========================================================================
// (n)/(o) Foreign hooksPath (Husky-like) wrapping.
// ===========================================================================

// (n)/(o) Install over a foreign relative hooksPath records the prior verbatim
// and repoints core.hooksPath to the delegated wrapper dir.
#[test]
fn install_over_foreign_records_prior() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    seed_prior(
        repo,
        ".husky/_",
        "pre-commit",
        "#!/usr/bin/env bash\nexit 0\n",
    );

    let out = install(repo, &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert_eq!(cfg_read(repo, "dvandva.priorHooksPath"), ".husky/_");
    assert_eq!(cfg_read(repo, "core.hooksPath"), HOOK_REL);
}

// (o RE-KEY) Enumerated pass-through stubs for foreign hook names are SYMLINKS
// to the binary (was: generated .sh stubs). pre-commit/prepare-commit-msg are
// the owned wrappers, never double-stubbed.
#[test]
fn install_materializes_symlink_stubs_for_foreign_hooks() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    // A foreign dir with several executable hooks (commit-msg, pre-push).
    write_hook(
        &repo.join(".husky/_/commit-msg"),
        "#!/usr/bin/env bash\nexit 0\n",
    );
    write_hook(
        &repo.join(".husky/_/pre-push"),
        "#!/usr/bin/env bash\nexit 0\n",
    );
    git(repo, &["config", "core.hooksPath", ".husky/_"]);

    assert_eq!(code(&install(repo, &[])), 0);

    let hookdir = repo.join(HOOK_REL);
    for stub in ["commit-msg", "pre-push"] {
        let meta = fs::symlink_metadata(hookdir.join(stub))
            .unwrap_or_else(|e| panic!("missing stub {stub}: {e}"));
        assert!(
            meta.file_type().is_symlink(),
            "{stub} stub must be a symlink"
        );
    }
}

// (o) Install produces ZERO tracked diff (only the gitignored dir + local
// config change).
#[test]
fn install_is_zero_tracked_diff() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    fs::write(repo.join(".gitignore"), ".dvandva/\n").unwrap();
    git(repo, &["add", ".gitignore"]);
    assert!(git(repo, &["commit", "-q", "-m", "ignore"])
        .status
        .success());

    assert_eq!(code(&install(repo, &[])), 0);
    let status = git(repo, &["status", "--porcelain"]);
    assert!(
        String::from_utf8_lossy(&status.stdout).trim().is_empty(),
        "expected clean tree, got: {}",
        String::from_utf8_lossy(&status.stdout)
    );
}

// (o) Idempotent re-install over a foreign prior never self-wraps: prior stays.
#[test]
fn install_over_foreign_idempotent_prior_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    seed_prior(
        repo,
        ".husky/_",
        "pre-commit",
        "#!/usr/bin/env bash\nexit 0\n",
    );
    assert_eq!(code(&install(repo, &[])), 0);

    assert_eq!(code(&install(repo, &[])), 0);
    assert_eq!(
        cfg_read(repo, "dvandva.priorHooksPath"),
        ".husky/_",
        "idempotent re-install must not self-wrap"
    );
}

// (o) Uninstall restores the foreign owner and removes the hook dir.
#[test]
fn uninstall_restores_foreign_hookspath() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    seed_prior(
        repo,
        ".husky/_",
        "pre-commit",
        "#!/usr/bin/env bash\nexit 0\n",
    );
    assert_eq!(code(&install(repo, &[])), 0);

    let out = install(repo, &["--uninstall"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert_eq!(cfg_read(repo, "core.hooksPath"), ".husky/_");
    assert!(!repo.join(HOOK_REL).exists(), "hook dir must be removed");
}

// ===========================================================================
// (p) Absolute prior hooksPath round-trips through record -> wrap -> restore.
// ===========================================================================
#[test]
fn absolute_prior_recorded_and_restored() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let abs_hooks = repo.join("abs-hooks");
    write_hook(
        &abs_hooks.join("pre-commit"),
        "#!/usr/bin/env bash\nexit 0\n",
    );
    let abs = abs_hooks.to_str().unwrap();
    git(repo, &["config", "core.hooksPath", abs]);

    assert_eq!(code(&install(repo, &[])), 0);
    assert_eq!(
        cfg_read(repo, "dvandva.priorHooksPath"),
        abs,
        "absolute prior must be recorded verbatim"
    );

    let out = install(repo, &["--uninstall"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    // Foreign --local value is preserved as the recorded prior on restore.
    assert_eq!(local_cfg(repo, "core.hooksPath"), abs);
}

// ===========================================================================
// (m) Unborn repo: the installer records the pending root baseline sentinel.
// ===========================================================================
#[test]
fn unborn_repo_records_pending_baseline() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo_unborn(repo);

    let out = install(repo, &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert_eq!(local_cfg(repo, "dvandva.hooksAdoptedAt"), PENDING_ROOT);
    assert_eq!(local_cfg(repo, "dvandva.hooksAdoptedAtInclusive"), "true");
}

// ===========================================================================
// End-to-end: the materialized symlink chain actually fires.
// ===========================================================================

// A prior pre-commit that FAILS blocks the commit through the wrapper's
// gate-pass -> exec-prior chain (no active baton => gate is a no-op).
#[test]
fn installed_chain_blocks_on_failing_prior_precommit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    seed_prior(
        repo,
        ".prior",
        "pre-commit",
        "#!/usr/bin/env bash\nexit 1\n",
    );
    assert_eq!(code(&install(repo, &[])), 0);

    fs::write(repo.join("blocked.txt"), "x").unwrap();
    git(repo, &["add", "blocked.txt"]);
    let out = commit(repo, None, "should be blocked");
    assert_ne!(
        code(&out),
        0,
        "failing prior pre-commit must block the commit"
    );
    assert_eq!(head_subject(repo), "initial", "no commit should be created");
}

// A passing prior pre-commit fires via the exec chain and the commit succeeds.
#[test]
fn installed_chain_delegates_to_passing_prior_precommit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let log = repo.join("hook.log");
    seed_prior(
        repo,
        ".prior",
        "pre-commit",
        &format!(
            "#!/usr/bin/env bash\necho PRIOR_PRECOMMIT_FIRED >> \"{}\"\nexit 0\n",
            log.display()
        ),
    );
    assert_eq!(code(&install(repo, &[])), 0);

    fs::write(repo.join("ok.txt"), "x").unwrap();
    git(repo, &["add", "ok.txt"]);
    let out = commit(repo, None, "ok commit");
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("PRIOR_PRECOMMIT_FIRED"),
        "prior pre-commit must fire via the exec chain"
    );
}

// A pass-through stub delegates a foreign hook (commit-msg) to the prior chain.
#[test]
fn installed_passthrough_stub_fires_prior_commit_msg() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let log = repo.join("hook.log");
    seed_prior(
        repo,
        ".prior",
        "commit-msg",
        &format!(
            "#!/usr/bin/env bash\necho PRIOR_COMMITMSG_FIRED >> \"{}\"\nexit 0\n",
            log.display()
        ),
    );
    assert_eq!(code(&install(repo, &[])), 0);

    fs::write(repo.join("c.txt"), "x").unwrap();
    git(repo, &["add", "c.txt"]);
    let out = commit(repo, None, "commit-msg delegation");
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("PRIOR_COMMITMSG_FIRED"),
        "commit-msg pass-through stub must delegate to the prior hook"
    );
}

// ===========================================================================
// CLI / usage contract.
// ===========================================================================
#[test]
fn unknown_option_exits_2() {
    let out = base_cmd(bin())
        .args(["install-hooks", "--bogus"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
    assert!(
        stderr(&out).contains("unknown option"),
        "stderr: {}",
        stderr(&out)
    );
}

#[test]
fn too_many_positional_exits_2() {
    let out = base_cmd(bin())
        .args(["install-hooks", "one", "two"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
    assert!(
        stderr(&out).contains("too many positional"),
        "stderr: {}",
        stderr(&out)
    );
}

#[test]
fn not_a_git_repo_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    let out = install(tmp.path(), &[]);
    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("not a git repository"),
        "stderr: {}",
        stderr(&out)
    );
}

#[test]
fn force_flag_is_accepted_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let out = install(repo, &["--force"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert_eq!(cfg_read(repo, "core.hooksPath"), HOOK_REL);
}

// ===========================================================================
// (u) Linked-worktree hooksPath isolation (silent fail-open guard).
//
// Installing in ONE worktree must never leave a SIBLING worktree pointing at a
// .dvandva/githooks dir it lacks. Because the adopted state is written at
// --worktree scope, the sibling keeps its default hooks and its prior chain
// fires. Behavioral proof: install in main, commit in the linked worktree, and
// the common .git/hooks prior must still fire (no silent bypass).
// ===========================================================================
#[test]
fn linked_worktree_install_does_not_bypass_sibling() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("main");
    let linked = tmp.path().join("linked");
    init_repo(&main);

    let log = tmp.path().join("common-prior.log");
    write_hook(
        &main.join(".git/hooks/pre-commit"),
        &format!(
            "#!/usr/bin/env bash\necho COMMON_PRIOR_FIRED >> \"{}\"\nexit 0\n",
            log.display()
        ),
    );

    let added = git(
        &main,
        &[
            "worktree",
            "add",
            "-b",
            "u-rev-branch",
            linked.to_str().unwrap(),
        ],
    );
    if !added.status.success() {
        // Linked-worktree fixture infeasible in this environment; skip.
        return;
    }

    // The Dvandva run lives in the main worktree; adopt there.
    assert_eq!(code(&install(&main, &[])), 0);

    // A commit in the LINKED worktree must not be silently bypassed.
    fs::write(linked.join("l.txt"), "x").unwrap();
    git(&linked, &["add", "l.txt"]);
    let out = commit(&linked, None, "linked commit after main install");
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("COMMON_PRIOR_FIRED"),
        "linked prior pre-commit BYPASSED after main-worktree install"
    );

    let _ = git(
        &main,
        &["worktree", "remove", "--force", linked.to_str().unwrap()],
    );
}
