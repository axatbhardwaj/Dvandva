---
status: superseded
---

# Delegate explicit-only skill turns to lane adapters

> Superseded by issue #3 and `docs/protocol/minimal-run-baton.md`. The minimal
> kernel does not dispatch skills. Explicit-only skills remain human-invoked
> before or alongside a run, and skill activation is not Baton state.

Matt Pocock's top-level workflow skills are intentionally user-invoked, and
his literal convention says a human types each command. Requiring a new human
turn at every Dvandva phase would prevent the approved walkaway run, while
making those skills implicitly model-invocable would weaken their policy.

After the human grants a run-scoped Skill Activation Lease, each non-model T3
Lane Session Adapter may submit an allowlisted explicit skill turn only to its
own already-running Role Session. Each Skill Turn Directive is revision-bound,
idempotent, and recorded with a host receipt. It cannot create or resume a
harness, target the opposite lane, answer a genuine human checkpoint, or grant
Git, Sites, release, or any other external-write authority.

This is a deliberate Dvandva extension of Matt's human-typing convention, not
implicit model invocation and not a claim that upstream skills call one
another. Strict human-typed dispatch remains the fallback. Missing or changed
skill capabilities fail closed rather than silently changing invocation mode.
