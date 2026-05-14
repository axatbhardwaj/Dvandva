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

MARKETPLACE_ROOT="$TMP_DIR/marketplace"
PLUGIN_DIR="$MARKETPLACE_ROOT/plugins/dvandva"

mkdir -p "$MARKETPLACE_ROOT/plugins"
cp -R "$ROOT_DIR/.claude-plugin" "$MARKETPLACE_ROOT/.claude-plugin"
cp -R "$ROOT_DIR/plugins/dvandva" "$PLUGIN_DIR"

run claude plugin validate "$PLUGIN_DIR"
run claude plugin validate "$MARKETPLACE_ROOT"

mkdir -p "$TMP_DIR/codex-home"
run env CODEX_HOME="$TMP_DIR/codex-home" codex plugin marketplace add "$MARKETPLACE_ROOT"
grep -q 'source = "' "$TMP_DIR/codex-home/config.toml"

run jq empty \
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
