//! The typed `Baton` serde model plus the `Status` and `Assignee` enums.
//!
//! The [`Baton`] struct pulls the fields the read path actually consumes into
//! typed slots and captures everything else in [`Baton::rest`] via
//! `#[serde(flatten)]`, so unread fields survive a deserialize/serialize
//! round-trip (value-equal). `checkpoint` is a strict `i64` — a fractional
//! value such as `1.5` is rejected. The crate is built with `serde_json`'s
//! `preserve_order` feature, so `rest` preserves key order.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Role;

/// A dvandva baton: typed core fields the read path consumes, plus [`rest`]
/// capturing every other key so nothing is lost on round-trip.
///
/// [`rest`]: Baton::rest
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baton {
    /// Schema tag, e.g. `dvandva.baton.v2`.
    pub schema: String,
    /// Lifecycle status (one of 21 catalog values).
    pub status: Status,
    /// The actor the baton is currently handed to.
    pub assignee: Assignee,
    /// Monotonic checkpoint counter. Strictly integral (`i64`).
    pub checkpoint: i64,
    /// Run identifier; may be absent or empty on a freshly seeded baton.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Last-updated ISO-8601 timestamp, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Run mode (`development`, `feature-pr`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Selected review profile (`fast`/`standard`/`full`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Current phase; typed as a raw JSON value because it may be a string
    /// (`"research"`, `"1"`) or a number depending on run mode.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub phase: Value,
    /// Every other baton key, preserved verbatim for round-trip fidelity.
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

impl Baton {
    /// Parse a baton from a JSON string.
    pub fn from_json_str(text: &str) -> Result<Baton, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Read and parse a baton from a file path.
    pub fn from_file(path: &Path) -> Result<Baton, BatonError> {
        let text = std::fs::read_to_string(path).map_err(BatonError::Io)?;
        Baton::from_json_str(&text).map_err(BatonError::Parse)
    }

    /// Serialize this baton back to a `serde_json::Value`.
    pub fn to_value(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

/// Error returned by [`Baton::from_file`].
#[derive(Debug)]
pub enum BatonError {
    /// The baton file could not be read.
    Io(std::io::Error),
    /// The baton file was read but did not parse as a valid baton.
    Parse(serde_json::Error),
}

impl fmt::Display for BatonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BatonError::Io(e) => write!(f, "baton io error: {e}"),
            BatonError::Parse(e) => write!(f, "baton parse error: {e}"),
        }
    }
}

impl std::error::Error for BatonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BatonError::Io(e) => Some(e),
            BatonError::Parse(e) => Some(e),
        }
    }
}

/// The lifecycle status of a run. The serde representation is the snake_case
/// token used in the baton `status` field and `status_catalog`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    ResearchDrafting,
    ResearchReview,
    ResearchRevision,
    SpecDrafting,
    SpecReview,
    SpecRevision,
    Implementing,
    ParallelImplementing,
    TestCreation,
    CrossReview,
    CrossFixing,
    DeepReview,
    ReviewOfReview,
    CounterReview,
    Deslop,
    TerminationReview,
    PhaseReview,
    PhaseFixing,
    HumanQuestion,
    HumanDecision,
    Done,
}

impl Status {
    /// The canonical snake_case token for this status.
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::ResearchDrafting => "research_drafting",
            Status::ResearchReview => "research_review",
            Status::ResearchRevision => "research_revision",
            Status::SpecDrafting => "spec_drafting",
            Status::SpecReview => "spec_review",
            Status::SpecRevision => "spec_revision",
            Status::Implementing => "implementing",
            Status::ParallelImplementing => "parallel_implementing",
            Status::TestCreation => "test_creation",
            Status::CrossReview => "cross_review",
            Status::CrossFixing => "cross_fixing",
            Status::DeepReview => "deep_review",
            Status::ReviewOfReview => "review_of_review",
            Status::CounterReview => "counter_review",
            Status::Deslop => "deslop",
            Status::TerminationReview => "termination_review",
            Status::PhaseReview => "phase_review",
            Status::PhaseFixing => "phase_fixing",
            Status::HumanQuestion => "human_question",
            Status::HumanDecision => "human_decision",
            Status::Done => "done",
        }
    }

    /// Whether this status is run-terminal. Only [`Status::Done`] is terminal.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Status::Done)
    }
}

/// Error returned when a status token cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseStatusError(pub String);

impl fmt::Display for ParseStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown status: {}", self.0)
    }
}

impl std::error::Error for ParseStatusError {}

impl FromStr for Status {
    type Err = ParseStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "research_drafting" => Ok(Status::ResearchDrafting),
            "research_review" => Ok(Status::ResearchReview),
            "research_revision" => Ok(Status::ResearchRevision),
            "spec_drafting" => Ok(Status::SpecDrafting),
            "spec_review" => Ok(Status::SpecReview),
            "spec_revision" => Ok(Status::SpecRevision),
            "implementing" => Ok(Status::Implementing),
            "parallel_implementing" => Ok(Status::ParallelImplementing),
            "test_creation" => Ok(Status::TestCreation),
            "cross_review" => Ok(Status::CrossReview),
            "cross_fixing" => Ok(Status::CrossFixing),
            "deep_review" => Ok(Status::DeepReview),
            "review_of_review" => Ok(Status::ReviewOfReview),
            "counter_review" => Ok(Status::CounterReview),
            "deslop" => Ok(Status::Deslop),
            "termination_review" => Ok(Status::TerminationReview),
            "phase_review" => Ok(Status::PhaseReview),
            "phase_fixing" => Ok(Status::PhaseFixing),
            "human_question" => Ok(Status::HumanQuestion),
            "human_decision" => Ok(Status::HumanDecision),
            "done" => Ok(Status::Done),
            other => Err(ParseStatusError(other.to_string())),
        }
    }
}

/// The actor a baton can be assigned to. Mirrors [`Role`] and serializes to the
/// same lowercase tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Assignee {
    Vadi,
    Prativadi,
    Team,
    Human,
}

impl Assignee {
    /// The canonical lowercase token for this assignee.
    pub fn as_str(&self) -> &'static str {
        match self {
            Assignee::Vadi => "vadi",
            Assignee::Prativadi => "prativadi",
            Assignee::Team => "team",
            Assignee::Human => "human",
        }
    }
}

impl From<Assignee> for Role {
    fn from(a: Assignee) -> Role {
        match a {
            Assignee::Vadi => Role::Vadi,
            Assignee::Prativadi => Role::Prativadi,
            Assignee::Team => Role::Team,
            Assignee::Human => Role::Human,
        }
    }
}

impl From<Role> for Assignee {
    fn from(r: Role) -> Assignee {
        match r {
            Role::Vadi => Assignee::Vadi,
            Role::Prativadi => Assignee::Prativadi,
            Role::Team => Assignee::Team,
            Role::Human => Assignee::Human,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFERENCE_BATON: &str =
        include_str!("../../../plugins/dvandva/references/baton-schema-v2.json");

    #[test]
    fn deserializes_reference_schema_baton() {
        let baton = Baton::from_json_str(REFERENCE_BATON).expect("reference baton parses");
        assert_eq!(baton.schema, "dvandva.baton.v2");
        assert_eq!(baton.status, Status::ResearchDrafting);
        assert_eq!(baton.assignee, Assignee::Vadi);
        assert_eq!(baton.checkpoint, 0);
        // Unread dynamic arrays / objects land in `rest` and are not lost.
        assert!(baton.rest.contains_key("status_catalog"));
        assert!(baton.rest.contains_key("work_split"));
        assert!(baton.rest.contains_key("profile_decision"));
    }

    #[test]
    fn unknown_keys_survive_round_trip() {
        // All typed optionals present + non-null, so re-serialization is exact.
        let json = r#"{
            "schema": "dvandva.baton.v2",
            "status": "cross_review",
            "assignee": "prativadi",
            "checkpoint": 7,
            "run_id": "demo",
            "updated_at": "2026-07-01T00:00:00Z",
            "mode": "development",
            "profile": "full",
            "phase": "1",
            "custom_top_level": {"nested": [1, 2, 3], "flag": true},
            "another_unknown": "keep-me"
        }"#;
        let original: Value = serde_json::from_str(json).unwrap();
        let baton = Baton::from_json_str(json).expect("parses");
        let reserialized = baton.to_value().expect("serializes");
        assert_eq!(original, reserialized, "round-trip must be value-equal");
        assert!(baton.rest.contains_key("custom_top_level"));
        assert_eq!(
            baton.rest.get("another_unknown"),
            Some(&Value::String("keep-me".to_string()))
        );
    }

    #[test]
    fn status_parses_research_review() {
        let parsed: Status = "research_review".parse().expect("known status parses");
        assert_eq!(parsed, Status::ResearchReview);
        assert_eq!(parsed.as_str(), "research_review");
        // serde and FromStr agree on the token.
        let via_serde: Status = serde_json::from_str("\"research_review\"").unwrap();
        assert_eq!(via_serde, Status::ResearchReview);
    }

    #[test]
    fn status_covers_all_twenty_one_tokens() {
        let catalog = [
            "research_drafting",
            "research_review",
            "research_revision",
            "spec_drafting",
            "spec_review",
            "spec_revision",
            "implementing",
            "parallel_implementing",
            "test_creation",
            "cross_review",
            "cross_fixing",
            "deep_review",
            "review_of_review",
            "counter_review",
            "deslop",
            "termination_review",
            "phase_review",
            "phase_fixing",
            "human_question",
            "human_decision",
            "done",
        ];
        assert_eq!(catalog.len(), 21);
        for token in catalog {
            let parsed: Status = token
                .parse()
                .unwrap_or_else(|_| panic!("catalog token must parse: {token}"));
            assert_eq!(parsed.as_str(), token, "as_str must round-trip {token}");
            // serde agrees with FromStr for every catalog token.
            let via_serde: Status = serde_json::from_str(&format!("\"{token}\"")).unwrap();
            assert_eq!(via_serde, parsed);
        }
    }

    #[test]
    fn is_terminal_only_for_done() {
        assert!(Status::Done.is_terminal());
        for s in [
            Status::ResearchReview,
            Status::Implementing,
            Status::HumanDecision,
            Status::HumanQuestion,
            Status::DeepReview,
            Status::Deslop,
            Status::PhaseReview,
        ] {
            assert!(!s.is_terminal(), "{s:?} must not be terminal");
        }
    }

    #[test]
    fn float_checkpoint_is_rejected() {
        let json = r#"{"schema":"s","status":"done","assignee":"vadi","checkpoint":1.5}"#;
        assert!(
            Baton::from_json_str(json).is_err(),
            "fractional checkpoint 1.5 must not parse as i64"
        );
    }

    #[test]
    fn integer_checkpoint_is_accepted() {
        let json = r#"{"schema":"s","status":"done","assignee":"vadi","checkpoint":42}"#;
        let baton = Baton::from_json_str(json).expect("integer checkpoint parses");
        assert_eq!(baton.checkpoint, 42);
        assert!(baton.status.is_terminal());
    }

    #[test]
    fn unknown_status_is_rejected() {
        let json = r#"{"schema":"s","status":"not_a_status","assignee":"vadi","checkpoint":0}"#;
        assert!(Baton::from_json_str(json).is_err());
    }

    #[test]
    fn assignee_role_bridge_round_trips() {
        for a in [
            Assignee::Vadi,
            Assignee::Prativadi,
            Assignee::Team,
            Assignee::Human,
        ] {
            let role: Role = a.into();
            let back: Assignee = role.into();
            assert_eq!(a, back);
            assert_eq!(a.as_str(), role.as_str());
        }
    }

    #[test]
    fn from_file_reads_a_real_baton() {
        let path = std::env::temp_dir().join(format!(
            "dvandva-baton-from-file-{}-{}.json",
            std::process::id(),
            line!()
        ));
        std::fs::write(&path, REFERENCE_BATON).unwrap();
        let baton = Baton::from_file(&path).expect("from_file parses");
        assert_eq!(baton.schema, "dvandva.baton.v2");
        let _ = std::fs::remove_file(&path);
    }
}
