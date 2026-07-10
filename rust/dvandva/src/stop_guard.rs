//! Pure decision logic for the `dvandva stop-guard` Claude Code Stop hook.
//!
//! A Claude-hosted walkaway Dvandva role must never silently end its turn while
//! it is bound to a live baton (the never-silent-stop invariant). Binding is
//! session/run-scoped: the `baton-guard` PreToolUse hook stamps a
//! [`crate::session_marker`] under a run the moment this session touches it —
//! recording the binding role in the marker — so the Stop hook can tell a
//! participant from a stranger and name that role without a `DVANDVA_ROLE` env
//! of its own. Given a Stop hook payload and each discoverable baton tagged with
//! whether this session is bound to it and the role its marker records,
//! [`decide`] returns whether the session may stop or must be nudged back into
//! `dvandva wait`. The I/O boundary (reading stdin, collecting batons, resolving
//! the session binding and its marker role, printing, and the process exit code)
//! lives in [`crate::cmd::stop_guard`].

use serde_json::Value;

use crate::baton_guard::payload_session_id;
use crate::commit_gate::is_gate_terminal;
use crate::util::coalesce;

/// The outcome of a [`decide`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopDecision {
    /// The session may end its turn.
    Allow,
    /// A live walkaway baton still binds this session: block the stop and nudge
    /// the session back into `dvandva wait`. Carries the model-visible reason.
    Block(String),
}

/// A discoverable baton, tagged with whether the current session is bound to
/// its run (a `.sessions/<session_id>` marker exists), and its `baton.json`
/// path for the resume command's `--file` argument.
#[derive(Debug, Clone)]
pub struct BoundBaton {
    /// Whether this session is stamped as bound to this baton's run.
    pub bound: bool,
    /// The peer role (`vadi`/`prativadi`) persisted in this session's marker for
    /// the run, or `None` when the marker records none — the Stop nudge then
    /// lists both roles' commands. Sourced from the marker, not the process
    /// environment, so it survives the unset-`DVANDVA_ROLE` Claude Stop-hook env.
    pub role: Option<String>,
    /// The baton's `baton.json` path (for the resume command's `--file` arg).
    pub path: String,
    /// The parsed baton (read leniently).
    pub baton: Value,
}

/// Decide the Stop-hook outcome for `payload` (raw JSON bytes from stdin) given
/// the repo's `batons` (each tagged with this session's binding and the peer
/// role persisted in its marker). The resume command's role comes from the
/// bound baton's marker, so it stays correct in the real Claude Stop-hook
/// environment, where `DVANDVA_ROLE` is unset.
///
/// Fails open (returns [`StopDecision::Allow`]) on a hook-continuation
/// (`stop_hook_active`, so the guard is a one-shot nudge that can never loop the
/// session) and whenever the session cannot be identified — an unparseable or
/// malformed payload, or one without a `session_id` (a stranger we cannot bind).
/// Blocks only when a baton this session is bound to is walkaway and in a
/// non-terminal / non-human-paused status, regardless of who the current
/// assignee is: for a bound session, waiting IS the job.
pub fn decide(payload: &[u8], batons: &[BoundBaton]) -> StopDecision {
    if stop_hook_active(payload) {
        return StopDecision::Allow;
    }
    // Fail open when the session cannot be identified: an unparseable/malformed
    // payload, or one without a `session_id`, is a stranger we cannot bind — a
    // guard defect must never strand an unrelated session.
    if payload_session_id(payload).is_none() {
        return StopDecision::Allow;
    }
    for bound in batons {
        if !bound.bound {
            continue;
        }
        if let Some(reason) = baton_blocks_stop(&bound.baton, &bound.path, bound.role.as_deref()) {
            return StopDecision::Block(reason);
        }
    }
    StopDecision::Allow
}

/// Whether the Stop payload marks this invocation as a hook-continuation.
/// Respecting it makes the guard a one-shot nudge (block once, then allow) so it
/// can never loop the session indefinitely. Absent/non-bool reads as `false`.
fn stop_hook_active(payload: &[u8]) -> bool {
    serde_json::from_slice::<Value>(payload)
        .ok()
        .and_then(|v| v.get("stop_hook_active").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// The block reason for a single bound `baton`, or `None` when it does not
/// demand an active wait (not a walkaway run, or terminal / human-paused).
fn baton_blocks_stop(baton: &Value, baton_path: &str, role: Option<&str>) -> Option<String> {
    // Supervised (or run_mode-less) runs never nudge — the loop is human-driven.
    if str_field(baton, "run_mode") != "walkaway" {
        return None;
    }
    let status = str_field(baton, "status");
    // Terminal (done/abandoned) or a human-assigned pause/gate
    // (human_question/human_decision/clarifying-answers): the surfacing session
    // legitimately idles in AskUserQuestion, not in `wait`, so never nudge it.
    if status.is_empty() || is_gate_terminal(&status) {
        return None;
    }
    Some(block_reason(baton, baton_path, role, &status))
}

/// The model-visible nudge: names the run and status, and spells the canonical
/// `dvandva wait` resume command from the marker's persisted `role`. When the
/// role is unknown it lists both peers' commands. A Claude Stop hook is
/// definitionally Claude-hosted, so no `--through-human` note is emitted (that
/// flag is a Codex-hosted concern, and it takes no argument — the old `note`
/// token was invalid).
fn block_reason(baton: &Value, baton_path: &str, role: Option<&str>, status: &str) -> String {
    let run_id = str_field(baton, "run_id");
    let resume = match role {
        Some(r) => wait_command(r, baton_path),
        None => format!(
            "(use the line matching your role)\n  {}\n  {}",
            wait_command("vadi", baton_path),
            wait_command("prativadi", baton_path),
        ),
    };
    format!(
        "dvandva stop-guard: walkaway run '{run_id}' is still live (status={status}) and this \
         session is bound to it. A walkaway session must never end its turn on a non-terminal \
         baton (never-silent-stop) — waiting IS the job. Re-enter `dvandva wait` to hold the \
         loop, or surface a human_question/human_decision:\n  \
         {resume}\n\
         This nudge fires once per stop."
    )
}

/// The canonical `dvandva wait` resume command for `role` against `baton_path`,
/// carrying the exact interval/max-wait/stall-max flags the loop expects.
fn wait_command(role: &str, baton_path: &str) -> String {
    format!(
        "dvandva wait --role {role} --file {baton_path} --interval 60 --max-wait 540 \
         --stall-max 1800 --until-actionable"
    )
}

/// A top-level string field, `""` when absent/null/false/non-string (jq
/// `.x // ""` with the crate's coalesce semantics).
fn str_field(baton: &Value, key: &str) -> String {
    match coalesce(baton.get(key)) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A Stop payload carrying a `session_id` (as Claude Code sends) with the
    /// given `stop_hook_active` flag.
    fn stop_payload(active: bool) -> Vec<u8> {
        json!({
            "hook_event_name": "Stop",
            "stop_hook_active": active,
            "session_id": "sess-1",
        })
        .to_string()
        .into_bytes()
    }

    /// A live team-owned walkaway baton in a work/review status.
    fn team_work_baton(status: &str) -> Value {
        json!({
            "schema": "dvandva.baton.v3",
            "run_id": "demo",
            "run_mode": "walkaway",
            "status": status,
            "assignee": "team",
            "active_roles": ["vadi", "prativadi"],
            "checkpoint": 12
        })
    }

    /// Wrap `baton` as bound to this session with a `vadi` marker role.
    fn bound(baton: Value) -> BoundBaton {
        bound_with(baton, Some("vadi"))
    }

    /// Wrap `baton` as bound to this session, carrying `role` as its marker role
    /// (`None` = the marker records no known role).
    fn bound_with(baton: Value, role: Option<&str>) -> BoundBaton {
        BoundBaton {
            bound: true,
            role: role.map(str::to_string),
            path: ".dvandva/runs/demo/baton.json".to_string(),
            baton,
        }
    }

    /// Wrap `baton` as NOT bound to this session (a stranger to the run).
    fn unbound(baton: Value) -> BoundBaton {
        BoundBaton {
            bound: false,
            ..bound(baton)
        }
    }

    #[test]
    fn blocks_bound_walkaway_work_for_any_marker_role() {
        for role in [Some("vadi"), Some("prativadi"), None] {
            let batons = [bound_with(team_work_baton("cross_review"), role)];
            assert!(
                matches!(
                    decide(&stop_payload(false), &batons),
                    StopDecision::Block(_)
                ),
                "a session bound to a live walkaway baton must not stop (marker role {role:?})"
            );
        }
    }

    #[test]
    fn blocks_bound_regardless_of_assignee() {
        // assignee/active_roles name only the peer; the bound session's own
        // role is not the assignee, but it is bound — so it must still block.
        let batons = [bound(json!({
            "schema": "dvandva.baton.v3",
            "run_id": "p1",
            "run_mode": "walkaway",
            "status": "implementing",
            "assignee": "prativadi",
            "active_roles": ["prativadi"],
            "checkpoint": 4
        }))];
        assert!(matches!(
            decide(&stop_payload(false), &batons),
            StopDecision::Block(_)
        ));
    }

    #[test]
    fn allows_unbound_stranger_even_with_active_role() {
        // The baton names vadi as active, but this session never touched the
        // run (unbound) — a stranger must be free to stop.
        let batons = [unbound(team_work_baton("implementing"))];
        assert_eq!(decide(&stop_payload(false), &batons), StopDecision::Allow);
    }

    #[test]
    fn block_reason_names_run_and_canonical_command() {
        // The nudge carries the exact resume command with its full flag set,
        // and the role from the marker — with no invalid --through-human note.
        let batons = [bound(team_work_baton("implementing"))];
        let StopDecision::Block(reason) = decide(&stop_payload(false), &batons) else {
            panic!("expected a block");
        };
        assert!(reason.contains("demo"), "reason names the run: {reason}");
        assert!(
            reason.contains("implementing"),
            "reason names the status: {reason}"
        );
        assert!(
            reason.contains("dvandva wait --role vadi --file .dvandva/runs/demo/baton.json"),
            "reason names the marker role and baton path: {reason}"
        );
        assert!(
            reason.contains("--interval 60 --max-wait 540 --stall-max 1800 --until-actionable"),
            "reason carries the canonical wait flags: {reason}"
        );
        assert!(
            !reason.contains("--through-human"),
            "a Claude Stop hook is Claude-hosted; no --through-human note: {reason}"
        );
    }

    #[test]
    fn block_reason_lists_both_roles_when_marker_role_unknown() {
        let batons = [bound_with(team_work_baton("implementing"), None)];
        let StopDecision::Block(reason) = decide(&stop_payload(false), &batons) else {
            panic!("expected a block");
        };
        assert!(
            !reason.contains("<your-role>"),
            "an unknown marker role lists both commands, not a placeholder: {reason}"
        );
        assert!(
            reason.contains("dvandva wait --role vadi --file")
                && reason.contains("dvandva wait --role prativadi --file"),
            "both peers' commands are listed: {reason}"
        );
        assert!(
            !reason.contains("--through-human"),
            "no --through-human note in the unknown-role form either: {reason}"
        );
    }

    #[test]
    fn allows_terminal_baton_even_when_bound() {
        for status in ["done", "abandoned"] {
            let batons = [bound(team_work_baton(status))];
            assert_eq!(
                decide(&stop_payload(false), &batons),
                StopDecision::Allow,
                "terminal status {status} must never block a stop"
            );
        }
    }

    #[test]
    fn allows_human_paused_baton_even_when_bound() {
        for status in ["human_question", "human_decision"] {
            let batons = [bound(json!({
                "schema": "dvandva.baton.v3",
                "run_id": "paused",
                "run_mode": "walkaway",
                "status": status,
                "assignee": "human",
                "active_roles": [],
                "checkpoint": 9
            }))];
            assert_eq!(
                decide(&stop_payload(false), &batons),
                StopDecision::Allow,
                "human-assigned status {status} must not block"
            );
        }
    }

    #[test]
    fn allows_clarifying_answer_human_gate_even_when_bound() {
        for status in [
            "clarifying_questions_answer",
            "clarifying_questions_followup_answer",
        ] {
            let batons = [bound(team_work_baton(status))];
            assert_eq!(
                decide(&stop_payload(false), &batons),
                StopDecision::Allow,
                "human-gate status {status} must not block"
            );
        }
    }

    #[test]
    fn allows_supervised_mode_even_when_bound() {
        let mut baton = team_work_baton("cross_review");
        baton["run_mode"] = json!("supervised");
        let batons = [bound(baton)];
        assert_eq!(
            decide(&stop_payload(false), &batons),
            StopDecision::Allow,
            "supervised runs are human-driven and never nudge"
        );
    }

    #[test]
    fn allows_when_run_mode_absent_even_when_bound() {
        let mut baton = team_work_baton("implementing");
        baton.as_object_mut().unwrap().remove("run_mode");
        let batons = [bound(baton)];
        assert_eq!(
            decide(&stop_payload(false), &batons),
            StopDecision::Allow,
            "a baton with no run_mode is not a declared walkaway run"
        );
    }

    #[test]
    fn allows_on_hook_continuation() {
        // stop_hook_active=true means we already nudged once; allow to prevent
        // an infinite continue loop, even with a bound live walkaway baton.
        let batons = [bound(team_work_baton("cross_review"))];
        assert_eq!(
            decide(&stop_payload(true), &batons),
            StopDecision::Allow,
            "a hook-continuation must always be allowed"
        );
    }

    #[test]
    fn allows_malformed_payload() {
        // Unparseable stdin fails open — no session id can be read, so a bound
        // live baton must not block.
        let batons = [bound(team_work_baton("implementing"))];
        assert_eq!(decide(b"{ not json", &batons), StopDecision::Allow);
    }

    #[test]
    fn allows_when_session_id_missing() {
        // A well-formed payload without a session_id cannot be bound to a run.
        let payload = json!({ "hook_event_name": "Stop", "stop_hook_active": false })
            .to_string()
            .into_bytes();
        let batons = [bound(team_work_baton("implementing"))];
        assert_eq!(decide(&payload, &batons), StopDecision::Allow);
    }

    #[test]
    fn allows_with_no_batons() {
        assert_eq!(decide(&stop_payload(false), &[]), StopDecision::Allow);
    }

    #[test]
    fn blocks_when_any_bound_baton_qualifies_among_many() {
        let mut done = team_work_baton("done");
        done["run_id"] = json!("finished");
        // A terminal bound baton, an unbound live one, and a bound live one:
        // only the last should trigger a block.
        let batons = [
            bound(done),
            unbound(team_work_baton("implementing")),
            bound(team_work_baton("cross_review")),
        ];
        assert!(
            matches!(
                decide(&stop_payload(false), &batons),
                StopDecision::Block(_)
            ),
            "one bound live walkaway baton still blocks"
        );
    }

    #[test]
    fn allows_v2_walkaway_paused_and_blocks_v2_work_when_bound() {
        // The token-based terminal/pause set is schema-agnostic (v1/v2/v3).
        let paused = bound(json!({
            "schema": "dvandva.baton.v2",
            "run_id": "v2",
            "run_mode": "walkaway",
            "status": "human_question",
            "assignee": "human"
        }));
        assert_eq!(decide(&stop_payload(false), &[paused]), StopDecision::Allow);
        let working = bound(json!({
            "schema": "dvandva.baton.v2",
            "run_id": "v2",
            "run_mode": "walkaway",
            "status": "implementing",
            "assignee": "vadi"
        }));
        assert!(matches!(
            decide(&stop_payload(false), &[working]),
            StopDecision::Block(_)
        ));
    }
}
