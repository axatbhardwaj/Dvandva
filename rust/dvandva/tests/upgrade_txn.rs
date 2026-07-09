//! Integration tests for the transactional core of `dvandva upgrade`
//! (`src/upgrade_txn.rs` + `src/upgrade_txn_engines.rs`, landed at 34ed734),
//! driven through the real `dvandva upgrade` CLI with fake `cargo`/`claude`/
//! `codex` executables — mirroring `tests/upgrade.rs` and
//! `tests/install_codex.rs`'s hermetic-stub pattern (fake bin dir prepended
//! onto `PATH`, `HOME`/`CODEX_HOME` pointed at a per-test tempdir).
//!
//! Every failure-window test seeds distinguishable "old" content across all
//! six W0 snapshot targets (live binary, Claude's `installed_plugins.json`,
//! Claude's plugin cache dir, Codex's marketplace tmp dir, Codex's
//! `config.toml`, Codex's plugin cache dir), lets the fake engines mutate
//! them to "new" content before failing, and asserts every target is
//! restored byte-identical to its pre-upgrade snapshot plus the documented
//! exit-code taxonomy (0 committed / 20 rolled back / 21 rollback
//! incomplete).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// ---------------------------------------------------------------------
// Stub scripts
// ---------------------------------------------------------------------

/// Fake `cargo install dvandva --root <stage>`.
///
/// - `CARGO_STAGE_FAIL=1`: fails before ever writing a staged binary (W1
///   "build" sub-case).
/// - Otherwise writes `<stage>/bin/dvandva`, a bash script that reports
///   `dvandva $NEW_VERSION` on `--version`, except:
///   - `STAGE_VERIFY_FAIL=1` makes it report garbage when invoked from the
///     staging path (W1 "verify" sub-case).
///   - `POST_COMMIT_VERIFY_FAIL=1` makes it report garbage when invoked from
///     any *other* path, i.e. after `install_staged_binary_last` has
///     `fs::rename`d it onto the live binary path (W5).
const FAKE_CARGO: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf 'cargo %s\n' "$*" >> "$UPGRADE_TEST_LOG"

if [[ "$1 $2" != "install dvandva" || "$3" != "--root" ]]; then
  echo "unexpected fake cargo invocation: $*" >&2
  exit 64
fi

if [[ "${CARGO_STAGE_FAIL:-0}" == "1" ]]; then
  echo "simulated cargo build failure" >&2
  exit 101
fi

root="$4"
mkdir -p "$root/bin"

staged_output="dvandva ${NEW_VERSION:-9.9.9}"
if [[ "${STAGE_VERIFY_FAIL:-0}" == "1" ]]; then
  staged_output="corrupted-staged-output"
fi
commit_output="dvandva ${NEW_VERSION:-9.9.9}"
if [[ "${POST_COMMIT_VERIFY_FAIL:-0}" == "1" ]]; then
  commit_output="corrupted-post-commit-output"
fi

cat > "$root/bin/dvandva" <<SCRIPT
#!/usr/bin/env bash
printf 'binary-verify %s\n' "\$0" >> "$UPGRADE_TEST_LOG"
case "\$0" in
  */upgrade-staging/*) echo "$staged_output" ;;
  *) echo "$commit_output" ;;
esac
SCRIPT
chmod +x "$root/bin/dvandva"
echo "Installed package \`dvandva v${NEW_VERSION:-9.9.9}\` (executable \`dvandva\`)"
"#;

/// Fake `claude`: marketplace add always succeeds silently; `plugin install
/// dvandva@dvandva` mutates `installed_plugins.json` + the plugin cache dir
/// to "new" content unconditionally, *then* fails when
/// `CLAUDE_INSTALL_FAIL=1` (mirrors a real engine partially mutating state
/// before erroring out); `plugin update` always succeeds.
const FAKE_CLAUDE: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf 'claude %s\n' "$*" >> "$UPGRADE_TEST_LOG"

case "$*" in
  plugin\ marketplace\ add\ *)
    ;;
  "plugin install dvandva@dvandva")
    mkdir -p "$(dirname "$CLAUDE_INSTALLED_PLUGINS")"
    printf '%s' "$CLAUDE_NEW_INSTALLED_PLUGINS_JSON" > "$CLAUDE_INSTALLED_PLUGINS"
    mkdir -p "$CLAUDE_CACHE_BASE/$NEW_VERSION"
    printf '%s' "$CLAUDE_NEW_CACHE_MARKER" > "$CLAUDE_CACHE_BASE/$NEW_VERSION/marker.txt"
    if [[ -n "${LIVE_BINARY_SEEN_LOG:-}" ]]; then
      cp "$LIVE_BINARY_PATH" "$LIVE_BINARY_SEEN_LOG" 2>/dev/null || true
    fi
    if [[ "${CLAUDE_INSTALL_FAIL:-0}" == "1" ]]; then
      echo "simulated claude install failure" >&2
      exit 1
    fi
    ;;
  "plugin update dvandva@dvandva")
    echo "Updated dvandva to the latest version."
    ;;
  *)
    echo "unexpected fake claude invocation: $*" >&2
    exit 64
    ;;
esac
"#;

/// Fake `codex` (modern `plugin add` path). `plugin marketplace add` writes
/// the marketplace manifest + an extra checkout file to "new" content plus
/// appends a config stanza, then fails when `CODEX_MARKETPLACE_FAIL=1`.
/// `plugin add dvandva@dvandva` writes the plugin cache dir to "new" content,
/// then fails when `CODEX_ADD_FAIL=1`.
const FAKE_CODEX_MODERN: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf 'codex %s\n' "$*" >> "$UPGRADE_TEST_LOG"

case "$*" in
  "plugin add --help")
    cat <<'HELP'
Install a plugin from a configured marketplace snapshot.
Usage: codex plugin add [OPTIONS] <PLUGIN[@MARKETPLACE]>
HELP
    ;;
  plugin\ marketplace\ add\ *)
    mkdir -p "$(dirname "$CODEX_MARKETPLACE_MANIFEST")"
    printf '%s' "$CODEX_NEW_MARKETPLACE_JSON" > "$CODEX_MARKETPLACE_MANIFEST"
    printf '%s' "$CODEX_NEW_CHECKOUT_MARKER" > "$(dirname "$CODEX_MARKETPLACE_MANIFEST")/../../checkout.txt"
    printf '\n%s\n' "$CODEX_NEW_CONFIG_STANZA" >> "$CODEX_CONFIG_PATH"
    if [[ "${CODEX_MARKETPLACE_FAIL:-0}" == "1" ]]; then
      echo "simulated codex marketplace failure" >&2
      exit 1
    fi
    ;;
  "plugin add dvandva@dvandva")
    mkdir -p "$CODEX_CACHE_BASE/$NEW_VERSION"
    printf '%s' "$CODEX_NEW_CACHE_MARKER" > "$CODEX_CACHE_BASE/$NEW_VERSION/marker.txt"
    if [[ "${CODEX_ADD_FAIL:-0}" == "1" ]]; then
      echo "simulated codex plugin add failure" >&2
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

/// Fake `codex` with no `plugin add` support: forces the legacy app-server
/// JSON-RPC install fallback (`tests/install_codex.rs`'s `FAKE_CODEX_FALLBACK`
/// pattern). `plugin marketplace add` still mutates state like the modern
/// stub so the same restore assertions apply.
const FAKE_CODEX_RPC_FALLBACK: &str = r#"#!/usr/bin/env bash
set -euo pipefail

printf 'codex %s\n' "$*" >> "$UPGRADE_TEST_LOG"

case "$*" in
  "plugin add --help")
    echo "unknown command: plugin add" >&2
    exit 1
    ;;
  plugin\ marketplace\ add\ *)
    mkdir -p "$(dirname "$CODEX_MARKETPLACE_MANIFEST")"
    printf '%s' "$CODEX_NEW_MARKETPLACE_JSON" > "$CODEX_MARKETPLACE_MANIFEST"
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

// ---------------------------------------------------------------------
// Shared fixture plumbing (mirrors tests/upgrade.rs)
// ---------------------------------------------------------------------

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

/// Local-dir marketplace fixture named `dvandva`, matching
/// `local_marketplace_name`'s manifest-name resolution used by both
/// `refresh_marketplace_cache_at` (installers.rs) and
/// `codex_marketplace_cache_dir` (upgrade_txn_engines.rs) — the two
/// independent computations of the same cache directory name that this
/// suite's W3/W4 tests depend on agreeing.
fn write_marketplace_fixture(dir: &Path) -> PathBuf {
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

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

const OLD_VERSION: &str = "3.1.0";
const NEW_VERSION: &str = "3.2.0";

/// All six W0 snapshot targets, their pre-seeded "old" bytes, and the env
/// vars that steer the fake engines' "new" mutations — enough state to run
/// `dvandva upgrade` under any failure-window toggle and assert exactly
/// which targets must come back byte-identical.
struct TxnFixture {
    _tmp: tempfile::TempDir,
    fake_bin: PathBuf,
    marketplace: PathBuf,
    home: PathBuf,
    log: PathBuf,

    live_binary: PathBuf,
    claude_installed_plugins: PathBuf,
    claude_cache_base: PathBuf,
    codex_marketplace_manifest: PathBuf,
    codex_marketplace_checkout: PathBuf,
    codex_config: PathBuf,
    codex_cache_base: PathBuf,

    old_live_binary: Vec<u8>,
    old_claude_installed_plugins: String,
    old_claude_cache_marker: String,
    old_codex_marketplace_manifest: String,
    old_codex_marketplace_checkout: String,
    old_codex_config: String,
    old_codex_cache_marker: String,

    envs: Vec<(String, OsString)>,
}

impl TxnFixture {
    fn new(codex_stub: &str) -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        let fake_bin = root.join("bin");
        fs::create_dir_all(&fake_bin).unwrap();
        write_executable(&fake_bin.join("cargo"), FAKE_CARGO);
        write_executable(&fake_bin.join("claude"), FAKE_CLAUDE);
        write_executable(&fake_bin.join("codex"), codex_stub);

        let marketplace = write_marketplace_fixture(&root);
        let home = root.join("home");
        let codex_home = root.join("codex-home");
        let log = root.join("upgrade.log");

        let live_binary = home.join(".cargo/bin/dvandva");
        let claude_installed_plugins = home.join(".claude/plugins/installed_plugins.json");
        let claude_cache_base = home.join(".claude/plugins/cache/dvandva/dvandva");
        let codex_marketplace_dir = codex_home.join(".tmp/marketplaces/dvandva");
        let codex_marketplace_manifest =
            codex_marketplace_dir.join(".agents/plugins/marketplace.json");
        let codex_marketplace_checkout = codex_marketplace_dir.join("checkout.txt");
        let codex_config = codex_home.join("config.toml");
        let codex_cache_base = codex_home.join("plugins/cache/dvandva/dvandva");

        let old_live_binary = format!(
            "#!/usr/bin/env bash\nprintf 'binary-verify %s\\n' \"$0\" >> '{}'\necho \"dvandva {OLD_VERSION}\"\n",
            log.display()
        )
        .into_bytes();
        let old_claude_installed_plugins =
            r#"{"plugins":{"dvandva":{"version":"3.1.0","installPath":"/old"}}}"#.to_string();
        let old_claude_cache_marker = "old-claude-cache-marker".to_string();
        let old_codex_marketplace_manifest =
            r#"{"name":"dvandva","plugins":[{"name":"dvandva","version":"3.1.0"}]}"#.to_string();
        let old_codex_marketplace_checkout = "old-codex-checkout-marker".to_string();
        let old_codex_config = "[marketplaces.dvandva]\nsource = \"old\"\n".to_string();
        let old_codex_cache_marker = "old-codex-cache-marker".to_string();

        for path in [
            &live_binary,
            &claude_installed_plugins,
            &claude_cache_base.join(OLD_VERSION).join("marker.txt"),
            &codex_marketplace_manifest,
            &codex_marketplace_checkout,
            &codex_config,
            &codex_cache_base.join(OLD_VERSION).join("marker.txt"),
        ] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
        }
        fs::write(&live_binary, &old_live_binary).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&live_binary, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::write(&claude_installed_plugins, &old_claude_installed_plugins).unwrap();
        fs::write(
            claude_cache_base.join(OLD_VERSION).join("marker.txt"),
            &old_claude_cache_marker,
        )
        .unwrap();
        fs::write(&codex_marketplace_manifest, &old_codex_marketplace_manifest).unwrap();
        fs::write(&codex_marketplace_checkout, &old_codex_marketplace_checkout).unwrap();
        fs::write(&codex_config, &old_codex_config).unwrap();
        fs::write(
            codex_cache_base.join(OLD_VERSION).join("marker.txt"),
            &old_codex_cache_marker,
        )
        .unwrap();

        let live_binary_seen_log = root.join("live-binary-seen-during-plugins");

        let envs = vec![
            ("HOME".to_string(), home.clone().into_os_string()),
            (
                "CARGO_HOME".to_string(),
                home.join(".cargo").into_os_string(),
            ),
            (
                "CODEX_HOME".to_string(),
                codex_home.clone().into_os_string(),
            ),
            ("UPGRADE_TEST_LOG".to_string(), log.clone().into_os_string()),
            ("NEW_VERSION".to_string(), NEW_VERSION.into()),
            (
                "CLAUDE_INSTALLED_PLUGINS".to_string(),
                claude_installed_plugins.clone().into_os_string(),
            ),
            (
                "CLAUDE_NEW_INSTALLED_PLUGINS_JSON".to_string(),
                r#"{"plugins":{"dvandva":{"version":"3.2.0","installPath":"/new"}}}"#.into(),
            ),
            (
                "CLAUDE_CACHE_BASE".to_string(),
                claude_cache_base.clone().into_os_string(),
            ),
            (
                "CLAUDE_NEW_CACHE_MARKER".to_string(),
                "new-claude-cache-marker".into(),
            ),
            (
                "LIVE_BINARY_PATH".to_string(),
                live_binary.clone().into_os_string(),
            ),
            (
                "LIVE_BINARY_SEEN_LOG".to_string(),
                live_binary_seen_log.clone().into_os_string(),
            ),
            (
                "CODEX_MARKETPLACE_MANIFEST".to_string(),
                codex_marketplace_manifest.clone().into_os_string(),
            ),
            (
                "CODEX_NEW_MARKETPLACE_JSON".to_string(),
                r#"{"name":"dvandva","plugins":[{"name":"dvandva","version":"3.2.0"}]}"#.into(),
            ),
            (
                "CODEX_NEW_CHECKOUT_MARKER".to_string(),
                "new-codex-checkout-marker".into(),
            ),
            (
                "CODEX_CONFIG_PATH".to_string(),
                codex_config.clone().into_os_string(),
            ),
            (
                "CODEX_NEW_CONFIG_STANZA".to_string(),
                "[marketplaces.dvandva]\nsource = \"new\"".into(),
            ),
            (
                "CODEX_CACHE_BASE".to_string(),
                codex_cache_base.clone().into_os_string(),
            ),
            (
                "CODEX_NEW_CACHE_MARKER".to_string(),
                "new-codex-cache-marker".into(),
            ),
        ];

        TxnFixture {
            _tmp: tmp,
            fake_bin,
            marketplace,
            home,
            log,
            live_binary,
            claude_installed_plugins,
            claude_cache_base,
            codex_marketplace_manifest,
            codex_marketplace_checkout,
            codex_config,
            codex_cache_base,
            old_live_binary,
            old_claude_installed_plugins,
            old_claude_cache_marker,
            old_codex_marketplace_manifest,
            old_codex_marketplace_checkout,
            old_codex_config,
            old_codex_cache_marker,
            envs,
        }
    }

    fn tmp_path(&self) -> &Path {
        self._tmp.path()
    }

    fn run(&self, extra_envs: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dvandva"));
        cmd.arg("upgrade").arg(&self.marketplace);
        cmd.env("PATH", prepend_path(&self.fake_bin));
        cmd.env_remove("CARGO_INSTALL_ROOT");
        for (key, value) in &self.envs {
            cmd.env(key, value);
        }
        for (key, value) in extra_envs {
            cmd.env(key, value);
        }
        cmd.output().expect("failed to run dvandva upgrade")
    }

    fn log_contents(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// Every W0 target must be byte-identical to its pre-run snapshot: the
    /// core assertion for every rolled-back window test.
    fn assert_all_targets_restored_to_old(&self) {
        assert_eq!(
            fs::read(&self.live_binary).unwrap(),
            self.old_live_binary,
            "live binary was not restored byte-identically"
        );
        self.assert_plugin_targets_restored_to_old();
    }

    /// The five plugin-engine W0 targets only (everything except the live
    /// binary itself) — split out so a test that deliberately reshapes the
    /// live-binary target (e.g. into a directory, to force a
    /// `install_staged_binary_last` failure without going through
    /// `assert_all_targets_restored_to_old`'s byte-for-byte file read) can
    /// still reuse the plugin-side restore assertions verbatim.
    fn assert_plugin_targets_restored_to_old(&self) {
        assert_eq!(
            fs::read_to_string(&self.claude_installed_plugins).unwrap(),
            self.old_claude_installed_plugins,
            "claude installed_plugins.json was not restored"
        );
        assert_eq!(
            fs::read_to_string(self.claude_cache_base.join(OLD_VERSION).join("marker.txt"))
                .unwrap(),
            self.old_claude_cache_marker,
            "claude cache dir old marker missing after restore"
        );
        assert!(
            !self.claude_cache_base.join(NEW_VERSION).exists(),
            "claude cache dir still has the new-version subdir after restore"
        );
        assert_eq!(
            fs::read_to_string(&self.codex_marketplace_manifest).unwrap(),
            self.old_codex_marketplace_manifest,
            "codex marketplace manifest was not restored"
        );
        assert_eq!(
            fs::read_to_string(&self.codex_marketplace_checkout).unwrap(),
            self.old_codex_marketplace_checkout,
            "codex marketplace checkout file was not restored"
        );
        assert_eq!(
            fs::read_to_string(&self.codex_config).unwrap(),
            self.old_codex_config,
            "codex config.toml was not restored (append leaked through)"
        );
        assert_eq!(
            fs::read_to_string(self.codex_cache_base.join(OLD_VERSION).join("marker.txt")).unwrap(),
            self.old_codex_cache_marker,
            "codex cache dir old marker missing after restore"
        );
        assert!(
            !self.codex_cache_base.join(NEW_VERSION).exists(),
            "codex cache dir still has the new-version subdir after restore"
        );
    }

    fn breadcrumb_path(&self) -> PathBuf {
        self.home.join(".dvandva/upgrade.breadcrumb.json")
    }
}

// ---------------------------------------------------------------------
// Pre-W0: state_dir creation fails
// ---------------------------------------------------------------------

/// `fs::create_dir_all(&config.state_dir)` is the very first fallible
/// operation in `run_transactional_upgrade_inner` — earlier than lock
/// acquisition, snapshot creation, or any engine step. Making `HOME` itself
/// read-only (so `HOME/.dvandva` can never come into existence) must exit
/// 20 with zero lock/snapshot/engine side effects and every target
/// untouched.
#[cfg(unix)]
#[test]
fn state_dir_creation_failure_exits_20_before_any_lock_or_engine_step() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);
    let original_mode = fs::metadata(&fixture.home).unwrap().permissions().mode();
    fs::set_permissions(&fixture.home, fs::Permissions::from_mode(0o555)).unwrap();
    let _guard = PermGuard {
        path: fixture.home.clone(),
        mode: original_mode,
    };

    let output = fixture.run(&[]);

    assert_eq!(
        output.status.code(),
        Some(20),
        "stderr: {}",
        combined(&output)
    );
    assert!(
        combined(&output).contains("could not create upgrade state dir"),
        "text: {}",
        combined(&output)
    );
    fixture.assert_all_targets_restored_to_old();
    assert!(
        !fixture.home.join(".dvandva").exists(),
        "state dir must never come into existence when its own creation fails"
    );
    let log = fixture.log_contents();
    assert!(
        !log.contains("cargo ") && !log.contains("claude ") && !log.contains("codex "),
        "no engine stub should have been invoked when state dir creation fails; log:\n{log}"
    );
}

// ---------------------------------------------------------------------
// W0: lock/snapshot fail
// ---------------------------------------------------------------------

/// A lock held by another (live-looking) process must block the transaction
/// before any snapshot or engine step runs: exit 20, and since nothing ever
/// started, every target is (trivially but genuinely) still exactly the
/// pre-run "old" content, and no engine was ever invoked.
#[test]
fn w0_lock_held_by_another_process_blocks_before_any_mutation() {
    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);
    let lock_path = fixture.home.join(".dvandva/upgrade.lock");
    fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    fs::write(
        &lock_path,
        format!(
            "pid=999999999\ntimestamp={}\ntoken=999999999:{}\n",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ),
    )
    .unwrap();

    let output = fixture.run(&[]);

    assert_eq!(
        output.status.code(),
        Some(20),
        "stderr: {}",
        combined(&output)
    );
    assert!(
        combined(&output).contains("upgrade lock unavailable"),
        "text: {}",
        combined(&output)
    );
    fixture.assert_all_targets_restored_to_old();
    let log = fixture.log_contents();
    assert!(
        !log.contains("cargo ") && !log.contains("claude ") && !log.contains("codex "),
        "no engine stub should have been invoked while the lock was held; log:\n{log}"
    );
    // (`run_upgrade`'s unconditional trailing version-table report still
    // probes the untouched live binary with `--version`, which is why the
    // log isn't strictly empty — that probe is expected regardless of
    // transaction outcome.)
    // The lock file we planted (not ours) must still be there — the CLI
    // must not have torn down a lock it does not own.
    assert!(lock_path.exists());
}

// ---------------------------------------------------------------------
// Between snapshot and W1: write_breadcrumb fails
// ---------------------------------------------------------------------

/// `write_breadcrumb` runs immediately after `Snapshot::create` succeeds,
/// and before `stage_binary` or any engine step. Pointing the breadcrumb
/// path at a dangling symlink (whose target's parent directory never
/// exists) isolates *just* that one write: `Path::exists()` follows
/// symlinks and reports `false` for a dangling link, so the pre-flight
/// `breadcrumb_path().exists()` recovery check at the top of
/// `run_transactional_upgrade_inner` does not fire and a real snapshot gets
/// taken first — but `write_breadcrumb`'s own `fs::write` follows the same
/// symlink and fails with ENOENT, before any engine or the binary swap ever
/// runs.
#[cfg(unix)]
#[test]
fn write_breadcrumb_failure_after_successful_snapshot_exits_20_with_no_live_mutation() {
    use std::os::unix::fs::symlink;

    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);
    let breadcrumb_path = fixture.breadcrumb_path();
    fs::create_dir_all(breadcrumb_path.parent().unwrap()).unwrap();
    let unreachable_target = fixture
        .tmp_path()
        .join("breadcrumb-target-parent-does-not-exist")
        .join("upgrade.breadcrumb.json");
    symlink(&unreachable_target, &breadcrumb_path).unwrap();
    assert!(
        !breadcrumb_path.exists(),
        "a dangling symlink must read as non-existent so the recovery path does not fire"
    );

    let output = fixture.run(&[]);

    assert_eq!(
        output.status.code(),
        Some(20),
        "stderr: {}",
        combined(&output)
    );
    assert!(
        combined(&output).contains("could not write upgrade breadcrumb"),
        "text: {}",
        combined(&output)
    );
    fixture.assert_all_targets_restored_to_old();
    let log = fixture.log_contents();
    assert!(
        !log.contains("cargo ") && !log.contains("claude ") && !log.contains("codex "),
        "no engine stub should have been invoked before the breadcrumb write; log:\n{log}"
    );
}

// ---------------------------------------------------------------------
// W1: staged-binary build/verify fail
// ---------------------------------------------------------------------

/// `cargo install` itself failing (before any staged binary exists) rolls
/// back before either engine ever runs.
#[test]
fn w1_cargo_build_failure_rolls_back_before_any_engine_mutation() {
    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);

    let output = fixture.run(&[("CARGO_STAGE_FAIL", "1")]);

    assert_eq!(
        output.status.code(),
        Some(20),
        "stderr: {}",
        combined(&output)
    );
    fixture.assert_all_targets_restored_to_old();
    let log = fixture.log_contents();
    assert!(
        !log.contains("claude ") && !log.contains("codex "),
        "engines must not run after a staged-binary build failure; log:\n{log}"
    );
}

/// Staged binary builds successfully but reports the wrong `--version`
/// output: `verify_binary` rejects it before `upgrade_plugins` or the binary
/// swap ever run, so the live binary is provably never touched (item 3).
#[test]
fn w1_staged_binary_verify_failure_leaves_live_binary_untouched() {
    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);

    let output = fixture.run(&[("STAGE_VERIFY_FAIL", "1")]);

    assert_eq!(
        output.status.code(),
        Some(20),
        "stderr: {}",
        combined(&output)
    );
    assert!(
        combined(&output).contains("verify-staged-binary"),
        "text: {}",
        combined(&output)
    );
    fixture.assert_all_targets_restored_to_old();
    let log = fixture.log_contents();
    assert!(
        !log.contains("claude ") && !log.contains("codex "),
        "engines must not run after a staged-binary verify failure; log:\n{log}"
    );
}

// ---------------------------------------------------------------------
// W2: claude-side fail
// ---------------------------------------------------------------------

/// Claude's `plugin install` mutates `installed_plugins.json` + its cache
/// dir, then fails. Codex's install runs regardless (the two engines are not
/// short-circuited) and succeeds, mutating its own state too — proving the
/// "plugins" step is all-or-nothing: a claude-only failure still rolls back
/// codex's successful mutation.
#[test]
fn w2_claude_install_failure_rolls_back_both_engines() {
    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);

    let output = fixture.run(&[("CLAUDE_INSTALL_FAIL", "1")]);

    assert_eq!(
        output.status.code(),
        Some(20),
        "stderr: {}",
        combined(&output)
    );
    let log = fixture.log_contents();
    assert!(
        log.contains("codex plugin add dvandva@dvandva"),
        "codex must still have run even though claude failed; log:\n{log}"
    );
    fixture.assert_all_targets_restored_to_old();
}

// ---------------------------------------------------------------------
// W3: codex marketplace destructive-clear then fail
// ---------------------------------------------------------------------

/// `installers::refresh_marketplace_cache_at` unconditionally
/// `remove_dir_all`s the codex marketplace tmp dir *before* `codex plugin
/// marketplace add` ever runs (staleness fix from `install_codex.rs`). The
/// W0 snapshot must have captured that directory before the destructive
/// clear, so even though the on-disk directory is gone by the time the
/// stub's `marketplace add` fails, rollback still restores the pre-clear
/// content exactly.
#[test]
fn w3_codex_marketplace_destructive_clear_then_failure_restores_old_marketplace() {
    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);

    let output = fixture.run(&[("CODEX_MARKETPLACE_FAIL", "1")]);

    assert_eq!(
        output.status.code(),
        Some(20),
        "stderr: {}",
        combined(&output)
    );
    let log = fixture.log_contents();
    assert!(
        !log.contains("plugin add dvandva@dvandva"),
        "codex plugin add must not run after marketplace add failed; log:\n{log}"
    );
    fixture.assert_all_targets_restored_to_old();
}

// ---------------------------------------------------------------------
// W4: codex add fail
// ---------------------------------------------------------------------

/// Marketplace registration succeeds (new marketplace content + config
/// stanza committed to disk), then `codex plugin add` mutates the cache dir
/// and fails. Rollback must restore all of it, including the marketplace
/// state that had already "succeeded".
#[test]
fn w4_codex_plugin_add_failure_after_marketplace_success_rolls_back_everything() {
    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);

    let output = fixture.run(&[("CODEX_ADD_FAIL", "1")]);

    assert_eq!(
        output.status.code(),
        Some(20),
        "stderr: {}",
        combined(&output)
    );
    let log = fixture.log_contents();
    assert!(log.contains("codex plugin marketplace add"), "log:\n{log}");
    assert!(log.contains("plugin add dvandva@dvandva"), "log:\n{log}");
    fixture.assert_all_targets_restored_to_old();
}

// ---------------------------------------------------------------------
// Between W4 and W5: install_staged_binary_last fails
// ---------------------------------------------------------------------

/// The narrow window between "plugins fully committed" (past the W4
/// boundary) and "post-commit verify runs" (W5): both engines succeed and
/// mutate their live state, then `install_staged_binary_last`'s own
/// `fs::rename(&tmp, live_binary)` fails — because the live binary path is
/// (deliberately, for this test only) a non-empty directory rather than a
/// regular file, so `rename` can never replace it — before `verify_committed`
/// ever runs. Unlike making the live binary's *parent* directory read-only
/// (which would also block `restore_snapshot`'s own remove-then-recopy of
/// that very target during rollback, producing exit 21 instead of 20), this
/// leaves the parent directory fully writable throughout, so rollback can
/// cleanly `copy_dir_all` the snapshot back and the run still exits 20.
/// Rollback must undo the already-committed plugin mutations in addition to
/// restoring the live-binary directory to its exact pre-run contents.
#[cfg(unix)]
#[test]
fn install_staged_binary_last_failure_rolls_back_committed_plugin_state() {
    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);

    // Replace the live-binary *file* the fixture just seeded with a
    // non-empty *directory* at the exact same path, so `install_staged_binary_last`'s
    // `fs::rename` onto it fails (a file can never be renamed over a
    // non-empty directory) while the containing `.cargo/bin` directory
    // itself stays fully writable.
    fs::remove_file(&fixture.live_binary).unwrap();
    fs::create_dir_all(&fixture.live_binary).unwrap();
    let marker = fixture.live_binary.join("marker.txt");
    fs::write(&marker, "old-live-binary-dir-marker").unwrap();

    let output = fixture.run(&[]);

    assert_eq!(
        output.status.code(),
        Some(20),
        "stderr: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(
        text.contains("binary-commit"),
        "must fail at the binary-commit stage, not verify-committed; text: {text}"
    );
    assert!(
        !text.contains("verify-committed-binary"),
        "verify_committed must never run once install_staged_binary_last fails; text: {text}"
    );

    let log = fixture.log_contents();
    assert!(
        log.contains("claude plugin install dvandva@dvandva"),
        "claude must have fully committed before the binary swap; log:\n{log}"
    );
    assert!(
        log.contains("codex plugin add dvandva@dvandva"),
        "codex must have fully committed before the binary swap; log:\n{log}"
    );
    assert!(
        log.contains("claude plugin update dvandva@dvandva"),
        "claude's post-install update call must have run too; log:\n{log}"
    );

    // The money assertion: plugin state was genuinely mutated to "new" and
    // then rolled back.
    fixture.assert_plugin_targets_restored_to_old();

    // ...and the live-binary directory itself is back to its exact pre-run
    // shape: still a directory, still holding only the original marker with
    // its original content — the swap never actually landed.
    assert!(
        fixture.live_binary.is_dir(),
        "live binary path must still be the original directory, not a swapped-in file"
    );
    assert_eq!(
        fs::read_to_string(&marker).unwrap(),
        "old-live-binary-dir-marker",
        "live binary directory's contents must be restored byte-identically"
    );
    assert_eq!(
        fs::read_dir(&fixture.live_binary).unwrap().count(),
        1,
        "no stray entries should have been left inside the live binary directory"
    );
}

// ---------------------------------------------------------------------
// W5: post-verify mismatch
// ---------------------------------------------------------------------

/// Everything succeeds through the binary swap (`install_staged_binary_last`
/// runs, live binary now holds "new" bytes), then `verify_committed` rejects
/// the live binary's `--version` output. Rollback must swap the live binary
/// back to its exact old bytes — the money case for "commit-ordering means
/// the swap can still be undone".
#[test]
fn w5_post_commit_verify_mismatch_swaps_live_binary_back_to_old() {
    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);

    let output = fixture.run(&[("POST_COMMIT_VERIFY_FAIL", "1")]);

    assert_eq!(
        output.status.code(),
        Some(20),
        "stderr: {}",
        combined(&output)
    );
    assert!(
        combined(&output).contains("verify-committed-binary"),
        "text: {}",
        combined(&output)
    );
    let log = fixture.log_contents();
    assert!(
        log.contains("plugin add dvandva@dvandva"),
        "codex must have fully run before the swap+post-verify; log:\n{log}"
    );
    // Prove the swap really happened before rollback: at least one
    // "binary-verify" line logged against the live path (not the staging
    // path) — the failing `verify_committed` probe against the just-swapped
    // binary. (`run_upgrade`'s unconditional trailing version-table report
    // adds a second live-path probe afterward, against whatever the binary
    // is by then — which `assert_all_targets_restored_to_old` below proves
    // is back to the old bytes.)
    let live_verify_calls = log
        .lines()
        .filter(|l| l.starts_with("binary-verify") && !l.contains("upgrade-staging"))
        .count();
    assert!(
        live_verify_calls >= 1,
        "expected at least one live-path --version probe (verify_committed); log:\n{log}"
    );
    fixture.assert_all_targets_restored_to_old();
}

// ---------------------------------------------------------------------
// rollback-itself-fails -> exit 21
// ---------------------------------------------------------------------

/// Drop guard restoring directory permissions even if an assertion panics,
/// so the tempdir's own cleanup never trips over a read-only directory.
struct PermGuard {
    path: PathBuf,
    #[cfg(unix)]
    mode: u32,
}

#[cfg(unix)]
impl Drop for PermGuard {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode));
    }
}

/// When restoring one target fails (its parent directory is made
/// unwritable), the transaction must exit 21 ("rollback incomplete") and
/// report the specific residual path — while every *other* target still
/// gets restored cleanly, and the crash breadcrumb survives for later
/// inspection.
#[cfg(unix)]
#[test]
fn rollback_itself_fails_exits_21_with_precise_residual_report() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);
    let unwritable_dir = fixture
        .claude_installed_plugins
        .parent()
        .unwrap()
        .to_path_buf();
    let original_mode = fs::metadata(&unwritable_dir).unwrap().permissions().mode();
    fs::set_permissions(&unwritable_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let _guard = PermGuard {
        path: unwritable_dir.clone(),
        mode: original_mode,
    };

    let output = fixture.run(&[("CLAUDE_INSTALL_FAIL", "1")]);

    assert_eq!(
        output.status.code(),
        Some(21),
        "stderr: {}",
        combined(&output)
    );
    let text = combined(&output);
    assert!(text.contains("rollback incomplete"), "text: {text}");
    assert!(
        text.contains(&fixture.claude_installed_plugins.display().to_string()),
        "residual report should name the unrestorable target; text: {text}"
    );
    assert!(
        fixture.breadcrumb_path().exists(),
        "an incomplete rollback must leave the breadcrumb for later recovery"
    );

    // Every other target — outside the deliberately-broken directory — must
    // still have been restored cleanly.
    assert_eq!(
        fs::read(&fixture.live_binary).unwrap(),
        fixture.old_live_binary
    );
    assert_eq!(
        fs::read_to_string(&fixture.codex_marketplace_manifest).unwrap(),
        fixture.old_codex_marketplace_manifest
    );
    assert_eq!(
        fs::read_to_string(&fixture.codex_config).unwrap(),
        fixture.old_codex_config
    );
}

// ---------------------------------------------------------------------
// commit-ordering: binary swap happens LAST
// ---------------------------------------------------------------------

/// Full success run: proves via the stub call-order log that (a) the live
/// binary still held its *old* bytes at the moment Claude's plugin install
/// ran (captured by the stub mid-run), and (b) only after cargo, both
/// engines, and the claude cache-bump all completed does the live binary
/// become the new binary and get its final `--version` probe
/// (`verify_committed`).
#[test]
fn commit_ordering_binary_swap_happens_last_on_a_successful_run() {
    let fixture = TxnFixture::new(FAKE_CODEX_MODERN);
    let seen_log = fixture.tmp_path().join("live-binary-seen-during-plugins");

    let output = fixture.run(&[]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );

    let seen_during_plugins = fs::read(&seen_log).expect("claude stub should have copied it");
    assert_eq!(
        seen_during_plugins, fixture.old_live_binary,
        "the live binary must still be the OLD binary while plugins are being upgraded"
    );

    assert!(
        fs::read_to_string(&fixture.live_binary)
            .unwrap()
            .contains(NEW_VERSION),
        "the live binary must be swapped to NEW by the end of a successful run"
    );

    let log = fixture.log_contents();
    let cargo_pos = log.find("cargo install dvandva").unwrap();
    let claude_install_pos = log.find("claude plugin install dvandva@dvandva").unwrap();
    let codex_add_pos = log.find("codex plugin add dvandva@dvandva").unwrap();
    let claude_update_pos = log.find("claude plugin update dvandva@dvandva").unwrap();
    let final_live_verify_pos = log
        .rfind("binary-verify")
        .expect("post-commit verify must have run");

    assert!(cargo_pos < claude_install_pos);
    assert!(cargo_pos < codex_add_pos);
    assert!(claude_install_pos < claude_update_pos);
    assert!(codex_add_pos < claude_update_pos);
    // The final binary-verify entry (verify_committed, on the just-swapped
    // live path) is strictly the last thing logged — after every plugin
    // mutation call.
    assert!(claude_update_pos < final_live_verify_pos, "log:\n{log}");
    assert!(codex_add_pos < final_live_verify_pos, "log:\n{log}");
    assert!(
        !log[final_live_verify_pos..]
            .lines()
            .next()
            .unwrap()
            .contains("upgrade-staging"),
        "the final binary-verify call must be on the live path, not the staging path; log:\n{log}"
    );
}

// ---------------------------------------------------------------------
// RPC-fallback window
// ---------------------------------------------------------------------

/// The legacy app-server JSON-RPC install path (no `codex plugin add`
/// support) is wired transparently through the transactional wrapper: a
/// full `dvandva upgrade` run using that fallback still commits (exit 0),
/// proving the RPC-fallback route is covered end-to-end by the transaction
/// core rather than only the standalone `install-codex` path.
#[test]
fn rpc_fallback_codex_route_succeeds_through_transactional_upgrade() {
    let fixture = TxnFixture::new(FAKE_CODEX_RPC_FALLBACK);

    let output = fixture.run(&[]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        combined(&output)
    );
    let log = fixture.log_contents();
    assert!(log.contains("app-server --listen stdio://"), "log:\n{log}");
    assert!(
        !log.contains("plugin add dvandva@dvandva"),
        "modern plugin-add path must not run under the RPC fallback; log:\n{log}"
    );
    assert!(fs::read_to_string(&fixture.live_binary)
        .unwrap()
        .contains(NEW_VERSION));
}
