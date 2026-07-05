//! `baton_guard` logic — B4 target (plan: superpowers/plans/2026-07-02-flow-patches.html),
//! extended with a baton-creation SLA (plan:
//! superpowers/plans/2026-07-06-clarifying-questions-phase-plan.html#p3).
//!
//! Pure decision logic for the `dvandva baton-guard` PreToolUse hook: given a
//! Claude Code hook payload, decide whether the tool call should be blocked —
//! either because it directly edits a Dvandva baton or its history, or
//! because no baton has been created within the SLA window. The I/O
//! boundary (reading stdin, printing to stderr, returning a process exit
//! code, resolving the SLA state) lives in [`crate::cmd::baton_guard`].

use serde_json::Value;
use std::path::Component;

/// The message printed to stderr when a tool call is blocked for a direct
/// edit to the baton or its history.
pub const BLOCK_MESSAGE: &str = "dvandva baton-guard: direct edits to the Dvandva baton are blocked. Scaffold a candidate with `dvandva next` (it lists and generates the legal edges) and install it with `dvandva write` — never edit baton.json or its history directly. For a human_question or human_decision resume (which `dvandva next` may not scaffold), edit the CANDIDATE file (baton.next.json, never baton.json) to the intended non-terminal state, then run `dvandva write`.";

/// The message printed to stderr when a tool call is blocked because no
/// baton has been created within the baton-creation SLA window.
pub const SLA_BLOCK_MESSAGE: &str = "dvandva baton-guard: no Dvandva baton has been created yet and the baton-creation SLA has expired. Run `dvandva resolve` to check for an existing run, then `dvandva write` to install a fresh baton — no other tool call is permitted until a baton exists.";

/// Tool names the direct-edit guard inspects. Any other `tool_name` is
/// allowed without inspection by that guard (the SLA guard still applies).
const GUARDED_TOOLS: [&str; 4] = ["Write", "Edit", "MultiEdit", "NotebookEdit"];

/// The outcome of a [`should_block`] decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// The tool call may proceed.
    Allow,
    /// The tool call directly edits the baton or its history.
    BlockDirectEdit,
    /// No baton exists and the baton-creation SLA has expired. Carries a
    /// human-readable reason (marker age vs. threshold).
    BlockSla(String),
}

/// Baton-creation SLA state for the current PreToolUse invocation.
///
/// Pure and I/O-free by design: [`crate::cmd::baton_guard`] resolves the
/// real role, active-run lookup, and on-disk marker age, then hands the
/// result here so [`SlaState::breached`] and [`should_block`] stay
/// unit-testable without a filesystem or a real baton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaState {
    /// The current `DVANDVA_ROLE` (e.g. `vadi`).
    pub role: String,
    /// Whether a resolvable non-terminal baton already exists for this run.
    pub has_baton: bool,
    /// Age in seconds of the per-role pending marker, if one has been
    /// written this session. `None` when no marker has been written yet.
    pub marker_age_secs: Option<u64>,
    /// The SLA threshold in seconds (`DVANDVA_BATON_SLA_SECONDS`, default 120).
    pub threshold_secs: u64,
}

impl SlaState {
    /// Whether the baton-creation SLA has been breached.
    ///
    /// A baton already existing, or no marker age being available yet,
    /// always means "not breached" — this fails open until state can
    /// actually be read, then fails closed once it's overdue.
    pub fn breached(&self) -> Option<String> {
        if self.has_baton {
            return None;
        }
        match self.marker_age_secs {
            Some(age) if age >= self.threshold_secs => Some(format!(
                "no baton after {age}s (limit {}s)",
                self.threshold_secs
            )),
            _ => None,
        }
    }
}

/// Decide whether a PreToolUse hook `payload` (raw JSON bytes from stdin)
/// should be blocked, given the current baton-creation `sla` state.
///
/// Fails open (returns [`Decision::Allow`]) on empty/malformed JSON or a
/// missing `tool_name` — a guard defect must never brick an unrelated tool
/// call. A `Bash` invocation of `dvandva` itself is always allowed, even
/// past the SLA, so recovery (`dvandva resolve` / `dvandva write`) stays
/// reachable.
pub fn should_block(payload: &[u8], sla: &SlaState) -> Decision {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return Decision::Allow;
    };
    let Some(tool_name) = value.get("tool_name").and_then(Value::as_str) else {
        return Decision::Allow;
    };

    // Never block a `dvandva` invocation itself -- recovery must stay reachable.
    if tool_name == "Bash" {
        if let Some(cmd) = value
            .get("tool_input")
            .and_then(|v| v.get("command"))
            .and_then(Value::as_str)
        {
            let trimmed = cmd.trim();
            if trimmed == "dvandva" || trimmed.starts_with("dvandva ") {
                return Decision::Allow;
            }
        }
    }

    // SLA: no non-terminal baton yet and the marker is past threshold.
    if let Some(reason) = sla.breached() {
        return Decision::BlockSla(reason);
    }

    // Existing direct-edit guard, unchanged in spirit, now reached only
    // after the two checks above -- still tool-scoped to the 4 edit tools.
    if !GUARDED_TOOLS.contains(&tool_name) {
        return Decision::Allow;
    }
    let Some(tool_input) = value.get("tool_input").and_then(Value::as_object) else {
        return Decision::Allow;
    };
    let target = tool_input
        .get("file_path")
        .and_then(Value::as_str)
        .or_else(|| tool_input.get("notebook_path").and_then(Value::as_str));
    let Some(path) = target else {
        return Decision::Allow;
    };
    if is_guarded_path(path) {
        Decision::BlockDirectEdit
    } else {
        Decision::Allow
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sla(has_baton: bool, marker_age_secs: Option<u64>, threshold_secs: u64) -> SlaState {
        SlaState {
            role: "vadi".to_string(),
            has_baton,
            marker_age_secs,
            threshold_secs,
        }
    }

    #[test]
    fn sla_never_breaches_when_baton_exists() {
        assert_eq!(sla(true, Some(999_999), 1).breached(), None);
    }

    #[test]
    fn sla_never_breaches_without_a_marker_age() {
        assert_eq!(sla(false, None, 120).breached(), None);
    }

    #[test]
    fn sla_not_breached_below_threshold() {
        assert_eq!(sla(false, Some(119), 120).breached(), None);
    }

    #[test]
    fn sla_breached_at_threshold() {
        let reason = sla(false, Some(120), 120)
            .breached()
            .expect("should breach at the threshold");
        assert!(reason.contains("120s"), "reason: {reason}");
    }

    #[test]
    fn sla_breached_past_threshold_with_custom_limit() {
        assert!(sla(false, Some(6), 5).breached().is_some());
    }

    fn payload(tool_name: &str, tool_input: Value) -> Vec<u8> {
        serde_json::json!({ "tool_name": tool_name, "tool_input": tool_input })
            .to_string()
            .into_bytes()
    }

    #[test]
    fn dvandva_bash_command_always_allowed_even_when_sla_breached() {
        let breached = sla(false, Some(999), 120);
        let body = payload(
            "Bash",
            serde_json::json!({ "command": "dvandva write --foo" }),
        );
        assert_eq!(should_block(&body, &breached), Decision::Allow);
    }

    #[test]
    fn dvandva_bare_command_allowed() {
        let breached = sla(false, Some(999), 120);
        let body = payload("Bash", serde_json::json!({ "command": "dvandva" }));
        assert_eq!(should_block(&body, &breached), Decision::Allow);
    }

    #[test]
    fn sla_breach_blocks_any_tool_not_just_guarded_ones() {
        let breached = sla(false, Some(999), 120);
        let body = payload(
            "Read",
            serde_json::json!({ "file_path": "/repo/.dvandva/baton.json" }),
        );
        assert!(matches!(
            should_block(&body, &breached),
            Decision::BlockSla(_)
        ));
    }

    #[test]
    fn sla_not_breached_falls_through_to_direct_edit_guard() {
        let ok = sla(true, None, 120);
        let body = payload(
            "Edit",
            serde_json::json!({ "file_path": "/repo/.dvandva/baton.json" }),
        );
        assert_eq!(should_block(&body, &ok), Decision::BlockDirectEdit);
    }

    #[test]
    fn sla_not_breached_allows_non_guarded_tool() {
        let ok = sla(true, None, 120);
        let body = payload(
            "Read",
            serde_json::json!({ "file_path": "/repo/.dvandva/baton.json" }),
        );
        assert_eq!(should_block(&body, &ok), Decision::Allow);
    }
}
