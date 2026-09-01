---
status: accepted
---

# Make the approved explainer the run's private status Site

The digest-bound run artifact from ADR 0004 remains canonical. Vadi authors the
local HTML and prativadi reviews those exact bytes; requested changes produce a
new digest and a new review. At the `run_started` handoff, this happens before
vadi continues domain work, so the user receives an agreed status-page shape at
the beginning of the run.

After approval, whichever participant is Codex mechanically publishes that
digest to one stable, owner-only ChatGPT Site for the run. Later approved
handoffs update the same Site, making its URL an easy-to-visit status page.
The Site never replaces the local artifact and prativadi never reviews through
the URL.

When a pairing contains Codex, finalization requires the local approval and the
matching Sites receipt. If neither participant is Codex, Sites publication is
skipped and the local approval is sufficient. This supersedes only ADR 0004's
optional-publication clause; its local-channel trust boundary remains intact.
Kernel `0.3.3` carries the role-owned author/reviewer rules and conditional
Codex Sites gate; the write schema remains `dvandva.run.v2` and role API 2.
