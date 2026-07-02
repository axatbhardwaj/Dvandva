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
//! human_question · `13` abandoned · `20` finite timeout · `21` baton missing
//! · `22` invalid JSON · `23` persist-max · `24` stall-max · `29` split-brain
//! · `2` usage.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde_json::Value;
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};
use ureq::Agent;

use crate::util::{coalesce, now_epoch, read_json_lenient};

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
    /// Best-effort webhook URL for pause-event notifications (`--notify` /
    /// `DVANDVA_NOTIFY_URL`, flag wins). `None` (or an empty string) disables
    /// notification entirely.
    pub notify_url: Option<String>,
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

    loop {
        let mut wait_detail = String::new();
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
                            &selected_run_id,
                            "",
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
                    &selected_run_id,
                    "",
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
        let next_action = field_str(&value, "next_action");

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

        match status.as_str() {
            "done" => {
                println!(
                    "DVANDVA_WAIT done phase={phase} checkpoint={checkpoint} assignee={assignee}"
                );
                notify_plain(cfg, "done", &selected_run_id, &next_action);
                return 10;
            }
            "human_decision" => {
                println!("DVANDVA_WAIT human_decision phase={phase} checkpoint={checkpoint} assignee={assignee}");
                notify_plain(cfg, "human_decision", &selected_run_id, &next_action);
                return 11;
            }
            "human_question" => {
                println!("DVANDVA_WAIT human_question phase={phase} checkpoint={checkpoint} assignee={assignee} resume_assignee={resume_assignee} resume_status={resume_status} question={question}");
                notify_question(
                    cfg,
                    &selected_run_id,
                    &question,
                    &resume_assignee,
                    &resume_status,
                );
                return 12;
            }
            "abandoned" => {
                println!(
                    "DVANDVA_WAIT abandoned phase={phase} checkpoint={checkpoint} assignee={assignee}"
                );
                notify_plain(cfg, "abandoned", &selected_run_id, &next_action);
                return 13;
            }
            _ => {}
        }

        let mut sibling_active_count = 0;
        if cfg.persist {
            let scan = scan_sibling_runs(cfg, &assignee, &updated_at);
            sibling_active_count = scan.active_count;
            match scan.human_status.as_deref() {
                Some("human_decision") => {
                    println!(
                        "DVANDVA_WAIT human_decision role={} selected_run_id={selected_run_id} sibling_run_id={} waiting_on={assignee} phase={phase} status={status} checkpoint={checkpoint} {}{}",
                        cfg.role,
                        scan.human_run_id,
                        heartbeat_meta(cfg, &selected_run_id, scan.active_count),
                        run_id_note,
                    );
                    notify_plain(
                        cfg,
                        "human_decision",
                        &scan.human_run_id,
                        &scan.human_next_action,
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
                    notify_question(
                        cfg,
                        &scan.human_run_id,
                        &scan.human_question,
                        &scan.human_resume_assignee,
                        &scan.human_resume_status,
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
                notify_plain(cfg, "split_brain", &selected_run_id, &next_action);
                return 29;
            }
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
                        wait_detail = " no_actionable_work=true".to_string();
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
                    wait_detail = " no_actionable_work=true".to_string();
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
                    &selected_run_id,
                    &next_action,
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
            &selected_run_id,
            &next_action,
        ) {
            return code;
        }
    }
}

/// Increment the interval accounting, then enforce persist-max and stall-max
/// (both wall-clock). Returns `Some(exit_code)` when a cap fires. `run_id` and
/// `next_action` are best-effort context for the `stalled` notification (empty
/// when the baton has never been successfully parsed, e.g. still missing).
fn record_wait_elapsed(
    cfg: &WaitConfig,
    elapsed: &mut u64,
    persist_started: u64,
    stall_started: u64,
    run_id: &str,
    next_action: &str,
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
    if cfg.stall_max > 0 {
        let total = now_epoch().saturating_sub(stall_started);
        if total >= cfg.stall_max {
            println!(
                "DVANDVA_WAIT stalled role={} file={} stall_elapsed={total}s stall_max={}s",
                cfg.role, cfg.baton_file, cfg.stall_max
            );
            notify_plain(cfg, "stalled", run_id, next_action);
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

// ── pause-event notifications (F3, best-effort) ──────────────────────────────

/// Best-effort webhook notification for a pause event whose body carries a
/// `next_action` excerpt (`done`, `human_decision`, `split_brain`, `stalled`).
/// `next_action` is truncated to 300 chars per the notify contract.
fn notify_plain(cfg: &WaitConfig, event: &str, run_id: &str, next_action: &str) {
    let body = format!(
        "run_id={run_id} event={event} next_action={}",
        truncate_chars(next_action, 300)
    );
    send_notify(cfg, event, run_id, &body);
}

/// Best-effort webhook notification for a `human_question` pause, carrying the
/// question text (truncated to 300 chars, per §F3) and resume fields instead
/// of `next_action`.
fn notify_question(
    cfg: &WaitConfig,
    run_id: &str,
    question: &str,
    resume_assignee: &str,
    resume_status: &str,
) {
    let body = format!(
        "run_id={run_id} event=human_question question={} resume_assignee={resume_assignee} resume_status={resume_status}",
        truncate_chars(question, 300)
    );
    send_notify(cfg, "human_question", run_id, &body);
}

/// POST `body` to `cfg.notify_url` (a no-op when disabled) with an ntfy-style
/// `Title: Dvandva <run_id>: <event>` header and a 3-second timeout. Strictly
/// best-effort: any failure is logged to stderr as
/// `DVANDVA_WAIT notify_failed url=<u> err=<short>` and never changes the
/// wait loop's exit code or timing beyond this worst-case 3s.
fn send_notify(cfg: &WaitConfig, event: &str, run_id: &str, body: &str) {
    let Some(url) = cfg.notify_url.as_deref().filter(|u| !u.is_empty()) else {
        return;
    };
    let config = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build();
    let agent: Agent = config.into();
    let result = agent
        .post(url)
        .header("Title", format!("Dvandva {run_id}: {event}"))
        .send(body);
    if let Err(err) = result {
        eprintln!(
            "DVANDVA_WAIT notify_failed url={url} err={}",
            truncate_chars(&err.to_string(), 200)
        );
    }
}

/// Truncate `s` to at most `max` chars (not bytes).
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
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
    human_question: String,
    human_resume_assignee: String,
    human_resume_status: String,
    human_next_action: String,
}

/// A human-pause sibling (`human_decision` / `human_question`) that qualifies
/// to propagate to the selected waiter — strictly newer than the selected
/// baton by RFC3339-parsed `updated_at` (see [`newer_sibling_time`]).
struct HumanCandidate {
    status: String,
    run_id: String,
    updated_at: OffsetDateTime,
    question: String,
    resume_assignee: String,
    resume_status: String,
    next_action: String,
}

/// A sibling that structurally triggers split-brain (claims this role while
/// the selected baton waits on the peer). `updated_at` is best-effort — `None`
/// when absent or unparseable — used only to rank *which* sibling gets
/// reported; it never gates whether split-brain fires.
struct SplitBrainCandidate {
    run_id: String,
    updated_at: Option<OffsetDateTime>,
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

    // The active legacy baton is a supported layout, so it competes for my role
    // like any named sibling. It is a literal (not a glob) — existence-guarded.
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

    let selected_id = file_identity(&cfg.baton_file);
    let mut human_candidates: Vec<HumanCandidate> = Vec::new();
    let mut split_brain_candidates: Vec<SplitBrainCandidate> = Vec::new();

    for sibling_file in files {
        if let (Some(sel), Some(sib)) = (selected_id, file_identity(&sibling_file)) {
            if sel == sib {
                continue; // self-skip by (dev, ino), never by run id
            }
        }
        let sibling_run_id = derive_run_id(&sibling_file, "");
        let Ok(sibling) = read_json_lenient(Path::new(&sibling_file)) else {
            continue;
        };
        let sibling_status = field_str(&sibling, "status");
        let sibling_assignee = field_str(&sibling, "assignee");
        let sibling_active_roles = active_roles_csv(&sibling);
        let sibling_updated_at = field_str(&sibling, "updated_at");

        match sibling_status.as_str() {
            // Completed or abandoned run: not competing for my role, and its
            // pause state (if any) is over — neither counts as active nor
            // propagates.
            "done" | "abandoned" => continue,
            // Paused on a human. A newer sibling propagates its intervention to a
            // paired waiter; an older one (or one without a comparable
            // timestamp) is parked.
            "human_decision" | "human_question" => {
                if !cfg.concurrent {
                    if let Some(parsed) = newer_sibling_time(
                        selected_updated_at,
                        &sibling_updated_at,
                        &sibling_run_id,
                    ) {
                        human_candidates.push(HumanCandidate {
                            status: sibling_status,
                            run_id: sibling_run_id,
                            updated_at: parsed,
                            question: field_str(&sibling, "question"),
                            resume_assignee: field_str(&sibling, "resume_assignee"),
                            resume_status: field_str(&sibling, "resume_status"),
                            next_action: field_str(&sibling, "next_action"),
                        });
                    }
                }
                continue;
            }
            _ => {
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
        scan.human_question = best.question;
        scan.human_resume_assignee = best.resume_assignee;
        scan.human_resume_status = best.resume_status;
        scan.human_next_action = best.next_action;
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
fn parse_rfc3339(value: &str) -> Option<OffsetDateTime> {
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

/// Whether this role has dependency-unblocked, non-terminal work to do.
///
/// For team-owned `parallel_implementing` / `cross_fixing` states this ports the
/// shell's `role_has_actionable_work` jq exactly, including the advance-owner
/// wake (vadi wakes to write the outbound transition once every implementation
/// chunk is terminal or blocked, preventing a both-roles-asleep deadlock). All
/// other active team states stay unconditionally actionable.
fn role_has_actionable_work(baton: &Value, role: &str, status: &str, phase: &str) -> bool {
    match status {
        "parallel_implementing" | "cross_fixing" => actionable_chunks(baton, role, status, phase),
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
    role == "vadi"
        && !work_split.iter().any(|chunk| {
            is_impl_chunk(chunk, status, phase)
                && !is_terminal_status(&chunk_status(chunk))
                && chunk_unblocked(chunk, work_split, &ids, status)
        })
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
    let dirs = watch_dirs(&cfg.baton_file);
    let (tx, rx) = mpsc::channel::<()>();
    let handler = move |result: notify::Result<notify::Event>| {
        if result.is_ok() {
            let _ = tx.send(());
        }
    };
    match notify::recommended_watcher(handler) {
        Ok(mut watcher) => {
            let mut watched = 0;
            for dir in &dirs {
                if watcher
                    .watch(Path::new(dir), RecursiveMode::NonRecursive)
                    .is_ok()
                {
                    watched += 1;
                }
            }
            if watched > 0 {
                let _ = rx.recv_timeout(Duration::from_secs(cfg.interval));
            } else {
                std::thread::sleep(Duration::from_secs(cfg.interval));
            }
        }
        Err(_) => std::thread::sleep(Duration::from_secs(cfg.interval)),
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
