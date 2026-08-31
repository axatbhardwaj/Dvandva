# Prativadi code-review pressure test

Date: 2026-09-01

## Method

The RED baseline is the immutable prativadi source at
`17a2191a614338b4d3f51714e4740877d3815ff5`. Its `SKILL.md` sha256 is
`d91cdcc12c6bd3d25087d18da3b2c82f9c4a50220a284e4c2a4b0562e2003168` and
its run-contract sha256 is
`16f74737cf5f7544202c4d95d1b1eb7ce5b5a9d5945454a0bc1c7d2a9f223fb2`.
Reproduce either with `git show <commit>:<path> | sha256sum`.

The GREEN guidance was first committed at
`ec9863d5424c8daac1d3b02e16a240f3d127361f` and hardened through the reviewed
candidate `efce1087e73f43aada884354dfdd11d9d0eaa4e9`. Two evidence forms are
retained:

- the deterministic contract assertions in `v4/tests/skill_flow.rs`, runnable
  with `cargo test --locked --manifest-path v4/Cargo.toml --test skill_flow`;
- the [sanitized prompt, result, and source hashes](evidence/2026-09-01-prativadi-final-retest.md)
  for the final fresh-context cross-casting retest.

Earlier exploratory agents informed checkpoint-drift, mutable-spec, and
fallback hardening, but their unretained transcripts are not counted as release
evidence here.

## Results

| Scenario | RED baseline | GREEN guidance |
| --- | --- | --- |
| Complete Git candidate, companion available | Source required human invocation for all Matt skills. | After the whole scope is implemented and checkpointed, selects `code-review` once, pins an immutable base, uses the exact checkpoint as `HEAD`, supplies immutable scope, and treats the reports as evidence rather than the Dvandva verdict. |
| Analysis checkpoint | Used native checkpoint review. | Explicitly keeps analysis native and applies separate Standards and Spec axes before a coordinate-bound verdict. |
| Complete Git candidate, companion unavailable | Used native checkpoint review without a precise companion disclosure. | Performs both axes as a disclosed native fallback after complete implementation and does not ask the absent human or treat availability as a blocker. |

The contract deliberately excludes partial and work-in-progress implementation.
If the first complete candidate receives requested changes, vadi finishes the
revision and submits a new complete candidate before Matt review runs again.

## Hardened evidence boundary

The deterministic assertions require exact checkpoint identity, manifest
digest, scope revision, Git `HEAD`, fixed-point SHA, spec digest, review mode,
both axis summaries, adjudication, and fallback disclosure. Guidance discards
reports when any bound coordinate drifts. A companion Spec report is valid only
when it used the supplied immutable snapshot; otherwise both axes fall back to
native review.

The retained fresh-context scenario covers WIP implementation, its first
complete candidate, a changes-requested revision, unavailable-companion
fallback, evidence transport, and peer independence. Its result is explicitly
a guidance-level PASS, not a runtime protocol test.

The final guidance keeps that boundary explicit. The current session reports a
compact provenance summary under `What was verified`; `record_review` durably
binds only the verdict and accepted actionable findings to the authorized
checkpoint. The peer consumes those facade-verified fields. Raw reports are not
smuggled through the explainer, and adding a new protocol evidence channel is
outside this compatibility change.

These checks do not prove host availability of a missing or disabled companion;
that is why the contract has a native fallback. They also do not claim that
local raw reports become durable protocol state. Current host-policy
documentation and the installed metadata check are recorded in
`docs/workflows/skill-only-run.md`.
