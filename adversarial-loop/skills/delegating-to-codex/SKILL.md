---
name: delegating-to-codex
description: Use when dispatching work to Codex/GPT via `codex exec` — implementation, tests, migrations, log digging, bounded hard problems, long analysis — or when a codex dispatch hangs, returns early, or claims completion that isn't on disk.
---

# Delegating to Codex

## Overview

Codex is the workhorse executor: the chair keeps judgment, Codex takes volume. A dispatch succeeds or fails on three things — the right **vehicle**, a self-contained **brief**, and a pinned **invocation**. Nearly every observed failure was one of those, not the model.

## The vehicle: always a sonnet wrapper, never direct

The chair never runs `codex exec` itself. Every codex invocation rides inside a **sonnet low-effort wrapper agent** — a standalone agent for a one-off dispatch, a workflow `agent()` lane for parallel fan-out. The wrapper is thin: assemble the brief, run the pinned invocation below, verify ground truth on disk, return the result verbatim. This keeps every dispatch visible as an agent in the harness and gives one consistent vehicle at both call sites.

For a run that can exceed the ~10-minute shell cap, the wrapper holds it with its **own background-Bash + completion notification — never a sleep-poll loop** (polling wrappers return early; every observed "agent finished but codex still running" incident was a poll loop giving up).

## Casting: model, effort, sandbox

| Task | `$MODEL` | `$EFFORT` | `$SANDBOX` |
|---|---|---|---|
| Routine implementation, docs | `gpt-5.6-terra` | `xhigh` | `workspace-write` |
| Hard bounded problems | `gpt-5.6-sol` | `xhigh` | `workspace-write` |
| Review-grade attack, log digging, analysis | `gpt-5.6-sol` / `terra` | `xhigh` | **`read-only`** |
| Proven-mechanical task classes only | `gpt-5.6-luna` | `high`/`medium`/`low` | per task |
| 5.6 unavailable on this surface | `gpt-5.5` | `xhigh` | per task |

- **Reviewers and analysts get `read-only`.** A reviewer must not be able to mutate the bytes it inspects — prompt "boundaries" are advisory; the sandbox is the control.
- The chair's own main thread runs `xhigh`. Every dispatched child gets an **explicit** `$EFFORT` from this table — omitting it falls back to whatever Codex's loaded config or model default specifies (this machine's `config.toml` sets `ultra`; exactly why you never omit it). Children never request `max` (only the human sets `max`, and only on the session's own main thread); `ultra` is never used.
- A fallback to a different model changes review credit — if the model you recorded isn't the one that ran, re-adjudicate.

## The brief

Codex has none of your conversation. A complete brief IS, in order:

1. **Goal + acceptance criteria** — what done looks like, concretely.
2. **Exact paths** — files to read, files to write.
3. **Decisions already made**, with the why — so it doesn't relitigate them.
4. **Boundaries** — paths it must not touch; whether it may commit (default: no).
5. **Verification to run** — commands + expected results. If the task didn't state them, derive them from the repo yourself; never leave this slot empty.
6. **Output contract** — what to deliver where, and it must be **writable under the sandbox you chose**: for a `read-only` dispatch the deliverable is the final message / `-o` file, never a file the agent writes itself.

## The invocation (pinned)

```bash
ATT="/tmp/codex-attempts/${GOAL}-${STEP}-r${REV}-a${N}"  # N increments on EVERY dispatch,
mkdir -p "$ATT"                                          # retry, and resume — never reuse
git -C "$REPO_ROOT" status --short > "$ATT/pre.status"
timeout --kill-after=10 600 codex exec \
  -C "$REPO_ROOT" \
  -m "$MODEL" -s "$SANDBOX" \
  -c "model_reasoning_effort=$EFFORT" \
  --json -o "$ATT/last-message.md" \
  "$(cat "$ATT/brief.md")" </dev/null \
  > "$ATT/events.jsonl" 2> "$ATT/stderr.log"
echo "EXIT:$?" > "$ATT/exit"
```

Pin the five load-bearing settings every dispatch — `-C` working root (relative paths mean nothing without it), `-m` model, `-s` sandbox, `-c` effort, and fresh per-attempt output paths. Be honest about the rest: the process still loads `~/.codex/config.toml`, project instructions, rules, MCP servers, and hooks — those remain trusted inputs, not isolated ones.

- `</dev/null` — an open stdin hangs on EOF.
- **stdout and stderr stay separate.** `--json` makes *stdout* JSONL; the CLI emits warnings on stderr, and merging them corrupts the event stream your completion check parses.
- `events.jsonl` holds the terminal event and the session uuid recovery needs; `stderr.log` holds the warnings.

## Completion & recovery

- **Complete** = exit 0 AND `turn.completed` in `events.jsonl` AND non-empty `last-message.md`. Any `turn.failed` or error event, a nonzero exit, or empty output = failed attempt.
- **Fallback without `turn.completed`** — only for a CLI build you have verified doesn't emit it (record the build + the absence): exit 0 AND non-empty `-o` AND the expected artifacts **changed versus `pre.status`** AND the brief's verification commands pass. Pre-existing files satisfying "exists" is not completion.
- **Timeout/kill = terminal failed attempt.** Confirm the process is dead first, then either
  (a) recover: allocate a **new attempt dir** and run
  ```bash
  codex exec resume <exact-uuid> \
    -c "model_reasoning_effort=$EFFORT" \
    --json -o "$ATT/last-message.md" \
    "<explicit continuation prompt>" </dev/null \
    > "$ATT/events.jsonl" 2> "$ATT/stderr.log"
  ```
  Resume starts a **new turn** in the old session — it is not an attach or flush for a live process. It does not accept `-s`; the original sandbox/model are assumed to carry over but that inheritance is not independently verifiable — when sandbox certainty matters, respawn fresh instead. Never `resume --last` (it grabs the newest session, whosever that is); or
  (b) respawn fresh with a longer budget, in the wrapper's background-Bash.
- Before any respawn: diff `git -C "$REPO_ROOT" status --short` against `pre.status`. Investigate unexplained deltas; never clean or retry over them.

## After it returns

Never accept the self-report. Rerun the verification commands yourself and check the boundaries held. Know what the status baseline can and cannot show: it catches **new** paths and status transitions, not further edits to a file that was already dirty — so any **mutating dispatch into a dirty tree gets an isolated worktree** (not only parallel ones). An **execution artifact** (code, plan, migration, research deliverable) then needs cross-family review per the adversarial-loop skill. A **reviewer's report is terminal**: the chair adjudicates it — it does not recursively open a new credited review step unless you deliberately add one to the goal.

## Preflight

`command -v codex`, authenticated, requested model available — codex errors fast on all three; record the requested model and CLI version with the attempt. Bounded retries: one respawn, then surface to the human.

## Common mistakes

| Mistake | Reality |
|---|---|
| Chair runs `codex exec` directly | always the sonnet-low wrapper — visibility + one vehicle everywhere |
| Wrapper sleep-polls a long run | background-Bash + completion notification; poll loops return early |
| Omitting `-m` / `-s` / `-c effort` / `-C` | ambient config decides silently (this machine's default effort is `ultra`) |
| Reviewer dispatched with `workspace-write` | it can edit the bytes it reviews; review/analysis = `read-only` |
| `2>&1` into the events file | stderr warnings corrupt the JSONL your completion check parses |
| Output contract conflicts with sandbox | a read-only agent can't write your findings file; deliverable = final message / `-o` |
| "exit 0 = done" | require terminal event + non-empty `-o`; fallback needs changed-vs-baseline + passing verification |
| Reusing the attempt dir on retry/resume | stale non-empty output passes the check; `N` increments every dispatch |
| `resume` to monitor or flush a live run | resume = recovery after confirmed death, with an explicit prompt |
| `resume --last` | races other sessions; exact uuid only |
| Thin brief ("fix the tests") | codex guesses scope; six parts, every brief |
| Accepting "all green" | rerun verification yourself; dirty-tree mutations go to a worktree |
