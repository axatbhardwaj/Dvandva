# Claude Code Goal Research

Research date: 2026-05-12.

Primary source: https://code.claude.com/docs/en/goal

## Findings

Claude Code has an official `/goal` command for autonomous progress toward a completion condition.

Key properties from the docs:

- `/goal` sets a session-scoped completion condition.
- A separate evaluator checks the condition after each turn.
- If the condition is not met, Claude starts another turn instead of returning control.
- One goal can be active per session.
- Running `/goal` with no argument shows status.
- `/goal clear` stops an active goal.
- A still-active goal is restored when resuming a session with `--resume` or `--continue`.
- It works non-interactively with `claude -p "/goal ..."` and runs the loop to completion in one invocation.

## Important Constraint

The evaluator does not run tools or read files independently. It judges based on what Claude has surfaced in the conversation.

Implication for Dvandva:

- Goal prompts must require Claude to print or summarize the verification evidence.
- "The baton file has assignee codex" is not enough unless Claude reads it and surfaces the result.
- Verification commands and their outcomes must be visible in the transcript before the evaluator can judge completion.

## Effective Goal Shape

The Claude docs recommend conditions with:

- One measurable end state.
- A stated check.
- Constraints that must hold.
- Optional turn or time bound.

Dvandva's current walkaway goal conditions should therefore include:

```text
Continue until .dvandva/baton.json reaches post-handshake status "done" or enters human-intervention status "human_question" or "human_decision".
If the baton assigns work to the other role, run ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh in the foreground and re-read the baton when it returns ready.
Do not treat final approval as a stop condition; termination_review is an active shared handoff where both roles keep polling until both approve stopping.
Before each checkpoint, surface BATON_STATE_COMPACT via dvandva-state.sh --compact, changed files, verification commands and outcomes, and final approval fields.
Never create a PR. Stop after the baton turn_cap and set status "human_decision" if blocked.
```

## Why It Replaces Polling

Claude's docs compare `/goal`, `/loop`, and Stop hooks:

- `/goal` starts the next turn after the previous turn finishes and stops when a model confirms the condition.
- `/loop` starts the next turn on a time interval.
- Stop hooks can be deterministic or model-evaluated and live in settings.

For Dvandva, `/goal` should supervise work and checkpoint surfacing, while `scripts/dvandva-wait.sh` handles cheap waiting when the baton belongs to the other role. Model turns should not be spent polling elapsed time.

## Open Questions

- Whether Claude Code goal behavior is stable enough to use in a public workflow without marking it experimental.
- Whether a future file-watch runner should start the other agent automatically after a baton transition.
- Whether the best reusable implementation is a Claude skill, a shell wrapper, or a tiny local orchestrator.
