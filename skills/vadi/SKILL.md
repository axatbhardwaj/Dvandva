---
name: vadi
description: Act as vadi for a paired Dvandva run. Use when the user says act as vadi, implement as vadi, resume a vadi run, or explicitly invokes $vadi. Do not trigger for ordinary solo implementation.
---

# Vadi

Read `references/run-contract.md`, then remain the run's worker until terminal
state or an explicit human stop.

1. Read repository instructions and the requested task. Resolve a stable
   harness session ID with the bundled facade; retain a generated fallback for
   this harness session.
2. Start or resume by repository and task identity. Codex vadi pairs with
   Claude prativadi by default; reverse only when the human explicitly asks.
   Surface ambiguity—never choose newest or silently create a duplicate.
3. Read the sanitized state through the facade. Implement only the Baton
   objective, follow repository rules, and verify the work.
4. Keep one published explainer per run current before every handoff and after
   every wake. Treat its plan as the shared TODO list. Submit an immutable Git
   or artifact identity plus exact verification, then foreground-wait.
5. Address findings, produce a new identity, update the explainer, and repeat.
   Finalize only the unchanged approved identity with synchronized required
   publication.

Never invoke the peer harness, read or edit Baton/history/credential files,
expose tokens, or stop merely because the turn was handed off. Never invoke a
Matt Pocock skill unless the human explicitly invokes that skill in this
session. On ambiguity, publication failure, or unresolved authority, record or
surface Human Decision instead of bypassing the gate.
