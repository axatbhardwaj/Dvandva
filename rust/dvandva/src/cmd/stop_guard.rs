//! CLI wrapper for `dvandva stop-guard` — the Claude Code Stop hook that keeps
//! a walkaway Dvandva role from silently ending its turn on a live baton.
//!
//! Reads a single Stop-hook payload (JSON) from stdin, collects the repo's
//! active-run batons, resolves `DVANDVA_ROLE`, and blocks the stop (exit 2 with
//! a stderr message, per the Claude Code Stop-hook contract — matching the
//! `baton-guard` hard-block form) when a walkaway baton still holds this
//! session. See [`dvandva::stop_guard::decide`] for the pure decision logic.

use std::io::Read;

use dvandva::commit_gate::collect_baton_paths;
use dvandva::gitcfg::repo_toplevel;
use dvandva::stop_guard::{decide, StopDecision};
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
    let role = std::env::var("DVANDVA_ROLE").ok().filter(|s| !s.is_empty());
    let batons: Vec<_> = collect_baton_paths(&repo_root)
        .iter()
        .filter_map(|path| read_json_lenient(path).ok())
        .collect();
    match decide(&payload, role.as_deref(), &batons) {
        StopDecision::Allow => 0,
        StopDecision::Block(reason) => {
            eprintln!("{reason}");
            2
        }
    }
}
