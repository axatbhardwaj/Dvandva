//! Dvandva commit-gate policy plus the baton helpers shared with the
//! git-hook family (`hooks`) and with [`crate::drift_lint`].
//!
//! Ports `scripts/dvandva-commit-gate.sh`: resolve the active baton(s) under
//! `.dvandva/` and allow a commit only when `DVANDVA_ROLE` is the assignee or
//! appears in `active_roles`. Repos without any `.dvandva` baton are entirely
//! unaffected (allow without reading role). All optional field reads go
//! through [`crate::util::coalesce`] so `null`/`false` coalesce like jq's `//`,
//! and JSON is read with [`crate::util::read_json_lenient`] so any valid-JSON
//! baton — including future/unknown status tokens — is accepted while
//! malformed JSON fails closed.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::gitcfg::{git_stdout, repo_toplevel};
use crate::util::{coalesce, read_json_lenient, JsonReadError};

/// Outcome of a commit-gate evaluation: the process exit code plus the stderr
/// lines to emit in order. The wording mirrors the shell gate verbatim so
/// existing diagnostics and log-scrapers keep matching.
pub struct GateResult {
    /// `0` allows the commit, `1` blocks it.
    pub code: i32,
    /// Diagnostic lines to print to stderr, in order.
    pub stderr: Vec<String>,
}

impl GateResult {
    fn allow() -> Self {
        GateResult {
            code: 0,
            stderr: Vec::new(),
        }
    }

    fn block(stderr: Vec<String>) -> Self {
        GateResult { code: 1, stderr }
    }
}

/// Statuses the gate treats as inactive (a run in one of these states is not
/// baton-gated). Distinct from [`crate::baton::Status::is_terminal`], which is
/// `Done`-only; the gate's terminal set is broader by design.
///
/// Shared with [`crate::drift_lint`], which uses the same terminal set to
/// decide whether an active baton makes unstamped commits reportable drift.
pub fn is_gate_terminal(status: &str) -> bool {
    matches!(
        status,
        "done" | "human_question" | "human_decision" | "abandoned"
    )
}

fn render_scalar(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Read a top-level field as a string with jq `//`-style coalescing: `null`
/// and `false` fall back to `default`, strings render unquoted, numbers render
/// as their literal.
///
/// Also used by [`crate::drift_lint`] to read a baton's `status` field.
pub(crate) fn field_str(value: &Value, key: &str, default: &str) -> String {
    match coalesce(value.get(key)) {
        Some(val) => render_scalar(val),
        None => default.to_string(),
    }
}

/// Whether `role` may commit against this baton: it is the assignee, or it is
/// listed in an `active_roles` array. Mirrors the shell's jq predicate.
pub(crate) fn role_allowed(value: &Value, role: &str) -> bool {
    if let Some(Value::String(assignee)) = value.get("assignee") {
        if assignee == role {
            return true;
        }
    }
    if let Some(Value::Array(roles)) = value.get("active_roles") {
        return roles
            .iter()
            .any(|entry| matches!(entry, Value::String(s) if s == role));
    }
    false
}

/// `(.active_roles // []) | join(", ")` over string elements.
fn active_roles_str(value: &Value) -> String {
    match coalesce(value.get("active_roles")) {
        Some(Value::Array(roles)) => roles
            .iter()
            .filter_map(|entry| match entry {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

/// Collect candidate baton paths under a repo root, mirroring the shell:
/// the legacy `.dvandva/baton.json` first, then run-scoped
/// `.dvandva/runs/*/baton.json` (via `find … -maxdepth 2 -name baton.json`).
/// Run-scoped paths are sorted for deterministic ordering.
///
/// Shared with [`crate::drift_lint`], which reuses this exact discovery
/// logic to locate the batons it checks for active-run status.
pub fn collect_baton_paths(repo_root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let legacy = repo_root.join(".dvandva").join("baton.json");
    if legacy.is_file() {
        paths.push(legacy);
    }

    let runs = repo_root.join(".dvandva").join("runs");
    if runs.is_dir() {
        let mut found = Vec::new();
        collect_baton_json(&runs, 1, &mut found);
        found.sort();
        paths.extend(found);
    }

    paths
}

// `find <runs> -maxdepth 2 -name baton.json`: files directly in runs (depth 1)
// and files one directory below (depth 2); no deeper descent.
fn collect_baton_json(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if path.file_name() == Some(OsStr::new("baton.json")) {
                out.push(path);
            }
        } else if path.is_dir() && depth < 2 {
            collect_baton_json(&path, depth + 1, out);
        }
    }
}

/// Evaluate the commit gate for a working directory.
///
/// `role` is the effective `DVANDVA_ROLE` (empty/absent treated as unset).
/// Returns [`GateResult`] with the exit code and any diagnostic lines.
pub fn evaluate(cwd: &Path, role: Option<&str>) -> GateResult {
    // Not inside a git repo — nothing to gate.
    let Some(repo_root) = repo_toplevel(cwd) else {
        return GateResult::allow();
    };

    let paths = collect_baton_paths(&repo_root);
    if paths.is_empty() {
        return GateResult::allow();
    }

    // Filter to active (non-terminal) batons; malformed JSON fails closed.
    let mut active: Vec<(PathBuf, Value)> = Vec::new();
    for path in &paths {
        match read_json_lenient(path) {
            Err(JsonReadError::Missing) => continue,
            Err(JsonReadError::Invalid) => {
                return GateResult::block(vec![format!(
                    "DVANDVA_GATE error: malformed baton JSON: {}",
                    path.display()
                )]);
            }
            Ok(value) => {
                let status = field_str(&value, "status", "");
                if !is_gate_terminal(&status) {
                    active.push((path.clone(), value));
                }
            }
        }
    }

    // All batons terminal (run complete) — allow.
    if active.is_empty() {
        return GateResult::allow();
    }

    // Ambiguity: more than one active run — fail closed.
    if active.len() > 1 {
        let mut lines = vec![format!(
            "DVANDVA_GATE error: {} active batons found — ambiguous active runs.",
            active.len()
        )];
        for (path, value) in &active {
            let status = field_str(value, "status", "unknown");
            let checkpoint = field_str(value, "checkpoint", "?");
            lines.push(format!(
                "  {}  status={status}  checkpoint={checkpoint}",
                path.display()
            ));
        }
        lines.push("Resolve to a single active run before committing.".to_string());
        return GateResult::block(lines);
    }

    // Exactly one active baton — read its fields.
    let (active_path, value) = &active[0];
    let status = field_str(value, "status", "");
    let assignee = field_str(value, "assignee", "");
    let checkpoint = field_str(value, "checkpoint", "0");

    // DVANDVA_ROLE must be set when an active baton is present.
    let role = match role {
        Some(role) if !role.is_empty() => role,
        _ => {
            return GateResult::block(vec![
                "DVANDVA_GATE error: DVANDVA_ROLE is unset but an active baton exists.".to_string(),
                format!("  baton: {}", active_path.display()),
                format!("  status={status}  assignee={assignee}  checkpoint={checkpoint}"),
                "Export DVANDVA_ROLE=vadi or DVANDVA_ROLE=prativadi before committing.".to_string(),
            ]);
        }
    };

    // DVANDVA_ROLE must be a known engine role.
    if role != "vadi" && role != "prativadi" {
        return GateResult::block(vec![format!(
            "DVANDVA_GATE error: DVANDVA_ROLE='{role}' is not a valid role (must be vadi or prativadi)."
        )]);
    }

    // Allow if this role is the assignee or appears in active_roles.
    if role_allowed(value, role) {
        return check_staged_paths(&repo_root, value);
    }

    // Block — emit a clear diagnostic.
    let mut lines = vec![
        format!("DVANDVA_GATE blocked: DVANDVA_ROLE={role} is not allowed to commit."),
        format!("  baton: {}", active_path.display()),
        format!("  status={status}  assignee={assignee}  checkpoint={checkpoint}"),
    ];
    let roles = active_roles_str(value);
    if !roles.is_empty() {
        lines.push(format!("  active_roles={roles}"));
    }
    lines.push("The baton is not currently assigned to your role. Wait for your turn.".to_string());
    GateResult::block(lines)
}

// ===========================================================================
// S4-T9: staged-path crosscheck against the active baton's declared scope.
// ===========================================================================

/// The `DVANDVA_COMMIT_GATE_PATHS` escape hatch: `off` skips the crosscheck,
/// `warn` prints offenders but allows, anything else (including unset) is the
/// default `block`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathsMode {
    Block,
    Warn,
    Off,
}

fn commit_gate_paths_mode() -> PathsMode {
    match std::env::var("DVANDVA_COMMIT_GATE_PATHS").ok().as_deref() {
        Some("off") => PathsMode::Off,
        Some("warn") => PathsMode::Warn,
        _ => PathsMode::Block,
    }
}

/// `git diff --cached --name-only` in `repo_root`, one path per line.
fn staged_paths(repo_root: &Path) -> Vec<String> {
    git_stdout(repo_root, &["diff", "--cached", "--name-only"])
        .map(|out| {
            out.lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The baton's declared path scope: `changed_paths` union every `work_split`
/// chunk's `paths` and `write_paths`. Run-level union across the WHOLE split
/// (not role-filtered) — simpler, and matches the run-level `changed_paths`
/// union semantics the baton already carries at the top level.
fn allowed_paths_from_baton(value: &Value) -> HashSet<String> {
    let mut allowed = HashSet::new();
    push_path_strings(&mut allowed, coalesce(value.get("changed_paths")));
    if let Some(Value::Array(chunks)) = coalesce(value.get("work_split")) {
        for chunk in chunks {
            push_path_strings(&mut allowed, coalesce(chunk.get("paths")));
            push_path_strings(&mut allowed, coalesce(chunk.get("write_paths")));
        }
    }
    allowed
}

fn push_path_strings(out: &mut HashSet<String>, value: Option<&Value>) {
    if let Some(Value::Array(items)) = value {
        for item in items {
            if let Value::String(s) = item {
                out.insert(s.clone());
            }
        }
    }
}

/// `.dvandva/` and `superpowers/` are always exempt from the crosscheck.
fn is_commit_gate_path_exempt(path: &str) -> bool {
    path == ".dvandva"
        || path.starts_with(".dvandva/")
        || path == "superpowers"
        || path.starts_with("superpowers/")
}

/// Conservative LOCAL subset of `write.rs`'s hard-path / security matchers
/// (`hard_path`/`is_security_path` in `write.rs` are private, not
/// `pub(crate)`, so they cannot be reused here). This subset only backs the
/// commit-gate's "recompute the profile floor" reminder line — it is NOT
/// byte-identical to `write.rs`'s canonical hard-path set, and drift between
/// the two is a known surface for a future schema-parity lint to guard.
fn matches_reminder_hard_path(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    base == ".env"
        || base.starts_with(".env.")
        || path.starts_with("secret/")
        || path.contains("/secret/")
        || path.starts_with("secrets/")
        || path.contains("/secrets/")
        || path.starts_with("credential/")
        || path.contains("/credential/")
        || path.starts_with("credentials/")
        || path.contains("/credentials/")
        || base == "product.md"
        || path.starts_with("plugins/dvandva/skills/")
        || path.starts_with("rust/dvandva/src/")
}

/// The S4-T9 staged-path crosscheck, run once the ordinary role checks have
/// already allowed the commit against `value` (the single active baton).
fn check_staged_paths(repo_root: &Path, value: &Value) -> GateResult {
    let mode = commit_gate_paths_mode();
    if mode == PathsMode::Off {
        return GateResult::allow();
    }

    let allowed = allowed_paths_from_baton(value);
    if allowed.is_empty() {
        // The baton hasn't declared a changed_paths/work_split scope, so
        // there is nothing to crosscheck staged paths against — fail open,
        // matching the gate's baseline behavior when it has no opinion.
        return GateResult::allow();
    }

    let offenders: Vec<String> = staged_paths(repo_root)
        .into_iter()
        .filter(|path| !is_commit_gate_path_exempt(path) && !allowed.contains(path.as_str()))
        .collect();
    if offenders.is_empty() {
        return GateResult::allow();
    }

    let verb = if mode == PathsMode::Warn {
        "warning"
    } else {
        "blocked"
    };
    let mut lines = vec![format!(
        "DVANDVA_GATE {verb}: {} staged path(s) outside the baton's allowed set.",
        offenders.len()
    )];
    for path in offenders.iter().take(10) {
        lines.push(format!("  {path}"));
    }
    if offenders.len() > 10 {
        lines.push(format!("  ... and {} more", offenders.len() - 10));
    }

    let profile = field_str(value, "profile", "");
    if profile != "full"
        && offenders
            .iter()
            .any(|path| matches_reminder_hard_path(path))
    {
        lines.push(
            "DVANDVA_GATE reminder: a staged path matches the hard-path set while the baton's effective profile is not full — recompute the profile floor.".to_string(),
        );
    }

    match mode {
        PathsMode::Warn => GateResult {
            code: 0,
            stderr: lines,
        },
        _ => GateResult::block(lines),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn gate_terminal_covers_broader_set_than_baton_status() {
        assert!(is_gate_terminal("done"));
        assert!(is_gate_terminal("human_question"));
        assert!(is_gate_terminal("human_decision"));
        assert!(is_gate_terminal("abandoned"));
        assert!(!is_gate_terminal("implementing"));
        assert!(!is_gate_terminal(""));
        assert!(!is_gate_terminal("some_future_status"));
    }

    #[test]
    fn field_str_coalesces_like_jq() {
        let v = json!({"status": "implementing", "checkpoint": 7, "n": null, "f": false});
        assert_eq!(field_str(&v, "status", ""), "implementing");
        assert_eq!(field_str(&v, "checkpoint", "0"), "7");
        assert_eq!(field_str(&v, "n", "fallback"), "fallback");
        assert_eq!(field_str(&v, "f", "fallback"), "fallback");
        assert_eq!(field_str(&v, "missing", "0"), "0");
    }

    #[test]
    fn role_allowed_by_assignee_or_active_roles() {
        let assignee = json!({"assignee": "vadi", "active_roles": []});
        assert!(role_allowed(&assignee, "vadi"));
        assert!(!role_allowed(&assignee, "prativadi"));

        let team = json!({"assignee": "team", "active_roles": ["vadi", "prativadi"]});
        assert!(role_allowed(&team, "vadi"));
        assert!(role_allowed(&team, "prativadi"));

        let scalar = json!({"assignee": "vadi"});
        assert!(role_allowed(&scalar, "vadi"));
        assert!(!role_allowed(&scalar, "prativadi"));
    }

    #[test]
    fn active_roles_str_joins_strings() {
        assert_eq!(
            active_roles_str(&json!({"active_roles": ["vadi", "prativadi"]})),
            "vadi, prativadi"
        );
        assert_eq!(active_roles_str(&json!({"active_roles": []})), "");
        assert_eq!(active_roles_str(&json!({})), "");
    }
}
