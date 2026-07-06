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

use crate::gitcfg;
use crate::lock::{self, Acquire};
use crate::snapshot::snapshot_baton;
use crate::util;
use crate::workflow::validate_run_workflow;

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

    let cf = candidate_file.display().to_string();

    // ---- candidate-only validation (schema / keys / shape / owner) ---------
    let shape = match validate_candidate_shape(baton_file, &cf, baton_file.is_file(), &cand) {
        Ok(shape) => shape,
        Err((code, msg)) => {
            eprintln!("{msg}");
            return code;
        }
    };

    // ---- lock timeout + baton directory ------------------------------------
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
        return 2;
    }
    let lock_timeout: u64 = lock_timeout_raw.parse().unwrap_or(30);

    // ---- acquire lock ------------------------------------------------------
    let guard = match lock::acquire(&baton_dir, lock_timeout) {
        Acquire::Held(token) => LockGuard::held(baton_dir.clone(), token),
        Acquire::NoDir => LockGuard::unlocked(),
        Acquire::SquattedNonDir => {
            eprintln!(
                "DVANDVA_WRITE lock_unavailable path={} reason=non_directory_at_lock_path",
                baton_dir.join(lock::LOCK_DIR_NAME).display()
            );
            return 28;
        }
    };

    // ---- read the current baton (strict: unparseable-on-disk -> 25) --------
    let cur_doc = if baton_file.is_file() {
        match util::read_json_lenient(baton_file) {
            Ok(value) => Some(value),
            Err(_) => {
                eprintln!(
                    "DVANDVA_WRITE current_baton_unparseable file={} refusing_to_overwrite=true",
                    baton_file.display()
                );
                return 25;
            }
        }
    } else {
        None
    };

    // ---- transition legality (inside the lock) -----------------------------
    let cx = shape.ctx(&cf);
    if let Err((code, msg)) = decide_transition(&baton_dir, cur_doc.as_ref(), &cand, &cx) {
        eprintln!("{msg}");
        return code;
    }

    let plan = InstallPlan {
        status: shape.new_status.clone(),
        assignee: shape.new_assignee.clone(),
        phase: shape.new_phase.clone(),
        checkpoint: shape.new_checkpoint_str.clone(),
        lock: guard,
    };
    install_and_snapshot(baton_file, candidate_file, &cand, plan)
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
fn validate_candidate_shape(
    baton_file: &Path,
    cf: &str,
    baton_exists: bool,
    cand: &Value,
) -> Result<CandidateShape, (i32, String)> {
    // ---- schema ∈ {v1, v2, v3} --------------------------------------------
    let schema = str_field(cand, "schema");
    if schema != "dvandva.baton.v1" && schema != "dvandva.baton.v2" && schema != "dvandva.baton.v3"
    {
        return Err((
            23,
            format!("DVANDVA_WRITE schema_mismatch candidate={cf} want=dvandva.baton.v3"),
        ));
    }

    // ---- S5-T2 (D5): v1 is retired from the WRITE path ---------------------
    // A `dvandva.baton.v1` candidate can no longer be written (scaffold OR
    // transition); the migration hint points at v2. The READ path
    // (state/resolve/wait/brief) stays lenient, so old batons remain observable.
    if schema == "dvandva.baton.v1" {
        return Err((
            23,
            format!(
                "DVANDVA_WRITE schema_retired candidate={cf} schema=dvandva.baton.v1 hint=migrate to dvandva.baton.v2"
            ),
        ));
    }
    if schema == "dvandva.baton.v2" {
        return Err((
            23,
            format!(
                "DVANDVA_WRITE schema_retired candidate={cf} schema=dvandva.baton.v2 hint=migrate to dvandva.baton.v3"
            ),
        ));
    }
    // v3 is the v2 validation contract plus a required run_workflow field,
    // shape-checked below (`validate_run_workflow`) and used to resolve the
    // run's own transition graph (`resolve_effective_graph`).
    let is_v2 = schema == "dvandva.baton.v3";

    // ---- run-dir / run_id consistency (bad_run_id_dir) ---------------------
    if let Some(named) = named_run_dir_id(baton_file) {
        let cand_named = if matches!(field(cand, "run_id"), Some(Value::String(_))) {
            str_field(cand, "run_id")
        } else {
            String::new()
        };
        if !is_v2 || cand_named != named {
            return Err((
                23,
                format!(
                    "DVANDVA_WRITE bad_run_id_dir baton={} candidate_run_id={cand_named} expected_run_id={named} schema={schema}",
                    baton_file.display()
                ),
            ));
        }
    }

    // ---- required keys -----------------------------------------------------
    let mut required = required_keys(is_v2);
    if schema == "dvandva.baton.v3" {
        required.push("run_workflow");
    }
    for key in required {
        if field(cand, key).is_none() {
            return Err((
                23,
                format!("DVANDVA_WRITE missing_key key={key} candidate={cf}"),
            ));
        }
    }

    if schema == "dvandva.baton.v3" {
        let run_workflow = field(cand, "run_workflow").expect("required key checked above");
        validate_run_workflow(run_workflow, V3_STATUS_CATALOG).map_err(|err| {
            (
                23,
                format!("DVANDVA_WRITE bad_run_workflow candidate={cf} reason={err:?}"),
            )
        })?;
    }

    // ---- review_target enum ------------------------------------------------
    if !review_target_ok(cand) {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_review_target candidate={cf}"),
        ));
    }

    let new_status = str_field(cand, "status");
    let new_assignee = str_field(cand, "assignee");
    let new_mode = str_field(cand, "mode");

    let mut new_effective_mode = String::new();
    let mut new_effective_profile = String::new();
    let mut new_profile_floor = String::new();

    // ---- v2-compatible block ----------------------------------------------
    if is_v2 {
        new_effective_mode = match canonical_mode(&new_mode) {
            Some(mode) => mode,
            None => {
                return Err((
                    23,
                    format!("DVANDVA_WRITE bad_mode mode={new_mode} candidate={cf}"),
                ));
            }
        };

        let new_run_id = str_field(cand, "run_id");
        if !(matches!(field(cand, "run_id"), Some(Value::String(s)) if !s.is_empty())
            && util::is_safe_run_id(&new_run_id))
        {
            return Err((23, format!("DVANDVA_WRITE bad_run_id candidate={cf}")));
        }
        if !matches!(field(cand, "original_ask"), Some(Value::String(s)) if !s.is_empty()) {
            return Err((23, format!("DVANDVA_WRITE bad_original_ask candidate={cf}")));
        }
        // F7: amendment_from_phase is additive and nullable (number | null;
        // absent == null). Only its shape is checked here; transition legality is
        // enforced in decide_transition.
        if !amendment_from_phase_shape_ok(cand) {
            return Err((23, format!("DVANDVA_WRITE bad_amendment candidate={cf}")));
        }
        // F9: phase_profiles is additive/nullable — a stringified-numeric-keyed
        // object mapping to "standard"|"full". Shape only here; spec-state-only
        // mutation and the per-phase floor are enforced below / in decide.
        if !phase_profiles_shape_ok(cand) {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_phase_profiles candidate={cf}"),
            ));
        }

        if new_effective_mode == "development" {
            // profile field/shape validation
            if !profile_block_ok(cand) {
                return Err((23, format!("DVANDVA_WRITE bad_profile candidate={cf}")));
            }
            if !baton_exists
                && new_status != "human_decision"
                && !fresh_scaffold_profile_present(cand)
            {
                return Err((23, format!("DVANDVA_WRITE bad_profile candidate={cf}")));
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
                    return Err((23, format!("DVANDVA_WRITE bad_profile candidate={cf}")));
                }
            }
            // downgrade guard
            if profile_rank(&new_effective_profile) < profile_rank(&new_profile_floor)
                && new_status != "human_decision"
            {
                return Err((
                    23,
                    format!("DVANDVA_WRITE bad_profile_downgrade candidate={cf}"),
                ));
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
                    return Err((
                        23,
                        format!("DVANDVA_WRITE bad_profile_floor candidate={cf}"),
                    ));
                }
            }
            // fast-allowlist gate
            if new_effective_profile == "fast" && !fast_allowlist_ok(cand) {
                return Err((
                    23,
                    format!("DVANDVA_WRITE bad_profile_floor candidate={cf}"),
                ));
            }
            // F9 per-phase floor: a phase declared "standard" may not carry a
            // hard path in its own work_split chunks (message names phase + path).
            if let Some((phase_n, path)) = phase_profile_floor_violation(cand) {
                return Err((
                    23,
                    format!("DVANDVA_WRITE bad_phase_profiles phase={phase_n} path={path} candidate={cf}"),
                ));
            }
        }

        if !active_roles_shape_ok(cand) {
            return Err((23, format!("DVANDVA_WRITE bad_active_roles candidate={cf}")));
        }
        if !agent_instances_ok(cand) {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_agent_instances candidate={cf}"),
            ));
        }
        if !agent_instances_write_paths_ok(cand) {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_agent_instances_write_paths candidate={cf}"),
            ));
        }
        if !work_split_nonempty(cand) {
            return Err((23, format!("DVANDVA_WRITE bad_work_split candidate={cf}")));
        }
        if !work_split_paths_ok(cand) {
            return Err((23, format!("DVANDVA_WRITE bad_work_split candidate={cf}")));
        }
        if !depends_on_ok(cand) {
            return Err((23, format!("DVANDVA_WRITE bad_depends_on candidate={cf}")));
        }
        if !work_split_write_paths_ok(cand) {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_work_split_write_paths candidate={cf}"),
            ));
        }
        if !verification_matrix_nonempty(cand) {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_verification_matrix candidate={cf}"),
            ));
        }
        if !subagent_tracks_ok(cand) {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_subagent_tracks candidate={cf}"),
            ));
        }
        if !subagent_tracks_owner_ok(cand) {
            let msg = if subagent_tracks_have_dynamic_owner(cand) {
                format!("DVANDVA_WRITE bad_agent_instances candidate={cf}")
            } else {
                format!("DVANDVA_WRITE bad_subagent_tracks candidate={cf}")
            };
            return Err((23, msg));
        }
        if new_status != "research_drafting"
            && new_status != "clarifying_questions_drafting"
            && new_status != "clarifying_questions_answer"
            && new_status != "clarifying_questions_followup"
            && new_status != "clarifying_questions_followup_answer"
            && new_status != "human_question"
            && new_status != "human_decision"
            && new_status != "abandoned"
            && !matches!(field(cand, "research_ref"), Some(Value::String(s)) if !s.is_empty())
        {
            return Err((23, format!("DVANDVA_WRITE bad_research_ref candidate={cf}")));
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
                            return Err((
                                23,
                                format!("DVANDVA_WRITE bad_run_explainer_ref candidate={cf}"),
                            ));
                        }
                        if !run_explainer_reviews_ok(cand) {
                            return Err((
                                23,
                                format!("DVANDVA_WRITE bad_run_explainer_reviews candidate={cf}"),
                            ));
                        }
                    } else if !compact_terminal_evidence_ok(cand) {
                        return Err((
                            23,
                            format!("DVANDVA_WRITE bad_compact_terminal_evidence candidate={cf}"),
                        ));
                    }
                }
                "research" if !research_done_ref_ok(cand) => {
                    return Err((
                        23,
                        format!("DVANDVA_WRITE bad_research_done_ref candidate={cf}"),
                    ));
                }
                "review" if !review_ref_ok(cand) => {
                    return Err((23, format!("DVANDVA_WRITE bad_review_ref candidate={cf}")));
                }
                _ => {}
            }
        }
    }

    // ---- done universal approvals -----------------------------------------
    if new_status == "done" && !done_state_ok(cand) {
        return Err((23, format!("DVANDVA_WRITE bad_done_state candidate={cf}")));
    }

    // ---- checkpoint type ---------------------------------------------------
    if !matches!(field(cand, "checkpoint"), Some(Value::Number(_))) {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_checkpoint_type candidate={cf}"),
        ));
    }
    let new_checkpoint_num = match field(cand, "checkpoint").and_then(|v| v.as_u64()) {
        Some(n) => n,
        None => {
            // number but not a non-negative integer -> ^[0-9]+$ fails below
            return Err((
                23,
                format!(
                    "DVANDVA_WRITE bad_checkpoint checkpoint={} candidate={cf}",
                    jq_render(field(cand, "checkpoint"))
                ),
            ));
        }
    };
    let new_checkpoint = new_checkpoint_num.to_string();
    let new_phase = jq_render(field(cand, "phase"));
    let new_vadi_approval = bool_field(cand, "vadi_final_approval");
    let new_prativadi_approval = bool_field(cand, "prativadi_final_approval");

    // ---- status enum -------------------------------------------------------
    if !status_enum_ok(&new_status) {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_status status={new_status} candidate={cf}"),
        ));
    }

    // ---- v2 phase↔status pairing ------------------------------------------
    if is_v2 && !phase_status_ok(&new_effective_mode, &new_status, cand) {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_phase_status status={new_status} candidate={cf}"),
        ));
    }

    // ---- assignee nonempty -------------------------------------------------
    if new_assignee.is_empty() || new_assignee == "null" {
        return Err((23, format!("DVANDVA_WRITE bad_assignee candidate={cf}")));
    }

    // ---- v2 status-owner + team active_roles ------------------------------
    if is_v2 {
        let expected = v2_expected_assignee(&new_status);
        if !expected.is_empty() && new_assignee != expected {
            return Err((
                23,
                format!(
                    "DVANDVA_WRITE bad_assignee_owner status={new_status} want={expected} got={new_assignee} candidate={cf}"
                ),
            ));
        }
        if is_team_sync_status(&new_status) {
            if !(new_assignee == "team" && active_roles_sorted_both(cand)) {
                return Err((
                    23,
                    format!("DVANDVA_WRITE bad_active_roles status={new_status} candidate={cf}"),
                ));
            }
        } else if count_len(field(cand, "active_roles")) != 0 {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_active_roles status={new_status} candidate={cf}"),
            ));
        }
    }

    // ---- checkpoint format -------------------------------------------------
    // new_checkpoint is already ^[0-9]+$ by construction (u64). Keep the shape.

    // ---- candidate question/resume null flags ------------------------------
    let cand_q_null = is_null_field(cand, "question");
    let cand_ra_null = is_null_field(cand, "resume_assignee");
    let cand_rs_null = is_null_field(cand, "resume_status");

    Ok(CandidateShape {
        schema,
        is_v2,
        new_status,
        new_assignee,
        new_mode,
        new_effective_mode,
        new_effective_profile,
        new_profile_floor,
        new_checkpoint: new_checkpoint_num,
        new_checkpoint_str: new_checkpoint,
        new_phase,
        new_vadi_approval,
        new_prativadi_approval,
        cand_q_null,
        cand_ra_null,
        cand_rs_null,
    })
}

/// The candidate-only validation result: the fields needed to build [`Ctx`] and
/// the [`InstallPlan`], all derived without the current baton or the lock.
struct CandidateShape {
    schema: String,
    is_v2: bool,
    new_status: String,
    new_assignee: String,
    new_mode: String,
    new_effective_mode: String,
    new_effective_profile: String,
    new_profile_floor: String,
    new_checkpoint: u64,
    new_checkpoint_str: String,
    new_phase: String,
    new_vadi_approval: bool,
    new_prativadi_approval: bool,
    cand_q_null: bool,
    cand_ra_null: bool,
    cand_rs_null: bool,
}

impl CandidateShape {
    /// Borrow the shape fields into a [`Ctx`] for [`decide_transition`].
    fn ctx<'a>(&'a self, cf: &str) -> Ctx<'a> {
        Ctx {
            cf: cf.to_string(),
            schema: &self.schema,
            is_v2: self.is_v2,
            new_status: &self.new_status,
            new_assignee: &self.new_assignee,
            new_mode: &self.new_mode,
            new_effective_mode: &self.new_effective_mode,
            new_effective_profile: &self.new_effective_profile,
            new_profile_floor: &self.new_profile_floor,
            new_checkpoint: self.new_checkpoint,
            new_phase: &self.new_phase,
            new_vadi_approval: self.new_vadi_approval,
            new_prativadi_approval: self.new_prativadi_approval,
            cand_q_null: self.cand_q_null,
            cand_ra_null: self.cand_ra_null,
            cand_rs_null: self.cand_rs_null,
        }
    }
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

// ===========================================================================
// pub(crate) transition surface (consumed by `dvandva next`).
// ===========================================================================

/// How a transition moves the baton `phase` relative to the current phase.
// The surface below is consumed by the B1 `next` module.
// `allow(dead_code)` keeps `cargo build` (without the binary/tests that wire
// it up) warning-free.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhaseMove {
    /// Stays in the current numeric phase (intra-phase loops, human, done).
    Same,
    /// Enters a numeric phase from spec, or advances to the next numeric phase.
    Advance,
    /// Lands in a non-numeric planning phase (`spec`/`research`).
    Spec,
}

/// One legal next transition from a current baton, as the validator would judge
/// it. Gates that depend on candidate *content* (evidence tracks, narrow_fixups,
/// parallel work_split, done approvals, the per-phase floor, F6/F10 angles) are
/// NOT reflected — those options may still be rejected by [`validate_candidate`]
/// once the candidate is materialised. Loop caps and the amendment-loop shape
/// ARE reflected (they depend only on the current baton).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransitionOption {
    pub(crate) to_status: String,
    pub(crate) to_phase: PhaseMove,
    pub(crate) assignee: String,
    pub(crate) active_roles: Vec<String>,
    pub(crate) review_target: Option<String>,
    /// `(edge, next_count, cap)` for loop edges (and the amendment entry edge).
    pub(crate) loop_key: Option<(String, u64, u64)>,
    /// Set on the amendment entry edge to the current numeric phase.
    pub(crate) sets_amendment_from_phase: Option<u64>,
    /// True on the amendment exit edge while an amendment loop is active.
    pub(crate) clears_amendment: bool,
}

/// The expected `(assignee, active_roles)` for a status under the v2 owner table
/// (F8-aware: team states carry both roles). `mode`/`effective_profile` are part
/// of the contract for `next` but do not currently alter the mapping, because
/// the numeric-phase status vocabularies are profile-disjoint (a standard phase
/// uses `implementing`/`phase_review` scalar owners; a full phase uses the team
/// states) — the status alone determines the owner.
#[allow(dead_code)]
pub(crate) fn expected_owner(
    schema: &str,
    mode: &str,
    effective_profile: &str,
    status: &str,
) -> (String, Vec<String>) {
    let _ = (mode, effective_profile);
    let assignee = v2_expected_assignee(status).to_string();
    let active_roles = if matches!(schema, "dvandva.baton.v2" | "dvandva.baton.v3")
        && is_team_sync_status(status)
    {
        vec!["prativadi".to_string(), "vadi".to_string()]
    } else {
        Vec::new()
    };
    (assignee, active_roles)
}

/// The exact `phase` value the engine's [`phase_status_ok`] requires for a
/// `(mode, status)` pair, given the CURRENT baton phase. This is the single
/// engine-owned producer `dvandva next` consumes so a generated candidate's
/// phase can never desync from what the validator demands.
///
/// Derived from the SAME decision tree `phase_status_ok` validates (`mode` is
/// canonicalised internally, exactly as the shape validator feeds it the
/// effective mode):
/// * development/research planning statuses resolve to `"research"` / `"spec"`.
/// * EVERY status in `review` mode resolves to `"review"`.
/// * `human_question` / `human_decision`, numeric-phase statuses, and any
///   unrecognised pairing preserve `current_phase` — the engine accepts any
///   phase there, and the moves that reach them are either same-phase (preserve)
///   or a numeric advancement whose target number is the caller's `--phase N`,
///   which this producer does not synthesise.
///
/// S5-T5 research-mode terminals (`termination_review`/`phase_fixing`/`done`)
/// label by the run's seed path: the current phase is the proxy `next` has in
/// hand — a run that drafted a spec is already at a `"spec"`-labeled state, so
/// its terminals keep `"spec"`; an exploratory run is at `"research"` and its
/// terminals use `"research"`. This matches [`phase_status_ok`], whose canonical
/// seed markers (`research_outcome`/`plan_ref`) `next` copies into the candidate
/// verbatim and which agree with the current phase on every reachable run.
#[allow(dead_code)]
pub(crate) fn expected_phase_for(mode: &str, status: &str, current_phase: &Value) -> Value {
    let effective = canonical_mode(mode);
    match (effective.as_deref(), status) {
        (_, "human_question") | (_, "human_decision") | (_, "abandoned") => current_phase.clone(),
        (
            Some("development") | Some("research") | Some("review"),
            "clarifying_questions_drafting"
            | "clarifying_questions_answer"
            | "clarifying_questions_followup"
            | "clarifying_questions_followup_answer",
        ) => Value::from("clarifying"),
        (Some("development"), "research_drafting" | "research_review" | "research_revision") => {
            Value::from("research")
        }
        (Some("development"), "spec_drafting" | "spec_review" | "spec_revision") => {
            Value::from("spec")
        }
        (Some("development"), "workflow_declaring" | "workflow_review" | "workflow_revision") => {
            Value::from("spec")
        }
        (Some("research"), "research_drafting" | "research_review" | "research_revision") => {
            Value::from("research")
        }
        (Some("research"), "termination_review" | "phase_fixing" | "done") => {
            if current_phase == &Value::from("spec") {
                Value::from("spec")
            } else {
                Value::from("research")
            }
        }
        (Some("research"), _) => Value::from("spec"),
        (Some("review"), _) => Value::from("review"),
        _ => current_phase.clone(),
    }
}

/// Run the full write validation pipeline against an in-memory current baton,
/// WITHOUT acquiring the lock, installing, or writing a snapshot.
///
/// `baton_dir` is the run directory (its `history/` is scanned for cycle
/// checkpoints exactly as the binary does; `baton_dir/baton.json` is used only
/// for the run-dir/run_id naming check). `current` is `None` for a scaffold
/// (first) write. Returns `Ok(())` when the candidate would be accepted, or the
/// `(exit_code, diagnostic)` the binary would have emitted.
#[allow(dead_code)]
pub(crate) fn validate_candidate(
    baton_dir: &Path,
    current: Option<&Value>,
    candidate: &Value,
) -> Result<(), (i32, String)> {
    let baton_file = baton_dir.join("baton.json");
    let cf = "<candidate>";
    let shape = validate_candidate_shape(&baton_file, cf, current.is_some(), candidate)?;
    let cx = shape.ctx(cf);
    decide_transition(baton_dir, current, candidate, &cx)
}

/// The whitelist targets reachable from `cur_status` under `edge_profile`,
/// enumerated by probing [`edge_whitelist`] over the full status universe.
#[allow(dead_code)]
fn whitelist_targets(
    graph: &EffectiveGraph,
    schema: &str,
    cur_mode: &str,
    edge_profile: &str,
    cur_status: &str,
) -> Vec<String> {
    const V2: &[&str] = &[
        "clarifying_questions_drafting",
        "clarifying_questions_answer",
        "clarifying_questions_followup",
        "clarifying_questions_followup_answer",
        "research_drafting",
        "research_review",
        "research_revision",
        "spec_drafting",
        "spec_review",
        "spec_revision",
        "workflow_declaring",
        "workflow_review",
        "workflow_revision",
        "implementing",
        "parallel_implementing",
        "test_creation",
        "cross_review",
        "cross_fixing",
        "deep_review",
        "deslop",
        "termination_review",
        "phase_review",
        "phase_fixing",
        "review_of_review",
        "counter_review",
        "done",
    ];
    const V1: &[&str] = &[
        "spec_drafting",
        "spec_review",
        "spec_revision",
        "implementing",
        "phase_review",
        "phase_fixing",
        "review_of_review",
        "counter_review",
        "done",
    ];
    let universe = if matches!(schema, "dvandva.baton.v2" | "dvandva.baton.v3") {
        V2
    } else {
        V1
    };
    universe
        .iter()
        .filter(|new_status| {
            let mut sink = String::new();
            edge_whitelist(
                graph,
                cur_mode,
                edge_profile,
                cur_status,
                new_status,
                &mut sink,
            )
        })
        .map(|s| s.to_string())
        .collect()
}

/// The legal next transitions from `current`, derived from the same whitelists
/// and precedence the validator uses. See [`TransitionOption`] for what is and
/// is not reflected. Advancement/entry into a numeric phase is an
/// over-approximation under F9: for a numeric source the entry state is pinned to
/// the (single) next phase's effective profile, but at the `spec_review` boundary
/// an amendment exit can re-enter ANY phase in `[amendment_from_phase,
/// total_phases]`, whose effective profiles may span both `standard` and `full`.
/// Both entry states are then offered (`implementing` for a reachable standard
/// re-entry phase, `parallel_implementing` for a full one); the caller supplies
/// `--phase N` and the validator arbitrates which entry state that specific phase
/// actually demands.
#[allow(dead_code)]
pub(crate) fn legal_transitions(current: &Value) -> Vec<TransitionOption> {
    let schema = str_field(current, "schema");
    if schema != "dvandva.baton.v1" && schema != "dvandva.baton.v2" && schema != "dvandva.baton.v3"
    {
        return Vec::new();
    }
    let is_v2 = matches!(schema.as_str(), "dvandva.baton.v2" | "dvandva.baton.v3");
    // The run's declared graph (v3) or its `(mode, profile)` preset (v2) — the
    // single legality/loop-cap authority the LIST surface probes.
    let graph = resolve_effective_graph(current);
    let cur_status = str_field(current, "status");
    let cur_mode = str_field(current, "mode");
    let cur_eff_mode = canonical_mode(&cur_mode).unwrap_or_else(|| "development".to_string());
    let run_profile = if is_v2 && cur_eff_mode == "development" {
        if present(field(current, "profile")) {
            str_field(current, "profile")
        } else {
            "full".to_string()
        }
    } else {
        "full".to_string()
    };
    let cur_phase = jq_render(field(current, "phase"));
    let cur_phase_num = cur_phase.parse::<i64>().ok();
    let cur_amendment = amendment_value(current);
    let cur_locked = bool_field(current, "master_plan_locked");

    // Edge-selection profile mirrors decide_transition: numeric-source states use
    // the current phase's effective profile; everything else uses the run profile.
    // The spec_review boundary is handled specially below — its entry state is
    // keyed to the TARGET phase's profile, and an amendment exit can span both
    // profiles across the reachable re-entry range `[amendment_from_phase,
    // total_phases]`.
    let at_spec_entry = is_v2 && cur_eff_mode == "development" && cur_status == "spec_review";
    let edge_profile = if is_v2 && is_numeric_phase_status(&cur_status) {
        effective_phase_profile(current, cur_phase_num, &run_profile)
    } else {
        run_profile.clone()
    };
    // The distinct effective profiles reachable from the spec_review entry; drives
    // which entry states (implementing/parallel_implementing) are offered.
    let reentry_profiles = if at_spec_entry {
        spec_entry_reentry_profiles(current, cur_amendment, &run_profile)
    } else {
        Vec::new()
    };

    let mut out: Vec<TransitionOption> = Vec::new();

    // At the spec_review boundary the entry state is keyed to the TARGET phase's
    // profile, and the reachable re-entry phases can span both profiles, so union
    // both whitelists and let the per-entry-state reachability filter below decide
    // which entry states surface. Every other source uses its single edge profile.
    let mut targets: Vec<String> = if at_spec_entry {
        let mut t = whitelist_targets(&graph, &schema, &cur_eff_mode, "full", &cur_status);
        for s in whitelist_targets(&graph, &schema, &cur_eff_mode, "standard", &cur_status) {
            if !t.contains(&s) {
                t.push(s);
            }
        }
        t
    } else {
        whitelist_targets(&graph, &schema, &cur_eff_mode, &edge_profile, &cur_status)
    };
    let mut add_target = |status: &str| {
        if !targets.iter().any(|s| s == status) {
            targets.push(status.to_string());
        }
    };
    if is_v2 && cur_eff_mode == "development" {
        match cur_status.as_str() {
            "research_review" if rw_approved_by(current).is_none() => {
                add_target("workflow_declaring");
            }
            "workflow_declaring" => add_target("workflow_review"),
            "workflow_revision" => add_target("workflow_review"),
            "workflow_review" => {
                add_target("spec_drafting");
                add_target("workflow_revision");
            }
            _ => {}
        }
    }

    for new_status in targets {
        if new_status == cur_status {
            continue;
        }
        // Amendment entry edge (F7): sets amendment_from_phase, loop-capped.
        let is_enter = is_v2
            && cur_eff_mode == "development"
            && is_amendment_enter_edge(&edge_profile, &cur_status, &new_status);
        // Amendment exit edge (F7): at spec_review the exit flavour follows the
        // ENTRY STATE's own profile (implementing<=>standard,
        // parallel_implementing<=>full), so both surface correctly under the union
        // enumeration; is_amendment_exit_edge still gates cur_status==spec_review,
        // so this is inert for every numeric source.
        let exit_profile: &str = match new_status.as_str() {
            "implementing" => "standard",
            "parallel_implementing" => "full",
            _ => edge_profile.as_str(),
        };
        let is_exit = is_v2
            && cur_eff_mode == "development"
            && cur_amendment.is_some()
            && is_amendment_exit_edge(exit_profile, &cur_status, &new_status);

        let loop_edge = format!("{cur_status}:{new_status}");
        let (loop_key, drop) = if is_enter {
            let edge = format!("plan_amendment:{cur_phase}");
            build_loop_key(current, &edge)
        } else if let Some(loop_key) = loop_key_for_edge(&graph, &loop_edge) {
            build_loop_key(current, &loop_key)
        } else {
            (None, false)
        };
        if drop {
            continue; // loop cap reached (or cap unset) -> edge not legal
        }

        let to_phase = classify_phase_move(&cur_status, &new_status, is_enter);
        // F9: pin an advancement/entry to the TARGET phase's entry state
        // (implementing <=> standard, parallel_implementing <=> full).
        if is_v2
            && cur_eff_mode == "development"
            && to_phase == PhaseMove::Advance
            && matches!(
                new_status.as_str(),
                "implementing" | "parallel_implementing"
            )
        {
            if at_spec_entry {
                // Offer an entry state iff some reachable re-entry phase carries its
                // profile; when the range spans both profiles, both are legal and
                // the caller's `--phase N` selects which the validator accepts.
                let want_profile = if new_status == "parallel_implementing" {
                    "full"
                } else {
                    "standard"
                };
                if !reentry_profiles.iter().any(|p| p == want_profile) {
                    continue;
                }
            } else {
                // Numeric source: the single next phase (N+1) fixes the entry state.
                let target = cur_phase_num.map(|n| n + 1);
                let target_eff = effective_phase_profile(current, target, &run_profile);
                let want = if target_eff == "full" {
                    "parallel_implementing"
                } else {
                    "implementing"
                };
                if new_status.as_str() != want {
                    continue;
                }
            }
        }
        let (assignee, active_roles) = owner_for(&schema, &cur_mode, &edge_profile, &new_status);
        out.push(TransitionOption {
            to_status: new_status.clone(),
            to_phase,
            assignee,
            active_roles,
            review_target: review_target_for(&new_status),
            loop_key,
            sets_amendment_from_phase: if is_enter {
                cur_phase_num.map(|n| n as u64)
            } else {
                None
            },
            clears_amendment: is_exit,
        });
    }

    // Same-status team-sync checkpoint.
    if is_v2 && is_team_sync_status(&cur_status) {
        let (assignee, active_roles) = owner_for(&schema, &cur_mode, &edge_profile, &cur_status);
        out.push(TransitionOption {
            to_status: cur_status.clone(),
            to_phase: PhaseMove::Same,
            assignee,
            active_roles,
            review_target: None,
            loop_key: None,
            sets_amendment_from_phase: None,
            clears_amendment: false,
        });
    }

    // Universal escalation to human_decision (never from a terminal state).
    if cur_status != "done" && cur_status != "abandoned" {
        out.push(TransitionOption {
            to_status: "human_decision".to_string(),
            to_phase: PhaseMove::Same,
            assignee: "human".to_string(),
            active_roles: Vec::new(),
            review_target: None,
            loop_key: None,
            sets_amendment_from_phase: None,
            clears_amendment: false,
        });
    }

    // human_question (S4-T5/D1): enters pre-lock from a research/spec planning
    // state, AND — for a v2 run — from a working state regardless of lock. Mirrors
    // the widened entry set decide_transition accepts, so the LIST stays coherent.
    let planning_hq = !cur_locked
        && matches!(
            cur_status.as_str(),
            "spec_drafting"
                | "spec_review"
                | "spec_revision"
                | "research_drafting"
                | "research_review"
                | "research_revision"
        );
    let working_hq = is_v2
        && matches!(
            cur_status.as_str(),
            "implementing"
                | "parallel_implementing"
                | "test_creation"
                | "cross_fixing"
                | "phase_fixing"
        );
    if planning_hq || working_hq {
        out.push(TransitionOption {
            to_status: "human_question".to_string(),
            to_phase: PhaseMove::Same,
            assignee: "human".to_string(),
            active_roles: Vec::new(),
            review_target: None,
            loop_key: None,
            sets_amendment_from_phase: None,
            clears_amendment: false,
        });
    }

    // Human-question resume (F2): restore the recorded (resume_status,
    // resume_assignee) and clear the question/resume fields — exactly the edge
    // decide_transition accepts, so `next` can always generate it. Skipped for the
    // illegal direct-to-done resume and when the resume fields are absent.
    if cur_status == "human_question" {
        let resume_status = str_field(current, "resume_status");
        let resume_assignee = str_field(current, "resume_assignee");
        if !resume_status.is_empty() && resume_status != "done" && !resume_assignee.is_empty() {
            out.push(TransitionOption {
                to_status: resume_status.clone(),
                to_phase: classify_phase_move(&cur_status, &resume_status, false),
                assignee: resume_assignee,
                active_roles: Vec::new(),
                review_target: review_target_for(&resume_status),
                loop_key: None,
                sets_amendment_from_phase: None,
                clears_amendment: false,
            });
        }
    }

    // Human-decision resume (F2): the engine authorises ANY non-terminal resume
    // from human_decision, which `next` cannot scaffold mechanically. Surface a
    // `human_resume` marker so LIST is honest that the edge exists; `next`'s
    // generate rejects `--to human_resume` with hand-authoring guidance.
    if cur_status == "human_decision" {
        out.push(TransitionOption {
            to_status: "human_resume".to_string(),
            to_phase: PhaseMove::Same,
            assignee: "human".to_string(),
            active_roles: Vec::new(),
            review_target: None,
            loop_key: None,
            sets_amendment_from_phase: None,
            clears_amendment: false,
        });
    }

    // S2-T1: from either human state the human may declare the run dead. The
    // abandoned terminal is human-owned, phase-preserving, and carries no roles.
    if cur_status == "human_question" || cur_status == "human_decision" {
        out.push(TransitionOption {
            to_status: "abandoned".to_string(),
            to_phase: PhaseMove::Same,
            assignee: "human".to_string(),
            active_roles: Vec::new(),
            review_target: None,
            loop_key: None,
            sets_amendment_from_phase: None,
            clears_amendment: false,
        });
    }

    out
}

/// `(loop_key, drop)` for an edge: `drop` when the cap is unset/zero or already
/// reached (the loop edge is not legal); otherwise the next-count/cap triple.
#[allow(dead_code)]
fn build_loop_key(current: &Value, edge: &str) -> (Option<(String, u64, u64)>, bool) {
    let cur_count = loop_count(current, edge).unwrap_or(0);
    let cap = loop_cap(current).unwrap_or(0);
    if cap == 0 || cur_count >= cap {
        (None, true)
    } else {
        (Some((edge.to_string(), cur_count + 1, cap)), false)
    }
}

#[allow(dead_code)]
fn is_loop_edge(edge: &str) -> bool {
    matches!(
        edge,
        "deep_review:phase_fixing"
            | "cross_review:cross_fixing"
            | "termination_review:phase_fixing"
            | "phase_review:phase_fixing"
            | "review_of_review:counter_review"
            | "counter_review:review_of_review"
    )
}

#[allow(dead_code)]
fn owner_for(
    schema: &str,
    mode: &str,
    effective_profile: &str,
    status: &str,
) -> (String, Vec<String>) {
    if status == "done" {
        return ("team".to_string(), Vec::new());
    }
    expected_owner(schema, mode, effective_profile, status)
}

#[allow(dead_code)]
fn classify_phase_move(cur_status: &str, new_status: &str, is_enter: bool) -> PhaseMove {
    if is_enter {
        return PhaseMove::Spec; // amendment entry lands in the spec phase
    }
    match new_status {
        "spec_drafting" | "spec_review" | "spec_revision" | "research_drafting"
        | "research_review" | "research_revision" | "workflow_declaring" | "workflow_review"
        | "workflow_revision" => PhaseMove::Spec,
        "implementing" | "parallel_implementing" => {
            // Entry from spec, or advancement from a prior phase's exit state.
            if matches!(cur_status, "spec_review" | "deslop" | "phase_review") {
                PhaseMove::Advance
            } else {
                PhaseMove::Same
            }
        }
        _ => PhaseMove::Same,
    }
}

#[allow(dead_code)]
fn review_target_for(status: &str) -> Option<String> {
    match status {
        "research_review" => Some("research"),
        "spec_review" => Some("spec"),
        "cross_review" | "deep_review" | "phase_review" => Some("implementation"),
        "review_of_review" => Some("prativadi_fixups"),
        "counter_review" => Some("vadi_counter"),
        _ => None,
    }
    .map(String::from)
}

// ===========================================================================
// F9 per-phase ceremony helpers.
// ===========================================================================

/// `phase_profiles`: absent/null OK; else an object with stringified-numeric
/// keys mapping to `"standard"`|`"full"` (fast is run-level only).
fn phase_profiles_shape_ok(cand: &Value) -> bool {
    match field(cand, "phase_profiles") {
        None | Some(Value::Null) => true,
        Some(Value::Object(m)) => m.iter().all(|(k, v)| {
            !k.is_empty()
                && k.chars().all(|c| c.is_ascii_digit())
                && matches!(v, Value::String(s) if s == "standard" || s == "full")
        }),
        _ => false,
    }
}

/// The effective profile override declared for numeric `phase_num`, if any.
fn phase_profile_override(doc: &Value, phase_num: Option<i64>) -> Option<String> {
    let n = phase_num?;
    let m = match field(doc, "phase_profiles") {
        Some(Value::Object(m)) => m,
        _ => return None,
    };
    match m.get(&n.to_string()) {
        Some(Value::String(s)) if s == "standard" || s == "full" => Some(s.clone()),
        _ => None,
    }
}

/// Effective profile of numeric phase `phase_num` = `phase_profiles[N]` // fallback.
fn effective_phase_profile(doc: &Value, phase_num: Option<i64>, fallback: &str) -> String {
    phase_profile_override(doc, phase_num).unwrap_or_else(|| fallback.to_string())
}

/// The distinct effective profiles across every phase a `spec_review` entry can
/// re-enter: during an active amendment the re-entry range is
/// `[amendment_from_phase, total_phases]`; otherwise the initial entry lands in
/// phase 1. Because a re-profiled phase in that range may differ from the
/// amendment's from-phase, the set can span BOTH `standard` and `full`, in which
/// case both entry states are legal from `spec_review`.
#[allow(dead_code)]
fn spec_entry_reentry_profiles(
    current: &Value,
    cur_amendment: Option<i64>,
    run_profile: &str,
) -> Vec<String> {
    let phases: Vec<i64> = match cur_amendment {
        Some(from) => {
            let total = total_phases_num(current).unwrap_or(from).max(from);
            (from..=total).collect()
        }
        None => vec![1],
    };
    let mut profiles: Vec<String> = phases
        .iter()
        .map(|p| effective_phase_profile(current, Some(*p), run_profile))
        .collect();
    profiles.sort();
    profiles.dedup();
    profiles
}

/// The work_split chunk paths (paths/read_paths/write_paths) declared for the
/// phase whose rendered number equals `phase_key`.
fn phase_work_split_paths(cand: &Value, phase_key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for item in iter_values(field(cand, "work_split")) {
        if jq_render(field(item, "phase")) != phase_key {
            continue;
        }
        for key in ["paths", "read_paths", "write_paths"] {
            if let Some(Value::Array(items)) = field(item, key) {
                for it in items {
                    if let Value::String(s) = it {
                        out.push(s.clone());
                    }
                }
            }
        }
    }
    out
}

/// F9 per-phase floor: the first `(phase, path)` where a phase declared
/// `"standard"` carries a hard path in its own work_split chunks. `None` = no
/// violation (a phase with no chunks yet imposes no constraint).
fn phase_profile_floor_violation(cand: &Value) -> Option<(String, String)> {
    let m = match field(cand, "phase_profiles") {
        Some(Value::Object(m)) => m,
        _ => return None,
    };
    for (phase_key, val) in m {
        if val.as_str() != Some("standard") {
            continue;
        }
        for p in phase_work_split_paths(cand, phase_key) {
            if hard_path(&p) {
                return Some((phase_key.clone(), p));
            }
        }
    }
    None
}

/// The v2 numeric-phase status set (edges select by the phase's effective
/// profile; spec/research states select by the run profile).
fn is_numeric_phase_status(status: &str) -> bool {
    matches!(
        status,
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
            | "done"
    )
}

// ===========================================================================
// F6 risk-triggered deep-review angle helpers.
// ===========================================================================

fn is_role_token(s: &str) -> bool {
    s == "vadi" || s == "prativadi"
}

/// F6 SECURITY trigger: changed_paths ∪ current-phase work_split paths/write_paths
/// match the hard-path security submatchers.
fn security_trigger_present(cand: &Value, phase_key: &str) -> bool {
    if arr(field(cand, "changed_paths"))
        .iter()
        .any(|v| matches!(v, Value::String(s) if is_security_path(s)))
    {
        return true;
    }
    iter_values(field(cand, "work_split"))
        .into_iter()
        .any(|item| {
            jq_render(field(item, "phase")) == phase_key
                && ["paths", "write_paths"].iter().any(|key| {
                    arr(field(item, key))
                        .iter()
                        .any(|v| matches!(v, Value::String(s) if is_security_path(s)))
                })
        })
}

/// F6 INTEGRATION trigger: the current phase has ≥2 write-capable chunks with
/// different owner_role AND a real seam (cross-owner depends_on or shared
/// conflict_group).
fn integration_trigger_present(cand: &Value, phase_key: &str) -> bool {
    let root_status = str_field(cand, "status");
    let chunks: Vec<&Value> = iter_values(field(cand, "work_split"))
        .into_iter()
        .filter(|item| {
            jq_render(field(item, "phase")) == phase_key
                && !effective_write_paths(item, &root_status).is_empty()
        })
        .collect();
    if chunks.len() < 2 {
        return false;
    }
    let distinct_roles: std::collections::HashSet<String> =
        chunks.iter().map(|c| owner_role_or_owner(c)).collect();
    if distinct_roles.len() < 2 {
        return false;
    }
    for a in &chunks {
        for b in &chunks {
            if owner_role_or_owner(a) == owner_role_or_owner(b) {
                continue;
            }
            let b_id = str_field(b, "id");
            let dep = !b_id.is_empty() && string_array(field(a, "depends_on")).contains(&b_id);
            let cga = str_field(a, "conflict_group");
            let cgb = str_field(b, "conflict_group");
            let shared_cg = !cga.is_empty() && cga == cgb;
            if dep || shared_cg {
                return true;
            }
        }
    }
    false
}

/// A completed current-cycle risk-review track whose name/track contains
/// `name_needle`, owned by `specific_owner` or a role, carrying evidence.
fn risk_angle_present(
    cand: &Value,
    required: i64,
    name_needle: &str,
    specific_owner: &str,
) -> bool {
    arr(field(cand, "subagent_tracks")).iter().any(|t| {
        let name_ok = str_field(t, "track").contains(name_needle)
            || str_field(t, "name").contains(name_needle);
        let owner = str_field(t, "owner");
        let owner_ok = owner == specific_owner
            || is_role_token(&owner)
            || is_role_token(&track_owner_role_or_role(t));
        let cycle_ok = field(t, "review_checkpoint").and_then(json_int) == Some(required);
        let evidence_ok =
            count_len(field(t, "outputs")) > 0 || count_len(field(t, "evidence_refs")) > 0;
        name_ok && owner_ok && str_field(t, "status") == "completed" && cycle_ok && evidence_ok
    })
}

// ===========================================================================
// F10 explainer-verification gate helper.
// ===========================================================================

/// A completed current-cycle `explainer-verification` track owned by
/// `dvandva-doc-verifier`, approved/passed, with outputs AND evidence_refs.
fn explainer_verification_ok(cand: &Value, required: i64) -> bool {
    arr(field(cand, "subagent_tracks")).iter().any(|t| {
        let name_ok = str_field(t, "track").contains("explainer-verification")
            || str_field(t, "name").contains("explainer-verification");
        // Current-cycle only: the track must be stamped with the checkpoint at
        // which the run entered its CURRENT termination_review block. A track's
        // phase == "termination_review" is NOT sufficient — a superseded cycle's
        // doc-verifier track carries that phase but an older review_checkpoint,
        // which would bypass the done gate. Key on == required, mirroring the
        // F6/base deep-review angle gates.
        let cycle_ok = field(t, "review_checkpoint").and_then(json_int) == Some(required);
        name_ok
            && str_field(t, "owner") == "dvandva-doc-verifier"
            && str_field(t, "status") == "completed"
            && good_result(field(t, "result"))
            && count_len(field(t, "outputs")) > 0
            && count_len(field(t, "evidence_refs")) > 0
            && cycle_ok
    })
}

// ===========================================================================
// Stage 2: transition decision (inside the lock).
// ===========================================================================
fn decide_transition(
    baton_dir: &Path,
    cur_doc_opt: Option<&Value>,
    cand: &Value,
    cx: &Ctx,
) -> Result<(), (i32, String)> {
    let cf = &cx.cf;

    let cur_doc = match cur_doc_opt {
        None => {
            // Scaffold: only the vadi may create the very first baton. v1/v2
            // candidates never reach here (rejected upstream with
            // `schema_retired`), so only the v3 seed is legal.
            let legal = cx.schema == "dvandva.baton.v3"
                && cx.new_status == "clarifying_questions_drafting"
                && cx.new_assignee == "vadi"
                && cx.new_checkpoint == 0;
            if !legal {
                return Err((24, format!(
                    "DVANDVA_WRITE illegal_transition scaffold requires v3 status=clarifying_questions_drafting with assignee=vadi checkpoint=0, got schema={} status={} assignee={} checkpoint={}",
                    cx.schema, cx.new_status, cx.new_assignee, cx.new_checkpoint
                )));
            }
            return Ok(());
        }
        Some(cur_doc) => cur_doc,
    };

    // ---- STRICT checks on the current baton (any anomaly -> 25) -------------
    let bf = baton_dir.join("baton.json");
    let bf = bf.display();

    if !matches!(field(cur_doc, "checkpoint"), Some(Value::Number(_))) {
        return Err((
            25,
            format!("DVANDVA_WRITE current_baton_unparseable file={bf} bad_checkpoint_type=true"),
        ));
    }

    let cur_schema = str_field(cur_doc, "schema");
    let cur_status = str_field(cur_doc, "status");
    let cur_checkpoint_i64 = match field(cur_doc, "checkpoint").and_then(json_int) {
        Some(n) => n,
        None => {
            return Err((
                25,
                format!(
                    "DVANDVA_WRITE current_baton_unparseable file={bf} bad_checkpoint={}",
                    jq_render(field(cur_doc, "checkpoint"))
                ),
            ));
        }
    };
    let cur_locked = bool_field(cur_doc, "master_plan_locked");
    let cur_resume_assignee = str_field(cur_doc, "resume_assignee");
    let cur_resume_status = str_field(cur_doc, "resume_status");
    let cur_run_id = str_field(cur_doc, "run_id");
    let cur_phase = jq_render(field(cur_doc, "phase"));
    let cur_vadi_approval = bool_field(cur_doc, "vadi_final_approval");
    let cur_prativadi_approval = bool_field(cur_doc, "prativadi_final_approval");
    let cur_mode = str_field(cur_doc, "mode");

    // S5-T2 (D5): a v1 CURRENT baton has no legal write forward — the whole
    // engine graph is v2. Reject with the same `schema_retired` migration hint a
    // v1 candidate gets, rather than the generic unparseable code, so the caller
    // is told to migrate. The READ path still surfaces v1 batons untouched.
    if cur_schema == "dvandva.baton.v1" {
        return Err((
            23,
            format!(
                "DVANDVA_WRITE schema_retired file={bf} schema=dvandva.baton.v1 hint=migrate to dvandva.baton.v2"
            ),
        ));
    }
    if cur_schema == "dvandva.baton.v2" {
        return Err((
            23,
            format!(
                "DVANDVA_WRITE schema_retired file={bf} schema=dvandva.baton.v2 hint=migrate to dvandva.baton.v3"
            ),
        ));
    }
    if cur_schema != "dvandva.baton.v3" {
        return Err((
            25,
            format!("DVANDVA_WRITE current_baton_unparseable file={bf} bad_schema={cur_schema}"),
        ));
    }
    if cur_schema == "dvandva.baton.v3" && !util::is_safe_run_id(&cur_run_id) {
        return Err((
            25,
            format!("DVANDVA_WRITE current_baton_unparseable file={bf} bad_run_id={cur_run_id}"),
        ));
    }

    // The transition-legality authority: the current run's own declared graph
    // (a v3 `run_workflow`) or, for a preset-source/v2 run, the `(mode, profile)`
    // preset. Every edge/loop-cap decision below reads from this one resolution.
    let eff_graph = resolve_effective_graph(cur_doc);

    let mut cur_effective_mode = String::new();
    let mut cur_effective_profile = String::new();
    let mut cur_profile_floor = String::new();
    if cur_schema == "dvandva.baton.v3" {
        cur_effective_mode = match canonical_mode(&cur_mode) {
            Some(mode) => mode,
            None => {
                return Err((
                    25,
                    format!(
                        "DVANDVA_WRITE current_baton_unparseable file={bf} bad_mode={cur_mode}"
                    ),
                ));
            }
        };
        if cur_effective_mode == "development" {
            cur_effective_profile = if present(field(cur_doc, "profile")) {
                str_field(cur_doc, "profile")
            } else {
                "full".to_string()
            };
            cur_profile_floor = if present(field(cur_doc, "profile_floor")) {
                str_field(cur_doc, "profile_floor")
            } else {
                cur_effective_profile.clone()
            };
        }
    }

    // ---- F7 amendment state + F9 per-phase effective profiles --------------
    let cur_amendment = amendment_value(cur_doc);
    let cand_amendment = amendment_value(cand);
    let cur_phase_num: Option<i64> = cur_phase.parse::<i64>().ok();
    let new_phase_num: Option<i64> = cx.new_phase.parse::<i64>().ok();
    // F9: the current phase's effective profile drives edge selection for
    // numeric-source states and the amendment-entry flavour; the target phase's
    // effective profile drives spec→implementation entry and the amendment exit.
    // Fallback = the run profile (== today's behaviour when phase_profiles is
    // absent, keeping the whole existing suite byte-identical).
    let cur_phase_eff = effective_phase_profile(cur_doc, cur_phase_num, cx.new_effective_profile);
    let new_phase_eff = effective_phase_profile(cand, new_phase_num, cx.new_effective_profile);
    // F9: phase_profiles may change only in a spec-state write (incl. the F7
    // amendment loop). human_decision is a human-authority wildcard.
    let cur_pp = field(cur_doc, "phase_profiles").filter(|v| !v.is_null());
    let cand_pp = field(cand, "phase_profiles").filter(|v| !v.is_null());
    if cx.is_v2
        && cur_effective_mode == "development"
        && cx.new_effective_mode == "development"
        && cx.new_status != "human_decision"
        && !matches!(
            cx.new_status,
            "spec_drafting" | "spec_review" | "spec_revision"
        )
        && cur_pp != cand_pp
    {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_phase_profiles candidate={cf}"),
        ));
    }

    // ---- S4-T2 (D2): master_plan_locked is a one-way latch -----------------
    // Once the current baton has the plan locked, no write may clear it — except
    // a human_decision, the human's authority to re-open the plan. Amendment
    // loops keep locked=true (F7), so they never trip this.
    if cx.is_v2
        && cur_effective_mode == "development"
        && cx.new_effective_mode == "development"
        && cur_locked
        && cx.new_status != "human_decision"
        && !bool_field(cand, "master_plan_locked")
    {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_master_plan_locked unlock_forbidden candidate={cf}"),
        ));
    }

    // ---- Fix 3: numeric phase never exceeds total_phases -------------------
    // A numeric phase is 1-indexed within [1, total_phases]. The F7 amendment
    // loop is the one path that can LOWER total_phases, so an exit could otherwise
    // re-enter a phase above the (now smaller) ceiling ("phase 2 of 1"). Guarded
    // for every numeric development candidate; total_phases == 0 is the pre-lock
    // "no ceiling yet" sentinel and imposes no bound, and human_decision is a
    // human-authority wildcard.
    if cx.is_v2 && cx.new_effective_mode == "development" && cx.new_status != "human_decision" {
        if let (Some(p), Some(t)) = (new_phase_num, total_phases_num(cand)) {
            if t >= 1 && p > t {
                return Err((
                    23,
                    format!(
                        "DVANDVA_WRITE bad_amendment phase_exceeds_total_phases phase={p} total_phases={t} candidate={cf}"
                    ),
                ));
            }
        }
    }

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
        // Loop-cap membership follows the resolved graph: a custom v3 graph
        // consults its declared edge's loop_cap_key/amendment_capped; every
        // preset/v2 graph falls back to the static six-edge set.
        let loop_key = loop_key_for_edge(&eff_graph, &edge);
        let amendment_enter = cur_effective_mode == "development"
            && cx.new_effective_mode == "development"
            && is_amendment_enter_edge(&cur_phase_eff, &cur_status, cx.new_status);
        let workflow_revision_reject = cur_effective_mode == "development"
            && cx.new_effective_mode == "development"
            && cur_status == "workflow_review"
            && cx.new_status == "workflow_revision";
        if amendment_enter {
            // F7: the amendment entry edge is loop-capped on
            // plan_amendment:<from-phase> (from-phase = current numeric phase),
            // and is exempt from the phase-advance loop-reset (spec is not a
            // numeric phase advance).
            let amendment_edge = format!("plan_amendment:{cur_phase}");
            loop_reason = loop_edge_reason(cur_doc, cand, &amendment_edge);
        } else if workflow_revision_reject {
            // P2: the per-run-workflow declaration reject loops under the single
            // "workflow_revision" key against disagreement_cap (per-episode);
            // cap exhaustion leaves the universal human_decision escalation.
            loop_reason = loop_edge_reason(cur_doc, cand, "workflow_revision");
        } else if cx.new_phase != cur_phase
            && loop_counts_nonempty(cand)
            && !research_planning_relabel(&cur_effective_mode, &cur_phase, cx.new_phase)
        {
            loop_reason = format!(
                "bad_loop_counts phase_advanced current={cur_phase} candidate={} must_reset=true",
                cx.new_phase
            );
        } else if let Some(loop_key) = loop_key {
            loop_reason = loop_edge_reason(cur_doc, cand, &loop_key);
        }
    }

    // ---- review ownership reason -------------------------------------------
    let mut review_ownership_reason = String::new();
    if cx.is_v2 && !run_explainer_reviews_ownership_ok(cur_doc, cand, &writer_role) {
        review_ownership_reason = "run explainer review ownership requires DVANDVA_ROLE=vadi/prativadi and only that role may change its own run_explainer_reviews entries".to_string();
    }

    // ---- S4-T4 lost_update reason (team-owned current status) --------------
    // A retry that dropped peer data must fail even when the edge is otherwise
    // legal. Escalations are exempt — an escalation must not be blocked by array
    // bookkeeping.
    let mut lost_update_reason = String::new();
    if cx.is_v2
        && is_team_sync_status(&cur_status)
        && !matches!(
            cx.new_status,
            "human_decision" | "human_question" | "abandoned"
        )
    {
        lost_update_reason = lost_update_violation(cur_doc, cand);
    }

    // ---- compact done phase-review checkpoint gate (baton exists) ----------
    if cx.is_v2
        && cx.new_effective_mode == "development"
        && cx.new_status == "done"
        && cx.new_effective_profile != "full"
    {
        let required =
            phase_review_cycle_checkpoint(baton_dir, cur_doc, cur_checkpoint_i64, &cur_phase);
        if !compact_done_phase_review_checkpoint_ok(cand, required) {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_compact_terminal_evidence candidate={cf}"),
            ));
        }
    }

    // ---- profile_history superset (append-only) ----------------------------
    if cx.is_v2
        && cur_effective_mode == "development"
        && cx.new_effective_mode == "development"
        && !profile_history_superset(cur_doc, cand)
    {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_profile_history candidate={cf}"),
        ));
    }

    // ---- profile_history low-floor guard -----------------------------------
    if cx.is_v2
        && cur_effective_mode == "development"
        && cx.new_effective_mode == "development"
        && cx.new_status != "human_decision"
        && !profile_history_low_floor_ok(cur_doc, cand, &cur_profile_floor)
    {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_profile_downgrade candidate={cf}"),
        ));
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
        return Err((
            23,
            format!("DVANDVA_WRITE bad_profile_history candidate={cf}"),
        ));
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
        return Err((
            23,
            format!("DVANDVA_WRITE bad_profile_downgrade candidate={cf}"),
        ));
    } else if (cx.new_checkpoint as i64) <= cur_checkpoint_i64 {
        return Err((
            27,
            format!(
                "DVANDVA_WRITE stale_checkpoint current={cur_checkpoint_i64} candidate={}",
                cx.new_checkpoint
            ),
        ));
    } else if cx.new_checkpoint as i64 != cur_checkpoint_i64 + 1 {
        reason = format!(
            "checkpoint must be {}, got {}",
            cur_checkpoint_i64 + 1,
            cx.new_checkpoint
        );
    } else if !lost_update_reason.is_empty() {
        // S4-T4: positioned after the checkpoint gates, before the whitelist —
        // a legal edge cannot rescue a candidate that lost installed peer data.
        return Err((23, format!("DVANDVA_WRITE {lost_update_reason}")));
    } else if approval_reason.starts_with("approval_out_of_band")
        || approval_reason.starts_with("stale_approval")
    {
        return Err((23, format!("DVANDVA_WRITE {approval_reason}")));
    } else if !approval_reason.is_empty() {
        reason = approval_reason.clone();
    } else if !loop_reason.is_empty() {
        return Err((23, format!("DVANDVA_WRITE {loop_reason}")));
    } else if !review_ownership_reason.is_empty()
        && (cx.new_status != "done"
            || (cur_status == "termination_review" && cur_vadi_approval && cur_prativadi_approval))
    {
        reason = review_ownership_reason.clone();
    } else if cx.new_status == cur_status {
        if cx.is_v2 {
            if is_team_sync_status(cx.new_status) {
                if cx.new_phase != cur_phase
                    && !research_planning_relabel(&cur_effective_mode, &cur_phase, cx.new_phase)
                {
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
        if cx.new_status == "human_decision" || cx.new_status == "abandoned" {
            // Escalate to human_decision, or the human declares the run dead
            // (S2-T1 abandoned).
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
        // S4-T5 (D1): human_question enters pre-lock from the research/spec
        // planning states, AND — for v2 development runs — from the working states
        // regardless of lock. Entering it is a stop-together pause, never a loop
        // edge; the resume machinery restores the exact prior state.
        let planning_entry = matches!(
            cur_status.as_str(),
            "spec_drafting"
                | "spec_review"
                | "spec_revision"
                | "research_drafting"
                | "research_review"
                | "research_revision"
        );
        let working_entry = cx.is_v2
            && matches!(
                cur_status.as_str(),
                "implementing"
                    | "parallel_implementing"
                    | "test_creation"
                    | "cross_fixing"
                    | "phase_fixing"
            );
        if planning_entry && cur_locked && !working_entry {
            reason = "human_question from a research/spec planning state is only legal before master_plan_locked; post-lock, enter it from a working state (implementing, parallel_implementing, test_creation, cross_fixing, phase_fixing)".to_string();
        } else if !planning_entry && !working_entry {
            reason = format!(
                "human_question enters from a research/spec planning state (pre-lock) or a working state (implementing, parallel_implementing, test_creation, cross_fixing, phase_fixing), not {cur_status}"
            );
        } else if cx.cand_q_null || cx.cand_ra_null || cx.cand_rs_null {
            reason = "human_question requires non-null question, resume_assignee, resume_status"
                .to_string();
        } else if str_field(cand, "resume_status") == "done" {
            reason = "human_question cannot resume directly to done".to_string();
        } else {
            legal = true;
        }
    } else if dev_dev && cur_status == "research_review" && cx.new_status == "workflow_declaring" {
        // P2 declaration loop entry: the vadi opens the run-workflow declaration
        // after research approval, but only while the workflow is unapproved (an
        // already-approved run has nothing to declare).
        if rw_approved_by(cur_doc).is_some() {
            reason = "research_review->workflow_declaring is only legal while run_workflow is unapproved".to_string();
        } else {
            legal = true;
        }
    } else if dev_dev && cur_status == "workflow_declaring" && cx.new_status == "workflow_review" {
        // P2: the vadi submits the declaration for peer review; the declaration
        // stamp must be coherent (declared_by == the submitting vadi, and
        // declared_at_checkpoint must not be from the future relative to the
        // checkpoint the declaration is submitted at).
        let declared_by = rw_declared_by(cand);
        if declared_by != writer_role || declared_by != "vadi" {
            reason = format!(
                "workflow_declaring->workflow_review requires run_workflow.declared_by={writer_role} (the submitting vadi), got declared_by={declared_by}"
            );
        } else if rw_declared_at_checkpoint(cand).is_some_and(|c| c > cx.new_checkpoint) {
            reason = format!(
                "workflow_declaring->workflow_review requires run_workflow.declared_at_checkpoint <= {} (the submitting checkpoint), got declared_at_checkpoint={:?}",
                cx.new_checkpoint,
                rw_declared_at_checkpoint(cand)
            );
        } else {
            legal = true;
        }
    } else if dev_dev && cur_status == "workflow_revision" && cx.new_status == "workflow_review" {
        // P2: the vadi resubmits a revised declaration (loop-capped above).
        // tc-p2-double-pending-amendment: the resubmission must not touch
        // amendments[] bookkeeping (only its free-form `reason` may change).
        if amendments_stable_ok(cur_doc, cand) {
            legal = true;
        } else {
            reason = "workflow_revision->workflow_review requires amendments[] unchanged: amendments immutable during reject/revise".to_string();
        }
    } else if dev_dev && cur_status == "workflow_review" && cx.new_status == "workflow_revision" {
        // P2: the prativadi rejects the declaration with findings (loop-capped
        // above); non-empty findings are required to name the objection.
        // tc-p2-double-pending-amendment: the rejection must not touch
        // amendments[] bookkeeping either (only its free-form `reason` may
        // change) — this closes the double-pending-amendment gap.
        if count_len(field(cand, "findings")) == 0 {
            reason = "workflow_review->workflow_revision requires non-empty findings".to_string();
        } else if !amendments_stable_ok(cur_doc, cand) {
            reason = "workflow_review->workflow_revision requires amendments[] unchanged: amendments immutable during reject/revise".to_string();
        } else {
            legal = true;
        }
    } else if dev_dev && cur_status == "workflow_review" && cx.new_status == "spec_drafting" {
        // P2: the prativadi APPROVES the declaration and the run enters
        // spec-drafting; the approval stamp (and, for custom graphs, the deep
        // invariants) must hold.
        match workflow_declaration_approve_ok(cand, &writer_role, cx.new_checkpoint) {
            Ok(()) => legal = true,
            Err(msg) => return Err((23, format!("DVANDVA_WRITE {msg} candidate={cf}"))),
        }
    } else if dev_dev
        && cur_status == "research_review"
        && cx.new_status == "spec_drafting"
        && rw_source(cur_doc) == "custom"
        && rw_approved_by(cur_doc).is_none()
    {
        // P2 approval enforcement: a custom, still-unapproved run-workflow cannot
        // jump research_review->spec_drafting directly — it must be declared and
        // approved via the workflow_declaring loop first. (Preset sources are the
        // engine's own pre-approved workflows and keep the direct edge.)
        reason = "research_review->spec_drafting requires an approved run_workflow; declare it via workflow_declaring first".to_string();
    } else if dev_dev
        && cx.new_status == "workflow_review"
        && !is_workflow_decl_status(&cur_status)
        && !matches!(cur_status.as_str(), "human_question" | "human_decision")
    {
        // P2 amendment entry: from any active non-terminal working status the
        // writer may raise a mid-flight amendment by appending a new pending
        // amendments[] entry that records where to resume.
        if amendment_entry_added_ok(cur_doc, cand, &writer_role, &cur_status, cx.new_checkpoint) {
            legal = true;
        } else {
            reason = "workflow_review amendment entry requires a new pending amendments[] entry (proposed_by=writer, at_checkpoint=current, resume_status=interrupted status)".to_string();
        }
    } else if dev_dev && cur_status == "workflow_review" && cx.new_status != "workflow_revision" {
        match amendment_resume_ok(
            cur_doc,
            cand,
            &writer_role,
            cx.new_status,
            cx.new_checkpoint,
        ) {
            Ok(true) => {
                // P2 amendment approve/resume: the peer stamps the pending amendment
                // approved and the run resumes to the recorded resume_status. For a
                // declared source=custom graph, the resume target must actually be
                // one of the declared states (mirroring custom_invariants_ok's
                // source=custom scoping; preset sources have no declared states[]
                // and are governed by the whitelist/edge interpreter instead).
                if rw_source(cand) == "custom"
                    && !rw_state_names(cand).iter().any(|s| s == cx.new_status)
                {
                    reason = "amendment resume target must be a declared-graph state".to_string();
                } else {
                    legal = true;
                }
            }
            Ok(false) => {
                // F9 edge selection: numeric-source states select by the current phase's
                // effective profile; the spec→implementation entry (and amendment exit)
                // selects by the target phase's effective profile; everything else uses
                // the run profile. When phase_profiles is absent every branch collapses
                // to cx.new_effective_profile — the pre-F9 behaviour.
                let edge_profile: String = if cur_status == "spec_review"
                    && matches!(cx.new_status, "implementing" | "parallel_implementing")
                {
                    new_phase_eff.clone()
                } else if is_numeric_phase_status(&cur_status) {
                    cur_phase_eff.clone()
                } else {
                    cx.new_effective_profile.to_string()
                };
                legal = edge_whitelist(
                    &eff_graph,
                    &cur_effective_mode,
                    &edge_profile,
                    &cur_status,
                    cx.new_status,
                    &mut reason,
                );
            }
            Err(msg) => return Err((23, format!("DVANDVA_WRITE {msg} candidate={cf}"))),
        }
    } else {
        // F9 edge selection: numeric-source states select by the current phase's
        // effective profile; the spec→implementation entry (and amendment exit)
        // selects by the target phase's effective profile; everything else uses
        // the run profile. When phase_profiles is absent every branch collapses
        // to cx.new_effective_profile — the pre-F9 behaviour.
        let edge_profile: String = if cur_status == "spec_review"
            && matches!(cx.new_status, "implementing" | "parallel_implementing")
        {
            new_phase_eff.clone()
        } else if is_numeric_phase_status(&cur_status) {
            cur_phase_eff.clone()
        } else {
            cx.new_effective_profile.to_string()
        };
        legal = edge_whitelist(
            &eff_graph,
            &cur_effective_mode,
            &edge_profile,
            &cur_status,
            cx.new_status,
            &mut reason,
        );
    }

    // ---- post-legality edge gates ------------------------------------------
    // F9 advancement entry-state gate: any advancement/entry into a numeric phase
    // must use the entry state that matches the TARGET phase's effective profile
    // (implementing <=> standard, parallel_implementing <=> full). The whitelist
    // alone cannot enforce this for cross-profile advancement — a full phase's
    // `deslop` reaches BOTH `implementing` and `parallel_implementing`, and a
    // standard phase's `phase_review` likewise — so the entry state is pinned to
    // the next phase's profile here. (The spec_review entry is already gated by
    // the target-profile whitelist selection above; this makes it uniform.)
    if legal
        && cx.is_v2
        && cur_effective_mode == "development"
        && matches!(cx.new_status, "implementing" | "parallel_implementing")
        && matches!(
            cur_status.as_str(),
            "spec_review" | "deslop" | "phase_review"
        )
    {
        let want = if new_phase_eff == "full" {
            "parallel_implementing"
        } else {
            "implementing"
        };
        if cx.new_status != want {
            return Err((
                24,
                format!(
                    "DVANDVA_WRITE illegal_transition entry_state={} target_phase={} effective_profile={} requires={}",
                    cx.new_status, cx.new_phase, new_phase_eff, want
                ),
            ));
        }
    }
    // S4-T2 (D2): the spec→implementation boundary — including a plan-amendment
    // exit — requires the master plan to be locked. Development graph only;
    // research/review modes never enter numeric implementation states.
    if legal
        && cx.is_v2
        && cur_effective_mode == "development"
        && cur_status == "spec_review"
        && matches!(cx.new_status, "implementing" | "parallel_implementing")
        && !bool_field(cand, "master_plan_locked")
    {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_master_plan_locked candidate={cf}"),
        ));
    }
    if legal && cx.is_v2 && cx.new_status == "parallel_implementing" {
        // S5-T3: a present-but-invalid waiver is a hard error at this gate; an
        // absent one leaves the ≥5 floor in force; a valid one waives it (the
        // per-role ≥2 floor still holds).
        let waiver = work_split_waiver_state(cand);
        if waiver == WaiverState::Malformed {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_work_split_waiver candidate={cf}"),
            ));
        }
        if !parallel_work_split_ok(cand, waiver == WaiverState::Valid) {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_parallel_work_split candidate={cf}"),
            ));
        }
    }
    if legal
        && cx.is_v2
        && cur_status == "parallel_implementing"
        && cx.new_status == "test_creation"
    {
        // S5-T3: same waiver rule as the parallel_implementing entry.
        let waiver = work_split_waiver_state(cand);
        if waiver == WaiverState::Malformed {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_work_split_waiver candidate={cf}"),
            ));
        }
        if !parallel_to_test_creation_ok(cand, waiver == WaiverState::Valid) {
            legal = false;
            reason = "parallel_implementing->test_creation requires completed implementation-chunk subagent_tracks for both roles".to_string();
        }
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
        let required = cross_review_cycle_checkpoint(baton_dir, cur_doc, cur_checkpoint_i64);
        if !cross_review_to_cross_fixing_ok(cand, required) {
            legal = false;
            reason = "cross_review->cross_fixing requires current-cycle completed cross-review subagent_tracks with non-approval evidence".to_string();
        }
    }
    if legal && cx.is_v2 && cur_status == "cross_review" && cx.new_status == "deep_review" {
        let required = cross_review_cycle_checkpoint(baton_dir, cur_doc, cur_checkpoint_i64);
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
        let required = deep_review_cycle_checkpoint(baton_dir, cur_doc, cur_checkpoint_i64);
        if !deep_review_angles_ok(cand, required) {
            legal = false;
            reason = "deep_review->deslop requires current-cycle three completed review-angle subagent_tracks".to_string();
        }
        // F6: risk-triggered angles for full-profile phases. Only checked once
        // the three base angles pass (legal still true); a missing angle is a
        // hard 23, naming the missing angle and its trigger.
        if legal && cur_phase_eff == "full" {
            if security_trigger_present(cand, &cur_phase)
                && !risk_angle_present(cand, required, "security", "dvandva-security-auditor")
            {
                return Err((23, format!(
                    "DVANDVA_WRITE bad_deep_review_angles missing_angle=security trigger=security_path candidate={cf}"
                )));
            }
            if integration_trigger_present(cand, &cur_phase)
                && !risk_angle_present(cand, required, "integration", "dvandva-integration-checker")
            {
                return Err((23, format!(
                    "DVANDVA_WRITE bad_deep_review_angles missing_angle=integration trigger=multi_owner_seam candidate={cf}"
                )));
            }
        }
    }

    // ---- P1: clarifying-questions per-state non-null gates -----------------
    if legal
        && cx.is_v2
        && cur_status == "clarifying_questions_drafting"
        && cx.new_status == "clarifying_questions_answer"
        && !clarifying_round_questions_asked(cand, 1)
    {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_clarifying_questions_round1 candidate={cf}"),
        ));
    }
    if legal
        && cx.is_v2
        && cur_status == "clarifying_questions_answer"
        && cx.new_status == "clarifying_questions_followup"
        && !clarifying_round_answered(cand, 1)
    {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_clarifying_questions_round1_answer candidate={cf}"),
        ));
    }
    if legal
        && cx.is_v2
        && cur_status == "clarifying_questions_followup"
        && cx.new_status == "clarifying_questions_followup_answer"
        && (!clarifying_round_questions_asked(cand, 2) || !clarifying_total_ok(cand))
    {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_clarifying_questions_round2 candidate={cf}"),
        ));
    }
    if legal
        && cx.is_v2
        && cur_status == "clarifying_questions_followup_answer"
        && cx.new_status == "research_drafting"
        && !clarifying_round_answered(cand, 2)
    {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_clarifying_questions_round2_answer candidate={cf}"),
        ));
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
        && jq_render(field(cur_doc, "total_phases")) != jq_render(field(cand, "total_phases"))
    {
        return Err((
            23,
            format!("DVANDVA_WRITE bad_amendment total_phases_frozen candidate={cf}"),
        ));
    }

    // F9: the amendment entry flavour follows the CURRENT phase's effective
    // profile; the exit re-enters the TARGET phase per its effective profile.
    let is_enter = dev_dev && is_amendment_enter_edge(&cur_phase_eff, &cur_status, cx.new_status);
    let is_exit = dev_dev && is_amendment_exit_edge(&new_phase_eff, &cur_status, cx.new_status);

    // The amendment entry edge MUST set amendment_from_phase == current phase.
    if legal && is_enter && (cand_amendment.is_none() || cand_amendment != cur_phase_num) {
        return Err((23, format!("DVANDVA_WRITE bad_amendment candidate={cf}")));
    }

    // amendment_from_phase may only BECOME non-null on an entry edge.
    if cx.is_v2 && cur_amendment.is_none() && cand_amendment.is_some() && !is_enter {
        return Err((23, format!("DVANDVA_WRITE bad_amendment candidate={cf}")));
    }

    // While the amendment loop is active (cur non-null, outside human states):
    // the exit edge must null the field and re-enter at phase >= from-phase; any
    // other step must leave amendment_from_phase unchanged.
    if let Some(from) = cur_amendment {
        if cx.is_v2 && cur_status != "human_decision" && cx.new_status != "human_decision" {
            if is_exit {
                if cand_amendment.is_some() {
                    return Err((23, format!("DVANDVA_WRITE bad_amendment candidate={cf}")));
                }
                if new_phase_num.map(|p| p < from).unwrap_or(true) {
                    legal = false;
                    reason = format!(
                        "amendment re-entry phase {} below amendment_from_phase {from}",
                        cx.new_phase
                    );
                }
            } else if cand_amendment != Some(from) {
                return Err((23, format!("DVANDVA_WRITE bad_amendment candidate={cf}")));
            }
        }
    }

    // ---- F10 explainer-verification gate (full-profile terminal done) ------
    if legal
        && cx.is_v2
        && cx.new_effective_mode == "development"
        && cx.new_status == "done"
        && cx.new_effective_profile == "full"
    {
        let required = termination_review_cycle_checkpoint(baton_dir, cur_doc, cur_checkpoint_i64);
        if !explainer_verification_ok(cand, required) {
            return Err((
                23,
                format!("DVANDVA_WRITE bad_explainer_verification candidate={cf}"),
            ));
        }
    }

    // ---- S4-T6 (D3): verification_matrix freshness (development done) -------
    // Full-profile done requires EVERY matrix row complete AND fresh; compact
    // done layers the freshness qualifier onto the existing good_matrix (which
    // already enforced completeness earlier). The anchor is the last
    // implementation-family checkpoint across history + current.
    if legal && cx.is_v2 && cx.new_effective_mode == "development" && cx.new_status == "done" {
        let anchor = implementation_family_anchor(baton_dir, cur_doc, cur_checkpoint_i64);
        let full = cx.new_effective_profile == "full";
        if let Some(row) = stale_verification_matrix_row(cand, anchor, full) {
            return Err((
                23,
                format!("DVANDVA_WRITE stale_verification_matrix row={row} anchor={anchor}"),
            ));
        }
    }

    // ---- S4-T1: required done-gate artifacts must resolve to real files ----
    if legal && cx.is_v2 && cx.new_status == "done" {
        required_done_artifacts_ok(baton_dir, cand, cx)?;
    }

    if !legal {
        return Err((24, format!("DVANDVA_WRITE illegal_transition {reason}")));
    }
    Ok(())
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
    // Test-only deterministic interleaving seam (pre-mv fence-check).
    barrier_wait("DVANDVA_WRITE_BARRIER");

    // Fencing: re-verify we still own the lock before the irreversible install.
    if !plan.lock.holds() {
        eprintln!(
            "DVANDVA_WRITE lock_lost fencing_token_mismatch path={} refusing_to_install=true",
            plan.lock.dir.join(lock::LOCK_DIR_NAME).display()
        );
        plan.lock.disarm(); // the lock now belongs to the thief; do not remove it
        return 29;
    }

    // S4-T10 test seam: a SECOND barrier stage that pauses AFTER the pre-mv
    // fence-check but BEFORE the rename, so a test can steal the lock inside the
    // one window the pre-mv fence cannot cover (fence passes, then a thief lands,
    // then this writer installs). Keyed on its own env var so the pre-mv barrier
    // (and the existing race tests) are untouched.
    barrier_wait("DVANDVA_WRITE_BARRIER_POSTFENCE");

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

    // S4-T10 post-mv fence: the rename is done — the baton IS installed. Re-verify
    // we still hold the lock. If a thief replaced our fencing token in the
    // fence-check→rename window, the baton we just wrote may already be superseded
    // by the lock's new owner, so the caller must re-read. Do NOT release the
    // thief's lock (disarm), and skip the snapshot (it would archive a state we no
    // longer own).
    if !plan.lock.holds() {
        eprintln!(
            "DVANDVA_WRITE lock_lost_post_install path={} baton_installed=true may_be_superseded=true caller_must_reread=true",
            plan.lock.dir.join(lock::LOCK_DIR_NAME).display()
        );
        plan.lock.disarm();
        return 29;
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

/// Test-only deterministic interleaving seam: when `env` names a non-empty
/// path, touch `<path>.arrived` and block until `<path>.release` appears (or a
/// bounded number of polls elapse). Two independent seams — `DVANDVA_WRITE_BARRIER`
/// (pre-mv fence-check) and `DVANDVA_WRITE_BARRIER_POSTFENCE` (fence→rename
/// window) — let a test drive either lock-theft race deterministically.
fn barrier_wait(env: &str) {
    if let Ok(barrier) = std::env::var(env) {
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

/// The exact top-level key set a `dvandva.baton.v2` candidate must carry (base +
/// v2 additions). Exposed as the single source of truth for `lint skills`'
/// inline-contract-block key check (S5-T2 re-key), so the engine's required-key
/// list and the skills' seed shape can never drift.
pub(crate) fn v2_required_keys() -> Vec<&'static str> {
    required_keys(true)
}

/// The canonical `dvandva.baton.v2` status catalog — the single in-code source
/// of truth for the 26 lifecycle status tokens. `status_enum_ok` (this file),
/// [`crate::baton::Status`], [`crate::preflight::V2_STATUS_TOKENS`], and every
/// doc copy the S6-T1 schema-parity lint checks
/// (`baton-schema-v2.json` `status_catalog`, `product.md`'s status catalog
/// line, `state-transition-table.md`'s status catalog line) must all agree
/// with this list. `status_enum_ok`'s match arm is kept as the hot-path
/// acceptor; the code-side parity is asserted by the schema-parity lint's unit
/// tests rather than by re-routing the acceptor through this slice.
pub(crate) const V2_STATUS_CATALOG: &[&str] = &[
    "clarifying_questions_drafting",
    "clarifying_questions_answer",
    "clarifying_questions_followup",
    "clarifying_questions_followup_answer",
    "research_drafting",
    "research_review",
    "research_revision",
    "spec_drafting",
    "spec_review",
    "spec_revision",
    "implementing",
    "parallel_implementing",
    "test_creation",
    "cross_review",
    "cross_fixing",
    "deep_review",
    "review_of_review",
    "counter_review",
    "deslop",
    "termination_review",
    "phase_review",
    "phase_fixing",
    "human_question",
    "human_decision",
    "done",
    "abandoned",
];

/// The live `dvandva.baton.v3` engine status catalog: the 26-token v2 lifecycle
/// base plus the three v3-only per-run-workflow declaration states
/// (`workflow_declaring`/`workflow_review`/`workflow_revision`). This is the
/// catalog `status_enum_ok` accepts, `baton::Status` enumerates, the
/// `run_workflow` shape validator legalises custom-declared states against, and
/// the v3 doc copy (`baton-schema-v3.json`) is pinned to by schema-parity. The
/// retired v2 copies (`baton-schema-v2.json`, `product.md`/state-table catalog
/// lines, `preflight::V2_STATUS_TOKENS`) stay frozen at the historical 26.
pub(crate) const V3_STATUS_CATALOG: &[&str] = &[
    "clarifying_questions_drafting",
    "clarifying_questions_answer",
    "clarifying_questions_followup",
    "clarifying_questions_followup_answer",
    "research_drafting",
    "research_review",
    "research_revision",
    "spec_drafting",
    "spec_review",
    "spec_revision",
    "workflow_declaring",
    "workflow_review",
    "workflow_revision",
    "implementing",
    "parallel_implementing",
    "test_creation",
    "cross_review",
    "cross_fixing",
    "deep_review",
    "review_of_review",
    "counter_review",
    "deslop",
    "termination_review",
    "phase_review",
    "phase_fixing",
    "human_question",
    "human_decision",
    "done",
    "abandoned",
];

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
        "clarifying_questions_drafting" => "vadi",
        "clarifying_questions_followup" => "prativadi",
        "clarifying_questions_answer" | "clarifying_questions_followup_answer" => "human",
        // F8: test_creation is team-owned in the v2 full profile (its only home).
        // v3 per-run-workflow declaration loop: vadi drafts/revises, prativadi
        // reviews.
        "workflow_declaring" | "workflow_revision" => "vadi",
        "workflow_review" => "prativadi",
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
        // S2-T1: abandoned is a human-declared terminal, human-owned like the
        // other human states.
        "human_question" | "human_decision" | "abandoned" => "human",
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

/// The numeric `total_phases` value (number, or a stringified integer), or None
/// when unset/non-numeric. `0` is the pre-lock "no ceiling yet" sentinel.
fn total_phases_num(doc: &Value) -> Option<i64> {
    match field(doc, "total_phases") {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.parse::<i64>().ok(),
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

/// The v2 status vocabulary. The v1 arm was removed with S5-T2 (v1 candidates
/// are rejected upstream with `schema_retired`), so only v2 statuses remain.
/// `pub(crate)` so the schema-parity lint's unit tests can assert this
/// hot-path acceptor agrees with [`V3_STATUS_CATALOG`] for every token.
pub(crate) fn status_enum_ok(status: &str) -> bool {
    matches!(
        status,
        "clarifying_questions_drafting"
            | "clarifying_questions_answer"
            | "clarifying_questions_followup"
            | "clarifying_questions_followup_answer"
            | "research_drafting"
            | "research_review"
            | "research_revision"
            | "spec_drafting"
            | "spec_review"
            | "spec_revision"
            | "workflow_declaring"
            | "workflow_review"
            | "workflow_revision"
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
            | "abandoned"
    )
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
/// S5-T5: did a research-mode run take the SEED path (research that produced a
/// development spec)? On the seed path a spec was drafted, so `plan_ref` is set
/// and/or `research_outcome == "seed_development"`; that path keeps the "spec"
/// phase label on its terminal statuses. The exploratory path (research only,
/// no spec) uses "research". Candidate-checkable — the fields the seed markers
/// live on are inherited across every transition of the run.
fn research_seed(cand: &Value) -> bool {
    matches!(field(cand, "research_outcome"), Some(Value::String(s)) if s == "seed_development")
        || matches!(field(cand, "plan_ref"), Some(Value::String(s)) if !s.trim().is_empty())
}

/// S5-T5 current-side leniency: an already-installed research-mode baton may
/// carry the OLD `"spec"` label on a terminal status where the run should now be
/// `"research"` (or vice-versa). A transition off such a baton must not be read
/// as a phase advancement / a forbidden same-status phase change merely because
/// the two ends disagree on the interchangeable research-planning label. Guards
/// the CURRENT side only — candidates are still held strict by `phase_status_ok`.
fn research_planning_relabel(mode: &str, a: &str, b: &str) -> bool {
    mode == "research" && matches!(a, "spec" | "research") && matches!(b, "spec" | "research")
}

fn phase_status_ok(mode: &str, status: &str, cand: &Value) -> bool {
    let phase = field(cand, "phase");
    let is_str = |want: &str| matches!(phase, Some(Value::String(s)) if s == want);
    let is_num = || matches!(phase, Some(Value::Number(_)));
    match (mode, status) {
        // S2-T1: abandoned preserves whatever phase the human state carried, just
        // like the other human statuses.
        (_, "human_question") | (_, "human_decision") | (_, "abandoned") => true,
        (
            "development" | "research" | "review",
            "clarifying_questions_drafting"
            | "clarifying_questions_answer"
            | "clarifying_questions_followup"
            | "clarifying_questions_followup_answer",
        ) => is_str("clarifying"),
        ("development", "research_drafting" | "research_review" | "research_revision") => {
            is_str("research")
        }
        ("development", "spec_drafting" | "spec_review" | "spec_revision") => is_str("spec"),
        // v3 per-run-workflow declaration loop lives in the "spec" planning phase.
        ("development", "workflow_declaring" | "workflow_review" | "workflow_revision") => {
            is_str("spec")
        }
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
        // S5-T5: research-mode terminals label by run outcome — "research" on the
        // exploratory path, "spec" only when the run seeded a development spec.
        ("research", "termination_review" | "phase_fixing" | "done") => {
            if research_seed(cand) {
                is_str("spec")
            } else {
                is_str("research")
            }
        }
        // The spec_* planning statuses (spec_drafting/spec_review/spec_revision)
        // are always "spec".
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
    static LOCKS_RE: OnceLock<Regex> = OnceLock::new();
    let skill_md =
        SKILL_MD.get_or_init(|| Regex::new(r"^plugins/dvandva/skills/[^/]+/SKILL\.md$").unwrap());
    let commands_md =
        COMMANDS_MD.get_or_init(|| Regex::new(r"^plugins/dvandva/commands/[^/]+\.md$").unwrap());
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
        // F6 reuses these three security submatchers (do not re-derive).
        || is_security_path(p)
        || locks_re.is_match(p)
}

/// The security submatchers embedded in [`hard_path`]: `.env*`, secret/credential
/// tokens, and api/client tokens. Extracted so the F6 deep-review SECURITY-angle
/// trigger reuses the exact same matchers rather than re-deriving them.
fn is_security_path(p: &str) -> bool {
    static ENV_RE: OnceLock<Regex> = OnceLock::new();
    static SECRETS_RE: OnceLock<Regex> = OnceLock::new();
    static API_RE: OnceLock<Regex> = OnceLock::new();
    let env_re = ENV_RE.get_or_init(|| Regex::new(r"(^|/)\.env(\..*)?$").unwrap());
    let secrets_re = SECRETS_RE
        .get_or_init(|| Regex::new(r"(^|/)(secret|secrets|credential|credentials)(/|$)").unwrap());
    let api_re = API_RE.get_or_init(|| Regex::new(r"(^|/)(api|apis|client|clients)(/|$)").unwrap());
    env_re.is_match(p) || secrets_re.is_match(p) || api_re.is_match(p)
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
        "opus-class|gpt-5.5-xhigh"
            | "sonnet-class|gpt-5.5-high"
            | "opus-class|gpt-5.5"
            | "sonnet-class|gpt-5.4"
            | "gpt-5.5"
            | "gpt-5.4"
            | "opus"
            | "sonnet"
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
/// The role required to have asked every `clarifying_questions` entry for
/// `round`: round 1 is vadi's planner/feasibility/scope lens, round 2 is
/// prativadi's reviewer/adversarial lens informed by round 1's answers.
/// Enforcing this (not just round membership/count) is what makes the
/// design's ">=1 question per role" requirement actually load-bearing.
fn clarifying_round_asked_by(round: i64) -> &'static str {
    if round == 1 {
        "vadi"
    } else {
        "prativadi"
    }
}

/// P1: every `clarifying_questions` entry for `round` has a non-empty
/// `question`, a still-null `answer`, and the correct `asked_by` role for
/// that round, AND at least one such entry exists — the gate for leaving
/// `clarifying_questions_drafting` (round 1) / entering
/// `clarifying_questions_followup` (round 2).
fn clarifying_round_questions_asked(cand: &Value, round: i64) -> bool {
    let entries: Vec<&Value> = arr(field(cand, "clarifying_questions"))
        .iter()
        .filter(|q| field(q, "round").and_then(json_int) == Some(round))
        .collect();
    let expected_asked_by = clarifying_round_asked_by(round);
    !entries.is_empty()
        && entries.iter().all(|q| {
            matches!(field(q, "question"), Some(Value::String(s)) if !s.is_empty())
                && field(q, "answer") == Some(&Value::Null)
                && str_field(q, "asked_by") == expected_asked_by
        })
}

/// P1: every `clarifying_questions` entry for `round` has a non-empty
/// `answer` and the correct `asked_by` role for that round, AND at least one
/// such entry exists — the gate for leaving `clarifying_questions_answer`
/// (round 1) / `clarifying_questions_followup_answer` (round 2).
fn clarifying_round_answered(cand: &Value, round: i64) -> bool {
    let entries: Vec<&Value> = arr(field(cand, "clarifying_questions"))
        .iter()
        .filter(|q| field(q, "round").and_then(json_int) == Some(round))
        .collect();
    let expected_asked_by = clarifying_round_asked_by(round);
    !entries.is_empty()
        && entries.iter().all(|q| {
            matches!(field(q, "answer"), Some(Value::String(s)) if !s.is_empty())
                && str_field(q, "asked_by") == expected_asked_by
        })
}

/// P1: the combined round-1 + round-2 `clarifying_questions` total is >=5,
/// with both rounds represented by their correct `asked_by` role (>=1
/// question per role) — the gate for leaving `clarifying_questions_followup`.
fn clarifying_total_ok(cand: &Value) -> bool {
    let all = arr(field(cand, "clarifying_questions"));
    let round1 = all.iter().any(|q| {
        field(q, "round").and_then(json_int) == Some(1)
            && str_field(q, "asked_by") == clarifying_round_asked_by(1)
    });
    let round2 = all.iter().any(|q| {
        field(q, "round").and_then(json_int) == Some(2)
            && str_field(q, "asked_by") == clarifying_round_asked_by(2)
    });
    all.len() >= 5 && round1 && round2
}

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
// declared-graph interpreter
// ===========================================================================

/// A single legal edge parsed from a v3 `run_workflow.edges` entry.
struct DeclaredEdge {
    from: String,
    to: String,
    /// Declared loop counter key for this edge, when the workflow author marks
    /// it as capped.
    loop_cap_key: Option<String>,
    /// Dynamic amendment cap marker for custom graphs.
    amendment_capped: bool,
}

/// The effective transition graph a baton's edges are legalized against.
///
/// The transition-legality authority is the baton's own declared workflow, not
/// a hardcoded profile match:
/// * a v3 baton whose `run_workflow.source` is `"custom"` is legalized by its
///   OWN declared edges — nothing else;
/// * a v3 baton with a `preset:<name>` source, and every v2 baton, resolve to
///   the `(mode, profile)` preset (identical selection to the pre-cutover
///   match; F9 cross-profile advancement stays param-driven, and a `research`/
///   `review` run keeps selecting its mode preset even though its stub source
///   reads `preset:full`);
/// * a v1 baton, or a v3 baton whose `run_workflow` is absent, has no legal
///   edges. v3 REQUIRES a shape-valid run_workflow — the write path shape-gates
///   it upstream, so an absent/malformed one only reaches here on the read/LIST
///   path, where surfacing zero edges is the honest answer, never a silent
///   preset fallback.
enum EffectiveGraph {
    /// v3 `source:custom` — the declared edges are the whole authority.
    Declared(Vec<DeclaredEdge>),
    /// v2, or v3 `source:preset:<name>` — select the preset by `(mode, profile)`.
    Preset,
    /// v1, or a v3 with an absent run_workflow — no legal edges.
    None,
}

/// Resolve the effective transition graph for a (current) baton document. This
/// is the single source item 5 mandates: v3 → its own `run_workflow`
/// (custom → declared edges), v2 → the `(mode, profile)` preset, everything
/// else → no edges.
fn resolve_effective_graph(doc: &Value) -> EffectiveGraph {
    match str_field(doc, "schema").as_str() {
        "dvandva.baton.v3" => {
            let Some(rw) = field(doc, "run_workflow") else {
                return EffectiveGraph::None;
            };
            if str_field(rw, "source") == "custom" {
                let edges = arr(field(rw, "edges"))
                    .iter()
                    .filter_map(|e| {
                        let from = str_field(e, "from");
                        let to = str_field(e, "to");
                        if from.is_empty() || to.is_empty() {
                            return None;
                        }
                        let loop_cap_key = match field(e, "loop_cap_key") {
                            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
                            _ => None,
                        };
                        Some(DeclaredEdge {
                            from,
                            to,
                            loop_cap_key,
                            amendment_capped: bool_field(e, "amendment_capped"),
                        })
                    })
                    .collect();
                EffectiveGraph::Declared(edges)
            } else {
                // A shape-valid non-custom source is `preset:<name>`; the run
                // adopts a standard preset and legality stays `(mode, profile)`-
                // selected, exactly like v2.
                EffectiveGraph::Preset
            }
        }
        "dvandva.baton.v2" => EffectiveGraph::Preset,
        _ => EffectiveGraph::None,
    }
}

/// The preset name a `(mode, profile)` pair selects, mirroring the pre-cutover
/// `edge_whitelist` match arms: development picks `fast`/`standard`/`full` by
/// profile; `research`/`review` pick their eponymous preset; anything else
/// selects nothing (the old `_ => false`).
fn preset_name_for(mode: &str, profile: &str) -> Option<&'static str> {
    match mode {
        "development" => match profile {
            "fast" => Some("fast"),
            "standard" => Some("standard"),
            "full" => Some("full"),
            _ => None,
        },
        "research" => Some("research"),
        "review" => Some("review"),
        _ => None,
    }
}

/// `true` when `(from, to)` is a legal edge of the `(mode, profile)` preset.
fn preset_edge_legal(mode: &str, profile: &str, from: &str, to: &str) -> bool {
    preset_name_for(mode, profile)
        .and_then(crate::workflow::preset)
        .map(|g| g.edges.iter().any(|e| e.from == from && e.to == to))
        .unwrap_or(false)
}

/// The loop-count key for `edge` (`"from:to"`) under `graph`: a custom graph
/// uses its declared `loop_cap_key`; every preset/v2 graph falls back to the
/// static loop-edge key, which is the edge string itself.
fn loop_key_for_edge(graph: &EffectiveGraph, edge: &str) -> Option<String> {
    match graph {
        EffectiveGraph::Declared(edges) => {
            let (from, to) = edge.split_once(':')?;
            edges.iter().find_map(|e| {
                if e.from == from && e.to == to {
                    e.loop_cap_key
                        .clone()
                        .or_else(|| e.amendment_capped.then(|| edge.to_string()))
                } else {
                    None
                }
            })
        }
        _ => is_loop_edge(edge).then(|| edge.to_string()),
    }
}

// ===========================================================================
// v3 per-run-workflow declaration loop + amendments (P2)
// ===========================================================================

/// The three v3-only per-run-workflow declaration statuses.
fn is_workflow_decl_status(status: &str) -> bool {
    matches!(
        status,
        "workflow_declaring" | "workflow_review" | "workflow_revision"
    )
}

/// `run_workflow.approved_by` as a non-empty string, or `None` when unapproved.
fn rw_approved_by(doc: &Value) -> Option<String> {
    match field(doc, "run_workflow").and_then(|rw| field(rw, "approved_by")) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// `run_workflow.source` (empty when absent).
fn rw_source(doc: &Value) -> String {
    field(doc, "run_workflow")
        .map(|rw| str_field(rw, "source"))
        .unwrap_or_default()
}

/// `run_workflow.declared_by` (empty when absent).
fn rw_declared_by(doc: &Value) -> String {
    field(doc, "run_workflow")
        .map(|rw| str_field(rw, "declared_by"))
        .unwrap_or_default()
}

/// `run_workflow.approved_at_checkpoint` as a `u64`, or `None`.
fn rw_approved_at_checkpoint(doc: &Value) -> Option<u64> {
    field(doc, "run_workflow")
        .and_then(|rw| field(rw, "approved_at_checkpoint"))
        .and_then(|v| v.as_u64())
}

/// `run_workflow.declared_at_checkpoint` as a `u64`, or `None`.
fn rw_declared_at_checkpoint(doc: &Value) -> Option<u64> {
    field(doc, "run_workflow")
        .and_then(|rw| field(rw, "declared_at_checkpoint"))
        .and_then(|v| v.as_u64())
}

/// `run_workflow.states[].name` (empty when absent, as for preset sources).
fn rw_state_names(doc: &Value) -> Vec<String> {
    field(doc, "run_workflow")
        .map(|rw| {
            arr(field(rw, "states"))
                .iter()
                .map(|s| str_field(s, "name"))
                .collect()
        })
        .unwrap_or_default()
}

/// The `run_workflow.amendments[]` entries (empty when absent/malformed).
fn rw_amendments(doc: &Value) -> Vec<Value> {
    field(doc, "run_workflow")
        .map(|rw| arr(field(rw, "amendments")).to_vec())
        .unwrap_or_default()
}

/// True when a candidate at `workflow_review` legitimately APPROVES a workflow
/// declaration: the peer prativadi stamps `approved_by=prativadi` (the shape
/// validator already enforced it differs from `declared_by`) and
/// `approved_at_checkpoint=<current>`. For a `source=custom` graph the deep
/// invariant checks must also pass. Returns the transition-level reason on
/// failure.
fn workflow_declaration_approve_ok(
    cand: &Value,
    writer_role: &str,
    checkpoint: u64,
) -> Result<(), String> {
    if writer_role != "prativadi" {
        return Err("bad_workflow_approval requires DVANDVA_ROLE=prativadi".to_string());
    }
    match rw_approved_by(cand) {
        Some(role) if role == "prativadi" => {}
        _ => return Err("bad_workflow_approval approved_by must be stamped prativadi".to_string()),
    }
    if rw_approved_at_checkpoint(cand) != Some(checkpoint) {
        return Err(format!(
            "bad_workflow_approval approved_at_checkpoint must be {checkpoint}"
        ));
    }
    custom_invariants_ok(cand)
}

/// Run the graph-level invariant checks for a `source=custom` candidate's
/// declared `run_workflow`; a no-op for preset sources (the engine presets are
/// pre-validated). Emits a `bad_workflow_invariants` reason on the first
/// violation.
fn custom_invariants_ok(cand: &Value) -> Result<(), String> {
    if rw_source(cand) != "custom" {
        return Ok(());
    }
    let Some(rw) = field(cand, "run_workflow") else {
        return Ok(());
    };
    let graph = build_custom_graph(rw);
    // Seed from the first declared state; the invariant checker walks the whole
    // graph from there (review-gate cut, escape reachability, absorbing states).
    let seed = graph
        .states
        .first()
        .map(|s| s.name)
        .unwrap_or("clarifying_questions_drafting");
    match crate::workflow::validate_workflow_invariants(&graph, seed) {
        Ok(()) => Ok(()),
        Err(violations) => Err(format!("bad_workflow_invariants {violations:?}")),
    }
}

/// Build an owned [`crate::workflow::WorkflowGraph`] from a candidate's declared
/// `run_workflow` so the invariant checker (which takes `&'static str` state
/// tokens) can run against it. The `Box::leak` here is deliberately scoped to
/// the rare `source=custom` declaration/approval write in a short-lived CLI
/// process; every preset run skips this path entirely.
fn build_custom_graph(rw: &Value) -> crate::workflow::WorkflowGraph {
    use crate::workflow::{StateClass, WfEdge, WfState, WorkflowGraph};
    fn leak(s: &str) -> &'static str {
        Box::leak(s.to_string().into_boxed_str())
    }
    let class_of = |c: &str| match c {
        "review_gate" => StateClass::ReviewGate,
        "human_gate" => StateClass::HumanGate,
        "pause" => StateClass::Pause,
        "terminal" => StateClass::Terminal,
        _ => StateClass::Work,
    };
    let states = arr(field(rw, "states"))
        .iter()
        .map(|s| WfState {
            name: leak(&str_field(s, "name")),
            owner: leak(&str_field(s, "owner")),
            class: class_of(&str_field(s, "class")),
        })
        .collect();
    let edges = arr(field(rw, "edges"))
        .iter()
        .map(|e| WfEdge {
            from: leak(&str_field(e, "from")),
            to: leak(&str_field(e, "to")),
            loop_cap_key: None,
            amendment_capped: false,
        })
        .collect();
    WorkflowGraph {
        name: "custom",
        states,
        edges,
    }
}

/// True when `amendments[]` is stable across the reject
/// (`workflow_review->workflow_revision`) and revise
/// (`workflow_revision->workflow_review`) edges (tc-p2-double-pending-
/// amendment): no entry may be appended or removed, every non-latest entry is
/// fully byte-identical, and the latest entry — when still pending
/// (`approved_by` null) — keeps its `proposed_by`, `at_checkpoint`, and
/// `resume_status` unchanged with `approved_by`/`approved_at_checkpoint`
/// still null (approval only happens on the `workflow_review->resume_status`
/// edge, never here). A latest entry that is already approved is treated the
/// same as a non-latest one (fully immutable) — these edges never touch an
/// approval stamp. Any other field on the latest entry (e.g. a free-form
/// `reason`) is not compared here and so may legitimately change; that is the
/// revision surface for amendment content itself. A run with no amendments
/// at all (the common declaration-only case) trivially passes.
fn amendments_stable_ok(cur_doc: &Value, cand: &Value) -> bool {
    let cur_am = rw_amendments(cur_doc);
    let cand_am = rw_amendments(cand);
    if cur_am.len() != cand_am.len() {
        return false; // no new/removed entries
    }
    let Some(last) = cur_am.len().checked_sub(1) else {
        return true; // no amendments to protect
    };
    if cur_am[..last] != cand_am[..last] {
        return false; // non-latest entries are fully immutable
    }
    let (c, n) = (&cur_am[last], &cand_am[last]);
    if !is_null_field(c, "approved_by") {
        return c == n; // already-approved latest entry: fully immutable too
    }
    str_field(c, "proposed_by") == str_field(n, "proposed_by")
        && field(c, "at_checkpoint") == field(n, "at_checkpoint")
        && str_field(c, "resume_status") == str_field(n, "resume_status")
        && is_null_field(n, "approved_by")
        && is_null_field(n, "approved_at_checkpoint")
}

/// True when a candidate legitimately RAISES a new amendment: from an active
/// non-terminal working status the writer appends exactly one `amendments[]`
/// entry (append-only prefix) whose `proposed_by`=writer, `at_checkpoint`=
/// current, `resume_status`=the interrupted status, and which is not yet
/// approved.
fn amendment_entry_added_ok(
    cur_doc: &Value,
    cand: &Value,
    writer_role: &str,
    cur_status: &str,
    checkpoint: u64,
) -> bool {
    let cur_am = rw_amendments(cur_doc);
    let cand_am = rw_amendments(cand);
    if cand_am.len() != cur_am.len() + 1 {
        return false;
    }
    if cur_am.iter().zip(cand_am.iter()).any(|(a, b)| a != b) {
        return false; // append-only: the existing prefix must be untouched
    }
    let e = &cand_am[cand_am.len() - 1];
    str_field(e, "proposed_by") == writer_role
        && field(e, "at_checkpoint").and_then(|v| v.as_u64()) == Some(checkpoint)
        && str_field(e, "resume_status") == cur_status
        && is_null_field(e, "approved_by")
}

/// True when a candidate at `workflow_review` legitimately APPROVES a pending
/// amendment and resumes to its `resume_status`: exactly one entry flips from
/// unapproved to `approved_by`=writer (peer) + `approved_at_checkpoint`=current
/// with its `resume_status` equal to `new_status`; every other entry is
/// untouched. For a `source=custom` graph the invariant checks are re-run.
fn amendment_resume_ok(
    cur_doc: &Value,
    cand: &Value,
    writer_role: &str,
    new_status: &str,
    checkpoint: u64,
) -> Result<bool, String> {
    let cur_am = rw_amendments(cur_doc);
    let cand_am = rw_amendments(cand);
    if cand_am.len() != cur_am.len() || cur_am.is_empty() {
        return Ok(false);
    }
    let mut flipped = 0;
    for (c, n) in cur_am.iter().zip(cand_am.iter()) {
        if c == n {
            continue;
        }
        let same_entry = str_field(c, "proposed_by") == str_field(n, "proposed_by")
            && field(c, "at_checkpoint") == field(n, "at_checkpoint")
            && str_field(c, "resume_status") == str_field(n, "resume_status");
        let newly_approved = is_null_field(c, "approved_by")
            && str_field(n, "approved_by") == writer_role
            && field(n, "approved_at_checkpoint").and_then(|v| v.as_u64()) == Some(checkpoint)
            && str_field(n, "resume_status") == new_status;
        if same_entry && newly_approved {
            flipped += 1;
        } else {
            return Ok(false);
        }
    }
    if flipped == 1 {
        custom_invariants_ok(cand).map(|()| true)
    } else {
        Ok(false)
    }
}

// ===========================================================================
// edge whitelist
// ===========================================================================
fn edge_whitelist(
    graph: &EffectiveGraph,
    cur_mode: &str,
    new_profile: &str,
    cur_status: &str,
    new_status: &str,
    reason: &mut String,
) -> bool {
    // Legality is drawn from the resolved graph, not a hardcoded profile match:
    // a custom v3 graph legalizes exactly its declared edges; a preset/v2 graph
    // legalizes the `(mode, profile)` preset (presets.rs is the single source).
    let legal = match graph {
        EffectiveGraph::Declared(edges) => edges
            .iter()
            .any(|e| e.from == cur_status && e.to == new_status),
        EffectiveGraph::Preset => preset_edge_legal(cur_mode, new_profile, cur_status, new_status),
        EffectiveGraph::None => false,
    };
    if !legal {
        *reason = format!("no legal edge {cur_status}->{new_status}");
    }
    legal
}

// ===========================================================================
// post-legality edge gates
// ===========================================================================
/// S5-T3 (D5): the `work_split_waiver` object is additive/nullable. Absent/null
/// = no waiver; a well-formed `{reason: nonblank string, approved_by:
/// "prativadi", checkpoint: number}` = a valid waiver; anything else present is
/// malformed. Consulted only at the two chunk-floor gates; ignored elsewhere.
#[derive(PartialEq)]
enum WaiverState {
    Absent,
    Valid,
    Malformed,
}

fn work_split_waiver_state(cand: &Value) -> WaiverState {
    match field(cand, "work_split_waiver") {
        None | Some(Value::Null) => WaiverState::Absent,
        Some(Value::Object(m)) => {
            let reason_ok = matches!(m.get("reason"), Some(Value::String(s)) if s.chars().any(|c| !c.is_whitespace()));
            let approved_ok =
                matches!(m.get("approved_by"), Some(Value::String(s)) if s == "prativadi");
            let checkpoint_ok = matches!(m.get("checkpoint"), Some(Value::Number(_)));
            if reason_ok && approved_ok && checkpoint_ok {
                WaiverState::Valid
            } else {
                WaiverState::Malformed
            }
        }
        _ => WaiverState::Malformed,
    }
}

/// S5-T3: the parallel-work-split floor is ≥2 write-capable implementation
/// chunks PER ROLE, AND (≥5 chunks total OR a valid `work_split_waiver`). The
/// per-role floor is never waived — only the ≥5 total is.
fn parallel_work_split_ok(cand: &Value, waived: bool) -> bool {
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
    let vadi = chunks
        .iter()
        .filter(|c| owner_role_or_owner(c) == "vadi")
        .count();
    let prativadi = chunks
        .iter()
        .filter(|c| owner_role_or_owner(c) == "prativadi")
        .count();
    vadi >= 2 && prativadi >= 2 && (chunks.len() >= 5 || waived)
}

fn track_owner_role_or_role(t: &Value) -> String {
    let r = str_field(t, "owner_role");
    if !r.is_empty() {
        return r;
    }
    str_field(t, "role")
}

/// S5-T3: the same chunk-floor waiver rule applies to the
/// parallel_implementing->test_creation evidence floor — ≥2 completed
/// implementation-chunk tracks PER ROLE, AND (≥5 tracks total OR a valid
/// `work_split_waiver`).
fn parallel_to_test_creation_ok(cand: &Value, waived: bool) -> bool {
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
    let vadi = tracks
        .iter()
        .filter(|t| track_owner_role_or_role(t) == "vadi")
        .count();
    let prativadi = tracks
        .iter()
        .filter(|t| track_owner_role_or_role(t) == "prativadi")
        .count();
    vadi >= 2 && prativadi >= 2 && (tracks.len() >= 5 || waived)
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
    baton_dir: &Path,
    cur_doc: &Value,
    current_checkpoint: i64,
    want_phase: bool,
) -> Vec<CycleRow> {
    let mut rows = Vec::new();
    let history_dir = baton_dir.join("history");
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
    if let Some(row) = cycle_row(cur_doc, want_phase) {
        rows.push(row);
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
fn cross_or_deep_cycle(
    baton_dir: &Path,
    cur_doc: &Value,
    current_checkpoint: i64,
    target: &str,
) -> i64 {
    let mut rows = collect_cycle_rows(baton_dir, cur_doc, current_checkpoint, false);
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

fn cross_review_cycle_checkpoint(
    baton_dir: &Path,
    cur_doc: &Value,
    current_checkpoint: i64,
) -> i64 {
    cross_or_deep_cycle(baton_dir, cur_doc, current_checkpoint, "cross_review")
}

fn deep_review_cycle_checkpoint(baton_dir: &Path, cur_doc: &Value, current_checkpoint: i64) -> i64 {
    cross_or_deep_cycle(baton_dir, cur_doc, current_checkpoint, "deep_review")
}

/// F10: the checkpoint at which the run entered its current `termination_review`
/// block — the floor for accepting an explainer-verification track as
/// current-cycle. Reuses the same contiguous-run scan as the cross/deep helpers.
fn termination_review_cycle_checkpoint(
    baton_dir: &Path,
    cur_doc: &Value,
    current_checkpoint: i64,
) -> i64 {
    cross_or_deep_cycle(baton_dir, cur_doc, current_checkpoint, "termination_review")
}

fn phase_review_cycle_checkpoint(
    baton_dir: &Path,
    cur_doc: &Value,
    current_checkpoint: i64,
    current_phase: &str,
) -> i64 {
    let mut rows = collect_cycle_rows(baton_dir, cur_doc, current_checkpoint, true);
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

// ===========================================================================
// S4-T1: required done-gate artifacts resolve to real files
// ===========================================================================

/// S4-T1: at a v2 `done` gate, every ref REQUIRED by the candidate's mode/profile
/// must resolve to an existing, non-empty, regular file under the baton's repo
/// root (git toplevel of the baton dir; fall back to the baton dir itself). Refs
/// are repo-root-relative paths, possibly starting `./`. Reason
/// `missing_artifact ref=<field> path=<resolved>` (exit 23).
fn required_done_artifacts_ok(
    baton_dir: &Path,
    cand: &Value,
    cx: &Ctx,
) -> Result<(), (i32, String)> {
    let root = gitcfg::repo_toplevel(baton_dir).unwrap_or_else(|| baton_dir.to_path_buf());
    let check = |field_name: &str| -> Result<(), (i32, String)> {
        let rel = str_field(cand, field_name);
        let rel_trim = rel.strip_prefix("./").unwrap_or(&rel);
        let resolved = root.join(rel_trim);
        let ok = std::fs::metadata(&resolved)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false);
        if ok {
            Ok(())
        } else {
            Err((
                23,
                format!(
                    "DVANDVA_WRITE missing_artifact ref={field_name} path={}",
                    resolved.display()
                ),
            ))
        }
    };
    // research_ref is required at every v2 done gate (all modes/profiles).
    check("research_ref")?;
    match cx.new_effective_mode {
        // Full development done additionally requires the run explainer.
        "development" if cx.new_effective_profile == "full" => check("run_explainer_ref")?,
        // Research done on the seed path additionally requires the plan.
        "research" => {
            if matches!(field(cand, "research_outcome"), Some(Value::String(s)) if s == "seed_development")
            {
                check("plan_ref")?;
            }
        }
        // Review done additionally requires the review artifact.
        "review" => check("review_ref")?,
        _ => {}
    }
    Ok(())
}

// ===========================================================================
// S4-T6 (D3): verification_matrix freshness
// ===========================================================================

/// The implementation-family freshness anchor: the max checkpoint across history
/// and current entries whose status is one of the implementation-family statuses
/// `phase_fixing` / `implementing` / `parallel_implementing` / `cross_fixing`.
/// Returns `0` when no such entry exists, which imposes no floor.
fn implementation_family_anchor(baton_dir: &Path, cur_doc: &Value, current_checkpoint: i64) -> i64 {
    collect_cycle_rows(baton_dir, cur_doc, current_checkpoint, false)
        .iter()
        .filter(|r| {
            matches!(
                r.status.as_str(),
                "phase_fixing" | "implementing" | "parallel_implementing" | "cross_fixing"
            )
        })
        .map(|r| r.checkpoint)
        .max()
        .unwrap_or(0)
}

/// A verification_matrix row's identity for the `stale_verification_matrix`
/// reason: its `id` field, else the supplied array-index / object-key fallback.
fn matrix_row_label(row: &Value, fallback: &str) -> String {
    match field(row, "id") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => fallback.to_string(),
    }
}

/// A row is complete when its coalesced `current // result` is passed/approved
/// (case-insensitive).
fn matrix_row_complete(row: &Value) -> bool {
    let result =
        util::coalesce(field(row, "current")).or_else(|| util::coalesce(field(row, "result")));
    matches!(result, Some(Value::String(s)) if {
        let s = s.to_ascii_lowercase();
        s == "passed" || s == "approved"
    })
}

/// A row's numeric evidence checkpoint: coalesced `evidence_checkpoint //
/// review_checkpoint`. `None` when neither is a number.
fn matrix_row_checkpoint(row: &Value) -> Option<i64> {
    for key in ["evidence_checkpoint", "review_checkpoint"] {
        if let Some(Value::Number(n)) = util::coalesce(field(row, key)) {
            if let Some(i) = n.as_i64() {
                return Some(i);
            }
        }
    }
    None
}

/// S4-T6: the first stale verification_matrix row, or `None` when every row is
/// fresh. A row is fresh iff it carries a numeric checkpoint `>= anchor` and (for
/// full profile, `require_complete`) is complete. Compact profile enforces
/// completeness earlier (good_matrix), so only the freshness qualifier is added.
/// Object matrices are checked over all values (labelled by key).
fn stale_verification_matrix_row(
    cand: &Value,
    anchor: i64,
    require_complete: bool,
) -> Option<String> {
    let rows: Vec<(String, &Value)> = match field(cand, "verification_matrix") {
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (matrix_row_label(v, &i.to_string()), v))
            .collect(),
        Some(Value::Object(m)) => m.iter().map(|(k, v)| (matrix_row_label(v, k), v)).collect(),
        _ => return None,
    };
    for (label, row) in rows {
        let fresh = matrix_row_checkpoint(row)
            .map(|c| c >= anchor)
            .unwrap_or(false);
        let complete = !require_complete || matrix_row_complete(row);
        if !fresh || !complete {
            return Some(label);
        }
    }
    None
}

// ===========================================================================
// S4-T4: lost_update superset guard (team-owned current status)
// ===========================================================================

/// The `id`-set of an array-or-object of entries (subagent_tracks /
/// agent_instances / work_split), keyed by each entry's `id` string field.
fn id_set(v: Option<&Value>) -> Vec<String> {
    iter_values(v)
        .iter()
        .filter_map(|e| match field(e, "id") {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        })
        .collect()
}

/// The identity-set of `findings`: string entries by their exact value, object
/// entries by their `id` string field.
fn findings_ids(v: Option<&Value>) -> Vec<String> {
    arr(v)
        .iter()
        .filter_map(|e| match e {
            Value::String(s) => Some(s.clone()),
            Value::Object(_) => match field(e, "id") {
                Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// S4-T4: when the CURRENT status is team-owned, the candidate's peer-data
/// id-sets must each remain a SUPERSET of the installed baton's; the first
/// dropped id yields the `lost_update field=<f> missing=<id>` reason (empty
/// string when clean).
fn lost_update_violation(cur_doc: &Value, cand: &Value) -> String {
    let checks: [(&str, Vec<String>, Vec<String>); 4] = [
        (
            "subagent_tracks",
            id_set(field(cur_doc, "subagent_tracks")),
            id_set(field(cand, "subagent_tracks")),
        ),
        (
            "agent_instances",
            id_set(field(cur_doc, "agent_instances")),
            id_set(field(cand, "agent_instances")),
        ),
        (
            "work_split",
            id_set(field(cur_doc, "work_split")),
            id_set(field(cand, "work_split")),
        ),
        (
            "findings",
            findings_ids(field(cur_doc, "findings")),
            findings_ids(field(cand, "findings")),
        ),
    ];
    for (name, installed, candidate) in &checks {
        if let Some(missing) = installed.iter().find(|id| !candidate.contains(id)) {
            return format!("lost_update field={name} missing={missing}");
        }
    }
    String::new()
}

// ===========================================================================
// Unit tests for the pub(crate) transition surface (in-crate: they exercise
// pub(crate) items that integration tests cannot see).
// ===========================================================================
#[cfg(test)]
mod surface_tests {
    use super::*;
    use serde_json::json;

    const V2_SEED: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../plugins/dvandva/references/baton-schema-v2.json"
    ));

    /// A valid full-profile v2 development baton, mirroring the integration
    /// suite's `make_baton_v2` field set.
    fn seed_v2(status: &str, assignee: &str, checkpoint: i64, phase: Value) -> Value {
        let mut b: Value = serde_json::from_str(V2_SEED).expect("v2 seed parses");
        b["updated_at"] = json!("2026-06-27T00:00:00Z");
        b["status"] = json!(status);
        b["assignee"] = json!(assignee);
        b["checkpoint"] = json!(checkpoint);
        b["phase"] = phase;
        b["run_id"] = json!("run-a");
        b["original_ask"] = json!("Original user ask for surface parity");
        b["research_ref"] = json!("./superpowers/research/run-a.html");
        b["profile"] = json!("full");
        b["profile_floor"] = json!("full");
        b["profile_decision"] = json!({
            "selected_profile": "full",
            "floor": "full",
            "reason": "surface test default preserves the full v2 graph",
            "decided_by": "test-suite",
            "decided_at": "2026-07-01T00:00:00Z",
            "risk_inputs": [],
            "hard_triggers": [],
            "allowlist_match": false,
            "allowlist_refs": [],
            "evidence_refs": ["test-helper"]
        });
        b["profile_history"] = json!([]);
        b["current_engine"] = json!("codex");
        b["branch"] = json!("test-branch");
        b["master_plan_locked"] = json!(false);
        b["question"] = Value::Null;
        b["resume_assignee"] = Value::Null;
        b["resume_status"] = Value::Null;
        b
    }

    fn minimal_run_workflow() -> Value {
        json!({
            "source": "preset:full",
            "declared_by": "vadi",
            "declared_at_checkpoint": 0,
            "approved_by": null,
            "approved_at_checkpoint": null,
            "revision_round": 0,
            "states": [],
            "edges": [],
            "amendments": []
        })
    }

    fn seed_v3(status: &str, assignee: &str, checkpoint: i64, phase: Value) -> Value {
        let mut b = seed_v2(status, assignee, checkpoint, phase);
        b["schema"] = json!("dvandva.baton.v3");
        b["run_workflow"] = minimal_run_workflow();
        b
    }

    #[test]
    fn legal_transitions_deep_review_full_option_set() {
        let cur = json!({
            "schema": "dvandva.baton.v2",
            "status": "deep_review",
            "mode": "development",
            "profile": "full",
            "phase": 1,
            "master_plan_locked": true,
            "loop_counts": {},
            "disagreement_cap": 3
        });
        let mut opts = legal_transitions(&cur);
        opts.sort_by(|a, b| a.to_status.cmp(&b.to_status));

        let statuses: Vec<&str> = opts.iter().map(|o| o.to_status.as_str()).collect();
        assert_eq!(
            statuses,
            vec![
                "deslop",
                "human_decision",
                "phase_fixing",
                "review_of_review"
            ],
            "deep_review (full) legal option set"
        );

        let by = |s: &str| opts.iter().find(|o| o.to_status == s).unwrap();
        // Non-loop review targets stay in the same numeric phase.
        assert_eq!(by("deslop").to_phase, PhaseMove::Same);
        assert_eq!(by("deslop").assignee, "vadi");
        assert!(by("deslop").loop_key.is_none());
        // review_of_review carries the prativadi_fixups review target.
        assert_eq!(
            by("review_of_review").review_target.as_deref(),
            Some("prativadi_fixups")
        );
        // The fixing loop edge carries loop_key = (edge, next=1, cap=3).
        assert_eq!(
            by("phase_fixing").loop_key,
            Some(("deep_review:phase_fixing".to_string(), 1, 3))
        );
        // Universal escalation is always offered (never from a terminal state).
        assert_eq!(by("human_decision").assignee, "human");
    }

    #[test]
    fn legal_transitions_at_loop_cap_drops_the_loop_edge() {
        let cur = json!({
            "schema": "dvandva.baton.v2",
            "status": "deep_review",
            "mode": "development",
            "profile": "full",
            "phase": 1,
            "master_plan_locked": true,
            "loop_counts": {"deep_review:phase_fixing": 3},
            "disagreement_cap": 3
        });
        let opts = legal_transitions(&cur);
        assert!(
            !opts.iter().any(|o| o.to_status == "phase_fixing"),
            "the fixing loop edge is dropped at the disagreement cap"
        );
        // The non-loop review edges and human_decision remain.
        assert!(opts.iter().any(|o| o.to_status == "deslop"));
        assert!(opts.iter().any(|o| o.to_status == "human_decision"));
    }

    #[test]
    fn legal_transitions_advance_pins_entry_state_to_target_phase_profile() {
        // A full phase's deslop advancing into a phase_profiles-overridden
        // STANDARD next phase (2) must offer only `implementing` (not
        // `parallel_implementing`).
        let cur = json!({
            "schema": "dvandva.baton.v2",
            "status": "deslop",
            "mode": "development",
            "profile": "full",
            "phase": 1,
            "phase_profiles": {"2": "standard"},
            "master_plan_locked": true,
            "loop_counts": {},
            "disagreement_cap": 3
        });
        let opts = legal_transitions(&cur);
        let advance: Vec<&str> = opts
            .iter()
            .filter(|o| o.to_phase == PhaseMove::Advance)
            .map(|o| o.to_status.as_str())
            .collect();
        assert_eq!(
            advance,
            vec!["implementing"],
            "full deslop -> standard next phase offers only implementing"
        );
        assert!(!opts.iter().any(|o| o.to_status == "parallel_implementing"));

        // Symmetric: a standard phase's phase_review advancing into a FULL next
        // phase (2) must offer only `parallel_implementing`.
        let cur2 = json!({
            "schema": "dvandva.baton.v2",
            "status": "phase_review",
            "mode": "development",
            "profile": "standard",
            "phase": 1,
            "phase_profiles": {"2": "full"},
            "master_plan_locked": true,
            "loop_counts": {},
            "disagreement_cap": 3
        });
        let opts2 = legal_transitions(&cur2);
        let advance2: Vec<&str> = opts2
            .iter()
            .filter(|o| o.to_phase == PhaseMove::Advance)
            .map(|o| o.to_status.as_str())
            .collect();
        assert_eq!(
            advance2,
            vec!["parallel_implementing"],
            "standard phase_review -> full next phase offers only parallel_implementing"
        );
        assert!(!opts2.iter().any(|o| o.to_status == "implementing"));
    }

    /// Reject-parity for a retired v2 candidate (23): validate_candidate and the
    /// spawned binary agree on the schema_retired exit code.
    #[test]
    fn validate_candidate_parity_rejects_v2_candidate_schema_retired_23() {
        let dir = tempfile::tempdir().unwrap();
        let baton_dir = dir.path();
        let baton_file = baton_dir.join("baton.json");
        let cand_file = baton_dir.join("baton.next.json");

        let cur = seed_v3("spec_drafting", "vadi", 4, json!("spec"));
        let cand = seed_v2("spec_review", "prativadi", 5, json!("spec"));
        std::fs::write(&baton_file, serde_json::to_string(&cur).unwrap()).unwrap();
        std::fs::write(&cand_file, serde_json::to_string(&cand).unwrap()).unwrap();

        let vc = validate_candidate(baton_dir, Some(&cur), &cand);
        assert!(matches!(vc, Err((23, _))), "expected 23, got {vc:?}");
        assert_eq!(run_write(&baton_file, &cand_file), 23);
    }

    /// Reject-parity for an unparseable-strict current baton (25): a valid-JSON
    /// current baton with a non-numeric checkpoint trips the strict current-baton
    /// guard identically in validate_candidate and the binary.
    #[test]
    fn validate_candidate_parity_rejects_unparseable_current_25() {
        let dir = tempfile::tempdir().unwrap();
        let baton_dir = dir.path();
        let baton_file = baton_dir.join("baton.json");
        let cand_file = baton_dir.join("baton.next.json");

        let mut cur = seed_v3("spec_drafting", "vadi", 4, json!("spec"));
        cur["checkpoint"] = json!("not-a-number");
        let cand = seed_v3("spec_review", "prativadi", 5, json!("spec"));
        std::fs::write(&baton_file, serde_json::to_string(&cur).unwrap()).unwrap();
        std::fs::write(&cand_file, serde_json::to_string(&cand).unwrap()).unwrap();

        let vc = validate_candidate(baton_dir, Some(&cur), &cand);
        assert!(matches!(vc, Err((25, _))), "expected 25, got {vc:?}");
        assert_eq!(run_write(&baton_file, &cand_file), 25);
    }

    /// Reject-parity for a stale checkpoint (27): a candidate at the same
    /// checkpoint as the current baton is rejected identically.
    #[test]
    fn validate_candidate_parity_rejects_stale_checkpoint_27() {
        let dir = tempfile::tempdir().unwrap();
        let baton_dir = dir.path();
        let baton_file = baton_dir.join("baton.json");
        let cand_file = baton_dir.join("baton.next.json");

        let cur = seed_v3("spec_drafting", "vadi", 5, json!("spec"));
        let cand = seed_v3("spec_review", "prativadi", 5, json!("spec"));
        std::fs::write(&baton_file, serde_json::to_string(&cur).unwrap()).unwrap();
        std::fs::write(&cand_file, serde_json::to_string(&cand).unwrap()).unwrap();

        let vc = validate_candidate(baton_dir, Some(&cur), &cand);
        assert!(matches!(vc, Err((27, _))), "expected 27, got {vc:?}");
        assert_eq!(run_write(&baton_file, &cand_file), 27);
    }

    #[test]
    fn validate_candidate_parity_accepts_and_binary_installs() {
        let dir = tempfile::tempdir().unwrap();
        let baton_dir = dir.path();
        let baton_file = baton_dir.join("baton.json");
        let cand_file = baton_dir.join("baton.next.json");

        let cur = seed_v3("spec_drafting", "vadi", 4, json!("spec"));
        let cand = seed_v3("spec_review", "prativadi", 5, json!("spec"));
        std::fs::write(&baton_file, serde_json::to_string(&cur).unwrap()).unwrap();
        std::fs::write(&cand_file, serde_json::to_string(&cand).unwrap()).unwrap();

        // validate_candidate accepts it...
        assert!(
            validate_candidate(baton_dir, Some(&cur), &cand).is_ok(),
            "validate_candidate should accept a legal spec_drafting->spec_review"
        );
        // ...and the binary write installs it (exit 0). Parity.
        assert_eq!(run_write(&baton_file, &cand_file), 0);
    }

    #[test]
    fn validate_candidate_parity_rejects_illegal_edge_same_code() {
        let dir = tempfile::tempdir().unwrap();
        let baton_dir = dir.path();
        let baton_file = baton_dir.join("baton.json");
        let cand_file = baton_dir.join("baton.next.json");

        let cur = seed_v3("spec_drafting", "vadi", 4, json!("spec"));
        // spec_drafting -> implementing is not a legal edge (24).
        let cand = seed_v3("implementing", "vadi", 5, json!(1));
        std::fs::write(&baton_file, serde_json::to_string(&cur).unwrap()).unwrap();
        std::fs::write(&cand_file, serde_json::to_string(&cand).unwrap()).unwrap();

        let vc = validate_candidate(baton_dir, Some(&cur), &cand);
        assert!(
            matches!(vc, Err((24, _))),
            "expected illegal-transition 24, got {vc:?}"
        );
        // The binary emits the same exit code.
        assert_eq!(run_write(&baton_file, &cand_file), 24);
    }

    #[test]
    fn expected_owner_team_states_carry_both_roles() {
        let (a, roles) = expected_owner("dvandva.baton.v2", "development", "full", "cross_review");
        assert_eq!(a, "team");
        assert_eq!(roles, vec!["prativadi".to_string(), "vadi".to_string()]);
        let (a, roles) = expected_owner(
            "dvandva.baton.v2",
            "development",
            "standard",
            "implementing",
        );
        assert_eq!(a, "vadi");
        assert!(roles.is_empty());
    }
}
