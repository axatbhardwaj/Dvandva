#!/usr/bin/env bash
# Dvandva Git Commit Gate — engine-agnostic repo-local commit policy.
#
# Called from .githooks/pre-commit.  Resolves the active Dvandva baton and
# allows the commit only if DVANDVA_ROLE is the current assignee or is
# listed in active_roles.  Repos without a .dvandva directory are
# completely unaffected (exit 0 without reading any files).
#
# Exit codes:
#   0  no active baton found (non-Dvandva repo or post-run commit) → allow
#   0  DVANDVA_ROLE is the current assignee or is in active_roles   → allow
#   1  DVANDVA_ROLE unset while an active baton exists              → block
#   1  DVANDVA_ROLE is not vadi or prativadi                        → block
#   1  multiple active batons found (ambiguous active runs)         → block
#   1  DVANDVA_ROLE is neither assignee nor in active_roles         → block
set -u

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Terminal statuses are treated as "inactive" by the gate.
# Commits during these states are not baton-gated.
is_terminal() {
  case "$1" in
    done|human_question|human_decision) return 0 ;;
    *) return 1 ;;
  esac
}

# ---------------------------------------------------------------------------
# Locate git repository root
# ---------------------------------------------------------------------------
if ! REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  # Not inside a git repository — nothing to gate.
  exit 0
fi

# ---------------------------------------------------------------------------
# Collect baton candidates
#   Legacy path : .dvandva/baton.json
#   Run-scoped  : .dvandva/runs/<run_id>/baton.json
# ---------------------------------------------------------------------------
BATON_PATHS=()

if [[ -f "$REPO_ROOT/.dvandva/baton.json" ]]; then
  BATON_PATHS+=("$REPO_ROOT/.dvandva/baton.json")
fi

if [[ -d "$REPO_ROOT/.dvandva/runs" ]]; then
  while IFS= read -r -d '' p; do
    BATON_PATHS+=("$p")
  done < <(find "$REPO_ROOT/.dvandva/runs" -maxdepth 2 -name "baton.json" -print0 2>/dev/null)
fi

# No .dvandva structure at all → not a Dvandva repo, allow.
if [[ ${#BATON_PATHS[@]} -eq 0 ]]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# Filter to active (non-terminal) batons
# ---------------------------------------------------------------------------
ACTIVE_BATONS=()
for bp in "${BATON_PATHS[@]}"; do
  [[ -f "$bp" ]] || continue
  # Skip files that are not valid JSON
  jq empty "$bp" 2>/dev/null || continue
  status="$(jq -r '.status // ""' "$bp")"
  if ! is_terminal "$status"; then
    ACTIVE_BATONS+=("$bp")
  fi
done

# All batons are terminal (run complete) → allow.
if [[ ${#ACTIVE_BATONS[@]} -eq 0 ]]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# Ambiguity: more than one active run → fail closed
# ---------------------------------------------------------------------------
if [[ ${#ACTIVE_BATONS[@]} -gt 1 ]]; then
  echo "DVANDVA_GATE error: ${#ACTIVE_BATONS[@]} active batons found — ambiguous active runs." >&2
  for b in "${ACTIVE_BATONS[@]}"; do
    st="$(jq -r '.status // "unknown"' "$b" 2>/dev/null || echo "invalid")"
    cp="$(jq -r '.checkpoint // "?"' "$b" 2>/dev/null || echo "?")"
    echo "  $b  status=$st  checkpoint=$cp" >&2
  done
  echo "Resolve to a single active run before committing." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Exactly one active baton — read its fields
# ---------------------------------------------------------------------------
ACTIVE="${ACTIVE_BATONS[0]}"
BATON_STATUS="$(jq -r '.status // ""' "$ACTIVE")"
BATON_ASSIGNEE="$(jq -r '.assignee // ""' "$ACTIVE")"
BATON_CHECKPOINT="$(jq -r '.checkpoint // 0' "$ACTIVE")"

# ---------------------------------------------------------------------------
# DVANDVA_ROLE must be set when an active baton is present
# ---------------------------------------------------------------------------
ROLE="${DVANDVA_ROLE:-}"
if [[ -z "$ROLE" ]]; then
  echo "DVANDVA_GATE error: DVANDVA_ROLE is unset but an active baton exists." >&2
  echo "  baton: $ACTIVE" >&2
  echo "  status=$BATON_STATUS  assignee=$BATON_ASSIGNEE  checkpoint=$BATON_CHECKPOINT" >&2
  echo "Export DVANDVA_ROLE=vadi or DVANDVA_ROLE=prativadi before committing." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# DVANDVA_ROLE must be a known engine role
# ---------------------------------------------------------------------------
case "$ROLE" in
  vadi|prativadi) ;;
  *)
    echo "DVANDVA_GATE error: DVANDVA_ROLE='$ROLE' is not a valid role (must be vadi or prativadi)." >&2
    exit 1
    ;;
esac

# ---------------------------------------------------------------------------
# Allow if this role is the assignee OR appears in active_roles
# ---------------------------------------------------------------------------
role_allowed="$(jq --arg role "$ROLE" '
  ($role == .assignee) or
  ((.active_roles | type) == "array" and
   ((.active_roles | index($role)) != null))
' "$ACTIVE")"

if [[ "$role_allowed" == "true" ]]; then
  exit 0
fi

# ---------------------------------------------------------------------------
# BLOCK — emit a clear diagnostic
# ---------------------------------------------------------------------------
active_roles_str="$(jq -r '(.active_roles // []) | join(", ")' "$ACTIVE" 2>/dev/null || echo "")"
echo "DVANDVA_GATE blocked: DVANDVA_ROLE=$ROLE is not allowed to commit." >&2
echo "  baton: $ACTIVE" >&2
echo "  status=$BATON_STATUS  assignee=$BATON_ASSIGNEE  checkpoint=$BATON_CHECKPOINT" >&2
[[ -n "$active_roles_str" ]] && echo "  active_roles=$active_roles_str" >&2
echo "The baton is not currently assigned to your role. Wait for your turn." >&2
exit 1
