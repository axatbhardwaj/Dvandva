#!/usr/bin/env bash
# Lint Run4 standalone-agent retirement contracts.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ROOT="${1:-$ROOT_DIR}"
FAILURES=0

EXPECTED_VERSION="0.4.0"
EXPECTED_AGENTS=(
  adversarial-analyst.md
  architect.md
  baton-auditor.md
  cross-reviewer.md
  debugger.md
  deep-reviewer.md
  deslopper.md
  doc-verifier.md
  implementer.md
  integration-checker.md
  pattern-mapper.md
  researcher.md
  sandbox-verifier.md
  security-auditor.md
  test-creator.md
)

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  FAILURES=$((FAILURES + 1))
}

pass() {
  printf 'PASS: %s\n' "$*"
}

file_path() {
  printf '%s/%s\n' "$TARGET_ROOT" "$1"
}

slurp() {
  local rel="$1"
  tr '\n' ' ' < "$(file_path "$rel")"
}

require_file() {
  local rel="$1"
  if [[ -f "$(file_path "$rel")" ]]; then
    pass "$rel exists"
  else
    fail "$rel is missing"
    return 1
  fi
}

require_match() {
  local rel="$1" regex="$2" message="$3"
  local path text
  path="$(file_path "$rel")"
  if [[ ! -f "$path" ]]; then
    fail "$rel is missing"
    return
  fi
  text="$(slurp "$rel")"
  if printf '%s\n' "$text" | grep -Eiq -- "$regex"; then
    pass "$message"
  else
    fail "$message"
  fi
}

required_files=(
  README.md
  product.md
  docs/protocol/local-baton-channel.md
  plugins/dvandva/references/state-transition-table.md
  plugins/dvandva/references/baton-schema-v2.json
  scripts/retire-standalone-agents.sh
  scripts/test-retire-standalone-agents.sh
  scripts/smoke-plugin-install.sh
  scripts/test-install.sh
  scripts/test-install-codex.sh
  .claude-plugin/marketplace.json
  plugins/dvandva/.claude-plugin/plugin.json
  plugins/dvandva/.codex-plugin/plugin.json
)

for rel in "${required_files[@]}"; do
  require_file "$rel"
done

require_match \
  README.md \
  'Dvandva-only.*retire|retire.*Dvandva-only' \
  'README.md must document Dvandva-only retirement'

require_match \
  README.md \
  'Dvandva-covered workflows' \
  'README.md must limit retirement to Dvandva-covered workflows'

if grep -Fq 'v0.2.0 ships' "$(file_path README.md)" \
  || grep -Fq 'Run 3 (in progress)' "$(file_path README.md)"; then
  fail 'README.md contains stale Run 3 or v0.2.0 wording'
else
  pass 'README.md contains no stale Run 3 or v0.2.0 wording'
fi

combined_docs="$(slurp README.md) $(slurp product.md)"
if printf '%s\n' "$combined_docs" | grep -Eiq 'Codex agent-axis.*no-op|no-op.*Codex agent-axis'; then
  pass 'Run4 docs document Codex agent-axis no-op'
else
  fail 'Run4 docs must document Codex agent-axis no-op'
fi

combined_parity="$(slurp README.md) $(slurp product.md) $(slurp scripts/retire-standalone-agents.sh)"
if printf '%s\n' "$combined_parity" | grep -Eiq '(functional parity|equivalent-or-better).*Runs 1-4|Runs 1-4.*(functional parity|equivalent-or-better)'; then
  pass 'Run4 docs/scripts cite functional parity via Runs 1-4 usage'
else
  fail 'Run4 docs/scripts must cite functional parity via Runs 1-4 usage'
fi

require_match \
  scripts/retire-standalone-agents.sh \
  'backup.*manifest.*restore|manifest.*restore|restore.*manifest' \
  'Run4 retirement surface must document backup manifest and restore'

require_match \
  scripts/retire-standalone-agents.sh \
  'skills.*never|never.*skills|no skill touches|skills out of scope' \
  'Run4 retirement helper must document no skill touches'

for agent in adversarial-analyst architect developer quality-reviewer sandbox-executor; do
  require_match \
    README.md \
    "$agent" \
    "README.md must name Claude symlink allowlist member $agent"
done

marketplace_version=""
claude_version=""
codex_version=""
if [[ -f "$(file_path .claude-plugin/marketplace.json)" ]]; then
  marketplace_version="$(jq -r '.plugins[]? | select(.name == "dvandva") | .version' "$(file_path .claude-plugin/marketplace.json)" 2>/dev/null | head -1)"
fi
if [[ -f "$(file_path plugins/dvandva/.claude-plugin/plugin.json)" ]]; then
  claude_version="$(jq -r '.version // ""' "$(file_path plugins/dvandva/.claude-plugin/plugin.json)" 2>/dev/null)"
fi
if [[ -f "$(file_path plugins/dvandva/.codex-plugin/plugin.json)" ]]; then
  codex_version="$(jq -r '.version // ""' "$(file_path plugins/dvandva/.codex-plugin/plugin.json)" 2>/dev/null)"
fi

if [[ "$marketplace_version" == "$EXPECTED_VERSION" \
  && "$claude_version" == "$EXPECTED_VERSION" \
  && "$codex_version" == "$EXPECTED_VERSION" ]]; then
  pass "Dvandva manifest versions all equal $EXPECTED_VERSION"
else
  fail "Dvandva manifest versions must all equal $EXPECTED_VERSION"
fi

agents_dir="$(file_path plugins/dvandva/agents)"
if [[ -d "$agents_dir" ]]; then
  mapfile -t actual_agents < <(find "$agents_dir" -maxdepth 1 -type f -name '*.md' -printf '%f\n' | sort)
  expected_joined="$(printf '%s\n' "${EXPECTED_AGENTS[@]}" | sort)"
  actual_joined="$(printf '%s\n' "${actual_agents[@]}")"
  if [[ "$actual_joined" == "$expected_joined" ]]; then
    pass 'plugins/dvandva/agents contains exactly the 15 canonical agents'
  else
    fail 'plugins/dvandva/agents must contain exactly the 15 canonical agents'
  fi

  bad_frontmatter=0
  for agent_file in "${EXPECTED_AGENTS[@]}"; do
    stem="${agent_file%.md}"
    if ! grep -Eq "^name:[[:space:]]*dvandva-$stem[[:space:]]*$" "$agents_dir/$agent_file" 2>/dev/null; then
      bad_frontmatter=1
    fi
  done
  if [[ "$bad_frontmatter" -eq 0 ]]; then
    pass 'agent frontmatter names use dvandva-*'
  else
    fail 'agent frontmatter names must use dvandva-*'
  fi
else
  fail 'plugins/dvandva/agents is missing'
fi

if [[ "$FAILURES" -gt 0 ]]; then
  exit 1
fi

exit 0
