#!/usr/bin/env bash
# Fixture-driven tests for scripts/lint-run4-path-gates.sh.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINT_SCRIPT="$ROOT_DIR/scripts/lint-run4-path-gates.sh"
TMP_DIR="$(mktemp -d)"
FAILURES=0

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

pass() {
  printf 'PASS: %s\n' "$*"
}

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  FAILURES=$((FAILURES + 1))
}

write_path_fixture() {
  local root="$1"
  mkdir -p \
    "$root/docs/protocol" \
    "$root/plugins/dvandva/references" \
    "$root/plugins/dvandva/skills/vadi/scripts" \
    "$root/plugins/dvandva/skills/prativadi/scripts" \
    "$root/plugins/dvandva/skills/vadi" \
    "$root/plugins/dvandva/skills/prativadi" \
    "$root/scripts" \
    "$root/.githooks"

  cat > "$root/README.md" <<'EOF'
Run 4 path-gate contract: live work_split implementation chunks declare
write_paths. Bare paths remain a backward-compatible write intent only for
implementation/cross_fixing chunks.
EOF

  cat > "$root/product.md" <<'EOF'
Run 4 generalizes the safe_rel_path validator from generated agent instances to
work_split, paths, read_paths, and write_paths. It remains a baton protocol with
no daemon and no hidden central process.
EOF

  cat > "$root/docs/protocol/local-baton-channel.md" <<'EOF'
Run 4 work_split path gates require write_paths for write-capable chunks.
write_paths supplements paths rather than narrowing them; the effective write
set is a union.
Overlaps require a shared conflict_group and explicit depends_on serialization.
cross_review is read-only unless explicit write_paths are present.
The Dvandva shell gate is local; there is no daemon or hidden orchestrator.
EOF

  cat > "$root/plugins/dvandva/references/state-transition-table.md" <<'EOF'
Run 4 validates work_split write_paths with safe_rel_path. Live overlapping
chunks are rejected unless they share conflict_group and depends_on serialization.
Closed or terminal historical chunks can reuse paths later because work_split
has no base_checkpoint wave model.
EOF

  cat > "$root/plugins/dvandva/references/baton-schema-v2.json" <<'EOF'
{
  "properties": {
    "work_split": {
      "items": {
        "properties": {
          "paths": {},
          "read_paths": {},
          "write_paths": {},
          "conflict_group": {},
          "depends_on": {}
        }
      }
    }
  }
}
EOF

  cat > "$root/plugins/dvandva/skills/vadi/scripts/dvandva-write.sh" <<'EOF'
safe_rel_path() { :; }
validate_work_split_paths() {
  echo work_split paths read_paths write_paths conflict_group depends_on cross_review
  echo paths write_paths unique
}
EOF

  cat > "$root/plugins/dvandva/skills/prativadi/scripts/dvandva-write.sh" <<'EOF'
safe_rel_path() { :; }
validate_work_split_paths() {
  echo work_split paths read_paths write_paths conflict_group depends_on cross_review
  echo paths write_paths unique
}
EOF

  cat > "$root/plugins/dvandva/skills/vadi/SKILL.md" <<'EOF'
Preflight runs scripts/install-dvandva-hooks.sh and asserts core.hooksPath=.githooks.
EOF

  cat > "$root/plugins/dvandva/skills/prativadi/SKILL.md" <<'EOF'
Preflight runs scripts/install-dvandva-hooks.sh and asserts core.hooksPath=.githooks.
EOF

  cat > "$root/.githooks/pre-commit" <<'EOF'
#!/usr/bin/env bash
echo ".dvandva/runs baton.json done human_question human_decision"
jq empty "$1" 2>/dev/null || exit 1
exec scripts/dvandva-commit-gate.sh
EOF

  cat > "$root/.githooks/prepare-commit-msg" <<'EOF'
#!/usr/bin/env bash
echo ".dvandva/runs baton.json done human_question human_decision"
jq empty "$1" 2>/dev/null || exit 1
echo "Dvandva-Checkpoint"
EOF

  cat > "$root/scripts/dvandva-commit-gate.sh" <<'EOF'
#!/usr/bin/env bash
echo "DVANDVA_ROLE changed_paths active_roles .dvandva/runs baton.json done human_question human_decision"
jq empty "$1" 2>/dev/null || exit 1
EOF

  cat > "$root/scripts/dvandva-drift-lint.sh" <<'EOF'
#!/usr/bin/env bash
echo "drift lint Dvandva-Checkpoint dvandva.hooksAdoptedAt .dvandva/runs baton.json done human_question human_decision"
jq empty "$1" 2>/dev/null || exit 1
EOF

  cat > "$root/scripts/install-dvandva-hooks.sh" <<'EOF'
#!/usr/bin/env bash
echo ".githooks core.hooksPath"
echo "dvandva.hooksAdoptedAt"
EOF
}

run_lint() {
  local root="$1"
  bash "$LINT_SCRIPT" "$root" 2>&1
}

expect_pass() {
  local name="$1"
  local root="$2"
  local output rc
  output="$(run_lint "$root")"
  rc=$?
  if [[ "$rc" -eq 0 ]]; then
    pass "$name"
  else
    fail "$name expected pass, got exit $rc: $output"
  fi
}

expect_fail() {
  local name="$1"
  local root="$2"
  local expected="$3"
  local output rc
  output="$(run_lint "$root")"
  rc=$?
  if [[ "$rc" -eq 0 ]]; then
    fail "$name expected failure"
    return
  fi
  if [[ "$output" == *"$expected"* ]]; then
    pass "$name"
  else
    fail "$name missing failure text '$expected'; got: $output"
  fi
}

GOOD="$TMP_DIR/good"
write_path_fixture "$GOOD"
expect_pass "path-gate lint accepts complete fixture" "$GOOD"

CASE="$TMP_DIR/no-readme-write-paths"
write_path_fixture "$CASE"
perl -0pi -e 's/write_paths/write intent/g' "$CASE/README.md"
expect_fail \
  "path-gate lint rejects README without write_paths" \
  "$CASE" \
  "README.md must document work_split write_paths"

CASE="$TMP_DIR/no-cross-review-readonly"
write_path_fixture "$CASE"
perl -0pi -e 's/cross_review is read-only unless explicit write_paths are present\.//g' \
  "$CASE/docs/protocol/local-baton-channel.md"
expect_fail \
  "path-gate lint rejects missing cross_review read-only rule" \
  "$CASE" \
  "local-baton-channel.md must document cross_review read-only semantics"

CASE="$TMP_DIR/no-write-path-union"
write_path_fixture "$CASE"
perl -0pi -e 's/write_paths supplements paths rather than narrowing them; the effective write\nset is a union\.//g' \
  "$CASE/docs/protocol/local-baton-channel.md"
expect_fail \
  "path-gate lint rejects missing write_paths union rule" \
  "$CASE" \
  "local-baton-channel.md must document write_paths cannot narrow write-capable paths"

CASE="$TMP_DIR/no-terminal-reuse-rationale"
write_path_fixture "$CASE"
perl -0pi -e 's/Closed or terminal historical chunks can reuse paths later because work_split\nhas no base_checkpoint wave model\.//g' \
  "$CASE/plugins/dvandva/references/state-transition-table.md"
expect_fail \
  "path-gate lint rejects missing terminal reuse rationale" \
  "$CASE" \
  "state-transition-table.md must document terminal work_split reuse rationale"

CASE="$TMP_DIR/no-precommit-gate"
write_path_fixture "$CASE"
printf '#!/usr/bin/env bash\necho gate\n' > "$CASE/.githooks/pre-commit"
expect_fail \
  "path-gate lint rejects pre-commit hook without gate delegation" \
  "$CASE" \
  ".githooks/pre-commit must delegate to dvandva-commit-gate.sh"

CASE="$TMP_DIR/no-checkpoint-trailer"
write_path_fixture "$CASE"
printf '#!/usr/bin/env bash\necho trailer\n' > "$CASE/.githooks/prepare-commit-msg"
expect_fail \
  "path-gate lint rejects prepare hook without checkpoint trailer" \
  "$CASE" \
  ".githooks/prepare-commit-msg must stamp Dvandva-Checkpoint"

CASE="$TMP_DIR/no-prativadi-safe-path"
write_path_fixture "$CASE"
perl -0pi -e 's/safe_rel_path/unsafe_path/g' \
  "$CASE/plugins/dvandva/skills/prativadi/scripts/dvandva-write.sh"
expect_fail \
  "path-gate lint rejects prativadi helper without safe_rel_path" \
  "$CASE" \
  "prativadi dvandva-write.sh must validate work_split paths with safe_rel_path"

CASE="$TMP_DIR/no-vadi-write-union"
write_path_fixture "$CASE"
perl -0pi -e 's/paths write_paths unique/write_paths only/g' \
  "$CASE/plugins/dvandva/skills/vadi/scripts/dvandva-write.sh"
expect_fail \
  "path-gate lint rejects vadi helper without write path union" \
  "$CASE" \
  "vadi dvandva-write.sh must union write-capable paths and write_paths"

CASE="$TMP_DIR/no-hook-adoption-baseline"
write_path_fixture "$CASE"
perl -0pi -e 's/dvandva\.hooksAdoptedAt//g' "$CASE/scripts/install-dvandva-hooks.sh"
expect_fail \
  "path-gate lint rejects installer without hook adoption baseline" \
  "$CASE" \
  "install-dvandva-hooks.sh must record hook-adoption baseline"

CASE="$TMP_DIR/no-vadi-hook-preflight"
write_path_fixture "$CASE"
perl -0pi -e 's#scripts/install-dvandva-hooks\.sh#install hooks#g' \
  "$CASE/plugins/dvandva/skills/vadi/SKILL.md"
expect_fail \
  "path-gate lint rejects vadi skill without hook preflight" \
  "$CASE" \
  "vadi skill preflight must enforce repo-local Dvandva hooks"

CASE="$TMP_DIR/no-prativadi-hook-preflight"
write_path_fixture "$CASE"
perl -0pi -e 's/core\.hooksPath=\.githooks/hooks enabled/g' \
  "$CASE/plugins/dvandva/skills/prativadi/SKILL.md"
expect_fail \
  "path-gate lint rejects prativadi skill without hook preflight" \
  "$CASE" \
  "prativadi skill preflight must enforce repo-local Dvandva hooks"

CASE="$TMP_DIR/no-resolver-run-scope"
write_path_fixture "$CASE"
perl -0pi -e 's#\.dvandva/runs#run dirs#g' "$CASE/scripts/dvandva-drift-lint.sh"
expect_fail \
  "path-gate lint rejects resolver without run-scoped scan" \
  "$CASE" \
  "scripts/dvandva-drift-lint.sh must scan run-scoped baton paths"

if [[ "$FAILURES" -gt 0 ]]; then
  exit 1
fi

exit 0
