//! `dvandva write` — lock / install / snapshot / current-baton integrity
//! themes.
//!
//! Ported from `scripts/test-dvandva-write.sh`. `common::run`/`run_env` are
//! synchronous, so the concurrency cases in this file spawn the binary
//! directly via the `spawn`/`write_cmd` helpers below (mirroring common's
//! env-clearing) to drive two writers against the same baton at once.

mod common;

use common::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}
fn paths(dir: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    (
        dir.path().join("baton.json"),
        dir.path().join("baton.next.json"),
    )
}

// ---------------------------------------------------------------------------
// Direct-process helpers for concurrency cases (common::run is synchronous).
// ---------------------------------------------------------------------------
fn write_command(baton: &Path, candidate: &Path, envs: &[(&str, &str)]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_dvandva"));
    cmd.arg("write").arg(baton).arg(candidate);
    cmd.env_remove("DVANDVA_ROLE")
        .env_remove("DVANDVA_LOCK_TIMEOUT")
        .env_remove("DVANDVA_WRITE_BARRIER")
        .env_remove("DVANDVA_WRITE_BARRIER_POSTFENCE");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd
}

/// Spawn `dvandva write` as a background child; the caller waits on it.
fn spawn(baton: &Path, candidate: &Path, envs: &[(&str, &str)]) -> Child {
    write_command(baton, candidate, envs)
        .spawn()
        .expect("failed to spawn dvandva write")
}

/// Run `dvandva write` synchronously and capture its `Output`.
fn write_cmd(baton: &Path, candidate: &Path, envs: &[(&str, &str)]) -> Output {
    write_command(baton, candidate, envs)
        .output()
        .expect("failed to run dvandva write")
}

fn create_live_foreign_lock(box_dir: &Path) {
    let lock_dir = box_dir.join(".baton.lock.d");
    std::fs::create_dir_all(&lock_dir).unwrap();
    std::fs::write(lock_dir.join("started_at"), "9999999999").unwrap();
    std::fs::write(lock_dir.join("owner"), "foreign").unwrap();
}

// ===================== current-baton integrity =====================

#[test]
fn broken_current_baton_never_clobbered() {
    // S5-T2: CONVERTED to a v2 candidate so the write reaches the current-baton
    // read (a v1 candidate would short-circuit to `schema_retired`). The current
    // is truncated JSON -> unparseable -> exit 25, bytes preserved.
    let d = tmp();
    let (b, n) = paths(&d);
    let raw: &[u8] = b"{\"schema\": \"dvandva.baton.v2\", \"assignee\": ";
    std::fs::write(&b, raw).unwrap();
    make_baton_v2(&n, "research_review", "prativadi", 5, |_| {});
    run(&b, &n).assert("unparseable current baton exits 25", 25);
    let bytes = std::fs::read(&b).unwrap();
    assert_eq!(
        bytes, raw,
        "broken current baton bytes must be preserved untouched"
    );
}

// ===================== install / snapshot failure =====================

#[test]
fn install_fail_on_read_only_dir() {
    // S5-T2: CONVERTED to a v2 research scaffold (a v1 scaffold is now
    // `schema_retired`); the read-only install failure path is engine-wide.
    use std::os::unix::fs::PermissionsExt;
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |_| {});
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = run(&b, &n);
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    result.assert("read-only baton dir exits 26", 26);
    assert!(!b.is_file(), "failed install must leave no baton behind");
}

#[test]
fn snapshot_failure_via_history_file_collision_exits_30() {
    // INTENTIONAL DIVERGENCE: the shell suite forced snapshot failure by
    // running a copy of the script whose sibling dvandva-snapshot.sh helper
    // was missing. The Rust binary runs the snapshot in-process, so that
    // mechanism does not exist. Instead we force the snapshot's
    // `create_dir_all(<dir>/history)` to fail natively by pre-creating
    // `history` as a regular file in the baton dir.
    // S5-T2: CONVERTED to a v2 research scaffold (a v1 scaffold is now
    // `schema_retired`); the snapshot-collision path is engine-wide.
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&n, "research_drafting", "vadi", 0, |_| {});
    std::fs::write(d.path().join("history"), b"x").unwrap();
    run(&b, &n).assert("snapshot failure via history collision exits 30", 30);
    let installed: Value = serde_json::from_slice(&std::fs::read(&b).unwrap()).unwrap();
    assert_eq!(
        installed["status"], "research_drafting",
        "baton must be installed despite the snapshot failure"
    );
}

// ===================== DVANDVA_LOCK_TIMEOUT validation =====================

#[test]
fn dvandva_lock_timeout_rejects_bad_values() {
    // S5-T2: CONVERTED to v2 batons — the DVANDVA_LOCK_TIMEOUT gate is
    // engine-wide, but a v1 candidate would now short-circuit to `schema_retired`
    // before the timeout check.
    for bad in ["abc", "-5", "08", "09", "07", "0", "00"] {
        let d = tmp();
        let (b, n) = paths(&d);
        make_baton_v2(&b, "implementing", "vadi", 4, |_| {});
        make_baton_v2(&n, "phase_review", "prativadi", 5, |_| {});
        create_live_foreign_lock(d.path());
        run_env(&b, &n, &[("DVANDVA_LOCK_TIMEOUT", bad)]).assert_contains(
            &format!("DVANDVA_LOCK_TIMEOUT={bad} rejected"),
            2,
            "bad_lock_timeout",
        );
        if matches!(bad, "-5" | "0" | "00") {
            let installed: Value = serde_json::from_slice(&std::fs::read(&b).unwrap()).unwrap();
            assert_eq!(
                installed["checkpoint"], 4,
                "DVANDVA_LOCK_TIMEOUT={bad} must not steal the live lock"
            );
        }
    }
}

#[test]
fn dvandva_lock_timeout_valid_value_accepted() {
    // S5-T2: CONVERTED to a v2 standard-profile implementing->phase_review edge
    // (the legal v2 analogue of the retired v1 edge).
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "implementing", "vadi", 4, standard_profile);
    make_baton_v2(&n, "phase_review", "prativadi", 5, standard_profile);
    run_env(&b, &n, &[("DVANDVA_LOCK_TIMEOUT", "5")]).assert("valid DVANDVA_LOCK_TIMEOUT=5", 0);
}

// ===================== lock path / fencing =====================

#[test]
fn lock_path_non_directory_exits_28() {
    let d = tmp();
    let run_dir = d.path().join(".dvandva/runs/alpha");
    std::fs::create_dir_all(&run_dir).unwrap();
    let baton = run_dir.join("baton.json");
    let candidate = run_dir.join("baton.next.json");
    make_baton_v2(&candidate, "research_drafting", "vadi", 0, |b| {
        b["run_id"] = json!("alpha");
        b["branch"] = json!("alpha-branch");
    });
    std::fs::write(run_dir.join(".baton.lock.d"), b"corrupt-non-directory\n").unwrap();
    run(&baton, &candidate).assert_contains(
        "non-directory at lock path fails closed exit 28",
        28,
        "DVANDVA_WRITE lock_unavailable",
    );
    assert!(!baton.is_file(), "no baton must be installed unlocked");
    let meta = std::fs::symlink_metadata(run_dir.join(".baton.lock.d")).unwrap();
    assert!(!meta.is_dir(), "squatter must remain a non-directory");
}

#[test]
fn single_uncontended_writer_passes_fencing() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Single writer keeps its own fencing token.");
        b["next_action"] = json!("Team: continue; the sole holder must not self-fence.");
    });
    run(&b, &n).assert("single uncontended writer passes its own fencing check", 0);
}

#[test]
fn stale_lock_recovery() {
    let d = tmp();
    let (b, n) = paths(&d);
    make_baton_v2(&b, "cross_review", "team", 4, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&n, "cross_review", "team", 5, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Stale-lock recovery: new writer steals an abandoned lock.");
        b["next_action"] = json!("Team: continue after recovering the abandoned lock.");
    });
    let lock_dir = d.path().join(".baton.lock.d");
    std::fs::create_dir_all(&lock_dir).unwrap();
    std::fs::write(lock_dir.join("started_at"), "0").unwrap();
    std::fs::write(lock_dir.join("owner"), "ghost-holder-token").unwrap();
    run(&b, &n).assert("abandoned stale lock is recovered and write succeeds", 0);
    let installed: Value = serde_json::from_slice(&std::fs::read(&b).unwrap()).unwrap();
    assert_eq!(
        installed["checkpoint"], 5,
        "stale-lock recovery must install checkpoint 5"
    );
}

#[test]
fn fencing_stolen_lock_exits_29() {
    let d = tmp();
    let box_dir = d.path();
    let baton = box_dir.join("baton.json");
    let cand_a = box_dir.join("cand-a.json");
    let cand_b = box_dir.join("cand-b.json");

    make_baton_v2(&baton, "cross_review", "team", 4, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&cand_a, "cross_review", "team", 5, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Fencing: slow writer A whose lock is stolen.");
        b["next_action"] = json!("Team: A must abort after losing the lock.");
    });
    make_baton_v2(&cand_b, "cross_review", "team", 5, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Fencing: peer writer B steals the lock and installs.");
        b["next_action"] = json!("Team: B wins the stolen-lock race.");
    });

    let barrier = box_dir.join("barrierA");
    let arrived = PathBuf::from(format!("{}.arrived", barrier.display()));
    let release = PathBuf::from(format!("{}.release", barrier.display()));

    // (1) spawn writer A holding the lock and parked at the install barrier.
    let mut child_a = spawn(
        &baton,
        &cand_a,
        &[("DVANDVA_WRITE_BARRIER", barrier.to_str().unwrap())],
    );

    // (2) poll until A has arrived at the barrier.
    let mut waited = 0;
    while !arrived.is_file() && waited < 200 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        waited += 1;
    }
    assert!(arrived.is_file(), "writer A never reached the barrier");

    // (3) backdate A's lock so B's age computation forces an immediate steal.
    std::fs::write(box_dir.join(".baton.lock.d").join("started_at"), "1").unwrap();

    // (4) writer B steals the lock and installs synchronously.
    let b_out = write_cmd(&baton, &cand_b, &[("DVANDVA_LOCK_TIMEOUT", "1")]);
    let rc_b = b_out.status.code().unwrap_or(-1);

    // (5) release the barrier so A resumes and re-checks its fencing token.
    std::fs::write(&release, b"").unwrap();

    // (6) wait for A.
    let status_a = child_a.wait().expect("writer A did not exit");
    let rc_a = status_a.code().unwrap_or(-1);

    let installed: Value = serde_json::from_slice(&std::fs::read(&baton).unwrap()).unwrap();

    assert_eq!(rc_a, 29, "fenced slow writer A must abort, got rc_a={rc_a}");
    assert_eq!(
        rc_b, 0,
        "peer writer B must win the stolen-lock race, got rc_b={rc_b}"
    );
    let zeros = [rc_a, rc_b].iter().filter(|&&rc| rc == 0).count();
    assert_eq!(
        zeros, 1,
        "exactly one writer must install, rc_a={rc_a} rc_b={rc_b}"
    );
    assert_eq!(installed["checkpoint"], 5);
    assert!(
        installed["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("peer writer B"),
        "surviving baton summary must be writer B's, got {:?}",
        installed["summary"]
    );
}

// ===================== concurrent write race =====================

#[test]
fn concurrent_write_race() {
    let d = tmp();
    let box_dir = d.path();
    let baton = box_dir.join("baton.json");
    let cand_a = box_dir.join("cand-a.json");
    let cand_b = box_dir.join("cand-b.json");

    make_baton_v2(&baton, "cross_review", "team", 4, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&cand_a, "cross_review", "team", 5, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Concurrent writer A team sync.");
        b["next_action"] = json!("Team: continue after writer A wins the race.");
    });
    make_baton_v2(&cand_b, "cross_review", "team", 5, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Concurrent writer B team sync.");
        b["next_action"] = json!("Team: continue after writer B wins the race.");
    });

    let mut child_a = spawn(&baton, &cand_a, &[]);
    let mut child_b = spawn(&baton, &cand_b, &[]);
    let rc_a = child_a
        .wait()
        .expect("writer A did not exit")
        .code()
        .unwrap_or(-1);
    let rc_b = child_b
        .wait()
        .expect("writer B did not exit")
        .code()
        .unwrap_or(-1);

    let zeros = [rc_a, rc_b].iter().filter(|&&rc| rc == 0).count();
    let staled = [rc_a, rc_b].iter().filter(|&&rc| rc == 27).count();
    assert_eq!(
        zeros, 1,
        "exactly one concurrent writer must install, rc_a={rc_a} rc_b={rc_b}"
    );
    assert_eq!(
        staled, 1,
        "exactly one concurrent writer must see stale_checkpoint, rc_a={rc_a} rc_b={rc_b}"
    );

    let installed: Value = serde_json::from_slice(&std::fs::read(&baton).unwrap()).unwrap();
    assert_eq!(installed["checkpoint"], 5);
    assert_eq!(installed["status"], "cross_review");
}

// ===================== S4-T10: post-mv fence =====================

/// S4-T10: a thief that steals the lock in the pre-mv-fence→rename window is
/// detected AFTER the rename. The install DID happen (baton carries the writer's
/// checkpoint), the writer exits 29 with `lock_lost_post_install`, and the
/// thief's lock is left intact. Uses the second barrier stage
/// (`DVANDVA_WRITE_BARRIER_POSTFENCE`) which pauses between the fence check and
/// the rename; the pre-mv barrier (`DVANDVA_WRITE_BARRIER`) is untouched.
#[test]
fn s4t10_post_mv_theft_detected_exits_29() {
    let d = tmp();
    let box_dir = d.path();
    let baton = box_dir.join("baton.json");
    let cand = box_dir.join("cand.json");

    make_baton_v2(&baton, "cross_review", "team", 4, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
    });
    make_baton_v2(&cand, "cross_review", "team", 5, |b| {
        cross_review_chunks(b);
        b["active_roles"] = json!(["vadi", "prativadi"]);
        b["summary"] = json!("Post-mv fence: writer parked after the fence check.");
        b["next_action"] = json!("Team: detect the theft in the fence->rename window.");
    });

    let barrier = box_dir.join("postmv");
    let arrived = PathBuf::from(format!("{}.arrived", barrier.display()));
    let release = PathBuf::from(format!("{}.release", barrier.display()));

    // (1) spawn the writer; it acquires the lock, PASSES the pre-mv fence check,
    //     then parks between the fence check and the rename, capturing stderr.
    let child = write_command(
        &baton,
        &cand,
        &[("DVANDVA_WRITE_BARRIER_POSTFENCE", barrier.to_str().unwrap())],
    )
    .stderr(std::process::Stdio::piped())
    .spawn()
    .expect("failed to spawn dvandva write");

    // (2) wait until the writer has passed the fence check and parked.
    let mut waited = 0;
    while !arrived.is_file() && waited < 200 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        waited += 1;
    }
    assert!(
        arrived.is_file(),
        "writer never reached the post-fence barrier"
    );

    // (3) a thief replaces the owner token while the writer is parked.
    let lock_owner = box_dir.join(".baton.lock.d").join("owner");
    std::fs::write(&lock_owner, "thief-token").unwrap();

    // (4) release the barrier: the writer renames (installs) then re-checks holds().
    std::fs::write(&release, b"").unwrap();

    let out = child.wait_with_output().expect("writer did not exit");
    let rc = out.status.code().unwrap_or(-1);
    let text = String::from_utf8_lossy(&out.stderr);

    assert_eq!(rc, 29, "post-mv theft must exit 29, got {rc}\n{text}");
    assert!(
        text.contains("lock_lost_post_install"),
        "post-mv theft must report lock_lost_post_install, got:\n{text}"
    );
    // the install DID happen: the baton carries the writer's checkpoint 5.
    let installed: Value = serde_json::from_slice(&std::fs::read(&baton).unwrap()).unwrap();
    assert_eq!(
        installed["checkpoint"], 5,
        "the rename must have installed the baton despite the lost lock"
    );
    // the thief's lock is intact (never deleted by the fenced writer).
    let owner = std::fs::read_to_string(&lock_owner).unwrap();
    assert_eq!(owner, "thief-token", "the thief's lock token must survive");
}

// ===================== usage =====================

#[test]
fn usage_error_without_args_exits_2() {
    let out = Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("write")
        .output()
        .expect("failed to run dvandva write");
    assert_eq!(out.status.code(), Some(2));
}
