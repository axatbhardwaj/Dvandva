#!/usr/bin/env bash
# Dvandva role hook preflight.
#
# Active hook preflight installs or refreshes the delegated Dvandva wrapper
# chain in the target repo, then proves gate reachability by exercising the
# active pre-commit hook with DVANDVA_HOOK_SELFCHECK=1.
set -u

ROLE=""
REPO_ARG=""
MODE="${DVANDVA_HOOK_PREFLIGHT:-auto}"
SENTINEL="DVANDVA_GATE_WIRED"

usage() {
  cat <<'EOF'
Usage: dvandva-hook-preflight.sh --role <vadi|prativadi> [--repo <path>] [--mode auto|strict|off]
EOF
}

resolve_repo_root() {
  local repo_arg="$1"
  if [[ -n "$repo_arg" ]]; then
    cd "$repo_arg" 2>/dev/null && git rev-parse --show-toplevel 2>/dev/null
    return
  fi
  git rev-parse --show-toplevel 2>/dev/null
}

active_hook_dir() {
  local current git_common
  current="$(git -C "$REPO_ROOT" config --local core.hooksPath 2>/dev/null || echo "")"
  if [[ -z "$current" ]]; then
    git_common="$(git -C "$REPO_ROOT" rev-parse --git-common-dir 2>/dev/null || echo "")"
    printf '%s/hooks\n' "$git_common"
    return
  fi
  case "$current" in
    /*) printf '%s\n' "$current" ;;
    *)  printf '%s/%s\n' "$REPO_ROOT" "$current" ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --role)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      ROLE="$2"
      shift 2
      ;;
    --repo)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      REPO_ARG="$2"
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
  echo "DVANDVA_HOOK_PREFLIGHT role=$ROLE mode=$MODE result=error reason=role_mismatch env_role=$ENV_ROLE"
  exit 1
fi
export DVANDVA_ROLE="$ROLE"

if ! REPO_ROOT="$(resolve_repo_root "$REPO_ARG")"; then
  echo "DVANDVA_HOOK_PREFLIGHT role=$ROLE mode=$MODE result=error reason=not_git repo=${REPO_ARG:-$PWD}"
  exit 1
fi

if [[ "$MODE" == "off" ]]; then
  echo "DVANDVA_HOOK_PREFLIGHT role=$ROLE mode=off result=off repo=$REPO_ROOT"
  exit 0
fi

INSTALLER="$REPO_ROOT/scripts/install-dvandva-hooks.sh"
if [[ ! -f "$INSTALLER" ]]; then
  echo "DVANDVA_HOOK_PREFLIGHT role=$ROLE mode=$MODE result=error reason=missing_installer repo=$REPO_ROOT"
  exit 1
fi

if ! install_out="$(bash "$INSTALLER" "$REPO_ROOT" 2>&1)"; then
  echo "DVANDVA_HOOK_PREFLIGHT role=$ROLE mode=$MODE result=error reason=install_failed repo=$REPO_ROOT"
  [[ -n "$install_out" ]] && printf '%s\n' "$install_out"
  exit 1
fi

HOOK_DIR="$(active_hook_dir)"
PRE_COMMIT="$HOOK_DIR/pre-commit"
if [[ ! -x "$PRE_COMMIT" ]]; then
  echo "DVANDVA_HOOK_PREFLIGHT role=$ROLE mode=$MODE result=error reason=broken_chain repo=$REPO_ROOT active_pre_commit=$(realpath -m "$PRE_COMMIT")"
  exit 1
fi

probe_out="$(cd "$REPO_ROOT" && DVANDVA_HOOK_SELFCHECK=1 "$PRE_COMMIT" 2>&1)"; probe_rc=$?
if [[ "$probe_rc" -ne 0 || "$probe_out" != *"$SENTINEL"* ]]; then
  echo "DVANDVA_HOOK_PREFLIGHT role=$ROLE mode=$MODE result=error reason=probe_failed repo=$REPO_ROOT active_pre_commit=$(realpath -m "$PRE_COMMIT") sentinel=$SENTINEL"
  [[ -n "$probe_out" ]] && printf '%s\n' "$probe_out"
  exit 1
fi

CURRENT_HOOKS="$(git -C "$REPO_ROOT" config --local core.hooksPath 2>/dev/null || echo "")"
PRIOR_HOOKS="$(git -C "$REPO_ROOT" config --local dvandva.priorHooksPath 2>/dev/null || echo "")"
echo "DVANDVA_HOOK_PREFLIGHT role=$ROLE mode=$MODE result=ok repo=$REPO_ROOT hooks_path=${CURRENT_HOOKS:-default} prior_hooks_path=${PRIOR_HOOKS:-unset} active_pre_commit=$(realpath -m "$PRE_COMMIT") sentinel=$SENTINEL"
exit 0
