//! Offline-testable coverage for `dvandva::smoke`, ported from the
//! self-contained pieces of `scripts/smoke-plugin-install.sh` (321 ln): the
//! version-parity pin, the exact 15-agent roster (including the
//! same-version stale-cache rejection fixture at shell lines 240-248), and
//! marketplace/seed JSON validation. These do not require the `claude` or
//! `codex` CLIs.
//!
//! The full engine-driven flow (temp marketplace, `claude plugin validate`,
//! the Codex plugin lifecycle, the wait/write/lint round-trip) is exercised
//! by exactly one `#[ignore]`d end-to-end test that runs `dvandva
//! smoke-install` against this repo, matching the shell script's role as a
//! self-verifying probe.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};

use dvandva::smoke::{
    assert_source_manifest_version_parity, collect_agent_ids, dvandva_plugin_version, has_turn_cap,
    recursive_strings, require_codex_skill_surface, require_commands_reference_wait_subcommand,
    require_exact_agent_roster, require_no_bundled_scripts_dir, roster_matches_expected,
    EXPECTED_AGENT_IDS, EXPECTED_DVANDVA_VERSION,
};
use dvandva::versions::PLUGIN_VERSION;

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

fn write_text(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// Build a fixture repo root with the three version-bearing manifests
/// (`.claude-plugin/marketplace.json`,
/// `plugins/dvandva/.claude-plugin/plugin.json`,
/// `plugins/dvandva/.codex-plugin/plugin.json`) all set to `version`.
fn fixture_repo_with_version(root: &Path, version: &str) {
    write_json(
        &root.join(".claude-plugin/marketplace.json"),
        &json!({
            "plugins": [
                {"name": "dvandva", "version": version}
            ]
        }),
    );
    write_json(
        &root.join("plugins/dvandva/.claude-plugin/plugin.json"),
        &json!({"name": "dvandva", "version": version}),
    );
    write_json(
        &root.join("plugins/dvandva/.codex-plugin/plugin.json"),
        &json!({"name": "dvandva", "version": version}),
    );
}

fn fixture_agents_dir(dir: &Path, agent_stems: &[&str]) {
    fs::create_dir_all(dir).unwrap();
    for stem in agent_stems {
        fs::write(dir.join(format!("{stem}.md")), b"").unwrap();
    }
}

// ---------------------------------------------------------------------
// Version parity, checked against fixture manifests.
// ---------------------------------------------------------------------

#[test]
fn version_parity_passes_when_all_three_match_expected_version() {
    let dir = tempfile::tempdir().unwrap();
    fixture_repo_with_version(dir.path(), EXPECTED_DVANDVA_VERSION);

    assert!(assert_source_manifest_version_parity(dir.path()).is_ok());
}

// `EXPECTED_DVANDVA_VERSION` is `pub const EXPECTED_DVANDVA_VERSION: &str =
// PLUGIN_VERSION;` (src/smoke.rs), but the test above builds its fixture
// from the same symbol it then checks against, which is tautological and
// would not notice `EXPECTED_DVANDVA_VERSION` silently re-pinning to a
// frozen literal. This test builds the fixture from the independent
// `versions::PLUGIN_VERSION` symbol instead, so a future partial refactor
// that de-links `EXPECTED_DVANDVA_VERSION` from `PLUGIN_VERSION` fails here.
#[test]
fn version_parity_passes_when_fixture_built_from_plugin_version_constant() {
    let dir = tempfile::tempdir().unwrap();
    fixture_repo_with_version(dir.path(), PLUGIN_VERSION);

    assert!(assert_source_manifest_version_parity(dir.path()).is_ok());
}

// The archived repository no longer distributes either marketplace catalog,
// but preserves both historical plugin source manifests. Pin that live-tree
// shape separately from the strict active-distribution fixture coverage above.
#[test]
fn archived_live_tree_delists_marketplaces_and_preserves_plugin_versions() {
    let root = dvandva::lint::resolve_root(&[]);

    for rel in [
        ".claude-plugin/marketplace.json",
        ".agents/plugins/marketplace.json",
    ] {
        assert!(!root.join(rel).exists(), "{rel} must remain delisted");
    }

    for rel in [
        "plugins/dvandva/.claude-plugin/plugin.json",
        "plugins/dvandva/.codex-plugin/plugin.json",
    ] {
        let path = root.join(rel);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        let manifest: Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("invalid JSON in {}: {error}", path.display()));
        assert_eq!(
            manifest.get("version").and_then(Value::as_str),
            Some(PLUGIN_VERSION),
            "{rel} must preserve plugin version {PLUGIN_VERSION}"
        );
    }
}

#[test]
fn version_parity_fails_on_marketplace_claude_plugin_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    fixture_repo_with_version(dir.path(), EXPECTED_DVANDVA_VERSION);
    write_json(
        &dir.path()
            .join("plugins/dvandva/.claude-plugin/plugin.json"),
        &json!({"name": "dvandva", "version": "1.1.0"}),
    );

    let error = assert_source_manifest_version_parity(dir.path()).unwrap_err();
    assert!(error.to_string().contains("claude-plugin"), "{error}");
}

#[test]
fn version_parity_fails_on_marketplace_codex_plugin_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    fixture_repo_with_version(dir.path(), EXPECTED_DVANDVA_VERSION);
    write_json(
        &dir.path().join("plugins/dvandva/.codex-plugin/plugin.json"),
        &json!({"name": "dvandva", "version": "1.1.0"}),
    );

    let error = assert_source_manifest_version_parity(dir.path()).unwrap_err();
    assert!(error.to_string().contains("codex-plugin"), "{error}");
}

#[test]
fn version_parity_fails_when_all_three_agree_but_not_on_the_expected_version() {
    let dir = tempfile::tempdir().unwrap();
    fixture_repo_with_version(dir.path(), "1.1.0");

    let error = assert_source_manifest_version_parity(dir.path()).unwrap_err();
    assert!(
        error.to_string().contains(EXPECTED_DVANDVA_VERSION),
        "{error}"
    );
}

#[test]
fn version_parity_fails_when_marketplace_has_no_dvandva_entry() {
    let dir = tempfile::tempdir().unwrap();
    write_json(
        &dir.path().join(".claude-plugin/marketplace.json"),
        &json!({"plugins": [{"name": "other-plugin", "version": EXPECTED_DVANDVA_VERSION}]}),
    );
    write_json(
        &dir.path()
            .join("plugins/dvandva/.claude-plugin/plugin.json"),
        &json!({"name": "dvandva", "version": EXPECTED_DVANDVA_VERSION}),
    );
    write_json(
        &dir.path().join("plugins/dvandva/.codex-plugin/plugin.json"),
        &json!({"name": "dvandva", "version": EXPECTED_DVANDVA_VERSION}),
    );

    let error = assert_source_manifest_version_parity(dir.path()).unwrap_err();
    assert!(
        error.to_string().contains("missing marketplace version"),
        "{error}"
    );
}

// ---------------------------------------------------------------------
// Roster validation, including the same-version stale-cache rejection
// scenario (shell lines 240-248: remove one agent, add a bogus one, and the
// exact roster check must still reject it).
// ---------------------------------------------------------------------

#[test]
fn collect_agent_ids_sorts_and_prefixes_top_level_md_files_only() {
    let dir = tempfile::tempdir().unwrap();
    fixture_agents_dir(dir.path(), &["zeta", "alpha", "mid"]);
    // A non-.md file and a nested directory must be ignored (maxdepth 1,
    // name '*.md').
    fs::write(dir.path().join("README"), b"").unwrap();
    fs::create_dir_all(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("nested/deep.md"), b"").unwrap();

    let ids = collect_agent_ids(dir.path()).unwrap();
    assert_eq!(
        ids,
        vec![
            "dvandva-alpha".to_string(),
            "dvandva-mid".to_string(),
            "dvandva-zeta".to_string(),
        ]
    );
}

#[test]
fn roster_matches_expected_true_for_exact_fifteen_agent_set() {
    let dir = tempfile::tempdir().unwrap();
    let stems: Vec<&str> = EXPECTED_AGENT_IDS
        .iter()
        .map(|id| id.strip_prefix("dvandva-").unwrap())
        .collect();
    fixture_agents_dir(dir.path(), &stems);

    assert!(roster_matches_expected(dir.path()));
    assert!(require_exact_agent_roster(dir.path(), "fixture").is_ok());
}

#[test]
fn roster_matches_expected_false_for_same_version_stale_cache_fixture() {
    // Mirrors the shell script's stale-cache scenario: start from the exact
    // roster, drop one agent, and add an unrelated one.
    let dir = tempfile::tempdir().unwrap();
    let mut stems: Vec<&str> = EXPECTED_AGENT_IDS
        .iter()
        .map(|id| id.strip_prefix("dvandva-").unwrap())
        .collect();
    stems.retain(|stem| *stem != "deslopper");
    fixture_agents_dir(dir.path(), &stems);
    fs::write(dir.path().join("not-a-dvandva-agent.md"), b"").unwrap();

    assert!(!roster_matches_expected(dir.path()));
}

#[test]
fn require_exact_agent_roster_error_message_includes_label_and_both_rosters() {
    let dir = tempfile::tempdir().unwrap();
    fixture_agents_dir(dir.path(), &["only-one"]);

    let error = require_exact_agent_roster(dir.path(), "source").unwrap_err();
    let message = error.to_string();
    assert!(message.contains("source"), "{message}");
    assert!(message.contains("Expected agent roster"), "{message}");
    assert!(message.contains("Actual agent roster"), "{message}");
    assert_eq!(error.exit_code(), 1);
}

// ---------------------------------------------------------------------
// Marketplace / seed JSON validation.
// ---------------------------------------------------------------------

#[test]
fn dvandva_plugin_version_reads_the_matching_entry() {
    let marketplace = json!({
        "plugins": [
            {"name": "other-plugin", "version": "9.9.9"},
            {"name": "dvandva", "version": "1.2.0"}
        ]
    });
    assert_eq!(dvandva_plugin_version(&marketplace), Some("1.2.0"));
}

#[test]
fn dvandva_plugin_version_none_when_plugins_key_missing() {
    let marketplace = json!({"name": "dvandva-marketplace"});
    assert_eq!(dvandva_plugin_version(&marketplace), None);
}

#[test]
fn dvandva_plugin_version_none_when_dvandva_entry_absent() {
    let marketplace = json!({"plugins": [{"name": "other-plugin", "version": "1.0.0"}]});
    assert_eq!(dvandva_plugin_version(&marketplace), None);
}

#[test]
fn has_turn_cap_matches_expected_seed_value() {
    assert!(has_turn_cap(&json!({"turn_cap": 60}), 60));
    assert!(!has_turn_cap(&json!({"turn_cap": 61}), 60));
    assert!(!has_turn_cap(&json!({}), 60));
}

// ---------------------------------------------------------------------
// Codex skill-surface extraction (offline: parsed JSON fixtures rather than
// a real `codex debug prompt-input` process).
// ---------------------------------------------------------------------

#[test]
fn recursive_strings_collects_every_nested_string_value() {
    let value = json!({
        "a": "top",
        "b": {"c": "nested", "d": 1, "e": null},
        "f": ["array-item", {"g": "deep"}]
    });
    let mut out = Vec::new();
    recursive_strings(&value, &mut out);
    out.sort();
    assert_eq!(
        out,
        vec![
            "array-item".to_string(),
            "deep".to_string(),
            "nested".to_string(),
            "top".to_string(),
        ]
    );
}

#[test]
fn require_codex_skill_surface_passes_when_all_six_tokens_present() {
    let value = json!([
        "some prose mentioning dvandva:vadi",
        "dvandva:prativadi",
        {"nested": "dvandva:research and dvandva:testing"},
        "dvandva:understanding",
        "dvandva:worktree-setup"
    ]);
    assert!(require_codex_skill_surface(&value, "fixture").is_ok());
}

#[test]
fn require_codex_skill_surface_fails_when_a_token_is_missing() {
    let value = json!(["dvandva:vadi", "dvandva:prativadi"]);
    let error = require_codex_skill_surface(&value, "fixture").unwrap_err();
    assert!(error.to_string().contains("dvandva:research"), "{error}");
}

// ---------------------------------------------------------------------
// Bundled-scripts-dir absence + wait-subcommand-reference checks (the
// re-keyed replacement for the shell script's "verifies both wait helpers
// exist / standalone development copies" assertions).
// ---------------------------------------------------------------------

#[test]
fn require_no_bundled_scripts_dir_passes_for_a_scriptless_plugin_tree() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("skills/vadi")).unwrap();
    fs::write(dir.path().join("skills/vadi/SKILL.md"), b"").unwrap();

    assert!(require_no_bundled_scripts_dir(dir.path()).is_ok());
}

#[test]
fn require_no_bundled_scripts_dir_fails_when_a_scripts_dir_exists_anywhere() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("skills/vadi/scripts")).unwrap();
    fs::write(dir.path().join("skills/vadi/scripts/dvandva-wait.sh"), b"").unwrap();

    let error = require_no_bundled_scripts_dir(dir.path()).unwrap_err();
    assert!(error.to_string().contains("scripts/ dir"), "{error}");
}

#[test]
fn require_commands_reference_wait_subcommand_passes_when_both_commands_name_it() {
    let dir = tempfile::tempdir().unwrap();
    write_text(
        &dir.path().join("vadi.md"),
        "wait with: dvandva wait --role vadi --file \"$BATON_FILE\"",
    );
    write_text(
        &dir.path().join("prativadi.md"),
        "wait with: dvandva wait --role prativadi --file \"$BATON_FILE\"",
    );

    assert!(require_commands_reference_wait_subcommand(dir.path()).is_ok());
}

#[test]
fn require_commands_reference_wait_subcommand_fails_when_a_command_still_names_the_shell_helper() {
    let dir = tempfile::tempdir().unwrap();
    write_text(
        &dir.path().join("vadi.md"),
        "wait with: ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh --role vadi",
    );
    write_text(
        &dir.path().join("prativadi.md"),
        "wait with: dvandva wait --role prativadi --file \"$BATON_FILE\"",
    );

    let error = require_commands_reference_wait_subcommand(dir.path()).unwrap_err();
    assert!(error.to_string().contains("vadi.md"), "{error}");
}

// ---------------------------------------------------------------------
// The one engine-driven end-to-end test.
// ---------------------------------------------------------------------

#[test]
#[ignore = "requires claude+codex engines"]
fn smoke_install_end_to_end() {
    let out = Command::new(env!("CARGO_BIN_EXE_dvandva"))
        .arg("smoke-install")
        .output()
        .expect("failed to run dvandva smoke-install");
    assert!(
        out.status.success(),
        "dvandva smoke-install failed (exit {:?}): {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}
