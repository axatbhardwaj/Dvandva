# Dvandva — retired

> **Dvandva is retired and archived.** It is no longer maintained or supported for new use.
> This repository is preserved as historical evidence for study. Do not
> install the kernel or role skills, and do not start a Dvandva run.
>
> **Replacement (personal setup, not distributed here).** The maintainer now
> works through a personal Astra-led [Paseo](https://github.com/getpaseo/paseo)
> setup: one driver owns scope and routing, dispatching specialist profiles
> only when they improve the result. Paseo owns agent and session
> orchestration; there is no Dvandva-style workflow state engine. This
> configuration is personal — it is not an installable replacement shipped by
> this repository.
>
> **Why:** the replacement removes the extra baton and workflow-state layer.
> A single driver coordinates bounded specialists, immutable-candidate review,
> and concise handoff artifacts through Paseo's existing orchestration skills.

The private kernel and the `setup-dvandva`, `vadi`, `prativadi`, and
`html-deliverables` skills have been removed from the maintainer's Codex and
Claude Code installations. Saved run history and backups are preserved.

## Replacement roles

| Profile | Model / effort | Job |
|---|---|---|
| `astra-driver` | Astra, medium | Owns scope, acceptance, routing, synthesis, decisions, recovery |
| `fable-planner` | Fable 5.1, high | Planning partner for substantial design choices |
| `research-sonnet` | Sonnet, medium | Requirements, documents, prior decisions, alternatives |
| `research-sol-medium` | Sol, medium | Code, dependencies, tests, implementation constraints |
| `research-grok` | Grok 4.6 | Targeted current external research |
| `implement-sol-high` | Sol, high | Implementation, tests, repairs |
| `review-opus` | Opus, high | One `code-review` (Standards + Spec) per applicable immutable Git candidate |
| `docs-muse` | Muse Spark, medium, free via OpenCode | Documentation and explainers from accepted evidence |

Tiny tasks stay direct — specialists are launched only when useful. A changed
revision is a new candidate and needs a fresh review. Matt Pocock's skills
remain available, with their human-only invocation boundaries preserved.
Ordinary authorized work has no automatic `/implement` gate.

## Historical record

The v4 skill-only manual below is preserved as unsupported history: it keeps
the retired installer, role prompts, release facts, and validator references
as archival evidence instead of repeating them as onboarding. For the
protocols, workflows, and decisions behind it, see
[`docs/protocol/minimal-run-baton.md`](docs/protocol/minimal-run-baton.md),
[`docs/workflows/skill-only-run.md`](docs/workflows/skill-only-run.md), and
[`docs/adr/`](docs/adr/).

<details>
<summary>Historical v4 manual (unsupported — archival evidence, not instructions)</summary>

> Everything inside this section is archival evidence. Do not run the install,
> setup, or role commands below, and do not treat the installer, role prompts,
> release automation, or validator as active onboarding.

> **Dvandva v4 was the final skill-only interface.** The `skills-v0.4.2`
> GitHub release pairs the private, non-publishable kernel `0.4.2` with schema
> `dvandva.run.v2` and role API 2. Source checkout and tests are
> development-only.
>
> **Supported platform: Linux x86_64 only, for now.** The release ships one
> kernel asset and the installer refuses every other operating system or
> architecture. macOS and native Windows are not supported; on Windows, use
> WSL2. T3 Code runs the role sessions on Linux, which is the tested path.

## Historical v4 skill-only interface (retired)

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
standalone checks. Its answer-first contract puts the conclusion, importance,
current status, and next action before progressively disclosed technical
evidence. See the role-local
[`model-selection.md`](skills/vadi/references/model-selection.md).
Matt Pocock's user-invoked workflow skills still require human invocation;
prativadi automatically uses the model-invocable `code-review` skill for Git
checkpoints, with native review for analysis checkpoints or when that companion
is unavailable. See
[`docs/workflows/skill-only-run.md`](docs/workflows/skill-only-run.md).

## Five workflows

- **Discovery:** Claude Fable 5.1/high vadi and Codex Astra/high prativadi
  discover relevant docs and code, independently investigate, reconcile questions,
  and review a spec. A second linked run in the same sessions produces verified
  tickets. You explicitly invoke Matt's `/grill-with-docs`, `/to-spec`, and
  `/to-tickets` at their entry points; Dvandva does not invoke them for you.
- **Implementation:** fresh Codex Sol/high vadi and Claude Opus prativadi consume
  approved spec/tickets. Invoke `/implement` in vadi when using Matt's method.
- **Babysitting:** repair and maintain our scoped PRs, including CI, rebases and
  feedback, with separate human merge authority.
- **Review:** independently review others' PRs and re-review changed candidates
  until every frozen same-repository member has a verified current approval or
  explicit closed/merged disposition. A batch remains one complete manifest and
  one atomic run-level verdict; authors own fixes.
- **Freeflow:** produce evidence-backed investigations, reports, diagnostics,
  real-interface testing, or scoped test-and-fix results without manufacturing
  a spec/ticket pipeline. The pair keeps one current plan and independently
  verifies the complete immutable delivery.

For discovery, start Fable with:

```text
Act as vadi for discovery of <objective>.
/grill-with-docs
```

Docs are discovered automatically; supplied references are optional search seeds.
Launch Astra independently with `Act as prativadi for discovery of <objective>`.
Prativadi discovers available runs in the shared XDG registry and exact-joins
an unambiguous scope. Multiple matches require a choice; no match waits for vadi.
Vadi's exact peer join prompt remains an optional recovery shortcut, so a delayed
activation message does not prevent the pair from forming. The existing initial explainer review
records source verification and establishes the pair. Skill-invocation waits
show the pending command and resume the exact run on your next message.
See the [initiation contract](skills/vadi/references/initiation.md) and
[discovery contract](skills/vadi/references/discovery.md) for source manifests,
human decisions, checkpoint gates and linked-run receipts. These are role-skill
policies on the existing v2 kernel, not new schema-level guarantees. Legacy
`babysit` and one-shot `pr_review` runs preserve their original semantics.
Persistent Review can be discovered through any exact canonical member URL;
the selected run is then exact-joined and its complete frozen scope rechecked.

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

</details>
