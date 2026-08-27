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

Pending the minimal store fence.

## Mutation check

Pending the minimal store fence.

## Residual boundary

Pending final verification. This slice does not reinterpret or reject existing
v1 history prefixes; dedicated upgrade remains responsible for the only legal
write across the v1-to-v2 epoch.
