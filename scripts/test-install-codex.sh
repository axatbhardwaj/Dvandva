#!/usr/bin/env bash
# Focused tests for the user-facing Codex installer wrapper.
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
TMP_DIR="$(mktemp -d "$TMP_PARENT/dvandva-test-install-codex.XXXXXX")"
case "$TMP_DIR" in
  "$TMP_PARENT"/dvandva-test-install-codex.*) ;;
  *)
    fail "mktemp returned an unexpected path: $TMP_DIR"
    ;;
esac

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
    if [[ "${CODEX_FAKE_ALREADY:-0}" == "1" ]]; then
      echo "Marketplace 'dvandva' already added" >&2
      exit 1
    fi
    mkdir -p "$CODEX_HOME/.tmp/marketplaces/dvandva/.agents/plugins"
    printf '{"name":"dvandva","plugins":[{"name":"dvandva"}]}\n' > "$CODEX_HOME/.tmp/marketplaces/dvandva/.agents/plugins/marketplace.json"
    ;;
  "plugin add dvandva@dvandva")
    if [[ "${CODEX_FAKE_ALREADY:-0}" == "1" ]]; then
      echo "Plugin 'dvandva@dvandva' already installed" >&2
      exit 1
    fi
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

ALREADY_LOG="$TMP_DIR/codex-already.log"
ALREADY_OUTPUT="$TMP_DIR/codex-already.out"
if ! PATH="$FAKE_BIN:$PATH" \
  CODEX_HOME="$TMP_DIR/codex-home-already" \
  HOME="$TMP_DIR/home-already" \
  CODEX_FAKE_LOG="$ALREADY_LOG" \
  CODEX_FAKE_ALREADY=1 \
  bash "$ROOT_DIR/scripts/install-codex.sh" "$FAKE_MARKETPLACE" > "$ALREADY_OUTPUT" 2>&1; then
  cat "$ALREADY_OUTPUT" >&2
  fail "install-codex.sh should tolerate already-registered marketplaces and plugins"
fi

grep -q "Codex marketplace already present; continuing." "$ALREADY_OUTPUT" \
  || fail "already-present Codex marketplace should be reported and tolerated"
grep -q "Codex plugin already present; continuing." "$ALREADY_OUTPUT" \
  || fail "already-present Codex plugin should be reported and tolerated"

FALLBACK_BIN="$TMP_DIR/fallback-bin"
FALLBACK_LOG="$TMP_DIR/codex-fallback.log"
mkdir -p "$FALLBACK_BIN"
cat > "$FALLBACK_BIN/codex" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >> "$CODEX_FAKE_LOG"

case "$*" in
  "plugin add --help")
    echo "unknown command: plugin add" >&2
    exit 1
    ;;
  plugin\ marketplace\ add\ *)
    ;;
  "app-server --listen stdio://")
    while IFS= read -r line; do
      case "$line" in
        *'"id": 1'*|*'"id":1'*)
          printf '{"id":1,"result":{}}\n'
          ;;
        *'"method": "plugin/install"'*|*'"method":"plugin/install"'*)
          printf '{"id":2,"result":{"pluginId":"dvandva@dvandva","installed":true}}\n'
          ;;
      esac
    done
    ;;
  "plugin add dvandva@dvandva")
    echo "modern plugin add path should not run in fallback fixture" >&2
    exit 42
    ;;
  *)
    echo "unexpected fallback fake codex invocation: $*" >&2
    exit 64
    ;;
esac
SH
chmod +x "$FALLBACK_BIN/codex"

FALLBACK_OUTPUT="$TMP_DIR/codex-fallback.out"
if ! PATH="$FALLBACK_BIN:$PATH" \
  CODEX_HOME="$TMP_DIR/codex-home-fallback" \
  HOME="$TMP_DIR/home-fallback" \
  CODEX_FAKE_LOG="$FALLBACK_LOG" \
  bash "$ROOT_DIR/scripts/install-codex.sh" "$FAKE_MARKETPLACE" > "$FALLBACK_OUTPUT" 2>&1; then
  cat "$FALLBACK_OUTPUT" >&2
  fail "install-codex.sh should exercise the legacy app-server fallback when plugin add is unavailable"
fi

grep -q "app-server --listen stdio://" "$FALLBACK_LOG" \
  || fail "fallback fixture did not invoke codex app-server"
grep -q "OK: dvandva@dvandva installed via app-server RPC" "$FALLBACK_OUTPUT" \
  || fail "fallback output should report app-server RPC success"

echo "PASS: install-codex.sh prefers codex plugin add and covers legacy fallback"
