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

Dvandva goal conditions should therefore include:

```text
Work until .dvandva/baton.json has assignee "codex" or status "done".
Before stopping, read the baton back into the transcript, list changed files, list verification commands and outcomes, and do not modify files outside the requested scope.
Stop after 20 turns and set status "human_decision" if blocked.
```

## Why It Replaces Polling

Claude's docs compare `/goal`, `/loop`, and Stop hooks:

- `/goal` starts the next turn after the previous turn finishes and stops when a model confirms the condition.
- `/loop` starts the next turn on a time interval.
- Stop hooks can be deterministic or model-evaluated and live in settings.

For Dvandva, `/goal` is better than a timer because the target is a state transition, not elapsed time.

## Open Questions

- Whether Claude Code goal behavior is stable enough to use in a public workflow without marking it experimental.
- Whether a future file-watch runner should start the other agent automatically after a baton transition.
- Whether the best reusable implementation is a Claude skill, a shell wrapper, or a tiny local orchestrator.

