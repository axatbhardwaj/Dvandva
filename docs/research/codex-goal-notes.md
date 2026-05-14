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

- Codex can participate as vadi or prativadi in a persistent walkaway session when goals are available.
- The workflow should still work manually if goal support is missing, but full walkaway requires a session that can keep following the baton.
- The baton file is the stable protocol; `/goal` is just one runner.

## Reviewer Goal Pattern

Codex should review when the baton assigns its current role and block in the wait helper when assigned away.

Recommended condition:

```text
/goal Continue the Dvandva walkaway run until .dvandva/baton.json status is "done", "human_question", or "human_decision". If assignee is not your current role, run scripts/dvandva-wait.sh --role <role> --interval 60 --max-wait 900, then re-read the baton when it returns 0. Before each checkpoint, surface BATON_STATE, verification commands and outcomes, findings, and final approval fields. Never create a PR.
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
