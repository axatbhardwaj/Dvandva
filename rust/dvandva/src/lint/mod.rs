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
    "Codex maps `opus` and `fable` to `gpt-5.6-sol` and `sonnet` and `gpt` to `gpt-5.6-terra`, falling back to `gpt-5.5` when a 5.6 model is unavailable on the active surface";
pub(crate) const MODEL_POLICY_CODEX_REVIEW_AUTHORITY: &str =
    "Codex-side `opus` and `fable` executions are GPT hygiene only and never earn review credit; credited deep/adversarial review remains a cross-vendor Anthropic Opus gate";
pub(crate) const MODEL_POLICY_CODEX_EFFORT: &str =
    "Codex reasoning effort is keyed to the thread role rather than the model class: the main session defaults to `xhigh` on every model and requests `max` only when the human sets it explicitly, while every dispatched Codex child is launched with an explicit `xhigh` effort because omitting it inherits the parent, may be lowered to `high`, `medium`, or `low` for proven-mechanical work, and never requests `max`; no Dvandva role uses `ultra` because its Codex-managed delegate threads run outside the baton's two-role coordination, and when a model does not support the requested effort the dispatching role keeps the selected model, drops to that model's highest supported effort, and logs the requested effort, effective effort, and reason";
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
pub(crate) const MODEL_POLICY_STALE_CODEX_EFFORT: &str =
    "Codex should request `xhigh` reasoning effort for opus-class and fable-class work and `high` reasoning effort for sonnet-class and gpt-class work where the active surface exposes it";
pub(crate) const MODEL_POLICY_STALE_CODEX_MAPPING_EFFORT: &str =
    "Codex maps `opus` and `fable` to `gpt-5.6-sol` with `xhigh` reasoning and `sonnet` and `gpt` to `gpt-5.6-terra` with `high` reasoning";
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

/// The canonical section heading that immediately precedes a SKILL file's
/// executable fenced `/goal` launch block. Extraction is anchored to this
/// heading (not merely to the first fenced `/goal` block) so a decoy fenced
/// `/goal` block placed elsewhere in the file cannot masquerade as the real
/// launch block.
const GOAL_LAUNCH_HEADING: &str = "## `/goal` condition (paste into your engine when launching)";

/// The canonical fenced `/goal` launch block that immediately follows the launch
/// heading ([`GOAL_LAUNCH_HEADING`]). By contract — verified against both live
/// SKILL launch sections — the block is EXACTLY: the heading, an opener fence, ONE
/// content line beginning with the `/goal` marker, and the bare closing fence on
/// the VERY NEXT line. The extractor returns just that single `/goal` content line.
///
/// Returns `None` — so every goal-scoped positive pin fails closed — whenever the
/// file departs from that canonical shape: no such block, MORE THAN ONE fenced
/// `/goal` block anywhere in the file, or a `/goal` line whose next line is not the
/// bare closer (a second content line, a heading, an info-string fence, a later
/// bare fence, or EOF). Three mechanisms guard the executable launch line:
///
/// * **Heading anchor.** Only fences after the canonical launch heading are
///   considered, so a fenced decoy `/goal` block placed BEFORE the heading
///   (p4-tc3-fenced-decoy-goal-bypass) is ignored rather than read in place of
///   the real block.
/// * **Duplicate fail-closed.** A second fenced `/goal` block anywhere in the
///   file is itself suspicious (a decoy alongside the real block), so the
///   extractor refuses to guess which is canonical and returns `None`.
/// * **Immediate-close contract.** The line after `/goal` MUST be the bare closing
///   fence; anything else fails closed. This ends the fence-closer whack-a-mole
///   permanently: because the block is bounded to exactly one content line, no
///   later fence of ANY kind — a bare fence opening a baton-seed block
///   (p4-cr11-later-plain-fence-tail-capture), an info-string fence
///   (p4-tc5-fence-closer-confusion), or an unclosed fence running to EOF
///   (p4-tc4-unclosed-goal-fence-tail-capture) — can be reinterpreted as this
///   block's closer to tail-capture unrelated later text (e.g. an F5 status row
///   that repeats the launch wording, p4-cr10). Multi-line capture is impossible
///   by construction.
///
/// Any `/goal` text OUTSIDE a code fence — a prose mention or an unfenced decoy
/// line (p4-dr12) — is ignored either way. SKILL liveness pins scope to this
/// block so neither a duplicate of the launch-text wording in a later status-row
/// table (p4-cr10) nor a decoy `/goal` block can mask a regression in the
/// executable goal line.
pub fn goal_block(root: &Path, rel: &str) -> Option<String> {
    let content = read(root, rel)?;

    // Fail closed on ambiguity: a second fenced `/goal` block anywhere in the
    // file is a decoy signal, so refuse to pick one.
    if count_fenced_goal_blocks(&content) > 1 {
        return None;
    }

    let mut past_heading = false;
    let mut in_fence = false;
    let mut block: Option<String> = None;
    let mut closed = false;
    for line in content.lines() {
        if !past_heading {
            past_heading = line.trim() == GOAL_LAUNCH_HEADING;
            continue;
        }
        let is_fence = line.trim_start().starts_with("```");
        match block {
            None => {
                if is_fence {
                    in_fence = !in_fence;
                } else if in_fence && line.trim_start().starts_with("/goal") {
                    block = Some(format!("{line}\n"));
                }
            }
            Some(_) => {
                // Canonical shape: the single `/goal` content line is IMMEDIATELY
                // followed by the bare closing fence. The very next line after the
                // `/goal` line MUST be that bare closer; a second content line, a
                // heading, an info-string fence (```json, ```text, …), a later bare
                // fence opening a new block, or EOF all mean this is not the
                // canonical launch block, so fail closed (`closed` stays false and
                // the trailing guard returns `None`). Bounding the block to exactly
                // one content line makes multi-line tail capture impossible by
                // construction: no later fence of ANY kind — bare or info-string —
                // can be reinterpreted as this block's closer to sweep unrelated
                // later text (an F5 status row repeating the launch wording, a
                // baton-seed block) into the captured block.
                closed = is_bare_fence(line);
                break;
            }
        }
    }
    // A `/goal` line not immediately followed by its bare closing fence fails
    // closed (the `Some` arm broke with `closed` still false) — the same
    // fail-closed policy as a duplicate block — so a malformed block never passes
    // goal-scoped pins against text that is not the launch block, including the
    // EOF case where the `/goal` line has no next line at all.
    if block.is_some() && !closed {
        return None;
    }
    block
}

/// A Markdown code fence closes only on a *bare* closing fence: ```` ``` ```` with
/// no info string (trailing whitespace allowed). A fence line carrying an info
/// string (```` ```json ````) is the OPENER of a new block and can never close an
/// already-open one, so it must not be mistaken for a closer.
fn is_bare_fence(line: &str) -> bool {
    line.trim() == "```"
}

/// Count the fenced code blocks that contain a line beginning with `/goal`.
///
/// Shares [`goal_block`]'s fence semantics: an info-string fence opens a block and
/// only a bare fence closes it, so an info-string fence encountered while already
/// inside a block is literal content, not a boundary.
fn count_fenced_goal_blocks(content: &str) -> usize {
    let mut count = 0;
    let mut in_fence = false;
    let mut this_fence_has_goal = false;
    for line in content.lines() {
        let is_fence = line.trim_start().starts_with("```");
        if is_fence && !in_fence {
            // Opener (bare or info-string): a new fenced block begins.
            in_fence = true;
            this_fence_has_goal = false;
            continue;
        }
        if is_fence && is_bare_fence(line) {
            // Bare closer: end the block, counting it if it held a `/goal`.
            if this_fence_has_goal {
                count += 1;
            }
            in_fence = false;
            this_fence_has_goal = false;
            continue;
        }
        // Either a non-fence line, or an info-string fence WHILE already inside a
        // block — the latter is literal content, never a closer.
        if in_fence && line.trim_start().starts_with("/goal") {
            this_fence_has_goal = true;
        }
    }
    count
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
