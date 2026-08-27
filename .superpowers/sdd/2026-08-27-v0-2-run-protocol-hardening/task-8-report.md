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

## GREEN evidence

Final implementation range: `4f9656a..ecc0433`.

- `bash tests/skills/package-release.sh`: pass. The wrong-version, v1/API1,
  and duplicate-key candidates were rejected before final-asset promotion or
  checksumming; the real stripped asset reported the exact v2/API2 probe and
  verified against `SHA256SUMS`.
- `bash tests/skills/role-skills.sh`: pass.
- `bash tests/skills/setup-dvandva.sh`: pass.
- `bash tests/skills/two-role-canary.sh`: pass.
- `cargo test --manifest-path v4/Cargo.toml --all-targets`: 172 passed, 0
  failed.
- `cargo fmt --manifest-path v4/Cargo.toml -- --check`: pass.
- `cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings`:
  pass.
- Shell syntax passed for the packager, package test, both role facades, and
  setup installer.
- `ruamel.yaml` 0.18.16 parsed the workflow and its structural assertions
  passed. The workflow pins the same parser version for hosted package tests.
- `git diff --check`: pass.

`actionlint`, `yamllint`, `yq`, PyYAML, and `shellcheck` were unavailable on
this host; none is represented as run.

## Preservation and distribution boundaries

- README archive SHA-256 from `## Retired v3 archive` onward:
  `83182b2773ae4c52a71c2568cb856770b4269d1cdeb47bd92fb400fa4807629a`.
- Historical two-mode body SHA-256 from its original heading onward:
  `12d51f85fc0ec5e945e99122f465ee5dcad205604993eeb7cb1340f65acdf6b8`.
- `v4/Cargo.toml` remains `publish = false`; release automation contains no
  `cargo publish` and distributes only a GitHub release asset.
- No push, tag, release, asset upload, crate/plugin publication, or installation
  was performed.
