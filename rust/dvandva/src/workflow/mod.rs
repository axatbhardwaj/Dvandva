//! The de-facto Dvandva phase graphs, transcribed as embedded preset data.
//!
//! This module hosts the workflow type surface ([`WfState`], [`WfEdge`],
//! [`WorkflowGraph`]) and [`preset`], a byte-faithful transcription of the
//! five phase graphs [`write::edge_whitelist`](crate::write) enforces at
//! write time: `fast`, `standard`, `full`, `research`, `review`. The actual
//! preset data lives in [`presets`].

pub mod invariants;
pub mod presets;
pub mod shape;

pub use invariants::{validate_workflow_invariants, InvariantViolation};
pub use shape::{validate_run_workflow, ShapeError};

/// The behavioral class of a workflow state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateClass {
    /// Ordinary drafting/fixing work performed by an agent.
    Work,
    /// A review checkpoint gating advancement (may loop back to a `Work` state).
    ReviewGate,
    /// A human-authored answer state in the clarifying-questions handshake.
    HumanGate,
    /// A human-only stop (`human_question`/`human_decision`).
    Pause,
    /// A terminal state (`done`/`abandoned`).
    Terminal,
}

impl StateClass {
    /// Parse the lowercase snake_case class token used in a v3 baton's
    /// `run_workflow.states[].class` field (the same 5-token vocabulary
    /// `shape::validate_run_workflow` accepts). `None` for anything else.
    pub fn from_token(token: &str) -> Option<StateClass> {
        match token {
            "work" => Some(StateClass::Work),
            "review_gate" => Some(StateClass::ReviewGate),
            "human_gate" => Some(StateClass::HumanGate),
            "pause" => Some(StateClass::Pause),
            "terminal" => Some(StateClass::Terminal),
            _ => None,
        }
    }
}

/// The static (v1/v2 read-path) class of a status token.
///
/// Replicates the pre-class-model `wait` semantics exactly — `done`/`abandoned`
/// are [`StateClass::Terminal`], `human_question`/`human_decision` are
/// [`StateClass::Pause`], everything else is [`StateClass::Work`] (the generic
/// heartbeat path) — with one retroactive addition: the F5 fix maps the two
/// human-assigned clarifying-answer states to [`StateClass::HumanGate`] so a
/// v2 baton parked on them still wakes the role that must surface them to the
/// human. This is a read-path-only reinterpretation; it never changes what the
/// write path accepts.
///
/// The `Work`/`ReviewGate` split is behaviorally irrelevant to `wait` (both
/// take the generic heartbeat path), so review states are left as `Work` here
/// rather than duplicating the preset `ReviewGate` map: the exact-replication
/// contract is about exit codes, and these tokens exit nowhere.
pub fn static_class(status: &str) -> StateClass {
    match status {
        "clarifying_questions_answer" | "clarifying_questions_followup_answer" => {
            StateClass::HumanGate
        }
        "human_question" | "human_decision" => StateClass::Pause,
        "done" | "abandoned" => StateClass::Terminal,
        _ => StateClass::Work,
    }
}

/// A single state in a workflow graph: its name, owning role, and class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WfState {
    pub name: &'static str,
    /// One of `"vadi"` | `"prativadi"` | `"team"` | `"human"`.
    pub owner: &'static str,
    pub class: StateClass,
}

/// A single legal transition in a workflow graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WfEdge {
    pub from: &'static str,
    pub to: &'static str,
    /// `Some("from:to")` when this edge has a static loop cap (per
    /// `write::is_loop_edge`); `None` otherwise. Dynamic plan-amendment caps
    /// are represented by `amendment_capped`.
    pub loop_cap_key: Option<&'static str>,
    /// `true` for the two `write::is_amendment_enter_edge` edges (`full`'s
    /// `deslop:spec_revision`, `standard`'s `phase_review:spec_revision`):
    /// these are capped by the plan-amendment mechanism rather than by
    /// `loop_cap_key`'s static set, so `loop_cap_key` is `None` on them even
    /// though they are, in fact, loop-capped. `false` for every other edge.
    pub amendment_capped: bool,
}

/// A named phase graph: its states and legal edges.
#[derive(Debug, Clone)]
pub struct WorkflowGraph {
    pub name: &'static str,
    pub states: Vec<WfState>,
    pub edges: Vec<WfEdge>,
}

/// Look up a preset workflow graph by name.
///
/// Recognised names: `"fast"`, `"standard"`, `"full"`, `"research"`,
/// `"review"`. Returns `None` for anything else.
pub fn preset(name: &str) -> Option<WorkflowGraph> {
    match name {
        "fast" => Some(presets::fast()),
        "standard" => Some(presets::standard()),
        "full" => Some(presets::full()),
        "research" => Some(presets::research()),
        "review" => Some(presets::review()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_recognises_all_five_names() {
        assert!(preset("fast").is_some());
        assert!(preset("standard").is_some());
        assert!(preset("full").is_some());
        assert!(preset("research").is_some());
        assert!(preset("review").is_some());
    }

    #[test]
    fn preset_rejects_unknown_name() {
        assert!(preset("bogus").is_none());
    }
}
