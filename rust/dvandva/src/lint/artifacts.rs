//! `lint artifacts` — port of `scripts/lint-artifacts.sh`.
//!
//! Lints generated human-facing Dvandva artifacts (`pr_review` / `bug_rca` /
//! `run_explainer` HTML, plus the blanket "no generated Markdown" and
//! "no external / path-traversal references" rules) under one or more
//! targets (directories, `.md` files, or `.html` files).
//!
//! Divergence from the shell: the shell derived its repo root from its own
//! script location (`$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)`),
//! which no longer exists once ported to a compiled binary. This port
//! derives the repo root from the current working directory's git toplevel
//! instead ([`crate::gitcfg::repo_toplevel`]), falling back to the cwd
//! itself when not inside a git worktree. The default target (used when no
//! args are given) is `<repo-root>/superpowers`.

use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::Value;

const RUN_EXPLAINER_SECTIONS: [&str; 5] = [
    "decisions",
    "development",
    "architecture",
    "verification",
    "diagrams",
];
const PR_REVIEW_SECTIONS: [&str; 4] = ["verdict", "severity", "findings", "ground-truth"];
const BUG_RCA_SECTIONS: [&str; 4] = ["symptom", "hypotheses", "root-cause", "fix-direction"];

/// Run the lint against `args`; returns the process exit code (0 pass, 1
/// findings).
pub fn run(args: &[String]) -> i32 {
    let mut failures = 0i32;
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root_dir = crate::gitcfg::repo_toplevel(&cwd).unwrap_or(cwd);

    let targets: Vec<String> = if args.is_empty() {
        vec![root_dir.join("superpowers").to_string_lossy().into_owned()]
    } else {
        args.to_vec()
    };

    let mut markdown_files: Vec<PathBuf> = Vec::new();
    let mut html_files: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut existing_targets = 0usize;

    for target in &targets {
        let target_path = Path::new(target);
        if !target_path.exists() {
            pass("no generated artifact directory present");
            continue;
        }

        existing_targets += 1;
        let target_abs = canonicalize_or_self(target_path);
        if target_abs.is_dir() {
            markdown_files.extend(collect_files_with_ext(&target_abs, "md"));
            for file in collect_files_with_ext(&target_abs, "html") {
                html_files.push((file, target_abs.clone()));
            }
        } else if has_ext(&target_abs, "md") {
            markdown_files.push(target_abs);
        } else if has_ext(&target_abs, "html") {
            let base = target_abs
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
            html_files.push((target_abs, base));
        }
    }

    if existing_targets == 0 {
        return 0;
    }

    let scope = targets.join(" ");

    if !markdown_files.is_empty() {
        fail(
            &format!("generated Markdown artifacts are not allowed under {scope}"),
            &mut failures,
        );
        for file in &markdown_files {
            println!("  {}", strip_root_prefix(file, &root_dir));
        }
    } else {
        pass(&format!("no generated Markdown artifacts under {scope}"));
    }

    if html_files.is_empty() {
        fail(
            &format!("no generated HTML artifacts found under {scope}"),
            &mut failures,
        );
    }

    for (file, base) in &html_files {
        lint_html_file(file, base, &root_dir, &mut failures);
    }

    if failures > 0 {
        1
    } else {
        0
    }
}

fn pass(msg: &str) {
    println!("PASS: {msg}");
}

fn fail(msg: &str, failures: &mut i32) {
    println!("FAIL: {msg}");
    *failures += 1;
}

fn canonicalize_or_self(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some(ext)
}

/// Recursively collect files with the given extension under `dir`, sorted
/// by absolute path (mirrors `find … | sort`).
fn collect_files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if has_ext(&path, ext) {
                out.push(path);
            }
        }
    }
    out.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    out
}

fn strip_root_prefix(path: &Path, root_dir: &Path) -> String {
    let path_s = path.to_string_lossy().into_owned();
    let prefix = format!("{}/", root_dir.to_string_lossy());
    path_s
        .strip_prefix(prefix.as_str())
        .map(str::to_owned)
        .unwrap_or(path_s)
}

fn artifact_rel_for_file(file: &Path, base: &Path, root_dir: &Path) -> String {
    let file_s = file.to_string_lossy().into_owned();
    let superpowers_prefix = format!("{}/superpowers/", root_dir.to_string_lossy());
    if let Some(rest) = file_s.strip_prefix(superpowers_prefix.as_str()) {
        return rest.to_string();
    }
    let base_s = base.to_string_lossy();
    if !base_s.is_empty() {
        let base_prefix = format!("{base_s}/");
        if let Some(rest) = file_s.strip_prefix(base_prefix.as_str()) {
            return rest.to_string();
        }
    }
    file.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(file_s)
}

fn any_line_matches(content: &str, re: &Regex) -> bool {
    content.lines().any(|line| re.is_match(line))
}

/// Port of the shell's `run_explainer_stem_matches_run_id`: date-prefix
/// aware comparison between a `run-reports/<stem>-explainer.html` filename
/// stem and the artifact's `run_id` metadata field.
fn run_explainer_stem_matches_run_id(stem: &str, run_id: &str) -> bool {
    if !crate::util::is_safe_run_id(run_id) {
        return false;
    }
    let date_prefixed = Regex::new(r"^\d{4}-\d{2}-\d{2}-").expect("static regex");
    if date_prefixed.is_match(run_id) {
        return stem == run_id;
    }
    let stem_date = Regex::new(r"^\d{4}-\d{2}-\d{2}-(.+)$").expect("static regex");
    if let Some(caps) = stem_date.captures(stem) {
        return &caps[1] == run_id;
    }
    false
}

/// Extract the text between the `id="dvandva-artifact-meta"` script tag and
/// the following `</script>`. Returns `None` when no such open tag is
/// found; `Some("")` when the tag is found but the block is empty.
fn extract_meta_block(content: &str) -> Option<String> {
    let open_re =
        Regex::new(r#"<script[^>]*id="dvandva-artifact-meta"[^>]*>"#).expect("static regex");
    let mut flag = false;
    let mut collected: Vec<&str> = Vec::new();
    for line in content.lines() {
        if !flag {
            if open_re.is_match(line) {
                flag = true;
            }
            continue;
        }
        if line.contains("</script>") {
            break;
        }
        collected.push(line);
    }
    if !flag {
        None
    } else {
        Some(collected.join("\n"))
    }
}

fn lint_html_file(file: &Path, base: &Path, root_dir: &Path, failures: &mut i32) {
    let rel = strip_root_prefix(file, root_dir);
    let artifact_rel = artifact_rel_for_file(file, base, root_dir);
    let content = std::fs::read_to_string(file).unwrap_or_default();

    let first_five = content.lines().take(5).collect::<Vec<_>>().join("\n");
    if first_five.to_lowercase().contains("<!doctype html") {
        pass(&format!("{rel} declares HTML doctype"));
    } else {
        fail(&format!("{rel} missing HTML doctype"), failures);
    }

    if content.contains("color-scheme: dark") {
        pass(&format!("{rel} declares dark color scheme"));
    } else {
        fail(&format!("{rel} missing dark color-scheme"), failures);
    }

    let meta_tag_re = Regex::new(
        r#"<script[^>]+type="application/json"[^>]+id="dvandva-artifact-meta"|<script[^>]+id="dvandva-artifact-meta"[^>]+type="application/json""#,
    )
    .expect("static regex");
    if any_line_matches(&content, &meta_tag_re) {
        pass(&format!("{rel} includes Dvandva artifact metadata block"));
    } else {
        fail(
            &format!("{rel} missing Dvandva artifact metadata block"),
            failures,
        );
    }

    let meta_text = extract_meta_block(&content);
    let meta_value: Option<Value> = meta_text
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .and_then(|t| serde_json::from_str::<Value>(t.trim()).ok());
    let schema_ok = meta_value
        .as_ref()
        .and_then(|v| v.get("schema"))
        .and_then(Value::as_str)
        .map(|s| s.starts_with("dvandva.artifact."))
        .unwrap_or(false);
    if schema_ok {
        pass(&format!("{rel} metadata JSON parses"));
    } else {
        fail(&format!("{rel} metadata JSON missing or invalid"), failures);
    }

    let artifact_type = meta_value
        .as_ref()
        .and_then(|v| v.get("artifact_type"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let artifact_schema = meta_value
        .as_ref()
        .and_then(|v| v.get("schema"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    if artifact_schema == "dvandva.artifact.pr_review.v1" && artifact_type != "pr_review" {
        fail(
            &format!("{rel} pr_review schema requires artifact_type pr_review"),
            failures,
        );
    }
    if artifact_schema == "dvandva.artifact.bug_rca.v1" && artifact_type != "bug_rca" {
        fail(
            &format!("{rel} bug_rca schema requires artifact_type bug_rca"),
            failures,
        );
    }
    if artifact_schema == "dvandva.artifact.run_explainer.v1" && artifact_type != "run_explainer" {
        fail(
            &format!("{rel} run_explainer schema requires artifact_type run_explainer"),
            failures,
        );
    }

    let baton_state_re = Regex::new(r"(?i)BATON_STATE([^_A-Z0-9]|$)").expect("static regex");
    let baton_dump_re = Regex::new(r#"(?i)"(work_split|subagent_tracks|verification_matrix)"\s*:"#)
        .expect("static regex");
    if any_line_matches(&content, &baton_state_re) && any_line_matches(&content, &baton_dump_re) {
        fail(
            &format!("{rel} contains routine full BATON_STATE dynamic-array dump"),
            failures,
        );
    } else {
        pass(&format!(
            "{rel} avoids routine full BATON_STATE dynamic-array dumps"
        ));
    }

    if artifact_type == "run_explainer" {
        lint_run_explainer(&rel, &artifact_rel, &content, meta_value.as_ref(), failures);
    }
    if artifact_type == "pr_review" {
        lint_pr_review(&rel, &content, meta_value.as_ref(), failures);
    }
    if artifact_type == "bug_rca" {
        lint_bug_rca(&rel, &content, meta_value.as_ref(), failures);
    }

    lint_external_and_traversal_refs(&rel, &content, failures);
}

fn check_section_id(rel: &str, section: &str, content: &str, failures: &mut i32) {
    let pattern = format!(r#"(?i)id=["']{section}["']"#);
    let re = Regex::new(&pattern).expect("dynamic id regex");
    if any_line_matches(content, &re) {
        pass(&format!("{rel} includes #{section} section"));
    } else {
        fail(&format!("{rel} missing #{section} section"), failures);
    }
}

#[allow(clippy::too_many_arguments)]
fn check_marker(
    rel: &str,
    content: &str,
    pattern: &str,
    pass_msg: &str,
    fail_msg: &str,
    failures: &mut i32,
) {
    let re = Regex::new(pattern).expect("static regex");
    if any_line_matches(content, &re) {
        pass(&format!("{rel} {pass_msg}"));
    } else {
        fail(&format!("{rel} {fail_msg}"), failures);
    }
}

fn run_explainer_metadata_complete(meta: Option<&Value>, meta_run_id: &str) -> bool {
    let Some(meta) = meta else {
        return false;
    };
    let schema_ok =
        meta.get("schema").and_then(Value::as_str) == Some("dvandva.artifact.run_explainer.v1");
    let type_ok = meta.get("artifact_type").and_then(Value::as_str) == Some("run_explainer");
    let run_id_ok = matches!(meta.get("run_id"), Some(Value::String(_)));
    let expected_baton_ref = format!(".dvandva/runs/{meta_run_id}/baton.json");
    let baton_ref_ok =
        meta.get("baton_ref").and_then(Value::as_str) == Some(expected_baton_ref.as_str());
    let has_final_commit = meta.get("final_commit").is_some();
    let sections_ok = meta
        .get("sections")
        .and_then(Value::as_array)
        .map(|arr| {
            let present: std::collections::HashSet<&str> =
                arr.iter().filter_map(Value::as_str).collect();
            RUN_EXPLAINER_SECTIONS.iter().all(|s| present.contains(s))
        })
        .unwrap_or(false);
    schema_ok && type_ok && run_id_ok && baton_ref_ok && has_final_commit && sections_ok
}

fn lint_run_explainer(
    rel: &str,
    artifact_rel: &str,
    content: &str,
    meta: Option<&Value>,
    failures: &mut i32,
) {
    let meta_run_id = meta
        .and_then(|m| m.get("run_id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let path_re =
        Regex::new(r"^run-reports/([A-Za-z0-9._-]+)-explainer\.html$").expect("static regex");
    let candidate_stem = path_re.captures(artifact_rel).map(|c| c[1].to_string());

    let file_stem = candidate_stem
        .as_deref()
        .filter(|stem| run_explainer_stem_matches_run_id(stem, &meta_run_id))
        .map(str::to_string);

    if file_stem.is_some() {
        pass(&format!("{rel} run explainer path is canonical"));
    } else {
        fail(
            &format!(
                "{rel} run explainer path must be run-reports/YYYY-MM-DD-<run_id>-explainer.html, or <run_id>-explainer.html when run_id is already date-prefixed"
            ),
            failures,
        );
    }

    let meta_complete = file_stem.is_some() && run_explainer_metadata_complete(meta, &meta_run_id);
    if meta_complete {
        pass(&format!("{rel} run explainer metadata is complete"));
    } else {
        fail(
            &format!("{rel} run explainer metadata missing required fields or sections"),
            failures,
        );
    }

    for section in RUN_EXPLAINER_SECTIONS {
        check_section_id(rel, section, content, failures);
    }

    check_marker(
        rel,
        content,
        r"(?i)<svg(\s|>)",
        "includes inline SVG diagram",
        "missing inline SVG diagram",
        failures,
    );
}

fn lint_pr_review(rel: &str, content: &str, meta: Option<&Value>, failures: &mut i32) {
    let ok = meta
        .map(|m| {
            m.get("schema").and_then(Value::as_str) == Some("dvandva.artifact.pr_review.v1")
                && m.get("artifact_type").and_then(Value::as_str) == Some("pr_review")
        })
        .unwrap_or(false);
    if ok {
        pass(&format!(
            "{rel} pr_review metadata schema and artifact_type match"
        ));
    } else {
        fail(
            &format!(
                "{rel} pr_review metadata schema must be dvandva.artifact.pr_review.v1 and artifact_type must be pr_review"
            ),
            failures,
        );
    }

    for section in PR_REVIEW_SECTIONS {
        check_section_id(rel, section, content, failures);
    }

    check_marker(
        rel,
        content,
        r"(?i)<table(\s|>)",
        "includes PR review severity table",
        "missing PR review severity table",
        failures,
    );
}

fn lint_bug_rca(rel: &str, content: &str, meta: Option<&Value>, failures: &mut i32) {
    let ok = meta
        .map(|m| {
            m.get("schema").and_then(Value::as_str) == Some("dvandva.artifact.bug_rca.v1")
                && m.get("artifact_type").and_then(Value::as_str) == Some("bug_rca")
        })
        .unwrap_or(false);
    if ok {
        pass(&format!(
            "{rel} bug_rca metadata schema and artifact_type match"
        ));
    } else {
        fail(
            &format!(
                "{rel} bug_rca metadata schema must be dvandva.artifact.bug_rca.v1 and artifact_type must be bug_rca"
            ),
            failures,
        );
    }

    for section in BUG_RCA_SECTIONS {
        check_section_id(rel, section, content, failures);
    }

    check_marker(
        rel,
        content,
        r"(?i)<svg(\s|>)",
        "includes bug RCA causal-chain SVG",
        "missing bug RCA causal-chain SVG",
        failures,
    );
}

fn resource_ref_regexes(value_pattern: &str) -> Vec<Regex> {
    let quote = r#"["']?"#;
    [
        format!(r#"(?i)<script[^>]+src\s*=\s*{quote}{value_pattern}"#),
        format!(r#"(?i)<link[^>]+href\s*=\s*{quote}{value_pattern}"#),
        format!(r#"(?i)<(img|iframe|source|video|audio)[^>]+src\s*=\s*{quote}{value_pattern}"#),
        format!(r#"(?i)url\(\s*{quote}{value_pattern}"#),
        format!(r#"(?i)@import\s+(url\(\s*)?{quote}{value_pattern}"#),
    ]
    .iter()
    .map(|p| Regex::new(p).expect("dynamic resource-ref regex"))
    .collect()
}

fn lint_external_and_traversal_refs(rel: &str, content: &str, failures: &mut i32) {
    let external = resource_ref_regexes(r"https?://");
    if external.iter().any(|re| any_line_matches(content, re)) {
        fail(
            &format!("{rel} contains external resource reference"),
            failures,
        );
    } else {
        pass(&format!("{rel} has no external resource references"));
    }

    let traversal = resource_ref_regexes(r"\.\./");
    if traversal.iter().any(|re| any_line_matches(content, re)) {
        fail(
            &format!("{rel} contains path-traversal ref (../)"),
            failures,
        );
    } else {
        pass(&format!("{rel} has no path-traversal refs"));
    }
}
