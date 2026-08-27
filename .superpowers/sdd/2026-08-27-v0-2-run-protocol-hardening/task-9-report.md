# Task 9 incident replay report

## Scope

- Starting HEAD: `5c483198d79c7619eaf7739c8b7ab7da630b2f4d`
- Replay commit: `8181f2e37bd752e26b53aa5049c94d5d544f41e4`
- Exact-byte assertion commit: `507ce9f53fdfc10487a36fff9028b3c843680450`
- Changed executable: `tests/skills/two-role-canary.sh`
- Production/kernel/docs changes: none

This lane added only the persistent incident replay requested by Task 9. The
existing normal Codex-vadi/Claude-prativadi canary, reverse
Claude-vadi/Codex-prativadi canary, and peer-executable trap remain in the same
test and still run before the incident replay.

## Baseline and source gap

Before editing:

```text
$ bash tests/skills/two-role-canary.sh
two-role skill canary: ok
exit 0

$ rg 'request_checkpoint_supersession|accept_checkpoint_supersession|supersession_pending|scope_mismatch' tests/skills/two-role-canary.sh
exit 1
```

The baseline castings passed, but the persistent canary had no scope-mismatch
or checkpoint-supersession replay.

## Incident obligations exercised

The added normal-casting run asserts:

1. Codex vadi creates revision 1 and immediately returns the exact run ID and
   `Act as prativadi and join Dvandva run <run-id>.` prompt.
2. Claude prativadi supplies conflicting exact-run scope, receives
   `scope_mismatch`, and leaves the Baton at revision 1 before joining by run ID
   alone at revision 2.
3. Checkpoint A contains only `review.md`; its already-published handoff reaches
   revision 7.
4. The worker requests supersession for the absent `reuse-analysis.md` at
   revision 8. Approval using revision 7 fails `revision_conflict`; approval
   using revision 8 fails `supersession_pending`.
5. Reviewer acceptance reaches revision 9, checkpoint B contains both files,
   B is published/reviewed/approved, and finalization reaches revision 18.

History is parsed as data and must contain exactly these five completed
obligation kinds in order:

```text
run_started
worker_to_reviewer
checkpoint_superseded
worker_to_reviewer
reviewer_to_worker
```

All five receipts must use one `site-incident` identity, five distinct ordered
versions (`incident-1` through `incident-5`), Codex as publisher, and Claude as
reviewer. The terminal Baton must bind checkpoint and review to B's exact
identity, manifest digest, and scope revision. Independently read worker and
reviewer terminal snapshots must contain byte-equal checkpoint JSON. The
existing fake `claude` and `codex` executables still prove that no role launches
its peer.

The negative-action helper invokes the facade directly, captures its exit code
before deleting the temporary action file, and returns that same code. It does
not call the success helper from an `if` condition.

## Fresh verification

Post-commit verification at `8181f2e37bd752e26b53aa5049c94d5d544f41e4`:

```text
$ cargo test --locked --manifest-path v4/Cargo.toml --all-targets
8 executable test targets
172 passed; 0 failed; 0 ignored
exit 0

$ bash tests/skills/two-role-canary.sh
two-role skill canary: ok
exit 0

$ bash -n tests/skills/two-role-canary.sh
exit 0

$ git diff --check HEAD^..HEAD
exit 0
```

The focused canary executed three complete temporary run pairs: the preserved
normal casting, the preserved reverse casting, and the incident replay.
After tightening the terminal comparison to encoded checkpoint bytes, the
focused canary and `bash -n` were rerun at `507ce9f53fdfc10487a36fff9028b3c843680450`;
both exited 0.

## Trust and side-effect boundary

The canary validates authenticated participant ownership and structurally
bound publication/review receipts. Its `https://sites.openai.test/...` values
are controlled fixtures. It does not call Codex Sites, verify a live
deployment, or provide a provider signature.

Installation occurs only under the canary's `mktemp`-backed `HOME`, XDG data,
and XDG state roots. This lane did not install into the user's real home, push,
tag, publish, upload, or deploy anything.
