//! The `BATON_STATE_COMPACT` projection.
//!
//! Filled by ws-4 (prativadi): mirrors `dvandva-state.sh`.

use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Number, Value};

use crate::Role;

const STRING_LIMIT: usize = 240;
const ACTION_LIMIT: usize = 500;
const ITEM_LIMIT: usize = 10;

/// Read and project a baton into the bounded `BATON_STATE_COMPACT` shape.
pub fn compact_state_from_file(path: &Path, role: Role) -> Result<Value, StateError> {
    if !path.is_file() {
        return Err(StateError::MissingFile {
            path: path.display().to_string(),
        });
    }

    let text = fs::read_to_string(path).map_err(|_| StateError::MissingFile {
        path: path.display().to_string(),
    })?;
    let value: Value = serde_json::from_str(&text).map_err(|_| StateError::InvalidJson {
        path: path.display().to_string(),
    })?;
    if !value.is_object() {
        return Err(StateError::NonObject {
            path: path.display().to_string(),
        });
    }

    Ok(compact_state(&value, &path.display().to_string(), role))
}

/// State helper errors and their shell-compatible exit codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    MissingFile { path: String },
    InvalidJson { path: String },
    NonObject { path: String },
}

impl StateError {
    pub fn exit_code(&self) -> i32 {
        match self {
            StateError::MissingFile { .. } => 21,
            StateError::InvalidJson { .. } | StateError::NonObject { .. } => 22,
        }
    }
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::MissingFile { path } => write!(f, "baton file not found: {path}"),
            StateError::InvalidJson { path } => write!(f, "baton JSON invalid: {path}"),
            StateError::NonObject { path } => write!(f, "baton JSON root must be object: {path}"),
        }
    }
}

impl std::error::Error for StateError {}

fn compact_state(root: &Value, baton_file: &str, role: Role) -> Value {
    let mut out = Map::new();
    out.insert(
        "kind".to_string(),
        Value::String("BATON_STATE_COMPACT".to_string()),
    );
    out.insert(
        "baton_file".to_string(),
        Value::String(baton_file.to_string()),
    );
    out.insert("role".to_string(), Value::String(role.as_str().to_string()));
    out.insert("schema".to_string(), clone_or_null(get(root, "schema")));
    out.insert("run_id".to_string(), clone_or_null(get(root, "run_id")));
    out.insert("mode".to_string(), clone_or_null(get(root, "mode")));
    out.insert("profile".to_string(), effective_profile(root));
    out.insert("profile_floor".to_string(), effective_profile_floor(root));
    out.insert("run_mode".to_string(), clone_or_null(get(root, "run_mode")));
    out.insert("phase".to_string(), clone_or_null(get(root, "phase")));
    out.insert("status".to_string(), clone_or_null(get(root, "status")));
    out.insert("assignee".to_string(), clone_or_null(get(root, "assignee")));
    out.insert(
        "active_roles".to_string(),
        match get(root, "active_roles") {
            Some(Value::Array(items)) => Value::Array(items.clone()),
            _ => Value::Array(Vec::new()),
        },
    );
    out.insert(
        "checkpoint".to_string(),
        clone_or_null(get(root, "checkpoint")),
    );
    out.insert("refs".to_string(), clean_refs(root));
    out.insert("counts".to_string(), counts(root));
    out.insert(
        "current_role_work".to_string(),
        Value::Array(compact_work(root, role)),
    );
    out.insert(
        "open_findings".to_string(),
        Value::Array(compact_findings(root)),
    );
    out.insert("verification_latest".to_string(), latest_verification(root));
    out.insert("next_action".to_string(), compact_next_action(root));
    Value::Object(out)
}

fn get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_object()?.get(key)
}

fn jq_coalesce(value: Option<&Value>) -> Option<&Value> {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => None,
        Some(value) => Some(value),
    }
}

fn clone_or_null(value: Option<&Value>) -> Value {
    jq_coalesce(value).cloned().unwrap_or(Value::Null)
}

fn num(value: usize) -> Value {
    Value::Number(Number::from(value as u64))
}

fn development_mode(root: &Value) -> bool {
    matches!(
        coalesce_tostring(get(root, "mode")).as_str(),
        "development" | "feature-pr"
    )
}

fn effective_profile(root: &Value) -> Value {
    if development_mode(root) {
        clone_or_null(get(root, "profile")).or_string_default("full")
    } else {
        clone_or_null(get(root, "profile"))
    }
}

fn effective_profile_floor(root: &Value) -> Value {
    if development_mode(root) {
        match jq_coalesce(get(root, "profile_floor")) {
            Some(value) => value.clone(),
            _ => effective_profile(root),
        }
    } else {
        clone_or_null(get(root, "profile_floor"))
    }
}

trait NullDefault {
    fn or_string_default(self, default: &str) -> Value;
}

impl NullDefault for Value {
    fn or_string_default(self, default: &str) -> Value {
        if matches!(self, Value::Null | Value::Bool(false)) {
            Value::String(default.to_string())
        } else {
            self
        }
    }
}

fn as_array(value: Option<&Value>) -> Vec<Value> {
    match value {
        Some(Value::Array(items)) => items.clone(),
        Some(Value::Object(map)) => map.values().cloned().collect(),
        _ => Vec::new(),
    }
}

fn count_value(value: Option<&Value>) -> usize {
    match value {
        Some(Value::Array(items)) => items.len(),
        Some(Value::Object(map)) => map.len(),
        _ => 0,
    }
}

fn counts(root: &Value) -> Value {
    let mut out = Map::new();
    out.insert(
        "work_split".to_string(),
        num(count_value(get(root, "work_split"))),
    );
    out.insert(
        "subagent_tracks".to_string(),
        num(count_value(get(root, "subagent_tracks"))),
    );
    out.insert(
        "verification_matrix".to_string(),
        num(count_value(get(root, "verification_matrix"))),
    );
    out.insert(
        "findings".to_string(),
        num(count_value(get(root, "findings"))),
    );
    out.insert(
        "blockers".to_string(),
        num(count_value(get(root, "blockers"))),
    );
    out.insert(
        "changed_paths".to_string(),
        num(count_value(get(root, "changed_paths"))),
    );
    Value::Object(out)
}

// Mirror of jq's `bounded_scalar`: strings are bounded to `max` codepoints; any
// other scalar (number/bool) is stringified via `to_string()` then bounded,
// exactly as jq applies `tostring`. With serde_json's `arbitrary_precision`
// feature the numeric literal is preserved verbatim, so integers and decimals
// stringify identically to jq (`1.50` -> "1.50", `42` -> "42").
//
// KNOWN RESIDUAL (exponential): jq's `tostring` normalizes exponential literals
// to uppercase-E form (`1e10` -> "1E+10") while serde emits lowercase-e
// (`1e10` -> "1e+10"). This narrow E-case divergence affects ONLY synthetic
// batons — no real Dvandva baton carries an exponential number in a
// string-context field. See rust/dvandva/README.md "Known limitations". Do not
// emit numbers as JSON numbers here: jq stringifies them in this path, so
// preserving the number type would DIVERGE from the shell (see F4 in the
// differential parity harness).
fn bounded_scalar(value: Option<&Value>, max: usize) -> Value {
    match value {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) => Value::String(bound_string(s, max)),
        Some(other) => Value::String(bound_string(&other.to_string(), max)),
    }
}

fn bound_string(value: &str, max: usize) -> String {
    if value.chars().count() > max {
        let prefix: String = value.chars().take(max).collect();
        format!("{prefix}...[truncated]")
    } else {
        value.to_string()
    }
}

fn coalesce_tostring(value: Option<&Value>) -> String {
    match jq_coalesce(value) {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

fn clean_refs(root: &Value) -> Value {
    let mut out = Map::new();
    if let Some(Value::Object(refs)) = get(root, "refs") {
        for (key, value) in refs {
            if matches!(
                key.as_str(),
                "branch"
                    | "base"
                    | "plan"
                    | "plan_ref"
                    | "research_ref"
                    | "run_explainer_ref"
                    | "review_ref"
            ) {
                out.insert(key.clone(), bounded_scalar(Some(value), STRING_LIMIT));
            }
        }
    }

    for key in [
        "research_ref",
        "plan_ref",
        "run_explainer_ref",
        "review_ref",
    ] {
        out.insert(
            key.to_string(),
            bounded_scalar(get(root, key), STRING_LIMIT),
        );
    }

    out.retain(|_, value| is_present(value));
    Value::Object(out)
}

fn is_present(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(s) => !s.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        _ => true,
    }
}

fn compact_work(root: &Value, role: Role) -> Vec<Value> {
    let current_phase = coalesce_tostring(get(root, "phase"));
    let current_status = coalesce_tostring(get(root, "status"));
    let mut items = Vec::new();

    for item in as_array(get(root, "work_split")) {
        let item_phase = coalesce_tostring(get(&item, "phase"));
        if item_phase != current_phase {
            continue;
        }

        let chunk_type = first_present(&item, &["chunk_type", "type"])
            .cloned()
            .unwrap_or_else(|| Value::String("implementation".to_string()));
        if current_status == "parallel_implementing"
            && coalesce_tostring(Some(&chunk_type)) != "implementation"
        {
            continue;
        }

        let owner = first_present(&item, &["owner_role", "owner"]);
        if coalesce_tostring(owner) != role.as_str() {
            continue;
        }

        let mut out = Map::new();
        out.insert("id".to_string(), clone_or_null(get(&item, "id")));
        out.insert("phase".to_string(), clone_or_null(get(&item, "phase")));
        out.insert("chunk_type".to_string(), chunk_type);
        out.insert("owner_role".to_string(), clone_or_null(owner));
        out.insert("status".to_string(), clone_or_null(get(&item, "status")));
        out.insert(
            "paths_count".to_string(),
            num(count_value(get(&item, "paths"))),
        );
        out.insert(
            "write_paths_count".to_string(),
            num(count_value(get(&item, "write_paths"))),
        );
        out.insert(
            "depends_on_count".to_string(),
            num(count_value(get(&item, "depends_on"))),
        );
        items.push(Value::Object(out));
    }

    cap_items(items)
}

fn first_present<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| jq_coalesce(get(value, key)))
}

fn compact_findings(root: &Value) -> Vec<Value> {
    let mut findings = Vec::new();
    for item in as_array(get(root, "findings")) {
        if !is_open_finding(&item) {
            continue;
        }
        if item.is_object() {
            let mut out = Map::new();
            out.insert("id".to_string(), clone_or_null(get(&item, "id")));
            out.insert(
                "severity".to_string(),
                clone_or_null(get(&item, "severity")),
            );
            out.insert("area".to_string(), clone_or_null(get(&item, "area")));
            out.insert(
                "status".to_string(),
                match jq_coalesce(get(&item, "status")) {
                    Some(value) => value.clone(),
                    _ => Value::String("open".to_string()),
                },
            );
            out.insert(
                "summary".to_string(),
                bounded_scalar(get(&item, "summary"), STRING_LIMIT),
            );
            out.retain(|_, value| !value.is_null() && *value != Value::String(String::new()));
            findings.push(Value::Object(out));
        } else {
            let mut out = Map::new();
            out.insert("id".to_string(), Value::Null);
            out.insert("severity".to_string(), Value::Null);
            out.insert("area".to_string(), Value::Null);
            out.insert("status".to_string(), Value::String("open".to_string()));
            out.insert(
                "summary".to_string(),
                bounded_scalar(Some(&item), STRING_LIMIT),
            );
            findings.push(Value::Object(out));
        }
    }
    cap_items(findings)
}

fn is_open_finding(value: &Value) -> bool {
    if value.is_object() {
        let status = match jq_coalesce(get(value, "status")) {
            Some(raw) => coalesce_tostring(Some(raw)).to_ascii_lowercase(),
            _ => "open".to_string(),
        };
        !matches!(
            status.as_str(),
            "closed" | "resolved" | "completed" | "approved"
        )
    } else {
        true
    }
}

fn latest_verification(root: &Value) -> Value {
    if matches!(get(root, "verification_latest"), Some(Value::Object(_))) {
        return compact_verification(get(root, "verification_latest"));
    }

    if let Some(Value::Array(items)) = get(root, "verification") {
        if let Some(last) = items.last() {
            return compact_verification(Some(last));
        }
    }

    Value::Object(Map::new())
}

fn compact_verification(value: Option<&Value>) -> Value {
    match value {
        Some(Value::Object(_)) => {
            let mut out = Map::new();
            out.insert(
                "command".to_string(),
                bounded_scalar(value.and_then(|v| get(v, "command")), STRING_LIMIT),
            );
            out.insert(
                "result".to_string(),
                bounded_scalar(value.and_then(|v| get(v, "result")), 80),
            );
            out.insert(
                "notes".to_string(),
                bounded_scalar(value.and_then(|v| get(v, "notes")), STRING_LIMIT),
            );
            out.retain(|_, field| !field.is_null() && *field != Value::String(String::new()));
            Value::Object(out)
        }
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(other) => {
            let mut out = Map::new();
            out.insert(
                "command".to_string(),
                bounded_scalar(Some(other), STRING_LIMIT),
            );
            out.insert("result".to_string(), Value::String("legacy".to_string()));
            Value::Object(out)
        }
    }
}

fn compact_next_action(root: &Value) -> Value {
    match get(root, "next_action") {
        Some(Value::Object(map)) => {
            let mut out = Map::new();
            for key in [
                "owner_role",
                "role",
                "assignee",
                "status",
                "prompt",
                "summary",
                "action",
                "command",
            ] {
                if let Some(value) = map.get(key) {
                    out.insert(key.to_string(), bounded_scalar(Some(value), ACTION_LIMIT));
                }
            }
            Value::Object(out)
        }
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(other) => bounded_scalar(Some(other), ACTION_LIMIT),
    }
}

fn cap_items(mut items: Vec<Value>) -> Vec<Value> {
    if items.len() > ITEM_LIMIT {
        let more = items.len() - ITEM_LIMIT;
        items.truncate(ITEM_LIMIT);
        let mut marker = Map::new();
        marker.insert("more_count".to_string(), num(more));
        items.push(Value::Object(marker));
    }
    items
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::json;

    use crate::Role;

    use super::{compact_state_from_file, StateError};

    fn temp_root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "dvandva-state-{name}-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_json(path: &Path, value: serde_json::Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    }

    #[test]
    fn compact_state_matches_parallel_work_contract() {
        let baton = temp_root("contract").join("baton.json");
        write_json(
            &baton,
            json!({
                "schema": "dvandva.baton.v2",
                "run_id": "state-contract",
                "mode": "development",
                "profile": "standard",
                "profile_floor": "standard",
                "run_mode": "walkaway",
                "phase": 1,
                "status": "parallel_implementing",
                "assignee": "team",
                "active_roles": ["vadi", "prativadi"],
                "checkpoint": 42,
                "refs": {"branch": "feat/rust", "plan": "superpowers/plans/rust.html", "huge": "drop"},
                "work_split": [
                    {"id": "vadi-1", "phase": 1, "chunk_type": "implementation", "owner_role": "vadi", "status": "ready", "paths": ["a"], "write_paths": ["a"], "depends_on": ["root"], "notes": "drop"},
                    {"id": "vadi-review", "phase": 1, "chunk_type": "cross_review", "owner_role": "vadi", "status": "planned", "paths": ["b"]},
                    {"id": "prativadi-1", "phase": 1, "chunk_type": "implementation", "owner_role": "prativadi", "status": "ready", "paths": ["c"]}
                ],
                "subagent_tracks": [{"id": "track"}],
                "verification_matrix": [{"id": "vm"}],
                "findings": [
                    {"id": "F-1", "status": "open", "severity": "medium", "summary": "open finding"},
                    {"id": "F-2", "status": "resolved", "severity": "low", "summary": "closed finding"}
                ],
                "blockers": [{"id": "B-1"}],
                "changed_paths": ["rust/dvandva/src/state.rs"],
                "verification_latest": {"command": "cargo test", "result": "passed", "notes": "ok", "extra": "drop"},
                "next_action": {"owner_role": "vadi", "prompt": "Continue.", "private": "drop"}
            }),
        );

        let state = compact_state_from_file(&baton, Role::Vadi).unwrap();

        assert_eq!(state["kind"], "BATON_STATE_COMPACT");
        assert_eq!(state["run_id"], "state-contract");
        assert_eq!(state["profile"], "standard");
        assert_eq!(state["profile_floor"], "standard");
        assert_eq!(state["counts"]["work_split"], 3);
        assert_eq!(state["counts"]["findings"], 2);
        assert_eq!(state["current_role_work"].as_array().unwrap().len(), 1);
        assert_eq!(state["current_role_work"][0]["id"], "vadi-1");
        assert!(state["current_role_work"][0].get("notes").is_none());
        assert_eq!(state["open_findings"].as_array().unwrap().len(), 1);
        assert_eq!(state["open_findings"][0]["id"], "F-1");
        assert_eq!(state["verification_latest"]["command"], "cargo test");
        assert!(state["verification_latest"].get("extra").is_none());
        assert_eq!(state["next_action"]["owner_role"], "vadi");
        assert!(state["next_action"].get("private").is_none());
        assert!(state.get("work_split").is_none());
        assert!(state.get("subagent_tracks").is_none());
        assert!(state["refs"].get("huge").is_none());
    }

    #[test]
    fn phase_less_work_is_not_dropped() {
        let baton = temp_root("phase-less").join("baton.json");
        write_json(
            &baton,
            json!({
                "schema": "dvandva.baton.v2",
                "run_id": "phase-less-run",
                "mode": "development",
                "status": "implementing",
                "assignee": "vadi",
                "checkpoint": 7,
                "work_split": [
                    {"id": "phase-less-work", "chunk_type": "implementation", "owner_role": "vadi", "status": "ready"}
                ],
                "subagent_tracks": [],
                "verification_matrix": [],
                "findings": [],
                "blockers": [],
                "changed_paths": []
            }),
        );

        let state = compact_state_from_file(&baton, Role::Vadi).unwrap();

        assert_eq!(state["phase"], serde_json::Value::Null);
        assert_eq!(state["profile"], "full");
        assert_eq!(state["profile_floor"], "full");
        assert_eq!(state["current_role_work"].as_array().unwrap().len(), 1);
        assert_eq!(state["current_role_work"][0]["id"], "phase-less-work");
    }

    #[test]
    fn legacy_string_fields_and_large_values_are_bounded() {
        let baton = temp_root("bounded").join("baton.json");
        let long = "x".repeat(1500);
        write_json(
            &baton,
            json!({
                "schema": "dvandva.baton.v2",
                "run_id": "bounded-run",
                "mode": "development",
                "status": "implementing",
                "assignee": "vadi",
                "phase": 1,
                "checkpoint": 9,
                "research_ref": format!("./superpowers/research/{long}.html"),
                "work_split": (0..15).map(|i| json!({"id": format!("work-{i}"), "phase": 1, "chunk_type": "implementation", "owner_role": "vadi", "status": "ready"})).collect::<Vec<_>>(),
                "subagent_tracks": [],
                "verification_matrix": [],
                "verification": [long.clone()],
                "findings": [long],
                "blockers": [],
                "changed_paths": [],
                "next_action": "y".repeat(1500)
            }),
        );

        let state = compact_state_from_file(&baton, Role::Vadi).unwrap();

        assert_eq!(state["current_role_work"].as_array().unwrap().len(), 11);
        assert_eq!(state["current_role_work"][10]["more_count"], 5);
        assert!(state["refs"]["research_ref"]
            .as_str()
            .unwrap()
            .ends_with("...[truncated]"));
        assert_eq!(state["verification_latest"]["result"], "legacy");
        assert!(state["verification_latest"]["command"]
            .as_str()
            .unwrap()
            .ends_with("...[truncated]"));
        assert_eq!(state["open_findings"][0]["status"], "open");
        assert!(state["next_action"]
            .as_str()
            .unwrap()
            .ends_with("...[truncated]"));
    }

    #[test]
    fn missing_invalid_and_non_object_inputs_return_shell_exit_codes() {
        let root = temp_root("errors");
        let missing = root.join("missing.json");
        assert_eq!(
            compact_state_from_file(&missing, Role::Vadi)
                .unwrap_err()
                .exit_code(),
            21
        );

        let invalid = root.join("invalid.json");
        fs::write(&invalid, "{ bad\n").unwrap();
        assert_eq!(
            compact_state_from_file(&invalid, Role::Vadi)
                .unwrap_err()
                .exit_code(),
            22
        );

        let non_object = root.join("array.json");
        fs::write(&non_object, "[]\n").unwrap();
        assert!(matches!(
            compact_state_from_file(&non_object, Role::Vadi).unwrap_err(),
            StateError::NonObject { .. }
        ));
    }
}
