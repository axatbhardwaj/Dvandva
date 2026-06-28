#!/usr/bin/env bash
# dvandva-drift-lint.sh
# Detects off-protocol Git commits: commits made while a Dvandva run was
# active but without the Dvandva-Checkpoint trailer.
#
# Algorithm:
#   1. Walk the git log to find the most recent commit that contains a
#      "Dvandva-Checkpoint: <N>" trailer.
#   2. List commits between that commit (exclusive) and HEAD.
#   3. For each of those commits, check whether the trailer is present.
#   4. Any commit without the trailer is "drift" — off-protocol work.
#
# A repo with no checkpointed commits at all exits 0 with an informational
# message (pre-run or non-Dvandva history is not drift).
#
# Usage:
#   dvandva-drift-lint.sh            — exit 1 if drift found (for CI)
#   dvandva-drift-lint.sh --warn     — advisory only, always exit 0
set -u

WARN_ONLY=0
for arg in "$@"; do
  case "$arg" in
    --warn) WARN_ONLY=1 ;;
    *)
      echo "dvandva-drift-lint: unknown option: $arg" >&2
      echo "Usage: dvandva-drift-lint.sh [--warn]" >&2
      exit 2
      ;;
  esac
done

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  echo "dvandva-drift-lint: not inside a git repository" >&2
  exit 1
}

# Terminal statuses are inactive for drift purposes.
is_terminal() {
  case "$1" in
    done|human_question|human_decision) return 0 ;;
    *) return 1 ;;
  esac
}

active_baton_exists() {
  local baton_paths=()

  if [[ -f "$REPO_ROOT/.dvandva/baton.json" ]]; then
    baton_paths+=("$REPO_ROOT/.dvandva/baton.json")
  fi

  if [[ -d "$REPO_ROOT/.dvandva/runs" ]]; then
    while IFS= read -r -d '' p; do
      baton_paths+=("$p")
    done < <(find "$REPO_ROOT/.dvandva/runs" -maxdepth 2 -name "baton.json" -print0 2>/dev/null)
  fi

  [[ ${#baton_paths[@]} -gt 0 ]] || return 1

  if ! command -v jq >/dev/null 2>&1; then
    echo "DVANDVA_DRIFT warning: jq is required to parse Dvandva baton files." >&2
    return 0
  fi

  local bp status
  for bp in "${baton_paths[@]}"; do
    if ! jq empty "$bp" 2>/dev/null; then
      echo "DVANDVA_DRIFT warning: malformed baton JSON: $bp" >&2
      return 0
    fi
    status="$(jq -r '.status // ""' "$bp")"
    if ! is_terminal "$status"; then
      return 0
    fi
  done

  return 1
}

# ---------------------------------------------------------------------------
# Find the most recent commit with a Dvandva-Checkpoint trailer
# ---------------------------------------------------------------------------
LAST_CHECKPOINT_SHA=""
LAST_CHECKPOINT_NUM=""

while IFS= read -r sha; do
  [[ -z "$sha" ]] && continue
  body="$(git -C "$REPO_ROOT" show -s --format="%B" "$sha" 2>/dev/null)" || continue
  if echo "$body" | grep -qE "^Dvandva-Checkpoint:[[:space:]]+[0-9]+"; then
    LAST_CHECKPOINT_SHA="$sha"
    LAST_CHECKPOINT_NUM="$(echo "$body" | grep -oE "^Dvandva-Checkpoint:[[:space:]]+[0-9]+" | grep -oE "[0-9]+$" | head -1)"
    break
  fi
done < <(git -C "$REPO_ROOT" log --format="%H" 2>/dev/null)

# No checkpointed commits found.  If no baton is active, this is pre-run or
# non-Dvandva history and is not drift.  If a baton is active, however,
# unstamped commits are visible first-run bypasses and must be reported.
if [[ -z "$LAST_CHECKPOINT_SHA" ]]; then
  if active_baton_exists; then
    if ! git -C "$REPO_ROOT" rev-parse --verify HEAD >/dev/null 2>&1; then
      echo "DVANDVA_DRIFT ok: no checkpointed commits in history — nothing to lint."
      exit 0
    fi

    DRIFT_SHAS=()
    while IFS= read -r sha; do
      [[ -z "$sha" ]] && continue
      body="$(git -C "$REPO_ROOT" show -s --format="%B" "$sha" 2>/dev/null)" || continue
      if ! echo "$body" | grep -qE "^Dvandva-Checkpoint:[[:space:]]"; then
        DRIFT_SHAS+=("$sha")
      fi
    done < <(git -C "$REPO_ROOT" log --format="%H" 2>/dev/null)

    if [[ ${#DRIFT_SHAS[@]} -eq 0 ]]; then
      echo "DVANDVA_DRIFT ok: active baton exists but all commits carry Dvandva-Checkpoint trailers."
      exit 0
    fi

    echo "DVANDVA_DRIFT warning: ${#DRIFT_SHAS[@]} off-protocol commit(s) found while an active baton exists and no checkpoint baseline exists" >&2
    for sha in "${DRIFT_SHAS[@]}"; do
      subject="$(git -C "$REPO_ROOT" show -s --format="%s" "$sha" 2>/dev/null || echo "(unreadable)")"
      echo "  $sha  $subject" >&2
    done

    if [[ "$WARN_ONLY" -eq 1 ]]; then
      echo "DVANDVA_DRIFT advisory: off-protocol commits detected — pass --warn suppresses failure." >&2
      exit 0
    fi

    exit 1
  fi

  echo "DVANDVA_DRIFT ok: no checkpointed commits in history — nothing to lint."
  exit 0
fi

# ---------------------------------------------------------------------------
# Collect commits between the last checkpoint and HEAD
# ---------------------------------------------------------------------------
DRIFT_SHAS=()
while IFS= read -r sha; do
  [[ -z "$sha" ]] && continue
  body="$(git -C "$REPO_ROOT" show -s --format="%B" "$sha" 2>/dev/null)" || continue
  if ! echo "$body" | grep -qE "^Dvandva-Checkpoint:[[:space:]]"; then
    DRIFT_SHAS+=("$sha")
  fi
done < <(git -C "$REPO_ROOT" log --format="%H" "${LAST_CHECKPOINT_SHA}..HEAD" 2>/dev/null)

# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------
if [[ ${#DRIFT_SHAS[@]} -eq 0 ]]; then
  echo "DVANDVA_DRIFT ok: no off-protocol commits since checkpoint $LAST_CHECKPOINT_NUM ($LAST_CHECKPOINT_SHA)"
  exit 0
fi

echo "DVANDVA_DRIFT warning: ${#DRIFT_SHAS[@]} off-protocol commit(s) found since checkpoint $LAST_CHECKPOINT_NUM ($LAST_CHECKPOINT_SHA)" >&2
for sha in "${DRIFT_SHAS[@]}"; do
  subject="$(git -C "$REPO_ROOT" show -s --format="%s" "$sha" 2>/dev/null || echo "(unreadable)")"
  echo "  $sha  $subject" >&2
done

if [[ "$WARN_ONLY" -eq 1 ]]; then
  echo "DVANDVA_DRIFT advisory: off-protocol commits detected — pass --warn suppresses failure." >&2
  exit 0
fi

exit 1
