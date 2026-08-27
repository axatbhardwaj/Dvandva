# Task 6 report: v0.2 facades and fail-closed installer

## What changed

- Bumped the private `v4` kernel package and lockfile to `0.2.0`; `publish = false`
  remains set.
- Paired byte-identical vadi/prativadi facades on schema v2 and role API 2.
  Every role operation performs the exact version/probe handshake first and
  passes `--api 2`. Exact joins omit every unsupplied scope coordinate. The
  facade now exposes claim, reclaim, and explicit v1 upgrade operations.
- Made setup validate checksum, exact binary version, probe schema/API/read
  schemas/v1 migration capability, and private release metadata before an
  atomic `current` symlink replacement. Candidate validation also runs for an
  existing version directory. Setup no longer creates, scans, chmods, or
  migrates run directories.
- Replaced the v1 publication canary with complete v2 explainer deployment and
  review bindings through the installed `.agents` and `.claude` facade copies.
  Both semantic castings use one Site ID per run; Codex records every deployment
  and Claude approves each exact deployment.

## RED evidence

1. Kernel version test:

   ```text
   cargo test --manifest-path v4/Cargo.toml --test skill_flow version_and_probe_report_the_installation_contract
   Unexpected stdout: dvandva-v4 0.1.1; expected dvandva-v4 0.2.0
   test result: FAILED. 0 passed; 1 failed
   ```

2. Facade handshake/API test:

   ```text
   bash tests/skills/role-skills.sh
   error: required argument --expected-role-api was not provided
   dvandva-role: incompatible kernel
   ```

3. Installer atomic validation test, using a real `skills-v0.1.1` build:

   ```text
   bash tests/skills/setup-dvandva.sh
   expected command to fail: ... wrong-version ... update --version 0.2.0
   ```

   The old installer accepted a checksummed 0.1.1 binary under a requested
   0.2.0 version before the production change.

4. Two-role canary:

   ```text
   bash tests/skills/two-role-canary.sh
   setup-dvandva: version_mismatch expected=0.1.1 reported=dvandva-v4 0.2.0
   ```

## GREEN evidence

- `bash tests/skills/role-skills.sh` — one shell suite passed, including the two
  truthful cross-version pairs and all eight role commands.
- `bash tests/skills/setup-dvandva.sh` — one shell suite passed against distinct
  tag-built 0.1.1 and HEAD-built 0.2.0 binaries.
- `bash tests/skills/two-role-canary.sh` — one shell suite passed with two full
  role castings and ten exact deployment/review receipt pairs total.
- `cargo test --manifest-path v4/Cargo.toml --test skill_flow` — 5 passed.
- `cargo test --manifest-path v4/Cargo.toml --all-targets` — 156 passed, 0
  failed (the 5 `skill_flow` tests are included in this count).
- `cargo fmt --manifest-path v4/Cargo.toml -- --check` — passed.
- `cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings` —
  passed.
- `bash -n` over all six changed shell files — passed.
- `cmp` over the two source facade files — passed.
- `git diff --check` — passed.

`shellcheck` is unavailable on this host (`command -v shellcheck` returned no
path), so the required `bash -n` fallback was used.

## Self-review

- Truthful compatibility fixtures are separate builds: the old kernel and
  facade are extracted from `skills-v0.1.1`; the new kernel is built from HEAD.
  The wrong-probe fixture is an explicit controlled stub, not a relabelled
  compiled binary.
- Handshake failures are observed before the run root exists. Installer failure
  snapshots include path, mode, size, and mtime and preserve the 0.1.1 symlink.
- Exact joins pass only `--run-id` in the regression path, returning the
  canonical objective without manufacturing one.
- The migration path observes `upgrade_required`, performs explicit upgrade,
  claims again, and reads the preserved objective from the v2 run.
- No peer harness, archived v3 source, external release, push, PR, Task 7 file,
  or SDD ledger was touched.

## Concerns

No blocking concern. `publish = false` is authoritative Cargo release metadata;
the installer records and reports it alongside the validated checksummed kernel.
The binary probe itself has no publishability field, so runtime validation is
limited to package name, exact version, schema/API, readable schemas, and
migration capability.
