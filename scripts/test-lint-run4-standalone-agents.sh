#!/usr/bin/env bash
# Fixture-driven tests for scripts/lint-run4-standalone-agents.sh.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINT_SCRIPT="$ROOT_DIR/scripts/lint-run4-standalone-agents.sh"
TMP_DIR="$(mktemp -d)"
FAILURES=0

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

DVANDVA_AGENTS=(
  adversarial-analyst
  architect
  baton-auditor
  cross-reviewer
  debugger
  deep-reviewer
  deslopper
  doc-verifier
  implementer
  integration-checker
  pattern-mapper
  researcher
  sandbox-verifier
  security-auditor
  test-creator
)

pass() {
  printf 'PASS: %s\n' "$*"
}

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  FAILURES=$((FAILURES + 1))
}

write_manifest_files() {
  local root="$1"
  mkdir -p "$root/.claude-plugin" "$root/plugins/dvandva/.claude-plugin" "$root/plugins/dvandva/.codex-plugin"
  cat > "$root/.claude-plugin/marketplace.json" <<'EOF'
{
  "plugins": [
    { "name": "dvandva", "source": "./plugins/dvandva", "version": "1.0.0" }
  ]
}
EOF
  cat > "$root/plugins/dvandva/.claude-plugin/plugin.json" <<'EOF'
{ "name": "dvandva", "version": "1.0.0" }
EOF
  cat > "$root/plugins/dvandva/.codex-plugin/plugin.json" <<'EOF'
{ "name": "dvandva", "version": "1.0.0" }
EOF
}

write_agent_files() {
  local root="$1"
  mkdir -p "$root/plugins/dvandva/agents"
  local agent
  for agent in "${DVANDVA_AGENTS[@]}"; do
    cat > "$root/plugins/dvandva/agents/$agent.md" <<EOF
---
name: dvandva-$agent
---
# dvandva-$agent
EOF
  done
}

write_retirement_fixture() {
  local root="$1"
  mkdir -p \
    "$root/docs/protocol" \
    "$root/plugins/dvandva/references" \
    "$root/scripts"
  write_manifest_files "$root"
  write_agent_files "$root"

  cat > "$root/README.md" <<'EOF'
Dvandva 1.0.0 ships the canonical Dvandva roster. Run 4 makes Dvandva-only
retirement available only for Dvandva-covered workflows. The retired Claude
symlink allowlist is adversarial-analyst, architect, developer, quality-reviewer,
and sandbox-executor. Functional parity is proven by Runs 1-4 usage, not only by
file count. Codex agent-axis retirement is a no-op. Skills are out of scope; no
skill files are touched. The helper writes a backup manifest and supports
restore.
EOF

  cat > "$root/product.md" <<'EOF'
Run 4 retires only Dvandva-covered standalone agents after version 1.0.0 cache
parity and functional parity via Runs 1-4 usage. The Claude allowlist is
adversarial-analyst, architect, developer, quality-reviewer, and
sandbox-executor. Codex agent-axis cleanup is explicitly no-op. Skills are out
of scope. Restore uses the backup manifest.
EOF

  cat > "$root/docs/protocol/local-baton-channel.md" <<'EOF'
Run 4 retirement is Dvandva-only and limited to Dvandva-covered workflows. It
does not retire Codex agent-axis files, does not touch skills, and is reversible
through a backup manifest restore path.
EOF

  cat > "$root/plugins/dvandva/references/state-transition-table.md" <<'EOF'
Run 4 records the 1.0.0 Dvandva roster parity, Dvandva-only retirement, Codex
agent-axis no-op, and functional parity via Runs 1-4 usage.
EOF

  cat > "$root/plugins/dvandva/references/baton-schema-v2.json" <<'EOF'
{
  "description": "Run 4 Dvandva-only retirement with backup manifest restore and no skill touches"
}
EOF

  cat > "$root/scripts/retire-standalone-agents.sh" <<'EOF'
#!/usr/bin/env bash
echo "Dvandva-only Dvandva-covered workflows functional parity via Runs 1-4 usage"
echo "adversarial-analyst architect developer quality-reviewer sandbox-executor"
echo "Codex agent-axis no-op skills out of scope no skill touches backup manifest restore 1.0.0"
EOF

  cat > "$root/scripts/test-retire-standalone-agents.sh" <<'EOF'
#!/usr/bin/env bash
echo "backup manifest restore Codex agent-axis no-op no skill touches"
EOF

  cat > "$root/scripts/smoke-plugin-install.sh" <<'EOF'
#!/usr/bin/env bash
echo "1.0.0 dvandva-adversarial-analyst dvandva-test-creator"
EOF

  cat > "$root/scripts/test-install.sh" <<'EOF'
#!/usr/bin/env bash
echo "1.0.0 canonical 15-agent roster"
EOF

  cat > "$root/scripts/test-install-codex.sh" <<'EOF'
#!/usr/bin/env bash
echo "1.0.0 canonical 15-agent roster"
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
write_retirement_fixture "$GOOD"
expect_pass "standalone-agent lint accepts complete fixture" "$GOOD"

CASE="$TMP_DIR/no-dvandva-only"
write_retirement_fixture "$CASE"
perl -0pi -e 's/Dvandva-only/general/g' "$CASE/README.md"
expect_fail \
  "standalone lint rejects README without Dvandva-only scope" \
  "$CASE" \
  "README.md must document Dvandva-only retirement"

CASE="$TMP_DIR/stale-readme"
write_retirement_fixture "$CASE"
printf '\nv0.2.0 ships legacy text\nRun 3 (in progress)\n' >> "$CASE/README.md"
expect_fail \
  "standalone lint rejects stale release wording" \
  "$CASE" \
  "README.md contains stale Run 3 or v0.2.0 wording"

CASE="$TMP_DIR/no-codex-noop"
write_retirement_fixture "$CASE"
perl -0pi -e 's/Codex agent-axis retirement is a no-op\.//g' "$CASE/README.md"
perl -0pi -e 's/Codex agent-axis cleanup is explicitly no-op\.//g' "$CASE/product.md"
expect_fail \
  "standalone lint rejects missing Codex no-op documentation" \
  "$CASE" \
  "Run4 docs must document Codex agent-axis no-op"

CASE="$TMP_DIR/no-functional-parity"
write_retirement_fixture "$CASE"
perl -0pi -e 's/Functional parity is proven by Runs 1-4 usage, not only by\nfile count\.//g' \
  "$CASE/README.md"
perl -0pi -e 's/functional parity via Runs 1-4 usage//g' "$CASE/product.md"
perl -0pi -e 's/functional parity via Runs 1-4 usage//g' \
  "$CASE/scripts/retire-standalone-agents.sh"
expect_fail \
  "standalone lint rejects missing Runs 1-4 parity rationale" \
  "$CASE" \
  "Run4 docs/scripts must cite functional parity via Runs 1-4 usage"

CASE="$TMP_DIR/no-manifest-restore"
write_retirement_fixture "$CASE"
perl -0pi -e 's/backup manifest restore//g; s/backup manifest and supports\nrestore/backup/g' \
  "$CASE/scripts/retire-standalone-agents.sh" "$CASE/README.md" "$CASE/product.md"
expect_fail \
  "standalone lint rejects missing manifest restore wording" \
  "$CASE" \
  "Run4 retirement surface must document backup manifest and restore"

CASE="$TMP_DIR/version-mismatch"
write_retirement_fixture "$CASE"
jq '.version = "0.3.0"' "$CASE/plugins/dvandva/.codex-plugin/plugin.json" > "$CASE/codex.tmp" \
  && mv "$CASE/codex.tmp" "$CASE/plugins/dvandva/.codex-plugin/plugin.json"
expect_fail \
  "standalone lint rejects manifest version mismatch" \
  "$CASE" \
  "Dvandva manifest versions must all equal 1.0.0"

CASE="$TMP_DIR/missing-agent"
write_retirement_fixture "$CASE"
rm "$CASE/plugins/dvandva/agents/security-auditor.md"
expect_fail \
  "standalone lint rejects missing canonical agent file" \
  "$CASE" \
  "plugins/dvandva/agents must contain exactly the 15 canonical agents"

CASE="$TMP_DIR/bad-frontmatter"
write_retirement_fixture "$CASE"
perl -0pi -e 's/name: dvandva-test-creator/name: test-creator/' \
  "$CASE/plugins/dvandva/agents/test-creator.md"
expect_fail \
  "standalone lint rejects non-dvandva frontmatter name" \
  "$CASE" \
  "agent frontmatter names must use dvandva-*"

if [[ "$FAILURES" -gt 0 ]]; then
  exit 1
fi

exit 0
