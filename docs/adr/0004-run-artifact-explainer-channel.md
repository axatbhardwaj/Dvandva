---
status: accepted
---

# Bind the explainer gate to run-directory bytes, not a Codex Site

ADR 0003 fixed publication to one stable owner-only Codex Site per run,
published by the authenticated Codex participant and reviewed at each handoff by
the authenticated Claude participant, and said a different publication policy
needs a future protocol epoch rather than an in-run override. This ADR is that
change, and it supersedes ADR 0003's publication clause only. Every other part
of the v2 / role API 2 security epoch stands: the one-way v1 upgrade, the
fenced credentials, the failure of mismatched facade/kernel combinations, and
the external-reference trust boundary are unchanged.

## Why the previous policy could not hold

`access: owner_only` means the Site is reachable only through the owner's
ChatGPT session. The Claude harness has no such session and no credential path
to one, so the participant the epoch designated as reviewer was required to read
an artifact it categorically could not open. Run
`fetch-and-analyze-pr-914-diff-assess-cor-9287dc85` completed its domain work and
then terminated at `human_decision` on an HTTP 401, with no checkpoint and no
verdict. That is not a recoverable failure: every run of this pairing ends there,
so the epoch's own gate made the epoch unsatisfiable.

## Decision

The publishing harness stages the explainer's bytes into the run directory at
`explainer/<source_digest>.html`, and the gate binds that sha256. Both roles are
local processes sharing the run directory, so the reviewing harness can always
read exactly what it is approving, and `access: run_private` is enforced with
explicit 0700/0600 modes rather than the caller's umask.

A Codex Sites deployment remains available as an optional human-facing rendering
of the already-staged bytes. It must name the same digest, and it satisfies
nothing by itself.

`start` refuses a policy whose reviewer cannot read its channel, and
`repair-policy` installs the readable channel and clears the stale receipts. A
capability mismatch is therefore a protocol-internal problem with a
deterministic recovery, not a Human Decision that parks the run.

Finalization rehashes the staged bytes instead of trusting the receipt, so
deletion or tampering after approval cannot reach a terminal state.

## Consequences

Kernel 0.3.0 carries this behaviour. The write schema stays `dvandva.run.v2` and
the facade stays role API 2, because the wire contract and the trust boundary did
not move; what moved is which artifact the gate binds. A 0.3.0 kernel reads
0.2.0 runs, including their Site-only publication bindings and their
unmodelled checkpoint kinds, and routes them to `repair-policy`. A 0.2.0 kernel
cannot read a 0.3.0 baton, which is what the kernel version bump records.

Explainer bytes accumulate in the run directory, one file per distinct
explainer. They are content-addressed, so re-staging identical bytes is free,
and they double as the run's audit trail.
