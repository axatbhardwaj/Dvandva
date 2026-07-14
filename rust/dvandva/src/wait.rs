//! Cheap foreground wait for Dvandva baton ownership (`dvandva wait`).
//!
//! Port of `plugins/dvandva/skills/vadi/scripts/dvandva-wait.sh`. By default the
//! wait is *continuous*: `--max-wait` is a heartbeat interval, not a stop, and
//! the loop keeps polling until this role owns the baton, the baton reaches
//! post-handshake `done`, it enters `human_question` / `human_decision`, a
//! sibling run demands a paired stop, a cap fires, or the user interrupts.
//!
//! Watch technique: the shell woke early on `inotifywait` events against the
//! baton *directory* (an atomic tmp+rename changes the inode, orphaning a file
//! watch). Here the `notify` crate watches the same directory set, falling back
//! to interval sleep when the watcher cannot start. Events are only an
//! optimization — every wake re-reads state and the interval poll stays
//! authoritative.
//!
//! Exit codes (protocol surface, never unified with sibling helpers):
//! `0` assigned/actionable · `10` done · `11` human_decision · `12`
//! human_question · `13` abandoned · `14` discover_ambiguous · `15` human_gate
//! · `20` finite timeout · `21` baton missing · `22` invalid JSON · `23`
//! persist-max · `24` stall-max · `29` split-brain · `2` usage.
//!
//! Status classification is [`StateClass`]-driven (see [`resolve_status_class`])
//! for BOTH the selected baton's own status and every sibling run scanned for
//! human-pause propagation / split-brain: a v3 baton resolves its current
//! status's class from its own `run_workflow` (custom -> declared `states[]`,
//! `preset:*` -> the resolved preset), a v1/v2 baton from the static token map
//! ([`workflow::static_class`]). For the selected baton, the class selects the
//! exit: `Terminal` -> 10/13, `Pause` -> 11/12, `HumanGate` -> 15, `Work`/
//! `ReviewGate` -> the generic heartbeat path. For a sibling (see
//! [`scan_sibling_runs`]), `Terminal` is skipped, `Pause` and `HumanGate` both
//! propagate a human pause (a human is needed either way), and `Work`/
//! `ReviewGate` are active/split-brain candidates.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde_json::Value;
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

use crate::util::{coalesce, is_open_finding_status, now_epoch, read_json_lenient};
use crate::workflow::{self, StateClass};

/// A fully-resolved wait invocation. Built by the `cmd::wait` wrapper after
/// selector/env resolution (including resolver delegation for the legacy
/// default) so the loop here is pure given a concrete baton path.
#[derive(Debug, Clone)]
pub struct WaitConfig {
    /// The `--role` value, verbatim (`vadi` / `prativadi`).
    pub role: String,
    /// Resolved baton path (relative paths are opened against the process cwd).
    pub baton_file: String,
    /// Selector provenance surfaced in heartbeat metadata: `legacy`, `env_file`,
    /// `run_dir`, `run_id`, `resolve`, or `resolve_create`. Note `--file` does
    /// not change this (it matches the shell's pre-arg-parse capture).
    pub selected_by: String,
    pub interval: u64,
    pub max_wait: u64,
    pub allow_missing: bool,
    pub persist: bool,
    pub persist_max: u64,
    pub stall_max: u64,
    pub since_checkpoint: Option<u64>,
    pub until_actionable: bool,
    /// `DVANDVA_CONCURRENT=1`: suppress sibling-run split-brain / paired-stop.
    pub concurrent: bool,
    /// `--through-human`: keep polling THROUGH human_question/human_decision
    /// pauses (own or a newer paired sibling's) instead of exiting 11/12.
    /// Each pause episode prints one note line, then normal wait logic
    /// resumes as soon as the pause clears.
    pub through_human: bool,
    /// `--discover`: run the adopt-and-continue discovery preamble (see
    /// [`run_discovery`]) before the normal wait loop instead of requiring a
    /// resolved baton path up front. Mutually exclusive with `--file`
    /// (enforced by the `cmd::wait` wrapper).
    pub discover: bool,
}

/// Drive the wait loop to a terminal exit code.
pub fn run(cfg: &WaitConfig) -> i32 {
    let mut elapsed: u64 = 0;
    let persist_started = now_epoch();
    // Dead-peer stall watchdog: wall-clock since the baton last advanced, reset
    // whenever the observed checkpoint changes. First read (empty baseline) also
    // resets. `--stall-max 0` disables it.
    let mut stall_started = now_epoch();
    let mut stall_last_checkpoint: Option<String> = None;
    // --through-human: in-process once-per-episode note bookkeeping.
    // The own-baton pause is keyed by (status, checkpoint); a
    // sibling-propagated pause also carries the sibling run id (a different
    // sibling could otherwise collide on the same (status, checkpoint) pair).
    // This is the fast path — it never touches disk on a repeat heartbeat.
    // [`mark_pause_episode_if_new`] additionally consults a marker file on
    // the *first* in-process observation of a key, catching the case where an
    // earlier process already noted the same episode before a shell-budget
    // re-entry.
    let mut through_human_selected_episode: Option<(String, String)> = None;
    let mut through_human_sibling_episode: Option<(String, String, String)> = None;

    // `--discover`: adopt-and-continue bootstrap. Runs before the baton is
    // known at all, so it cannot reuse `cfg.baton_file`; the stall/persist
    // clocks above already started at wait entry, so they cover this period
    // too. `discovered_cfg` outlives the `if` so the reborrow below can point
    // at it for the rest of the function.
    let discovered_cfg;
    let cfg: &WaitConfig = if cfg.discover {
        match run_discovery(cfg, persist_started, stall_started) {
            DiscoverOutcome::Adopted { file, run_id } => {
                println!("DVANDVA_WAIT discovered file={file} run_id={run_id}");
                discovered_cfg = WaitConfig {
                    baton_file: file,
                    ..cfg.clone()
                };
                &discovered_cfg
            }
            DiscoverOutcome::Exit(code) => return code,
        }
    } else {
        cfg
    };

    loop {
        let mut wait_detail = String::new();
        // Set inside the --through-human pause branches below (own or
        // sibling-propagated). Suspends the stall-max check for this
        // iteration's `record_wait_elapsed` call: resetting `stall_started`
        // to "now" in those branches is not enough by itself, since one full
        // `--interval` still elapses between that reset and the check below
        // it, which fires immediately whenever `stall_max <= interval`.
        let mut through_human_paused = false;
        let baton_path = Path::new(&cfg.baton_file);
        let is_file = std::fs::metadata(baton_path)
            .map(|m| m.is_file())
            .unwrap_or(false);

        if !is_file {
            let selected_run_id = derive_run_id(&cfg.baton_file, "");
            let scan = scan_sibling_runs(cfg, "", "");
            if cfg.allow_missing {
                if elapsed >= cfg.max_wait {
                    if cfg.persist {
                        if cfg.interval == 0 {
                            eprintln!("ERROR: continuous wait mode requires --interval > 0 when the baton is not ready; use --finite for an immediate heartbeat");
                            return 2;
                        }
                        println!(
                            "DVANDVA_WAIT heartbeat role={} waiting_for=baton {} elapsed={elapsed}s",
                            cfg.role,
                            heartbeat_meta(cfg, &selected_run_id, scan.active_count),
                        );
                        elapsed = 0;
                        wait_one_interval(cfg);
                        if let Some(code) = record_wait_elapsed(
                            cfg,
                            &mut elapsed,
                            persist_started,
                            stall_started,
                            through_human_paused,
                        ) {
                            return code;
                        }
                        continue;
                    }
                    println!(
                        "DVANDVA_WAIT timeout role={} waiting_for=baton file={} elapsed={elapsed}s",
                        cfg.role, cfg.baton_file
                    );
                    return 20;
                }
                wait_one_interval(cfg);
                if let Some(code) = record_wait_elapsed(
                    cfg,
                    &mut elapsed,
                    persist_started,
                    stall_started,
                    through_human_paused,
                ) {
                    return code;
                }
                continue;
            }
            println!("DVANDVA_WAIT missing file={}", cfg.baton_file);
            return 21;
        }

        // Torn-read tolerance: a concurrent writer may be mid-replace. One 1s retry.
        let value = match read_json_lenient(baton_path) {
            Ok(value) => value,
            Err(_) => {
                std::thread::sleep(Duration::from_secs(1));
                match read_json_lenient(baton_path) {
                    Ok(value) => value,
                    Err(_) => {
                        println!("DVANDVA_WAIT invalid_json file={}", cfg.baton_file);
                        return 22;
                    }
                }
            }
        };

        let baton_run_id = field_str(&value, "run_id");
        let assignee = field_str(&value, "assignee");
        let status = field_str(&value, "status");
        let phase = field_str(&value, "phase");
        let checkpoint = checkpoint_str(&value);
        let question = field_str(&value, "question");
        let resume_assignee = field_str(&value, "resume_assignee");
        let resume_status = field_str(&value, "resume_status");
        let active_roles = active_roles_csv(&value);
        let updated_at = field_str(&value, "updated_at");
        let current_engine = field_str(&value, "current_engine");

        // Reset the stall watchdog whenever the baton advances (checkpoint change
        // is progress); the empty first-read baseline also resets.
        if stall_last_checkpoint.as_deref() != Some(checkpoint.as_str()) {
            stall_last_checkpoint = Some(checkpoint.clone());
            stall_started = now_epoch();
        }

        let selected_run_id = derive_run_id(&cfg.baton_file, &baton_run_id);
        // Path is authoritative; a `.run_id` field disagreeing with the directory
        // is surfaced (not trusted) so it drives no logic.
        let run_id_note = if !baton_run_id.is_empty() && baton_run_id != selected_run_id {
            format!(" run_id_field_mismatch={baton_run_id}")
        } else {
            String::new()
        };

        // Classify the current status, then dispatch by class. v3 batons read
        // the class from their own run_workflow; v1/v2 batons from the static
        // token map (which retroactively maps the clarifying-answer states to
        // HumanGate — the F5 fix).
        let status_class = resolve_status_class(&value, &status);
        match status_class {
            StateClass::Terminal => {
                // `done` -> 10, `abandoned` -> 13, both with their legacy line
                // grammar. A DECLARED v3 terminal that is neither legacy token
                // is treated as abandoned-equivalent (exit 13) — the honest
                // "this run is over, and not via the done handshake" outcome.
                if status == "done" {
                    println!("DVANDVA_WAIT done phase={phase} checkpoint={checkpoint} assignee={assignee}");
                    return 10;
                }
                println!("DVANDVA_WAIT abandoned phase={phase} checkpoint={checkpoint} assignee={assignee}");
                return 13;
            }
            StateClass::HumanGate | StateClass::Pause if cfg.through_human => {
                // --through-human passive watch: HumanGate participates exactly
                // like a Pause (human_question) does — suspend the stall
                // watchdog for the pause's duration (it resumes counting from
                // "now" the instant the gate/pause clears), note the episode at
                // most once, then fall through and keep polling. HumanGate does
                // NOT exit 15 here: --through-human means the surfacing is being
                // handled out of band, so we wait it out like any human pause.
                stall_started = now_epoch();
                through_human_paused = true;
                let episode = (status.clone(), checkpoint.clone());
                if through_human_selected_episode.as_ref() != Some(&episode) {
                    through_human_selected_episode = Some(episode);
                    let marker_key = format!("own status={status} checkpoint={checkpoint}");
                    if mark_pause_episode_if_new(cfg, &marker_key) {
                        eprintln!(
                            "DVANDVA_WAIT note human_pause status={status} checkpoint={checkpoint}"
                        );
                    }
                }
                // Fall through: no return, keep polling at the normal
                // interval/heartbeat cadence below.
            }
            StateClass::HumanGate => {
                // F5: a human-assigned gate (e.g. clarifying_questions_answer)
                // must wake the role that surfaces it to the human. A plain
                // exit is correct — surfacing is an action the role must take,
                // so there are no once-per-episode semantics to preserve here.
                println!("DVANDVA_WAIT human_gate status={status} checkpoint={checkpoint}");
                return 15;
            }
            StateClass::Pause => {
                // `human_question` -> 12 (with its resume/question fields),
                // `human_decision` -> 11. A DECLARED v3 pause state that is
                // neither legacy token is treated as human_decision-equivalent
                // (exit 11) — the generic "stopped for a human" outcome.
                if status == "human_question" {
                    println!("DVANDVA_WAIT human_question phase={phase} checkpoint={checkpoint} assignee={assignee} resume_assignee={resume_assignee} resume_status={resume_status} question={question}");
                    return 12;
                }
                println!("DVANDVA_WAIT human_decision phase={phase} checkpoint={checkpoint} assignee={assignee}");
                return 11;
            }
            // Work / ReviewGate: fall through to the generic heartbeat path.
            StateClass::Work | StateClass::ReviewGate => {}
        }

        let mut sibling_active_count = 0;
        if cfg.persist {
            let scan = scan_sibling_runs(cfg, &assignee, &updated_at);
            sibling_active_count = scan.active_count;
            match scan.human_status.as_deref() {
                Some("human_decision") if cfg.through_human => {
                    stall_started = now_epoch();
                    through_human_paused = true;
                    let episode = (
                        scan.human_run_id.clone(),
                        "human_decision".to_string(),
                        scan.human_checkpoint.clone(),
                    );
                    if through_human_sibling_episode.as_ref() != Some(&episode) {
                        through_human_sibling_episode = Some(episode);
                        let marker_key = format!(
                            "sibling run_id={} status=human_decision checkpoint={}",
                            scan.human_run_id, scan.human_checkpoint
                        );
                        if mark_pause_episode_if_new(cfg, &marker_key) {
                            eprintln!(
                                "DVANDVA_WAIT note human_pause status=human_decision checkpoint={} sibling_run_id={}",
                                scan.human_checkpoint, scan.human_run_id
                            );
                        }
                    }
                }
                Some("human_question") if cfg.through_human => {
                    stall_started = now_epoch();
                    through_human_paused = true;
                    let episode = (
                        scan.human_run_id.clone(),
                        "human_question".to_string(),
                        scan.human_checkpoint.clone(),
                    );
                    if through_human_sibling_episode.as_ref() != Some(&episode) {
                        through_human_sibling_episode = Some(episode);
                        let marker_key = format!(
                            "sibling run_id={} status=human_question checkpoint={}",
                            scan.human_run_id, scan.human_checkpoint
                        );
                        if mark_pause_episode_if_new(cfg, &marker_key) {
                            eprintln!(
                                "DVANDVA_WAIT note human_pause status=human_question checkpoint={} sibling_run_id={}",
                                scan.human_checkpoint, scan.human_run_id
                            );
                        }
                    }
                }
                Some("human_decision") => {
                    println!(
                        "DVANDVA_WAIT human_decision role={} selected_run_id={selected_run_id} sibling_run_id={} waiting_on={assignee} phase={phase} status={status} checkpoint={checkpoint} {}{}",
                        cfg.role,
                        scan.human_run_id,
                        heartbeat_meta(cfg, &selected_run_id, scan.active_count),
                        run_id_note,
                    );
                    return 11;
                }
                Some("human_question") => {
                    println!(
                        "DVANDVA_WAIT human_question role={} selected_run_id={selected_run_id} sibling_run_id={} waiting_on={assignee} phase={phase} status={status} checkpoint={checkpoint} resume_assignee={} resume_status={} question={} {}{}",
                        cfg.role,
                        scan.human_run_id,
                        scan.human_resume_assignee,
                        scan.human_resume_status,
                        scan.human_question,
                        heartbeat_meta(cfg, &selected_run_id, scan.active_count),
                        run_id_note,
                    );
                    return 12;
                }
                _ => {}
            }
            if let Some(sibling) = scan.split_brain_run_id.as_deref() {
                println!(
                    "DVANDVA_WAIT split_brain role={} selected_run_id={selected_run_id} sibling_run_id={sibling} waiting_on={assignee} phase={phase} status={status} checkpoint={checkpoint} active_roles={active_roles} {} elapsed={elapsed}s last_seen_engine={current_engine} updated_at={updated_at}{}",
                    cfg.role,
                    heartbeat_meta(cfg, &selected_run_id, scan.active_count),
                    run_id_note,
                );
                return 29;
            }
        }

        // Dispatch-request wake (dr-opus-dispatch-liveness-gap): a role named by
        // an OPEN `dispatch_requests` entry is actionable in an ACTIVE-work
        // state, even when it is neither the assignee nor a member of
        // active_roles. The gate is the SAME `status_class` the exit dispatch
        // above uses: Terminal returned above, but Pause/HumanGate do NOT return
        // under `--through-human` (they fall through to keep watching the human
        // pause), so this line is reached while parked for a human. Firing the
        // wake there would exit 0 mid-pause — breaking zero-touch walkaway watch
        // and risking a duplicate PAID credited-Opus dispatch. Restrict it to
        // Work/ReviewGate so only a genuinely active state (e.g. a Codex-hosted
        // `deep_review` with assignee=prativadi, active_roles=[]) wakes the
        // Claude-side vadi to dispatch the credited Opus reviewers.
        if matches!(status_class, StateClass::Work | StateClass::ReviewGate)
            && role_has_open_dispatch_request(&value, &cfg.role)
        {
            println!(
                "DVANDVA_WAIT dispatch_requested role={} phase={phase} status={status} checkpoint={checkpoint} assignee={assignee} active_roles={active_roles}",
                cfg.role
            );
            return 0;
        }

        if let Some(since) = cfg.since_checkpoint {
            if !is_all_digits(&checkpoint) {
                println!(
                    "DVANDVA_WAIT invalid_checkpoint file={} checkpoint={checkpoint}",
                    cfg.baton_file
                );
                return 22;
            }
            let checkpoint_num: u64 = checkpoint.parse().unwrap_or(0);
            if checkpoint_num > since {
                if cfg.until_actionable {
                    if assignee == cfg.role {
                        println!("DVANDVA_WAIT checkpoint_advanced role={} phase={phase} status={status} checkpoint={checkpoint} since_checkpoint={since} assignee={assignee} active_roles={active_roles}", cfg.role);
                        return 0;
                    }
                    if contains_role(&active_roles, &cfg.role) {
                        if role_has_actionable_work(&value, &cfg.role, &status, &phase) {
                            println!("DVANDVA_WAIT actionable role={} phase={phase} status={status} checkpoint={checkpoint} since_checkpoint={since} assignee={assignee} active_roles={active_roles}", cfg.role);
                            return 0;
                        }
                        wait_detail = no_actionable_detail(&value, &cfg.role, &status, &phase);
                    }
                } else {
                    println!("DVANDVA_WAIT checkpoint_advanced role={} phase={phase} status={status} checkpoint={checkpoint} since_checkpoint={since} assignee={assignee} active_roles={active_roles}", cfg.role);
                    return 0;
                }
            }
        } else {
            if assignee == cfg.role {
                println!("DVANDVA_WAIT ready role={} phase={phase} status={status} checkpoint={checkpoint}", cfg.role);
                return 0;
            }
            if contains_role(&active_roles, &cfg.role) {
                if cfg.until_actionable
                    && !role_has_actionable_work(&value, &cfg.role, &status, &phase)
                {
                    wait_detail = no_actionable_detail(&value, &cfg.role, &status, &phase);
                } else {
                    println!("DVANDVA_WAIT ready role={} phase={phase} status={status} checkpoint={checkpoint} assignee={assignee} active_roles={active_roles}", cfg.role);
                    return 0;
                }
            }
        }

        if elapsed >= cfg.max_wait {
            if cfg.persist {
                if cfg.interval == 0 {
                    eprintln!("ERROR: continuous wait mode requires --interval > 0 when the baton is not ready; use --finite for an immediate heartbeat");
                    return 2;
                }
                println!(
                    "DVANDVA_WAIT heartbeat role={} waiting_on={assignee} phase={phase} status={status} checkpoint={checkpoint} active_roles={active_roles}{wait_detail} {} elapsed={elapsed}s last_seen_engine={current_engine} updated_at={updated_at}{}",
                    cfg.role,
                    heartbeat_meta(cfg, &selected_run_id, sibling_active_count),
                    run_id_note,
                );
                elapsed = 0;
                wait_one_interval(cfg);
                if let Some(code) = record_wait_elapsed(
                    cfg,
                    &mut elapsed,
                    persist_started,
                    stall_started,
                    through_human_paused,
                ) {
                    return code;
                }
                continue;
            }
            println!(
                "DVANDVA_WAIT timeout role={} waiting_on={assignee} phase={phase} status={status} checkpoint={checkpoint} active_roles={active_roles} elapsed={elapsed}s",
                cfg.role
            );
            return 20;
        }

        wait_one_interval(cfg);
        if let Some(code) = record_wait_elapsed(
            cfg,
            &mut elapsed,
            persist_started,
            stall_started,
            through_human_paused,
        ) {
            return code;
        }
    }
}

// ── `--discover` (adopt-and-continue) ───────────────────────────────────────

/// Result of the `--discover` preamble: either exactly one non-terminal baton
/// was found (adopt it and fall through to the normal wait loop) or the loop
/// hit a terminal outcome of its own (an exit code — ambiguous discovery, or
/// a persist-max/stall-max cap firing while still empty-handed).
enum DiscoverOutcome {
    Adopted { file: String, run_id: String },
    Exit(i32),
}

/// A candidate baton seen by the discovery scan, with just enough surfaced
/// to print a `candidate` line if discovery turns out ambiguous.
struct DiscoveryCandidate {
    file: String,
    run_id: String,
    status: String,
    assignee: String,
}

/// Every non-terminal (not `done`/`abandoned`) managed baton under the
/// repo-root `.dvandva` layout — the legacy baton plus `runs/*/baton.json` —
/// via the same file listing [`scan_sibling_runs`] uses. `human_question` /
/// `human_decision` count as active/resumable here, unlike the sibling-scan's
/// separate pause-propagation bucket, because discovery has no "selected"
/// baton yet to propagate a pause *to*.
fn scan_discovery_candidates() -> Vec<DiscoveryCandidate> {
    let mut candidates = Vec::new();
    for file in list_managed_batons(".dvandva") {
        let Ok(value) = read_json_lenient(Path::new(&file)) else {
            continue;
        };
        let status = field_str(&value, "status");
        if matches!(status.as_str(), "done" | "abandoned") {
            continue;
        }
        let run_id = derive_run_id(&file, &field_str(&value, "run_id"));
        let assignee = field_str(&value, "assignee");
        candidates.push(DiscoveryCandidate {
            file,
            run_id,
            status,
            assignee,
        });
    }
    candidates
}

/// Drive the discovery loop: heartbeat (`waiting_on=discovery`) on the
/// `--max-wait` cadence while zero candidates exist, adopt the sole candidate
/// the instant exactly one appears, or exit `14` the instant two or more
/// appear. `persist_started`/`stall_started` are the same clocks the normal
/// loop uses (started at wait entry, before this preamble runs), reused via
/// [`record_wait_elapsed`] so persist-max and stall-max apply across the
/// discovery period exactly like the rest of the wait.
fn run_discovery(cfg: &WaitConfig, persist_started: u64, stall_started: u64) -> DiscoverOutcome {
    let mut elapsed: u64 = 0;
    loop {
        let mut candidates = scan_discovery_candidates();
        match candidates.len() {
            0 => {
                if elapsed >= cfg.max_wait {
                    println!(
                        "DVANDVA_WAIT heartbeat role={} waiting_on=discovery elapsed={elapsed}s",
                        cfg.role
                    );
                    elapsed = 0;
                }
                wait_discovery_interval(cfg);
                if let Some(code) =
                    record_wait_elapsed(cfg, &mut elapsed, persist_started, stall_started, false)
                {
                    return DiscoverOutcome::Exit(code);
                }
            }
            1 => {
                let candidate = candidates.remove(0);
                return DiscoverOutcome::Adopted {
                    file: candidate.file,
                    run_id: candidate.run_id,
                };
            }
            _ => {
                for candidate in &candidates {
                    println!(
                        "DVANDVA_WAIT candidate file={} run_id={} status={} assignee={}",
                        candidate.file, candidate.run_id, candidate.status, candidate.assignee
                    );
                }
                println!("DVANDVA_WAIT discover_ambiguous count={}", candidates.len());
                return DiscoverOutcome::Exit(14);
            }
        }
    }
}

/// Increment the interval accounting, then enforce persist-max and stall-max
/// (both wall-clock). Returns `Some(exit_code)` when a cap fires.
/// `suspend_stall` skips the stall-max check for this call: under
/// `--through-human` the caller resets `stall_started` to "now" for every
/// iteration observed inside a pause, but that reset alone does not stop the
/// check below from firing on the very same pass whenever `stall_max <=
/// interval` (one full interval always elapses between the reset and this
/// call) — the flag closes that gap.
fn record_wait_elapsed(
    cfg: &WaitConfig,
    elapsed: &mut u64,
    persist_started: u64,
    stall_started: u64,
    suspend_stall: bool,
) -> Option<i32> {
    *elapsed += cfg.interval;
    if cfg.persist && cfg.persist_max > 0 {
        let total = now_epoch().saturating_sub(persist_started);
        if total >= cfg.persist_max {
            println!(
                "DVANDVA_WAIT persist_max role={} file={} total_elapsed={total}s persist_max={}s",
                cfg.role, cfg.baton_file, cfg.persist_max
            );
            return Some(23);
        }
    }
    if !suspend_stall && cfg.stall_max > 0 {
        let total = now_epoch().saturating_sub(stall_started);
        if total >= cfg.stall_max {
            println!(
                "DVANDVA_WAIT stalled role={} file={} stall_elapsed={total}s stall_max={}s",
                cfg.role, cfg.baton_file, cfg.stall_max
            );
            return Some(24);
        }
    }
    None
}

fn heartbeat_meta(cfg: &WaitConfig, run_id: &str, sibling_active_count: u64) -> String {
    format!(
        "run_id={run_id} file={} selected_by={} sibling_active_runs={sibling_active_count}",
        cfg.baton_file, cfg.selected_by
    )
}

// ── --through-human cross-process episode marker (best-effort) ──────────────

/// The marker path for a `--through-human` pause episode: a tiny file next to
/// the baton, named for the role, so a `wait` invocation that re-enters after
/// a shell-budget cap (persist-max exit + immediate re-invoke) can see what
/// an earlier invocation already noted and not re-note the same pause.
fn pause_marker_path(cfg: &WaitConfig) -> std::path::PathBuf {
    Path::new(&dirname(&cfg.baton_file)).join(format!(".wait-pause-{}", cfg.role))
}

/// `true` exactly when `key` was not already the last-persisted marker
/// content for this role (and persists it as a side effect). Read/write
/// failures degrade silently to "treat as new" — the in-process episode
/// check at each call site (checked before this function is ever reached)
/// remains the fallback guard against re-noting within one process, so a
/// broken marker file never causes a crash and, at worst, only loses
/// cross-process dedup.
fn mark_pause_episode_if_new(cfg: &WaitConfig, key: &str) -> bool {
    let path = pause_marker_path(cfg);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(key) {
        return false;
    }
    let _ = std::fs::write(&path, key);
    true
}

/// The other coordinating role (`vadi` <-> `prativadi`); anything not `vadi`
/// pairs with `vadi`, matching the shell `peer_role`.
fn peer_role(role: &str) -> &'static str {
    if role == "vadi" {
        "prativadi"
    } else {
        "vadi"
    }
}

/// `contains_role`: membership in the comma-joined `active_roles`, matching the
/// shell `[[ ,csv, == *,role,* ]]`.
fn contains_role(active_roles_csv: &str, role: &str) -> bool {
    format!(",{active_roles_csv},").contains(&format!(",{role},"))
}

fn is_all_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

/// Path-authoritative run id. The on-disk location wins; the `.run_id` field is
/// only a fallback for paths the layout cannot classify.
fn derive_run_id(file: &str, baton_run_id: &str) -> String {
    if file == ".dvandva/baton.json" || file.ends_with("/.dvandva/baton.json") {
        "legacy".to_string()
    } else if let Some(dir) = file.strip_suffix("/baton.json") {
        basename(dir).to_string()
    } else if !baton_run_id.is_empty() {
        baton_run_id.to_string()
    } else {
        "unknown".to_string()
    }
}

/// The `.dvandva` root enclosing a baton path, or `""` for paths outside the
/// managed layout (explicit `--file` / `DVANDVA_RUN_DIR`), which disables
/// sibling scanning. Mirrors the shell `derive_dvandva_root` case arms.
fn derive_dvandva_root(file: &str) -> String {
    if file == ".dvandva/baton.json"
        || (file.starts_with(".dvandva/runs/") && file.ends_with("/baton.json"))
    {
        ".dvandva".to_string()
    } else if let Some(prefix) = file.strip_suffix("/.dvandva/baton.json") {
        format!("{prefix}/.dvandva")
    } else if file.contains("/.dvandva/runs/") && file.ends_with("/baton.json") {
        // shell `${file%/runs/*/baton.json}` removes the shortest matching
        // suffix, i.e. from the rightmost "/runs/".
        match file.rfind("/runs/") {
            Some(idx) => file[..idx].to_string(),
            None => String::new(),
        }
    } else {
        String::new()
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn dirname(file: &str) -> String {
    match Path::new(file).parent() {
        Some(parent) if parent.as_os_str().is_empty() => ".".to_string(),
        Some(parent) => parent.to_string_lossy().into_owned(),
        None => ".".to_string(),
    }
}

#[derive(Default)]
struct SiblingScan {
    active_count: u64,
    split_brain_run_id: Option<String>,
    human_status: Option<String>,
    human_run_id: String,
    human_checkpoint: String,
    human_question: String,
    human_resume_assignee: String,
    human_resume_status: String,
}

/// A human-pause sibling (`human_decision` / `human_question`) that qualifies
/// to propagate to the selected waiter — strictly newer than the selected
/// baton by RFC3339-parsed `updated_at` (see [`newer_sibling_time`]).
struct HumanCandidate {
    status: String,
    run_id: String,
    updated_at: OffsetDateTime,
    checkpoint: String,
    question: String,
    resume_assignee: String,
    resume_status: String,
}

/// A sibling that structurally triggers split-brain (claims this role while
/// the selected baton waits on the peer). `updated_at` is best-effort — `None`
/// when absent or unparseable — used only to rank *which* sibling gets
/// reported; it never gates whether split-brain fires.
struct SplitBrainCandidate {
    run_id: String,
    updated_at: Option<OffsetDateTime>,
}

/// The active legacy baton plus every `runs/*/baton.json` under `root`,
/// existence-guarded and sorted the way a bash glob expands (`runs/*` sorted;
/// the legacy path is a literal, always first when present). Shared by
/// [`scan_sibling_runs`] and the `--discover` preamble
/// ([`scan_discovery_candidates`]) — the one place this repo lists managed
/// baton files.
fn list_managed_batons(root: &str) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    let legacy = format!("{root}/baton.json");
    if Path::new(&legacy).is_file() {
        files.push(legacy);
    }
    let runs_dir = format!("{root}/runs");
    if let Ok(entries) = std::fs::read_dir(&runs_dir) {
        let mut run_batons: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let candidate = format!("{root}/runs/{}/baton.json", name.to_string_lossy());
            if Path::new(&candidate).is_file() {
                run_batons.push(candidate);
            }
        }
        run_batons.sort(); // bash glob expands sorted
        files.extend(run_batons);
    }
    files
}

/// Scan sibling runs under the selected run's `.dvandva` root. `selected_assignee`
/// is who the selected baton waits on; split-brain only matters when that is the
/// peer. The selected file is skipped by device+inode identity, never by run id,
/// so a stale `.run_id` field cannot hide a genuine sibling.
///
/// Every sibling is visited (no early return): human-pause and split-brain
/// candidates are collected across the whole set, then in each category the
/// MAX by parsed `updated_at` wins (ties broken by lexicographic run id, for
/// determinism) — a mid-listing sibling with the newest timestamp beats an
/// earlier-sorted one with an older timestamp. A human-pause winner still
/// takes priority over split-brain, matching the caller's check order.
fn scan_sibling_runs(
    cfg: &WaitConfig,
    selected_assignee: &str,
    selected_updated_at: &str,
) -> SiblingScan {
    let mut scan = SiblingScan::default();
    let root = derive_dvandva_root(&cfg.baton_file);
    if root.is_empty() {
        return scan;
    }

    let files = list_managed_batons(&root);

    // Robust self-identity, captured once. All three axes are stable across the
    // *one* thing that used to defeat the self-skip: an atomic temp-file+rename
    // replace of the selected baton mid-scan, which swaps its (dev, ino) between
    // this pre-loop capture and the per-file check below (finding
    // vadi-wait-split-brain-false-positive — the run was scanned as its own
    // split-brain sibling).
    let selected_id = file_identity(&cfg.baton_file);
    let selected_canonical = canonical_path(&cfg.baton_file);
    // Path-authoritative id (empty field fallback): never spoofable by a stale
    // `.run_id`, and — unlike inode/canonical — immune to inode churn entirely.
    let selected_run_id = derive_run_id(&cfg.baton_file, "");
    let mut human_candidates: Vec<HumanCandidate> = Vec::new();
    let mut split_brain_candidates: Vec<SplitBrainCandidate> = Vec::new();

    for sibling_file in files {
        let sibling_run_id = derive_run_id(&sibling_file, "");
        if is_selected_self(
            selected_id.as_ref(),
            selected_canonical.as_deref(),
            &selected_run_id,
            &sibling_file,
            &sibling_run_id,
        ) {
            continue;
        }
        let Ok(sibling) = read_json_lenient(Path::new(&sibling_file)) else {
            continue;
        };
        let sibling_status = field_str(&sibling, "status");
        let sibling_assignee = field_str(&sibling, "assignee");
        let sibling_active_roles = active_roles_csv(&sibling);
        let sibling_updated_at = field_str(&sibling, "updated_at");

        // Class-driven, exactly like the selected baton's own status
        // (`resolve_status_class`, below): a v3 sibling resolves its class
        // from its own `run_workflow`, a v1/v2 sibling from the static token
        // map. This is what keeps a v3 custom-graph sibling parked at a
        // non-legacy declared state from leaking through as a false "active"
        // candidate.
        let sibling_class = resolve_status_class(&sibling, &sibling_status);
        match sibling_class {
            // Completed/abandoned (legacy tokens) or any DECLARED terminal
            // state under a non-legacy name: the sibling run is over, and its
            // pause state (if any) is over — neither counts as active nor
            // propagates.
            StateClass::Terminal => continue,
            // Paused on a human — `human_decision`/`human_question` (legacy
            // tokens, both statically `Pause`) or any DECLARED v3 `pause`
            // state. A DECLARED v3 `human_gate` state is folded into this
            // same propagation path: a human is needed either way, so a
            // sibling parked on one propagates a pause exactly like `Pause`
            // does rather than counting as an active split-brain candidate.
            // A newer sibling propagates its intervention to a paired
            // waiter; an older one (or one without a comparable timestamp)
            // is parked. The propagated status label mirrors the
            // selected-baton convention (see the `StateClass::Pause` arm
            // above, in the main wait loop): only the literal
            // `human_question` token keeps that label, every other
            // Pause/HumanGate status propagates as `human_decision`.
            StateClass::Pause | StateClass::HumanGate => {
                if !cfg.concurrent {
                    if let Some(parsed) = newer_sibling_time(
                        selected_updated_at,
                        &sibling_updated_at,
                        &sibling_run_id,
                    ) {
                        let propagated_status = if sibling_status == "human_question" {
                            "human_question".to_string()
                        } else {
                            "human_decision".to_string()
                        };
                        human_candidates.push(HumanCandidate {
                            status: propagated_status,
                            run_id: sibling_run_id,
                            updated_at: parsed,
                            checkpoint: checkpoint_str(&sibling),
                            question: field_str(&sibling, "question"),
                            resume_assignee: field_str(&sibling, "resume_assignee"),
                            resume_status: field_str(&sibling, "resume_status"),
                        });
                    }
                }
                continue;
            }
            StateClass::Work | StateClass::ReviewGate => {
                scan.active_count += 1;
                if !cfg.concurrent
                    && selected_assignee == peer_role(&cfg.role)
                    && (sibling_assignee == cfg.role
                        || contains_role(&sibling_active_roles, &cfg.role))
                {
                    split_brain_candidates.push(SplitBrainCandidate {
                        run_id: sibling_run_id,
                        updated_at: parse_rfc3339(&sibling_updated_at),
                    });
                }
            }
        }
    }

    if let Some(best) = human_candidates.into_iter().max_by(|a, b| {
        a.updated_at
            .cmp(&b.updated_at)
            .then_with(|| a.run_id.cmp(&b.run_id))
    }) {
        scan.human_status = Some(best.status);
        scan.human_run_id = best.run_id;
        scan.human_checkpoint = best.checkpoint;
        scan.human_question = best.question;
        scan.human_resume_assignee = best.resume_assignee;
        scan.human_resume_status = best.resume_status;
        return scan;
    }

    if let Some(best) = split_brain_candidates.into_iter().max_by(|a, b| {
        a.updated_at
            .cmp(&b.updated_at)
            .then_with(|| a.run_id.cmp(&b.run_id))
    }) {
        scan.split_brain_run_id = Some(best.run_id);
    }

    scan
}

/// Parsed sibling `updated_at`, but only when it is strictly newer than
/// `selected_updated_at` — both non-empty and RFC3339-parseable. Fails closed
/// (returns `None`, no propagation) when either side is empty or either
/// non-empty side fails to parse; a genuinely malformed (non-empty) sibling
/// value additionally logs `DVANDVA_WAIT note updated_at_unparseable
/// run=<id>` to stderr. Cross-run checkpoints are not globally comparable, so
/// there is no numeric fallback here — propagating on unparseable data is
/// exactly the risk this closes.
fn newer_sibling_time(
    selected_updated_at: &str,
    sibling_updated_at: &str,
    sibling_run_id: &str,
) -> Option<OffsetDateTime> {
    if selected_updated_at.is_empty() || sibling_updated_at.is_empty() {
        return None;
    }
    match (
        parse_rfc3339(selected_updated_at),
        parse_rfc3339(sibling_updated_at),
    ) {
        (Some(selected), Some(sibling)) if sibling > selected => Some(sibling),
        (Some(_), Some(_)) => None,
        _ => {
            eprintln!("DVANDVA_WAIT note updated_at_unparseable run={sibling_run_id}");
            None
        }
    }
}

/// Hand-rolled RFC3339 parse (`YYYY-MM-DDThh:mm:ss[.fff...][Z|±hh:mm]`,
/// matching every `updated_at` this codebase writes via
/// `next::now_iso8601_utc`). The `time` crate's format-description parser
/// needs its `parsing` cargo feature, which is not enabled on this crate
/// (`Cargo.toml` is a shared file outside this task's file ownership); this
/// builds the same [`OffsetDateTime`] from the always-available core
/// constructors instead. Returns `None` (never panics) on anything that does
/// not match the shape, including non-ASCII input.
///
/// `pub(crate)`: also reused by [`crate::watchdog`] to compute a baton's
/// out-of-band staleness age from the same `updated_at` shape.
pub(crate) fn parse_rfc3339(value: &str) -> Option<OffsetDateTime> {
    if !value.is_ascii() || value.len() < 20 {
        return None;
    }
    let bytes = value.as_bytes();
    let two_digits = |s: &str| -> Option<u8> {
        if s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()) {
            s.parse().ok()
        } else {
            None
        }
    };

    if !value[0..4].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i32 = value[0..4].parse().ok()?;
    if bytes[4] != b'-' {
        return None;
    }
    let month = two_digits(&value[5..7])?;
    if bytes[7] != b'-' {
        return None;
    }
    let day = two_digits(&value[8..10])?;
    match bytes[10] {
        b'T' | b't' | b' ' => {}
        _ => return None,
    }
    let hour = two_digits(&value[11..13])?;
    if bytes[13] != b':' {
        return None;
    }
    let minute = two_digits(&value[14..16])?;
    if bytes[16] != b':' {
        return None;
    }
    let second = two_digits(&value[17..19])?;

    let mut rest = &value[19..];
    let mut nanosecond: u32 = 0;
    if let Some(frac) = rest.strip_prefix('.') {
        let end = frac
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(frac.len());
        let (digits, remainder) = frac.split_at(end);
        if digits.is_empty() {
            return None;
        }
        let mut padded = digits.to_string();
        padded.truncate(9);
        while padded.len() < 9 {
            padded.push('0');
        }
        nanosecond = padded.parse().ok()?;
        rest = remainder;
    }

    let offset = if rest == "Z" || rest == "z" {
        UtcOffset::UTC
    } else {
        let sign: i8 = match rest.as_bytes().first() {
            Some(b'+') => 1,
            Some(b'-') => -1,
            _ => return None,
        };
        let body = &rest[1..];
        if body.len() != 5 || body.as_bytes().get(2) != Some(&b':') {
            return None;
        }
        let offset_hour = two_digits(&body[0..2])? as i8;
        let offset_minute = two_digits(&body[3..5])? as i8;
        UtcOffset::from_hms(sign * offset_hour, sign * offset_minute, 0).ok()?
    };

    let month = Month::try_from(month).ok()?;
    let date = Date::from_calendar_date(year, month, day).ok()?;
    let time = Time::from_hms_nano(hour, minute, second, nanosecond).ok()?;
    Some(PrimitiveDateTime::new(date, time).assume_offset(offset))
}

#[cfg(unix)]
fn file_identity(path: &str) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

#[cfg(not(unix))]
fn file_identity(path: &str) -> Option<std::path::PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// Absolute, symlink-resolved path. Stable across an atomic temp-file+rename
/// replace of the target (the directory entry is what's swapped, not the path),
/// so it identifies the selected run even after its inode churns mid-scan.
fn canonical_path(path: &str) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// Whether `sibling_file` IS the selected run and so must be excluded from its
/// own sibling set. Three independent identity axes, any one sufficient, so no
/// single filesystem event can make the run masquerade as its own sibling:
///   1. Path-derived run id — a pure lexical id from the on-disk layout, immune
///      to inode churn. Restricted to concretely named runs: the generic
///      `legacy`/`unknown` fallbacks can name genuinely distinct batons (e.g. a
///      run literally named `legacy` vs. the legacy baton), so they defer to the
///      path/inode axes rather than risk hiding a real peer.
///   2. Canonical path — stable across an atomic rename replace.
///   3. (dev, ino) — the original check, kept for hardlink/bind-mount layouts
///      whose path strings differ.
fn is_selected_self(
    selected_id: Option<&FileId>,
    selected_canonical: Option<&Path>,
    selected_run_id: &str,
    sibling_file: &str,
    sibling_run_id: &str,
) -> bool {
    if sibling_run_id == selected_run_id
        && selected_run_id != "legacy"
        && selected_run_id != "unknown"
    {
        return true;
    }
    if let (Some(a), Some(b)) = (selected_canonical, canonical_path(sibling_file)) {
        if a == b {
            return true;
        }
    }
    if let (Some(a), Some(b)) = (selected_id, file_identity(sibling_file).as_ref()) {
        if a == b {
            return true;
        }
    }
    false
}

#[cfg(unix)]
type FileId = (u64, u64);
#[cfg(not(unix))]
type FileId = std::path::PathBuf;

/// Whether this role has dependency-unblocked, non-terminal work to do.
///
/// For team-owned `parallel_implementing` / `cross_fixing` states this ports the
/// shell's `role_has_actionable_work` jq exactly, including the advance-owner
/// wake (vadi wakes to write the outbound transition once every implementation
/// chunk is terminal or blocked, preventing a both-roles-asleep deadlock). All
/// other active team states stay unconditionally actionable.
fn role_has_actionable_work(baton: &Value, role: &str, status: &str, phase: &str) -> bool {
    match status {
        "parallel_implementing" | "cross_fixing" => {
            actionable_chunks(baton, role, status, phase) || role_has_open_finding(baton, role)
        }
        _ => true,
    }
}

fn actionable_chunks(baton: &Value, role: &str, status: &str, phase: &str) -> bool {
    let empty = Vec::new();
    let work_split = match coalesce(baton.get("work_split")) {
        Some(Value::Array(items)) => items,
        _ => &empty,
    };
    let ids: Vec<String> = work_split.iter().filter_map(chunk_id).collect();

    // This role owns an unblocked, non-terminal implementation chunk.
    let role_owns = work_split.iter().any(|chunk| {
        is_impl_chunk(chunk, status, phase)
            && chunk_owner(chunk) == role
            && !is_terminal_status(&chunk_status(chunk))
            && chunk_unblocked(chunk, work_split, &ids, status)
    });
    if role_owns {
        return true;
    }

    // Advance-owner wake: when no implementation chunk is unblocked and
    // non-terminal for EITHER role, vadi wakes to write the outbound transition.
    // Owner-role findings are actionable work too, so they suppress this
    // shortcut until the named role handles them.
    role == "vadi"
        && !work_split.iter().any(|chunk| {
            is_impl_chunk(chunk, status, phase)
                && !is_terminal_status(&chunk_status(chunk))
                && chunk_unblocked(chunk, work_split, &ids, status)
        })
        && !has_any_open_owner_role_finding(baton)
}

fn no_actionable_detail(baton: &Value, role: &str, status: &str, phase: &str) -> String {
    format!(
        " no_actionable_work=true scanned_chunks={} scanned_findings={}",
        chunk_scan_summary(baton, role, status, phase),
        finding_scan_summary(baton, role),
    )
}

fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
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

fn role_has_open_finding(baton: &Value, role: &str) -> bool {
    findings(baton)
        .iter()
        .any(|finding| finding_owner_role(finding) == role && finding_is_open(finding))
}

/// Whether a `dispatch_requests` entry naming `role` is a live WAKE token —
/// status EXACTLY `"open"`. A dispatch request is a `{id, role, purpose, status}`
/// object with an `open|acknowledged|completed|cancelled` vocabulary.
///
/// The wake is strict (`== "open"`) while findings stay fail-open on unknown
/// tokens, and the asymmetry is deliberate. Findings fail open because a missed
/// wake there strands real work; a dispatch REQUEST is producer-validated
/// (write.rs's shape gate refuses to create any token outside the vocabulary),
/// so an unknown token cannot occur here, and the failure that DOES matter is the
/// opposite one — re-firing the wake after the vadi has claimed the dispatch
/// (`acknowledged`) would trigger a duplicate PAID cross-vendor dispatch. A
/// duplicate paid dispatch is worse than a missed wake, so only the pristine
/// `open` token wakes; `acknowledged`/`completed`/`cancelled` (and absent) do not.
fn role_has_open_dispatch_request(baton: &Value, role: &str) -> bool {
    baton
        .get("dispatch_requests")
        .and_then(Value::as_array)
        .map(|requests| {
            requests.iter().any(|request| {
                field_str(request, "role") == role && field_str(request, "status") == "open"
            })
        })
        .unwrap_or(false)
}

fn has_any_open_owner_role_finding(baton: &Value) -> bool {
    findings(baton).iter().any(|finding| {
        matches!(finding_owner_role(finding).as_str(), "vadi" | "prativadi")
            && finding_is_open(finding)
    })
}

fn findings(baton: &Value) -> &[Value] {
    baton
        .get("findings")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn finding_owner_role(finding: &Value) -> String {
    coalesce(finding.get("owner_role"))
        .map(jq_scalar_string)
        .unwrap_or_default()
}

fn finding_status(finding: &Value) -> String {
    coalesce(finding.get("status"))
        .map(jq_scalar_string)
        .unwrap_or_default()
}

fn finding_is_open(finding: &Value) -> bool {
    let status = finding_status(finding);
    is_open_finding_status(Some(&status))
}

fn chunk_scan_summary(baton: &Value, role: &str, status: &str, phase: &str) -> String {
    let work_split = baton
        .get("work_split")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let ids: Vec<String> = work_split.iter().filter_map(chunk_id).collect();
    let mut owned = 0usize;
    let mut unblocked = 0usize;
    let mut terminal = 0usize;
    let mut by_type = std::collections::BTreeMap::<String, usize>::new();
    for chunk in work_split
        .iter()
        .filter(|chunk| is_impl_chunk(chunk, status, phase) && chunk_owner(chunk) == role)
    {
        owned += 1;
        let chunk_status = chunk_status(chunk);
        if is_terminal_status(&chunk_status) {
            terminal += 1;
        }
        if chunk_unblocked(chunk, work_split, &ids, status) {
            unblocked += 1;
        }
        *by_type.entry(chunk_type(chunk)).or_insert(0) += 1;
    }
    let types = if by_type.is_empty() {
        "none".to_string()
    } else {
        by_type
            .into_iter()
            .map(|(kind, count)| format!("{kind}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!("owned:{owned},unblocked:{unblocked},terminal:{terminal},types:{types}")
}

fn finding_scan_summary(baton: &Value, role: &str) -> String {
    let mut owned_open = 0usize;
    let mut owned_closed = 0usize;
    let mut peer_open = 0usize;
    let mut without_owner = 0usize;
    for finding in findings(baton) {
        let owner = finding_owner_role(finding);
        if owner.is_empty() {
            without_owner += 1;
        } else if owner == role {
            if finding_is_open(finding) {
                owned_open += 1;
            } else {
                owned_closed += 1;
            }
        } else if finding_is_open(finding) {
            peer_open += 1;
        }
    }
    format!(
        "owned_open:{owned_open},owned_closed:{owned_closed},peer_open:{peer_open},without_owner:{without_owner}"
    )
}

fn is_impl_chunk(chunk: &Value, status: &str, phase: &str) -> bool {
    let chunk_phase = coalesce(chunk.get("phase"))
        .map(jq_scalar_string)
        .unwrap_or_default();
    if chunk_phase != phase {
        return false;
    }
    let owner = chunk_owner(chunk);
    if owner != "vadi" && owner != "prativadi" {
        return false;
    }
    if status == "parallel_implementing" {
        let chunk_type = chunk_type(chunk);
        let cross_review_by = coalesce(chunk.get("cross_review_by"))
            .map(jq_scalar_string)
            .unwrap_or_default();
        chunk_type == "implementation"
            && (cross_review_by == "vadi" || cross_review_by == "prativadi")
            && cross_review_by != owner
            && paths_non_empty(chunk)
    } else {
        let chunk_type = chunk_type(chunk);
        chunk_type == "cross_fixing" || chunk_type == "fix"
    }
}

fn chunk_unblocked(chunk: &Value, work_split: &[Value], ids: &[String], status: &str) -> bool {
    depends_on(chunk)
        .iter()
        .all(|dep| dep_satisfied(dep, work_split, ids, status))
}

fn dep_satisfied(dep: &str, work_split: &[Value], ids: &[String], status: &str) -> bool {
    if ids.iter().any(|id| id == dep) {
        // Chunk-id ref: the referenced chunk must be terminal.
        work_split.iter().any(|chunk| {
            chunk_id(chunk).as_deref() == Some(dep) && is_terminal_status(&chunk_status(chunk))
        })
    } else {
        // Anchor (spec-approved / a status name): satisfied unless it equals the
        // current status (you are still inside that stage).
        dep != status
    }
}

fn chunk_owner(chunk: &Value) -> String {
    coalesce(chunk.get("owner_role"))
        .or_else(|| coalesce(chunk.get("owner")))
        .map(jq_scalar_string)
        .unwrap_or_default()
}

fn chunk_type(chunk: &Value) -> String {
    coalesce(chunk.get("chunk_type"))
        .or_else(|| coalesce(chunk.get("type")))
        .map(jq_scalar_string)
        .unwrap_or_default()
}

fn chunk_status(chunk: &Value) -> String {
    coalesce(chunk.get("status"))
        .map(jq_scalar_string)
        .unwrap_or_default()
}

fn chunk_id(chunk: &Value) -> Option<String> {
    chunk
        .get("id")
        .filter(|value| !value.is_null())
        .map(jq_scalar_string)
}

fn paths_non_empty(chunk: &Value) -> bool {
    matches!(chunk.get("paths"), Some(Value::Array(items)) if !items.is_empty())
}

fn depends_on(chunk: &Value) -> Vec<String> {
    match coalesce(chunk.get("depends_on")) {
        Some(Value::Array(items)) => items.iter().map(jq_scalar_string).collect(),
        _ => Vec::new(),
    }
}

// ── status classification (StateClass resolution) ───────────────────────────

/// The [`StateClass`] of the baton's current `status`.
///
/// A v3 baton (identified by a `run_workflow` object) is class-authoritative
/// over its own statuses:
/// * `source: "custom"` — the class is read from the matching `states[]` entry
///   (`states[].class`, one of the five class tokens);
/// * `source: "preset:<name>"` — the class is resolved from the named preset's
///   states (the class of a given token is preset-independent, so any preset
///   carrying it agrees).
///
/// Any resolution miss (absent/unparseable class, status not declared, unknown
/// preset) and every v1/v2 baton fall back to [`workflow::static_class`], the
/// exact-replication token map. This keeps the read path honest: a malformed
/// or partial v3 workflow degrades to legacy semantics rather than guessing.
fn resolve_status_class(value: &Value, status: &str) -> StateClass {
    if let Some(rw) = value.get("run_workflow").filter(|v| v.is_object()) {
        let source = field_str(rw, "source");
        if let Some(preset_name) = source.strip_prefix("preset:") {
            if let Some(graph) = workflow::preset(preset_name) {
                if let Some(st) = graph.states.iter().find(|s| s.name == status) {
                    return st.class;
                }
            }
        } else if let Some(Value::Array(states)) = rw.get("states") {
            for s in states {
                if field_str(s, "name") == status {
                    if let Some(class) = StateClass::from_token(&field_str(s, "class")) {
                        return class;
                    }
                }
            }
        }
    }
    workflow::static_class(status)
}

// ── field extraction (jq `//` + `join`/`tostring` string coercion) ───────────

/// `.field // ""` then jq-string-coerced: `null`/`false`/absent -> `""`.
fn field_str(value: &Value, key: &str) -> String {
    coalesce(value.get(key))
        .map(jq_scalar_string)
        .unwrap_or_default()
}

/// `(.checkpoint // 0 | tostring)`: `null`/`false`/absent -> `"0"`.
fn checkpoint_str(value: &Value) -> String {
    coalesce(value.get("checkpoint"))
        .map(jq_scalar_string)
        .unwrap_or_else(|| "0".to_string())
}

/// `(.active_roles // []) | join(",")`.
fn active_roles_csv(value: &Value) -> String {
    match coalesce(value.get("active_roles")) {
        Some(Value::Array(items)) => items
            .iter()
            .map(jq_scalar_string)
            .collect::<Vec<_>>()
            .join(","),
        _ => String::new(),
    }
}

/// How jq stringifies a scalar inside `join` / `tostring`: strings pass through,
/// numbers/booleans use their literal form, null is empty.
fn jq_scalar_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

// ── interruptible wait ───────────────────────────────────────────────────────

/// Sleep one interval, waking early on baton-directory events when a `notify`
/// watcher can start. The poll loop stays authoritative: events only shorten the
/// wait, they never substitute for re-reading state.
fn wait_one_interval(cfg: &WaitConfig) {
    if cfg.interval == 0 {
        return;
    }
    wait_watching_dirs(cfg.interval, &watch_dirs(&cfg.baton_file));
}

/// Same wake-on-event sleep as [`wait_one_interval`], but for the
/// `--discover` preamble: no baton path is known yet, so the directories to
/// watch are the fixed `.dvandva` layout roots instead of [`watch_dirs`].
fn wait_discovery_interval(cfg: &WaitConfig) {
    if cfg.interval == 0 {
        return;
    }
    let mut dirs: Vec<String> = Vec::new();
    for candidate in [".dvandva", ".dvandva/runs"] {
        if Path::new(candidate).is_dir() {
            dirs.push(candidate.to_string());
        }
    }
    wait_watching_dirs(cfg.interval, &dirs);
}

/// Sleep `interval` seconds, waking early on an event in any of `dirs` when a
/// `notify` watcher can start on at least one of them.
fn wait_watching_dirs(interval: u64, dirs: &[String]) {
    let (tx, rx) = mpsc::channel::<()>();
    let handler = move |result: notify::Result<notify::Event>| {
        if result.is_ok() {
            let _ = tx.send(());
        }
    };
    match notify::recommended_watcher(handler) {
        Ok(mut watcher) => {
            let mut watched = 0;
            for dir in dirs {
                if watcher
                    .watch(Path::new(dir), RecursiveMode::NonRecursive)
                    .is_ok()
                {
                    watched += 1;
                }
            }
            if watched > 0 {
                let _ = rx.recv_timeout(Duration::from_secs(interval));
            } else {
                std::thread::sleep(Duration::from_secs(interval));
            }
        }
        Err(_) => std::thread::sleep(Duration::from_secs(interval)),
    }
}

/// Directories to watch: the baton's own dir plus, for a managed layout, the
/// `.dvandva` root, its `runs`, and each run dir (so paired human intervention
/// in another active run also wakes us). Only existing dirs, deduped.
///
/// `allow-missing` before the baton's own directory exists (e.g. a run
/// directory not yet created) falls back to the nearest existing ancestor, so
/// its creation still wakes the loop within one interval; the next iteration
/// re-resolves this function once the real directory (and then the file)
/// materializes, converging on the direct-watch case above.
fn watch_dirs(baton_file: &str) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    let add = |candidate: String, dirs: &mut Vec<String>| {
        if !candidate.is_empty() && Path::new(&candidate).is_dir() && !dirs.contains(&candidate) {
            dirs.push(candidate);
        }
    };
    let file_dir = dirname(baton_file);
    if Path::new(&file_dir).is_dir() {
        add(file_dir, &mut dirs);
    } else if let Some(ancestor) = nearest_existing_ancestor(&file_dir) {
        add(ancestor, &mut dirs);
    }
    let root = derive_dvandva_root(baton_file);
    if !root.is_empty() {
        add(root.clone(), &mut dirs);
        add(format!("{root}/runs"), &mut dirs);
        if let Ok(entries) = std::fs::read_dir(format!("{root}/runs")) {
            for entry in entries.flatten() {
                add(entry.path().to_string_lossy().into_owned(), &mut dirs);
            }
        }
    }
    dirs
}

/// Nearest existing ancestor of `dir` (including `dir` itself), walking up
/// parents the same way [`dirname`] treats an exhausted relative path (empty
/// parent -> `.`, the cwd). `None` only if no ancestor ever resolves to an
/// existing directory.
fn nearest_existing_ancestor(dir: &str) -> Option<String> {
    let mut current = dir.to_string();
    loop {
        if Path::new(&current).is_dir() {
            return Some(current);
        }
        let next = match Path::new(&current).parent() {
            Some(parent) if parent.as_os_str().is_empty() => ".".to_string(),
            Some(parent) => parent.to_string_lossy().into_owned(),
            None => return None,
        };
        if next == current {
            return None;
        }
        current = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn derive_run_id_is_path_authoritative() {
        assert_eq!(derive_run_id(".dvandva/baton.json", "x"), "legacy");
        assert_eq!(derive_run_id("/repo/.dvandva/baton.json", "x"), "legacy");
        assert_eq!(
            derive_run_id(".dvandva/runs/alpha/baton.json", "beta"),
            "alpha"
        );
        assert_eq!(derive_run_id("custom-run/baton.json", ""), "custom-run");
        assert_eq!(derive_run_id("weird.json", "field-id"), "field-id");
        assert_eq!(derive_run_id("weird.json", ""), "unknown");
    }

    #[test]
    fn derive_dvandva_root_arms() {
        assert_eq!(derive_dvandva_root(".dvandva/baton.json"), ".dvandva");
        assert_eq!(
            derive_dvandva_root(".dvandva/runs/alpha/baton.json"),
            ".dvandva"
        );
        assert_eq!(derive_dvandva_root("/x/.dvandva/baton.json"), "/x/.dvandva");
        assert_eq!(
            derive_dvandva_root("/x/.dvandva/runs/alpha/baton.json"),
            "/x/.dvandva"
        );
        assert_eq!(derive_dvandva_root("/tmp/custom/baton.json"), "");
    }

    #[test]
    fn contains_role_matches_csv_membership() {
        assert!(contains_role("vadi,prativadi", "vadi"));
        assert!(contains_role("vadi,prativadi", "prativadi"));
        assert!(!contains_role("prativadi", "vadi"));
        assert!(!contains_role("", "vadi"));
    }

    #[test]
    fn peer_role_pairs_the_two_actors() {
        assert_eq!(peer_role("vadi"), "prativadi");
        assert_eq!(peer_role("prativadi"), "vadi");
        assert_eq!(peer_role("team"), "vadi");
    }

    #[test]
    fn field_and_checkpoint_extraction_follow_jq_semantics() {
        let baton = json!({
            "assignee": "vadi",
            "phase": 1,
            "checkpoint": 7,
            "question": null,
            "active_roles": ["vadi", "prativadi"],
        });
        assert_eq!(field_str(&baton, "assignee"), "vadi");
        assert_eq!(field_str(&baton, "phase"), "1");
        assert_eq!(field_str(&baton, "question"), ""); // null coalesces
        assert_eq!(field_str(&baton, "missing"), "");
        assert_eq!(checkpoint_str(&baton), "7");
        assert_eq!(checkpoint_str(&json!({})), "0");
        assert_eq!(active_roles_csv(&baton), "vadi,prativadi");
    }

    #[test]
    fn advance_owner_wakes_vadi_only_when_nothing_actionable() {
        // Both impl chunks completed -> no chunk is non-terminal+unblocked, so the
        // advance-owner (vadi) is actionable and the peer is not.
        let baton = json!({
            "status": "parallel_implementing",
            "phase": 1,
            "work_split": [
                {"id":"v","phase":"1","chunk_type":"implementation","owner_role":"vadi","status":"completed","depends_on":[],"paths":["a"],"cross_review_by":"prativadi"},
                {"id":"p","phase":"1","chunk_type":"implementation","owner_role":"prativadi","status":"completed","depends_on":[],"paths":["b"],"cross_review_by":"vadi"}
            ]
        });
        assert!(role_has_actionable_work(
            &baton,
            "vadi",
            "parallel_implementing",
            "1"
        ));
        assert!(!role_has_actionable_work(
            &baton,
            "prativadi",
            "parallel_implementing",
            "1"
        ));
    }

    #[test]
    fn chunk_id_dependency_gates_unblocked() {
        // v depends on non-terminal chunk p -> v blocked; p itself unblocked, so
        // vadi's advance-owner wake is suppressed (p is actionable for prativadi).
        let baton = json!({
            "status": "parallel_implementing",
            "phase": 1,
            "work_split": [
                {"id":"v","phase":"1","chunk_type":"implementation","owner_role":"vadi","status":"ready","depends_on":["p"],"paths":["a"],"cross_review_by":"prativadi"},
                {"id":"p","phase":"1","chunk_type":"implementation","owner_role":"prativadi","status":"ready","depends_on":[],"paths":["b"],"cross_review_by":"vadi"}
            ]
        });
        assert!(!role_has_actionable_work(
            &baton,
            "vadi",
            "parallel_implementing",
            "1"
        ));
        assert!(role_has_actionable_work(
            &baton,
            "prativadi",
            "parallel_implementing",
            "1"
        ));
    }

    #[test]
    fn lifecycle_gate_chunk_is_not_implementation_work() {
        // vadi impl done, prativadi impl ready, plus a vadi test gate chunk. The
        // gate must not count -> vadi waits, prativadi ready.
        let baton = json!({
            "status": "parallel_implementing",
            "phase": 1,
            "work_split": [
                {"id":"v","phase":"1","chunk_type":"implementation","owner_role":"vadi","status":"completed","depends_on":["spec-approved"],"paths":["a"],"cross_review_by":"prativadi"},
                {"id":"p","phase":"1","chunk_type":"implementation","owner_role":"prativadi","status":"ready","depends_on":["spec-approved"],"paths":["b"],"cross_review_by":"vadi"},
                {"id":"g","phase":"1","chunk_type":"test","owner_role":"vadi","status":"planned","depends_on":["parallel_implementing"],"paths":["t"]}
            ]
        });
        assert!(!role_has_actionable_work(
            &baton,
            "vadi",
            "parallel_implementing",
            "1"
        ));
        assert!(role_has_actionable_work(
            &baton,
            "prativadi",
            "parallel_implementing",
            "1"
        ));
    }

    #[test]
    fn parse_rfc3339_accepts_the_zulu_second_precision_shape() {
        let a = parse_rfc3339("2026-06-29T15:00:00Z").expect("parseable");
        let b = parse_rfc3339("2026-06-29T15:01:00Z").expect("parseable");
        assert!(b > a);
    }

    #[test]
    fn parse_rfc3339_accepts_fractional_seconds_and_numeric_offset() {
        let parsed = parse_rfc3339("2026-06-29T15:00:00.123456789+02:00").expect("parseable");
        // 15:00 +02:00 is 13:00 UTC, strictly before the Zulu instant below.
        let zulu = parse_rfc3339("2026-06-29T13:00:01Z").expect("parseable");
        assert!(zulu > parsed);
    }

    #[test]
    fn parse_rfc3339_rejects_malformed_or_invalid_input() {
        assert!(parse_rfc3339("").is_none());
        assert!(parse_rfc3339("not-a-timestamp").is_none());
        assert!(parse_rfc3339("2026-13-29T15:00:00Z").is_none()); // month 13
        assert!(parse_rfc3339("2026-06-29T15:00:00").is_none()); // missing offset
    }

    #[test]
    fn newer_sibling_time_fails_closed_on_unparseable_sibling() {
        assert!(newer_sibling_time("2026-06-29T15:00:00Z", "not-a-timestamp", "beta").is_none());
    }

    #[test]
    fn newer_sibling_time_fails_closed_on_empty_side() {
        assert!(newer_sibling_time("", "2026-06-29T15:00:00Z", "beta").is_none());
        assert!(newer_sibling_time("2026-06-29T15:00:00Z", "", "beta").is_none());
    }

    #[test]
    fn newer_sibling_time_returns_none_when_sibling_is_not_newer() {
        assert!(
            newer_sibling_time("2026-06-29T15:00:00Z", "2026-06-29T14:00:00Z", "beta").is_none()
        );
    }

    #[test]
    fn newer_sibling_time_returns_parsed_instant_when_sibling_is_newer() {
        let parsed = newer_sibling_time("2026-06-29T15:00:00Z", "2026-06-29T16:00:00Z", "beta")
            .expect("sibling is newer");
        assert_eq!(parsed, parse_rfc3339("2026-06-29T16:00:00Z").unwrap());
    }

    #[test]
    fn nearest_existing_ancestor_walks_up_to_an_existing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("run-not-created-yet");
        let found = nearest_existing_ancestor(missing.to_str().unwrap()).expect("ancestor");
        assert_eq!(found, dir.path().to_string_lossy());
    }

    #[test]
    fn nearest_existing_ancestor_returns_self_when_already_present() {
        let dir = tempfile::tempdir().unwrap();
        let found = nearest_existing_ancestor(dir.path().to_str().unwrap()).expect("ancestor");
        assert_eq!(found, dir.path().to_string_lossy());
    }
}
