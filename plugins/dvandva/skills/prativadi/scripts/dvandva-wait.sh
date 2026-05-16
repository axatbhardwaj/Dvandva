#!/usr/bin/env bash
# Cheap foreground wait for Dvandva baton ownership.
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
MAX_WAIT=900
ALLOW_MISSING=0

usage() {
  cat >&2 <<'USAGE'
Usage: dvandva-wait.sh --role <vadi|prativadi> [--file .dvandva/baton.json] [--interval seconds] [--max-wait seconds] [--allow-missing]

Defaults: --interval 60 --max-wait 900

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

while true; do
  if [[ ! -f "$BATON_FILE" ]]; then
    if [[ "$ALLOW_MISSING" -eq 1 ]]; then
      if [[ "$elapsed" -ge "$MAX_WAIT" ]]; then
        echo "DVANDVA_WAIT timeout role=$ROLE waiting_for=baton file=$BATON_FILE elapsed=${elapsed}s"
        exit 20
      fi
      sleep "$INTERVAL"
      elapsed=$((elapsed + INTERVAL))
      continue
    fi
    echo "DVANDVA_WAIT missing file=$BATON_FILE"
    exit 21
  fi

  if ! state="$(jq -r '[.assignee // "", .status // "", .phase // "", (.checkpoint // 0 | tostring), .question // "", .resume_assignee // "", .resume_status // ""] | @tsv' "$BATON_FILE" 2>/dev/null)"; then
    echo "DVANDVA_WAIT invalid_json file=$BATON_FILE"
    exit 22
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

  sleep "$INTERVAL"
  elapsed=$((elapsed + INTERVAL))
done
