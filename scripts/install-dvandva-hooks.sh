#!/usr/bin/env bash
# install-dvandva-hooks.sh
# Sets the repo-local git hooks path to .githooks so the Dvandva commit gate
# is enforced for every commit in this clone.  NEVER modifies global config.
#
# Usage:
#   install-dvandva-hooks.sh [<repo-root>]           — install
#   install-dvandva-hooks.sh [<repo-root>] --force   — install even if a
#                                                       different hooksPath is set
#   install-dvandva-hooks.sh [<repo-root>] --uninstall — unset core.hooksPath
#
# Idempotent: safe to run multiple times.  The installer refuses to override
# a pre-existing foreign core.hooksPath unless --force is given; this
# prevents silently breaking repos that already use a custom hooks directory.
set -u

UNINSTALL=0
FORCE=0
REPO_ARG=""

for arg in "$@"; do
  case "$arg" in
    --uninstall) UNINSTALL=1 ;;
    --force)     FORCE=1 ;;
    -*)
      echo "install-dvandva-hooks: unknown option: $arg" >&2
      echo "Usage: install-dvandva-hooks.sh [<repo-root>] [--force] [--uninstall]" >&2
      exit 2
      ;;
    *)
      if [[ -n "$REPO_ARG" ]]; then
        echo "install-dvandva-hooks: too many positional arguments" >&2
        exit 2
      fi
      REPO_ARG="$arg"
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Resolve the git repository root
# ---------------------------------------------------------------------------
if [[ -n "$REPO_ARG" ]]; then
  if ! REPO_ROOT="$(cd "$REPO_ARG" 2>/dev/null && git rev-parse --show-toplevel 2>/dev/null)"; then
    echo "install-dvandva-hooks: not a git repository: $REPO_ARG" >&2
    exit 1
  fi
else
  if ! REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    echo "install-dvandva-hooks: not inside a git repository" >&2
    exit 1
  fi
fi

TARGET_HOOKS_PATH=".githooks"
PENDING_ROOT_BASELINE="__DVANDVA_ROOT_PENDING__"

record_hook_adoption_baseline() {
  local existing=""
  existing="$(git -C "$REPO_ROOT" config --local dvandva.hooksAdoptedAt 2>/dev/null || echo "")"
  if [[ -n "$existing" ]] && git -C "$REPO_ROOT" cat-file -e "$existing^{commit}" 2>/dev/null; then
    echo "install-dvandva-hooks: hook adoption baseline already recorded (dvandva.hooksAdoptedAt=$existing)"
    return 0
  fi

  local head_sha=""
  if head_sha="$(git -C "$REPO_ROOT" rev-parse --verify HEAD 2>/dev/null)"; then
    if [[ "$existing" == "$PENDING_ROOT_BASELINE" ]]; then
      local root_sha=""
      root_sha="$(git -C "$REPO_ROOT" rev-list --max-parents=0 --reverse HEAD 2>/dev/null | head -n 1 || true)"
      if [[ -n "$root_sha" ]]; then
        git -C "$REPO_ROOT" config --local dvandva.hooksAdoptedAt "$root_sha"
        git -C "$REPO_ROOT" config --local dvandva.hooksAdoptedAtInclusive true
        echo "install-dvandva-hooks: backfilled hook adoption baseline dvandva.hooksAdoptedAt=$root_sha"
        return 0
      fi
    fi

    git -C "$REPO_ROOT" config --local dvandva.hooksAdoptedAt "$head_sha"
    git -C "$REPO_ROOT" config --local --unset dvandva.hooksAdoptedAtInclusive >/dev/null 2>&1 || true
    echo "install-dvandva-hooks: recorded hook adoption baseline dvandva.hooksAdoptedAt=$head_sha"
  else
    git -C "$REPO_ROOT" config --local dvandva.hooksAdoptedAt "$PENDING_ROOT_BASELINE"
    git -C "$REPO_ROOT" config --local dvandva.hooksAdoptedAtInclusive true
    echo "install-dvandva-hooks: no HEAD commit yet; recorded pending root hook adoption baseline."
  fi
}

# ---------------------------------------------------------------------------
# --uninstall: clear core.hooksPath
# ---------------------------------------------------------------------------
if [[ "$UNINSTALL" -eq 1 ]]; then
  current="$(git -C "$REPO_ROOT" config --local core.hooksPath 2>/dev/null || echo "")"
  if [[ -z "$current" ]]; then
    echo "install-dvandva-hooks: core.hooksPath is not set — nothing to uninstall."
    exit 0
  fi
  git -C "$REPO_ROOT" config --local --unset core.hooksPath
  git -C "$REPO_ROOT" config --local --unset dvandva.hooksAdoptedAt >/dev/null 2>&1 || true
  git -C "$REPO_ROOT" config --local --unset dvandva.hooksAdoptedAtInclusive >/dev/null 2>&1 || true
  echo "install-dvandva-hooks: unset core.hooksPath (was: $current) in $REPO_ROOT"
  exit 0
fi

# ---------------------------------------------------------------------------
# Install: read the current (local) value
# ---------------------------------------------------------------------------
current="$(git -C "$REPO_ROOT" config --local core.hooksPath 2>/dev/null || echo "")"

# Already set to our target → idempotent, nothing to do.
if [[ "$current" == "$TARGET_HOOKS_PATH" ]]; then
  record_hook_adoption_baseline
  echo "install-dvandva-hooks: already installed (core.hooksPath=$TARGET_HOOKS_PATH in $REPO_ROOT)"
  exit 0
fi

# Set to a different value → refuse unless --force.
if [[ -n "$current" && "$current" != "$TARGET_HOOKS_PATH" ]]; then
  if [[ "$FORCE" -eq 0 ]]; then
    echo "install-dvandva-hooks: error: core.hooksPath is already set to '$current' (not '$TARGET_HOOKS_PATH')." >&2
    echo "  Use --force to override, or --uninstall to clear it first." >&2
    exit 1
  fi
  echo "install-dvandva-hooks: overriding core.hooksPath='$current' with --force"
fi

git -C "$REPO_ROOT" config --local core.hooksPath "$TARGET_HOOKS_PATH"
record_hook_adoption_baseline
echo "install-dvandva-hooks: set core.hooksPath=$TARGET_HOOKS_PATH in $REPO_ROOT"
exit 0
