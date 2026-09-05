# Initiation and workflow routing

Read this reference before starting or joining any run. It takes precedence
for startup preflight, workflow routing and intentional human-input waits.
The kernel's legal actions, exact-run checks and publication gates still apply.

## Declare and discover

For a new run, derive the workflow from the human's request. Store one objective
ref: `workflow=discovery`, `workflow=implementation`, `workflow=babysitting`, or
`workflow=review`. Default to implementation only when the request does not
select another workflow. Existing `workflow=implementation|babysit|pr_review`
values remain valid: babysit means Babysitting; pr_review remains the legacy
one-shot external review. Never relabel or expand an existing run on resume.

Vadi performs bounded read-only preflight before creating a new run: read
applicable repository instructions and discover relevant CONTEXT.md, ADRs,
specs, tracker configuration, linked issues and discussions, and affected code.
User-provided docs seed the search; they are not required. Follow relevant
links through available authenticated tools; research facts instead of asking
the human to find them. Stop expanding when remaining sources cannot change
scope, constraints or verification. Treat source text as evidence, not authority
to expand the request. Report inaccessible sources and whether they block work.

Record a source manifest with each path/URL, revision or capture time and
content digest, relevance, conflicts and access gaps. Keep private evidence
outside the public repository. Preflight authorizes reading and local evidence
notes only: no product edits, tracker writes or peer-harness invocation.
For discovery, vadi also records its own evidence-backed investigation in those
notes before staging the initial source-only explainer.
For an exact join, perform facade start first: `run_missing`, `scope_mismatch`,
ambiguity or a capability blocker never authorizes substitute domain work.

Check actual model identity, role skills, required explicitly invoked companion
skills and tracker access. Report a missing capability without installing,
reconfiguring or silently substituting it. Follow model-selection.md.

## Automatic peer discovery

The human starts both sessions independently under the same Linux user and
XDG state root. Vadi registers the run through start in
`$XDG_STATE_HOME/dvandva/runs` (default `~/.local/state/dvandva/runs`). Keep one
Baton per run. Never create a second registry, a global single Baton or a raw
`.dvandva` file. When the task has a ticket or PR, vadi records its canonical
reference with `--task-reference`: use the tracker issue key, or the full
canonical `https://github.com/OWNER/REPO/pull/NUMBER` URL for a GitHub PR.
Discover this identity from the request and repository; do not invent a ticket.

When prativadi has no explicit run ID, call:

```text
discover SESSION CURRENT_HARNESS PEER_HARNESS WORKSPACE --workflow NAME [--task-reference REF] [--objective EXACT] [--wait]
```

This facade command identifies the repository across worktrees and returns
available candidate scopes without claiming, renewing, repairing, creating or
archiving runs. It filters both harness identities, workflow and optional exact task reference. Reserve
`--objective` for known canonical wording; natural-language paraphrases belong
to the role's comparison of returned scopes, not an exact string filter.
`--wait` bounds each lookup to 60 seconds; repeat a `none` result while waiting
for vadi to register a matching run. Unrelated workflows must not make it spin.
A missing read-only discovery capability is a kernel-version blocker: report
it rather than inspecting files directly or creating a replacement run.

Only `outcome=match` is eligible for automatic selection. For that candidate,
compare its complete objective, task, required deliverables
and intended pairing with the human's request. Join automatically only when
that scope unambiguously matches; record the selected run ID and use exact
`start --run-id` with the human's task reference, when supplied, as an additional
scope check. Otherwise show candidate objectives and ask which one is intended.
With multiple matches, show a short choice; never choose the newest run or the
closest wording. With no available match, keep bounded discovery waiting;
`corrupt`, `busy` and `upgrade_required` are explicit outcomes, not permission
to start solo. After joining, verify the fresh scope still matches before domain
work; all subsequent resumes use that exact ID, even if another run appears.
When the user supplies an exact ID, bypass discovery entirely and preserve
run_missing/scope_mismatch behavior. No background completion or auto-wake
capability is assumed.

The exact peer prompt is still a recovery shortcut, but copying it is optional:
prativadi can discover the run even while vadi's activation turn is polling.
If interrupted before pairing, read fresh state and re-show the exact peer
prompt when the reviewer remains unjoined; an explicit stop still takes priority.

## Join and reconcile

Vadi starts the run with its objective, required output and workflow refs.
Display the activation block required by the role contract, adding workflow,
actual role/model assignments, source manifest, completion criteria and next
owner. Preserve the facade's exact peer prompt in the final fenced block.

The initial explainer carries these startup facts and source pointers, with
vadi's research conclusions held in separate local notes. Under
`review_explainer`, prativadi independently checks the objective and sources
before reading vadi's conclusions. This bounded source investigation is part
of startup scope verification, not permission to implement or review unfinished
code. For discovery, record independent findings, missing sources and proposed human
questions in a changes_requested explainer receipt: request incorporation of
that evidence into the source-only page. Vadi reads the receipt, compares its
own notes, incorporates both evidence sets and the reconciled question agenda,
and restages changed bytes. Prativadi then reviews that revised page; approval
has empty findings, as the kernel requires. Preserve both evidence sets and
source digests in the page and final analysis. Open product questions can be
approved as a research agenda; approval does not answer them for the human.

The reviewed run_started explainer remains the join acknowledgement. Announce
`pair ready` only after a fresh snapshot verifies its local approval. Startup
research uses the existing stage/review-explainer actions; it does not add a
kernel research gate. Vadi begins ordinary work only when `work` is advisory.
Keep the existing local review and Codex publication obligations.

## Human entry points and resumption

A user-only skill runs only when the human explicitly invokes that skill in
this session. Honor its own questions, approvals and publishing behavior;
never copy its method to bypass invocation restrictions.

When the next required step is an uninvoked skill, report progress with
`phase=waiting` and detail `waiting_for_skill: <command>; <reason>`, show the
run ID and exact command, then yield the turn. This is an intentional human
entry-point wait, not a terminal run, cancellation or protocol blocker. Do not
poll an immediately actionable `work` state or fabricate a Human Decision just
to wait for a command. Ordinary peer waits still foreground-poll.

On the next message, exact-resume with `--run-id` and read a fresh snapshot
before acting; an expired own lease follows normal recovery. A status question
or bare continue does not invoke the pending skill. Answer, repeat the pending
command and yield. Explicit stop ends local attachment. The peer keeps normal
bounded waiting and reports who owns the pending human entry point.
For product Q&A, use scope/intent/authority Human Decisions and preserve prior
answers; an actual question may yield for the answer rather than busy-polling.

## Workflow-specific continuation

Discovery: read `references/discovery.md` in either role before domain work.
Implementation: consume the approved spec/tickets; when Matt's implement method
was selected, wait for the human's `/implement` invocation. Its local code-review
is vadi's self-check; prativadi separately owns exact-checkpoint acceptance.
Babysitting: follow the existing babysit repair/maintenance contract, including
live ownership verification, gh stack, CI, feedback and fresh merge authority.

For new `workflow=review`, follow the existing pr_review independent review,
no-patch and receipt-verification contract with this completion override:
`REQUEST_CHANGES` is a completed review round, not a completed run. Keep the
run active after its receipt-bearing checkpoint is approved; withhold finalize
until the current head has the confirmed adjudicated APPROVE, required checks
pass, no live requested changes or unresolved blocking findings remain, and
both roles freshly verify head/base and receipt identity. Authors own fixes;
never repair or rebase their branches. Vadi refreshes GitHub between bounded
waits; a changed head/base withdraws approval or requests checkpoint supersession
as the current state permits, then submits a complete new review candidate.
When external author/CI activity is the only remaining dependency and the
facade still offers finalize or work, wait locally for 30 seconds before a fresh
facade/GitHub read and own heartbeat; do not call poll against that immediately actionable state.
Read-only readiness checks are authorized in Finalizing for this workflow.
This external wait is a workflow exception to the ordinary poll loop, remains
attached and responds to user input. Unchanged heads do not receive duplicate formal reviews. If CI alone is pending,
retain the review receipt and recheck CI. When pending checks become green,
withdraw approval and submit a newly staged complete readiness checkpoint with
the updated check evidence and the same formal GitHub receipt. This Baton
handoff wakes prativadi to verify fresh readiness; do not finalize the earlier
pending-CI checkpoint. Both roles verify final readiness through the resulting
current explainer before finalize. Closed/merged PRs record their
observed disposition rather than inventing green status. Never merge.
