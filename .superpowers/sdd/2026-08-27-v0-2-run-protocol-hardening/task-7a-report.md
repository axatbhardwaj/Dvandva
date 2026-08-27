# Task 7A report: kernel-authoritative Human Decision escalation

## What changed

- Every active, non-Human-Decision role snapshot now includes
  `request_human_decision` in `legal_actions`.
- The escape action is deliberately excluded from `next_actions` and
  `actionable`, so a waiting role continues to block in `role wait`.
- Human Decision and terminal snapshots do not advertise another request.
- `request_human_decision` now accepts only `question`, `evidence`, and
  `options`. Action decoding rejects obsolete caller-supplied routing fields.
- Inside the claim-fenced, locked transition, the kernel records the requesting
  role as the contact and copies the pre-transition status and assignee as the
  resume target.
- Existing focused fixtures were migrated to the smaller action payload. The
  two direct `Action` constructors in `role_session.rs` were test-only compile
  adaptations; no role-session production behavior changed.

## RED evidence

1. The active snapshot regression failed because the legal escape was absent:

   ```text
   test claimed_active_snapshots_advertise_human_escape_without_making_wait_actionable ... FAILED
   Working/Worker/Worker omitted the escape action
   test result: FAILED. 0 passed; 1 failed
   ```

2. The minimal public payload failed because caller routing was still required:

   ```text
   test human_decision_derives_requester_contact_and_authoritative_resume_target ... FAILED
   {"error":"invalid_baton","message":"invalid baton JSON: missing field `contact_role`"}
   test result: FAILED. 0 passed; 1 failed
   ```

3. The legacy-key regression did not reject the obsolete key. Decoding instead
   continued looking for the other caller routing fields:

   ```text
   test human_decision_rejects_legacy_caller_supplied_routing ... FAILED
   Unexpected stderr, failed var.contains(unknown field)
   var: {"error":"invalid_baton","message":"invalid baton JSON: missing field `resume_status`"}
   test result: FAILED. 0 passed; 1 failed
   ```

The Human Decision/terminal non-advertisement regression passed before the
production change, confirming that the existing exclusion remained intact.

## GREEN evidence

- `cargo test --manifest-path v4/Cargo.toml --test role_session` — 34 passed.
- `cargo test --manifest-path v4/Cargo.toml --test run_channel human_decision`
  — 3 passed.
- `cargo test --manifest-path v4/Cargo.toml --test skill_flow` — 5 passed.
- `cargo test --manifest-path v4/Cargo.toml --all-targets` — 161 passed, 0
  failed.
- `cargo fmt --manifest-path v4/Cargo.toml -- --check` — passed.
- `cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings` —
  passed.
- `git diff --check` — passed.

The new role-wait regression claims an out-of-turn reviewer whose only workflow
action is `wait`; it still reaches the structured timeout rather than returning
immediately because the exogenous escape appears only in `legal_actions`.

## Commit

- `937f97f feat(v4): make human escalation kernel authoritative`

## Boundary and blockers

- No role skills, active docs, installer, packaging, v3 archive, push, PR, or
  release state was changed.
- No blocker remains for Task 7 to consume the kernel-provided escape action.

## Next action

The reviewer owns the next action. Run:

```bash
git diff fc7a6c1..HEAD -- v4/src/action.rs v4/src/transition.rs \
  v4/src/next_action.rs v4/src/role_session.rs v4/tests/role_session.rs \
  v4/tests/run_channel.rs \
  .superpowers/sdd/2026-08-27-v0-2-run-protocol-hardening/task-7a-report.md
```
