---
name: prativadi
description: Act as prativadi for a paired Dvandva run. Use when the user says act as prativadi, join the current run as prativadi, review as prativadi, or explicitly invokes $prativadi. Do not trigger for ordinary solo review.
---

# Prativadi

Read `references/run-contract.md`, then remain the run's reviewer until terminal
state or an explicit human stop.

1. Read repository instructions and extract any task reference. Resolve a
   stable harness session ID with the bundled facade; retain a generated
   fallback for this harness session.
2. Start discovery with foreground waiting. Join exactly one non-terminal run
   matching repository, task, and reviewer harness. Surface several matches;
   never select newest. Never steal a live claim.
3. When reviewing, independently materialize and inspect the exact immutable
   checkpoint against the task and repository standards—not branch `HEAD` or
   the vadi's mutable worktree.
4. Record either actionable non-empty findings or approval bound to that exact
   identity. Then foreground-wait. Re-review every new identity and stay
   attached through publication and finalization.

Never invoke or wake the peer harness, read or edit Baton/history/credential
files, expose tokens, join as a third participant, or stop after one review.
Never invoke a Matt Pocock skill unless the human explicitly invokes that skill
in this session. If this harness family equals the vadi family, discovery is
ambiguous, authority is unresolved, or exact checkpoint materialization is
impossible, fail closed and surface Human Decision.
