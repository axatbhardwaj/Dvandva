# Minimal Run Baton Kernel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a non-publishable, run-centric v4 coordination kernel whose two local participants autonomously alternate work, exact-checkpoint review, revision, and finalization through one JSON Baton and a filesystem watcher.

**Architecture:** A new Rust crate under the isolated v4 boundary exposes one deep `RunChannel` module and a thin `dvandva-v4` CLI. The Run Channel owns schema validation, participant fencing, transition legality, compare-and-swap persistence, immutable history, recovery, and local notification; tracker and explainer data remain opaque projection references. All behavioral tests use the public CLI against temporary run directories, including two-process wait/wake scenarios.

**Tech Stack:** Rust 2021, serde/serde_json, clap, thiserror, time, notify, fs2, sha2, uuid, assert_cmd, predicates, tempfile.

**Spec:** https://github.com/axatbhardwaj/Dvandva/issues/3

## Global Constraints

- Preserve the archived v3.5.1 crate, plugin source, release record, and historical tests without modification.
- Keep all active successor code in the non-publishable `v4/` boundary established by ADR-0001.
- Use exactly one versioned `dvandva.run.v1` Baton per autonomous run; do not create tracker-, ticket-, review-, or publication-specific protocols.
- Use the local filesystem watcher as the primary wake-up mechanism and interval polling only as fallback; T3 and trackers provide no wake-up semantics.
- Never launch one harness from the other or model-invoke an explicit-only Matt Pocock skill.
- Keep source-changing runs isolated by branch/worktree and bind review receipts to immutable Git or artifact identities.
- Follow TDD for every behavior: add one failing black-box test, observe the expected failure, implement the minimum, and rerun the focused and full v4 suites.
- Do not install, publish, release, push, or start a real Dvandva run while implementing this plan.

---

### Task 1: Create the non-publishable v4 crate and canonical schema

**Files:**
- Modify: `.gitignore`
- Create: `v4/Cargo.toml`
- Create: `v4/src/lib.rs`
- Create: `v4/src/main.rs`
- Create: `v4/src/model.rs`
- Create: `v4/src/cli.rs`
- Create: `v4/tests/run_channel.rs`

**Interfaces:**
- Produces: `RunBaton::new(run_id, objective, worker_harness, reviewer_harness) -> RunBaton`
- Produces: `dvandva-v4 init --run-dir <dir> --run-id <id> --objective <text> --worker <harness> --reviewer <harness>`
- Produces: `dvandva-v4 read --run-dir <dir>` emitting canonical JSON.

- [ ] **Step 1: Add crate-only scaffolding and ignore its build output**

Create a `publish = false` Rust package with library name `dvandva_v4`, binary name `dvandva-v4`, and the dependencies named in the plan header. Add `/v4/target/` to `.gitignore`. Keep `main` empty so the first behavioral test can fail through the public executable.

- [ ] **Step 2: Write the failing initialization test**

```rust
#[test]
fn init_creates_a_run_centric_baton() {
    let dir = tempfile::tempdir().unwrap();
    command()
        .args([
            "init", "--run-dir", dir.path().to_str().unwrap(),
            "--run-id", "run-a", "--objective", "Implement DEF-123",
            "--worker", "codex", "--reviewer", "claude",
        ])
        .assert()
        .success();

    let baton: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.path().join("baton.json")).unwrap(),
    ).unwrap();
    assert_eq!(baton["schema"], "dvandva.run.v1");
    assert_eq!(baton["run_id"], "run-a");
    assert_eq!(baton["status"], "working");
    assert_eq!(baton["assignee"], "worker");
    assert_eq!(baton["participants"]["worker"]["harness"], "codex");
    assert_eq!(baton["participants"]["reviewer"]["harness"], "claude");
}
```

- [ ] **Step 3: Run the focused test and confirm RED**

Run: `cargo test --manifest-path v4/Cargo.toml --test run_channel init_creates_a_run_centric_baton -- --exact`

Expected: FAIL because `dvandva-v4 init` does not yet exist or does not create `baton.json`.

- [ ] **Step 4: Implement the minimal schema and init/read commands**

Define serializable `RunBaton`, `Objective`, `Participants`, `Participant`, `Status`, `Assignee`, `Checkpoint`, `ReviewReceipt`, `Publication`, and `HumanDecision` types. Initial state is revision `0`, status `working`, assignee `worker`, no checkpoint/review/human decision, and publication not required. Reject unsafe run IDs, blank objectives, identical harness-family bindings, and an existing Baton.

- [ ] **Step 5: Verify GREEN and schema rejection cases**

Run: `cargo test --manifest-path v4/Cargo.toml --test run_channel`

Expected: initialization passes; add and pass cases for unsafe run IDs, same-family participants, blank objectives, duplicate initialization, and canonical `read` output.

- [ ] **Step 6: Commit**

```bash
git add .gitignore v4
git commit -m "feat: add minimal run baton schema"
```

### Task 2: Add atomic storage and immutable history

**Files:**
- Create: `v4/src/store.rs`
- Modify: `v4/src/lib.rs`
- Modify: `v4/src/cli.rs`
- Modify: `v4/tests/run_channel.rs`

**Interfaces:**
- Produces: `RunChannel::open(path) -> Result<RunChannel>`
- Produces: `RunChannel::create(initial) -> Result<RunBaton>`
- Produces: `RunChannel::read() -> Result<RunBaton>`
- Produces: `RunChannel::compare_and_swap(expected_revision, next, event) -> Result<RunBaton>`

- [ ] **Step 1: Write a failing concurrent-initialization test**

Start two `init` processes against the same empty run directory and assert exactly one succeeds, exactly one returns structured `run_exists`, one valid `baton.json` remains, and history contains exactly one revision-zero snapshot.

- [ ] **Step 2: Run the focused test and confirm RED**

Run: `cargo test --manifest-path v4/Cargo.toml --test run_channel concurrent_initialization_has_one_winner -- --exact`

Expected: FAIL because initialization is not yet protected by a shared lock and durable store.

- [ ] **Step 3: Implement the file store**

Use a run-local lock file with an exclusive `fs2` lock. Under the lock, reread and strictly parse current state, compare `revision`, validate the next state, serialize to a unique temporary file, flush file contents, atomically rename to `baton.json`, sync the directory, and write the accepted revision to `history/<revision>.json`. Never overwrite corrupt current state or install an invalid candidate.

- [ ] **Step 4: Add black-box durability cases**

Add cases proving corrupt current state, lock contention, immutable initialization history, and independent run directories. Defer monotonic compare-and-swap rejection to Task 3, where claim actions provide a public state-changing command. Assert behavior through CLI exit codes and JSON diagnostics.

- [ ] **Step 5: Run focused and full v4 tests**

Run: `cargo test --manifest-path v4/Cargo.toml`

Expected: all v4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add v4/src v4/tests
git commit -m "feat: persist baton revisions atomically"
```

### Task 3: Add participant claims, fencing, leases, and replacement

**Files:**
- Create: `v4/src/claim.rs`
- Modify: `v4/src/model.rs`
- Modify: `v4/src/cli.rs`
- Modify: `v4/src/store.rs`
- Modify: `v4/tests/run_channel.rs`

**Interfaces:**
- Produces: `claim --role <worker|reviewer> --session-id <id> --lease-seconds <n> --expected-revision <n>` returning the raw token once.
- Produces: `heartbeat --role <role> --session-id <id> --token <token> --expected-revision <n>`.
- Produces: `reclaim --role <role> --session-id <id> --lease-seconds <n> --expected-revision <n>` after expiry.
- Stores only a SHA-256 token digest in the Baton; action commands present the raw token.

- [ ] **Step 1: Write the failing split-brain test**

Claim the worker role from `worker-1`, attempt a second live claim from `worker-2`, and assert the second claim is rejected. Use a one-second lease and wait for expiry, reclaim with `worker-2`, then assert `worker-1` cannot heartbeat or transition with its stale token. Also submit two heartbeat commands with the same expected revision and assert only the first can win.

- [ ] **Step 2: Run the focused test and confirm RED**

Run: `cargo test --manifest-path v4/Cargo.toml --test run_channel expired_claim_replacement_fences_the_old_session -- --exact`

Expected: FAIL because claims do not exist.

- [ ] **Step 3: Implement claims and token verification**

Add `ParticipantClaim { session_id, epoch, token_digest, lease_expires_at }`. A first claim uses epoch `1`; reclaim requires expiry and increments the epoch. Heartbeat validates role, session, token digest, and current epoch before extending the lease. Claim and heartbeat increment the Baton revision but do not change semantic checkpoint identity.

- [ ] **Step 4: Add claim boundary cases**

Cover wrong role, wrong session, wrong token, non-expired reclaim, expired reclaim, independent worker/reviewer claims, heartbeat after terminal state, and heartbeat preserving an existing review receipt.

- [ ] **Step 5: Run the full v4 suite**

Run: `cargo test --manifest-path v4/Cargo.toml`

Expected: all v4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add v4/src v4/tests
git commit -m "feat: fence run participants"
```

### Task 4: Implement checkpoint, review, revision, and finalization transitions

**Files:**
- Create: `v4/src/action.rs`
- Create: `v4/src/transition.rs`
- Modify: `v4/src/model.rs`
- Modify: `v4/src/cli.rs`
- Modify: `v4/tests/run_channel.rs`

**Interfaces:**
- Produces: `apply --run-dir <dir> --action <json-file>`.
- Consumes tagged actions: `submit_checkpoint`, `record_review`, `finalize`, `request_human_decision`, `resume_human_decision`, and `abandon`.
- Produces legal semantic states `working`, `reviewing`, `revising`, `finalizing`, `human_decision`, `done`, and `abandoned`.

- [ ] **Step 1: Write the failing complete-loop test**

Through only `init`, `claim`, `apply`, and `read`, exercise `working -> reviewing -> revising -> reviewing -> finalizing -> done`. Use artifact identities `sha256:first` and `sha256:second`; bind findings to the first and approval to the second. Assert assignee changes on every semantic handoff and finalization retains the approved identity.

- [ ] **Step 2: Run the focused test and confirm RED**

Run: `cargo test --manifest-path v4/Cargo.toml --test run_channel complete_review_fix_loop_reaches_done -- --exact`

Expected: FAIL because semantic actions and transitions do not exist.

- [ ] **Step 3: Implement the action interpreter**

Validate writer claim before interpreting an action. `submit_checkpoint` is worker-only from `working` or `revising`, requires a new Git or artifact identity plus non-empty verification, clears stale review, and assigns `reviewing` to the reviewer. `changes_requested` is reviewer-only, binds the current identity, requires actionable findings, and assigns `revising`. `approved` is reviewer-only, binds the current identity, rejects blocking findings, and assigns `finalizing`. `finalize` is worker-only, requires unchanged approved identity and satisfied required publication, then writes terminal `done`.

- [ ] **Step 4: Add transition rejection cases**

Cover wrong owner, illegal source state, reused checkpoint identity, missing verification, empty findings, approval with blocking findings, stale reviewed identity, post-review work mutation, unsynchronized required publication, and every write attempted after a terminal state.

- [ ] **Step 5: Add generic Git and artifact checkpoint cases**

Prove a Git checkpoint carries base/head identities and an artifact checkpoint carries a digest/reference, while the transition graph and receipt validation remain identical.

- [ ] **Step 6: Run the full v4 suite**

Run: `cargo test --manifest-path v4/Cargo.toml`

Expected: all v4 tests pass.

- [ ] **Step 7: Commit**

```bash
git add v4/src v4/tests
git commit -m "feat: enforce review handoff transitions"
```

### Task 5: Add Human Decision, publication synchronization, and terminal provenance

**Files:**
- Modify: `v4/src/action.rs`
- Modify: `v4/src/model.rs`
- Modify: `v4/src/transition.rs`
- Modify: `v4/tests/run_channel.rs`

**Interfaces:**
- `request_human_decision` records question, evidence, options, contact role, resume status, and resume assignee.
- `resume_human_decision` records the answer and restores the declared state/assignee.
- `record_publication` advances desired/published projection revisions without changing semantic checkpoint identity.

- [ ] **Step 1: Write failing pause/resume and publication tests**

Assert either active participant can request a Human Decision with complete resume metadata; only the designated contact can record the answer; the declared role becomes actionable afterward; and a required publication prevents `done` until its published revision equals the desired revision.

- [ ] **Step 2: Run focused tests and confirm RED**

Run: `cargo test --manifest-path v4/Cargo.toml --test run_channel human_decision_resumes_declared_owner -- --exact && cargo test --manifest-path v4/Cargo.toml --test run_channel required_publication_blocks_done_until_synchronized -- --exact`

Expected: FAIL because these actions are absent.

- [ ] **Step 3: Implement pause, resume, publication, and abandonment**

Require non-blank questions, at least two options, evidence of attempted resolution, a valid contact role, and a non-terminal resume target. Publication data remains provider-neutral opaque references. `abandon` requires a reason and is terminal. Record an optional predecessor run reference at initialization but never reopen terminal state.

- [ ] **Step 4: Add negative cases and run all tests**

Cover duplicate Human Decision surfacing, invalid resume target, wrong contact, publication revision regression, tracker/provider strings changing transition behavior, and terminal mutation.

Run: `cargo test --manifest-path v4/Cargo.toml`

Expected: all v4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add v4/src v4/tests
git commit -m "feat: govern pauses and publication"
```

### Task 6: Implement the foreground local watcher and autonomous two-process loop

**Files:**
- Create: `v4/src/wait.rs`
- Modify: `v4/src/cli.rs`
- Modify: `v4/src/lib.rs`
- Modify: `v4/tests/run_channel.rs`

**Interfaces:**
- Produces: `wait --run-dir <dir> --role <role> --session-id <id> --token <token> --after-revision <n> [--poll-interval-ms <n>] [--timeout-ms <n>]`.
- Returns actionable JSON when the role becomes assignee, terminal JSON for `done`/`abandoned`, contact JSON for a designated Human Decision, or a structured timeout/fencing error.

- [ ] **Step 1: Write the failing real-watcher handoff test**

Spawn reviewer `wait` as a child process after revision `N`, assert it remains blocked, submit a worker checkpoint, and assert the child exits promptly with status `reviewing`, assignee `reviewer`, and the new revision. Repeat in the same test for reviewer findings waking the worker, revised checkpoint waking the reviewer, approval waking the worker, and `done` waking the reviewer terminally.

- [ ] **Step 2: Run the focused test and confirm RED**

Run: `cargo test --manifest-path v4/Cargo.toml --test run_channel local_watcher_drives_the_complete_two_process_loop -- --exact`

Expected: FAIL because `wait` does not exist.

- [ ] **Step 3: Implement directory notification with polling fallback**

Watch the Baton directory, never the Baton inode, because accepted writes use atomic replacement. On every event or interval, strictly reread state and decide whether the caller is actionable, terminal, designated human contact, fenced, or still waiting. Renew the participant lease before expiry through the same Run Channel write path. Test configuration may disable notification to force polling fallback.

- [ ] **Step 4: Add watcher failure cases**

Cover unrelated run updates, spurious filesystem events, deleted Baton, corrupt Baton, stale token after replacement, timeout, terminal arrival, Human Decision contact/non-contact behavior, and polling-only wake-up.

- [ ] **Step 5: Run the full v4 suite repeatedly**

Run: `for i in 1 2 3; do cargo test --manifest-path v4/Cargo.toml --test run_channel || exit 1; done`

Expected: three clean passes without timing flakes.

- [ ] **Step 6: Commit**

```bash
git add v4/src v4/tests
git commit -m "feat: wake paired sessions locally"
```

### Task 7: Add explicit history recovery and prove run isolation

**Files:**
- Modify: `v4/src/store.rs`
- Modify: `v4/src/cli.rs`
- Modify: `v4/tests/run_channel.rs`

**Interfaces:**
- Produces: `recover --run-dir <dir> --from-revision <n>` as an explicit administrative action.
- Recovery validates the selected snapshot and complete prefix, restores it with a new monotonic recovery revision, expires prior claims, and records provenance.

- [ ] **Step 1: Write the failing corrupt-state recovery test**

Create several accepted revisions, corrupt `baton.json`, assert ordinary read/apply/wait fail closed, recover from the latest valid history revision, assert both prior participant tokens are fenced, and verify the recovered state preserves checkpoint/review evidence.

- [ ] **Step 2: Run the focused test and confirm RED**

Run: `cargo test --manifest-path v4/Cargo.toml --test run_channel recovery_fences_old_sessions_and_preserves_evidence -- --exact`

Expected: FAIL because recovery is absent.

- [ ] **Step 3: Implement explicit recovery**

Validate schema, run identity, history revision ordering, and snapshot digest before restoring. Never infer recovery from transcript state. Write a recovery history entry, clear active claims, and require both participants to claim again.

- [ ] **Step 4: Prove independent concurrent runs**

Start two run directories with the same workspace and unrelated objectives. Keep waits active in both, transition only one, and assert the other remains blocked and byte-identical. No global lock, project state, or ticket scheduler may be created.

- [ ] **Step 5: Run all v4 and archived tests**

Run: `cargo test --manifest-path v4/Cargo.toml && cargo test --manifest-path rust/Cargo.toml`

Expected: both suites pass.

- [ ] **Step 6: Commit**

```bash
git add v4/src v4/tests
git commit -m "feat: recover durable run state"
```

### Task 8: Align active documentation and perform the current-harness canary

**Files:**
- Modify: `CONTEXT.md`
- Modify: `docs/adr/0002-delegated-explicit-skill-dispatch.md`
- Create: `docs/protocol/minimal-run-baton.md`
- Create: `v4/README.md`
- Modify: `docs/superpowers/specs/2026-08-26-independent-harness-v4-design.md`
- Modify: `docs/superpowers/specs/2026-08-26-v4-kernel-simulator-design.md`

**Interfaces:**
- Documents the public CLI, schema, state graph, participant startup prompts, recovery, and boundary between the authoritative Run Baton and external projections.

- [ ] **Step 1: Write a documentation conformance test that fails**

Add a black-box test that reads the active protocol and README and asserts they name `dvandva.run.v1`, every public semantic state, the local watcher, the no-cross-harness rule, and the archived v3 boundary. Assert the superseded design and ADR contain an explicit superseded status rather than remaining simultaneously accepted.

- [ ] **Step 2: Run the focused test and confirm RED**

Run: `cargo test --manifest-path v4/Cargo.toml --test run_channel active_docs_match_the_minimal_protocol -- --exact`

Expected: FAIL because active docs still describe the larger discarded design.

- [ ] **Step 3: Simplify the domain and protocol documentation**

Retain only Run Pair, Role Session, Walkaway Run, Run Channel, Coordination Kernel, Baton, Handoff, Handoff Checkpoint, Adversarial Review, Human Decision, participant claim, worker, and reviewer. Mark delegated explicit-skill dispatch and the larger v4 controller designs superseded by issue #3. Keep every v3 reference explicitly historical.

- [ ] **Step 4: Run the automated verification suite**

Run: `cargo fmt --manifest-path v4/Cargo.toml -- --check && cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings && cargo test --manifest-path v4/Cargo.toml && cargo test --manifest-path rust/Cargo.toml && git diff --check`

Expected: formatting, clippy, both Rust suites, and whitespace checks pass.

- [ ] **Step 5: Prepare the disposable current-harness canary handoff**

Document the two exact startup prompts and local commands for one Codex worker and one Claude reviewer against a disposable run directory and artifact-only objective. The human must start both harness sessions; one harness must not launch the other. Once both exist, confirm each foreground waiter resumes from a local Baton change without a T3 event or tracker update and exercise one findings/revision cycle plus approval/finalization. Do not install the binary globally, touch a real project branch, or publish external state.

- [ ] **Step 6: Record canary evidence and commit**

Record only commands, versions, observed transitions, and sanitized results in the protocol document; do not include transcripts or hidden reasoning.

```bash
git add CONTEXT.md docs/adr docs/protocol docs/superpowers/specs v4/README.md v4/tests
git commit -m "docs: define the minimal run baton protocol"
```

### Task 9: Final verification and handoff

**Files:**
- Modify only files required to fix verification failures introduced by Tasks 1–8.

**Interfaces:**
- Produces a clean, non-publishable v4 implementation with issue #3 traceability and no v3 distribution changes.

- [ ] **Step 1: Run the complete verification matrix**

Run: `cargo fmt --manifest-path v4/Cargo.toml -- --check && cargo clippy --manifest-path v4/Cargo.toml --all-targets -- -D warnings && cargo test --manifest-path v4/Cargo.toml && cargo test --manifest-path rust/Cargo.toml && git diff --check`

Expected: every command exits `0` with no warnings promoted by clippy and no test failures.

- [ ] **Step 2: Audit scope and distribution boundaries**

Run: `git diff origin/main...HEAD --stat && git diff origin/main...HEAD -- rust/dvandva plugins .github`

Expected: v3 implementation, plugin source, and distribution workflows contain no successor implementation changes; any pre-existing branch documentation is clearly separated from new v4 code.

- [ ] **Step 3: Review commits and working tree**

Run: `git status --short && git log --oneline origin/main..HEAD`

Expected: only intentional files remain changed, commits are semantic and recoverable, and no temporary run directories, tokens, or private artifacts are tracked.

- [ ] **Step 4: Prepare the handoff**

Report what changed, exact verification commands and results, any deferred work, current branch/commit, and the next `gh stack` command. Do not push, create a PR, merge, install, publish, or release without separate authorization.
