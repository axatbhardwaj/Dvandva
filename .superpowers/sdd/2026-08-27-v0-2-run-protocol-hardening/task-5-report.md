# Task 5 report: canonical role snapshots and exact-run fail-fast

## Status

Implemented. Commits:

- `28d1843` (`feat(v4): classify role-relative next actions`)
- `9ba850c` (`fix(v4): fail fast on exact discovery`)
- `7d07e3f` (`feat(v4): return canonical role snapshots`)

## Strict TDD evidence

The initial public CLI regressions were added before production changes.

RED commands:

```bash
cargo test --manifest-path v4/Cargo.toml --test role_session snapshot
cargo test --manifest-path v4/Cargo.toml --test skill_flow explicit_run_id
cargo test --manifest-path v4/Cargo.toml --test role_session snapshot_exact_join_normalizes
cargo test --manifest-path v4/Cargo.toml --test role_session snapshot_exact_join_needs_no_objective
cargo test --manifest-path v4/Cargo.toml --test role_session snapshot_exact_join_needs_no_objective
```

Preserved RED results:

- `role_session snapshot`: 2 run, 0 passed, 2 failed. Exact join rejected the
  new objective-reference input before reaching the optional-objective case;
  near-expiry actionable wait renewed first and timed out. Log:
  `/tmp/dvandva-task5-snapshot-red.log`.
- `skill_flow explicit_run_id`: 1 run, 0 passed, 1 failed. Exact selection
  silently claimed a canonically different scope instead of returning
  `scope_mismatch`. Log: `/tmp/dvandva-task5-exact-red.log`.
- `role_session snapshot_exact_join_normalizes`: 1 run, 0 passed, 1 failed.
  Canonically equivalent whitespace in an explicit deliverable declaration was
  classified as a mismatch. Log: `/tmp/dvandva-task5-normalization-red.log`.
- `role_session snapshot_exact_join_needs_no_objective` scope matrix: 1 run,
  0 passed, 1 failed. Once the reviewer had claimed the run, an explicitly
  different exact scope returned `busy` before comparing canonical scope.
  Log: `/tmp/dvandva-task5-scope-matrix-red.log`.
- The same exact-scope matrix with `--new-run`: 1 run, 0 passed, 1 failed.
  The separate-run branch overrode the exact mismatch instead of preserving its
  fail-fast result. Log: `/tmp/dvandva-task5-exact-new-run-red.log`.

## GREEN evidence

Final commands and results:

```bash
cargo test --manifest-path v4/Cargo.toml --test discovery exact
# 2 passed, 0 failed, 19 filtered out

cargo test --manifest-path v4/Cargo.toml --test role_session snapshot -- --test-threads=1
# 5 passed, 0 failed, 19 filtered out

cargo test --manifest-path v4/Cargo.toml --test skill_flow
# 5 passed, 0 failed

cargo test --manifest-path v4/Cargo.toml --test role_session role_wait_returns_immediately
# 1 passed, 0 failed, 23 filtered out

cargo test --manifest-path v4/Cargo.toml --all-targets
# 138 integration tests passed, 0 failed; unit targets also green

cargo fmt --manifest-path v4/Cargo.toml -- --check
# exit 0

cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
# exit 0

git diff --check
# exit 0
```

Final logs:

- `/tmp/dvandva-task5-discovery-green.log`
- `/tmp/dvandva-task5-snapshot-green.log`
- `/tmp/dvandva-task5-skill-flow-green.log`
- `/tmp/dvandva-task5-wait-green.log`
- `/tmp/dvandva-task5-full-green.log`
- `/tmp/dvandva-task5-fmt-green.log`
- `/tmp/dvandva-task5-clippy-green.log`
- `/tmp/dvandva-task5-diff-check.log`

## Implementation

- `v4/src/next_action.rs` is the sole pure classifier for role state, wake
  reason, advisory domain work, legal mutations, combined `next_actions`,
  actionability, and publication-blocking reason.
- `role start`, `read`, `apply`, and `wait` now emit one flattened canonical
  `RoleSnapshot`. Started responses add disposition, canonical run path,
  private credential path, and the exact prativadi join prompt for vadi.
- Exact joins accept omitted objective/scope, compare every supplied scope
  coordinate after normalization, and return typed `scope_mismatch`,
  `run_missing`, `busy`, or `upgrade_required` without discovery waiting.
- Role start accepts repeatable `--objective-ref kind=value` and
  `--required-deliverable id=description` declarations. New worker creation
  still rejects a missing objective.
- Wait evaluates classifier actionability before heartbeat renewal and after
  every authoritative read. The near-expiry regression returns revision 1
  immediately instead of consuming revision 2 with a heartbeat and timing out.

## Self-review

- Confirmed snapshots are built from the post-claim/post-mutation authoritative
  baton, preserve compatibility keys, include canonical objective/task/scope,
  and never serialize the raw credential token.
- Confirmed created vadi output exposes the run and copyable peer prompt before
  domain work; claimed/reclaimed/resumed starts share the same snapshot shape.
- Confirmed exact summary, objective ref, task reference, and deliverable
  mismatches all preserve the installed revision and claims, including when a
  different live session already owns the role.
- Confirmed exact missing and busy return in under the test's 500 ms bound even
  with a 2 s `--wait`, while broad reviewer discovery waiting remains intact.
- Confirmed semantic actions and harness-family duties remain independent in
  normal and reversed casting: Codex publishes and Claude reviews regardless
  of worker/reviewer role.
- Confirmed publication-blocked submit/review/finalize paths advertise useful
  advisory work plus a concise reason without advertising the blocked mutation.
- Confirmed pending checkpoint supersession advertises acceptance and never
  `record_review`; finalizing workers retain the safe withdrawal route even
  while publication blocks finalization.
- Confirmed terminal snapshots advertise `stop`, assigned-away/non-contact
  snapshots advertise `wait` unless an orthogonal harness duty is currently
  legal, and no goal or peer-harness invocation was introduced.

## Concerns

No known blocker or residual concern remains.

## Fix round 1: broad scope fencing and exact-run isolation

### Strict TDD evidence

The review regressions were written and run against `cbcb89f` before production
fixes.

```bash
cargo test --manifest-path v4/Cargo.toml --test discovery exact
# 4 run: 3 passed, 1 failed

cargo test --manifest-path v4/Cargo.toml --test role_session snapshot_ -- --test-threads=1
# 9 run: 5 passed, 4 failed

cargo test --manifest-path v4/Cargo.toml --test role_session migration_exact -- --test-threads=1
# 2 run: 0 passed, 2 failed
```

The failures proved that an unrelated corrupt sibling masked an exact match;
non-exact objective omission reached repository IO; immediate and post-wait
broad scope mismatches claimed the reviewer; exact role start was masked by an
unrelated corrupt sibling; exact v1 scope mismatch returned
`upgrade_required`; and upgrade output omitted canonical task identity.

Preserved logs:

- `/tmp/dvandva-task5-fix1-discovery-red.log`
- `/tmp/dvandva-task5-fix1-snapshot-red.log`
- `/tmp/dvandva-task5-fix1-upgrade-red.log`
- `/tmp/dvandva-task5-fix1-identity-red.log` (1 run, 0 passed, 1 failed;
  a named directory containing a different valid run identity returned
  `run_missing` instead of failing closed as `corrupt`)
- `/tmp/dvandva-task5-fix1-v1-default-red.log` (1 run, 0 passed, 1 failed;
  the deterministic `legacy_objective` scope declaration was compared against
  an empty legacy representation instead of the canonical upgrade default)

### GREEN evidence

```bash
cargo test --manifest-path v4/Cargo.toml --test discovery exact
# 5 passed, 0 failed, 19 filtered out

cargo test --manifest-path v4/Cargo.toml --test role_session snapshot_ -- --test-threads=1
# 9 passed, 0 failed, 21 filtered out

cargo test --manifest-path v4/Cargo.toml --test role_session migration_exact -- --test-threads=1
# 3 passed, 0 failed, 27 filtered out

cargo test --manifest-path v4/Cargo.toml --test skill_flow
# 5 passed, 0 failed

cargo test --manifest-path v4/Cargo.toml --all-targets
# 147 integration tests passed, 0 failed; unit targets also green

cargo fmt --manifest-path v4/Cargo.toml -- --check
# exit 0

cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
# exit 0

git diff --check
# exit 0
```

Final logs:

- `/tmp/dvandva-task5-fix1-discovery-green.log`
- `/tmp/dvandva-task5-fix1-snapshot-green.log`
- `/tmp/dvandva-task5-fix1-upgrade-green.log`
- `/tmp/dvandva-task5-fix1-skill-flow-green.log`
- `/tmp/dvandva-task5-fix1-full-green.log`
- `/tmp/dvandva-task5-fix1-fmt-green.log`
- `/tmp/dvandva-task5-fix1-clippy-green.log`
- `/tmp/dvandva-task5-fix1-diff-check.log`

### Self-review and concerns

- Confirmed every non-exact start rejects an omitted objective before workspace,
  discovery, credential, wait, or claim work; exact joins remain scope-optional.
- Confirmed one scope classifier compares unique immediate and post-wait
  candidates for summary, refs, task, and deliverables, including
  `TaskMismatch`, `Busy`, and `UpgradeRequired`, while explicit `--new-run`
  bypasses adoption and creates its separate worker run.
- Confirmed every mismatch preserves baton/history revision, reviewer claim,
  and credential state; mismatch remains higher priority than busy.
- Confirmed exact discovery examines only the directory whose filename equals
  the requested run ID, without joining or traversing caller input. Unrelated
  corruption cannot mask match, busy, missing, v1 upgrade, or scope mismatch;
  named invalid JSON and named run-identity mismatch remain `corrupt`.
- Confirmed v1 candidates expose and compare the deterministic
  `legacy_objective` default that dedicated upgrade will install. Conflicting
  summary/ref/task/deliverables return `scope_mismatch`; omitted or exactly
  matching scope returns `upgrade_required` without credentials or mutation.
- Confirmed upgrade-required output now includes canonical `task_reference` and
  `task_summary` alongside objective/scope/status/assignee/revision.

No known blocker or residual concern remains.
