# Task 7D report: fence generic CAS from new legacy writes

## TDD mutation target

The public store regression must fail if generic `compare_and_swap` lacks a
current-head schema fence. Without that fence, the intentionally opaque v1
history-edge validator accepts same-schema successors and lets callers create,
erase, mutate, or blank the legacy task identity.

The test exercises the real public `RunChannel` against persisted v1 heads with
both absent and present task identities. Every rejected attempt must preserve
the head bytes and decoded baton and must not create revision 1 history.

## RED evidence

Before any production edit:

```text
$ cargo test --manifest-path v4/Cargo.toml --test run_channel legacy_cas -- --nocapture
running 1 test
thread 'legacy_cas_rejects_task_identity_mutations_without_writing_history' panicked at tests/run_channel.rs:1045:9:
legacy CAS accepted the create task-identity mutation
test legacy_cas_rejects_task_identity_mutations_without_writing_history ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 86 filtered out
```

The command exited 101. The failure is the intended illegal v1 successor, not
a fixture, compilation, or setup error.

The existing eligible-crossing regression was also tightened to the public
error contract and failed before the production edit:

```text
$ cargo test --manifest-path v4/Cargo.toml --test run_channel migration_integrity_generic_cas_rejects_even_an_eligible_crossing -- --exact
running 1 test
test migration_integrity_generic_cas_rejects_even_an_eligible_crossing ... FAILED
assertion failed: matches!(channel.compare_and_swap(0, &next), Err(StoreError::MigrationRequired))
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 86 filtered out
```

## GREEN evidence

Commits before this report update:

- `7f12984` `test(v4): expose writable legacy CAS seam`
- `99ff7f6` `test(v4): require migration error for legacy CAS`
- `1155453` `fix(v4): fence generic CAS from legacy writes`

Fresh verification after restoring the production fence:

```text
$ cargo test --manifest-path v4/Cargo.toml --test run_channel legacy_cas
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 86 filtered out

$ cargo test --manifest-path v4/Cargo.toml --test run_channel migration
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 76 filtered out

$ cargo test --manifest-path v4/Cargo.toml --all-targets
lib:           6 passed
credential:    3 passed
discovery:    26 passed
identity:      4 passed
role_session: 34 passed
run_channel:  87 passed
skill_flow:   12 passed
total:       172 passed; 0 failed

$ cargo fmt --manifest-path v4/Cargo.toml -- --check
exit 0

$ cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
Finished `dev` profile; exit 0

$ git diff --check
exit 0
```

## Mutation check

Removing only the new three-line current-schema fence and rerunning the focused
regression reproduced the original failure:

```text
$ cargo test --manifest-path v4/Cargo.toml --test run_channel legacy_cas -- --nocapture
running 1 test
legacy CAS accepted the create task-identity mutation
test legacy_cas_rejects_task_identity_mutations_without_writing_history ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 86 filtered out
```

The command exited 101. The fence was restored exactly before the fresh GREEN
verification above.

## Residual boundary

This slice does not reinterpret or reject existing v1 history prefixes;
dedicated upgrade remains responsible for the only legal write across the
v1-to-v2 epoch. As before, an attacker with the same operating-system identity
who can replace the complete run directory can rewrite untrusted filesystem
history; provider signing or a stronger local trust boundary is outside this
patch.
