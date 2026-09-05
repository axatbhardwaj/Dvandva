# Active v4 model casting

This is the user-selected workflow policy, not a model-performance benchmark.
Discovery and implementation use separate sessions.

## Discovery drivers

For `workflow=discovery`, Claude Fable 5.1 at high is vadi and Codex
Astra (`gpt-6-astra`) at high is prativadi. Verify the actual host catalog;
never infer model identity from a prompt. Spec and ticket runs reuse these
sessions and keep separate run identities. Both investigate independently
through startup scope verification before comparing conclusions. Each driver
owns its role's explainer work at its selected reasoning level. The Sol/Opus
HTML and implementation casting below applies to implementation, babysitting
and review, not discovery. Required model unavailability has no silent fallback.

Matt's user-only entry points remain human-invoked in the owning session:
`/grill-with-docs`, `/to-spec`, `/to-tickets`, then `/implement` in the fresh
Codex implementation session. No wrapper automatically invokes those skills.
Read initiation.md and discovery.md for the paired planning lifecycle.

## Planning handoff

Use Astra (`gpt-6-astra`) and Fable for planning and judgment with the human's
chosen planning skills. Save the approved scope, decisions, acceptance criteria,
relevant files, and verification commands as a concise plan. The implementation
pair reads that plan and verifies it against canonical scope; it does not need
the entire planning transcript. Human-invoked Matt Pocock skills remain
human-invoked; Dvandva does not automatically rerun the planning process.

## Implementation and review drivers

- **Codex vadi: `gpt-5.6-sol` with `high` reasoning.** Sol implements, fixes,
  and verifies directly. HTML authoring uses `gpt-5.6-sol` at `medium` reasoning.
  If the parent is running at high, dispatch a bounded native Sol/medium HTML
  task and integrate its returned bytes; do not merely prompt it to act medium.
  Delegate other bounded independent work only when useful:
  that earns its extra context and coordination cost; a second Sol instance is
  not required just to execute the parent's task.
- **Claude prativadi: Opus.** Opus independently reviews only the exact immutable
  delivery checkpoint authorized by `review_checkpoint`, and owns the verdict.
  Run the required Git `code-review` companion once inside the Opus review
  station for each newly authorized candidate, using native local subagents if
  the companion requires them. If unavailable, the documented native review
  fallback is still Opus; do not duplicate the same review in another parent.

Astra and Fable are optional advisers during implementation, not mandatory
chairs. Ask the locally available adviser only for a concrete design ambiguity,
a disputed interpretation, or a necessary plan revision. Give it the relevant
plan excerpt and evidence, not the whole transcript. Its advice never replaces
Opus review or grants human authority. An unavailable optional adviser does not
block ordinary implementation or review.

## Authorization and isolation

Discovery follows the Discovery drivers above, including authoring and review
of explainers. The remaining authorization examples describe implementation.
Select models from the actual host catalog. Use Sol at `high` for general
implementation and `medium` for HTML authoring.
Prompt personas do not prove model identity. Required Sol/Opus drivers have no
silent fallback; report unavailable required capability and preserve the run
until the user selects an alternative or that capability returns. Capability
unavailability is not a Human Decision.

Dispatch semantic work only when the current facade snapshot authorizes it.
`work` authorizes scoped implementation. `stage_explainer` authorizes Sol to
author or revise the status HTML, including the initial pre-join page before
`work` becomes available. `review_explainer` authorizes Opus to read and inspect
those exact staged bytes and their rendering. Use the installed
`html-deliverables` skill for both authoring and visual review. Codex publishes
only the approved digest through the existing Sites gate.

Only the vadi or prativadi parent may call the facade, mutate Baton state, or
mutate GitHub. Subagents return scoped work or evidence to that parent. The
human starts both role sessions; neither session nor a subagent invokes, wakes,
or controls the other harness. Optional Astra advice stays on the Codex side;
optional Fable advice stays on the Claude side.

The user may explicitly override or reverse the casting when each selected
harness natively supports its assigned models and the same isolation and review
authority hold. Never invent access or relabel another model's review as Opus.
