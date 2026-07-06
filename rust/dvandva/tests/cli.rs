//! ws-1 integration tests for the `dvandva` multicall binary: `--version`
//! output and unknown-subcommand exit code. Uses the Cargo-provided binary
//! path (`CARGO_BIN_EXE_dvandva`), so no extra dev-dependency is needed.

use std::process::Command;

fn dvandva() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva"))
}

#[test]
fn version_flag_prints_exact_line() {
    let out = dvandva()
        .arg("--version")
        .output()
        .expect("failed to run dvandva --version");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("dvandva {}\n", env!("CARGO_PKG_VERSION")),
        "--version must track Cargo.toml so releases can never misreport"
    );
}

#[test]
fn unknown_subcommand_exits_2() {
    let out = dvandva()
        .arg("definitely-not-a-subcommand")
        .output()
        .expect("failed to run dvandva with unknown subcommand");
    assert_eq!(out.status.code(), Some(2));
}
