# Prativadi run contract

The facade JSON is authoritative. Use only `scripts/dvandva-role.sh`; first
`probe`, then resolve one stable session with `session-id` or one retained
`session-id --generate` fallback.

```text
start SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE [OBJECTIVE [TASK]] [--objective-ref KIND=VALUE] [--required-deliverable ID=DESCRIPTION] [--wait|--new-run|--run-id ID]
read  SESSION RUN_DIR
apply SESSION RUN_DIR EXPECTED_REVISION ACTION_JSON
wait  SESSION RUN_DIR AFTER_REVISION [TIMEOUT_MS]
heartbeat SESSION RUN_DIR EXPECTED_REVISION
upgrade SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION
claim SESSION RUN_DIR EXPECTED_REVISION
reclaim SESSION RUN_DIR EXPECTED_REVISION
```

## Start and snapshot contract

Exact joins pass only `--run-id` unless the human explicitly supplied objective,
reference, task, or deliverable coordinates to compare. Never
invent an objective for an exact join. Exact run ID selects state but never
amends or overrides scope: surface `scope_mismatch` without claiming or
working. Without an exact run, use `--wait`; surface multiple matches for human
selection rather than choosing newest.

Prativadi never creates a run. If requested scope belongs in a separate run,
return that choice to the human so vadi/worker can create it.

Surface `ambiguous`, `busy`, `run_missing`, and `upgrade_required` rather than
guessing. Surface the start outcome and canonical snapshot before domain-tool
work.

For `upgrade_required`, run `upgrade` with the exact returned run directory,
harnesses, and revision. Upgrade clears claims: use its returned revision with
`claim`, then read a fresh v2 snapshot. Use `reclaim` only when a later facade
snapshot reports this role's claim expired. Never route migration through an
ordinary action payload.

After every facade operation, use the fresh facade snapshot. `next_actions`
combines `advisory_actions` and ordinary `legal_actions`; semantic work happens
only when the returned advisory action authorizes it. Apply only a returned
legal action. `request_human_decision` may be selected directly from
`legal_actions` solely for new human scope or ambiguity; it is never an
ordinary wake or action.

## Checkpoint and review bindings

Review only when `advisory_actions` includes `review_checkpoint`. Materialize
the exact immutable checkpoint, whose complete deliverable manifest covers the
canonical deliverable IDs exactly once. Never review branch `HEAD` or the
vadi's mutable worktree. A submission has this v2 shape:

```json
{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"<immutable SHA>","deliverables":[{"id":"<canonical ID>","artifacts":[{"kind":"commit","value":"<immutable SHA>"}]}],"verification":["<exact command and result>"]}}
```

Before a verdict, read or claim a fresh snapshot, then copy the exact current `checkpoint` coordinates.
Never type, increment, or reuse them from an older snapshot. Bind every verdict
to all three and discard a stale verdict:

```json
{"type":"record_review","verdict":"changes_requested","checkpoint_identity":"<snapshot.checkpoint.identity>","manifest_digest":"<snapshot.checkpoint.manifest_digest>","scope_revision":"<snapshot.checkpoint.scope_revision>","findings":["<actionable finding>"]}
```

In `reviewing`, newly discovered work uses
`request_checkpoint_supersession`; when returned, the reviewer uses
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
{"type":"request_human_decision","question":"<one decision>","evidence":["<verified fact>"],"options":["<concrete option A>","<concrete option B>"]}
{"type":"resume_human_decision","answer":"<human answer>"}
```

`answer_human` maps to `resume_human_decision`; copy the human's answer. If the
human changes scope, include the exact human-approved `scope_amendment` shape
returned by that decision instead of silently changing scope.

## Explainer obligation

At every semantic handoff, the Codex harness publishes or updates one
owner-only Codex Site and the Claude harness reviews that exact deployment,
regardless of semantic casting. The explainer carries this exact content:
canonical scope, complete manifest, findings and decisions, and a current plan/TODO.
Reuse one stable Site ID for the run and record a new Site version for each
obligation.

For `publish_explainer`, copy `publication_binding.obligation` unchanged from
the fresh snapshot. Preserve `publication_binding.site_id` when non-null; on
the first deployment, create the run's stable Site ID. Compute the deployed
source digest and record the new Site version and exact resulting URL:

```json
{"type":"record_explainer_publication","obligation":"<snapshot.publication_binding.obligation>","source_digest":"<64 lowercase hex>","site_id":"<stable run Site ID>","site_version":"<new version>","url":"<exact deployment URL>","channel":"codex_sites","access":"owner_only"}
```

For `review_explainer`, copy the same obligation unchanged and copy every
deployment coordinate from `publication_binding.deployment` in the fresh
snapshot:

```json
{"type":"record_explainer_review","obligation":"<snapshot.publication_binding.obligation>","source_digest":"<snapshot.publication_binding.deployment.source_digest>","site_id":"<snapshot.publication_binding.deployment.site_id>","site_version":"<snapshot.publication_binding.deployment.site_version>","url":"<snapshot.publication_binding.deployment.url>","verdict":"approved","findings":[]}
```

A Claude Artifact, generic publisher, public access, or silent fallback cannot
satisfy the gate. Missing Sites or exact review capability routes to Human
Decision and leaves the run blocked.

`finalize` maps directly to:

```json
{"type":"finalize"}
```

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
