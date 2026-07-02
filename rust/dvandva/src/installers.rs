//! Installer logic behind `dvandva install` / `dvandva install-codex`.
//!
//! Ports `scripts/install.sh` and `scripts/install-codex.sh`. Both scripts
//! are imperative orchestrations of side-effecting subprocess calls
//! interleaved with progress `echo`s (the `run_idempotent` helper's
//! conditional output, the per-step banners) — unlike the read-path modules
//! (`resolve`, `state`), this module owns both the subprocess side effects
//! and the printing; `cmd::install` / `cmd::install_codex` only parse
//! arguments and delegate.
//!
//! `install.sh`'s Codex branch shells out to `install-codex.sh` as a child
//! process, so its own progress lines (including its final "Done." banner)
//! appear in `install.sh`'s combined output too. [`run_install`] reproduces
//! this by calling [`run_install_codex`] in-process — same printed output,
//! no subprocess spawn of the `dvandva` binary itself.

use std::env;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{json, Value};

/// Default marketplace when the caller supplies none.
pub const DEFAULT_MARKETPLACE: &str = "axatbhardwaj/Dvandva";

/// Which engine(s) `dvandva install` should target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallTargets {
    pub claude: bool,
    pub codex: bool,
}

/// Timeout for each app-server JSON-RPC response, mirroring the legacy
/// Python fallback's `read_response(..., timeout=30)`.
const APP_SERVER_RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum recursion depth for the `find`-equivalent marketplace.json
/// search fallback (bounds an otherwise unbounded filesystem walk).
const MAX_MARKETPLACE_SEARCH_DEPTH: usize = 8;

// ---------------------------------------------------------------------
// `dvandva install` — ports install.sh
// ---------------------------------------------------------------------

/// Runs the `install` flow, printing exactly like `install.sh` and
/// returning the effective process exit code.
pub fn run_install(targets: InstallTargets, marketplace: &str) -> i32 {
    if targets.claude {
        if !command_exists("claude") {
            eprintln!("ERROR: claude CLI not found on PATH");
            return 1;
        }

        println!("Claude Code: registering marketplace '{marketplace}'...");
        let code = run_idempotent(
            "Claude Code marketplace",
            "claude",
            &["plugin", "marketplace", "add", marketplace],
        );
        if code != 0 {
            return code;
        }

        println!("Claude Code: installing dvandva plugin...");
        let code = run_idempotent(
            "Claude Code plugin",
            "claude",
            &["plugin", "install", "dvandva@dvandva"],
        );
        if code != 0 {
            return code;
        }

        println!("Claude Code install complete");
    }

    if targets.codex {
        if !command_exists("codex") {
            eprintln!("ERROR: codex CLI not found on PATH");
            return 1;
        }

        println!("Codex: installing dvandva plugin...");
        let code = run_install_codex(marketplace);
        if code != 0 {
            return code;
        }

        println!("Codex install complete");
    }

    println!(
        "Done. Verify the installed engine(s) can see dvandva:vadi, dvandva:prativadi, \
         dvandva:research, dvandva:testing, dvandva:understanding, and dvandva:worktree-setup \
         in /skills."
    );
    0
}

// ---------------------------------------------------------------------
// `dvandva install-codex` — ports install-codex.sh
// ---------------------------------------------------------------------

/// Runs the Codex install flow, printing exactly like `install-codex.sh`
/// and returning the effective process exit code. Shared in-process by
/// `cmd::install_codex` (standalone) and [`run_install`] (Codex branch).
pub fn run_install_codex(marketplace: &str) -> i32 {
    if !command_exists("codex") {
        eprintln!("ERROR: codex CLI not found on PATH");
        return 1;
    }

    println!("Step 1: registering marketplace '{marketplace}'...");
    let code = run_idempotent(
        "Codex marketplace",
        "codex",
        &["plugin", "marketplace", "add", marketplace],
    );
    if code != 0 {
        return code;
    }

    if command_probe_succeeds("codex", &["plugin", "add", "--help"]) {
        println!("Step 2: installing dvandva plugin with: codex plugin add dvandva@dvandva");
        let code = run_idempotent(
            "Codex plugin",
            "codex",
            &["plugin", "add", "dvandva@dvandva"],
        );
        if code != 0 {
            return code;
        }
        print_codex_done();
        return 0;
    }

    let codex_home = codex_home_dir();
    let marketplace_path =
        resolve_marketplace_path(marketplace, &codex_home).filter(|path| path.is_file());
    let Some(marketplace_path) = marketplace_path else {
        eprintln!("ERROR: could not find Dvandva marketplace.json after marketplace registration");
        return 1;
    };

    println!("Step 2: installing dvandva plugin via legacy app-server RPC fallback...");
    match install_via_app_server_rpc(&marketplace_path) {
        Ok(()) => {
            println!("OK: dvandva@dvandva installed via app-server RPC");
            print_codex_done();
            0
        }
        Err(err) => {
            eprintln!("ERROR: {err}");
            1
        }
    }
}

fn print_codex_done() {
    println!(
        "Done. Verify with: codex, then check /skills for dvandva:vadi, dvandva:prativadi, \
         dvandva:research, dvandva:testing, dvandva:understanding, and dvandva:worktree-setup."
    );
}

// ---------------------------------------------------------------------
// Shared subprocess helpers
// ---------------------------------------------------------------------

/// `command -v NAME`: true when an executable regular file named `name`
/// exists in some `PATH` directory.
fn command_exists(name: &str) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_var).any(|dir| is_executable_file(&dir.join(name)))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// A bare boolean probe: `command args... >/dev/null 2>&1` followed by an
/// exit-status check (output is discarded either way).
fn command_probe_succeeds(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Ports the shell `run_idempotent` helper: spawn `command args...`, and
/// treat a failure whose combined stdout+stderr matches the "already
/// present" pattern (case-insensitive) as success. Prints progress exactly
/// like the shell function and returns the effective exit code.
fn run_idempotent(label: &str, command: &str, args: &[&str]) -> i32 {
    let (combined, exit_code) = match Command::new(command).args(args).output() {
        Ok(Output {
            status,
            stdout,
            stderr,
        }) => {
            let combined = combine_output(&stdout, &stderr);
            if status.success() {
                (combined, 0)
            } else {
                (combined, status.code().unwrap_or(1))
            }
        }
        Err(err) => (format!("ERROR: failed to execute {command}: {err}"), 127),
    };

    if exit_code == 0 {
        if !combined.is_empty() {
            println!("{combined}");
        }
        return 0;
    }

    if !combined.is_empty() {
        eprintln!("{combined}");
    }
    if already_present_pattern().is_match(&combined) {
        println!("{label} already present; continuing.");
        return 0;
    }

    exit_code
}

/// Mirrors `$("$@" 2>&1)`: concatenate stdout and stderr, then strip
/// trailing newlines the way command substitution does.
fn combine_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut bytes = Vec::with_capacity(stdout.len() + stderr.len());
    bytes.extend_from_slice(stdout);
    bytes.extend_from_slice(stderr);
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    while text.ends_with('\n') {
        text.pop();
    }
    text
}

/// `grep -Eiq 'already|exists|registered|installed|duplicate'`.
fn already_present_pattern() -> Regex {
    Regex::new("(?i)already|exists|registered|installed|duplicate").expect("static regex")
}

// ---------------------------------------------------------------------
// Marketplace.json path resolution (legacy app-server fallback)
// ---------------------------------------------------------------------

/// `${CODEX_HOME:-$HOME/.codex}`.
fn codex_home_dir() -> PathBuf {
    compute_codex_home_dir(env::var_os("CODEX_HOME"), env::var_os("HOME"))
}

/// Pure computation behind [`codex_home_dir`], split out for testability
/// without mutating process-global environment variables.
///
/// Mirrors the shell's literal `${HOME}/.codex` string concatenation: an
/// empty or unset `HOME` still yields the absolute path `/.codex`, not the
/// current-directory-relative `.codex` that `PathBuf::join` would produce
/// for an empty base.
fn compute_codex_home_dir(codex_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    match codex_home {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => {
            let home = home.unwrap_or_default();
            PathBuf::from(format!("{}/.codex", home.to_string_lossy()))
        }
    }
}

/// Resolves the marketplace.json path the app-server expects: a local
/// directory argument resolves directly (no `find` fallback, matching the
/// shell's local-dir branch); a remote/repo-style argument computes the
/// cached-checkout default path and falls back to a bounded recursive
/// search for a `dvandva`-named marketplace.json.
///
/// Returns the computed path even when it may not exist yet — callers must
/// check existence (mirrors the shell's `[[ ! -f "$MARKETPLACE_PATH" ]]`
/// final gate).
fn resolve_marketplace_path(marketplace: &str, codex_home: &Path) -> Option<PathBuf> {
    let candidate = Path::new(marketplace);
    if candidate.is_dir() {
        let canonical = candidate.canonicalize().ok()?;
        return Some(canonical.join(".agents/plugins/marketplace.json"));
    }

    let name = marketplace_name(marketplace);
    let default_path = codex_home
        .join(".tmp/marketplaces")
        .join(&name)
        .join(".agents/plugins/marketplace.json");
    if default_path.is_file() {
        return Some(default_path);
    }

    find_dvandva_marketplace_json(&codex_home.join(".tmp/marketplaces"))
}

/// `basename "${MARKETPLACE%.git}" | tr '[:upper:]' '[:lower:]'`.
fn marketplace_name(marketplace: &str) -> String {
    let trimmed = marketplace.strip_suffix(".git").unwrap_or(marketplace);
    let base = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed);
    base.to_ascii_lowercase()
}

/// `find "$dir" -path '*/.agents/plugins/marketplace.json' -type f -print`,
/// piped through a `grep` for a `"name": "dvandva"` field, first match wins.
///
/// Bounded to [`MAX_MARKETPLACE_SEARCH_DEPTH`] and skips symlinked entries
/// (the shell's unbounded `find` has no such cap, but an unbounded
/// filesystem walk is not an acceptable port here).
fn find_dvandva_marketplace_json(root: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    collect_marketplace_json_candidates(root, 0, &mut candidates);
    candidates.sort();
    candidates
        .into_iter()
        .find(|path| marketplace_json_names_dvandva(path))
}

fn collect_marketplace_json_candidates(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_MARKETPLACE_SEARCH_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_marketplace_json_candidates(&path, depth + 1, out);
        } else if file_type.is_file() && is_agents_plugins_marketplace_json(&path) {
            out.push(path);
        }
    }
}

/// True when `path`'s last three components are `.agents/plugins/marketplace.json`.
fn is_agents_plugins_marketplace_json(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some("marketplace.json")
        && path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("plugins")
        && path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some(".agents")
}

fn marketplace_json_names_dvandva(path: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    dvandva_name_pattern().is_match(&contents)
}

/// `grep -q '"name"[[:space:]]*:[[:space:]]*"dvandva"'`.
fn dvandva_name_pattern() -> Regex {
    Regex::new(r#""name"\s*:\s*"dvandva""#).expect("static regex")
}

// ---------------------------------------------------------------------
// Legacy app-server JSON-RPC fallback (replaces the python3 heredoc)
// ---------------------------------------------------------------------

/// Failure modes of the legacy `codex app-server` JSON-RPC install.
#[derive(Debug)]
enum AppServerError {
    Spawn(std::io::Error),
    Io(std::io::Error),
    Timeout(u64),
    Rpc(String),
}

impl std::fmt::Display for AppServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppServerError::Spawn(err) => write!(f, "failed to start codex app-server: {err}"),
            AppServerError::Io(err) => write!(f, "app-server RPC I/O error: {err}"),
            AppServerError::Timeout(id) => {
                write!(f, "timed out waiting for app-server response id={id}")
            }
            AppServerError::Rpc(message) => write!(f, "plugin/install failed: {message}"),
        }
    }
}

/// Drives `codex app-server --listen stdio://` over JSON-RPC: `initialize`,
/// `initialized`, then `plugin/install`. Ports the install-codex.sh python3
/// heredoc frame-for-frame (see module docs); native, no python3 required.
fn install_via_app_server_rpc(marketplace_path: &Path) -> Result<(), AppServerError> {
    let mut child = Command::new("codex")
        .args(["app-server", "--listen", "stdio://"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(AppServerError::Spawn)?;

    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");

    let (tx, rx) = mpsc::channel::<String>();
    let reader_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let result = run_app_server_handshake(&mut stdin, &rx, marketplace_path);

    drop(stdin);
    // The shell used SIGTERM, then a grace period, then SIGKILL; this sends
    // SIGKILL only, since std has no portable SIGTERM without a new
    // dependency and the app-server subprocess here is short-lived.
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader_handle.join();

    result
}

fn run_app_server_handshake(
    stdin: &mut ChildStdin,
    rx: &Receiver<String>,
    marketplace_path: &Path,
) -> Result<(), AppServerError> {
    send_request(
        stdin,
        1,
        "initialize",
        Some(json!({
            "clientInfo": {"name": "dvandva-install", "version": "0"},
            "capabilities": {"experimentalApi": true},
        })),
    )?;
    read_response(rx, 1, APP_SERVER_RPC_TIMEOUT)?;

    send_notification(stdin, "initialized")?;

    let marketplace_path_str = marketplace_path.to_string_lossy().into_owned();
    send_request(
        stdin,
        2,
        "plugin/install",
        Some(json!({
            "marketplacePath": marketplace_path_str,
            "pluginName": "dvandva",
            "remoteMarketplaceName": Value::Null,
        })),
    )?;
    let response = read_response(rx, 2, APP_SERVER_RPC_TIMEOUT)?;
    if let Some(error) = response.get("error") {
        if json_truthy(error) {
            return Err(AppServerError::Rpc(error.to_string()));
        }
    }

    Ok(())
}

fn send_request(
    stdin: &mut ChildStdin,
    id: u64,
    method: &str,
    params: Option<Value>,
) -> Result<(), AppServerError> {
    let mut msg = serde_json::Map::new();
    msg.insert("id".to_string(), Value::from(id));
    msg.insert("method".to_string(), Value::from(method));
    if let Some(params) = params {
        msg.insert("params".to_string(), params);
    }
    write_json_line(stdin, &Value::Object(msg))
}

fn send_notification(stdin: &mut ChildStdin, method: &str) -> Result<(), AppServerError> {
    write_json_line(stdin, &json!({"method": method}))
}

fn write_json_line(stdin: &mut ChildStdin, value: &Value) -> Result<(), AppServerError> {
    let text = serde_json::to_string(value)
        .map_err(|err| AppServerError::Io(std::io::Error::other(err)))?;
    writeln!(stdin, "{text}").map_err(AppServerError::Io)?;
    stdin.flush().map_err(AppServerError::Io)
}

fn read_response(
    rx: &Receiver<String>,
    want_id: u64,
    timeout: Duration,
) -> Result<Value, AppServerError> {
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(AppServerError::Timeout(want_id));
        }
        match rx.recv_timeout(deadline - now) {
            Ok(line) => {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if value.get("id").and_then(Value::as_u64) == Some(want_id) {
                        return Ok(value);
                    }
                }
            }
            Err(_) => return Err(AppServerError::Timeout(want_id)),
        }
    }
}

/// Python truthiness for the JSON-RPC `error` field: `null`/`false`/`0`/
/// `""`/`[]`/`{}` are all falsy, mirroring `if response.get("error"):`.
fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marketplace_name_strips_git_suffix_and_lowercases() {
        assert_eq!(marketplace_name("axatbhardwaj/Dvandva"), "dvandva");
        assert_eq!(marketplace_name("axatbhardwaj/Dvandva.git"), "dvandva");
        assert_eq!(marketplace_name("https://example.com/Foo/BAR.git"), "bar");
    }

    #[test]
    fn resolve_marketplace_path_local_dir_never_falls_back_to_find() {
        let dir = tempfile::tempdir().unwrap();
        // No .agents/plugins/marketplace.json created here on purpose: the
        // local-dir branch must not search elsewhere even when it's absent.
        let path =
            resolve_marketplace_path(dir.path().to_str().unwrap(), Path::new("/nonexistent"))
                .unwrap();
        assert_eq!(
            path,
            dir.path()
                .canonicalize()
                .unwrap()
                .join(".agents/plugins/marketplace.json")
        );
        assert!(!path.is_file());
    }

    #[test]
    fn resolve_marketplace_path_remote_uses_default_cache_path_when_present() {
        let codex_home = tempfile::tempdir().unwrap();
        let expected = codex_home
            .path()
            .join(".tmp/marketplaces/dvandva/.agents/plugins/marketplace.json");
        std::fs::create_dir_all(expected.parent().unwrap()).unwrap();
        std::fs::write(&expected, r#"{"name":"dvandva"}"#).unwrap();

        let path = resolve_marketplace_path("axatbhardwaj/Dvandva", codex_home.path()).unwrap();
        assert_eq!(path, expected);
    }

    #[test]
    fn resolve_marketplace_path_remote_falls_back_to_find_search() {
        let codex_home = tempfile::tempdir().unwrap();
        // Default cache path absent; a differently-named directory holds the
        // marketplace.json instead (simulates a remote-name/cache-name skew).
        let buried = codex_home
            .path()
            .join(".tmp/marketplaces/some-other-name/.agents/plugins/marketplace.json");
        std::fs::create_dir_all(buried.parent().unwrap()).unwrap();
        std::fs::write(&buried, r#"{"name": "dvandva", "plugins": []}"#).unwrap();

        let path = resolve_marketplace_path("axatbhardwaj/Dvandva", codex_home.path()).unwrap();
        assert_eq!(path, buried);
    }

    #[test]
    fn find_dvandva_marketplace_json_skips_non_matching_manifests() {
        let root = tempfile::tempdir().unwrap();
        let other = root
            .path()
            .join("not-dvandva/.agents/plugins/marketplace.json");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, r#"{"name": "somethingelse"}"#).unwrap();

        assert!(find_dvandva_marketplace_json(root.path()).is_none());
    }

    #[test]
    fn compute_codex_home_dir_empty_or_unset_home_yields_absolute_path() {
        assert_eq!(
            compute_codex_home_dir(None, Some(OsString::new())),
            PathBuf::from("/.codex")
        );
        assert_eq!(compute_codex_home_dir(None, None), PathBuf::from("/.codex"));
    }

    #[test]
    fn compute_codex_home_dir_joins_home_when_set() {
        assert_eq!(
            compute_codex_home_dir(None, Some(OsString::from("/home/user"))),
            PathBuf::from("/home/user/.codex")
        );
    }

    #[test]
    fn compute_codex_home_dir_prefers_codex_home_env() {
        assert_eq!(
            compute_codex_home_dir(
                Some(OsString::from("/opt/codex")),
                Some(OsString::from("/home/user"))
            ),
            PathBuf::from("/opt/codex")
        );
    }

    #[test]
    fn already_present_pattern_matches_case_insensitively() {
        let re = already_present_pattern();
        assert!(re.is_match("Marketplace 'dvandva' already registered"));
        assert!(re.is_match("ALREADY installed"));
        assert!(re.is_match("plugin exists"));
        assert!(re.is_match("duplicate entry"));
        assert!(!re.is_match("permission denied"));
    }

    #[test]
    fn combine_output_strips_only_trailing_newlines() {
        assert_eq!(combine_output(b"a\n", b"b\n\n"), "a\nb");
        assert_eq!(combine_output(b"", b""), "");
    }

    #[test]
    fn json_truthy_matches_python_semantics() {
        assert!(!json_truthy(&json!(null)));
        assert!(!json_truthy(&json!(false)));
        assert!(!json_truthy(&json!(0)));
        assert!(!json_truthy(&json!("")));
        assert!(!json_truthy(&json!([])));
        assert!(!json_truthy(&json!({})));
        assert!(json_truthy(&json!(true)));
        assert!(json_truthy(&json!("boom")));
        assert!(json_truthy(&json!({"message": "boom"})));
    }
}
