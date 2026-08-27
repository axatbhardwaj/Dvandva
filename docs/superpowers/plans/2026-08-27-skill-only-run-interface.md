# Skill-Only Dvandva v4 Implementation Plan

> **Execution note:** Follow this plan with `superpowers:executing-plans`. Use
> `superpowers:test-driven-development` for every behavior change,
> `superpowers:writing-skills` plus `writing-for-agents` for the three skills,
> and `superpowers:verification-before-completion` before each completion or
> release claim.

**Goal:** Release a Linux skill package in which a human can start ordinary
Codex and Claude sessions, say “act as vadi” or “act as prativadi,” and complete
one autonomous implementation/review loop without calling the Rust kernel or
copying credentials by hand.

**Architecture:** Keep `v4/` as the only authority for state and claims. Add
repository/task discovery and a private role-session facade to that kernel,
then expose only three thin skills: `setup-dvandva`, `vadi`, and `prativadi`.
Package the kernel as a checksummed GitHub Release asset installed under XDG
data; do not revive or modify archived v3 distribution code.

**Technology:** Rust 2021, Clap, Serde, Notify, SHA-256, POSIX shell, GitHub
Actions, Agent Skills CLI.

---

## Guardrails and fixed decisions

- Work only on the v4 successor and new `skills/` package. Archived v3 remains
  frozen and unsupported.
- The helper is private and is never added to `PATH`.
- Skills may start and wait on the helper, but neither harness may invoke the
  other harness.
- `vadi` maps to kernel `worker`; `prativadi` maps to kernel `reviewer`.
- Default ticket casting is Codex vadi and Claude prativadi, while explicit
  role reversal remains valid.
- Discovery matches normalized repository identity first and an explicit task
  reference second. It never silently chooses the newest run.
- The raw claim token is written only to a mode-0600 credential file and is
  never emitted by the role-session commands.
- Matt Pocock skills remain explicit-only and are never invoked by these
  skills.
- Tag the first skill release `skills-v0.1.0`; do not create a crates.io
  release or a v3 plugin release.

## Task 1: Persist repository and task identity

**Files:**

- Modify: `v4/src/model.rs`
- Modify: `v4/src/cli.rs`
- Modify: `v4/tests/run_channel.rs`

### Step 1: Write failing black-box initialization tests

Add test coverage proving that `init` accepts:

```text
--repository-id github.com/axatbhardwaj/dvandva
--origin git@github.com:axatbhardwaj/Dvandva.git
--worktree /tmp/dvandva-a
--task-reference DEF-123
```

and persists trimmed `workspace` and `task` objects. Add negative tests for a
blank repository ID and blank optional task reference. Add a compatibility
test that a pre-extension baton with neither field still deserializes.

Run:

```bash
cargo test --manifest-path v4/Cargo.toml init_persists_workspace_and_task_identity -- --exact
```

Expected: fail because the flags and model fields do not exist.

### Step 2: Add backward-compatible model types

In `model.rs`, add:

```rust
pub struct WorkspaceIdentity {
    pub repository_id: String,
    pub origin: Option<String>,
    pub worktree: Option<String>,
}

pub struct TaskIdentity {
    pub reference: Option<String>,
    pub summary: String,
}
```

Add `workspace: Option<WorkspaceIdentity>` and `task: Option<TaskIdentity>` to
`RunBaton` with Serde defaults so PR #4 history remains readable. New CLI
initialization must always populate both fields; legacy `RunBaton::new` may
remain available to unit callers by delegating to a richer constructor.

### Step 3: Extend and validate `init`

Add required `--repository-id` plus optional `--origin`, `--worktree`, and
`--task-reference` arguments. Trim all text fields before persistence. Reject
blank required fields, blank values supplied for optional fields, unsafe run
IDs, and same-family participants before creating the directory.

Run:

```bash
cargo test --manifest-path v4/Cargo.toml init_
```

Expected: all initialization tests pass.

### Step 4: Run the kernel suite and commit

```bash
cargo fmt --manifest-path v4/Cargo.toml -- --check
cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v4/Cargo.toml
git add v4/src/model.rs v4/src/cli.rs v4/tests/run_channel.rs
git commit -m "feat(v4): persist run discovery identity"
```

## Task 2: Derive a stable repository identity

**Files:**

- Create: `v4/src/identity.rs`
- Modify: `v4/src/lib.rs`
- Modify: `v4/src/cli.rs`
- Create: `v4/tests/identity.rs`

### Step 1: Write failing identity tests

Cover these cases using temporary Git repositories:

1. HTTPS and SCP-like GitHub origins normalize to the same lowercase
   `github.com/owner/repo` identity and strip `.git`.
2. A linked worktree and its main worktree produce the same local fingerprint.
3. A repository with no origin receives a stable canonical common-directory
   fingerprint.
4. A non-Git directory fails with structured `repository_missing` output.

Exercise the public command:

```bash
dvandva-v4 identify --workspace "$TEST_REPO"
```

Expected JSON fields are `repository_id`, optional `origin`, and canonical
`worktree`.

### Step 2: Implement identity without shell interpolation

Implement Git calls with `std::process::Command` argument arrays:

- `git -C "$WORKSPACE" rev-parse --show-toplevel`
- `git -C "$WORKSPACE" rev-parse --path-format=absolute --git-common-dir`
- `git -C "$WORKSPACE" config --get remote.origin.url`

Normalize URL/scp remotes structurally. For no-origin repositories, hash the
canonical common-directory path and prefix it with `local:`. Never incorporate
the current worktree into `repository_id`.

### Step 3: Add `identify` and structured diagnostics

Expose `identity::identify(&Path)` and a JSON-emitting CLI command. Add stable
error codes for a missing Git repository and invalid origin rather than
passing through raw command stderr.

### Step 4: Verify and commit

```bash
cargo test --manifest-path v4/Cargo.toml --test identity
cargo test --manifest-path v4/Cargo.toml
git add v4/src/identity.rs v4/src/lib.rs v4/src/cli.rs v4/tests/identity.rs
git commit -m "feat(v4): identify repositories across worktrees"
```

## Task 3: Discover and foreground-wait for matching runs

**Files:**

- Create: `v4/src/discovery.rs`
- Modify: `v4/src/lib.rs`
- Modify: `v4/src/cli.rs`
- Create: `v4/tests/discovery.rs`

### Step 1: Write failing discovery matrix tests

Create independent run directories under a temporary `runs/` root and test:

- one matching active run returns `outcome: match`;
- zero matching runs returns `outcome: none` without mutation;
- multiple matching runs return `outcome: ambiguous` and all concise choices;
- an explicit task reference narrows candidates exactly;
- terminal runs are ignored;
- the wrong repository or reviewer harness is ignored;
- a corrupt matching baton is reported separately and never selected;
- two runs without task references remain ambiguous;
- no outcome uses mtime or lexical newest as a tie-breaker.

Use a `discover` CLI command with `--runs-dir`, `--repository-id`,
`--reviewer-harness`, and optional `--task-reference`.

### Step 2: Implement read-only candidate scanning

Scan only direct child directories and their `baton.json` files. Reuse
`RunChannel::read` so schema validation stays centralized. Return a typed
outcome containing candidates and corrupt entries. Do not acquire claims or
create a mutable index.

### Step 3: Write failing wait tests

Test a `discover-wait` command for:

- starting before the runs root exists and waking after a matching run is
  created;
- unrelated filesystem events not waking it with a match;
- polling-only fallback finding a run;
- timeout returning `outcome: none` cleanly;
- several simultaneous matches returning `ambiguous`;
- a corrupt candidate returning a fail-closed diagnostic.

### Step 4: Implement foreground notification with polling fallback

Watch the runs root or nearest existing parent using `notify`, but re-scan on
every relevant event and at a bounded poll interval. A notification is only a
wake hint; the scan result is authoritative. No background process survives
the command.

### Step 5: Verify and commit

```bash
cargo test --manifest-path v4/Cargo.toml --test discovery
cargo test --manifest-path v4/Cargo.toml
git add v4/src/discovery.rs v4/src/lib.rs v4/src/cli.rs v4/tests/discovery.rs
git commit -m "feat(v4): discover and wait for role runs"
```

## Task 4: Add private role-session credentials

**Files:**

- Create: `v4/src/credential.rs`
- Create: `v4/src/role_session.rs`
- Modify: `v4/src/lib.rs`
- Modify: `v4/src/cli.rs`
- Create: `v4/tests/role_session.rs`

### Step 1: Write failing credential-security tests

Test a role-facing `role claim` facade which receives run directory, role,
session ID, lease, expected revision, and credential root. Assert:

- stdout contains the revision and credential locator but not the token;
- the credential file and every newly created credential directory deny group
  and other access;
- the file stores run, role, session, epoch, and token;
- no token occurs in `baton.json`, `history/`, stderr, or command JSON output;
- an existing credential owned by another session is not reused;
- recovery-invalidated credentials fail closed.

### Step 2: Implement atomic private credential storage

Write credentials through a same-directory temporary file, `sync_all`, mode
0600, rename, and directory sync. Create credential directories as 0700.
Reject symlinked credential roots/files and unexpected ownership metadata.
Load credentials only when run, role, and session match the request.

### Step 3: Add credential-backed role commands

Provide a nested `role` facade for:

- `role claim` and `role reclaim`;
- `role read`;
- `role heartbeat`;
- `role apply --action "$ACTION_FILE"`;
- `role wait`.

These commands load/store the raw token internally. Existing low-level commands
remain available for kernel testing and recovery, but the skills must use only
the role facade. Map CAS contention and expiry to stable diagnostics.

### Step 4: Cover replacement and racing claims

Add black-box tests showing that two reviewer sessions racing to claim one run
have exactly one winner; the loser receives claim contention and creates no
usable credential. After expiry, reclaim increments the epoch and the previous
credential can no longer mutate the run.

### Step 5: Verify and commit

```bash
cargo test --manifest-path v4/Cargo.toml --test role_session
cargo test --manifest-path v4/Cargo.toml
git add v4/src/credential.rs v4/src/role_session.rs v4/src/lib.rs v4/src/cli.rs v4/tests/role_session.rs
git commit -m "feat(v4): fence private role credentials"
```

## Task 5: Add a skill-safe run facade

**Files:**

- Modify: `v4/src/role_session.rs`
- Modify: `v4/src/cli.rs`
- Create: `v4/tests/skill_flow.rs`

### Step 1: Write the failing end-to-end helper test

Drive only skill-safe commands through this sequence:

1. vadi identifies the repository, finds no matching run, and creates one with
   a slug plus random suffix;
2. prativadi discovers and claims it;
3. vadi records a checkpoint and waits;
4. prativadi requests changes and waits;
5. vadi submits a distinct checkpoint;
6. prativadi approves the exact checkpoint;
7. vadi records required publication at the current desired revision and
   finalizes without changing the approved identity;
8. both waiters observe `done` and exit.

Assert there is one run directory, no raw token in captured output, and no
process invokes `claude`, `codex`, or `t3`.

### Step 2: Add idempotent `role start`

Create a high-level start command that combines discovery, initialization or
joining, and claim under existing per-run CAS rules. For vadi, exactly one
resumable repository/task match resumes; none creates; many returns choices.
For prativadi, none returns a wait outcome; one claims; many returns choices.
Concurrent creation must have one winner and then converge on the winner.

Generate safe run IDs from the task reference or summary plus a UUID suffix.
Reject same-family worker/reviewer bindings.

### Step 3: Add version and compatibility probing

Enable Clap `--version` and add `probe --expected-schema dvandva.run.v1` that
emits machine-readable package, version, schema, and compatibility. Probe must
perform no run mutation.

### Step 4: Verify and commit

```bash
cargo test --manifest-path v4/Cargo.toml --test skill_flow
cargo test --manifest-path v4/Cargo.toml
git add v4/src/role_session.rs v4/src/cli.rs v4/tests/skill_flow.rs
git commit -m "feat(v4): expose skill-safe run operations"
```

## Task 6: Build the deterministic setup skill

**Files:**

- Create: `skills/setup-dvandva/SKILL.md`
- Create: `skills/setup-dvandva/scripts/setup-dvandva.sh`
- Create: `skills/setup-dvandva/references/installation.md`
- Create: `tests/skills/setup-dvandva.sh`

### Step 1: Apply the skill-writing workflow

Read and follow `superpowers:writing-skills`, `writing-for-agents`, and
`skill-creator` before authoring files. Pressure-test trigger wording before
writing the final instructions. Mark `setup-dvandva` explicit-only in its
frontmatter because it changes user-level installation state.

### Step 2: Write failing isolated installer tests

The shell test must create temporary XDG data/state roots and a fake local
release containing the compiled helper plus SHA256 manifest. Test:

- clean install and compatibility probe;
- checksum mismatch leaves `current` untouched;
- update installs beside the previous version then atomically switches;
- doctor reports missing, corrupt, wrong-version, and healthy states;
- uninstall removes only manifest-owned binary data;
- uninstall preserves the runs root;
- purge requires a separate explicit confirmation argument;
- an unowned existing path is refused and preserved.

Expose test-only source overrides through environment variables rather than
hard-coded production branches.

### Step 3: Implement setup, update, doctor, and uninstall

The script must:

- derive XDG defaults without writing inside a repository;
- accept only supported Linux architectures;
- download the versioned asset and `SHA256SUMS`;
- verify the exact asset digest before installation;
- use a manifest to identify owned files;
- write installation metadata atomically;
- keep the private helper out of `PATH`;
- preserve run history unless `uninstall --purge-runs --yes-purge-runs` is
  explicitly supplied.

Keep `SKILL.md` concise and move deterministic operator details into the
reference. It should instruct the agent to run the bundled script and report
the returned evidence, not reconstruct installation logic itself.

### Step 4: Verify and commit

```bash
bash tests/skills/setup-dvandva.sh
git add skills/setup-dvandva tests/skills/setup-dvandva.sh
git commit -m "feat(skills): add deterministic kernel setup"
```

## Task 7: Author the vadi and prativadi skills

**Files:**

- Create: `skills/vadi/SKILL.md`
- Create: `skills/vadi/references/run-contract.md`
- Create: `skills/vadi/scripts/dvandva-role.sh`
- Create: `skills/prativadi/SKILL.md`
- Create: `skills/prativadi/references/run-contract.md`
- Create: `skills/prativadi/scripts/dvandva-role.sh`
- Create: `tests/skills/role-skills.sh`

### Step 1: Pressure-test natural-language discovery

Before final prose, test descriptions against prompts that should invoke:

- “Act as vadi and implement DEF-123.”
- “Resume my vadi run for DEF-123.”
- “Act as prativadi for the current run.”
- “Join DEF-123 as prativadi.”

Also test prompts that must not invoke: ordinary implementation/review without
role language, `$setup-dvandva`, and explicit Matt Pocock skill requests.

### Step 2: Write failing structural and sandbox tests

Validate frontmatter, expected trigger phrases, explicit statements that the
skills never invoke the peer harness or Matt skills, and use only role-facade
commands. In a temporary HOME/XDG environment, exercise wrappers against the
fake installed helper and prove they resolve `bin/current/dvandva-kernel`
without relying on `PATH`.

### Step 3: Write the minimal role skills

`vadi` must guide the hosting model to identify instructions/task, start or
resume the worker claim, implement only the Baton objective, checkpoint exact
immutable work plus verification, wait, revise, publish when required, and
finalize only the approved identity.

`prativadi` must identify the repository, wait for or auto-join exactly one
reviewer match, review the exact checkpoint independently, record actionable
findings or approval, and wait for the next revision. Ambiguity is surfaced to
the human; no newest-run tie-breaker is allowed.

Both skills retain their role until terminal state or an explicit human stop.
They may use ordinary model tools and applicable repository instructions, but
they may invoke a Matt skill only when the human explicitly names it in that
session.

### Step 4: Verify Skills CLI discovery and commit

Use an isolated home and list/install from the local repository:

```bash
bash tests/skills/role-skills.sh
npx --yes skills add . --list
```

Confirm exactly `setup-dvandva`, `vadi`, and `prativadi` are distributable
skills. Do not use `--full-depth`: that diagnostic mode intentionally descends
into the preserved v3 plugin archive, while normal repository discovery stops
at the active root `skills/` surface. Then commit:

```bash
git add skills/vadi skills/prativadi tests/skills/role-skills.sh
git commit -m "feat(skills): add vadi and prativadi roles"
```

## Task 8: Package release assets and document the active boundary

**Files:**

- Create: `.github/workflows/skills-release.yml`
- Create: `scripts/package-skills-release.sh`
- Create: `tests/skills/package-release.sh`
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`
- Modify: `v4/README.md`
- Create: `docs/workflows/skill-only-run.md`

### Step 1: Write failing packaging tests

Test that packaging produces:

```text
dvandva-kernel-linux-x86_64
SHA256SUMS
```

and that the manifest verifies, the binary reports `0.1.0`, and no v3
artifact/plugin path is included. Fail if the working tree source version and
requested `skills-vX.Y.Z` tag disagree.

### Step 2: Implement reproducible Linux packaging

Build `v4` with `--locked --release`, copy the result to the stable asset name,
strip only when available, compute SHA-256, and write all artifacts to a caller
provided empty output directory. Do not publish from the packaging script.

### Step 3: Add a tag-gated release workflow

On `skills-v*` tags, the workflow must:

1. check out the exact tag;
2. run fmt, Clippy, v4 tests, archived v3 tests, and all shell skill tests;
3. package the locked Linux x86_64 binary;
4. create a GitHub Release with the binary and checksum manifest.

Grant only `contents: write`; pin official actions to immutable major tags or
commit SHAs according to repository convention. A normal push/PR must run the
same verification without publishing.

### Step 4: Reconcile archive documentation

Keep v3.5.1 explicitly retired. State separately that the v4 kernel and three
skills are the active successor, with installation:

```bash
npx --yes skills add axatbhardwaj/Dvandva --global \
  --agent claude-code codex \
  --skill setup-dvandva vadi prativadi
```

If the Skills CLI rejects either agent identifier during verification, replace
the command with the exact accepted identifiers before committing. Document
that `$setup-dvandva` must be run after skill installation and that no daemon
or cross-harness invocation exists.

### Step 5: Verify and commit

```bash
bash tests/skills/package-release.sh
cargo test --manifest-path v4/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
git diff --check
git add .github/workflows/skills-release.yml scripts/package-skills-release.sh \
  tests/skills/package-release.sh README.md AGENTS.md CLAUDE.md v4/README.md \
  docs/workflows/skill-only-run.md
git commit -m "ci(skills): package versioned kernel releases"
```

## Task 9: Run the automated two-role canary

**Files:**

- Create: `tests/skills/two-role-canary.sh`
- Modify: documentation only if the canary reveals a contract correction

### Step 1: Build an isolated installed environment

Install the local three skills and locally packaged kernel into temporary
Claude/Codex-style homes and XDG roots. Start prativadi's discovery wait before
the run exists, then start vadi. Drive the full loop using two independent
process environments and distinct session IDs.

### Step 2: Assert the acceptance invariants

The canary must prove:

- pre-start prativadi wakes after vadi creation;
- post-start discovery joins the same run;
- a second racing reviewer cannot join;
- findings bind to checkpoint A, approval binds to checkpoint B;
- finalization succeeds only after publication synchronization;
- both roles observe terminal state;
- role reversal completes in a second run;
- credentials are mode 0600 and raw tokens appear nowhere else;
- no peer harness executable is launched.

### Step 3: Run focused then full verification and commit

```bash
bash tests/skills/two-role-canary.sh
cargo fmt --manifest-path v4/Cargo.toml -- --check
cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v4/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
bash tests/skills/setup-dvandva.sh
bash tests/skills/role-skills.sh
bash tests/skills/package-release.sh
git diff --check
git add tests/skills/two-role-canary.sh
git commit -m "test(skills): prove the two-role run canary"
```

## Task 10: Adversarial review and exact-head fixes

### Step 1: Self-review both standards and spec

Invoke the repository `code-review` skill against the merge base with PR #4.
Review the exact local HEAD along both axes:

- standards: KISS, modularity, no v3 revival, credentials, durability, shell
  safety, release permissions;
- spec: all ten design acceptance criteria and every approved invocation rule.

### Step 2: Fix every blocking finding test-first

For each confirmed issue, add a reproducing test before changing production
code. Commit coherent fixes separately with semantic messages. Re-run focused
tests after each fix.

### Step 3: Run final local verification

```bash
cargo fmt --manifest-path v4/Cargo.toml -- --check
cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path v4/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
for test_script in tests/skills/*.sh; do bash "$test_script"; done
npx --yes skills add . --list
git diff --check
git status --short
```

Record exact output counts and `git rev-parse HEAD` for the handoff.

## Task 11: Publish and independently verify `skills-v0.1.0`

### Step 1: Push the stack and open/update the follow-up PR

Use `gh stack` per repository preference. Ensure the follow-up PR targets PR
#4's branch until the parent is merged. Include verification evidence and a
clear statement that v3 distribution is unchanged.

### Step 2: Obtain the real two-session T3 evidence

The automated canary is necessary but cannot prove model-level skill discovery.
Run one actual Codex vadi session and one actual Claude prativadi session from
the locally installed candidate. Confirm natural-language activation, exact-one
discovery, review/resubmission, and terminal exit. Because one harness may not
launch the other, this step requires the human to start the peer session when
requested; do not fake it with a subprocess.

### Step 3: Integrate only with explicit authority

After parent and follow-up PR checks/reviews are green, use the approved gh
stack integration path. Re-query the live PR heads immediately before merge.
Do not tag a commit that is absent from the repository's release branch.

### Step 4: Tag and observe the release workflow

Create and push annotated tag `skills-v0.1.0`, then monitor the exact workflow
run to completion. A tag alone is not publication evidence.

Verify live state:

```bash
gh release view skills-v0.1.0 --json tagName,isDraft,isPrerelease,assets,url
RELEASE_VERIFY_DIR="$(mktemp -d)"
gh release download skills-v0.1.0 --pattern SHA256SUMS \
  --pattern 'dvandva-kernel-*' --dir "$RELEASE_VERIFY_DIR"
(cd "$RELEASE_VERIFY_DIR" && sha256sum -c SHA256SUMS)
```

### Step 5: Verify the public installation path

In fresh temporary homes:

```bash
npx --yes skills add axatbhardwaj/Dvandva --list
npx --yes skills add axatbhardwaj/Dvandva --global --agent claude-code codex \
  --skill setup-dvandva vadi prativadi -y
```

Invoke the installed setup script in doctor/install/doctor order against the
live GitHub Release. Confirm the installed helper checksum and compatibility
probe, and verify neither archived plugin paths nor existing user skills were
modified.

### Step 6: Report the release

Only after GitHub assets and the fresh install both verify, report:

- release URL and tag;
- released commit SHA;
- asset names and checksum result;
- test totals and real-canary result;
- exact installation command;
- any remaining non-blocking follow-up.
