#!/usr/bin/env bash
# One-shot Codex install for Dvandva.
#
# Backend: app-server JSON-RPC `plugin/install` over `codex app-server --listen stdio://`.
# This is the install half of scripts/smoke-plugin-install.sh:92-149, extracted as a
# user-facing wrapper. See docs/research/2026-05-16-codex-install.md for the full
# discovery (Q2 confirms no `codex plugin install <name>` CLI exists; Q3 confirms
# the RPC backend works non-interactively).
#
# Usage: bash scripts/install-codex.sh [<marketplace-path-or-repo>]
#
# Default marketplace: axatbhardwaj/Dvandva (the upstream repo).
# Override with a local path for development:
#   bash scripts/install-codex.sh /path/to/local/Dvandva
set -euo pipefail

MARKETPLACE="${1:-axatbhardwaj/Dvandva}"

if ! command -v codex >/dev/null 2>&1; then
  echo "ERROR: codex CLI not found on PATH" >&2
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "ERROR: python3 not found on PATH (needed for app-server RPC driver)" >&2
  exit 1
fi

echo "Step 1: registering marketplace '$MARKETPLACE'..."
codex plugin marketplace add "$MARKETPLACE"

# If a local-path marketplace was given, resolve to the absolute marketplace.json path
# the app-server expects. For remote repos, the marketplace.json lives under the
# registered location's standard layout.
if [[ -d "$MARKETPLACE" ]]; then
  MARKETPLACE_PATH="$(cd "$MARKETPLACE" && pwd)/.agents/plugins/marketplace.json"
else
  # Remote marketplace: the marketplace.json path inside the codex_home isn't trivially
  # exposed. Fall back to letting the RPC resolve by remote-marketplace name (the second
  # plugin/install param shape). We pass remoteMarketplaceName instead of marketplacePath.
  MARKETPLACE_PATH=""
fi

echo "Step 2: installing dvandva plugin via app-server RPC..."
python3 - "$MARKETPLACE" "$MARKETPLACE_PATH" <<'PY'
import json, os, select, subprocess, sys, time

REMOTE_NAME = sys.argv[1]
MARKETPLACE_PATH = sys.argv[2]

def send(proc, request_id, method, params=None):
    msg = {"id": request_id, "method": method}
    if params is not None:
        msg["params"] = params
    proc.stdin.write(json.dumps(msg) + "\n")
    proc.stdin.flush()

def notify(proc, method):
    proc.stdin.write(json.dumps({"method": method}) + "\n")
    proc.stdin.flush()

def read_response(proc, request_id, timeout=30):
    deadline = time.time() + timeout
    while time.time() < deadline:
        readable, _, _ = select.select([proc.stdout, proc.stderr], [], [], 0.5)
        for stream in readable:
            line = stream.readline()
            if not line or stream is proc.stderr:
                continue
            payload = json.loads(line)
            if payload.get("id") == request_id:
                return payload
    raise RuntimeError(f"timed out waiting for app-server response id={request_id}")

proc = subprocess.Popen(
    ["codex", "app-server", "--listen", "stdio://"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    text=True, env=os.environ.copy(),
)
try:
    send(proc, 1, "initialize", {
        "clientInfo": {"name": "dvandva-install", "version": "0"},
        "capabilities": {"experimentalApi": True},
    })
    read_response(proc, 1)
    notify(proc, "initialized")

    if MARKETPLACE_PATH:
        params = {
            "marketplacePath": MARKETPLACE_PATH,
            "pluginName": "dvandva",
            "remoteMarketplaceName": None,
        }
    else:
        params = {
            "marketplacePath": None,
            "pluginName": "dvandva",
            "remoteMarketplaceName": REMOTE_NAME,
        }
    send(proc, 2, "plugin/install", params)
    response = read_response(proc, 2)
    if response.get("error"):
        raise RuntimeError(f"plugin/install failed: {response['error']}")
    print("OK: dvandva@dvandva installed via app-server RPC")
finally:
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)
PY

echo "Done. Verify with: codex, then check /skills | grep dvandva and /dvandva:vadi"
