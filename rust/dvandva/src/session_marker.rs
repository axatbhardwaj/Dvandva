//! Per-run session-binding markers: `.dvandva/runs/<run_id>/.sessions/<session_id>`.
//!
//! A Claude/Codex session becomes *bound* to a run the moment one of its tool
//! calls references that run's directory: the `baton-guard` PreToolUse hook —
//! which already sees every tool call and the payload `session_id` — stamps a
//! marker file under the run's `.sessions/` dir whose content records the
//! binding role (`vadi`/`prativadi`, or `unknown`). Persisting the role lets the
//! Stop hook render a role-correct resume command even in the real Claude
//! Stop-hook environment, where `DVANDVA_ROLE` is unset. The Stop hook then blocks
//! only sessions bound to a live walkaway run; a session that never touched the
//! run is a stranger and may end its turn freely (fail-open).
//!
//! Markers live under `.dvandva/`, which is gitignored in full, so they never
//! enter version control. The file-marker pattern mirrors [`crate::sla_marker`]
//! (same session-stamping mechanism), scoped per-run instead of per-repo.

use std::path::{Path, PathBuf};

/// The `.sessions` directory holding a run's binding markers, a sibling of its
/// `baton.json` (i.e. `<run_dir>/.sessions`).
pub fn sessions_dir(run_dir: &Path) -> PathBuf {
    run_dir.join(".sessions")
}

/// The marker path binding `session_id` to the run at `run_dir`.
pub fn marker_path(run_dir: &Path, session_id: &str) -> PathBuf {
    sessions_dir(run_dir).join(sanitize(session_id))
}

/// Whether `session_id` is stamped as bound to the run at `run_dir`.
pub fn is_bound(run_dir: &Path, session_id: &str) -> bool {
    marker_path(run_dir, session_id).is_file()
}

/// The peer role (`vadi`/`prativadi`) persisted in `session_id`'s marker for the
/// run at `run_dir`, or `None` when unbound or the marker records no known role
/// (`unknown`/empty). The Stop hook uses this to render a role-correct resume
/// command when `DVANDVA_ROLE` is absent from its environment.
pub fn bound_role(run_dir: &Path, session_id: &str) -> Option<String> {
    bound_role_at(&marker_path(run_dir, session_id))
}

/// [`bound_role`] against an already-built marker path.
fn bound_role_at(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let role = content.trim();
    (role == "vadi" || role == "prativadi").then(|| role.to_string())
}

/// Stamp `session_id` as bound to the run at `run_dir`, recording `role`
/// (`vadi`/`prativadi`, or `unknown` when the binding call named none) as the
/// marker's content. A no-op when the run directory does not exist, so a
/// mistyped or not-yet-created run path never litters stray marker dirs.
/// Idempotent and best-effort (I/O errors are swallowed — a binding miss must
/// never brick a tool call). A role-less call never downgrades a role already
/// persisted by an earlier call for the same session.
pub fn bind(run_dir: &Path, session_id: &str, role: Option<&str>) {
    if session_id.is_empty() || !run_dir.is_dir() {
        return;
    }
    let dir = sessions_dir(run_dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(sanitize(session_id));
    if role.is_none() && bound_role_at(&path).is_some() {
        return;
    }
    let _ = std::fs::write(path, role.unwrap_or("unknown").as_bytes());
}

/// The run directory names referenced by a raw tool-call payload `text`, via
/// either a `.dvandva/runs/<name>/` path OR an environment-selector assignment
/// in the command string (`DVANDVA_RUN_ID=<id>`, `DVANDVA_RUN_DIR=<dir>`,
/// `DVANDVA_BATON_FILE=<file>`). Binding follows any tool call that touches a
/// run's directory — running `dvandva wait`/`write`/`preflight` against its
/// baton, drafting its candidate, etc. A canonical selector call names its run
/// only through an env assignment (e.g. `DVANDVA_RUN_ID=live dvandva preflight`)
/// and carries no path, so the path scan alone would bind nothing.
/// Deduplicated, order-preserving, and skips `.`/`..` segments.
pub fn referenced_run_dirs(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // Path form: any `.dvandva/runs/<name>/` reference.
    const NEEDLE: &str = ".dvandva/runs/";
    let mut rest = text;
    while let Some(idx) = rest.find(NEEDLE) {
        rest = &rest[idx + NEEDLE.len()..];
        let name: String = rest
            .chars()
            .take_while(|&c| c != '/' && c != '"' && c != '\\' && !c.is_whitespace())
            .collect();
        push_run_name(&mut out, &name);
    }
    // Env-selector forms: a run named only via an env assignment in the command
    // string carries no `.dvandva/runs/` path, so the scan above misses it.
    for name in env_selector_run_names(text) {
        push_run_name(&mut out, &name);
    }
    out
}

/// Push `name` as a run-dir name if it is a usable single segment (non-empty,
/// not a `.`/`..` traversal token) not already collected.
fn push_run_name(out: &mut Vec<String>, name: &str) {
    if !name.is_empty() && name != "." && name != ".." && !out.iter().any(|n| n == name) {
        out.push(name.to_string());
    }
}

/// Run-dir names declared by the environment-selector assignments a Dvandva
/// caller uses when it spells no `.dvandva/runs/` path: `DVANDVA_RUN_ID=<id>`
/// (the id is the run-dir name), `DVANDVA_RUN_DIR=<dir>` (the name is the dir's
/// final path segment), and `DVANDVA_BATON_FILE=<dir>/baton*.json` (the name is
/// the baton file's parent segment).
fn env_selector_run_names(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for id in values_after(text, "DVANDVA_RUN_ID=") {
        // A run id is a single safe path segment; take it up to the first char
        // outside `[A-Za-z0-9._-]` so a stray separator can never widen it.
        let seg: String = id
            .chars()
            .take_while(|&c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            .collect();
        push_run_name(&mut out, &seg);
    }
    for dir in values_after(text, "DVANDVA_RUN_DIR=") {
        if let Some(name) = path_segments(&dir).last() {
            push_run_name(&mut out, name);
        }
    }
    for file in values_after(text, "DVANDVA_BATON_FILE=") {
        let segs = path_segments(&file);
        // The run dir is the baton file's parent: the second-to-last segment.
        if segs.len() >= 2 {
            push_run_name(&mut out, segs[segs.len() - 2]);
        }
    }
    out
}

/// The value tokens immediately following each occurrence of `prefix` in
/// `text`, each parsed by [`read_value`]. Empty tokens are dropped.
fn values_after(text: &str, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(idx) = rest.find(prefix) {
        rest = &rest[idx + prefix.len()..];
        let token = read_value(rest);
        if !token.is_empty() {
            out.push(token);
        }
    }
    out
}

/// Read a single assignment value from the start of `rest`. A value wrapped in
/// matching single or double quotes (the shell forms `KEY='v'` / `KEY="v"`)
/// yields its unquoted interior, so a quoted selector like
/// `DVANDVA_RUN_ID='live'` binds like the bare form; an unquoted value runs up
/// to the first whitespace, `"`, or `\` — the shell / JSON terminators of a
/// bare assignment value.
fn read_value(rest: &str) -> String {
    let mut chars = rest.chars();
    match chars.next() {
        Some(quote @ ('\'' | '"')) => chars.take_while(|&c| c != quote).collect(),
        _ => rest
            .chars()
            .take_while(|&c| c != '"' && c != '\\' && !c.is_whitespace())
            .collect(),
    }
}

/// The non-empty, non-`.`/`..` path segments of `path`, split on `/`.
fn path_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect()
}

/// The Dvandva peer role (`vadi`/`prativadi`) declared in a raw tool-call
/// payload `text`, via a `DVANDVA_ROLE=<role>` env assignment or a
/// `--role=<role>` / `--role <role>` flag in the command string. Returns `None`
/// when none is present or the token is not a peer role. Persisted in the
/// session marker at bind time so the Stop hook can name the role without a
/// `DVANDVA_ROLE` env of its own.
pub fn referenced_role(text: &str) -> Option<String> {
    for prefix in ["DVANDVA_ROLE=", "--role=", "--role "] {
        if let Some(role) = values_after(text, prefix)
            .into_iter()
            .find_map(|token| peer_role(&token))
        {
            return Some(role);
        }
    }
    None
}

/// `token` iff it is a peer role (`vadi`/`prativadi`).
fn peer_role(token: &str) -> Option<String> {
    matches!(token, "vadi" | "prativadi").then(|| token.to_string())
}

/// Collapse a `session_id` into a safe single path segment: anything outside
/// `[A-Za-z0-9._-]` becomes `_`, and the traversal tokens `.`/`..`/empty are
/// rewritten to `_`. The id comes from the hook payload (Claude-controlled),
/// but it still lands in a filesystem path, so it is neutralized before use.
fn sanitize(session_id: &str) -> String {
    let mapped: String = session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    match mapped.as_str() {
        "" | "." | ".." => "_".to_string(),
        _ => mapped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_then_is_bound_roundtrips_under_a_run_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run_dir = dir.path().join(".dvandva/runs/demo");
        std::fs::create_dir_all(&run_dir).expect("mkdir run");

        assert!(!is_bound(&run_dir, "sess-1"), "unbound before stamping");
        bind(&run_dir, "sess-1", Some("vadi"));
        assert!(is_bound(&run_dir, "sess-1"), "bound after stamping");
        assert_eq!(
            bound_role(&run_dir, "sess-1").as_deref(),
            Some("vadi"),
            "the persisted role roundtrips"
        );
        assert!(
            !is_bound(&run_dir, "sess-2"),
            "a different session stays unbound"
        );
    }

    #[test]
    fn bind_persists_role_and_unknown_when_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run_dir = dir.path().join(".dvandva/runs/demo");
        std::fs::create_dir_all(&run_dir).expect("mkdir run");

        bind(&run_dir, "sess-roleless", None);
        assert!(is_bound(&run_dir, "sess-roleless"), "still bound");
        assert_eq!(
            bound_role(&run_dir, "sess-roleless"),
            None,
            "an unknown-role marker reports no role"
        );
    }

    #[test]
    fn bind_never_downgrades_a_persisted_role() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run_dir = dir.path().join(".dvandva/runs/demo");
        std::fs::create_dir_all(&run_dir).expect("mkdir run");

        bind(&run_dir, "sess-1", Some("prativadi"));
        // A later role-less call for the same session must keep the role.
        bind(&run_dir, "sess-1", None);
        assert_eq!(bound_role(&run_dir, "sess-1").as_deref(), Some("prativadi"));
    }

    #[test]
    fn bind_is_a_noop_when_the_run_dir_is_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run_dir = dir.path().join(".dvandva/runs/ghost");
        bind(&run_dir, "sess-1", Some("vadi"));
        assert!(
            !sessions_dir(&run_dir).exists(),
            "no .sessions dir should be created for a missing run"
        );
    }

    #[test]
    fn referenced_run_dirs_binds_env_selector_forms() {
        // DVANDVA_RUN_ID with no path at all — the round-5 P1 case.
        assert_eq!(
            referenced_run_dirs("DVANDVA_RUN_ID=model-table-5.6 dvandva preflight"),
            vec!["model-table-5.6".to_string()]
        );
        // RUN_DIR names the run by its final segment; BATON_FILE by its parent.
        assert_eq!(
            referenced_run_dirs("DVANDVA_RUN_DIR=/abs/runs/alpha dvandva wait"),
            vec!["alpha".to_string()]
        );
        assert_eq!(
            referenced_run_dirs("DVANDVA_BATON_FILE=work/beta/baton.json dvandva write"),
            vec!["beta".to_string()]
        );
    }

    #[test]
    fn referenced_run_dirs_merges_path_and_env_forms_without_dupes() {
        let text = "DVANDVA_RUN_ID=live dvandva wait --file .dvandva/runs/live/baton.json";
        assert_eq!(referenced_run_dirs(text), vec!["live".to_string()]);
    }

    #[test]
    fn referenced_role_parses_env_and_flag_forms() {
        assert_eq!(
            referenced_role("DVANDVA_ROLE=vadi dvandva preflight").as_deref(),
            Some("vadi")
        );
        assert_eq!(
            referenced_role("dvandva wait --role prativadi --file x").as_deref(),
            Some("prativadi")
        );
        assert_eq!(
            referenced_role("dvandva wait --role=vadi").as_deref(),
            Some("vadi")
        );
        assert_eq!(referenced_role("dvandva wait --role team").as_deref(), None);
        assert_eq!(referenced_role("no role here").as_deref(), None);
    }

    #[test]
    fn referenced_run_dirs_extracts_and_dedupes() {
        let text = r#"{"tool_input":{"command":"dvandva wait --file .dvandva/runs/live/baton.json && dvandva write .dvandva/runs/live/baton.json"}}"#;
        assert_eq!(referenced_run_dirs(text), vec!["live".to_string()]);
    }

    #[test]
    fn referenced_run_dirs_handles_multiple_and_skips_traversal() {
        let text = ".dvandva/runs/a/baton.json .dvandva/runs/b/baton.next.json .dvandva/runs/../x";
        assert_eq!(
            referenced_run_dirs(text),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn referenced_run_dirs_ignores_the_bare_runs_dir() {
        assert!(referenced_run_dirs(".dvandva/runs/").is_empty());
        assert!(referenced_run_dirs("no runs here").is_empty());
    }

    #[test]
    fn sanitize_neutralizes_separators_and_traversal() {
        // A hostile id with separators cannot escape the `.sessions` dir: the
        // marker path stays a single segment under it.
        let dir = tempfile::tempdir().expect("tempdir");
        let run_dir = dir.path().join(".dvandva/runs/demo");
        std::fs::create_dir_all(&run_dir).expect("mkdir run");

        let marker = marker_path(&run_dir, "../../etc/passwd");
        assert_eq!(
            marker.parent(),
            Some(sessions_dir(&run_dir).as_path()),
            "sanitized marker must stay directly under .sessions"
        );
        assert_eq!(
            marker_path(&run_dir, ".."),
            sessions_dir(&run_dir).join("_")
        );
    }
}
