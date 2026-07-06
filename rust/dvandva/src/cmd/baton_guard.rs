//! CLI wrapper for `dvandva baton-guard` — flow-patch target, extended with
//! a baton-creation SLA (plan:
//! superpowers/plans/2026-07-06-clarifying-questions-phase-plan.html#p3).
//!
//! Reads a single Claude Code PreToolUse hook payload (JSON) from stdin,
//! resolves the current baton-creation SLA state, and decides whether to
//! block the tool call. See [`dvandva::baton_guard::should_block`] for the
//! pure decision logic.

use std::io::Read;
use std::path::Path;

use dvandva::baton_guard::{
    payload_session_id, should_block, Decision, SlaState, BLOCK_MESSAGE, SLA_WARN_MESSAGE,
};
use dvandva::gitcfg::repo_toplevel;
use dvandva::resolve::{resolve_active_run, ResolveEnv, ResolveOutcome};
use dvandva::{sla_marker, Role};
use serde_json::Value;

/// Default baton-creation SLA threshold in seconds, overridable via
/// `DVANDVA_BATON_SLA_SECONDS`.
const DEFAULT_SLA_THRESHOLD_SECS: u64 = 120;

/// A marker this many times older than the threshold belongs to a run that
/// died long ago; it is removed instead of enforced, so a dead run can
/// never block later sessions in the same repo.
const DEAD_MARKER_CEILING_MULTIPLIER: u64 = 30;

/// Run the `baton-guard` subcommand, returning the process exit code.
///
/// Ignores any CLI `args` — the hook payload arrives on stdin per the Claude
/// Code PreToolUse contract. A stdin read failure fails open (exit 0).
pub fn run(args: &[String]) -> i32 {
    let _ = args;
    let mut payload = Vec::new();
    if std::io::stdin().read_to_end(&mut payload).is_err() {
        return 0;
    }
    let sla = sla_state(&payload);
    match should_block(&payload, &sla) {
        Decision::Allow => 0,
        Decision::BlockDirectEdit => {
            eprintln!("{BLOCK_MESSAGE}");
            2
        }
        Decision::WarnSla(reason) => {
            record_warn_if_possible();
            println!("{}", warn_hook_response(&reason));
            0
        }
    }
}

fn warn_hook_response(reason: &str) -> String {
    let context = format!("{SLA_WARN_MESSAGE} ({reason})");
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "additionalContext": context,
        },
        "systemMessage": context,
    })
    .to_string()
}

/// Resolve the current baton-creation SLA state from real process state:
/// the repo's active-run resolver plus the vadi SLA marker that
/// `dvandva resolve`/`preflight` arm on a `CREATE` outcome.
///
/// The guard is a pure reader of the marker — it never creates one, so a
/// session that never engages the protocol is never armed. Enforcement is
/// scoped to the session that owns the countdown: the first session to see
/// an unstamped marker stamps it with the payload's `session_id`; a marker
/// stamped by a different session is ignored (fail-open — the concurrent
/// two-engines-one-repo case), and a marker past the dead-run ceiling is
/// removed. Explicit `DVANDVA_ROLE=prativadi` sessions are always inert:
/// the SLA is vadi-owned (a batonless prativadi is sent to
/// `wait --discover`, not told to scaffold).
fn sla_state(payload: &[u8]) -> SlaState {
    let role = std::env::var("DVANDVA_ROLE").unwrap_or_else(|_| "vadi".to_string());
    let threshold_secs = std::env::var("DVANDVA_BATON_SLA_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SLA_THRESHOLD_SECS);
    let inert = SlaState {
        role: role.clone(),
        has_baton: true,
        marker_age_secs: None,
        threshold_secs,
        last_warned_secs_ago: None,
    };

    if role == "prativadi" {
        return inert;
    }
    let Some(repo_root) = std::env::current_dir()
        .ok()
        .and_then(|cwd| repo_toplevel(&cwd))
    else {
        return inert;
    };

    let parsed_role = Role::parse(&role).unwrap_or(Role::Vadi);
    let has_baton = matches!(
        resolve_active_run(parsed_role, Some(repo_root.as_path()), ResolveEnv::from_process_env()),
        Ok(ResolveOutcome::Resolved(path)) if is_valid_baton(&repo_root.join(&path))
    );
    if has_baton {
        sla_marker::clear(&repo_root);
        return SlaState {
            has_baton: true,
            ..inert
        };
    }

    let no_marker = SlaState {
        role: role.clone(),
        has_baton: false,
        marker_age_secs: None,
        threshold_secs,
        last_warned_secs_ago: None,
    };
    let Some(marker) = sla_marker::read(&repo_root) else {
        if sla_marker::marker_path(&repo_root).exists() {
            // Unreadable garbage — remove it rather than guess at its age.
            sla_marker::clear(&repo_root);
        }
        return no_marker;
    };

    let now = sla_marker::now_epoch();
    let age = now.saturating_sub(marker.epoch);
    if age >= threshold_secs.saturating_mul(DEAD_MARKER_CEILING_MULTIPLIER) {
        sla_marker::clear(&repo_root);
        return no_marker;
    }

    let owned = match (&marker.session, payload_session_id(payload)) {
        (Some(owner), Some(current)) => *owner == current,
        (None, Some(current)) => {
            sla_marker::stamp(&repo_root, &marker, &current);
            true
        }
        // No session id in the payload: cannot distinguish sessions, so
        // stay conservative and treat the marker as this session's own.
        (_, None) => true,
    };

    SlaState {
        role,
        has_baton: false,
        marker_age_secs: owned.then_some(age),
        threshold_secs,
        last_warned_secs_ago: marker.last_warned.map(|warned| now.saturating_sub(warned)),
    }
}

fn record_warn_if_possible() {
    let role = std::env::var("DVANDVA_ROLE").unwrap_or_else(|_| "vadi".to_string());
    if role == "prativadi" {
        return;
    }
    let Some(repo_root) = std::env::current_dir()
        .ok()
        .and_then(|cwd| repo_toplevel(&cwd))
    else {
        return;
    };
    let Some(marker) = sla_marker::read(&repo_root) else {
        return;
    };
    sla_marker::record_warn(&repo_root, &marker, sla_marker::now_epoch());
}

fn is_valid_baton(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    let schema = value
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();

    matches!(schema, "dvandva.baton.v1" | "dvandva.baton.v2") && !status.is_empty()
}
