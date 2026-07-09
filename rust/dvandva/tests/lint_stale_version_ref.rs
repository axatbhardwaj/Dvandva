//! Fixture-driven tests for `dvandva lint stale-version-ref`
//! (src/lint/stale_version_ref.rs), landed at 3bac780 (core) + 8aacb30
//! (registration). Mirrors tests/lints.rs's tempdir fixture pattern: each
//! test builds a small fixture tree under a tempdir and drives the lint's
//! `report(root)` seam directly, asserting on findings.
//!
//! Fixture uses deliberately fictitious versions (crate 9.9.9, plugin 8.8.8,
//! kept distinct from each other) so a namespace conflation bug would be
//! caught rather than accidentally matching by coincidence.

use std::fs;
use std::path::Path;

use dvandva::lint::stale_version_ref;
use tempfile::TempDir;

const CRATE_VERSION: &str = "9.9.9";
const PLUGIN_VERSION: &str = "8.8.8";
const CRATE_VERSION_STALE: &str = "9.9.8";
const PLUGIN_VERSION_STALE: &str = "8.8.7";

fn w(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn tmp() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn edit(root: &Path, rel: &str, from: &str, to: &str) {
    let path = root.join(rel);
    let text = fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(from),
        "fixture edit target not found in {rel}: {from:?}"
    );
    fs::write(path, text.replace(from, to)).unwrap();
}

/// A complete, internally-consistent fixture tree: crate version and plugin
/// version declared once each (Cargo.toml / versions.rs / three plugin
/// manifests) and then referenced from every anchored-prose family the
/// landed lint scans.
fn base_fixture(root: &Path, crate_version: &str, plugin_version: &str) {
    w(
        root,
        "rust/dvandva/Cargo.toml",
        &format!(
            "[package]\nname = \"dvandva\"\nversion = \"{crate_version}\"\nedition = \"2021\"\n"
        ),
    );
    w(
        root,
        "rust/dvandva/src/versions.rs",
        &format!(
            "//! Shared release-version constants.\npub const PLUGIN_VERSION: &str = \"{plugin_version}\";\n"
        ),
    );
    w(
        root,
        "plugins/dvandva/.claude-plugin/plugin.json",
        &format!("{{\n  \"name\": \"dvandva\",\n  \"version\": \"{plugin_version}\"\n}}\n"),
    );
    w(
        root,
        "plugins/dvandva/.codex-plugin/plugin.json",
        &format!("{{\n  \"name\": \"dvandva\",\n  \"version\": \"{plugin_version}\"\n}}\n"),
    );
    w(
        root,
        ".claude-plugin/marketplace.json",
        &format!(
            "{{\n  \"plugins\": [\n    {{ \"name\": \"dvandva\", \"version\": \"{plugin_version}\" }}\n  ]\n}}\n"
        ),
    );

    // Root README: plugin-namespace anchor + crate-namespace anchors, side
    // by side, deliberately using two DIFFERENT version numbers.
    w(
        root,
        "README.md",
        &format!(
            "Dvandva ships as an installable plugin (version `{plugin_version}`) for both engines.\nIt is published on crates.io as `dvandva {crate_version}`.\ncargo install dvandva --version {crate_version}\n"
        ),
    );

    // Crate README: binary version prose (triggered via the `dvandva
    // --version` phrase rather than the crates.io phrase) + the dedicated
    // "Version `x.y.z`" line anchor.
    w(
        root,
        "rust/dvandva/README.md",
        &format!(
            "`dvandva --version` prints the version line (`dvandva {crate_version}`).\nVersion `{crate_version}`. Licensed under MIT.\n"
        ),
    );

    // SKILL.md install hints (both roles).
    let skill_hint = format!(
        "install it with `cargo install dvandva --version {crate_version}`, or `cargo install --path rust/dvandva` from a checkout.\n"
    );
    w(root, "plugins/dvandva/skills/vadi/SKILL.md", &skill_hint);
    w(
        root,
        "plugins/dvandva/skills/prativadi/SKILL.md",
        &skill_hint,
    );

    // retire-agents default plugin version prose (rel-pinned anchor).
    w(
        root,
        "rust/dvandva/src/cmd/retire.rs",
        &format!(
            "// Safety:\n//   (default: {plugin_version}) contains all 15 required dvandva-* agent files.\n// Environment:\n//   DVANDVA_EXPECTED_VERSION   Required dvandva cache version (default: {plugin_version}).\n"
        ),
    );
}

// ---------------------------------------------------------------------------
// 1. clean-pass
// ---------------------------------------------------------------------------

#[test]
fn clean_fixture_passes() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    let r = stale_version_ref::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

// ---------------------------------------------------------------------------
// 2. per-family stale rejection
// ---------------------------------------------------------------------------

#[test]
fn rejects_root_readme_stale_crate_install_hint() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "README.md",
        &format!("cargo install dvandva --version {CRATE_VERSION}"),
        &format!("cargo install dvandva --version {CRATE_VERSION_STALE}"),
    );
    let r = stale_version_ref::report(d.path());
    assert_eq!(r.failures(), 1, "expected exactly one stale finding");
    let msg = r.findings.iter().find(|f| !f.ok).unwrap().message.clone();
    assert!(msg.starts_with("README.md:"), "unexpected file: {msg}");
    assert!(
        msg.contains(&format!(
            "uses {CRATE_VERSION_STALE}, expected {CRATE_VERSION}"
        )),
        "unexpected message: {msg}"
    );
}

#[test]
fn rejects_root_readme_stale_installable_plugin_phrase() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "README.md",
        &format!("installable plugin (version `{PLUGIN_VERSION}`)"),
        &format!("installable plugin (version `{PLUGIN_VERSION_STALE}`)"),
    );
    let r = stale_version_ref::report(d.path());
    assert_eq!(r.failures(), 1, "expected exactly one stale finding");
    let msg = r.findings.iter().find(|f| !f.ok).unwrap().message.clone();
    assert!(msg.starts_with("README.md:"), "unexpected file: {msg}");
    assert!(
        msg.contains(&format!(
            "installable plugin prose uses {PLUGIN_VERSION_STALE}, expected {PLUGIN_VERSION}"
        )),
        "unexpected message: {msg}"
    );
}

#[test]
fn rejects_crate_readme_stale_version_line() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "rust/dvandva/README.md",
        &format!("Version `{CRATE_VERSION}`."),
        &format!("Version `{CRATE_VERSION_STALE}`."),
    );
    let r = stale_version_ref::report(d.path());
    assert_eq!(r.failures(), 1, "expected exactly one stale finding");
    let msg = r.findings.iter().find(|f| !f.ok).unwrap().message.clone();
    assert!(
        msg.starts_with("rust/dvandva/README.md:"),
        "unexpected file: {msg}"
    );
    assert!(
        msg.contains("crate README version line"),
        "unexpected anchor: {msg}"
    );
}

#[test]
fn rejects_crate_readme_stale_binary_version_prose() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "rust/dvandva/README.md",
        &format!("(`dvandva {CRATE_VERSION}`)"),
        &format!("(`dvandva {CRATE_VERSION_STALE}`)"),
    );
    let r = stale_version_ref::report(d.path());
    assert_eq!(r.failures(), 1, "expected exactly one stale finding");
    let msg = r.findings.iter().find(|f| !f.ok).unwrap().message.clone();
    assert!(
        msg.starts_with("rust/dvandva/README.md:"),
        "unexpected file: {msg}"
    );
    assert!(
        msg.contains("dvandva binary version prose"),
        "unexpected anchor: {msg}"
    );
}

#[test]
fn rejects_vadi_skill_stale_install_hint() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "plugins/dvandva/skills/vadi/SKILL.md",
        &format!("cargo install dvandva --version {CRATE_VERSION}"),
        &format!("cargo install dvandva --version {CRATE_VERSION_STALE}"),
    );
    let r = stale_version_ref::report(d.path());
    assert_eq!(r.failures(), 1, "expected exactly one stale finding");
    let msg = r.findings.iter().find(|f| !f.ok).unwrap().message.clone();
    assert!(
        msg.starts_with("plugins/dvandva/skills/vadi/SKILL.md:"),
        "unexpected file: {msg}"
    );
}

#[test]
fn rejects_prativadi_skill_stale_install_hint() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "plugins/dvandva/skills/prativadi/SKILL.md",
        &format!("cargo install dvandva --version {CRATE_VERSION}"),
        &format!("cargo install dvandva --version {CRATE_VERSION_STALE}"),
    );
    let r = stale_version_ref::report(d.path());
    assert_eq!(r.failures(), 1, "expected exactly one stale finding");
    let msg = r.findings.iter().find(|f| !f.ok).unwrap().message.clone();
    assert!(
        msg.starts_with("plugins/dvandva/skills/prativadi/SKILL.md:"),
        "unexpected file: {msg}"
    );
}

#[test]
fn rejects_retire_agents_stale_default_plugin_version() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "rust/dvandva/src/cmd/retire.rs",
        &format!("(default: {PLUGIN_VERSION}) contains all 15"),
        &format!("(default: {PLUGIN_VERSION_STALE}) contains all 15"),
    );
    let r = stale_version_ref::report(d.path());
    assert_eq!(r.failures(), 1, "expected exactly one stale finding");
    let msg = r.findings.iter().find(|f| !f.ok).unwrap().message.clone();
    assert!(
        msg.starts_with("rust/dvandva/src/cmd/retire.rs:"),
        "unexpected file: {msg}"
    );
    assert!(
        msg.contains("retire-agents default plugin version"),
        "unexpected anchor: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 3. two-namespace non-collision
// ---------------------------------------------------------------------------

#[test]
fn crate_and_plugin_versions_never_conflated_when_both_correct() {
    // The base fixture already places a crate-namespace anchor and a
    // plugin-namespace anchor on adjacent lines of the same doc (README.md),
    // using two DIFFERENT version numbers. If the lint conflated the two
    // namespaces (e.g. compared the plugin anchor against crate_version),
    // this would fail; instead it must pass.
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    let r = stale_version_ref::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn plugin_stale_flags_as_plugin_not_crate() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "README.md",
        &format!("installable plugin (version `{PLUGIN_VERSION}`)"),
        &format!("installable plugin (version `{PLUGIN_VERSION_STALE}`)"),
    );
    let r = stale_version_ref::report(d.path());
    // Exactly the plugin-anchor finding — the neighboring crate-namespace
    // anchors on the same file must remain unflagged.
    assert_eq!(r.failures(), 1, "expected exactly one stale finding");
    let msg = r.findings.iter().find(|f| !f.ok).unwrap().message.clone();
    assert!(
        msg.contains(&format!("expected {PLUGIN_VERSION}")),
        "expected plugin version in message: {msg}"
    );
    assert!(
        !msg.contains(CRATE_VERSION),
        "plugin finding must not reference the crate version: {msg}"
    );
}

#[test]
fn crate_stale_flags_as_crate_not_plugin() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "README.md",
        &format!("cargo install dvandva --version {CRATE_VERSION}"),
        &format!("cargo install dvandva --version {CRATE_VERSION_STALE}"),
    );
    let r = stale_version_ref::report(d.path());
    assert_eq!(r.failures(), 1, "expected exactly one stale finding");
    let msg = r.findings.iter().find(|f| !f.ok).unwrap().message.clone();
    assert!(
        msg.contains(&format!("expected {CRATE_VERSION}")),
        "expected crate version in message: {msg}"
    );
    assert!(
        !msg.contains(PLUGIN_VERSION),
        "crate finding must not reference the plugin version: {msg}"
    );
}

// ---------------------------------------------------------------------------
// 4. third-party immunity
// ---------------------------------------------------------------------------

#[test]
fn third_party_tool_version_never_flagged() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    // A third-party CLI's own version, unrelated to any Dvandva anchor
    // phrase, appended to an otherwise-clean scanned file.
    let p = d.path().join("README.md");
    let mut text = fs::read_to_string(&p).unwrap();
    text.push_str("Install the codex CLI with `npm install -g @openai/codex@0.45.0`.\n");
    fs::write(&p, text).unwrap();
    let r = stale_version_ref::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn allowlisted_product_md_stale_ref_ignored() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    // Historical prose in an allowlisted file, carrying a deliberately wrong
    // anchored version — must not be scanned at all.
    w(
        d.path(),
        "product.md",
        &format!("cargo install dvandva --version {CRATE_VERSION_STALE}\n"),
    );
    let r = stale_version_ref::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn allowlisted_claude_md_stale_ref_ignored() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    w(
        d.path(),
        "CLAUDE.md",
        &format!("installable plugin (version `{PLUGIN_VERSION_STALE}`) — historical note.\n"),
    );
    let r = stale_version_ref::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

#[test]
fn allowlisted_test_fixture_dir_stale_ref_ignored() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    // rust/dvandva/tests/ is allowlisted so lint fixtures (like this very
    // file's sibling tests) never trip the lint on their own scratch data.
    w(
        d.path(),
        "rust/dvandva/tests/some_fixture.rs",
        &format!(
            "// (default: {PLUGIN_VERSION_STALE}) contains all 15 required dvandva-* agent files.\n"
        ),
    );
    let r = stale_version_ref::report(d.path());
    assert!(r.passed(), "expected clean, failures: {}", r.failures());
}

// ---------------------------------------------------------------------------
// 5. fail-closed
// ---------------------------------------------------------------------------

#[test]
fn missing_cargo_toml_fails_closed() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    fs::remove_file(d.path().join("rust/dvandva/Cargo.toml")).unwrap();
    let r = stale_version_ref::report(d.path());
    assert!(
        !r.passed(),
        "missing Cargo.toml must fail the lint, not silently pass"
    );
    assert!(r.fails_with("declares a package version"));
}

#[test]
fn unparseable_cargo_toml_fails_closed() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    // No `version = "..."` line at all — the anchor regex cannot capture it.
    w(
        d.path(),
        "rust/dvandva/Cargo.toml",
        "[package]\nname = \"dvandva\"\nedition = \"2021\"\n",
    );
    let r = stale_version_ref::report(d.path());
    assert!(
        !r.passed(),
        "unparseable Cargo.toml must fail the lint, not silently pass"
    );
    assert!(r.fails_with("declares a package version"));
}

#[test]
fn plugin_manifest_mismatch_fails_closed() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "plugins/dvandva/.codex-plugin/plugin.json",
        &format!("\"version\": \"{PLUGIN_VERSION}\""),
        &format!("\"version\": \"{PLUGIN_VERSION_STALE}\""),
    );
    let r = stale_version_ref::report(d.path());
    assert_eq!(r.failures(), 1, "expected exactly one manifest mismatch");
    assert!(r.fails_with(&format!(
        "plugins/dvandva/.codex-plugin/plugin.json plugin version matches {PLUGIN_VERSION}"
    )));
}

#[test]
fn versions_rs_plugin_version_mismatch_fails_closed() {
    let d = tmp();
    base_fixture(d.path(), CRATE_VERSION, PLUGIN_VERSION);
    edit(
        d.path(),
        "rust/dvandva/src/versions.rs",
        &format!("PLUGIN_VERSION: &str = \"{PLUGIN_VERSION}\""),
        &format!("PLUGIN_VERSION: &str = \"{PLUGIN_VERSION_STALE}\""),
    );
    let r = stale_version_ref::report(d.path());
    assert_eq!(r.failures(), 1, "expected exactly one const mismatch");
    assert!(r.fails_with(&format!(
        "rust/dvandva/src/versions.rs PLUGIN_VERSION matches manifests ({PLUGIN_VERSION})"
    )));
}
