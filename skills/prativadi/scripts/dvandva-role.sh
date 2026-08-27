#!/usr/bin/env bash
set -euo pipefail

skill_name="$(basename "$(dirname "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")")")"
case "$skill_name" in
  vadi) role="worker" ;;
  prativadi) role="reviewer" ;;
  *)
    printf 'dvandva-role: unsupported skill directory: %s\n' "$skill_name" >&2
    exit 2
    ;;
esac
data_home="${XDG_DATA_HOME:-${HOME:?HOME is required}/.local/share}"
state_home="${XDG_STATE_HOME:-${HOME:?HOME is required}/.local/state}"
binary="$data_home/dvandva/bin/current/dvandva-kernel"
runs_dir="$state_home/dvandva/runs"
credentials_root="$state_home/dvandva/credentials"
schema="dvandva.run.v1"

require_kernel() {
  test -x "$binary" || {
    printf 'dvandva-role: kernel missing; explicitly invoke $setup-dvandva first\n' >&2
    exit 1
  }
  local probe_output
  probe_output="$("$binary" probe --expected-schema "$schema")" || {
    printf 'dvandva-role: incompatible kernel; explicitly invoke $setup-dvandva doctor\n' >&2
    exit 1
  }
  grep -Fq '"compatible": true' <<<"$probe_output" || {
      printf 'dvandva-role: incompatible kernel; explicitly invoke $setup-dvandva doctor\n' >&2
      exit 1
    }
}

session_id() {
  local candidate
  for candidate in \
    "${DVANDVA_SESSION_ID:-}" \
    "${T3_SESSION_ID:-}" \
    "${CODEX_SESSION_ID:-}" \
    "${CODEX_THREAD_ID:-}" \
    "${CLAUDE_SESSION_ID:-}"; do
    if test -n "$candidate"; then
      printf '%s\n' "$candidate"
      return
    fi
  done
  if test "${1:-}" = "--generate"; then
    if test -r /proc/sys/kernel/random/uuid; then
      tr -d '\n' </proc/sys/kernel/random/uuid
      printf '\n'
      return
    fi
    if command -v uuidgen >/dev/null 2>&1; then
      uuidgen | tr '[:upper:]' '[:lower:]'
      return
    fi
  fi
  printf 'dvandva-role: no stable harness session ID; rerun with session-id --generate and retain it for this harness session\n' >&2
  exit 1
}

start_role() {
  test "$#" -ge 5 || {
    printf 'usage: dvandva-role.sh start SESSION HARNESS PEER WORKSPACE OBJECTIVE [TASK] [--wait|--new-run|--run-id ID]\n' >&2
    exit 2
  }
  local session="$1"
  local harness="$2"
  local peer="$3"
  local workspace="$4"
  local objective="$5"
  shift 5
  case "$harness:$peer" in
    codex:claude|claude:codex) ;;
    *)
      printf 'dvandva-role: harness families must be exactly codex and claude\n' >&2
      exit 2
      ;;
  esac
  local task=""
  local wait_flag=""
  local new_flag=""
  local selected_run=""
  while (($#)); do
    case "$1" in
      --wait) wait_flag="--wait" ;;
      --new-run) new_flag="--new-run" ;;
      --run-id)
        selected_run="${2:-}"
        test -n "$selected_run" || {
          printf 'dvandva-role: --run-id requires a value\n' >&2
          exit 2
        }
        shift
        ;;
      *)
        if test -n "$task"; then
          printf 'dvandva-role: unexpected start argument: %s\n' "$1" >&2
          exit 2
        fi
        task="$1"
        ;;
    esac
    shift
  done
  local args=(
    role start
    --workspace "$workspace"
    --runs-dir "$runs_dir"
    --credentials-root "$credentials_root"
    --role "$role"
    --session-id "$session"
    --current-harness "$harness"
    --peer-harness "$peer"
    --objective "$objective"
    --lease-seconds "${DVANDVA_LEASE_SECONDS:-1800}"
    --timeout-ms "${DVANDVA_WAIT_TIMEOUT_MS:-300000}"
  )
  if test -n "$task"; then
    args+=(--task-reference "$task")
  fi
  if test -n "$wait_flag"; then
    args+=("$wait_flag")
  fi
  if test -n "$new_flag"; then
    args+=("$new_flag")
  fi
  if test -n "$selected_run"; then
    test -z "$new_flag" || {
      printf 'dvandva-role: --run-id and --new-run are mutually exclusive\n' >&2
      exit 2
    }
    args+=(--run-id "$selected_run")
  fi
  "$binary" "${args[@]}"
}

run_dir_command() {
  local command="$1"
  shift
  test "$#" -ge 2 || {
    printf 'dvandva-role: %s requires SESSION RUN_DIR\n' "$command" >&2
    exit 2
  }
  local session="$1"
  local run_dir="$2"
  shift 2
  case "$command" in
    read)
      test "$#" -eq 0
      "$binary" role read \
        --run-dir "$run_dir" \
        --role "$role" \
        --session-id "$session" \
        --credentials-root "$credentials_root"
      ;;
    apply)
      test "$#" -eq 2 || {
        printf 'usage: dvandva-role.sh apply SESSION RUN_DIR REVISION ACTION_JSON\n' >&2
        exit 2
      }
      "$binary" role apply \
        --run-dir "$run_dir" \
        --role "$role" \
        --session-id "$session" \
        --expected-revision "$1" \
        --credentials-root "$credentials_root" \
        --action "$2"
      ;;
    wait)
      test "$#" -ge 1 && test "$#" -le 2 || {
        printf 'usage: dvandva-role.sh wait SESSION RUN_DIR REVISION [TIMEOUT_MS]\n' >&2
        exit 2
      }
      local timeout="${2:-300000}"
      "$binary" role wait \
        --run-dir "$run_dir" \
        --role "$role" \
        --session-id "$session" \
        --credentials-root "$credentials_root" \
        --after-revision "$1" \
        --timeout-ms "$timeout"
      ;;
    heartbeat)
      test "$#" -eq 1 || {
        printf 'usage: dvandva-role.sh heartbeat SESSION RUN_DIR REVISION\n' >&2
        exit 2
      }
      "$binary" role heartbeat \
        --run-dir "$run_dir" \
        --role "$role" \
        --session-id "$session" \
        --lease-seconds "${DVANDVA_LEASE_SECONDS:-1800}" \
        --expected-revision "$1" \
        --credentials-root "$credentials_root"
      ;;
  esac
}

operation="${1:-}"
shift || true
case "$operation" in
  session-id) session_id "$@" ;;
  probe)
    require_kernel
    "$binary" probe --expected-schema "$schema"
    ;;
  start)
    require_kernel
    start_role "$@"
    ;;
  read|apply|wait|heartbeat)
    require_kernel
    run_dir_command "$operation" "$@"
    ;;
  *)
    printf 'usage: dvandva-role.sh {session-id|probe|start|read|apply|wait|heartbeat} ...\n' >&2
    exit 2
    ;;
esac
