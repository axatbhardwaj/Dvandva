#!/usr/bin/env bash
# Test suite for scripts/retire-standalone-agents.sh
#
# Builds a FAKE HOME with fake symlinks and a fake dvandva cache.
# Never reads or writes ~/.claude, ~/.codex, or any real agent file.
#
# Covered scenarios:
#   (a) dry-run: touches nothing (immutability)
#   (b) apply with parity-pass: moves exactly the 5 allowlisted symlinks, writes a
#       complete manifest, leaves sources and a non-allowlisted decoy untouched
#   (c) restore: returns the 5 symlinks to their original locations
#   (d) apply with stale/missing cache: parity gate refuses, exits nonzero
#   (e) skills dir untouched after apply
#   (f) Codex dirs empty → no-op report; never retires anything from Codex
#   (g) partial pre-existing retirement: apply handles already-absent symlinks
#   (h) double restore guard: a second restore exits nonzero
#   (i) crafted restore manifest cannot restore non-allowlisted agent paths
#   (j) manifest JSON is valid and restore round-trips quoted path components
#   (k) manifest JSON round-trips a HOME path containing a space
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RETIRE_SCRIPT="$ROOT_DIR/scripts/retire-standalone-agents.sh"
FAILURES=0
TMP_DIR=""

cleanup() {
  if [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]]; then
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  FAILURES=$((FAILURES + 1))
}

pass() {
  echo "PASS: $*"
}

# ---------------------------------------------------------------------------
# Constant fixtures
# ---------------------------------------------------------------------------

TMP_PARENT="${TMPDIR:-/tmp}"
TMP_PARENT="${TMP_PARENT%/}"
TMP_DIR="$(mktemp -d "$TMP_PARENT/dvandva-test-retire.XXXXXX")"

EXPECTED_VER="0.4.0"

STANDALONE_AGENTS=(
  adversarial-analyst.md
  architect.md
  developer.md
  quality-reviewer.md
  sandbox-executor.md
)

# All 15 agent files that must be present in the dvandva plugin cache
DVANDVA_AGENTS=(
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

# Haoshoku source dir (fake, lives outside FAKE_HOME so we can verify it is never touched)
FAKE_HAOSHOKU_DIR="$TMP_DIR/haoshoku-sources"
mkdir -p "$FAKE_HAOSHOKU_DIR"
for _a in "${STANDALONE_AGENTS[@]}"; do
  printf '# fake haoshoku source: %s\n' "$_a" > "$FAKE_HAOSHOKU_DIR/$_a"
done
unset _a

# ---------------------------------------------------------------------------
# Helper: build a fake HOME
#   $1 = subdirectory name under TMP_DIR
#   $2 = 1 (default) to include a valid dvandva 0.4.0 cache; 0 to omit
#   $3 = "full" (default) or "partial" to drop 2 agents from the cache
# ---------------------------------------------------------------------------
build_fake_home() {
  local name="$1"
  local include_cache="${2:-1}"
  local cache_completeness="${3:-full}"
  local fake_home="$TMP_DIR/$name"

  # .claude/agents with 5 symlinks to haoshoku sources + 1 non-allowlisted decoy
  mkdir -p "$fake_home/.claude/agents"
  for agent in "${STANDALONE_AGENTS[@]}"; do
    ln -s "$FAKE_HAOSHOKU_DIR/$agent" "$fake_home/.claude/agents/$agent"
  done
  printf '# decoy agent; must not be touched\n' > "$fake_home/.claude/agents/decoy.md"

  # A fake skills dir (must survive apply untouched)
  mkdir -p "$fake_home/.claude/skills"
  printf '# fake skill\n' > "$fake_home/.claude/skills/some-skill.md"

  if [[ "$include_cache" -eq 1 ]]; then
    local cache_agents="$fake_home/.claude/plugins/cache/dvandva/dvandva/$EXPECTED_VER/agents"
    mkdir -p "$cache_agents"
    for agent in "${DVANDVA_AGENTS[@]}"; do
      printf '# fake dvandva agent: %s\n' "$agent" > "$cache_agents/$agent"
    done
    if [[ "$cache_completeness" == "partial" ]]; then
      # Remove 2 agents to simulate an incomplete cache
      rm "$cache_agents/debugger.md" "$cache_agents/pattern-mapper.md"
    fi
  fi

  # Empty Codex dirs (agent-axis locations that exist but hold no files)
  mkdir -p \
    "$fake_home/.codex/agents" \
    "$fake_home/.codex/prompts" \
    "$fake_home/.codex/subagents"

  printf '%s\n' "$fake_home"
}

# ---------------------------------------------------------------------------
# Shared env helper: always override HOME and CODEX_HOME so the script is
# sandboxed regardless of what the outer shell has set.
# ---------------------------------------------------------------------------
run_retire() {
  local fake_home="$1"
  shift
  HOME="$fake_home" \
  CODEX_HOME="$fake_home/.codex" \
  DVANDVA_EXPECTED_VERSION="$EXPECTED_VER" \
    bash "$RETIRE_SCRIPT" "$@"
}

# ---------------------------------------------------------------------------
# (a) Dry-run immutability
# ---------------------------------------------------------------------------
test_dry_run_immutability() {
  echo "--- test (a): dry-run immutability ---"
  local fake_home
  fake_home="$(build_fake_home "home-dryrun")"

  local output rc
  output="$(run_retire "$fake_home" 2>&1)"
  rc=$?

  if [[ "$rc" -eq 0 ]]; then
    pass "[dry-run] exit code 0"
  else
    fail "[dry-run] unexpected nonzero exit: $rc"
  fi

  # No symlink moved
  local moved=0
  for agent in "${STANDALONE_AGENTS[@]}"; do
    if [[ ! -L "$fake_home/.claude/agents/$agent" ]]; then
      fail "[dry-run] $agent symlink missing after dry-run"
      moved=1
    fi
  done
  [[ "$moved" -eq 0 ]] && pass "[dry-run] all 5 allowlisted symlinks untouched"

  # Decoy file untouched
  if [[ -f "$fake_home/.claude/agents/decoy.md" ]]; then
    pass "[dry-run] decoy.md untouched"
  else
    fail "[dry-run] decoy.md missing after dry-run"
  fi

  # Haoshoku sources untouched
  local src_ok=1
  for agent in "${STANDALONE_AGENTS[@]}"; do
    if [[ ! -f "$FAKE_HAOSHOKU_DIR/$agent" ]]; then
      fail "[dry-run] haoshoku source $agent missing after dry-run"
      src_ok=0
    fi
  done
  [[ "$src_ok" -eq 1 ]] && pass "[dry-run] haoshoku sources untouched"

  # No .retired-* backup dir created
  local retired_dirs
  retired_dirs="$(find "$fake_home/.claude/agents" -maxdepth 1 -name '.retired-*' -type d 2>/dev/null || true)"
  if [[ -z "$retired_dirs" ]]; then
    pass "[dry-run] no .retired-* backup dir created"
  else
    fail "[dry-run] unexpected .retired-* backup dir: $retired_dirs"
  fi

  # Output mentions WOULD RETIRE
  if printf '%s\n' "$output" | grep -qi "WOULD RETIRE"; then
    pass "[dry-run] output mentions WOULD RETIRE"
  else
    fail "[dry-run] output does not mention WOULD RETIRE; got: $output"
  fi
}

# ---------------------------------------------------------------------------
# (b) Apply moves exactly 5, writes manifest, leaves decoy + sources intact
# (c) Restore returns symlinks to original locations
# ---------------------------------------------------------------------------
test_apply_and_restore() {
  echo "--- test (b): apply moves 5 + writes manifest ---"
  local fake_home
  fake_home="$(build_fake_home "home-roundtrip")"

  local apply_output apply_rc
  apply_output="$(run_retire "$fake_home" --apply 2>&1)"
  apply_rc=$?

  if [[ "$apply_rc" -eq 0 ]]; then
    pass "[apply] exit code 0"
  else
    fail "[apply] unexpected nonzero exit: $apply_rc; output: $apply_output"
  fi

  # All 5 allowlisted symlinks must be gone from original location
  local moved=0
  for agent in "${STANDALONE_AGENTS[@]}"; do
    if [[ ! -L "$fake_home/.claude/agents/$agent" ]] && [[ ! -e "$fake_home/.claude/agents/$agent" ]]; then
      moved=$((moved + 1))
    else
      fail "[apply] $agent still at original location after apply"
    fi
  done
  [[ "$moved" -eq 5 ]] && pass "[apply] all 5 symlinks moved from original location"

  # Exactly one .retired-* backup dir
  local backup_dir=""
  local backup_dir_count=0
  while IFS= read -r d; do
    backup_dir="$d"
    backup_dir_count=$((backup_dir_count + 1))
  done < <(find "$fake_home/.claude/agents" -maxdepth 1 -name '.retired-*' -type d 2>/dev/null || true)

  if [[ "$backup_dir_count" -eq 1 && -n "$backup_dir" ]]; then
    pass "[apply] exactly 1 .retired-* backup dir created"
  else
    fail "[apply] expected 1 .retired-* backup dir, found $backup_dir_count"
    return
  fi

  # All 5 symlinks present in backup dir, still pointing to haoshoku sources
  local in_backup=0
  for agent in "${STANDALONE_AGENTS[@]}"; do
    if [[ -L "$backup_dir/$agent" ]]; then
      in_backup=$((in_backup + 1))
      local target
      target="$(readlink "$backup_dir/$agent")"
      if [[ "$target" == "$FAKE_HAOSHOKU_DIR/$agent" ]]; then
        pass "[apply] $agent in backup, symlink target preserved"
      else
        fail "[apply] $agent in backup but symlink target wrong: $target"
      fi
    else
      fail "[apply] $agent not found as symlink in backup dir"
    fi
  done
  [[ "$in_backup" -eq 5 ]] && pass "[apply] all 5 symlinks in backup dir"

  # manifest.json exists in backup dir
  if [[ -f "$backup_dir/manifest.json" ]]; then
    pass "[apply] manifest.json exists in backup dir"
  else
    fail "[apply] manifest.json missing from backup dir"
  fi

  # Manifest contains all 5 original paths
  local manifest_content
  manifest_content="$(cat "$backup_dir/manifest.json")"
  local manifest_ok=1
  for agent in "${STANDALONE_AGENTS[@]}"; do
    local orig_path="$fake_home/.claude/agents/$agent"
    if printf '%s\n' "$manifest_content" | grep -qF "$orig_path"; then
      : # found
    else
      fail "[apply] manifest missing original_path for $agent"
      manifest_ok=0
    fi
  done
  [[ "$manifest_ok" -eq 1 ]] && pass "[apply] manifest contains all 5 original paths"

  # Manifest contains all 5 backup paths
  local manifest_backup_ok=1
  for agent in "${STANDALONE_AGENTS[@]}"; do
    local backup_path="$backup_dir/$agent"
    if printf '%s\n' "$manifest_content" | grep -qF "$backup_path"; then
      : # found
    else
      fail "[apply] manifest missing backup_path for $agent"
      manifest_backup_ok=0
    fi
  done
  [[ "$manifest_backup_ok" -eq 1 ]] && pass "[apply] manifest contains all 5 backup paths"

  # Decoy untouched
  if [[ -f "$fake_home/.claude/agents/decoy.md" ]]; then
    pass "[apply] decoy.md untouched"
  else
    fail "[apply] decoy.md missing after apply"
  fi

  # Haoshoku sources untouched
  local src_ok=1
  for agent in "${STANDALONE_AGENTS[@]}"; do
    if [[ ! -f "$FAKE_HAOSHOKU_DIR/$agent" ]]; then
      fail "[apply] haoshoku source $agent missing after apply"
      src_ok=0
    fi
  done
  [[ "$src_ok" -eq 1 ]] && pass "[apply] haoshoku sources untouched"

  # Output mentions RETIRED
  if printf '%s\n' "$apply_output" | grep -qi "RETIRED"; then
    pass "[apply] output mentions RETIRED"
  else
    fail "[apply] output does not mention RETIRED; got: $apply_output"
  fi

  # ----- (c) Restore -----
  echo "--- test (c): restore returns symlinks ---"

  local restore_output restore_rc
  restore_output="$(run_retire "$fake_home" --restore "$backup_dir" 2>&1)"
  restore_rc=$?

  if [[ "$restore_rc" -eq 0 ]]; then
    pass "[restore] exit code 0"
  else
    fail "[restore] unexpected nonzero exit: $restore_rc; output: $restore_output"
  fi

  # All 5 symlinks must be back at original locations
  local back=0
  for agent in "${STANDALONE_AGENTS[@]}"; do
    if [[ -L "$fake_home/.claude/agents/$agent" ]]; then
      back=$((back + 1))
      local target
      target="$(readlink "$fake_home/.claude/agents/$agent")"
      if [[ "$target" == "$FAKE_HAOSHOKU_DIR/$agent" ]]; then
        pass "[restore] $agent restored with correct target"
      else
        fail "[restore] $agent restored but target wrong: $target"
      fi
    else
      fail "[restore] $agent not restored to original location"
    fi
  done
  [[ "$back" -eq 5 ]] && pass "[restore] all 5 symlinks back at original locations"

  # Output mentions RESTORED
  if printf '%s\n' "$restore_output" | grep -qi "RESTORED"; then
    pass "[restore] output mentions RESTORED"
  else
    fail "[restore] output does not mention RESTORED; got: $restore_output"
  fi

  # A second restore is not a successful no-op.  It should fail loudly so an
  # operator does not treat an already-consumed backup as fresh evidence.
  local second_restore_output second_restore_rc
  second_restore_output="$(run_retire "$fake_home" --restore "$backup_dir" 2>&1)"
  second_restore_rc=$?

  if [[ "$second_restore_rc" -ne 0 ]]; then
    pass "[restore/double] second restore exits nonzero"
  else
    fail "[restore/double] second restore should exit nonzero; output: $second_restore_output"
  fi

  if printf '%s\n' "$second_restore_output" | grep -qi "already restored"; then
    pass "[restore/double] output mentions already restored"
  else
    fail "[restore/double] output should mention already restored; got: $second_restore_output"
  fi
}

# ---------------------------------------------------------------------------
# (d) Stale/missing cache → parity gate refuses, exits nonzero, nothing moves
# ---------------------------------------------------------------------------
test_parity_fail() {
  echo "--- test (d): parity gate refuses stale or missing cache ---"

  # Case 1: no cache dir at all
  local fake_home_no_cache
  fake_home_no_cache="$(build_fake_home "home-parity-no-cache" 0)"

  local output rc
  output="$(run_retire "$fake_home_no_cache" --apply 2>&1)"
  rc=$?

  if [[ "$rc" -ne 0 ]]; then
    pass "[parity-fail/no-cache] apply exited nonzero (exit $rc)"
  else
    fail "[parity-fail/no-cache] apply should refuse when cache missing, got exit 0"
  fi

  if printf '%s\n' "$output" | grep -qi "parity"; then
    pass "[parity-fail/no-cache] output mentions parity failure"
  else
    fail "[parity-fail/no-cache] output should mention parity failure; got: $output"
  fi

  # No symlink moved
  local moved=0
  for agent in "${STANDALONE_AGENTS[@]}"; do
    if [[ ! -L "$fake_home_no_cache/.claude/agents/$agent" ]]; then
      fail "[parity-fail/no-cache] $agent was moved despite parity failure"
      moved=1
    fi
  done
  [[ "$moved" -eq 0 ]] && pass "[parity-fail/no-cache] no symlinks moved on parity failure"

  # Case 2: cache present but incomplete (missing 2 agents)
  local fake_home_partial
  fake_home_partial="$(build_fake_home "home-parity-partial" 1 "partial")"

  output="$(run_retire "$fake_home_partial" --apply 2>&1)"
  rc=$?

  if [[ "$rc" -ne 0 ]]; then
    pass "[parity-fail/partial] apply exited nonzero (exit $rc)"
  else
    fail "[parity-fail/partial] apply should refuse when cache is incomplete, got exit 0"
  fi

  if printf '%s\n' "$output" | grep -qi "parity"; then
    pass "[parity-fail/partial] output mentions parity failure"
  else
    fail "[parity-fail/partial] output should mention parity failure; got: $output"
  fi

  local moved_partial=0
  for agent in "${STANDALONE_AGENTS[@]}"; do
    if [[ ! -L "$fake_home_partial/.claude/agents/$agent" ]]; then
      fail "[parity-fail/partial] $agent was moved despite parity failure"
      moved_partial=1
    fi
  done
  [[ "$moved_partial" -eq 0 ]] && pass "[parity-fail/partial] no symlinks moved on partial-cache parity failure"
}

# ---------------------------------------------------------------------------
# (e) Skills dir untouched after apply
# ---------------------------------------------------------------------------
test_skills_untouched() {
  echo "--- test (e): skills dir untouched after apply ---"
  local fake_home
  fake_home="$(build_fake_home "home-skills")"

  run_retire "$fake_home" --apply >/dev/null 2>&1 || true

  if [[ -f "$fake_home/.claude/skills/some-skill.md" ]]; then
    pass "[skills] skills dir untouched after apply"
  else
    fail "[skills] some-skill.md missing after apply — skills dir was modified"
  fi
}

# ---------------------------------------------------------------------------
# (f) Codex empty dirs → no-op report; never retires from Codex
# ---------------------------------------------------------------------------
test_codex_noop() {
  echo "--- test (f): Codex dirs empty → no-op report ---"
  local fake_home
  fake_home="$(build_fake_home "home-codex")"

  # dry-run (default) with empty Codex dirs
  local output rc
  output="$(run_retire "$fake_home" 2>&1)"
  rc=$?

  if [[ "$rc" -eq 0 ]]; then
    pass "[codex] dry-run exit code 0 with empty Codex dirs"
  else
    fail "[codex] dry-run exited nonzero: $rc"
  fi

  if printf '%s\n' "$output" | grep -qi "no-op"; then
    pass "[codex] output mentions no-op"
  else
    fail "[codex] output should mention no-op for Codex; got: $output"
  fi

  if printf '%s\n' "$output" | grep -qi "codex"; then
    pass "[codex] output mentions Codex"
  else
    fail "[codex] output should mention Codex; got: $output"
  fi

  # Also verify apply doesn't invent Codex retirement work
  local apply_output apply_rc
  apply_output="$(run_retire "$fake_home" --apply 2>&1)"
  apply_rc=$?

  if [[ "$apply_rc" -eq 0 ]]; then
    pass "[codex] apply succeeds (parity available)"
  else
    fail "[codex] apply exited nonzero: $apply_rc; output: $apply_output"
  fi

  if printf '%s\n' "$apply_output" | grep -qi "no-op"; then
    pass "[codex] apply output also mentions no-op for Codex"
  else
    fail "[codex] apply output should mention no-op for Codex; got: $apply_output"
  fi
}

# ---------------------------------------------------------------------------
# (g) Partial pre-existing retirement: missing allowlisted symlinks are skipped,
#     remaining allowlisted symlinks are retired/restored, and manifest tracks
#     only what moved in this run.
# ---------------------------------------------------------------------------
test_partial_pre_existing_retirement() {
  echo "--- test (g): partial pre-existing retirement ---"
  local fake_home
  fake_home="$(build_fake_home "home-partial-preexisting")"

  rm "$fake_home/.claude/agents/adversarial-analyst.md"
  rm "$fake_home/.claude/agents/architect.md"

  local apply_output apply_rc
  apply_output="$(run_retire "$fake_home" --apply 2>&1)"
  apply_rc=$?

  if [[ "$apply_rc" -eq 0 ]]; then
    pass "[partial] apply exit code 0"
  else
    fail "[partial] unexpected nonzero exit: $apply_rc; output: $apply_output"
  fi

  if printf '%s\n' "$apply_output" | grep -q "3 agent(s) retired"; then
    pass "[partial] output reports 3 retired agents"
  else
    fail "[partial] output should report 3 retired agents; got: $apply_output"
  fi

  local backup_dir=""
  backup_dir="$(find "$fake_home/.claude/agents" -maxdepth 1 -name '.retired-*' -type d 2>/dev/null | head -1)"
  if [[ -n "$backup_dir" ]]; then
    pass "[partial] backup dir created"
  else
    fail "[partial] backup dir missing"
    return
  fi

  local manifest_count
  manifest_count="$(grep -c '"original_path"' "$backup_dir/manifest.json" 2>/dev/null || echo 0)"
  if [[ "$manifest_count" -eq 3 ]]; then
    pass "[partial] manifest tracks only 3 moved symlinks"
  else
    fail "[partial] expected 3 manifest entries, got $manifest_count"
  fi

  for agent in developer.md quality-reviewer.md sandbox-executor.md; do
    if [[ -L "$backup_dir/$agent" ]]; then
      pass "[partial] $agent moved to backup"
    else
      fail "[partial] $agent missing from backup"
    fi
  done

  local restore_output restore_rc
  restore_output="$(run_retire "$fake_home" --restore "$backup_dir" 2>&1)"
  restore_rc=$?
  if [[ "$restore_rc" -eq 0 ]]; then
    pass "[partial] restore exit code 0"
  else
    fail "[partial] restore exited nonzero: $restore_rc; output: $restore_output"
  fi

  for agent in developer.md quality-reviewer.md sandbox-executor.md; do
    if [[ -L "$fake_home/.claude/agents/$agent" ]]; then
      pass "[partial] $agent restored"
    else
      fail "[partial] $agent not restored"
    fi
  done

  for agent in adversarial-analyst.md architect.md; do
    if [[ ! -e "$fake_home/.claude/agents/$agent" && ! -L "$fake_home/.claude/agents/$agent" ]]; then
      pass "[partial] pre-existing absent $agent remains absent"
    else
      fail "[partial] pre-existing absent $agent was recreated"
    fi
  done
}

# ---------------------------------------------------------------------------
# (i) Crafted/corrupted manifest must not restore outside the allowlist.
# ---------------------------------------------------------------------------
test_restore_rejects_non_allowlisted_manifest() {
  echo "--- test (i): restore rejects non-allowlisted manifest entry ---"
  local fake_home
  fake_home="$(build_fake_home "home-crafted-manifest")"

  local backup_dir="$fake_home/.claude/agents/.retired-crafted"
  mkdir -p "$backup_dir"
  printf '# fake haoshoku source: decoy.md\n' > "$FAKE_HAOSHOKU_DIR/decoy.md"
  ln -s "$FAKE_HAOSHOKU_DIR/decoy.md" "$backup_dir/decoy.md"
  rm -f "$fake_home/.claude/agents/decoy.md"

  jq -n \
    --arg backup_dir "$backup_dir" \
    --arg original_path "$fake_home/.claude/agents/decoy.md" \
    --arg backup_path "$backup_dir/decoy.md" \
    --arg symlink_target "$FAKE_HAOSHOKU_DIR/decoy.md" \
    '{
      retired_at: "test",
      dvandva_version: "0.4.0",
      backup_dir: $backup_dir,
      entries: [
        {
          original_path: $original_path,
          backup_path: $backup_path,
          symlink_target: $symlink_target
        }
      ]
    }' > "$backup_dir/manifest.json"

  local output rc
  output="$(run_retire "$fake_home" --restore "$backup_dir" 2>&1)"
  rc=$?

  if [[ "$rc" -ne 0 ]]; then
    pass "[restore/allowlist] crafted manifest exits nonzero"
  else
    fail "[restore/allowlist] crafted manifest should fail; output: $output"
  fi

  if printf '%s\n' "$output" | grep -qi "invalid manifest entry"; then
    pass "[restore/allowlist] output mentions invalid manifest entry"
  else
    fail "[restore/allowlist] output should mention invalid manifest entry; got: $output"
  fi

  if [[ ! -e "$fake_home/.claude/agents/decoy.md" && ! -L "$fake_home/.claude/agents/decoy.md" ]]; then
    pass "[restore/allowlist] decoy.md was not restored"
  else
    fail "[restore/allowlist] decoy.md was restored from crafted manifest"
  fi

  if [[ -L "$backup_dir/decoy.md" ]]; then
    pass "[restore/allowlist] crafted backup symlink left untouched"
  else
    fail "[restore/allowlist] crafted backup symlink was moved"
  fi

  local mixed_backup_dir="$fake_home/.claude/agents/.retired-mixed"
  mkdir -p "$mixed_backup_dir"
  ln -s "$FAKE_HAOSHOKU_DIR/developer.md" "$mixed_backup_dir/developer.md"
  ln -s "$FAKE_HAOSHOKU_DIR/decoy.md" "$mixed_backup_dir/decoy.md"
  rm -f "$fake_home/.claude/agents/developer.md"

  jq -n \
    --arg backup_dir "$mixed_backup_dir" \
    --arg valid_original "$fake_home/.claude/agents/developer.md" \
    --arg valid_backup "$mixed_backup_dir/developer.md" \
    --arg valid_target "$FAKE_HAOSHOKU_DIR/developer.md" \
    --arg invalid_original "$fake_home/.claude/agents/decoy.md" \
    --arg invalid_backup "$mixed_backup_dir/decoy.md" \
    --arg invalid_target "$FAKE_HAOSHOKU_DIR/decoy.md" \
    '{
      retired_at: "test",
      dvandva_version: "0.4.0",
      backup_dir: $backup_dir,
      entries: [
        {
          original_path: $valid_original,
          backup_path: $valid_backup,
          symlink_target: $valid_target
        },
        {
          original_path: $invalid_original,
          backup_path: $invalid_backup,
          symlink_target: $invalid_target
        }
      ]
    }' > "$mixed_backup_dir/manifest.json"

  output="$(run_retire "$fake_home" --restore "$mixed_backup_dir" 2>&1)"
  rc=$?

  if [[ "$rc" -ne 0 ]]; then
    pass "[restore/allowlist] mixed manifest exits nonzero"
  else
    fail "[restore/allowlist] mixed manifest should fail; output: $output"
  fi

  if [[ -L "$mixed_backup_dir/developer.md" ]]; then
    pass "[restore/allowlist] valid backup left untouched after later invalid entry"
  else
    fail "[restore/allowlist] valid backup moved before later invalid entry was rejected"
  fi

  if [[ ! -e "$fake_home/.claude/agents/developer.md" && ! -L "$fake_home/.claude/agents/developer.md" ]]; then
    pass "[restore/allowlist] valid original remains absent after mixed manifest failure"
  else
    fail "[restore/allowlist] valid original was restored before mixed manifest rejection"
  fi

  if [[ -L "$mixed_backup_dir/decoy.md" ]]; then
    pass "[restore/allowlist] mixed decoy backup left untouched"
  else
    fail "[restore/allowlist] mixed decoy backup was moved"
  fi
}

# ---------------------------------------------------------------------------
# (j) Manifest JSON must be parser-valid and paths with quotes must restore.
# ---------------------------------------------------------------------------
test_manifest_json_roundtrip_with_quoted_paths() {
  echo "--- test (j): manifest JSON handles quoted path components ---"
  local fake_home
  fake_home="$(build_fake_home 'home-with-"quote')"

  local apply_output apply_rc
  apply_output="$(run_retire "$fake_home" --apply 2>&1)"
  apply_rc=$?

  if [[ "$apply_rc" -eq 0 ]]; then
    pass "[manifest/json] apply exit code 0"
  else
    fail "[manifest/json] apply should succeed with quoted HOME; output: $apply_output"
    return
  fi

  local backup_dir
  backup_dir="$(find "$fake_home/.claude/agents" -maxdepth 1 -name '.retired-*' -type d 2>/dev/null | head -1)"
  if [[ -n "$backup_dir" ]]; then
    pass "[manifest/json] backup dir created"
  else
    fail "[manifest/json] backup dir missing"
    return
  fi

  if jq empty "$backup_dir/manifest.json" >/dev/null 2>&1; then
    pass "[manifest/json] manifest parses as JSON"
  else
    fail "[manifest/json] manifest is not valid JSON"
  fi

  local restore_output restore_rc
  restore_output="$(run_retire "$fake_home" --restore "$backup_dir" 2>&1)"
  restore_rc=$?

  if [[ "$restore_rc" -eq 0 ]]; then
    pass "[manifest/json] restore exit code 0"
  else
    fail "[manifest/json] restore failed for quoted HOME; output: $restore_output"
  fi

  local restored=0
  for agent in "${STANDALONE_AGENTS[@]}"; do
    if [[ -L "$fake_home/.claude/agents/$agent" ]]; then
      restored=$((restored + 1))
    else
      fail "[manifest/json] $agent not restored under quoted HOME"
    fi
  done
  [[ "$restored" -eq 5 ]] && pass "[manifest/json] all 5 symlinks restored under quoted HOME"
}

# ---------------------------------------------------------------------------
# (k) A HOME path with a space must round-trip through apply → restore.
#     Spaces are the most common word-splitting hazard for unquoted paths;
#     the jq --arg / jq -r path must not break on them.
# ---------------------------------------------------------------------------
test_manifest_json_roundtrip_with_space_in_path() {
  echo "--- test (k): manifest JSON handles space in HOME path ---"
  local fake_home
  fake_home="$(build_fake_home 'home with space')"

  local apply_output apply_rc
  apply_output="$(run_retire "$fake_home" --apply 2>&1)"
  apply_rc=$?

  if [[ "$apply_rc" -eq 0 ]]; then
    pass "[manifest/space] apply exit code 0"
  else
    fail "[manifest/space] apply should succeed with space in HOME; output: $apply_output"
    return
  fi

  local backup_dir
  backup_dir="$(find "$fake_home/.claude/agents" -maxdepth 1 -name '.retired-*' -type d 2>/dev/null | head -1)"
  if [[ -n "$backup_dir" ]]; then
    pass "[manifest/space] backup dir created"
  else
    fail "[manifest/space] backup dir missing"
    return
  fi

  if jq empty "$backup_dir/manifest.json" >/dev/null 2>&1; then
    pass "[manifest/space] manifest parses as JSON"
  else
    fail "[manifest/space] manifest is not valid JSON"
    return
  fi

  local restore_output restore_rc
  restore_output="$(run_retire "$fake_home" --restore "$backup_dir" 2>&1)"
  restore_rc=$?

  if [[ "$restore_rc" -eq 0 ]]; then
    pass "[manifest/space] restore exit code 0"
  else
    fail "[manifest/space] restore failed for space HOME; output: $restore_output"
  fi

  local restored=0
  for agent in "${STANDALONE_AGENTS[@]}"; do
    if [[ -L "$fake_home/.claude/agents/$agent" ]]; then
      restored=$((restored + 1))
    else
      fail "[manifest/space] $agent not restored under space HOME"
    fi
  done
  [[ "$restored" -eq 5 ]] && pass "[manifest/space] all 5 symlinks restored under space HOME"
}

# ---------------------------------------------------------------------------
# Pre-flight: confirm the script under test exists
# ---------------------------------------------------------------------------
if [[ ! -f "$RETIRE_SCRIPT" ]]; then
  echo "FAIL: implementation script not found: $RETIRE_SCRIPT" >&2
  echo "  All tests cannot run until retire-standalone-agents.sh is created." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Run all scenarios
# ---------------------------------------------------------------------------
echo "=== retire-standalone-agents test suite ==="
echo ""

test_dry_run_immutability
echo ""
test_apply_and_restore
echo ""
test_parity_fail
echo ""
test_skills_untouched
echo ""
test_codex_noop
echo ""
test_partial_pre_existing_retirement
echo ""
test_restore_rejects_non_allowlisted_manifest
echo ""
test_manifest_json_roundtrip_with_quoted_paths
echo ""
test_manifest_json_roundtrip_with_space_in_path
echo ""

if [[ "$FAILURES" -gt 0 ]]; then
  echo "=== FAILED: $FAILURES assertion(s) failed ===" >&2
  exit 1
fi

echo "=== PASSED: all assertions passed ==="
exit 0
