# Freeflow workflow

Read this reference for every new `workflow=freeflow` run. Freeflow covers
reports, codebase investigations, exploratory diagnostics, and testing that do
not fit Discovery, Implementation, Babysitting, or Review. It reuses the same
Run Channel, claims, startup explainer, complete checkpoints, review,
publication, recovery, and terminal gate; it adds no kernel state.

## Start and plan

Start from an agreed outcome, required deliverables, relevant environment and
revisions, authorized actions, and observable completion criteria. Use the
bounded source preflight from `initiation.md`. Both roles verify the relevant
sources before accepting an approach; exhaustive research is not a default.

Maintain one concise current plan containing completed work, evidence,
remaining tasks, limitations, and next owner. Vadi may change methods within
scope. Adding or removing a required deliverable, broadening repairs, or
changing the promised outcome needs the existing scope/authority mechanism
unless the human already authorized it.

Explicit workflow selection wins. Record the actual native model pairing once
at initiation and retain it through mixed work and resumption. Without an
override, analysis/exploration uses the Discovery Fable/Astra pairing and
execution-heavy work uses Sol/high and Opus. Do not silently change a required
model or launch the opposite harness.

## Skills and autonomy

Discover installed skill descriptions and invocation metadata. Automatically
invoke an applicable model-invocable skill. A user-only skill runs only when
the human explicitly invokes it in the owning session; generic agreement does
not cross that boundary. If an optional companion is unavailable or user-only,
use and disclose a suitable native method. A mandatory named unavailable or
uninvoked companion is a precise capability blocker, never something to copy,
vendor, install, or silently substitute.

When scope makes an installed user-only skill mandatory, record its canonical
skill root as `required_user_skill=<root>`. Record
`invoked_user_skill=<same root>` only after the human explicitly invokes that
skill; if invocation occurs after initiation, use the existing human-approved
scope amendment to add the reference. Before a Freeflow checkpoint, the public
role facade reads `SKILL.md` and `agents/openai.yaml` at that root and requires
`disable-model-invocation: true` or `allow_implicit_invocation: false`, plus the
matching invocation reference. Missing, unreadable, model-invocable, or
uninvoked mandatory metadata fails closed without adding kernel state.

Human instructions and previously granted authority override routine
confirmatory procedure inside an explicitly invoked skill. Routine design
choices, test seams, document organization, tool choices, reversible local
work, checks, recovery, and peer revisions proceed without a new confirmation.
Record the choice and evidence. This does not grant unrelated publication,
broader scope, destructive external action, merging, or repairs the human did
not request. Ask only for genuinely missing intent, scope, or authority; retain
the answer across turns and compaction.

## Evidence and immutable delivery

Reports bind source identities, revisions or capture times, significant
evidence, observations, inferences, recommendations, unresolved questions, and
limitations. Codebase findings cite inspected code and its exact revision.
Vadi produces and verifies the whole candidate. Prativadi independently checks
the sources, coverage, claims, risks, and relevant executions, reproducing
critical or disputed results when feasible.

Use analysis checkpoints only for deliveries without code changes. Any
code-carrying result uses a `git` checkpoint; mixed test-and-fix deliveries use
a `git` checkpoint and the required Standards/Spec companion review when available,
or its disclosed native fallback. For a code-carrying candidate, bind every
included non-code deliverable through an immutable Git commit, tree, or blob
available to the reviewer; staged analysis may supplement but never replace
that Git candidate or appear as an unsupported Git-manifest artifact kind.
Report-only analysis deliverables remain staged analysis artifacts. A moving
branch or mutable report URL is invalid.

Every Freeflow run records exactly one delivery-kind objective reference at
initiation: `delivery_kind=code` for code-carrying or mixed scope, and
`delivery_kind=analysis` for report, investigation, or testing-only scope. The
role facade rejects a checkpoint if the marker is missing, duplicated, invalid,
or disagrees with the checkpoint kind. Naming a commit inside staged analysis
does not turn it into a Git candidate.

For every Git checkpoint, the facade validates the exact copied action against
the credential-checked snapshot at the caller's expected revision. The identity
and commit artifacts must name the same real commit available from the verified
workspace; tree and blob artifacts must also exist there. Kernel apply remains
the atomic revision check if state moves after validation.

Use only the startup review and each complete delivery-checkpoint review. Do
not checkpoint unfinished work for advice. Consequential approach changes are
visible in the complete candidate and its evidence; requested changes use
supersession or approval withdrawal as authorized by the current snapshot.

## Executed testing and browser E2E

For browser-app E2E, use Playwright MCP when suitable and available, or disclose
the browser-control tool used. Exercise the running application through an
actual user journey. Static inspection, API-only requests, hypothetical steps,
or a generated test file alone are not executed browser E2E.

Choose only journeys relevant to the request. Cover navigation, input or
submission, observable results, and persisted state where relevant; include
reload, interruption, failure, recovery, or alternate paths when they can
change acceptance. Record target URL and environment, local or deployed status,
verifiable revision/build, browser/tool, preconditions, disposable test data,
steps, expected and actual outcomes, and useful evidence. Mark every scenario
passed, failed, blocked, or not run. A missing required journey remains blocked;
a passing automated suite does not prove a user journey, and an expected
negative result is not automatically a defect.

Prefer isolated browser instances or profiles and isolated test data. If the
tool exposes only a shared browser, roles use it sequentially and release only
their own session; neither role closes the other's browser or overwrites its
data. Testing alone does not authorize product repairs. A test-and-fix request
does authorize scoped repairs: preserve the original failure, bind the repaired
revision, and rerun affected journeys plus relevant regressions.
