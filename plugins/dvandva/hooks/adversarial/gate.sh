#!/usr/bin/env bash
# Claude Code Stop hook adapter for adversarial_loop_gate_predicate.
#
# This is intentionally a bounded nudge, not a tamper-proof supervisor: a
# malicious chair can edit or remove goal.json and is outside the threat model.
# Claude Code re-fires Stop hooks with stop_hook_active=true after a block. Do
# not auto-allow that signal: re-evaluate and block again while the predicate
# fails. Claude Code force-ends after 8 consecutive blocks; that is the honest
# ceiling of this hook and is documented rather than bypassed.

set -o pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
# shellcheck source=predicate.sh
source "$SCRIPT_DIR/predicate.sh"

json_escape() {
  local value=${1-}
  local char code index
  local escaped=''
  local LC_ALL=C

  for ((index = 0; index < ${#value}; index++)); do
    char=${value:index:1}
    case "$char" in
      \\) escaped+='\\\\' ;;
      \") escaped+='\\"' ;;
      *)
        printf -v code '%d' "'$char"
        if [[ $code -lt 32 ]]; then
          printf -v char '\\u%04x' "$code"
        fi
        escaped+="$char"
        ;;
    esac
  done

  printf '%s' "$escaped"
}

emit_block() {
  printf '{"decision":"block","reason":"%s"}\n' "$(json_escape "$1")"
}

hook_json=$(cat)
session_id=''
hook_cwd=''

# jq is also the predicate's JSON dependency. When it is absent, the
# predicate sees a present goal and blocks fail-closed without trusting stdin.
if command -v jq >/dev/null 2>&1; then
  session_id=$(jq -r 'if (.session_id? | type) == "string" then .session_id else "" end' <<<"$hook_json" 2>/dev/null || true)
  hook_cwd=$(jq -r 'if (.cwd? | type) == "string" then .cwd else "" end' <<<"$hook_json" 2>/dev/null || true)
fi

candidate_cwd=$PWD
if [[ -n "$hook_cwd" && -d "$hook_cwd" ]]; then
  candidate_cwd=$hook_cwd
fi

repo_root=$(git -C "$candidate_cwd" rev-parse --show-toplevel 2>/dev/null || true)
if [[ -z "$repo_root" ]]; then
  repo_root=$(cd -- "$candidate_cwd" && pwd -P)
fi

predicate_result=$(adversarial_loop_gate_predicate "$repo_root" "$session_id")
if [[ "$predicate_result" != *$'\t'* ]]; then
  emit_block 'gate predicate returned an invalid response'
  exit 0
fi

decision=${predicate_result%%$'\t'*}
reason=${predicate_result#*$'\t'}
if [[ "$decision" == block ]]; then
  emit_block "$reason"
fi

# Claude Code Stop hooks use exit 0 for both allow (no stdout) and block
# (exactly one decision JSON object); never combine a block JSON with exit 2.
exit 0
