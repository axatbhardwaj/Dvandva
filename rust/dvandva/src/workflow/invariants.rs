// Graph-level invariants for declared Dvandva workflows.
//
// Shape validation proves that a `run_workflow` is well-typed and internally
// referenced. This module checks the stronger graph semantics used at
// declaration/approval time: review gates must cut all routes to `done`,
// escape states must stay reachable from non-terminal states, state owners
// must be known roles, `done` must carry the terminal contract, non-terminal
// states must not be absorbing, and reserved Dvandva status names must keep
// their engine-owned owner/class contracts.

use std::collections::{HashMap, HashSet, VecDeque};

use super::{StateClass, WorkflowGraph};

pub const DEFAULT_DONE_STATE: &str = "done";
pub const DEFAULT_ESCAPE_STATES: &[&str] = &["human_question", "human_decision", "abandoned"];

const OWNERS: &[&str] = &["vadi", "prativadi", "team", "human"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvariantOptions<'a> {
    pub seed: &'a str,
    pub done: &'a str,
    pub escape_states: &'a [&'a str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantViolation {
    MissingState(String),
    DuplicateState(String),
    BadOwner {
        state: String,
        owner: String,
    },
    DanglingEdge {
        from: String,
        to: String,
    },
    DoneContractViolation {
        state: String,
    },
    ReservedContractViolation {
        state: String,
        expected_owner: String,
        expected_class: StateClass,
    },
    ReviewGateBypass {
        seed: String,
        done: String,
    },
    EscapeUnreachable {
        from: String,
        escape: String,
    },
    AbsorbingNonTerminal(String),
}

pub fn validate_workflow_invariants(
    graph: &WorkflowGraph,
    seed: &str,
) -> Result<(), Vec<InvariantViolation>> {
    validate_workflow_invariants_with_options(
        graph,
        InvariantOptions {
            seed,
            done: DEFAULT_DONE_STATE,
            escape_states: DEFAULT_ESCAPE_STATES,
        },
    )
}

pub fn validate_workflow_invariants_with_options(
    graph: &WorkflowGraph,
    options: InvariantOptions<'_>,
) -> Result<(), Vec<InvariantViolation>> {
    let mut violations = Vec::new();
    let states = collect_states(graph, &mut violations);

    require_state(&states, options.seed, &mut violations);
    require_state(&states, options.done, &mut violations);
    for escape in options.escape_states {
        require_state(&states, escape, &mut violations);
    }

    check_owner_totality(&states, &mut violations);
    check_edges_resolve(graph, &states, &mut violations);
    check_done_contract(&states, options.done, &mut violations);
    check_reserved_contracts(&states, &mut violations);

    if states.contains_key(options.seed)
        && states.contains_key(options.done)
        && reaches_done_without_review_gate(graph, &states, options.seed, options.done)
    {
        violations.push(InvariantViolation::ReviewGateBypass {
            seed: options.seed.to_string(),
            done: options.done.to_string(),
        });
    }

    check_escape_reachability(graph, &states, options.escape_states, &mut violations);
    check_absorbing_non_terminals(graph, &states, &mut violations);

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn collect_states<'a>(
    graph: &'a WorkflowGraph,
    violations: &mut Vec<InvariantViolation>,
) -> HashMap<&'a str, &'a super::WfState> {
    let mut states = HashMap::with_capacity(graph.states.len());
    for state in &graph.states {
        if states.insert(state.name, state).is_some() {
            violations.push(InvariantViolation::DuplicateState(state.name.to_string()));
        }
    }
    states
}

fn require_state(
    states: &HashMap<&str, &super::WfState>,
    name: &str,
    violations: &mut Vec<InvariantViolation>,
) {
    if !states.contains_key(name) {
        violations.push(InvariantViolation::MissingState(name.to_string()));
    }
}

fn check_owner_totality(
    states: &HashMap<&str, &super::WfState>,
    violations: &mut Vec<InvariantViolation>,
) {
    for state in states.values() {
        if !OWNERS.contains(&state.owner) {
            violations.push(InvariantViolation::BadOwner {
                state: state.name.to_string(),
                owner: state.owner.to_string(),
            });
        }
    }
}

fn check_edges_resolve(
    graph: &WorkflowGraph,
    states: &HashMap<&str, &super::WfState>,
    violations: &mut Vec<InvariantViolation>,
) {
    for edge in &graph.edges {
        if !states.contains_key(edge.from) || !states.contains_key(edge.to) {
            violations.push(InvariantViolation::DanglingEdge {
                from: edge.from.to_string(),
                to: edge.to.to_string(),
            });
        }
    }
}

fn check_done_contract(
    states: &HashMap<&str, &super::WfState>,
    done: &str,
    violations: &mut Vec<InvariantViolation>,
) {
    if let Some(state) = states.get(done) {
        if state.class != StateClass::Terminal {
            violations.push(InvariantViolation::DoneContractViolation {
                state: done.to_string(),
            });
        }
    }
}

fn check_reserved_contracts(
    states: &HashMap<&str, &super::WfState>,
    violations: &mut Vec<InvariantViolation>,
) {
    for state in states.values() {
        let Some((expected_owner, expected_class)) = reserved_contract(state.name) else {
            continue;
        };
        if state.owner != expected_owner || state.class != expected_class {
            violations.push(InvariantViolation::ReservedContractViolation {
                state: state.name.to_string(),
                expected_owner: expected_owner.to_string(),
                expected_class,
            });
        }
    }
}

fn reserved_contract(name: &str) -> Option<(&'static str, StateClass)> {
    match name {
        "workflow_declaring" | "workflow_revision" => Some(("vadi", StateClass::Work)),
        "workflow_review" => Some(("prativadi", StateClass::ReviewGate)),
        "clarifying_questions_answer" | "clarifying_questions_followup_answer" => {
            Some(("human", StateClass::HumanGate))
        }
        "research_review" | "spec_review" | "deep_review" | "phase_review" | "counter_review" => {
            Some(("prativadi", StateClass::ReviewGate))
        }
        "cross_review" | "termination_review" => Some(("team", StateClass::ReviewGate)),
        "review_of_review" => Some(("vadi", StateClass::ReviewGate)),
        "human_question" | "human_decision" => Some(("human", StateClass::Pause)),
        "done" => Some(("team", StateClass::Terminal)),
        "abandoned" => Some(("human", StateClass::Terminal)),
        _ => None,
    }
}

fn reaches_done_without_review_gate(
    graph: &WorkflowGraph,
    states: &HashMap<&str, &super::WfState>,
    seed: &str,
    done: &str,
) -> bool {
    if states
        .get(seed)
        .is_some_and(|state| state.class == StateClass::ReviewGate)
    {
        return false;
    }

    let mut queue = VecDeque::from([seed]);
    let mut seen = HashSet::from([seed]);

    while let Some(cur) = queue.pop_front() {
        if cur == done {
            return true;
        }
        if states
            .get(cur)
            .is_some_and(|state| state.class == StateClass::ReviewGate)
        {
            continue;
        }

        for edge in graph.edges.iter().filter(|edge| edge.from == cur) {
            if states
                .get(edge.to)
                .is_some_and(|state| state.class == StateClass::ReviewGate)
            {
                continue;
            }
            if seen.insert(edge.to) {
                queue.push_back(edge.to);
            }
        }
    }

    false
}

fn check_escape_reachability(
    graph: &WorkflowGraph,
    states: &HashMap<&str, &super::WfState>,
    escape_states: &[&str],
    violations: &mut Vec<InvariantViolation>,
) {
    let reverse = reverse_adjacency(graph, states);
    for escape in escape_states {
        if !states.contains_key(escape) {
            continue;
        }
        let reachable = reverse_reachable(*escape, &reverse);
        for state in states.values() {
            if state.class == StateClass::Terminal {
                continue;
            }
            if !reachable.contains(state.name) {
                violations.push(InvariantViolation::EscapeUnreachable {
                    from: state.name.to_string(),
                    escape: (*escape).to_string(),
                });
            }
        }
    }
}

fn reverse_adjacency<'a>(
    graph: &'a WorkflowGraph,
    states: &HashMap<&str, &super::WfState>,
) -> HashMap<&'a str, Vec<&'a str>> {
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &graph.edges {
        if states.contains_key(edge.from) && states.contains_key(edge.to) {
            reverse.entry(edge.to).or_default().push(edge.from);
        }
    }
    reverse
}

fn reverse_reachable<'a>(
    escape: &'a str,
    reverse: &HashMap<&'a str, Vec<&'a str>>,
) -> HashSet<&'a str> {
    let mut queue = VecDeque::from([escape]);
    let mut seen = HashSet::from([escape]);

    while let Some(cur) = queue.pop_front() {
        for prev in reverse.get(cur).into_iter().flatten() {
            if seen.insert(*prev) {
                queue.push_back(prev);
            }
        }
    }

    seen
}

fn check_absorbing_non_terminals(
    graph: &WorkflowGraph,
    states: &HashMap<&str, &super::WfState>,
    violations: &mut Vec<InvariantViolation>,
) {
    for state in states.values() {
        if state.class == StateClass::Terminal {
            continue;
        }
        let has_non_self_exit = graph.edges.iter().any(|edge| {
            edge.from == state.name && edge.to != state.name && states.contains_key(edge.to)
        });
        if !has_non_self_exit {
            violations.push(InvariantViolation::AbsorbingNonTerminal(
                state.name.to_string(),
            ));
        }
    }
}
