//! `lint run4-standalone-agents` — standalone-agent retirement contract.
//!
//! RE-KEYED: manifest versions bump to `1.4.0` (S2/S4/S5/S6 hardening; was `1.3.0` flow patches, `1.2.0` port); the retire helper
//! (`scripts/retire-standalone-agents.sh`) becomes `dvandva retire-agents`
//! (`rust/dvandva/src/retire.rs`); its test suite, the smoke script, and the
//! install test scripts become the Rust ports (`retire.rs` `#[cfg(test)]`,
//! `smoke.rs`, `installers.rs`). The 15-agent roster assertions are unchanged.

use std::path::Path;

use regex::Regex;
use serde_json::Value;

use crate::lint::{
    file_contains, file_exists, file_matches_ci, file_slurp_matches_ci, list_md, read,
    resolve_root, union_slurp_matches_ci, Report,
};
use crate::versions::PLUGIN_VERSION;

const EXPECTED_VERSION: &str = PLUGIN_VERSION;

const EXPECTED_AGENTS: [&str; 15] = [
    "adversarial-analyst",
    "architect",
    "baton-auditor",
    "cross-reviewer",
    "debugger",
    "deep-reviewer",
    "deslopper",
    "doc-verifier",
    "implementer",
    "integration-checker",
    "pattern-mapper",
    "researcher",
    "sandbox-verifier",
    "security-auditor",
    "test-creator",
];

fn marketplace_version(root: &Path) -> Option<String> {
    let content = read(root, ".claude-plugin/marketplace.json")?;
    let value: Value = serde_json::from_str(&content).ok()?;
    value
        .get("plugins")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("dvandva"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn plugin_version(root: &Path, rel: &str) -> Option<String> {
    let content = read(root, rel)?;
    let value: Value = serde_json::from_str(&content).ok()?;
    value
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Build the run4 standalone-agent retirement findings for a repo root.
pub fn report(root: &Path) -> Report {
    let mut r = Report::new();

    let required = [
        "README.md",
        "product.md",
        "docs/protocol/local-baton-channel.md",
        "plugins/dvandva/references/state-transition-table.md",
        "plugins/dvandva/references/baton-schema-v2.json",
        // RE-KEYED: shell scripts -> Rust command ports.
        "rust/dvandva/src/retire.rs",
        "rust/dvandva/src/smoke.rs",
        "rust/dvandva/src/installers.rs",
        ".claude-plugin/marketplace.json",
        "plugins/dvandva/.claude-plugin/plugin.json",
        "plugins/dvandva/.codex-plugin/plugin.json",
    ];
    for rel in required {
        let exists = file_exists(root, rel);
        let msg = if exists {
            format!("{rel} exists")
        } else {
            format!("{rel} is missing")
        };
        r.add(exists, msg);
    }

    // RE-KEYED: `test-retire-standalone-agents.sh` -> retire port test coverage.
    r.add(
        file_contains(root, "rust/dvandva/src/retire.rs", "#[cfg(test)]"),
        "retire-agents port carries Rust test coverage",
    );
    // RE-KEYED: `test-install-codex.sh` -> the install-codex command port.
    r.add(
        file_contains(root, "rust/dvandva/src/installers.rs", "install-codex"),
        "install-codex command port present",
    );

    // RE-KEYED: the shell's `require_match` slurped the file (`tr '\n' ' '`)
    // before regex-matching, so multi-token patterns could span line wraps in
    // prose; match per-file slurp here to restore that fidelity rather than
    // false-failing on wrapped Markdown/comment lines.
    r.add(
        file_slurp_matches_ci(
            root,
            "README.md",
            "Dvandva-only.*retire|retire.*Dvandva-only",
        ),
        "README.md must document Dvandva-only retirement",
    );
    r.add(
        file_matches_ci(root, "README.md", "Dvandva-covered workflows"),
        "README.md must limit retirement to Dvandva-covered workflows",
    );
    r.add(
        !(file_contains(root, "README.md", "v0.2.0 ships")
            || file_contains(root, "README.md", "Run 3 (in progress)")),
        "README.md must not contain stale Run 3 or v0.2.0 wording",
    );
    r.add(
        union_slurp_matches_ci(
            root,
            &["README.md", "product.md"],
            "Codex agent-axis.*no-op|no-op.*Codex agent-axis",
        ),
        "Run4 docs must document Codex agent-axis no-op",
    );
    r.add(
        union_slurp_matches_ci(
            root,
            &["README.md", "product.md", "rust/dvandva/src/retire.rs"],
            "(functional parity|equivalent-or-better).*Runs 1-4|Runs 1-4.*(functional parity|equivalent-or-better)",
        ),
        "Run4 docs/scripts must cite functional parity via Runs 1-4 usage",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "rust/dvandva/src/retire.rs",
            "backup.*manifest.*restore|manifest.*restore|restore.*manifest",
        ),
        "Run4 retirement surface must document backup manifest and restore",
    );
    r.add(
        file_slurp_matches_ci(
            root,
            "rust/dvandva/src/retire.rs",
            "skills.*never|never.*skills|no skill touches|skills out of scope",
        ),
        "Run4 retirement helper must document no skill touches",
    );

    for agent in [
        "adversarial-analyst",
        "architect",
        "developer",
        "quality-reviewer",
        "sandbox-executor",
    ] {
        r.add(
            file_matches_ci(root, "README.md", agent),
            format!("README.md must name Claude symlink allowlist member {agent}"),
        );
    }

    let versions_ok = marketplace_version(root).as_deref() == Some(EXPECTED_VERSION)
        && plugin_version(root, "plugins/dvandva/.claude-plugin/plugin.json").as_deref()
            == Some(EXPECTED_VERSION)
        && plugin_version(root, "plugins/dvandva/.codex-plugin/plugin.json").as_deref()
            == Some(EXPECTED_VERSION);
    r.add(
        versions_ok,
        format!("Dvandva manifest versions must all equal {EXPECTED_VERSION}"),
    );

    let mut actual: Vec<String> = list_md(root, "plugins/dvandva/agents")
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect();
    actual.sort();
    let mut expected: Vec<String> = EXPECTED_AGENTS.iter().map(|a| format!("{a}.md")).collect();
    expected.sort();
    r.add(
        actual == expected,
        "plugins/dvandva/agents must contain exactly the 15 canonical agents",
    );

    let mut frontmatter_ok = !actual.is_empty();
    for agent in EXPECTED_AGENTS {
        let rel = format!("plugins/dvandva/agents/{agent}.md");
        let re = Regex::new(&format!(
            r"(?m)^name:[[:space:]]*dvandva-{agent}[[:space:]]*$"
        ))
        .unwrap();
        let matched = read(root, &rel).map(|c| re.is_match(&c)).unwrap_or(false);
        if !matched {
            frontmatter_ok = false;
        }
    }
    r.add(frontmatter_ok, "agent frontmatter names must use dvandva-*");

    r
}

/// CLI entry: resolve root, run findings, print, return exit code.
pub fn run(args: &[String]) -> i32 {
    let root = resolve_root(args);
    let r = report(&root);
    r.print();
    r.exit_code()
}
