//! CLI wrapper for `dvandva baton-guard` — flow-patch target, extended with
//! a baton-creation SLA (plan:
//! superpowers/plans/2026-07-06-clarifying-questions-phase-plan.html#p3).
//!
//! Reads a single Claude Code PreToolUse hook payload (JSON) from stdin,
//! resolves the current baton-creation SLA state, and decides whether to
//! block the tool call. See [`dvandva::baton_guard::should_block`] for the
//! pure decision logic.

use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use dvandva::baton_guard::{should_block, Decision, SlaState, BLOCK_MESSAGE, SLA_BLOCK_MESSAGE};
use dvandva::gitcfg::repo_toplevel;
use dvandva::resolve::{resolve_active_run, ResolveEnv, ResolveOutcome};
use dvandva::Role;

/// Default baton-creation SLA threshold in seconds, overridable via
/// `DVANDVA_BATON_SLA_SECONDS`.
const DEFAULT_SLA_THRESHOLD_SECS: u64 = 120;

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
    let sla = sla_state();
    match should_block(&payload, &sla) {
        Decision::Allow => 0,
        Decision::BlockDirectEdit => {
            eprintln!("{BLOCK_MESSAGE}");
            2
        }
        Decision::BlockSla(reason) => {
            eprintln!("{SLA_BLOCK_MESSAGE} ({reason})");
            2
        }
    }
}

/// Resolve the current baton-creation SLA state from real process state:
/// `DVANDVA_ROLE` (default `vadi`), the repo's active-run resolver, and a
/// per-role marker file at `.dvandva/.session-baton-pending.<role>`.
///
/// Fails open (an inert [`SlaState`] that never breaches) when the repo
/// root can't be determined — "can't tell" stays permissive; only "can
/// tell, and it's overdue" blocks.
fn sla_state() -> SlaState {
    let role = std::env::var("DVANDVA_ROLE").unwrap_or_else(|_| "vadi".to_string());
    let threshold_secs = std::env::var("DVANDVA_BATON_SLA_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SLA_THRESHOLD_SECS);

    let Some(repo_root) = std::env::current_dir()
        .ok()
        .and_then(|cwd| repo_toplevel(&cwd))
    else {
        return SlaState {
            role,
            has_baton: true,
            marker_age_secs: None,
            threshold_secs,
        };
    };

    let parsed_role = Role::parse(&role).unwrap_or(Role::Vadi);
    let has_baton = matches!(
        resolve_active_run(parsed_role, Some(repo_root.as_path()), ResolveEnv::from_process_env()),
        Ok(ResolveOutcome::Resolved(path)) if repo_root.join(&path).exists()
    );

    let marker = repo_root
        .join(".dvandva")
        .join(format!(".session-baton-pending.{role}"));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let marker_age_secs = if has_baton {
        let _ = std::fs::remove_file(&marker);
        None
    } else {
        match std::fs::read_to_string(&marker)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
        {
            Some(written) => Some(now.saturating_sub(written)),
            None => {
                if let Some(parent) = marker.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&marker, now.to_string());
                None
            }
        }
    };

    SlaState {
        role,
        has_baton,
        marker_age_secs,
        threshold_secs,
    }
}
