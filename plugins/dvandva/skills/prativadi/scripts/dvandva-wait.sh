#!/usr/bin/env bash
# Cheap foreground wait for Dvandva baton ownership.
#
# Wakes early on baton-directory inotify events when inotifywait is
# available; otherwise sleeps INTERVAL between checks. By default it keeps
# waiting across heartbeat intervals until the role is assigned, the baton
# reaches a terminal state, or the user interrupts. Use --finite only for
# compatibility tests or harnesses that must cap one helper invocation.
#
# This helper is bundled as a real executable inside each runtime skill:
#   plugins/dvandva/skills/vadi/scripts/dvandva-wait.sh
#   plugins/dvandva/skills/prativadi/scripts/dvandva-wait.sh
# The two copies must stay byte-identical so copy-installs and plugin installs
# keep the helper findable via ${CLAUDE_SKILL_DIR}/scripts/dvandva-wait.sh.
# scripts/test-dvandva-wait.sh fails if either runtime copy is missing or drifts.
#
# Exit codes:
#   0  role is assigned
#   10 baton status is done
#   11 baton status is human_decision
#   12 baton status is human_question
#   20 timed out while another actor owns the baton (--finite heartbeat)
#   21 baton file missing
#   22 baton JSON invalid
#   23 persistent wait exceeded --persist-max
#   29 split-brain detected: selected run waits on peer while sibling waits on me
#   2  usage error
set -u

ROLE=""
BATON_SOURCE="legacy"
SELECTED_BY="legacy"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESOLVER_SCRIPT="$SCRIPT_DIR/dvandva-resolve.sh"
is_safe_run_id() {
  local value="$1"
  [[ "$value" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] && [[ "$value" != *".."* ]]
}

if [[ -n "${DVANDVA_BATON_FILE:-}" ]]; then
  BATON_FILE="$DVANDVA_BATON_FILE"
  BATON_SOURCE="env_file"
elif [[ -n "${DVANDVA_RUN_DIR:-}" ]]; then
  BATON_FILE="${DVANDVA_RUN_DIR%/}/baton.json"
  BATON_SOURCE="run_dir"
elif [[ -n "${DVANDVA_RUN_ID:-}" ]]; then
  BATON_FILE=".dvandva/runs/$DVANDVA_RUN_ID/baton.json"
  BATON_SOURCE="run_id"
else
  BATON_FILE=".dvandva/baton.json"
fi
SELECTED_BY="$BATON_SOURCE"
INTERVAL=60
MAX_WAIT=540
ALLOW_MISSING=0
PERSIST=1
PERSIST_MAX=0

usage() {
  cat >&2 <<'USAGE'
Usage: dvandva-wait.sh --role <vadi|prativadi> [--file .dvandva/baton.json] [--interval seconds] [--max-wait seconds] [--allow-missing] [--persist] [--persist-max seconds] [--finite]

Defaults: --interval 60 --max-wait 540
Default file resolution: --file wins; otherwise DVANDVA_BATON_FILE,
DVANDVA_RUN_DIR/baton.json, DVANDVA_RUN_ID mapped to
.dvandva/runs/<id>/baton.json, then legacy .dvandva/baton.json.
DVANDVA_RUN_ID must be one safe path segment: letters, numbers, dot,
underscore, or dash; no slash or '..'.

Wakes early on baton-directory changes when inotifywait is available;
otherwise sleeps INTERVAL between checks. The default mode is continuous:
--max-wait is a heartbeat interval, not a stop condition, and the helper
keeps polling until this role owns the baton, the baton reaches done /
human_question / human_decision, or the user interrupts.

With --allow-missing, a missing baton file does not exit 21 immediately;
the helper instead sleeps INTERVAL and retries until the file appears
or --finite --max-wait elapses (returns 20 on timeout).

--persist is accepted for older call sites and is now the default behavior.
Use --persist-max to set a total wall-clock cap for continuous waits; 0 means
no cap. Use --finite to restore the old single-heartbeat exit-20 behavior.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --role)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      ROLE="$2"
      shift 2
      ;;
    --file)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      BATON_FILE="$2"
      BATON_SOURCE="file"
      shift 2
      ;;
    --interval)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      INTERVAL="$2"
      shift 2
      ;;
    --max-wait)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      MAX_WAIT="$2"
      shift 2
      ;;
    --allow-missing)
      ALLOW_MISSING=1
      shift 1
      ;;
    --persist)
      PERSIST=1
      shift 1
      ;;
    --persist-max)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      PERSIST_MAX="$2"
      shift 2
      ;;
    --finite)
      PERSIST=0
      shift 1
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

if [[ -z "$ROLE" ]]; then
  usage
  exit 2
fi

if ! [[ "$INTERVAL" =~ ^[0-9]+$ && "$MAX_WAIT" =~ ^[0-9]+$ && "$PERSIST_MAX" =~ ^[0-9]+$ ]]; then
  echo "ERROR: --interval, --max-wait, and --persist-max must be non-negative integers" >&2
  exit 2
fi

if [[ "$INTERVAL" -eq 0 && "$MAX_WAIT" -gt 0 ]]; then
  echo "ERROR: --interval 0 is only valid with --max-wait 0" >&2
  exit 2
fi

if [[ "$PERSIST" -ne 1 && "$PERSIST_MAX" -gt 0 ]]; then
  echo "ERROR: --persist-max requires continuous wait mode; remove --finite" >&2
  exit 2
fi

if [[ "$BATON_SOURCE" == "run_id" ]] && ! is_safe_run_id "$DVANDVA_RUN_ID"; then
  echo "ERROR: DVANDVA_RUN_ID must be one safe path segment (letters, numbers, dot, underscore, dash; no slash or '..')" >&2
  exit 2
fi

elapsed=0
persist_started_at="$(date +%s)"

enforce_persist_max() {
  [[ "$PERSIST" -eq 1 && "$PERSIST_MAX" -gt 0 ]] || return 0

  local now total_elapsed
  now="$(date +%s)"
  total_elapsed=$((now - persist_started_at))
  if [[ "$total_elapsed" -ge "$PERSIST_MAX" ]]; then
    echo "DVANDVA_WAIT persist_max role=$ROLE file=$BATON_FILE total_elapsed=${total_elapsed}s persist_max=${PERSIST_MAX}s"
    exit 23
  fi
}

record_wait_elapsed() {
  elapsed=$((elapsed + INTERVAL))
  enforce_persist_max
}

contains_role() {
  local roles_csv="$1"
  [[ ",$roles_csv," == *",$ROLE,"* ]]
}

peer_role() {
  if [[ "$ROLE" == "vadi" ]]; then
    printf '%s' "prativadi"
  else
    printf '%s' "vadi"
  fi
}

derive_run_id() {
  # Path-authoritative: the baton's on-disk location is the source of truth for
  # its run id. The optional .run_id field is only a fallback for paths the
  # layout cannot classify. Trusting the field over the path lets a baton whose
  # .run_id disagrees with its directory mis-skip a genuine sibling (the
  # cr-c6-selfskip split-brain blind spot), so the path always wins when it is
  # determinate.
  local file="$1"
  local baton_run_id="${2:-}"
  if [[ "$file" == ".dvandva/baton.json" || "$file" == */.dvandva/baton.json ]]; then
    printf '%s' "legacy"
  elif [[ "$file" == */baton.json ]]; then
    printf '%s' "$(basename "$(dirname "$file")")"
  elif [[ -n "$baton_run_id" ]]; then
    printf '%s' "$baton_run_id"
  else
    printf '%s' "unknown"
  fi
}

derive_dvandva_root() {
  local file="$1"
  case "$file" in
    .dvandva/baton.json|.dvandva/runs/*/baton.json)
      printf '%s' ".dvandva"
      ;;
    */.dvandva/baton.json)
      printf '%s' "${file%/baton.json}"
      ;;
    */.dvandva/runs/*/baton.json)
      printf '%s' "${file%/runs/*/baton.json}"
      ;;
    *)
      printf '%s' ""
      ;;
  esac
}

scan_sibling_runs() {
  # selected_assignee is who the *selected* baton currently waits on; split-brain
  # only matters when that is the peer. The selected run is identified by file
  # path (-ef self-skip below), not by run id, so no run-id argument is needed.
  local selected_assignee="$1"
  local sibling_root sibling_file sibling_state sibling_status sibling_assignee sibling_active_roles sibling_run_id
  SIBLING_ACTIVE_COUNT=0
  SPLIT_BRAIN_SIBLING_RUN_ID=""
  sibling_root="$(derive_dvandva_root "$BATON_FILE")"
  if [[ -z "$sibling_root" ]]; then
    return 0
  fi

  shopt -s nullglob
  # Scan the active legacy baton (.dvandva/baton.json) alongside the named
  # runs/*/baton.json siblings: legacy is still a supported layout, so a
  # non-terminal legacy baton assigned to my role is just as much a split-brain
  # sibling as a named one. The legacy entry is a literal (not a glob), so guard
  # it with an existence test for the common case where no legacy baton exists.
  for sibling_file in "$sibling_root"/baton.json "$sibling_root"/runs/*/baton.json; do
    [[ -f "$sibling_file" ]] || continue
    # Self-skip by path identity (-ef), never by run id: the selected baton's
    # directory is authoritative. A run-id comparison would skip a genuine
    # sibling whose directory happens to match a stale .run_id field carried on
    # the selected baton (cr-c6-selfskip). -ef compares device+inode, so it
    # matches only the selected file itself.
    if [[ "$sibling_file" -ef "$BATON_FILE" ]]; then
      continue
    fi
    sibling_run_id="$(derive_run_id "$sibling_file")"
    sibling_state="$(jq -r '[.status // "", .assignee // "", ((.active_roles // []) | join(","))] | join("\u001f")' "$sibling_file" 2>/dev/null)" || continue
    IFS=$'\x1f' read -r sibling_status sibling_assignee sibling_active_roles <<< "$sibling_state"
    if [[ "$sibling_status" == "done" ]]; then
      continue
    fi
    SIBLING_ACTIVE_COUNT=$((SIBLING_ACTIVE_COUNT + 1))
    if [[ "${DVANDVA_CONCURRENT:-0}" != "1" ]] && [[ "$selected_assignee" == "$(peer_role)" ]] && { [[ "$sibling_assignee" == "$ROLE" ]] || contains_role "$sibling_active_roles"; }; then
      SPLIT_BRAIN_SIBLING_RUN_ID="$sibling_run_id"
      shopt -u nullglob
      return 0
    fi
  done
  shopt -u nullglob
}

heartbeat_selector_meta() {
  local run_id="$1"
  printf 'run_id=%s file=%s selected_by=%s sibling_active_runs=%s' \
    "$run_id" "$BATON_FILE" "$SELECTED_BY" "$SIBLING_ACTIVE_COUNT"
}

resolve_default_baton() {
  [[ "$BATON_SOURCE" == "legacy" ]] || return 0
  [[ -x "$RESOLVER_SCRIPT" ]] || { echo "ERROR: missing resolver at $RESOLVER_SCRIPT" >&2; exit 2; }

  local resolver_stdout resolver_rc resolver_stderr_file
  resolver_stderr_file="$(mktemp)"
  resolver_stdout="$("$RESOLVER_SCRIPT" --role "$ROLE" --cwd "$PWD" 2>"$resolver_stderr_file")"
  resolver_rc=$?
  if [[ -s "$resolver_stderr_file" ]]; then
    cat "$resolver_stderr_file" >&2
  fi
  rm -f "$resolver_stderr_file"

  case "$resolver_rc:$resolver_stdout" in
    0:RESOLVED\ *)
      BATON_FILE="${resolver_stdout#RESOLVED }"
      SELECTED_BY="resolve"
      ;;
    0:CREATE\ *)
      BATON_FILE="${resolver_stdout#CREATE }"
      SELECTED_BY="resolve_create"
      ;;
    12:ASK\ *)
      echo "DVANDVA_WAIT selection_required role=$ROLE ${resolver_stdout}" >&2
      exit 2
      ;;
    *)
      [[ -n "$resolver_stdout" ]] && echo "$resolver_stdout" >&2
      exit "${resolver_rc:-2}"
      ;;
  esac
}

wait_one_interval() {
  # Interruptible sleep: wake early on baton-directory events when
  # inotifywait exists. Watch the directory, not the file — an atomic
  # tmp+mv replace changes the inode and would orphan a file watch.
  # Spurious events are harmless; the loop re-checks state every wake.
  local dir
  dir="$(dirname "$BATON_FILE")"
  if command -v inotifywait >/dev/null 2>&1 && [[ -d "$dir" ]]; then
    # Exit 0 = event, 2 = timeout (both fine). Anything else (e.g. watch
    # limit exhausted) must fall back to sleep, or the loop would burn
    # elapsed time without any wall-clock wait and hit max-wait early.
    local rc=0
    inotifywait -qq -t "$INTERVAL" -e create,moved_to,close_write "$dir" 2>/dev/null || rc=$?
    if [[ "$rc" -ne 0 && "$rc" -ne 2 ]]; then
      sleep "$INTERVAL"
    fi
  else
    sleep "$INTERVAL"
  fi
}

resolve_default_baton

while true; do
  if [[ ! -f "$BATON_FILE" ]]; then
    selected_run_id="$(derive_run_id "$BATON_FILE")"
    scan_sibling_runs ""
    if [[ "$ALLOW_MISSING" -eq 1 ]]; then
      if [[ "$elapsed" -ge "$MAX_WAIT" ]]; then
        if [[ "$PERSIST" -eq 1 ]]; then
          if [[ "$INTERVAL" -eq 0 ]]; then
            echo "ERROR: continuous wait mode requires --interval > 0 when the baton is not ready; use --finite for an immediate heartbeat" >&2
            exit 2
          fi
          echo "DVANDVA_WAIT heartbeat role=$ROLE waiting_for=baton $(heartbeat_selector_meta "$selected_run_id") elapsed=${elapsed}s"
          elapsed=0
          wait_one_interval
          record_wait_elapsed
          continue
        fi
        echo "DVANDVA_WAIT timeout role=$ROLE waiting_for=baton file=$BATON_FILE elapsed=${elapsed}s"
        exit 20
      fi
      wait_one_interval
      record_wait_elapsed
      continue
    fi
    echo "DVANDVA_WAIT missing file=$BATON_FILE"
    exit 21
  fi

  JQ_STATE='[.run_id // "", .assignee // "", .status // "", .phase // "", (.checkpoint // 0 | tostring), .question // "", .resume_assignee // "", .resume_status // "", ((.active_roles // []) | join(",")), .updated_at // "", .current_engine // ""] | join("\u001f")'
  if ! state="$(jq -r "$JQ_STATE" "$BATON_FILE" 2>/dev/null)"; then
    # Torn-read tolerance: a concurrent writer may be mid-replace. One retry.
    sleep 1
    if ! state="$(jq -r "$JQ_STATE" "$BATON_FILE" 2>/dev/null)"; then
      echo "DVANDVA_WAIT invalid_json file=$BATON_FILE"
      exit 22
    fi
  fi

  IFS=$'\x1f' read -r baton_run_id assignee status phase checkpoint question resume_assignee resume_status active_roles updated_at current_engine <<< "$state"
  selected_run_id="$(derive_run_id "$BATON_FILE" "$baton_run_id")"
  # A baton whose .run_id field disagrees with its directory is itself suspect.
  # Path is authoritative (selected_run_id is path-derived above), so fail loud
  # by surfacing the inconsistent field rather than letting it drive any logic.
  run_id_note=""
  if [[ -n "$baton_run_id" && "$baton_run_id" != "$selected_run_id" ]]; then
    run_id_note=" run_id_field_mismatch=$baton_run_id"
  fi

  case "$status" in
    done)
      echo "DVANDVA_WAIT done phase=$phase checkpoint=$checkpoint assignee=$assignee"
      exit 10
      ;;
    human_decision)
      echo "DVANDVA_WAIT human_decision phase=$phase checkpoint=$checkpoint assignee=$assignee"
      exit 11
      ;;
    human_question)
      echo "DVANDVA_WAIT human_question phase=$phase checkpoint=$checkpoint assignee=$assignee resume_assignee=$resume_assignee resume_status=$resume_status question=$question"
      exit 12
      ;;
  esac

  if [[ "$assignee" == "$ROLE" ]]; then
    echo "DVANDVA_WAIT ready role=$ROLE phase=$phase status=$status checkpoint=$checkpoint"
    exit 0
  fi

  if [[ ",$active_roles," == *",$ROLE,"* ]]; then
    echo "DVANDVA_WAIT ready role=$ROLE phase=$phase status=$status checkpoint=$checkpoint assignee=$assignee active_roles=$active_roles"
    exit 0
  fi

  if [[ "$elapsed" -ge "$MAX_WAIT" ]]; then
    if [[ "$PERSIST" -eq 1 ]]; then
      if [[ "$INTERVAL" -eq 0 ]]; then
        echo "ERROR: continuous wait mode requires --interval > 0 when the baton is not ready; use --finite for an immediate heartbeat" >&2
        exit 2
      fi
      scan_sibling_runs "$assignee"
      if [[ -n "$SPLIT_BRAIN_SIBLING_RUN_ID" ]]; then
        echo "DVANDVA_WAIT split_brain role=$ROLE selected_run_id=$selected_run_id sibling_run_id=$SPLIT_BRAIN_SIBLING_RUN_ID waiting_on=$assignee $(heartbeat_selector_meta "$selected_run_id")$run_id_note"
        exit 29
      fi
      echo "DVANDVA_WAIT heartbeat role=$ROLE waiting_on=$assignee phase=$phase status=$status checkpoint=$checkpoint active_roles=$active_roles $(heartbeat_selector_meta "$selected_run_id") elapsed=${elapsed}s last_seen_engine=$current_engine updated_at=$updated_at$run_id_note"
      elapsed=0
      wait_one_interval
      record_wait_elapsed
      continue
    fi
    echo "DVANDVA_WAIT timeout role=$ROLE waiting_on=$assignee phase=$phase status=$status checkpoint=$checkpoint active_roles=$active_roles elapsed=${elapsed}s"
    exit 20
  fi

  wait_one_interval
  record_wait_elapsed
done
