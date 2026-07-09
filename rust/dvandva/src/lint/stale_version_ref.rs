//! `lint stale-version-ref` — anchored Dvandva version-reference drift guard.
//!
//! This lint intentionally avoids a bare semver grep. It checks only lines with
//! Dvandva-specific version anchors, so third-party versions and historical
//! prose do not become release blockers.

use std::fs;
use std::path::Path;

use regex::Regex;
use serde_json::Value;

use crate::lint::{read, resolve_root, Report};

const CARGO_TOML: &str = "rust/dvandva/Cargo.toml";
const VERSIONS_RS: &str = "rust/dvandva/src/versions.rs";
const CLAUDE_PLUGIN: &str = "plugins/dvandva/.claude-plugin/plugin.json";
const CODEX_PLUGIN: &str = "plugins/dvandva/.codex-plugin/plugin.json";
const MARKETPLACE: &str = ".claude-plugin/marketplace.json";

/// Build the stale-version-reference findings for a repo root.
pub fn report(root: &Path) -> Report {
    let mut r = Report::new();

    let crate_version = cargo_version(root);
    r.add(
        crate_version.is_some(),
        format!("{CARGO_TOML} declares a package version"),
    );

    let plugin_versions = [
        (CLAUDE_PLUGIN, plugin_json_version(root, CLAUDE_PLUGIN)),
        (CODEX_PLUGIN, plugin_json_version(root, CODEX_PLUGIN)),
        (MARKETPLACE, marketplace_version(root)),
    ];
    let plugin_truth = plugin_versions
        .iter()
        .find_map(|(_, version)| version.as_deref())
        .map(str::to_string);
    for (rel, version) in &plugin_versions {
        r.add(
            version.is_some(),
            format!("{rel} declares a Dvandva plugin version"),
        );
    }
    if let Some(want) = plugin_truth.as_deref() {
        for (rel, version) in &plugin_versions {
            r.add(
                version.as_deref() == Some(want),
                format!("{rel} plugin version matches {want}"),
            );
        }
    }

    let const_version = versions_rs_plugin_version(root);
    r.add(
        const_version.is_some(),
        format!("{VERSIONS_RS} declares PLUGIN_VERSION"),
    );
    if let Some(want) = plugin_truth.as_deref() {
        r.add(
            const_version.as_deref() == Some(want),
            format!("{VERSIONS_RS} PLUGIN_VERSION matches manifests ({want})"),
        );
    }

    if let (Some(crate_version), Some(plugin_version)) =
        (crate_version.as_deref(), plugin_truth.as_deref())
    {
        let stale = anchored_version_findings(root, crate_version, plugin_version);
        if stale.is_empty() {
            r.add(
                true,
                format!("anchored Dvandva version references match crate {crate_version} and plugin {plugin_version}"),
            );
        } else {
            for finding in stale {
                r.add(false, finding);
            }
        }
    }

    r
}

/// CLI entry: resolve root, run findings, print, return exit code.
pub fn run(args: &[String]) -> i32 {
    let root = resolve_root(args);
    let r = report(&root);
    r.print();
    r.exit_code()
}

fn cargo_version(root: &Path) -> Option<String> {
    let text = read(root, CARGO_TOML)?;
    let re = Regex::new(r#"(?m)^version[[:space:]]*=[[:space:]]*"([^"]+)""#).ok()?;
    re.captures(&text)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn plugin_json_version(root: &Path, rel: &str) -> Option<String> {
    let text = read(root, rel)?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn marketplace_version(root: &Path) -> Option<String> {
    let text = read(root, MARKETPLACE)?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("plugins")?
        .as_array()?
        .iter()
        .find(|plugin| plugin.get("name").and_then(Value::as_str) == Some("dvandva"))
        .and_then(|plugin| plugin.get("version"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn versions_rs_plugin_version(root: &Path) -> Option<String> {
    let text = read(root, VERSIONS_RS)?;
    let re =
        Regex::new(r#"(?m)^pub const PLUGIN_VERSION: &str[[:space:]]*=[[:space:]]*"([^"]+)";"#)
            .ok()?;
    re.captures(&text)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn anchored_version_findings(
    root: &Path,
    crate_version: &str,
    plugin_version: &str,
) -> Vec<String> {
    let mut findings = Vec::new();
    for rel in scan_files(root) {
        if allowlisted(&rel) {
            continue;
        }
        let Some(text) = read(root, &rel) else {
            continue;
        };
        for (idx, line) in text.lines().enumerate() {
            let line_no = idx + 1;
            check_crate_anchor(&rel, line, line_no, crate_version, &mut findings);
            check_plugin_anchor(&rel, line, line_no, plugin_version, &mut findings);
        }
    }
    findings
}

fn check_crate_anchor(
    rel: &str,
    line: &str,
    line_no: usize,
    crate_version: &str,
    findings: &mut Vec<String>,
) {
    if let Some(found) = capture_version(
        line,
        r#"cargo install[[:space:]]+dvandva[[:space:]]+--version[[:space:]]+([0-9][0-9A-Za-z_.-]*)"#,
    ) {
        expect_version(
            rel,
            line_no,
            "cargo install dvandva",
            &found,
            crate_version,
            findings,
        );
    }

    if line.contains("published on crates.io as") || line.contains("dvandva --version") {
        if let Some(found) = capture_version(line, r#"`dvandva[[:space:]]+([0-9][0-9A-Za-z_.-]*)`"#)
        {
            expect_version(
                rel,
                line_no,
                "dvandva binary version prose",
                &found,
                crate_version,
                findings,
            );
        }
    }

    if line.trim_start().starts_with("Version `") {
        if let Some(found) = capture_version(line, r#"Version[[:space:]]+`([0-9][0-9A-Za-z_.-]*)`"#)
        {
            expect_version(
                rel,
                line_no,
                "crate README version line",
                &found,
                crate_version,
                findings,
            );
        }
    }
}

fn check_plugin_anchor(
    rel: &str,
    line: &str,
    line_no: usize,
    plugin_version: &str,
    findings: &mut Vec<String>,
) {
    if line.contains("installable plugin") {
        if let Some(found) = capture_version(line, r#"version[[:space:]]+`([0-9][0-9A-Za-z_.-]*)`"#)
        {
            expect_version(
                rel,
                line_no,
                "installable plugin prose",
                &found,
                plugin_version,
                findings,
            );
        }
    }

    if rel == "rust/dvandva/src/cmd/retire.rs" && line.contains("default:") {
        if let Some(found) = capture_version(line, r#"default:[[:space:]]*([0-9][0-9A-Za-z_.-]*)"#)
        {
            expect_version(
                rel,
                line_no,
                "retire-agents default plugin version",
                &found,
                plugin_version,
                findings,
            );
        }
    }
}

fn capture_version(line: &str, pattern: &str) -> Option<String> {
    Regex::new(pattern)
        .ok()?
        .captures(line)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

fn expect_version(
    rel: &str,
    line_no: usize,
    anchor: &str,
    found: &str,
    expected: &str,
    findings: &mut Vec<String>,
) {
    if found != expected {
        findings.push(format!(
            "{rel}:{line_no} {anchor} uses {found}, expected {expected}"
        ));
    }
}

fn scan_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    collect_files(root, root, &mut out);
    out.sort();
    out
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = rel_path(root, &path);
        if skip_dir(&rel) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_files(root, &path, out);
        } else if file_type.is_file() && scanned_extension(&path) {
            out.push(rel);
        }
    }
}

fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn scanned_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("md" | "rs" | "toml" | "json")
    )
}

fn skip_dir(rel: &str) -> bool {
    rel == ".git"
        || rel == ".dvandva"
        || rel == "target"
        || rel == "rust/target"
        || rel == ".superpowers"
        || rel.starts_with(".git/")
        || rel.starts_with(".dvandva/")
        || rel.starts_with(".superpowers/")
        || rel.starts_with("rust/target/")
}

fn allowlisted(rel: &str) -> bool {
    rel == "rust/Cargo.lock"
        || rel == "product.md"
        || rel == "CLAUDE.md"
        || rel.starts_with("rust/dvandva/tests/")
        || rel.starts_with("superpowers/")
        || rel.starts_with(".superpowers/")
}
