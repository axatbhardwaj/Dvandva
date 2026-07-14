//! Integration tests for the P2 workflow invariant checker.

mod workflow {
    pub use dvandva::workflow::{StateClass, WfEdge, WfState, WorkflowGraph};

    pub mod invariants {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/workflow/invariants.rs"
        ));
    }
}

use workflow::invariants::{
    validate_workflow_invariants, validate_workflow_invariants_with_options, InvariantOptions,
    InvariantViolation,
};
use workflow::{StateClass, WfEdge, WfState, WorkflowGraph};

fn state(name: &'static str, owner: &'static str, class: StateClass) -> WfState {
    WfState { name, owner, class }
}

fn edge(from: &'static str, to: &'static str) -> WfEdge {
    WfEdge {
        from,
        to,
        loop_cap_key: None,
        amendment_capped: false,
    }
}

fn graph(states: Vec<WfState>, edges: Vec<WfEdge>) -> WorkflowGraph {
    WorkflowGraph {
        name: "test",
        states,
        edges,
    }
}

fn revision_cycle_graph() -> WorkflowGraph {
    graph(
        vec![
            state("workflow_declaring", "vadi", StateClass::Work),
            state("implementing", "vadi", StateClass::Work),
            state("phase_fixing", "vadi", StateClass::Work),
            state("deep_review", "prativadi", StateClass::ReviewGate),
            state("human_question", "human", StateClass::Pause),
            state("human_decision", "human", StateClass::Pause),
            state("abandoned", "human", StateClass::Terminal),
            state("done", "team", StateClass::Terminal),
        ],
        vec![
            edge("workflow_declaring", "implementing"),
            edge("implementing", "deep_review"),
            edge("deep_review", "phase_fixing"),
            edge("phase_fixing", "deep_review"),
            edge("deep_review", "done"),
            edge("deep_review", "human_question"),
            edge("deep_review", "human_decision"),
            edge("human_question", "human_decision"),
            edge("human_question", "abandoned"),
            edge("human_decision", "human_question"),
            edge("human_decision", "abandoned"),
        ],
    )
}

fn options_without_escapes(seed: &'static str) -> InvariantOptions<'static> {
    InvariantOptions {
        seed,
        done: "done",
        escape_states: &[],
    }
}

fn violations(graph: &WorkflowGraph) -> Vec<InvariantViolation> {
    validate_workflow_invariants(graph, "workflow_declaring").unwrap_err()
}

#[test]
fn revision_cycle_graph_passes_when_review_gate_cuts_done() {
    let graph = revision_cycle_graph();

    assert_eq!(
        validate_workflow_invariants(&graph, "workflow_declaring"),
        Ok(())
    );
}

#[test]
fn happy_path_bypass_to_done_is_rejected_even_with_revision_cycle() {
    let mut graph = revision_cycle_graph();
    graph.edges.push(edge("workflow_declaring", "done"));

    let got = violations(&graph);

    assert!(
        got.contains(&InvariantViolation::ReviewGateBypass {
            seed: "workflow_declaring".to_string(),
            done: "done".to_string(),
        }),
        "expected review-gate bypass violation, got {got:?}"
    );
}

#[test]
fn escape_reachability_requires_each_non_terminal_to_reach_each_escape() {
    let mut graph = revision_cycle_graph();
    graph
        .edges
        .retain(|e| !(e.from == "human_question" && e.to == "human_decision"));

    let got = violations(&graph);

    assert!(
        got.contains(&InvariantViolation::EscapeUnreachable {
            from: "human_question".to_string(),
            escape: "human_decision".to_string(),
        }),
        "expected human_question to lose human_decision escape reachability, got {got:?}"
    );
}

#[test]
fn absorbing_non_terminal_is_rejected() {
    let graph = graph(
        vec![
            state("workflow_declaring", "vadi", StateClass::Work),
            state("deep_review", "prativadi", StateClass::ReviewGate),
            state("stuck", "vadi", StateClass::Work),
            state("done", "team", StateClass::Terminal),
        ],
        vec![
            edge("workflow_declaring", "deep_review"),
            edge("deep_review", "done"),
            edge("deep_review", "stuck"),
        ],
    );

    let got = validate_workflow_invariants_with_options(
        &graph,
        options_without_escapes("workflow_declaring"),
    )
    .unwrap_err();

    assert!(
        got.contains(&InvariantViolation::AbsorbingNonTerminal(
            "stuck".to_string()
        )),
        "expected stuck absorbing-state violation, got {got:?}"
    );
}

#[test]
fn reserved_deep_review_contract_requires_prativadi_review_gate() {
    let mut graph = revision_cycle_graph();
    let deep_review = graph
        .states
        .iter_mut()
        .find(|state| state.name == "deep_review")
        .unwrap();
    deep_review.owner = "vadi";
    deep_review.class = StateClass::Work;

    let got = violations(&graph);

    assert!(
        got.contains(&InvariantViolation::ReservedContractViolation {
            state: "deep_review".to_string(),
            expected_owner: "prativadi".to_string(),
            expected_class: StateClass::ReviewGate,
        }),
        "expected reserved deep_review contract violation, got {got:?}"
    );
}

#[test]
fn done_must_be_terminal() {
    let mut graph = revision_cycle_graph();
    graph
        .states
        .iter_mut()
        .find(|state| state.name == "done")
        .unwrap()
        .class = StateClass::Work;

    let got = violations(&graph);

    assert!(
        got.contains(&InvariantViolation::DoneContractViolation {
            state: "done".to_string(),
        }),
        "expected done terminal-state contract violation, got {got:?}"
    );
}

#[test]
fn owner_totality_rejects_unknown_owner() {
    let mut graph = revision_cycle_graph();
    graph
        .states
        .iter_mut()
        .find(|state| state.name == "implementing")
        .unwrap()
        .owner = "nobody";

    let got = violations(&graph);

    assert!(
        got.contains(&InvariantViolation::BadOwner {
            state: "implementing".to_string(),
            owner: "nobody".to_string(),
        }),
        "expected owner-totality violation, got {got:?}"
    );
}
