#!/usr/bin/env bash
# CLI / git-pre-commit adapter for adversarial_loop_gate_predicate.
# Unlike the Claude Code Stop adapter, this command reads no hook JSON. The
# owning session comes from --session or ADVERSARIAL_LOOP_SESSION.

set -o pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
# shellcheck source=../lib/predicate.sh
source "$SCRIPT_DIR/../lib/predicate.sh"

usage() {
  printf 'usage: %s [--session <id>]\n' "${0##*/}" >&2
}

session_id=${ADVERSARIAL_LOOP_SESSION-}
while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      if [[ $# -lt 2 ]]; then
        usage
        exit 2
      fi
      session_id=$2
      shift 2
      ;;
    --session=*)
      session_id=${1#--session=}
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf 'adversarial-loop: unknown argument: %s\n' "$1" >&2
      usage
      exit 2
      ;;
  esac
done

repo_root=$(git -C "$PWD" rev-parse --show-toplevel 2>/dev/null || true)
if [[ -z "$repo_root" ]]; then
  repo_root=$(pwd -P)
fi

predicate_result=$(adversarial_loop_gate_predicate "$repo_root" "$session_id")
if [[ "$predicate_result" != *$'\t'* ]]; then
  printf '%s\n' 'adversarial-loop: gate predicate returned an invalid response' >&2
  exit 2
fi

decision=${predicate_result%%$'\t'*}
reason=${predicate_result#*$'\t'}
case "$decision" in
  allow)
    exit 0
    ;;
  block)
    printf 'adversarial-loop: blocked: %s\n' "$reason" >&2
    exit 1
    ;;
  *)
    printf 'adversarial-loop: invalid predicate decision: %s\n' "$decision" >&2
    exit 2
    ;;
esac
