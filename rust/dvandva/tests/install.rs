//! Integration tests for `dvandva install`, porting `scripts/test-install.sh`
//! (excluding its `assert_source_manifest_version_parity` /
//! `assert_source_agent_roster` preflight checks, which validate generic
//! repo manifests unrelated to the installer and are already covered by
//! `dvandva::smoke` / `tests/smoke.rs`).
//!
//! Fake `claude`/`codex` executables are written as `#!/usr/bin/env bash`
//! scripts into a per-test tempdir and prepended onto `PATH`, mirroring the
//! shell suite's `FAKE_BIN` fixtures verbatim (same `case "$*"` argv
//! matching, same idempotency toggle via `DVANDVA_INSTALL_TEST_ALREADY`).
//!
//! Cases past the ported set (marked "extra") close CLI-surface gaps the
//! shell suite doesn't exercise: `-h`/`--help`, an unknown flag, a missing
//! `claude`/`codex` CLI, and the in-process Codex delegation producing the
//! same nested output as a standalone `install-codex.sh` run.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// `scripts/test-install.sh`'s `FAKE_BIN/claude` stub, verbatim.
const FAKE_CLAUDE: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf 'claude %s\n' "$*" >> "$DVANDVA_INSTALL_TEST_LOG"

case "$*" in
  plugin\ marketplace\ add\ *)
    if [[ "${DVANDVA_INSTALL_TEST_ALREADY:-0}" == "1" ]]; then
      echo "Marketplace 'dvandva' already registered" >&2
      exit 1
    fi
    ;;
  "plugin install dvandva@dvandva")
    if [[ "${DVANDVA_INSTALL_TEST_ALREADY:-0}" == "1" ]]; then
      echo "Plugin 'dvandva@dvandva' already installed" >&2
      exit 1
    fi
    ;;
  *)
    echo "unexpected fake claude invocation: $*" >&2
    exit 64
    ;;
esac
"#;

/// `scripts/test-install.sh`'s `FAKE_BIN/codex` stub, verbatim (modern
/// `codex plugin add` path only — the app-server fallback exits 42 if hit,
/// matching the shell fixture's own guard).
const FAKE_CODEX_MODERN: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf 'codex %s\n' "$*" >> "$DVANDVA_INSTALL_TEST_LOG"

case "$*" in
  "plugin add --help")
    cat <<'HELP'
Install a plugin from a configured marketplace snapshot.
Usage: codex plugin add [OPTIONS] <PLUGIN[@MARKETPLACE]>
HELP
    ;;
  plugin\ marketplace\ add\ *)
    if [[ "${DVANDVA_INSTALL_TEST_ALREADY:-0}" == "1" ]]; then
      echo "Marketplace 'dvandva' already added" >&2
      exit 1
    fi
    ;;
  "plugin add dvandva@dvandva")
    if [[ "${DVANDVA_INSTALL_TEST_ALREADY:-0}" == "1" ]]; then
      echo "Plugin 'dvandva@dvandva' already installed" >&2
      exit 1
    fi
    ;;
  app-server\ *)
    echo "app-server fallback should not run when codex plugin add exists" >&2
    exit 42
    ;;
  *)
    echo "unexpected fake codex invocation: $*" >&2
    exit 64
    ;;
esac
"#;

/// `scripts/test-install-codex.sh`'s `FALLBACK_BIN/codex` stub, verbatim:
/// `plugin add --help` fails, so the legacy app-server JSON-RPC path runs.
const FAKE_CODEX_FALLBACK: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >> "$CODEX_FAKE_LOG"

case "$*" in
  "plugin add --help")
    echo "unknown command: plugin add" >&2
    exit 1
    ;;
  plugin\ marketplace\ add\ *)
    ;;
  "app-server --listen stdio://")
    while IFS= read -r line; do
      case "$line" in
        *'"id": 1'*|*'"id":1'*)
          printf '{"id":1,"result":{}}\n'
          ;;
        *'"method": "plugin/install"'*|*'"method":"plugin/install"'*)
          printf '{"id":2,"result":{"pluginId":"dvandva@dvandva","installed":true}}\n'
          ;;
      esac
    done
    ;;
  "plugin add dvandva@dvandva")
    echo "modern plugin add path should not run in fallback fixture" >&2
    exit 42
    ;;
  *)
    echo "unexpected fallback fake codex invocation: $*" >&2
    exit 64
    ;;
esac
"#;

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

/// Local-dir marketplace fixture: `<dir>/marketplace/.agents/plugins/marketplace.json`.
fn write_marketplace_fixture(dir: &Path) -> std::path::PathBuf {
    let marketplace = dir.join("marketplace");
    fs::create_dir_all(marketplace.join(".agents/plugins")).unwrap();
    fs::write(
        marketplace.join(".agents/plugins/marketplace.json"),
        r#"{"name":"dvandva","plugins":[{"name":"dvandva"}]}"#,
    )
    .unwrap();
    marketplace
}

fn prepend_path(dir: &Path) -> OsString {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![dir.to_path_buf()];
    paths.extend(std::env::split_paths(&existing));
    std::env::join_paths(paths).expect("join PATH")
}

struct InstallRun {
    fake_bin: std::path::PathBuf,
    _tmp: tempfile::TempDir,
    envs: Vec<(String, OsString)>,
    replace_path: Option<OsString>,
}

impl InstallRun {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fake_bin = tmp.path().join("bin");
        fs::create_dir_all(&fake_bin).unwrap();
        InstallRun {
            fake_bin,
            _tmp: tmp,
            envs: Vec::new(),
            replace_path: None,
        }
    }

    fn tmp_path(&self) -> &Path {
        self._tmp.path()
    }

    fn env(mut self, key: &str, value: impl Into<OsString>) -> Self {
        self.envs.push((key.to_string(), value.into()));
        self
    }

    /// Replace PATH entirely instead of prepending `fake_bin` (used to
    /// simulate a truly absent `claude`/`codex`, regardless of the host's
    /// own PATH contents).
    fn with_empty_path(mut self) -> Self {
        self.replace_path = Some(self.fake_bin.clone().into_os_string());
        self
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dvandva"));
        cmd.arg("install").args(args);
        cmd.env(
            "PATH",
            self.replace_path
                .clone()
                .unwrap_or_else(|| prepend_path(&self.fake_bin)),
        );
        for (key, value) in &self.envs {
            cmd.env(key, value);
        }
        cmd.output().expect("failed to run dvandva install")
    }
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn contains(text: &str, needle: &str) -> bool {
    text.contains(needle)
}

// ---------------------------------------------------------------------
// Ported: full default run installs Dvandva for both engines
// ---------------------------------------------------------------------
#[test]
fn full_default_run_installs_both_engines() {
    let run = InstallRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("claude"), FAKE_CLAUDE);
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_MODERN);
    let log = tmp.join("install.log");

    let run = run
        .env("CODEX_HOME", tmp.join("codex-home"))
        .env("HOME", tmp.join("home"))
        .env("DVANDVA_INSTALL_TEST_LOG", log.clone());
    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let install_log = fs::read_to_string(&log).unwrap_or_default();

    assert!(install_log.contains(&format!(
        "claude plugin marketplace add {}",
        marketplace.display()
    )));
    assert!(install_log.contains("claude plugin install dvandva@dvandva"));
    assert!(install_log.contains(&format!(
        "codex plugin marketplace add {}",
        marketplace.display()
    )));
    assert!(install_log.contains("codex plugin add dvandva@dvandva"));

    let text = combined(&output);
    assert!(contains(&text, "Claude Code install complete"));
    assert!(contains(&text, "Codex install complete"));
    assert!(contains(&text, "dvandva:research"));
    assert!(contains(&text, "dvandva:testing"));
    assert!(contains(&text, "dvandva:understanding"));
    assert!(contains(&text, "dvandva:worktree-setup"));
}

// ---------------------------------------------------------------------
// Ported: --claude-only + --codex-only conflict rejected before engines run
// ---------------------------------------------------------------------
#[test]
fn conflicting_flags_rejected_before_invoking_engines() {
    let run = InstallRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("claude"), FAKE_CLAUDE);
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_MODERN);
    let log = tmp.join("conflict.log");

    let run = run
        .env("CODEX_HOME", tmp.join("codex-home-conflict"))
        .env("HOME", tmp.join("home-conflict"))
        .env("DVANDVA_INSTALL_TEST_LOG", log.clone());
    let output = run.run(&[
        "--claude-only",
        "--codex-only",
        marketplace.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(contains(&combined(&output), "cannot be combined"));
    assert!(
        !log.exists(),
        "conflicting flags must not invoke any engine CLI"
    );
}

// ---------------------------------------------------------------------
// Ported: already-registered marketplace / already-installed plugin is
// tolerated for both engines
// ---------------------------------------------------------------------
#[test]
fn already_present_marketplace_and_plugin_are_tolerated() {
    let run = InstallRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("claude"), FAKE_CLAUDE);
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_MODERN);
    let log = tmp.join("already.log");

    let run = run
        .env("CODEX_HOME", tmp.join("codex-home-already"))
        .env("HOME", tmp.join("home-already"))
        .env("DVANDVA_INSTALL_TEST_LOG", log)
        .env("DVANDVA_INSTALL_TEST_ALREADY", "1");
    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(contains(
        &text,
        "Claude Code marketplace already present; continuing."
    ));
    assert!(contains(
        &text,
        "Claude Code plugin already present; continuing."
    ));
    assert!(contains(
        &text,
        "Codex marketplace already present; continuing."
    ));
    assert!(contains(&text, "Codex plugin already present; continuing."));
}

// ---------------------------------------------------------------------
// Ported: --claude-only installs only Claude Code
// ---------------------------------------------------------------------
#[test]
fn claude_only_skips_codex() {
    let run = InstallRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("claude"), FAKE_CLAUDE);
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_MODERN);
    let log = tmp.join("claude-only.log");

    let run = run
        .env("CODEX_HOME", tmp.join("codex-home-claude-only"))
        .env("HOME", tmp.join("home-claude-only"))
        .env("DVANDVA_INSTALL_TEST_LOG", log.clone());
    let output = run.run(&["--claude-only", marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let install_log = fs::read_to_string(&log).unwrap_or_default();
    assert!(install_log.contains(&format!(
        "claude plugin marketplace add {}",
        marketplace.display()
    )));
    assert!(install_log.contains("claude plugin install dvandva@dvandva"));
    assert!(
        !install_log.lines().any(|line| line.starts_with("codex ")),
        "claude-only install should not invoke codex: {install_log}"
    );
}

// ---------------------------------------------------------------------
// Ported: --codex-only installs only Codex
// ---------------------------------------------------------------------
#[test]
fn codex_only_skips_claude() {
    let run = InstallRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("claude"), FAKE_CLAUDE);
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_MODERN);
    let log = tmp.join("codex-only.log");

    let run = run
        .env("CODEX_HOME", tmp.join("codex-home-codex-only"))
        .env("HOME", tmp.join("home-codex-only"))
        .env("DVANDVA_INSTALL_TEST_LOG", log.clone());
    let output = run.run(&["--codex-only", marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let install_log = fs::read_to_string(&log).unwrap_or_default();
    assert!(install_log.contains(&format!(
        "codex plugin marketplace add {}",
        marketplace.display()
    )));
    assert!(install_log.contains("codex plugin add dvandva@dvandva"));
    assert!(
        !install_log.lines().any(|line| line.starts_with("claude ")),
        "codex-only install should not invoke claude: {install_log}"
    );
}

// ---------------------------------------------------------------------
// Extra: -h/--help prints usage to stdout and exits 0
// ---------------------------------------------------------------------
#[test]
fn help_flag_prints_usage_and_exits_zero() {
    let run = InstallRun::new().with_empty_path();
    let output = run.run(&["--help"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "stderr: {}", combined(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: dvandva install"));
    assert!(stdout.contains("--claude-only"));
    assert!(stdout.contains("--codex-only"));
}

// ---------------------------------------------------------------------
// Extra: an unknown flag exits 2
// ---------------------------------------------------------------------
#[test]
fn unknown_flag_exits_two() {
    let run = InstallRun::new().with_empty_path();
    let output = run.run(&["--bogus"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown option: --bogus"),
        "stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------
// Extra: missing `claude` on PATH exits 1 with the shell's exact message
// ---------------------------------------------------------------------
#[test]
fn missing_claude_cli_exits_one() {
    let run = InstallRun::new().with_empty_path();
    let output = run.run(&["--claude-only"]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ERROR: claude CLI not found on PATH"),
        "stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------
// Extra: missing `codex` on PATH exits 1 with the shell's exact message
// ---------------------------------------------------------------------
#[test]
fn missing_codex_cli_exits_one() {
    let run = InstallRun::new().with_empty_path();
    let output = run.run(&["--codex-only"]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ERROR: codex CLI not found on PATH"),
        "stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------
// Extra: `dvandva install`'s Codex branch delegates to the install-codex
// logic in-process, producing the same nested output a standalone
// `install-codex.sh` run would print (its own "Step N" lines and "Done."
// banner), plus install.sh's own wrapper lines around it.
// ---------------------------------------------------------------------
#[test]
fn install_delegates_to_codex_fallback_in_process_with_same_output() {
    let run = InstallRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_FALLBACK);
    let log = tmp.join("codex-fallback.log");

    let run = run
        .env("CODEX_HOME", tmp.join("codex-home-fallback"))
        .env("HOME", tmp.join("home-fallback"))
        .env("CODEX_FAKE_LOG", log);
    let output = run.run(&["--codex-only", marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(contains(&text, "Codex: installing dvandva plugin..."));
    assert!(contains(&text, "Step 1: registering marketplace"));
    assert!(contains(
        &text,
        "Step 2: installing dvandva plugin via legacy app-server RPC fallback..."
    ));
    assert!(contains(
        &text,
        "OK: dvandva@dvandva installed via app-server RPC"
    ));
    assert!(contains(
        &text,
        "Done. Verify with: codex, then check /skills for"
    ));
    assert!(contains(&text, "Codex install complete"));
    assert!(contains(
        &text,
        "Done. Verify the installed engine(s) can see"
    ));
}
