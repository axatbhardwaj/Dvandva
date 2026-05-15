#!/usr/bin/env bash
# Smoke-test the installable Dvandva plugin package from a temp marketplace.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_PARENT="${DVANDVA_TMPDIR:-/tmp}"
TMP_DIR="$(mktemp -d "$TMP_PARENT/dvandva-smoke.XXXXXX")"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "FAIL: required command not found: $1" >&2
    exit 1
  fi
}

run() {
  echo "SMOKE: $*"
  "$@"
}

need_cmd claude
need_cmd codex
need_cmd jq
need_cmd python3

MARKETPLACE_ROOT="$TMP_DIR/marketplace"
PLUGIN_DIR="$MARKETPLACE_ROOT/plugins/dvandva"

mkdir -p "$MARKETPLACE_ROOT/plugins"
mkdir -p "$MARKETPLACE_ROOT/.agents/plugins"
cp -R "$ROOT_DIR/.claude-plugin" "$MARKETPLACE_ROOT/.claude-plugin"
cp "$ROOT_DIR/.agents/plugins/marketplace.json" "$MARKETPLACE_ROOT/.agents/plugins/marketplace.json"
cp -R "$ROOT_DIR/plugins/dvandva" "$PLUGIN_DIR"

run claude plugin validate "$PLUGIN_DIR"
run claude plugin validate "$MARKETPLACE_ROOT"

mkdir -p "$TMP_DIR/codex-home"
run env CODEX_HOME="$TMP_DIR/codex-home" codex plugin marketplace add "$MARKETPLACE_ROOT"
grep -q 'source = "' "$TMP_DIR/codex-home/config.toml"

CODEX_USER_HOME="$TMP_DIR/codex-user-home"
mkdir -p "$CODEX_USER_HOME"
run env \
  CODEX_HOME="$TMP_DIR/codex-home" \
  HOME="$CODEX_USER_HOME" \
  MARKETPLACE_PATH="$MARKETPLACE_ROOT/.agents/plugins/marketplace.json" \
  MARKETPLACE_CWD="$MARKETPLACE_ROOT" \
  python3 - <<'PY'
import json
import os
import select
import subprocess
import sys
import time


def send(proc, request_id, method, params=None):
    message = {"id": request_id, "method": method}
    if params is not None:
        message["params"] = params
    proc.stdin.write(json.dumps(message) + "\n")
    proc.stdin.flush()


def notify(proc, method):
    proc.stdin.write(json.dumps({"method": method}) + "\n")
    proc.stdin.flush()


def read_response(proc, request_id, timeout=15):
    deadline = time.time() + timeout
    while time.time() < deadline:
        readable, _, _ = select.select([proc.stdout, proc.stderr], [], [], 0.5)
        for stream in readable:
            line = stream.readline()
            if not line:
                continue
            if stream is proc.stderr:
                continue
            payload = json.loads(line)
            if payload.get("id") == request_id:
                return payload
    raise RuntimeError(f"timed out waiting for app-server response id={request_id}")


env = os.environ.copy()
proc = subprocess.Popen(
    ["codex", "app-server", "--listen", "stdio://"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
    env=env,
)

try:
    send(proc, 1, "initialize", {
        "clientInfo": {"name": "dvandva-smoke", "version": "0"},
        "capabilities": {"experimentalApi": True},
    })
    read_response(proc, 1)
    notify(proc, "initialized")

    send(proc, 2, "plugin/list", {
        "cwds": [os.environ["MARKETPLACE_CWD"]],
        "marketplaceKinds": ["local"],
    })
    response = read_response(proc, 2)
    marketplaces = response["result"]["marketplaces"]
    plugins = [plugin for marketplace in marketplaces for plugin in marketplace["plugins"]]
    if not any(plugin["id"] == "dvandva@dvandva" for plugin in plugins):
        raise RuntimeError("dvandva@dvandva was not listed in the Codex marketplace")

    send(proc, 3, "plugin/install", {
        "marketplacePath": os.environ["MARKETPLACE_PATH"],
        "pluginName": "dvandva",
        "remoteMarketplaceName": None,
    })
    read_response(proc, 3)

    send(proc, 4, "plugin/list", {
        "cwds": [os.environ["MARKETPLACE_CWD"]],
        "marketplaceKinds": ["local"],
    })
    response = read_response(proc, 4)
    installed = [
        plugin
        for marketplace in response["result"]["marketplaces"]
        for plugin in marketplace["plugins"]
        if plugin["id"] == "dvandva@dvandva"
        and plugin["installed"]
        and plugin["enabled"]
    ]
    if not installed:
        raise RuntimeError("dvandva@dvandva was not installed and enabled")
finally:
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)
PY

CODEX_SKILLS_TXT="$TMP_DIR/codex-skills.txt"
env \
  CODEX_HOME="$TMP_DIR/codex-home" \
  HOME="$CODEX_USER_HOME" \
  codex debug prompt-input "probe dvandva skills" \
  | jq -r '.. | strings? // empty' > "$CODEX_SKILLS_TXT"
grep -q 'dvandva:prativadi' "$CODEX_SKILLS_TXT"
grep -q 'dvandva:vadi' "$CODEX_SKILLS_TXT"

run jq empty \
  "$MARKETPLACE_ROOT/.agents/plugins/marketplace.json" \
  "$PLUGIN_DIR/.claude-plugin/plugin.json" \
  "$PLUGIN_DIR/.codex-plugin/plugin.json" \
  "$PLUGIN_DIR/references/baton-schema.json"

run "$PLUGIN_DIR/skills/vadi/scripts/dvandva-wait.sh" \
  --role vadi \
  --file "$PLUGIN_DIR/references/baton-schema.json" \
  --interval 0 \
  --max-wait 0

jq '.assignee = "prativadi" | .status = "spec_review" | .review_target = "spec"' \
  "$PLUGIN_DIR/references/baton-schema.json" > "$TMP_DIR/prativadi-baton.json"
run "$PLUGIN_DIR/skills/prativadi/scripts/dvandva-wait.sh" \
  --role prativadi \
  --file "$TMP_DIR/prativadi-baton.json" \
  --interval 0 \
  --max-wait 0

DEV_HOME="$TMP_DIR/dev-home"
mkdir -p "$DEV_HOME/.claude/skills" "$DEV_HOME/.agents/skills"
cp -R "$PLUGIN_DIR/skills/vadi" "$DEV_HOME/.claude/skills/vadi"
cp -R "$PLUGIN_DIR/skills/prativadi" "$DEV_HOME/.agents/skills/prativadi"

run bash "$ROOT_DIR/scripts/lint-skills.sh" "$DEV_HOME/.claude/skills/vadi/SKILL.md"
run bash "$ROOT_DIR/scripts/lint-skills.sh" "$DEV_HOME/.agents/skills/prativadi/SKILL.md"
run "$DEV_HOME/.claude/skills/vadi/scripts/dvandva-wait.sh" \
  --role vadi \
  --file "$PLUGIN_DIR/references/baton-schema.json" \
  --interval 0 \
  --max-wait 0

echo "SMOKE: plugin install surfaces passed"
