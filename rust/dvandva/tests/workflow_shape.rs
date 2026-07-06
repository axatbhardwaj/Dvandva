//! Integration tests for `dvandva::workflow::shape::validate_run_workflow`
//! — the shape validator for the `run_workflow` (v3 baton field) value.
//!
//! Covers, in the fixed check order the validator itself documents: field
//! presence/typing (`MissingField`), `source` (`BadSource`), state-token
//! catalog membership + owner/class enums + duplicate-name detection
//! (`UnknownStateToken` / `BadOwner` / `BadClass` / `DuplicateStateToken`),
//! edge endpoint references (`DanglingEdge`), the declare/approve stamps
//! (`BadApprovalStamp`), `amendments` (`BadAmendment`), and the five-preset
//! acceptance case.
//!
//! Does NOT cover graph topology semantics (reachability, review-gate cuts,
//! absorbing states) — that is a separate P2 invariants module layered on
//! top of a shape-valid workflow.

use dvandva::workflow::{preset, ShapeError, StateClass};
use serde_json::{json, Value};

const PRESET_NAMES: [&str; 5] = ["fast", "standard", "full", "research", "review"];

fn class_str(c: StateClass) -> &'static str {
    match c {
        StateClass::Work => "work",
        StateClass::ReviewGate => "review_gate",
        StateClass::HumanGate => "human_gate",
        StateClass::Pause => "pause",
        StateClass::Terminal => "terminal",
    }
}

/// The union of every state name across all five presets — the catalog a
/// real caller would supply for a `run_workflow` sourced from any preset.
fn full_catalog() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for p in PRESET_NAMES {
        for s in preset(p).unwrap().states {
            if !names.contains(&s.name) {
                names.push(s.name);
            }
        }
    }
    names
}

/// A shape-valid `run_workflow` built from a named preset's states/edges,
/// with valid (non-self) declare/approve stamps and no amendments.
fn valid_workflow(preset_name: &str) -> Value {
    let g = preset(preset_name).unwrap();
    let states: Vec<Value> = g
        .states
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "owner": s.owner,
                "class": class_str(s.class),
            })
        })
        .collect();
    let edges: Vec<Value> = g
        .edges
        .iter()
        .map(|e| {
            json!({
                "from": e.from,
                "to": e.to,
                "loop_cap_key": e.loop_cap_key,
            })
        })
        .collect();
    json!({
        "source": format!("preset:{preset_name}"),
        "declared_by": "vadi",
        "declared_at_checkpoint": 0,
        "approved_by": "prativadi",
        "approved_at_checkpoint": 1,
        "revision_round": 0,
        "states": states,
        "edges": edges,
        "amendments": [],
    })
}

// ===================== 8: acceptance — all five presets =====================

#[test]
fn accepts_a_valid_workflow_for_every_preset() {
    let catalog = full_catalog();
    for name in PRESET_NAMES {
        let rw = valid_workflow(name);
        assert_eq!(
            dvandva::workflow::validate_run_workflow(&rw, &catalog),
            Ok(()),
            "preset {name} should be shape-valid"
        );
    }
}

// ===================== 7: field presence / typing =====================

#[test]
fn missing_source_is_missing_field() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw.as_object_mut().unwrap().remove("source");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::MissingField("source".to_string()))
    );
}

#[test]
fn mistyped_declared_at_checkpoint_is_missing_field() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["declared_at_checkpoint"] = json!("not-an-int");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::MissingField(
            "declared_at_checkpoint".to_string()
        ))
    );
}

#[test]
fn missing_amendments_is_missing_field() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw.as_object_mut().unwrap().remove("amendments");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::MissingField("amendments".to_string()))
    );
}

// ===================== 1: source =====================

#[test]
fn source_custom_is_accepted_shape() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["source"] = json!("custom");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Ok(())
    );
}

#[test]
fn source_unknown_preset_name_is_bad_source() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["source"] = json!("preset:bogus");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::BadSource("preset:bogus".to_string()))
    );
}

#[test]
fn source_freeform_junk_is_bad_source() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["source"] = json!("whatever");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::BadSource("whatever".to_string()))
    );
}

// ===================== 2: states — catalog membership (D1) =====================

#[test]
fn state_name_outside_catalog_is_unknown_state_token() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["states"][0]["name"] = json!("made_up_state");
    // Keep edges referencing the renamed state so this is purely a states
    // check, not incidentally a dangling edge.
    for e in rw["edges"].as_array_mut().unwrap() {
        if e["from"] == "clarifying_questions_drafting" {
            e["from"] = json!("made_up_state");
        }
    }
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::UnknownStateToken("made_up_state".to_string()))
    );
}

// ===================== 3: states — owner / class enums =====================

#[test]
fn bad_owner_is_rejected() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["states"][0]["owner"] = json!("nobody");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::BadOwner("nobody".to_string()))
    );
}

#[test]
fn bad_class_is_rejected() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["states"][0]["class"] = json!("bogus_class");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::BadClass("bogus_class".to_string()))
    );
}

// ===================== 4: edges — dangling references =====================

#[test]
fn dangling_edge_from_is_rejected() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["edges"][0]["from"] = json!("nowhere_state");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::DanglingEdge("nowhere_state".to_string()))
    );
}

#[test]
fn dangling_edge_to_is_rejected() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["edges"][0]["to"] = json!("nowhere_state");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::DanglingEdge("nowhere_state".to_string()))
    );
}

// ===================== 5: declare/approve stamps =====================

#[test]
fn declared_by_outside_role_set_is_bad_approval_stamp() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["declared_by"] = json!("human");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::BadApprovalStamp(
            "declared_by=human".to_string()
        ))
    );
}

#[test]
fn negative_declared_at_checkpoint_is_bad_approval_stamp() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["declared_at_checkpoint"] = json!(-1);
    let err = dvandva::workflow::validate_run_workflow(&rw, &catalog).unwrap_err();
    assert!(matches!(err, ShapeError::BadApprovalStamp(_)));
}

#[test]
fn self_approval_is_bad_approval_stamp() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["declared_by"] = json!("vadi");
    rw["approved_by"] = json!("vadi");
    let err = dvandva::workflow::validate_run_workflow(&rw, &catalog).unwrap_err();
    assert!(matches!(err, ShapeError::BadApprovalStamp(_)));
}

#[test]
fn approved_before_declared_checkpoint_is_bad_approval_stamp() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["declared_at_checkpoint"] = json!(5);
    rw["approved_at_checkpoint"] = json!(2);
    let err = dvandva::workflow::validate_run_workflow(&rw, &catalog).unwrap_err();
    assert!(matches!(err, ShapeError::BadApprovalStamp(_)));
}

#[test]
fn approved_at_checkpoint_equal_to_declared_is_accepted() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["declared_at_checkpoint"] = json!(3);
    rw["approved_at_checkpoint"] = json!(3);
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Ok(())
    );
}

#[test]
fn null_approval_is_accepted_shape() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["approved_by"] = json!(null);
    rw["approved_at_checkpoint"] = json!(null);
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Ok(())
    );
}

#[test]
fn approved_by_without_approved_at_checkpoint_is_bad_approval_stamp() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["approved_at_checkpoint"] = json!(null);
    let err = dvandva::workflow::validate_run_workflow(&rw, &catalog).unwrap_err();
    assert!(matches!(err, ShapeError::BadApprovalStamp(_)));
}

// ===================== 6: amendments =====================

#[test]
fn amendment_proposed_by_outside_role_set_is_bad_amendment() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["amendments"] = json!([{
        "proposed_by": "human",
        "at_checkpoint": 1,
        "resume_status": "implementing",
        "reason": "test",
        "approved_by": null,
        "approved_at_checkpoint": null,
    }]);
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::BadAmendment("proposed_by=human".to_string()))
    );
}

#[test]
fn amendment_resume_status_outside_catalog_is_bad_amendment() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["amendments"] = json!([{
        "proposed_by": "vadi",
        "at_checkpoint": 1,
        "resume_status": "made_up_status",
        "reason": "test",
        "approved_by": null,
        "approved_at_checkpoint": null,
    }]);
    let err = dvandva::workflow::validate_run_workflow(&rw, &catalog).unwrap_err();
    assert!(matches!(err, ShapeError::BadAmendment(_)));
}

#[test]
fn amendment_self_approval_is_bad_amendment() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["amendments"] = json!([{
        "proposed_by": "vadi",
        "at_checkpoint": 1,
        "resume_status": "implementing",
        "reason": "test",
        "approved_by": "vadi",
        "approved_at_checkpoint": 2,
    }]);
    let err = dvandva::workflow::validate_run_workflow(&rw, &catalog).unwrap_err();
    assert!(matches!(err, ShapeError::BadAmendment(_)));
}

#[test]
fn amendment_peer_approval_is_accepted() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["amendments"] = json!([{
        "proposed_by": "vadi",
        "at_checkpoint": 1,
        "resume_status": "implementing",
        "reason": "test",
        "approved_by": "prativadi",
        "approved_at_checkpoint": 2,
    }]);
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Ok(())
    );
}

// ===================== deterministic ordering =====================

#[test]
fn a_fixture_violating_two_rules_reports_the_earlier_check() {
    // Bad `source` (checked 2nd) AND an unknown state token (checked 3rd,
    // after source) in the same fixture — the validator must report
    // `BadSource`, not `UnknownStateToken`.
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["source"] = json!("not-a-real-source");
    rw["states"][0]["name"] = json!("made_up_state");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::BadSource("not-a-real-source".to_string()))
    );
}

// ===================== residual coverage: shape edge cases =====================

#[test]
fn duplicate_state_names_are_rejected() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    let mut conflicting = rw["states"][0].clone();
    assert_ne!(conflicting["owner"], json!("human"));
    conflicting["owner"] = json!("human"); // deliberately conflicts with the original entry
    let duplicated = conflicting["name"].as_str().unwrap().to_string();
    rw["states"].as_array_mut().unwrap().push(conflicting);
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::DuplicateStateToken(duplicated))
    );
}

#[test]
fn amendment_enter_edges_are_marked_as_dynamic_loop_capped() {
    let g = preset("full").unwrap();
    let edge = g
        .edges
        .iter()
        .find(|e| e.from == "deslop" && e.to == "spec_revision")
        .expect("full preset must include deslop -> spec_revision amendment entry");

    assert!(edge.amendment_capped);
    assert_eq!(edge.loop_cap_key, None);
}

#[test]
fn empty_states_and_edges_with_custom_source_is_currently_accepted() {
    // DOCUMENTED CURRENT BEHAVIOR: `validate_states`/`validate_edges` both
    // no-op successfully on empty arrays, and there is no minimum-size /
    // "states must be non-empty" rule tied to `source: "custom"`. A custom
    // run_workflow declaring zero states and zero edges is shape-valid.
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["source"] = json!("custom");
    rw["states"] = json!([]);
    rw["edges"] = json!([]);
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Ok(())
    );
}

#[test]
fn self_loop_edge_is_currently_accepted() {
    // DOCUMENTED CURRENT BEHAVIOR: `validate_edges` only checks that
    // `from`/`to` each resolve to a declared state — it never rejects
    // `from == to`. The module doc explicitly scopes graph SEMANTICS
    // (reachability, absorbing states, review-gate cuts) out of shape
    // validation, so a self-loop being accepted here may be intentional;
    // flagged so a future semantics layer has this pinned as its "before"
    // state rather than discovering it undocumented.
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    let state_name = rw["states"][0]["name"].clone();
    rw["edges"].as_array_mut().unwrap().push(json!({
        "from": state_name,
        "to": state_name,
        "loop_cap_key": null,
    }));
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Ok(())
    );
}

#[test]
fn amendment_approved_at_checkpoint_without_approved_by_is_bad_amendment() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["amendments"] = json!([{
        "proposed_by": "vadi",
        "at_checkpoint": 1,
        "resume_status": "implementing",
        "reason": "test",
        "approved_by": null,
        "approved_at_checkpoint": 5,
    }]);
    let err = dvandva::workflow::validate_run_workflow(&rw, &catalog).unwrap_err();
    assert!(matches!(err, ShapeError::BadAmendment(_)));
}

#[test]
fn amendment_approved_by_without_approved_at_checkpoint_is_bad_amendment() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["amendments"] = json!([{
        "proposed_by": "vadi",
        "at_checkpoint": 1,
        "resume_status": "implementing",
        "reason": "test",
        "approved_by": "prativadi",
        "approved_at_checkpoint": null
    }]);
    let err = dvandva::workflow::validate_run_workflow(&rw, &catalog).unwrap_err();
    assert!(matches!(err, ShapeError::BadAmendment(_)));
}

#[test]
fn amendment_negative_at_checkpoint_is_bad_amendment() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["amendments"] = json!([{
        "proposed_by": "vadi",
        "at_checkpoint": -1,
        "resume_status": "implementing",
        "reason": "test",
        "approved_by": null,
        "approved_at_checkpoint": null,
    }]);
    let err = dvandva::workflow::validate_run_workflow(&rw, &catalog).unwrap_err();
    assert!(matches!(err, ShapeError::BadAmendment(_)));
}

#[test]
fn amendment_approved_before_proposed_checkpoint_is_bad_amendment() {
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["amendments"] = json!([{
        "proposed_by": "vadi",
        "at_checkpoint": 5,
        "resume_status": "implementing",
        "reason": "test",
        "approved_by": "prativadi",
        "approved_at_checkpoint": 4,
    }]);
    let err = dvandva::workflow::validate_run_workflow(&rw, &catalog).unwrap_err();
    assert!(matches!(err, ShapeError::BadAmendment(_)));
}

#[test]
fn missing_field_outranks_source_and_states_violations() {
    // Missing `declared_by` (checked 1st) AND a bad `source` (checked 2nd)
    // in the same fixture — the validator must report `MissingField`, not
    // `BadSource`.
    let catalog = full_catalog();
    let mut rw = valid_workflow("fast");
    rw["source"] = json!("not-a-real-source");
    rw.as_object_mut().unwrap().remove("declared_by");
    assert_eq!(
        dvandva::workflow::validate_run_workflow(&rw, &catalog),
        Err(ShapeError::MissingField("declared_by".to_string()))
    );
}
