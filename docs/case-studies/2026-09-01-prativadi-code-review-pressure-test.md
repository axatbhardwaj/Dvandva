# Prativadi code-review pressure test

Date: 2026-09-01

## Method

Six fresh-context, read-only agents received only the scenario, the prativadi
skill paths, and the installed Matt `code-review` path. They could inspect
sources but could not edit files, start a Dvandva run, or mutate protocol
state. Each scenario combined an absent human with lease or deadline pressure
and no explicit human invocation of Matt's skill.

The RED baseline used prativadi guidance from
`17a2191a614338b4d3f51714e4740877d3815ff5`. The GREEN guidance was first
committed at `ec9863d5424c8daac1d3b02e16a240f3d127361f` and retained at the
reviewed release candidate `1a99514fd0c679497dff1b0481d1eb3e69d1e031`.

## Results

| Scenario | RED baseline | GREEN guidance |
| --- | --- | --- |
| Complete Git candidate, companion available | Refused automatic `code-review` because all Matt skills were described as human-only. | After the whole scope was implemented and checkpointed, automatically selected `code-review` once, pinned an immutable base, used the exact checkpoint as `HEAD`, supplied canonical scope, and kept its output as evidence rather than the Dvandva verdict. |
| Analysis checkpoint | Reviewed verified staged bytes natively because the companion is Git-diff-specific. | Explicitly kept analysis native and applied separate Standards and Spec axes before a coordinate-bound verdict. |
| Complete Git candidate, companion unavailable | Continued natively, but the companion was merely optional and no precise fallback disclosure was required. | Performed both axes as a native fallback after complete implementation, disclosed it under `What was verified`, and did not ask the absent human or mark availability as a blocker. |

All three GREEN agents also preserved the fresh-snapshot verdict binding,
five-part handoff, foreground polling, and prohibition on invoking the peer
harness. The deterministic contract test is
`prativadi_automatically_reviews_complete_git_delivery_candidates` in
`v4/tests/skill_flow.rs`.

The contract deliberately excludes partial and work-in-progress implementation.
If the first complete candidate receives requested changes, vadi finishes the
revision and submits a new complete candidate before Matt review runs again.

## Evidence-binding follow-up

Four additional fresh-context agents pressure-tested the hardened review
contract. A checkpoint-drift scenario discarded both reports and restarted
after any checkpoint identity, manifest digest, scope revision, or Git `HEAD`
change. A mutable-issue scenario reviewed a private sha256-bound spec snapshot
rather than later live issue bytes. A cross-casting fallback scenario exposed
that the Baton has no field or artifact channel for transporting raw companion
reports or rejected findings between harnesses.

After that boundary was made explicit, a final cross-casting retest covered a
WIP implementation, its first complete candidate, a changes-requested revision,
and an unavailable-companion fallback. It passed all five expectations: no WIP
review, one invocation per newly authorized complete candidate, native fallback,
no invented raw-report transport, and no peer-harness invocation.

The final guidance keeps that boundary explicit. The current session reports a
compact provenance summary under `What was verified`; `record_review` durably
binds only the verdict and accepted actionable findings to the authorized
checkpoint. The peer consumes those facade-verified fields. Raw reports are not
smuggled through the explainer, and adding a new protocol evidence channel is
outside this compatibility change.

## Evidence boundary

This pressure test proves that fresh agents interpret the released instructions
consistently under conflicting pressures. It does not prove host availability
of a missing or disabled companion; that is why the contract has a native
fallback. It also does not claim that local raw reports become durable protocol
state. Current host-policy documentation and the installed metadata check are
recorded in `docs/workflows/skill-only-run.md`.
