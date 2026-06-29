#!/usr/bin/env bash
# Tests for the unified Dvandva turn preflight helper.
#
# The turn preflight must resolve the active baton first, stop on ASK, and only
# invoke hook preflight once a baton is RESOLVED.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VADI_PREFLIGHT="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-preflight.sh"
PRATIVADI_PREFLIGHT="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-preflight.sh"
VADI_HOOK="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-hook-preflight.sh"
PRATIVADI_HOOK="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-hook-preflight.sh"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

failures=0

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

stage_stubbed_runtime() {
  local dir="$1" runtime="$2" resolve_body="$3" hook_body="$4"
  mkdir -p "$dir"
  cp "$runtime" "$dir/dvandva-preflight.sh"
  cat > "$dir/dvandva-resolve.sh" <<EOF
#!/usr/bin/env bash
set -u
$resolve_body
EOF
  cat > "$dir/dvandva-hook-preflight.sh" <<EOF
#!/usr/bin/env bash
set -u
$hook_body
EOF
  chmod +x "$dir/dvandva-preflight.sh" "$dir/dvandva-resolve.sh" "$dir/dvandva-hook-preflight.sh"
}

if cmp -s "$VADI_PREFLIGHT" "$PRATIVADI_PREFLIGHT"; then
  echo "PASS: vadi and prativadi turn preflight helpers are byte-identical"
else
  echo "FAIL: vadi and prativadi turn preflight helpers must be byte-identical"
  failures=$((failures + 1))
fi

if cmp -s "$VADI_HOOK" "$PRATIVADI_HOOK"; then
  echo "PASS: vadi and prativadi hook-stage helpers are byte-identical"
else
  echo "FAIL: vadi and prativadi hook-stage helpers must be byte-identical"
  failures=$((failures + 1))
fi

BOX="$TMP_DIR/resolved"
mkdir -p "$BOX/repo"
stage_stubbed_runtime \
  "$BOX/runtime" \
  "$VADI_PREFLIGHT" \
  'printf "RESOLVED .dvandva/runs/accuracy/baton.json\n"' \
  'echo "HOOK_CALLED role=$2"'
out="$(cd "$BOX/repo" && DVANDVA_ROLE=vadi DVANDVA_RUN_ID=accuracy bash "$BOX/runtime/dvandva-preflight.sh" --role vadi 2>&1)"; rc=$?
check_msg "resolved path exits 0" 0 "$rc" "$out" "DVANDVA_PREFLIGHT"
check_msg "resolved path prints canonical baton path" 0 "$rc" "$out" "baton=$BOX/repo/.dvandva/runs/accuracy/baton.json"
check_msg "resolved path prints run id" 0 "$rc" "$out" "run_id=accuracy"
check_msg "resolved path prints selector source" 0 "$rc" "$out" "selected_by=DVANDVA_RUN_ID"
check_msg "resolved path runs hook stage" 0 "$rc" "$out" "HOOK_CALLED role=vadi"

BOX="$TMP_DIR/ask"
mkdir -p "$BOX/repo"
stage_stubbed_runtime \
  "$BOX/runtime" \
  "$VADI_PREFLIGHT" \
  'printf "ASK [{\"run_id\":\"a\"}]\n"; exit 12' \
  'echo "HOOK_CALLED" > "$PWD/hook-called.txt"'
out="$(cd "$BOX/repo" && DVANDVA_ROLE=vadi bash "$BOX/runtime/dvandva-preflight.sh" --role vadi 2>&1)"; rc=$?
check_msg "ask exits 12" 12 "$rc" "$out" "result=ask"
if [[ -f "$BOX/repo/hook-called.txt" ]]; then
  echo "FAIL: ask must not run hook stage"
  failures=$((failures + 1))
else
  echo "PASS: ask does not run hook stage"
fi

BOX="$TMP_DIR/create"
mkdir -p "$BOX/repo"
stage_stubbed_runtime \
  "$BOX/runtime" \
  "$VADI_PREFLIGHT" \
  'printf "CREATE .dvandva/runs/run-2/baton.json\n"' \
  'echo "HOOK_CALLED" > "$PWD/hook-called.txt"'
out="$(cd "$BOX/repo" && DVANDVA_ROLE=vadi bash "$BOX/runtime/dvandva-preflight.sh" --role vadi 2>&1)"; rc=$?
check_msg "create exits 0" 0 "$rc" "$out" "result=create"
check_msg "create prints scaffold path" 0 "$rc" "$out" "scaffold=$BOX/repo/.dvandva/runs/run-2/baton.json"
if [[ -f "$BOX/repo/hook-called.txt" ]]; then
  echo "FAIL: create must not run hook stage"
  failures=$((failures + 1))
else
  echo "PASS: create does not run hook stage"
fi

BOX="$TMP_DIR/role-mismatch"
mkdir -p "$BOX/repo"
stage_stubbed_runtime \
  "$BOX/runtime" \
  "$VADI_PREFLIGHT" \
  'printf "RESOLVED .dvandva/runs/accuracy/baton.json\n"' \
  'echo "HOOK_CALLED" > "$PWD/hook-called.txt"'
out="$(cd "$BOX/repo" && DVANDVA_ROLE=vadi bash "$BOX/runtime/dvandva-preflight.sh" --role prativadi 2>&1)"; rc=$?
check_msg "role mismatch exits 1" 1 "$rc" "$out" "role_mismatch"
if [[ -f "$BOX/repo/hook-called.txt" ]]; then
  echo "FAIL: role mismatch must not run hook stage"
  failures=$((failures + 1))
else
  echo "PASS: role mismatch does not run hook stage"
fi

echo ""
if [[ "$failures" -eq 0 ]]; then
  echo "All tests passed."
  exit 0
else
  echo "$failures test(s) failed."
  exit 1
fi
