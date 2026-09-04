# Dvandva

> **Dvandva v4 is the active skill-only interface.** The `skills-v0.3.8`
> GitHub release pairs the private, non-publishable kernel `0.3.8` with schema
> `dvandva.run.v2` and role API 2. Source checkout and tests are
> development-only.
>
> **Supported platform: Linux x86_64 only, for now.** The release ships one
> kernel asset and the installer refuses every other operating system or
> architecture. macOS and native Windows are not supported; on Windows, use
> WSL2. T3 Code runs the role sessions on Linux, which is the tested path.

## Active v4 skill-only interface

Install the four source skills for Claude Code and Codex:

```bash
npx --yes skills add axatbhardwaj/Dvandva --global \
  --agent claude-code codex \
  --skill setup-dvandva vadi prativadi html-deliverables
```

On a Linux x86_64 host, explicitly invoke `$setup-dvandva` with an install
request. Setup verifies the GitHub asset's checksum and complete
v2/API2 probe before installing it under XDG data and outside `PATH`. The
kernel remains `publish = false`; no marketplace package is part of v4
distribution.

Start two independent T3 Code sessions for one ticket:

```text
Codex: Act as vadi and implement DEF-123 with deliverable implementation.
```

Vadi immediately returns the canonical run ID and this exact peer prompt:

```text
Act as prativadi and join Dvandva run <run-id>.
```

The vadi submits one complete immutable checkpoint for canonical scope. Each
work-carrying handoff opens an obligation: vadi stages the local digest-bound
HTML and prativadi reviews it. At run start, vadi proposes the first artifact before
continuing domain work and incorporates requested changes. Once approved,
whichever participant is Codex publishes the same digest through ChatGPT Sites
to one stable, owner-only status page the user can revisit for progress. If a pairing has
no Codex participant, Sites publication is skipped and local approval is the
gate. A work-carrying handoff replaces the obligation; an approval preserves it
with its receipts, so finalization checks only the current applicable receipts
and an approved delivery finalizes in one handshake. Only finalization waits on
that gate, so a finished deliverable can always be checkpointed, and the
`run_started` approval doubles as the join gate: vadi's `work` advisory waits
for prativadi's first receipt. The explainer plan is the live TODO
list; the Baton remains authoritative.

The sessions coordinate only through the local run. Neither harness invokes
the other, there is no daemon, and user-owned harness goals remain untouched.
For separate planning sessions, use Astra/Fable with the human's chosen skills
and hand off a concise approved plan. Implementation sessions run Sol at `high`
as Codex vadi and Opus as Claude prativadi; Astra/Fable are optional advisers.
The restored `html-deliverables` skill supplies the shared visual template and
standalone checks. See the role-local
[`model-selection.md`](skills/vadi/references/model-selection.md).
Matt Pocock's user-invoked workflow skills still require human invocation;
prativadi automatically uses the model-invocable `code-review` skill for Git
checkpoints, with native review for analysis checkpoints or when that companion
is unavailable. See
[`docs/workflows/skill-only-run.md`](docs/workflows/skill-only-run.md).

## Repo map

```
skills/
  setup-dvandva/                    # explicit-only kernel installer skill
  html-deliverables/                # shared HTML template and validation
  vadi/                             # implementer role skill
  prativadi/                        # reviewer role skill
v4/                                 # private kernel crate (publish = false) + tests
tests/skills/                       # release, setup, role, and canary suites
scripts/                            # release packaging and ref verification
docs/
  adr/                              # system-wide architecture decision records
  protocol/minimal-run-baton.md     # dvandva.run.v2 Run Baton protocol
  protocol/v4-git-discipline.md     # git discipline for role sessions
  workflows/skill-only-run.md       # capability evidence for the skill-only run
  dvandva-explainer.html            # visual explainer (live at axatbhardwaj.github.io/Dvandva/)
CONTEXT.md                          # domain glossary
product.md                          # product specification and acceptance criteria
```

### Reading order

1. `CONTEXT.md` — domain glossary
2. `docs/protocol/minimal-run-baton.md` — Run Baton protocol
3. `docs/workflows/skill-only-run.md` — skill-only run evidence
4. `docs/adr/` — architecture decisions

### Non-goals

- No runtime daemon, hidden central process, or process launcher.
- No GitHub API integration.
- No PR creation.
- No npm-first distribution path.
