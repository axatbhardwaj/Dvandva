# Codex Plugin Install Discovery

**Date:** 2026-05-16
**Context:** Dvandva protocol-ergonomics run (Phase 1 of 5). Informs Phase 4 (Codex slash commands) and Phase 5 (Codex install one-liner). Verified against `codex-cli 0.130.0` on Linux.

**2026-06-27 update:** Current local verification against `codex-cli 0.142.3`
shows Codex now exposes a stable non-interactive install command:
`codex plugin add dvandva@dvandva`. `scripts/install-codex.sh` now uses that
command as the primary path after `codex plugin marketplace add`, and keeps the
app-server JSON-RPC method below only as a fallback for older Codex builds.

## Q1: What does `codex plugin marketplace add <path>` write to disk?

It writes a `[marketplaces.<name>]` table into `$CODEX_HOME/config.toml` (default `~/.codex/config.toml`). A local smoke with `CODEX_HOME=<tmp>` produced this shape:

```toml
[marketplaces.dvandva]
last_updated = "2026-05-16T13:21:22Z"
source_type = "local"
source = "/home/xzat/personal/Dvandva"
```

The smoke script at `scripts/smoke-plugin-install.sh:44-45` exercises the same config write:

```bash
run env CODEX_HOME="$TMP_DIR/codex-home" codex plugin marketplace add "$MARKETPLACE_ROOT"
grep -q 'source = "' "$TMP_DIR/codex-home/config.toml"
```

So `marketplace add` is purely a config write — it registers a marketplace location but does not install any plugins from it.

## Q2: Does Codex expose a non-interactive plugin install CLI?

**Historical answer for `codex-cli 0.130.0`: no.** As of the local `codex` CLI
version installed on 2026-05-16:

```
$ codex plugin --help
Manage Codex plugins

Usage: codex plugin [OPTIONS] <COMMAND>

Commands:
  marketplace  Manage plugin marketplaces for Codex
  help         Print this message or the help of the given subcommand(s)
```

```
$ codex plugin install --help
error: unrecognized subcommand 'install'

Usage: codex plugin [OPTIONS] <COMMAND>
```

The only `codex plugin` subcommand was `marketplace`. No `install`, no `add`,
no `enable`. The CLI route for non-interactive plugin install was closed.

**Current answer for `codex-cli 0.142.3`: yes.**

```
$ codex plugin add --help
Install a plugin from a configured marketplace snapshot.

Usage: codex plugin add [OPTIONS] <PLUGIN[@MARKETPLACE]>

Examples:
  codex plugin add sample@debug
  codex plugin add sample --marketplace debug
```

For Dvandva the current path is:

```bash
codex plugin marketplace add axatbhardwaj/Dvandva
codex plugin add dvandva@dvandva
```

## Q3: What is the JSON-RPC install path?

**Historical fallback:** Codex exposes plugin install through the experimental
`app-server` JSON-RPC interface. Older Dvandva installers drove it end-to-end:

```python
# Launch the server speaking JSON-RPC over stdio
proc = subprocess.Popen(
    ["codex", "app-server", "--listen", "stdio://"],
    stdin=PIPE, stdout=PIPE, stderr=PIPE, text=True,
)
# Initialize
send(proc, 1, "initialize", {
    "clientInfo": {"name": "...", "version": "0"},
    "capabilities": {"experimentalApi": True},
})
notify(proc, "initialized")
# Install
send(proc, 3, "plugin/install", {
    "marketplacePath": <abs path to marketplace.json>,
    "pluginName": "dvandva",
    "remoteMarketplaceName": None,
})
```

The sequence is `initialize` → `initialized` notification → `plugin/install` request. Verification via a follow-up `plugin/list` confirms `installed: true, enabled: true`.

`codex app-server --help` confirmed the surface was **experimental** but usable.
Current smoke tests no longer depend on it because `codex plugin add` is
available; `scripts/install-codex.sh` keeps the RPC sequence only as a legacy
fallback.

**Updated implication for Phase 5:** Shape A (programmatic wrapper) remains
viable, but the backend should be `codex plugin add` first and RPC fallback only
when `codex plugin add --help` is unavailable.

## Q4: What schema does Codex use for slash commands shipped from a plugin?

**Format:** Markdown files at `<plugin-root>/commands/<command-name>.md` (NOT inside `.codex-plugin/`). Auto-discovered — no need to reference them from `plugin.json`.

**Frontmatter keys** (YAML, validated by example plugins under `~/.codex/.tmp/plugins/plugins/`):

| Key | Required | Purpose |
|---|---|---|
| `description` | yes | One-line summary surfaced in `/skills` and slash-command listings |
| `argument-hint` | no | Placeholder shown for the command's `$ARGUMENTS` value |
| `allowed-tools` | no | YAML list of tool names the command is allowed to use |

**Example** (from `~/.codex/.tmp/plugins/plugins/cloudflare/commands/build-agent.md:1-5`):

```markdown
---
description: Build an AI agent on Cloudflare using the Agents SDK
argument-hint: [agent-description]
allowed-tools: [Read, Glob, Grep, Bash, Write, Edit, WebFetch]
---

# Build AI Agent on Cloudflare
... (body is the prompt injected when the command is invoked)
```

**Invocation syntax:** `/<plugin-name>:<command-name>`. The cloudflare plugin's `build-agent.md` is invoked as `/cloudflare:build-agent`, per its own example block (`/cloudflare:build-agent a customer support chatbot`). The plugin name comes from `.codex-plugin/plugin.json` `name` field.

**Confirmed via:** Looking at multiple installed plugins (cloudflare, vercel, expo, figma, build-macos-apps). All use the same convention: `commands/<name>.md` at plugin root, no `commands` field in `plugin.json`, colon-separated invocation.

**Implications for Phase 4 plan adjustment (handback finding for prativadi or implementing-vadi):**

1. Target paths in the plan are wrong: `plugins/dvandva/.codex-plugin/commands/dvandva-vadi.<ext>` should be `plugins/dvandva/commands/<name>.md`.
2. Slash command naming: the user picked `/dvandva-vadi` and `/dvandva-prativadi`, but Codex's convention is `/<plugin>:<command>`. To get `/dvandva-vadi` you'd need a plugin literally named `dvandva-vadi`, which would be confusing. Two viable interpretations:
   - **(a) `/dvandva:vadi` and `/dvandva:prativadi`** — commands named `vadi.md` and `prativadi.md`. Matches Codex convention. Cost: invocation syntax differs slightly from the user's stated preference (colon vs hyphen).
   - **(b) `/dvandva:walkaway-vadi` and `/dvandva:walkaway-prativadi`** — commands named `walkaway-vadi.md` and `walkaway-prativadi.md`. Closer in spirit to the verbose form discussed during scoping. Still uses required colon.
3. The plan's Phase 4 step about updating `.codex-plugin/plugin.json` with a `commands` field is unnecessary — commands are auto-discovered.

## Q5: Recommended Phase 5 shape

**Shape A — programmatic wrapper, current CLI backend.**

Justification:

- Q2 is now open via `codex plugin add <plugin>@<marketplace>`.
- Q3 remains useful only as a fallback for older Codex builds.
- Shape B (docs-only) is unnecessary here — we have a working backend.
- The user gets a real one-liner: `bash scripts/install-codex.sh`. Friction goes from 3 manual steps to 0.

Backend = **current Codex CLI**, specifically:

```bash
codex plugin marketplace add <repo-or-path>
codex plugin add dvandva@dvandva
```

Legacy fallback = **app-server JSON-RPC**, specifically the `initialize` →
`plugin/install` sequence over `codex app-server --listen stdio://`.

## Open questions for follow-up runs

- **Can the app-server fallback eventually be deleted?** Keep it until the
  minimum supported Codex version is known to include `codex plugin add`.
- **Is `experimentalApi: true` in the `initialize` capabilities the right opt-in long-term?** The smoke passes it; semantically it gates feature visibility. May need to revisit if a stable API emerges.
- **Does Codex eventually rename `plugin add` to `plugin install`?** If it does,
  update `scripts/install-codex.sh`, README, and this note together.
- **Slash-command argument plumbing:** the Dvandva `/goal` blocks don't take arguments; the `argument-hint` and `$ARGUMENTS` interpolation in command files are unused for our use case. If we later want `/dvandva:supervised-vadi` or other variants, we can either ship multiple command files or accept an argument.
