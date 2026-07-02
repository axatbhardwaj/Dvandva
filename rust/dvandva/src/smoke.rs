//! `smoke-install` — the end-to-end packaging probe for the Dvandva plugin.
//!
//! Ported from `scripts/smoke-plugin-install.sh` (321 ln). Builds a temp
//! marketplace from the repo's plugin sources, drives `claude plugin
//! validate`, the full Codex plugin lifecycle (`marketplace add` /
//! `list --available` / `add` / `list`), the bundled skill-surface probe,
//! seed-JSON validation, and the read/write/lint round-trip through this
//! same binary (`std::env::current_exe()`) plus the in-process
//! [`crate::lint::skills`] module. Requires the `claude` and `codex` CLIs on
//! `PATH`.
//!
//! Re-keyed from the shell source for the post-port grammar (design doc
//! §3): every bundled `dvandva-wait.sh` / `dvandva-write.sh` invocation
//! becomes a `dvandva wait` / `dvandva write` call against this same binary;
//! `scripts/install.sh` / `scripts/install-codex.sh` become `dvandva
//! install` / `dvandva install-codex`; `scripts/lint-skills.sh` becomes an
//! in-process call into [`crate::lint::skills::run`]. The plugin tree no
//! longer bundles any `scripts/` directory, so the shell script's "verifies
//! both wait helpers exist" and "standalone development copies" assertions
//! are replaced by [`require_no_bundled_scripts_dir`] and
//! [`require_commands_reference_wait_subcommand`].

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

use crate::lint;

/// Expected published Dvandva plugin version (bumped to `1.4.0` for the S2/S4/S5/S6
/// hardening slice; was `1.3.0` for the flow patches and `1.2.0` for the Rust port).
pub const EXPECTED_DVANDVA_VERSION: &str = "1.4.0";

/// The exact 15-agent seed roster, mirroring the shell script's array (and
/// its already-alphabetical order).
pub const EXPECTED_AGENT_IDS: [&str; 15] = [
    "dvandva-adversarial-analyst",
    "dvandva-architect",
    "dvandva-baton-auditor",
    "dvandva-cross-reviewer",
    "dvandva-debugger",
    "dvandva-deep-reviewer",
    "dvandva-deslopper",
    "dvandva-doc-verifier",
    "dvandva-implementer",
    "dvandva-integration-checker",
    "dvandva-pattern-mapper",
    "dvandva-researcher",
    "dvandva-sandbox-verifier",
    "dvandva-security-auditor",
    "dvandva-test-creator",
];

/// Codex skill surface tokens the installed plugin must expose.
const REQUIRED_CODEX_SKILLS: [&str; 6] = [
    "dvandva:prativadi",
    "dvandva:vadi",
    "dvandva:research",
    "dvandva:testing",
    "dvandva:understanding",
    "dvandva:worktree-setup",
];

/// A smoke-test failure.
///
/// Every failure mode in the shell script routes through its `fail()`
/// helper, which prints to stderr and exits `1` — so [`SmokeError::exit_code`]
/// is always `1`; the message is what distinguishes failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmokeError(pub String);

impl SmokeError {
    pub fn exit_code(&self) -> i32 {
        1
    }
}

impl fmt::Display for SmokeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SmokeError {}

fn fail(message: impl Into<String>) -> SmokeError {
    SmokeError(message.into())
}

// ---------------------------------------------------------------------
// Offline helpers: roster collection/validation, manifest version parity,
// marketplace JSON validation, codex skill-surface extraction, and the
// bundled-scripts-dir / wait-subcommand-reference checks. These are the
// pieces exercised without `claude`/`codex` present.
// ---------------------------------------------------------------------

/// List `dvandva-<stem>` ids for every `*.md` file directly under
/// `agents_dir` (non-recursive), sorted lexicographically. Mirrors `find
/// -maxdepth 1 -type f -name '*.md' -exec basename {} \; | sort` piped
/// through the `dvandva-` prefix.
pub fn collect_agent_ids(agents_dir: &Path) -> Result<Vec<String>, SmokeError> {
    let entries = fs::read_dir(agents_dir).map_err(|error| {
        fail(format!(
            "cannot read agents dir {}: {error}",
            agents_dir.display()
        ))
    })?;

    let mut ids = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| fail(format!("cannot read agents dir entry: {error}")))?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            fail(format!(
                "non-utf8 agent filename in {}",
                agents_dir.display()
            ))
        })?;
        ids.push(format!("dvandva-{stem}"));
    }
    ids.sort();
    Ok(ids)
}

fn expected_agent_ids() -> Vec<String> {
    EXPECTED_AGENT_IDS
        .iter()
        .map(|id| (*id).to_string())
        .collect()
}

/// True when `agents_dir` holds exactly the expected 15-agent roster.
/// Read/parse failures count as a mismatch rather than propagating an error.
pub fn roster_matches_expected(agents_dir: &Path) -> bool {
    collect_agent_ids(agents_dir)
        .map(|actual| actual == expected_agent_ids())
        .unwrap_or(false)
}

/// Require `agents_dir` to hold exactly the expected 15-agent roster,
/// producing a diagnostic listing both rosters on mismatch.
pub fn require_exact_agent_roster(agents_dir: &Path, label: &str) -> Result<(), SmokeError> {
    let actual = collect_agent_ids(agents_dir)?;
    let expected = expected_agent_ids();
    if actual == expected {
        return Ok(());
    }
    Err(fail(format!(
        "{label} agent roster did not match the expected 15-agent Dvandva set\nExpected agent roster:\n{}\nActual agent roster:\n{}",
        expected.join("\n"),
        actual.join("\n"),
    )))
}

fn read_json(path: &Path) -> Result<Value, SmokeError> {
    let text = fs::read_to_string(path)
        .map_err(|error| fail(format!("cannot read {}: {error}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|error| fail(format!("invalid JSON in {}: {error}", path.display())))
}

/// Extract the `dvandva` plugin's `.version` from a parsed
/// `.claude-plugin/marketplace.json` document (`.plugins[] | select(.name ==
/// "dvandva") | .version`). Returns `None` for any shape that does not
/// carry a well-formed entry.
pub fn dvandva_plugin_version(marketplace: &Value) -> Option<&str> {
    marketplace
        .get("plugins")?
        .as_array()?
        .iter()
        .find(|plugin| plugin.get("name").and_then(Value::as_str) == Some("dvandva"))?
        .get("version")?
        .as_str()
}

/// Read the marketplace entry plus both plugin manifests under `root_dir`
/// and require all three versions to match each other and
/// [`EXPECTED_DVANDVA_VERSION`].
pub fn assert_source_manifest_version_parity(root_dir: &Path) -> Result<(), SmokeError> {
    let marketplace_path = root_dir.join(".claude-plugin/marketplace.json");
    let claude_plugin_path = root_dir.join("plugins/dvandva/.claude-plugin/plugin.json");
    let codex_plugin_path = root_dir.join("plugins/dvandva/.codex-plugin/plugin.json");

    let marketplace_json = read_json(&marketplace_path)?;
    let claude_json = read_json(&claude_plugin_path)?;
    let codex_json = read_json(&codex_plugin_path)?;

    let marketplace_version = dvandva_plugin_version(&marketplace_json).ok_or_else(|| {
        fail(format!(
            "missing marketplace version in {}",
            marketplace_path.display()
        ))
    })?;
    let claude_version = claude_json
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("");
    let codex_version = codex_json
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("");

    if marketplace_version != claude_version {
        return Err(fail(format!(
            "version mismatch: marketplace={marketplace_version} claude-plugin={claude_version}"
        )));
    }
    if marketplace_version != codex_version {
        return Err(fail(format!(
            "version mismatch: marketplace={marketplace_version} codex-plugin={codex_version}"
        )));
    }
    if marketplace_version != EXPECTED_DVANDVA_VERSION {
        return Err(fail(format!(
            "expected Dvandva plugin version {EXPECTED_DVANDVA_VERSION}, got {marketplace_version}"
        )));
    }
    Ok(())
}

/// Require an installed Codex plugin cache at `<codex_home>/plugins/cache/
/// dvandva/dvandva/<EXPECTED_DVANDVA_VERSION>` with matching Claude/Codex
/// manifest versions and the exact 15-agent roster.
pub fn require_installed_codex_cache(codex_home: &Path, label: &str) -> Result<(), SmokeError> {
    let plugin_root = codex_home
        .join("plugins/cache/dvandva/dvandva")
        .join(EXPECTED_DVANDVA_VERSION);

    if !plugin_root.is_dir() {
        return Err(fail(format!(
            "{label} missing Codex cache at {}",
            plugin_root.display()
        )));
    }

    let claude_json = read_json(&plugin_root.join(".claude-plugin/plugin.json"))?;
    if claude_json.get("version").and_then(Value::as_str) != Some(EXPECTED_DVANDVA_VERSION) {
        return Err(fail(format!(
            "{label} cached Claude manifest version mismatch"
        )));
    }
    let codex_json = read_json(&plugin_root.join(".codex-plugin/plugin.json"))?;
    if codex_json.get("version").and_then(Value::as_str) != Some(EXPECTED_DVANDVA_VERSION) {
        return Err(fail(format!(
            "{label} cached Codex manifest version mismatch"
        )));
    }

    require_exact_agent_roster(&plugin_root.join("agents"), &format!("{label} cached"))
}

/// Recursively collect every JSON string value, mirroring jq's `.. |
/// strings?`.
pub fn recursive_strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|item| recursive_strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| recursive_strings(item, out)),
        _ => {}
    }
}

/// Require every Dvandva Codex skill surface token to appear among the
/// strings nested anywhere in `value` (mirrors piping `codex debug
/// prompt-input` output through `jq -r '.. | strings? // empty'` and
/// grepping each required skill).
pub fn require_codex_skill_surface(value: &Value, source_label: &str) -> Result<(), SmokeError> {
    let mut strings = Vec::new();
    recursive_strings(value, &mut strings);
    let corpus = strings.join("\n");
    for skill in REQUIRED_CODEX_SKILLS {
        if !corpus.contains(skill) {
            return Err(fail(format!(
                "installed Codex skill surface missing {skill} in {source_label}"
            )));
        }
    }
    Ok(())
}

/// Require that no directory literally named `scripts` exists anywhere
/// under `plugin_dir`.
///
/// Re-keyed replacement for the shell script's "verifies both wait helpers
/// exist" assertion: post-port the plugin ships zero bundled shell helpers,
/// so the contract is the absence of any `scripts/` directory rather than
/// the presence of specific files inside one.
pub fn require_no_bundled_scripts_dir(plugin_dir: &Path) -> Result<(), SmokeError> {
    fn walk(dir: &Path) -> Result<(), SmokeError> {
        let entries = fs::read_dir(dir)
            .map_err(|error| fail(format!("cannot read {}: {error}", dir.display())))?;
        for entry in entries {
            let entry = entry.map_err(|error| fail(format!("cannot read dir entry: {error}")))?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some("scripts") {
                return Err(fail(format!(
                    "plugin tree ships a scripts/ dir at {}",
                    path.display()
                )));
            }
            walk(&path)?;
        }
        Ok(())
    }
    walk(plugin_dir)
}

/// Require both role command files to reference the `dvandva wait --role`
/// subcommand grammar.
///
/// Re-keyed replacement for the shell script's "standalone development
/// copies" wait-helper check: the bundled per-skill `dvandva-wait.sh` copy
/// no longer exists, so the contract shifts to the docs naming the single
/// canonical subcommand.
pub fn require_commands_reference_wait_subcommand(commands_dir: &Path) -> Result<(), SmokeError> {
    for name in ["vadi.md", "prativadi.md"] {
        let path = commands_dir.join(name);
        let text = fs::read_to_string(&path)
            .map_err(|error| fail(format!("cannot read {}: {error}", path.display())))?;
        if !text.contains("dvandva wait --role") {
            return Err(fail(format!(
                "commands/{name} does not reference 'dvandva wait --role'"
            )));
        }
    }
    Ok(())
}

/// True when `value`'s `.turn_cap` field equals `expected` (mirrors `jq -e
/// '.turn_cap == N'`).
pub fn has_turn_cap(value: &Value, expected: i64) -> bool {
    value.get("turn_cap").and_then(Value::as_i64) == Some(expected)
}

fn validate_json_files(paths: &[PathBuf]) -> Result<(), SmokeError> {
    for path in paths {
        read_json(path)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Process/filesystem plumbing for the full engine-driven run.
// ---------------------------------------------------------------------

fn need_cmd(name: &str) -> Result<(), SmokeError> {
    let path_var = std::env::var_os("PATH");
    let found = path_var.as_ref().is_some_and(|path_var| {
        std::env::split_paths(path_var).any(|dir| is_executable_file(&dir.join(name)))
    });
    if found {
        Ok(())
    } else {
        Err(fail(format!("required command not found: {name}")))
    }
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn discover_root_dir() -> Result<PathBuf, SmokeError> {
    let cwd = std::env::current_dir()
        .map_err(|error| fail(format!("cannot read current directory: {error}")))?;
    Ok(crate::gitcfg::repo_toplevel(&cwd).unwrap_or(cwd))
}

/// Allocate a fresh, uniquely-named directory under `parent` (mimics
/// `mktemp -d "$parent/$prefix.XXXXXX"` without a dev-only crate dependency
/// in production code).
fn make_temp_dir(parent: &Path, prefix: &str) -> Result<PathBuf, SmokeError> {
    fs::create_dir_all(parent)
        .map_err(|error| fail(format!("cannot create {}: {error}", parent.display())))?;
    for attempt in 0..64u32 {
        let unique = format!(
            "{prefix}.{}-{}-{attempt}",
            std::process::id(),
            crate::util::now_epoch_nanos()
        );
        let candidate = parent.join(unique);
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(fail(format!(
                    "cannot create temp dir {}: {error}",
                    candidate.display()
                )))
            }
        }
    }
    Err(fail(format!(
        "could not allocate a unique temp dir under {}",
        parent.display()
    )))
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), SmokeError> {
    fs::create_dir_all(dst)
        .map_err(|error| fail(format!("cannot create {}: {error}", dst.display())))?;
    let entries = fs::read_dir(src)
        .map_err(|error| fail(format!("cannot read {}: {error}", src.display())))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            fail(format!(
                "cannot read dir entry in {}: {error}",
                src.display()
            ))
        })?;
        let file_type = entry
            .file_type()
            .map_err(|error| fail(format!("cannot stat {}: {error}", entry.path().display())))?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|error| {
                fail(format!(
                    "cannot copy {} -> {}: {error}",
                    entry.path().display(),
                    target.display()
                ))
            })?;
        }
    }
    Ok(())
}

fn run_cmd(command: &mut Command, description: &str) -> Result<Output, SmokeError> {
    println!("SMOKE: {description}");
    let output = command
        .output()
        .map_err(|error| fail(format!("failed to run {description}: {error}")))?;
    if !output.status.success() {
        return Err(fail(format!(
            "{description} failed (exit {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(output)
}

/// Run this same binary (`std::env::current_exe()`) with `args`, optionally
/// overriding environment variables.
///
/// Re-keyed replacement for the shell script's calls into the bundled
/// per-role `dvandva-wait.sh` / `dvandva-write.sh` helpers and into
/// `scripts/install.sh` / `scripts/install-codex.sh`: all of those now
/// resolve to subcommands on this one binary.
fn run_self(args: &[&str], envs: &[(&str, &Path)], description: &str) -> Result<(), SmokeError> {
    let exe = std::env::current_exe()
        .map_err(|error| fail(format!("cannot resolve current_exe(): {error}")))?;
    let mut command = Command::new(exe);
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    run_cmd(&mut command, description)?;
    Ok(())
}

fn run_codex_json(
    codex_home: &Path,
    codex_user_home: &Path,
    args: &[&str],
    description: &str,
) -> Result<Value, SmokeError> {
    println!("SMOKE: {description}");
    let output = Command::new("codex")
        .env("CODEX_HOME", codex_home)
        .env("HOME", codex_user_home)
        .args(args)
        .output()
        .map_err(|error| fail(format!("failed to run {description}: {error}")))?;
    if !output.status.success() {
        return Err(fail(format!(
            "{description} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| fail(format!("{description} produced invalid JSON: {error}")))
}

fn with_fields(mut value: Value, fields: &[(&str, Value)]) -> Value {
    if let Value::Object(map) = &mut value {
        for (key, field_value) in fields {
            map.insert((*key).to_string(), field_value.clone());
        }
    }
    value
}

fn write_json(path: &Path, value: &Value) -> Result<(), SmokeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| fail(format!("cannot create {}: {error}", parent.display())))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|error| {
        fail(format!(
            "cannot serialize JSON for {}: {error}",
            path.display()
        ))
    })?;
    fs::write(path, text).map_err(|error| fail(format!("cannot write {}: {error}", path.display())))
}

fn lint_skill_md(path: &Path) -> Result<(), SmokeError> {
    let path_str = path
        .to_str()
        .ok_or_else(|| fail(format!("non-utf8 SKILL.md path: {}", path.display())))?
        .to_string();
    println!("SMOKE: dvandva lint skills {path_str}");
    let code = lint::skills::run(std::slice::from_ref(&path_str));
    if code != 0 {
        return Err(fail(format!(
            "dvandva lint skills {path_str} failed (exit {code})"
        )));
    }
    Ok(())
}

fn probe_codex_skill_surface(
    codex_home: &Path,
    codex_user_home: &Path,
    prompt: &str,
) -> Result<(), SmokeError> {
    let value = run_codex_json(
        codex_home,
        codex_user_home,
        &["debug", "prompt-input", prompt],
        &format!(
            "env CODEX_HOME={} HOME={} codex debug prompt-input \"{prompt}\"",
            codex_home.display(),
            codex_user_home.display()
        ),
    )?;
    require_codex_skill_surface(&value, prompt)
}

fn require_commands_bundled(plugin_dir: &Path) -> Result<(), SmokeError> {
    let commands_dir = plugin_dir.join("commands");
    for (name, goal_prefix) in [
        ("vadi.md", "/goal You are Dvandva vadi"),
        ("prativadi.md", "/goal You are Dvandva prativadi"),
    ] {
        let path = commands_dir.join(name);
        let text = fs::read_to_string(&path).map_err(|error| {
            fail(format!(
                "dvandva commands/{name} missing from bundled plugin: {error}"
            ))
        })?;
        if !text.lines().any(|line| line.starts_with("description:")) {
            return Err(fail(format!(
                "{name} missing required 'description' frontmatter key"
            )));
        }
        if !text.lines().any(|line| line.starts_with(goal_prefix)) {
            return Err(fail(format!("{name} body missing /goal block")));
        }
    }
    println!("SMOKE: dvandva slash commands bundled correctly");
    Ok(())
}

fn require_runtime_skills_bundled(plugin_dir: &Path) -> Result<(), SmokeError> {
    for skill in ["research", "understanding", "testing", "worktree-setup"] {
        let path = plugin_dir.join(format!("skills/{skill}/SKILL.md"));
        let metadata = fs::metadata(&path).map_err(|error| {
            fail(format!(
                "dvandva skills/{skill}/SKILL.md missing or empty from bundled plugin: {error}"
            ))
        })?;
        if metadata.len() == 0 {
            return Err(fail(format!(
                "dvandva skills/{skill}/SKILL.md missing or empty from bundled plugin"
            )));
        }
    }
    println!("SMOKE: runtime skills (research, understanding, testing, worktree-setup) bundled correctly");
    Ok(())
}

// ---------------------------------------------------------------------
// The full engine-driven run, decomposed into the shell script's phases.
// ---------------------------------------------------------------------

/// Run the full `smoke-install` probe. Requires the `claude` and `codex`
/// CLIs on `PATH`.
pub fn run() -> Result<(), SmokeError> {
    need_cmd("claude")?;
    need_cmd("codex")?;

    let root_dir = discover_root_dir()?;
    let tmp_parent = std::env::var_os("DVANDVA_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let tmp_dir = make_temp_dir(&tmp_parent, "dvandva-smoke")?;
    let _cleanup = TempDirGuard(tmp_dir.clone());

    phase_source_checks(&root_dir)?;

    let marketplace_root = tmp_dir.join("marketplace");
    let plugin_dir = marketplace_root.join("plugins/dvandva");
    phase_build_marketplace(&root_dir, &marketplace_root, &plugin_dir)?;
    phase_claude_validate(&plugin_dir, &marketplace_root)?;

    let codex_home = tmp_dir.join("codex-home");
    let codex_user_home = tmp_dir.join("codex-user-home");
    phase_codex_lifecycle(&marketplace_root, &codex_home, &codex_user_home)?;
    require_installed_codex_cache(&codex_home, "direct Codex install")?;
    probe_codex_skill_surface(&codex_home, &codex_user_home, "probe dvandva skills")?;

    require_commands_bundled(&plugin_dir)?;
    require_runtime_skills_bundled(&plugin_dir)?;

    phase_install_scripts(&tmp_dir, &marketplace_root)?;
    phase_stale_cache_rejection(&tmp_dir)?;
    phase_seed_json_validation(&marketplace_root, &plugin_dir)?;
    phase_wait_probe(&plugin_dir, &tmp_dir)?;
    phase_write_probe(&plugin_dir, &tmp_dir)?;

    lint_skill_md(&plugin_dir.join("skills/vadi/SKILL.md"))?;
    lint_skill_md(&plugin_dir.join("skills/prativadi/SKILL.md"))?;
    require_no_bundled_scripts_dir(&plugin_dir)?;
    require_commands_reference_wait_subcommand(&plugin_dir.join("commands"))?;

    println!("SMOKE: plugin install surfaces passed");
    Ok(())
}

fn phase_source_checks(root_dir: &Path) -> Result<(), SmokeError> {
    assert_source_manifest_version_parity(root_dir)?;
    require_exact_agent_roster(&root_dir.join("plugins/dvandva/agents"), "source")
}

fn phase_build_marketplace(
    root_dir: &Path,
    marketplace_root: &Path,
    plugin_dir: &Path,
) -> Result<(), SmokeError> {
    fs::create_dir_all(marketplace_root.join("plugins"))
        .map_err(|error| fail(format!("cannot create marketplace plugins dir: {error}")))?;
    fs::create_dir_all(marketplace_root.join(".agents/plugins")).map_err(|error| {
        fail(format!(
            "cannot create marketplace .agents/plugins dir: {error}"
        ))
    })?;
    copy_dir_all(
        &root_dir.join(".claude-plugin"),
        &marketplace_root.join(".claude-plugin"),
    )?;
    fs::copy(
        root_dir.join(".agents/plugins/marketplace.json"),
        marketplace_root.join(".agents/plugins/marketplace.json"),
    )
    .map_err(|error| {
        fail(format!(
            "cannot copy .agents/plugins/marketplace.json: {error}"
        ))
    })?;
    copy_dir_all(&root_dir.join("plugins/dvandva"), plugin_dir)
}

fn phase_claude_validate(plugin_dir: &Path, marketplace_root: &Path) -> Result<(), SmokeError> {
    run_cmd(
        Command::new("claude")
            .arg("plugin")
            .arg("validate")
            .arg(plugin_dir),
        &format!("claude plugin validate {}", plugin_dir.display()),
    )?;
    run_cmd(
        Command::new("claude")
            .arg("plugin")
            .arg("validate")
            .arg(marketplace_root),
        &format!("claude plugin validate {}", marketplace_root.display()),
    )?;
    Ok(())
}

fn phase_codex_lifecycle(
    marketplace_root: &Path,
    codex_home: &Path,
    codex_user_home: &Path,
) -> Result<(), SmokeError> {
    fs::create_dir_all(codex_home)
        .map_err(|error| fail(format!("cannot create {}: {error}", codex_home.display())))?;
    fs::create_dir_all(codex_user_home).map_err(|error| {
        fail(format!(
            "cannot create {}: {error}",
            codex_user_home.display()
        ))
    })?;

    run_cmd(
        Command::new("codex")
            .env("CODEX_HOME", codex_home)
            .arg("plugin")
            .arg("marketplace")
            .arg("add")
            .arg(marketplace_root),
        &format!(
            "env CODEX_HOME={} codex plugin marketplace add {}",
            codex_home.display(),
            marketplace_root.display()
        ),
    )?;
    let config_toml = fs::read_to_string(codex_home.join("config.toml"))
        .map_err(|error| fail(format!("cannot read codex-home/config.toml: {error}")))?;
    if !config_toml.contains("source = \"") {
        return Err(fail(
            "codex-home/config.toml missing source = \" entry after marketplace add",
        ));
    }

    let available = run_codex_json(
        codex_home,
        codex_user_home,
        &["plugin", "list", "--available", "--json"],
        &format!(
            "env CODEX_HOME={} HOME={} codex plugin list --available --json",
            codex_home.display(),
            codex_user_home.display()
        ),
    )?;
    let has_available_uninstalled = available
        .get("available")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("pluginId").and_then(Value::as_str) == Some("dvandva@dvandva")
                    && matches!(item.get("installed"), Some(Value::Bool(false)))
            })
        });
    if !has_available_uninstalled {
        return Err(fail(
            "codex plugin list --available --json missing dvandva@dvandva as available+uninstalled",
        ));
    }

    let installed_result = run_codex_json(
        codex_home,
        codex_user_home,
        &["plugin", "add", "dvandva@dvandva", "--json"],
        &format!(
            "env CODEX_HOME={} HOME={} codex plugin add dvandva@dvandva --json",
            codex_home.display(),
            codex_user_home.display()
        ),
    )?;
    let install_ok = installed_result.get("pluginId").and_then(Value::as_str)
        == Some("dvandva@dvandva")
        && installed_result.get("name").and_then(Value::as_str) == Some("dvandva")
        && installed_result
            .get("marketplaceName")
            .and_then(Value::as_str)
            == Some("dvandva");
    if !install_ok {
        return Err(fail(
            "codex plugin add --json result did not match pluginId/name/marketplaceName",
        ));
    }

    let installed = run_codex_json(
        codex_home,
        codex_user_home,
        &["plugin", "list", "--json"],
        &format!(
            "env CODEX_HOME={} HOME={} codex plugin list --json",
            codex_home.display(),
            codex_user_home.display()
        ),
    )?;
    let installed_ok = installed
        .get("installed")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("pluginId").and_then(Value::as_str) == Some("dvandva@dvandva")
                    && matches!(item.get("installed"), Some(Value::Bool(true)))
                    && matches!(item.get("enabled"), Some(Value::Bool(true)))
            })
        });
    if !installed_ok {
        return Err(fail(
            "codex plugin list --json missing dvandva@dvandva as installed+enabled",
        ));
    }

    Ok(())
}

/// Phases 5 and 6: re-install via `dvandva install` / `dvandva
/// install-codex` into fresh Claude/Codex homes.
///
/// Re-keyed from `scripts/install.sh` / `scripts/install-codex.sh`: the
/// dual-engine one-liner and the Codex-only fallback are now subcommands on
/// this same binary (design doc §3 grammar) rather than separate shell
/// scripts, since the whole `scripts/` tree is deleted later in the port.
fn phase_install_scripts(tmp_dir: &Path, marketplace_root: &Path) -> Result<(), SmokeError> {
    let marketplace_arg = marketplace_root
        .to_str()
        .ok_or_else(|| fail("marketplace root path is not valid UTF-8"))?;

    let dual_codex_home = tmp_dir.join("codex-home-via-dual-install");
    let dual_user_home = tmp_dir.join("user-home-via-dual-install");
    fs::create_dir_all(&dual_codex_home).map_err(|error| {
        fail(format!(
            "cannot create {}: {error}",
            dual_codex_home.display()
        ))
    })?;
    fs::create_dir_all(&dual_user_home).map_err(|error| {
        fail(format!(
            "cannot create {}: {error}",
            dual_user_home.display()
        ))
    })?;
    run_self(
        &["install", marketplace_arg],
        &[("CODEX_HOME", &dual_codex_home), ("HOME", &dual_user_home)],
        &format!(
            "env CODEX_HOME={} HOME={} dvandva install {marketplace_arg}",
            dual_codex_home.display(),
            dual_user_home.display()
        ),
    )?;
    probe_codex_skill_surface(
        &dual_codex_home,
        &dual_user_home,
        "probe dvandva skills after dual install",
    )?;
    require_installed_codex_cache(&dual_codex_home, "dual dvandva install Codex path")?;
    println!("SMOKE: dvandva install dual-engine install passed");

    let codex_only_home = tmp_dir.join("codex-home-via-install-codex");
    let codex_only_user_home = tmp_dir.join("codex-user-home-via-install-codex");
    fs::create_dir_all(&codex_only_home).map_err(|error| {
        fail(format!(
            "cannot create {}: {error}",
            codex_only_home.display()
        ))
    })?;
    fs::create_dir_all(&codex_only_user_home).map_err(|error| {
        fail(format!(
            "cannot create {}: {error}",
            codex_only_user_home.display()
        ))
    })?;
    run_self(
        &["install-codex", marketplace_arg],
        &[
            ("CODEX_HOME", &codex_only_home),
            ("HOME", &codex_only_user_home),
        ],
        &format!(
            "env CODEX_HOME={} HOME={} dvandva install-codex {marketplace_arg}",
            codex_only_home.display(),
            codex_only_user_home.display()
        ),
    )?;
    probe_codex_skill_surface(
        &codex_only_home,
        &codex_only_user_home,
        "probe dvandva skills after codex helper install",
    )?;
    require_installed_codex_cache(&codex_only_home, "dvandva install-codex helper path")?;
    println!("SMOKE: dvandva install-codex end-to-end install passed");

    Ok(())
}

fn phase_stale_cache_rejection(tmp_dir: &Path) -> Result<(), SmokeError> {
    let source_cache = tmp_dir
        .join("codex-home-via-install-codex/plugins/cache/dvandva/dvandva")
        .join(EXPECTED_DVANDVA_VERSION);
    let stale_cache_dir = tmp_dir
        .join("stale-codex-cache")
        .join(EXPECTED_DVANDVA_VERSION);
    fs::create_dir_all(tmp_dir.join("stale-codex-cache"))
        .map_err(|error| fail(format!("cannot create stale-codex-cache dir: {error}")))?;
    copy_dir_all(&source_cache, &stale_cache_dir)?;
    fs::remove_file(stale_cache_dir.join("agents/deslopper.md"))
        .map_err(|error| fail(format!("cannot remove stale-cache fixture agent: {error}")))?;
    fs::write(stale_cache_dir.join("agents/not-a-dvandva-agent.md"), b"")
        .map_err(|error| fail(format!("cannot write stale-cache fixture agent: {error}")))?;
    if roster_matches_expected(&stale_cache_dir.join("agents")) {
        return Err(fail(
            "same-version stale cache fixture unexpectedly passed exact roster validation",
        ));
    }
    println!("SMOKE: same-version stale cache rejected by exact roster validation");
    Ok(())
}

fn phase_seed_json_validation(
    marketplace_root: &Path,
    plugin_dir: &Path,
) -> Result<(), SmokeError> {
    validate_json_files(&[
        marketplace_root.join(".agents/plugins/marketplace.json"),
        plugin_dir.join(".claude-plugin/plugin.json"),
        plugin_dir.join(".codex-plugin/plugin.json"),
        plugin_dir.join("references/baton-schema.json"),
        plugin_dir.join("references/baton-schema-v2.json"),
    ])?;

    let schema_v1 = read_json(&plugin_dir.join("references/baton-schema.json"))?;
    if !has_turn_cap(&schema_v1, 60) {
        return Err(fail("references/baton-schema.json turn_cap != 60"));
    }
    let schema_v2 = read_json(&plugin_dir.join("references/baton-schema-v2.json"))?;
    if !has_turn_cap(&schema_v2, 60) {
        return Err(fail("references/baton-schema-v2.json turn_cap != 60"));
    }
    Ok(())
}

/// Re-keyed from the bundled `dvandva-wait.sh --role vadi|prativadi`
/// invocations: drives this same binary's `wait` subcommand instead.
fn phase_wait_probe(plugin_dir: &Path, tmp_dir: &Path) -> Result<(), SmokeError> {
    let schema_path = plugin_dir.join("references/baton-schema.json");
    let schema_arg = schema_path
        .to_str()
        .ok_or_else(|| fail("baton-schema.json path is not valid UTF-8"))?;

    run_self(
        &[
            "wait",
            "--role",
            "vadi",
            "--file",
            schema_arg,
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        &[],
        &format!("dvandva wait --role vadi --file {schema_arg} --interval 0 --max-wait 0"),
    )?;

    let vadi_baton = read_json(&schema_path)?;
    let prativadi_baton = with_fields(
        vadi_baton,
        &[
            ("assignee", Value::String("prativadi".to_string())),
            ("status", Value::String("spec_review".to_string())),
            ("review_target", Value::String("spec".to_string())),
        ],
    );
    let prativadi_baton_path = tmp_dir.join("prativadi-baton.json");
    write_json(&prativadi_baton_path, &prativadi_baton)?;
    let prativadi_arg = prativadi_baton_path
        .to_str()
        .ok_or_else(|| fail("prativadi-baton.json path is not valid UTF-8"))?;
    run_self(
        &[
            "wait",
            "--role",
            "prativadi",
            "--file",
            prativadi_arg,
            "--interval",
            "0",
            "--max-wait",
            "0",
        ],
        &[],
        &format!("dvandva wait --role prativadi --file {prativadi_arg} --interval 0 --max-wait 0"),
    )
}

/// Re-keyed from the bundled `dvandva-write.sh` invocations: drives this
/// same binary's `write` subcommand instead. S5-T2 retired v1 from the write
/// path, so this now probes that a v1 scaffold candidate is REJECTED with
/// `schema_retired`, then exercises the live v2 research seed scaffold + one
/// legal transition.
fn phase_write_probe(plugin_dir: &Path, tmp_dir: &Path) -> Result<(), SmokeError> {
    let write_box = tmp_dir.join("write-helper");
    fs::create_dir_all(&write_box)
        .map_err(|error| fail(format!("cannot create write-helper dir: {error}")))?;
    let baton_path = write_box.join("baton.json");
    let baton_next_path = write_box.join("baton.next.json");

    // S5-T2: a v1 seed candidate can no longer be written — expect exit 23
    // `schema_retired` and no installed baton.
    let baton_schema = read_json(&plugin_dir.join("references/baton-schema.json"))?;
    let v1_scaffold = with_fields(
        baton_schema,
        &[
            ("status", Value::String("spec_drafting".to_string())),
            ("assignee", Value::String("vadi".to_string())),
            ("checkpoint", Value::from(0)),
            ("master_plan_locked", Value::Bool(false)),
            ("question", Value::Null),
            ("resume_assignee", Value::Null),
            ("resume_status", Value::Null),
        ],
    );
    write_json(&baton_next_path, &v1_scaffold)?;
    run_write_expect_reject(
        &baton_path,
        &baton_next_path,
        23,
        "schema_retired",
        "v1 scaffold retired",
    )?;
    if baton_path.is_file() {
        return Err(fail("v1 schema_retired probe must not install a baton"));
    }

    let v2_write_box = tmp_dir.join("write-helper-v2/.dvandva/runs/smoke");
    fs::create_dir_all(&v2_write_box)
        .map_err(|error| fail(format!("cannot create v2 write-helper dir: {error}")))?;
    let v2_baton_path = v2_write_box.join("baton.json");
    let v2_baton_next_path = v2_write_box.join("baton.next.json");
    let schema_v2 = read_json(&plugin_dir.join("references/baton-schema-v2.json"))?;
    let v2_scaffold = with_fields(
        schema_v2,
        &[
            (
                "updated_at",
                Value::String("2026-06-27T00:00:00Z".to_string()),
            ),
            ("run_id", Value::String("smoke".to_string())),
            ("original_ask", Value::String("Smoke v2 helper".to_string())),
            (
                "research_ref",
                Value::String("./superpowers/research/smoke.html".to_string()),
            ),
            ("current_engine", Value::String("codex".to_string())),
            ("branch", Value::String("smoke".to_string())),
            ("status", Value::String("research_drafting".to_string())),
            ("assignee", Value::String("vadi".to_string())),
            ("checkpoint", Value::from(0)),
            ("master_plan_locked", Value::Bool(false)),
            ("question", Value::Null),
            ("resume_assignee", Value::Null),
            ("resume_status", Value::Null),
        ],
    );
    write_json(&v2_baton_next_path, &v2_scaffold)?;
    run_write(&v2_baton_path, &v2_baton_next_path, "v2 scaffold")?;

    let v2_history_0 = v2_write_box.join("history/0-research_drafting-vadi.json");
    if !v2_history_0.is_file() {
        return Err(fail("v2 write helper did not snapshot research scaffold"));
    }

    Ok(())
}

/// Drive `dvandva write` expecting a specific non-zero rejection: the exit code
/// must match and stderr must contain `needle`. Used by the S5-T2 v1
/// `schema_retired` probe.
fn run_write_expect_reject(
    baton_path: &Path,
    baton_next_path: &Path,
    expected_code: i32,
    needle: &str,
    step: &str,
) -> Result<(), SmokeError> {
    let baton_arg = baton_path
        .to_str()
        .ok_or_else(|| fail("baton path is not valid UTF-8"))?;
    let baton_next_arg = baton_next_path
        .to_str()
        .ok_or_else(|| fail("baton.next path is not valid UTF-8"))?;
    let description = format!("dvandva write {baton_arg} {baton_next_arg} ({step})");
    println!("SMOKE: {description}");
    let exe = std::env::current_exe()
        .map_err(|error| fail(format!("cannot resolve current_exe(): {error}")))?;
    let output = Command::new(exe)
        .args(["write", baton_arg, baton_next_arg])
        .output()
        .map_err(|error| fail(format!("failed to run {description}: {error}")))?;
    let code = output.status.code();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if code != Some(expected_code) || !stderr.contains(needle) {
        return Err(fail(format!(
            "{description} expected exit {expected_code} with '{needle}', got exit {code:?}: {stderr}"
        )));
    }
    Ok(())
}

fn run_write(baton_path: &Path, baton_next_path: &Path, step: &str) -> Result<(), SmokeError> {
    let baton_arg = baton_path
        .to_str()
        .ok_or_else(|| fail("baton path is not valid UTF-8"))?;
    let baton_next_arg = baton_next_path
        .to_str()
        .ok_or_else(|| fail("baton.next path is not valid UTF-8"))?;
    run_self(
        &["write", baton_arg, baton_next_arg],
        &[],
        &format!("dvandva write {baton_arg} {baton_next_arg} ({step})"),
    )
}
