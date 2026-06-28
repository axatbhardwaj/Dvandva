#!/usr/bin/env bash
# Focused tests for the cross-engine Dvandva installer wrapper.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR=""

cleanup() {
  if [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]]; then
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

TMP_PARENT="${TMPDIR:-/tmp}"
TMP_PARENT="${TMP_PARENT%/}"
TMP_DIR="$(mktemp -d "$TMP_PARENT/dvandva-test-install.XXXXXX")"
case "$TMP_DIR" in
  "$TMP_PARENT"/dvandva-test-install.*) ;;
  *)
    fail "mktemp returned an unexpected path: $TMP_DIR"
    ;;
esac

FAKE_BIN="$TMP_DIR/bin"
FAKE_MARKETPLACE="$TMP_DIR/marketplace"
INSTALL_LOG="$TMP_DIR/install.log"
mkdir -p "$FAKE_BIN" "$FAKE_MARKETPLACE/.agents/plugins" "$TMP_DIR/codex-home" "$TMP_DIR/home"
cat > "$FAKE_MARKETPLACE/.agents/plugins/marketplace.json" <<'JSON'
{"name":"dvandva","plugins":[{"name":"dvandva"}]}
JSON

cat > "$FAKE_BIN/claude" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf 'claude %s\n' "$*" >> "$DVANDVA_INSTALL_TEST_LOG"

case "$*" in
  plugin\ marketplace\ add\ *)
    if [[ "${DVANDVA_INSTALL_TEST_ALREADY:-0}" == "1" ]]; then
      echo "Marketplace 'dvandva' already registered" >&2
      exit 1
    fi
    ;;
  "plugin install dvandva@dvandva")
    if [[ "${DVANDVA_INSTALL_TEST_ALREADY:-0}" == "1" ]]; then
      echo "Plugin 'dvandva@dvandva' already installed" >&2
      exit 1
    fi
    ;;
  *)
    echo "unexpected fake claude invocation: $*" >&2
    exit 64
    ;;
esac
SH
chmod +x "$FAKE_BIN/claude"

cat > "$FAKE_BIN/codex" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf 'codex %s\n' "$*" >> "$DVANDVA_INSTALL_TEST_LOG"

case "$*" in
  "plugin add --help")
    cat <<'HELP'
Install a plugin from a configured marketplace snapshot.
Usage: codex plugin add [OPTIONS] <PLUGIN[@MARKETPLACE]>
HELP
    ;;
  plugin\ marketplace\ add\ *)
    if [[ "${DVANDVA_INSTALL_TEST_ALREADY:-0}" == "1" ]]; then
      echo "Marketplace 'dvandva' already added" >&2
      exit 1
    fi
    ;;
  "plugin add dvandva@dvandva")
    if [[ "${DVANDVA_INSTALL_TEST_ALREADY:-0}" == "1" ]]; then
      echo "Plugin 'dvandva@dvandva' already installed" >&2
      exit 1
    fi
    ;;
  app-server\ *)
    echo "app-server fallback should not run when codex plugin add exists" >&2
    exit 42
    ;;
  *)
    echo "unexpected fake codex invocation: $*" >&2
    exit 64
    ;;
esac
SH
chmod +x "$FAKE_BIN/codex"

OUTPUT="$TMP_DIR/install.out"
if ! PATH="$FAKE_BIN:$PATH" \
  CODEX_HOME="$TMP_DIR/codex-home" \
  HOME="$TMP_DIR/home" \
  DVANDVA_INSTALL_TEST_LOG="$INSTALL_LOG" \
  bash "$ROOT_DIR/scripts/install.sh" "$FAKE_MARKETPLACE" > "$OUTPUT" 2>&1; then
  cat "$OUTPUT" >&2
  fail "scripts/install.sh should install Dvandva for both Claude Code and Codex"
fi

grep -q "claude plugin marketplace add $FAKE_MARKETPLACE" "$INSTALL_LOG" \
  || fail "installer did not register the Claude marketplace"
grep -q "claude plugin install dvandva@dvandva" "$INSTALL_LOG" \
  || fail "installer did not install the Claude plugin"
grep -q "codex plugin marketplace add $FAKE_MARKETPLACE" "$INSTALL_LOG" \
  || fail "installer did not register the Codex marketplace"
grep -q "codex plugin add dvandva@dvandva" "$INSTALL_LOG" \
  || fail "installer did not install the Codex plugin"
grep -q "Claude Code install complete" "$OUTPUT" \
  || fail "installer output should report Claude completion"
grep -q "Codex install complete" "$OUTPUT" \
  || fail "installer output should report Codex completion"
grep -q "dvandva:testing" "$OUTPUT" \
  || fail "installer output should tell users to verify absorbed testing skill"
grep -q "dvandva:understanding" "$OUTPUT" \
  || fail "installer output should tell users to verify absorbed understanding skill"
grep -q "dvandva:worktree-setup" "$OUTPUT" \
  || fail "installer output should tell users to verify absorbed worktree skill"

CONFLICT_LOG="$TMP_DIR/conflict.log"
CONFLICT_OUTPUT="$TMP_DIR/conflict.out"
if PATH="$FAKE_BIN:$PATH" \
  CODEX_HOME="$TMP_DIR/codex-home-conflict" \
  HOME="$TMP_DIR/home-conflict" \
  DVANDVA_INSTALL_TEST_LOG="$CONFLICT_LOG" \
  bash "$ROOT_DIR/scripts/install.sh" --claude-only --codex-only "$FAKE_MARKETPLACE" > "$CONFLICT_OUTPUT" 2>&1; then
  cat "$CONFLICT_OUTPUT" >&2
  fail "scripts/install.sh should reject contradictory single-engine flags"
fi

grep -q "cannot be combined" "$CONFLICT_OUTPUT" \
  || fail "conflicting flag output should explain the invalid combination"
if [[ -f "$CONFLICT_LOG" ]]; then
  fail "conflicting flags should be rejected before invoking engine CLIs"
fi

ALREADY_LOG="$TMP_DIR/already.log"
ALREADY_OUTPUT="$TMP_DIR/already.out"
if ! PATH="$FAKE_BIN:$PATH" \
  CODEX_HOME="$TMP_DIR/codex-home-already" \
  HOME="$TMP_DIR/home-already" \
  DVANDVA_INSTALL_TEST_LOG="$ALREADY_LOG" \
  DVANDVA_INSTALL_TEST_ALREADY=1 \
  bash "$ROOT_DIR/scripts/install.sh" "$FAKE_MARKETPLACE" > "$ALREADY_OUTPUT" 2>&1; then
  cat "$ALREADY_OUTPUT" >&2
  fail "scripts/install.sh should tolerate already-registered marketplaces and plugins"
fi

grep -q "Claude Code marketplace already present; continuing." "$ALREADY_OUTPUT" \
  || fail "already-present Claude marketplace should be reported and tolerated"
grep -q "Claude Code plugin already present; continuing." "$ALREADY_OUTPUT" \
  || fail "already-present Claude plugin should be reported and tolerated"
grep -q "Codex marketplace already present; continuing." "$ALREADY_OUTPUT" \
  || fail "already-present Codex marketplace should be reported and tolerated"
grep -q "Codex plugin already present; continuing." "$ALREADY_OUTPUT" \
  || fail "already-present Codex plugin should be reported and tolerated"

CLAUDE_ONLY_LOG="$TMP_DIR/claude-only.log"
CLAUDE_ONLY_OUTPUT="$TMP_DIR/claude-only.out"
if ! PATH="$FAKE_BIN:$PATH" \
  CODEX_HOME="$TMP_DIR/codex-home-claude-only" \
  HOME="$TMP_DIR/home-claude-only" \
  DVANDVA_INSTALL_TEST_LOG="$CLAUDE_ONLY_LOG" \
  bash "$ROOT_DIR/scripts/install.sh" --claude-only "$FAKE_MARKETPLACE" > "$CLAUDE_ONLY_OUTPUT" 2>&1; then
  cat "$CLAUDE_ONLY_OUTPUT" >&2
  fail "scripts/install.sh --claude-only should install only the Claude plugin"
fi

grep -q "claude plugin marketplace add $FAKE_MARKETPLACE" "$CLAUDE_ONLY_LOG" \
  || fail "claude-only install did not register the Claude marketplace"
grep -q "claude plugin install dvandva@dvandva" "$CLAUDE_ONLY_LOG" \
  || fail "claude-only install did not install the Claude plugin"
if grep -q '^codex ' "$CLAUDE_ONLY_LOG"; then
  fail "claude-only install should not invoke codex"
fi

CODEX_ONLY_LOG="$TMP_DIR/codex-only.log"
CODEX_ONLY_OUTPUT="$TMP_DIR/codex-only.out"
if ! PATH="$FAKE_BIN:$PATH" \
  CODEX_HOME="$TMP_DIR/codex-home-codex-only" \
  HOME="$TMP_DIR/home-codex-only" \
  DVANDVA_INSTALL_TEST_LOG="$CODEX_ONLY_LOG" \
  bash "$ROOT_DIR/scripts/install.sh" --codex-only "$FAKE_MARKETPLACE" > "$CODEX_ONLY_OUTPUT" 2>&1; then
  cat "$CODEX_ONLY_OUTPUT" >&2
  fail "scripts/install.sh --codex-only should install only the Codex plugin"
fi

grep -q "codex plugin marketplace add $FAKE_MARKETPLACE" "$CODEX_ONLY_LOG" \
  || fail "codex-only install did not register the Codex marketplace"
grep -q "codex plugin add dvandva@dvandva" "$CODEX_ONLY_LOG" \
  || fail "codex-only install did not install the Codex plugin"
if grep -q '^claude ' "$CODEX_ONLY_LOG"; then
  fail "codex-only install should not invoke claude"
fi

echo "PASS: install.sh installs Dvandva for Claude Code and Codex"
