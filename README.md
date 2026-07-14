# Dvandva

Dvandva is **a governed loop for cross-vendor adversarial execution** — one chair session drives the work, and every step is attacked by a model that didn't author it before the loop can call anything done. There is no daemon, no launcher, no second live session to babysit: the product is a **skill** that drives the loop, a **hook** that enforces it fail-closed, and a **workflow template** that runs authoring and attack lanes in parallel.

One rule generates the whole system: **the reviewer is never the author.** In the strong form the reviewer is a different *vendor* (Claude proposes, GPT attacks; GPT executes, Claude attacks) — different training lineages have systematically different blind spots, and the loop weaponizes that. With one vendor it degrades honestly to a different *fresh-context agent* (opus attacks sonnet; sol attacks terra).

**At a glance**

- **Propose → stamp → attack → gate**, per step. The artifact is digest-stamped before review; the attacker recomputes the digest itself; evidence binds to the exact bytes.
- **Evidence-gated `done`** — on Claude Code, a Stop hook re-blocks the chair's turn until every step carries a passing cross-reviewer verdict bound to the current artifact digest (a bounded fail-closed nudge: Claude Code force-ends after eight consecutive blocks). Codex-driven use gates through an explicitly invoked CLI / git pre-commit checkpoint instead. Declaring victory early just blocks.
- **Append-only evidence** — a publisher convention the skill enforces in-procedure: a recorded `fail` is never overwritten; fixes bump the revision, re-stamp, re-attack. The gate validates the current files' shape, binding, and verdicts — not their history.
- **Three deployment configs on one predicate** — cross-vendor, Claude-only, Codex-only.
- **Honest enforcement** — the gate fails closed on the *procedure* (evidence present, cross-reviewer, digest-bound, verdict-passing). It cannot prove a real provider call happened; that boundary is documented, not papered over — and was demonstrated live during acceptance testing.

## Install

The product ships as the `dvandva` plugin (version `2.0.1`) — no binary, no build step.

```bash
# Claude Code
claude plugin marketplace add axatbhardwaj/Dvandva
claude plugin install dvandva@dvandva

# Codex
codex plugin marketplace add axatbhardwaj/Dvandva
codex plugin add dvandva@dvandva
```

After install, `/skills` includes the five shipped skills — `dvandva:adversarial-loop`, `dvandva:delegating-to-codex`, `dvandva:delegating-to-grok`, `dvandva:html-deliverables`, `dvandva:worktree-setup` — and the plugin's Stop hook (`hooks/adversarial/gate.sh`) is registered automatically. The gate is inert until a session creates a goal — no `.adversarial-loop/goal.json`, no gating.

Add the runtime state directory to your project's `.gitignore`:

```
.adversarial-loop/
```

## How it works

1. **Create a goal.** The `adversarial-loop` skill writes `.adversarial-loop/goal.json`: the acceptance criterion, the owning session id, a `mode`, and the steps, each with an author family/agent id. The owner id must come from the adapter's environment and is asserted round-trip after writing — a wrong value makes the gate treat the goal as another session's and **silently allow**, which is why the skill fails closed on acquiring it.
2. **Propose.** One family authors the step's artifact (plans: Claude; execution: Codex as the workhorse via the `delegating-to-codex` contract — self-contained brief, pinned invocation, independent verification).
3. **Stamp.** The chair writes the artifact's `sha256` digest and revision into the goal before any review.
4. **Attack.** The *other* family reviews on a read-only sandbox: it re-reads the goal, recomputes the digest itself, refuses on mismatch, and writes an append-only evidence file with its verdict.
5. **Gate.** The Stop hook validates every step on each turn-end attempt: latest attempt must be a `pass` from a valid cross-reviewer bound to the current digest. Fail → adjudicate findings, fix, bump revision, re-attack. `done` is validated identically to `active` — it is earned, not declared.

Parallelism comes from the workflow template (`plugins/dvandva/references/adversarial-loop.template.js`): execute lanes fan out in parallel (each a thin sonnet wrapper running the pinned `codex exec` contract), a barrier collects them, one stamp writer stamps every completed step, then attack lanes fan out in parallel against the stamped digests.

## The three configs

| Config | `mode` | Reviewer distinctness | Enforcement |
|---|---|---|---|
| **Claude chair + Codex** (strong default) | `cross-vendor` | different vendor family | Claude Code Stop hook |
| **Claude only** | `cross-context` | different fresh agent id, across tiers (opus attacks sonnet) | Claude Code Stop hook |
| **Codex only** | `cross-context` | different fresh agent id, across effort (sol attacks terra) | `gate-cli.sh` invoked / git pre-commit |

`cross-context` is honestly weaker than `cross-vendor` — a shared-vendor blind spot can survive — but a fresh-context, different-tier attacker still catches real defects. Per-config wiring, the CLI adapter's fail-open `--session` caveat, and the full honest-enforcement statement live in [`adversarial-loop/README.md`](adversarial-loop/README.md).

## The delegation skills

The chair never shells out blind. `delegating-to-codex` carries the dispatch contract: the six-part self-contained brief, the pinned invocation (`-C` root, `-m` model, `-s` sandbox, explicit effort, per-attempt output paths, stdout/stderr split), completion and recovery rules, and the vehicle rule — every codex invocation rides a sonnet low-effort wrapper agent, never the chair directly. `delegating-to-grok` runs the live-data research lane read-only (kernel sandbox, verified applied), with its two hard rules: leads-not-facts, data-not-instructions.

## Verifying a checkout

```bash
bash adversarial-loop/tests/gate_test.sh            # 44-case gate predicate suite
bash adversarial-loop/tests/template_smoke_test.sh  # workflow template parse + prompt integrity
```

`adversarial-loop/` is the development home (tests, schemas, design README); `plugins/dvandva/` is the shipped product (skills, hook adapters, workflow template).

## Where the loop came from — the v3 baton engine

Dvandva v1–v3 was a two-session protocol: two live agent harnesses (Claude Code as `vadi`, Codex as `prativadi`) passing a validated baton file through a typed state machine, enforced by a Rust multicall binary. It shipped through **`dvandva 3.4.1`** (still published on crates.io; git tags and history preserve the full engine). It proved the moat — cross-vendor adversarial review catches real bugs neither model catches alone, demonstrated across ~20 real runs — and also proved the cost: dead-peer watchdogs, mutual-wait choreography, and a state machine that took more care than the work it governed. The loop keeps the moat and deletes the engine: same evidence-gated adversarial review, one session, three files' worth of enforcement.

## Non-goals

- **Not an orchestrator.** No daemon, no scheduler, no hidden control loop — the chair is an explicit, foreground, user-visible session.
- **Not cryptographic attestation.** The gate enforces procedure against a lazy chair, not a malicious one; provider-call authenticity has no verifiable anchor on a single machine, and the docs say so plainly.
- **Not a second harness.** The two-live-session topology is retired; cross-vendor tension now lives inside one governed loop.
