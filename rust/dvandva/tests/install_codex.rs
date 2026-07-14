//! Integration tests for `dvandva install-codex`, porting
//! `scripts/test-install-codex.sh` (excluding its
//! `assert_source_manifest_version_parity` / `assert_source_agent_roster`
//! preflight checks — generic repo-manifest validation unrelated to the
//! installer, already covered by `dvandva::smoke` / `tests/smoke.rs`).
//!
//! Fake `codex` executables are written as `#!/usr/bin/env bash` scripts
//! into a per-test tempdir and prepended onto `PATH`, mirroring the shell
//! suite's fixtures verbatim (same `case "$*"` argv matching).
//!
//! Cases past the ported set (marked "extra") close gaps the shell suite
//! doesn't exercise: a missing `codex` CLI, and `install-codex.sh`'s
//! `MARKETPLACE="${1:-...}"` behavior of silently ignoring any argument
//! past the first.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// `scripts/test-install-codex.sh`'s modern-path `codex` stub, verbatim.
const FAKE_CODEX_MODERN: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >> "$CODEX_FAKE_LOG"

case "$*" in
  "plugin add --help")
    cat <<'HELP'
Install a plugin from a configured marketplace snapshot.
Usage: codex plugin add [OPTIONS] <PLUGIN[@MARKETPLACE]>
HELP
    ;;
  plugin\ marketplace\ add\ *)
    if [[ "${CODEX_FAKE_ALREADY:-0}" == "1" ]]; then
      echo "Marketplace 'dvandva' already added" >&2
      exit 1
    fi
    mkdir -p "$CODEX_HOME/.tmp/marketplaces/dvandva/.agents/plugins"
    printf '{"name":"dvandva","plugins":[{"name":"dvandva"}]}\n' > "$CODEX_HOME/.tmp/marketplaces/dvandva/.agents/plugins/marketplace.json"
    ;;
  "plugin add dvandva@dvandva")
    if [[ "${CODEX_FAKE_ALREADY:-0}" == "1" ]]; then
      echo "Plugin 'dvandva@dvandva' already installed" >&2
      exit 1
    fi
    printf '{"id":"dvandva@dvandva","installed":true}\n'
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

struct InstallCodexRun {
    fake_bin: std::path::PathBuf,
    _tmp: tempfile::TempDir,
    envs: Vec<(String, OsString)>,
    replace_path: Option<OsString>,
}

impl InstallCodexRun {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fake_bin = tmp.path().join("bin");
        fs::create_dir_all(&fake_bin).unwrap();
        InstallCodexRun {
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
    /// simulate a truly absent `codex`, regardless of the host's own PATH).
    fn with_empty_path(mut self) -> Self {
        self.replace_path = Some(self.fake_bin.clone().into_os_string());
        self
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dvandva"));
        cmd.arg("install-codex").args(args);
        cmd.env(
            "PATH",
            self.replace_path
                .clone()
                .unwrap_or_else(|| prepend_path(&self.fake_bin)),
        );
        for (key, value) in &self.envs {
            cmd.env(key, value);
        }
        cmd.output().expect("failed to run dvandva install-codex")
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
// Ported: prefers `codex plugin add` when available
// ---------------------------------------------------------------------
#[test]
fn prefers_modern_codex_plugin_add_path() {
    let run = InstallCodexRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_MODERN);
    let log = tmp.join("codex.log");

    let run = run
        .env("CODEX_HOME", tmp.join("codex-home"))
        .env("HOME", tmp.join("home"))
        .env("CODEX_FAKE_LOG", log.clone());
    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let codex_log = fs::read_to_string(&log).unwrap_or_default();
    assert!(codex_log.contains(&format!("plugin marketplace add {}", marketplace.display())));
    assert!(codex_log.contains("plugin add dvandva@dvandva"));
    assert!(
        !codex_log.contains("app-server"),
        "unexpected app-server invocation: {codex_log}"
    );

    let text = combined(&output);
    assert!(contains(&text, "codex plugin add dvandva@dvandva"));
    assert!(contains(&text, "dvandva:research"));
    assert!(contains(&text, "dvandva:testing"));
    assert!(contains(&text, "dvandva:understanding"));
    assert!(contains(&text, "dvandva:worktree-setup"));
}

// ---------------------------------------------------------------------
// Ported: already-registered marketplace / already-installed plugin is
// tolerated on the modern path
// ---------------------------------------------------------------------
#[test]
fn already_present_marketplace_and_plugin_are_tolerated() {
    let run = InstallCodexRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_MODERN);
    let log = tmp.join("codex-already.log");

    let run = run
        .env("CODEX_HOME", tmp.join("codex-home-already"))
        .env("HOME", tmp.join("home-already"))
        .env("CODEX_FAKE_LOG", log)
        .env("CODEX_FAKE_ALREADY", "1");
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
        "Codex marketplace already present; continuing."
    ));
    assert!(contains(&text, "Codex plugin already present; continuing."));
}

// ---------------------------------------------------------------------
// Ported: legacy app-server JSON-RPC fallback when `plugin add` is
// unavailable
// ---------------------------------------------------------------------
#[test]
fn legacy_app_server_fallback_when_plugin_add_unavailable() {
    let run = InstallCodexRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_FALLBACK);
    let log = tmp.join("codex-fallback.log");

    let run = run
        .env("CODEX_HOME", tmp.join("codex-home-fallback"))
        .env("HOME", tmp.join("home-fallback"))
        .env("CODEX_FAKE_LOG", log.clone());
    let output = run.run(&[marketplace.to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let codex_log = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        codex_log.contains("app-server --listen stdio://"),
        "log: {codex_log}"
    );

    let text = combined(&output);
    assert!(contains(
        &text,
        "OK: dvandva@dvandva installed via app-server RPC"
    ));
    assert!(contains(&text, "dvandva:research"));
    assert!(contains(&text, "dvandva:testing"));
    assert!(contains(&text, "dvandva:understanding"));
    assert!(contains(&text, "dvandva:worktree-setup"));
}

/// Fake `codex` stub reproducing the observed real-world staleness bug:
/// `plugin marketplace add` copies the source marketplace.json into the
/// `CODEX_HOME` cache the first time, but on every later add treats the mere
/// existence of the cache directory as "already registered" and skips
/// re-copying — even when the source content changed underneath it — unless
/// the caller clears the cache first.
const FAKE_CODEX_STALE_CACHE: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >> "$CODEX_FAKE_LOG"

case "$1 $2 $3" in
  "plugin add --help")
    cat <<'HELP'
Install a plugin from a configured marketplace snapshot.
Usage: codex plugin add [OPTIONS] <PLUGIN[@MARKETPLACE]>
HELP
    ;;
  "plugin marketplace add")
    src="$4/.agents/plugins/marketplace.json"
    dest="$CODEX_HOME/.tmp/marketplaces/dvandva/.agents/plugins/marketplace.json"
    if [[ -f "$dest" ]]; then
      echo "Marketplace 'dvandva' already registered" >&2
      exit 1
    fi
    mkdir -p "$(dirname "$dest")"
    cp "$src" "$dest"
    ;;
  "plugin add dvandva@dvandva")
    printf '{"id":"dvandva@dvandva","installed":true}\n'
    ;;
  *)
    echo "unexpected fake codex invocation: $*" >&2
    exit 64
    ;;
esac
"#;

// ---------------------------------------------------------------------
// Reproduces the observed live bug: a re-install after the source
// marketplace changed must refresh the CODEX_HOME cache, not keep serving a
// stale copy from a prior install (fixed by removing the cache dir before
// every `codex plugin marketplace add`).
// ---------------------------------------------------------------------
#[test]
fn stale_marketplace_cache_is_refreshed_on_reinstall() {
    let run = InstallCodexRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    let marketplace_manifest = marketplace.join(".agents/plugins/marketplace.json");
    fs::write(
        &marketplace_manifest,
        r#"{"name":"dvandva","plugins":[{"name":"dvandva","version":"1.0.0"}]}"#,
    )
    .unwrap();
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_STALE_CACHE);
    let log = tmp.join("codex-stale.log");
    let codex_home = tmp.join("codex-home-stale");

    let run = run
        .env("CODEX_HOME", codex_home.clone())
        .env("HOME", tmp.join("home-stale"))
        .env("CODEX_FAKE_LOG", log);

    let first = run.run(&[marketplace.to_str().unwrap()]);
    assert_eq!(first.status.code(), Some(0), "stderr: {}", combined(&first));

    let cache_manifest =
        codex_home.join(".tmp/marketplaces/dvandva/.agents/plugins/marketplace.json");
    let first_contents = fs::read_to_string(&cache_manifest).unwrap();
    assert!(
        first_contents.contains("1.0.0"),
        "first install did not populate the cache: {first_contents}"
    );

    // Bump the source marketplace version, simulating a plugin release.
    fs::write(
        &marketplace_manifest,
        r#"{"name":"dvandva","plugins":[{"name":"dvandva","version":"2.0.0"}]}"#,
    )
    .unwrap();

    let second = run.run(&[marketplace.to_str().unwrap()]);
    assert_eq!(
        second.status.code(),
        Some(0),
        "stderr: {}",
        combined(&second)
    );

    let second_contents = fs::read_to_string(&cache_manifest).unwrap();
    assert!(
        second_contents.contains("2.0.0"),
        "stale marketplace cache was not refreshed on reinstall: {second_contents}"
    );
}

// ---------------------------------------------------------------------
// Extra: missing `codex` on PATH exits 1 with the shell's exact message
// ---------------------------------------------------------------------
#[test]
fn missing_codex_cli_exits_one() {
    let run = InstallCodexRun::new().with_empty_path();
    let output = run.run(&[]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ERROR: codex CLI not found on PATH"),
        "stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------
// Extra: install-codex.sh's `MARKETPLACE="${1:-...}"` only ever consults
// the first argument; further positional arguments are silently ignored
// rather than rejected (no flag/usage parsing exists in the shell source).
// ---------------------------------------------------------------------
#[test]
fn extra_positional_arguments_are_silently_ignored() {
    let run = InstallCodexRun::new();
    let tmp = run.tmp_path().to_path_buf();
    let marketplace = write_marketplace_fixture(&tmp);
    write_executable(&run.fake_bin.join("codex"), FAKE_CODEX_MODERN);
    let log = tmp.join("codex-extra-args.log");

    let run = run
        .env("CODEX_HOME", tmp.join("codex-home-extra"))
        .env("HOME", tmp.join("home-extra"))
        .env("CODEX_FAKE_LOG", log.clone());
    let output = run.run(&[
        marketplace.to_str().unwrap(),
        "unused-second-arg",
        "and-a-third",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let codex_log = fs::read_to_string(&log).unwrap_or_default();
    assert!(codex_log.contains(&format!("plugin marketplace add {}", marketplace.display())));
}
