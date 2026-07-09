//! `dvandva lint <target>` family — repo content and artifact lints.
//!
//! This module hosts the shared lint scaffolding (`Report`/`Finding` plus the
//! file-predicate helpers) reused by every repo-content lint submodule. The
//! shell scripts derived `ROOT_DIR` from the script location; the Rust ports
//! take an optional repo-root argument and otherwise fall back to the git
//! toplevel of the current directory.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

pub mod artifacts;
pub mod phase4_research;
pub mod protocol_phase1;
pub mod run3_dynamic_agents;
pub mod run4_path_gates;
pub mod run4_standalone_agents;
pub mod schema_parity;
pub mod skill_phase3;
pub mod skills;
pub mod stale_version_ref;

pub(crate) const MODEL_POLICY_VENDOR_NEUTRAL_DOCS: &str =
    "Dvandva model classes are vendor-neutral";
pub(crate) const MODEL_POLICY_VENDOR_NEUTRAL_COMMANDS: &str =
    "Model-class mapping is vendor-neutral";
pub(crate) const MODEL_POLICY_CLAUDE_MAPPING: &str =
    "Claude Code maps `opus` to Opus-class, `sonnet` to Sonnet-class, `fable` to Fable-class, and `gpt` to a Sonnet-class wrapper that shells to Codex where available";
pub(crate) const MODEL_POLICY_CODEX_MAPPING: &str =
    "Codex maps `opus` and `fable` to `gpt-5.5` with `xhigh` reasoning and `sonnet` and `gpt` to `gpt-5.5` with `high` reasoning";
pub(crate) const MODEL_POLICY_CODEX_EFFORT: &str =
    "Codex should request `xhigh` reasoning effort for opus-class and fable-class work and `high` reasoning effort for sonnet-class and gpt-class work where the active surface exposes it";
pub(crate) const MODEL_POLICY_OPUS_ROUTING: &str =
    "Use `opus` for architecture, planning, deep review, adversarial/security/integration/doc-verification, and baton-audit work";
pub(crate) const MODEL_POLICY_SONNET_ROUTING: &str =
    "Use `sonnet` for bounded implementation, documentation, research, verification, routine cross-review, debugging, test creation, sandbox probes, and deslop";
pub(crate) const MODEL_POLICY_NO_HAIKU_SUBAGENTS: &str = "Do not use `haiku` for Dvandva subagents";
pub(crate) const MODEL_POLICY_NO_HAIKU_COMMANDS: &str = "Never use `haiku`";
pub(crate) const MODEL_POLICY_STALE_OPUS_ROUTING: &str =
    "strongest available planning/review/architecture class";
pub(crate) const MODEL_POLICY_STALE_SONNET_ROUTING: &str =
    "implementation/documentation workhorse class";
pub(crate) const MODEL_POLICY_STALE_CODEX_MAPPING: &str =
    "Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`";
pub(crate) const MODEL_POLICY_STALE_CANONICAL_COMPAT_MAPPING: &str =
    "Accepted compatibility strings remain vendor-neutral: `opus-class|gpt-5.5` maps to `opus`, and `sonnet-class|gpt-5.4` maps to `sonnet`";

/// One PASS/FAIL assertion outcome.
pub struct Finding {
    pub ok: bool,
    pub message: String,
}

/// Accumulated lint findings; drives the `PASS:`/`FAIL:` idiom and exit code.
#[derive(Default)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn new() -> Self {
        Report::default()
    }

    /// Record an assertion outcome.
    pub fn add(&mut self, ok: bool, message: impl Into<String>) {
        self.findings.push(Finding {
            ok,
            message: message.into(),
        });
    }

    /// Number of failing assertions.
    pub fn failures(&self) -> usize {
        self.findings.iter().filter(|f| !f.ok).count()
    }

    /// True when every assertion passed.
    pub fn passed(&self) -> bool {
        self.failures() == 0
    }

    /// Exit code: 0 when clean, 1 when any assertion failed.
    pub fn exit_code(&self) -> i32 {
        if self.passed() {
            0
        } else {
            1
        }
    }

    /// Emit `PASS:`/`FAIL:` lines (pass to stdout, fail to stderr).
    pub fn print(&self) {
        for f in &self.findings {
            if f.ok {
                println!("PASS: {}", f.message);
            } else {
                eprintln!("FAIL: {}", f.message);
            }
        }
    }

    /// True when a failing finding's message contains `needle` — the test-side
    /// analog of the shell meta-tests' `expect_fail "<failure text>"`.
    pub fn fails_with(&self, needle: &str) -> bool {
        self.findings
            .iter()
            .any(|f| !f.ok && f.message.contains(needle))
    }
}

/// Optional repo-root argument, else the git toplevel of the current dir.
pub fn resolve_root(args: &[String]) -> PathBuf {
    if let Some(first) = args.iter().find(|a| !a.starts_with("--")) {
        return PathBuf::from(first);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    crate::gitcfg::repo_toplevel(&cwd).unwrap_or(cwd)
}

/// Read a repo-relative file to a string, `None` when absent/unreadable.
pub fn read(root: &Path, rel: &str) -> Option<String> {
    fs::read_to_string(root.join(rel)).ok()
}

/// Collapse newlines to spaces, mirroring the shell `tr '\n' ' '` slurp.
pub fn slurp_spaces(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

fn compile(pattern: &str) -> Option<Regex> {
    Regex::new(pattern).ok()
}

fn compile_ci(pattern: &str) -> Option<Regex> {
    Regex::new(&format!("(?i){pattern}")).ok()
}

/// `grep -Fq` — literal substring anywhere in the file (missing file: false).
pub fn file_contains(root: &Path, rel: &str, needle: &str) -> bool {
    read(root, rel).map(|c| c.contains(needle)).unwrap_or(false)
}

/// `rg`/`grep -E` — case-sensitive regex, matched per line.
pub fn file_matches(root: &Path, rel: &str, pattern: &str) -> bool {
    let Some(re) = compile(pattern) else {
        return false;
    };
    read(root, rel)
        .map(|c| c.lines().any(|line| re.is_match(line)))
        .unwrap_or(false)
}

/// `grep -Ei` — case-insensitive regex, matched per line.
pub fn file_matches_ci(root: &Path, rel: &str, pattern: &str) -> bool {
    let Some(re) = compile_ci(pattern) else {
        return false;
    };
    read(root, rel)
        .map(|c| c.lines().any(|line| re.is_match(line)))
        .unwrap_or(false)
}

/// `require_slurp_match` — case-insensitive regex over the newline-flattened
/// file, so patterns may span lines.
pub fn file_slurp_matches_ci(root: &Path, rel: &str, pattern: &str) -> bool {
    let Some(re) = compile_ci(pattern) else {
        return false;
    };
    read(root, rel)
        .map(|c| re.is_match(&slurp_spaces(&c)))
        .unwrap_or(false)
}

/// The fenced `/goal` launch block of a skill file: the run of lines from the
/// `/goal …` marker up to (but not including) the closing ``` fence or the next
/// `## ` section heading, whichever comes first. `None` when the file has no
/// `/goal` marker.
///
/// SKILL liveness pins scope to this block so a duplicate of the launch-text
/// wording in a later status-row table cannot mask a regression in the
/// executable goal line (p4-cr10).
pub fn goal_block(root: &Path, rel: &str) -> Option<String> {
    let content = read(root, rel)?;
    let mut block: Option<String> = None;
    for line in content.lines() {
        match block {
            None => {
                if line.trim_start().starts_with("/goal") {
                    block = Some(format!("{line}\n"));
                }
            }
            Some(ref mut b) => {
                if line.trim() == "```" || line.starts_with("## ") {
                    break;
                }
                b.push_str(line);
                b.push('\n');
            }
        }
    }
    block
}

/// Case-insensitive slurp regex over a skill file's `/goal` launch block only.
pub fn goal_block_matches_ci(root: &Path, rel: &str, pattern: &str) -> bool {
    let Some(re) = compile_ci(pattern) else {
        return false;
    };
    goal_block(root, rel)
        .map(|b| re.is_match(&slurp_spaces(&b)))
        .unwrap_or(false)
}

/// Slurp-match over the concatenation of several files (used where a single
/// shell invariant now spans more than one Rust source file).
pub fn union_slurp_matches_ci(root: &Path, rels: &[&str], pattern: &str) -> bool {
    let Some(re) = compile_ci(pattern) else {
        return false;
    };
    let mut joined = String::new();
    for rel in rels {
        if let Some(c) = read(root, rel) {
            joined.push_str(&c);
            joined.push(' ');
        }
    }
    re.is_match(&slurp_spaces(&joined))
}

/// True when the repo-relative path is a regular file.
pub fn file_exists(root: &Path, rel: &str) -> bool {
    root.join(rel).is_file()
}

/// `grep -Fxq` — some line equals `needle` exactly.
pub fn file_has_exact_line(root: &Path, rel: &str, needle: &str) -> bool {
    read(root, rel)
        .map(|c| c.lines().any(|line| line == needle))
        .unwrap_or(false)
}

/// Count lines matching a case-sensitive anchored regex (`grep -Ec`).
pub fn count_lines_matching(root: &Path, rel: &str, pattern: &str) -> usize {
    let Some(re) = compile(pattern) else {
        return 0;
    };
    read(root, rel)
        .map(|c| c.lines().filter(|line| re.is_match(line)).count())
        .unwrap_or(0)
}

/// `require_jq '.turn_cap == 60'` — top-level integer `turn_cap` equals 60.
pub fn json_turn_cap_60(root: &Path, rel: &str) -> bool {
    read(root, rel)
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|v| v.get("turn_cap").map(|x| x.to_string()))
        .map(|s| s == "60")
        .unwrap_or(false)
}

/// Substring inside the `## Output Contract` .. `## Evidence Rules` section
/// (ports the shell's awk range scan).
pub fn output_contract_contains(root: &Path, rel: &str, needle: &str) -> bool {
    let Some(content) = read(root, rel) else {
        return false;
    };
    let mut in_contract = false;
    for line in content.lines() {
        if line.starts_with("## Output Contract") {
            in_contract = true;
            continue;
        }
        if line.starts_with("## Evidence Rules") {
            in_contract = false;
        }
        if in_contract && line.contains(needle) {
            return true;
        }
    }
    false
}

/// Concatenated text of every file under `surface` (files read as-is,
/// directories walked recursively, sorted), mirroring an `rg` sweep.
pub fn gather_surface(root: &Path, surface: &[&str]) -> String {
    let mut out = String::new();
    for rel in surface {
        collect_into(&root.join(rel), &mut out);
    }
    out
}

fn collect_into(path: &Path, out: &mut String) {
    if path.is_file() {
        if let Ok(c) = fs::read_to_string(path) {
            out.push_str(&c);
            out.push('\n');
        }
    } else if path.is_dir() {
        if let Ok(rd) = fs::read_dir(path) {
            let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            entries.sort();
            for entry in entries {
                collect_into(&entry, out);
            }
        }
    }
}

/// Literal substring present anywhere in gathered surface text.
pub fn surface_contains(surface: &str, needle: &str) -> bool {
    surface.contains(needle)
}

/// Case-sensitive regex matched against any line of the gathered surface.
pub fn surface_matches(surface: &str, pattern: &str) -> bool {
    let Some(re) = compile(pattern) else {
        return false;
    };
    surface.lines().any(|line| re.is_match(line))
}

/// Sorted list of `*.md` files directly under a repo-relative directory.
pub fn list_md(root: &Path, rel_dir: &str) -> Vec<PathBuf> {
    let dir = root.join(rel_dir);
    let mut v = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().map(|x| x == "md").unwrap_or(false) {
                v.push(p);
            }
        }
    }
    v.sort();
    v
}
