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
kernel_version="0.2.0"
schema="dvandva.run.v2"
role_api="2"

incompatible_kernel() {
  printf 'dvandva-role: incompatible kernel; explicitly invoke $setup-dvandva doctor\n' >&2
  exit 1
}

require_kernel() {
  test -x "$binary" || {
    printf 'dvandva-role: kernel missing; explicitly invoke $setup-dvandva first\n' >&2
    exit 1
  }
  local version_output probe_output
  version_output="$("$binary" --version 2>/dev/null)" || incompatible_kernel
  test "$version_output" = "dvandva-v4 $kernel_version" || incompatible_kernel
  probe_output="$("$binary" probe \
    --expected-schema "$schema" --expected-role-api "$role_api" 2>/dev/null)" || incompatible_kernel
  grep -Fq '"package": "dvandva-v4"' <<<"$probe_output" || incompatible_kernel
  grep -Fq "\"version\": \"$kernel_version\"" <<<"$probe_output" || incompatible_kernel
  grep -Fq "\"write_schema\": \"$schema\"" <<<"$probe_output" || incompatible_kernel
  grep -Fq '"role_api": 2' <<<"$probe_output" || incompatible_kernel
  grep -Fq '"compatible": true' <<<"$probe_output" || incompatible_kernel
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
  test "$#" -ge 4 || {
    printf 'usage: dvandva-role.sh start SESSION HARNESS PEER WORKSPACE [OBJECTIVE [TASK]] [--objective-ref KIND=VALUE] [--required-deliverable ID=DESCRIPTION] [--wait|--new-run|--run-id ID]\n' >&2
    exit 2
  }
  local session="$1" harness="$2" peer="$3" workspace="$4"
  shift 4
  case "$harness:$peer" in
    codex:claude|claude:codex) ;;
    *)
      printf 'dvandva-role: harness families must be exactly codex and claude\n' >&2
      exit 2
      ;;
  esac

  local objective="" task="" wait_flag="" new_flag="" selected_run=""
  local -a objective_refs=() deliverables=()
  if (($#)) && [[ "$1" != --* ]]; then
    objective="$1"
    shift
  fi
  if (($#)) && [[ "$1" != --* ]]; then
    task="$1"
    shift
  fi
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
      --objective-ref)
        test -n "${2:-}" || {
          printf 'dvandva-role: --objective-ref requires a value\n' >&2
          exit 2
        }
        objective_refs+=("$2")
        shift
        ;;
      --task-reference)
        test -n "${2:-}" || {
          printf 'dvandva-role: --task-reference requires a value\n' >&2
          exit 2
        }
        task="$2"
        shift
        ;;
      --required-deliverable)
        test -n "${2:-}" || {
          printf 'dvandva-role: --required-deliverable requires a value\n' >&2
          exit 2
        }
        deliverables+=("$2")
        shift
        ;;
      *)
        printf 'dvandva-role: unexpected start argument: %s\n' "$1" >&2
        exit 2
        ;;
    esac
    shift
  done
  if test -z "$selected_run"; then
    test -n "$objective" || {
      printf 'dvandva-role: new or discovered runs require an objective\n' >&2
      exit 2
    }
    test "${#deliverables[@]}" -gt 0 || {
      printf 'dvandva-role: new runs require --required-deliverable\n' >&2
      exit 2
    }
  fi
  test -z "$selected_run" || test -z "$new_flag" || {
    printf 'dvandva-role: --run-id and --new-run are mutually exclusive\n' >&2
    exit 2
  }

  local args=(
    role start --api "$role_api"
    --workspace "$workspace"
    --runs-dir "$runs_dir"
    --credentials-root "$credentials_root"
    --role "$role"
    --session-id "$session"
    --current-harness "$harness"
    --peer-harness "$peer"
    --lease-seconds "${DVANDVA_LEASE_SECONDS:-1800}"
    --timeout-ms "${DVANDVA_WAIT_TIMEOUT_MS:-300000}"
  )
  test -z "$objective" || args+=(--objective "$objective")
  test -z "$task" || args+=(--task-reference "$task")
  local value
  for value in "${objective_refs[@]}"; do args+=(--objective-ref "$value"); done
  for value in "${deliverables[@]}"; do args+=(--required-deliverable "$value"); done
  test -z "$wait_flag" || args+=("$wait_flag")
  test -z "$new_flag" || args+=("$new_flag")
  test -z "$selected_run" || args+=(--run-id "$selected_run")
  "$binary" "${args[@]}"
}

run_dir_command() {
  local command="$1"
  shift
  test "$#" -ge 2 || {
    printf 'dvandva-role: %s requires SESSION RUN_DIR\n' "$command" >&2
    exit 2
  }
  local session="$1" run_dir="$2"
  shift 2
  local -a common=(--api "$role_api" --run-dir "$run_dir" --role "$role" \
    --session-id "$session" --credentials-root "$credentials_root")
  case "$command" in
    read)
      test "$#" -eq 0
      "$binary" role read "${common[@]}"
      ;;
    claim|reclaim)
      test "$#" -eq 1 || {
        printf 'usage: dvandva-role.sh %s SESSION RUN_DIR REVISION\n' "$command" >&2
        exit 2
      }
      "$binary" role "$command" "${common[@]}" \
        --lease-seconds "${DVANDVA_LEASE_SECONDS:-1800}" --expected-revision "$1"
      ;;
    apply)
      test "$#" -eq 2 || {
        printf 'usage: dvandva-role.sh apply SESSION RUN_DIR REVISION ACTION_JSON\n' >&2
        exit 2
      }
      "$binary" role apply "${common[@]}" --expected-revision "$1" --action "$2"
      ;;
    wait)
      test "$#" -ge 1 && test "$#" -le 2 || {
        printf 'usage: dvandva-role.sh wait SESSION RUN_DIR REVISION [TIMEOUT_MS]\n' >&2
        exit 2
      }
      "$binary" role wait "${common[@]}" --after-revision "$1" \
        --timeout-ms "${2:-300000}"
      ;;
    heartbeat)
      test "$#" -eq 1 || {
        printf 'usage: dvandva-role.sh heartbeat SESSION RUN_DIR REVISION\n' >&2
        exit 2
      }
      "$binary" role heartbeat "${common[@]}" \
        --lease-seconds "${DVANDVA_LEASE_SECONDS:-1800}" --expected-revision "$1"
      ;;
  esac
}

upgrade_role() {
  test "$#" -eq 5 || {
    printf 'usage: dvandva-role.sh upgrade SESSION RUN_DIR HARNESS PEER REVISION\n' >&2
    exit 2
  }
  local session="$1" run_dir="$2" harness="$3" peer="$4" revision="$5"
  case "$harness:$peer" in codex:claude|claude:codex) ;; *)
    printf 'dvandva-role: harness families must be exactly codex and claude\n' >&2; exit 2 ;;
  esac
  "$binary" role upgrade --api "$role_api" --run-dir "$run_dir" --role "$role" \
    --session-id "$session" --current-harness "$harness" --peer-harness "$peer" \
    --expected-revision "$revision" --credentials-root "$credentials_root"
}

operation="${1:-}"
shift || true
case "$operation" in
  session-id) session_id "$@" ;;
  probe)
    require_kernel
    "$binary" probe --expected-schema "$schema" --expected-role-api "$role_api"
    ;;
  start)
    require_kernel
    start_role "$@"
    ;;
  read|claim|reclaim|apply|wait|heartbeat)
    require_kernel
    run_dir_command "$operation" "$@"
    ;;
  upgrade)
    require_kernel
    upgrade_role "$@"
    ;;
  *)
    printf 'usage: dvandva-role.sh {session-id|probe|start|read|claim|reclaim|apply|wait|heartbeat|upgrade} ...\n' >&2
    exit 2
    ;;
esac
