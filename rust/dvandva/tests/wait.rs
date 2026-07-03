//! Integration tests for `dvandva wait` (Task B3 port of
//! `scripts/test-dvandva-wait.sh`, 61 shell cases).
//!
//! Each shell case maps to one `#[test]` here. The five shell cases that only
//! assert the on-disk *shell* helper layout (executable bit, byte-identical
//! plugin copies, removed legacy dirs) are documented in the task report as
//! not-applicable to the Rust binary and are not reproduced here.
//!
//! Determinism under parallel `cargo test`: every spawn clears the five
//! `DVANDVA_*` selector/role env vars via `env_remove`, then sets only what the
//! case needs. `timeout 124` (shell "keeps polling") maps to "process still
//! running at the kill deadline" — asserted via [`Outcome::kept_polling`].

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

const SELECTOR_ENV: [&str; 5] = [
    "DVANDVA_ROLE",
    "DVANDVA_BATON_FILE",
    "DVANDVA_RUN_DIR",
    "DVANDVA_RUN_ID",
    "DVANDVA_CONCURRENT",
];

/// Result of a spawned `dvandva wait`: `code: None` means the process was still
/// running at the kill deadline (shell `timeout` exit 124 — "keeps polling").
struct Outcome {
    code: Option<i32>,
    out: String,
}

impl Outcome {
    fn kept_polling(&self) -> bool {
        self.code.is_none()
    }
    fn contains(&self, needle: &str) -> bool {
        self.out.contains(needle)
    }
}

/// Spawn `dvandva wait <args>` in `cwd` with `envs` set (selector env cleared
/// first), poll until it exits or `budget` elapses, then kill. Returns the exit
/// code (or `None` if killed) and the combined stdout+stderr.
fn run_wait(cwd: Option<&Path>, envs: &[(&str, &str)], args: &[&str], budget: Duration) -> Outcome {
    let mut cmd = Command::new(bin());
    cmd.arg("wait");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for key in SELECTOR_ENV {
        cmd.env_remove(key);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("spawn dvandva wait");
    let start = Instant::now();
    let mut exit_code: Option<i32> = None;
    let mut killed = false;
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => {
                exit_code = status.code();
                break;
            }
            None => {
                if start.elapsed() >= budget {
                    let _ = child.kill();
                    let _ = child.wait();
                    killed = true;
                    break;
                }
                thread::sleep(Duration::from_millis(40));
            }
        }
    }

    let out = drain(&mut child);
    Outcome {
        code: if killed { None } else { exit_code },
        out,
    }
}

fn drain(child: &mut Child) -> String {
    let mut combined = String::new();
    if let Some(mut so) = child.stdout.take() {
        let _ = so.read_to_string(&mut combined);
    }
    let mut err = String::new();
    if let Some(mut se) = child.stderr.take() {
        let _ = se.read_to_string(&mut err);
    }
    combined.push_str(&err);
    combined
}

// ── baton writers (mirror the shell test's write_* helpers) ──────────────────

fn mkparent(file: &Path) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
}

fn write_baton(file: &Path, assignee: &str, status: &str) {
    mkparent(file);
    std::fs::write(
        file,
        format!(
            r#"{{
  "schema": "dvandva.baton.v1",
  "assignee": "{assignee}",
  "status": "{status}",
  "phase": 1,
  "checkpoint": 7
}}"#
        ),
    )
    .unwrap();
}

fn write_question_baton(file: &Path) {
    mkparent(file);
    std::fs::write(
        file,
        r#"{
  "schema": "dvandva.baton.v1",
  "assignee": "human",
  "status": "human_question",
  "phase": "spec",
  "checkpoint": 8,
  "question": "Which scope should Dvandva choose?",
  "resume_assignee": "prativadi",
  "resume_status": "spec_review"
}"#,
    )
    .unwrap();
}

fn write_observed_baton(file: &Path, assignee: &str, status: &str, updated_at: &str, engine: &str) {
    mkparent(file);
    std::fs::write(
        file,
        format!(
            r#"{{
  "schema": "dvandva.baton.v1",
  "assignee": "{assignee}",
  "status": "{status}",
  "phase": 2,
  "checkpoint": 8,
  "question": null,
  "resume_assignee": null,
  "resume_status": null,
  "updated_at": "{updated_at}",
  "current_engine": "{engine}"
}}"#
        ),
    )
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn write_named_observed_baton(
    file: &Path,
    run_id: &str,
    assignee: &str,
    status: &str,
    updated_at: &str,
    engine: &str,
) {
    mkparent(file);
    std::fs::write(
        file,
        format!(
            r#"{{
  "schema": "dvandva.baton.v2",
  "run_id": "{run_id}",
  "assignee": "{assignee}",
  "status": "{status}",
  "phase": 2,
  "checkpoint": 8,
  "question": null,
  "resume_assignee": null,
  "resume_status": null,
  "updated_at": "{updated_at}",
  "current_engine": "{engine}"
}}"#
        ),
    )
    .unwrap();
}

fn write_named_question_baton(file: &Path, run_id: &str, updated_at: &str, engine: &str) {
    mkparent(file);
    std::fs::write(
        file,
        format!(
            r#"{{
  "schema": "dvandva.baton.v2",
  "run_id": "{run_id}",
  "assignee": "human",
  "status": "human_question",
  "phase": "spec",
  "checkpoint": 9,
  "question": "Which scope should Dvandva choose?",
  "resume_assignee": "prativadi",
  "resume_status": "spec_review",
  "updated_at": "{updated_at}",
  "current_engine": "{engine}"
}}"#
        ),
    )
    .unwrap();
}

fn write_active_roles_baton(file: &Path) {
    mkparent(file);
    std::fs::write(
        file,
        r#"{
  "schema": "dvandva.baton.v2",
  "assignee": "team",
  "active_roles": ["vadi", "prativadi"],
  "status": "parallel_implementing",
  "phase": 1,
  "checkpoint": 9,
  "question": null,
  "resume_assignee": null,
  "resume_status": null
}"#,
    )
    .unwrap();
}

fn write_termination_review_baton(file: &Path) {
    mkparent(file);
    std::fs::write(
        file,
        r#"{
  "schema": "dvandva.baton.v2",
  "assignee": "team",
  "active_roles": ["vadi", "prativadi"],
  "status": "termination_review",
  "phase": 1,
  "checkpoint": 10,
  "question": null,
  "resume_assignee": null,
  "resume_status": null
}"#,
    )
    .unwrap();
}

/// `ready_owner` ∈ {"vadi","prativadi","none"} sets that role's chunk to
/// `ready`; the other(s) stay `completed`.
fn write_named_parallel_work_baton(
    file: &Path,
    run_id: &str,
    checkpoint: u64,
    updated_at: &str,
    ready_owner: &str,
) {
    let vadi_status = if ready_owner == "vadi" {
        "ready"
    } else {
        "completed"
    };
    let prativadi_status = if ready_owner == "prativadi" {
        "ready"
    } else {
        "completed"
    };
    let work_split = format!(
        r#"[
    {{
      "id": "vadi-ready-chunk",
      "phase": "1",
      "chunk_type": "implementation",
      "owner_role": "vadi",
      "status": "{vadi_status}",
      "depends_on": [],
      "paths": ["src/vadi.rs"],
      "cross_review_by": "prativadi"
    }},
    {{
      "id": "prativadi-ready-chunk",
      "phase": "1",
      "chunk_type": "implementation",
      "owner_role": "prativadi",
      "status": "{prativadi_status}",
      "depends_on": [],
      "paths": ["src/prativadi.rs"],
      "cross_review_by": "vadi"
    }}
  ]"#
    );
    write_custom_parallel_baton(file, run_id, checkpoint, updated_at, &work_split);
}

fn write_custom_parallel_baton(
    file: &Path,
    run_id: &str,
    checkpoint: u64,
    updated_at: &str,
    work_split: &str,
) {
    mkparent(file);
    std::fs::write(
        file,
        format!(
            r#"{{
  "schema": "dvandva.baton.v2",
  "run_id": "{run_id}",
  "assignee": "team",
  "active_roles": ["vadi", "prativadi"],
  "status": "parallel_implementing",
  "phase": 1,
  "checkpoint": {checkpoint},
  "question": null,
  "resume_assignee": null,
  "resume_status": null,
  "updated_at": "{updated_at}",
  "current_engine": "codex",
  "work_split": {work_split}
}}"#
        ),
    )
    .unwrap();
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

// Budgets: fast cases exit in milliseconds; `_POLL` is the kill deadline for
// "keeps polling"; `_SLOW` is a generous upper bound for cases that exit after
// ~1s of wall-clock (persist-max / stall / torn-read retry).
const BUDGET_FAST: Duration = Duration::from_secs(6);
const BUDGET_POLL: Duration = Duration::from_millis(2600);
const BUDGET_SLOW: Duration = Duration::from_secs(9);

// ── Case 1 ───────────────────────────────────────────────────────────────────
#[test]
fn returns_0_when_role_is_assigned() {
    let d = tmp();
    let f = d.path().join("ready.json");
    write_baton(&f, "vadi", "implementing");
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Cases 2 & 3 ────────────────────────────────────────────────────────────────
#[test]
fn returns_0_for_vadi_active_roles_concurrent_baton() {
    let d = tmp();
    let f = d.path().join("active-roles.json");
    write_active_roles_baton(&f);
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

#[test]
fn returns_0_for_prativadi_active_roles_concurrent_baton() {
    let d = tmp();
    let f = d.path().join("active-roles.json");
    write_active_roles_baton(&f);
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "prativadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 4 ───────────────────────────────────────────────────────────────────
#[test]
fn since_checkpoint_keeps_polling_on_unchanged_active_team_checkpoint() {
    let d = tmp();
    let f = d.path().join("handoff-wait.json");
    write_active_roles_baton(&f);
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "1",
            "--max-wait",
            "540",
            "--since-checkpoint",
            "9",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
}

// ── Case 5 ───────────────────────────────────────────────────────────────────
#[test]
fn since_checkpoint_wakes_when_baton_checkpoint_advances() {
    let d = tmp();
    let f = d.path().join("handoff-advance.json");
    write_active_roles_baton(&f);
    let advance = f.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(700));
        std::fs::write(
            &advance,
            r#"{
  "schema": "dvandva.baton.v2",
  "assignee": "team",
  "active_roles": ["vadi", "prativadi"],
  "status": "parallel_implementing",
  "phase": 1,
  "checkpoint": 10,
  "updated_at": "2026-06-29T21:00:00Z"
}"#,
        )
        .unwrap();
    });
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "1",
            "--max-wait",
            "540",
            "--since-checkpoint",
            "9",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
    assert!(o.contains("checkpoint_advanced"), "{}", o.out);
    assert!(o.contains("since_checkpoint=9"), "{}", o.out);
    assert!(o.contains("checkpoint=10"), "{}", o.out);
}

// ── Case 6 ───────────────────────────────────────────────────────────────────
#[test]
fn until_actionable_keeps_inactive_team_role_polling() {
    let d = tmp();
    write_named_parallel_work_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        80,
        "2026-07-01T09:00:00Z",
        "vadi",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "prativadi",
            "--until-actionable",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(o.contains("no_actionable_work"), "{}", o.out);
    assert!(o.contains("run_id=alpha"), "{}", o.out);
}

// ── Case 7 ───────────────────────────────────────────────────────────────────
#[test]
fn until_actionable_returns_ready_when_team_owned_work_is_actionable() {
    let d = tmp();
    write_named_parallel_work_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        80,
        "2026-07-01T09:05:00Z",
        "prativadi",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "prativadi",
            "--until-actionable",
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 8 ───────────────────────────────────────────────────────────────────
#[test]
fn newer_sibling_human_decision_stops_action_aware_team_state_waiter() {
    let d = tmp();
    write_named_parallel_work_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        80,
        "2026-07-01T09:10:00Z",
        "vadi",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "human",
        "human_decision",
        "2026-07-01T09:11:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "prativadi",
            "--until-actionable",
            "--since-checkpoint",
            "80",
            "--interval",
            "1",
            "--max-wait",
            "540",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(11), "{}", o.out);
    assert!(o.contains("sibling_run_id=beta"), "{}", o.out);
    assert!(o.contains("selected_run_id=alpha"), "{}", o.out);
}

// ── Case 9 ───────────────────────────────────────────────────────────────────
#[test]
fn s1t1_all_chunks_terminal_wakes_advance_owner_vadi() {
    let d = tmp();
    write_named_parallel_work_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        80,
        "2026-07-01T10:00:00Z",
        "none",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--until-actionable",
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 10 ──────────────────────────────────────────────────────────────────
#[test]
fn s1t1_all_terminal_keeps_non_advance_owner_prativadi_waiting() {
    let d = tmp();
    write_named_parallel_work_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        80,
        "2026-07-01T10:00:00Z",
        "none",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "prativadi",
            "--until-actionable",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
}

// ── Case 11 ──────────────────────────────────────────────────────────────────
#[test]
fn s1t1_spec_approved_anchor_makes_owner_actionable() {
    let d = tmp();
    write_custom_parallel_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        80,
        "2026-07-01T10:05:00Z",
        r#"[
  {"id":"v1","phase":"1","chunk_type":"implementation","owner_role":"vadi","status":"ready","depends_on":["spec-approved"],"paths":["src/v.rs"],"cross_review_by":"prativadi"},
  {"id":"p1","phase":"1","chunk_type":"implementation","owner_role":"prativadi","status":"completed","depends_on":["spec-approved"],"paths":["src/p.rs"],"cross_review_by":"vadi"}
]"#,
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--until-actionable",
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 12 ──────────────────────────────────────────────────────────────────
#[test]
fn s1t1_chunk_id_dependency_still_gates_actionability() {
    let d = tmp();
    write_custom_parallel_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        80,
        "2026-07-01T10:06:00Z",
        r#"[
  {"id":"v1","phase":"1","chunk_type":"implementation","owner_role":"vadi","status":"ready","depends_on":["p1"],"paths":["src/v.rs"],"cross_review_by":"prativadi"},
  {"id":"p1","phase":"1","chunk_type":"implementation","owner_role":"prativadi","status":"ready","depends_on":[],"paths":["src/p.rs"],"cross_review_by":"vadi"}
]"#,
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--until-actionable",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
}

// ── Case 13 ──────────────────────────────────────────────────────────────────
#[test]
fn s1t1_lifecycle_gate_chunk_excluded_from_vadi_actionability() {
    let d = tmp();
    write_custom_parallel_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        80,
        "2026-07-01T10:07:00Z",
        r#"[
  {"id":"v1","phase":"1","chunk_type":"implementation","owner_role":"vadi","status":"completed","depends_on":["spec-approved"],"paths":["src/v.rs"],"cross_review_by":"prativadi"},
  {"id":"p1","phase":"1","chunk_type":"implementation","owner_role":"prativadi","status":"ready","depends_on":["spec-approved"],"paths":["src/p.rs"],"cross_review_by":"vadi"},
  {"id":"test_creation","phase":"1","chunk_type":"test","owner_role":"vadi","status":"planned","depends_on":["parallel_implementing"],"paths":["tests/t.sh"]}
]"#,
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--until-actionable",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
}

// ── Case 14 ──────────────────────────────────────────────────────────────────
#[test]
fn s1t1_prativadi_ready_impl_chunk_actionable_despite_gate_chunk() {
    let d = tmp();
    write_custom_parallel_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        80,
        "2026-07-01T10:07:00Z",
        r#"[
  {"id":"v1","phase":"1","chunk_type":"implementation","owner_role":"vadi","status":"completed","depends_on":["spec-approved"],"paths":["src/v.rs"],"cross_review_by":"prativadi"},
  {"id":"p1","phase":"1","chunk_type":"implementation","owner_role":"prativadi","status":"ready","depends_on":["spec-approved"],"paths":["src/p.rs"],"cross_review_by":"vadi"},
  {"id":"test_creation","phase":"1","chunk_type":"test","owner_role":"vadi","status":"planned","depends_on":["parallel_implementing"],"paths":["tests/t.sh"]}
]"#,
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "prativadi",
            "--until-actionable",
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 15 ──────────────────────────────────────────────────────────────────
#[test]
fn s1t5_stall_max_exits_24_on_non_advancing_baton() {
    let d = tmp();
    write_named_parallel_work_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        80,
        "2026-07-01T10:08:00Z",
        "vadi",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "prativadi",
            "--until-actionable",
            "--interval",
            "1",
            "--max-wait",
            "540",
            "--stall-max",
            "1",
        ],
        BUDGET_SLOW,
    );
    assert_eq!(o.code, Some(24), "{}", o.out);
    assert!(o.contains("stalled"), "{}", o.out);
    assert!(o.contains("stall_max=1s"), "{}", o.out);
}

// ── Cases 16 & 17 ──────────────────────────────────────────────────────────────
#[test]
fn returns_0_for_vadi_termination_review_active_roles() {
    let d = tmp();
    let f = d.path().join("termination-review.json");
    write_termination_review_baton(&f);
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

#[test]
fn returns_0_for_prativadi_termination_review_active_roles() {
    let d = tmp();
    let f = d.path().join("termination-review.json");
    write_termination_review_baton(&f);
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "prativadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 18 ──────────────────────────────────────────────────────────────────
#[test]
fn termination_review_is_not_terminal_done() {
    let d = tmp();
    let f = d.path().join("termination-review-wait.json");
    write_baton(&f, "team", "termination_review");
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
            "--finite",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(20), "{}", o.out);
}

// ── Case 19 ──────────────────────────────────────────────────────────────────
#[test]
fn returns_10_when_run_is_done() {
    let d = tmp();
    let f = d.path().join("done.json");
    write_baton(&f, "human", "done");
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(10), "{}", o.out);
}

// ── Case 20 ──────────────────────────────────────────────────────────────────
#[test]
fn returns_11_on_human_decision() {
    let d = tmp();
    let f = d.path().join("human.json");
    write_baton(&f, "human", "human_decision");
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(11), "{}", o.out);
}

// ── Case 21 ──────────────────────────────────────────────────────────────────
#[test]
fn returns_12_on_human_question_with_resume_fields() {
    let d = tmp();
    let f = d.path().join("question.json");
    write_question_baton(&f);
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(12), "{}", o.out);
    assert!(o.contains("resume_assignee=prativadi"), "{}", o.out);
    assert!(o.contains("resume_status=spec_review"), "{}", o.out);
    assert!(
        o.contains("Which scope should Dvandva choose?"),
        "{}",
        o.out
    );
}

// ── Case 22 ──────────────────────────────────────────────────────────────────
#[test]
fn returns_20_on_timeout_while_assigned_away() {
    let d = tmp();
    let f = d.path().join("wait.json");
    write_baton(&f, "prativadi", "phase_review");
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
            "--finite",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(20), "{}", o.out);
}

// ── Case 23 ──────────────────────────────────────────────────────────────────
#[test]
fn no_selector_wait_delegates_to_named_run_resolver_before_legacy_baton() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "vadi",
        "implementing",
        "2026-06-29T10:00:00Z",
        "codex",
    );
    write_baton(&d.path().join(".dvandva/baton.json"), "human", "done");
    let o = run_wait(
        Some(d.path()),
        &[],
        &["--role", "vadi", "--interval", "0", "--max-wait", "0"],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 24 ──────────────────────────────────────────────────────────────────
#[test]
fn default_walkaway_wait_survives_heartbeat_until_role_returns() {
    let d = tmp();
    let f = d.path().join("continuous.json");
    write_baton(&f, "prativadi", "phase_review");
    let flip = f.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1500));
        write_baton(&flip, "vadi", "implementing");
    });
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_SLOW,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 25 ──────────────────────────────────────────────────────────────────
#[test]
fn rejects_zero_interval_with_positive_max_wait() {
    let d = tmp();
    let f = d.path().join("wait.json");
    write_baton(&f, "prativadi", "phase_review");
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "1",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(2), "{}", o.out);
}

// ── Case 26 ──────────────────────────────────────────────────────────────────
#[test]
fn persist_heartbeat_includes_last_seen_metadata() {
    let d = tmp();
    let f = d.path().join("heartbeat-content.json");
    write_observed_baton(
        &f,
        "prativadi",
        "phase_review",
        "2026-06-27T14:09:08Z",
        "codex",
    );
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(o.contains("last_seen_engine=codex"), "{}", o.out);
    assert!(o.contains("updated_at=2026-06-27T14:09:08Z"), "{}", o.out);
}

// ── Case 27 ──────────────────────────────────────────────────────────────────
#[test]
fn resolver_heartbeat_includes_selector_metadata() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T14:09:08Z",
        "codex",
    );
    let o = run_wait(
        Some(d.path()),
        &[],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(o.contains("run_id=alpha"), "{}", o.out);
    assert!(
        o.contains("file=.dvandva/runs/alpha/baton.json"),
        "{}",
        o.out
    );
    assert!(o.contains("selected_by=resolve"), "{}", o.out);
    assert!(o.contains("sibling_active_runs=0"), "{}", o.out);
}

// ── Case 28 ──────────────────────────────────────────────────────────────────
#[test]
fn split_brain_guard_exits_29_with_selected_and_sibling_run_ids() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T15:00:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "vadi",
        "implementing",
        "2026-06-29T15:01:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(29), "{}", o.out);
    assert!(o.contains("split_brain"), "{}", o.out);
    assert!(o.contains("selected_run_id=alpha"), "{}", o.out);
    assert!(o.contains("sibling_run_id=beta"), "{}", o.out);
}

// ── Case 29 ──────────────────────────────────────────────────────────────────
#[test]
fn concurrent_suppresses_split_brain_exit() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T15:00:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "vadi",
        "implementing",
        "2026-06-29T15:01:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha"), ("DVANDVA_CONCURRENT", "1")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(o.contains("run_id=alpha"), "{}", o.out);
    assert!(o.contains("selected_by=run_id"), "{}", o.out);
    assert!(o.contains("sibling_active_runs=1"), "{}", o.out);
}

// ── Case 30 ──────────────────────────────────────────────────────────────────
#[test]
fn active_legacy_baton_counts_as_split_brain_sibling() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T16:00:00Z",
        "codex",
    );
    write_baton(
        &d.path().join(".dvandva/baton.json"),
        "vadi",
        "implementing",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(29), "{}", o.out);
    assert!(o.contains("split_brain"), "{}", o.out);
    assert!(o.contains("selected_run_id=alpha"), "{}", o.out);
    assert!(o.contains("sibling_run_id=legacy"), "{}", o.out);
}

// ── Case 31 ──────────────────────────────────────────────────────────────────
#[test]
fn self_skip_is_path_based_stale_run_id_field_does_not_hide_sibling() {
    let d = tmp();
    // alpha's on-disk .run_id is a *stale* "beta"; path is authoritative.
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "beta",
        "prativadi",
        "phase_review",
        "2026-06-29T17:00:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "vadi",
        "implementing",
        "2026-06-29T17:01:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(29), "{}", o.out);
    assert!(o.contains("split_brain"), "{}", o.out);
    assert!(o.contains("selected_run_id=alpha"), "{}", o.out);
    assert!(o.contains("sibling_run_id=beta"), "{}", o.out);
}

// ── Case 32 ──────────────────────────────────────────────────────────────────
#[test]
fn older_human_decision_sibling_with_stale_my_role_assignee_is_ignored() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T18:01:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "vadi",
        "human_decision",
        "2026-06-29T18:00:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(!o.contains("split_brain"), "{}", o.out);
    assert!(o.contains("sibling_active_runs=0"), "{}", o.out);
}

// ── Case 33 ──────────────────────────────────────────────────────────────────
#[test]
fn older_human_question_sibling_with_stale_my_role_assignee_is_ignored() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T18:11:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "vadi",
        "human_question",
        "2026-06-29T18:10:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(!o.contains("split_brain"), "{}", o.out);
    assert!(o.contains("sibling_active_runs=0"), "{}", o.out);
}

// ── Case 34 ──────────────────────────────────────────────────────────────────
#[test]
fn newer_sibling_human_decision_stops_paired_vadi_wait() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T20:00:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "human",
        "human_decision",
        "2026-06-29T20:01:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "540",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(11), "{}", o.out);
    assert!(o.contains("sibling_run_id=beta"), "{}", o.out);
    assert!(o.contains("selected_run_id=alpha"), "{}", o.out);
}

// ── Case 35 ──────────────────────────────────────────────────────────────────
#[test]
fn newer_sibling_human_question_stops_paired_prativadi_wait_with_metadata() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "vadi",
        "phase_review",
        "2026-06-29T20:10:00Z",
        "codex",
    );
    write_named_question_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "2026-06-29T20:11:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "prativadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "540",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(12), "{}", o.out);
    assert!(o.contains("sibling_run_id=beta"), "{}", o.out);
    assert!(o.contains("resume_assignee=prativadi"), "{}", o.out);
    assert!(o.contains("resume_status=spec_review"), "{}", o.out);
    assert!(
        o.contains("Which scope should Dvandva choose?"),
        "{}",
        o.out
    );
}

// ── Case 36 ──────────────────────────────────────────────────────────────────
#[test]
fn concurrent_suppresses_newer_sibling_human_decision_stop() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T20:00:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "human",
        "human_decision",
        "2026-06-29T20:01:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha"), ("DVANDVA_CONCURRENT", "1")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "540",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
}

// ── Case 37 ──────────────────────────────────────────────────────────────────
#[test]
fn concurrent_suppresses_newer_sibling_human_question_stop() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "vadi",
        "phase_review",
        "2026-06-29T20:10:00Z",
        "codex",
    );
    write_named_question_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "2026-06-29T20:11:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha"), ("DVANDVA_CONCURRENT", "1")],
        &[
            "--role",
            "prativadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "540",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
}

// ── Case 38 ──────────────────────────────────────────────────────────────────
#[test]
fn terminal_sibling_listing_my_role_in_active_roles_is_skipped() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T18:20:00Z",
        "codex",
    );
    let beta = d.path().join(".dvandva/runs/beta/baton.json");
    mkparent(&beta);
    std::fs::write(
        &beta,
        r#"{
  "schema": "dvandva.baton.v2",
  "run_id": "beta",
  "assignee": "human",
  "active_roles": ["vadi", "prativadi"],
  "status": "human_decision",
  "phase": 2,
  "checkpoint": 8
}"#,
    )
    .unwrap();
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(!o.contains("split_brain"), "{}", o.out);
    assert!(o.contains("sibling_active_runs=0"), "{}", o.out);
}

// ── Case 39 ──────────────────────────────────────────────────────────────────
#[test]
fn legacy_human_decision_sibling_with_stale_my_role_assignee_is_terminal() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T19:00:00Z",
        "codex",
    );
    write_baton(
        &d.path().join(".dvandva/baton.json"),
        "vadi",
        "human_decision",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(!o.contains("split_brain"), "{}", o.out);
    assert!(o.contains("sibling_active_runs=0"), "{}", o.out);
}

// ── Case 40 ──────────────────────────────────────────────────────────────────
#[test]
fn legacy_human_question_sibling_with_stale_my_role_assignee_is_terminal() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T19:10:00Z",
        "codex",
    );
    write_baton(
        &d.path().join(".dvandva/baton.json"),
        "vadi",
        "human_question",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(!o.contains("split_brain"), "{}", o.out);
    assert!(o.contains("sibling_active_runs=0"), "{}", o.out);
}

// ── Case 41 ──────────────────────────────────────────────────────────────────
#[test]
fn non_terminal_sibling_assigned_to_my_role_still_fires_split_brain() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T19:30:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "vadi",
        "phase_review",
        "2026-06-29T19:31:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(29), "{}", o.out);
    assert!(o.contains("split_brain"), "{}", o.out);
    assert!(o.contains("selected_run_id=alpha"), "{}", o.out);
    assert!(o.contains("sibling_run_id=beta"), "{}", o.out);
}

// ── Case 42 ──────────────────────────────────────────────────────────────────
#[test]
fn persist_max_caps_total_wall_clock_wait() {
    let d = tmp();
    let f = d.path().join("wait.json");
    write_baton(&f, "prativadi", "phase_review");
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--persist",
            "--persist-max",
            "1",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_SLOW,
    );
    assert_eq!(o.code, Some(23), "{}", o.out);
    assert!(o.contains("DVANDVA_WAIT persist_max"), "{}", o.out);
    assert!(o.contains("persist_max=1s"), "{}", o.out);
}

// ── Case 43 ──────────────────────────────────────────────────────────────────
#[test]
fn dvandva_baton_file_sets_default_baton_path() {
    let d = tmp();
    let cwd = d.path().join("no-default-baton-here");
    std::fs::create_dir_all(&cwd).unwrap();
    let f = d.path().join("env-file/custom-baton.json");
    write_baton(&f, "vadi", "implementing");
    let o = run_wait(
        Some(&cwd),
        &[("DVANDVA_BATON_FILE", f.to_str().unwrap())],
        &["--role", "vadi", "--interval", "0", "--max-wait", "0"],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 44 ──────────────────────────────────────────────────────────────────
#[test]
fn dvandva_run_id_sets_run_scoped_default_baton_path() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "vadi",
        "implementing",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &["--role", "vadi", "--interval", "0", "--max-wait", "0"],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 45 ──────────────────────────────────────────────────────────────────
#[test]
fn dvandva_run_id_rejects_parent_traversal() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "vadi",
        "implementing",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "../escape")],
        &["--role", "vadi", "--interval", "0", "--max-wait", "0"],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(2), "{}", o.out);
}

// ── Case 46 ──────────────────────────────────────────────────────────────────
#[test]
fn dvandva_run_id_rejects_nested_path() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "vadi",
        "implementing",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha/beta")],
        &["--role", "vadi", "--interval", "0", "--max-wait", "0"],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(2), "{}", o.out);
}

// ── Case 47 ──────────────────────────────────────────────────────────────────
#[test]
fn dvandva_run_id_alpha_does_not_read_beta_for_prativadi() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "vadi",
        "implementing",
    );
    write_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "prativadi",
        "phase_review",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "prativadi",
            "--interval",
            "0",
            "--max-wait",
            "0",
            "--finite",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(20), "{}", o.out);
}

// ── Case 48 ──────────────────────────────────────────────────────────────────
#[test]
fn dvandva_run_id_beta_resolves_independent_prativadi_baton() {
    let d = tmp();
    write_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "vadi",
        "implementing",
    );
    write_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "prativadi",
        "phase_review",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "beta")],
        &["--role", "prativadi", "--interval", "0", "--max-wait", "0"],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 49 ──────────────────────────────────────────────────────────────────
#[test]
fn dvandva_run_dir_sets_run_directory_default_baton_path() {
    let d = tmp();
    let cwd = d.path().join("no-default-baton-here");
    std::fs::create_dir_all(&cwd).unwrap();
    let run_dir = d.path().join("run-dir-box/custom-run");
    write_baton(&run_dir.join("baton.json"), "vadi", "implementing");
    let o = run_wait(
        Some(&cwd),
        &[("DVANDVA_RUN_DIR", run_dir.to_str().unwrap())],
        &["--role", "vadi", "--interval", "0", "--max-wait", "0"],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 50 ──────────────────────────────────────────────────────────────────
#[test]
fn persist_waits_across_missing_baton_heartbeat_until_ready() {
    let d = tmp();
    let baton = d.path().join(".dvandva/runs/persist/baton.json");
    std::fs::create_dir_all(baton.parent().unwrap()).unwrap();
    let late = baton.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(700));
        write_baton(&late, "prativadi", "phase_review");
    });
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "persist")],
        &[
            "--role",
            "prativadi",
            "--allow-missing",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_SLOW,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 51 ──────────────────────────────────────────────────────────────────
#[test]
fn allow_missing_returns_0_when_file_appears() {
    let d = tmp();
    let f = d.path().join("late/baton.json");
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    let late = f.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(700));
        write_baton(&late, "prativadi", "phase_review");
    });
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "prativadi",
            "--file",
            f.to_str().unwrap(),
            "--allow-missing",
            "--interval",
            "1",
            "--max-wait",
            "5",
        ],
        BUDGET_SLOW,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 52 ──────────────────────────────────────────────────────────────────
#[test]
fn allow_missing_returns_20_on_file_missing_timeout() {
    let d = tmp();
    let f = d.path().join("never/baton.json");
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "prativadi",
            "--file",
            f.to_str().unwrap(),
            "--allow-missing",
            "--interval",
            "1",
            "--max-wait",
            "2",
            "--finite",
        ],
        BUDGET_SLOW,
    );
    assert_eq!(o.code, Some(20), "{}", o.out);
}

// ── Case 53 ──────────────────────────────────────────────────────────────────
#[test]
fn no_flag_returns_21_on_missing_baton() {
    let d = tmp();
    let f = d.path().join("never/baton.json");
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "prativadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(21), "{}", o.out);
}

// ── Case 54 ──────────────────────────────────────────────────────────────────
#[test]
fn persistently_invalid_baton_exits_22_after_retry() {
    let d = tmp();
    let f = d.path().join("bad.json");
    std::fs::write(&f, r#"{"schema": "dvandva.baton.v1", "assignee": "#).unwrap();
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_SLOW,
    );
    assert_eq!(o.code, Some(22), "{}", o.out);
}

// ── Case 55 ──────────────────────────────────────────────────────────────────
#[test]
fn torn_read_healed_by_retry_exits_0() {
    let d = tmp();
    let f = d.path().join("heal.json");
    std::fs::write(&f, r#"{"schema": "dvandva.baton.v1", "assignee": "#).unwrap();
    let heal = f.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        write_baton(&heal, "vadi", "implementing");
    });
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_SLOW,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Case 56 ──────────────────────────────────────────────────────────────────
#[test]
fn usage_advertises_540_default_and_help_exits_0() {
    let o = run_wait(None, &[], &["--help"], BUDGET_FAST);
    assert_eq!(o.code, Some(0), "{}", o.out);
    assert!(o.contains("--max-wait 540"), "{}", o.out);
}

// ── Task S2-T1 (abandoned terminal) ─────────────────────────────────────────

#[test]
fn returns_13_and_abandoned_line_grammar_when_run_is_abandoned() {
    let d = tmp();
    let f = d.path().join("abandoned.json");
    write_baton(&f, "human", "abandoned");
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(13), "{}", o.out);
    assert!(
        o.contains("DVANDVA_WAIT abandoned phase=1 checkpoint=7 assignee=human"),
        "{}",
        o.out
    );
}

#[test]
fn sibling_abandoned_does_not_propagate_or_count_as_active() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T15:00:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "vadi",
        "abandoned",
        "2026-06-29T15:01:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(!o.contains("split_brain"), "{}", o.out);
    assert!(o.contains("sibling_active_runs=0"), "{}", o.out);
}

// ── Task S4-T11 (A-12/A-13: RFC3339 fail-closed + max-selection) ───────────

#[test]
fn unparseable_sibling_updated_at_does_not_propagate_and_logs_note() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T18:01:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "vadi",
        "human_decision",
        "not-a-timestamp",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(!o.contains("human_decision role="), "{}", o.out);
    assert!(o.contains("sibling_active_runs=0"), "{}", o.out);
    assert!(
        o.contains("DVANDVA_WAIT note updated_at_unparseable run=beta"),
        "{}",
        o.out
    );
}

#[test]
fn three_sibling_scan_selects_max_updated_at_not_first_in_listing_order() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T10:00:00Z",
        "codex",
    );
    // File-listing order is beta, delta, gamma (sorted). "delta" — the middle
    // entry — carries the newest `updated_at` and must win over the
    // earlier-sorted "beta", proving selection is by max timestamp, not by
    // first match in directory order.
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "human",
        "human_decision",
        "2026-06-29T11:00:00Z",
        "claude",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/delta/baton.json"),
        "delta",
        "human",
        "human_decision",
        "2026-06-29T13:00:00Z",
        "claude",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/gamma/baton.json"),
        "gamma",
        "human",
        "human_decision",
        "2026-06-29T12:00:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "540",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(11), "{}", o.out);
    assert!(o.contains("sibling_run_id=delta"), "{}", o.out);
    assert!(!o.contains("sibling_run_id=beta"), "{}", o.out);
    assert!(!o.contains("sibling_run_id=gamma"), "{}", o.out);
}

#[test]
fn three_sibling_split_brain_selects_max_updated_at_not_first_in_listing_order() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-06-29T10:00:00Z",
        "codex",
    );
    // File-listing order is beta, delta, gamma (sorted). All three name my role
    // (vadi) as assignee, so all three qualify as split-brain candidates.
    // "delta" — the middle entry — carries the newest `updated_at` and must win
    // over the earlier-sorted "beta", proving selection is by max timestamp,
    // not by first match in directory order.
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "vadi",
        "implementing",
        "2026-06-29T11:00:00Z",
        "claude",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/delta/baton.json"),
        "delta",
        "vadi",
        "implementing",
        "2026-06-29T13:00:00Z",
        "claude",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/gamma/baton.json"),
        "gamma",
        "vadi",
        "implementing",
        "2026-06-29T12:00:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "1",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(29), "{}", o.out);
    assert!(o.contains("sibling_run_id=delta"), "{}", o.out);
    assert!(!o.contains("sibling_run_id=beta"), "{}", o.out);
    assert!(!o.contains("sibling_run_id=gamma"), "{}", o.out);
}

#[test]
fn allow_missing_wakes_within_interval_when_run_dir_does_not_exist_yet() {
    let d = tmp();
    // The run directory itself does not exist yet at wait-start; only its
    // parent (`d`) does. A correct directory-watcher fallback must watch the
    // nearest existing ancestor so the run dir's creation (and the baton
    // write immediately after) wakes the loop well inside the interval,
    // rather than sleeping the full interval blind.
    let f = d.path().join("late-run/baton.json");
    let late = f.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(400));
        write_baton(&late, "prativadi", "phase_review");
    });
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "prativadi",
            "--file",
            f.to_str().unwrap(),
            "--allow-missing",
            "--interval",
            "8",
            "--max-wait",
            "8",
            "--finite",
        ],
        Duration::from_secs(4),
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
}

// ── Task S6 (--through-human) ────────────────────────────────────────────────
//
// A paired session that does NOT own surfacing pauses keeps polling THROUGH
// human_question/human_decision instead of exiting 11/12, noting exactly once
// per pause episode, and auto-wakes when the pause resolves.

fn write_question_baton_at(file: &Path, checkpoint: u64, question: &str) {
    mkparent(file);
    std::fs::write(
        file,
        format!(
            r#"{{
  "schema": "dvandva.baton.v1",
  "assignee": "human",
  "status": "human_question",
  "phase": "spec",
  "checkpoint": {checkpoint},
  "question": "{question}",
  "resume_assignee": "prativadi",
  "resume_status": "spec_review"
}}"#
        ),
    )
    .unwrap();
}

#[test]
fn s6th_flag_finite_human_question_no_exit_note_once_no_duplicate() {
    let d = tmp();
    let f = d.path().join("question.json");
    write_question_baton(&f); // checkpoint 8
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "1",
            "--max-wait",
            "2",
            "--finite",
            "--through-human",
        ],
        BUDGET_SLOW,
    );
    // Non-actionable for the whole finite window -> finite returns its normal
    // "kept waiting, ran out of budget" code, never 12.
    assert_eq!(o.code, Some(20), "{}", o.out);
    assert_eq!(
        o.out
            .matches("DVANDVA_WAIT note human_pause status=human_question checkpoint=8")
            .count(),
        1,
        "{}",
        o.out
    );
}

#[test]
fn s6th_flag_finite_human_decision_no_exit() {
    let d = tmp();
    let f = d.path().join("human.json");
    write_baton(&f, "human", "human_decision"); // checkpoint 7
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "1",
            "--max-wait",
            "1",
            "--finite",
            "--through-human",
        ],
        BUDGET_SLOW,
    );
    assert_eq!(o.code, Some(20), "{}", o.out);
    assert_eq!(
        o.out
            .matches("DVANDVA_WAIT note human_pause status=human_decision checkpoint=7")
            .count(),
        1,
        "{}",
        o.out
    );
}

#[test]
fn s6th_without_flag_human_question_regression_pairing() {
    let d = tmp();
    let f = d.path().join("question.json");
    write_question_baton(&f);
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(12), "{}", o.out);
}

#[test]
fn s6th_flag_continuous_auto_wake_on_resume() {
    let d = tmp();
    let f = d.path().join("question.json");
    write_question_baton(&f);
    let resume = f.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(700));
        write_baton(&resume, "vadi", "implementing");
    });
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "1",
            "--max-wait",
            "540",
            "--through-human",
        ],
        BUDGET_SLOW,
    );
    assert_eq!(o.code, Some(0), "{}", o.out);
    assert!(o.contains("DVANDVA_WAIT ready role=vadi"), "{}", o.out);
    assert!(
        o.contains("DVANDVA_WAIT note human_pause status=human_question checkpoint=8"),
        "{}",
        o.out
    );
}

#[test]
fn s6th_flag_sibling_pause_no_exit_note_once() {
    let d = tmp();
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "prativadi",
        "phase_review",
        "2026-07-02T10:00:00Z",
        "codex",
    );
    write_named_observed_baton(
        &d.path().join(".dvandva/runs/beta/baton.json"),
        "beta",
        "human",
        "human_decision",
        "2026-07-02T10:01:00Z",
        "claude",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--persist",
            "--interval",
            "1",
            "--max-wait",
            "540",
            "--through-human",
        ],
        BUDGET_POLL,
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert_eq!(
        o.out
            .matches("DVANDVA_WAIT note human_pause status=human_decision checkpoint=8 sibling_run_id=beta")
            .count(),
        1,
        "{}",
        o.out
    );
}

#[test]
fn s6th_flag_abandoned_still_exits_13() {
    let d = tmp();
    let f = d.path().join("abandoned.json");
    write_baton(&f, "human", "abandoned");
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "0",
            "--max-wait",
            "0",
            "--through-human",
        ],
        BUDGET_FAST,
    );
    assert_eq!(o.code, Some(13), "{}", o.out);
    assert!(
        o.contains("DVANDVA_WAIT abandoned phase=1 checkpoint=7 assignee=human"),
        "{}",
        o.out
    );
}

#[test]
fn s6th_flag_stall_suspended_during_pause() {
    let d = tmp();
    write_named_question_baton(
        &d.path().join(".dvandva/runs/alpha/baton.json"),
        "alpha",
        "2026-07-02T10:20:00Z",
        "codex",
    );
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--interval",
            "1",
            "--max-wait",
            "540",
            "--stall-max",
            "1",
            "--through-human",
        ],
        Duration::from_secs(4),
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling (stall suspended during pause), got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(!o.contains("stalled"), "{}", o.out);
}

#[test]
fn s6th_flag_stall_resumes_after_pause_ends() {
    let d = tmp();
    let f = d.path().join(".dvandva/runs/alpha/baton.json");
    write_named_question_baton(&f, "alpha", "2026-07-02T10:30:00Z", "codex"); // checkpoint 9
    let flip = f.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1200));
        // Resolves the pause into a plain non-actionable-for-vadi state at a
        // different checkpoint (8), then never moves again.
        write_named_observed_baton(
            &flip,
            "alpha",
            "prativadi",
            "phase_review",
            "2026-07-02T10:31:00Z",
            "codex",
        );
    });
    let o = run_wait(
        Some(d.path()),
        &[("DVANDVA_RUN_ID", "alpha")],
        &[
            "--role",
            "vadi",
            "--interval",
            "1",
            "--max-wait",
            "540",
            "--stall-max",
            "1",
            "--through-human",
        ],
        Duration::from_secs(6),
    );
    assert_eq!(o.code, Some(24), "{}", o.out);
    assert!(o.contains("stalled"), "{}", o.out);
}

#[test]
fn s6th_flag_episode_rekey_on_new_checkpoint() {
    let d = tmp();
    let f = d.path().join("question.json");
    write_question_baton_at(&f, 8, "Which scope should Dvandva choose?");
    let flip = f.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1200));
        write_question_baton_at(&flip, 9, "Which module owns retries?");
    });
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "1",
            "--max-wait",
            "540",
            "--through-human",
        ],
        Duration::from_secs(4),
    );
    assert!(
        o.kept_polling(),
        "expected keeps-polling, got {:?}\n{}",
        o.code,
        o.out
    );
    assert_eq!(
        o.out
            .matches("DVANDVA_WAIT note human_pause status=human_question checkpoint=8")
            .count(),
        1,
        "{}",
        o.out
    );
    assert_eq!(
        o.out
            .matches("DVANDVA_WAIT note human_pause status=human_question checkpoint=9")
            .count(),
        1,
        "{}",
        o.out
    );
}

// ── Contract amendment (A): indefinite wait during a pause ─────────────────

#[test]
fn s6th_flag_continuous_survives_max_wait_during_pause() {
    let d = tmp();
    let f = d.path().join("question.json");
    write_question_baton(&f);
    let o = run_wait(
        None,
        &[],
        &[
            "--role",
            "vadi",
            "--file",
            f.to_str().unwrap(),
            "--interval",
            "1",
            "--max-wait",
            "1",
            "--through-human",
        ],
        Duration::from_secs(4),
    );
    assert!(
        o.kept_polling(),
        "max-wait must never end a --through-human wait during a pause, got {:?}\n{}",
        o.code,
        o.out
    );
    assert!(
        o.out.matches("DVANDVA_WAIT heartbeat").count() >= 2,
        "expected several max-wait heartbeat cycles to have elapsed without exiting\n{}",
        o.out
    );
    assert_eq!(
        o.out
            .matches("DVANDVA_WAIT note human_pause status=human_question checkpoint=8")
            .count(),
        1,
        "{}",
        o.out
    );
}

// ── Contract amendment (B): cross-process episode dedupe ───────────────────

#[test]
fn s6th_flag_cross_process_dedupe_same_episode() {
    let d = tmp();
    let f = d.path().join("question.json");
    write_question_baton(&f);
    let args: Vec<&str> = vec![
        "--role",
        "vadi",
        "--file",
        f.to_str().unwrap(),
        "--interval",
        "0",
        "--max-wait",
        "0",
        "--finite",
        "--through-human",
    ];

    let first = run_wait(None, &[], &args, BUDGET_FAST);
    assert_eq!(first.code, Some(20), "{}", first.out);
    assert_eq!(
        first
            .out
            .matches("DVANDVA_WAIT note human_pause status=human_question checkpoint=8")
            .count(),
        1,
        "{}",
        first.out
    );

    // A second, separate process re-observing the SAME (status, checkpoint)
    // episode must not re-note: the marker file next to the baton persisted
    // what the first invocation already noted.
    let second = run_wait(None, &[], &args, BUDGET_FAST);
    assert_eq!(second.code, Some(20), "{}", second.out);
    assert_eq!(
        second.out.matches("DVANDVA_WAIT note human_pause").count(),
        0,
        "second invocation must not re-note the same episode\n{}",
        second.out
    );

    let marker = d.path().join(".wait-pause-vadi");
    let marker_content = std::fs::read_to_string(&marker).expect("marker file written");
    assert_eq!(marker_content, "own status=human_question checkpoint=8");
}

#[test]
fn s6th_flag_cross_process_new_episode_after_rekey() {
    let d = tmp();
    let f = d.path().join("question.json");
    write_question_baton_at(&f, 8, "Which scope should Dvandva choose?");
    let args: Vec<&str> = vec![
        "--role",
        "vadi",
        "--file",
        f.to_str().unwrap(),
        "--interval",
        "0",
        "--max-wait",
        "0",
        "--finite",
        "--through-human",
    ];

    let first = run_wait(None, &[], &args, BUDGET_FAST);
    assert_eq!(first.code, Some(20), "{}", first.out);

    // The human answered, installing a NEW human_question at a later
    // checkpoint before the second invocation starts.
    write_question_baton_at(&f, 9, "Which module owns retries?");

    let second = run_wait(None, &[], &args, BUDGET_FAST);
    assert_eq!(second.code, Some(20), "{}", second.out);
    assert_eq!(
        second
            .out
            .matches("DVANDVA_WAIT note human_pause status=human_question checkpoint=9")
            .count(),
        1,
        "{}",
        second.out
    );
}
