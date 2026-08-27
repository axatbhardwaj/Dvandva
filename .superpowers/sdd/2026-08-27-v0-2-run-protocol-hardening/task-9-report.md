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

### Controller full matrix

The controller pinned
`a1f0d293e072bdcdf98e887300d0bdc20fdde35a`, verified its merge base was
`4e502529b73d7cd6b2f5eb819b67275d4b8a7da3`, and confirmed the HEAD stayed
unchanged through the complete run.

```text
cargo fmt --manifest-path v4/Cargo.toml -- --check: exit 0
cargo clippy --locked --manifest-path v4/Cargo.toml --all-targets -- -D warnings: exit 0
cargo test --locked --manifest-path v4/Cargo.toml --all-targets: 172 passed, 0 failed across 8 executable targets
cargo run --quiet --locked --manifest-path v4/Cargo.toml -- probe --expected-schema dvandva.run.v2 --expected-role-api 2: exact compatible v2/API2 private-release probe, exit 0
cargo test --locked --manifest-path rust/Cargo.toml --workspace: 1,740 passed, 0 failed across 38 executable targets plus one zero-test doc target
bash tests/skills/role-skills.sh: role skill wrappers: ok
bash tests/skills/setup-dvandva.sh: setup-dvandva installer tests: ok
bash tests/skills/package-release.sh: skills release packaging: ok
bash tests/skills/two-role-canary.sh: two-role skill canary: ok
```

Shell syntax passed for all eight changed active shell files. Duplicate-safe
`ruamel.yaml` 0.18.16 parsing verified all-branch push and pull-request
verification, `skills-v*` tag gating, inherited read-only verification
permissions, and the write-scoped release job. The first controller-only YAML
assertion incorrectly expected the push branch list to be `main`; source and
the committed package test require `"**"`. The corrected structural check
passed without a product change.

`shellcheck`, `actionlint`, `yamllint`, `yq`, and PyYAML remain unavailable and
were not claimed as run. `git diff --check 4e502529...HEAD`, the no-diff gate
for `rust/` and `plugins/dvandva/`, crate `publish = false`, the no-`cargo
publish` release check, and clean-worktree check passed. `gh stack view`
reported the current local branch directly above `origin/main`.

Archive hashes remained exact:

```text
README retired-v3 suffix: 83182b2773ae4c52a71c2568cb856770b4269d1cdeb47bd92fb400fa4807629a
historical two-mode body: 12d51f85fc0ec5e945e99122f465ee5dcad205604993eeb7cb1340f65acdf6b8
```

## Whole-branch review and consolidated fix wave

The six-axis review of `4e502529b73d7cd6b2f5eb819b67275d4b8a7da3...4bc994e348be8d1b6efe3c4968cfb4f531cbb400`
found blocking gaps in immutable history installation, taskless and terminal
role discovery, supersession control gates, installer cleanup/purge safety,
facade byte validation and contracts, and release-ref identity. The accepted
fix range is `4bc994e348be8d1b6efe3c4968cfb4f531cbb400..947be7b49c585b364d572c58b925b08e4ccdd531`.

The fix wave now provides:

- failure-atomic history staging with no-replace installation;
- discoverable taskless upgraded runs and credential-free exact terminal
  `stop` snapshots;
- exact current-publication gates for checkpoint-supersession acceptance and
  approval withdrawal in transition, history, and next-action logic;
- bounded raw candidate validation, retry-safe failed fresh installs, and
  symlink-safe purge preflight;
- reviewer-first discovery without invented deliverables, exact-start reclaim,
  private `ACTION_FILE` lifecycle guidance, and a numeric review scope stamp;
- local/event/remote tag-object and peeled-commit verification immediately
  before packaging and publication, including tree/blob rejection; and
- one canonical handshake contract across packaging, setup, and both facades:
  256 version bytes, 16,384 probe bytes, strict UTF-8/NUL rejection, and zero
  or one final LF.

The final handshake alignment was test-driven. Before the production change,
setup accepted a valid 17,000-byte internally padded probe and packaging
accepted extra-LF and trailing-space probes. The new shared boundary fixtures
failed for those exact reasons, then all three affected suites passed after the
validators were aligned.

Independent scoped re-review reported no remaining findings in kernel/storage,
role/spec, release/setup, or standards. The kernel reviewer reran 126 focused
role/session tests plus the full 177-test v4 lane. Release/setup reviewers
reproduced lightweight, annotated, moved, tree, and blob tag cases. The final
standards recheck confirmed all four validation surfaces share the same byte
contract.

### Final controller matrix

The controller pinned production HEAD
`947be7b49c585b364d572c58b925b08e4ccdd531` and reran the complete matrix:

```text
cargo fmt --manifest-path v4/Cargo.toml -- --check: exit 0
cargo clippy --locked --manifest-path v4/Cargo.toml --all-targets -- -D warnings: exit 0
cargo test --locked --manifest-path v4/Cargo.toml --all-targets: 177 passed, 0 failed across 8 executable targets
cargo test --locked --manifest-path rust/Cargo.toml --workspace: 1,740 passed, 0 failed across 38 executable targets plus one zero-test doc target
bash tests/skills/role-skills.sh: role skill wrappers: ok
bash tests/skills/setup-dvandva.sh: setup-dvandva installer tests: ok
bash tests/skills/package-release.sh: skills release packaging: ok
bash tests/skills/two-role-canary.sh: two-role skill canary: ok
```

The exact v2/API2 private-release probe returned `publish: false` and
`compatible: true`. Bash syntax passed for all active release/setup/facade test
surfaces, the two role facades remained byte-identical, and duplicate-safe YAML
1.2 parsing confirmed all-branch verification, least-privilege permissions,
both release-ref checks, and the exact release-step order. A controller-only
first YAML assertion incorrectly treated the empty `pull_request` mapping as a
branch map; the corrected assertion passed without a product change.

`git diff --check`, the no-diff gate for `rust/` and `plugins/dvandva/`, crate
`publish = false`, the no-`cargo publish` workflow check, archive hashes, and
the clean-worktree gate all passed. `gh stack view` showed the current branch
directly above `origin/main`. `shellcheck`, `actionlint`, `yamllint`, `yq`, and
PyYAML remain unavailable and were not claimed as run.

The remote-tag check still has the documented narrow interval between its
second lookup and GitHub accepting the release request. Protected,
non-force-updatable `skills-v*` tags are the external operational control for
that interval; this change does not claim to configure repository rules.

## Trust and side-effect boundary

The canary validates authenticated participant ownership and structurally
bound publication/review receipts. Its `https://sites.openai.test/...` values
are controlled fixtures. It does not call Codex Sites, verify a live
deployment, or provide a provider signature.

Installation occurs only under the canary's `mktemp`-backed `HOME`, XDG data,
and XDG state roots. This lane did not install into the user's real home, push,
tag, publish, upload, or deploy anything.
