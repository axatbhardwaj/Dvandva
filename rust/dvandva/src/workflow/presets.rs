//! Embedded preset data: byte-faithful transcriptions of the five phase
//! graphs `write::edge_whitelist` enforces (development/{fast,standard,full},
//! research, review).
//!
//! Each graph's edges are transcribed verbatim from the corresponding
//! `matches!` arm in `write::edge_whitelist` (~write.rs:3954-4105); each
//! state's owner is transcribed from `write::v2_expected_assignee`
//! (~write.rs:2589-2610), with `done` set to `"team"` per the `owner_for`
//! wrapper's explicit special case (`v2_expected_assignee` alone returns `""`
//! for `done`, since it is a same-status handshake, not a role-assigned
//! status). Loop-capped edges (`loop_cap_key: Some("from:to")`) are
//! transcribed from `write::is_loop_edge` (~write.rs:1154-1164).

use super::{StateClass, WfEdge, WfState, WorkflowGraph};

/// `Some("from:to")` when `edge` is one of the six loop-capped edges
/// `write::is_loop_edge` recognises; `None` otherwise.
fn loop_cap_key(edge: &'static str) -> Option<&'static str> {
    match edge {
        "deep_review:phase_fixing"
        | "cross_review:cross_fixing"
        | "termination_review:phase_fixing"
        | "phase_review:phase_fixing"
        | "review_of_review:counter_review"
        | "counter_review:review_of_review" => Some(edge),
        _ => None,
    }
}

fn edge(from: &'static str, to: &'static str) -> WfEdge {
    let key = loop_cap_key_for(from, to);
    WfEdge {
        from,
        to,
        loop_cap_key: key,
    }
}

/// Builds the `"from:to"` lookup key and resolves it against the loop-edge
/// set without allocating (the six candidates are all `'static`).
fn loop_cap_key_for(from: &'static str, to: &'static str) -> Option<&'static str> {
    match (from, to) {
        ("deep_review", "phase_fixing") => loop_cap_key("deep_review:phase_fixing"),
        ("cross_review", "cross_fixing") => loop_cap_key("cross_review:cross_fixing"),
        ("termination_review", "phase_fixing") => loop_cap_key("termination_review:phase_fixing"),
        ("phase_review", "phase_fixing") => loop_cap_key("phase_review:phase_fixing"),
        ("review_of_review", "counter_review") => loop_cap_key("review_of_review:counter_review"),
        ("counter_review", "review_of_review") => loop_cap_key("counter_review:review_of_review"),
        _ => None,
    }
}

/// The owner for a state name, per `write::v2_expected_assignee`, with
/// `done` special-cased to `"team"` (see module doc comment).
fn owner(name: &str) -> &'static str {
    match name {
        "clarifying_questions_drafting" => "vadi",
        "clarifying_questions_followup" => "prativadi",
        "clarifying_questions_answer" | "clarifying_questions_followup_answer" => "human",
        "research_drafting" | "research_revision" | "spec_drafting" | "spec_revision"
        | "implementing" | "deslop" | "phase_fixing" | "review_of_review" => "vadi",
        "parallel_implementing"
        | "test_creation"
        | "cross_review"
        | "cross_fixing"
        | "termination_review" => "team",
        "research_review" | "spec_review" | "deep_review" | "phase_review" | "counter_review" => {
            "prativadi"
        }
        "human_question" | "human_decision" | "abandoned" => "human",
        "done" => "team",
        _ => "",
    }
}

/// The [`StateClass`] for a state name, per design decision D6.
fn class(name: &str) -> StateClass {
    match name {
        "clarifying_questions_answer" | "clarifying_questions_followup_answer" => {
            StateClass::HumanGate
        }
        "human_question" | "human_decision" => StateClass::Pause,
        "done" | "abandoned" => StateClass::Terminal,
        "research_review" | "spec_review" | "cross_review" | "phase_review" | "deep_review"
        | "review_of_review" | "counter_review" | "termination_review" => StateClass::ReviewGate,
        _ => StateClass::Work,
    }
}

/// Builds a graph's `states` list from the deduplicated endpoints of its
/// already-transcribed `edges`, in first-appearance order.
fn states_from_edges(edges: &[WfEdge]) -> Vec<WfState> {
    let mut names: Vec<&'static str> = Vec::new();
    for e in edges {
        if !names.contains(&e.from) {
            names.push(e.from);
        }
        if !names.contains(&e.to) {
            names.push(e.to);
        }
    }
    names
        .into_iter()
        .map(|name| WfState {
            name,
            owner: owner(name),
            class: class(name),
        })
        .collect()
}

/// `development/fast` — write.rs:3969-3985.
pub fn fast() -> WorkflowGraph {
    let edges = vec![
        edge(
            "clarifying_questions_drafting",
            "clarifying_questions_answer",
        ),
        edge(
            "clarifying_questions_answer",
            "clarifying_questions_followup",
        ),
        edge(
            "clarifying_questions_followup",
            "clarifying_questions_followup_answer",
        ),
        edge("clarifying_questions_followup_answer", "research_drafting"),
        edge("research_drafting", "research_review"),
        edge("research_review", "research_revision"),
        edge("research_revision", "research_review"),
        edge("research_review", "implementing"),
        edge("implementing", "phase_review"),
        edge("phase_review", "phase_fixing"),
        edge("phase_fixing", "phase_review"),
        edge("phase_review", "termination_review"),
        edge("termination_review", "phase_fixing"),
        edge("termination_review", "done"),
    ];
    let states = states_from_edges(&edges);
    WorkflowGraph {
        name: "fast",
        states,
        edges,
    }
}

/// `development/standard` — write.rs:3986-4020.
pub fn standard() -> WorkflowGraph {
    let edges = vec![
        edge(
            "clarifying_questions_drafting",
            "clarifying_questions_answer",
        ),
        edge(
            "clarifying_questions_answer",
            "clarifying_questions_followup",
        ),
        edge(
            "clarifying_questions_followup",
            "clarifying_questions_followup_answer",
        ),
        edge("clarifying_questions_followup_answer", "research_drafting"),
        edge("research_drafting", "research_review"),
        edge("research_review", "research_revision"),
        edge("research_revision", "research_review"),
        edge("research_review", "spec_drafting"),
        edge("spec_drafting", "spec_review"),
        edge("spec_review", "spec_revision"),
        edge("spec_revision", "spec_review"),
        edge("spec_review", "implementing"),
        edge("implementing", "phase_review"),
        edge("phase_review", "phase_fixing"),
        edge("phase_review", "implementing"),
        edge("phase_review", "spec_revision"),
        edge("phase_fixing", "phase_review"),
        edge("phase_review", "termination_review"),
        edge("termination_review", "phase_fixing"),
        edge("termination_review", "done"),
        edge("phase_review", "parallel_implementing"),
        edge("phase_review", "review_of_review"),
        edge("review_of_review", "counter_review"),
        edge("review_of_review", "phase_review"),
        edge("counter_review", "review_of_review"),
        edge("counter_review", "phase_review"),
    ];
    let states = states_from_edges(&edges);
    WorkflowGraph {
        name: "standard",
        states,
        edges,
    }
}

/// `development/full` — write.rs:4021-4056.
pub fn full() -> WorkflowGraph {
    let edges = vec![
        edge(
            "clarifying_questions_drafting",
            "clarifying_questions_answer",
        ),
        edge(
            "clarifying_questions_answer",
            "clarifying_questions_followup",
        ),
        edge(
            "clarifying_questions_followup",
            "clarifying_questions_followup_answer",
        ),
        edge("clarifying_questions_followup_answer", "research_drafting"),
        edge("research_drafting", "research_review"),
        edge("research_review", "research_revision"),
        edge("research_revision", "research_review"),
        edge("research_review", "spec_drafting"),
        edge("spec_drafting", "spec_review"),
        edge("spec_review", "spec_revision"),
        edge("spec_review", "parallel_implementing"),
        edge("spec_revision", "spec_review"),
        edge("parallel_implementing", "test_creation"),
        edge("test_creation", "cross_review"),
        edge("cross_review", "cross_fixing"),
        edge("cross_fixing", "test_creation"),
        edge("cross_review", "deep_review"),
        edge("deep_review", "phase_fixing"),
        edge("deep_review", "review_of_review"),
        edge("deep_review", "deslop"),
        edge("review_of_review", "counter_review"),
        edge("review_of_review", "deslop"),
        edge("counter_review", "review_of_review"),
        edge("counter_review", "deslop"),
        edge("phase_fixing", "test_creation"),
        edge("deslop", "phase_fixing"),
        edge("deslop", "parallel_implementing"),
        edge("deslop", "implementing"),
        edge("deslop", "spec_revision"),
        edge("deslop", "termination_review"),
        edge("termination_review", "phase_fixing"),
        edge("termination_review", "done"),
    ];
    let states = states_from_edges(&edges);
    WorkflowGraph {
        name: "full",
        states,
        edges,
    }
}

/// `research` — write.rs:4059-4077.
pub fn research() -> WorkflowGraph {
    let edges = vec![
        edge(
            "clarifying_questions_drafting",
            "clarifying_questions_answer",
        ),
        edge(
            "clarifying_questions_answer",
            "clarifying_questions_followup",
        ),
        edge(
            "clarifying_questions_followup",
            "clarifying_questions_followup_answer",
        ),
        edge("clarifying_questions_followup_answer", "research_drafting"),
        edge("research_drafting", "research_review"),
        edge("research_review", "research_revision"),
        edge("research_revision", "research_review"),
        edge("research_review", "spec_drafting"),
        edge("spec_drafting", "spec_review"),
        edge("spec_review", "spec_revision"),
        edge("spec_revision", "spec_review"),
        edge("research_review", "termination_review"),
        edge("spec_review", "termination_review"),
        edge("termination_review", "phase_fixing"),
        edge("phase_fixing", "research_review"),
        edge("termination_review", "done"),
    ];
    let states = states_from_edges(&edges);
    WorkflowGraph {
        name: "research",
        states,
        edges,
    }
}

/// `review` — write.rs:4078-4096.
pub fn review() -> WorkflowGraph {
    let edges = vec![
        edge(
            "clarifying_questions_drafting",
            "clarifying_questions_answer",
        ),
        edge(
            "clarifying_questions_answer",
            "clarifying_questions_followup",
        ),
        edge(
            "clarifying_questions_followup",
            "clarifying_questions_followup_answer",
        ),
        edge("clarifying_questions_followup_answer", "research_drafting"),
        edge("research_drafting", "research_review"),
        edge("research_review", "research_revision"),
        edge("research_revision", "research_review"),
        edge("research_review", "deep_review"),
        edge("deep_review", "deslop"),
        edge("deep_review", "phase_fixing"),
        edge("deslop", "termination_review"),
        edge("termination_review", "phase_fixing"),
        edge("phase_fixing", "deep_review"),
        edge("termination_review", "done"),
    ];
    let states = states_from_edges(&edges);
    WorkflowGraph {
        name: "review",
        states,
        edges,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_graphs() -> Vec<WorkflowGraph> {
        vec![fast(), standard(), full(), research(), review()]
    }

    #[test]
    fn edge_counts_match_source() {
        assert_eq!(fast().edges.len(), 14, "fast");
        assert_eq!(standard().edges.len(), 26, "standard");
        assert_eq!(full().edges.len(), 32, "full");
        assert_eq!(research().edges.len(), 16, "research");
        assert_eq!(review().edges.len(), 14, "review");
    }

    #[test]
    fn every_edge_endpoint_appears_in_states() {
        for g in all_graphs() {
            for e in &g.edges {
                assert!(
                    g.states.iter().any(|s| s.name == e.from),
                    "{}: edge from {} has no matching state",
                    g.name,
                    e.from
                );
                assert!(
                    g.states.iter().any(|s| s.name == e.to),
                    "{}: edge to {} has no matching state",
                    g.name,
                    e.to
                );
            }
        }
    }

    #[test]
    fn every_state_has_a_nonempty_owner() {
        for g in all_graphs() {
            for s in &g.states {
                assert!(
                    matches!(s.owner, "vadi" | "prativadi" | "team" | "human"),
                    "{}: state {} has invalid owner {:?}",
                    g.name,
                    s.name,
                    s.owner
                );
            }
        }
    }

    fn has_edge(g: &WorkflowGraph, from: &str, to: &str) -> bool {
        g.edges.iter().any(|e| e.from == from && e.to == to)
    }

    #[test]
    fn full_contains_expected_spot_check_edges() {
        let g = full();
        assert!(has_edge(&g, "parallel_implementing", "test_creation"));
        assert!(has_edge(&g, "deslop", "termination_review"));
        assert!(has_edge(&g, "deslop", "implementing"));
        // F9: full advancing into a standard next phase is NOT present as
        // "phase_review:parallel_implementing" in the `full` arm itself —
        // that edge belongs to the `standard` arm (a standard run advancing
        // into a full next phase). `full`'s own cross-profile edges are the
        // `deslop:*` fallbacks above.
        assert!(!has_edge(&g, "phase_review", "parallel_implementing"));
    }

    #[test]
    fn standard_contains_implementing_to_phase_review() {
        assert!(has_edge(&standard(), "implementing", "phase_review"));
    }

    #[test]
    fn research_contains_research_drafting_to_research_review() {
        assert!(has_edge(
            &research(),
            "research_drafting",
            "research_review"
        ));
    }

    #[test]
    fn loop_edges_carry_loop_cap_key() {
        let g = full();
        let e = g
            .edges
            .iter()
            .find(|e| e.from == "deep_review" && e.to == "phase_fixing")
            .expect("deep_review:phase_fixing must exist in full");
        assert_eq!(e.loop_cap_key, Some("deep_review:phase_fixing"));
    }

    #[test]
    fn non_loop_edges_carry_no_loop_cap_key() {
        let g = fast();
        let e = g
            .edges
            .iter()
            .find(|e| e.from == "implementing" && e.to == "phase_review")
            .expect("implementing:phase_review must exist in fast");
        assert_eq!(e.loop_cap_key, None);
    }

    #[test]
    fn state_classes_match_design_decision_d6() {
        let g = full();
        let done = g.states.iter().find(|s| s.name == "done").unwrap();
        assert_eq!(done.class, StateClass::Terminal);
        assert_eq!(done.owner, "team");

        let cross_review = g.states.iter().find(|s| s.name == "cross_review").unwrap();
        assert_eq!(cross_review.class, StateClass::ReviewGate);

        let parallel_implementing = g
            .states
            .iter()
            .find(|s| s.name == "parallel_implementing")
            .unwrap();
        assert_eq!(parallel_implementing.class, StateClass::Work);
    }
}
