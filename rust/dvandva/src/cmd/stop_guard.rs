//! CLI wrapper for `dvandva stop-guard` — the Claude Code Stop hook that keeps
//! a walkaway Dvandva role from silently ending its turn while bound to a live
//! baton.
//!
//! Reads a single Stop-hook payload (JSON) from stdin, collects the repo's
//! active-run batons, tags each with whether this session (per the payload's
//! `session_id`) is bound to its run and the peer role persisted in that run's
//! marker, and blocks the stop (exit 2 with a stderr message, per the Claude
//! Code Stop-hook contract — matching the `baton-guard` hard-block form) when a
//! bound walkaway baton still holds this session. The resume command's role is
//! read from the marker, not `DVANDVA_ROLE`, which is absent from the real
//! Claude Stop-hook environment. See [`dvandva::stop_guard::decide`] for the
//! pure logic.

use std::io::Read;

use dvandva::baton_guard::payload_session_id;
use dvandva::commit_gate::collect_baton_paths;
use dvandva::gitcfg::repo_toplevel;
use dvandva::session_marker;
use dvandva::stop_guard::{decide, BoundBaton, StopDecision};
use dvandva::util::read_json_lenient;

/// Run the `stop-guard` subcommand, returning the process exit code.
///
/// Ignores any CLI `args` — the hook payload arrives on stdin per the Stop-hook
/// contract. Fails open (exit 0) on a stdin read failure or when not inside a
/// git repo: a guard defect must never strand a session.
pub fn run(args: &[String]) -> i32 {
    let _ = args;
    let mut payload = Vec::new();
    if std::io::stdin().read_to_end(&mut payload).is_err() {
        return 0;
    }
    let Some(repo_root) = std::env::current_dir()
        .ok()
        .and_then(|cwd| repo_toplevel(&cwd))
    else {
        return 0;
    };
    let session_id = payload_session_id(&payload);
    let batons: Vec<BoundBaton> = collect_baton_paths(&repo_root)
        .into_iter()
        .filter_map(|path| {
            let baton = read_json_lenient(&path).ok()?;
            let run_dir = path.parent();
            let bound = session_id
                .as_deref()
                .zip(run_dir)
                .is_some_and(|(sid, dir)| session_marker::is_bound(dir, sid));
            // The role rendered in the nudge comes from the marker (persisted at
            // bind time), so it survives the unset-DVANDVA_ROLE Stop-hook env.
            let role = session_id
                .as_deref()
                .zip(run_dir)
                .and_then(|(sid, dir)| session_marker::bound_role(dir, sid));
            Some(BoundBaton {
                bound,
                role,
                path: path.to_string_lossy().into_owned(),
                baton,
            })
        })
        .collect();
    match decide(&payload, &batons) {
        StopDecision::Allow => 0,
        StopDecision::Block(reason) => {
            eprintln!("{reason}");
            2
        }
    }
}
