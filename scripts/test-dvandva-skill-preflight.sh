#!/usr/bin/env bash
# Static checks for Dvandva role preflight wording.  These protect the process
# contract that cannot be fully enforced by dvandva-write.sh.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VADI="$ROOT_DIR/plugins/dvandva/skills/vadi/SKILL.md"
PRATIVADI="$ROOT_DIR/plugins/dvandva/skills/prativadi/SKILL.md"
STATE_REF="$ROOT_DIR/plugins/dvandva/references/state-transition-table.md"

failures=0

pass() {
  printf 'PASS: %s\n' "$*"
}

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  failures=$((failures + 1))
}

require_match() {
  local file="$1" regex="$2" message="$3"
  local text
  text="$(tr '\n' ' ' < "$file")"
  if printf '%s\n' "$text" | grep -Eiq -- "$regex"; then
    pass "$message"
  else
    fail "$message"
  fi
}

reject_match() {
  local file="$1" regex="$2" message="$3"
  local text
  text="$(tr '\n' ' ' < "$file")"
  if printf '%s\n' "$text" | grep -Eiq -- "$regex"; then
    fail "$message"
  else
    pass "$message"
  fi
}

for file in "$VADI" "$PRATIVADI"; do
  role="$(basename "$(dirname "$file")")"
  require_match "$file" \
    'Baton creation/resume discovery is mandatory before active work' \
    "$role skill makes baton discovery a hard preflight gate"
  require_match "$file" \
    'Resolve the active baton path before reading or writing' \
    "$role skill resolves baton before reads/writes"
  require_match "$file" \
    'before active non-wait work' \
    "$role skill runs hook preflight only after baton resolution"
  require_match "$file" \
    "export[[:space:]]+DVANDVA_ROLE=$role" \
    "$role skill exports DVANDVA_ROLE=$role"
  require_match "$file" \
    'asserts?[[:space:]]+[`]?DVANDVA_ROLE='"$role"'[`]?' \
    "$role skill asserts DVANDVA_ROLE=$role"
  require_match "$file" \
    'detects?[[:space:]]+Dvandva hook adoption|hook adoption status' \
    "$role skill detects hook adoption instead of forcing it"
  require_match "$file" \
    'foreign[[:space:]-]+hooksPath.*must not be modified|must not modify.*foreign[[:space:]-]+hooksPath' \
    "$role skill preserves foreign hooksPath values"
  require_match "$file" \
    'Checkpoint commits require Dvandva hook adoption' \
    "$role skill gates checkpoint commits on adopted hooks"
  require_match "$file" \
    'Final commits require Dvandva hook adoption' \
    "$role skill gates final commits on adopted hooks"
  reject_match "$file" \
    'bash[[:space:]]+scripts/install-dvandva-hooks\.sh' \
    "$role skill does not require target-repo scripts/install-dvandva-hooks.sh"
done

require_match "$VADI" \
  'human_question.*resumable for discovery|resumable for discovery.*human_question' \
  "vadi skill treats human_question as resumable during discovery"
reject_match "$VADI" \
  'only terminal `done`/`human_decision`/`human_question` archives remain, auto-create' \
  "vadi skill does not classify human_question as only a terminal archive"

reject_match "$STATE_REF" \
  'role preflight exports and asserts `DVANDVA_ROLE=<role>`,[[:space:]]*`scripts/install-dvandva-hooks\.sh` sets and verifies `core\.hooksPath=\.githooks`' \
  "state reference no longer documents unconditional target-repo hook install"
require_match "$STATE_REF" \
  'Checkpoint commits require Dvandva hook adoption' \
  "state reference gates checkpoint commits on adopted hooks"

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
