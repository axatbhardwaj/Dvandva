#!/usr/bin/env bash
# Selector-first active-run resolver for Dvandva.
#
# This is the single source of run selection. It runs before any baton
# read / write / wait / scaffold so every actor agrees on which run is
# active. It emits exactly one line to stdout and a matching exit code:
#
#   RESOLVED <path>   exit 0   an existing baton is selected — either an
#                              explicit selector, or exactly one resumable
#                              run was discovered.
#   CREATE <path>     exit 0   no resumable run exists (only `done` archives
#                              or none) — a deterministic new named path is
#                              proposed for the caller to scaffold.
#   ASK <json-array>  exit 12  more than one resumable run AND no explicit
#                              selector — the candidate list is printed and
#                              the caller MUST stop and choose one.
#
# Selector precedence (explicit wins, highest first):
#   DVANDVA_BATON_FILE   -> RESOLVED that exact path.
#   DVANDVA_RUN_DIR      -> RESOLVED <dir>/baton.json (trailing slash trimmed).
#   DVANDVA_RUN_ID       -> RESOLVED .dvandva/runs/<id>/baton.json; the id must
#                           be ONE safe path segment (letters, numbers, dot,
#                           underscore, dash; no slash, backslash, or '..').
#                           Anything else (including empty / whitespace) is a
#                           usage error (exit 2) raised BEFORE any filesystem
#                           operation.
#
# Discovery (no explicit selector): scan .dvandva/runs/*/baton.json plus the
# legacy .dvandva/baton.json. Taxonomy — only status==done is run-terminal
# (eligible for auto-create-instead); human_decision and human_question are
# RESUMABLE (prefer resuming them); every other non-terminal status is also
# resumable. Zero resumable -> CREATE; exactly one -> RESOLVED; more than one
# -> ASK. When a deterministic order is needed (ASK list), candidates are
# ordered by updated_at descending, then run_id ascending.
#
# --role only affects the human-readable messaging on stderr; it never affects
# selection, so vadi and prativadi produce identical stdout for identical
# inputs. The runtime is shipped byte-identical inside each role skill:
#   plugins/dvandva/skills/vadi/scripts/dvandva-resolve.sh
#   plugins/dvandva/skills/prativadi/scripts/dvandva-resolve.sh
# There is NO repo-root runtime copy. scripts/test-dvandva-resolve.sh fails if
# either copy is missing or the two drift.
#
# CREATE id derivation: with no input to derive a name from, the base slug is
# `run`; if .dvandva/runs/run already exists the smallest free `run-N` (N>=2)
# is used. This is a pure function of the on-disk run directories, so it is
# deterministic and identical for both roles, and never collides with an
# existing (e.g. done) run directory. An explicit DVANDVA_RUN_ID/RUN_DIR/
# BATON_FILE always short-circuits to RESOLVED, so CREATE only ever runs for
# pure discovery.
#
# Exit codes:
#   0   RESOLVED or CREATE
#   12  ASK (caller must stop)
#   2   usage error (bad/unsafe selector or arguments)
set -u

# --- Delegating shim -------------------------------------------------------
# Prefer a compiled dvandva binary (DVANDVA_BIN > co-located binary > PATH).
# When found, exec it with the subcommand derived from this shim's own
# basename, forwarding all args unchanged (DVANDVA_ROLE / DVANDVA_* selectors
# pass through the environment automatically; the binary derives role from
# --role > DVANDVA_ROLE > argv0). When no binary is found, fall through to
# the preserved shell implementation below (unchanged behavior).
__dvandva_shim_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
__dvandva_shim_name="$(basename "${BASH_SOURCE[0]}")"
__dvandva_shim_subcmd="${__dvandva_shim_name#dvandva-}"
__dvandva_shim_subcmd="${__dvandva_shim_subcmd%.sh}"
__dvandva_shim_bin=""
if [[ -n "${DVANDVA_BIN:-}" && -x "${DVANDVA_BIN:-}" ]]; then
  __dvandva_shim_bin="$DVANDVA_BIN"
elif [[ -x "$__dvandva_shim_dir/dvandva" ]]; then
  __dvandva_shim_bin="$__dvandva_shim_dir/dvandva"
elif command -v dvandva >/dev/null 2>&1; then
  __dvandva_shim_bin="$(command -v dvandva)"
fi
if [[ -n "$__dvandva_shim_bin" ]]; then
  exec "$__dvandva_shim_bin" "$__dvandva_shim_subcmd" "$@"
fi
unset __dvandva_shim_dir __dvandva_shim_name __dvandva_shim_subcmd __dvandva_shim_bin
# --- End delegating shim; preserved shell fallback continues below --------

ROLE=""
CWD=""

is_safe_run_id() {
  local value="$1"
  [[ "$value" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] && [[ "$value" != *".."* ]]
}

usage() {
  cat >&2 <<'USAGE'
Usage: dvandva-resolve.sh --role <vadi|prativadi> [--cwd <dir>]

Resolves the active Dvandva run selector-first, then by discovery. Prints one
of:
  RESOLVED <path>   (exit 0)  an existing baton is selected
  CREATE <path>     (exit 0)  no resumable run -> new named path to scaffold
  ASK <json-array>  (exit 12) >1 resumable run + no selector -> caller stops

Selector precedence (explicit wins): DVANDVA_BATON_FILE, then
DVANDVA_RUN_DIR (/baton.json), then DVANDVA_RUN_ID mapped to
.dvandva/runs/<id>/baton.json. DVANDVA_RUN_ID must be one safe path segment:
letters, numbers, dot, underscore, dash; no slash, backslash, or '..'.
Only status==done is run-terminal; human_decision and human_question are
resumable.
USAGE
}

# Deterministic CREATE slug: first free .dvandva/runs/<slug> directory.
derive_create_slug() {
  local base="run" n
  if [[ ! -e ".dvandva/runs/$base" ]]; then
    printf '%s' "$base"
    return
  fi
  n=2
  while [[ -e ".dvandva/runs/$base-$n" ]]; do
    n=$((n + 1))
  done
  printf '%s' "$base-$n"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --role)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      ROLE="$2"
      shift 2
      ;;
    --cwd)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      CWD="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

case "$ROLE" in
  vadi|prativadi) ;;
  *)
    usage
    exit 2
    ;;
esac

# Resolve the explicit selector (if any) with strict precedence. Validate the
# run id BEFORE changing directory or touching the filesystem so an unsafe
# selector fails fast.
SELECTED_PATH=""
HAVE_SELECTOR=0
if [[ -n "${DVANDVA_BATON_FILE:-}" ]]; then
  SELECTED_PATH="$DVANDVA_BATON_FILE"
  HAVE_SELECTOR=1
elif [[ -n "${DVANDVA_RUN_DIR:-}" ]]; then
  SELECTED_PATH="${DVANDVA_RUN_DIR%/}/baton.json"
  HAVE_SELECTOR=1
elif [[ -n "${DVANDVA_RUN_ID+x}" ]]; then
  # Set, possibly empty. Empty / whitespace / traversal are all unsafe.
  if ! is_safe_run_id "${DVANDVA_RUN_ID:-}"; then
    echo "ERROR: DVANDVA_RUN_ID must be one safe path segment (letters, numbers, dot, underscore, dash; no slash, backslash, or '..')" >&2
    exit 2
  fi
  SELECTED_PATH=".dvandva/runs/$DVANDVA_RUN_ID/baton.json"
  HAVE_SELECTOR=1
fi

if [[ -n "$CWD" ]]; then
  cd "$CWD" || { echo "ERROR: --cwd is not a directory: $CWD" >&2; exit 2; }
fi

if [[ "$HAVE_SELECTOR" -eq 1 ]]; then
  printf 'RESOLVED %s\n' "$SELECTED_PATH"
  exit 0
fi

# Discovery: collect candidate baton files.
shopt -s nullglob
candidate_files=( .dvandva/runs/*/baton.json )
shopt -u nullglob
if [[ -f .dvandva/baton.json ]]; then
  candidate_files+=( .dvandva/baton.json )
fi

# Build NDJSON of resumable candidates (everything whose status != done).
resumable_ndjson=""
for cf in "${candidate_files[@]}"; do
  if [[ "$cf" == ".dvandva/baton.json" ]]; then
    cf_fallback="legacy"
  else
    cf_fallback="$(basename "$(dirname "$cf")")"
  fi
  cf_obj="$(jq -c --arg path "$cf" --arg fid "$cf_fallback" '
    {
      run_id: ((.run_id // "") | if . == "" then $fid else . end),
      path: $path,
      status: (.status // ""),
      assignee: (.assignee // ""),
      updated_at: (.updated_at // "")
    }' "$cf" 2>/dev/null)" || {
    # FAIL CLOSED: baton is not valid JSON — cannot safely determine run state.
    # Silently skipping it could hide an active run and allow a wrong CREATE or
    # RESOLVED of a sibling.  Emit ASK (STOP) naming the corrupt path so the
    # operator can repair it or choose an explicit selector to bypass discovery.
    printf 'ASK []\n'
    {
      echo "DVANDVA_RESOLVE corrupt_baton path=$cf role=$ROLE"
      echo "ERROR: baton at '$cf' is not valid JSON; cannot safely discover runs."
      echo "Hint: inspect/repair the file or bypass discovery with an explicit selector"
      echo "      (DVANDVA_RUN_ID, DVANDVA_RUN_DIR, or DVANDVA_BATON_FILE)."
    } >&2
    exit 12
  }
  [[ -n "$cf_obj" ]] || continue
  cf_status="$(printf '%s' "$cf_obj" | jq -r '.status')"
  if [[ "$cf_status" != "done" ]]; then
    resumable_ndjson+="$cf_obj"$'\n'
  fi
done

# Sort resumable candidates: updated_at desc, then run_id asc.
resumable_json="$(printf '%s' "$resumable_ndjson" | jq -s '
  if length == 0 then []
  else group_by(.updated_at) | reverse | map(sort_by(.run_id)) | add
  end')"
count="$(printf '%s' "$resumable_json" | jq 'length')"

if [[ "$count" -eq 0 ]]; then
  printf 'CREATE .dvandva/runs/%s/baton.json\n' "$(derive_create_slug)"
  exit 0
elif [[ "$count" -eq 1 ]]; then
  printf 'RESOLVED %s\n' "$(printf '%s' "$resumable_json" | jq -r '.[0].path')"
  exit 0
fi

# More than one resumable run and no explicit selector: ASK and stop.
printf 'ASK %s\n' "$(printf '%s' "$resumable_json" | jq -c '.')"
{
  echo "DVANDVA_RESOLVE ask role=$ROLE reason=multiple_resumable_runs count=$count"
  printf '%s' "$resumable_json" | jq -r '.[] | "  - run_id=\(.run_id) status=\(.status) assignee=\(.assignee) updated_at=\(.updated_at) path=\(.path)"'
  echo "Choose one via DVANDVA_RUN_ID, DVANDVA_RUN_DIR, or DVANDVA_BATON_FILE, then re-run."
} >&2
exit 12
