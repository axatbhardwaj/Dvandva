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

Commits before this report update:

- `f506a69` `test(v4): cover task identity after legacy upgrade`
- `ecdaad4` `fix(v4): preserve amended task identity after upgrade`

Fresh verification after the production fix:

```text
$ cargo test --manifest-path v4/Cargo.toml --test run_channel scope_amendment
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 83 filtered out

$ cargo test --manifest-path v4/Cargo.toml --all-targets
lib:          6 passed
credential:   3 passed
discovery:   26 passed
identity:     4 passed
role_session: 34 passed
run_channel: 86 passed
skill_flow:  12 passed
total:       171 passed; 0 failed

$ cargo fmt --manifest-path v4/Cargo.toml -- --check
exit 0

$ cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
Finished `dev` profile; exit 0

$ git diff --check
exit 0
```

The focused regression also deletes the installed head and invokes `recover`
from revision 4, forcing the channel to replay and validate the immutable
taskless-v1 to v2 to scope-amended history before the reopened read succeeds.

## Residual risk

No known correctness gap remains in this slice. The history edge must infer the
approved action from committed state, so it permits `None -> Some(task)` only
when the new task has a non-null exact reference. A second regression proves a
null task reference keeps an upgraded taskless run taskless; existing coverage
continues to prove that a taskful run may clear its reference without losing its
task summary.
