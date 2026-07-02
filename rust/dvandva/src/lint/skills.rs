//! `lint skills` — port of `scripts/lint-skills.sh`.
//!
//! Lints a single `SKILL.md`: closed frontmatter, required `name` /
//! `description` fields, a description length cap, and a body length cap.
//! For `vadi`/`prativadi` role skills only, it additionally rejects
//! out-of-band final-approval instructions and requires a fenced ```json
//! block whose keys exactly match the top-level keys of
//! `plugins/dvandva/references/baton-schema.json` (schema
//! `dvandva.baton.v1`).
//!
//! Divergence from the shell: the shell resolved the v1 schema reference
//! relative to its own fixed script location
//! (`plugins/dvandva/references/baton-schema.json` next to the repo the
//! script lived in). This port instead walks up from the SKILL.md's
//! directory looking for a sibling `references/baton-schema.json` (i.e. the
//! enclosing plugin root), falling back to
//! `<repo-root>/plugins/dvandva/references/baton-schema.json` (repo root via
//! [`crate::gitcfg::repo_toplevel`] from the current working directory)
//! when no such ancestor is found — e.g. for standalone fixture files that
//! live outside any plugin tree.

use std::path::{Path, PathBuf};

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
        == Some("dvandva.baton.v1");
    if !schema_ok {
        eprintln!("FAIL: inlined JSON block does not have schema=dvandva.baton.v1 in {file_arg}");
        return 1;
    }
    let inline_obj = parsed
        .as_ref()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let v1_schema_path = resolve_v1_schema_path(file);
    let Ok(schema_bytes) = std::fs::read(&v1_schema_path) else {
        eprintln!(
            "FAIL: v1 baton schema reference not found: {}",
            v1_schema_path.display()
        );
        return 1;
    };
    let Ok(schema_value) = serde_json::from_slice::<Value>(&schema_bytes) else {
        eprintln!(
            "FAIL: v1 baton schema reference not found: {}",
            v1_schema_path.display()
        );
        return 1;
    };
    let schema_obj = schema_value.as_object().cloned().unwrap_or_default();

    let mut required_keys: Vec<&String> = schema_obj.keys().collect();
    required_keys.sort();
    for key in &required_keys {
        if !inline_obj.contains_key(key.as_str()) {
            eprintln!("FAIL: inlined JSON block missing required key '{key}' in {file_arg}");
            return 1;
        }
    }

    let mut unexpected: Vec<&String> = inline_obj
        .keys()
        .filter(|k| !schema_obj.contains_key(k.as_str()))
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

/// Port of the shell's awk fence scanner: collects lines inside the first
/// ` ```json ` … ` ``` ` fence found in the body (after the second `---`).
fn extract_fenced_json_block(content: &str) -> String {
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

/// Resolve the v1 baton schema reference file for `skill_file`: walk up
/// from its directory looking for a sibling `references/baton-schema.json`
/// (the plugin root), falling back to
/// `<repo-root>/plugins/dvandva/references/baton-schema.json`.
fn resolve_v1_schema_path(skill_file: &Path) -> PathBuf {
    let abs = std::fs::canonicalize(skill_file).unwrap_or_else(|_| skill_file.to_path_buf());
    let mut dir = abs.parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        let candidate = d.join("references").join("baton-schema.json");
        if candidate.is_file() {
            return candidate;
        }
        dir = d.parent().map(Path::to_path_buf);
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_root = crate::gitcfg::repo_toplevel(&cwd).unwrap_or(cwd);
    repo_root
        .join("plugins")
        .join("dvandva")
        .join("references")
        .join("baton-schema.json")
}
