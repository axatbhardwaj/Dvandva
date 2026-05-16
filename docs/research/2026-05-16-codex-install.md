# Codex Plugin Install Discovery

**Date:** 2026-05-16
**Context:** Dvandva protocol-ergonomics run (Phase 1 of 5). Informs Phase 4 (Codex slash commands) and Phase 5 (Codex install one-liner). Verified against `codex-cli 0.130.0` on Linux.

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

## Q2: Does Codex expose a non-interactive `plugin install` CLI?

**No.** As of the local `codex` CLI version installed today:

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

The only `codex plugin` subcommand is `marketplace`. No `install`, no `add`, no `enable`. The CLI route for non-interactive plugin install is closed.

## Q3: What is the JSON-RPC install path?

**Codex exposes plugin install through the experimental `app-server` JSON-RPC interface.** The smoke script at `scripts/smoke-plugin-install.sh:92-149` drives it end-to-end:

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

`codex app-server --help` confirms the surface is **experimental** but stable enough that the existing smoke depends on it for CI. The `--listen stdio://` transport is the default.

**Implication for Phase 5:** Shape A (programmatic wrapper) is viable via the RPC backend. Shape A's `scripts/install-codex.sh` should embed the same `initialize` → `plugin/install` sequence used by the smoke script.

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

**Shape A — programmatic wrapper, RPC backend.**

Justification:

- Q2 closed the CLI route (`codex plugin install <name>` does not exist).
- Q3 proves the RPC route works non-interactively. The smoke script has been exercising it successfully for v0.1.0; the install half is easily extracted into a standalone script per the plan's Step 5.A.2-RPC pseudocode.
- Shape B (docs-only) is unnecessary here — we have a working backend.
- The user gets a real one-liner: `bash scripts/install-codex.sh`. Friction goes from 3 manual steps to 0.

Backend = **app-server JSON-RPC**, specifically the `initialize` → `plugin/install` sequence over `codex app-server --listen stdio://`.

## Open questions for follow-up runs

- **Is the app-server protocol stable?** It's flagged `[experimental]` in `codex --help`. If it churns, `install-codex.sh` will break. Worth tracking via an issue on the Codex repo and pinning to a documented protocol version if one becomes available.
- **Is `experimentalApi: true` in the `initialize` capabilities the right opt-in long-term?** The smoke passes it; semantically it gates feature visibility. May need to revisit if a stable API emerges.
- **Should we ship `codex plugin install` upstream?** A user-facing non-interactive install CLI would let us delete `install-codex.sh` entirely. Worth filing a feature request on the Codex repository, even though we're not blocked on it.
- **Slash-command argument plumbing:** the Dvandva `/goal` blocks don't take arguments; the `argument-hint` and `$ARGUMENTS` interpolation in command files are unused for our use case. If we later want `/dvandva:supervised-vadi` or other variants, we can either ship multiple command files or accept an argument.
