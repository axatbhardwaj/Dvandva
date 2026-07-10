//! Per-run session-binding markers: `.dvandva/runs/<run_id>/.sessions/<session_id>`.
//!
//! A Claude/Codex session becomes *bound* to a run the moment one of its tool
//! calls references that run's directory: the `baton-guard` PreToolUse hook —
//! which already sees every tool call and the payload `session_id` — stamps an
//! empty marker file under the run's `.sessions/` dir. The Stop hook then blocks
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

/// Stamp `session_id` as bound to the run at `run_dir`. A no-op when the run
/// directory does not exist, so a mistyped or not-yet-created run path never
/// litters stray marker dirs. Idempotent and best-effort (I/O errors are
/// swallowed — a binding miss must never brick a tool call).
pub fn bind(run_dir: &Path, session_id: &str) {
    if session_id.is_empty() || !run_dir.is_dir() {
        return;
    }
    let dir = sessions_dir(run_dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = std::fs::write(dir.join(sanitize(session_id)), b"");
}

/// The run directory names referenced by `.dvandva/runs/<name>/` paths anywhere
/// in `text` (a raw tool-call payload). Binding follows any tool call that
/// touches a run's directory — running `dvandva wait`/`write` against its
/// baton, drafting its candidate, etc. Deduplicated, order-preserving, and
/// skips `.`/`..` segments.
pub fn referenced_run_dirs(text: &str) -> Vec<String> {
    const NEEDLE: &str = ".dvandva/runs/";
    let mut out: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(idx) = rest.find(NEEDLE) {
        rest = &rest[idx + NEEDLE.len()..];
        let name: String = rest
            .chars()
            .take_while(|&c| c != '/' && c != '"' && c != '\\' && !c.is_whitespace())
            .collect();
        if !name.is_empty() && name != "." && name != ".." && !out.contains(&name) {
            out.push(name);
        }
    }
    out
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
        bind(&run_dir, "sess-1");
        assert!(is_bound(&run_dir, "sess-1"), "bound after stamping");
        assert!(
            !is_bound(&run_dir, "sess-2"),
            "a different session stays unbound"
        );
    }

    #[test]
    fn bind_is_a_noop_when_the_run_dir_is_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run_dir = dir.path().join(".dvandva/runs/ghost");
        bind(&run_dir, "sess-1");
        assert!(
            !sessions_dir(&run_dir).exists(),
            "no .sessions dir should be created for a missing run"
        );
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
