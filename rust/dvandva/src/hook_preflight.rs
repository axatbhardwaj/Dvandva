//! `dvandva-hook-preflight.sh` port — the role hook-stage preflight.
//!
//! Installs or refreshes the delegated Dvandva wrapper chain via
//! [`crate::install_hooks::run_install`], then proves gate reachability by
//! probing the active `pre-commit` hook with `DVANDVA_HOOK_SELFCHECK=1`.
//!
//! DESIGN DECISION (re-key of the shell's `resolve_installer`): the shell
//! helper resolved an installer SCRIPT PATH — plugin-co-located first,
//! falling back to the target repo's `scripts/` — and invoked it as a
//! subprocess. Post-port there is no installer script to resolve:
//! [`crate::install_hooks::run_install`] is called in-process directly, so
//! the `resolve_installer` function and its `missing_installer` failure mode
//! are gone entirely. A plugin-installed target repo with NO committed
//! Dvandva source now works exactly like every other repo, since the
//! installer IS the running binary (it materializes symlinks to itself).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::gitcfg::{self, git_stdout, repo_toplevel};
use crate::install_hooks::run_install;
use crate::preflight::realpath_m;
use crate::Role;

const SENTINEL: &str = "DVANDVA_GATE_WIRED";

/// The three hook-preflight modes: `auto`/`strict` run the install-and-probe
/// sequence (identical behavior today — `strict` is reserved for future
/// stricter checks, mirroring the shell helper); `off` skips it entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookMode {
    Auto,
    Strict,
    Off,
}

impl HookMode {
    /// Parse a mode token (`auto`/`strict`/`off`). Returns `None` for
    /// anything else, including the empty string.
    pub fn parse(value: &str) -> Option<HookMode> {
        match value {
            "auto" => Some(HookMode::Auto),
            "strict" => Some(HookMode::Strict),
            "off" => Some(HookMode::Off),
            _ => None,
        }
    }

    /// The canonical lowercase token for this mode.
    pub fn as_str(&self) -> &'static str {
        match self {
            HookMode::Auto => "auto",
            HookMode::Strict => "strict",
            HookMode::Off => "off",
        }
    }
}

/// Run the hook-stage preflight for `role` against `repo_arg` (default: the
/// process cwd) at `mode`. Prints `DVANDVA_HOOK_PREFLIGHT ...` lines exactly
/// like the shell helper and returns the process exit code.
pub fn run_hook_preflight(role: Role, repo_arg: Option<&Path>, mode: HookMode) -> i32 {
    let role_str = role.as_str();

    if let Some(env_role) = env_role() {
        if env_role != role_str {
            println!(
                "DVANDVA_HOOK_PREFLIGHT role={role_str} mode={} result=error reason=role_mismatch env_role={env_role}",
                mode.as_str()
            );
            return 1;
        }
    }
    std::env::set_var("DVANDVA_ROLE", role_str);

    let Some(repo_root) = resolve_repo_root(repo_arg) else {
        let shown = match repo_arg {
            Some(path) => path.display().to_string(),
            None => std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        };
        println!(
            "DVANDVA_HOOK_PREFLIGHT role={role_str} mode={} result=error reason=not_git repo={shown}",
            mode.as_str()
        );
        return 1;
    };

    if mode == HookMode::Off {
        println!(
            "DVANDVA_HOOK_PREFLIGHT role={role_str} mode=off result=off repo={}",
            repo_root.display()
        );
        return 0;
    }

    let install_code = run_install(&repo_root, false);
    if install_code != 0 {
        println!(
            "DVANDVA_HOOK_PREFLIGHT role={role_str} mode={} result=error reason=install_failed repo={}",
            mode.as_str(),
            repo_root.display()
        );
        return 1;
    }

    let hook_dir = active_hook_dir(&repo_root);
    let pre_commit = hook_dir.join("pre-commit");
    if !is_executable(&pre_commit) {
        println!(
            "DVANDVA_HOOK_PREFLIGHT role={role_str} mode={} result=error reason=broken_chain repo={} active_pre_commit={}",
            mode.as_str(),
            repo_root.display(),
            realpath_m(&pre_commit).display()
        );
        return 1;
    }

    let (probe_ok, probe_out) = probe_pre_commit(&repo_root, &pre_commit);
    if !probe_ok || !probe_out.contains(SENTINEL) {
        println!(
            "DVANDVA_HOOK_PREFLIGHT role={role_str} mode={} result=error reason=probe_failed repo={} active_pre_commit={} sentinel={SENTINEL}",
            mode.as_str(),
            repo_root.display(),
            realpath_m(&pre_commit).display()
        );
        if !probe_out.is_empty() {
            println!("{probe_out}");
        }
        return 1;
    }

    let current_hooks = gitcfg::cfg_get(&repo_root, "core.hooksPath").unwrap_or_default();
    let prior_hooks = gitcfg::cfg_get(&repo_root, "dvandva.priorHooksPath").unwrap_or_default();
    println!(
        "DVANDVA_HOOK_PREFLIGHT role={role_str} mode={} result=ok repo={} hooks_path={} prior_hooks_path={} active_pre_commit={} sentinel={SENTINEL}",
        mode.as_str(),
        repo_root.display(),
        if current_hooks.is_empty() { "default".to_string() } else { current_hooks },
        if prior_hooks.is_empty() { "unset".to_string() } else { prior_hooks },
        realpath_m(&pre_commit).display()
    );
    0
}

/// The effective `DVANDVA_ROLE`, treating an empty value as unset.
fn env_role() -> Option<String> {
    std::env::var("DVANDVA_ROLE").ok().filter(|r| !r.is_empty())
}

/// `git -C <repo_arg> rev-parse --show-toplevel`, or the process cwd's
/// toplevel when `repo_arg` is `None`.
fn resolve_repo_root(repo_arg: Option<&Path>) -> Option<PathBuf> {
    match repo_arg {
        Some(path) => repo_toplevel(path),
        None => std::env::current_dir()
            .ok()
            .and_then(|cwd| repo_toplevel(&cwd)),
    }
}

/// Per-worktree-correct active hook directory (mirrors the shell's
/// `active_hook_dir`): the configured `core.hooksPath`, or — when unset —
/// the repo's true default hooks dir derived from `--git-common-dir`.
fn active_hook_dir(root: &Path) -> PathBuf {
    let current = gitcfg::cfg_get(root, "core.hooksPath").unwrap_or_default();
    if current.is_empty() {
        let common = git_stdout(root, &["rev-parse", "--git-common-dir"]).unwrap_or_default();
        let dir = PathBuf::from(format!("{common}/hooks"));
        return if dir.is_absolute() {
            dir
        } else {
            root.join(dir)
        };
    }
    if current.starts_with('/') {
        PathBuf::from(current)
    } else {
        root.join(current)
    }
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Probe the active `pre-commit` with `DVANDVA_HOOK_SELFCHECK=1`, returning
/// whether it exited 0 and its combined stdout+stderr.
fn probe_pre_commit(root: &Path, pre_commit: &Path) -> (bool, String) {
    match Command::new(pre_commit)
        .current_dir(root)
        .env("DVANDVA_HOOK_SELFCHECK", "1")
        .output()
    {
        Ok(out) => {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            (out.status.success(), combined)
        }
        Err(_) => (false, String::new()),
    }
}
