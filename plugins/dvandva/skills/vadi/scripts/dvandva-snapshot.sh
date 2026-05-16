#!/usr/bin/env bash
# Snapshot a Dvandva baton checkpoint to .dvandva/history/ and (on terminal status)
# to a named archive at the baton's parent dir.
#
# This helper is bundled as a real executable inside each runtime skill:
#   plugins/dvandva/skills/vadi/scripts/dvandva-snapshot.sh
#   plugins/dvandva/skills/prativadi/scripts/dvandva-snapshot.sh
# The two copies must stay byte-identical so copy-installs and plugin installs
# keep the helper findable via ${CLAUDE_SKILL_DIR}/scripts/dvandva-snapshot.sh.
# scripts/test-dvandva-snapshot.sh fails if either runtime copy is missing or drifts.
#
# Usage: dvandva-snapshot.sh <path-to-baton.json>
# Exit codes:
#   0  snapshot written (or skipped on no-op)
#   2  usage error
#   21 baton file missing
#   22 baton JSON invalid
set -u

if [[ $# -ne 1 ]]; then
  echo "Usage: dvandva-snapshot.sh <path-to-baton.json>" >&2
  exit 2
fi

BATON_FILE="$1"

if [[ ! -f "$BATON_FILE" ]]; then
  echo "DVANDVA_SNAPSHOT missing file=$BATON_FILE" >&2
  exit 21
fi

if ! fields="$(jq -r '[(.checkpoint // 0 | tostring), .status // "", .assignee // "", .branch // "unknown"] | @tsv' "$BATON_FILE" 2>/dev/null)"; then
  echo "DVANDVA_SNAPSHOT invalid_json file=$BATON_FILE" >&2
  exit 22
fi

IFS=$'\t' read -r checkpoint status assignee branch <<< "$fields"

# Sanitize branch for filesystem use: branches like "feature/foo" would
# otherwise create subpaths or copy failures in the terminal archive name.
# Replace '/' with '-' to keep the archive at the .dvandva/ root.
sanitized_branch="${branch//\//-}"

PARENT_DIR="$(dirname "$BATON_FILE")"
HISTORY_DIR="$PARENT_DIR/history"
mkdir -p "$HISTORY_DIR"

HISTORY_TARGET="$HISTORY_DIR/${checkpoint}-${status}-${assignee}.json"

write_with_no_clobber() {
  local target="$1"
  if [[ -f "$target" ]]; then
    if cmp -s "$BATON_FILE" "$target"; then
      return 0
    fi
    local ts
    ts="$(date +%s%N)"
    local dup="${target%.json}.dup-${ts}.json"
    cp "$BATON_FILE" "$dup"
    echo "DVANDVA_SNAPSHOT no_clobber wrote=$dup" >&2
    return 0
  fi
  cp "$BATON_FILE" "$target"
}

write_with_no_clobber "$HISTORY_TARGET"

case "$status" in
  done|human_decision|human_question)
    ARCHIVE_TARGET="$PARENT_DIR/baton.${sanitized_branch}-${checkpoint}-${status}.json"
    write_with_no_clobber "$ARCHIVE_TARGET"
    ;;
esac

exit 0
