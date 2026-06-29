#!/usr/bin/env bash
# dvandva-hook-lib.sh
# Shared helpers for the Dvandva delegating git-hook wrappers.
#
# Sourced by the materialized hook dir scripts (pre-commit, prepare-commit-msg,
# and the enumerated pass-through stubs).  It provides:
#
#   resolve_prior_hook <name>   echo the path to the prior hook of <name> if one
#                               exists, is executable, and is NOT one of our own
#                               wrappers (self-loop guard); else return non-zero.
#
#   dvandva_hook_selfcheck <token> [<dir>]
#                               when DVANDVA_HOOK_SELFCHECK=1, print "<token>:<dir>"
#                               and return 0 so the caller can short-circuit BEFORE
#                               running the real gate; otherwise return 1.
#
# The caller is expected to export (or pre-set) DVANDVA_REPO_ROOT and
# DVANDVA_HOOK_SELF before sourcing.  Both have safe fallbacks here.
#
# Pure POSIX-ish bash + git; no jq dependency in the library itself.

# Repo root the hooks operate on (the repo being committed to).
: "${DVANDVA_REPO_ROOT:=$(git rev-parse --show-toplevel 2>/dev/null || true)}"

# DVANDVA_HOOK_SELFCHECK sentinel short-circuit helper.
#   Usage: dvandva_hook_selfcheck "DVANDVA_GATE_WIRED" "$HOOK_DIR" && exit 0
dvandva_hook_selfcheck() {
  [[ "${DVANDVA_HOOK_SELFCHECK:-0}" == "1" ]] || return 1
  printf '%s:%s\n' "$1" "${2:-}"
  return 0
}

# resolve_prior_hook <name>
#   Resolution of the prior hooks directory mirrors the installer's record:
#     dvandva.priorHooksPath empty | __DVANDVA_DEFAULT__ -> the repo's TRUE
#         default hooks dir ($GIT_COMMON_DIR/hooks).  We intentionally use
#         --git-common-dir rather than `git rev-parse --git-path hooks`: the
#         latter honors the now-overridden core.hooksPath (which points at OUR
#         dir) and would resolve to ourselves.  --git-common-dir always yields
#         the real default, and is worktree-correct.
#     absolute path  -> used verbatim.
#     relative path  -> resolved against DVANDVA_REPO_ROOT.
#   The candidate hook is skipped (return 1) when it does not exist, when it is
#   not executable, or when it resolves to this very wrapper (self-loop guard).
resolve_prior_hook() {
  local name="$1"
  local self="${DVANDVA_HOOK_SELF:-$0}"
  local root="${DVANDVA_REPO_ROOT:-}"
  local gitc="${root:-.}"
  local prior dir hook

  prior="$(git -C "$gitc" config --local dvandva.priorHooksPath 2>/dev/null || echo "")"

  if [[ -z "$prior" || "$prior" == "__DVANDVA_DEFAULT__" ]]; then
    dir="$(git -C "$gitc" rev-parse --git-common-dir 2>/dev/null || echo "")/hooks"
  elif [[ "$prior" == /* ]]; then
    dir="$prior"
  else
    dir="${root}/$prior"
  fi

  # Anchor any relative dir to the repo root so resolution is cwd-independent.
  case "$dir" in
    /*) ;;
    *)  dir="${root}/$dir" ;;
  esac

  hook="$dir/$name"
  [[ -e "$hook" ]] || return 1

  # Self-loop guard: never delegate to one of our own materialized hooks.
  local rp_hook rp_self
  rp_hook="$(realpath "$hook" 2>/dev/null || printf '%s' "$hook")"
  rp_self="$(realpath "$self" 2>/dev/null || printf '%s' "$self")"
  [[ "$rp_hook" != "$rp_self" ]] || return 1

  [[ -x "$hook" ]] || return 1

  printf '%s\n' "$hook"
  return 0
}
