//! `baton_guard` logic — B4 target (plan: superpowers/plans/2026-07-02-flow-patches.html).
//!
//! Pure decision logic for the `dvandva baton-guard` PreToolUse hook: given a
//! Claude Code hook payload, decide whether the tool call attempts a direct
//! edit to a Dvandva baton or its history and should be blocked. The I/O
//! boundary (reading stdin, printing to stderr, returning a process exit
//! code) lives in [`crate::cmd::baton_guard`].

use serde_json::Value;
use std::path::Component;

/// The message printed to stderr when a tool call is blocked.
pub const BLOCK_MESSAGE: &str = "dvandva baton-guard: direct edits to the Dvandva baton are blocked. Scaffold a candidate with `dvandva next` (it lists and generates the legal edges) and install it with `dvandva write` — never edit baton.json or its history directly. For a human_question or human_decision resume (which `dvandva next` may not scaffold), edit the CANDIDATE file (baton.next.json, never baton.json) to the intended non-terminal state, then run `dvandva write`.";

/// Tool names this guard inspects. Any other `tool_name` is allowed without
/// inspection.
const GUARDED_TOOLS: [&str; 4] = ["Write", "Edit", "MultiEdit", "NotebookEdit"];

/// Decide whether a PreToolUse hook `payload` (raw JSON bytes from stdin)
/// should be blocked. Returns `true` to block, `false` to allow.
///
/// Fails open (returns `false`) on empty/malformed JSON, a missing or
/// non-guarded `tool_name`, or a missing/non-string target path — a guard
/// defect must never brick an unrelated tool call.
pub fn should_block(payload: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return false;
    };
    let Some(tool_name) = value.get("tool_name").and_then(Value::as_str) else {
        return false;
    };
    if !GUARDED_TOOLS.contains(&tool_name) {
        return false;
    }
    let Some(tool_input) = value.get("tool_input").and_then(Value::as_object) else {
        return false;
    };
    let target = tool_input
        .get("file_path")
        .and_then(Value::as_str)
        .or_else(|| tool_input.get("notebook_path").and_then(Value::as_str));
    let Some(path) = target else {
        return false;
    };
    is_guarded_path(path)
}

/// Whether a (possibly relative, possibly nonexistent) `path` targets a
/// baton or its history: exact basename `baton.json` with a `.dvandva`
/// component above it, or any path with a `.dvandva` component followed
/// later by a `history` component.
fn is_guarded_path(path: &str) -> bool {
    let components = normalize_components(path);
    let Some(dvandva_index) = components.iter().position(|c| c == ".dvandva") else {
        return false;
    };
    if components.last().map(String::as_str) == Some("baton.json") {
        return true;
    }
    components[dvandva_index + 1..]
        .iter()
        .any(|c| c == "history")
}

/// Lexically normalize `path` into its component strings, resolving `.` and
/// `..` without touching the filesystem (the target file need not exist).
fn normalize_components(path: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for component in std::path::Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part.to_string_lossy().into_owned()),
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    out
}
