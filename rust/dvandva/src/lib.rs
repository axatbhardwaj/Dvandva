//! `dvandva` — the library API behind the `dvandva` multicall binary.
//!
//! This crate hosts the read-path building blocks shared by the binary and the
//! differential-parity harness:
//!
//! * [`baton`] — the typed `Baton` serde model plus `Status` / `Assignee` enums.
//! * [`emit`]  — JSON serialization policy (preserve-order) and `DVANDVA_*`
//!   token-line builders with exact spacing.
//! * [`resolve`] — active-run discovery, selector precedence, and outcome.
//! * [`state`] — the `BATON_STATE_COMPACT` projection.
//!
//! The [`Role`] enum captures the two coordinating actors (`vadi`/`prativadi`)
//! plus the `team`/`human` handoff targets, and mirrors the shell role
//! derivation precedence: `--role` value > `DVANDVA_ROLE` env > the
//! parent-of-parent directory of `argv[0]`.

pub mod baton;
pub mod commit_gate;
pub mod drift_lint;
pub mod emit;
pub mod gitcfg;
pub mod hook_preflight;
pub mod hooks;
pub mod install_hooks;
pub mod installers;
pub mod lint;
pub mod lock;
pub mod preflight;
pub mod resolve;
pub mod retire;
pub mod smoke;
pub mod snapshot;
pub mod state;
pub mod util;
pub mod wait;
pub mod write;

use std::path::Path;

/// A coordinating actor in a dvandva run.
///
/// `Vadi` and `Prativadi` are the two peer agents; `Team` and `Human` are
/// handoff targets. The serde renames match the lowercase tokens used on the
/// CLI, in the baton `assignee` field, and in the skill directory layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Vadi,
    Prativadi,
    Team,
    Human,
}

impl Role {
    /// The canonical lowercase token for this role.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Vadi => "vadi",
            Role::Prativadi => "prativadi",
            Role::Team => "team",
            Role::Human => "human",
        }
    }

    /// Parse a role token (`vadi`/`prativadi`/`team`/`human`). Returns `None`
    /// for anything else, including the empty string.
    pub fn parse(value: &str) -> Option<Role> {
        match value {
            "vadi" => Some(Role::Vadi),
            "prativadi" => Some(Role::Prativadi),
            "team" => Some(Role::Team),
            "human" => Some(Role::Human),
            _ => None,
        }
    }

    /// Derive the role from an `argv[0]` path by taking the basename of its
    /// parent-of-parent directory.
    ///
    /// For `plugins/dvandva/skills/vadi/scripts/dvandva-state.sh` the parent is
    /// `scripts`, its parent is `vadi`, so the derived role is [`Role::Vadi`] —
    /// matching the shell helpers' `basename "$(dirname "$SCRIPT_DIR")"`.
    pub fn from_argv0(argv0: &str) -> Option<Role> {
        let path = Path::new(argv0);
        let grandparent = path.parent()?.parent()?;
        let name = grandparent.file_name()?.to_str()?;
        Role::parse(name)
    }

    /// Resolve the effective role using the shell precedence:
    /// `--role` value > `DVANDVA_ROLE` env > `argv[0]` parent-of-parent dir.
    ///
    /// Each source is consulted only when the higher-precedence source is
    /// absent; an empty string is treated as absent for the flag/env sources.
    pub fn resolve(role_flag: Option<&str>, env_role: Option<&str>, argv0: &str) -> Option<Role> {
        if let Some(flag) = role_flag.filter(|s| !s.is_empty()) {
            return Role::parse(flag);
        }
        if let Some(env) = env_role.filter(|s| !s.is_empty()) {
            return Role::parse(env);
        }
        Role::from_argv0(argv0)
    }
}
