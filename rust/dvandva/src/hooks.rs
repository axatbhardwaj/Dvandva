//! Git-hook behavior for the `dvandva` multicall binary.
//!
//! Post-port there are no shell wrapper scripts: git invokes a SYMLINK named
//! after the hook (`pre-commit`, `prepare-commit-msg`, and the other canonical
//! client-side hook names) pointing at the `dvandva` binary, so `argv[0]`
//! carries the hook name. These handlers fold together what the shell wrapper,
//! `dvandva-hook-lib.sh`, and `dvandva-commit-gate.sh` did as separate files:
//!
//! * `pre-commit` — run the commit gate FIRST; on a block, propagate its exit
//!   code WITHOUT delegating; on pass, `exec` the prior pre-commit chain if one
//!   resolves, else exit 0.
//! * `prepare-commit-msg` — delegate to the prior chain FIRST (propagating its
//!   exit), then stamp the `Dvandva-Checkpoint: <N>` trailer (skipped for
//!   merge/squash sources, when a trailer is already present, or when no active
//!   baton assigns the current role).
//! * any other hook name — resolve and `exec` the prior hook of that name with
//!   the same arguments; exit 0 if none resolves.
//!
//! `argv[0]` (the invoked symlink path) is read via [`std::env::args_os`], not
//! [`std::env::current_exe`], so hook-dir derivation and the prior-hook
//! self-loop guard see the symlink, not its resolved binary target.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commit_gate;
use crate::gitcfg::{git_stdout, repo_toplevel};
use crate::util::{read_json_lenient, JsonReadError};

/// Canonical client-side git hook names. Shared by two consumers: `main.rs`'s
/// multicall `argv[0]` dispatch (which hook basenames route to
/// [`run`] instead of a normal subcommand) and
/// [`crate::install_hooks`]'s pass-through stub materialization (which
/// canonical hook names get a symlink stub for every executable prior hook).
/// `pre-commit` and `prepare-commit-msg` carry Dvandva behavior
/// ([`run_pre_commit`], [`run_prepare_commit_msg`]); the rest pass through to
/// the prior hook chain ([`run_pass_through`]).
pub const GIT_HOOK_NAMES: [&str; 24] = [
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

/// Entry point for a git-hook invocation. `name` is the hook name (the
/// invoking symlink's basename, or the `git-hook <name>` argument); `args` are
/// the arguments git passed after `argv[0]`.
pub fn run(name: &str, args: &[String]) -> i32 {
    let argv0 = argv0();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match name {
        "pre-commit" => run_pre_commit(&argv0, &cwd),
        "prepare-commit-msg" => run_prepare_commit_msg(&argv0, &cwd, args),
        _ => run_pass_through(name, &argv0, &cwd, args),
    }
}

/// The path git invoked this hook with (the symlink), NOT the resolved binary.
fn argv0() -> String {
    std::env::args_os()
        .next()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn selfcheck_active() -> bool {
    std::env::var("DVANDVA_HOOK_SELFCHECK").as_deref() == Ok("1")
}

/// The effective `DVANDVA_ROLE`, treating an empty value as unset.
fn env_role() -> Option<String> {
    std::env::var("DVANDVA_ROLE").ok().filter(|r| !r.is_empty())
}

// --- pre-commit -------------------------------------------------------------

fn run_pre_commit(argv0: &str, cwd: &Path) -> i32 {
    // Functional-probe short-circuit: prove the gate is wired without running.
    if selfcheck_active() {
        println!("DVANDVA_GATE_WIRED:{}", hook_dir_from_argv0(argv0));
        return 0;
    }

    // Run the commit gate. On block, propagate its exit WITHOUT delegating.
    let result = commit_gate::evaluate(cwd, env_role().as_deref());
    for line in &result.stderr {
        eprintln!("{line}");
    }
    if result.code != 0 {
        return result.code;
    }

    // Gate passed (or no active baton) — delegate to the prior pre-commit.
    match resolve_prior_hook("pre-commit", argv0, cwd) {
        Some(prior) => exec_prior(&prior, &[]),
        None => 0,
    }
}

// --- prepare-commit-msg -----------------------------------------------------

fn run_prepare_commit_msg(argv0: &str, cwd: &Path, args: &[String]) -> i32 {
    if selfcheck_active() {
        println!("DVANDVA_PREPARE_WIRED:{}", hook_dir_from_argv0(argv0));
        return 0;
    }

    // Delegate to the prior prepare-commit-msg FIRST with the original args;
    // propagate a non-zero exit so a failing prior chain aborts the commit.
    if let Some(prior) = resolve_prior_hook("prepare-commit-msg", argv0, cwd) {
        match Command::new(&prior).args(args).status() {
            Ok(status) => {
                let code = status.code().unwrap_or(1);
                if code != 0 {
                    return code;
                }
            }
            Err(err) => {
                eprintln!(
                    "Dvandva prepare-commit-msg: failed to run prior hook {}: {err}",
                    prior.display()
                );
                return 1;
            }
        }
    }

    stamp_checkpoint(cwd, args)
}

/// Append the `Dvandva-Checkpoint: <N>` trailer to the commit message file.
///
/// Signature args: `<msg-file> [<commit-source> [<sha1>]]`.
fn stamp_checkpoint(cwd: &Path, args: &[String]) -> i32 {
    // Skip auto-generated messages (merge/squash sources).
    let commit_source = args.get(1).map(String::as_str).unwrap_or("");
    if commit_source == "merge" || commit_source == "squash" {
        return 0;
    }

    let Some(repo_root) = repo_toplevel(cwd) else {
        return 0;
    };
    let msg_arg = match args.first() {
        Some(file) if !file.is_empty() => file,
        _ => return 0,
    };

    // DVANDVA_ROLE unset — pre-commit already gated (or there is no baton).
    let Some(role) = env_role() else {
        return 0;
    };

    // Repos without Dvandva state remain unaffected.
    let paths = commit_gate::collect_baton_paths(&repo_root);
    if paths.is_empty() {
        return 0;
    }

    // Count active batons; record the checkpoint of each where the role is
    // allowed to stamp. Malformed JSON fails closed.
    let mut active_count = 0usize;
    let mut allowed_checkpoints: Vec<String> = Vec::new();
    for path in &paths {
        match read_json_lenient(path) {
            Err(JsonReadError::Missing) => continue,
            Err(JsonReadError::Invalid) => {
                eprintln!(
                    "Dvandva prepare-commit-msg error: malformed baton JSON: {}",
                    path.display()
                );
                return 1;
            }
            Ok(value) => {
                let status = commit_gate::field_str(&value, "status", "");
                if commit_gate::is_gate_terminal(&status) {
                    continue;
                }
                active_count += 1;
                if commit_gate::role_allowed(&value, &role) {
                    allowed_checkpoints.push(commit_gate::field_str(&value, "checkpoint", ""));
                }
            }
        }
    }

    if active_count > 1 {
        eprintln!(
            "Dvandva prepare-commit-msg error: {active_count} active batons found — ambiguous active runs."
        );
        eprintln!("Resolve to a single active run before committing.");
        return 1;
    }

    // No active baton, or none where the role may stamp — nothing to do.
    if active_count == 0 || allowed_checkpoints.is_empty() {
        return 0;
    }
    let checkpoint = &allowed_checkpoints[0];

    let msg_path = {
        let raw = Path::new(msg_arg);
        if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            cwd.join(raw)
        }
    };

    // Skip if the trailer is already present.
    let existing = fs::read_to_string(&msg_path).unwrap_or_default();
    if existing.lines().any(|line| {
        line.strip_prefix("Dvandva-Checkpoint:")
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
    }) {
        return 0;
    }

    // Two leading newlines guarantee a blank-line separator from the body,
    // which git's trailer convention requires.
    use std::io::Write;
    match fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&msg_path)
    {
        Ok(mut file) => {
            if write!(file, "\n\nDvandva-Checkpoint: {checkpoint}\n").is_err() {
                return 1;
            }
            0
        }
        Err(_) => 1,
    }
}

// --- pass-through -----------------------------------------------------------

fn run_pass_through(name: &str, argv0: &str, cwd: &Path, args: &[String]) -> i32 {
    match resolve_prior_hook(name, argv0, cwd) {
        Some(prior) => exec_prior(&prior, args),
        None => 0,
    }
}

// --- shared helpers ---------------------------------------------------------

/// `exec` the prior hook, replacing this process. Only returns on failure.
fn exec_prior(prior: &Path, args: &[String]) -> i32 {
    let err = Command::new(prior).args(args).exec();
    eprintln!(
        "dvandva hook: failed to exec prior hook {}: {err}",
        prior.display()
    );
    126
}

/// Absolute directory containing the invoked hook file, derived from `argv0`
/// (the symlink path), mirroring the shell's `cd "$(dirname "$0")" && pwd`.
fn hook_dir_from_argv0(argv0: &str) -> String {
    let path = Path::new(argv0);
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    if let Ok(canonical) = parent.canonicalize() {
        return canonical.to_string_lossy().into_owned();
    }
    if parent.is_absolute() {
        parent.to_string_lossy().into_owned()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(parent).to_string_lossy().into_owned()
    } else {
        parent.to_string_lossy().into_owned()
    }
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Resolve the prior hook of `name`, mirroring `dvandva-hook-lib.sh`'s
/// `resolve_prior_hook`:
///
/// * `dvandva.priorHooksPath` empty or `__DVANDVA_DEFAULT__` → the repo's TRUE
///   default hooks dir (`$(git rev-parse --git-common-dir)/hooks`).
/// * an absolute path → used verbatim.
/// * a relative path → resolved against the repo root.
///
/// Returns `None` when the candidate is missing, non-executable, or resolves
/// (after canonicalization) to the invoked hook itself (self-loop guard).
fn resolve_prior_hook(name: &str, argv0: &str, cwd: &Path) -> Option<PathBuf> {
    let root = repo_toplevel(cwd)?;

    let prior = crate::gitcfg::cfg_get(&root, "dvandva.priorHooksPath").unwrap_or_default();
    let dir: PathBuf = if prior.is_empty() || prior == "__DVANDVA_DEFAULT__" {
        // Use --git-common-dir (not `--git-path hooks`, which honours our own
        // overriding core.hooksPath) so worktrees reach the real default dir.
        let common = git_stdout(&root, &["rev-parse", "--git-common-dir"]).unwrap_or_default();
        PathBuf::from(format!("{common}/hooks"))
    } else if prior.starts_with('/') {
        PathBuf::from(&prior)
    } else {
        root.join(&prior)
    };

    // Anchor any relative dir to the repo root so resolution is cwd-independent.
    let dir = if dir.is_absolute() {
        dir
    } else {
        root.join(dir)
    };

    let hook = dir.join(name);
    if !hook.exists() {
        return None;
    }

    // Self-loop guard: never delegate to one of our own materialized hooks.
    let resolved_hook = hook.canonicalize().unwrap_or_else(|_| hook.clone());
    let self_path = Path::new(argv0);
    let resolved_self = self_path
        .canonicalize()
        .unwrap_or_else(|_| self_path.to_path_buf());
    if resolved_hook == resolved_self {
        return None;
    }

    if !is_executable(&hook) {
        return None;
    }

    Some(hook)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_dir_is_absolute_parent_of_argv0() {
        let dir = tempfile::tempdir().unwrap();
        let hook = dir.path().join("pre-commit");
        std::fs::write(&hook, "").unwrap();
        let got = hook_dir_from_argv0(hook.to_str().unwrap());
        assert_eq!(
            PathBuf::from(&got),
            dir.path().canonicalize().unwrap(),
            "hook dir must be the canonical parent directory"
        );
    }

    #[test]
    fn env_role_treats_empty_as_unset() {
        // Purely exercises the empty-string filter without touching process env.
        let empty: Option<String> = Some(String::new()).filter(|r| !r.is_empty());
        assert!(empty.is_none());
    }
}
