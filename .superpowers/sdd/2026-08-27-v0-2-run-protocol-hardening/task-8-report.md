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
