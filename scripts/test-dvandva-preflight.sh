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

# cr-c2-ask-stderr: ASK on a corrupt baton must surface the resolver's stderr so
# the operator can see WHY choices are ambiguous (e.g. a parse error).
BOX="$TMP_DIR/ask-with-stderr"
mkdir -p "$BOX/repo"
stage_stubbed_runtime \
  "$BOX/runtime" \
  "$VADI_PREFLIGHT" \
  'printf "ASK [{\"run_id\":\"a\"}]\n"; printf "reason: baton json unparseable at key=status\n" >&2; exit 12' \
  'echo "HOOK_CALLED" > "$PWD/hook-called.txt"'
out="$(cd "$BOX/repo" && DVANDVA_ROLE=vadi bash "$BOX/runtime/dvandva-preflight.sh" --role vadi 2>&1)"; rc=$?
check_msg "ask-with-stderr exits 12" 12 "$rc" "$out" "result=ask"
check_msg "ask-with-stderr surfaces resolver reason" 12 "$rc" "$out" "baton json unparseable"

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

# ---------------------------------------------------------------------------
# Real integration (addresses cr-c2-stub-tests): exercise the REAL orchestrator
# + REAL resolver + REAL hook stage + REAL installer against a target repo that
# has NO committed scripts/.  Because the only Dvandva source is the in-repo
# plugin tree the orchestrator lives in, RESOLVED must drive a plugin-installer
# adoption (the plugin-only enforcement case), while ASK/CREATE must never run
# the hook stage.
# ---------------------------------------------------------------------------
box_cfg_get() {
  local box="$1" key="$2" v
  if [[ "$(git -C "$box" config --bool extensions.worktreeConfig 2>/dev/null || echo false)" == "true" ]] \
     && v="$(git -C "$box" config --worktree --get "$key" 2>/dev/null)"; then
    printf '%s' "$v"
    return 0
  fi
  git -C "$box" config --local --get "$key" 2>/dev/null || true
}

new_target_repo() {
  local dir="$1"
  mkdir -p "$dir"
  git -C "$dir" init --quiet
  git -C "$dir" config user.email "test@dvandva.test"
  git -C "$dir" config user.name "Dvandva Test"
  touch "$dir/.gitkeep"
  git -C "$dir" add .gitkeep
  git -C "$dir" commit --quiet -m "initial"
}

write_baton() {
  local path="$1" run_id="$2" status="$3"
  mkdir -p "$(dirname "$path")"
  cat > "$path" <<EOF
{"run_id":"$run_id","status":"$status","assignee":"vadi","updated_at":"2026-06-29T00:00:00Z"}
EOF
}

# RESOLVED -> runs the hook stage and adopts via the PLUGIN installer (the target
# repo has no scripts/).  Old installer resolution would fail with
# missing_installer here.
BOX="$TMP_DIR/real-resolved"
new_target_repo "$BOX"
write_baton "$BOX/.dvandva/runs/accuracy/baton.json" accuracy in_progress
out="$(cd "$BOX" && DVANDVA_ROLE=vadi bash "$VADI_PREFLIGHT" --role vadi 2>&1)"; rc=$?
check_msg "real RESOLVED exits 0" 0 "$rc" "$out" "result=resolved"
check_msg "real RESOLVED runs hook stage to ok" 0 "$rc" "$out" "result=ok"
if [[ "$out" == *"missing_installer"* ]]; then
  echo "FAIL: real RESOLVED must resolve the plugin installer (got missing_installer)"
  echo "  got: $out"
  failures=$((failures + 1))
else
  echo "PASS: real RESOLVED resolves the plugin installer (no missing_installer)"
fi
current="$(box_cfg_get "$BOX" core.hooksPath)"
if [[ "$current" == ".dvandva/githooks" ]]; then
  echo "PASS: real RESOLVED adopts the delegated wrapper"
else
  echo "FAIL: real RESOLVED adopts the delegated wrapper — got '$current'"
  failures=$((failures + 1))
fi

# ASK -> never runs the hook stage; no adoption side effect.
BOX="$TMP_DIR/real-ask"
new_target_repo "$BOX"
write_baton "$BOX/.dvandva/runs/aa/baton.json" aa in_progress
write_baton "$BOX/.dvandva/runs/bb/baton.json" bb in_progress
out="$(cd "$BOX" && DVANDVA_ROLE=vadi bash "$VADI_PREFLIGHT" --role vadi 2>&1)"; rc=$?
check_msg "real ASK exits 12" 12 "$rc" "$out" "result=ask"
if [[ -d "$BOX/.dvandva/githooks" ]]; then
  echo "FAIL: real ASK must not run the hook stage (found .dvandva/githooks)"
  failures=$((failures + 1))
else
  echo "PASS: real ASK does not run the hook stage"
fi

# CREATE -> never runs the hook stage; no adoption side effect.
BOX="$TMP_DIR/real-create"
new_target_repo "$BOX"
out="$(cd "$BOX" && DVANDVA_ROLE=vadi bash "$VADI_PREFLIGHT" --role vadi 2>&1)"; rc=$?
check_msg "real CREATE exits 0" 0 "$rc" "$out" "result=create"
if [[ -d "$BOX/.dvandva/githooks" ]]; then
  echo "FAIL: real CREATE must not run the hook stage (found .dvandva/githooks)"
  failures=$((failures + 1))
else
  echo "PASS: real CREATE does not run the hook stage"
fi

echo ""
if [[ "$failures" -eq 0 ]]; then
  echo "All tests passed."
  exit 0
else
  echo "$failures test(s) failed."
  exit 1
fi
