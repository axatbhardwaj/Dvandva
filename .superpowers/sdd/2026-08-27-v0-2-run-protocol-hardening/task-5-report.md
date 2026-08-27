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

## Fix round 2: ambiguous upgrades and revision-bound snapshots

Implementation commits:

- `c1e356e` (`fix(v4): fail closed on ambiguous run discovery`)
- `2158989` (`fix(v4): bind role snapshots to checked revisions`)

### Strict TDD RED evidence

The second review regressions were written and run against `14b1c8b` before
production fixes.

```bash
cargo test --manifest-path v4/Cargo.toml --test discovery \
  broad_discovery_is_ambiguous_for_multiple_or_mixed_upgrade_candidates
# 1 run: 0 passed, 1 failed; returned upgrade_required instead of ambiguous

cargo test --manifest-path v4/Cargo.toml --test discovery \
  exact_named_non_directory_missing_head_and_terminal_identity_mismatch_are_corrupt
# 1 run: 0 passed, 1 failed; named non-directory returned run_missing

cargo test --manifest-path v4/Cargo.toml --lib \
  resumed_candidate_revision_drift_is_rediscovered_before_snapshot
# 1 run: 0 passed, 1 failed; revision-1 candidate emitted resumed revision 3

cargo test --manifest-path v4/Cargo.toml --test role_session \
  migration_broad_start_is_ambiguous_across_multiple_upgrade_candidates
# 1 run: 0 passed, 1 failed; returned upgrade_required instead of ambiguous

cargo test --manifest-path v4/Cargo.toml --lib \
  created_start_accepts_an_immediate_peer_claim_without_recreating
# 1 run: 0 passed, 1 failed; peer claim advanced revision 1 to 2 and caused
# created start to return a revision conflict
```

Preserved logs:

- `/tmp/dvandva-task5-fix2-ambiguity-red.log`
- `/tmp/dvandva-task5-fix2-corrupt-red.log`
- `/tmp/dvandva-task5-fix2-resume-red.log`
- `/tmp/dvandva-task5-fix2-role-ambiguity-red.log`
- `/tmp/dvandva-task5-fix2-created-race-red.log`

### GREEN evidence

```bash
cargo test --manifest-path v4/Cargo.toml --test discovery exact
# 6 passed, 0 failed, 20 filtered out

cargo test --manifest-path v4/Cargo.toml --test role_session snapshot -- --test-threads=1
# 9 passed, 0 failed, 22 filtered out

cargo test --manifest-path v4/Cargo.toml --test role_session migration -- --test-threads=1
# 9 passed, 0 failed, 22 filtered out

cargo test --manifest-path v4/Cargo.toml --test skill_flow
# 5 passed, 0 failed

cargo test --manifest-path v4/Cargo.toml --lib
# 2 passed, 0 failed

cargo test --manifest-path v4/Cargo.toml --all-targets
# 150 integration tests and 2 unit tests passed, 0 failed

cargo fmt --manifest-path v4/Cargo.toml -- --check
# exit 0

cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
# exit 0

git diff --check
# exit 0
```

Final logs:

- `/tmp/dvandva-task5-fix2-discovery-exact-green.log`
- `/tmp/dvandva-task5-fix2-snapshot-green.log`
- `/tmp/dvandva-task5-fix2-migration-green.log`
- `/tmp/dvandva-task5-fix2-skill-flow-green.log`
- `/tmp/dvandva-task5-fix2-role-session-unit-green.log`
- `/tmp/dvandva-task5-fix2-full-green.log`
- `/tmp/dvandva-task5-fix2-fmt-green.log`
- `/tmp/dvandva-task5-fix2-clippy-green.log`
- `/tmp/dvandva-task5-fix2-diff-check.log`

### Self-review and concerns

- Confirmed two v1 upgrades, or any mixed set of multiple current/upgrade
  candidates, returns sorted canonical `ambiguous` candidates. Only one unique
  v1 candidate returns `upgrade_required`; discovery performs no mutation.
- Confirmed an exact named non-directory, missing/unreadable Baton head, and
  terminal Baton with a mismatched stored run ID fail closed as `corrupt`.
  A truly absent name remains `run_missing`, unrelated corrupt siblings remain
  isolated, and the query run ID is never joined into a filesystem path.
- Confirmed candidate scope is reconciled at revision N and claimed/reclaimed
  snapshots are checked at the exact successful CAS revision. Owned revision
  drift becomes a typed revision conflict, enters bounded rediscovery, and
  reclassifies the new canonical scope before emitting a snapshot.
- Confirmed started output is built directly from the checked snapshot with no
  post-check reread. Newly created runs use a separate non-recursive completion
  path: an immediate peer claim can advance the revision and the creator still
  returns the current authoritative snapshot without creating another run.
- Confirmed round-1 objective/scope fences, explicit `--new-run`, exact v1
  mismatch precedence and task identity, exact sibling isolation, actionable
  wait ordering, and the full skill flow remain green.

No known blocker or residual concern remains.

## Fix round 3: claim linearization and same-run creation retry

Implementation commit: `7fa9238` (`fix(v4): linearize role start claim
completion`).

### Strict TDD RED evidence

The deterministic private-seam regressions were written and run against
`b98da09` before their production fixes.

```bash
cargo test --manifest-path v4/Cargo.toml --lib \
  new_run_creation_conflict_retries_worker_claim_on_the_same_run
# 1 run: 0 passed, 1 failed; after reviewer claim revision 1, the worker
# continuation returned a new def-123-* run instead of the created run-a

cargo test --manifest-path v4/Cargo.toml --lib \
  claimed_and_reclaimed_completion_keep_the_committed_scope
# 1 run: 0 passed, 1 failed; the post-CAS head advanced from claimed scope A
# revision 1 to amended scope B revision 3, and retry reclassified the
# successful claim as scope_mismatch

cargo test --manifest-path v4/Cargo.toml --lib \
  new_run_creation_retry_rejects_a_changed_canonical_scope
# 1 run: 0 passed, 1 failed; recovered amended scope B was silently claimed
# as the continuation of newly created scope A
```

Preserved logs:

- `/tmp/dvandva-task5-fix3-create-claim-red.log`
- `/tmp/dvandva-task5-fix3-post-claim-red.log`
- `/tmp/dvandva-task5-fix3-scope-change-red.log`

### GREEN evidence

```bash
cargo test --manifest-path v4/Cargo.toml --lib
# 5 passed, 0 failed

cargo test --manifest-path v4/Cargo.toml --test discovery exact
# 6 passed, 0 failed, 20 filtered out

cargo test --manifest-path v4/Cargo.toml --test role_session snapshot -- --test-threads=1
# 9 passed, 0 failed, 22 filtered out

cargo test --manifest-path v4/Cargo.toml --test role_session migration -- --test-threads=1
# 9 passed, 0 failed, 22 filtered out

cargo test --manifest-path v4/Cargo.toml --test skill_flow
# 5 passed, 0 failed

cargo test --manifest-path v4/Cargo.toml --all-targets
# 150 integration tests and 5 unit tests passed, 0 failed

cargo fmt --manifest-path v4/Cargo.toml -- --check
# exit 0

cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
# exit 0

git diff --check
# exit 0
```

Final logs:

- `/tmp/dvandva-task5-fix3-unit-green.log`
- `/tmp/dvandva-task5-fix3-discovery-green.log`
- `/tmp/dvandva-task5-fix3-snapshot-green.log`
- `/tmp/dvandva-task5-fix3-migration-green.log`
- `/tmp/dvandva-task5-fix3-skill-flow-green.log`
- `/tmp/dvandva-task5-fix3-full-green.log`
- `/tmp/dvandva-task5-fix3-fmt-green.log`
- `/tmp/dvandva-task5-fix3-clippy-green.log`
- `/tmp/dvandva-task5-fix3-diff-check.log`

### Self-review and concerns

- Confirmed a post-create claim conflict spends the existing bounded retry
  budget against the same run directory. It never returns to discovery or
  creation, including with explicit `--new-run`; the peer and worker claims and
  private credentials remain bound to one run ID.
- Confirmed each same-run retry accepts only revision/participant-claim drift.
  Worker ownership and terminal state fail through typed claim errors; corrupt
  storage fails through the store error; any identity, scope, task, status, or
  other semantic change fails before a worker claim or credential mutation.
- Confirmed claim and reclaim grants carry the exact Baton produced inside the
  locked CAS. Claimed/reclaimed start snapshots linearize on that immutable
  result, so later scope amendments cannot trigger rediscovery or a false
  no-mutation `scope_mismatch` after claim/history/credential side effects.
- Confirmed owned/resumed candidates retain the revision-bound authoritative
  reread and scope reclassification behavior. Newly created completion still
  reads one current verified snapshot after its successful same-run claim and
  tolerates later orthogonal peer revisions.
- Confirmed the committed Baton is skipped from both claim JSON contracts; raw
  tokens remain private, and the existing public claim/snapshot/skill-flow
  compatibility tests remain green.

No known blocker or residual concern remains.
