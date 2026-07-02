//! `write` logic — the validated, atomic baton install with auto-snapshot.
//!
//! Direct port of `plugins/dvandva/skills/vadi/scripts/dvandva-write.sh`. The
//! validation pipeline, reason strings, exit codes, and evaluation order are
//! load-bearing and mirror the shell byte-for-byte, with ONE deliberate change
//! (design D6): the hard-path floor set that used to key three shell-script
//! patterns now keys the Rust source/test trees (`rust/dvandva/src/**`,
//! `rust/dvandva/tests/**`); every other hard-path entry is unchanged.
//!
//! Diagnostics are printed to stderr here (matching the monolithic shell), so
//! an in-process caller sees exactly what a subprocess would have produced.
//! `run_write` returns the process exit code.
//!
//! Exit codes:
//!   0  candidate validated, installed, snapshot written
//!   2  usage error / bad DVANDVA_LOCK_TIMEOUT
//!   21 candidate file missing
//!   22 candidate is not valid JSON
//!   23 candidate fails schema/required-keys/enum/gate checks
//!   24 illegal state transition
//!   25 current baton exists but is unparseable (never overwritten)
//!   26 install failed (cp/mv error; baton unchanged)
//!   27 stale checkpoint (candidate is same or older than current baton)
//!   28 lock unavailable: a non-directory squats the lock path
//!   29 lock ownership lost: this writer's fencing token was replaced
//!   30 candidate installed but snapshot failed (baton IS updated)

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::lock::{self, Acquire};
use crate::snapshot::snapshot_baton;
use crate::util;

/// Run `dvandva write <baton> <candidate>`, returning the process exit code.
///
/// All `DVANDVA_WRITE ...` diagnostics are emitted to stderr; the success line
/// is emitted to stdout.
pub fn run_write(baton_file: &Path, candidate_file: &Path) -> i32 {
    // ---- candidate exists / valid JSON -------------------------------------
    let cand = match util::read_json_lenient(candidate_file) {
        Ok(value) => value,
        Err(util::JsonReadError::Missing) => {
            eprintln!(
                "DVANDVA_WRITE missing candidate={}",
                candidate_file.display()
            );
            return 21;
        }
        Err(util::JsonReadError::Invalid) => {
            eprintln!(
                "DVANDVA_WRITE invalid_json candidate={}",
                candidate_file.display()
            );
            return 22;
        }
    };

    match validate_candidate_and_transition(baton_file, candidate_file, &cand) {
        Ok(plan) => install_and_snapshot(baton_file, candidate_file, &cand, plan),
        Err(code) => code,
    }
}

/// A validated candidate ready for the critical section: the fields needed to
/// print the success line, plus the pre-acquired lock guard.
struct InstallPlan {
    status: String,
    assignee: String,
    phase: String,
    checkpoint: String,
    lock: LockGuard,
}

// ---------------------------------------------------------------------------
// Lock guard: releases on scope exit unless disarmed (theft / unlocked path).
// `lock::release` re-checks the fencing token, so a Drop after a theft is a
// no-op — it never deletes the thief's lock.
// ---------------------------------------------------------------------------
struct LockGuard {
    dir: PathBuf,
    token: Option<String>,
}

impl LockGuard {
    fn unlocked() -> LockGuard {
        LockGuard {
            dir: PathBuf::new(),
            token: None,
        }
    }
    fn held(dir: PathBuf, token: String) -> LockGuard {
        LockGuard {
            dir,
            token: Some(token),
        }
    }
    fn holds(&self) -> bool {
        match &self.token {
            Some(token) => lock::holds(&self.dir, token),
            None => true, // unlocked path: no fencing to lose (rc-1 in shell)
        }
    }
    fn disarm(&mut self) {
        self.token = None;
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if let Some(token) = &self.token {
            lock::release(&self.dir, token);
        }
    }
}

// ===========================================================================
// Stage 1: pure candidate validation (outside the lock) + transition legality
// (inside the lock). Returns the InstallPlan on success, or the exit code.
// ===========================================================================
fn validate_candidate_and_transition(
    baton_file: &Path,
    candidate_file: &Path,
    cand: &Value,
) -> Result<InstallPlan, i32> {
    let cf = candidate_file.display();

    // ---- schema ∈ {v1, v2} -------------------------------------------------
    let schema = str_field(cand, "schema");
    if schema != "dvandva.baton.v1" && schema != "dvandva.baton.v2" {
        eprintln!(
            "DVANDVA_WRITE schema_mismatch candidate={cf} want=dvandva.baton.v1|dvandva.baton.v2"
        );
        return Err(23);
    }
    let is_v2 = schema == "dvandva.baton.v2";

    // ---- run-dir / run_id consistency (bad_run_id_dir) ---------------------
    if let Some(named) = named_run_dir_id(baton_file) {
        let cand_named = if matches!(field(cand, "run_id"), Some(Value::String(_))) {
            str_field(cand, "run_id")
        } else {
            String::new()
        };
        if !is_v2 || cand_named != named {
            eprintln!(
                "DVANDVA_WRITE bad_run_id_dir baton={} candidate_run_id={cand_named} expected_run_id={named} schema={schema}",
                baton_file.display()
            );
            return Err(23);
        }
    }

    // ---- required keys -----------------------------------------------------
    for key in required_keys(is_v2) {
        if field(cand, key).is_none() {
            eprintln!("DVANDVA_WRITE missing_key key={key} candidate={cf}");
            return Err(23);
        }
    }

    // ---- review_target enum ------------------------------------------------
    if !review_target_ok(cand) {
        eprintln!("DVANDVA_WRITE bad_review_target candidate={cf}");
        return Err(23);
    }

    let new_status = str_field(cand, "status");
    let new_assignee = str_field(cand, "assignee");
    let new_mode = str_field(cand, "mode");

    let mut new_effective_mode = String::new();
    let mut new_effective_profile = String::new();
    let mut new_profile_floor = String::new();

    // ---- v2 block ----------------------------------------------------------
    if is_v2 {
        new_effective_mode = match canonical_mode(&new_mode) {
            Some(mode) => mode,
            None => {
                eprintln!("DVANDVA_WRITE bad_mode mode={new_mode} candidate={cf}");
                return Err(23);
            }
        };

        let new_run_id = str_field(cand, "run_id");
        if !(matches!(field(cand, "run_id"), Some(Value::String(s)) if !s.is_empty())
            && util::is_safe_run_id(&new_run_id))
        {
            eprintln!("DVANDVA_WRITE bad_run_id candidate={cf}");
            return Err(23);
        }
        if !matches!(field(cand, "original_ask"), Some(Value::String(s)) if !s.is_empty()) {
            eprintln!("DVANDVA_WRITE bad_original_ask candidate={cf}");
            return Err(23);
        }
        // F7: amendment_from_phase is additive and nullable (number | null;
        // absent == null). Only its shape is checked here; transition legality is
        // enforced in decide_transition.
        if !amendment_from_phase_shape_ok(cand) {
            eprintln!("DVANDVA_WRITE bad_amendment candidate={cf}");
            return Err(23);
        }

        let baton_exists = baton_file.is_file();

        if new_effective_mode == "development" {
            // profile field/shape validation
            if !profile_block_ok(cand) {
                eprintln!("DVANDVA_WRITE bad_profile candidate={cf}");
                return Err(23);
            }
            if !baton_exists
                && new_status != "human_decision"
                && !fresh_scaffold_profile_present(cand)
            {
                eprintln!("DVANDVA_WRITE bad_profile candidate={cf}");
                return Err(23);
            }
            // effective profile + floor
            new_effective_profile = if present(field(cand, "profile")) {
                str_field(cand, "profile")
            } else if !baton_exists {
                "standard".to_string()
            } else {
                "full".to_string()
            };
            new_profile_floor = if present(field(cand, "profile_floor")) {
                str_field(cand, "profile_floor")
            } else {
                new_effective_profile.clone()
            };
            // profile_decision consistency
            if present(field(cand, "profile_decision")) {
                let pd = field(cand, "profile_decision");
                let sel = pd.and_then(|v| field(v, "selected_profile"));
                let flr = pd.and_then(|v| field(v, "floor"));
                if jq_render(sel) != new_effective_profile || jq_render(flr) != new_profile_floor {
                    eprintln!("DVANDVA_WRITE bad_profile candidate={cf}");
                    return Err(23);
                }
            }
            // downgrade guard
            if profile_rank(&new_effective_profile) < profile_rank(&new_profile_floor)
                && new_status != "human_decision"
            {
                eprintln!("DVANDVA_WRITE bad_profile_downgrade candidate={cf}");
                return Err(23);
            }
            // hard-path floor gate
            if candidate_paths(cand).iter().any(|p| hard_path(p)) {
                let decision_floor_full = match field(cand, "profile_decision") {
                    None | Some(Value::Null) => true,
                    Some(pd) => jq_render(field(pd, "floor")) == "full",
                };
                if new_effective_profile != "full"
                    || new_profile_floor != "full"
                    || !decision_floor_full
                {
                    eprintln!("DVANDVA_WRITE bad_profile_floor candidate={cf}");
                    return Err(23);
                }
            }
            // fast-allowlist gate
            if new_effective_profile == "fast" && !fast_allowlist_ok(cand) {
                eprintln!("DVANDVA_WRITE bad_profile_floor candidate={cf}");
                return Err(23);
            }
        }

        if !active_roles_shape_ok(cand) {
            eprintln!("DVANDVA_WRITE bad_active_roles candidate={cf}");
            return Err(23);
        }
        if !agent_instances_ok(cand) {
            eprintln!("DVANDVA_WRITE bad_agent_instances candidate={cf}");
            return Err(23);
        }
        if !agent_instances_write_paths_ok(cand) {
            eprintln!("DVANDVA_WRITE bad_agent_instances_write_paths candidate={cf}");
            return Err(23);
        }
        if !work_split_nonempty(cand) {
            eprintln!("DVANDVA_WRITE bad_work_split candidate={cf}");
            return Err(23);
        }
        if !work_split_paths_ok(cand) {
            eprintln!("DVANDVA_WRITE bad_work_split candidate={cf}");
            return Err(23);
        }
        if !depends_on_ok(cand) {
            eprintln!("DVANDVA_WRITE bad_depends_on candidate={cf}");
            return Err(23);
        }
        if !work_split_write_paths_ok(cand) {
            eprintln!("DVANDVA_WRITE bad_work_split_write_paths candidate={cf}");
            return Err(23);
        }
        if !verification_matrix_nonempty(cand) {
            eprintln!("DVANDVA_WRITE bad_verification_matrix candidate={cf}");
            return Err(23);
        }
        if !subagent_tracks_ok(cand) {
            eprintln!("DVANDVA_WRITE bad_subagent_tracks candidate={cf}");
            return Err(23);
        }
        if !subagent_tracks_owner_ok(cand) {
            if subagent_tracks_have_dynamic_owner(cand) {
                eprintln!("DVANDVA_WRITE bad_agent_instances candidate={cf}");
            } else {
                eprintln!("DVANDVA_WRITE bad_subagent_tracks candidate={cf}");
            }
            return Err(23);
        }
        if new_status != "research_drafting"
            && new_status != "human_question"
            && new_status != "human_decision"
            && !matches!(field(cand, "research_ref"), Some(Value::String(s)) if !s.is_empty())
        {
            eprintln!("DVANDVA_WRITE bad_research_ref candidate={cf}");
            return Err(23);
        }
        if new_status == "done" {
            let new_run_id = str_field(cand, "run_id");
            match new_effective_mode.as_str() {
                "development" => {
                    if new_effective_profile == "full" {
                        let explainer =
                            if matches!(field(cand, "run_explainer_ref"), Some(Value::String(_))) {
                                str_field(cand, "run_explainer_ref")
                            } else {
                                String::new()
                            };
                        if !run_explainer_ref_matches_run_id(&explainer, &new_run_id) {
                            eprintln!("DVANDVA_WRITE bad_run_explainer_ref candidate={cf}");
                            return Err(23);
                        }
                        if !run_explainer_reviews_ok(cand) {
                            eprintln!("DVANDVA_WRITE bad_run_explainer_reviews candidate={cf}");
                            return Err(23);
                        }
                    } else if !compact_terminal_evidence_ok(cand) {
                        eprintln!("DVANDVA_WRITE bad_compact_terminal_evidence candidate={cf}");
                        return Err(23);
                    }
                }
                "research" if !research_done_ref_ok(cand) => {
                    eprintln!("DVANDVA_WRITE bad_research_done_ref candidate={cf}");
                    return Err(23);
                }
                "review" if !review_ref_ok(cand) => {
                    eprintln!("DVANDVA_WRITE bad_review_ref candidate={cf}");
                    return Err(23);
                }
                _ => {}
            }
        }
    }

    // ---- done universal approvals -----------------------------------------
    if new_status == "done" && !done_state_ok(cand) {
        eprintln!("DVANDVA_WRITE bad_done_state candidate={cf}");
        return Err(23);
    }

    // ---- checkpoint type ---------------------------------------------------
    if !matches!(field(cand, "checkpoint"), Some(Value::Number(_))) {
        eprintln!("DVANDVA_WRITE bad_checkpoint_type candidate={cf}");
        return Err(23);
    }
    let new_checkpoint_num = match field(cand, "checkpoint").and_then(|v| v.as_u64()) {
        Some(n) => n,
        None => {
            // number but not a non-negative integer -> ^[0-9]+$ fails below
            eprintln!(
                "DVANDVA_WRITE bad_checkpoint checkpoint={} candidate={cf}",
                jq_render(field(cand, "checkpoint"))
            );
            return Err(23);
        }
    };
    let new_checkpoint = new_checkpoint_num.to_string();
    let new_phase = jq_render(field(cand, "phase"));
    let new_vadi_approval = bool_field(cand, "vadi_final_approval");
    let new_prativadi_approval = bool_field(cand, "prativadi_final_approval");

    // ---- status enum -------------------------------------------------------
    if !status_enum_ok(is_v2, &new_status) {
        eprintln!("DVANDVA_WRITE bad_status status={new_status} candidate={cf}");
        return Err(23);
    }

    // ---- v2 phase↔status pairing ------------------------------------------
    if is_v2 && !phase_status_ok(&new_effective_mode, &new_status, cand) {
        eprintln!("DVANDVA_WRITE bad_phase_status status={new_status} candidate={cf}");
        return Err(23);
    }

    // ---- assignee nonempty -------------------------------------------------
    if new_assignee.is_empty() || new_assignee == "null" {
        eprintln!("DVANDVA_WRITE bad_assignee candidate={cf}");
        return Err(23);
    }

    // ---- v2 status-owner + team active_roles ------------------------------
    if is_v2 {
        let expected = v2_expected_assignee(&new_status);
        if !expected.is_empty() && new_assignee != expected {
            eprintln!(
                "DVANDVA_WRITE bad_assignee_owner status={new_status} want={expected} got={new_assignee} candidate={cf}"
            );
            return Err(23);
        }
        if is_team_sync_status(&new_status) {
            if !(new_assignee == "team" && active_roles_sorted_both(cand)) {
                eprintln!("DVANDVA_WRITE bad_active_roles status={new_status} candidate={cf}");
                return Err(23);
            }
        } else if count_len(field(cand, "active_roles")) != 0 {
            eprintln!("DVANDVA_WRITE bad_active_roles status={new_status} candidate={cf}");
            return Err(23);
        }
    }

    // ---- checkpoint format -------------------------------------------------
    // new_checkpoint is already ^[0-9]+$ by construction (u64). Keep the shape.

    // ---- candidate question/resume null flags ------------------------------
    let cand_q_null = is_null_field(cand, "question");
    let cand_ra_null = is_null_field(cand, "resume_assignee");
    let cand_rs_null = is_null_field(cand, "resume_status");

    // ---- lock timeout ------------------------------------------------------
    let baton_dir = baton_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&baton_dir);

    let lock_timeout_raw =
        std::env::var("DVANDVA_LOCK_TIMEOUT").unwrap_or_else(|_| "30".to_string());
    if !canonical_positive_decimal(&lock_timeout_raw) {
        eprintln!("DVANDVA_WRITE bad_lock_timeout value={lock_timeout_raw}");
        return Err(2);
    }
    let lock_timeout: u64 = lock_timeout_raw.parse().unwrap_or(30);

    // ---- acquire lock ------------------------------------------------------
    let mut guard = match lock::acquire(&baton_dir, lock_timeout) {
        Acquire::Held(token) => LockGuard::held(baton_dir.clone(), token),
        Acquire::NoDir => LockGuard::unlocked(),
        Acquire::SquattedNonDir => {
            eprintln!(
                "DVANDVA_WRITE lock_unavailable path={} reason=non_directory_at_lock_path",
                baton_dir.join(lock::LOCK_DIR_NAME).display()
            );
            return Err(28);
        }
    };

    // ---- transition legality -----------------------------------------------
    let cx = Ctx {
        cf: candidate_file.display().to_string(),
        schema: &schema,
        is_v2,
        new_status: &new_status,
        new_assignee: &new_assignee,
        new_mode: &new_mode,
        new_effective_mode: &new_effective_mode,
        new_effective_profile: &new_effective_profile,
        new_profile_floor: &new_profile_floor,
        new_checkpoint: new_checkpoint_num,
        new_phase: &new_phase,
        new_vadi_approval,
        new_prativadi_approval,
        cand_q_null,
        cand_ra_null,
        cand_rs_null,
    };

    match decide_transition(baton_file, cand, &cx, &mut guard) {
        TransitionOutcome::Legal => {}
        TransitionOutcome::Exit(code) => return Err(code),
    }

    Ok(InstallPlan {
        status: new_status,
        assignee: new_assignee,
        phase: new_phase,
        checkpoint: new_checkpoint,
        lock: guard,
    })
}

/// Context passed into the transition decision, mirroring the shell locals.
struct Ctx<'a> {
    cf: String,
    schema: &'a str,
    is_v2: bool,
    new_status: &'a str,
    new_assignee: &'a str,
    new_mode: &'a str,
    new_effective_mode: &'a str,
    new_effective_profile: &'a str,
    new_profile_floor: &'a str,
    new_checkpoint: u64,
    new_phase: &'a str,
    new_vadi_approval: bool,
    new_prativadi_approval: bool,
    cand_q_null: bool,
    cand_ra_null: bool,
    cand_rs_null: bool,
}

enum TransitionOutcome {
    Legal,
    Exit(i32),
}

// ===========================================================================
// Stage 2: transition decision (inside the lock).
// ===========================================================================
fn decide_transition(
    baton_file: &Path,
    cand: &Value,
    cx: &Ctx,
    _guard: &mut LockGuard,
) -> TransitionOutcome {
    let cf = &cx.cf;

    if !baton_file.is_file() {
        // Scaffold: only the vadi may create the very first baton.
        let legal = (cx.schema == "dvandva.baton.v1"
            && cx.new_status == "spec_drafting"
            && cx.new_assignee == "vadi"
            && cx.new_checkpoint == 0)
            || (cx.schema == "dvandva.baton.v2"
                && cx.new_status == "research_drafting"
                && cx.new_assignee == "vadi"
                && cx.new_checkpoint == 0);
        if !legal {
            eprintln!(
                "DVANDVA_WRITE illegal_transition scaffold requires v1 status=spec_drafting or v2 status=research_drafting with assignee=vadi checkpoint=0, got schema={} status={} assignee={} checkpoint={}",
                cx.schema, cx.new_status, cx.new_assignee, cx.new_checkpoint
            );
            return TransitionOutcome::Exit(24);
        }
        return TransitionOutcome::Legal;
    }

    // ---- STRICT re-parse of the current baton (any anomaly -> 25) ----------
    let cur_doc = match util::read_json_lenient(baton_file) {
        Ok(value) => value,
        _ => {
            eprintln!(
                "DVANDVA_WRITE current_baton_unparseable file={} refusing_to_overwrite=true",
                baton_file.display()
            );
            return TransitionOutcome::Exit(25);
        }
    };
    let bf = baton_file.display();

    if !matches!(field(&cur_doc, "checkpoint"), Some(Value::Number(_))) {
        eprintln!("DVANDVA_WRITE current_baton_unparseable file={bf} bad_checkpoint_type=true");
        return TransitionOutcome::Exit(25);
    }

    let cur_schema = str_field(&cur_doc, "schema");
    let cur_status = str_field(&cur_doc, "status");
    let cur_checkpoint_i64 = match field(&cur_doc, "checkpoint").and_then(json_int) {
        Some(n) => n,
        None => {
            eprintln!(
                "DVANDVA_WRITE current_baton_unparseable file={bf} bad_checkpoint={}",
                jq_render(field(&cur_doc, "checkpoint"))
            );
            return TransitionOutcome::Exit(25);
        }
    };
    let cur_locked = bool_field(&cur_doc, "master_plan_locked");
    let cur_resume_assignee = str_field(&cur_doc, "resume_assignee");
    let cur_resume_status = str_field(&cur_doc, "resume_status");
    let cur_run_id = str_field(&cur_doc, "run_id");
    let cur_phase = jq_render(field(&cur_doc, "phase"));
    let cur_vadi_approval = bool_field(&cur_doc, "vadi_final_approval");
    let cur_prativadi_approval = bool_field(&cur_doc, "prativadi_final_approval");
    let cur_mode = str_field(&cur_doc, "mode");

    if cur_schema != "dvandva.baton.v1" && cur_schema != "dvandva.baton.v2" {
        eprintln!("DVANDVA_WRITE current_baton_unparseable file={bf} bad_schema={cur_schema}");
        return TransitionOutcome::Exit(25);
    }
    if cur_schema == "dvandva.baton.v2" && !util::is_safe_run_id(&cur_run_id) {
        eprintln!("DVANDVA_WRITE current_baton_unparseable file={bf} bad_run_id={cur_run_id}");
        return TransitionOutcome::Exit(25);
    }

    let mut cur_effective_mode = String::new();
    let mut cur_effective_profile = String::new();
    let mut cur_profile_floor = String::new();
    if cur_schema == "dvandva.baton.v2" {
        cur_effective_mode = match canonical_mode(&cur_mode) {
            Some(mode) => mode,
            None => {
                eprintln!("DVANDVA_WRITE current_baton_unparseable file={bf} bad_mode={cur_mode}");
                return TransitionOutcome::Exit(25);
            }
        };
        if cur_effective_mode == "development" {
            cur_effective_profile = if present(field(&cur_doc, "profile")) {
                str_field(&cur_doc, "profile")
            } else {
                "full".to_string()
            };
            cur_profile_floor = if present(field(&cur_doc, "profile_floor")) {
                str_field(&cur_doc, "profile_floor")
            } else {
                cur_effective_profile.clone()
            };
        }
    }

    // ---- F7 amendment state -----------------------------------------------
    let cur_amendment = amendment_value(&cur_doc);
    let cand_amendment = amendment_value(cand);
    let cur_phase_num: Option<i64> = cur_phase.parse::<i64>().ok();
    let new_phase_num: Option<i64> = cx.new_phase.parse::<i64>().ok();

    // ---- approval gate reason ----------------------------------------------
    let writer_role = std::env::var("DVANDVA_ROLE").unwrap_or_default();
    let mut approval_reason = String::new();
    if cx.is_v2 && cx.new_status != "done" {
        let approval_reset = cur_status == "termination_review" && cx.new_status == "phase_fixing";
        if approval_reset && (cx.new_vadi_approval || cx.new_prativadi_approval) {
            approval_reason =
                "stale_approval: termination_review->phase_fixing must reset both final approvals"
                    .to_string();
        } else if cx.new_status != "termination_review"
            && cx.new_vadi_approval
            && !cur_vadi_approval
        {
            approval_reason = "approval_out_of_band: vadi_final_approval can only be raised while entering termination_review".to_string();
        } else if cx.new_status != "termination_review"
            && cx.new_prativadi_approval
            && !cur_prativadi_approval
        {
            approval_reason = "approval_out_of_band: prativadi_final_approval can only be raised while entering termination_review".to_string();
        } else if !approval_reset
            && cx.new_vadi_approval != cur_vadi_approval
            && writer_role != "vadi"
        {
            approval_reason =
                "final approval ownership requires DVANDVA_ROLE=vadi to change vadi_final_approval"
                    .to_string();
        } else if !approval_reset
            && cx.new_prativadi_approval != cur_prativadi_approval
            && writer_role != "prativadi"
        {
            approval_reason = "final approval ownership requires DVANDVA_ROLE=prativadi to change prativadi_final_approval".to_string();
        }
    }

    // ---- loop gate reason --------------------------------------------------
    let mut loop_reason = String::new();
    if cx.is_v2 && cx.new_status != "human_decision" {
        let edge = format!("{cur_status}:{}", cx.new_status);
        let is_loop_edge = matches!(
            edge.as_str(),
            "deep_review:phase_fixing"
                | "cross_review:cross_fixing"
                | "termination_review:phase_fixing"
                | "phase_review:phase_fixing"
                | "review_of_review:counter_review"
                | "counter_review:review_of_review"
        );
        let amendment_enter = cur_effective_mode == "development"
            && cx.new_effective_mode == "development"
            && is_amendment_enter_edge(cx.new_effective_profile, &cur_status, cx.new_status);
        if amendment_enter {
            // F7: the amendment entry edge is loop-capped on
            // plan_amendment:<from-phase> (from-phase = current numeric phase),
            // and is exempt from the phase-advance loop-reset (spec is not a
            // numeric phase advance).
            let amendment_edge = format!("plan_amendment:{cur_phase}");
            loop_reason = loop_edge_reason(&cur_doc, cand, &amendment_edge);
        } else if cx.new_phase != cur_phase && loop_counts_nonempty(cand) {
            loop_reason = format!(
                "bad_loop_counts phase_advanced current={cur_phase} candidate={} must_reset=true",
                cx.new_phase
            );
        } else if is_loop_edge {
            loop_reason = loop_edge_reason(&cur_doc, cand, &edge);
        }
    }

    // ---- review ownership reason -------------------------------------------
    let mut review_ownership_reason = String::new();
    if cx.is_v2 && !run_explainer_reviews_ownership_ok(&cur_doc, cand, &writer_role) {
        review_ownership_reason = "run explainer review ownership requires DVANDVA_ROLE=vadi/prativadi and only that role may change its own run_explainer_reviews entries".to_string();
    }

    // ---- compact done phase-review checkpoint gate (baton exists) ----------
    if cx.is_v2
        && cx.new_effective_mode == "development"
        && cx.new_status == "done"
        && cx.new_effective_profile != "full"
    {
        let required = phase_review_cycle_checkpoint(baton_file, cur_checkpoint_i64, &cur_phase);
        if !compact_done_phase_review_checkpoint_ok(cand, required) {
            eprintln!("DVANDVA_WRITE bad_compact_terminal_evidence candidate={cf}");
            return TransitionOutcome::Exit(23);
        }
    }

    // ---- profile_history superset (append-only) ----------------------------
    if cx.is_v2
        && cur_effective_mode == "development"
        && cx.new_effective_mode == "development"
        && !profile_history_superset(&cur_doc, cand)
    {
        eprintln!("DVANDVA_WRITE bad_profile_history candidate={cf}");
        return TransitionOutcome::Exit(23);
    }

    // ---- profile_history low-floor guard -----------------------------------
    if cx.is_v2
        && cur_effective_mode == "development"
        && cx.new_effective_mode == "development"
        && cx.new_status != "human_decision"
        && !profile_history_low_floor_ok(&cur_doc, cand, &cur_profile_floor)
    {
        eprintln!("DVANDVA_WRITE bad_profile_downgrade candidate={cf}");
        return TransitionOutcome::Exit(23);
    }

    // ---- profile escalation history entry ----------------------------------
    if cx.is_v2
        && cur_effective_mode == "development"
        && cx.new_effective_mode == "development"
        && cx.new_status != "human_decision"
        && (cx.new_effective_profile != cur_effective_profile
            || cx.new_profile_floor != cur_profile_floor)
        && !profile_escalation_entry_ok(
            cand,
            &cur_effective_profile,
            cx.new_effective_profile,
            cx.new_profile_floor,
            cx.new_checkpoint,
        )
    {
        eprintln!("DVANDVA_WRITE bad_profile_history candidate={cf}");
        return TransitionOutcome::Exit(23);
    }

    // ---- precedence chain (order load-bearing) -----------------------------
    let mut legal = false;
    let mut reason = String::new();

    let dev_dev =
        cx.is_v2 && cur_effective_mode == "development" && cx.new_effective_mode == "development";

    if cur_schema != cx.schema {
        reason = format!("schema_change current={cur_schema} candidate={}", cx.schema);
    } else if cx.is_v2 && cur_run_id != str_field(cand, "run_id") {
        reason = format!(
            "run_id_change current={cur_run_id} candidate={}",
            str_field(cand, "run_id")
        );
    } else if cx.is_v2 && cur_effective_mode != cx.new_effective_mode {
        reason = format!("mode_change current={cur_mode} candidate={}", cx.new_mode);
    } else if dev_dev
        && cx.new_status != "human_decision"
        && (profile_rank(cx.new_effective_profile) < profile_rank(&cur_profile_floor)
            || profile_rank(cx.new_profile_floor) < profile_rank(&cur_profile_floor))
    {
        // Two distinct downgrade guards (effective profile below floor; floor
        // itself lowered below the current floor) share one action.
        eprintln!("DVANDVA_WRITE bad_profile_downgrade candidate={cf}");
        return TransitionOutcome::Exit(23);
    } else if (cx.new_checkpoint as i64) <= cur_checkpoint_i64 {
        eprintln!(
            "DVANDVA_WRITE stale_checkpoint current={cur_checkpoint_i64} candidate={}",
            cx.new_checkpoint
        );
        return TransitionOutcome::Exit(27);
    } else if cx.new_checkpoint as i64 != cur_checkpoint_i64 + 1 {
        reason = format!(
            "checkpoint must be {}, got {}",
            cur_checkpoint_i64 + 1,
            cx.new_checkpoint
        );
    } else if approval_reason.starts_with("approval_out_of_band")
        || approval_reason.starts_with("stale_approval")
    {
        eprintln!("DVANDVA_WRITE {approval_reason}");
        return TransitionOutcome::Exit(23);
    } else if !approval_reason.is_empty() {
        reason = approval_reason.clone();
    } else if !loop_reason.is_empty() {
        eprintln!("DVANDVA_WRITE {loop_reason}");
        return TransitionOutcome::Exit(23);
    } else if !review_ownership_reason.is_empty()
        && (cx.new_status != "done"
            || (cur_status == "termination_review" && cur_vadi_approval && cur_prativadi_approval))
    {
        reason = review_ownership_reason.clone();
    } else if cx.new_status == cur_status {
        if cx.is_v2 {
            if is_team_sync_status(cx.new_status) {
                if cx.new_phase != cur_phase {
                    reason = format!(
                        "same-status team sync cannot change phase current={cur_phase} candidate={}",
                        cx.new_phase
                    );
                } else if team_sync_fields_ok(cand) {
                    legal = true;
                } else {
                    reason = "same-status team sync requires team assignee, both active_roles, summary, and next_action".to_string();
                }
            } else {
                reason = "same-status rewrite (only v2 team sync checkpoints may keep status)"
                    .to_string();
            }
        } else {
            reason = "same-status rewrite (one baton write per handoff)".to_string();
        }
    } else if cur_status == "human_question" {
        if cx.new_status == "human_decision" {
            legal = true;
        } else if cur_resume_status == "done" || cx.new_status == "done" {
            reason = "human_question cannot resume directly to done".to_string();
        } else if cx.new_status == cur_resume_status
            && cx.new_assignee == cur_resume_assignee
            && cx.cand_q_null
            && cx.cand_ra_null
            && cx.cand_rs_null
        {
            legal = true;
        } else {
            reason = format!(
                "human_question resume must restore status={cur_resume_status} assignee={cur_resume_assignee} and clear question/resume fields"
            );
        }
    } else if cx.is_v2 && cx.new_status == "done" && cur_status != "termination_review" {
        reason = "done requires current status termination_review".to_string();
    } else if cx.is_v2 && cx.new_status == "done" && (!cur_vadi_approval || !cur_prativadi_approval)
    {
        reason = "done requires current termination_review with both final approvals".to_string();
    } else if cx.new_status == "human_decision" || cur_status == "human_decision" {
        // Two distinct legal reasons share the "legal" action: universal
        // escalation TO human_decision, and human-authorized resume FROM it
        // to any non-terminal protocol state.
        legal = true;
    } else if cx.new_status == "human_question" {
        if cur_locked {
            reason = "human_question is only legal before master_plan_locked".to_string();
        } else if !matches!(
            cur_status.as_str(),
            "spec_drafting"
                | "spec_review"
                | "spec_revision"
                | "research_drafting"
                | "research_review"
                | "research_revision"
        ) {
            reason = format!(
                "human_question only enters from spec or research states, not {cur_status}"
            );
        } else if cx.cand_q_null || cx.cand_ra_null || cx.cand_rs_null {
            reason = "human_question requires non-null question, resume_assignee, resume_status"
                .to_string();
        } else if str_field(cand, "resume_status") == "done" {
            reason = "human_question cannot resume directly to done".to_string();
        } else {
            legal = true;
        }
    } else {
        legal = edge_whitelist(
            cx.schema,
            &cur_effective_mode,
            cx.new_effective_profile,
            &cur_status,
            cx.new_status,
            &mut reason,
        );
    }

    // ---- post-legality edge gates ------------------------------------------
    if legal
        && cx.is_v2
        && cx.new_status == "parallel_implementing"
        && !parallel_work_split_ok(cand)
    {
        eprintln!("DVANDVA_WRITE bad_parallel_work_split candidate={cf}");
        return TransitionOutcome::Exit(23);
    }
    if legal
        && cx.is_v2
        && cur_status == "parallel_implementing"
        && cx.new_status == "test_creation"
        && !parallel_to_test_creation_ok(cand)
    {
        legal = false;
        reason = "parallel_implementing->test_creation requires completed implementation-chunk subagent_tracks for both roles".to_string();
    }
    if legal
        && cx.is_v2
        && cur_status == "test_creation"
        && cx.new_status == "cross_review"
        && !test_creation_to_cross_review_ok(cand)
    {
        legal = false;
        reason = "test_creation->cross_review requires completed test-creation subagent_track from dvandva-test-creator".to_string();
    }
    if legal && cx.is_v2 && cur_status == "cross_review" && cx.new_status == "cross_fixing" {
        let required = cross_review_cycle_checkpoint(baton_file, cur_checkpoint_i64);
        if !cross_review_to_cross_fixing_ok(cand, required) {
            legal = false;
            reason = "cross_review->cross_fixing requires current-cycle completed cross-review subagent_tracks with non-approval evidence".to_string();
        }
    }
    if legal && cx.is_v2 && cur_status == "cross_review" && cx.new_status == "deep_review" {
        let required = cross_review_cycle_checkpoint(baton_file, cur_checkpoint_i64);
        if !cross_review_to_deep_review_ok(cand, required) {
            legal = false;
            reason = "cross_review->deep_review requires current-cycle completed cross-review subagent_tracks for both roles with phase=\"cross_review\"".to_string();
        }
    }
    if legal && cx.is_v2 && cx.new_status == "review_of_review" && !narrow_fixups_ok(cand) {
        legal = false;
        reason = "review_of_review requires non-empty narrow_fixups".to_string();
    }
    if legal
        && cx.is_v2
        && cur_status == "deep_review"
        && (cx.new_status == "deslop" || cx.new_status == "review_of_review")
    {
        let required = deep_review_cycle_checkpoint(baton_file, cur_checkpoint_i64);
        if !deep_review_angles_ok(cand, required) {
            legal = false;
            reason = "deep_review->deslop requires current-cycle three completed review-angle subagent_tracks".to_string();
        }
    }

    // ---- F7 plan-amendment gates -------------------------------------------
    // total_phases is frozen once the master plan is locked, EXCEPT while a plan
    // amendment loop is active (amendment_from_phase non-null on either side).
    if legal
        && dev_dev
        && cur_locked
        && cur_amendment.is_none()
        && cand_amendment.is_none()
        && cx.new_status != "human_decision"
        && jq_render(field(&cur_doc, "total_phases")) != jq_render(field(cand, "total_phases"))
    {
        eprintln!("DVANDVA_WRITE bad_amendment total_phases_frozen candidate={cf}");
        return TransitionOutcome::Exit(23);
    }

    let is_enter =
        dev_dev && is_amendment_enter_edge(cx.new_effective_profile, &cur_status, cx.new_status);
    let is_exit =
        dev_dev && is_amendment_exit_edge(cx.new_effective_profile, &cur_status, cx.new_status);

    // The amendment entry edge MUST set amendment_from_phase == current phase.
    if legal && is_enter && (cand_amendment.is_none() || cand_amendment != cur_phase_num) {
        eprintln!("DVANDVA_WRITE bad_amendment candidate={cf}");
        return TransitionOutcome::Exit(23);
    }

    // amendment_from_phase may only BECOME non-null on an entry edge.
    if cx.is_v2 && cur_amendment.is_none() && cand_amendment.is_some() && !is_enter {
        eprintln!("DVANDVA_WRITE bad_amendment candidate={cf}");
        return TransitionOutcome::Exit(23);
    }

    // While the amendment loop is active (cur non-null, outside human states):
    // the exit edge must null the field and re-enter at phase >= from-phase; any
    // other step must leave amendment_from_phase unchanged.
    if let Some(from) = cur_amendment {
        if cx.is_v2 && cur_status != "human_decision" && cx.new_status != "human_decision" {
            if is_exit {
                if cand_amendment.is_some() {
                    eprintln!("DVANDVA_WRITE bad_amendment candidate={cf}");
                    return TransitionOutcome::Exit(23);
                }
                if new_phase_num.map(|p| p < from).unwrap_or(true) {
                    legal = false;
                    reason = format!(
                        "amendment re-entry phase {} below amendment_from_phase {from}",
                        cx.new_phase
                    );
                }
            } else if cand_amendment != Some(from) {
                eprintln!("DVANDVA_WRITE bad_amendment candidate={cf}");
                return TransitionOutcome::Exit(23);
            }
        }
    }

    if !legal {
        eprintln!("DVANDVA_WRITE illegal_transition {reason}");
        return TransitionOutcome::Exit(24);
    }
    TransitionOutcome::Legal
}

// ===========================================================================
// Stage 3: barrier, fencing, install, snapshot.
// ===========================================================================
fn install_and_snapshot(
    baton_file: &Path,
    candidate_file: &Path,
    _cand: &Value,
    mut plan: InstallPlan,
) -> i32 {
    // Test-only deterministic interleaving seam.
    if let Ok(barrier) = std::env::var("DVANDVA_WRITE_BARRIER") {
        if !barrier.is_empty() {
            let _ = std::fs::write(format!("{barrier}.arrived"), b"");
            let release = PathBuf::from(format!("{barrier}.release"));
            let mut waited = 0;
            while !release.exists() && waited < 200 {
                std::thread::sleep(std::time::Duration::from_millis(50));
                waited += 1;
            }
        }
    }

    // Fencing: re-verify we still own the lock before the irreversible install.
    if !plan.lock.holds() {
        eprintln!(
            "DVANDVA_WRITE lock_lost fencing_token_mismatch path={} refusing_to_install=true",
            plan.lock.dir.join(lock::LOCK_DIR_NAME).display()
        );
        plan.lock.disarm(); // the lock now belongs to the thief; do not remove it
        return 29;
    }

    let baton_dir = baton_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // Reap tmp files orphaned by a killed writer.
    reap_stale_tmp(&baton_dir);
    let tmp = baton_dir.join(format!(".baton.json.tmp.{}", std::process::id()));

    if std::fs::copy(candidate_file, &tmp).is_err() {
        eprintln!("DVANDVA_WRITE install_failed stage=cp");
        let _ = std::fs::remove_file(&tmp);
        return 26;
    }
    if std::fs::rename(&tmp, baton_file).is_err() {
        eprintln!("DVANDVA_WRITE install_failed stage=mv");
        let _ = std::fs::remove_file(&tmp);
        return 26;
    }

    if snapshot_baton(baton_file).is_err() {
        eprintln!(
            "DVANDVA_WRITE snapshot_failed file={} baton_is_installed=true",
            baton_file.display()
        );
        return 30;
    }

    println!(
        "DVANDVA_WRITE ok status={} assignee={} phase={} checkpoint={}",
        plan.status, plan.assignee, plan.phase, plan.checkpoint
    );
    0
}

fn reap_stale_tmp(baton_dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(baton_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(".baton.json.tmp.") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

// ===========================================================================
// Field / jq helpers
// ===========================================================================
fn field<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.as_object()?.get(key)
}

/// jq `.field // ""` -r semantics: coalesce null/false to absent, then render.
fn str_field(v: &Value, key: &str) -> String {
    match util::coalesce(field(v, key)) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => jq_tostring(other),
        None => String::new(),
    }
}

/// jq `.field // false` compared to boolean `true`.
fn bool_field(v: &Value, key: &str) -> bool {
    matches!(util::coalesce(field(v, key)), Some(Value::Bool(true)))
}

/// True when the field is JSON `null` or absent (jq `.field == null`).
fn is_null_field(v: &Value, key: &str) -> bool {
    matches!(field(v, key), None | Some(Value::Null))
}

/// jq `//` present test: not null and not false.
fn present(v: Option<&Value>) -> bool {
    util::coalesce(v).is_some()
}

/// jq `-r` / `tostring` rendering (strings unquoted, everything else as JSON).
fn jq_tostring(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// jq `-r` render of an optional field, `null`/absent -> "null".
fn jq_render(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "null".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// jq `nonblank`: a string containing at least one non-whitespace character.
fn nonblank(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::String(s)) if s.chars().any(|c| !c.is_whitespace()))
}

fn count_len(value: Option<&Value>) -> usize {
    match value {
        Some(Value::Array(a)) => a.len(),
        Some(Value::Object(o)) => o.len(),
        _ => 0,
    }
}

/// serde integer read that tolerates the `arbitrary_precision` feature.
fn json_int(value: &Value) -> Option<i64> {
    value.as_i64()
}

fn is_string_array_all(value: Option<&Value>, pred: impl Fn(&str) -> bool) -> bool {
    match value {
        Some(Value::Array(items)) => items.iter().all(|item| match item {
            Value::String(s) => pred(s),
            _ => false,
        }),
        _ => false,
    }
}

/// Iterate the values of a work_split/verification_matrix that may be an array
/// or an object (jq `.[]?`).
fn iter_values(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Object(map)) => map.values().collect(),
        _ => Vec::new(),
    }
}

fn arr(value: Option<&Value>) -> &[Value] {
    match value {
        Some(Value::Array(items)) => items.as_slice(),
        _ => &[],
    }
}

// ===========================================================================
// Static config: required keys, enums, canonical mode, ranks
// ===========================================================================
fn required_keys(is_v2: bool) -> Vec<&'static str> {
    let mut keys = vec![
        "schema",
        "updated_at",
        "mode",
        "run_mode",
        "phase",
        "total_phases",
        "status",
        "assignee",
        "current_engine",
        "review_target",
        "plan_ref",
        "master_plan_locked",
        "question",
        "resume_assignee",
        "resume_status",
        "disagreement_round",
        "disagreement_cap",
        "turn_cap",
        "branch",
        "checkpoint",
        "allow_commit",
        "allow_push",
        "allow_pr",
        "vadi_final_approval",
        "prativadi_final_approval",
        "final_commit",
        "pushed_ref",
        "summary",
        "changed_paths",
        "verification",
        "findings",
        "narrow_fixups",
        "vadi_counter",
        "deferred",
        "blockers",
        "next_action",
    ];
    if is_v2 {
        keys.extend_from_slice(&[
            "run_id",
            "original_ask",
            "research_ref",
            "run_explainer_ref",
            "active_roles",
            "agent_instances",
            "work_split",
            "subagent_tracks",
            "verification_matrix",
        ]);
    }
    keys
}

fn review_target_ok(cand: &Value) -> bool {
    match field(cand, "review_target") {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => matches!(
            s.as_str(),
            "research" | "spec" | "implementation" | "prativadi_fixups" | "vadi_counter"
        ),
        _ => false,
    }
}

fn canonical_mode(mode: &str) -> Option<String> {
    match mode {
        "development" | "feature-pr" => Some("development".to_string()),
        "research" | "review" => Some(mode.to_string()),
        _ => None,
    }
}

fn profile_rank(profile: &str) -> i32 {
    match profile {
        "fast" => 1,
        "standard" => 2,
        "full" => 3,
        _ => 0,
    }
}

fn v2_expected_assignee(status: &str) -> &'static str {
    match status {
        // F8: test_creation is team-owned in the v2 full profile (its only home).
        "research_drafting" | "research_revision" | "spec_drafting" | "spec_revision"
        | "implementing" | "deslop" | "phase_fixing" | "review_of_review" => "vadi",
        "parallel_implementing"
        | "test_creation"
        | "cross_review"
        | "cross_fixing"
        | "termination_review" => "team",
        "research_review" | "spec_review" | "deep_review" | "phase_review" | "counter_review" => {
            "prativadi"
        }
        "human_question" | "human_decision" => "human",
        _ => "",
    }
}

fn is_team_sync_status(status: &str) -> bool {
    matches!(
        status,
        // F8: test_creation joins the team-sync same-status set.
        "parallel_implementing"
            | "test_creation"
            | "cross_review"
            | "cross_fixing"
            | "termination_review"
    )
}

// ---------------------------------------------------------------------------
// F7 plan-amendment loop helpers.
// ---------------------------------------------------------------------------

/// `amendment_from_phase`: absent/null/non-integer-number -> None; integer -> Some.
fn amendment_from_phase_shape_ok(cand: &Value) -> bool {
    match field(cand, "amendment_from_phase") {
        None | Some(Value::Null) => true,
        Some(Value::Number(n)) => n.as_i64().is_some(),
        _ => false,
    }
}

/// The numeric `amendment_from_phase` value, or None when null/absent.
fn amendment_value(doc: &Value) -> Option<i64> {
    match field(doc, "amendment_from_phase") {
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

/// The plan-amendment entry edges: full `deslop -> spec_revision`, standard
/// `phase_review -> spec_revision`.
fn is_amendment_enter_edge(profile: &str, cur_status: &str, new_status: &str) -> bool {
    new_status == "spec_revision"
        && ((profile == "full" && cur_status == "deslop")
            || (profile == "standard" && cur_status == "phase_review"))
}

/// The plan-amendment exit edges: full `spec_review -> parallel_implementing`,
/// standard `spec_review -> implementing`.
fn is_amendment_exit_edge(profile: &str, cur_status: &str, new_status: &str) -> bool {
    cur_status == "spec_review"
        && ((profile == "full" && new_status == "parallel_implementing")
            || (profile == "standard" && new_status == "implementing"))
}

fn status_enum_ok(is_v2: bool, status: &str) -> bool {
    if is_v2 {
        matches!(
            status,
            "research_drafting"
                | "research_review"
                | "research_revision"
                | "spec_drafting"
                | "spec_review"
                | "spec_revision"
                | "human_question"
                | "implementing"
                | "parallel_implementing"
                | "test_creation"
                | "cross_review"
                | "cross_fixing"
                | "deep_review"
                | "deslop"
                | "termination_review"
                | "phase_review"
                | "phase_fixing"
                | "review_of_review"
                | "counter_review"
                | "human_decision"
                | "done"
        )
    } else {
        matches!(
            status,
            "spec_drafting"
                | "spec_review"
                | "spec_revision"
                | "human_question"
                | "implementing"
                | "phase_review"
                | "phase_fixing"
                | "review_of_review"
                | "counter_review"
                | "human_decision"
                | "done"
        )
    }
}

fn named_run_dir_id(baton_file: &Path) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(^|/)\.dvandva/runs/([^/]+)/baton\.json$").expect("static regex")
    });
    let path = baton_file.to_string_lossy();
    re.captures(&path)
        .map(|caps| caps.get(2).unwrap().as_str().to_string())
}

/// LOCK_TIMEOUT canonical positive decimal `^[1-9][0-9]*$`.
fn canonical_positive_decimal(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(c) if ('1'..='9').contains(&c) => chars.all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

// ===========================================================================
// v2 phase↔status pairing
// ===========================================================================
fn phase_status_ok(mode: &str, status: &str, cand: &Value) -> bool {
    let phase = field(cand, "phase");
    let is_str = |want: &str| matches!(phase, Some(Value::String(s)) if s == want);
    let is_num = || matches!(phase, Some(Value::Number(_)));
    match (mode, status) {
        (_, "human_question") | (_, "human_decision") => true,
        ("development", "research_drafting" | "research_review" | "research_revision") => {
            is_str("research")
        }
        ("development", "spec_drafting" | "spec_review" | "spec_revision") => is_str("spec"),
        (
            "development",
            "implementing"
            | "parallel_implementing"
            | "test_creation"
            | "cross_review"
            | "cross_fixing"
            | "deep_review"
            | "deslop"
            | "termination_review"
            | "phase_review"
            | "phase_fixing"
            | "review_of_review"
            | "counter_review"
            | "done",
        ) => is_num(),
        ("research", "research_drafting" | "research_review" | "research_revision") => {
            is_str("research")
        }
        ("research", _) => is_str("spec"),
        ("review", _) => is_str("review"),
        _ => true,
    }
}

// ===========================================================================
// active_roles / done universal
// ===========================================================================
fn active_roles_shape_ok(cand: &Value) -> bool {
    match field(cand, "active_roles") {
        Some(Value::Array(items)) => {
            let all_valid = items
                .iter()
                .all(|r| matches!(r, Value::String(s) if s == "vadi" || s == "prativadi"));
            // jq `unique` sorts+dedups; duplicates anywhere must be caught.
            let unique_len = {
                let mut seen: Vec<&Value> = Vec::new();
                for item in items {
                    if !seen.contains(&item) {
                        seen.push(item);
                    }
                }
                seen.len()
            };
            all_valid && unique_len == items.len()
        }
        _ => false,
    }
}

fn active_roles_sorted_both(cand: &Value) -> bool {
    match field(cand, "active_roles") {
        Some(Value::Array(items)) => {
            let mut roles: Vec<String> = items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            roles.sort();
            roles == ["prativadi".to_string(), "vadi".to_string()]
        }
        _ => false,
    }
}

fn done_state_ok(cand: &Value) -> bool {
    let assignee_ok = matches!(
        field(cand, "assignee"),
        Some(Value::String(s)) if matches!(s.as_str(), "human" | "team" | "vadi" | "prativadi")
    );
    assignee_ok
        && bool_field(cand, "vadi_final_approval")
        && bool_field(cand, "prativadi_final_approval")
}

fn team_sync_fields_ok(cand: &Value) -> bool {
    matches!(field(cand, "assignee"), Some(Value::String(s)) if s == "team")
        && active_roles_sorted_both(cand)
        && nonblank(field(cand, "summary"))
        && nonblank(field(cand, "next_action"))
}

// ===========================================================================
// Profile validation
// ===========================================================================
fn profile_value(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::String(s)) if matches!(s.as_str(), "fast" | "standard" | "full"))
}

fn profile_block_ok(cand: &Value) -> bool {
    // profile / profile_floor: absent, null, or a valid profile string.
    let prof_ok = match field(cand, "profile") {
        None | Some(Value::Null) => true,
        v => profile_value(v),
    };
    let floor_ok = match field(cand, "profile_floor") {
        None | Some(Value::Null) => true,
        v => profile_value(v),
    };
    if !prof_ok || !floor_ok {
        return false;
    }
    // profile_history: absent -> ok; present -> must be array with valid entries.
    if let Some(hist) = field(cand, "profile_history") {
        match hist {
            Value::Array(entries) => {
                for e in entries {
                    if !profile_history_entry_ok(e) {
                        return false;
                    }
                }
            }
            _ => return false,
        }
    }
    // profile_decision: absent/null -> ok; else object with required shape.
    match field(cand, "profile_decision") {
        None | Some(Value::Null) => true,
        Some(Value::Object(_)) => profile_decision_ok(field(cand, "profile_decision").unwrap()),
        _ => false,
    }
}

fn profile_history_entry_ok(e: &Value) -> bool {
    let from_ok = match field(e, "from") {
        Some(Value::Null) | None => true,
        v => profile_value(v),
    };
    from_ok
        && profile_value(field(e, "to"))
        && profile_value(field(e, "floor"))
        && matches!(field(e, "checkpoint"), Some(Value::Number(_)))
        && matches!(field(e, "actor_role"), Some(Value::String(s)) if matches!(s.as_str(), "vadi" | "prativadi" | "human" | "team"))
        && nonblank(field(e, "reason"))
        && matches!(field(e, "evidence_refs"), Some(Value::Array(_)))
}

fn profile_decision_ok(pd: &Value) -> bool {
    if !pd.is_object() {
        return false;
    }
    profile_value(field(pd, "selected_profile"))
        && profile_value(field(pd, "floor"))
        && nonblank(field(pd, "reason"))
        && nonblank(field(pd, "decided_by"))
        && matches!(
            field(pd, "decided_at"),
            None | Some(Value::Null) | Some(Value::String(_))
        )
        && matches!(field(pd, "risk_inputs"), Some(Value::Array(_)))
        && matches!(field(pd, "hard_triggers"), Some(Value::Array(_)))
        && matches!(field(pd, "allowlist_match"), Some(Value::Bool(_)))
        && matches!(field(pd, "allowlist_refs"), Some(Value::Array(_)))
        && matches!(field(pd, "evidence_refs"), Some(Value::Array(_)))
}

fn fresh_scaffold_profile_present(cand: &Value) -> bool {
    present(field(cand, "profile"))
        && present(field(cand, "profile_floor"))
        && matches!(field(cand, "profile_decision"), Some(Value::Object(_)))
        && matches!(field(cand, "profile_history"), Some(Value::Array(_)))
}

/// All candidate paths considered by the hard-path / fast-allowlist gates:
/// changed_paths, work_split paths/read_paths/write_paths, agent_instances
/// read_paths/write_paths.
fn candidate_paths(cand: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let push_strings = |out: &mut Vec<String>, v: Option<&Value>| {
        if let Some(Value::Array(items)) = v {
            for it in items {
                if let Value::String(s) = it {
                    out.push(s.clone());
                }
            }
        }
    };
    push_strings(&mut out, field(cand, "changed_paths"));
    for item in iter_values(field(cand, "work_split")) {
        push_strings(&mut out, field(item, "paths"));
        push_strings(&mut out, field(item, "read_paths"));
        push_strings(&mut out, field(item, "write_paths"));
    }
    for item in arr(field(cand, "agent_instances")) {
        push_strings(&mut out, field(item, "read_paths"));
        push_strings(&mut out, field(item, "write_paths"));
    }
    out
}

/// The hard-path set that forces profile floor `full`.
///
/// DESIGN D6 (the one deliberate change): the three shell-script glob patterns
/// (`plugins/dvandva/skills/*/scripts/dvandva-*.sh`, `scripts/*.sh`,
/// `plugins/dvandva/scripts/*.sh`) are replaced by the Rust source and test
/// trees. Every other entry is byte-identical to the shell.
fn hard_path(p: &str) -> bool {
    static SKILL_MD: OnceLock<Regex> = OnceLock::new();
    static COMMANDS_MD: OnceLock<Regex> = OnceLock::new();
    static ENV_RE: OnceLock<Regex> = OnceLock::new();
    static SECRETS_RE: OnceLock<Regex> = OnceLock::new();
    static API_RE: OnceLock<Regex> = OnceLock::new();
    static LOCKS_RE: OnceLock<Regex> = OnceLock::new();
    let skill_md =
        SKILL_MD.get_or_init(|| Regex::new(r"^plugins/dvandva/skills/[^/]+/SKILL\.md$").unwrap());
    let commands_md =
        COMMANDS_MD.get_or_init(|| Regex::new(r"^plugins/dvandva/commands/[^/]+\.md$").unwrap());
    let env_re = ENV_RE.get_or_init(|| Regex::new(r"(^|/)\.env(\..*)?$").unwrap());
    let secrets_re = SECRETS_RE
        .get_or_init(|| Regex::new(r"(^|/)(secret|secrets|credential|credentials)(/|$)").unwrap());
    let api_re = API_RE.get_or_init(|| Regex::new(r"(^|/)(api|apis|client|clients)(/|$)").unwrap());
    let locks_re = LOCKS_RE.get_or_init(|| {
        Regex::new(r"(^|/)(package-lock\.json|package\.json|pnpm-lock\.yaml|yarn\.lock|requirements\.txt|pyproject\.toml|Cargo\.toml|Cargo\.lock)$").unwrap()
    });

    p == ".dvandva"
        || p.starts_with(".dvandva/")
        || p.starts_with(".githooks/")
        || p.starts_with(".dvandva/githooks/")
        || p == "product.md"
        || p == "plugins/dvandva/references/baton-schema-v2.json"
        || p == "plugins/dvandva/references/state-transition-table.md"
        || p == "plugins/dvandva/references/local-baton-channel.md"
        || p == "docs/protocol/local-baton-channel.md"
        || p == "templates/channel/baton.json"
        || skill_md.is_match(p)
        || commands_md.is_match(p)
        // ---- D6 re-key: Rust source + test trees ----
        || p.starts_with("rust/dvandva/src/")
        || p.starts_with("rust/dvandva/tests/")
        // ---------------------------------------------
        || env_re.is_match(p)
        || secrets_re.is_match(p)
        || api_re.is_match(p)
        || locks_re.is_match(p)
}

fn fast_allowlist_ok(cand: &Value) -> bool {
    let pd = field(cand, "profile_decision");
    let allowlist_match = pd
        .and_then(|v| field(v, "allowlist_match"))
        .map(|v| matches!(v, Value::Bool(true)))
        .unwrap_or(false);
    let evidence_ok = matches!(
        pd.and_then(|v| field(v, "evidence_refs")),
        Some(Value::Array(a)) if !a.is_empty()
    );
    let allow_path = |p: &str| {
        p == "README.md" || p.starts_with("docs/research/") || p.starts_with("docs/case-studies/")
    };
    allowlist_match && evidence_ok && candidate_paths(cand).iter().all(|p| allow_path(p))
}

// ===========================================================================
// Agent instances
// ===========================================================================
fn agent_instances_ok(cand: &Value) -> bool {
    let instances = match field(cand, "agent_instances") {
        Some(Value::Array(items)) => items,
        _ => return false,
    };
    // unique ids
    let ids: Vec<&Value> = instances
        .iter()
        .map(|i| field(i, "id").unwrap_or(&Value::Null))
        .collect();
    let mut seen: Vec<&Value> = Vec::new();
    for id in &ids {
        if !seen.contains(id) {
            seen.push(id);
        }
    }
    if seen.len() != ids.len() {
        return false;
    }
    instances.iter().all(agent_instance_entry_ok)
}

fn generated_instance(inst: &Value) -> bool {
    str_field(inst, "agent_kind") == "generated"
        || field(inst, "parent_role").is_some()
        || field(inst, "permission_class").is_some()
        || field(inst, "model_class").is_some()
}

fn agent_instance_entry_ok(inst: &Value) -> bool {
    let id_ok = matches!(field(inst, "id"), Some(Value::String(s)) if util::is_safe_run_id(s));
    if !id_ok {
        return false;
    }
    if !generated_instance(inst) {
        return true;
    }
    let id = str_field(inst, "id");
    if reserved_agent_id(&id) {
        return false;
    }
    let parent_ok = matches!(field(inst, "parent_role"), Some(Value::String(s)) if s == "vadi" || s == "prativadi");
    let valid_model = matches!(field(inst, "model_class"), Some(Value::String(s)) if matches!(
        s.as_str(),
        "opus-class|gpt-5.5" | "sonnet-class|gpt-5.4" | "opus" | "sonnet" | "gpt-5.5" | "gpt-5.4"
    ));
    let valid_perm = matches!(field(inst, "permission_class"), Some(Value::String(s)) if matches!(
        s.as_str(),
        "readonly" | "verify-only" | "edit-scoped" | "write-artifact-only"
    ));
    let phase_ok = match field(inst, "phase") {
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(_)) => true,
        _ => false,
    };
    let status = str_field(inst, "status");
    let status_ok = matches!(
        status.as_str(),
        "planned" | "running" | "closed" | "rejected" | "collapsed"
    );

    let base = parent_ok
        && nonblank(field(inst, "spawned_by"))
        && matches!(field(inst, "spawned_at_checkpoint"), Some(Value::Number(_)))
        && phase_ok
        && nonblank(field(inst, "purpose"))
        && str_field(inst, "agent_kind") == "generated"
        && valid_model
        && valid_perm
        && status_ok
        && matches!(field(inst, "work_item_ids"), Some(Value::Array(_)))
        && is_string_array_all(field(inst, "read_paths"), util::is_safe_rel_path)
        && is_string_array_all(field(inst, "write_paths"), util::is_safe_rel_path)
        && matches!(field(inst, "depends_on"), Some(Value::Array(_)))
        && matches!(field(inst, "output_refs"), Some(Value::Array(_)))
        && matches!(field(inst, "evidence_refs"), Some(Value::Array(_)))
        && matches!(field(inst, "base_checkpoint"), Some(Value::Number(_)));
    if !base {
        return false;
    }
    if status == "closed" {
        let closed_ok = nonblank(field(inst, "closed_at"))
            && nonblank(field(inst, "result"))
            && count_len(field(inst, "work_item_ids")) > 0
            && count_len(field(inst, "evidence_refs")) > 0
            && arr(field(inst, "evidence_refs"))
                .iter()
                .any(|e| matches!(e, Value::String(s) if s.starts_with("closed:")));
        return closed_ok;
    }
    true
}

fn reserved_agent_id(id: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(dvandva-(researcher|architect|implementer|test-creator|cross-reviewer|adversarial-analyst|deep-reviewer|deslopper|sandbox-verifier|baton-auditor|security-auditor|integration-checker|debugger|doc-verifier|pattern-mapper)|adversarial-analyst|quality-reviewer|sandbox-executor|architect|developer|vadi|prativadi|team|human)$").unwrap()
    });
    re.is_match(id)
}

fn path_overlap(left: &str, right: &str) -> bool {
    left == right
        || left.starts_with(&format!("{right}/"))
        || right.starts_with(&format!("{left}/"))
}

fn agent_instances_write_paths_ok(cand: &Value) -> bool {
    let generated_live = |inst: &Value| {
        str_field(inst, "agent_kind") == "generated"
            && str_field(inst, "status") != "rejected"
            && str_field(inst, "status") != "collapsed"
    };
    let live = |inst: &Value| {
        let s = str_field(inst, "status");
        s == "planned" || s == "running"
    };
    let instances: Vec<&Value> = arr(field(cand, "agent_instances"))
        .iter()
        .filter(|i| generated_live(i) && count_len(field(i, "write_paths")) > 0)
        .collect();
    for i in 0..instances.len() {
        for j in (i + 1)..instances.len() {
            let a = instances[i];
            let b = instances[j];
            let base_eq = field(a, "base_checkpoint") == field(b, "base_checkpoint");
            let both_live = live(a) && live(b);
            if (base_eq || both_live) && write_paths_overlap(a, b) && !serialized(a, b) {
                return false;
            }
        }
    }
    true
}

fn write_paths_overlap(a: &Value, b: &Value) -> bool {
    let aw = arr(field(a, "write_paths"));
    let bw = arr(field(b, "write_paths"));
    aw.iter().any(|pa| {
        if let Value::String(pa) = pa {
            bw.iter()
                .any(|pb| matches!(pb, Value::String(pb) if path_overlap(pa, pb)))
        } else {
            false
        }
    })
}

fn serialized(a: &Value, b: &Value) -> bool {
    let cga = str_field(a, "conflict_group");
    let cgb = str_field(b, "conflict_group");
    let id_a = field(a, "id");
    let id_b = field(b, "id");
    let dep = |x: &Value, target: Option<&Value>| {
        target.is_some()
            && arr(field(x, "depends_on"))
                .iter()
                .any(|d| Some(d) == target)
    };
    !cga.is_empty() && cga == cgb && (dep(a, id_b) || dep(b, id_a))
}

// ===========================================================================
// work_split
// ===========================================================================
fn work_split_nonempty(cand: &Value) -> bool {
    matches!(
        field(cand, "work_split"),
        Some(Value::Array(_)) | Some(Value::Object(_))
    ) && count_len(field(cand, "work_split")) > 0
}

fn work_split_paths_ok(cand: &Value) -> bool {
    for item in iter_values(field(cand, "work_split")) {
        for key in ["paths", "read_paths", "write_paths"] {
            if let Some(v) = field(item, key) {
                if !is_string_array_all(Some(v), util::is_safe_rel_path) {
                    return false;
                }
            }
        }
        if let Some(v) = field(item, "depends_on") {
            if !matches!(v, Value::Array(_)) {
                return false;
            }
        }
        if let Some(v) = field(item, "conflict_group") {
            if !matches!(v, Value::String(_)) {
                return false;
            }
        }
    }
    true
}

/// Work item as (id, deps, value) for the depends_on DAG; objects default id to key.
fn work_items_with_ids(cand: &Value) -> Vec<(String, Vec<String>, Value)> {
    let mut out = Vec::new();
    match field(cand, "work_split") {
        Some(Value::Array(items)) => {
            for item in items {
                if item.is_object() {
                    let id = str_field(item, "id");
                    let deps = string_array(field(item, "depends_on"));
                    out.push((id, deps, item.clone()));
                }
            }
        }
        Some(Value::Object(map)) => {
            for (key, val) in map {
                if val.is_object() {
                    let id = if present(field(val, "id")) {
                        str_field(val, "id")
                    } else {
                        key.clone()
                    };
                    let deps = string_array(field(val, "depends_on"));
                    out.push((id, deps, val.clone()));
                }
            }
        }
        _ => {}
    }
    out
}

fn string_array(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(items)) => items
            .iter()
            .map(|it| match it {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn allowed_anchor(dep: &str) -> bool {
    matches!(
        dep,
        "spec-approved"
            | "parallel_implementing"
            | "implementing"
            | "test_creation"
            | "cross_review"
            | "deep_review"
            | "phase_review"
            | "deslop"
    )
}

fn depends_on_ok(cand: &Value) -> bool {
    let items = work_items_with_ids(cand);
    // depends_on must be arrays (jq re-checks here).
    for item in iter_values(field(cand, "work_split")) {
        match field(item, "depends_on") {
            None => {}
            Some(Value::Array(_)) => {}
            _ => return false,
        }
    }
    // collect ids that are non-empty strings
    let ids: Vec<String> = items
        .iter()
        .filter(|(id, _, _)| !id.is_empty())
        .map(|(id, _, _)| id.clone())
        .collect();
    // every dep must be a known id or an allowed anchor
    for (_, deps, _) in &items {
        for dep in deps {
            if !(ids.iter().any(|i| i == dep) || allowed_anchor(dep)) {
                return false;
            }
        }
    }
    // acyclicity via iterative strip-ready over intra-id deps only
    let mut nodes: Vec<(String, Vec<String>)> = items
        .iter()
        .filter(|(id, _, _)| !id.is_empty())
        .map(|(id, deps, _)| {
            let intra: Vec<String> = deps
                .iter()
                .filter(|d| ids.iter().any(|i| &i == d))
                .cloned()
                .collect();
            (id.clone(), intra)
        })
        .collect();

    loop {
        let ready: Vec<String> = nodes
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(id, _)| id.clone())
            .collect();
        if ready.is_empty() {
            break;
        }
        nodes = nodes
            .into_iter()
            .filter(|(id, _)| !ready.contains(id))
            .map(|(id, deps)| {
                let deps = deps.into_iter().filter(|d| !ready.contains(d)).collect();
                (id, deps)
            })
            .collect();
    }
    nodes.is_empty()
}

fn terminal_status(s: &str) -> bool {
    matches!(
        s,
        "completed"
            | "approved"
            | "passed"
            | "closed"
            | "done"
            | "rejected"
            | "collapsed"
            | "skipped"
            | "cancelled"
    )
}

fn chunk_kind(item: &Value, root_status: &str) -> String {
    let ct = str_field(item, "chunk_type");
    if !ct.is_empty() {
        return ct;
    }
    let t = str_field(item, "type");
    if !t.is_empty() {
        return t;
    }
    match root_status {
        "parallel_implementing" => "implementation".to_string(),
        "cross_fixing" => "cross_fixing".to_string(),
        _ => String::new(),
    }
}

fn owner_role_or_owner(item: &Value) -> String {
    let r = str_field(item, "owner_role");
    if !r.is_empty() {
        return r;
    }
    str_field(item, "owner")
}

fn effective_write_paths(item: &Value, root_status: &str) -> Vec<String> {
    let kind = chunk_kind(item, root_status);
    let write_capable = matches!(kind.as_str(), "implementation" | "cross_fixing" | "fix");
    if write_capable {
        let mut set: Vec<String> = Vec::new();
        for p in string_array(field(item, "paths"))
            .into_iter()
            .chain(string_array(field(item, "write_paths")))
        {
            if !set.contains(&p) {
                set.push(p);
            }
        }
        set
    } else if field(item, "write_paths").is_some() {
        string_array(field(item, "write_paths"))
    } else {
        Vec::new()
    }
}

fn work_split_write_paths_ok(cand: &Value) -> bool {
    let root_status = str_field(cand, "status");
    let root_phase = jq_render(field(cand, "phase"));
    let items = iter_values(field(cand, "work_split"));

    // Condition A: parallel_implementing requires each parallel impl chunk to
    // carry >=1 effective write path.
    if root_status == "parallel_implementing" {
        for item in &items {
            if parallel_impl_chunk(item, &root_phase)
                && effective_write_paths(item, &root_status).is_empty()
            {
                return false;
            }
        }
    }

    // Condition B: live writers must not have overlapping non-serialized paths.
    let writers: Vec<&Value> = items
        .iter()
        .copied()
        .filter(|item| {
            let terminal = terminal_status(&str_field(item, "status"));
            !terminal && !effective_write_paths(item, &root_status).is_empty()
        })
        .collect();
    for i in 0..writers.len() {
        for j in (i + 1)..writers.len() {
            let a = writers[i];
            let b = writers[j];
            if work_overlap(a, b, &root_status) && !serialized_work(a, b) {
                return false;
            }
        }
    }
    true
}

fn parallel_impl_chunk(item: &Value, root_phase: &str) -> bool {
    chunk_kind(item, "").eq("implementation")
        && jq_render(field(item, "phase")) == *root_phase
        && matches!(owner_role_or_owner(item).as_str(), "vadi" | "prativadi")
        && matches!(
            str_field(item, "cross_review_by").as_str(),
            "vadi" | "prativadi"
        )
        && (field(item, "write_paths").is_some() || !string_array(field(item, "paths")).is_empty())
}

fn work_overlap(a: &Value, b: &Value, root_status: &str) -> bool {
    let aw = effective_write_paths(a, root_status);
    let bw = effective_write_paths(b, root_status);
    aw.iter().any(|pa| bw.iter().any(|pb| path_overlap(pa, pb)))
}

fn serialized_work(a: &Value, b: &Value) -> bool {
    let cga = str_field(a, "conflict_group");
    let cgb = str_field(b, "conflict_group");
    let id_a = str_field(a, "id");
    let id_b = str_field(b, "id");
    let dep = |x: &Value, target: &str| {
        !target.is_empty()
            && string_array(field(x, "depends_on"))
                .iter()
                .any(|d| d == target)
    };
    !cga.is_empty() && cga == cgb && (dep(a, &id_b) || dep(b, &id_a))
}

// ===========================================================================
// verification_matrix / subagent_tracks
// ===========================================================================
fn verification_matrix_nonempty(cand: &Value) -> bool {
    matches!(
        field(cand, "verification_matrix"),
        Some(Value::Array(_)) | Some(Value::Object(_))
    ) && count_len(field(cand, "verification_matrix")) > 0
}

fn subagent_tracks_ok(cand: &Value) -> bool {
    let tracks = match field(cand, "subagent_tracks") {
        Some(Value::Array(items)) if !items.is_empty() => items,
        _ => return false,
    };
    tracks.iter().all(|t| {
        let str_nonempty = |k: &str| matches!(field(t, k), Some(Value::String(s)) if !s.is_empty());
        let phase_ok = match field(t, "phase") {
            Some(Value::String(s)) => !s.is_empty(),
            Some(Value::Number(_)) => true,
            _ => false,
        };
        str_nonempty("id")
            && phase_ok
            && str_nonempty("status")
            && str_nonempty("track")
            && str_nonempty("owner")
            && matches!(field(t, "parallelized"), Some(Value::Bool(_)))
            && str_nonempty("rationale")
            && matches!(field(t, "inputs"), Some(Value::Array(_)))
            && matches!(field(t, "outputs"), Some(Value::Array(_)))
            && matches!(field(t, "evidence_refs"), Some(Value::Array(_)))
            && str_nonempty("result")
    })
}

fn track_static_owner(owner: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^dvandva-(researcher|architect|implementer|test-creator|cross-reviewer|adversarial-analyst|deep-reviewer|deslopper|sandbox-verifier|baton-auditor|security-auditor|integration-checker|debugger|doc-verifier|pattern-mapper)$").unwrap()
    });
    re.is_match(owner)
}

fn track_legacy_owner(owner: &str) -> bool {
    matches!(
        owner,
        "adversarial-analyst" | "quality-reviewer" | "sandbox-executor" | "architect" | "developer"
    )
}

fn track_coordinator_owner(owner: &str) -> bool {
    matches!(owner, "vadi" | "prativadi" | "team" | "human")
}

fn subagent_tracks_owner_ok(cand: &Value) -> bool {
    let tracks = arr(field(cand, "subagent_tracks"));
    tracks.iter().all(|track| {
        let owner = str_field(track, "owner");
        if track_coordinator_owner(&owner)
            || track_static_owner(&owner)
            || track_legacy_owner(&owner)
        {
            if bool_field(track, "parallelized") {
                count_len(field(track, "outputs")) > 0
                    || count_len(field(track, "evidence_refs")) > 0
            } else {
                true
            }
        } else {
            closed_agent_instance_for_track(cand, track)
                && (count_len(field(track, "outputs")) > 0
                    || count_len(field(track, "evidence_refs")) > 0)
        }
    })
}

fn subagent_tracks_have_dynamic_owner(cand: &Value) -> bool {
    arr(field(cand, "subagent_tracks")).iter().any(|track| {
        let owner = str_field(track, "owner");
        !track_static_owner(&owner)
            && !track_legacy_owner(&owner)
            && !track_coordinator_owner(&owner)
    })
}

fn closed_agent_instance_for_track(cand: &Value, track: &Value) -> bool {
    let owner = str_field(track, "owner");
    let track_owner_role = str_field(track, "owner_role");
    arr(field(cand, "agent_instances")).iter().any(|inst| {
        str_field(inst, "id") == owner
            && str_field(inst, "agent_kind") == "generated"
            && (track_owner_role.is_empty() || track_owner_role == str_field(inst, "parent_role"))
            && str_field(inst, "status") == "closed"
            && count_len(field(inst, "output_refs")) > 0
            && count_len(field(inst, "evidence_refs")) > 0
            && arr(field(inst, "evidence_refs"))
                .iter()
                .any(|e| matches!(e, Value::String(s) if s.starts_with("closed:")))
    })
}

// ===========================================================================
// done evidence by mode
// ===========================================================================
fn run_explainer_ref_matches_run_id(reference: &str, run_id: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    static DATE_PREFIX: OnceLock<Regex> = OnceLock::new();
    static STEM_DATE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^\./superpowers/run-reports/([A-Za-z0-9._-]+)-explainer\.html$").unwrap()
    });
    let date_prefix =
        DATE_PREFIX.get_or_init(|| Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}-").unwrap());
    let stem_date =
        STEM_DATE.get_or_init(|| Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}-(.+)$").unwrap());

    let stem = match re.captures(reference) {
        Some(caps) => caps.get(1).unwrap().as_str().to_string(),
        None => return false,
    };
    if date_prefix.is_match(run_id) {
        stem == run_id
    } else if let Some(caps) = stem_date.captures(&stem) {
        caps.get(1).unwrap().as_str() == run_id
    } else {
        false
    }
}

fn run_explainer_reviews_ok(cand: &Value) -> bool {
    let reference = match field(cand, "run_explainer_ref") {
        Some(Value::String(s)) => s.clone(),
        _ => return false,
    };
    let reviews = match field(cand, "run_explainer_reviews") {
        Some(Value::Array(items)) => items,
        _ => return false,
    };
    let reviewed_by = |role: &str| {
        reviews.iter().any(|r| {
            str_field(r, "role") == role
                && str_field(r, "artifact_ref") == reference
                && str_field(r, "status") == "completed"
                && str_field(r, "result") == "approved"
                && nonblank(field(r, "summary"))
                && matches!(field(r, "evidence_refs"), Some(Value::Array(a)) if !a.is_empty())
        })
    };
    reviewed_by("vadi") && reviewed_by("prativadi")
}

fn good_result(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::String(s)) if s == "passed" || s == "approved")
}

fn compact_terminal_evidence_ok(cand: &Value) -> bool {
    // profile_decision object
    if !matches!(field(cand, "profile_decision"), Some(Value::Object(_))) {
        return false;
    }
    // good_verification: any verification[] with result passed/approved and nonblank command
    let good_verification = arr(field(cand, "verification"))
        .iter()
        .any(|v| good_result(field(v, "result")) && nonblank(field(v, "command")));
    if !good_verification {
        return false;
    }
    // good_matrix
    let matrix = iter_values(field(cand, "verification_matrix"));
    let good_matrix = !matrix.is_empty()
        && matrix.iter().all(|m| {
            let current = field(m, "current").or_else(|| field(m, "result"));
            good_result(current)
                && matches!(field(m, "evidence_refs"), Some(Value::Array(a)) if !a.is_empty())
        });
    if !good_matrix {
        return false;
    }
    // good_phase_review
    arr(field(cand, "subagent_tracks")).iter().any(|t| {
        jq_render(field(t, "phase")) == "phase_review"
            && str_field(t, "track") == "phase-review"
            && str_field(t, "status") == "completed"
            && good_result(field(t, "result"))
            && phase_review_owner(t) == "prativadi"
            && count_len(field(t, "outputs")) > 0
            && count_len(field(t, "evidence_refs")) > 0
    })
}

fn phase_review_owner(t: &Value) -> String {
    let r = str_field(t, "owner_role");
    if !r.is_empty() {
        return r;
    }
    let role = str_field(t, "role");
    if !role.is_empty() {
        return role;
    }
    str_field(t, "owner")
}

fn compact_done_phase_review_checkpoint_ok(cand: &Value, required: i64) -> bool {
    arr(field(cand, "subagent_tracks")).iter().any(|t| {
        jq_render(field(t, "phase")) == "phase_review"
            && str_field(t, "track") == "phase-review"
            && field(t, "review_checkpoint").and_then(json_int) == Some(required)
            && str_field(t, "status") == "completed"
            && good_result(field(t, "result"))
            && phase_review_owner(t) == "prativadi"
            && count_len(field(t, "outputs")) > 0
            && count_len(field(t, "evidence_refs")) > 0
    })
}

fn research_done_ref_ok(cand: &Value) -> bool {
    let outcome_ok = matches!(field(cand, "research_outcome"), None | Some(Value::Null))
        || matches!(field(cand, "research_outcome"), Some(Value::String(s)) if s == "exploratory" || s == "seed_development");
    if !outcome_ok {
        return false;
    }
    let outcome = match field(cand, "research_outcome") {
        Some(Value::String(s)) => s.clone(),
        _ => "exploratory".to_string(),
    };
    let research_ref_ok =
        matches!(field(cand, "research_ref"), Some(Value::String(s)) if !s.is_empty());
    if !research_ref_ok {
        return false;
    }
    if outcome == "seed_development" {
        matches!(field(cand, "plan_ref"), Some(Value::String(s)) if !s.is_empty())
    } else {
        true
    }
}

fn review_ref_ok(cand: &Value) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    static BAD: OnceLock<Regex> = OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"^\./superpowers/reviews/[A-Za-z0-9._/-]+\.html$").unwrap());
    let bad = BAD.get_or_init(|| Regex::new(r"(^|/)\.\.(/|$)|//").unwrap());
    match field(cand, "review_ref") {
        Some(Value::String(s)) => re.is_match(s) && !bad.is_match(s),
        _ => false,
    }
}

// ===========================================================================
// loop counts
// ===========================================================================
fn loop_counts_nonempty(cand: &Value) -> bool {
    matches!(field(cand, "loop_counts"), Some(Value::Object(m)) if !m.is_empty())
}

/// `(.loop_counts // {})[edge] // 0` as a parseable non-negative integer.
/// Returns None when the value exists but is not a `^[0-9]+$` integer.
fn loop_count(doc: &Value, edge: &str) -> Option<u64> {
    let counts = match field(doc, "loop_counts") {
        Some(Value::Object(m)) => m,
        None | Some(Value::Null) | Some(Value::Bool(false)) => return Some(0),
        _ => return None, // loop_counts is a non-object -> jq index error -> "invalid"
    };
    match counts.get(edge) {
        None => Some(0),
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::Null) | Some(Value::Bool(false)) => Some(0),
        _ => None,
    }
}

/// The raw candidate value backing a [`loop_count`] `None` (malformed)
/// result, for echoing in a `bad_loop_counts` diagnostic — mirrors the
/// shell's `jq -r --arg edge "$edge" '(.loop_counts // {})[$edge] // 0'`,
/// which prints the actual offending value (a string raw/unquoted, a
/// non-scalar as its JSON text) rather than a canned "0".
fn loop_count_raw_display(doc: &Value, edge: &str) -> String {
    let counts = match field(doc, "loop_counts") {
        Some(Value::Object(m)) => m,
        None | Some(Value::Null) | Some(Value::Bool(false)) => return "0".to_string(),
        _ => return "invalid".to_string(), // loop_counts is a non-object -> jq index error
    };
    match counts.get(edge) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "0".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

fn loop_cap(cand: &Value) -> Option<u64> {
    match util::coalesce(field(cand, "disagreement_cap")) {
        Some(Value::Number(n)) => n.as_u64(),
        None => Some(0),
        _ => None,
    }
}

/// The loop-gate reason (empty when legal) for a single loop `edge`: cap
/// enforcement plus the mandatory exactly-one increment. Shared by the existing
/// review/fix loop edges and the F7 `plan_amendment:<from-phase>` edge.
fn loop_edge_reason(cur_doc: &Value, cand: &Value, edge: &str) -> String {
    let cur_count = loop_count(cur_doc, edge);
    let new_count = loop_count(cand, edge);
    let cap = loop_cap(cand);
    match (cur_count, new_count, cap) {
        (Some(cur_c), Some(new_c), Some(cap)) if cap > 0 => {
            if cur_c >= cap {
                format!("loop_cap edge={edge} count={cur_c} cap={cap}")
            } else if new_c != cur_c + 1 {
                format!(
                    "bad_loop_counts edge={edge} expected={} got={new_c}",
                    cur_c + 1
                )
            } else {
                String::new()
            }
        }
        (_, new_c, _) => {
            let got = new_c
                .map(|n| n.to_string())
                .unwrap_or_else(|| loop_count_raw_display(cand, edge));
            format!("bad_loop_counts edge={edge} count={got}")
        }
    }
}

// ===========================================================================
// run_explainer_reviews ownership
// ===========================================================================
fn run_explainer_reviews_ownership_ok(cur: &Value, cand: &Value, role: &str) -> bool {
    let protected = |doc: &Value| -> Vec<String> {
        let reviews = match field(doc, "run_explainer_reviews") {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        let mut out: Vec<String> = reviews
            .iter()
            .filter(|r| str_field(r, "role") != role)
            .map(|r| serde_json::to_string(r).unwrap_or_default())
            .collect();
        out.sort();
        out
    };
    protected(cur) == protected(cand)
}

// ===========================================================================
// profile_history transition-time checks
// ===========================================================================
fn profile_history_superset(cur: &Value, cand: &Value) -> bool {
    let old = arr(field(cur, "profile_history"));
    let new = arr(field(cand, "profile_history"));
    old.iter().all(|entry| new.iter().any(|e| e == entry))
}

fn profile_history_low_floor_ok(cur: &Value, cand: &Value, floor: &str) -> bool {
    let floor_rank = profile_rank(floor);
    let low_multiset = |doc: &Value| {
        let mut map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for e in arr(field(doc, "profile_history")) {
            let e_floor = str_field(e, "floor");
            if profile_rank(&e_floor) < floor_rank {
                *map.entry(serde_json::to_string(e).unwrap_or_default())
                    .or_insert(0) += 1;
            }
        }
        map
    };
    let old = low_multiset(cur);
    let new = low_multiset(cand);
    let mut keys: Vec<&String> = old.keys().chain(new.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .all(|k| old.get(k).copied().unwrap_or(0) == new.get(k).copied().unwrap_or(0))
}

fn profile_escalation_entry_ok(
    cand: &Value,
    from: &str,
    to: &str,
    floor: &str,
    checkpoint: u64,
) -> bool {
    arr(field(cand, "profile_history")).iter().any(|e| {
        str_field(e, "from") == from
            && str_field(e, "to") == to
            && str_field(e, "floor") == floor
            && field(e, "checkpoint").and_then(|v| v.as_u64()) == Some(checkpoint)
            && matches!(field(e, "actor_role"), Some(Value::String(s)) if matches!(s.as_str(), "vadi" | "prativadi" | "human" | "team"))
            && nonblank(field(e, "reason"))
            && matches!(field(e, "evidence_refs"), Some(Value::Array(a)) if !a.is_empty())
    })
}

// ===========================================================================
// edge whitelist
// ===========================================================================
fn edge_whitelist(
    schema: &str,
    cur_mode: &str,
    new_profile: &str,
    cur_status: &str,
    new_status: &str,
    reason: &mut String,
) -> bool {
    let edge = format!("{cur_status}:{new_status}");
    let legal = match schema {
        "dvandva.baton.v1" => matches!(
            edge.as_str(),
            "spec_drafting:spec_review"
                | "spec_review:spec_revision"
                | "spec_review:implementing"
                | "spec_revision:spec_review"
                | "implementing:phase_review"
                | "phase_review:phase_fixing"
                | "phase_review:review_of_review"
                | "phase_review:implementing"
                | "phase_review:done"
                | "phase_fixing:phase_review"
                | "review_of_review:implementing"
                | "review_of_review:done"
                | "review_of_review:counter_review"
                | "counter_review:implementing"
                | "counter_review:done"
                | "counter_review:review_of_review"
        ),
        "dvandva.baton.v2" => match cur_mode {
            "development" => match new_profile {
                "fast" => matches!(
                    edge.as_str(),
                    "research_drafting:research_review"
                        | "research_review:research_revision"
                        | "research_revision:research_review"
                        | "research_review:implementing"
                        | "implementing:phase_review"
                        | "phase_review:phase_fixing"
                        | "phase_fixing:phase_review"
                        | "phase_review:termination_review"
                        | "termination_review:phase_fixing"
                        | "termination_review:done"
                ),
                "standard" => matches!(
                    edge.as_str(),
                    "research_drafting:research_review"
                        | "research_review:research_revision"
                        | "research_revision:research_review"
                        | "research_review:spec_drafting"
                        | "spec_drafting:spec_review"
                        | "spec_review:spec_revision"
                        | "spec_revision:spec_review"
                        | "spec_review:implementing"
                        | "implementing:phase_review"
                        | "phase_review:phase_fixing"
                        | "phase_review:implementing"
                        | "phase_review:spec_revision"
                        | "phase_fixing:phase_review"
                        | "phase_review:termination_review"
                        | "termination_review:phase_fixing"
                        | "termination_review:done"
                ),
                "full" => matches!(
                    edge.as_str(),
                    "research_drafting:research_review"
                        | "research_review:research_revision"
                        | "research_revision:research_review"
                        | "research_review:spec_drafting"
                        | "spec_drafting:spec_review"
                        | "spec_review:spec_revision"
                        | "spec_review:parallel_implementing"
                        | "spec_revision:spec_review"
                        | "parallel_implementing:test_creation"
                        | "test_creation:cross_review"
                        | "cross_review:cross_fixing"
                        | "cross_fixing:test_creation"
                        | "cross_review:deep_review"
                        | "deep_review:phase_fixing"
                        | "deep_review:review_of_review"
                        | "deep_review:deslop"
                        | "review_of_review:counter_review"
                        | "review_of_review:deslop"
                        | "counter_review:review_of_review"
                        | "counter_review:deslop"
                        | "phase_fixing:test_creation"
                        | "deslop:phase_fixing"
                        | "deslop:parallel_implementing"
                        | "deslop:spec_revision"
                        | "deslop:termination_review"
                        | "termination_review:phase_fixing"
                        | "termination_review:done"
                ),
                _ => false,
            },
            "research" => matches!(
                edge.as_str(),
                "research_drafting:research_review"
                    | "research_review:research_revision"
                    | "research_revision:research_review"
                    | "research_review:spec_drafting"
                    | "spec_drafting:spec_review"
                    | "spec_review:spec_revision"
                    | "spec_revision:spec_review"
                    | "research_review:termination_review"
                    | "spec_review:termination_review"
                    | "termination_review:phase_fixing"
                    | "phase_fixing:research_review"
                    | "termination_review:done"
            ),
            "review" => matches!(
                edge.as_str(),
                "research_drafting:research_review"
                    | "research_review:research_revision"
                    | "research_revision:research_review"
                    | "research_review:deep_review"
                    | "deep_review:deslop"
                    | "deslop:termination_review"
                    | "termination_review:phase_fixing"
                    | "phase_fixing:deep_review"
                    | "termination_review:done"
            ),
            _ => false,
        },
        _ => false,
    };
    if !legal {
        *reason = format!("no legal edge {cur_status}->{new_status}");
    }
    legal
}

// ===========================================================================
// post-legality edge gates
// ===========================================================================
fn parallel_work_split_ok(cand: &Value) -> bool {
    let root_phase = jq_render(field(cand, "phase"));
    let chunks: Vec<&Value> = arr(field(cand, "work_split"))
        .iter()
        .filter(|item| {
            jq_render(field(item, "phase")) == root_phase
                && {
                    let ct = str_field(item, "chunk_type");
                    let t = str_field(item, "type");
                    let kind = if !ct.is_empty() {
                        ct
                    } else if !t.is_empty() {
                        t
                    } else {
                        "implementation".to_string()
                    };
                    kind == "implementation"
                }
                && matches!(owner_role_or_owner(item).as_str(), "vadi" | "prativadi")
                && matches!(
                    str_field(item, "cross_review_by").as_str(),
                    "vadi" | "prativadi"
                )
                && str_field(item, "cross_review_by") != owner_role_or_owner(item)
                && matches!(field(item, "paths"), Some(Value::Array(a)) if !a.is_empty())
        })
        .collect();
    chunks.len() >= 5
        && chunks.iter().any(|c| owner_role_or_owner(c) == "vadi")
        && chunks.iter().any(|c| owner_role_or_owner(c) == "prativadi")
}

fn track_owner_role_or_role(t: &Value) -> String {
    let r = str_field(t, "owner_role");
    if !r.is_empty() {
        return r;
    }
    str_field(t, "role")
}

fn parallel_to_test_creation_ok(cand: &Value) -> bool {
    let root_phase = jq_render(field(cand, "phase"));
    let tracks: Vec<&Value> = arr(field(cand, "subagent_tracks"))
        .iter()
        .filter(|t| {
            jq_render(field(t, "phase")) == root_phase
                && str_field(t, "track") == "implementation-chunk"
                && str_field(t, "status") == "completed"
                && good_result(field(t, "result"))
                && matches!(track_owner_role_or_role(t).as_str(), "vadi" | "prativadi")
                && count_len(field(t, "outputs")) > 0
                && count_len(field(t, "evidence_refs")) > 0
        })
        .collect();
    tracks.len() >= 5
        && tracks.iter().any(|t| track_owner_role_or_role(t) == "vadi")
        && tracks
            .iter()
            .any(|t| track_owner_role_or_role(t) == "prativadi")
}

fn test_creation_to_cross_review_ok(cand: &Value) -> bool {
    arr(field(cand, "subagent_tracks")).iter().any(|t| {
        str_field(t, "phase") == "test_creation"
            && str_field(t, "track") == "test-creation"
            && str_field(t, "owner") == "dvandva-test-creator"
            && str_field(t, "status") == "completed"
            && good_result(field(t, "result"))
            && count_len(field(t, "outputs")) > 0
            && count_len(field(t, "evidence_refs")) > 0
    })
}

fn cross_review_to_cross_fixing_ok(cand: &Value, required: i64) -> bool {
    arr(field(cand, "subagent_tracks")).iter().any(|t| {
        str_field(t, "phase") == "cross_review"
            && str_field(t, "track") == "cross-review"
            && field(t, "review_checkpoint").and_then(json_int) == Some(required)
            && matches!(track_owner_role_or_role(t).as_str(), "vadi" | "prativadi")
            && str_field(t, "status") == "completed"
            && !good_result(field(t, "result"))
            && count_len(field(t, "outputs")) > 0
            && count_len(field(t, "evidence_refs")) > 0
    })
}

fn cross_review_to_deep_review_ok(cand: &Value, required: i64) -> bool {
    let done_cross = |role: &str| {
        arr(field(cand, "subagent_tracks")).iter().any(|t| {
            str_field(t, "phase") == "cross_review"
                && str_field(t, "track") == "cross-review"
                && field(t, "review_checkpoint").and_then(json_int) == Some(required)
                && track_owner_role_or_role(t) == role
                && str_field(t, "status") == "completed"
                && good_result(field(t, "result"))
                && count_len(field(t, "outputs")) > 0
                && count_len(field(t, "evidence_refs")) > 0
        })
    };
    done_cross("vadi") && done_cross("prativadi")
}

fn narrow_fixups_ok(cand: &Value) -> bool {
    match field(cand, "narrow_fixups") {
        Some(Value::Array(items)) if !items.is_empty() => items
            .iter()
            .all(|it| matches!(it, Value::String(s) if s.chars().any(|c| !c.is_whitespace()))),
        _ => false,
    }
}

fn deep_review_angles_ok(cand: &Value, required: i64) -> bool {
    let done_angle = |name: &str| {
        arr(field(cand, "subagent_tracks")).iter().any(|t| {
            str_field(t, "phase") == "deep_review"
                && str_field(t, "track") == name
                && field(t, "review_checkpoint").and_then(json_int) == Some(required)
                && str_field(t, "status") == "completed"
                && good_result(field(t, "result"))
                && count_len(field(t, "outputs")) > 0
                && count_len(field(t, "evidence_refs")) > 0
        })
    };
    done_angle("correctness-regression")
        && done_angle("test-evidence")
        && done_angle("protocol-handoff")
}

// ===========================================================================
// cycle checkpoint scanners (history/*.json + current)
// ===========================================================================
struct CycleRow {
    checkpoint: i64,
    status: String,
    phase: String,
}

fn collect_cycle_rows(
    baton_file: &Path,
    current_checkpoint: i64,
    want_phase: bool,
) -> Vec<CycleRow> {
    let mut rows = Vec::new();
    let history_dir = baton_file
        .parent()
        .map(|p| p.join("history"))
        .unwrap_or_else(|| PathBuf::from("history"));
    if let Ok(entries) = std::fs::read_dir(&history_dir) {
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().map(|e| e == "json").unwrap_or(false))
            .collect();
        files.sort();
        for file in files {
            if let Ok(v) = util::read_json_lenient(&file) {
                if let Some(row) = cycle_row(&v, want_phase) {
                    if row.checkpoint <= current_checkpoint {
                        rows.push(row);
                    }
                }
            }
        }
    }
    if let Ok(v) = util::read_json_lenient(baton_file) {
        if let Some(row) = cycle_row(&v, want_phase) {
            rows.push(row);
        }
    }
    rows
}

fn cycle_row(v: &Value, want_phase: bool) -> Option<CycleRow> {
    let checkpoint = match field(v, "checkpoint") {
        Some(Value::Number(n)) => n.as_i64()?,
        _ => return None,
    };
    let status = match field(v, "status") {
        Some(Value::String(s)) => s.clone(),
        _ => return None,
    };
    let phase = if want_phase {
        jq_render(field(v, "phase"))
    } else {
        String::new()
    };
    Some(CycleRow {
        checkpoint,
        status,
        phase,
    })
}

/// Find the checkpoint of the most recent `target` status row (scanning from the
/// highest checkpoint down, contiguous run), mirroring the shell awk logic.
fn cross_or_deep_cycle(baton_file: &Path, current_checkpoint: i64, target: &str) -> i64 {
    let mut rows = collect_cycle_rows(baton_file, current_checkpoint, false);
    if rows.is_empty() {
        return current_checkpoint;
    }
    rows.retain(|r| r.checkpoint <= current_checkpoint);
    rows.sort_by_key(|r| r.checkpoint);
    let mut cycle = current_checkpoint;
    let mut found = false;
    for row in rows.iter().rev() {
        if row.status == target {
            cycle = row.checkpoint;
            found = true;
        } else if found {
            break;
        }
    }
    cycle
}

fn cross_review_cycle_checkpoint(baton_file: &Path, current_checkpoint: i64) -> i64 {
    cross_or_deep_cycle(baton_file, current_checkpoint, "cross_review")
}

fn deep_review_cycle_checkpoint(baton_file: &Path, current_checkpoint: i64) -> i64 {
    cross_or_deep_cycle(baton_file, current_checkpoint, "deep_review")
}

fn phase_review_cycle_checkpoint(
    baton_file: &Path,
    current_checkpoint: i64,
    current_phase: &str,
) -> i64 {
    let mut rows = collect_cycle_rows(baton_file, current_checkpoint, true);
    if rows.is_empty() {
        return current_checkpoint;
    }
    rows.retain(|r| r.checkpoint <= current_checkpoint);
    rows.sort_by_key(|r| r.checkpoint);
    let mut cycle = current_checkpoint;
    for row in rows.iter().rev() {
        if row.status == "phase_review" && row.phase == current_phase {
            cycle = row.checkpoint;
            break;
        }
    }
    cycle
}
