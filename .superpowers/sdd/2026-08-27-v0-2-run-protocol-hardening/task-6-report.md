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

No blocking concern. The checksummed candidate now authenticates `publish:
false` through its typed probe contract as well as retaining `publish = false`
in Cargo metadata.

## Review fix round 1

### Additional RED evidence

1. The focused kernel probe test failed before the metadata change:

   ```text
   assertion failed: left Null, right false
   test result: FAILED. 0 passed; 1 failed
   ```

2. Both facade and installer accepted an exit-zero probe with wrong top-level
   types when all expected strings appeared in a nested decoy:

   ```text
   expected command to fail: ... dvandva-role.sh probe
   expected command to fail: ... decoy-probe ... update --version 0.2.0
   ```

3. The installer accepted a checksummed exit-zero candidate reporting
   `publish: true`:

   ```text
   expected command to fail: ... wrong-publish ... update --version 0.2.0
   ```

4. A pre-existing unowned `0.2.0` directory was accepted:

   ```text
   expected command to fail: ... update --version 0.2.0
   ```

5. A deterministic update against pre-fix commit `01f5636` made the data root
   non-writable at manifest creation. The command failed but had already split
   installation state:

   ```text
   status=1 current=0.2.0 promoted=true manifest_preserved=true
   ```

6. Release packaging exposed its stale test fixture after the kernel bump:

   ```text
   package-skills-release: version_mismatch tag=skills-v0.1.1 source=0.2.0
   ```

### Fixes and GREEN evidence

- Probe JSON is decoded with Python using duplicate-key rejection. Facades and
  setup require the exact top-level key set, exact types and values, canonical
  `[v2, v1]` read-schema order, the exact migration capability object,
  `compatible: true`, and `publish: false`. Both explicitly require Python 3.
- Installation manifests are opened without following the final symlink,
  duplicate-safe decoded, and accepted only as an exact legacy-v1 or current-v2
  owned shape. Version directories, owner markers, binaries, data roots, bin
  roots, and lock files reject symlinks. Owner marker bytes must exactly equal
  `dvandva-skill-v1\n`.
- Every operation uses a persistent sibling Linux `flock`. Manifest/current
  replacements are prepared under the lock, candidate promotion uses `mv -T`,
  and manifest then current commit as a rollback-capable pair. Verified rollback
  removes only a version promoted by that invocation. Uncertain rollback keeps
  the candidate and transaction evidence instead of deleting a referenced
  binary.
- Tests cover pre-promotion permission failure, deterministic post-manifest
  current-commit failure, byte-identical manifest restoration, fresh-candidate
  cleanup, pre-existing candidate retention, managed-parent symlinks, owner
  marker symlink/wrong bytes, malformed/foreign manifest decoys, and a bounded
  externally-held lock.
- Hosted verify checkout now uses `fetch-depth: 0`, and the setup suite requires
  `refs/tags/skills-v0.1.1` before building the truthful old fixture.
- Both terminal role waits now assert checkpoint-B identity, manifest digest,
  and scope revision and compare the complete checkpoint snapshots.
- Fresh final verification: four shell suites passed (`role-skills`,
  `setup-dvandva`, `two-role-canary`, `package-release`); focused `skill_flow`
  passed 5/5; all-targets passed 156/156; fmt and clippy with denied warnings
  passed. Shell syntax, facade byte equality, and diff checks also passed.
