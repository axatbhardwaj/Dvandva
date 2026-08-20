# Final Dvandva Retirement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish a coherent final Dvandva retirement, remove every active local installation, purge maintained Haoshoku references, and archive GitHub last.

**Architecture:** Treat the public source archive, local uninstall, and Haoshoku cleanup as separate boundaries joined by one ordered release transaction. Tests first distinguish an intentionally archived source tree from a broken active marketplace; native uninstallers own live state before any exact residual cleanup.

**Tech Stack:** Rust/Cargo, Bun, Git, Claude Code CLI, Codex CLI, GitHub CLI, crates.io, npm

**Spec:** `docs/superpowers/specs/2026-08-20-final-retirement-design.md`

## Global Constraints

- Final Dvandva crate version: `3.5.1`; historical plugin source version stays `1.7.0`.
- Final Haoshoku version: `10.0.1`.
- Preserve Git history, old tags/releases, Pages, sessions, memories, run artifacts, and unrelated configuration.
- Delete no broad home/config/cache root; use native uninstallers first and validate every exact residual target.
- Never print the crates.io token from `/home/xzat/dev/Dvandva/.env`.
- Archive `axatbhardwaj/Dvandva` only after every push and publication succeeds.
- Use semantic, reviewable commits and keep unrelated dirty work untouched.

---

### Task 1: Make the Dvandva source tree archive-aware

**Files:**
- Modify: `rust/dvandva/tests/smoke.rs`
- Modify: `rust/dvandva/tests/lint_stale_version_ref.rs`
- Modify: `rust/dvandva/tests/lints.rs`
- Modify: `rust/dvandva/src/lint/stale_version_ref.rs`
- Modify: `rust/dvandva/src/lint/run4_standalone_agents.rs`
- Delete: `.claude-plugin/marketplace.json`
- Delete: `.agents/plugins/marketplace.json`

**Interfaces:**
- Consumes: an archive marker in root `README.md` containing `Dvandva is retired and archived`.
- Produces: archived live-tree invariants that require both marketplace catalogs absent and both plugin source manifests pinned to `PLUGIN_VERSION`.

- [ ] **Step 1: Replace the active live-tree smoke test with a failing archive invariant**

```rust
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
        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(root.join(rel)).unwrap(),
        ).unwrap();
        assert_eq!(manifest.get("version").and_then(Value::as_str), Some(PLUGIN_VERSION));
    }
}
```

- [ ] **Step 2: Run the focused smoke test and capture RED**

Run: `cargo test --manifest-path rust/Cargo.toml --test smoke archived_live_tree_delists_marketplaces_and_preserves_plugin_versions -- --nocapture`

Expected: FAIL because both root marketplace catalogs still exist.

- [ ] **Step 3: Add fixture tests for active and archived lint modes**

Add fixture coverage proving:

```text
active tree + missing marketplace     -> fail closed
archived marker + both catalogs absent + matching internal manifests -> pass
archived marker + either catalog present -> fail
archived marker + internal manifest mismatch -> fail
```

Run the new focused tests before implementation and record their expected failures.

- [ ] **Step 4: Implement the minimal archive-aware lint behavior**

Use one helper per lint:

```rust
fn archived(root: &Path) -> bool {
    read(root, "README.md")
        .is_some_and(|text| text.contains("Dvandva is retired and archived"))
}
```

In archived mode, require both root catalogs absent and derive plugin-version consensus from the two preserved source manifests. In active mode, retain the existing three-manifest fail-closed behavior.

- [ ] **Step 5: Delete only the two root marketplace catalogs**

Use `apply_patch` to delete:

```text
.claude-plugin/marketplace.json
.agents/plugins/marketplace.json
```

- [ ] **Step 6: Run focused GREEN and commit**

Run:

```bash
cargo test --manifest-path rust/Cargo.toml --test smoke archived_live_tree_delists_marketplaces_and_preserves_plugin_versions -- --nocapture
cargo test --manifest-path rust/Cargo.toml --test lint_stale_version_ref -- --nocapture
cargo test --manifest-path rust/Cargo.toml --test lints standalone_ -- --nocapture
git diff --check
```

Commit: `test: enforce archived distribution boundary`

### Task 2: Reframe and version the final Dvandva archive

**Files:**
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`
- Modify: `README.md`
- Modify: `product.md`
- Modify: `docs/dvandva-explainer.html`
- Modify: `docs/index.html`
- Modify: `rust/Cargo.lock`
- Modify: `rust/dvandva/Cargo.toml`
- Modify: `rust/dvandva/README.md`

**Interfaces:**
- Consumes: Task 1's archive marker and delisted catalogs.
- Produces: package `dvandva 3.5.1` with retired metadata and historical-only onboarding.

- [ ] **Step 1: Restore archive framing against the current implementation**

Add a prominent retirement banner to repository instructions and product docs. Recast installation, upgrading, invocation, and development sections as historical and unsupported. Preserve the current 3.5.0-era implementation description instead of reverting to the older 3.4.2 tree.

- [ ] **Step 2: Bump only the crate version and retired metadata**

Set `rust/dvandva/Cargo.toml` to:

```toml
version = "3.5.1"
description = "Retired and archived. A historical learning experiment in governed agent loops, subagent delegation, review gates, and two-agent coordination for Claude Code and Codex."
```

Keep both internal plugin manifests and `rust/dvandva/src/versions.rs` at `1.7.0`.

- [ ] **Step 3: Update crate and root README version anchors**

Every checked crate-version anchor must use `3.5.1`. Plugin history must use `1.7.0`. Mark `3.5.1` as the final release.

- [ ] **Step 4: Refresh the lockfile**

Run: `cargo check --manifest-path rust/Cargo.toml --locked`

If the lock rejects the package-version edit, run `cargo check --manifest-path rust/Cargo.toml`, inspect that only the local package version changed in `rust/Cargo.lock`, then rerun with `--locked`.

- [ ] **Step 5: Verify the complete source archive**

Run:

```bash
cargo fmt --manifest-path rust/Cargo.toml --check
cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rust/Cargo.toml
cargo run --manifest-path rust/dvandva/Cargo.toml -- lint stale-version-ref .
cargo run --manifest-path rust/dvandva/Cargo.toml -- lint run4-standalone-agents .
cargo publish --manifest-path rust/dvandva/Cargo.toml --locked --dry-run
git diff --check
```

- [ ] **Step 6: Commit documentation and package metadata**

Commit: `docs: retire and archive Dvandva`

### Task 3: Publish the final Dvandva release

**Files:**
- Read: `/home/xzat/dev/Dvandva/.env`
- Create outside tree: a temporary release-notes file under `/home/xzat/agent-temp-dir-work/dvandva-final-retirement/`

**Interfaces:**
- Consumes: verified clean source at crate `3.5.1`.
- Produces: origin `main`, crates.io `3.5.1`, annotated tag and GitHub release `v3.5.1`.

- [ ] **Step 1: Re-pin remote state and inspect the outgoing commits**

Run:

```bash
git fetch origin
git rev-parse origin/main
git log --oneline origin/main..HEAD
git status --short --branch
```

Expected: origin remains the previously audited `81980f4...` lineage and the worktree is clean.

- [ ] **Step 2: Push the final archive source**

Run: `git push origin HEAD:main`

- [ ] **Step 3: Publish the crate without exposing the token**

Load `cargo_token` from `/home/xzat/dev/Dvandva/.env` in the publishing shell and pass it to `cargo publish --token` without echoing it.

Run: `cargo publish --manifest-path rust/dvandva/Cargo.toml --locked --token "${DVANDVA_CARGO_TOKEN:?}"`

- [ ] **Step 4: Tag and release the retirement**

```bash
git tag -a v3.5.1 -m "Dvandva 3.5.1 — retired and archived"
git push origin v3.5.1
gh release create v3.5.1 --repo axatbhardwaj/Dvandva --verify-tag \
  --title "Dvandva 3.5.1 — retired and archived" \
  --notes-file /home/xzat/agent-temp-dir-work/dvandva-final-retirement/release-notes.md
```

- [ ] **Step 5: Verify public package and release state**

Check crates.io reports `3.5.1`, `gh release view v3.5.1` resolves to the final commit, and `origin/main` equals `HEAD`.

### Task 4: Uninstall Dvandva from local harnesses

**Files:**
- Native-managed: `~/.claude/settings.json`, `~/.claude/plugins/**`
- Native-managed: `~/.codex/config.toml`, `~/.codex/plugins/**`, `~/.codex/.tmp/marketplaces/**`
- Native-managed: `~/.cargo/bin/dvandva`, `~/.cargo/.crates.toml`
- Generated: `~/.t3/caches/claudeAgent.json`, `~/.t3/caches/codex.json`
- Runtime: `~/.dvandva`

**Interfaces:**
- Consumes: installed plugin ID `dvandva@dvandva`, marketplace name `dvandva`, crate `dvandva 3.5.0`.
- Produces: fresh native harness discovery with no Dvandva integration.

- [ ] **Step 1: Re-enumerate exact state and running processes**

Confirm plugin IDs, marketplace names, binary path, exact cache paths, T3 generated cache entries, and that no separate Dvandva/Sol research process is active. Abort exact residual cleanup if any target resolves outside the audited paths.

- [ ] **Step 2: Run native uninstallers in ownership order**

```bash
claude plugin uninstall dvandva@dvandva --scope user --yes
claude plugin marketplace remove dvandva
codex plugin remove dvandva@dvandva --json
codex plugin marketplace remove dvandva
cargo uninstall dvandva
```

- [ ] **Step 3: Inspect, then remove only exact residuals**

Verify native commands removed their records. If exact Dvandva cache/data directories or the empty `~/.dvandva` remain, validate path, type, contents, and symlink status, then move each exact target to the desktop Trash. Surgically remove only surviving Dvandva TOML blocks or hook hashes; preserve every unrelated entry.

- [ ] **Step 4: Invalidate generated T3 discovery**

After inspecting both files as regular generated cache files, move only `~/.t3/caches/claudeAgent.json` and `~/.t3/caches/codex.json` to Trash so T3 rebuilds them from current native state. Do not remove the cache directory.

- [ ] **Step 5: Verify local absence**

Check Claude/Codex plugin and marketplace lists, `cargo install --list`, `command -v dvandva`, configuration searches, exact cache paths, and rebuilt/fresh T3 discovery. Record that this already-running session retains its loaded skill metadata until restart.

### Task 5: Purge maintained Haoshoku references and publish 10.0.1

**Files:**
- Modify: `/home/xzat/dev/Haoshoku/tests/ignore_boundary.test.js`
- Modify: `/home/xzat/dev/Haoshoku/tests/html_explainer_active_boundary.test.js`
- Create: `/home/xzat/dev/Haoshoku/tests/retired_orchestration_boundary.test.js`
- Modify: `/home/xzat/dev/Haoshoku/.gitignore`
- Modify: `/home/xzat/dev/Haoshoku/.npmignore`
- Modify: `/home/xzat/dev/Haoshoku/configs/claude/README.md`
- Modify: `/home/xzat/dev/Haoshoku/configs/codex/skills/html-explainer/SKILL.md`
- Modify: `/home/xzat/dev/Haoshoku/configs/codex/skills/html-explainer/template.html`
- Modify: `/home/xzat/dev/Haoshoku/docs/runbooks/prime-video-hd.html`
- Modify: `/home/xzat/dev/Haoshoku/docs/runbooks/routing-gate-fix/PLAN.md`
- Modify: `/home/xzat/dev/Haoshoku/docs/runbooks/routing-gate-fix/defect-report.html`
- Modify: `/home/xzat/dev/Haoshoku/CHANGELOG.md`
- Release-owned: `/home/xzat/dev/Haoshoku/package.json`, `/home/xzat/dev/Haoshoku/haoshoku.js`

**Interfaces:**
- Consumes: the current `stable` tree at version `10.0.0`.
- Produces: a reference-free maintained/package tree and npm/GitHub release `10.0.1`.

- [ ] **Step 1: Add a failing maintained-tree boundary test**

Build the retired product identifier and role tokens from character codes so the test source does not itself contain them. Enumerate real maintained files with `git ls-files -co --exclude-standard`, normalize single-character bracket expressions such as `[x]`, and report every matching path. The production regression caught is reintroducing retired orchestration vocabulary into a shipped/maintained file.

- [ ] **Step 2: Run targeted RED**

Run:

```bash
bun test tests/ignore_boundary.test.js tests/html_explainer_active_boundary.test.js tests/retired_orchestration_boundary.test.js
```

Expected: FAIL on the audited ignore, documentation, metadata, and HTML-skill references.

- [ ] **Step 3: Remove direct/split references and neutralize actor tokens**

Remove the legacy ignore entries and current/historical prose references. Rename HTML tokens consistently:

```text
--vadi  -> --actor-a
--prat  -> --actor-b
k-vadi  -> k-actor-a
k-prat  -> k-actor-b
```

Apply the same neutral vocabulary in the historical HTML artifact. Preserve unrelated Sanskrit terms that are not unique to Dvandva.
Add a `## Unreleased` changelog section describing the removal in neutral terms;
the release helper requires that exact heading and will rename it to `10.0.1`.

- [ ] **Step 4: Recompute independent skill digests and run GREEN**

Use `sha256sum` over the three bundled skill files and update only the affected pinned hashes. Run the targeted tests until green.

- [ ] **Step 5: Verify and commit the cleanup**

```bash
bun test
bun run lint
git diff --check
npm pack --dry-run --json --ignore-scripts
```

Scan the dry-run manifest and an actual temporary npm tarball for the retired identifier and role tokens. Commit: `chore: remove retired orchestration references`.

- [ ] **Step 6: Release and verify Haoshoku 10.0.1**

Run `bun run release --bump=patch --yes`, monitor the `Publish to NPM` workflow to completion, verify `npm view haoshoku version` returns `10.0.1`, download/pack the registry artifact into a temporary directory, and repeat the reference scan on the published tarball.

### Task 6: Archive GitHub and perform the terminal audit

**Files:** None

**Interfaces:**
- Consumes: completed Dvandva and Haoshoku publications plus local uninstall evidence.
- Produces: final read-only public archive and a verified ready-to-restart native environment.

- [ ] **Step 1: Re-run all terminal state checks**

Recheck Dvandva GitHub release/tag/main, crates.io, Haoshoku stable/release/npm, Claude/Codex marketplace/plugin lists, Cargo installation, exact caches, T3 discovery, and preserved evidence paths.

- [ ] **Step 2: Archive the repository last**

Run: `gh repo archive axatbhardwaj/Dvandva --yes`

- [ ] **Step 3: Verify archive and Pages state**

Confirm `isArchived: true`, default branch points at the final release commit, latest release is `v3.5.1`, and Pages remains built/accessible from `main:/docs`.

- [ ] **Step 4: Report restart requirement**

State explicitly that new Claude, Codex, and T3 sessions will use only their native subagent/model surfaces; this current session's preloaded Dvandva skill list disappears only after restart.
