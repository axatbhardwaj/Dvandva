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

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

readonly EXPECTED_DVANDVA_VERSION="1.0.0"
readonly EXPECTED_AGENT_IDS=(
  "dvandva-adversarial-analyst"
  "dvandva-architect"
  "dvandva-baton-auditor"
  "dvandva-cross-reviewer"
  "dvandva-debugger"
  "dvandva-deep-reviewer"
  "dvandva-deslopper"
  "dvandva-doc-verifier"
  "dvandva-implementer"
  "dvandva-integration-checker"
  "dvandva-pattern-mapper"
  "dvandva-researcher"
  "dvandva-sandbox-verifier"
  "dvandva-security-auditor"
  "dvandva-test-creator"
)

collect_agent_ids() {
  local agents_dir="$1"
  local file

  find "$agents_dir" -maxdepth 1 -type f -name '*.md' -exec basename {} \; \
    | sort \
    | while IFS= read -r file; do
        printf 'dvandva-%s\n' "${file%.md}"
      done
}

assert_source_manifest_version_parity() {
  local marketplace_version claude_version codex_version

  marketplace_version="$(
    jq -r '.plugins[] | select(.name == "dvandva") | .version' \
      "$ROOT_DIR/.claude-plugin/marketplace.json"
  )"
  claude_version="$(jq -r '.version' "$ROOT_DIR/plugins/dvandva/.claude-plugin/plugin.json")"
  codex_version="$(jq -r '.version' "$ROOT_DIR/plugins/dvandva/.codex-plugin/plugin.json")"

  [[ -n "$marketplace_version" && "$marketplace_version" != "null" ]] \
    || fail "missing marketplace version in .claude-plugin/marketplace.json"
  [[ "$marketplace_version" == "$claude_version" ]] \
    || fail "version mismatch: marketplace=$marketplace_version claude-plugin=$claude_version"
  [[ "$marketplace_version" == "$codex_version" ]] \
    || fail "version mismatch: marketplace=$marketplace_version codex-plugin=$codex_version"
  [[ "$marketplace_version" == "$EXPECTED_DVANDVA_VERSION" ]] \
    || fail "expected Dvandva plugin version $EXPECTED_DVANDVA_VERSION, got $marketplace_version"
}

roster_matches_expected() {
  local agents_dir="$1"
  local actual expected

  actual="$(collect_agent_ids "$agents_dir")"
  expected="$(printf '%s\n' "${EXPECTED_AGENT_IDS[@]}")"
  [[ "$actual" == "$expected" ]]
}

require_exact_agent_roster() {
  local agents_dir="$1"
  local label="$2"
  local actual expected

  actual="$(collect_agent_ids "$agents_dir")"
  expected="$(printf '%s\n' "${EXPECTED_AGENT_IDS[@]}")"
  [[ "$actual" == "$expected" ]] || {
    printf 'Expected agent roster:\n%s\nActual agent roster:\n%s\n' "$expected" "$actual" >&2
    fail "$label agent roster did not match the expected 15-agent Dvandva set"
  }
}

require_installed_codex_cache() {
  local codex_home="$1"
  local label="$2"
  local plugin_root="$codex_home/plugins/cache/dvandva/dvandva/$EXPECTED_DVANDVA_VERSION"

  test -d "$plugin_root" || fail "$label missing Codex cache at $plugin_root"
  jq -e --arg version "$EXPECTED_DVANDVA_VERSION" '.version == $version' \
    "$plugin_root/.claude-plugin/plugin.json" >/dev/null \
    || fail "$label cached Claude manifest version mismatch"
  jq -e --arg version "$EXPECTED_DVANDVA_VERSION" '.version == $version' \
    "$plugin_root/.codex-plugin/plugin.json" >/dev/null \
    || fail "$label cached Codex manifest version mismatch"
  require_exact_agent_roster "$plugin_root/agents" "$label cached"
}

require_codex_skill_surface() {
  local file="$1"
  for skill in \
    dvandva:prativadi \
    dvandva:vadi \
    dvandva:research \
    dvandva:testing \
    dvandva:understanding \
    dvandva:worktree-setup; do
    grep -q "$skill" "$file" || {
      echo "FAIL: installed Codex skill surface missing $skill in $file" >&2
      exit 1
    }
  done
}

need_cmd claude
need_cmd codex
need_cmd jq

assert_source_manifest_version_parity
require_exact_agent_roster "$ROOT_DIR/plugins/dvandva/agents" "source"

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

require_installed_codex_cache "$TMP_DIR/codex-home" "direct Codex install"

CODEX_SKILLS_TXT="$TMP_DIR/codex-skills.txt"
env \
  CODEX_HOME="$TMP_DIR/codex-home" \
  HOME="$CODEX_USER_HOME" \
  codex debug prompt-input "probe dvandva skills" \
  | jq -r '.. | strings? // empty' > "$CODEX_SKILLS_TXT"
require_codex_skill_surface "$CODEX_SKILLS_TXT"

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

# Phase 4b: verify absorbed skill directories ship with a non-empty SKILL.md.
test -s "$PLUGIN_DIR/skills/research/SKILL.md" || { echo "FAIL: dvandva skills/research/SKILL.md missing or empty from bundled plugin" >&2; exit 1; }
test -s "$PLUGIN_DIR/skills/understanding/SKILL.md" || { echo "FAIL: dvandva skills/understanding/SKILL.md missing or empty from bundled plugin" >&2; exit 1; }
test -s "$PLUGIN_DIR/skills/testing/SKILL.md" || { echo "FAIL: dvandva skills/testing/SKILL.md missing or empty from bundled plugin" >&2; exit 1; }
test -s "$PLUGIN_DIR/skills/worktree-setup/SKILL.md" || { echo "FAIL: dvandva skills/worktree-setup/SKILL.md missing or empty from bundled plugin" >&2; exit 1; }
echo "SMOKE: runtime skills (research, understanding, testing, worktree-setup) bundled correctly"

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
require_codex_skill_surface "$CODEX_DUAL_SKILLS_TXT"
require_installed_codex_cache "$SCRIPT_BOTH_CODEX_HOME" "dual install.sh Codex path"
echo "SMOKE: install.sh dual-engine install passed"

# Phase 6: keep the Codex-only helper covered because scripts/install.sh
# delegates to it for Codex and older Codex builds still use its fallback path.
SCRIPT_CODEX_HOME="$TMP_DIR/codex-home-via-script"
SCRIPT_USER_HOME="$TMP_DIR/codex-user-home-via-script"
mkdir -p "$SCRIPT_CODEX_HOME" "$SCRIPT_USER_HOME"
run env CODEX_HOME="$SCRIPT_CODEX_HOME" HOME="$SCRIPT_USER_HOME" \
  bash "$ROOT_DIR/scripts/install-codex.sh" "$MARKETPLACE_ROOT"
CODEX_SCRIPT_SKILLS_TXT="$TMP_DIR/codex-script-skills.txt"
env \
  CODEX_HOME="$SCRIPT_CODEX_HOME" \
  HOME="$SCRIPT_USER_HOME" \
  codex debug prompt-input "probe dvandva skills after codex helper install" \
  | jq -r '.. | strings? // empty' > "$CODEX_SCRIPT_SKILLS_TXT"
require_codex_skill_surface "$CODEX_SCRIPT_SKILLS_TXT"
require_installed_codex_cache "$SCRIPT_CODEX_HOME" "install-codex.sh helper path"
echo "SMOKE: install-codex.sh end-to-end install passed"

STALE_CACHE_DIR="$TMP_DIR/stale-codex-cache/$EXPECTED_DVANDVA_VERSION"
mkdir -p "$TMP_DIR/stale-codex-cache"
cp -R "$SCRIPT_CODEX_HOME/plugins/cache/dvandva/dvandva/$EXPECTED_DVANDVA_VERSION" "$STALE_CACHE_DIR"
rm -f "$STALE_CACHE_DIR/agents/deslopper.md"
touch "$STALE_CACHE_DIR/agents/not-a-dvandva-agent.md"
if roster_matches_expected "$STALE_CACHE_DIR/agents"; then
  fail "same-version stale cache fixture unexpectedly passed exact roster validation"
fi
echo "SMOKE: same-version stale cache rejected by exact roster validation"

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
