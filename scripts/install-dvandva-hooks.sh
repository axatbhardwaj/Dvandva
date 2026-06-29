#!/usr/bin/env bash
# install-dvandva-hooks.sh — delegating, plugin-shipped, reversible work-gate.
#
# Dvandva no longer fights over the single core.hooksPath slot.  It points
# core.hooksPath at its OWN gitignored hook dir (.dvandva/githooks), records the
# prior owner (Husky / lefthook / default), and the materialized wrappers run
# the Dvandva gate then exec the prior chain — so foreign hooks keep firing with
# ZERO tracked diff (only a local .git/config value changes, reversible on
# uninstall).
#
# Usage:
#   install-dvandva-hooks.sh [<repo-root>]              — install / refresh
#   install-dvandva-hooks.sh [<repo-root>] --uninstall  — restore prior owner
#   (--force is accepted but ignored: coexistence never needs to clobber.)
#
# Idempotent: re-running refreshes the materialized scripts and re-probes
# without re-recording the prior (no self-wrap).  NEVER modifies global config.
set -u

UNINSTALL=0
REPO_ARG=""
SENTINEL_DEFAULT="__DVANDVA_DEFAULT__"
PENDING_ROOT_BASELINE="__DVANDVA_ROOT_PENDING__"
HOOK_REL=".dvandva/githooks"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" 2>/dev/null && pwd)"

# Canonical client-side git hook names enumerated for pass-through stubs.
GIT_HOOK_NAMES=(
  applypatch-msg pre-applypatch post-applypatch
  pre-commit pre-merge-commit prepare-commit-msg commit-msg post-commit
  pre-rebase post-checkout post-merge pre-push
  pre-receive update proc-receive post-receive post-update
  reference-transaction push-to-checkout pre-auto-gc post-rewrite
  sendemail-validate fsmonitor-watchman post-index-change
)

for arg in "$@"; do
  case "$arg" in
    --uninstall) UNINSTALL=1 ;;
    --force)     : ;;  # deprecated no-op: coexistence never clobbers
    -*)
      echo "install-dvandva-hooks: unknown option: $arg" >&2
      echo "Usage: install-dvandva-hooks.sh [<repo-root>] [--uninstall]" >&2
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

HOOK_DIR_ABS="$REPO_ROOT/$HOOK_REL"

# ---------------------------------------------------------------------------
# Per-worktree config scoping (fixes the linked-worktree fail-open bypass).
#
# core.hooksPath written with `git config --local` lives in the SHARED
# .git/config, so adopting in one worktree would point EVERY worktree at the
# relative .dvandva/githooks while only the adopting worktree materializes that
# dir.  A sibling worktree is then left aiming at a missing dir and git silently
# runs NO hooks (neither the Dvandva gate nor the prior chain).  We instead keep
# the hook state at --worktree scope (extensions.worktreeConfig) so each
# worktree is self-contained: a worktree we never adopted keeps its own
# default/prior hooks.  Reads fall back to --local so a pre-adoption foreign
# hooksPath (Husky's shared core.hooksPath) and legacy --local installs are
# still observed and migrated.  dvandva.hooksAdoptedAt stays at --local (shared)
# because the drift-lint baseline tracks commit history, not worktree identity.
# ---------------------------------------------------------------------------
dv_wt_enabled() {
  [[ "$(git -C "$REPO_ROOT" config --bool extensions.worktreeConfig 2>/dev/null || echo false)" == "true" ]]
}
dv_enable_wt() {
  dv_wt_enabled || git -C "$REPO_ROOT" config extensions.worktreeConfig true
}
dv_cfg_get() {                       # dv_cfg_get <key>  (worktree value, else --local)
  local key="$1" v
  if dv_wt_enabled && v="$(git -C "$REPO_ROOT" config --worktree --get "$key" 2>/dev/null)"; then
    printf '%s\n' "$v"
    return 0
  fi
  git -C "$REPO_ROOT" config --local --get "$key" 2>/dev/null || true
}
dv_cfg_set() {                       # dv_cfg_set <key> <value>  (worktree scope)
  git -C "$REPO_ROOT" config --worktree "$1" "$2"
}
dv_cfg_unset_wt() {
  git -C "$REPO_ROOT" config --worktree --unset "$1" >/dev/null 2>&1 || true
}
dv_cfg_unset_local() {
  git -C "$REPO_ROOT" config --local --unset "$1" >/dev/null 2>&1 || true
}
dv_local_get() {
  git -C "$REPO_ROOT" config --local --get "$1" 2>/dev/null || echo ""
}

# Pin every per-worktree Dvandva key at --worktree scope, migrating then dropping
# any legacy shared --local copies so a sibling worktree never inherits our
# hooksPath via the shared config.  Never clears a FOREIGN --local core.hooksPath
# (that is the recorded prior we restore on uninstall).
pin_state_worktree() {
  dv_enable_wt
  # Migrate a legacy shared prior into worktree scope before cleaning it.
  if ! git -C "$REPO_ROOT" config --worktree --get dvandva.priorHooksPath >/dev/null 2>&1; then
    local local_prior
    local_prior="$(dv_local_get dvandva.priorHooksPath)"
    [[ -n "$local_prior" ]] && dv_cfg_set dvandva.priorHooksPath "$local_prior"
  fi
  dv_cfg_set core.hooksPath "$HOOK_REL"
  dv_cfg_set dvandva.hooksAdopted true
  dv_cfg_set dvandva.hookDir "$HOOK_REL"
  # Drop legacy shared copies (only our own hooksPath value, never a foreign one).
  [[ "$(dv_local_get core.hooksPath)" == "$HOOK_REL" ]] && dv_cfg_unset_local core.hooksPath
  dv_cfg_unset_local dvandva.hooksAdopted
  dv_cfg_unset_local dvandva.hookDir
  dv_cfg_unset_local dvandva.priorHooksPath
}

# ---------------------------------------------------------------------------
# Adoption baseline (unchanged semantics; drift-lint depends on these keys).
# ---------------------------------------------------------------------------
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
# Restore the pre-adoption state (shared by --uninstall and rollback).
# ---------------------------------------------------------------------------
restore_prior_state() {
  local prior
  prior="$(dv_cfg_get dvandva.priorHooksPath)"
  # Drop our per-worktree hooksPath override first.
  dv_cfg_unset_wt core.hooksPath
  # Remove any shared --local hooksPath that points at OUR dir (legacy install or
  # stray double-set) so sibling worktrees never inherit a dangling path.  A
  # FOREIGN --local value is the recorded prior and is left intact.
  [[ "$(dv_local_get core.hooksPath)" == "$HOOK_REL" ]] && dv_cfg_unset_local core.hooksPath
  if [[ -n "$prior" && "$prior" != "$SENTINEL_DEFAULT" ]]; then
    # Re-pin the prior per-worktree only when --local does not already provide it
    # (it usually does: a foreign owner like Husky records it in shared config).
    if [[ "$(dv_local_get core.hooksPath)" != "$prior" ]]; then
      if dv_wt_enabled; then
        dv_cfg_set core.hooksPath "$prior"
      else
        git -C "$REPO_ROOT" config --local core.hooksPath "$prior"
      fi
    fi
  fi
  rm -rf "$HOOK_DIR_ABS"
  local k
  for k in priorHooksPath hooksAdopted hookDir; do
    dv_cfg_unset_wt "dvandva.$k"
    dv_cfg_unset_local "dvandva.$k"
  done
  for k in hooksAdoptedAt hooksAdoptedAtInclusive; do
    dv_cfg_unset_local "dvandva.$k"
  done
}

# ---------------------------------------------------------------------------
# --uninstall: restore the recorded prior owner (or default), drop our state.
# ---------------------------------------------------------------------------
if [[ "$UNINSTALL" -eq 1 ]]; then
  prior_seen="$(dv_cfg_get dvandva.priorHooksPath)"
  current_seen="$(dv_cfg_get core.hooksPath)"
  if [[ -z "$prior_seen" && "$current_seen" != "$HOOK_REL" && ! -d "$HOOK_DIR_ABS" ]]; then
    echo "install-dvandva-hooks: nothing to uninstall (no Dvandva hook adoption found)."
    exit 0
  fi
  restore_prior_state
  restored="$(git -C "$REPO_ROOT" config --local core.hooksPath 2>/dev/null || echo "(default/unset)")"
  echo "install-dvandva-hooks: uninstalled; core.hooksPath restored to: $restored"
  exit 0
fi

# ---------------------------------------------------------------------------
# Locate the plugin source tree (hooks/ + scripts/) to materialize from.
# Anchored on the installer's own location so the dogfood root copy and the
# plugin copy both find plugins/dvandva/{hooks,scripts}.
# ---------------------------------------------------------------------------
find_source_root() {
  local c
  for c in \
    "$(dirname "$SCRIPT_DIR")" \
    "$SCRIPT_DIR/../plugins/dvandva" \
    "$SCRIPT_DIR/../../plugins/dvandva"; do
    if [[ -f "$c/hooks/pre-commit" && -f "$c/hooks/dvandva-hook-lib.sh" \
       && -f "$c/scripts/dvandva-commit-gate.sh" && -f "$c/scripts/dvandva-drift-lint.sh" ]]; then
      (cd "$c" 2>/dev/null && pwd)
      return 0
    fi
  done
  return 1
}

if ! SRC_ROOT="$(find_source_root)"; then
  echo "install-dvandva-hooks: cannot locate plugin hook sources (hooks/ + scripts/)." >&2
  exit 1
fi

materialize_file() {
  local src="$1" dst="$2"
  cp "$src" "$dst" || return 1
  chmod 0755 "$dst" || return 1
}

# ---------------------------------------------------------------------------
# Resolve the prior hooks directory (mirrors dvandva-hook-lib resolve_prior_hook)
# for stub enumeration.
# ---------------------------------------------------------------------------
installer_prior_dir() {
  local prior dir
  prior="$(dv_cfg_get dvandva.priorHooksPath)"
  if [[ -z "$prior" || "$prior" == "$SENTINEL_DEFAULT" ]]; then
    dir="$(git -C "$REPO_ROOT" rev-parse --git-common-dir 2>/dev/null || echo "")/hooks"
  elif [[ "$prior" == /* ]]; then
    dir="$prior"
  else
    dir="$REPO_ROOT/$prior"
  fi
  case "$dir" in
    /*) ;;
    *)  dir="$REPO_ROOT/$dir" ;;
  esac
  printf '%s\n' "$dir"
}

write_stub() {
  local path="$1"
  cat > "$path" <<'STUB'
#!/usr/bin/env bash
# Dvandva pass-through stub: delegates a foreign git hook to the prior chain so
# non-Dvandva hooks (commitlint, pre-push, ...) keep firing after adoption.
set -u
DVANDVA_HOOK_SELF="$0"
HOOK_DIR="$(cd "$(dirname "$0")" 2>/dev/null && pwd)"
DVANDVA_REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "")"
export DVANDVA_HOOK_SELF DVANDVA_REPO_ROOT
[ -f "$HOOK_DIR/dvandva-hook-lib.sh" ] || exit 0
# shellcheck source=/dev/null
. "$HOOK_DIR/dvandva-hook-lib.sh"
if _prior="$(resolve_prior_hook "$(basename "$0")")"; then
  exec "$_prior" "$@"
fi
exit 0
STUB
  chmod 0755 "$path"
}

materialize_stubs() {
  local prior_dir name rp_prior rp_dir
  prior_dir="$(installer_prior_dir)"
  [[ -d "$prior_dir" ]] || return 0
  rp_prior="$(realpath "$prior_dir" 2>/dev/null || printf '%s' "$prior_dir")"
  rp_dir="$(realpath "$HOOK_DIR_ABS" 2>/dev/null || printf '%s' "$HOOK_DIR_ABS")"
  [[ "$rp_prior" != "$rp_dir" ]] || return 0   # never stub against ourselves
  for name in "${GIT_HOOK_NAMES[@]}"; do
    case "$name" in
      pre-commit|prepare-commit-msg) continue ;;   # owned by our wrappers
    esac
    if [[ -x "$prior_dir/$name" ]]; then
      write_stub "$HOOK_DIR_ABS/$name"
    fi
  done
}

# ---------------------------------------------------------------------------
# Functional probe: exec the active wrappers with DVANDVA_HOOK_SELFCHECK=1 and
# assert the positive wiring markers (path-string independent).
# ---------------------------------------------------------------------------
functional_probe() {
  local out
  out="$(cd "$REPO_ROOT" && DVANDVA_HOOK_SELFCHECK=1 "$HOOK_DIR_ABS/pre-commit" 2>/dev/null)" || true
  [[ "$out" == *"DVANDVA_GATE_WIRED"* ]] || return 1
  out="$(cd "$REPO_ROOT" && DVANDVA_HOOK_SELFCHECK=1 "$HOOK_DIR_ABS/prepare-commit-msg" 2>/dev/null)" || true
  [[ "$out" == *"DVANDVA_PREPARE_WIRED"* ]] || return 1
  return 0
}

# ---------------------------------------------------------------------------
# Materialize / refresh the hook dir (always — keeps re-installs current).
# ---------------------------------------------------------------------------
mkdir -p "$HOOK_DIR_ABS"
materialize_file "$SRC_ROOT/hooks/dvandva-hook-lib.sh"     "$HOOK_DIR_ABS/dvandva-hook-lib.sh"
materialize_file "$SRC_ROOT/hooks/pre-commit"              "$HOOK_DIR_ABS/pre-commit"
materialize_file "$SRC_ROOT/hooks/prepare-commit-msg"      "$HOOK_DIR_ABS/prepare-commit-msg"
materialize_file "$SRC_ROOT/scripts/dvandva-commit-gate.sh" "$HOOK_DIR_ABS/dvandva-commit-gate.sh"
materialize_file "$SRC_ROOT/scripts/dvandva-drift-lint.sh"  "$HOOK_DIR_ABS/dvandva-drift-lint.sh"

adopted="$(dv_cfg_get dvandva.hooksAdopted)"
current="$(dv_cfg_get core.hooksPath)"
recorded_prior="$(dv_cfg_get dvandva.priorHooksPath)"

# ---------------------------------------------------------------------------
# Double-wrap guard: already adopted + pointing at our dir -> refresh + reprobe.
# Never re-record the prior (would self-loop).  pin_state_worktree also migrates
# any legacy shared --local install to the per-worktree scope.
# ---------------------------------------------------------------------------
if [[ "$adopted" == "true" && "$current" == "$HOOK_REL" ]]; then
  pin_state_worktree
  materialize_stubs
  record_hook_adoption_baseline
  if functional_probe; then
    recorded_prior="$(dv_cfg_get dvandva.priorHooksPath)"
    echo "install-dvandva-hooks: already adopted; refreshed scripts + stubs and re-probed (prior=$recorded_prior)."
    exit 0
  fi
  echo "install-dvandva-hooks: error: functional probe failed after refresh in $REPO_ROOT." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Fresh adoption (or re-pointing from a foreign owner).
# Record the prior owner exactly once; never record our own dir as the prior.
# State is written at --worktree scope so a sibling worktree is never left with
# core.hooksPath aimed at a .dvandva/githooks dir it lacks (silent fail-open).
# ---------------------------------------------------------------------------
if [[ -z "$recorded_prior" ]]; then
  dv_enable_wt
  if [[ -z "$current" || "$current" == "$HOOK_REL" ]]; then
    dv_cfg_set dvandva.priorHooksPath "$SENTINEL_DEFAULT"
  else
    dv_cfg_set dvandva.priorHooksPath "$current"
  fi
fi

pin_state_worktree
record_hook_adoption_baseline
materialize_stubs

if ! functional_probe; then
  echo "install-dvandva-hooks: error: functional probe failed; rolling back in $REPO_ROOT." >&2
  restore_prior_state
  exit 1
fi

recorded_prior="$(dv_cfg_get dvandva.priorHooksPath)"
echo "install-dvandva-hooks: adopted core.hooksPath=$HOOK_REL (prior=$recorded_prior) in $REPO_ROOT"
exit 0
