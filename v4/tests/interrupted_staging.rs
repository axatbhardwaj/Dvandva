//! Abrupt write-interruption coverage for run creation and history staging.
//!
//! The interruption has to be a genuine, uncatchable process death. A handled
//! write error lets the kernel unwind and clean up, which is not the failure
//! being tested. It also has to leave the host quiet: an intentional injection
//! must not produce a core dump or a desktop crash notification.
//!
//! SIGKILL satisfies both. It cannot be caught, blocked, or handled, so the
//! writer dies wherever it happens to be, with no unwinding; and SIGKILL is not
//! a core-generating signal, so nothing is dumped and nothing is reported.
//!
//! The kill lands at ramping offsets so the writer is interrupted at many
//! different points, including inside history staging and inside head install.

use std::{path::Path, process::Command, time::Duration};

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

#[test]
fn an_abruptly_killed_writer_never_exposes_a_partial_revision() {
    let mut interrupted = 0usize;
    let mut completed = 0usize;
    let mut observed = Vec::new();

    // Ramp across the window in which creation stages and installs, so the kill
    // lands before, during, and after each write.
    for step in 0..40 {
        let dir = tempfile::tempdir().unwrap();
        let run_dir = dir.path().join("run-a");
        let mut child = Command::new(kernel())
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
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();

        std::thread::sleep(Duration::from_micros(50 * step));
        // Uncatchable and never dumped: the writer gets no chance to clean up.
        let killed_before_exit = child.try_wait().unwrap().is_none();
        let _ = child.kill();
        let status = child.wait().unwrap();

        if killed_before_exit && !status.success() {
            interrupted += 1;
        } else {
            completed += 1;
        }

        match inspect(&run_dir) {
            Ok(state) => observed.push(state),
            Err(error) => panic!("step {step}: {error}"),
        }
    }

    assert!(
        interrupted > 0,
        "no iteration actually killed the writer mid-flight; the interruption is untested"
    );
    // Sanity: the ramp is wide enough to also let creation finish, so the test
    // is exercising the whole window rather than only the earliest instant.
    assert!(
        completed > 0 || observed.contains(&"complete"),
        "the ramp never reached a completed creation, so late writes are untested"
    );
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
