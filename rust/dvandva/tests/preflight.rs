//! Integration tests for the `dvandva preflight` subcommand, porting
//! `scripts/test-dvandva-preflight.sh`.
//!
//! RE-KEY (whole-file): the shell suite's `stage_stubbed_runtime` section
//! (its "resolved path" / "ask" / "ask-with-stderr" / "create" /
//! "role-mismatch" cases at lines 39-144) replaced `dvandva-resolve.sh` and
//! `dvandva-hook-preflight.sh` with FAKE scripts to test the outer
//! orchestrator's dispatch logic in isolation from the real resolver/hook
//! stage. Post-port there is no subprocess indirection to stub: `preflight`
//! calls `dvandva::resolve::resolve_active_run` and
//! `dvandva::hook_preflight::run_hook_preflight` in-process as ordinary
//! function calls, so the orchestrator and the real resolver/hook stage
//! cannot be exercised separately. Every stubbed case is re-keyed below to
//! the equivalent REAL-repo integration test (`real_resolved_*`,
//! `real_ask_*`, `real_ask_corrupt_*`, `real_create_*`,
//! `real_role_mismatch_*`), extending the shell suite's own "Real
//! integration" section (lines 146-231) to also cover role-mismatch and
//! corrupt-baton-stderr, which that section didn't.
//!
//! DROPPED: the "vadi and prativadi turn/hook-stage preflight helpers are
//! byte-identical" cases (lines 56-68) — post-port there is exactly one
//! compiled binary handling both roles via `--role`, so no per-role script
//! pair exists to compare.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dvandva"))
}

/// A command with an isolated git environment and no ambient Dvandva vars.
fn base_cmd<P: AsRef<std::ffi::OsStr>>(program: P) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_HOOK_SELFCHECK")
        .env_remove("DVANDVA_HOOK_PREFLIGHT")
        .env_remove("DVANDVA_BATON_FILE")
        .env_remove("DVANDVA_RUN_DIR")
        .env_remove("DVANDVA_RUN_ID");
    cmd
}

fn git(repo: &Path, args: &[&str]) -> Output {
    base_cmd("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git invocation")
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    assert!(git(dir, &["init", "-q"]).status.success(), "git init");
    git(dir, &["config", "user.email", "test@dvandva.test"]);
    git(dir, &["config", "user.name", "Dvandva Test"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    fs::write(dir.join(".gitkeep"), "").unwrap();
    git(dir, &["add", ".gitkeep"]);
    assert!(
        git(dir, &["commit", "-q", "-m", "initial"])
            .status
            .success(),
        "initial commit"
    );
}

fn write_baton(path: &Path, run_id: &str, status: &str, updated_at: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!(
            r#"{{"run_id":"{run_id}","status":"{status}","assignee":"vadi","updated_at":"{updated_at}"}}"#
        ),
    )
    .unwrap();
}

/// A schema-v2 baton with an explicit owner/active_roles combination, for the
/// S2-T3 preflight sanity-check cases.
fn write_baton_v2(
    path: &Path,
    run_id: &str,
    status: &str,
    assignee: &str,
    active_roles: &str,
    updated_at: &str,
) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!(
            r#"{{"schema":"dvandva.baton.v2","run_id":"{run_id}","status":"{status}","assignee":"{assignee}","active_roles":{active_roles},"mode":"development","profile":"standard","updated_at":"{updated_at}"}}"#
        ),
    )
    .unwrap();
}

/// A schema-v3 baton with the live `run_workflow` envelope, for the P3 sanity
/// cases that close the v3 skip hole without depending on write-path fixtures.
fn write_baton_v3(
    path: &Path,
    run_id: &str,
    status: &str,
    assignee: &str,
    active_roles: &str,
    updated_at: &str,
) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!(
            r#"{{"schema":"dvandva.baton.v3","run_id":"{run_id}","status":"{status}","assignee":"{assignee}","active_roles":{active_roles},"mode":"development","profile":"full","updated_at":"{updated_at}","run_workflow":{{"source":"preset:full","declared_by":"vadi","declared_at_checkpoint":0,"approved_by":null,"approved_at_checkpoint":null,"revision_round":0,"states":[],"edges":[],"amendments":[]}}}}"#
        ),
    )
    .unwrap();
}

/// A schema-v1 baton, for the S2-T3 legacy-skip case.
fn write_baton_v1(path: &Path, run_id: &str, status: &str, assignee: &str, updated_at: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!(
            r#"{{"schema":"dvandva.baton.v1","run_id":"{run_id}","status":"{status}","assignee":"{assignee}","updated_at":"{updated_at}"}}"#
        ),
    )
    .unwrap();
}

/// Run `dvandva preflight <extra...>` with cwd set to `repo`.
fn preflight(repo: &Path, role: Option<&str>, extra: &[&str]) -> Output {
    let mut cmd = base_cmd(bin());
    cmd.arg("preflight").args(extra).current_dir(repo);
    if let Some(role) = role {
        cmd.env("DVANDVA_ROLE", role);
    }
    cmd.output().expect("dvandva preflight")
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn read_sla_marker(repo: &Path) -> Option<String> {
    fs::read_to_string(repo.join(".dvandva/.session-baton-pending.vadi")).ok()
}

fn write_sla_marker(repo: &Path, text: &str) {
    fs::create_dir_all(repo.join(".dvandva")).unwrap();
    fs::write(repo.join(".dvandva/.session-baton-pending.vadi"), text).unwrap();
}

fn cfg_read(repo: &Path, key: &str) -> String {
    let wt = git(repo, &["config", "--bool", "extensions.worktreeConfig"]);
    if String::from_utf8_lossy(&wt.stdout).trim() == "true" {
        let out = git(repo, &["config", "--worktree", "--get", key]);
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    let out = git(repo, &["config", "--local", "--get", key]);
    if out.status.success() {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        String::new()
    }
}

const HOOK_REL: &str = ".dvandva/githooks";

// ===========================================================================
// RESOLVED: a single resumable run drives the hook stage in-process.
// ===========================================================================

// (re-key of the stubbed "resolved path" cases, lines 70-82) — a real repo
// with exactly one resumable baton resolves via discovery, prints the
// canonical baton path / run_id / selected_by=discovery, then runs the hook
// stage to completion (result=ok), adopting the delegated wrapper.
#[test]
fn real_resolved_runs_hook_stage_to_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "in_progress",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("vadi"), &["--role", "vadi"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("DVANDVA_PREFLIGHT"), "stdout: {text}");
    assert!(text.contains("result=resolved"), "stdout: {text}");
    assert!(
        text.contains(&format!(
            "baton={}",
            repo.join(".dvandva/runs/accuracy/baton.json")
                .canonicalize()
                .unwrap()
                .display()
        )),
        "stdout: {text}"
    );
    assert!(text.contains("run_id=accuracy"), "stdout: {text}");
    assert!(text.contains("selected_by=discovery"), "stdout: {text}");
    assert!(text.contains("DVANDVA_HOOK_PREFLIGHT"), "stdout: {text}");
    assert!(text.contains("result=ok"), "stdout: {text}");
    assert_eq!(cfg_read(repo, "core.hooksPath"), HOOK_REL);
}

// (re-key) an explicit DVANDVA_RUN_ID selector short-circuits discovery and
// is reported via selected_by=DVANDVA_RUN_ID.
#[test]
fn real_resolved_via_explicit_run_id_reports_selector() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "in_progress",
        "2026-06-29T00:00:00Z",
    );
    write_baton(
        &repo.join(".dvandva/runs/other/baton.json"),
        "other",
        "in_progress",
        "2026-06-29T01:00:00Z",
    );

    let mut cmd = base_cmd(bin());
    cmd.arg("preflight")
        .args(["--role", "vadi"])
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .env("DVANDVA_RUN_ID", "accuracy");
    let out = cmd.output().expect("dvandva preflight");
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("run_id=accuracy"), "stdout: {text}");
    assert!(
        text.contains("selected_by=DVANDVA_RUN_ID"),
        "stdout: {text}"
    );
}

// (re-key) a legacy `.dvandva/baton.json` path reports run_id=legacy.
#[test]
fn real_resolved_legacy_baton_reports_legacy_run_id() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton(
        &repo.join(".dvandva/baton.json"),
        "legacy-run",
        "in_progress",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("vadi"), &["--role", "vadi"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("run_id=legacy"),
        "stdout: {}",
        stdout(&out)
    );
}

// (re-key) `--mode off` threads through to the hook stage: the preflight
// still resolves and reports result=resolved, but the hook stage reports
// mode=off / result=off and never adopts the delegated wrapper.
#[test]
fn real_resolved_mode_off_skips_hook_adoption() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "in_progress",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("vadi"), &["--role", "vadi", "--mode", "off"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=resolved"), "stdout: {text}");
    assert!(text.contains("mode=off"), "stdout: {text}");
    assert!(text.contains("result=off"), "stdout: {text}");
    assert_eq!(cfg_read(repo, "core.hooksPath"), "");
}

// ===========================================================================
// ASK: more than one resumable run and no explicit selector; the hook stage
// must never run.
// ===========================================================================

// (re-key of the stubbed "ask" case, lines 84-98; matches the shell suite's
// real-integration "real ASK" case, lines 207-219)
#[test]
fn real_ask_does_not_run_hook_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton(
        &repo.join(".dvandva/runs/aa/baton.json"),
        "aa",
        "in_progress",
        "2026-06-29T00:00:00Z",
    );
    write_baton(
        &repo.join(".dvandva/runs/bb/baton.json"),
        "bb",
        "in_progress",
        "2026-06-29T01:00:00Z",
    );

    let out = preflight(repo, Some("vadi"), &["--role", "vadi"]);
    assert_eq!(code(&out), 12, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("result=ask"),
        "stdout: {}",
        stdout(&out)
    );
    assert!(
        !repo.join(".dvandva/githooks").exists(),
        "ask must not run the hook stage"
    );
}

// (re-key of the stubbed "ask-with-stderr" case, lines 100-111) — a corrupt
// baton fails discovery closed (ASK []) and the resolver's own diagnostic
// (not the stub's synthetic text) is surfaced on stderr.
#[test]
fn real_ask_corrupt_baton_surfaces_resolver_diagnostic() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton(
        &repo.join(".dvandva/runs/aa/baton.json"),
        "aa",
        "in_progress",
        "2026-06-29T00:00:00Z",
    );
    let corrupt = repo.join(".dvandva/runs/corrupt/baton.json");
    fs::create_dir_all(corrupt.parent().unwrap()).unwrap();
    fs::write(&corrupt, "{ not valid json\n").unwrap();

    let out = preflight(repo, Some("vadi"), &["--role", "vadi"]);
    assert_eq!(code(&out), 12, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("result=ask"),
        "stdout: {}",
        stdout(&out)
    );
    assert!(
        stdout(&out).contains("choices=[]"),
        "stdout: {}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(err.contains("DVANDVA_RESOLVE"), "stderr: {err}");
    assert!(err.contains("corrupt_baton"), "stderr: {err}");
    assert!(
        !repo.join(".dvandva/githooks").exists(),
        "ask must not run the hook stage"
    );
}

// ===========================================================================
// CREATE: no resumable run; the hook stage must never run.
// ===========================================================================

// (re-key of the stubbed "create" case, lines 113-128; matches the shell
// suite's real-integration "real CREATE" case, lines 221-231)
#[test]
fn real_create_does_not_run_hook_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);

    let out = preflight(repo, Some("vadi"), &["--role", "vadi"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=create"), "stdout: {text}");
    assert!(
        text.contains(&format!(
            "scaffold={}",
            repo.canonicalize()
                .unwrap()
                .join(".dvandva/runs/run/baton.json")
                .display()
        )),
        "stdout: {text}"
    );
    assert!(text.contains("run_id=run"), "stdout: {text}");
    assert!(
        !repo.join(".dvandva/githooks").exists(),
        "create must not run the hook stage"
    );
}

#[test]
fn run_id_selector_missing_baton_creates_and_arms_for_vadi() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    fs::create_dir_all(repo.join(".dvandva/runs/fresh")).unwrap();

    let mut cmd = base_cmd(bin());
    cmd.arg("preflight")
        .args(["--role", "vadi", "--mode", "off"])
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .env("DVANDVA_RUN_ID", "fresh");
    let out = cmd.output().expect("dvandva preflight");

    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=create"), "stdout: {text}");
    assert!(
        text.contains("selected_by=DVANDVA_RUN_ID"),
        "stdout: {text}"
    );
    assert!(
        read_sla_marker(repo).is_some(),
        "selector bootstrap for vadi should arm the SLA marker"
    );
}

#[test]
fn run_id_selector_missing_baton_waits_without_arming_for_prativadi() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    fs::create_dir_all(repo.join(".dvandva/runs/fresh")).unwrap();

    let mut cmd = base_cmd(bin());
    cmd.arg("preflight")
        .args(["--role", "prativadi", "--mode", "off"])
        .current_dir(repo)
        .env("DVANDVA_ROLE", "prativadi")
        .env("DVANDVA_RUN_ID", "fresh");
    let out = cmd.output().expect("dvandva preflight");

    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=wait"), "stdout: {text}");
    assert!(
        read_sla_marker(repo).is_none(),
        "prativadi selector bootstrap must not arm the vadi SLA marker"
    );
}

#[test]
fn run_id_selector_invalid_candidate_stops_as_stale_run_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let run_dir = repo.join(".dvandva/runs/fresh");
    fs::create_dir_all(&run_dir).unwrap();
    fs::write(run_dir.join("baton.next.json"), "{ not json\n").unwrap();

    let mut cmd = base_cmd(bin());
    cmd.arg("preflight")
        .args(["--role", "vadi", "--mode", "off"])
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .env("DVANDVA_RUN_ID", "fresh");
    let out = cmd.output().expect("dvandva preflight");

    assert_eq!(code(&out), 1, "stdout: {}", stdout(&out));
    let text = stdout(&out);
    assert!(text.contains("result=error"), "stdout: {text}");
    assert!(text.contains("reason=stale_run_dir"), "stdout: {text}");
    assert!(text.contains("detail=invalid_candidate"), "stdout: {text}");
    assert!(
        read_sla_marker(repo).is_none(),
        "stale run-dir gate must not arm a fresh SLA marker"
    );
    assert!(
        run_dir.join("baton.next.json").exists(),
        "preflight should leave the invalid leftover candidate for human inspection"
    );
}

#[test]
fn run_id_selector_garbage_marker_stops_as_stale_run_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    fs::create_dir_all(repo.join(".dvandva/runs/fresh")).unwrap();
    write_sla_marker(repo, "not-a-number\n");

    let mut cmd = base_cmd(bin());
    cmd.arg("preflight")
        .args(["--role", "vadi", "--mode", "off"])
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .env("DVANDVA_RUN_ID", "fresh");
    let out = cmd.output().expect("dvandva preflight");

    assert_eq!(code(&out), 1, "stdout: {}", stdout(&out));
    let text = stdout(&out);
    assert!(text.contains("result=error"), "stdout: {text}");
    assert!(text.contains("reason=stale_run_dir"), "stdout: {text}");
    assert!(text.contains("detail=garbage_marker"), "stdout: {text}");
    assert_eq!(
        read_sla_marker(repo).as_deref(),
        Some("not-a-number\n"),
        "garbage marker should stay in place for human inspection"
    );
}

#[test]
fn run_id_selector_invalid_baton_stops_as_stale_run_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let baton = repo.join(".dvandva/runs/fresh/baton.json");
    fs::create_dir_all(baton.parent().unwrap()).unwrap();
    fs::write(&baton, "{ not json\n").unwrap();

    let mut cmd = base_cmd(bin());
    cmd.arg("preflight")
        .args(["--role", "vadi", "--mode", "off"])
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .env("DVANDVA_RUN_ID", "fresh");
    let out = cmd.output().expect("dvandva preflight");

    assert_eq!(code(&out), 1, "stdout: {}", stdout(&out));
    let text = stdout(&out);
    assert!(text.contains("result=error"), "stdout: {text}");
    assert!(text.contains("reason=stale_run_dir"), "stdout: {text}");
    assert!(text.contains("detail=invalid_baton"), "stdout: {text}");
}

#[test]
fn run_id_selector_valid_baton_resolves_without_stale_gate() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_sla_marker(repo, "100\n");
    write_baton_v2(
        &repo.join(".dvandva/runs/fresh/baton.json"),
        "fresh",
        "implementing",
        "vadi",
        "[]",
        "2026-06-29T00:00:00Z",
    );

    let mut cmd = base_cmd(bin());
    cmd.arg("preflight")
        .args(["--role", "vadi", "--mode", "off"])
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .env("DVANDVA_RUN_ID", "fresh");
    let out = cmd.output().expect("dvandva preflight");

    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=resolved"), "stdout: {text}");
    assert!(!text.contains("reason=stale_run_dir"), "stdout: {text}");
    assert!(
        read_sla_marker(repo).is_none(),
        "valid selected baton should satisfy the SLA and clear the marker"
    );
}

#[test]
fn run_dir_selector_missing_baton_creates_and_arms_for_vadi() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let run_dir = repo.join(".dvandva/runs/from-dir");
    fs::create_dir_all(&run_dir).unwrap();

    let mut cmd = base_cmd(bin());
    cmd.arg("preflight")
        .args(["--role", "vadi", "--mode", "off"])
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .env("DVANDVA_RUN_DIR", &run_dir);
    let out = cmd.output().expect("dvandva preflight");

    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=create"), "stdout: {text}");
    assert!(
        text.contains("selected_by=DVANDVA_RUN_DIR"),
        "stdout: {text}"
    );
    assert!(
        read_sla_marker(repo).is_some(),
        "run-dir selector bootstrap for vadi should arm the SLA marker"
    );
}

#[test]
fn baton_file_selector_invalid_baton_stops_as_stale_run_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    let baton = repo.join(".dvandva/runs/from-file/baton.json");
    fs::create_dir_all(baton.parent().unwrap()).unwrap();
    fs::write(&baton, "{ not json\n").unwrap();

    let mut cmd = base_cmd(bin());
    cmd.arg("preflight")
        .args(["--role", "vadi", "--mode", "off"])
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .env("DVANDVA_BATON_FILE", &baton);
    let out = cmd.output().expect("dvandva preflight");

    assert_eq!(code(&out), 1, "stdout: {}", stdout(&out));
    let text = stdout(&out);
    assert!(text.contains("result=error"), "stdout: {text}");
    assert!(text.contains("reason=stale_run_dir"), "stdout: {text}");
    assert!(text.contains("detail=invalid_baton"), "stdout: {text}");
}

// ===========================================================================
// Role mismatch: DVANDVA_ROLE set and different from --role. The hook stage
// must never run. (re-key of the stubbed "role-mismatch" case, lines
// 130-144 — no real-integration equivalent existed in the shell suite.)
// ===========================================================================
#[test]
fn real_role_mismatch_exits_1_without_hook_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "in_progress",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("vadi"), &["--role", "prativadi"]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("reason=role_mismatch"),
        "stdout: {}",
        stdout(&out)
    );
    assert!(
        !repo.join(".dvandva/githooks").exists(),
        "role mismatch must not run the hook stage"
    );
}

// A role matching (or absent) DVANDVA_ROLE proceeds normally.
#[test]
fn role_matches_absent_env_role_proceeds() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);

    let out = preflight(repo, None, &["--role", "vadi"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("result=create"),
        "stdout: {}",
        stdout(&out)
    );
}

// ===========================================================================
// S2-T3: preflight sanity check on a RESOLVED baton, before the hook stage.
// ===========================================================================

// (a) A v2 baton whose assignee does not match the engine's expected owner
// for its status is rejected before the hook stage ever runs.
#[test]
fn sanity_check_owner_mismatch_is_invalid_baton() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton_v2(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "implementing",
        "prativadi",
        "[]",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("prativadi"), &["--role", "prativadi"]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=error"), "stdout: {text}");
    assert!(text.contains("reason=invalid_baton"), "stdout: {text}");
    assert!(text.contains("detail="), "stdout: {text}");
    assert!(
        !repo.join(".dvandva/githooks").exists(),
        "invalid_baton must not run the hook stage"
    );
}

// (b) A team-owned v2 status with empty active_roles is rejected.
#[test]
fn sanity_check_team_status_with_empty_active_roles_is_invalid_baton() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton_v2(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "cross_review",
        "team",
        "[]",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("vadi"), &["--role", "vadi"]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=error"), "stdout: {text}");
    assert!(text.contains("reason=invalid_baton"), "stdout: {text}");
    assert!(text.contains("detail="), "stdout: {text}");
    assert!(
        !repo.join(".dvandva/githooks").exists(),
        "invalid_baton must not run the hook stage"
    );
}

// A healthy v2 baton (owner matches, team status carries active_roles)
// proceeds through the hook stage exactly like before this check existed.
#[test]
fn sanity_check_healthy_v2_baton_proceeds_as_today() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton_v2(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "implementing",
        "vadi",
        "[]",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("vadi"), &["--role", "vadi"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=resolved"), "stdout: {text}");
    assert!(text.contains("DVANDVA_HOOK_PREFLIGHT"), "stdout: {text}");
    assert!(text.contains("result=ok"), "stdout: {text}");
}

// A team-owned status WITH populated active_roles passes the sanity check.
#[test]
fn sanity_check_team_status_with_active_roles_proceeds() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton_v2(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "cross_review",
        "team",
        r#"["vadi","prativadi"]"#,
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("vadi"), &["--role", "vadi"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        stdout(&out).contains("result=resolved"),
        "stdout: {}",
        stdout(&out)
    );
}

// (cross-wave fix) A v2 baton in the human-declared `abandoned` terminal
// resolves through the sanity check instead of failing closed with
// `unknown_status`: `abandoned` is part of the v2 status catalog, and its
// write-path transition always sets assignee=human, matching
// `expected_owner`'s human-owned mapping for the status.
#[test]
fn sanity_check_abandoned_v2_baton_resolves_via_run_id() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton_v2(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "abandoned",
        "human",
        "[]",
        "2026-06-29T00:00:00Z",
    );

    let mut cmd = base_cmd(bin());
    cmd.arg("preflight")
        .args(["--role", "vadi"])
        .current_dir(repo)
        .env("DVANDVA_ROLE", "vadi")
        .env("DVANDVA_RUN_ID", "accuracy");
    let out = cmd.output().expect("dvandva preflight");
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=resolved"), "stdout: {text}");
    assert!(
        text.contains("selected_by=DVANDVA_RUN_ID"),
        "stdout: {text}"
    );
    assert!(!text.contains("reason=invalid_baton"), "stdout: {text}");
}

// P3: v3 batons must run the same pre-hook sanity check, but against the live
// 29-token v3 catalog instead of silently taking the legacy skip path.
#[test]
fn sanity_check_v3_unknown_status_is_invalid_baton() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton_v3(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "bogus_status",
        "vadi",
        "[]",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("vadi"), &["--role", "vadi"]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=error"), "stdout: {text}");
    assert!(text.contains("reason=invalid_baton"), "stdout: {text}");
    assert!(
        text.contains("unknown_status status=bogus_status"),
        "stdout: {text}"
    );
    assert!(
        !repo.join(".dvandva/githooks").exists(),
        "invalid v3 baton must not run the hook stage"
    );
}

#[test]
fn sanity_check_v3_workflow_status_uses_v3_owner_table() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton_v3(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "workflow_declaring",
        "prativadi",
        "[]",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("vadi"), &["--role", "vadi"]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("reason=invalid_baton"), "stdout: {text}");
    assert!(
        text.contains("owner_mismatch status=workflow_declaring expected=vadi actual=prativadi"),
        "stdout: {text}"
    );
}

// P3 sweep item 2: a v3 baton at one of the three NEW per-run-workflow
// declaration tokens (`workflow_review`), correctly owned, passes the v3
// sanity check and proceeds to the hook stage exactly like any other v3
// status — the 29-token v3 catalog accepts it (unlike the v2 catalog below).
#[test]
fn sanity_check_v3_workflow_review_status_proceeds() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton_v3(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "workflow_review",
        "prativadi",
        "[]",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("prativadi"), &["--role", "prativadi"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=resolved"), "stdout: {text}");
    assert!(!text.contains("reason=invalid_baton"), "stdout: {text}");
    assert!(text.contains("DVANDVA_HOOK_PREFLIGHT"), "stdout: {text}");
    assert!(text.contains("result=ok"), "stdout: {text}");
}

// P3 sweep item 2: `workflow_review` is a v3-only addition to the status
// catalog. A v2 baton is still checked against the legacy 26-token catalog
// ([`V2_STATUS_TOKENS`]), which does not contain it — so the very same token
// that passes sanity on a v3 baton above fails `unknown_status` on a v2 one.
#[test]
fn sanity_check_v2_baton_rejects_v3_only_workflow_review_token() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton_v2(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "workflow_review",
        "prativadi",
        "[]",
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("prativadi"), &["--role", "prativadi"]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=error"), "stdout: {text}");
    assert!(text.contains("reason=invalid_baton"), "stdout: {text}");
    assert!(
        text.contains("unknown_status status=workflow_review"),
        "stdout: {text}"
    );
    assert!(
        !repo.join(".dvandva/githooks").exists(),
        "invalid v2 baton must not run the hook stage"
    );
}

// (v1 legacy) A v1-schema baton skips the sanity check entirely and notes the
// skip on the result=resolved line.
#[test]
fn sanity_check_v1_schema_baton_skips_with_note() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo(repo);
    write_baton_v1(
        &repo.join(".dvandva/runs/accuracy/baton.json"),
        "accuracy",
        "implementing",
        "prativadi", // deliberately "wrong" for v2 rules — must not matter for v1
        "2026-06-29T00:00:00Z",
    );

    let out = preflight(repo, Some("prativadi"), &["--role", "prativadi"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("result=resolved"), "stdout: {text}");
    assert!(text.contains("note=v1_skipped"), "stdout: {text}");
    assert!(text.contains("DVANDVA_HOOK_PREFLIGHT"), "stdout: {text}");
    assert!(text.contains("result=ok"), "stdout: {text}");
}

// ===========================================================================
// CLI / usage contract (new: the shell suite never exercised the argument
// parser directly, since it ran the script's own internal `usage`/exit-2
// paths implicitly; mirrors the "CLI / usage contract" section convention
// established in tests/install_hooks.rs).
// ===========================================================================

#[test]
fn missing_role_exits_2() {
    let out = base_cmd(bin()).args(["preflight"]).output().unwrap();
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("Usage"), "stderr: {}", stderr(&out));
}

#[test]
fn invalid_role_exits_2() {
    let out = base_cmd(bin())
        .args(["preflight", "--role", "team"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
}

#[test]
fn invalid_mode_exits_2() {
    let out = base_cmd(bin())
        .args(["preflight", "--role", "vadi", "--mode", "bogus"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
}

#[test]
fn unknown_flag_exits_2() {
    let out = base_cmd(bin())
        .args(["preflight", "--bogus"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 2);
}

#[test]
fn help_flag_exits_0() {
    let out = base_cmd(bin())
        .args(["preflight", "--help"])
        .output()
        .unwrap();
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("Usage"), "stdout: {}", stdout(&out));
}
