//! Integration tests for the git-hook family: the commit gate, the
//! `pre-commit` / `prepare-commit-msg` symlink handlers, prior-hook delegation,
//! and the checkpoint stamp. Ports the wrapper/gate/stamp cases of
//! `scripts/test-dvandva-commit-gate.sh`.
//!
//! Post-port there are no shell wrappers: git invokes a SYMLINK named after the
//! hook (`pre-commit`, `prepare-commit-msg`, ...) that points at the `dvandva`
//! multicall binary; `argv[0]` carries the hook name. Gate-only cases invoke
//! `dvandva commit-gate` directly; wrapper cases build a `.dvandva/githooks`
//! fixture with the test-built binary symlinked in.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

/// A command with an isolated git environment and no ambient Dvandva vars.
fn base_cmd<P: AsRef<std::ffi::OsStr>>(program: P) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_HOOK_SELFCHECK")
        .env_remove("DVANDVA_COMMIT_GATE_PATHS");
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

fn make_baton(path: &Path, status: &str, assignee: &str, checkpoint: i64, active_roles: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let json = format!(
        "{{\n  \"schema\": \"dvandva.baton.v1\",\n  \"status\": \"{status}\",\n  \
         \"assignee\": \"{assignee}\",\n  \"checkpoint\": {checkpoint},\n  \
         \"active_roles\": {active_roles}\n}}\n"
    );
    fs::write(path, json).unwrap();
}

/// A v2 baton declaring a `changed_paths` scope plus an optional `profile`,
/// for the S4-T9 staged-path crosscheck tests.
fn make_baton_with_paths(
    path: &Path,
    status: &str,
    assignee: &str,
    active_roles: &str,
    changed_paths: &[&str],
    profile: &str,
) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let paths_json = changed_paths
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect::<Vec<_>>()
        .join(",");
    let json = format!(
        "{{\n  \"schema\": \"dvandva.baton.v2\",\n  \"status\": \"{status}\",\n  \
         \"assignee\": \"{assignee}\",\n  \"checkpoint\": 1,\n  \
         \"active_roles\": {active_roles},\n  \"profile\": \"{profile}\",\n  \
         \"changed_paths\": [{paths_json}]\n}}\n"
    );
    fs::write(path, json).unwrap();
}

fn write_hook(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}

/// Symlink the built binary into `<repo>/<rel_hookdir>` under the given hook
/// names and point `core.hooksPath` at that dir. Returns the hook dir path.
fn install_symlink_hooks(repo: &Path, rel_hookdir: &str, names: &[&str]) -> PathBuf {
    let hookdir = repo.join(rel_hookdir);
    fs::create_dir_all(&hookdir).unwrap();
    for name in names {
        let link = hookdir.join(name);
        let _ = fs::remove_file(&link);
        symlink(bin(), &link).unwrap();
    }
    git(repo, &["config", "core.hooksPath", rel_hookdir]);
    hookdir
}

/// Run `dvandva commit-gate` in `repo` with an optional role.
fn run_gate(repo: &Path, role: Option<&str>) -> Output {
    run_gate_with_paths_mode(repo, role, None)
}

/// Run `dvandva commit-gate` in `repo` with an optional role and an optional
/// `DVANDVA_COMMIT_GATE_PATHS` override (S4-T9).
fn run_gate_with_paths_mode(repo: &Path, role: Option<&str>, paths_mode: Option<&str>) -> Output {
    let mut cmd = base_cmd(bin());
    cmd.arg("commit-gate").current_dir(repo);
    if let Some(role) = role {
        cmd.env("DVANDVA_ROLE", role);
    }
    if let Some(mode) = paths_mode {
        cmd.env("DVANDVA_COMMIT_GATE_PATHS", mode);
    }
    cmd.output().expect("commit-gate")
}

/// `git commit -m <msg>` with an optional role and extra args.
fn commit(repo: &Path, role: Option<&str>, msg: &str, extra: &[&str]) -> Output {
    let mut cmd = base_cmd("git");
    cmd.arg("-C")
        .arg(repo)
        .args(["commit", "-m", msg])
        .args(extra);
    if let Some(role) = role {
        cmd.env("DVANDVA_ROLE", role);
    }
    cmd.output().expect("git commit")
}

fn head_body(repo: &Path) -> String {
    let out = git(repo, &["show", "-s", "--format=%B", "HEAD"]);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn head_subject(repo: &Path) -> String {
    let out = git(repo, &["log", "-1", "--format=%s"]);
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn has_trailer(repo: &Path, checkpoint: i64) -> bool {
    let want = format!("Dvandva-Checkpoint: {checkpoint}");
    head_body(repo).lines().any(|l| l == want)
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ===========================================================================
// Gate cases (invoked via `dvandva commit-gate`)
// ===========================================================================

// (a) No .dvandva directory -> gate exits 0.
#[test]
fn gate_no_dvandva_exits_0() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let out = run_gate(repo, None);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
}

// (c) Active baton + wrong role -> gate exits 1 with blocked message.
#[test]
fn gate_wrong_role_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        5,
        "[]",
    );
    let out = run_gate(repo, Some("prativadi"));
    assert_eq!(code(&out), 1);
    let err = stderr(&out);
    assert!(err.contains("DVANDVA_GATE blocked"), "err: {err}");
    assert!(err.contains("assignee=vadi"), "err: {err}");
}

// (d) DVANDVA_ROLE unset + active baton -> gate exits 1.
#[test]
fn gate_role_unset_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        5,
        "[]",
    );
    let out = run_gate(repo, None);
    assert_eq!(code(&out), 1);
    let err = stderr(&out);
    assert!(err.contains("DVANDVA_ROLE is unset"), "err: {err}");
    assert!(err.contains("checkpoint=5"), "err: {err}");
}

// (e) Terminal statuses -> gate exits 0 (inactive).
#[test]
fn gate_terminal_done_exits_0() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(&repo.join(".dvandva/baton.json"), "done", "human", 10, "[]");
    assert_eq!(code(&run_gate(repo, Some("vadi"))), 0);
}

#[test]
fn gate_terminal_human_question_exits_0() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "human_question",
        "human",
        3,
        "[]",
    );
    assert_eq!(code(&run_gate(repo, Some("vadi"))), 0);
}

#[test]
fn gate_terminal_human_decision_exits_0() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "human_decision",
        "human",
        4,
        "[]",
    );
    assert_eq!(code(&run_gate(repo, Some("prativadi"))), 0);
}

// S2-T1: `abandoned` is a terminal status too — a run that only has an
// abandoned baton is inactive for gating purposes.
#[test]
fn gate_terminal_abandoned_exits_0() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "abandoned",
        "human",
        6,
        "[]",
    );
    assert_eq!(code(&run_gate(repo, Some("vadi"))), 0);
}

// (f) Two active batons -> gate exits 1 (ambiguous).
#[test]
fn gate_two_active_ambiguous() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        5,
        "[]",
    );
    make_baton(
        &repo.join(".dvandva/runs/run-a/baton.json"),
        "spec_drafting",
        "vadi",
        0,
        "[]",
    );
    let out = run_gate(repo, Some("vadi"));
    assert_eq!(code(&out), 1);
    let err = stderr(&out);
    assert!(err.contains("ambiguous"), "err: {err}");
    assert!(err.contains("2 active batons"), "err: {err}");
}

// (f) Malformed JSON must fail closed.
#[test]
fn gate_malformed_legacy_fails_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    fs::create_dir_all(repo.join(".dvandva")).unwrap();
    fs::write(repo.join(".dvandva/baton.json"), "{ bad json\n").unwrap();
    let out = run_gate(repo, Some("vadi"));
    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("malformed baton"),
        "err: {}",
        stderr(&out)
    );
}

#[test]
fn gate_malformed_run_scoped_fails_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    fs::create_dir_all(repo.join(".dvandva/runs/run-bad")).unwrap();
    fs::write(
        repo.join(".dvandva/runs/run-bad/baton.json"),
        "{ bad json\n",
    )
    .unwrap();
    let out = run_gate(repo, Some("vadi"));
    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("malformed baton"),
        "err: {}",
        stderr(&out)
    );
}

// (g) active_roles membership allows the role.
#[test]
fn gate_team_active_roles_vadi() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "parallel_implementing",
        "team",
        8,
        "[\"vadi\",\"prativadi\"]",
    );
    assert_eq!(code(&run_gate(repo, Some("vadi"))), 0);
}

#[test]
fn gate_team_active_roles_prativadi() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "cross_review",
        "team",
        9,
        "[\"vadi\",\"prativadi\"]",
    );
    assert_eq!(code(&run_gate(repo, Some("prativadi"))), 0);
}

#[test]
fn gate_run_scoped_team_vadi() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/runs/run-b/baton.json"),
        "cross_fixing",
        "team",
        12,
        "[\"vadi\",\"prativadi\"]",
    );
    assert_eq!(code(&run_gate(repo, Some("vadi"))), 0);
}

#[test]
fn gate_run_scoped_scalar_match_and_wrong() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/runs/run-c/baton.json"),
        "phase_fixing",
        "vadi",
        13,
        "[]",
    );
    assert_eq!(code(&run_gate(repo, Some("vadi"))), 0);
    let out = run_gate(repo, Some("prativadi"));
    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("assignee=vadi"),
        "err: {}",
        stderr(&out)
    );
}

// Role must be vadi|prativadi when an active baton exists.
#[test]
fn gate_invalid_role_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        5,
        "[]",
    );
    let out = run_gate(repo, Some("bystander"));
    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("is not a valid role"),
        "err: {}",
        stderr(&out)
    );
}

// ===========================================================================
// S4-T9: commit-gate staged-path crosscheck.
// ===========================================================================

// A commit whose staged paths are entirely within the baton's declared
// changed_paths passes.
#[test]
fn gate_paths_within_allowed_commits_pass() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton_with_paths(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        "[]",
        &["allowed.txt"],
        "standard",
    );
    fs::write(repo.join("allowed.txt"), "x").unwrap();
    git(repo, &["add", "allowed.txt"]);

    assert_eq!(code(&run_gate(repo, Some("vadi"))), 0);
}

// A staged path outside the baton's allowed set blocks the commit and lists
// the offender.
#[test]
fn gate_paths_offender_blocks_with_listing() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton_with_paths(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        "[]",
        &["allowed.txt"],
        "standard",
    );
    fs::write(repo.join("intruder.txt"), "x").unwrap();
    git(repo, &["add", "intruder.txt"]);

    let out = run_gate(repo, Some("vadi"));
    assert_eq!(code(&out), 1);
    let err = stderr(&out);
    assert!(
        err.contains("outside the baton's allowed set"),
        "err: {err}"
    );
    assert!(err.contains("intruder.txt"), "err: {err}");
}

// DVANDVA_COMMIT_GATE_PATHS=warn prints the offenders but allows the commit.
#[test]
fn gate_paths_warn_mode_allows_with_output() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton_with_paths(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        "[]",
        &["allowed.txt"],
        "standard",
    );
    fs::write(repo.join("intruder.txt"), "x").unwrap();
    git(repo, &["add", "intruder.txt"]);

    let out = run_gate_with_paths_mode(repo, Some("vadi"), Some("warn"));
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("intruder.txt"), "err: {err}");
}

// DVANDVA_COMMIT_GATE_PATHS=off skips the crosscheck entirely.
#[test]
fn gate_paths_off_mode_skips() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton_with_paths(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        "[]",
        &["allowed.txt"],
        "standard",
    );
    fs::write(repo.join("intruder.txt"), "x").unwrap();
    git(repo, &["add", "intruder.txt"]);

    let out = run_gate_with_paths_mode(repo, Some("vadi"), Some("off"));
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
}

// `.dvandva/` and `superpowers/` paths are always exempt from the crosscheck.
#[test]
fn gate_paths_exemptions_respected() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton_with_paths(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        "[]",
        &["allowed.txt"],
        "standard",
    );
    fs::create_dir_all(repo.join("superpowers")).unwrap();
    fs::write(repo.join("superpowers/plan.html"), "x").unwrap();
    git(
        repo,
        &["add", "superpowers/plan.html", ".dvandva/baton.json"],
    );

    assert_eq!(code(&run_gate(repo, Some("vadi"))), 0);
}

// An offender matching the hard-path reminder set, while the baton's
// effective profile is not `full`, adds a recompute-floor reminder line.
#[test]
fn gate_paths_hard_path_reminder_fires() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton_with_paths(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        "[]",
        &["allowed.txt"],
        "standard",
    );
    fs::create_dir_all(repo.join("rust/dvandva/src")).unwrap();
    fs::write(repo.join("rust/dvandva/src/extra.rs"), "x").unwrap();
    git(repo, &["add", "rust/dvandva/src/extra.rs"]);

    let out = run_gate(repo, Some("vadi"));
    assert_eq!(code(&out), 1);
    let err = stderr(&out);
    assert!(err.contains("recompute the profile floor"), "err: {err}");
}

// ===========================================================================
// Wrapper / stamp cases (symlink fixtures + git commit)
// ===========================================================================

// (b) Active baton + matching role -> commit allowed + trailer stamped.
#[test]
fn precommit_match_role_commits_and_stamps() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        7,
        "[]",
    );
    install_symlink_hooks(
        repo,
        ".dvandva/githooks",
        &["pre-commit", "prepare-commit-msg"],
    );

    fs::write(repo.join("file.txt"), "x").unwrap();
    git(repo, &["add", "file.txt"]);
    let out = commit(repo, Some("vadi"), "vadi commit", &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(has_trailer(repo, 7), "body: {}", head_body(repo));
}

// (h) Selfcheck probes: active wrappers self-identify.
#[test]
fn selfcheck_precommit_prints_gate_wired() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let hookdir = install_symlink_hooks(repo, ".dvandva/githooks", &["pre-commit"]);
    let out = base_cmd(hookdir.join("pre-commit"))
        .current_dir(repo)
        .env("DVANDVA_HOOK_SELFCHECK", "1")
        .output()
        .unwrap();
    assert_eq!(code(&out), 0);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("DVANDVA_GATE_WIRED:"),
        "stdout: {stdout}"
    );
}

#[test]
fn selfcheck_prepare_prints_prepare_wired() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let hookdir = install_symlink_hooks(repo, ".dvandva/githooks", &["prepare-commit-msg"]);
    let out = base_cmd(hookdir.join("prepare-commit-msg"))
        .current_dir(repo)
        .env("DVANDVA_HOOK_SELFCHECK", "1")
        .output()
        .unwrap();
    assert_eq!(code(&out), 0);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("DVANDVA_PREPARE_WIRED:"),
        "stdout: {stdout}"
    );
}

// (j) --no-verify with role unset: commit succeeds, and prepare-commit-msg
// (which --no-verify does NOT bypass) does not stamp a trailer.
#[test]
fn noverify_role_unset_no_stamp() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        14,
        "[]",
    );
    install_symlink_hooks(
        repo,
        ".dvandva/githooks",
        &["pre-commit", "prepare-commit-msg"],
    );

    fs::write(repo.join("noverify.txt"), "x").unwrap();
    git(repo, &["add", "noverify.txt"]);
    let out = commit(repo, None, "bypass without role", &["--no-verify"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        !head_body(repo)
            .lines()
            .any(|l| l.starts_with("Dvandva-Checkpoint:")),
        "body unexpectedly stamped: {}",
        head_body(repo)
    );
}

// (k) --no-verify must not let prepare-commit-msg stamp under multi-run
// ambiguity: two active run-scoped batons -> prepare blocks the commit.
#[test]
fn prepare_ambiguous_noverify_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/runs/run-a/baton.json"),
        "implementing",
        "vadi",
        21,
        "[]",
    );
    make_baton(
        &repo.join(".dvandva/runs/run-b/baton.json"),
        "spec_drafting",
        "vadi",
        22,
        "[]",
    );
    install_symlink_hooks(
        repo,
        ".dvandva/githooks",
        &["pre-commit", "prepare-commit-msg"],
    );

    fs::write(repo.join("ambiguous.txt"), "x").unwrap();
    git(repo, &["add", "ambiguous.txt"]);
    let out = commit(repo, Some("vadi"), "ambiguous no-verify", &["--no-verify"]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("ambiguous active runs"),
        "err: {}",
        stderr(&out)
    );
    assert_eq!(head_subject(repo), "initial", "no commit should be created");
}

// (n) Wrong-role commit is blocked by the gate BEFORE the prior chain runs.
#[test]
fn wrong_role_blocked_before_prior() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        5,
        "[]",
    );
    install_symlink_hooks(
        repo,
        ".dvandva/githooks",
        &["pre-commit", "prepare-commit-msg"],
    );

    let log = repo.join("hook.log");
    write_hook(
        &repo.join(".prior/pre-commit"),
        &format!(
            "#!/usr/bin/env bash\necho PRIOR_PRECOMMIT_FIRED >> \"{}\"\nexit 0\n",
            log.display()
        ),
    );
    git(
        repo,
        &["config", "--local", "dvandva.priorHooksPath", ".prior"],
    );

    fs::write(repo.join("wrong.txt"), "x").unwrap();
    git(repo, &["add", "wrong.txt"]);
    let out = commit(repo, Some("prativadi"), "wrong role", &[]);
    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("DVANDVA_GATE blocked"),
        "err: {}",
        stderr(&out)
    );
    assert!(!log.exists(), "prior pre-commit fired despite gate block");
}

// (o) Allowed role: gate passes -> prior pre-commit fires -> trailer stamped.
#[test]
fn allowed_role_delegates_and_stamps() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        8,
        "[]",
    );
    install_symlink_hooks(
        repo,
        ".dvandva/githooks",
        &["pre-commit", "prepare-commit-msg"],
    );

    let log = repo.join("hook.log");
    write_hook(
        &repo.join(".prior/pre-commit"),
        &format!(
            "#!/usr/bin/env bash\necho PRIOR_PRECOMMIT_FIRED >> \"{}\"\nexit 0\n",
            log.display()
        ),
    );
    git(
        repo,
        &["config", "--local", "dvandva.priorHooksPath", ".prior"],
    );

    fs::write(repo.join("feat.txt"), "x").unwrap();
    git(repo, &["add", "feat.txt"]);
    let out = commit(repo, Some("vadi"), "vadi feature", &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("PRIOR_PRECOMMIT_FIRED"),
        "prior pre-commit did not fire"
    );
    assert!(has_trailer(repo, 8), "body: {}", head_body(repo));
}

// (B4 regression) A PRIOR pre-commit that exits non-zero must propagate that
// EXACT code when the Dvandva pre-commit symlink is invoked DIRECTLY (not
// through `git commit`). Every other pre-commit prior-chain case above uses
// an exit-0 prior, so a regression swapping the `exec()` process replacement
// for a `status()`-then-`return 0` implementation would go uncaught.
#[test]
fn direct_invocation_propagates_prior_nonzero_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let hookdir = install_symlink_hooks(repo, ".dvandva/githooks", &["pre-commit"]);

    write_hook(
        &repo.join(".prior/pre-commit"),
        "#!/usr/bin/env bash\nexit 42\n",
    );
    git(
        repo,
        &["config", "--local", "dvandva.priorHooksPath", ".prior"],
    );

    // No active baton -> the gate is a no-op and delegates straight to the
    // prior chain.
    let out = base_cmd(hookdir.join("pre-commit"))
        .current_dir(repo)
        .output()
        .unwrap();
    assert_eq!(code(&out), 42, "stderr: {}", stderr(&out));
}

// (o stub) Pass-through hook forwards argv unchanged and propagates exit code.
#[test]
fn passthrough_forwards_argv_and_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let hookdir = install_symlink_hooks(repo, ".dvandva/githooks", &["commit-msg"]);

    let probe = repo.join("stub-probe.log");
    write_hook(
        &repo.join(".prior/commit-msg"),
        &format!(
            "#!/usr/bin/env bash\necho \"ARGV=[$*]\" > \"{}\"\nexit 7\n",
            probe.display()
        ),
    );
    git(
        repo,
        &["config", "--local", "dvandva.priorHooksPath", ".prior"],
    );

    let out = base_cmd(hookdir.join("commit-msg"))
        .current_dir(repo)
        .args([".git/COMMIT_EDITMSG", "extra"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 7, "stderr: {}", stderr(&out));
    let logged = fs::read_to_string(&probe).unwrap_or_default();
    assert!(
        logged.contains("ARGV=[.git/COMMIT_EDITMSG extra]"),
        "argv not forwarded: {logged}"
    );
}

// Pass-through with no resolvable prior hook exits 0.
#[test]
fn passthrough_no_prior_exits_0() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let hookdir = install_symlink_hooks(repo, ".dvandva/githooks", &["post-commit"]);
    let out = base_cmd(hookdir.join("post-commit"))
        .current_dir(repo)
        .output()
        .unwrap();
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
}

// (p) Absolute prior hooksPath is used verbatim and delegated to.
#[test]
fn absolute_prior_precommit_delegated() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    install_symlink_hooks(
        repo,
        ".dvandva/githooks",
        &["pre-commit", "prepare-commit-msg"],
    );

    let abs_hooks = repo.join("abs-hooks");
    let log = repo.join("hook.log");
    write_hook(
        &abs_hooks.join("pre-commit"),
        &format!(
            "#!/usr/bin/env bash\necho ABS_PRECOMMIT_FIRED >> \"{}\"\nexit 0\n",
            log.display()
        ),
    );
    git(
        repo,
        &[
            "config",
            "--local",
            "dvandva.priorHooksPath",
            abs_hooks.to_str().unwrap(),
        ],
    );

    fs::write(repo.join("p.txt"), "x").unwrap();
    git(repo, &["add", "p.txt"]);
    // No active baton -> gate is a no-op; commit proceeds and delegates.
    let out = commit(repo, None, "p commit", &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("ABS_PRECOMMIT_FIRED"),
        "absolute prior pre-commit not fired"
    );
}

// (r) Self-loop guard: prior hooksPath pointing at our own dir must not recurse.
#[test]
fn self_loop_prior_no_recursion() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    install_symlink_hooks(
        repo,
        ".dvandva/githooks",
        &["pre-commit", "prepare-commit-msg"],
    );
    // Poison the recorded prior to point at our own hook dir.
    git(
        repo,
        &[
            "config",
            "--local",
            "dvandva.priorHooksPath",
            ".dvandva/githooks",
        ],
    );

    fs::write(repo.join("r.txt"), "x").unwrap();
    git(repo, &["add", "r.txt"]);

    let mut child = base_cmd("git")
        .arg("-C")
        .arg(repo)
        .args(["commit", "-m", "self loop"])
        .env("DVANDVA_ROLE", "vadi")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(20);
    let timed_out = loop {
        if child.try_wait().unwrap().is_some() {
            break false;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break true;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(
        !timed_out,
        "self-loop prior caused infinite recursion (hang)"
    );
}

// prepare-commit-msg skips the stamp for merge/squash commit sources.
#[test]
fn prepare_merge_source_skips_stamp() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        9,
        "[]",
    );
    let hookdir = install_symlink_hooks(repo, ".dvandva/githooks", &["prepare-commit-msg"]);

    let msg = repo.join("MERGE_MSG");
    fs::write(&msg, "merge message\n").unwrap();
    let out = base_cmd(hookdir.join("prepare-commit-msg"))
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .args([msg.to_str().unwrap(), "merge"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let contents = fs::read_to_string(&msg).unwrap();
    assert!(
        !contents.contains("Dvandva-Checkpoint:"),
        "merge source must not be stamped: {contents}"
    );
}

// prepare-commit-msg does not double-stamp when a trailer is already present.
#[test]
fn prepare_trailer_already_present_skips() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        9,
        "[]",
    );
    let hookdir = install_symlink_hooks(repo, ".dvandva/githooks", &["prepare-commit-msg"]);

    let msg = repo.join("COMMIT_MSG");
    fs::write(&msg, "subject\n\nDvandva-Checkpoint: 3\n").unwrap();
    let out = base_cmd(hookdir.join("prepare-commit-msg"))
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .args([msg.to_str().unwrap(), "message"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let contents = fs::read_to_string(&msg).unwrap();
    let count = contents.matches("Dvandva-Checkpoint:").count();
    assert_eq!(count, 1, "trailer must not be duplicated: {contents}");
    assert!(contents.contains("Dvandva-Checkpoint: 3"));
    assert!(!contents.contains("Dvandva-Checkpoint: 9"));
}

// prepare-commit-msg propagates a non-zero exit from the prior hook and does
// not stamp when the prior chain fails.
#[test]
fn prepare_delegate_propagates_prior_nonzero() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    make_baton(
        &repo.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        9,
        "[]",
    );
    let hookdir = install_symlink_hooks(repo, ".dvandva/githooks", &["prepare-commit-msg"]);
    write_hook(
        &repo.join(".prior/prepare-commit-msg"),
        "#!/usr/bin/env bash\nexit 3\n",
    );
    git(
        repo,
        &["config", "--local", "dvandva.priorHooksPath", ".prior"],
    );

    let msg = repo.join("COMMIT_MSG");
    fs::write(&msg, "subject\n").unwrap();
    let out = base_cmd(hookdir.join("prepare-commit-msg"))
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .args([msg.to_str().unwrap(), "message"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 3, "stderr: {}", stderr(&out));
    assert!(
        !fs::read_to_string(&msg)
            .unwrap()
            .contains("Dvandva-Checkpoint:"),
        "must not stamp when prior chain fails"
    );
}

// (t) Linked-worktree delegation: the gate fires in a linked worktree and the
// prior hook in the main .git/hooks fires via --git-common-dir resolution.
#[test]
fn linked_worktree_prior_via_common_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("main");
    let linked = tmp.path().join("linked");
    init_repo(&main);

    let log = tmp.path().join("prior.log");
    write_hook(
        &main.join(".git/hooks/pre-commit"),
        &format!(
            "#!/usr/bin/env bash\necho PRIOR_PRECOMMIT_FIRED >> \"{}\"\nexit 0\n",
            log.display()
        ),
    );

    let added = git(
        &main,
        &[
            "worktree",
            "add",
            "-b",
            "t-linked-branch",
            linked.to_str().unwrap(),
        ],
    );
    if !added.status.success() {
        // Linked worktree fixture infeasible in this environment; skip.
        return;
    }

    make_baton(
        &linked.join(".dvandva/baton.json"),
        "implementing",
        "vadi",
        20,
        "[]",
    );
    // Absolute hooksPath so the relative-vs-cwd resolution is unambiguous in
    // the shared config visible to both worktrees. Leave priorHooksPath unset
    // so the default (--git-common-dir/hooks) resolution is exercised.
    let hookdir = linked.join(".dvandva/githooks");
    fs::create_dir_all(&hookdir).unwrap();
    for name in ["pre-commit", "prepare-commit-msg"] {
        symlink(bin(), hookdir.join(name)).unwrap();
    }
    git(
        &linked,
        &["config", "core.hooksPath", hookdir.to_str().unwrap()],
    );

    // Wrong role blocked in the linked worktree.
    fs::write(linked.join("t1.txt"), "x").unwrap();
    git(&linked, &["add", "t1.txt"]);
    let out = commit(&linked, Some("prativadi"), "wrong role in linked wt", &[]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("DVANDVA_GATE blocked"),
        "err: {}",
        stderr(&out)
    );

    // Correct role: commit succeeds and the main .git/hooks prior fires.
    fs::write(linked.join("t2.txt"), "x").unwrap();
    git(&linked, &["add", "t2.txt"]);
    let out = commit(&linked, Some("vadi"), "vadi in linked wt", &[]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("PRIOR_PRECOMMIT_FIRED"),
        "prior hook at main .git/hooks did not fire via --git-common-dir"
    );

    let _ = git(
        &main,
        &["worktree", "remove", "--force", linked.to_str().unwrap()],
    );
}
