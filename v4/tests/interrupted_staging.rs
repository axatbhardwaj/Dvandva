//! Abrupt write-interruption coverage for run creation and history staging.
//!
//! The interruption has to be a genuine, abrupt process death: no unwinding,
//! no destructors, no flushing. A handled write error lets the kernel clean up,
//! which is not the failure under test. It also has to leave the host quiet —
//! an intentional injection must not produce a core dump or a desktop crash
//! notification.
//!
//! A debug-only `_exit` failpoint satisfies both, and, unlike a scheduler-timed
//! kill, proves the writer actually reached the boundary being tested. These
//! tests therefore only run in debug builds, where the failpoint exists.
#![cfg(debug_assertions)]

use std::os::unix::process::ExitStatusExt;
use std::{path::Path, process::Command};

fn kernel() -> &'static str {
    env!("CARGO_BIN_EXE_dvandva-v4")
}

/// Whatever a reader would see after the writer died here. Returns an error
/// string when a partial or unbacked state is visible.
fn inspect(run_dir: &Path) -> Result<&'static str, String> {
    let head_path = run_dir.join("baton.json");
    let revision_path = run_dir.join("history/00000000000000000000.json");
    let parse = |path: &Path| -> Result<Option<serde_json::Value>, String> {
        match std::fs::read(path) {
            Err(_) => Ok(None),
            Ok(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes)
                .map(Some)
                .map_err(|error| format!("{} is not valid JSON: {error}", path.display())),
        }
    };
    let head = parse(&head_path)?;
    let revision = parse(&revision_path)?;
    match (head, revision) {
        // Nothing installed yet: the ordinary outcome of an early kill.
        (None, None) => Ok("nothing"),
        // Staged but not installed: a reader sees no run, which is correct.
        (None, Some(_)) => Ok("revision-only"),
        // A head must never be visible without the revision that backs it.
        (Some(_), None) => Err("installed head has no history revision".to_owned()),
        (Some(head), Some(revision)) => {
            if head != revision {
                return Err("installed head does not match its history revision".to_owned());
            }
            Ok("complete")
        }
    }
}

/// Die at each atomicity boundary in turn, deterministically, and require that
/// a reader never sees a partial or unbacked run. Scheduler-timed kills could
/// pass without ever reaching the write; a failpoint proves the boundary was hit.
#[test]
fn dying_at_each_staging_boundary_never_exposes_a_partial_revision() {
    // `during_history_stage`: the revision is written but not yet linked into
    // place. `after_history_stage`: the revision is linked, but no head is
    // installed. `during_head_install`: the head is written to a temporary but
    // not yet renamed into place. All three must be invisible to a reader.
    for (failpoint, expected) in [
        ("during_history_stage", "nothing"),
        ("after_history_stage", "revision-only"),
        ("during_head_install", "revision-only"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let run_dir = dir.path().join("run-a");
        let status = Command::new(kernel())
            .env("DVANDVA_TEST_FAILPOINT", failpoint)
            .args([
                "init",
                "--run-dir",
                run_dir.to_str().unwrap(),
                "--run-id",
                "run-a",
                "--objective",
                "Interrupt staging",
                "--worker",
                "codex",
                "--reviewer",
                "claude",
                "--repository-id",
                "github.com/axatbhardwaj/dvandva",
                "--required-deliverable",
                "implementation=output",
            ])
            .status()
            .unwrap();

        // The writer really died at the boundary, abruptly and without a core.
        assert_eq!(
            status.code(),
            Some(137),
            "{failpoint} did not stop the writer at its boundary"
        );
        assert_eq!(
            status.signal(),
            None,
            "an intentional injection must not terminate the host abnormally"
        );
        assert_eq!(
            inspect(&run_dir).unwrap_or_else(|error| panic!("{failpoint}: {error}")),
            expected,
            "{failpoint} left a state a reader must never see"
        );

        // Whatever was left behind, the next creation still succeeds.
        let recovered = Command::new(kernel())
            .args([
                "init",
                "--run-dir",
                run_dir.to_str().unwrap(),
                "--run-id",
                "run-a",
                "--objective",
                "Interrupt staging",
                "--worker",
                "codex",
                "--reviewer",
                "claude",
                "--repository-id",
                "github.com/axatbhardwaj/dvandva",
                "--required-deliverable",
                "implementation=output",
            ])
            .status()
            .unwrap();
        assert!(recovered.success(), "{failpoint} wedged the next creation");
        assert_eq!(inspect(&run_dir).unwrap(), "complete");

        // Recovery scavenged whatever the dead writer left behind, so repeated
        // interruptions cannot grow junk without bound.
        let leftovers = [run_dir.clone(), run_dir.join("history")]
            .iter()
            .flat_map(|directory| std::fs::read_dir(directory).unwrap().flatten())
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.starts_with('.') && name.ends_with(".tmp")
            })
            .count();
        assert_eq!(
            leftovers, 0,
            "{failpoint} left {leftovers} staging temporaries"
        );
    }
}

/// A staging temporary left behind by a killed writer must not wedge the next
/// mutation: recovery is part of surviving an abrupt death, not just atomicity.
#[test]
fn a_leaked_staging_temporary_does_not_wedge_the_next_creation() {
    let dir = tempfile::tempdir().unwrap();
    let run_dir = dir.path().join("run-a");
    std::fs::create_dir_all(run_dir.join("history")).unwrap();
    std::fs::write(
        run_dir.join("history/.00000000000000000000.abandoned.tmp"),
        b"{ partial",
    )
    .unwrap();

    let status = Command::new(kernel())
        .args([
            "init",
            "--run-dir",
            run_dir.to_str().unwrap(),
            "--run-id",
            "run-a",
            "--objective",
            "Recover after an interrupted write",
            "--worker",
            "codex",
            "--reviewer",
            "claude",
            "--repository-id",
            "github.com/axatbhardwaj/dvandva",
            "--required-deliverable",
            "implementation=output",
        ])
        .status()
        .unwrap();
    assert!(
        status.success(),
        "a leaked staging temporary wedged creation"
    );
    assert_eq!(inspect(&run_dir).unwrap(), "complete");
}
