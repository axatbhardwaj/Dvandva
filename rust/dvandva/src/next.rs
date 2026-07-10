//! `next` logic — B1 target (design §F1,
//! superpowers/specs/2026-07-02-flow-patches-design.html).
//!
//! `dvandva next` is the candidate scaffolder. It never hand-builds a divergent
//! copy of the transition rules: both modes drive the SAME `pub(crate)` surface
//! that `dvandva write` validates with
//! ([`crate::write::legal_transitions`] / [`crate::write::validate_candidate`]),
//! and every generated candidate is re-validated by that engine in-process before
//! it is emitted. The LIST surface over-approximates (it may offer an edge the
//! validator later refines — e.g. a spec-entry state that only one reachable
//! target phase accepts), so the in-process validate step, not the LIST, is the
//! single arbiter of what actually installs.
//!
//! * LIST mode (`dvandva next [--file <baton>] [--role <r>]`): resolve the
//!   baton, read it leniently, and print one line per legal transition from the
//!   CURRENT baton, followed by the fixed over-approximation note. Exit 0.
//! * GENERATE mode (`--to <status> --summary <t> --next-action <t> [...]`):
//!   deep-copy the baton, apply the chosen edge's status/assignee/active_roles/
//!   review_target/phase/loop_counts/amendment fields, then run the full write
//!   validation pipeline in-process. A validation failure is a bug — the
//!   candidate is NEVER emitted; the baton itself is never touched.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::util::{self, is_safe_run_id};
use crate::write::{
    canonical_mode, expected_phase_for, legal_transitions, validate_candidate, PhaseMove,
    TransitionOption,
};

const USAGE: &str = "\
Usage:
  dvandva next [--file <baton>] [--role <vadi|prativadi>]          # list legal transitions
  dvandva next [--file <baton>] --to <status> [--phase N]
               --summary <text> --next-action <text>
               [--question <t> --resume-assignee <r> --resume-status <s>]
               [--dispatch-request <id>] [--out <file>]            # generate a candidate

LIST prints one line per legal transition from the current baton:
  DVANDVA_NEXT <status> owner=<assignee> phase=<same|advance|spec> [loop=<k>/<cap>] [review_target=<t>]
followed by:
  DVANDVA_NEXT note content_gates_not_reflected
--role filters to transitions this role (or a team including it) owns.

GENERATE copies the baton, applies the chosen edge, validates the result with
the same pipeline `dvandva write` runs, and writes the candidate (default
<baton-dir>/baton.next.json). It NEVER writes the baton itself.
--phase N is required when the target is ambiguous (a phase advancement or an
amendment exit). --summary and --next-action are required. human_question
additionally requires --question, --resume-assignee, and --resume-status.

For a same-status `--to deep_review` ack, the effective role (--role, else
DVANDVA_ROLE) selects which OPEN dispatch request is acknowledged: the CANONICAL
credited-Opus request is preferred, else the sole open request for that role. If
several non-canonical requests tie, pass --dispatch-request <id> to choose one.

Default baton resolution: --file, else DVANDVA_BATON_FILE,
DVANDVA_RUN_DIR/baton.json, DVANDVA_RUN_ID mapped to
.dvandva/runs/<id>/baton.json, then legacy .dvandva/baton.json.

Exit codes: 0 ok · 2 usage/illegal-target/ambiguous · 21 baton missing ·
22 baton invalid JSON · 25 baton unparseable-strict · 23 generated-candidate
validation failure.";

const RUN_ID_UNSAFE: &str =
    "DVANDVA_RUN_ID must be one safe path segment (letters, numbers, dot, underscore, dash; no slash or '..')";

#[derive(Default)]
struct Args {
    file: Option<String>,
    role: Option<String>,
    to: Option<String>,
    phase: Option<String>,
    summary: Option<String>,
    next_action: Option<String>,
    question: Option<String>,
    resume_assignee: Option<String>,
    resume_status: Option<String>,
    out: Option<String>,
    dispatch_request: Option<String>,
}

enum ParseError {
    /// Structural error (missing value / unknown flag): print usage, exit 2.
    Usage,
    /// `-h` / `--help`: print usage, exit 0.
    Help,
}

/// Run the `next` subcommand, returning the process exit code.
pub fn run(args: &[String]) -> i32 {
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(ParseError::Help) => {
            eprintln!("{USAGE}");
            return 0;
        }
        Err(ParseError::Usage) => {
            eprintln!("{USAGE}");
            return 2;
        }
    };

    if let Some(role) = parsed.role.as_deref() {
        if role != "vadi" && role != "prativadi" {
            eprintln!("ERROR: --role must be vadi or prativadi");
            eprintln!("{USAGE}");
            return 2;
        }
    }

    let baton_file = match resolve_baton_file(parsed.file.clone()) {
        Ok(file) => PathBuf::from(file),
        Err(code) => return code,
    };

    let baton = match util::read_json_lenient(&baton_file) {
        Ok(value) => value,
        Err(util::JsonReadError::Missing) => {
            eprintln!("DVANDVA_NEXT missing baton={}", baton_file.display());
            return 21;
        }
        Err(util::JsonReadError::Invalid) => {
            eprintln!("DVANDVA_NEXT invalid_json baton={}", baton_file.display());
            return 22;
        }
    };

    match parsed.to.as_deref() {
        None => run_list(&baton, parsed.role.as_deref()),
        Some(_) => run_generate(&baton_file, &baton, &parsed),
    }
}

// ===========================================================================
// LIST mode
// ===========================================================================
fn run_list(baton: &Value, role_filter: Option<&str>) -> i32 {
    for option in legal_transitions(baton) {
        if let Some(role) = role_filter {
            if !role_owns(&option, role) {
                continue;
            }
        }
        let mut line = format!(
            "DVANDVA_NEXT {} owner={} phase={}",
            option.to_status,
            option.assignee,
            phase_move_token(option.to_phase)
        );
        if let Some((_edge, next, cap)) = &option.loop_key {
            line.push_str(&format!(" loop={next}/{cap}"));
        }
        if let Some(target) = &option.review_target {
            line.push_str(&format!(" review_target={target}"));
        }
        // Distinguish the per-request deep_review ack options (one per open dispatch
        // request) — otherwise two same-role acks print byte-identical lines.
        if let Some(id) = &option.dispatch_ack_request_id {
            line.push_str(&format!(" dispatch_ack={id}"));
        }
        println!("{line}");
    }
    // Fixed token: the surface over-approximates content gates (evidence tracks,
    // done approvals, parallel work_split, F6/F10 angles); a listed transition
    // may still be rejected once its candidate is materialised.
    println!("DVANDVA_NEXT note content_gates_not_reflected");
    0
}

/// `--role` filter: keep transitions this role owns directly, or a team
/// transition whose `active_roles` include it. The same-status deep_review
/// dispatch-ack is owned by the ADDRESSED role (the one whose open request it
/// acknowledges), not the unchanged phase assignee, so it is matched by
/// `dispatch_ack_role`.
fn role_owns(option: &TransitionOption, role: &str) -> bool {
    option.assignee == role
        || (option.assignee == "team" && option.active_roles.iter().any(|r| r == role))
        || option.dispatch_ack_role.as_deref() == Some(role)
}

fn phase_move_token(move_: PhaseMove) -> &'static str {
    match move_ {
        PhaseMove::Same => "same",
        PhaseMove::Advance => "advance",
        PhaseMove::Spec => "spec",
    }
}

// ===========================================================================
// GENERATE mode
// ===========================================================================
fn run_generate(baton_file: &Path, baton: &Value, args: &Args) -> i32 {
    let to = args.to.as_deref().unwrap_or_default();

    // Human-decision resumes are hand-authored: `dvandva next` surfaces the
    // `human_resume` marker in LIST but cannot scaffold the (arbitrary
    // non-terminal) target. Guide the user to the candidate file instead.
    if to == "human_resume" {
        eprintln!(
            "DVANDVA_NEXT human_resume: a resume from human_decision is hand-authored. Edit the CANDIDATE file (baton.next.json — never baton.json) to the intended non-terminal state, then run `dvandva write`."
        );
        return 2;
    }

    // --summary / --next-action are always required.
    let (summary, next_action) = match (args.summary.as_deref(), args.next_action.as_deref()) {
        (Some(summary), Some(next_action)) => (summary, next_action),
        _ => {
            eprintln!(
                "DVANDVA_NEXT usage: --to requires --summary <text> and --next-action <text>"
            );
            return 2;
        }
    };

    // Select the matching legal transition (one source of truth with the engine).
    // Selection splits into two regimes (r6): an ORDINARY transition has at most one
    // option per to_status; a same-status `deep_review` ack emits ONE option PER open
    // dispatch request, so it needs an explicit, role-bound, canonical-first choice.
    let options = legal_transitions(baton);
    let option = match select_option(baton, &options, to, args) {
        Ok(option) => option,
        Err((code, message)) => {
            eprintln!("{message}");
            return code;
        }
    };

    // human_question needs the resume triple in addition to summary/next_action.
    let is_human_question = to == "human_question";
    if is_human_question
        && (args.question.is_none()
            || args.resume_assignee.is_none()
            || args.resume_status.is_none())
    {
        eprintln!(
            "DVANDVA_NEXT usage: --to human_question requires --question <t>, --resume-assignee <r>, and --resume-status <s>"
        );
        return 2;
    }

    // Resolve the candidate phase for the chosen edge.
    let phase_value = match option.to_phase {
        PhaseMove::Advance => match args.phase.as_deref() {
            Some(raw) => match raw.parse::<i64>() {
                Ok(n) if n >= 0 => Value::from(n),
                _ => {
                    eprintln!(
                        "DVANDVA_NEXT bad_phase value={raw} (expected a non-negative integer)"
                    );
                    return 2;
                }
            },
            None => {
                eprintln!(
                    "DVANDVA_NEXT ambiguous to={to}: a phase advancement / amendment exit requires --phase N"
                );
                return 2;
            }
        },
        PhaseMove::Spec | PhaseMove::Same => {
            // Consume the engine-owned phase producer regardless of move class: the
            // planning-phase value is MODE-AWARE (development/research resolve
            // research_*/spec_* to "research"/"spec"; review mode pins every status
            // to "review"), so the candidate's phase can never desync from what
            // phase_status_ok demands. Its fallback arm already preserves
            // current_phase for numeric/human statuses, which covers every
            // PhaseMove::Same edge that isn't a mode-pinned planning status (e.g.
            // research_review->termination_review under research mode, which the
            // engine pins to phase "spec" even though the edge stays PhaseMove::Same).
            let mode = baton
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let current_phase = baton.get("phase").cloned().unwrap_or(Value::Null);
            expected_phase_for(mode, to, &current_phase)
        }
    };

    // ---- build the candidate (deep copy, then apply the edge) --------------
    let mut candidate = baton.clone();
    let current_checkpoint = baton.get("checkpoint").and_then(Value::as_i64).unwrap_or(0);
    let new_checkpoint = current_checkpoint + 1;

    candidate["checkpoint"] = Value::from(new_checkpoint);
    candidate["status"] = Value::from(to);
    candidate["assignee"] = Value::from(option.assignee.as_str());
    candidate["active_roles"] = Value::Array(
        option
            .active_roles
            .iter()
            .map(|role| Value::String(role.clone()))
            .collect(),
    );
    candidate["review_target"] = match &option.review_target {
        Some(target) => Value::from(target.as_str()),
        None => Value::Null,
    };
    candidate["phase"] = phase_value;
    candidate["updated_at"] = Value::from(now_iso8601_utc());
    candidate["summary"] = Value::from(summary);
    candidate["next_action"] = Value::from(next_action);

    if is_human_question {
        candidate["question"] = Value::from(args.question.as_deref().unwrap_or_default());
        candidate["resume_assignee"] =
            Value::from(args.resume_assignee.as_deref().unwrap_or_default());
        candidate["resume_status"] = Value::from(args.resume_status.as_deref().unwrap_or_default());
    } else {
        candidate["question"] = Value::Null;
        candidate["resume_assignee"] = Value::Null;
        candidate["resume_status"] = Value::Null;
    }

    apply_amendment(&mut candidate, option);
    apply_loop_counts(&mut candidate, baton, option);
    scaffold_dispatch_request(&mut candidate, baton, to, &option.assignee, new_checkpoint);
    scaffold_dispatch_ack(&mut candidate, option);

    // ---- validate the generated candidate in-process (never emit invalid) --
    let baton_dir = baton_dir_of(baton_file);
    match validate_candidate(&baton_dir, Some(baton), &candidate) {
        Ok(()) => {}
        // A strictly-broken CURRENT baton surfaces from the engine as code 25
        // (mirroring `dvandva write`'s read namespace), not a candidate defect.
        Err((25, message)) => {
            eprintln!("{message}");
            return 25;
        }
        Err((code, message)) => {
            eprintln!("DVANDVA_NEXT invalid_candidate code={code} reason={message}");
            return 23;
        }
    }

    // ---- write the candidate (NEVER the baton) -----------------------------
    let out_path = match &args.out {
        Some(path) => PathBuf::from(path),
        None => baton_dir.join("baton.next.json"),
    };
    if same_file(&out_path, baton_file) {
        eprintln!(
            "DVANDVA_NEXT refusing to write the candidate onto the baton itself: {}",
            out_path.display()
        );
        return 2;
    }
    let rendered = serde_json::to_string_pretty(&candidate).unwrap_or_default();
    if std::fs::write(&out_path, rendered).is_err() {
        eprintln!("DVANDVA_NEXT write_failed out={}", out_path.display());
        return 2;
    }

    // Note the acked request id when an ack option was selected (r5 P3): with
    // several same-role open requests the deterministic lowest-id choice is
    // otherwise invisible, so surface exactly which one this write claims.
    let ack_note = match &option.dispatch_ack_request_id {
        Some(id) => format!(" dispatch_ack={id}"),
        None => String::new(),
    };
    println!(
        "DVANDVA_NEXT ok wrote={} to={to} checkpoint={new_checkpoint}{ack_note}",
        out_path.display()
    );
    0
}

// ===========================================================================
// Transition selection (ordinary vs deep_review ack)
// ===========================================================================

/// Pick the one legal transition to materialise for `--to <to>`.
///
/// Two regimes:
/// * ORDINARY — every non-ack target. There is at most one option per to_status;
///   the explicit `--role` flag filters, and the ack-id tie-break (always `None`
///   here) is inert.
/// * DEEP_REVIEW ACK — a same-status `deep_review` rewrite emits one option PER
///   open dispatch request. The effective role (`--role`, else `DVANDVA_ROLE`, else
///   argv0 — the CLI-wide precedence) binds the choice; among that role's open
///   requests the CANONICAL credited-Opus request wins, else the sole candidate.
///   `--dispatch-request <id>` overrides with an explicit, validated selection.
fn select_option<'a>(
    baton: &Value,
    options: &'a [TransitionOption],
    to: &str,
    args: &Args,
) -> Result<&'a TransitionOption, (i32, String)> {
    let by_status: Vec<&TransitionOption> = options.iter().filter(|o| o.to_status == to).collect();
    let ack_options: Vec<&TransitionOption> = by_status
        .iter()
        .copied()
        .filter(|o| o.dispatch_ack_request_id.is_some())
        .collect();

    // The explicit selector is meaningful only for a deep_review ack.
    if let Some(id) = args.dispatch_request.as_deref() {
        return select_explicit_ack(baton, &ack_options, to, id, args.role.as_deref());
    }

    if !ack_options.is_empty() {
        return select_default_ack(baton, &ack_options, args.role.as_deref());
    }

    // Ordinary regime: filter by the explicit --role flag, exactly as before.
    let role_filter = args.role.as_deref();
    let mut matches: Vec<&TransitionOption> = by_status
        .into_iter()
        .filter(|o| role_filter.is_none_or(|role| role_owns(o, role)))
        .collect();
    matches.sort_by(|a, b| a.dispatch_ack_request_id.cmp(&b.dispatch_ack_request_id));
    match matches.first() {
        Some(option) => Ok(*option),
        None => {
            let mut legal: Vec<&str> = options.iter().map(|o| o.to_status.as_str()).collect();
            legal.dedup();
            Err((
                2,
                format!(
                    "DVANDVA_NEXT illegal_target to={to} legal={}",
                    legal.join(",")
                ),
            ))
        }
    }
}

/// Explicit `--dispatch-request <id>` selection: the invoking role names exactly
/// one OPEN dispatch request to ack. Errors clearly when the flag is misused (a
/// non-deep_review target), the id is unknown, the request is not open, or it is
/// owned by another role.
fn select_explicit_ack<'a>(
    baton: &Value,
    ack_options: &[&'a TransitionOption],
    to: &str,
    id: &str,
    role_flag: Option<&str>,
) -> Result<&'a TransitionOption, (i32, String)> {
    if to != "deep_review" {
        return Err((
            2,
            format!(
                "DVANDVA_NEXT dispatch_request={id}: --dispatch-request applies only to a deep_review ack, not to={to}"
            ),
        ));
    }
    let effective_role = effective_ack_role(role_flag);
    if let Some(option) = ack_options
        .iter()
        .copied()
        .find(|o| o.dispatch_ack_request_id.as_deref() == Some(id))
    {
        if let Some(role) = effective_role.as_deref() {
            if option.dispatch_ack_role.as_deref() != Some(role) {
                let owner = option.dispatch_ack_role.as_deref().unwrap_or("?");
                return Err((
                    2,
                    format!(
                        "DVANDVA_NEXT dispatch_request={id}: request is owned by {owner}, not the invoking role {role}; a role may only ack its own dispatch request"
                    ),
                ));
            }
        }
        return Ok(option);
    }
    // Not an open ack option — report precisely from the baton's request list.
    match dispatch_request_status(baton, id) {
        Some(status) => Err((
            2,
            format!(
                "DVANDVA_NEXT dispatch_request={id}: request status is '{status}', not open; only an open dispatch request can be acked"
            ),
        )),
        None => Err((
            2,
            format!(
                "DVANDVA_NEXT dispatch_request={id}: no dispatch request with id '{id}' exists on the baton"
            ),
        )),
    }
}

/// Default (no explicit selector) deep_review ack selection: role-bound and
/// canonical-first. Among the effective role's open requests, prefer the canonical
/// credited-Opus request; else take the sole open request. A tie of non-canonical
/// requests requires --dispatch-request.
fn select_default_ack<'a>(
    baton: &Value,
    ack_options: &[&'a TransitionOption],
    role_flag: Option<&str>,
) -> Result<&'a TransitionOption, (i32, String)> {
    let effective_role = effective_ack_role(role_flag);
    let candidates: Vec<&TransitionOption> = match effective_role.as_deref() {
        Some(role) => ack_options
            .iter()
            .copied()
            .filter(|o| o.dispatch_ack_role.as_deref() == Some(role))
            .collect(),
        None => ack_options.to_vec(),
    };
    if candidates.is_empty() {
        let role = effective_role.as_deref().unwrap_or("?");
        return Err((
            2,
            format!(
                "DVANDVA_NEXT no open dispatch request for role={role}; open dispatch-ack ids: {}",
                ack_ids(ack_options).join(",")
            ),
        ));
    }
    // Canonical-first: the credited-Opus wake the protocol produces on entry.
    let mut canonical: Vec<&TransitionOption> = candidates
        .iter()
        .copied()
        .filter(|o| {
            o.dispatch_ack_request_id
                .as_deref()
                .is_some_and(|id| request_is_canonical(baton, id))
        })
        .collect();
    if !canonical.is_empty() {
        canonical.sort_by(|a, b| a.dispatch_ack_request_id.cmp(&b.dispatch_ack_request_id));
        return Ok(canonical[0]);
    }
    if candidates.len() == 1 {
        return Ok(candidates[0]);
    }
    // No canonical present and several candidates tie — refuse to guess.
    let role = effective_role.as_deref().unwrap_or("?");
    Err((
        2,
        format!(
            "DVANDVA_NEXT ambiguous dispatch ack: {} open non-canonical requests for role={role} ({}); pass --dispatch-request <id> to select one",
            candidates.len(),
            ack_ids(&candidates).join(",")
        ),
    ))
}

/// Effective role for ack selection: `--role` flag, else `DVANDVA_ROLE`, else the
/// argv0-derived role — the same precedence every other subcommand uses
/// ([`crate::Role::resolve`]) — restricted to the two peer roles. `None` when
/// unresolvable (the write-side ack carve is the backstop that still requires
/// `DVANDVA_ROLE` to equal the acked request's role).
fn effective_ack_role(role_flag: Option<&str>) -> Option<String> {
    let env_role = std::env::var("DVANDVA_ROLE").ok();
    let argv0 = std::env::args().next().unwrap_or_default();
    crate::Role::resolve(role_flag, env_role.as_deref(), &argv0)
        .filter(|r| matches!(r, crate::Role::Vadi | crate::Role::Prativadi))
        .map(|r| r.as_str().to_string())
}

/// The `status` of the dispatch request with `id`, if one exists on the baton.
fn dispatch_request_status(baton: &Value, id: &str) -> Option<String> {
    baton
        .get("dispatch_requests")
        .and_then(Value::as_array)?
        .iter()
        .find(|req| req.get("id").and_then(Value::as_str) == Some(id))
        .and_then(|req| req.get("status").and_then(Value::as_str))
        .map(str::to_string)
}

/// Whether the dispatch request with `id` carries EXACTLY the canonical
/// credited-Opus purpose — the wake the deep_review entry gate produces. Exact
/// string equality, shared with the write-side entry gate via
/// [`crate::write::CANONICAL_OPUS_DISPATCH_PURPOSE`].
fn request_is_canonical(baton: &Value, id: &str) -> bool {
    baton
        .get("dispatch_requests")
        .and_then(Value::as_array)
        .is_some_and(|reqs| {
            reqs.iter().any(|req| {
                req.get("id").and_then(Value::as_str) == Some(id)
                    && req.get("purpose").and_then(Value::as_str)
                        == Some(crate::write::CANONICAL_OPUS_DISPATCH_PURPOSE)
            })
        })
}

/// The ack-request ids carried by a set of ack options (for error listings),
/// sorted for a stable message.
fn ack_ids(options: &[&TransitionOption]) -> Vec<String> {
    let mut ids: Vec<String> = options
        .iter()
        .filter_map(|o| o.dispatch_ack_request_id.clone())
        .collect();
    ids.sort_unstable();
    ids
}

/// Scaffold the vadi dispatch request that a development-mode prativadi-owned
/// `deep_review` entry MUST carry. `dvandva write`'s producer gate rejects such
/// an entry (exit 23 `missing_dispatch_request`) unless an OPEN
/// `dispatch_requests` entry names the vadi — the signal the vadi's waiter wakes
/// on (`DVANDVA_WAIT dispatch_requested`) to dispatch the credited cross-vendor
/// Opus reviewers. Without this, the canonical `dvandva next --to deep_review`
/// scaffold self-fails validation and never emits, so the tooling path the
/// walkaway loop is built on is broken. Scoped to exactly the gate's orientation
/// (development mode + deep_review + assignee prativadi), and to an ENTRY only:
/// when the current baton is ALREADY in deep_review the `--to deep_review` write is
/// a same-status ACK (open->acknowledged), which must leave the request set
/// untouched — scaffolding a fresh entry there would add a second request and break
/// the one-flip ack delta the write-side carve requires. Idempotent: if the copied
/// baton already carries an open CANONICAL vadi request, nothing is added.
fn scaffold_dispatch_request(
    candidate: &mut Value,
    baton: &Value,
    to: &str,
    assignee: &str,
    checkpoint: i64,
) {
    let development = canonical_mode(
        baton
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
    .as_deref()
        == Some("development");
    let entering = baton.get("status").and_then(Value::as_str) != Some("deep_review");
    if !(development && to == "deep_review" && assignee == "prativadi" && entering) {
        return;
    }
    // Idempotence keys on the CANONICAL open vadi request specifically, not "any
    // open vadi request" (FIX 2b). An unrelated open vadi request is not the
    // credited-dispatch signal — with the exact-purpose entry gate it would leave
    // the candidate failing `missing_dispatch_request` — so it must NOT suppress
    // scaffolding of the canonical entry.
    if candidate
        .get("dispatch_requests")
        .and_then(Value::as_array)
        .is_some_and(|reqs| {
            reqs.iter().any(|req| {
                req.get("role").and_then(Value::as_str) == Some("vadi")
                    && req.get("purpose").and_then(Value::as_str)
                        == Some(crate::write::CANONICAL_OPUS_DISPATCH_PURPOSE)
                    && util::is_open_finding_status(req.get("status").and_then(Value::as_str))
            })
        })
    {
        return;
    }
    let entry = serde_json::json!({
        "id": format!("credited-opus-dispatch-{checkpoint}"),
        "role": "vadi",
        "purpose": crate::write::CANONICAL_OPUS_DISPATCH_PURPOSE,
        "status": "open"
    });
    if !candidate["dispatch_requests"].is_array() {
        candidate["dispatch_requests"] = Value::Array(Vec::new());
    }
    candidate["dispatch_requests"]
        .as_array_mut()
        .expect("dispatch_requests array")
        .push(entry);
}

/// Flip EXACTLY the one selected OPEN dispatch request to `acknowledged` for the
/// same-status deep_review ack option (FIX 2a; r5 P2). The option carries the id
/// (`dispatch_ack_request_id`) and role (`dispatch_ack_role`) of the single request
/// whose wake is being claimed; only that entry flips, and only its `status`
/// changes — producing the identical-except-one-flip delta the write-side ack carve
/// requires (one wake, one ack; ids are unique per the shape gate, so this matches
/// at most one entry). A no-op for every ordinary (non-ack) option.
fn scaffold_dispatch_ack(candidate: &mut Value, option: &TransitionOption) {
    let (Some(role), Some(id)) = (
        option.dispatch_ack_role.as_deref(),
        option.dispatch_ack_request_id.as_deref(),
    ) else {
        return;
    };
    if let Some(reqs) = candidate
        .get_mut("dispatch_requests")
        .and_then(Value::as_array_mut)
    {
        for req in reqs.iter_mut() {
            if req.get("id").and_then(Value::as_str) == Some(id)
                && req.get("role").and_then(Value::as_str) == Some(role)
                && req.get("status").and_then(Value::as_str) == Some("open")
            {
                req["status"] = Value::from("acknowledged");
            }
        }
    }
}

/// Apply the amendment field per the edge: entry sets it to the current numeric
/// phase, exit nulls it, everything else preserves the copied value.
fn apply_amendment(candidate: &mut Value, option: &TransitionOption) {
    if let Some(from_phase) = option.sets_amendment_from_phase {
        candidate["amendment_from_phase"] = Value::from(from_phase);
    } else if option.clears_amendment {
        candidate["amendment_from_phase"] = Value::Null;
    }
}

/// Apply `loop_counts` per the edge: increment the loop / amendment key,
/// reset to `{}` on a phase change, otherwise preserve the copied counts.
fn apply_loop_counts(candidate: &mut Value, baton: &Value, option: &TransitionOption) {
    if option.clears_amendment {
        // Amendment exit re-enters a numeric phase and resets the per-episode
        // loop counts.
        candidate["loop_counts"] = Value::Object(serde_json::Map::new());
        return;
    }
    if let Some((edge, next, _cap)) = &option.loop_key {
        // Loop edge (including the amendment-entry `plan_amendment:<from>` edge):
        // set the incremented count, preserving any sibling keys.
        let mut counts = baton
            .get("loop_counts")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        counts.insert(edge.clone(), Value::from(*next));
        candidate["loop_counts"] = Value::Object(counts);
        return;
    }
    // No loop key: reset on any phase change (the engine forbids carrying loop
    // counts across a phase boundary), else preserve the copied counts.
    let current_phase = phase_string(baton.get("phase"));
    let new_phase = phase_string(candidate.get("phase"));
    if current_phase != new_phase {
        candidate["loop_counts"] = Value::Object(serde_json::Map::new());
    }
}

// ===========================================================================
// helpers
// ===========================================================================
/// jq `-r` render of an optional phase field (`null`/absent -> "null").
fn phase_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "null".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

fn baton_dir_of(baton_file: &Path) -> PathBuf {
    baton_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// True when two paths point at the same file (canonicalised where possible).
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// ISO-8601 UTC stamp `YYYY-MM-DDTHH:MM:SSZ`, matching the baton `updated_at`
/// convention. `chrono` is not a dependency; the `time` crate is.
fn now_iso8601_utc() -> String {
    let format = time::format_description::parse_borrowed::<2>(
        "[year]-[month]-[day]T[hour]:[minute]:[second]Z",
    )
    .expect("static format");
    time::OffsetDateTime::now_utc()
        .format(&format)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Baton-path precedence, mirroring `cmd::brief` / `cmd::wait`'s non-legacy
/// branches exactly. The legacy default is returned as-is; a missing file is
/// reported as exit 21 by the caller.
fn resolve_baton_file(file_flag: Option<String>) -> Result<String, i32> {
    if let Some(file) = file_flag {
        return Ok(file);
    }
    if let Some(file) = non_empty_env("DVANDVA_BATON_FILE") {
        return Ok(file);
    }
    if let Some(dir) = non_empty_env("DVANDVA_RUN_DIR") {
        return Ok(format!(
            "{}/baton.json",
            dir.strip_suffix('/').unwrap_or(&dir)
        ));
    }
    if let Some(run_id) = non_empty_env("DVANDVA_RUN_ID") {
        if !is_safe_run_id(&run_id) {
            eprintln!("ERROR: {RUN_ID_UNSAFE}");
            return Err(2);
        }
        return Ok(format!(".dvandva/runs/{run_id}/baton.json"));
    }
    Ok(".dvandva/baton.json".to_string())
}

fn parse_args(args: &[String]) -> Result<Args, ParseError> {
    let mut parsed = Args::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--file" => {
                parsed.file = Some(take_value(args, index)?);
                index += 2;
            }
            "--role" => {
                parsed.role = Some(take_value(args, index)?);
                index += 2;
            }
            "--to" => {
                parsed.to = Some(take_value(args, index)?);
                index += 2;
            }
            "--phase" => {
                parsed.phase = Some(take_value(args, index)?);
                index += 2;
            }
            "--summary" => {
                parsed.summary = Some(take_value(args, index)?);
                index += 2;
            }
            "--next-action" => {
                parsed.next_action = Some(take_value(args, index)?);
                index += 2;
            }
            "--question" => {
                parsed.question = Some(take_value(args, index)?);
                index += 2;
            }
            "--resume-assignee" => {
                parsed.resume_assignee = Some(take_value(args, index)?);
                index += 2;
            }
            "--resume-status" => {
                parsed.resume_status = Some(take_value(args, index)?);
                index += 2;
            }
            "--out" => {
                parsed.out = Some(take_value(args, index)?);
                index += 2;
            }
            "--dispatch-request" => {
                parsed.dispatch_request = Some(take_value(args, index)?);
                index += 2;
            }
            "-h" | "--help" => return Err(ParseError::Help),
            _ => return Err(ParseError::Usage),
        }
    }
    Ok(parsed)
}

fn take_value(args: &[String], index: usize) -> Result<String, ParseError> {
    args.get(index + 1).cloned().ok_or(ParseError::Usage)
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}
