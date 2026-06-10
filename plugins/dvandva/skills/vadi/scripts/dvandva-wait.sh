#!/usr/bin/env bash
# Cheap foreground wait for Dvandva baton ownership.
#
# Wakes early on baton-directory inotify events when inotifywait is
# available; otherwise sleeps INTERVAL between checks. The 540s default
# max-wait keeps one invocation inside Claude Code's 600s Bash-tool cap.
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
#   20 timed out while another actor owns the baton
#   21 baton file missing
#   22 baton JSON invalid
#   2  usage error
set -u

ROLE=""
BATON_FILE=".dvandva/baton.json"
INTERVAL=60
MAX_WAIT=540
ALLOW_MISSING=0

usage() {
  cat >&2 <<'USAGE'
Usage: dvandva-wait.sh --role <vadi|prativadi> [--file .dvandva/baton.json] [--interval seconds] [--max-wait seconds] [--allow-missing]

Defaults: --interval 60 --max-wait 540

Wakes early on baton-directory changes when inotifywait is available;
otherwise sleeps INTERVAL between checks. 540 keeps one invocation
inside Claude Code's 600s Bash-tool maximum.

With --allow-missing, a missing baton file does not exit 21 immediately;
the helper instead sleeps INTERVAL and retries until the file appears
or --max-wait elapses (returns 20 on timeout).
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

if ! [[ "$INTERVAL" =~ ^[0-9]+$ && "$MAX_WAIT" =~ ^[0-9]+$ ]]; then
  echo "ERROR: --interval and --max-wait must be non-negative integers" >&2
  exit 2
fi

if [[ "$INTERVAL" -eq 0 && "$MAX_WAIT" -gt 0 ]]; then
  echo "ERROR: --interval 0 is only valid with --max-wait 0" >&2
  exit 2
fi

elapsed=0

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

while true; do
  if [[ ! -f "$BATON_FILE" ]]; then
    if [[ "$ALLOW_MISSING" -eq 1 ]]; then
      if [[ "$elapsed" -ge "$MAX_WAIT" ]]; then
        echo "DVANDVA_WAIT timeout role=$ROLE waiting_for=baton file=$BATON_FILE elapsed=${elapsed}s"
        exit 20
      fi
      wait_one_interval
      elapsed=$((elapsed + INTERVAL))
      continue
    fi
    echo "DVANDVA_WAIT missing file=$BATON_FILE"
    exit 21
  fi

  JQ_STATE='[.assignee // "", .status // "", .phase // "", (.checkpoint // 0 | tostring), .question // "", .resume_assignee // "", .resume_status // ""] | @tsv'
  if ! state="$(jq -r "$JQ_STATE" "$BATON_FILE" 2>/dev/null)"; then
    # Torn-read tolerance: a concurrent writer may be mid-replace. One retry.
    sleep 1
    if ! state="$(jq -r "$JQ_STATE" "$BATON_FILE" 2>/dev/null)"; then
      echo "DVANDVA_WAIT invalid_json file=$BATON_FILE"
      exit 22
    fi
  fi

  IFS=$'\t' read -r assignee status phase checkpoint question resume_assignee resume_status <<< "$state"

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

  if [[ "$elapsed" -ge "$MAX_WAIT" ]]; then
    echo "DVANDVA_WAIT timeout role=$ROLE waiting_on=$assignee phase=$phase status=$status checkpoint=$checkpoint elapsed=${elapsed}s"
    exit 20
  fi

  wait_one_interval
  elapsed=$((elapsed + INTERVAL))
done
