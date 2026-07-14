//! Integration tests for `dvandva lint artifacts` — ported case-by-case
//! from `scripts/test-lint-artifacts.sh`, plus cwd-based coverage of the
//! default-target divergence (repo root derived from the current working
//! directory's git toplevel instead of the script's own location).

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};

fn dvandva() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dvandva"))
}

fn run_lint(args: &[&str]) -> std::process::Output {
    dvandva()
        .arg("lint")
        .arg("artifacts")
        .args(args)
        .output()
        .expect("failed to run dvandva lint artifacts")
}

fn assert_exit(args: &[&str], expected: i32) {
    let out = run_lint(args);
    assert_eq!(
        out.status.code(),
        Some(expected),
        "args: {args:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn assert_exit_in_dir(cwd: &Path, args: &[&str], expected: i32) {
    let out = dvandva()
        .current_dir(cwd)
        .arg("lint")
        .arg("artifacts")
        .args(args)
        .output()
        .expect("failed to run dvandva lint artifacts");
    assert_eq!(
        out.status.code(),
        Some(expected),
        "cwd: {}\nargs: {args:?}\nstdout: {}\nstderr: {}",
        cwd.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write_html(path: &Path, title: &str, body: &str, meta: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let content = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>{title}</title>\n  <style>:root{{color-scheme: dark;--bg:#090b10}}body{{background:var(--bg);color:#eef3f8}}</style>\n</head>\n<body>\n{body}\n<script type=\"application/json\" id=\"dvandva-artifact-meta\">\n{meta}\n</script>\n</body>\n</html>\n"
    );
    std::fs::write(path, content).unwrap();
}

fn write_artifact(path: &Path, body: &str) {
    write_html(
        path,
        "Artifact test",
        body,
        &json!({"schema": "dvandva.artifact.test.v1", "artifact_type": "test"}),
    );
}

fn write_run_explainer(path: &Path, body: &str, run_id: &str) {
    let meta = json!({
        "schema": "dvandva.artifact.run_explainer.v1",
        "artifact_type": "run_explainer",
        "run_id": run_id,
        "baton_ref": format!(".dvandva/runs/{run_id}/baton.json"),
        "final_commit": null,
        "sections": ["decisions", "development", "architecture", "verification", "diagrams"],
    });
    write_html(path, "Run explainer", body, &meta);
}

fn write_pr_review(path: &Path, body: &str) {
    write_html(
        path,
        "PR Review",
        body,
        &json!({"schema": "dvandva.artifact.pr_review.v1", "artifact_type": "pr_review"}),
    );
}

fn write_bug_rca(path: &Path, body: &str) {
    write_html(
        path,
        "Bug RCA",
        body,
        &json!({"schema": "dvandva.artifact.bug_rca.v1", "artifact_type": "bug_rca"}),
    );
}

const RUN_EXPLAINER_GOOD_BODY: &str = r#"
  <main>
    <section id="decisions"><h2>Decisions</h2></section>
    <section id="development"><h2>Development</h2></section>
    <section id="architecture"><h2>Architecture</h2></section>
    <section id="verification"><h2>Verification</h2></section>
    <section id="diagrams"><h2>Diagrams</h2><svg viewBox="0 0 10 10"><path d="M1 1h8v8H1z"/></svg></section>
  </main>"#;

const PR_REVIEW_GOOD_BODY: &str = r#"
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2><table><tr><th>Severity</th><th>Count</th></tr><tr><td>None</td><td>0</td></tr></table></section>
  <section id="findings"><h2>Findings</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>"#;

const BUG_RCA_GOOD_BODY: &str = r#"
  <section id="symptom"><h2>Symptom</h2></section>
  <section id="hypotheses"><h2>Hypotheses</h2></section>
  <section id="root-cause"><h2>Root Cause</h2><svg viewBox="0 0 10 10"><path d="M1 5h8"/></svg></section>
  <section id="fix-direction"><h2>Fix Direction</h2></section>"#;

// --- prose / navigation URLs are allowed ---

#[test]
fn prose_source_url_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    write_artifact(
        &dir.path().join("report.html"),
        "<p>Source inventory: https://github.com/axatbhardwaj/Dvandva</p>",
    );
    assert_exit(&[dir.path().to_str().unwrap()], 0);
}

#[test]
fn navigation_href_url_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    write_artifact(
        &dir.path().join("report.html"),
        r#"<p><a href="https://github.com/axatbhardwaj/Dvandva">source</a></p>"#,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 0);
}

// --- external resource references are rejected ---

#[test]
fn external_script_resource_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    write_artifact(
        &dir.path().join("report.html"),
        r#"<script src="https://cdn.example.com/app.js"></script>"#,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn external_image_resource_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    write_artifact(
        &dir.path().join("report.html"),
        r#"<img src="http://example.com/image.png" alt="external">"#,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn external_iframe_resource_with_spaced_attribute_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    write_artifact(
        &dir.path().join("report.html"),
        r#"<iframe src = "https://example.com/embed"></iframe>"#,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn external_css_resource_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    write_artifact(
        &dir.path().join("report.html"),
        r#"<style>.hero{background-image:url("https://example.com/bg.png")}@import url("http://example.com/theme.css");</style>"#,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

// --- metadata tag presence check is case-insensitive (B10) ---

#[test]
fn uppercase_meta_tag_passes_presence_check() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("uppercase-meta.html");
    std::fs::write(
        &file,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>Uppercase meta tag test</title>\n  <style>:root{color-scheme: dark;--bg:#090b10}body{background:var(--bg);color:#eef3f8}</style>\n</head>\n<body>\n<SCRIPT TYPE=\"application/json\" ID=\"dvandva-artifact-meta\">\n{\"schema\": \"dvandva.artifact.test.v1\", \"artifact_type\": \"test\"}\n</SCRIPT>\n</body>\n</html>\n",
    )
    .unwrap();

    let out = run_lint(&[file.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("includes Dvandva artifact metadata block"),
        "presence check must pass for an uppercase <SCRIPT ... ID=\"dvandva-artifact-meta\"> tag: {stdout}"
    );
}

// --- generated Markdown is rejected ---

#[test]
fn generated_markdown_artifact_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let md = dir.path().join("plans/generated.md");
    std::fs::create_dir_all(md.parent().unwrap()).unwrap();
    std::fs::write(&md, "# generated markdown artifact\n").unwrap();
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

// --- BATON_STATE dump checks ---

#[test]
fn routine_full_baton_state_dynamic_arrays_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    write_artifact(
        &dir.path().join("report.html"),
        r#"<pre>BATON_STATE: {"work_split":[{"id":"too-much"}],"subagent_tracks":[],"verification_matrix":[]}</pre>"#,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn routine_full_baton_state_with_any_dynamic_array_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    write_artifact(
        &dir.path().join("report.html"),
        r#"<pre>BATON_STATE: {"work_split":[{"id":"too-much"}],"subagent_tracks":[]}</pre>"#,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn compact_baton_state_counts_and_refs_are_accepted() {
    let dir = tempfile::tempdir().unwrap();
    write_artifact(
        &dir.path().join("report.html"),
        r#"<pre>BATON_STATE_COMPACT: {"counts":{"work_split":12,"subagent_tracks":8,"verification_matrix":5},"refs":{"plan_ref":"./superpowers/plans/x.html"}}</pre>"#,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 0);
}

// --- target existence / multiple targets ---

#[test]
fn missing_artifact_directory_remains_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing-artifacts");
    assert_exit(&[missing.to_str().unwrap()], 0);
}

#[test]
fn direct_html_file_target_is_linted() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("direct-bad-meta.html");
    std::fs::write(
        &file,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>Direct file artifact test</title>\n  <style>:root{color-scheme: dark;--bg:#090b10}body{background:var(--bg);color:#eef3f8}</style>\n</head>\n<body>\n  <p>This direct file intentionally lacks Dvandva artifact metadata.</p>\n</body>\n</html>\n",
    )
    .unwrap();
    assert_exit(&[file.to_str().unwrap()], 1);
}

#[test]
fn multiple_artifact_targets_are_all_linted() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("multi-good");
    write_artifact(&good.join("report.html"), "<p>Good first target.</p>");
    let bad = dir.path().join("multi-bad.html");
    std::fs::write(
        &bad,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>Second target artifact test</title>\n  <style>:root{color-scheme: dark;--bg:#090b10}body{background:var(--bg);color:#eef3f8}</style>\n</head>\n<body>\n  <p>This second target intentionally lacks Dvandva artifact metadata.</p>\n</body>\n</html>\n",
    )
    .unwrap();
    assert_exit(&[good.to_str().unwrap(), bad.to_str().unwrap()], 1);
}

// --- run_explainer cases ---

#[test]
fn run_explainer_artifact_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("run-reports/2026-06-28-run-a-explainer.html");
    write_run_explainer(&file, RUN_EXPLAINER_GOOD_BODY, "run-a");
    assert_exit(&[dir.path().to_str().unwrap()], 0);
}

#[test]
fn date_prefixed_run_explainer_artifact_is_accepted_without_double_date() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("run-reports/2026-06-29-baton-accuracy-hook-coexist-explainer.html");
    write_run_explainer(
        &file,
        RUN_EXPLAINER_GOOD_BODY,
        "2026-06-29-baton-accuracy-hook-coexist",
    );
    assert_exit(&[dir.path().to_str().unwrap()], 0);
}

#[test]
fn date_prefixed_run_explainer_artifact_rejects_double_date() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("run-reports/2026-06-30-2026-06-29-baton-accuracy-hook-coexist-explainer.html");
    write_run_explainer(
        &file,
        RUN_EXPLAINER_GOOD_BODY,
        "2026-06-29-baton-accuracy-hook-coexist",
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn run_explainer_outside_run_reports_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("report.html");
    write_run_explainer(&file, RUN_EXPLAINER_GOOD_BODY, "run-a");
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn run_explainer_missing_required_section_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("run-reports/2026-06-28-run-a-explainer.html");
    let body = r#"
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="verification"></section>
  <section id="diagram"><svg viewBox="0 0 10 10"></svg></section>"#;
    write_run_explainer(&file, body, "run-a");
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn run_explainer_missing_verification_section_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("run-reports/2026-06-28-run-a-explainer.html");
    let body = r#"
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="diagrams"><svg viewBox="0 0 10 10"></svg></section>"#;
    write_run_explainer(&file, body, "run-a");
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn run_explainer_missing_inline_svg_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("run-reports/2026-06-28-run-a-explainer.html");
    let body = r#"
  <section id="decisions"></section>
  <section id="development"></section>
  <section id="architecture"></section>
  <section id="verification"></section>
  <section id="diagrams"></section>"#;
    write_run_explainer(&file, body, "run-a");
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn run_explainer_metadata_missing_sections_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("run-reports/2026-06-28-run-a-explainer.html");
    let meta = json!({
        "schema": "dvandva.artifact.run_explainer.v1",
        "artifact_type": "run_explainer",
        "run_id": "run-a",
        "baton_ref": ".dvandva/runs/run-a/baton.json",
        "final_commit": null,
        "sections": ["decisions", "development", "architecture"],
    });
    write_html(&file, "Run explainer", RUN_EXPLAINER_GOOD_BODY, &meta);
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn run_explainer_metadata_run_id_must_match_filename() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("run-reports/2026-06-28-run-a-explainer.html");
    let meta = json!({
        "schema": "dvandva.artifact.run_explainer.v1",
        "artifact_type": "run_explainer",
        "run_id": "other-run",
        "baton_ref": ".dvandva/runs/other-run/baton.json",
        "final_commit": null,
        "sections": ["decisions", "development", "architecture", "verification", "diagrams"],
    });
    write_html(&file, "Run explainer", RUN_EXPLAINER_GOOD_BODY, &meta);
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn run_explainer_reserved_schema_with_wrong_artifact_type_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("run-reports/2026-06-28-run-a-explainer.html");
    let meta = json!({
        "schema": "dvandva.artifact.run_explainer.v1",
        "artifact_type": "test",
        "run_id": "run-a",
        "baton_ref": ".dvandva/runs/run-a/baton.json",
        "final_commit": null,
        "sections": ["decisions", "development", "architecture", "verification", "diagrams"],
    });
    write_html(&file, "Run explainer", RUN_EXPLAINER_GOOD_BODY, &meta);
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn run_explainer_reserved_schema_missing_artifact_type_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("run-reports/2026-06-28-run-a-explainer.html");
    let meta = json!({
        "schema": "dvandva.artifact.run_explainer.v1",
        "run_id": "run-a",
        "baton_ref": ".dvandva/runs/run-a/baton.json",
        "final_commit": null,
        "sections": ["decisions", "development", "architecture", "verification", "diagrams"],
    });
    write_html(&file, "Run explainer", RUN_EXPLAINER_GOOD_BODY, &meta);
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

// --- pr_review cases ---

#[test]
fn valid_pr_review_artifact_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    write_pr_review(&dir.path().join("report.html"), PR_REVIEW_GOOD_BODY);
    assert_exit(&[dir.path().to_str().unwrap()], 0);
}

#[test]
fn pr_review_missing_findings_section_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>"#;
    write_pr_review(&dir.path().join("report.html"), body);
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn pr_review_external_https_resource_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let body = format!(
        r#"<link href="https://cdn.example.com/style.css" rel="stylesheet">{PR_REVIEW_GOOD_BODY}"#
    );
    write_pr_review(&dir.path().join("report.html"), &body);
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn pr_review_path_traversal_ref_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let body = format!(r#"<link href="../styles.css" rel="stylesheet">{PR_REVIEW_GOOD_BODY}"#);
    write_pr_review(&dir.path().join("report.html"), &body);
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn pr_review_with_wrong_schema_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let meta = json!({"schema": "dvandva.artifact.bogus.v1", "artifact_type": "pr_review"});
    write_html(
        &dir.path().join("report.html"),
        "PR Review",
        PR_REVIEW_GOOD_BODY,
        &meta,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn pr_review_reserved_schema_with_wrong_artifact_type_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let meta = json!({"schema": "dvandva.artifact.pr_review.v1", "artifact_type": "test"});
    write_html(
        &dir.path().join("report.html"),
        "PR Review",
        PR_REVIEW_GOOD_BODY,
        &meta,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn pr_review_reserved_schema_missing_artifact_type_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let meta = json!({"schema": "dvandva.artifact.pr_review.v1"});
    write_html(
        &dir.path().join("report.html"),
        "PR Review",
        PR_REVIEW_GOOD_BODY,
        &meta,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn pr_review_missing_severity_table_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"
  <section id="verdict"><h2>Verdict</h2></section>
  <section id="severity"><h2>Severity</h2></section>
  <section id="findings"><h2>Findings</h2></section>
  <section id="ground-truth"><h2>Ground Truth</h2></section>"#;
    write_pr_review(&dir.path().join("report.html"), body);
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

// --- bug_rca cases ---

#[test]
fn valid_bug_rca_artifact_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    write_bug_rca(&dir.path().join("report.html"), BUG_RCA_GOOD_BODY);
    assert_exit(&[dir.path().to_str().unwrap()], 0);
}

#[test]
fn bug_rca_missing_root_cause_section_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"
  <section id="symptom"><h2>Symptom</h2></section>
  <section id="hypotheses"><h2>Hypotheses</h2></section>
  <section id="fix-direction"><h2>Fix Direction</h2></section>"#;
    write_bug_rca(&dir.path().join("report.html"), body);
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn bug_rca_reserved_schema_with_wrong_artifact_type_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let meta = json!({"schema": "dvandva.artifact.bug_rca.v1", "artifact_type": "test"});
    write_html(
        &dir.path().join("report.html"),
        "Bug RCA",
        BUG_RCA_GOOD_BODY,
        &meta,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn bug_rca_reserved_schema_missing_artifact_type_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let meta = json!({"schema": "dvandva.artifact.bug_rca.v1"});
    write_html(
        &dir.path().join("report.html"),
        "Bug RCA",
        BUG_RCA_GOOD_BODY,
        &meta,
    );
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

#[test]
fn bug_rca_missing_causal_chain_svg_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"
  <section id="symptom"><h2>Symptom</h2></section>
  <section id="hypotheses"><h2>Hypotheses</h2></section>
  <section id="root-cause"><h2>Root Cause</h2></section>
  <section id="fix-direction"><h2>Fix Direction</h2></section>"#;
    write_bug_rca(&dir.path().join("report.html"), body);
    assert_exit(&[dir.path().to_str().unwrap()], 1);
}

// --- default-target divergence: cwd-derived repo root (git toplevel) ---

fn init_git_repo(dir: &Path) {
    let status = Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(dir)
        .status()
        .expect("failed to run git init");
    assert!(status.success(), "git init failed in {}", dir.display());
}

#[test]
fn default_target_missing_superpowers_dir_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    assert_exit_in_dir(dir.path(), &[], 0);
}

#[test]
fn default_target_lints_superpowers_dir_in_repo_root() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    write_artifact(
        &dir.path().join("superpowers/report.html"),
        "<p>Good default-target artifact.</p>",
    );
    assert_exit_in_dir(dir.path(), &[], 0);
}

#[test]
fn default_target_rejects_bad_artifact_in_superpowers_dir() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    std::fs::create_dir_all(dir.path().join("superpowers")).unwrap();
    std::fs::write(
        dir.path().join("superpowers/report.html"),
        "<!doctype html>\n<html lang=\"en\"><head><title>t</title></head><body>no meta</body></html>\n",
    )
    .unwrap();
    assert_exit_in_dir(dir.path(), &[], 1);
}
