# Freeflow and multi-PR Review validation — 2026-09-05

## Scope and evidence boundary

This report validates the active v4 role-contract changes for issue #31 as one
immutable candidate under review. The candidate's exact identity belongs to
the final checkpoint evidence rather than mutable worktree prose. It does not
change the v4 kernel schema, legal statuses, claim model, or archived v3 tree.

The evidence classes below are intentionally separate. A deterministic fixture
or local two-role canary is not evidence that two live model sessions completed
the workflow, that a Site rendered correctly, or that a browser journey ran.

| Evidence class | Status | Boundary |
| --- | --- | --- |
| Simulated service and canary evidence | Passed | Deterministic fake GitHub state and installed public role facades in isolated Git/XDG directories. |
| Actual paired/model evidence | Startup observed; issue behavior not executed | The current pair approved its startup evidence, but installed role/kernel 0.3.9 does not contain these uninstalled source changes. |
| Actual Sites rendering evidence | Local rendering passed; deployment succeeded | The approved startup bytes passed desktop/mobile inspection and the owner-only Site has a succeeded deployment receipt; the deployed URL was not independently opened. |
| Actual browser E2E | Passed with disclosed fallback | CUA exposed no browser surface, so Playwright 1.63 drove local Chromium 151 through the disposable app's failure, fix, persistence, recovery, and isolated-profile journeys. |

## Simulated service and canary evidence

`tests/skills/fixtures/github_review_lifecycle.py` models only mutable GitHub
observations and review writes. It is not a second Baton protocol. The installed
vadi/prativadi facades still create, join, stage, submit, adjudicate, withdraw,
replace, approve, and finalize the one real local canary run.

The deterministic five-PR fixture passed these cases:

- Mixed `APPROVE` and `REQUEST_CHANGES` adjudication, with one member's CI
  moving from pending to green.
- Exact author/acting-reviewer, PR, full head/base, state, body, and body-digest
  binding. Self-review and actor/body mismatches do not match a receipt.
- Head drift and base/dependency drift immediately before a write block the
  stale submission.
- Changing PR 103 invalidates PR 103 and dependent PR 104, while checked
  evidence for unrelated PRs 101, 102, and 105 remains valid.
- New blocking feedback invalidates PR 102. Its author repair changes the head,
  clears the finding, and requires a fresh review and receipt.
- Interruptions immediately before and after review writes use exact receipt
  lookup. The confirmed unchanged retry creates zero duplicate submissions.
- Merged PR 101 and closed PR 105 retain their literal dispositions rather than
  fabricated green/readiness claims.
- All still-open members finish with current approval receipts, green required
  checks, no blocking feedback, and evidence at the current revisions.

The fixture produced five unique writes and five receipts across 22 recorded
events. Its initial, confirmed-`REQUEST_CHANGES`, and final per-PR JSON artifacts were staged by digest into
the existing five-deliverable analysis checkpoint. The public facade rejected
missing and duplicate manifest coverage, then exercised the complete
review-round → approval → receipt update/approval withdrawal → complete
replacement → readiness approval → finalization cycle. There was no per-PR
kernel approval or child run. A stale approval carrying the superseded round's
identity and manifest digest is rejected before the current receipt-bearing
round is approved. Finalize is behaviorally refused for both the initial mixed
round and the confirmed `REQUEST_CHANGES` round. It succeeds only for the final
complete manifest: the facade obtains the current credential-checked snapshot,
materializes every cited digest with pinned-kernel `role analysis`, validates
the documented member/receipt/readiness fields, and then lets kernel apply
atomically compare the expected revision. Neither guard reads Baton directly.

The discovery regression enumerates a Review member, applies a human-approved
scope amendment before exact join, and proves that the join returns the new
scope revision/member/deliverable rather than the earlier enumeration. The old
member no longer matches observational lookup. Case-insensitive canonical
member lookup succeeds, while malformed duplicate/cross-repository candidates
are reported under `invalid_candidates` without aborting a healthy match.

The same canary proves the checkpoint-kind acceptance rule. Every Freeflow run
must carry exactly one `delivery_kind=code|analysis` marker. A code-carrying run
cannot submit an analysis checkpoint merely because its staged report names a
commit; an analysis-only run cannot submit a Git checkpoint. Missing and
invalid markers also return `invalid_checkpoint`. The matching Git and analysis
checkpoints succeed.
The guard reads its run snapshot through the pinned kernel's credential-checked
`role read` operation and never opens Baton state directly. It evaluates the
policy only when that snapshot revision matches the facade caller's expected
revision; kernel apply remains the atomic authority for later races.
The facade copies the caller action once into a mode-600 private temporary file,
then gives those same bytes to the guard and kernel. Git identities and commit
artifacts must match a real commit in the verified worktree; cited tree and blob
objects must also be available. The canary rejects a syntactically valid fake
commit and rejects a real commit mislabeled as a tree before accepting a real,
exactly typed fixture commit.

The pre-existing `autonomy_regression.rs` suite covers general kernel autonomy
and recovery invariants; it is not issue-specific role behavior. The new
two-role Freeflow canary uses public role operations to show routine test-seam,
format, and tool choices; a recoverable tool failure; recorded model selection;
and interrupted exact resumption continuing without a Human Decision. It then
submits a complete analysis checkpoint, receives peer-requested changes, and
exact-resumes in `revising` with the model reference retained. A bounded poll
trace proves the waiting loop does not spin tightly. The objective and required
deliverable predeclare broader publication, so the missing publication authority
is necessary rather than a confirmatory pause; its exact human answer is retained
and reused, and a duplicate question is rejected. A disposable mandatory
user-only skill carries real `SKILL.md` and `agents/openai.yaml` metadata. The
facade discovers those documented flags and rejects checkpoint submission when
only `required_user_skill` is present, then accepts the same checkpoint shape in
a positive run where explicit invocation is represented by the matching
`invoked_user_skill` reference.

The five-PR fixture also evaluates readiness before protocol finalization. Its
initial mixed round and its confirmed `REQUEST_CHANGES` receipt both remain not
ready; only the final state with current approvals, green checks, resolved
findings, fresh evidence, and literal closed/merged dispositions becomes ready.

## Commands and observed results

Focused RED evidence:

- The Freeflow source-contract test failed before the Git-binding language was
  scoped to non-code deliverables inside code-carrying candidates.
- `tests/skills/discover.sh` failed before duplicate `review_member` identities
  were folded case-insensitively.
- `tests/skills/two-role-canary.sh` failed before the role-facade checkpoint-kind
  guard rejected a code-marked analysis submission.
- The expanded facade canary failed while a syntactically valid but unavailable
  Git identity was accepted, and while Freeflow could omit its delivery marker.
- The lookup regression failed before an exact join returned the amended scope
  rather than the member set observed during enumeration.
- The validation-report contract failed while this case-study file was absent.
- The expanded public-facade canary failed before `review_guard.py` existed;
  Review finalize therefore had no behavioral readiness gate for its materialized
  per-member artifacts.

Focused GREEN evidence recorded during implementation:

```text
python3 tests/skills/fixtures/github_review_lifecycle.py OUTPUT_DIR
github fixture: ok; writes=5; receipts=5; events=22

bash tests/skills/discover.sh
automatic run discovery: ok

bash tests/skills/checkpoint-guard.sh
checkpoint guard: ok

bash tests/skills/two-role-canary.sh
github fixture: ok; writes=5; receipts=5; events=22
two-role skill canary: ok
```

Final full-suite GREEN evidence, rerun after the extracted-guard,
discovery-isolation, readiness, sanitization, and issue-specific autonomy repairs:

```text
cargo fmt --check --manifest-path v4/Cargo.toml
passed

cargo test --all-targets --locked --manifest-path v4/Cargo.toml
passed; skill_flow 23/23, autonomy_regression 36/36, all other v4 targets green

bash tests/skills/checkpoint-guard.sh
checkpoint guard: ok

bash tests/skills/discover.sh
automatic run discovery: ok

bash tests/skills/role-skills.sh
role skill wrappers: ok

bash tests/skills/two-role-canary.sh
github fixture: ok; writes=5; receipts=5; events=22
two-role skill canary: ok

bash tests/skills/poll.sh
poll behaviour: ok

bash tests/skills/poll-errors.sh
poll errors: ok

bash tests/skills/html-deliverables.sh
html-deliverables tests: ok

bash tests/skills/setup-dvandva.sh
setup-dvandva installer tests: ok

bash tests/skills/package-release.sh
dvandva-kernel-linux-x86_64: OK
skills release packaging: ok
```

`git diff --check`, Bash syntax checks, Python compilation, archived-v3
exclusion, and byte comparisons for every shared role reference/script also
passed. The packaging command built a disposable release artifact for
verification only; it did not publish, install, or change release state.

## Actual paired/model evidence

The parent reported current paired run
`https-github-com-axatbhardwaj-dvandva-is-c60efdfc`, started from repository
revision `bf568c49a53f936c9140c355b9884b8f36e3f6c3`. Codex is the vadi
coordinator on Sol/medium, with this native Sol/high implementation station;
Claude Opus is prativadi. The startup explainer at digest
`c5c1c61a68fc0d67409fc630240b31e7083dc1151aefce894f8f906e9f5d8c73`
was reviewed and approved.

This proves current paired startup and evidence review only. The installed role
and kernel remain version 0.3.9, and the issue #31 source changes in this
worktree are neither installed nor released. Therefore no live Freeflow
routing, persistent multi-PR Review lifecycle, final checkpoint, recovery, or
terminal-completion claim is made from this run. Those issue behaviors are
**not executed** as live paired/model acceptance in this report.

## Actual Sites rendering evidence

The parent reported an owner-only succeeded deployment for the approved startup
explainer. The Site URL, project/version identifiers, and deployment receipt
remain only in private run evidence; this public case study intentionally
redacts them. The approved source digest is
`c5c1c61a68fc0d67409fc630240b31e7083dc1151aefce894f8f906e9f5d8c73`.

That receipt proves deployment of the startup bytes. Before staging, the exact
local artifact was rendered at 1440 px and 390 px, visually inspected, corrected
to keep the evidence table's overflow local on mobile, re-rendered, and passed
the active standalone HTML validator. The deployed URL itself was not opened,
so this report does not claim an independent deployed-render acceptance. The
local canary's isolated fake Sites receipt remains simulated evidence and is not
evidence of the live deployment.

## Actual browser E2E: passed with a disclosed fallback

The preferred CUA path was unavailable: `getBrowser` returned
`No browser is available`, `createBrowserTab('iab')` returned
`Browser is not available: iab`, and `getState` returned
`{apps:[],browsers:[]}`. Issue #31 explicitly permits another disclosed
browser-control tool, so the same local fixture at `127.0.0.1:43131` was driven
with Playwright 1.63.0 using Chromium 151.0.7922.173 in headless mode. The final
fixture SHA-256 was `d6584b0163b38982711a31f8b7e432bf6b50e61810238db8696c4b20fee21924`;
the journey-spec SHA-256 was
`ab3e1c696d0032ce708142ee097d53352901e5f755df24ae29715fc8228d5c38`.

The first execution navigated to the actual page, exercised the empty-submit
validation path, entered and submitted a note, observed the success status, and
then failed on reload because the save and load paths used different
`localStorage` keys. Both the persistence journey and the isolated-profile
journey failed at their expected post-reload value assertions. The scoped fix
changed the save key from `journey_note` to `journey-note`; the rerun produced
`2 passed (829ms)`. It verified navigation, input/submission, observable validation
and success states, reload persistence, separate browser contexts with isolated
data, closure of one context without disrupting the other, and the repaired
path. The fixture was disposable test data outside the repository; no product
repair or external write occurred.

This is executed browser/test-and-fix evidence, not evidence that the uninstalled
Freeflow routing contract ran in a live role session. The current paired/model
limitation above remains explicit.
