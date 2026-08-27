# Task 7C report: preserve task identity after legacy upgrade

## TDD mutation target

The black-box regression must fail if production restores both old behaviors:

- `apply_scope_amendment` updates a task only through `if let Some(task)`; and
- `valid_scope_amended` requires task presence to be identical across the edge.

Those behaviors silently discard a human-approved non-null task reference after
a taskless v1 run is upgraded to v2.

## RED evidence

Before any production edit, the regression exercised the real CLI sequence:
taskless v1 history, dedicated protocol upgrade, worker claim, Human Decision,
scope-amended resume, and recovery from immutable history.

```text
$ cargo test --manifest-path v4/Cargo.toml --test run_channel scope_amendment_adds_human_approved_task_identity_after_taskless_legacy_upgrade -- --exact
running 1 test
test scope_amendment_adds_human_approved_task_identity_after_taskless_legacy_upgrade ... FAILED

thread 'scope_amendment_adds_human_approved_task_identity_after_taskless_legacy_upgrade' panicked at tests/run_channel.rs:695:5:
assertion `left == right` failed
  left: Null
 right: "DEF-456"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 84 filtered out
```

The failure is the intended missing-task behavior, not fixture or command setup.

## GREEN evidence

Pending.

## Residual risk

Pending final verification.
