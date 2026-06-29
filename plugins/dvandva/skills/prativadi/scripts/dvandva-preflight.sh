#!/usr/bin/env bash
# Unified Dvandva turn preflight.
#
# Resolve the active baton first, then run the hook stage only when a baton is
# already selected.
set -u

ROLE=""
MODE="${DVANDVA_HOOK_PREFLIGHT:-auto}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" 2>/dev/null && pwd)"
WORK_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd -P)"

usage() {
  cat <<'EOF'
Usage: dvandva-preflight.sh --role <vadi|prativadi> [--mode auto|strict|off]
EOF
}

selected_by() {
  if [[ -n "${DVANDVA_BATON_FILE:-}" ]]; then
    printf '%s\n' "DVANDVA_BATON_FILE"
  elif [[ -n "${DVANDVA_RUN_DIR:-}" ]]; then
    printf '%s\n' "DVANDVA_RUN_DIR"
  elif [[ -n "${DVANDVA_RUN_ID:-}" ]]; then
    printf '%s\n' "DVANDVA_RUN_ID"
  else
    printf '%s\n' "discovery"
  fi
}

canonical_path() {
  local raw="$1"
  case "$raw" in
    /*) realpath -m "$raw" ;;
    *)  realpath -m "$WORK_ROOT/$raw" ;;
  esac
}

run_id_for_path() {
  local path="$1"
  if [[ "$path" == */.dvandva/baton.json ]]; then
    printf '%s\n' "legacy"
    return
  fi
  basename "$(dirname "$path")"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --role)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      ROLE="$2"
      shift 2
      ;;
    --mode)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      MODE="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

case "$ROLE" in
  vadi|prativadi) ;;
  *)
    usage >&2
    exit 2
    ;;
esac

case "$MODE" in
  auto|strict|off) ;;
  *)
    usage >&2
    exit 2
    ;;
esac

ENV_ROLE="${DVANDVA_ROLE:-}"
if [[ -n "$ENV_ROLE" && "$ENV_ROLE" != "$ROLE" ]]; then
  echo "DVANDVA_PREFLIGHT role=$ROLE result=error reason=role_mismatch env_role=$ENV_ROLE"
  exit 1
fi
export DVANDVA_ROLE="$ROLE"

RESOLVE="$SCRIPT_DIR/dvandva-resolve.sh"
HOOK="$SCRIPT_DIR/dvandva-hook-preflight.sh"
if [[ ! -f "$RESOLVE" ]]; then
  echo "DVANDVA_PREFLIGHT role=$ROLE result=error reason=missing_resolver path=$RESOLVE"
  exit 1
fi
if [[ ! -f "$HOOK" ]]; then
  echo "DVANDVA_PREFLIGHT role=$ROLE result=error reason=missing_hook_preflight path=$HOOK"
  exit 1
fi

RESOLVE_ERR="$(mktemp)"
if resolve_out="$(bash "$RESOLVE" --role "$ROLE" --cwd "$WORK_ROOT" 2>"$RESOLVE_ERR")"; then
  resolve_rc=0
else
  resolve_rc=$?
fi
resolve_err="$(cat "$RESOLVE_ERR")"
rm -f "$RESOLVE_ERR"

case "$resolve_out" in
  "ASK "*)
    echo "DVANDVA_PREFLIGHT role=$ROLE result=ask selected_by=$(selected_by) choices=${resolve_out#ASK }"
    [[ "$resolve_rc" -eq 12 ]] || [[ -z "$resolve_err" ]]
    exit 12
    ;;
  "CREATE "*)
    scaffold_path="$(canonical_path "${resolve_out#CREATE }")"
    echo "DVANDVA_PREFLIGHT role=$ROLE result=create scaffold=$scaffold_path run_id=$(run_id_for_path "$scaffold_path") selected_by=$(selected_by)"
    exit 0
    ;;
  "RESOLVED "*)
    if [[ "$resolve_rc" -ne 0 ]]; then
      echo "DVANDVA_PREFLIGHT role=$ROLE result=error reason=resolve_failed exit=$resolve_rc"
      [[ -n "$resolve_err" ]] && printf '%s\n' "$resolve_err"
      exit "$resolve_rc"
    fi
    baton_path="$(canonical_path "${resolve_out#RESOLVED }")"
    run_id="$(run_id_for_path "$baton_path")"
    chosen_by="$(selected_by)"
    export DVANDVA_BATON_FILE="$baton_path"
    export DVANDVA_RUN_ID="$run_id"
    export DVANDVA_SELECTED_BY="$chosen_by"
    echo "DVANDVA_PREFLIGHT role=$ROLE result=resolved baton=$baton_path run_id=$run_id selected_by=$chosen_by"
    exec bash "$HOOK" --role "$ROLE" --repo "$WORK_ROOT" --mode "$MODE"
    ;;
  *)
    echo "DVANDVA_PREFLIGHT role=$ROLE result=error reason=unexpected_resolver_output"
    [[ -n "$resolve_out" ]] && printf '%s\n' "$resolve_out"
    [[ -n "$resolve_err" ]] && printf '%s\n' "$resolve_err"
    exit 1
    ;;
esac
