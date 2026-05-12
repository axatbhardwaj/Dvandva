# Codex Goal Notes

Research date: 2026-05-12.

## Local Verification

On this machine, `codex features list` reports:

```text
goals experimental true
```

This matches the PR 353 observation that Codex `/goal` was useful once enabled.

## Current Treatment

Treat Codex goals as available on this machine but experimental for a public workflow.

For public Dvandva docs, the safe phrasing is:

- Codex can be launched as a reviewer/fixer in an autonomous goal-style loop when goals are available.
- The workflow should still work manually if goal support is missing.
- The baton file is the stable protocol; `/goal` is just one runner.

## Reviewer Goal Pattern

Codex should review until the baton state no longer assigns review work to Codex.

Recommended condition:

```text
/goal Review the current branch using .dvandva/baton.json as the handoff. Apply only narrow fixups. Stop when the baton has assignee "claude", assignee "human", or status "done". Before stopping, surface verification commands and write .dvandva/codex-review.md.
```

## Fix Permissions

Codex may directly fix:

- Formatting.
- Lint or type errors with obvious one-line fixes.
- Broken docs references.
- Missing audit-row updates after the implementation already changed the source.
- Small test expectation mistakes when behavior is clearly unchanged.

Codex should not directly fix:

- Architecture.
- Schema migrations.
- Shared infra.
- Dependency removals.
- Product behavior changes.
- Anything that requires guessing user intent.

