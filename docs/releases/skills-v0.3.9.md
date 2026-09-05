# Dvandva skills 0.3.9

This release adds paired discovery and automatic peer lookup, so independently
started sessions can form a pair without copying a join prompt.

- Four workflows: **Discovery**, **Implementation**, **Babysitting**, **Review**.
- Discovery uses Claude Fable 5.1/high as vadi and Codex Astra/high as prativadi.
  Both discover sources and investigate independently. Spec approval and verified
  ticket creation are separate linked runs in the same sessions.
- Matt Pocock's user-only skills remain explicit entry points: grill-with-docs,
  to-spec and to-tickets for discovery; implement in the fresh implementation
  session. Waiting for an invocation yields and exact-resumes the active run.
- Prativadi discovers available runs in the existing XDG registry, filtering
  repository, workflow, task and harness pairing. Ambiguous scopes require a
  choice; no match waits. Selected runs use the unchanged exact-join protocol.
- Registry enumeration is observational: it neither reconciles history nor
  creates run locks. The facade requires the read-only discovery capability;
  exact joins independently validate the peer harness before claiming.
- New Review runs persist across requested changes and pending CI. A complete
  readiness handoff wakes the reviewer when checks become green, preserving the
  existing formal GitHub review receipt. Legacy pr_review remains one-shot.
- Preserve the initial explainer join gate, immutable checkpoints, existing
  publication gates and separate human merge authority. This removes the join
  prompt dependency; it does not claim to fix T3 message rendering.

Kernel **0.3.9** writes **dvandva.run.v2** with **role API 2**. Linux x86_64 only.
The private kernel stays outside PATH; archived v3 sources and plugin are unchanged.

Update all four released skills together, then run setup-dvandva update for
0.3.9. Existing run histories and old kernel versions remain intact; setup
never migrates runs. Start fresh host sessions to load the new instructions.
