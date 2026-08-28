# Vadi run contract

The facade JSON is authoritative. Use only `scripts/dvandva-role.sh`; first
`probe`, then resolve one stable session with `session-id` or one retained
`session-id --generate` fallback.

```text
start SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE [OBJECTIVE [TASK]] [--objective-ref KIND=VALUE] [--required-deliverable ID=DESCRIPTION] [--wait|--new-run|--run-id ID]
read  SESSION RUN_DIR
apply SESSION RUN_DIR EXPECTED_REVISION ACTION_FILE
wait  SESSION RUN_DIR AFTER_REVISION [TIMEOUT_MS]
heartbeat SESSION RUN_DIR EXPECTED_REVISION
explainer SESSION RUN_DIR
upgrade SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION
repair-policy SESSION RUN_DIR EXPECTED_REVISION
claim SESSION RUN_DIR EXPECTED_REVISION
reclaim SESSION RUN_DIR EXPECTED_REVISION
```

## Start and snapshot contract

New runs require the human's objective and every required deliverable.
Exact joins pass only `--run-id` unless the human explicitly supplied objective,
reference, task, or deliverable coordinates to compare. Never invent an
objective for an exact join. Exact run ID selects state but never amends or
overrides scope: surface `scope_mismatch` without claiming or working.

Surface `ambiguous`, `busy`, `run_missing`, and `upgrade_required` rather than
guessing. Use `--new-run` only on an explicit request for a separate run.
Vadi's first user-visible protocol output includes returned run ID, canonical
objective and scope, status and assignee, `next_actions`, and exact
`peer_prompt` before domain-tool work.

For `publication_unreadable`, run `repair-policy` with the exact returned run
directory and revision; it installs the readable channel and clears the current
obligation's receipts so the publisher restages.

For `upgrade_required`, run `upgrade` with the exact returned run directory,
harnesses, and revision. Upgrade clears claims: use its returned revision with
`claim`, then read a fresh v2 snapshot. For ordinary expired-claim recovery,
exact `start --run-id` automatically reclaims a claim owned by the same
session. Reserve direct `reclaim` for a revision explicitly returned by the
facade, and follow it immediately with `read`. Never route migration through
an ordinary action payload.

After every facade operation, use the fresh facade snapshot. `next_actions`
combines `advisory_actions` and ordinary `legal_actions`; semantic work happens
only when the returned advisory action authorizes it. Apply only a returned
legal action. `request_human_decision` may be selected directly from
`legal_actions` solely for new human scope, ambiguity, or unavailable mandated
publication/review capability; it is never an ordinary wake or action.

## Checkpoint and worker mutations

For every `apply` action, the role writes its JSON to a private temporary file
with mode 0600, passes its path as `ACTION_FILE`, and deletes it after `apply`.

A checkpoint contains one complete deliverable manifest. It covers the
canonical deliverable IDs exactly once and includes non-empty verification.
Use immutable artifacts, not a branch or mutable `HEAD`. Checkpoints are typed:
`git` binds full-length commit object names, and `analysis` binds sha256 content
digests for deliverables that produce a review, audit, or finding rather than a
commit:

```json
{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"<full-length commit object name>","deliverables":[{"id":"<canonical ID>","artifacts":[{"kind":"commit","value":"<full-length commit object name>"}]}],"verification":["<exact command and result>"]}}
```

```json
{"type":"submit_checkpoint","checkpoint":{"kind":"analysis","identity":"<sha256 of the analysis>","deliverables":[{"id":"<canonical ID>","artifacts":[{"kind":"analysis_digest","value":"<sha256 of the artifact>"}]}],"verification":["<exact command and result>"]}}
```

Checkpoint submission never waits on the explainer. Only `finalize` is gated.

After submission, the reviewer owns the verdict and copies the exact current
`checkpoint` coordinates. Do not apply reviewer-owned `record_review` or
`accept_checkpoint_supersession` actions. In `reviewing`, request newly
discovered work with `request_checkpoint_supersession`; after approval, reopen
required work with `withdraw_approval`.

Publication never substitutes for supersession or withdrawal.

```json
{"type":"request_checkpoint_supersession","reason":"<new required work>"}
{"type":"withdraw_approval","reason":"<new required work>"}
```

## Human Decision

Use only the minimal request. The kernel derives contact and resume routing:

```json
{"type":"request_human_decision","question":"<one decision>","evidence":["<verified fact>"],"options":["<concrete option A>","<concrete option B>"]}
{"type":"resume_human_decision","answer":"<human answer>"}
```

`answer_human` maps to `resume_human_decision`; copy the human's answer. If the
human changes scope, populate every field below only from explicit
human-approved values. `task_reference` must be a JSON string or `null`; it is
not returned by the Human Decision object and must never be inferred:

```json
{"type":"resume_human_decision","answer":"<human-approved answer>","scope_amendment":{"objective":"<human-approved objective>","objective_refs":[{"kind":"<human-approved ref kind>","value":"<human-approved ref value>"}],"task_reference":"<human-approved task reference>","scope_deliverables":[{"id":"<human-approved deliverable ID>","description":"<human-approved deliverable description>"}]}}
```

## Explainer obligation

At every semantic handoff, the Codex harness stages the explainer's bytes into
the run directory and the Claude harness reviews those exact bytes,
regardless of semantic casting. Staging is first: the gate binds a digest, not a URL.
The explainer carries this exact content:
canonical scope, complete manifest, findings and decisions, and a current plan/TODO.

For `stage_explainer`, write the explainer HTML to a private path and
copy `publication_binding.obligation` unchanged from the fresh snapshot. The kernel
hashes the bytes, stores them at `explainer/<source_digest>.html` inside the run
directory, and binds that digest to the obligation. Staging different bytes
discards any earlier rendering and review of the obligation:

```json
{"type":"stage_explainer","obligation":"<snapshot.publication_binding.obligation>","source_path":"<absolute path to the explainer HTML>"}
```

For `review_explainer`, read the staged bytes through the facade with
`dvandva-role.sh explainer`, which verifies the digest for you, then copy the
same obligation and `publication_binding.artifact.source_digest` unchanged:

```json
{"type":"record_explainer_review","obligation":"<snapshot.publication_binding.obligation>","source_digest":"<snapshot.publication_binding.artifact.source_digest>","verdict":"approved","findings":[]}
```

`publish_explainer` is optional and never gates the run. It records a
human-facing Codex Site that renders the already-staged bytes; its
`source_digest` must equal the staged digest. Reuse one stable Site ID for the
run and record a new Site version for each deployment:

```json
{"type":"record_explainer_publication","obligation":"<snapshot.publication_binding.obligation>","source_digest":"<snapshot.publication_binding.artifact.source_digest>","site_id":"<stable run Site ID>","site_version":"<new version>","url":"<exact deployment URL>","channel":"codex_sites","access":"owner_only"}
```

Never record a verdict on bytes you did not read. Recording an unread approval,
or substituting a Claude Artifact, generic publisher, or any other
silent fallback, cannot satisfy the gate.

A run whose `publication_policy` names a channel the reviewer cannot read is
refused at `start` with `publication_unreadable`; repair it with `repair-policy`
rather than working around it.

## Liveness

Before and during long authorized work, publish progress so the peer can tell
slow from dead. `report_progress` also renews your own lease, and is never a
reason for the peer to wake:

```json
{"type":"report_progress","phase":"publishing_explainer","detail":"<current step>"}
```

Phases are `working`, `publishing_explainer`, `reviewing_explainer`,
`reviewing_checkpoint`, and `waiting`. Read the peer's phase, claim state, and
lease expiry from the snapshot's `peer` block; never infer a dead peer from an
expired lease alone.

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
