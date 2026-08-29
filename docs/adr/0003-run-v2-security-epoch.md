---
status: accepted
---

# Treat run v2 and role API 2 as one security epoch

Dvandva adopts `dvandva.run.v2`, kernel 0.2.0, and role API 2 as one security
epoch. V1 is accepted only by a dedicated one-way upgrade that preserves every
historical byte, appends one v2 revision, clears claims, and fences old
credentials. Ordinary v1 operations, schema downgrade, old-facade/new-kernel,
and new-facade/old-kernel combinations fail before run mutation. Setup installs
kernel bytes but never migrates runs.

The epoch fixed publication to one stable owner-only Codex Site per run,
published by the authenticated Codex participant and reviewed at each handoff
by the authenticated Claude participant. A different publication policy needs
a deliberate protocol decision rather than an in-run override.

> **Superseded in part by
> [ADR 0004](0004-run-artifact-explainer-channel.md).** An owner-only Site is
> readable only through the publisher owner's session, so the designated Claude
> reviewer could never open it and the gate was unsatisfiable. The explainer gate
> now binds digest-bound bytes staged in the run directory; a Site is an optional
> rendering. Every other clause of this epoch stands.

## External-reference trust boundary

The kernel authenticates the claimed participant and structurally binds the
checkpoint, deployment, and review coordinates. The two harnesses attest that
the named Git object, artifact, Site deployment, and reviewed content exist.
V0.2 does not contact Codex Sites or resolve every external reference, and its
structural receipts are not provider-signed proof.
