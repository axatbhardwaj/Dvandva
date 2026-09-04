# Vadi run contract

The facade JSON is authoritative. Use only `scripts/dvandva-role.sh`; first
`probe`, then resolve one stable session with `session-id` or one retained
`session-id --generate` fallback.

```text
start SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE [OBJECTIVE [TASK]] [--objective-ref KIND=VALUE] [--required-deliverable ID=DESCRIPTION] [--wait|--new-run|--run-id ID] [--autonomous]
read  SESSION RUN_DIR
observe SESSION RUN_DIR
apply SESSION RUN_DIR EXPECTED_REVISION ACTION_FILE
wait  SESSION RUN_DIR AFTER_REVISION [TIMEOUT_MS]
poll  SESSION RUN_DIR AFTER_REVISION [MAX_MS]
heartbeat SESSION RUN_DIR EXPECTED_REVISION
explainer SESSION RUN_DIR
analysis SESSION RUN_DIR DIGEST
upgrade SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION
repair-policy SESSION RUN_DIR CURRENT_HARNESS PEER_HARNESS EXPECTED_REVISION
claim SESSION RUN_DIR EXPECTED_REVISION
reclaim SESSION RUN_DIR EXPECTED_REVISION
```

`observe` is read-only and claim-independent: it returns the snapshot without
verifying, renewing, or fencing any claim, so a watcher can tell a finished
run from a lapsed claim. It never substitutes for the claim-verified `read`
before a mutation.

Harness identity is protocol data. A Codex participant must use the harness
name `codex` (case-insensitive); aliases such as `codex-cli`, `gpt`, or
`openai` take the no-Codex branch and skip Sites publication.

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
`peer_prompt` before domain-tool work. The first `poll` is illegal until that
activation block, with the exact `peer_prompt`, has been shown to the human.

For `publication_unreadable`, run `repair-policy` with the exact returned run
directory and revision; it installs the readable channel and clears the current
obligation's receipts so vadi restages.

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
`legal_actions` solely for a decision that is the human's alone — `scope`,
`intent`, or `authority` — and never for protocol approval; it is never an
ordinary wake or action.

Protocol-internal problems never block on human approval, because the human may
be absent. Every one has a deterministic recovery to take instead:
`publication_unreadable` takes `repair-policy`, `upgrade_required` takes
`upgrade`, an expired own claim takes exact `start --run-id`, a
changes-requested explainer takes `stage_explainer` again, and a wait timeout
(`wait_outcome: idle_timeout`) takes a fresh snapshot and another wait. The
kernel never leaves `request_human_decision` as the only way forward.

## Workflow selection and vadi lifecycle

Read objective ref `workflow=implementation|babysit|pr_review`; when absent, use
`implementation`. Existing checkpoint, supersession, explainer, Human Decision,
polling, and Matt `code-review` rules remain in force. Parents alone mutate Baton or GitHub; read-only evidence may use subagents.

In `implementation`, deliver and verify the complete canonical scope through the existing checkpoint and review cycle; never checkpoint work in progress merely to obtain review.

In `babysit`, fail closed before writable actions unless live GitHub verifies own-authored/owned scoped work on the PR and branches. Then reproduce feedback or CI failures; patch, test, commit, push, rerun CI, synchronize/rebase, and re-request the existing colleague reviewer. Reply with fix evidence but leave colleague-owned threads; prativadi clearance only permits re-request, while the colleague owns real approval. Changed head, feedback, failed gate, or requested changes reopen the loop. Merge readiness requires the exact internally reviewed head, CI, mergeability, external approvals, no live requested changes, dispositioned threads, current stack/base, and no pending work. Never merge autonomously: even when ready, merge needs fresh merge authorization from the human, including explicit authority for every affected stack PR. After colleague approval, progress from `merge_ready` to `maintaining_ready` and refresh live GitHub between bounded Baton waits; GitHub does not wake Baton. Head, base, CI, approval, requested-change, or thread drift reopens the fix and review loop.

In `pr_review`, create one independent run per external PR. It is read-only except formal GitHub review submission, and vadi must never patch another author's PR. First prepare a constructive report on intent, behavior, integration, tests, maintainability, and practical failures; prativadi adjudicates final `APPROVE` or `REQUEST_CHANGES`, and vadi submits prativadi's adjudicated `APPROVE` or `REQUEST_CHANGES` exact and unmodified. Before submission, recheck PR identity, current head, actor versus author, and permission; self-approval, missing authority, or drift fails closed. A confirmed `REQUEST_CHANGES` completes; prativadi Dvandva approval approves the review artifact, not the external verdict. After submission, vadi queries GitHub and verifies review ID, exact PR, actor, state, reviewed commit/head, and body digest; then prativadi independently re-queries the same receipt. Use the existing receipt-bearing explainer gate: vadi stages and prativadi approves exact local bytes; the Codex participant publishes the approved status Site when present. Sources: [workflow-mode evidence](../../../docs/research/2026-09-01-workflow-mode-github-evidence.md).

For every workflow, recoverable CI, review, scoped-branch failures, and other uncertainty stay autonomous. Before every actual `request_human_decision` for scope, intent, or authority, exchange evidence, attempt an available scoped fix, and use available local Fable adjudication before irreversible human escalation. Authority is permission the human alone can grant; unavailable capability is not a Human Decision. Escalate only an action both roles cannot establish as safe or an unavoidable external permission barrier; Fable is advisory and never a Baton participant. In babysit, design is intent; security and secret-policy are scope or authority, never new kinds.

## Checkpoint and worker mutations

For every `apply` action, the role writes its JSON to a private temporary file
with mode 0600, passes its path as `ACTION_FILE`, and deletes it after `apply`.

A checkpoint contains one complete deliverable manifest. It covers the
canonical deliverable IDs exactly once and includes non-empty verification.
Submit it only after implementation and verification for the whole canonical
scope are complete. Never submit partial, work-in-progress, or incremental
implementation merely to obtain a review. A revision after requested changes
is another complete delivery candidate, not an intermediate checkpoint.
Use immutable artifacts, not a branch or mutable `HEAD`. Checkpoints are typed:
`git` binds full-length commit object names, and `analysis` binds sha256 content
digests for deliverables that produce a review, audit, or finding rather than a
commit:

```json
{"type":"submit_checkpoint","checkpoint":{"kind":"git","identity":"<full-length commit object name>","deliverables":[{"id":"<canonical ID>","artifacts":[{"kind":"commit","value":"<full-length commit object name>"}]}],"verification":["<exact command and result>"]}}
```

An `analysis` checkpoint may only cite digests this run has staged, so the
reviewer can materialize exactly what the manifest names. Its `identity` is
derived from the cited digests — sha256 of them sorted, deduplicated, and joined
with newlines — so a manifest cannot name one thing and carry another. Every
cited artifact is rehashed at approval and at finalization. Stage the bytes first
with `stage_analysis`, then cite the digests it records:

```json
{"type":"stage_analysis","source_path":"<absolute path to the analysis bytes>"}
```

```json
{"type":"submit_checkpoint","checkpoint":{"kind":"analysis","identity":"<sha256 of the cited digests, sorted, deduplicated, newline-joined>","deliverables":[{"id":"<canonical ID>","artifacts":[{"kind":"analysis_digest","value":"<sha256 of the artifact>"}]}],"verification":["<exact command and result>"]}}
```

Read staged analysis bytes back with `dvandva-role.sh analysis SESSION RUN_DIR
DIGEST`, which verifies the digest before returning them.

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

Every request declares what it asks for: `scope` for what the work should cover,
`intent` for which reading of the request is meant, or `authority` for
permission that is the human's alone to give. There is deliberately
no approval kind: a protocol-internal problem has a deterministic recovery, and
the human may be absent.

The options are the decision. The human answers by choosing one of them, and
the kernel refuses any other answer. A `scope` decision resolves through the
chosen `proposals` entry (one concrete scope per option) or an explicit scope
amendment; an `intent` or `authority` answer is recorded as an objective
reference of that kind. A pause that would change nothing about the run cannot
be resolved, and the decision just answered cannot be asked again.

A run started with `--autonomous` admits a decision only as a choice among
scope proposals, so when the human may be absent there is no admissible shape
for "please approve": every pause is a set of concrete scopes the kernel
applies itself.

Use only the minimal request. The kernel derives contact and resume routing:

```json
{"type":"request_human_decision","kind":"scope","question":"<one decision>","evidence":["<verified fact>"],"options":["<concrete option A>","<concrete option B>"],"proposals":[{"objective":"<scope if A>","objective_refs":[],"task_reference":null,"scope_deliverables":[{"id":"<deliverable ID>","description":"<deliverable description>"}]},{"objective":"<scope if B>","objective_refs":[],"task_reference":null,"scope_deliverables":[{"id":"<deliverable ID>","description":"<deliverable description>"}]}]}
{"type":"resume_human_decision","answer":"<one of the recorded options>"}
```

`answer_human` maps to `resume_human_decision`; copy the human's answer. If the
human changes scope, populate every field below only from explicit
human-approved values. `task_reference` must be a JSON string or `null`; it is
not returned by the Human Decision object and must never be inferred:

```json
{"type":"resume_human_decision","answer":"<human-approved answer>","scope_amendment":{"objective":"<human-approved objective>","objective_refs":[{"kind":"<human-approved ref kind>","value":"<human-approved ref value>"}],"task_reference":"<human-approved task reference>","scope_deliverables":[{"id":"<human-approved deliverable ID>","description":"<human-approved deliverable description>"}]}}
```

## Explainer obligation

Each work-carrying handoff opens an obligation. Vadi stages the explainer's bytes
into the run directory and prativadi reviews those exact bytes, regardless of which harness fills either role. Staging is first: the gate binds a digest, not a URL. For `run_started`, vadi proposes this initial HTML before continuing
domain work and incorporates every requested change until prativadi approves.
The kernel enforces this join gate: `work` is not advisory until that approval
— the reviewer's first receipt, proof the pair has formed — and the wait rests
until it lands, while a finished deliverable may still be checkpointed.
On an upgraded run, a complete pre-`0.3.3` receipt pair remains valid against
the stored fixed policy. An incomplete legacy author receipt makes
`stage_explainer` actionable so current vadi restages before current prativadi
reviews.
A work-carrying handoff replaces the current obligation; an approval preserves
the obligation and its receipts, so an approved delivery finalizes on the
explainer already staged and reviewed for its checkpoint. The gate requires the
current obligation to be staged and reviewed, not every obligation the run has
opened.
The explainer carries this exact content:
canonical scope, complete manifest, findings and decisions, and a current plan/TODO.

For vadi-owned `stage_explainer`, write the explainer HTML to a private path and
copy `publication_binding.obligation` unchanged from the fresh snapshot. The kernel
hashes the bytes, stores them at `explainer/<source_digest>.html` inside the run
directory, and binds that digest to the obligation. Staging different bytes
discards any earlier rendering and review of the obligation:

```json
{"type":"stage_explainer","obligation":"<snapshot.publication_binding.obligation>","after_seq":<snapshot.publication_binding.receipt_seq>,"source_path":"<absolute path to the explainer HTML>"}
```

Every receipt carries `after_seq`, copied from
`publication_binding.receipt_seq` in the same fresh snapshot. Only receipts
advance it, so an unrelated peer heartbeat or progress report never invalidates
a prepared write, while a delayed or out-of-order receipt is refused instead of
overwriting newer state. Re-applying an identical receipt is a no-op.

For peer-owned `review_explainer`, prativadi reads the staged bytes through the
facade with `dvandva-role.sh explainer`, then copies the same obligation and
`publication_binding.artifact.source_digest` unchanged:

```json
{"type":"record_explainer_review","obligation":"<snapshot.publication_binding.obligation>","after_seq":<snapshot.publication_binding.receipt_seq>,"source_digest":"<snapshot.publication_binding.artifact.source_digest>","verdict":"approved","findings":[]}
```

After prativadi records an approved local review, `publish_explainer` is
required work for whichever participant is Codex and wakes that participant.
If neither participant is Codex, skip Sites publication. Otherwise treat it as
a mechanical adapter:

1. Refresh the snapshot, then use `dvandva-role.sh explainer` to materialize the
   verified staged bytes into a private, isolated Sites checkout. Keep the exact
   HTML as the status page's rendered source; packaging may add hosting machinery
   but may not rewrite the reviewed content.
2. Invoke the host's `sites:sites-building` and `sites:sites-hosting`
   capabilities. Reuse `publication_binding.site_id` when present; otherwise
   create one Site exactly once. Push the exact source state, save one version,
   deploy it owner-only with the private deployment operation, and poll that
   deployment to terminal success.
3. Refresh the facade again. Record only the connector-returned Site ID, saved
   version, and production URL, bound to the still-current obligation, receipt
   sequence, and local digest. Reuse one stable Site ID for the run and record a
   new Site version for each approved digest:

```json
{"type":"record_explainer_publication","obligation":"<snapshot.publication_binding.obligation>","after_seq":<snapshot.publication_binding.receipt_seq>,"source_digest":"<snapshot.publication_binding.artifact.source_digest>","site_id":"<stable run Site ID>","site_version":"<new version>","url":"<exact deployment URL>","channel":"codex_sites","access":"owner_only"}
```

When Codex participates, finalization requires both the approved local digest
and this matching private Sites receipt. Without Codex, local approval is
sufficient. A connector failure remains Codex-owned publication work: report
the exact non-secret failure, retry only errors identified as temporary, and
leave the run active. Never record a guessed receipt, publish to broader access,
substitute generic hosting, or ask prativadi to review through the Site.

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
unchanged. Third-party user-invoked workflow skills run only when the human
explicitly invokes them in this session.

After each handoff, report these exact fields and, in the same turn, continue
in a foreground local wait until terminal state or human stop:

- What changed
- What was verified
- What is blocked
- Who owns the next action
- Exact command or prompt

Before the first `poll`, after all semantic work and the handoff, the
activation block with the exact `peer_prompt` is the final protocol output
immediately before that first wait.
The foreground wait is `poll`, which re-enters the kernel wait on every
`idle_timeout` until a real wake, a terminal run, or its budget. When `poll`
returns with `wait_outcome: idle_timeout`, call it again immediately; on any
other JSON outcome, read a fresh snapshot and act. A `poll` that exits non-zero
or returns no JSON is a human interrupt: it force-ends the turn, and that is
expected. Ending the turn is not a wait: it stops the poll, lets the lease
lapse, and stalls the peer. After an interrupt force-ends the turn, the next
turn, unless its message is an explicit human stop, first reads a fresh
snapshot, answers anything the human asked, and re-enters `poll`. A bare
continue or an empty resume is never a stop and never a no-op. Heartbeat before
long authorized work. Keep action files private (mode 0600), exclude
credentials, and delete them after `apply`. Sources: the reproduction recorded
in the [issue #22 owner refinement](https://github.com/axatbhardwaj/Dvandva/issues/22#issuecomment-5537575950),
verified locally with `gh issue view 22 --repo axatbhardwaj/Dvandva --comments`.
