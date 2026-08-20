# Final Dvandva Retirement Design

**Status:** Approved by the user on 2026-08-20

## Objective

Retire Dvandva as an active product, remove its live integration from the
local Claude, Codex/T3, and Cargo harnesses, and remove its maintained-tree
references from Haoshoku. Preserve history and evidence so the retirement is
auditable and reversible where platform semantics permit.

## Why

The installed Dvandva 1.7.0 prompts required a Claude-hosted role to start a
separate `codex exec` process for Sol-owned research. That was policy encoded
in the plugin, not an action started by Haoshoku or the `dvandva` binary. The
user now prefers to use each engine independently and has chosen retirement
instead of another routing redesign.

## Scope

### Public Dvandva repository

- Preserve source, branches, tags, releases, Pages, and Git history.
- Reframe the current tree as a historical, unsupported artifact.
- Delete the two root marketplace catalogs so a checkout of `main` is no
  longer an installable marketplace.
- Preserve the internal Claude and Codex plugin manifests as historical
  source.
- Publish crate `3.5.1` with retired metadata so crates.io's default package
  page no longer describes an active product.
- Tag and release `v3.5.1` as the final retired release.
- Archive the GitHub repository only after all required pushes and releases.
- Keep GitHub Pages available as an archived explainer.

### Local harnesses

- Remove `dvandva@dvandva` and its marketplace from Claude with Claude's
  native plugin commands.
- Remove `dvandva@dvandva` and its marketplace from Codex with Codex's native
  plugin commands.
- Remove the installed crate with `cargo uninstall dvandva`.
- Verify native commands removed their exact cache and configuration state.
- Remove only exact residual Dvandva records or empty runtime directories
  that survive native uninstall; preserve unrelated configuration.
- Invalidate only generated T3 discovery caches that still advertise the
  removed plugin. Fresh Claude/Codex/T3 sessions must no longer discover it.

### Maintained Haoshoku repository

- Keep Haoshoku's generic native Claude/Codex and external-skill support.
- Remove all current-tree direct, split, and semantic Dvandva references,
  including historical prose kept in the maintained tree.
- Neutralize Dvandva-specific role names in the bundled HTML skill without
  changing the skill's behavior.
- Add a behavior-oriented maintained-tree retirement guard that detects the
  retired identifier without spelling it in the test source.
- Publish Haoshoku `10.0.1`; immutable prior releases and Git history remain
  historical records.

## Preservation Boundary

The cleanup must not delete or rewrite:

- either repository's Git history, old tags, or old releases;
- Claude/Codex memories, sessions, transcripts, file history, or job evidence;
- `/home/xzat/agent-temp-dir-work/dv-run-def1129` or the material
  `/home/xzat/agent-temp-dir-work/dvandva-review` evidence tree;
- entire `~/.claude`, `~/.codex`, `~/.cargo`, `~/.t3`, or
  `/home/xzat/agent-temp-dir-work` roots;
- stale historical clones such as `Haoshoku-t3-connect`;
- unrelated native plugins, system skills, personal instructions, settings,
  or statusline configuration.

## Safety and Ordering

1. Capture failing retirement guards before implementation.
2. Make and verify the final Dvandva source/archive changes.
3. Publish the final crate, push `main`, and publish `v3.5.1`.
4. Use native uninstallers, inspect exact leftovers, then clean only verified
   residual paths or records.
5. Make, verify, push, and publish the Haoshoku cleanup as `10.0.1`.
6. Recheck live plugin, binary, cache, configuration, registry, and package
   state.
7. Archive `axatbhardwaj/Dvandva` on GitHub last.

This sequence prevents GitHub's read-only archive state from blocking a final
fix and avoids hand-editing configuration that native uninstallers own.

## Acceptance Criteria

- Dvandva `main` is an archived historical tree with no root marketplace
  catalogs, while both internal plugin manifests remain intact.
- crates.io and GitHub expose `3.5.1`/`v3.5.1` as the final retired release.
- GitHub reports the repository archived and Pages remains available.
- Claude and Codex list neither the plugin nor its marketplace.
- `cargo install --list` and command resolution show no installed Dvandva
  binary.
- Fresh T3 discovery contains no Dvandva command, skill, agent, or hook entry.
- Haoshoku `stable` and npm `latest` are `10.0.1`, with no direct, split, or
  Dvandva-specific role reference in the maintained package tree.
- Full repository tests, lints, package dry-runs, and registry tarball scans
  pass for the affected projects.
