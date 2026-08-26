# Minimal Run Baton Protocol

Issue: [#3](https://github.com/axatbhardwaj/Dvandva/issues/3)

## Boundary

One run has one directory, one `baton.json`, one worker session, and one
reviewer session. The human starts the sessions separately in T3 Code. The
kernel never invokes Claude from Codex or Codex from Claude. GitHub, Linear,
Sites, and other systems may be recorded as opaque references; they do not
coordinate or wake the pair. The v4 crate is non-publishable and independent
of archived v3.5.1.

## State graph

```text
working -> reviewing -> finalizing -> done
              |             ^
              v             |
           revising --------+

any active state -> human_decision -> declared active state
any active state -> abandoned
```

The worker submits a new immutable checkpoint from `working` or `revising`.
The reviewer binds findings or approval to that exact identity. Finalization
requires an unchanged approved identity and, when publication is required, a
synchronized projection revision. Terminal state cannot be reopened.

## Storage and authority

- `.baton.lock` serializes writers within this run only.
- `baton.json` is flushed and atomically replaced.
- `history/<revision>.json` is immutable.
- every mutation supplies an expected revision;
- role claims store only a SHA-256 token digest;
- expired claims can be replaced at a higher epoch;
- `recover` validates a complete history prefix, creates a new revision, and
  clears both claims.

## Commands

Build with `cargo build --manifest-path v4/Cargo.toml`. The binary supports
`init`, `read`, `claim`, `heartbeat`, `reclaim`, `apply`, `wait`, and `recover`.
`apply` consumes a tagged JSON action file and requires role, session ID,
secret token, and expected revision. Run `dvandva-v4 <command> --help` for all
flags. Errors are single-line JSON diagnostics on stderr.

## Starting the pair

Worker session prompt:

> Join run `<RUN_DIR>` as the worker session. Claim `worker`, read the Baton,
> perform only the current objective, submit immutable checkpoints with
> verification, maintain publication references when requested, and use
> `wait` whenever the reviewer owns the next action. Do not invoke or launch
> Claude; coordinate only through this run directory.

Reviewer session prompt:

> Join run `<RUN_DIR>` as the reviewer session. Claim `reviewer`, adversarially
> review only the current checkpoint identity, record actionable findings or
> approval, and use `wait` whenever the worker owns the next action. Do not
> invoke or launch Codex; coordinate only through this run directory.

The human starts both sessions and passes each its own one-time claim token.
Tokens must not be committed, pasted into the Baton, or published.

## External workflow

Grilling, spec review, ticket decomposition, and explicit Matt Pocock skill
invocations occur before the implementation Run Pair or through deliberate
human turns. A plan may be projected as a published to-do list, but the Baton
remains authoritative. Codex normally records the projection before and after
handoffs; a stale required projection prevents `done` without changing who
owns the semantic task.

The current-harness canary is prepared by these prompts but remains pending
until a human starts both real harness sessions. Automated tests prove two
local processes and filesystem wake-up; they do not claim a cross-harness
canary was run.
