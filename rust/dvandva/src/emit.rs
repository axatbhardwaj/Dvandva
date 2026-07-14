//! JSON emit policy and `DVANDVA_*` token-line builders.
//!
//! Two concerns live here so both the binary and the parity harness share one
//! policy:
//!
//! * **JSON serialization** — always through `serde_json` compiled with the
//!   `preserve_order` feature, so object key order is insertion order (never
//!   sorted). [`to_json_pretty`] mirrors jq's default 2-space indent.
//! * **Token lines** — the exact stdout grammar (`RESOLVED`/`CREATE`/`ASK`)
//!   and the `DVANDVA_*` stderr diagnostics emitted by the resolve read path,
//!   built with exact single-space separation to match the shell output.

use serde::Serialize;

/// Serialize `value` to compact single-line JSON.
///
/// Object key order follows insertion order (the `preserve_order` feature is
/// enabled crate-wide), so the output is deterministic and never key-sorted.
pub fn to_json_compact<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(value)
}

/// Serialize `value` to pretty JSON with a 2-space indent, matching jq's
/// default output shape.
pub fn to_json_pretty<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

/// `RESOLVED <path>` — an existing baton was selected (resolve exit 0).
pub fn resolved_line(path: &str) -> String {
    format!("RESOLVED {path}")
}

/// `CREATE <path>` — no resumable run; a new named path is proposed (exit 0).
pub fn create_line(path: &str) -> String {
    format!("CREATE {path}")
}

/// `ASK <json-array>` — more than one resumable run; caller must stop (exit 12).
pub fn ask_line(json_array: &str) -> String {
    format!("ASK {json_array}")
}

/// Stderr diagnostic emitted when a candidate baton is not valid JSON
/// (fail-closed discovery).
pub fn dvandva_resolve_corrupt(path: &str, role: &str) -> String {
    format!("DVANDVA_RESOLVE corrupt_baton path={path} role={role}")
}

/// Stderr diagnostic emitted when discovery finds more than one resumable run.
pub fn dvandva_resolve_ask(role: &str, count: usize) -> String {
    format!("DVANDVA_RESOLVE ask role={role} reason=multiple_resumable_runs count={count}")
}

/// Stdout line surfacing the armed baton-creation SLA countdown, printed by
/// `resolve` and `preflight` on every invocation while the marker is armed
/// (turn-entry visibility on any engine, hook or no hook).
pub fn dvandva_sla_armed(role: &str, deadline_epoch: u64, threshold_secs: u64) -> String {
    format!("DVANDVA_SLA armed role={role} deadline={deadline_epoch} threshold_s={threshold_secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn compact_json_preserves_key_order() {
        // preserve_order: keys stay in insertion order, they are NOT sorted.
        let v: Value = serde_json::from_str(r#"{"b":1,"a":2,"c":3}"#).unwrap();
        assert_eq!(to_json_compact(&v).unwrap(), r#"{"b":1,"a":2,"c":3}"#);
    }

    #[test]
    fn pretty_json_uses_two_space_indent() {
        let v: Value = serde_json::from_str(r#"{"k":1}"#).unwrap();
        assert_eq!(to_json_pretty(&v).unwrap(), "{\n  \"k\": 1\n}");
    }

    #[test]
    fn outcome_lines_have_exact_single_space() {
        assert_eq!(
            resolved_line(".dvandva/runs/x/baton.json"),
            "RESOLVED .dvandva/runs/x/baton.json"
        );
        assert_eq!(
            create_line(".dvandva/runs/run/baton.json"),
            "CREATE .dvandva/runs/run/baton.json"
        );
        assert_eq!(ask_line("[]"), "ASK []");
    }

    #[test]
    fn dvandva_sla_armed_line_is_exact() {
        assert_eq!(
            dvandva_sla_armed("vadi", 1700000120, 120),
            "DVANDVA_SLA armed role=vadi deadline=1700000120 threshold_s=120"
        );
    }

    #[test]
    fn dvandva_diagnostic_lines_match_shell() {
        assert_eq!(
            dvandva_resolve_corrupt(".dvandva/baton.json", "vadi"),
            "DVANDVA_RESOLVE corrupt_baton path=.dvandva/baton.json role=vadi"
        );
        assert_eq!(
            dvandva_resolve_ask("prativadi", 3),
            "DVANDVA_RESOLVE ask role=prativadi reason=multiple_resumable_runs count=3"
        );
    }
}
