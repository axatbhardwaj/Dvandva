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
CODEX_AVAILABLE_JSON="$TMP_DIR/codex-available.json"
CODEX_INSTALL_JSON="$TMP_DIR/codex-install.json"
CODEX_INSTALLED_JSON="$TMP_DIR/codex-installed.json"
echo "SMOKE: env CODEX_HOME=$TMP_DIR/codex-home HOME=$CODEX_USER_HOME codex plugin list --available --json"
env CODEX_HOME="$TMP_DIR/codex-home" HOME="$CODEX_USER_HOME" \
  codex plugin list --available --json > "$CODEX_AVAILABLE_JSON"
jq -e '.available[] | select(.pluginId == "dvandva@dvandva" and .installed == false)' "$CODEX_AVAILABLE_JSON" >/dev/null

echo "SMOKE: env CODEX_HOME=$TMP_DIR/codex-home HOME=$CODEX_USER_HOME codex plugin add dvandva@dvandva --json"
env CODEX_HOME="$TMP_DIR/codex-home" HOME="$CODEX_USER_HOME" \
  codex plugin add dvandva@dvandva --json > "$CODEX_INSTALL_JSON"
jq -e '.pluginId == "dvandva@dvandva" and .name == "dvandva" and .marketplaceName == "dvandva"' "$CODEX_INSTALL_JSON" >/dev/null

echo "SMOKE: env CODEX_HOME=$TMP_DIR/codex-home HOME=$CODEX_USER_HOME codex plugin list --json"
env CODEX_HOME="$TMP_DIR/codex-home" HOME="$CODEX_USER_HOME" \
  codex plugin list --json > "$CODEX_INSTALLED_JSON"
jq -e '.installed[] | select(.pluginId == "dvandva@dvandva" and .installed == true and .enabled == true)' "$CODEX_INSTALLED_JSON" >/dev/null

CODEX_SKILLS_TXT="$TMP_DIR/codex-skills.txt"
env \
  CODEX_HOME="$TMP_DIR/codex-home" \
  HOME="$CODEX_USER_HOME" \
  codex debug prompt-input "probe dvandva skills" \
  | jq -r '.. | strings? // empty' > "$CODEX_SKILLS_TXT"
grep -q 'dvandva:prativadi' "$CODEX_SKILLS_TXT"
grep -q 'dvandva:vadi' "$CODEX_SKILLS_TXT"

# Phase 4: verify slash-command files are bundled into the plugin.
# Codex auto-discovers <plugin-root>/commands/<name>.md per
# docs/research/2026-05-16-codex-install.md Q4; invocation is
# /dvandva:vadi and /dvandva:prativadi.
test -f "$PLUGIN_DIR/commands/vadi.md" || { echo "FAIL: dvandva commands/vadi.md missing from bundled plugin" >&2; exit 1; }
test -f "$PLUGIN_DIR/commands/prativadi.md" || { echo "FAIL: dvandva commands/prativadi.md missing from bundled plugin" >&2; exit 1; }
grep -q '^description:' "$PLUGIN_DIR/commands/vadi.md" || { echo "FAIL: vadi.md missing required 'description' frontmatter key" >&2; exit 1; }
grep -q '^description:' "$PLUGIN_DIR/commands/prativadi.md" || { echo "FAIL: prativadi.md missing required 'description' frontmatter key" >&2; exit 1; }
grep -q '^/goal You are Dvandva vadi' "$PLUGIN_DIR/commands/vadi.md" || { echo "FAIL: vadi.md body missing /goal block" >&2; exit 1; }
grep -q '^/goal You are Dvandva prativadi' "$PLUGIN_DIR/commands/prativadi.md" || { echo "FAIL: prativadi.md body missing /goal block" >&2; exit 1; }
echo "SMOKE: dvandva slash commands bundled correctly"

# Phase 5: re-install via scripts/install.sh into fresh Claude/Codex homes
# to confirm the user-facing dual-engine one-liner works end-to-end.
SCRIPT_BOTH_CODEX_HOME="$TMP_DIR/codex-home-via-dual-script"
SCRIPT_BOTH_USER_HOME="$TMP_DIR/user-home-via-dual-script"
mkdir -p "$SCRIPT_BOTH_CODEX_HOME" "$SCRIPT_BOTH_USER_HOME"
run env CODEX_HOME="$SCRIPT_BOTH_CODEX_HOME" HOME="$SCRIPT_BOTH_USER_HOME" \
  bash "$ROOT_DIR/scripts/install.sh" "$MARKETPLACE_ROOT"
CODEX_DUAL_SKILLS_TXT="$TMP_DIR/codex-dual-skills.txt"
env \
  CODEX_HOME="$SCRIPT_BOTH_CODEX_HOME" \
  HOME="$SCRIPT_BOTH_USER_HOME" \
  codex debug prompt-input "probe dvandva skills after dual install" \
  | jq -r '.. | strings? // empty' > "$CODEX_DUAL_SKILLS_TXT"
grep -q 'dvandva:prativadi' "$CODEX_DUAL_SKILLS_TXT"
grep -q 'dvandva:vadi' "$CODEX_DUAL_SKILLS_TXT"
echo "SMOKE: install.sh dual-engine install passed"

# Phase 6: keep the Codex-only helper covered because scripts/install.sh
# delegates to it for Codex and older Codex builds still use its fallback path.
SCRIPT_CODEX_HOME="$TMP_DIR/codex-home-via-script"
SCRIPT_USER_HOME="$TMP_DIR/codex-user-home-via-script"
mkdir -p "$SCRIPT_CODEX_HOME" "$SCRIPT_USER_HOME"
run env CODEX_HOME="$SCRIPT_CODEX_HOME" HOME="$SCRIPT_USER_HOME" \
  bash "$ROOT_DIR/scripts/install-codex.sh" "$MARKETPLACE_ROOT"
echo "SMOKE: install-codex.sh end-to-end install passed"

run jq empty \
  "$MARKETPLACE_ROOT/.agents/plugins/marketplace.json" \
  "$PLUGIN_DIR/.claude-plugin/plugin.json" \
  "$PLUGIN_DIR/.codex-plugin/plugin.json" \
  "$PLUGIN_DIR/references/baton-schema.json" \
  "$PLUGIN_DIR/references/baton-schema-v2.json"

jq -e '.turn_cap == 60' "$PLUGIN_DIR/references/baton-schema.json" >/dev/null
jq -e '.turn_cap == 60' "$PLUGIN_DIR/references/baton-schema-v2.json" >/dev/null

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

# Exercise both bundled write helpers: scaffold then one legal transition.
WRITE_BOX="$TMP_DIR/write-helper"
mkdir -p "$WRITE_BOX"
jq '.status = "spec_drafting" | .assignee = "vadi" | .checkpoint = 0 | .master_plan_locked = false | .question = null | .resume_assignee = null | .resume_status = null' \
  "$PLUGIN_DIR/references/baton-schema.json" > "$WRITE_BOX/baton.next.json"
run "$PLUGIN_DIR/skills/vadi/scripts/dvandva-write.sh" \
  "$WRITE_BOX/baton.json" "$WRITE_BOX/baton.next.json"
jq '.status = "spec_review" | .assignee = "prativadi" | .review_target = "spec" | .checkpoint = 1' \
  "$WRITE_BOX/baton.json" > "$WRITE_BOX/baton.next.json"
run "$PLUGIN_DIR/skills/prativadi/scripts/dvandva-write.sh" \
  "$WRITE_BOX/baton.json" "$WRITE_BOX/baton.next.json"
test -f "$WRITE_BOX/history/0-spec_drafting-vadi.json" || { echo "FAIL: write helper did not snapshot checkpoint 0" >&2; exit 1; }
test -f "$WRITE_BOX/history/1-spec_review-prativadi.json" || { echo "FAIL: write helper did not snapshot checkpoint 1" >&2; exit 1; }

V2_WRITE_BOX="$TMP_DIR/write-helper-v2/.dvandva/runs/smoke"
mkdir -p "$V2_WRITE_BOX"
jq '.updated_at = "2026-06-27T00:00:00Z"
  | .run_id = "smoke"
  | .original_ask = "Smoke v2 helper"
  | .research_ref = "./superpowers/research/smoke.html"
  | .current_engine = "codex"
  | .branch = "smoke"
  | .status = "research_drafting"
  | .assignee = "vadi"
  | .checkpoint = 0
  | .master_plan_locked = false
  | .question = null
  | .resume_assignee = null
  | .resume_status = null' \
  "$PLUGIN_DIR/references/baton-schema-v2.json" > "$V2_WRITE_BOX/baton.next.json"
run "$PLUGIN_DIR/skills/vadi/scripts/dvandva-write.sh" \
  "$V2_WRITE_BOX/baton.json" "$V2_WRITE_BOX/baton.next.json"
test -f "$V2_WRITE_BOX/history/0-research_drafting-vadi.json" || { echo "FAIL: v2 write helper did not snapshot research scaffold" >&2; exit 1; }

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
