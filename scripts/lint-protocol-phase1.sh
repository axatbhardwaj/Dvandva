#!/usr/bin/env bash
# Lint protocol/source docs for the phase-1 Dvandva v2 design contract.
set -uo pipefail

failures=0

require_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if rg -q "$pattern" "$file"; then
    echo "PASS: $message"
  else
    echo "FAIL: $message" >&2
    failures=$((failures + 1))
  fi
}

reject_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if rg -q "$pattern" "$file"; then
    echo "FAIL: $message" >&2
    failures=$((failures + 1))
  else
    echo "PASS: $message"
  fi
}

require_jq() {
  local filter="$1"
  local file="$2"
  local message="$3"
  if jq -e "$filter" "$file" >/dev/null 2>&1; then
    echo "PASS: $message"
  else
    echo "FAIL: $message" >&2
    failures=$((failures + 1))
  fi
}

require_rg 'dvandva\.baton\.v2' product.md 'product spec defines baton v2'
require_rg 'run_id' product.md 'product spec defines run_id'
require_rg 'original_ask' product.md 'product spec defines original_ask'
require_rg 'research_ref' product.md 'product spec defines research_ref'
require_rg 'research_drafting|research_review|research_revision' product.md 'product spec defines research states'
require_rg 'dvandva-wait\.sh --persist|--persist' product.md 'product spec defines persistent shell wait'
require_rg 'Continuous polling is the hard rule' product.md 'product spec makes continuous polling mandatory'
require_rg 'generated user-facing artifacts.*HTML|HTML.*generated user-facing artifacts' product.md 'product spec scopes HTML migration to generated user-facing artifacts'
reject_rg 'No multi-baton-per-repo support|One active baton per worktree' product.md 'product spec no longer excludes multi-run support'

for file in docs/protocol/local-baton-channel.md plugins/dvandva/references/local-baton-channel.md; do
  require_rg 'runs/<run_id>|runs/\$|DVANDVA_RUN_ID|run_id' "$file" "$file documents run-scoped baton paths"
  require_rg 'generated user-facing artifacts|HTML' "$file" "$file documents HTML generated artifact policy"
  require_rg 'Continuous polling is the hard rule' "$file" "$file makes continuous polling mandatory"
  require_rg 'Phase convention: implementation-chunk' "$file" "$file documents subagent track phase convention"
done

require_rg '"schema": "dvandva\.baton\.v2"' plugins/dvandva/references/baton-schema-v2.json 'v2 schema seed declares dvandva.baton.v2'
require_rg '"run_id"' plugins/dvandva/references/baton-schema-v2.json 'v2 schema seed includes run_id'
require_rg '"original_ask"' plugins/dvandva/references/baton-schema-v2.json 'v2 schema seed includes original_ask'
require_rg '"research_ref"' plugins/dvandva/references/baton-schema-v2.json 'v2 schema seed includes research_ref'
require_jq '.turn_cap == 60' plugins/dvandva/references/baton-schema.json 'v1 plugin schema seed uses turn_cap 60'
require_jq '.turn_cap == 60' templates/channel/baton.json 'channel template seed uses turn_cap 60'
require_jq '.turn_cap == 60' plugins/dvandva/references/baton-schema-v2.json 'v2 schema seed uses turn_cap 60'
reject_rg 'extended v1 seed|legacy v1 default 20|Legacy v1 defaults to 20' product.md 'product spec no longer mentions stale v1 turn_cap seed/default wording'
require_rg 'dvandva\.baton\.v2' plugins/dvandva/references/state-transition-table.md 'transition table documents baton v2'
require_rg 'research_drafting|research_review|research_revision' plugins/dvandva/references/state-transition-table.md 'transition table documents research states'

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
exit 0
