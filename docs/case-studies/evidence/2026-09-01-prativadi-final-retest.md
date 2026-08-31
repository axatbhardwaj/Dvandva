# Prativadi final fresh-context retest evidence

Date: 2026-09-01

This is a sanitized evidence record. The absolute workspace prefix was replaced
with `<repo>`, Markdown links were normalized, and line wrapping was adjusted.
The scenario, verdicts, and substantive result wording are unchanged.

## Reviewed source snapshot

- Commit: `efce1087e73f43aada884354dfdd11d9d0eaa4e9`
- `skills/prativadi/SKILL.md` sha256:
  `e2564b5cb53ec4aed8616071dbf05846588c7dfc4eecfabeb2a61d39edb914e6`
- `skills/prativadi/references/run-contract.md` sha256:
  `51fb22ed9556a2c0e81a0009cb9b179a5485bd12d9ad17f4ed591e8b22155228`
- `skills/vadi/SKILL.md` sha256:
  `456c51d2f37fa5b4d6896920ac36a5f99f7037826ff84b9f873d8a282d805b1c`
- `skills/vadi/references/run-contract.md` sha256:
  `3efb238d05930ed8ac34ad454606654eb92696d6fa4f5b13db557e8c1e819bb9`

Reproduce a source digest with:

```bash
git show efce1087e73f43aada884354dfdd11d9d0eaa4e9:skills/prativadi/SKILL.md | sha256sum
```

## Sanitized prompt

```text
Fresh read-only retest of the CURRENT uncommitted prativadi/vadi guidance in
<repo>. Scenario: Claude is prativadi, Codex is vadi, Matt code-review is
available; vadi is midway through implementation, later submits a complete Git
delivery candidate, code-review then requests changes, vadi revises and submits
a new complete candidate. Human is offline. Separately consider missing
code-review fallback and the lack of a raw-report peer transport. Report
PASS/FAIL with exact source evidence for: (1) no code-review during WIP, (2)
exactly once per newly authorized complete candidate, including revised
candidate, (3) native fallback when unavailable, (4) only accepted
findings/verdict are durable Baton/facade state, with raw reports remaining
local session evidence and no invented explainer transport, (5) no peer harness
invocation. Do not edit files or mutate protocol state.
```

## Sanitized result transcript

```text
Overall: PASS at the guidance level. No runtime protocol test was performed.

1. PASS — no `code-review` during WIP.

   Prativadi may review only when `advisory_actions` authorizes
   `review_checkpoint`, and the contract says: “Never invoke a companion
   against partial work [or] an implementation-in-progress.” ([prativadi
   contract](<repo>/skills/prativadi/references/run-contract.md:71))

   Vadi likewise must “Never submit partial, work-in-progress, or incremental
   implementation merely to obtain a review.” ([vadi
   contract](<repo>/skills/vadi/references/run-contract.md:69))

2. PASS — exactly once per newly authorized complete candidate, including the
   revision.

   The controlling instruction is: “Invoke `code-review` once for each newly
   authorized complete `git` delivery candidate.”

   After accepted changes are requested, vadi's next submission must again be
   complete: “A revision after requested changes is another complete delivery
   candidate, not an intermediate checkpoint.” ([vadi
   contract](<repo>/skills/vadi/references/run-contract.md:71))

   Thus candidate A gets one invocation; after adjudicated findings and
   revision, newly authorized candidate B gets one fresh invocation. Companion
   reports are evidence, not automatically the Dvandva verdict. ([prativadi
   contract](<repo>/skills/prativadi/references/run-contract.md:102))

3. PASS — native fallback when unavailable.

   Absence, hidden/user-only status, unreadability, host rejection, or failure
   to return both reports makes the companion unavailable. Claude then
   completes Standards and Spec natively “without installing or changing
   skills”; availability never blocks review. ([prativadi
   contract](<repo>/skills/prativadi/references/run-contract.md:102), [fallback
   disclosure](<repo>/skills/prativadi/references/run-contract.md:126))

4. PASS — durable state is deliberately limited; no raw-report transport is
   invented.

   The provenance block and raw adjudication are explicitly “operator-visible
   session evidence, not Baton state or a peer transport.” The contract
   prohibits claiming raw reports or rejected findings were staged in the
   explainer. `record_review` durably stores only the checkpoint-bound verdict
   and accepted actionable findings, which are all Codex may rely on through
   the facade. ([prativadi
   contract](<repo>/skills/prativadi/references/run-contract.md:110))

   The verdict remains bound to checkpoint identity, manifest digest, and scope
   revision; stale reports must be discarded rather than rebound. ([prativadi
   contract](<repo>/skills/prativadi/references/run-contract.md:141))

   Both explainer contracts require only canonical scope, complete manifest,
   durable findings and decisions, and current plan/TODO—not raw companion
   reports. ([prativadi
   contract](<repo>/skills/prativadi/references/run-contract.md:199), [vadi
   contract](<repo>/skills/vadi/references/run-contract.md:153))

5. PASS — no peer-harness invocation.

   Matt `code-review` runs locally inside Claude prativadi and “is not a
   peer-harness invocation.” Neither role invokes or wakes the other harness.
   ([prativadi skill](<repo>/skills/prativadi/SKILL.md:58), [prativadi
   contract](<repo>/skills/prativadi/references/run-contract.md:265), [vadi
   contract](<repo>/skills/vadi/references/run-contract.md:225))

The offline human does not interrupt this sequence: accepted findings move
through the checkpoint-bound facade receipt, while each independently started
role continues foreground polling.

Snapshot hashes: prativadi skill `e2564b5c…`, prativadi contract `51fb22ed…`,
vadi skill `456c51d2…`, vadi contract `3efb238d…`.

No files or protocol state were changed.
```
