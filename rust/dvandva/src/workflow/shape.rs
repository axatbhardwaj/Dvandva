//! Shape validation for the `run_workflow` (v3) baton field.
//!
//! [`validate_run_workflow`] checks the *shape* of a declared workflow: field
//! presence/typing, the `source` tag, state-token membership in a
//! caller-supplied catalog (design decision D1 — per-run custom tokens
//! outside the catalog are a deferred follow-on), owner/class enums, edge
//! endpoint references, declare/approve stamp integrity (peer approval, not
//! self-approval), and amendment entries.
//!
//! Graph *semantics* — reachability, review-gate cuts, absorbing-state
//! guarantees — are explicitly out of scope here; that is a separate P2
//! invariants module layered on top of a shape-valid workflow.
//!
//! Checks run in a fixed order so a fixture violating multiple rules always
//! reports the earliest one: field presence/typing, then `source`, then
//! `states` (catalog membership, then owner/class enums, then duplicate-name
//! detection last), then `edges`, then the declare/approve stamps, then
//! `amendments`.

use std::collections::HashSet;

use serde_json::Value;

/// The four legal state/role owners.
const OWNERS: [&str; 4] = ["vadi", "prativadi", "team", "human"];
/// The five legal state classes.
const CLASSES: [&str; 5] = ["work", "review_gate", "human_gate", "pause", "terminal"];
/// The two coordinating roles allowed to declare/approve/amend a workflow.
const ROLES: [&str; 2] = ["vadi", "prativadi"];
/// The five recognised `preset:<name>` suffixes.
const PRESET_NAMES: [&str; 5] = ["fast", "standard", "full", "research", "review"];

/// Why a `run_workflow` value failed shape validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeError {
    /// A `states[].name` (or `amendments[].resume_status`) outside the
    /// caller-supplied catalog.
    UnknownStateToken(String),
    /// A `states[].name` appeared more than once in the declaration.
    DuplicateStateToken(String),
    /// A `states[].owner` outside the 4-value owner set.
    BadOwner(String),
    /// A `states[].class` outside the 5-value class set.
    BadClass(String),
    /// An `edges[].from`/`edges[].to` naming a state not declared in `states`.
    DanglingEdge(String),
    /// A `source` value that is neither `"custom"` nor a recognised
    /// `"preset:<name>"`.
    BadSource(String),
    /// A malformed declare/approve stamp: bad role, self-approval, or a
    /// checkpoint ordering violation.
    BadApprovalStamp(String),
    /// A malformed `amendments[]` entry.
    BadAmendment(String),
    /// A required field is absent or the wrong JSON type.
    MissingField(String),
}

fn require_field<'a>(v: &'a Value, key: &str) -> Result<&'a Value, ShapeError> {
    v.get(key)
        .ok_or_else(|| ShapeError::MissingField(key.to_string()))
}

fn require_str<'a>(v: &'a Value, key: &str) -> Result<&'a str, ShapeError> {
    require_field(v, key)?
        .as_str()
        .ok_or_else(|| ShapeError::MissingField(key.to_string()))
}

fn require_i64(v: &Value, key: &str) -> Result<i64, ShapeError> {
    require_field(v, key)?
        .as_i64()
        .ok_or_else(|| ShapeError::MissingField(key.to_string()))
}

fn require_array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, ShapeError> {
    require_field(v, key)?
        .as_array()
        .ok_or_else(|| ShapeError::MissingField(key.to_string()))
}

/// A required field whose value may explicitly be JSON `null`; still an
/// error if the key is entirely absent.
fn require_nullable_str<'a>(v: &'a Value, key: &str) -> Result<Option<&'a str>, ShapeError> {
    let val = require_field(v, key)?;
    if val.is_null() {
        return Ok(None);
    }
    val.as_str()
        .map(Some)
        .ok_or_else(|| ShapeError::MissingField(key.to_string()))
}

fn require_nullable_i64(v: &Value, key: &str) -> Result<Option<i64>, ShapeError> {
    let val = require_field(v, key)?;
    if val.is_null() {
        return Ok(None);
    }
    val.as_i64()
        .map(Some)
        .ok_or_else(|| ShapeError::MissingField(key.to_string()))
}

fn validate_source(source: &str) -> Result<(), ShapeError> {
    if source == "custom" {
        return Ok(());
    }
    if let Some(name) = source.strip_prefix("preset:") {
        if PRESET_NAMES.contains(&name) {
            return Ok(());
        }
    }
    Err(ShapeError::BadSource(source.to_string()))
}

/// Validates `states[]`, returning the declared state names (in declaration
/// order) for the subsequent edge-reference check. Per entry, checks catalog
/// membership, then the owner/class enums, then (last) whether the name has
/// already appeared earlier in `states[]` — so a duplicate name is only
/// reported once every earlier-checked rule on that entry, and every
/// preceding entry, has already passed.
fn validate_states(states: &[Value], catalog: &[&str]) -> Result<Vec<String>, ShapeError> {
    let mut names = Vec::with_capacity(states.len());
    let mut seen = HashSet::with_capacity(states.len());
    for s in states {
        let name = s
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ShapeError::MissingField("states[].name".to_string()))?;
        let owner = s
            .get("owner")
            .and_then(Value::as_str)
            .ok_or_else(|| ShapeError::MissingField("states[].owner".to_string()))?;
        let class = s
            .get("class")
            .and_then(Value::as_str)
            .ok_or_else(|| ShapeError::MissingField("states[].class".to_string()))?;
        if !catalog.contains(&name) {
            return Err(ShapeError::UnknownStateToken(name.to_string()));
        }
        if !OWNERS.contains(&owner) {
            return Err(ShapeError::BadOwner(owner.to_string()));
        }
        if !CLASSES.contains(&class) {
            return Err(ShapeError::BadClass(class.to_string()));
        }
        if !seen.insert(name) {
            return Err(ShapeError::DuplicateStateToken(name.to_string()));
        }
        names.push(name.to_string());
    }
    Ok(names)
}

fn validate_edges(edges: &[Value], state_names: &[String]) -> Result<(), ShapeError> {
    for e in edges {
        let from = e
            .get("from")
            .and_then(Value::as_str)
            .ok_or_else(|| ShapeError::MissingField("edges[].from".to_string()))?;
        let to = e
            .get("to")
            .and_then(Value::as_str)
            .ok_or_else(|| ShapeError::MissingField("edges[].to".to_string()))?;
        if !state_names.iter().any(|n| n == from) {
            return Err(ShapeError::DanglingEdge(from.to_string()));
        }
        if !state_names.iter().any(|n| n == to) {
            return Err(ShapeError::DanglingEdge(to.to_string()));
        }
    }
    Ok(())
}

/// Validates the declare/approve stamps: `declared_by` must be a role,
/// `declared_at_checkpoint` non-negative; when `approved_by` is non-null it
/// must be a *different* role (peer approval, never self-approval) and
/// `approved_at_checkpoint` must be non-null, non-negative, and at or after
/// `declared_at_checkpoint`.
fn validate_stamps(
    declared_by: &str,
    declared_at_checkpoint: i64,
    approved_by: Option<&str>,
    approved_at_checkpoint: Option<i64>,
) -> Result<(), ShapeError> {
    if !ROLES.contains(&declared_by) {
        return Err(ShapeError::BadApprovalStamp(format!(
            "declared_by={declared_by}"
        )));
    }
    if declared_at_checkpoint < 0 {
        return Err(ShapeError::BadApprovalStamp(format!(
            "declared_at_checkpoint={declared_at_checkpoint} must be non-negative"
        )));
    }
    match (approved_by, approved_at_checkpoint) {
        (None, None) => Ok(()),
        (None, Some(cp)) => Err(ShapeError::BadApprovalStamp(format!(
            "approved_at_checkpoint={cp} set without approved_by"
        ))),
        (Some(role), None) => Err(ShapeError::BadApprovalStamp(format!(
            "approved_by={role} set without approved_at_checkpoint"
        ))),
        (Some(role), Some(cp)) => {
            if !ROLES.contains(&role) {
                return Err(ShapeError::BadApprovalStamp(format!("approved_by={role}")));
            }
            if role == declared_by {
                return Err(ShapeError::BadApprovalStamp(format!(
                    "approved_by={role} matches declared_by (self-approval)"
                )));
            }
            if cp < 0 {
                return Err(ShapeError::BadApprovalStamp(format!(
                    "approved_at_checkpoint={cp} must be non-negative"
                )));
            }
            if cp < declared_at_checkpoint {
                return Err(ShapeError::BadApprovalStamp(format!(
                    "approved_at_checkpoint={cp} precedes declared_at_checkpoint={declared_at_checkpoint}"
                )));
            }
            Ok(())
        }
    }
}

/// Validates `amendments[]` entries: `proposed_by` must be a role,
/// `at_checkpoint` non-negative, `resume_status` in the caller-supplied
/// catalog; when `approved_by` is non-null it must be a *different* role
/// (peer approval, never self-approval) and `approved_at_checkpoint` must be
/// non-null, non-negative, and at or after `at_checkpoint`. This mirrors the
/// stamp-symmetry rule `validate_stamps` enforces at the top level —
/// `approved_by` and `approved_at_checkpoint` must be set together or not at
/// all — applied per amendment entry.
fn validate_amendments(amendments: &[Value], catalog: &[&str]) -> Result<(), ShapeError> {
    for a in amendments {
        let proposed_by = a
            .get("proposed_by")
            .and_then(Value::as_str)
            .ok_or_else(|| ShapeError::MissingField("amendments[].proposed_by".to_string()))?;
        let at_checkpoint = a
            .get("at_checkpoint")
            .and_then(Value::as_i64)
            .ok_or_else(|| ShapeError::MissingField("amendments[].at_checkpoint".to_string()))?;
        let resume_status = a
            .get("resume_status")
            .and_then(Value::as_str)
            .ok_or_else(|| ShapeError::MissingField("amendments[].resume_status".to_string()))?;
        let approved_by_val = a
            .get("approved_by")
            .ok_or_else(|| ShapeError::MissingField("amendments[].approved_by".to_string()))?;
        let approved_at_checkpoint =
            require_nullable_i64(a, "approved_at_checkpoint").map_err(|_| {
                ShapeError::MissingField("amendments[].approved_at_checkpoint".to_string())
            })?;

        if !ROLES.contains(&proposed_by) {
            return Err(ShapeError::BadAmendment(format!(
                "proposed_by={proposed_by}"
            )));
        }
        if at_checkpoint < 0 {
            return Err(ShapeError::BadAmendment(format!(
                "at_checkpoint={at_checkpoint} must be non-negative"
            )));
        }
        if !catalog.contains(&resume_status) {
            return Err(ShapeError::BadAmendment(format!(
                "resume_status={resume_status} not in catalog"
            )));
        }

        let approved_by =
            if approved_by_val.is_null() {
                None
            } else {
                Some(approved_by_val.as_str().ok_or_else(|| {
                    ShapeError::MissingField("amendments[].approved_by".to_string())
                })?)
            };
        match (approved_by, approved_at_checkpoint) {
            (None, None) => {}
            (None, Some(cp)) => {
                return Err(ShapeError::BadAmendment(format!(
                    "approved_at_checkpoint={cp} set without approved_by"
                )));
            }
            (Some(role), None) => {
                return Err(ShapeError::BadAmendment(format!(
                    "approved_by={role} set without approved_at_checkpoint"
                )));
            }
            (Some(role), Some(cp)) => {
                if !ROLES.contains(&role) {
                    return Err(ShapeError::BadAmendment(format!("approved_by={role}")));
                }
                if role == proposed_by {
                    return Err(ShapeError::BadAmendment(format!(
                        "approved_by={role} matches proposed_by (self-approval)"
                    )));
                }
                if cp < 0 {
                    return Err(ShapeError::BadAmendment(format!(
                        "approved_at_checkpoint={cp} must be non-negative"
                    )));
                }
                if cp < at_checkpoint {
                    return Err(ShapeError::BadAmendment(format!(
                        "approved_at_checkpoint={cp} precedes at_checkpoint={at_checkpoint}"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Validates the shape of a `run_workflow` (v3 baton field) value against a
/// caller-supplied state-token catalog.
///
/// Checks run in a fixed order — field presence/typing, `source`, `states`
/// (catalog membership, owner/class enums, duplicate-name detection last),
/// `edges`, the declare/approve stamps, then `amendments` — so a fixture
/// violating multiple rules always surfaces the earliest one.
///
/// Does not validate graph topology semantics (reachability, review-gate
/// cuts, absorbing states); that is a separate invariants layer.
pub fn validate_run_workflow(rw: &Value, catalog: &[&str]) -> Result<(), ShapeError> {
    // ---- field presence / typing -------------------------------------------
    let source = require_str(rw, "source")?;
    let declared_by = require_str(rw, "declared_by")?;
    let declared_at_checkpoint = require_i64(rw, "declared_at_checkpoint")?;
    let approved_by = require_nullable_str(rw, "approved_by")?;
    let approved_at_checkpoint = require_nullable_i64(rw, "approved_at_checkpoint")?;
    require_i64(rw, "revision_round")?;
    let states = require_array(rw, "states")?;
    let edges = require_array(rw, "edges")?;
    let amendments = require_array(rw, "amendments")?;

    // ---- source -------------------------------------------------------------
    validate_source(source)?;

    // ---- states ---------------------------------------------------------------
    let state_names = validate_states(states, catalog)?;

    // ---- edges ----------------------------------------------------------------
    validate_edges(edges, &state_names)?;

    // ---- declare/approve stamps -------------------------------------------
    validate_stamps(
        declared_by,
        declared_at_checkpoint,
        approved_by,
        approved_at_checkpoint,
    )?;

    // ---- amendments -----------------------------------------------------------
    validate_amendments(amendments, catalog)?;

    Ok(())
}
