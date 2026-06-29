#!/usr/bin/env bash
# Tests for the Dvandva hook preflight helper.
#
# C2 changes the contract from passive detection to active wrapping:
# auto mode must install or refresh the delegated Dvandva wrapper chain and
# prove gate reachability via the active pre-commit selfcheck sentinel.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HELPER="$ROOT_DIR/scripts/dvandva-hook-preflight.sh"
VADI_HELPER="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-hook-preflight.sh"
PRATIVADI_HELPER="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-hook-preflight.sh"
INSTALLER="$ROOT_DIR/scripts/install-dvandva-hooks.sh"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

failures=0

new_git_repo() {
  local dir="$1"
  mkdir -p "$dir"
  git -C "$dir" init --quiet
  git -C "$dir" config user.email "test@dvandva.test"
  git -C "$dir" config user.name "Dvandva Test"
  touch "$dir/.gitkeep"
  git -C "$dir" add .gitkeep
  git -C "$dir" commit --quiet -m "initial"
}

new_husky_repo() {
  local dir="$1"
  mkdir -p "$dir/.husky/_"
  new_git_repo "$dir"
  cat > "$dir/.husky/_/pre-commit" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  chmod +x "$dir/.husky/_/pre-commit"
  git -C "$dir" config core.hooksPath ".husky/_"
}

stage_hook_sources() {
  local dir="$1"
  mkdir -p \
    "$dir/scripts" \
    "$dir/plugins/dvandva/hooks" \
    "$dir/plugins/dvandva/scripts"
  cp "$INSTALLER" "$dir/scripts/install-dvandva-hooks.sh"
  cp "$ROOT_DIR/plugins/dvandva/hooks/pre-commit" \
    "$dir/plugins/dvandva/hooks/pre-commit"
  cp "$ROOT_DIR/plugins/dvandva/hooks/prepare-commit-msg" \
    "$dir/plugins/dvandva/hooks/prepare-commit-msg"
  cp "$ROOT_DIR/plugins/dvandva/hooks/dvandva-hook-lib.sh" \
    "$dir/plugins/dvandva/hooks/dvandva-hook-lib.sh"
  cp "$ROOT_DIR/plugins/dvandva/scripts/dvandva-commit-gate.sh" \
    "$dir/plugins/dvandva/scripts/dvandva-commit-gate.sh"
  cp "$ROOT_DIR/plugins/dvandva/scripts/dvandva-drift-lint.sh" \
    "$dir/plugins/dvandva/scripts/dvandva-drift-lint.sh"
}

check_msg() {
  local name="$1" expected_exit="$2" actual_exit="$3" output="$4" expected_text="$5"
  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    echo "FAIL: $name — expected exit $expected_exit, got $actual_exit"
    [[ -n "$output" ]] && echo "  output: $output"
    failures=$((failures + 1))
    return 1
  fi
  if [[ "$output" != *"$expected_text"* ]]; then
    echo "FAIL: $name — missing text: $expected_text"
    echo "  got: $output"
    failures=$((failures + 1))
    return 1
  fi
  echo "PASS: $name"
}

check_value() {
  local name="$1" expected="$2" actual="$3"
  if [[ "$actual" != "$expected" ]]; then
    echo "FAIL: $name — expected '$expected', got '$actual'"
    failures=$((failures + 1))
    return 1
  fi
  echo "PASS: $name"
}

check_absent() {
  local name="$1" actual="$2"
  if [[ -n "$actual" ]]; then
    echo "FAIL: $name — expected empty value, got '$actual'"
    failures=$((failures + 1))
    return 1
  fi
  echo "PASS: $name"
}

if cmp -s "$VADI_HELPER" "$PRATIVADI_HELPER"; then
  echo "PASS: vadi and prativadi hook preflight helpers are byte-identical"
else
  echo "FAIL: vadi and prativadi hook preflight helpers must be byte-identical"
  failures=$((failures + 1))
fi

BOX="$TMP_DIR/foreign-auto"
new_husky_repo "$BOX"
stage_hook_sources "$BOX"
out="$(DVANDVA_ROLE=prativadi bash "$HELPER" --role prativadi --repo "$BOX" 2>&1)"; rc=$?
check_msg "auto mode succeeds in foreign-hook repo" 0 "$rc" "$out" "DVANDVA_HOOK_PREFLIGHT"
check_msg "auto mode reports ok result" 0 "$rc" "$out" "result=ok"
check_msg "auto mode reports probe sentinel" 0 "$rc" "$out" "sentinel=DVANDVA_GATE_WIRED"
current="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
check_value "auto mode repoints core.hooksPath to delegated wrapper" ".dvandva/githooks" "$current"
prior="$(git -C "$BOX" config --local dvandva.priorHooksPath 2>/dev/null || echo "")"
check_value "auto mode records prior foreign hooksPath" ".husky/_" "$prior"
probe="$(cd "$BOX" && DVANDVA_HOOK_SELFCHECK=1 .dvandva/githooks/pre-commit 2>&1)"; probe_rc=$?
check_msg "active pre-commit selfcheck stays reachable" 0 "$probe_rc" "$probe" "DVANDVA_GATE_WIRED"

BOX="$TMP_DIR/off-mode"
new_git_repo "$BOX"
stage_hook_sources "$BOX"
out="$(DVANDVA_ROLE=vadi bash "$HELPER" --role vadi --repo "$BOX" --mode off 2>&1)"; rc=$?
check_msg "off mode exits 0" 0 "$rc" "$out" "mode=off"
current="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
check_absent "off mode leaves core.hooksPath unset" "$current"

BOX="$TMP_DIR/role-mismatch"
new_git_repo "$BOX"
stage_hook_sources "$BOX"
out="$(DVANDVA_ROLE=vadi bash "$HELPER" --role prativadi --repo "$BOX" 2>&1)"; rc=$?
check_msg "role mismatch exits 1" 1 "$rc" "$out" "role_mismatch"

BOX="$TMP_DIR/broken-chain"
new_git_repo "$BOX"
stage_hook_sources "$BOX"
cat > "$BOX/plugins/dvandva/hooks/pre-commit" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$BOX/plugins/dvandva/hooks/pre-commit"
out="$(DVANDVA_ROLE=vadi bash "$HELPER" --role vadi --repo "$BOX" 2>&1)"; rc=$?
check_msg "broken chain exits 1" 1 "$rc" "$out" "result=error"
check_msg "broken chain reports a reason" 1 "$rc" "$out" "reason="

echo ""
if [[ "$failures" -eq 0 ]]; then
  echo "All tests passed."
  exit 0
else
  echo "$failures test(s) failed."
  exit 1
fi
