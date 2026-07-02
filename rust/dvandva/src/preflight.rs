//! `dvandva-preflight.sh` port — the unified turn preflight.
//!
//! Resolves the active baton first (in-process, via [`crate::resolve`]), then
//! — only once a baton is RESOLVED — runs the hook-stage preflight in-process
//! via [`crate::hook_preflight::run_hook_preflight`]. The shell helper
//! `exec`'d a sibling script (`dvandva-hook-preflight.sh`) for the hook
//! stage; post-port that process replacement becomes a direct in-process
//! function call, so a `missing_hook_preflight`/`missing_resolver` failure
//! mode (the sibling script not existing on disk) is no longer possible.

use std::path::{Path, PathBuf};

use crate::emit;
use crate::hook_preflight::{run_hook_preflight, HookMode};
use crate::resolve::{resolve_active_run, ResolveEnv, ResolveError, ResolveOutcome};
use crate::Role;

/// Run the unified turn preflight for `role` at `mode`. Resolves the active
/// run under the derived work root (`git rev-parse --show-toplevel`, else
/// the process cwd), prints `DVANDVA_PREFLIGHT ...` lines exactly like the
/// shell helper, and returns the process exit code.
pub fn run_preflight(role: Role, mode: HookMode) -> i32 {
    let role_str = role.as_str();

    if let Some(env_role) = env_role() {
        if env_role != role_str {
            println!(
                "DVANDVA_PREFLIGHT role={role_str} result=error reason=role_mismatch env_role={env_role}"
            );
            return 1;
        }
    }
    std::env::set_var("DVANDVA_ROLE", role_str);

    let root = work_root();
    let env = ResolveEnv::from_process_env();
    let chosen_by = selected_by(&env);

    match resolve_active_run(role, Some(&root), env) {
        Ok(ResolveOutcome::AskMultiple(candidates)) => {
            let choices = emit::to_json_compact(&candidates).unwrap_or_else(|_| "[]".to_string());
            println!(
                "DVANDVA_PREFLIGHT role={role_str} result=ask selected_by={chosen_by} choices={choices}"
            );
            eprintln!("{}", emit::dvandva_resolve_ask(role_str, candidates.len()));
            12
        }
        Ok(ResolveOutcome::AskCorrupt { path }) => {
            println!(
                "DVANDVA_PREFLIGHT role={role_str} result=ask selected_by={chosen_by} choices=[]"
            );
            eprintln!("{}", emit::dvandva_resolve_corrupt(&path, role_str));
            12
        }
        Ok(ResolveOutcome::Create(rel_path)) => {
            let scaffold = canonical_path(&root, &rel_path);
            let run_id = run_id_for_path(&scaffold);
            println!(
                "DVANDVA_PREFLIGHT role={role_str} result=create scaffold={} run_id={run_id} selected_by={chosen_by}",
                scaffold.display()
            );
            0
        }
        Ok(ResolveOutcome::Resolved(rel_path)) => {
            let baton = canonical_path(&root, &rel_path);
            let run_id = run_id_for_path(&baton);
            std::env::set_var("DVANDVA_BATON_FILE", &baton);
            std::env::set_var("DVANDVA_RUN_ID", &run_id);
            std::env::set_var("DVANDVA_SELECTED_BY", chosen_by);
            println!(
                "DVANDVA_PREFLIGHT role={role_str} result=resolved baton={} run_id={run_id} selected_by={chosen_by}",
                baton.display()
            );
            run_hook_preflight(role, Some(&root), mode)
        }
        // The shell resolver's catch-all `unexpected_resolver_output` branch:
        // reachable only when the resolver itself fails (usage/cwd errors),
        // which post-port surfaces as a typed `Err`, not unparseable stdout.
        Err(ResolveError::Usage(message)) => {
            println!(
                "DVANDVA_PREFLIGHT role={role_str} result=error reason=unexpected_resolver_output"
            );
            eprintln!("ERROR: {message}");
            1
        }
        Err(ResolveError::Cwd { path }) => {
            println!(
                "DVANDVA_PREFLIGHT role={role_str} result=error reason=unexpected_resolver_output"
            );
            eprintln!("ERROR: --cwd is not a directory: {path}");
            1
        }
    }
}

/// The effective `DVANDVA_ROLE`, treating an empty value as unset.
fn env_role() -> Option<String> {
    std::env::var("DVANDVA_ROLE").ok().filter(|r| !r.is_empty())
}

/// `git rev-parse --show-toplevel`, else the process cwd (mirrors the
/// shell's `WORK_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd -P)"`).
fn work_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    crate::gitcfg::repo_toplevel(&cwd).unwrap_or(cwd)
}

/// Which selector chose the active run, in shell precedence order. Mirrors
/// the shell's `selected_by`, including its `-n` (non-empty) checks.
fn selected_by(env: &ResolveEnv) -> &'static str {
    if env.baton_file.as_deref().is_some_and(|v| !v.is_empty()) {
        "DVANDVA_BATON_FILE"
    } else if env.run_dir.as_deref().is_some_and(|v| !v.is_empty()) {
        "DVANDVA_RUN_DIR"
    } else if env.run_id.as_deref().is_some_and(|v| !v.is_empty()) {
        "DVANDVA_RUN_ID"
    } else {
        "discovery"
    }
}

/// `basename(dirname(path))`, except a legacy `.dvandva/baton.json` path
/// always yields `"legacy"`. Mirrors the shell's `run_id_for_path`.
pub(crate) fn run_id_for_path(path: &Path) -> String {
    if path.ends_with(".dvandva/baton.json") {
        return "legacy".to_string();
    }
    path.parent()
        .and_then(Path::file_name)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Join `raw` under `work_root` (verbatim when `raw` is already absolute),
/// then resolve it the way `realpath -m` would. Mirrors the shell's
/// `canonical_path`.
pub(crate) fn canonical_path(work_root: &Path, raw: &str) -> PathBuf {
    let candidate = if raw.starts_with('/') {
        PathBuf::from(raw)
    } else {
        work_root.join(raw)
    };
    realpath_m(&candidate)
}

/// `realpath -m`: canonicalize the longest existing ancestor (resolving
/// symlinks), then lexically append whatever trailing components don't
/// exist yet — never requiring the full path to exist.
pub(crate) fn realpath_m(path: &Path) -> PathBuf {
    let normalized = lexical_normalize(path);
    let mut ancestor = normalized;
    let mut suffix: Vec<std::ffi::OsString> = Vec::new();
    while !ancestor.as_os_str().is_empty() && !ancestor.exists() {
        if let Some(name) = ancestor.file_name() {
            suffix.push(name.to_os_string());
        }
        if !ancestor.pop() {
            break;
        }
    }
    let base = if ancestor.as_os_str().is_empty() {
        PathBuf::from("/")
    } else {
        ancestor.canonicalize().unwrap_or(ancestor)
    };
    let mut result = base;
    for part in suffix.into_iter().rev() {
        result.push(part);
    }
    result
}

/// Collapse `.`/`..` components without touching the filesystem.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{canonical_path, realpath_m, run_id_for_path};

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "dvandva-preflight-{name}-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn run_id_for_path_treats_legacy_baton_specially() {
        assert_eq!(
            run_id_for_path(Path::new("/repo/.dvandva/baton.json")),
            "legacy"
        );
    }

    #[test]
    fn run_id_for_path_uses_parent_basename_otherwise() {
        assert_eq!(
            run_id_for_path(Path::new("/repo/.dvandva/runs/accuracy/baton.json")),
            "accuracy"
        );
    }

    #[test]
    fn canonical_path_joins_relative_under_work_root() {
        let root = temp_dir("canonical-relative");
        let out = canonical_path(&root, ".dvandva/runs/run-2/baton.json");
        assert_eq!(
            out,
            root.canonicalize()
                .unwrap()
                .join(".dvandva/runs/run-2/baton.json")
        );
    }

    #[test]
    fn canonical_path_uses_absolute_raw_verbatim() {
        let root = temp_dir("canonical-absolute-root");
        let abs = temp_dir("canonical-absolute-target").join("baton.json");
        let out = canonical_path(&root, abs.to_str().unwrap());
        assert_eq!(out, realpath_m(&abs));
    }

    #[test]
    fn realpath_m_does_not_require_full_path_to_exist() {
        let root = temp_dir("realpath-missing-tail");
        let out = realpath_m(&root.join("does/not/exist.json"));
        assert_eq!(
            out,
            root.canonicalize().unwrap().join("does/not/exist.json")
        );
    }
}
