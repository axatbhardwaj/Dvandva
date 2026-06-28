#!/usr/bin/env bash
# One-shot Dvandva install for Claude Code and Codex.
#
# Usage:
#   bash scripts/install.sh [--claude-only|--codex-only] [<marketplace-path-or-repo>]
#
# Default marketplace: axatbhardwaj/Dvandva (the upstream repo).
# Override with a local path for development:
#   bash scripts/install.sh /path/to/local/Dvandva
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MARKETPLACE="axatbhardwaj/Dvandva"
INSTALL_CLAUDE=1
INSTALL_CODEX=1

usage() {
  cat <<'EOF'
Usage: bash scripts/install.sh [--claude-only|--codex-only] [<marketplace-path-or-repo>]

Installs the dvandva@dvandva plugin into Claude Code and Codex by default.

Options:
  --claude-only   Install only the Claude Code plugin.
  --codex-only    Install only the Codex plugin.
  -h, --help      Show this help.

Default marketplace: axatbhardwaj/Dvandva
EOF
}

run_idempotent() {
  local label="$1"
  local output status
  shift

  if output="$("$@" 2>&1)"; then
    [[ -z "$output" ]] || printf '%s\n' "$output"
    return 0
  fi

  status=$?
  [[ -z "$output" ]] || printf '%s\n' "$output" >&2
  if printf '%s\n' "$output" | grep -Eiq 'already|exists|registered|installed|duplicate'; then
    echo "$label already present; continuing."
    return 0
  fi

  return "$status"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --claude-only)
      INSTALL_CODEX=0
      shift
      ;;
    --codex-only)
      INSTALL_CLAUDE=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      echo "ERROR: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      MARKETPLACE="$1"
      shift
      if [[ $# -gt 0 ]]; then
        echo "ERROR: expected at most one marketplace argument" >&2
        usage >&2
        exit 2
      fi
      ;;
  esac
done

if [[ "$INSTALL_CLAUDE" -eq 0 && "$INSTALL_CODEX" -eq 0 ]]; then
  echo "ERROR: --claude-only and --codex-only cannot be combined" >&2
  usage >&2
  exit 2
fi

if [[ "$INSTALL_CLAUDE" -eq 1 ]]; then
  if ! command -v claude >/dev/null 2>&1; then
    echo "ERROR: claude CLI not found on PATH" >&2
    exit 1
  fi

  echo "Claude Code: registering marketplace '$MARKETPLACE'..."
  run_idempotent "Claude Code marketplace" claude plugin marketplace add "$MARKETPLACE"
  echo "Claude Code: installing dvandva plugin..."
  run_idempotent "Claude Code plugin" claude plugin install dvandva@dvandva
  echo "Claude Code install complete"
fi

if [[ "$INSTALL_CODEX" -eq 1 ]]; then
  if ! command -v codex >/dev/null 2>&1; then
    echo "ERROR: codex CLI not found on PATH" >&2
    exit 1
  fi

  echo "Codex: installing dvandva plugin..."
  bash "$SCRIPT_DIR/install-codex.sh" "$MARKETPLACE"
  echo "Codex install complete"
fi

echo "Done. Verify the installed engine(s) can see dvandva:vadi, dvandva:prativadi, dvandva:testing, dvandva:understanding, and dvandva:worktree-setup in /skills."
