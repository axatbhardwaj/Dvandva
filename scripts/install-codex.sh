#!/usr/bin/env bash
# One-shot Codex install for Dvandva.
#
# Current backend: `codex plugin add dvandva@dvandva`.
# Legacy fallback: app-server JSON-RPC `plugin/install` over
# `codex app-server --listen stdio://` for older Codex builds that do not expose
# `codex plugin add`.
#
# Usage: bash scripts/install-codex.sh [<marketplace-path-or-repo>]
#
# Default marketplace: axatbhardwaj/Dvandva (the upstream repo).
# Override with a local path for development:
#   bash scripts/install-codex.sh /path/to/local/Dvandva
set -euo pipefail

MARKETPLACE="${1:-axatbhardwaj/Dvandva}"

run_idempotent() {
  local label="$1"
  local output status
  shift

  if output="$("$@" 2>&1)"; then
    [[ -z "$output" ]] || printf '%s\n' "$output"
    return 0
  fi

  status=$?
  [[ -z "$output" ]] || printf '%s\n' "$output" >&2
  if printf '%s\n' "$output" | grep -Eiq 'already|exists|registered|installed|duplicate'; then
    echo "$label already present; continuing."
    return 0
  fi

  return "$status"
}

if ! command -v codex >/dev/null 2>&1; then
  echo "ERROR: codex CLI not found on PATH" >&2
  exit 1
fi

echo "Step 1: registering marketplace '$MARKETPLACE'..."
run_idempotent "Codex marketplace" codex plugin marketplace add "$MARKETPLACE"

if codex plugin add --help >/dev/null 2>&1; then
  echo "Step 2: installing dvandva plugin with: codex plugin add dvandva@dvandva"
  run_idempotent "Codex plugin" codex plugin add dvandva@dvandva
  echo "Done. Verify with: codex, then check /skills for dvandva:vadi, dvandva:prativadi, dvandva:research, dvandva:testing, dvandva:understanding, and dvandva:worktree-setup."
  exit 0
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "ERROR: python3 not found on PATH (needed for legacy app-server RPC fallback)" >&2
  exit 1
fi

# Resolve the marketplace.json path the app-server expects. Even for remote
# repos, prefer marketplacePath over remoteMarketplaceName: remoteMarketplaceName
# asks the app-server to read the hosted catalog and can require ChatGPT auth,
# while marketplacePath uses the checkout that `marketplace add` just cached.
if [[ -d "$MARKETPLACE" ]]; then
  MARKETPLACE_PATH="$(cd "$MARKETPLACE" && pwd)/.agents/plugins/marketplace.json"
else
  CODEX_HOME_DIR="${CODEX_HOME:-$HOME/.codex}"
  MARKETPLACE_NAME="$(basename "${MARKETPLACE%.git}")"
  MARKETPLACE_NAME="$(printf '%s' "$MARKETPLACE_NAME" | tr '[:upper:]' '[:lower:]')"
  MARKETPLACE_PATH="$CODEX_HOME_DIR/.tmp/marketplaces/$MARKETPLACE_NAME/.agents/plugins/marketplace.json"
  if [[ ! -f "$MARKETPLACE_PATH" ]]; then
    MARKETPLACE_PATH="$(
      find "$CODEX_HOME_DIR/.tmp/marketplaces" -path '*/.agents/plugins/marketplace.json' -type f -print 2>/dev/null \
        | while IFS= read -r candidate; do
            if grep -q '"name"[[:space:]]*:[[:space:]]*"dvandva"' "$candidate"; then
              printf '%s\n' "$candidate"
              break
            fi
          done
    )"
  fi
fi

if [[ -z "${MARKETPLACE_PATH:-}" || ! -f "$MARKETPLACE_PATH" ]]; then
  echo "ERROR: could not find Dvandva marketplace.json after marketplace registration" >&2
  exit 1
fi

echo "Step 2: installing dvandva plugin via legacy app-server RPC fallback..."
python3 - "$MARKETPLACE" "$MARKETPLACE_PATH" <<'PY'
import json, os, select, subprocess, sys, time

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

    params = {
        "marketplacePath": MARKETPLACE_PATH,
        "pluginName": "dvandva",
        "remoteMarketplaceName": None,
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

echo "Done. Verify with: codex, then check /skills for dvandva:vadi, dvandva:prativadi, dvandva:research, dvandva:testing, dvandva:understanding, and dvandva:worktree-setup."
