//! Pure decision logic for the `dvandva stop-guard` Claude Code Stop hook.
//!
//! A Claude-hosted walkaway Dvandva role must never silently end its turn while
//! it still holds a live baton (the never-silent-stop invariant). Given a Stop
//! hook payload and the resolved role, [`decide`] returns whether the session
//! may stop or must be nudged back into `dvandva wait`. The I/O boundary
//! (reading stdin, collecting on-disk batons, resolving the role, printing, and
//! the process exit code) lives in [`crate::cmd::stop_guard`].

use serde_json::Value;

use crate::commit_gate::{is_gate_terminal, role_allowed};
use crate::util::{coalesce, is_open_finding_status};

/// The outcome of a [`decide`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopDecision {
    /// The session may end its turn.
    Allow,
    /// A live walkaway baton still holds this session: block the stop and nudge
    /// the session back into `dvandva wait`. Carries the model-visible reason.
    Block(String),
}

/// Decide the Stop-hook outcome for `payload` (raw JSON bytes from stdin) given
/// the resolved `role` (`None` = `DVANDVA_ROLE` unset/unknown → treat either
/// engine role as ours) and the repo's `batons` (every discoverable baton,
/// parsed leniently).
///
/// Fails open (returns [`StopDecision::Allow`]) on a hook-continuation
/// (`stop_hook_active`, so the guard is a one-shot nudge that can never loop the
/// session) and whenever no baton demands an active wait. Blocks only when a
/// baton is walkaway, in a non-terminal / non-human-paused status, AND names
/// this role in `assignee`/`active_roles` or carries an open dispatch request
/// for it.
pub fn decide(payload: &[u8], role: Option<&str>, batons: &[Value]) -> StopDecision {
    if stop_hook_active(payload) {
        return StopDecision::Allow;
    }
    for baton in batons {
        if let Some(reason) = baton_blocks_stop(baton, role) {
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

/// The block reason for a single `baton`, or `None` when it does not demand an
/// active wait from `role`.
fn baton_blocks_stop(baton: &Value, role: Option<&str>) -> Option<String> {
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
    if !role_active_on(baton, role) {
        return None;
    }
    let run_id = str_field(baton, "run_id");
    let assignee = str_field(baton, "assignee");
    Some(format!(
        "dvandva stop-guard: walkaway run '{run_id}' is still live (status={status}, \
         assignee={assignee}) and this session holds an active role. A walkaway session \
         must never end its turn on a non-terminal baton (never-silent-stop). Re-enter \
         `dvandva wait` (e.g. `dvandva wait --role <your-role> --file <run>/baton.json \
         --until-actionable`) to hold the loop, or surface a human_question/human_decision. \
         This nudge fires once per stop."
    ))
}

/// Whether `role` (or, when `None`, either engine role) is active on `baton`.
fn role_active_on(baton: &Value, role: Option<&str>) -> bool {
    match role {
        Some(r) if r == "vadi" || r == "prativadi" => single_role_active(baton, r),
        _ => single_role_active(baton, "vadi") || single_role_active(baton, "prativadi"),
    }
}

/// Whether `role` is the assignee, listed in `active_roles`, or the owner of an
/// open dispatch request on `baton`.
fn single_role_active(baton: &Value, role: &str) -> bool {
    role_allowed(baton, role) || has_open_dispatch_request(baton, role)
}

/// Whether `baton` carries a still-open `dispatch_requests` entry for `role`.
/// `acknowledged` counts as open (the wake is claimed but not closed); only
/// `completed`/`cancelled` (and the other closed tokens) close it.
fn has_open_dispatch_request(baton: &Value, role: &str) -> bool {
    baton
        .get("dispatch_requests")
        .and_then(Value::as_array)
        .is_some_and(|reqs| {
            reqs.iter().any(|req| {
                req.get("role").and_then(Value::as_str) == Some(role)
                    && is_open_finding_status(req.get("status").and_then(Value::as_str))
            })
        })
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

    /// A Stop payload with the given `stop_hook_active` flag.
    fn stop_payload(active: bool) -> Vec<u8> {
        json!({ "hook_event_name": "Stop", "stop_hook_active": active })
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

    #[test]
    fn blocks_walkaway_team_work_for_named_role() {
        let batons = [team_work_baton("cross_review")];
        for role in [Some("vadi"), Some("prativadi"), None] {
            assert!(
                matches!(
                    decide(&stop_payload(false), role, &batons),
                    StopDecision::Block(_)
                ),
                "role {role:?} is active on a live walkaway cross_review baton"
            );
        }
    }

    #[test]
    fn block_reason_names_the_run_and_wait() {
        let batons = [team_work_baton("implementing")];
        let StopDecision::Block(reason) = decide(&stop_payload(false), Some("vadi"), &batons)
        else {
            panic!("expected a block");
        };
        assert!(reason.contains("dvandva wait"), "reason: {reason}");
        assert!(reason.contains("demo"), "reason names the run: {reason}");
        assert!(
            reason.contains("implementing"),
            "reason names the status: {reason}"
        );
    }

    #[test]
    fn allows_assignee_role_but_not_the_peer() {
        // assignee=vadi, active_roles empty: vadi is held, prativadi is free.
        let batons = [json!({
            "schema": "dvandva.baton.v3",
            "run_id": "solo",
            "run_mode": "walkaway",
            "status": "implementing",
            "assignee": "vadi",
            "active_roles": [],
            "checkpoint": 4
        })];
        assert!(matches!(
            decide(&stop_payload(false), Some("vadi"), &batons),
            StopDecision::Block(_)
        ));
        assert_eq!(
            decide(&stop_payload(false), Some("prativadi"), &batons),
            StopDecision::Allow,
            "the peer not on the baton may stop"
        );
    }

    #[test]
    fn allows_terminal_baton() {
        for status in ["done", "abandoned"] {
            let mut baton = team_work_baton(status);
            baton["assignee"] = json!("team");
            let batons = [baton];
            assert_eq!(
                decide(&stop_payload(false), None, &batons),
                StopDecision::Allow,
                "terminal status {status} must never block a stop"
            );
        }
    }

    #[test]
    fn allows_human_paused_baton() {
        // human_question / human_decision (assignee=human): the surfacing
        // session sits in AskUserQuestion, not in wait — never nudge it.
        for status in ["human_question", "human_decision"] {
            let batons = [json!({
                "schema": "dvandva.baton.v3",
                "run_id": "paused",
                "run_mode": "walkaway",
                "status": status,
                "assignee": "human",
                "active_roles": [],
                "checkpoint": 9
            })];
            assert_eq!(
                decide(&stop_payload(false), Some("vadi"), &batons),
                StopDecision::Allow,
                "human-assigned status {status} must not block"
            );
        }
    }

    #[test]
    fn allows_clarifying_answer_human_gate() {
        for status in [
            "clarifying_questions_answer",
            "clarifying_questions_followup_answer",
        ] {
            let batons = [json!({
                "schema": "dvandva.baton.v3",
                "run_id": "clarify",
                "run_mode": "walkaway",
                "status": status,
                "assignee": "human",
                "active_roles": [],
                "checkpoint": 1
            })];
            assert_eq!(
                decide(&stop_payload(false), None, &batons),
                StopDecision::Allow,
                "human-gate status {status} must not block"
            );
        }
    }

    #[test]
    fn allows_supervised_mode() {
        let mut baton = team_work_baton("cross_review");
        baton["run_mode"] = json!("supervised");
        let batons = [baton];
        assert_eq!(
            decide(&stop_payload(false), Some("vadi"), &batons),
            StopDecision::Allow,
            "supervised runs are human-driven and never nudge"
        );
    }

    #[test]
    fn allows_when_run_mode_absent() {
        let mut baton = team_work_baton("implementing");
        baton.as_object_mut().unwrap().remove("run_mode");
        let batons = [baton];
        assert_eq!(
            decide(&stop_payload(false), Some("vadi"), &batons),
            StopDecision::Allow,
            "a baton with no run_mode is not a declared walkaway run"
        );
    }

    #[test]
    fn allows_on_hook_continuation() {
        // stop_hook_active=true means we already nudged once; allow to prevent
        // an infinite continue loop, even with a live walkaway baton present.
        let batons = [team_work_baton("cross_review")];
        assert_eq!(
            decide(&stop_payload(true), Some("vadi"), &batons),
            StopDecision::Allow,
            "a hook-continuation must always be allowed"
        );
    }

    #[test]
    fn blocks_on_open_dispatch_request_even_without_active_role() {
        // The role is neither assignee nor in active_roles, but owns an open
        // dispatch request — its waiter is the wake target, so it must not stop.
        let batons = [json!({
            "schema": "dvandva.baton.v3",
            "run_id": "dispatch",
            "run_mode": "walkaway",
            "status": "deep_review",
            "assignee": "prativadi",
            "active_roles": ["prativadi"],
            "dispatch_requests": [
                { "id": "d1", "role": "vadi", "purpose": "x", "status": "open" }
            ],
            "checkpoint": 20
        })];
        assert!(
            matches!(
                decide(&stop_payload(false), Some("vadi"), &batons),
                StopDecision::Block(_)
            ),
            "an open dispatch request holds the vadi's waiter"
        );
    }

    #[test]
    fn allows_when_dispatch_request_is_closed() {
        let batons = [json!({
            "schema": "dvandva.baton.v3",
            "run_id": "dispatch",
            "run_mode": "walkaway",
            "status": "deep_review",
            "assignee": "prativadi",
            "active_roles": ["prativadi"],
            "dispatch_requests": [
                { "id": "d1", "role": "vadi", "purpose": "x", "status": "completed" }
            ],
            "checkpoint": 20
        })];
        assert_eq!(
            decide(&stop_payload(false), Some("vadi"), &batons),
            StopDecision::Allow,
            "a completed dispatch request no longer holds the vadi"
        );
    }

    #[test]
    fn allows_with_no_batons() {
        assert_eq!(
            decide(&stop_payload(false), Some("vadi"), &[]),
            StopDecision::Allow
        );
    }

    #[test]
    fn blocks_when_any_baton_qualifies_among_many() {
        let mut done = team_work_baton("done");
        done["run_id"] = json!("finished");
        let batons = [done, team_work_baton("cross_review")];
        assert!(
            matches!(
                decide(&stop_payload(false), Some("prativadi"), &batons),
                StopDecision::Block(_)
            ),
            "one live walkaway baton among terminal ones still blocks"
        );
    }

    #[test]
    fn allows_v2_walkaway_paused_and_blocks_v2_work() {
        // The token-based terminal/pause set is schema-agnostic (v1/v2/v3).
        let paused = json!({
            "schema": "dvandva.baton.v2",
            "run_id": "v2",
            "run_mode": "walkaway",
            "status": "human_question",
            "assignee": "human"
        });
        assert_eq!(
            decide(&stop_payload(false), Some("vadi"), &[paused]),
            StopDecision::Allow
        );
        let working = json!({
            "schema": "dvandva.baton.v2",
            "run_id": "v2",
            "run_mode": "walkaway",
            "status": "implementing",
            "assignee": "vadi"
        });
        assert!(matches!(
            decide(&stop_payload(false), Some("vadi"), &[working]),
            StopDecision::Block(_)
        ));
    }
}
