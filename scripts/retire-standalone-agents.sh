#!/usr/bin/env bash
# retire-standalone-agents.sh -- Reversibly retire the 5 standalone Claude user agents
# now superseded by the dvandva-* roster.
#
# VALIDATION RATIONALE
# The dvandva-* replacements are proven equivalent-or-better by EMPIRICAL usage across
# Dvandva Runs 1-4, not merely by file presence.  They executed as primary agents in
# real reviewed implementation/review work across all four runs.  Mapping:
#
#   architect          → dvandva-architect
#   developer          → dvandva-implementer
#   quality-reviewer   → dvandva-deep-reviewer + dvandva-cross-reviewer
#   adversarial-analyst→ dvandva-adversarial-analyst
#   sandbox-executor   → dvandva-sandbox-verifier
#
# SAFETY INVARIANTS
#   1. DEFAULT = dry-run.  --apply must be passed explicitly.
#   2. ALLOWLIST ONLY: exactly the 5 named symlinks are ever touched.
#      No other files, skills, or non-allowlisted agents are modified.
#   3. PARITY GATE: --apply first verifies that the installed dvandva cache
#      ($HOME/.claude/plugins/cache/dvandva/dvandva/<version>/agents/) contains
#      all 15 dvandva-* agent files at the expected version.  If parity fails,
#      --apply is refused and exits nonzero.
#   4. REVERSIBLE: --apply moves the 5 symlink pointers into a timestamped
#      .retired-<ts>/ dir and writes a manifest.json.  --restore <backup-dir>
#      reads the manifest to undo.  The Haoshoku source targets are never touched.
#   5. Codex side: inspect $CODEX_HOME/{agents,prompts,subagents}; report no-op.
#      Never retire anything from Codex dirs.
#
# USAGE
#   bash scripts/retire-standalone-agents.sh [--dry-run|--apply|--restore <dir>]
set -euo pipefail

# ---------------------------------------------------------------------------
# Environment (honour overrides from tests / CI)
# ---------------------------------------------------------------------------
# HOME is already in the environment; CODEX_HOME defaults to $HOME/.codex
CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
DVANDVA_EXPECTED_VERSION="${DVANDVA_EXPECTED_VERSION:-1.2.0}"

CLAUDE_AGENTS_DIR="$HOME/.claude/agents"
DVANDVA_CACHE_BASE="$HOME/.claude/plugins/cache/dvandva/dvandva"

# ---------------------------------------------------------------------------
# ALLOWLIST: exactly the 5 standalone agents eligible for retirement
# ---------------------------------------------------------------------------
ALLOWLIST=(
  adversarial-analyst.md
  architect.md
  developer.md
  quality-reviewer.md
  sandbox-executor.md
)

# ---------------------------------------------------------------------------
# PARITY CHECK: all 15 dvandva-* agent files that must be in the cache
# ---------------------------------------------------------------------------
DVANDVA_REQUIRED_AGENTS=(
  adversarial-analyst.md
  architect.md
  baton-auditor.md
  cross-reviewer.md
  debugger.md
  deep-reviewer.md
  deslopper.md
  doc-verifier.md
  implementer.md
  integration-checker.md
  pattern-mapper.md
  researcher.md
  sandbox-verifier.md
  security-auditor.md
  test-creator.md
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage: bash scripts/retire-standalone-agents.sh [--dry-run|--apply|--restore <backup-dir>]

Reversibly retire the 5 standalone Claude user agents now superseded by the
dvandva-* roster (bundled in the dvandva plugin).

Modes:
  (no flags)          Dry-run: print what WOULD be retired; touch nothing. (default)
  --dry-run           Same as above (explicit).
  --apply             Execute retirement after the parity gate passes.
  --restore <dir>     Reverse a prior --apply run using its manifest.json.
  -h, --help          Show this help.

Safety:
  • Only the 5 allowlisted symlinks are ever moved.
  • --apply refuses unless the dvandva cache at DVANDVA_EXPECTED_VERSION
    (default: 1.2.0) contains all 15 required dvandva-* agent files.
  • Haoshoku source targets are never touched; only the symlink pointers move.
  • Skills, non-allowlisted agents, and Codex dirs are never modified.

Environment:
  HOME                       Overridable home dir (used in tests).
  CODEX_HOME                 Codex home dir (default: \$HOME/.codex).
  DVANDVA_EXPECTED_VERSION   Required dvandva cache version (default: 1.2.0).
EOF
}

require_jq() {
  command -v jq >/dev/null 2>&1 || die "jq is required for manifest JSON handling"
}

allowlisted_agent() {
  local candidate="$1"
  local agent
  for agent in "${ALLOWLIST[@]}"; do
    [[ "$candidate" == "$agent" ]] && return 0
  done
  return 1
}

# ---------------------------------------------------------------------------
# Parity gate: refuse --apply unless the dvandva cache is complete
# ---------------------------------------------------------------------------
parity_gate() {
  local cache_agents="$DVANDVA_CACHE_BASE/$DVANDVA_EXPECTED_VERSION/agents"

  if [[ ! -d "$cache_agents" ]]; then
    printf 'PARITY FAIL: dvandva %s cache not found.\n' "$DVANDVA_EXPECTED_VERSION" >&2
    printf '  Expected directory: %s\n' "$cache_agents" >&2
    printf '  Install dvandva %s first: bash scripts/install.sh\n' \
      "$DVANDVA_EXPECTED_VERSION" >&2
    exit 1
  fi

  local missing=()
  for agent in "${DVANDVA_REQUIRED_AGENTS[@]}"; do
    [[ -f "$cache_agents/$agent" ]] || missing+=("$agent")
  done

  if [[ "${#missing[@]}" -gt 0 ]]; then
    printf 'PARITY FAIL: dvandva %s cache is incomplete.\n' "$DVANDVA_EXPECTED_VERSION" >&2
    printf '  Missing %d agent(s):\n' "${#missing[@]}" >&2
    for f in "${missing[@]}"; do
      printf '    %s\n' "$f" >&2
    done
    printf '  Reinstall dvandva %s: bash scripts/install.sh\n' \
      "$DVANDVA_EXPECTED_VERSION" >&2
    exit 1
  fi

  printf 'Parity OK: dvandva %s cache has all %d required agent files.\n' \
    "$DVANDVA_EXPECTED_VERSION" "${#DVANDVA_REQUIRED_AGENTS[@]}"
}

# ---------------------------------------------------------------------------
# Codex side-report: always a no-op, never retires from Codex dirs
# ---------------------------------------------------------------------------
codex_check() {
  local found=0
  for subdir in agents prompts subagents; do
    local dir="$CODEX_HOME/$subdir"
    if [[ -d "$dir" ]] && [[ -n "$(ls -A "$dir" 2>/dev/null || true)" ]]; then
      found=1
      break
    fi
  done

  if [[ "$found" -eq 0 ]]; then
    printf 'Codex (%s): no agent-axis files to retire (no-op).\n' "$CODEX_HOME"
  else
    printf 'Codex (%s): agent-axis files found but outside retirement allowlist (no-op).\n' \
      "$CODEX_HOME"
  fi
}

# ---------------------------------------------------------------------------
# Dry-run: show what WOULD happen, touch nothing
# ---------------------------------------------------------------------------
cmd_dry_run() {
  printf '=== Dvandva Standalone Agent Retirement (DRY RUN) ===\n'
  printf 'Allowlisted agents directory: %s\n\n' "$CLAUDE_AGENTS_DIR"

  local found=0
  for agent in "${ALLOWLIST[@]}"; do
    local src="$CLAUDE_AGENTS_DIR/$agent"
    if [[ -L "$src" ]]; then
      local target
      target="$(readlink "$src")"
      printf '  WOULD RETIRE: %s -> %s\n' "$agent" "$target"
      found=$((found + 1))
    elif [[ -e "$src" ]]; then
      printf '  SKIP (not a symlink): %s\n' "$agent"
    else
      printf '  SKIP (not present): %s\n' "$agent"
    fi
  done

  printf '\n'
  if [[ "$found" -eq 0 ]]; then
    printf 'Nothing to retire.\n'
  else
    printf '%d symlink(s) would be moved to: %s/.retired-<timestamp>/\n' \
      "$found" "$CLAUDE_AGENTS_DIR"
    printf 'Run with --apply to execute (requires parity gate to pass).\n'
  fi

  printf '\n'
  codex_check
}

# ---------------------------------------------------------------------------
# Apply: parity gate → move 5 symlinks → write manifest
# ---------------------------------------------------------------------------
cmd_apply() {
  printf '=== Dvandva Standalone Agent Retirement (APPLY) ===\n\n'

  require_jq
  parity_gate
  printf '\n'

  local ts
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  local backup_dir="$CLAUDE_AGENTS_DIR/.retired-$ts"
  mkdir -p "$backup_dir"

  local retired_originals=()
  local retired_backups=()
  local retired_targets=()
  local retired=0

  for agent in "${ALLOWLIST[@]}"; do
    local src="$CLAUDE_AGENTS_DIR/$agent"
    if [[ ! -L "$src" ]]; then
      if [[ -e "$src" ]]; then
        printf '  SKIP (not a symlink): %s\n' "$agent"
      else
        printf '  SKIP (not present): %s\n' "$agent"
      fi
      continue
    fi

    local target
    target="$(readlink "$src")"
    local dst="$backup_dir/$agent"

    mv "$src" "$dst"

    retired_originals+=("$src")
    retired_backups+=("$dst")
    retired_targets+=("$target")

    printf '  RETIRED: %s -> %s\n' "$agent" "$target"
    retired=$((retired + 1))
  done

  # Write manifest.json with a real JSON writer so paths with quotes or
  # backslashes round-trip through restore.
  local manifest_file="$backup_dir/manifest.json"
  local entries_file="$backup_dir/.manifest.entries.json"
  printf '[]\n' > "$entries_file"
  local n="${#retired_originals[@]}"
  local i
  for (( i = 0; i < n; i++ )); do
    local next_entries="$entries_file.tmp"
    jq \
      --arg original_path "${retired_originals[$i]}" \
      --arg backup_path "${retired_backups[$i]}" \
      --arg symlink_target "${retired_targets[$i]}" \
      '. + [{
        original_path: $original_path,
        backup_path: $backup_path,
        symlink_target: $symlink_target
      }]' "$entries_file" > "$next_entries"
    mv "$next_entries" "$entries_file"
  done

  jq -n \
    --arg retired_at "$ts" \
    --arg dvandva_version "$DVANDVA_EXPECTED_VERSION" \
    --arg backup_dir "$backup_dir" \
    --slurpfile entries "$entries_file" \
    '{
      retired_at: $retired_at,
      dvandva_version: $dvandva_version,
      backup_dir: $backup_dir,
      entries: $entries[0]
    }' > "$manifest_file"
  rm -f "$entries_file" "$entries_file.tmp"

  printf '\n%d agent(s) retired to: %s\n' "$retired" "$backup_dir"
  printf 'Manifest: %s\n' "$manifest_file"
  printf '\nTo restore: bash scripts/retire-standalone-agents.sh --restore '"'"'%s'"'"'\n' "$backup_dir"

  printf '\n'
  codex_check
}

# ---------------------------------------------------------------------------
# Restore: read manifest → move symlinks back to original locations
# ---------------------------------------------------------------------------
cmd_restore() {
  local restore_dir="$1"
  local manifest_file="$restore_dir/manifest.json"

  if [[ ! -f "$manifest_file" ]]; then
    die "Manifest not found: $manifest_file"
  fi

  require_jq
  if ! jq empty "$manifest_file" >/dev/null 2>&1; then
    die "Manifest is not valid JSON: $manifest_file"
  fi

  if ! jq -e '
    (.backup_dir | type) == "string" and
    (.entries | type) == "array" and
    all(.entries[];
      (.original_path | type) == "string" and
      (.backup_path | type) == "string" and
      (.symlink_target | type) == "string"
    )
  ' "$manifest_file" >/dev/null 2>&1; then
    die "Manifest is not valid Dvandva retirement JSON: $manifest_file"
  fi

  local manifest_backup_dir
  manifest_backup_dir="$(jq -r '.backup_dir' "$manifest_file")"
  if [[ "$manifest_backup_dir" != "$restore_dir" ]]; then
    die "Invalid manifest entry: backup_dir does not match restore dir: $manifest_backup_dir"
  fi

  printf '=== Dvandva Standalone Agent Retirement (RESTORE) ===\n'
  printf 'Reading manifest: %s\n\n' "$manifest_file"
  printf 'Allowlist validation: restore will only move Dvandva retirement entries for the 5 approved standalone agent symlinks.\n\n'

  local restored=0
  local attempted=0
  local missing_backup=0
  local entry_json orig backup agent expected_orig expected_backup

  # Validate every entry before moving anything.  A crafted or corrupted
  # manifest must fail as a whole, not partially restore unsafe paths.
  while IFS= read -r entry_json; do
    orig="$(printf '%s\n' "$entry_json" | jq -r '.original_path')"
    backup="$(printf '%s\n' "$entry_json" | jq -r '.backup_path')"
    agent="${orig##*/}"

    if ! allowlisted_agent "$agent"; then
      die "Invalid manifest entry: non-allowlisted agent: $orig"
    fi

    expected_orig="$CLAUDE_AGENTS_DIR/$agent"
    if [[ "$orig" != "$expected_orig" ]]; then
      die "Invalid manifest entry: original_path outside allowlist: $orig"
    fi

    expected_backup="$restore_dir/$agent"
    if [[ "$backup" != "$expected_backup" ]]; then
      die "Invalid manifest entry: backup_path outside restore dir: $backup"
    fi

    if [[ -e "$backup" || -L "$backup" ]]; then
      if [[ ! -L "$backup" ]]; then
        die "Invalid manifest entry: backup_path is not a symlink: $backup"
      fi
    else
      missing_backup=1
    fi

    attempted=$((attempted + 1))
  done < <(jq -c '.entries[]' "$manifest_file")

  if [[ "$missing_backup" -eq 1 ]]; then
    printf 'ERROR: no agents restored; backup appears already restored or incomplete.\n' >&2
    return 1
  fi

  while IFS= read -r entry_json; do
    orig="$(printf '%s\n' "$entry_json" | jq -r '.original_path')"
    backup="$(printf '%s\n' "$entry_json" | jq -r '.backup_path')"

    if [[ -e "$orig" || -L "$orig" ]]; then
      printf '  WARNING: original path already occupied, skipping: %s\n' "$orig" >&2
      continue
    fi

    mv "$backup" "$orig"
    printf '  RESTORED: %s\n' "$orig"
    restored=$((restored + 1))
  done < <(jq -c '.entries[]' "$manifest_file")

  printf '\n%d agent(s) restored.\n' "$restored"
  if [[ "$attempted" -gt 0 && "$restored" -eq 0 ]]; then
    printf 'ERROR: no agents restored; backup appears already restored or incomplete.\n' >&2
    return 1
  fi
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
MODE="dry-run"
RESTORE_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      MODE="dry-run"
      shift
      ;;
    --apply)
      MODE="apply"
      shift
      ;;
    --restore)
      [[ $# -ge 2 ]] || die "--restore requires a backup directory argument"
      MODE="restore"
      RESTORE_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'ERROR: unknown option: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------
case "$MODE" in
  dry-run)
    cmd_dry_run
    ;;
  apply)
    cmd_apply
    ;;
  restore)
    [[ -n "$RESTORE_DIR" ]] || die "--restore requires a backup directory"
    cmd_restore "$RESTORE_DIR"
    ;;
esac
