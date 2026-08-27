# Vadi run contract

The facade JSON is authoritative. Use only `scripts/dvandva-role.sh`; first
`probe`, then resolve one stable session with `session-id` or one retained
`session-id --generate` fallback.

```text
start SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE [OBJECTIVE [TASK]] [--objective-ref KIND=VALUE] [--required-deliverable ID=DESCRIPTION] [--wait|--new-run|--run-id ID]
read  SESSION RUN_DIR
apply SESSION RUN_DIR EXPECTED_REVISION ACTION_JSON
wait  SESSION RUN_DIR AFTER_REVISION [TIMEOUT_MS]
heartbeat SESSION RUN_DIR EXPECTED_REVISION
```

## Start and snapshot contract

New runs require the human's objective and every required deliverable. Exact
joins pass only `--run-id` unless the human explicitly supplied objective,
reference, task, or deliverable coordinates to compare. Never invent an
objective for an exact join. Exact run ID selects state but never amends or
overrides scope: surface `scope_mismatch` without claiming or working.

Surface `ambiguous`, `busy`, `run_missing`, and `upgrade_required` rather than
guessing. Use `--new-run` only on an explicit request for a separate run.
Vadi's first user-visible protocol output includes returned run ID, canonical
objective and scope, status and assignee, `next_actions`, and exact
`peer_prompt` before domain-tool work.

After every facade operation, use the fresh facade snapshot. `next_actions`
combines `advisory_actions` and ordinary `legal_actions`; semantic work happens
only when the returned advisory action authorizes it. Apply only a returned
legal action. `request_human_decision` may be selected directly from
`legal_actions` solely for new human scope or ambiguity; it is never an
ordinary wake or action.

## Checkpoint and review bindings

A checkpoint contains one complete deliverable manifest covering the canonical
deliverable IDs exactly once, plus non-empty verification. Use immutable
artifacts, not a branch or mutable `HEAD`:

```json
{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"<immutable SHA>","deliverables":[{"id":"<canonical ID>","artifacts":[{"kind":"commit","value":"<immutable SHA>"}]}],"verification":["<exact command and result>"]}}
```

Each review binds checkpoint identity, `manifest_digest`, and `scope_revision`:

```json
{"type":"record_review","verdict":"approved","checkpoint_identity":"<checkpoint.identity>","manifest_digest":"<checkpoint.manifest_digest>","scope_revision":1,"findings":[]}
```

In `reviewing`, newly discovered work uses
`request_checkpoint_supersession`; the reviewer uses
`accept_checkpoint_supersession`. After approval, new work uses
`withdraw_approval`.

Publication never substitutes for supersession or withdrawal.

```json
{"type":"request_checkpoint_supersession","reason":"<new required work>"}
{"type":"accept_checkpoint_supersession"}
{"type":"withdraw_approval","reason":"<new required work>"}
```

## Human Decision

Use only the minimal request. The kernel derives contact and resume routing:

```json
{"type":"request_human_decision","question":"<one decision>","evidence":["<verified fact>"],"options":["<concrete option>"]}
```

## Explainer obligation

At every semantic handoff, the Codex harness publishes or updates one
owner-only Codex Site and the Claude harness reviews that exact deployment,
regardless of semantic casting. The explainer carries this exact content:
canonical scope, complete manifest, findings and decisions, and a current plan/TODO.
Reuse one stable Site ID for the run and record a new Site version for each
obligation.

The Codex participant copies the exact current obligation and records:

```json
{"type":"record_explainer_publication","obligation":{"handoff_revision":12,"kind":"worker_to_reviewer","scope_revision":1,"checkpoint":{"identity":"<checkpoint.identity>","manifest_digest":"<checkpoint.manifest_digest>","scope_revision":1}},"source_digest":"<64 lowercase hex>","site_id":"<stable run Site ID>","site_version":"<new version>","url":"<exact deployment URL>","channel":"codex_sites","access":"owner_only"}
```

The Claude participant binds review to that obligation and deployment:

```json
{"type":"record_explainer_review","obligation":{"handoff_revision":12,"kind":"worker_to_reviewer","scope_revision":1,"checkpoint":{"identity":"<checkpoint.identity>","manifest_digest":"<checkpoint.manifest_digest>","scope_revision":1}},"source_digest":"<deployment.source_digest>","site_id":"<deployment.site_id>","site_version":"<deployment.site_version>","url":"<deployment.url>","verdict":"approved","findings":[]}
```

A Claude Artifact, generic publisher, public access, or silent fallback cannot
satisfy the gate. Missing Sites or exact review capability routes to Human
Decision and leaves the run blocked.

## Run boundaries and handoff

The human starts the peer session with the returned prompt. Neither role
invokes or wakes the other harness. User-created harness goals remain
unchanged. Third-party and explicit-only skills, including Matt Pocock's
skills, run only when the human explicitly invokes them in this session.

After each handoff, report these exact fields and continue in a foreground
local wait until terminal state or human stop:

- What changed
- What was verified
- What is blocked
- Who owns the next action
- Exact command or prompt

On timeout, read a fresh snapshot and wait again. Heartbeat before long
authorized work. Keep action files private (mode 0600), exclude credentials,
and delete them after `apply`.
