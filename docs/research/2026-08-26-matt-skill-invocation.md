# Matt Pocock skill invocation in a two-lane Dvandva run

Date: 2026-08-26

## Conclusion

The default two-lane run is technically feasible, but Dvandva must not depend
on either model automatically choosing Matt Pocock's top-level workflow skills.

The installed workflow entry points are intentionally **explicit-only** in
both harnesses. Dvandva should therefore use a small, non-model session adapter
to submit an explicit skill turn to the already-running Claude or Codex
session. This removes the need for the human to type every command while
preserving the rule that neither harness launches or invokes the other. It is,
however, a deliberate Dvandva extension of Matt's literal convention that only
a human typing may fire a user-invoked skill; it must not be presented as
upstream behavior.

Merely mentioning a skill in an assistant message, handoff document, or baton
does **not** invoke it. The mention must become a new host input:

- Claude Agent SDK: a prompt whose command is `/<skill>`.
- Codex app server: `turn/start` with `$<skill>` in the text and a structured
  `skill` input item.

## What is explicit-only in the installed snapshot

Matt's own invocation contract defines user-invoked skills as unreachable by
the model: Claude uses `disable-model-invocation: true`, while Codex uses
`policy.allow_implicit_invocation: false`. It also says a user-invoked skill may
call model-invoked skills, but cannot call another user-invoked skill.
([upstream invocation contract](https://github.com/mattpocock/skills/blob/main/.agents/invocation.md))

The local installation matches that contract:

| Default-run skill | Installed policy | Dvandva treatment |
| --- | --- | --- |
| `grill-with-docs` | Explicit-only in Claude and Codex | Session adapter dispatches it to Claude |
| `to-spec` | Explicit-only in Claude and Codex | Session adapter dispatches it to Claude |
| `to-tickets` | Explicit-only in Claude and Codex | Session adapter dispatches it to Claude |
| `implement` | Explicit-only in Claude and Codex | Session adapter dispatches it once per assigned track to either lane |
| `ask-matt`, `handoff`, `loop-me` | Explicit-only in Claude and Codex | Optional/manual; not part of the default runtime loop |
| `implement-spec` | Explicit-only in Claude and Codex | Do not use inside a run; it duplicates Dvandva scheduling/integration |
| `claude-handoff` | Explicit-only and launches `claude --bg` | Prohibit inside a run; it violates harness independence |
| `grilling`, `domain-modeling`, `tdd`, `code-review`, `diagnosing-bugs`, `research`, `prototype`, `codebase-design` | Model-invocable by default | May be selected within the owning lane; Dvandva may still dispatch them explicitly when deterministic evidence is useful |

Installed evidence: [`grill-with-docs`](/home/xzat/.agents/skills/grill-with-docs/SKILL.md:4),
[`to-spec`](/home/xzat/.agents/skills/to-spec/SKILL.md:4),
[`to-tickets`](/home/xzat/.agents/skills/to-tickets/SKILL.md:4),
[`implement`](/home/xzat/.agents/skills/implement/SKILL.md:4), and their
adjacent `agents/openai.yaml` files all carry the paired explicit-only policy.
The installed `implement` workflow then calls model-invocable `tdd` and
`code-review` internally, which is permitted by Matt's dependency rule.

Claude's official documentation agrees with the metadata: skills are
model-invocable by default, while `disable-model-invocation: true` hides the
skill from Claude and reserves it for direct command dispatch.
([Claude Code skills](https://code.claude.com/docs/en/skills#control-who-invokes-a-skill))
OpenAI documents the equivalent Codex policy: implicit invocation is normally
allowed, and `allow_implicit_invocation: false` disables matching by
description while preserving explicit `$skill` invocation.
([OpenAI skill invocation](https://learn.chatgpt.com/docs/build-skills#how-chatgpt-and-codex-use-skills))

## Does an agent-authored prompt count?

There are two different cases:

1. **Assistant prose or a handoff artifact:** no. The current model cannot turn
   a suggestion such as "next use `implement`" into a Skill-tool call when the
   skill is explicit-only. Matt's contract intentionally forbids that path.
2. **A controller-submitted input turn:** yes, operationally. The Claude Agent
   SDK documents dispatching a user-invocable command by sending `/<name>` as
   the prompt, even when the skill is omitted from the model's skill allowlist.
   ([Claude Agent SDK command dispatch](https://code.claude.com/docs/en/agent-sdk/skills#dispatch-commands-by-name))
   Codex app server documents the analogous programmatic `turn/start` request
   using `$<skill>` plus a structured `skill` input item; the structured item is
   recommended because it injects the exact skill rather than asking the model
   to resolve a name.
   ([Codex app-server skill invocation](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md#example-start-a-turn-invoke-a-skill))

The second case is not implicit model invocation. It is explicit dispatch by
the application acting as the session's user-side controller. Dvandva should
only permit it after the human authorizes the run and only for the skill and
scope recorded in the Run Plan.

## Recommended Dvandva mechanism

Add a narrow `LaneSession` adapter owned by the T3/Dvandva integration, not by
either model:

```text
dispatch_skill(lane_session, skill_name, arguments_ref, dispatch_id)
```

- For Claude, submit `/<skill> <arguments>` through the existing Claude Agent
  SDK session.
- For Codex, submit `$<skill> <arguments>` to the existing app-server thread and
  include the exact `skill` item returned by `skills/list`.
- Never create or resume the opposite harness from inside a lane.
- Record `dispatch_id`, lane, skill, arguments reference, and resulting
  checkpoint in the Run Channel and Run Site.
- Make dispatch idempotent: acknowledge a dispatch only after the host starts
  the skill turn; do not infer success from a handoff message.
- At join time, fail closed unless Claude's init `slash_commands` and Codex's
  `skills/list` contain every required explicit-only skill.

The default sequence becomes:

```text
Claude: /grill-with-docs -> /to-spec -> /to-tickets
Codex:  explicit plan review
Both:   /implement <assigned ticket> in parallel track worktrees
Cross:  explicit /code-review or $code-review at exact checkpoints
Codex:  integrate, verify, publish
```

`implement`'s same-family internal `code-review` remains a preflight. It does
not satisfy Dvandva's opposite-family adversarial-review gate.

## Approved design decision

Keep Matt's upstream invocation policies unchanged. Do not fork the skills to
make `grill-with-docs`, `to-spec`, `to-tickets`, or `implement` implicitly
model-invocable. Deterministic host-level dispatch is both more reliable and
safer: the Site can show exactly which skill Dvandva authorized, which lane ran
it, and which checkpoint it produced. The human grants one run-scoped Skill
Activation Lease at run start; genuine human checkpoints and all external-write
authority remain outside that lease. This extension is recorded in
`docs/adr/0002-delegated-explicit-skill-dispatch.md`.
