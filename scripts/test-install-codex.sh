#!/usr/bin/env bash
# Focused tests for the user-facing Codex installer wrapper.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

FAKE_BIN="$TMP_DIR/bin"
FAKE_MARKETPLACE="$TMP_DIR/marketplace"
CODEX_LOG="$TMP_DIR/codex.log"
mkdir -p "$FAKE_BIN" "$FAKE_MARKETPLACE/.agents/plugins" "$TMP_DIR/codex-home" "$TMP_DIR/home"
cat > "$FAKE_MARKETPLACE/.agents/plugins/marketplace.json" <<'JSON'
{"name":"dvandva","plugins":[{"name":"dvandva"}]}
JSON

cat > "$FAKE_BIN/codex" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >> "$CODEX_FAKE_LOG"

case "$*" in
  "plugin add --help")
    cat <<'HELP'
Install a plugin from a configured marketplace snapshot.
Usage: codex plugin add [OPTIONS] <PLUGIN[@MARKETPLACE]>
HELP
    ;;
  plugin\ marketplace\ add\ *)
    mkdir -p "$CODEX_HOME/.tmp/marketplaces/dvandva/.agents/plugins"
    printf '{"name":"dvandva","plugins":[{"name":"dvandva"}]}\n' > "$CODEX_HOME/.tmp/marketplaces/dvandva/.agents/plugins/marketplace.json"
    ;;
  "plugin add dvandva@dvandva")
    printf '{"id":"dvandva@dvandva","installed":true}\n'
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
  CODEX_FAKE_LOG="$CODEX_LOG" \
  bash "$ROOT_DIR/scripts/install-codex.sh" "$FAKE_MARKETPLACE" > "$OUTPUT" 2>&1; then
  cat "$OUTPUT" >&2
  fail "install-codex.sh should use current codex plugin add path"
fi

grep -q "plugin marketplace add $FAKE_MARKETPLACE" "$CODEX_LOG" \
  || fail "installer did not register the marketplace"
grep -q "plugin add dvandva@dvandva" "$CODEX_LOG" \
  || fail "installer did not call codex plugin add"
if grep -q "app-server" "$CODEX_LOG"; then
  cat "$CODEX_LOG" >&2
  fail "installer unexpectedly used app-server fallback"
fi
grep -q "codex plugin add dvandva@dvandva" "$OUTPUT" \
  || fail "installer output should explain the current Codex install command"

echo "PASS: install-codex.sh prefers codex plugin add"
