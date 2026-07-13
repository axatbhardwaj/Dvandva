---
name: adversarial-loop
description: Use when driving any non-trivial goal to completion with enforced adversarial review — the author never reviews its own work; a different model (ideally a different vendor) attacks every step, and the turn cannot end "done" until each step carries passing cross-reviewer evidence bound to the actual artifact. Invoke at the start of a goal ("build X", "fix Y end-to-end", "ship Z") to create the goal file the enforcement hook gates on, then follow it to run the propose→attack→gate loop via parallel workflows.
---

# Adversarial Loop

One rule generates the whole system: **the reviewer is never the author.** In the strong form the reviewer is a different *vendor* (Claude proposes, GPT/Codex attacks; GPT executes, Claude attacks). When only one vendor is available it degrades to a different *fresh-context agent across tiers* (opus attacks sonnet, sol attacks terra). Every step is `propose → attack → gate`, and a hook refuses to let the turn end "done" until each step carries passing cross-reviewer evidence bound to the actual artifact. If the goal isn't met, the loop isn't achieved.

This skill is the driver. The hook (`adversarial-loop/hooks/gate.sh`, or the `gate-cli.sh` adapter) is the gate. `goal.json` is the contract between them.

## When to use
Any goal worth reviewing: a feature, an end-to-end bug fix, a shippable change, a research/design pass. Not for throwaway one-liners or pure questions.

## Pick the mode (auto-detect the strongest available)

Set `mode` in `goal.json` by what's available:

| Config | `mode` | Reviewer distinctness | Reviewers dispatched |
|---|---|---|---|
| **Claude chair + Codex** (strong) | `cross-vendor` | different vendor family | Claude proposes/attacks; `codex exec` `gpt-5.6-sol/terra` attacks/executes (main executor, generous limits) |
| **Claude only** | `cross-context` | different fresh agent, across tiers | fresh Claude subagents: `opus` attacks `sonnet`'s work, a fresh `opus` attacks the plan |
| **Codex only** (Codex drives) | `cross-context` | different fresh agent, across effort | fresh Codex subagents: `sol` attacks `terra`'s work; enforce via the `gate-cli.sh` git-precommit adapter |

Detect: is `codex` on PATH and authenticated **and** is a Claude chair present? → `cross-vendor`. Only one vendor → `cross-context` with that vendor's fresh subagents. **Honest degradation:** `cross-context` is weaker than `cross-vendor` — a shared-vendor blind spot (both models share training/biases) can survive. It is still materially better than no adversarial review: a fresh-context, different-tier attacker catches real defects. `cross-vendor` is the recommended tier.

## The loop, per goal

### 1. Create the goal (fail-closed session binding)

Obtain the owner id from the environment — **do not guess it** (a wrong value makes the hook treat the goal as another session's and silently allow, disarming enforcement). The source differs by adapter:

```bash
# Claude Code Stop-hook mode (the hook reads session_id from stdin):
: "${CLAUDE_CODE_SESSION_ID:?adversarial-loop requires a Claude Code session id}"
OWNER=$CLAUDE_CODE_SESSION_ID

# Codex-only / CLI-adapter mode (gate-cli.sh reads ADVERSARIAL_LOOP_SESSION or --session):
# : "${ADVERSARIAL_LOOP_SESSION:?set ADVERSARIAL_LOOP_SESSION for the CLI adapter}"
# OWNER=$ADVERSARIAL_LOOP_SESSION   # and invoke every check as: gate-cli.sh --session "$OWNER"
```

Write `.adversarial-loop/goal.json` (gitignored) with `owner_session_id` = `$OWNER`, then **assert it round-tripped** (a typo, stale value, or failed write silently disarms the gate — this catches it):

```bash
jq -e --arg owner "$OWNER" '.owner_session_id == $owner' .adversarial-loop/goal.json >/dev/null \
  || { echo "owner_session_id round-trip mismatch — aborting" >&2; exit 1; }
```

Example (fields the hook checks):

```json
{
  "goal_id": "<^[a-z0-9][a-z0-9_-]{0,63}$, unique>",
  "owner_session_id": "<$OWNER — exact>",
  "mode": "cross-vendor",
  "status": "active",
  "acceptance": "<one line: what 'done' means, concretely>",
  "steps": [
    { "id": "plan",   "kind": "plan",    "author_family": "claude", "author_agent_id": "fable-chair", "revision": 1, "status": "pending", "artifact_path": "<plan file>", "artifact_digest": "" },
    { "id": "impl-a", "kind": "execute", "author_family": "gpt",    "author_agent_id": "codex-terra-1", "revision": 1, "status": "pending", "artifact_path": "<output>",   "artifact_digest": "" }
  ]
}
```

Rules the hook enforces, so honor them: `goal_id` **and every `steps[].id`** must match `^[a-z0-9][a-z0-9_-]{0,63}$` and be unique. Pending steps carry an empty `artifact_digest` (valid only while `pending`). **`author_family` is mode-dependent** (the hook does *not* couple `kind` to a family — you record who actually authored): in `cross-vendor`, split naturally (plan → `claude`, execute → `gpt`); in **claude-only** both are `claude`; in **codex-only** both are `gpt`. The reviewer distinctness comes from the mode: `cross-vendor` needs a different `reviewer_family`; `cross-context` (claude-only / codex-only) needs `author_agent_id` (required, non-empty) and a `reviewer_agent_id` that differs from it — that distinct dispatch id *is* the decorrelation.

### 2. Run each step: propose → **stamp** → attack → gate

1. **Author** the artifact. Plan: Claude authors. Execute: dispatch the GPT executor (hardened `codex exec` contract below) — or a fresh subagent in single-vendor mode. Write it to `artifact_path`.
2. **Stamp** the step in `goal.json` (single-writer): set `status:"complete"` and `artifact_digest = $(sha256sum -- "<artifact_path>" | cut -d" " -f1)`. Do this **before** attacking, and never edit the artifact after stamping (any change breaks the digest bind — re-stamp if you do).
3. **Attack** with the *other* vendor/agent, in parallel — a genuine adversary, not a rubber stamp. Claude-authored → dispatch `gpt-5.6-sol` via a direct `codex exec` shell-out (or a fresh `opus` in claude-only). GPT-authored → dispatch `opus` (+ optional `sonnet`/`grok`). **Reviewer preflight (mandatory):** before reviewing, the reviewer reads `goal.json`, requires exactly one matching step with `status:"complete"` and the expected `revision` and `artifact_path`, **recomputes `sha256sum -- <artifact_path>` itself**, and refuses if it differs from the stamped `artifact_digest` — so the evidence provably covers the bytes that were stamped. The evidence file records the digest the reviewer *observed*, never a chair-supplied value.
4. **Write evidence** — one append-only file per attack attempt at `.adversarial-loop/evidence/<goal_id>/<step_id>/attempt-<N>.json`, built with `jq -n` (not a hand-rolled heredoc), published atomically:

```json
{ "goal_id":"...", "step_id":"...", "step_revision":1,
  "artifact_digest":"<same sha256 the step was stamped with>",
  "reviewer_family":"gpt|claude", "reviewer_model":"gpt-5.6-sol|opus|...",
  "reviewer_agent_id":"<the dispatched reviewer's id — MUST differ from the step's author_agent_id>",
  "verdict":"pass|fail", "findings":[...], "transcript_ref":"<reviewer transcript>",
  "created_at":"<date -u +%Y-%m-%dT%H:%M:%SZ — fixed-width UTC Zulu, exact>" }
```

   The reviewer must differ from the author (family in `cross-vendor`, `reviewer_agent_id` in `cross-context`). `created_at` must match `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$` or it's rejected as malformed. Never overwrite a recorded `fail`; a later `pass` is a new attempt file.
5. **Gate**: if the latest attempt is `fail`, adjudicate the findings, fix the artifact, **bump the step `revision`, re-stamp the digest, re-attack** (a new attempt). Loop until the latest attempt passes.

### 3. Reach done

Set `goal.json` `status:"done"` only once the acceptance criterion is genuinely met. The hook validates `done` **identically to `active`** — it is allowed *only* when every step is `complete` with a latest-attempt `pass` from a valid cross-reviewer bound to the current digest. Setting `done` early does not shortcut the gate; it blocks. (`abandoned` is inert.)

## Hardened `codex exec` wrapper contract
Every GPT dispatch: foreground, `</dev/null` stdin (else EOF hang), explicit `-s <sandbox>` AND `-c model_reasoning_effort=<tier>` (never trust ambient `~/.codex/config.toml` = forbidden `ultra`+full-access), `--json` + `-o`. Main thread `xhigh`; children `xhigh` (down to `high`/`medium`/`low` for proven-mechanical; never `max` unless the human set it, never `ultra`). Completion = exit 0 AND a terminal event AND a non-empty `-o` — but some `codex-cli` builds don't emit `turn.completed`, so verify ground truth on disk. Deep analysis can exceed a 10-min Bash call → detached run or `codex exec resume <exact-uuid>` (never `--last`). A killed process is a terminal `failed` attempt; `timeout --kill-after` for group cleanup; `git status` reconcile before respawn. Use `sha256sum -- "$path"` (the `--` guards leading-dash paths).

## What is enforced, honestly
The hook fails **closed** on the *procedure*: per-step evidence must exist, be cross-reviewer, digest-bound, verdict-passing, latest-attempt-current, and `done` is allowed only when earned. Precisely: **a chair that omits required evidence cannot pass the gate** — up to Claude Code's 8-consecutive-block ceiling, which makes actually doing the review the path of least resistance. **Advisory, not enforced** (state this plainly — do not overclaim): the gate validates JSON shape, declared identity labels, digest/revision equality, and verdict — it **cannot establish that a real independent review occurred.** A chair could skip the provider call and fabricate structurally-valid passing evidence; provider-call authenticity and truthful agent ids remain procedural/advisory (no provider-verifiable anchor on a single machine). Threat model: a **lazy** chair, not a malicious one; a chair that edits/deletes `goal.json` or fabricates evidence to disarm the gate is out of scope, documented.

**Enforcement scope differs by adapter (don't overstate it):** the **Claude Stop-hook** (`gate.sh`) gates *turn termination* — Claude can't end the turn "done" without passing evidence. The **CLI adapter** (`gate-cli.sh`) is only an *invoked / git-pre-commit* gate — it evaluates when explicitly called and blocks that call or a commit, **not** a Codex turn ending. So in codex-only mode there is no turn-stop hook: require an explicit final `gate-cli.sh --session "$OWNER"` (and/or a pre-commit wiring) before claiming completion, and treat it as a procedural checkpoint, not an automatic turn gate.

## Common mistakes
| Mistake | Correction |
|---|---|
| Guessing / hardcoding `owner_session_id` | Use `CLAUDE_CODE_SESSION_ID` (or `gate-cli --session`), fail-closed, verify round-trip — a wrong value silently disarms the gate |
| Same reviewer as author | Reviewer's family (cross-vendor) or agent id (cross-context) must differ — the hook blocks self-review |
| Attacking before stamping the digest | Stamp `status:complete` + real digest first; the reviewer binds evidence to that digest |
| Setting `status:"done"` to finish faster | `done` is validated like `active` — it blocks unless the evidence earns it |
| Overwriting a failed attempt | Append-only; each attempt is a new file; the fail stays on record |
| Non-canonical `created_at` | Fixed-width UTC Zulu (`date -u +%Y-%m-%dT%H:%M:%SZ`) or evidence is rejected |
| Rubber-stamp reviews | The attacker must genuinely try to break the artifact |
| Treating the hook as an absolute wall | It's a bounded nudge (8 blocks); the guarantee is against a lazy chair, stated honestly |
