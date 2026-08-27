# Task 8 report: active docs and GitHub-only v0.2 packaging

## RED evidence

Baseline: `b3c6a211303348a124a66b0149b9aa334bd3a823`.

After extending `tests/skills/package-release.sh` and before changing the
packager, workflow, or active documentation:

```text
$ bash -n tests/skills/package-release.sh
exit 0

$ bash tests/skills/package-release.sh
exit 1
FAIL: correct-version v1/API1 kernel unexpectedly packaged
FAIL: .../incompatible.out missing: probe_mismatch
FAIL: incompatible kernel was checksummed
FAIL: .../skills-release.yml missing: probe --expected-schema dvandva.run.v2 --expected-role-api 2
FAIL: .../skill-only-run.md missing: skills-v0.2.0
FAIL: .../README.md active prefix missing: dvandva.run.v2
FAIL: .../minimal-run-baton.md unexpectedly contains: projection revision
FAIL: .../0003-run-v2-security-epoch.md missing: status: accepted
skills release packaging: 44 failure(s)
```

The fake candidate reported `dvandva-v4 0.2.0` from `--version` but advertised
the v1 schema and role API 1 from `probe`. The baseline packager accepted and
checksummed it, proving the missing compatibility gate rather than a tag or
build-fixture failure. The same run recorded the expected active-document,
ADR, and migration-aware workflow gaps.

A second RED fixture supplied duplicate root and nested capability keys with
invalid values first and valid values last. Against the unchanged packager it
also exited 1 for the expected reason:

```text
FAIL: duplicate-key probe unexpectedly packaged
FAIL: duplicate-key kernel was checksummed
```

Before staging was introduced, the packager rejected incompatible probes but
left the stripped candidate at the final asset path. A third RED run added
wrong-version coverage and required all failures to leave no final asset:

```text
FAIL: incompatible kernel was promoted to the final asset path
FAIL: duplicate-key kernel was promoted to the final asset path
FAIL: wrong-version kernel was promoted to the final asset path
skills release packaging: 3 failure(s)
```

### Review-fix wave 1 RED

Baseline: `42ac162c6db129f8901228bf90990085474db723`.

Before the fix wave changed the packager, workflow, or active instructions,
the expanded black-box package suite exited 1 and reproduced each accepted
release-boundary finding:

```text
FAIL: nul-bearing probe unexpectedly packaged
FAIL: oversized probe unexpectedly packaged
FAIL: pre-existing empty output directory unexpectedly accepted
FAIL: pre-existing empty output directory was modified
FAIL: pre-existing output symlink unexpectedly accepted
FAIL: pre-existing output symlink target was modified
FAIL: checksum failure exposed a partial output path
FAIL: promotion collision unexpectedly packaged
FAIL: promotion collision replaced the foreign path
```

Focused source checks on the same baseline showed the probe captured through
`probe_output="$(...)"`, the workflow omitted `--all-targets`, release notes
hard-coded `Kernel 0.2.0`, and the glossary omitted Approval Withdrawal and
Protocol Upgrade. The workflow structural test also exited 1 before any
workflow edit.

### Review-fix wave 2 RED

Baseline: `4c60b5bc93569c9f7280efb2540ffc1b11bf3d57`.

The test-first package run proved that command substitution made the version
gate lossy and ignored the candidate's status. Bash accepted the exact version
followed by a NUL, 20,000 trailing newlines, or a second final newline; the
`|| true` path also accepted exact output followed by exit 7:

```text
FAIL: nul-bearing-version version unexpectedly packaged
FAIL: oversized-version version unexpectedly packaged
FAIL: extra-newline-version version unexpectedly packaged
FAIL: nonzero-version version unexpectedly packaged
FAIL: scripts/package-skills-release.sh missing: version_max_bytes=
FAIL: scripts/package-skills-release.sh missing: .version
```

Invalid UTF-8 and CRLF were already rejected and remain regression fixtures.
Exact bytes with no final newline and with one final newline remain the only
accepted forms.

A read-only mutation probe showed that the earlier workflow assertions accepted
all four unsafe edits while the strengthened structural assertions rejected
them:

```text
verify ref main: old assertions accepted; hardened assertions rejected
release ref main: old assertions accepted; hardened assertions rejected
verify contents write: old assertions accepted; hardened assertions rejected
third release asset: old assertions accepted; hardened assertions rejected
```

The same RED run reported that the glossary lacked Scope Revision and Manifest
Digest and that its generic Checkpoint Binding, Protocol Upgrade, Handoff, and
Publication Gate definitions did not meet the exact domain contracts.

The post-implementation terminology review found that only acceptance of a
pending Checkpoint Supersession creates a Handoff. Changing the exact glossary
assertion first produced one focused failure against the premature wording;
the definition now says `accepted Checkpoint Supersession`.

## GREEN evidence

- Original Task 8 range: `b3c6a21..42ac162`.
- Review-fix wave 1 range: `42ac162..4c60b5b`.
- Review-fix wave 2 implementation, test, and documentation range, excluding
  this evidence-only report commit: `4c60b5b..cd2f67e`.

The immutable endpoint for the overall evidence range includes this report
commit and is recorded in the final handoff because a commit cannot contain its
own object ID.

- `bash tests/skills/package-release.sh`: pass. Raw version output is captured
  in hidden sibling staging with a 256-byte cap, strict UTF-8, preserved exit
  status, and exact zero-or-one-final-newline semantics. Wrong-version,
  NUL-bearing, oversized, invalid-UTF-8, CRLF, extra-newline, and nonzero
  candidates were rejected. V1/API1, root-duplicate, nested-duplicate,
  NUL-bearing, oversized, invalid-UTF-8, and malformed probe candidates were
  also rejected without exposing a final output path.
  The packager rejected existing empty, non-empty, and symlink destinations,
  preserved a colliding foreign path, and cleaned a failed sibling staging
  directory. The real stripped candidate produced the exact v2/API2 probe; its
  final directory contained only the binary and matching `SHA256SUMS`.
- `bash tests/skills/role-skills.sh`: pass.
- `bash tests/skills/setup-dvandva.sh`: pass.
- `bash tests/skills/two-role-canary.sh`: pass.
- `cargo test --manifest-path v4/Cargo.toml --all-targets`: 172 passed, 0
  failed.
- `cargo test --manifest-path rust/Cargo.toml --workspace`: pass for the full
  archived v3 workspace.
- `cargo test --manifest-path rust/dvandva/Cargo.toml --locked`: pass.
- `cargo run --quiet --manifest-path v4/Cargo.toml -- probe
  --expected-schema dvandva.run.v2 --expected-role-api 2`: pass with the exact
  private v2/API2 contract.
- `cargo fmt --manifest-path v4/Cargo.toml -- --check`: pass.
- `cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings`:
  pass.
- Shell syntax passed for the packager, package test, both role facades, and
  setup installer.
- `ruamel.yaml` 0.18.16 parsed the workflow and its structural assertions
  passed, including both checkout mappings, inherited read-only verification,
  exact two-asset release arguments, and four mutation sentinels. The workflow
  pins the same parser version for hosted package tests.
- `git diff --check`: pass.

`actionlint`, `yamllint`, `yq`, PyYAML, and `shellcheck` were unavailable on
this host; none is represented as run.

The workflow continues to use GitHub's `ubuntu-latest` environment, Rust's
`stable` channel, and the runner-provided `gh`. Task 8 supplies no principled
immutable versions for those platform-managed surfaces, so the review fix did
not invent pins or imply stronger reproducibility than the workflow has.

## Codex Sites capability evidence

On 2026-08-28, a read-only inspection of the active Codex tool registry using
`ALL_TOOLS.filter(({name}) => name.includes("sites_"))` advertised these local
surfaces:

```text
mcp__codex_apps__sites_create_site
mcp__codex_apps__sites_save_site_version
mcp__codex_apps__sites_deploy_private_site_version
mcp__codex_apps__sites_get_deployment_status
```

No Sites operation was called. This proves only that the current Codex session
exposes the expected create/save/private-deploy/status tool surface; it does not
prove account authorization, a successful deployment, access control, or
provider-signed evidence. The official-source research and earlier read-only
connectivity probe remain recorded in
`docs/research/2026-08-26-published-artifact-channels.md`.

## Preservation and distribution boundaries

- README archive SHA-256 from `## Retired v3 archive` onward:
  `83182b2773ae4c52a71c2568cb856770b4269d1cdeb47bd92fb400fa4807629a`.
- Historical two-mode body SHA-256 from its original heading onward:
  `12d51f85fc0ec5e945e99122f465ee5dcad205604993eeb7cb1340f65acdf6b8`.
- `v4/Cargo.toml` remains `publish = false`; release automation contains no
  `cargo publish` and targets exactly two GitHub release assets: the private
  kernel binary and `SHA256SUMS`.
- No push, tag, release, asset upload, crate/plugin publication, or installation
  was performed.
