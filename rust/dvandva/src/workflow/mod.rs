//! The de-facto Dvandva phase graphs, transcribed as embedded preset data.
//!
//! This module hosts the workflow type surface ([`WfState`], [`WfEdge`],
//! [`WorkflowGraph`]) and [`preset`], a byte-faithful transcription of the
//! five phase graphs [`write::edge_whitelist`](crate::write) enforces at
//! write time: `fast`, `standard`, `full`, `research`, `review`. The actual
//! preset data lives in [`presets`].

pub mod presets;
pub mod shape;

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
    /// `Some("from:to")` when this edge is loop-capped (per
    /// `write::is_loop_edge`); `None` otherwise.
    pub loop_cap_key: Option<&'static str>,
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
