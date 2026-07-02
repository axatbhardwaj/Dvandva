//! CLI wrapper for `dvandva baton-guard` — flow-patch target.
//!
//! Reads a single Claude Code PreToolUse hook payload (JSON) from stdin and
//! decides whether to block the tool call. See
//! [`dvandva::baton_guard::should_block`] for the decision logic.

use std::io::Read;

use dvandva::baton_guard::{should_block, BLOCK_MESSAGE};

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
    if should_block(&payload) {
        eprintln!("{BLOCK_MESSAGE}");
        return 2;
    }
    0
}
