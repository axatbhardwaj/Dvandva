//! `install-dvandva-hooks` port — the delegating, reversible work-gate
//! installer (plan: superpowers/plans/2026-07-02-rust-port-completion.html,
//! Task B6).
//!
//! Dvandva points `core.hooksPath` at its OWN gitignored hook dir
//! (`.dvandva/githooks`), records the prior owner (Husky / lefthook / default),
//! and the materialized hooks run the Dvandva gate then `exec` the prior chain
//! — so foreign hooks keep firing with ZERO tracked diff (only a local
//! `.git/config` value changes, reversible on `--uninstall`).
//!
//! DESIGN DECISION D2 — the one deliberate change from the shell installer:
//! post-port there are NO shell files to copy. Instead the installer
//! materializes `.dvandva/githooks/pre-commit` and `prepare-commit-msg` as
//! SYMLINKS to the running binary (`std::env::current_exe`); when `symlink(2)`
//! fails it falls back to hard-COPYING the binary and records which mode was
//! used. For every OTHER canonical git-hook name that exists executable in the
//! prior hooks dir it materializes a symlink pass-through stub of that name, so
//! `argv[0]` dispatch delegates to the prior chain. The old
//! `dvandva-hook-lib.sh` / `dvandva-commit-gate.sh` / `dvandva-drift-lint.sh`
//! materialization is gone — that logic now lives in the binary.
//!
//! The config choreography (worktree-scoped adoption keys, prior-owner
//! recording, adoption baseline, functional probe with full rollback on a
//! fresh adopt, idempotent refresh, `--uninstall` restore) mirrors the shell
//! installer exactly.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::gitcfg::{self, git, git_stdout, repo_toplevel};

/// Prior sentinel recorded when there was no prior hooksPath (or it was self).
const SENTINEL_DEFAULT: &str = "__DVANDVA_DEFAULT__";
/// Adoption-baseline sentinel recorded when the repo has no HEAD commit yet.
const PENDING_ROOT_BASELINE: &str = "__DVANDVA_ROOT_PENDING__";
/// The gitignored, per-repo Dvandva hook dir, relative to the repo root.
const HOOK_REL: &str = ".dvandva/githooks";

/// Canonical client-side git hook names enumerated for pass-through stubs.
/// Mirrors `main.rs`'s `GIT_HOOK_NAMES` (a binary-private const) and the shell
/// installer's `GIT_HOOK_NAMES` array.
const GIT_HOOK_NAMES: [&str; 24] = [
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-receive",
    "update",
    "proc-receive",
    "post-receive",
    "post-update",
    "reference-transaction",
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
    "fsmonitor-watchman",
    "post-index-change",
];

/// How the `pre-commit` / `prepare-commit-msg` wrappers were materialized.
enum MaterializeMode {
    /// Symlinked to the running binary (the normal path).
    Symlink,
    /// `symlink(2)` failed (carried error); the binary was hard-copied instead.
    Copy(String),
}

/// Install, refresh, or uninstall the delegating Dvandva hook chain in `repo`.
///
/// `repo` may be any path inside the work tree; the repo root is resolved via
/// `git rev-parse --show-toplevel`. Exit codes mirror the shell installer:
/// 0 ok / uninstall / nothing-to-do · 1 not-git / missing-binary / probe-failed.
pub fn run_install(repo: &Path, uninstall: bool) -> i32 {
    let Some(root) = repo_toplevel(repo) else {
        eprintln!(
            "install-dvandva-hooks: not a git repository: {}",
            repo.display()
        );
        return 1;
    };
    let hook_dir_abs = root.join(HOOK_REL);

    if uninstall {
        return uninstall_hooks(&root, &hook_dir_abs);
    }

    let binary = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("install-dvandva-hooks: cannot resolve running binary: {err}");
            return 1;
        }
    };

    if let Err(err) = fs::create_dir_all(&hook_dir_abs) {
        eprintln!(
            "install-dvandva-hooks: cannot create hook dir {}: {err}",
            hook_dir_abs.display()
        );
        return 1;
    }

    let mode = match materialize_wrappers(&binary, &hook_dir_abs) {
        Ok(mode) => mode,
        Err(err) => {
            eprintln!("install-dvandva-hooks: cannot materialize hook wrappers: {err}");
            return 1;
        }
    };
    report_mode(&binary, &mode);

    let adopted = cfg_get(&root, "dvandva.hooksAdopted");
    let current = cfg_get(&root, "core.hooksPath");

    // Double-wrap guard: already adopted + pointing at our dir -> refresh +
    // reprobe. Never re-record the prior (would self-loop).
    if adopted == "true" && current == HOOK_REL {
        pin_state_worktree(&root);
        materialize_stubs(&root, &hook_dir_abs, &binary);
        record_hook_adoption_baseline(&root);
        if functional_probe(&root, &hook_dir_abs) {
            let prior = cfg_get(&root, "dvandva.priorHooksPath");
            println!(
                "install-dvandva-hooks: already adopted; refreshed scripts + stubs and re-probed (prior={prior})."
            );
            return 0;
        }
        eprintln!(
            "install-dvandva-hooks: error: functional probe failed after refresh in {}.",
            root.display()
        );
        return 1;
    }

    // Fresh adoption (or re-pointing from a foreign owner). Record the prior
    // owner exactly once; never record our own dir as the prior.
    if cfg_get(&root, "dvandva.priorHooksPath").is_empty() {
        if current.is_empty() || current == HOOK_REL {
            set_wt(&root, "dvandva.priorHooksPath", SENTINEL_DEFAULT);
        } else {
            set_wt(&root, "dvandva.priorHooksPath", &current);
        }
    }

    pin_state_worktree(&root);
    record_hook_adoption_baseline(&root);
    materialize_stubs(&root, &hook_dir_abs, &binary);

    if !functional_probe(&root, &hook_dir_abs) {
        eprintln!(
            "install-dvandva-hooks: error: functional probe failed; rolling back in {}.",
            root.display()
        );
        restore_prior_state(&root, &hook_dir_abs);
        return 1;
    }

    let prior = cfg_get(&root, "dvandva.priorHooksPath");
    println!(
        "install-dvandva-hooks: adopted core.hooksPath={HOOK_REL} (prior={prior}) in {}",
        root.display()
    );
    0
}

// --- config helpers ---------------------------------------------------------

/// Worktree-then-local config read; absent keys read as the empty string.
fn cfg_get(root: &Path, key: &str) -> String {
    gitcfg::cfg_get(root, key).unwrap_or_default()
}

/// A `--local`-scope read; absent keys read as the empty string.
fn local_get(root: &Path, key: &str) -> String {
    git_stdout(root, &["config", "--local", "--get", key]).unwrap_or_default()
}

/// True when `extensions.worktreeConfig` is enabled for the repo.
fn wt_enabled(root: &Path) -> bool {
    gitcfg::worktree_config_enabled(root)
}

/// Enable `extensions.worktreeConfig` (idempotent).
fn enable_wt(root: &Path) {
    let _ = git(root, &["config", "extensions.worktreeConfig", "true"]);
}

/// Set a key at `--worktree` scope (enabling `extensions.worktreeConfig`).
fn set_wt(root: &Path, key: &str, value: &str) {
    gitcfg::cfg_set_worktree(root, key, value);
}

/// Pin every per-worktree Dvandva key at `--worktree` scope, migrating then
/// dropping any legacy shared `--local` copies so a sibling worktree never
/// inherits our hooksPath. Never clears a FOREIGN `--local` core.hooksPath.
fn pin_state_worktree(root: &Path) {
    enable_wt(root);
    // Migrate a legacy shared prior into worktree scope before cleaning it.
    if git_stdout(
        root,
        &["config", "--worktree", "--get", "dvandva.priorHooksPath"],
    )
    .is_none()
    {
        let local_prior = local_get(root, "dvandva.priorHooksPath");
        if !local_prior.is_empty() {
            set_wt(root, "dvandva.priorHooksPath", &local_prior);
        }
    }
    set_wt(root, "core.hooksPath", HOOK_REL);
    set_wt(root, "dvandva.hooksAdopted", "true");
    set_wt(root, "dvandva.hookDir", HOOK_REL);
    // Drop legacy shared copies (only our own hooksPath value, never a foreign).
    if local_get(root, "core.hooksPath") == HOOK_REL {
        gitcfg::cfg_unset_local(root, "core.hooksPath");
    }
    gitcfg::cfg_unset_local(root, "dvandva.hooksAdopted");
    gitcfg::cfg_unset_local(root, "dvandva.hookDir");
    gitcfg::cfg_unset_local(root, "dvandva.priorHooksPath");
}

/// Record the adoption baseline at `--local` scope (drift-lint depends on it):
/// the HEAD sha, a pending sentinel in an unborn repo, or a backfilled root sha.
fn record_hook_adoption_baseline(root: &Path) {
    let existing = local_get(root, "dvandva.hooksAdoptedAt");
    if !existing.is_empty() && commit_exists(root, &existing) {
        println!(
            "install-dvandva-hooks: hook adoption baseline already recorded (dvandva.hooksAdoptedAt={existing})"
        );
        return;
    }

    match git_stdout(root, &["rev-parse", "--verify", "HEAD"]) {
        Some(head_sha) => {
            if existing == PENDING_ROOT_BASELINE {
                if let Some(root_sha) =
                    git_stdout(root, &["rev-list", "--max-parents=0", "--reverse", "HEAD"])
                        .and_then(|s| s.lines().next().map(str::to_string))
                        .filter(|s| !s.is_empty())
                {
                    gitcfg::cfg_set_local(root, "dvandva.hooksAdoptedAt", &root_sha);
                    gitcfg::cfg_set_local(root, "dvandva.hooksAdoptedAtInclusive", "true");
                    println!(
                        "install-dvandva-hooks: backfilled hook adoption baseline dvandva.hooksAdoptedAt={root_sha}"
                    );
                    return;
                }
            }
            gitcfg::cfg_set_local(root, "dvandva.hooksAdoptedAt", &head_sha);
            gitcfg::cfg_unset_local(root, "dvandva.hooksAdoptedAtInclusive");
            println!(
                "install-dvandva-hooks: recorded hook adoption baseline dvandva.hooksAdoptedAt={head_sha}"
            );
        }
        None => {
            gitcfg::cfg_set_local(root, "dvandva.hooksAdoptedAt", PENDING_ROOT_BASELINE);
            gitcfg::cfg_set_local(root, "dvandva.hooksAdoptedAtInclusive", "true");
            println!(
                "install-dvandva-hooks: no HEAD commit yet; recorded pending root hook adoption baseline."
            );
        }
    }
}

/// True when `<rev>^{commit}` resolves to an existing commit object.
fn commit_exists(root: &Path, rev: &str) -> bool {
    git(root, &["cat-file", "-e", &format!("{rev}^{{commit}}")])
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Restore the pre-adoption state (shared by `--uninstall` and fresh-adopt
/// rollback): drop our worktree override, re-pin a recorded foreign prior when
/// needed, remove the hook dir, and clear all `dvandva.*` adoption keys.
fn restore_prior_state(root: &Path, hook_dir_abs: &Path) {
    let prior = cfg_get(root, "dvandva.priorHooksPath");
    gitcfg::cfg_unset_worktree(root, "core.hooksPath");
    // Remove a shared --local hooksPath that points at OUR dir; a FOREIGN value
    // is the recorded prior and is left intact.
    if local_get(root, "core.hooksPath") == HOOK_REL {
        gitcfg::cfg_unset_local(root, "core.hooksPath");
    }
    if !prior.is_empty() && prior != SENTINEL_DEFAULT && local_get(root, "core.hooksPath") != prior
    {
        // Re-pin the prior only when --local does not already provide it.
        if wt_enabled(root) {
            set_wt(root, "core.hooksPath", &prior);
        } else {
            gitcfg::cfg_set_local(root, "core.hooksPath", &prior);
        }
    }
    let _ = fs::remove_dir_all(hook_dir_abs);
    for key in ["priorHooksPath", "hooksAdopted", "hookDir"] {
        gitcfg::cfg_unset_worktree(root, &format!("dvandva.{key}"));
        gitcfg::cfg_unset_local(root, &format!("dvandva.{key}"));
    }
    for key in ["hooksAdoptedAt", "hooksAdoptedAtInclusive"] {
        gitcfg::cfg_unset_local(root, &format!("dvandva.{key}"));
    }
}

/// `--uninstall`: restore the recorded prior owner (or default) and drop state.
fn uninstall_hooks(root: &Path, hook_dir_abs: &Path) -> i32 {
    let prior_seen = cfg_get(root, "dvandva.priorHooksPath");
    let current_seen = cfg_get(root, "core.hooksPath");
    if prior_seen.is_empty() && current_seen != HOOK_REL && !hook_dir_abs.is_dir() {
        println!("install-dvandva-hooks: nothing to uninstall (no Dvandva hook adoption found).");
        return 0;
    }
    restore_prior_state(root, hook_dir_abs);
    let restored = git_stdout(root, &["config", "--local", "core.hooksPath"])
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "(default/unset)".to_string());
    println!("install-dvandva-hooks: uninstalled; core.hooksPath restored to: {restored}");
    0
}

// --- materialization --------------------------------------------------------

/// Materialize `pre-commit` + `prepare-commit-msg` as symlinks to `binary`,
/// falling back to a hard copy (chmod 0755) when `symlink(2)` fails.
fn materialize_wrappers(binary: &Path, hook_dir_abs: &Path) -> std::io::Result<MaterializeMode> {
    let mut mode = MaterializeMode::Symlink;
    for name in ["pre-commit", "prepare-commit-msg"] {
        let dst = hook_dir_abs.join(name);
        let _ = fs::remove_file(&dst);
        if let Err(sym_err) = symlink(binary, &dst) {
            fs::copy(binary, &dst)?;
            let mut perm = fs::metadata(&dst)?.permissions();
            perm.set_mode(0o755);
            fs::set_permissions(&dst, perm)?;
            if matches!(mode, MaterializeMode::Symlink) {
                mode = MaterializeMode::Copy(sym_err.to_string());
            }
        }
    }
    Ok(mode)
}

/// Narrate which materialization mode was used (tests assert on this).
fn report_mode(binary: &Path, mode: &MaterializeMode) {
    match mode {
        MaterializeMode::Symlink => println!(
            "install-dvandva-hooks: materialized pre-commit + prepare-commit-msg as symlinks -> {}",
            binary.display()
        ),
        MaterializeMode::Copy(err) => println!(
            "install-dvandva-hooks: symlink unavailable ({err}); materialized pre-commit + prepare-commit-msg by copying {}",
            binary.display()
        ),
    }
}

/// Resolve the prior hooks directory for stub enumeration (mirrors
/// `dvandva-hook-lib`'s `resolve_prior_hook`): default sentinel -> the repo's
/// true default hooks dir; absolute -> verbatim; relative -> under the root.
fn installer_prior_dir(root: &Path) -> PathBuf {
    let prior = cfg_get(root, "dvandva.priorHooksPath");
    let dir: PathBuf = if prior.is_empty() || prior == SENTINEL_DEFAULT {
        let common = git_stdout(root, &["rev-parse", "--git-common-dir"]).unwrap_or_default();
        PathBuf::from(format!("{common}/hooks"))
    } else if prior.starts_with('/') {
        PathBuf::from(&prior)
    } else {
        root.join(&prior)
    };
    if dir.is_absolute() {
        dir
    } else {
        root.join(dir)
    }
}

/// Materialize a symlink pass-through stub for every canonical hook name that
/// exists executable in the prior hooks dir (except the two owned wrappers).
/// Never stubs against ourselves (prior dir resolving to our own hook dir).
fn materialize_stubs(root: &Path, hook_dir_abs: &Path, binary: &Path) {
    let prior_dir = installer_prior_dir(root);
    if !prior_dir.is_dir() {
        return;
    }
    let rp_prior = prior_dir
        .canonicalize()
        .unwrap_or_else(|_| prior_dir.clone());
    let rp_dir = hook_dir_abs
        .canonicalize()
        .unwrap_or_else(|_| hook_dir_abs.to_path_buf());
    if rp_prior == rp_dir {
        return; // never stub against ourselves
    }
    for name in GIT_HOOK_NAMES {
        if name == "pre-commit" || name == "prepare-commit-msg" {
            continue; // owned by our wrappers
        }
        if is_executable_file(&prior_dir.join(name)) {
            let dst = hook_dir_abs.join(name);
            let _ = fs::remove_file(&dst);
            let _ = symlink(binary, &dst);
        }
    }
}

fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Exec the materialized wrappers with `DVANDVA_HOOK_SELFCHECK=1` and require
/// the positive wiring sentinels (path-string independent).
fn functional_probe(root: &Path, hook_dir_abs: &Path) -> bool {
    probe_sentinel(root, &hook_dir_abs.join("pre-commit"), "DVANDVA_GATE_WIRED")
        && probe_sentinel(
            root,
            &hook_dir_abs.join("prepare-commit-msg"),
            "DVANDVA_PREPARE_WIRED",
        )
}

fn probe_sentinel(root: &Path, hook: &Path, sentinel: &str) -> bool {
    match Command::new(hook)
        .current_dir(root)
        .env("DVANDVA_HOOK_SELFCHECK", "1")
        .output()
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout).contains(sentinel),
        Err(_) => false,
    }
}
