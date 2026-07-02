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

use serde_json::Value;

use crate::emit;
use crate::hook_preflight::{run_hook_preflight, HookMode};
use crate::resolve::{resolve_active_run, ResolveEnv, ResolveError, ResolveOutcome};
use crate::util::{coalesce, read_json_lenient};
use crate::write::expected_owner;
use crate::Role;

/// The v2 status catalog, mirrored locally (S2-T3 read-only sanity check):
/// a status token outside this set is `invalid_baton`. A local literal list
/// rather than a `write.rs` coupling — matches `crate::baton::Status`'s
/// current 21-token catalog. Future status additions (e.g. `abandoned`) need
/// a matching update here; the S6-T1 schema-parity lint is the intended
/// long-term guard against that drift.
const V2_STATUS_TOKENS: &[&str] = &[
    "research_drafting",
    "research_review",
    "research_revision",
    "spec_drafting",
    "spec_review",
    "spec_revision",
    "implementing",
    "parallel_implementing",
    "test_creation",
    "cross_review",
    "cross_fixing",
    "deep_review",
    "review_of_review",
    "counter_review",
    "deslop",
    "termination_review",
    "phase_review",
    "phase_fixing",
    "human_question",
    "human_decision",
    "done",
];

/// Outcome of the read-only baton sanity check run on a RESOLVED baton,
/// before the hook stage.
enum SanityCheck {
    /// Schema/owner/active_roles all check out (or the status carries no
    /// owner expectation, e.g. a terminal status) — proceed as today.
    Ok,
    /// Not a v2-schema baton: legacy read-only tolerance, skip entirely.
    V1Skipped,
    /// The baton file could not be read or parsed; defer to the hook stage's
    /// own handling rather than duplicating that failure mode here.
    Unreadable,
    /// A v2 baton failed one of the sanity checks; `detail` is the
    /// human-readable violation to surface on the `DVANDVA_PREFLIGHT` line.
    Invalid(String),
}

/// Read-only sanity check (S2-T3) on a RESOLVED baton: does its assignee
/// match the v2 engine's expected owner for its status, does a team-owned
/// status carry non-empty `active_roles`, and is its status token part of
/// the v2 enum. Never mutates the baton file.
fn sanity_check(baton_path: &Path) -> SanityCheck {
    let Ok(value) = read_json_lenient(baton_path) else {
        return SanityCheck::Unreadable;
    };

    let schema = str_field(&value, "schema");
    if schema != "dvandva.baton.v2" {
        return SanityCheck::V1Skipped;
    }

    let status = str_field(&value, "status");
    if !V2_STATUS_TOKENS.contains(&status.as_str()) {
        return SanityCheck::Invalid(format!("unknown_status status={status}"));
    }

    let mode = str_field(&value, "mode");
    let profile = str_field(&value, "profile");
    let (expected_assignee, expected_active_roles) =
        expected_owner(&schema, &mode, &profile, &status);

    let assignee = str_field(&value, "assignee");
    if !expected_assignee.is_empty() && assignee != expected_assignee {
        return SanityCheck::Invalid(format!(
            "owner_mismatch status={status} expected={expected_assignee} actual={assignee}"
        ));
    }

    if !expected_active_roles.is_empty() && active_roles_of(&value).is_empty() {
        return SanityCheck::Invalid(format!("team_status_missing_active_roles status={status}"));
    }

    SanityCheck::Ok
}

/// jq `//`-style string read: `null`/`false`/absent coalesce to `""`.
fn str_field(value: &Value, key: &str) -> String {
    match coalesce(value.get(key)) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// The baton's `active_roles` array as owned strings (empty when absent,
/// null, or not an array of strings).
fn active_roles_of(value: &Value) -> Vec<String> {
    match coalesce(value.get("active_roles")) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

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

            let note = match sanity_check(&baton) {
                SanityCheck::Invalid(detail) => {
                    println!(
                        "DVANDVA_PREFLIGHT baton={} result=error reason=invalid_baton detail={detail}",
                        baton.display()
                    );
                    return 1;
                }
                SanityCheck::V1Skipped => " note=v1_skipped",
                SanityCheck::Ok | SanityCheck::Unreadable => "",
            };

            std::env::set_var("DVANDVA_BATON_FILE", &baton);
            std::env::set_var("DVANDVA_RUN_ID", &run_id);
            std::env::set_var("DVANDVA_SELECTED_BY", chosen_by);
            println!(
                "DVANDVA_PREFLIGHT role={role_str} result=resolved baton={} run_id={run_id} selected_by={chosen_by}{note}",
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
