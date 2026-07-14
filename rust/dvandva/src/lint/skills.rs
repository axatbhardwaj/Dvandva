//! `lint skills` — port of `scripts/lint-skills.sh`.
//!
//! Lints a single `SKILL.md`: closed frontmatter, required `name` /
//! `description` fields, a description length cap, and a body length cap.
//! For `vadi`/`prativadi` role skills only, it additionally rejects
//! out-of-band final-approval instructions and requires EXACTLY ONE fenced
//! ` ```json ` block (the A2 precondition — a body carrying more than one
//! FAILS outright) whose top-level keys exactly match the engine's
//! `dvandva.baton.v3` required-key list (schema `dvandva.baton.v3`).
//!
//! The check compares the inline block against
//! [`crate::write::v2_required_keys`] plus the v3 `run_workflow` key — the
//! engine's OWN required-key base plus the live v3 envelope — rather than the retired
//! `plugins/dvandva/references/baton-schema.json` v1 file or historical v2
//! read-path reference. This keeps the skills' seed shape and the write
//! engine's contract from ever drifting.

use std::path::Path;

use serde_json::Value;

const USAGE: &str = "Usage: dvandva lint skills <path/to/SKILL.md>";

/// Run the lint against `args`; returns the process exit code (0 ok, 1
/// lint failure, 2 usage error).
pub fn run(args: &[String]) -> i32 {
    let [file_arg] = args else {
        eprintln!("{USAGE}");
        return 2;
    };
    let file = Path::new(file_arg);

    if !file.is_file() {
        eprintln!("FAIL: file not found: {file_arg}");
        return 1;
    }
    let Ok(content) = std::fs::read_to_string(file) else {
        eprintln!("FAIL: file not found: {file_arg}");
        return 1;
    };

    let dash_count = content.lines().filter(|l| *l == "---").count();
    if dash_count < 2 {
        eprintln!("FAIL: frontmatter block not closed (need two '---' lines) in {file_arg}");
        return 1;
    }

    let frontmatter = extract_frontmatter(&content);
    if frontmatter.trim().is_empty() {
        eprintln!("FAIL: no frontmatter block found in {file_arg}");
        return 1;
    }
    let frontmatter_lines: Vec<&str> = frontmatter.lines().collect();

    if !frontmatter_lines.iter().any(|l| l.starts_with("name: ")) {
        eprintln!("FAIL: missing required frontmatter field 'name' in {file_arg}");
        return 1;
    }
    if !frontmatter_lines
        .iter()
        .any(|l| l.starts_with("description: "))
    {
        eprintln!("FAIL: missing required frontmatter field 'description' in {file_arg}");
        return 1;
    }

    let desc: String = frontmatter_lines
        .iter()
        .filter(|l| l.starts_with("description: "))
        .map(|l| l.trim_start_matches("description: "))
        .collect::<Vec<_>>()
        .join("\n");
    let desc_len = desc.chars().count();
    if desc_len > 1536 {
        eprintln!("FAIL: description is {desc_len} chars (max 1536) in {file_arg}");
        return 1;
    }

    let name = frontmatter_lines
        .iter()
        .find(|l| l.starts_with("name: "))
        .map(|l| l.trim_start_matches("name: "))
        .unwrap_or("");

    let body_lines = body_line_count(&content);
    if body_lines > 500 {
        eprintln!("FAIL: body is {body_lines} lines (max 500) in {file_arg}");
        return 1;
    }

    if name != "vadi" && name != "prativadi" {
        println!("OK: {file_arg}");
        return 0;
    }

    let stale_lines: Vec<String> = content
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains("_final_approval: true") && !l.contains("termination_review"))
        .map(|(i, l)| format!("{}:{}", i + 1, l))
        .collect();
    if !stale_lines.is_empty() {
        for line in &stale_lines {
            eprintln!("FAIL: out-of-band final approval instruction in {file_arg}: {line}");
        }
        return 1;
    }

    let fence_count = count_json_fences(&content);
    if fence_count > 1 {
        eprintln!(
            "FAIL: {fence_count} fenced json blocks found in body of {file_arg} (single JSON fence required)"
        );
        return 1;
    }

    let json_block = extract_fenced_json_block(&content);
    if json_block.trim().is_empty() {
        eprintln!("FAIL: no fenced JSON block found in body of {file_arg}");
        return 1;
    }

    let parsed: Option<Value> = serde_json::from_str(&json_block).ok();
    let schema_ok = parsed
        .as_ref()
        .and_then(|v| v.get("schema"))
        .and_then(Value::as_str)
        == Some("dvandva.baton.v3");
    if !schema_ok {
        eprintln!("FAIL: inlined JSON block does not have schema=dvandva.baton.v3 in {file_arg}");
        return 1;
    }
    let inline_obj = parsed
        .as_ref()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    // The exact-key check compares against the engine's own v2 required-key
    // base plus the v3-only run_workflow envelope, not a bundled schema file.
    let mut required_keys = crate::write::v2_required_keys();
    required_keys.push("run_workflow");
    let mut sorted = required_keys.clone();
    sorted.sort_unstable();
    for key in &sorted {
        if !inline_obj.contains_key(*key) {
            eprintln!("FAIL: inlined JSON block missing required key '{key}' in {file_arg}");
            return 1;
        }
    }

    let mut unexpected: Vec<&String> = inline_obj
        .keys()
        .filter(|k| !required_keys.contains(&k.as_str()))
        .collect();
    unexpected.sort();
    if !unexpected.is_empty() {
        for key in &unexpected {
            eprintln!("FAIL: inlined JSON block has unexpected key '{key}' in {file_arg}");
        }
        return 1;
    }

    println!("OK: {file_arg}");
    0
}

/// Lines strictly between the first and second `^---$` marker.
fn extract_frontmatter(content: &str) -> String {
    let mut c = 0u32;
    let mut lines: Vec<&str> = Vec::new();
    for line in content.lines() {
        if line == "---" {
            c += 1;
            continue;
        }
        if c == 1 {
            lines.push(line);
        }
    }
    lines.join("\n")
}

/// Count of lines from the second `^---$` marker onward (exclusive of the
/// marker lines themselves).
fn body_line_count(content: &str) -> usize {
    let mut c = 0u32;
    let mut n = 0usize;
    for line in content.lines() {
        if line == "---" {
            c += 1;
            continue;
        }
        if c >= 2 {
            n += 1;
        }
    }
    n
}

/// Port of the shell's awk fence scanner: collects lines inside every
/// ` ```json ` … ` ``` ` fence found in the body (after the second `---`),
/// concatenated in document order. `pub(crate)` so the schema-parity lint
/// consumes this SAME scanner rather than a local re-implementation, keeping
/// the two lints' notion of "the inline contract block" from ever diverging.
/// Callers that require exactly one fence should check [`count_json_fences`]
/// first — a body with more than one fence is rejected upstream by both
/// lints before this function's concatenating behavior can mask the drift.
pub(crate) fn extract_fenced_json_block(content: &str) -> String {
    let mut c = 0u32;
    let mut flag = false;
    let mut collected: Vec<&str> = Vec::new();
    for line in content.lines() {
        if line == "---" {
            c += 1;
            continue;
        }
        if c >= 2 && line == "```json" {
            flag = true;
            continue;
        }
        if c >= 2 && line == "```" {
            flag = false;
        }
        if flag {
            collected.push(line);
        }
    }
    collected.join("\n")
}

/// Count of ` ```json ` fence-open lines in the body (after the second
/// `---`). The A2 precondition: a SKILL.md body must carry exactly one JSON
/// fence — more than one is rejected outright rather than silently
/// concatenated by [`extract_fenced_json_block`].
pub(crate) fn count_json_fences(content: &str) -> usize {
    let mut c = 0u32;
    let mut n = 0usize;
    for line in content.lines() {
        if line == "---" {
            c += 1;
            continue;
        }
        if c >= 2 && line == "```json" {
            n += 1;
        }
    }
    n
}
