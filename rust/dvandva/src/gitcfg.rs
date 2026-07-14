//! Worktree-scoped git plumbing shared by the hook family, the hook
//! installer, and the preflights.
//!
//! The shell helpers agreed on one config-scoping model: enable
//! `extensions.worktreeConfig`, write adoption keys at `--worktree`, and read
//! with worktree-then-local fallback. Worktrees silently fail open if any
//! consumer diverges from this, so all git config access funnels through
//! here.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Run `git -C <repo> <args…>` and return the raw output.
pub fn git(repo: &Path, args: &[&str]) -> std::io::Result<Output> {
    Command::new("git").arg("-C").arg(repo).args(args).output()
}

/// Run git and return trimmed stdout when the command succeeded.
pub fn git_stdout(repo: &Path, args: &[&str]) -> Option<String> {
    let out = git(repo, args).ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    Some(s)
}

/// `git rev-parse --show-toplevel` for `cwd`, if inside a work tree.
pub fn repo_toplevel(cwd: &Path) -> Option<PathBuf> {
    git_stdout(cwd, &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

/// True when `extensions.worktreeConfig` is enabled for this repo.
pub fn worktree_config_enabled(repo: &Path) -> bool {
    git_stdout(repo, &["config", "--get", "extensions.worktreeConfig"])
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Read a config key with the shell helpers' worktree-then-local fallback:
/// when worktree config is enabled prefer `--worktree`, else `--local`.
pub fn cfg_get(repo: &Path, key: &str) -> Option<String> {
    if worktree_config_enabled(repo) {
        if let Some(v) = git_stdout(repo, &["config", "--worktree", "--get", key]) {
            return Some(v);
        }
    }
    git_stdout(repo, &["config", "--local", "--get", key])
}

/// Set a key at worktree scope, enabling `extensions.worktreeConfig` first.
pub fn cfg_set_worktree(repo: &Path, key: &str, value: &str) -> bool {
    let enabled = git(repo, &["config", "extensions.worktreeConfig", "true"])
        .map(|o| o.status.success())
        .unwrap_or(false);
    enabled
        && git(repo, &["config", "--worktree", key, value])
            .map(|o| o.status.success())
            .unwrap_or(false)
}

/// Set a key at local (shared-across-worktrees) scope.
pub fn cfg_set_local(repo: &Path, key: &str, value: &str) -> bool {
    git(repo, &["config", "--local", key, value])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Unset a key at worktree scope; absent keys count as success.
pub fn cfg_unset_worktree(repo: &Path, key: &str) -> bool {
    match git(repo, &["config", "--worktree", "--unset", key]) {
        Ok(out) => out.status.success() || out.status.code() == Some(5),
        Err(_) => false,
    }
}

/// Unset a key at local scope; absent keys count as success.
pub fn cfg_unset_local(repo: &Path, key: &str) -> bool {
    match git(repo, &["config", "--local", "--unset", key]) {
        Ok(out) => out.status.success() || out.status.code() == Some(5),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let ok = git(dir.path(), &["init", "-q"]).unwrap().status.success();
        assert!(ok, "git init failed");
        dir
    }

    #[test]
    fn toplevel_resolves_inside_repo() {
        let repo = temp_repo();
        let top = repo_toplevel(repo.path()).unwrap();
        assert_eq!(
            top.canonicalize().unwrap(),
            repo.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn worktree_set_then_get_falls_back_correctly() {
        let repo = temp_repo();
        assert!(cfg_get(repo.path(), "dvandva.testKey").is_none());
        assert!(cfg_set_worktree(repo.path(), "dvandva.testKey", "wt"));
        assert_eq!(
            cfg_get(repo.path(), "dvandva.testKey").as_deref(),
            Some("wt")
        );
        assert!(cfg_set_local(repo.path(), "dvandva.localKey", "loc"));
        assert_eq!(
            cfg_get(repo.path(), "dvandva.localKey").as_deref(),
            Some("loc")
        );
        assert!(cfg_unset_worktree(repo.path(), "dvandva.testKey"));
        assert!(cfg_get(repo.path(), "dvandva.testKey").is_none());
    }
}
