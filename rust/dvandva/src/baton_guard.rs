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

/// The model-visible warning injected (as PreToolUse `additionalContext`)
/// when the baton-creation SLA has expired. Warn-only by human decision
/// (run baton-guard-sla-scoping, Q1): the call is allowed and work
/// continues; the message spells the creation sequence and repeats on a
/// throttle until a baton exists.
pub const SLA_WARN_MESSAGE: &str = "dvandva baton-guard WARNING (not a block — this tool call ran): the baton-creation SLA has expired and no Dvandva baton exists yet. Create or resume it now: (1) run `dvandva resolve --role vadi` to get the run path, (2) Write the seed candidate to <run-dir>/baton.next.json, (3) run `dvandva write <run-dir>/baton.json <run-dir>/baton.next.json`. This warning repeats every ~5 minutes until a baton exists.";

/// Re-emit interval for the SLA breach warning, in seconds.
pub const WARN_INTERVAL_SECS: u64 = 300;

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
    /// No baton exists, the baton-creation SLA has expired, and the warn
    /// throttle has elapsed: allow the call but emit the model-visible
    /// warning. Carries a human-readable reason (marker age vs. threshold).
    WarnSla(String),
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
    /// Seconds since the last breach warning was emitted for this marker,
    /// if one ever was. `None` means never warned.
    pub last_warned_secs_ago: Option<u64>,
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

    /// Whether a breach warning should be (re-)emitted now: never warned,
    /// or the last warning is at least [`WARN_INTERVAL_SECS`] old.
    pub fn should_warn(&self) -> bool {
        self.last_warned_secs_ago
            .is_none_or(|ago| ago >= WARN_INTERVAL_SECS)
    }
}

/// Decide the PreToolUse outcome for `payload` (raw JSON bytes from stdin),
/// given the current baton-creation `sla` state.
///
/// Fails open (returns [`Decision::Allow`]) on empty/malformed JSON or a
/// missing `tool_name` — a guard defect must never brick an unrelated tool
/// call. The direct-edit integrity guard is the only hard block and wins in
/// every SLA state. The SLA itself is warn-only (run baton-guard-sla-scoping,
/// Q1/Q5): a breach never blocks; when the warn throttle has elapsed the
/// call is allowed with a model-visible warning attached.
pub fn should_block(payload: &[u8], sla: &SlaState) -> Decision {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return Decision::Allow;
    };
    let Some(tool_name) = value.get("tool_name").and_then(Value::as_str) else {
        return Decision::Allow;
    };

    // Baton-integrity guard first: a direct baton/history edit is blocked
    // in every SLA state (warn-only never applies to integrity).
    if GUARDED_TOOLS.contains(&tool_name) {
        if let Some(tool_input) = value.get("tool_input").and_then(Value::as_object) {
            let target = tool_input
                .get("file_path")
                .and_then(Value::as_str)
                .or_else(|| tool_input.get("notebook_path").and_then(Value::as_str));
            if let Some(path) = target {
                if is_guarded_path(path) {
                    return Decision::BlockDirectEdit;
                }
            }
        }
    }

    // Baton-creation SLA: breached + throttle elapsed => warn (allow with
    // context); breached + recently warned => silent allow.
    if let Some(reason) = sla.breached() {
        if sla.should_warn() {
            return Decision::WarnSla(reason);
        }
    }

    Decision::Allow
}

/// Extract the `session_id` Claude Code includes on every hook payload, if
/// present. Used by the CLI wrapper to scope SLA-marker ownership to the
/// session that armed it.
pub fn payload_session_id(payload: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(payload)
        .ok()?
        .get("session_id")?
        .as_str()
        .map(str::to_string)
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
            last_warned_secs_ago: None,
        }
    }

    fn sla_warned(marker_age_secs: u64, threshold_secs: u64, warned_ago: u64) -> SlaState {
        SlaState {
            last_warned_secs_ago: Some(warned_ago),
            ..sla(false, Some(marker_age_secs), threshold_secs)
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
    fn breach_warns_any_tool_when_never_warned() {
        let breached = sla(false, Some(999), 120);
        for (tool, input) in [
            ("Read", serde_json::json!({ "file_path": "src/lib.rs" })),
            ("Write", serde_json::json!({ "file_path": "some/file.txt" })),
            (
                "Bash",
                serde_json::json!({ "command": "dvandva --version && touch anything" }),
            ),
        ] {
            let body = payload(tool, input);
            assert!(
                matches!(should_block(&body, &breached), Decision::WarnSla(_)),
                "first breached {tool} call should warn (and run)"
            );
        }
    }

    #[test]
    fn breach_stays_silent_within_the_warn_throttle() {
        let recently = sla_warned(999, 120, 10);
        let body = payload("Write", serde_json::json!({ "file_path": "a.txt" }));
        assert_eq!(should_block(&body, &recently), Decision::Allow);
    }

    #[test]
    fn breach_rewarns_once_the_throttle_elapses() {
        let stale_warn = sla_warned(999, 120, WARN_INTERVAL_SECS);
        let body = payload("Write", serde_json::json!({ "file_path": "a.txt" }));
        assert!(matches!(
            should_block(&body, &stale_warn),
            Decision::WarnSla(_)
        ));
    }

    #[test]
    fn direct_edit_blocked_in_every_sla_state() {
        // Q5: warn-only applies to the creation SLA alone; baton integrity
        // keeps its hard block regardless of breach or throttle state.
        let body = payload(
            "Edit",
            serde_json::json!({ "file_path": "/repo/.dvandva/baton.json" }),
        );
        for state in [
            sla(true, None, 120),
            sla(false, Some(999), 120),
            sla_warned(999, 120, 10),
        ] {
            assert_eq!(should_block(&body, &state), Decision::BlockDirectEdit);
        }
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
