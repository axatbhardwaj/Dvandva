#!/usr/bin/env bash
# Tests for the Dvandva Git commit gate, hook installer, and drift lint.
# Creates throwaway git repos under a temp dir and asserts exit codes + messages.
#
# Cases:
#  (a) no .dvandva dir       → gate exits 0  (no-op)
#  (b) active + match role   → commit allowed + Dvandva-Checkpoint trailer stamped
#  (c) active + wrong role   → gate exits 1 "DVANDVA_GATE blocked"
#  (d) active + role unset   → gate exits 1 "DVANDVA_ROLE is unset"
#  (e) terminal done baton   → gate exits 0  (no-op)
#  (f) two active batons     → gate exits 1 "ambiguous"
#  (g) team active_roles     → gate exits 0 for vadi and prativadi
#  (h) installer idempotency + foreign hooksPath refusal
#  (i) drift lint flags unstamped commits
#  (j) --no-verify bypass is visible: commit succeeds without trailer
#      and drift-lint catches the first active-baton bypass commit
#  (k) --no-verify cannot stamp an arbitrary checkpoint under multi-run ambiguity
#  (l) empty git repo has no drift
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GATE="$ROOT_DIR/scripts/dvandva-commit-gate.sh"
INSTALLER="$ROOT_DIR/scripts/install-dvandva-hooks.sh"
DRIFT_LINT="$ROOT_DIR/scripts/dvandva-drift-lint.sh"
PREPARE_HOOK="$ROOT_DIR/.githooks/prepare-commit-msg"

TMP_DIR="$(mktemp -d)"
cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT

failures=0

# ---------------------------------------------------------------------------
# Helper: create a temp git repo with an initial commit
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# Helper: write a minimal baton JSON (gate only needs status/assignee/checkpoint/active_roles)
# ---------------------------------------------------------------------------
make_gate_baton() {
  local file="$1" status="$2" assignee="$3"
  local checkpoint="${4:-5}"
  local active_roles="${5:-[]}"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v1",
  "status": "$status",
  "assignee": "$assignee",
  "checkpoint": $checkpoint,
  "active_roles": $active_roles
}
JSON
}

# ---------------------------------------------------------------------------
# Assertion helpers
# ---------------------------------------------------------------------------
check() {
  local name="$1" expected="$2" actual="$3" output="$4"
  if [[ "$actual" -ne "$expected" ]]; then
    echo "FAIL: $name — expected exit $expected, got $actual"
    [[ -n "$output" ]] && echo "  output: $output"
    failures=$((failures + 1))
    return 1
  fi
  echo "PASS: $name"
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

# ===========================================================================
# (a) No .dvandva directory → gate exits 0 (no-op)
# ===========================================================================
BOX="$TMP_DIR/a-no-dvandva"
new_git_repo "$BOX"
out="$(cd "$BOX" && "$GATE" 2>&1)"; rc=$?
check "(a) no .dvandva: gate exits 0" 0 "$rc" "$out"

# ===========================================================================
# (b) Active baton + DVANDVA_ROLE matches assignee
#     → commit allowed + Dvandva-Checkpoint trailer stamped
# ===========================================================================
BOX="$TMP_DIR/b-active-match"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "implementing" "vadi" 7

# Install hooks that delegate to our actual scripts
mkdir -p "$BOX/.githooks"
# pre-commit: call the gate
cat > "$BOX/.githooks/pre-commit" <<HOOK
#!/usr/bin/env bash
exec "${GATE}"
HOOK
# prepare-commit-msg: call our prepare-commit-msg hook
cat > "$BOX/.githooks/prepare-commit-msg" <<HOOK
#!/usr/bin/env bash
exec "${PREPARE_HOOK}" "\$@"
HOOK
chmod +x "$BOX/.githooks/pre-commit" "$BOX/.githooks/prepare-commit-msg"
git -C "$BOX" config core.hooksPath ".githooks"

touch "$BOX/file.txt"
git -C "$BOX" add file.txt
out="$(DVANDVA_ROLE=vadi git -C "$BOX" commit -m "vadi commit" 2>&1)"; rc=$?
check "(b) matching role: commit allowed" 0 "$rc" "$out"
if [[ $rc -eq 0 ]]; then
  commit_body="$(git -C "$BOX" show -s --format="%B" HEAD)"
  if echo "$commit_body" | grep -qE "^Dvandva-Checkpoint: 7$"; then
    echo "PASS: (b) Dvandva-Checkpoint: 7 trailer present"
  else
    echo "FAIL: (b) Dvandva-Checkpoint: 7 trailer not found"
    echo "  commit body: $commit_body"
    failures=$((failures + 1))
  fi
fi

# ===========================================================================
# (c) Active baton + wrong role → gate exits 1 with blocked message
# ===========================================================================
BOX="$TMP_DIR/c-wrong-role"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "implementing" "vadi" 5
out="$(cd "$BOX" && DVANDVA_ROLE=prativadi "$GATE" 2>&1)"; rc=$?
check_msg "(c) wrong role: gate exits 1" 1 "$rc" "$out" "DVANDVA_GATE blocked"
check_msg "(c) wrong role: names assignee" 1 "$rc" "$out" "assignee=vadi"

# ===========================================================================
# (d) DVANDVA_ROLE unset + active baton → gate exits 1
# ===========================================================================
BOX="$TMP_DIR/d-no-role"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "implementing" "vadi" 5
out="$(cd "$BOX" && (unset DVANDVA_ROLE; "$GATE") 2>&1)"; rc=$?
check_msg "(d) unset DVANDVA_ROLE: gate exits 1" 1 "$rc" "$out" "DVANDVA_ROLE is unset"
check_msg "(d) unset DVANDVA_ROLE: names checkpoint" 1 "$rc" "$out" "checkpoint=5"

# ===========================================================================
# (e) Terminal done baton → gate exits 0 (no-op)
# ===========================================================================
BOX="$TMP_DIR/e-done"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "done" "human" 10
out="$(cd "$BOX" && DVANDVA_ROLE=vadi "$GATE" 2>&1)"; rc=$?
check "(e) terminal done baton: gate exits 0" 0 "$rc" "$out"

BOX="$TMP_DIR/e-human-question"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "human_question" "human" 3
out="$(cd "$BOX" && DVANDVA_ROLE=vadi "$GATE" 2>&1)"; rc=$?
check "(e) terminal human_question baton: gate exits 0" 0 "$rc" "$out"

BOX="$TMP_DIR/e-human-decision"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "human_decision" "human" 4
out="$(cd "$BOX" && DVANDVA_ROLE=prativadi "$GATE" 2>&1)"; rc=$?
check "(e) terminal human_decision baton: gate exits 0" 0 "$rc" "$out"

# ===========================================================================
# (f) Two active batons → gate exits 1 (ambiguous)
# ===========================================================================
BOX="$TMP_DIR/f-two-active"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "implementing" "vadi" 5
mkdir -p "$BOX/.dvandva/runs/run-a"
make_gate_baton "$BOX/.dvandva/runs/run-a/baton.json" "spec_drafting" "vadi" 0
out="$(cd "$BOX" && DVANDVA_ROLE=vadi "$GATE" 2>&1)"; rc=$?
check_msg "(f) two active batons: gate exits 1" 1 "$rc" "$out" "ambiguous"
check_msg "(f) two active batons: reports count" 1 "$rc" "$out" "2 active batons"

# Malformed baton JSON must fail closed.  Silently skipping it would turn a
# broken active-run gate into "no active baton found" and allow the commit.
BOX="$TMP_DIR/f-malformed-legacy"
new_git_repo "$BOX"
mkdir -p "$BOX/.dvandva"
printf '{ bad json\n' > "$BOX/.dvandva/baton.json"
out="$(cd "$BOX" && DVANDVA_ROLE=vadi "$GATE" 2>&1)"; rc=$?
check_msg "(f) malformed legacy baton: gate fails closed" 1 "$rc" "$out" "malformed baton"

BOX="$TMP_DIR/f-malformed-run-scoped"
new_git_repo "$BOX"
mkdir -p "$BOX/.dvandva/runs/run-bad"
printf '{ bad json\n' > "$BOX/.dvandva/runs/run-bad/baton.json"
out="$(cd "$BOX" && DVANDVA_ROLE=vadi "$GATE" 2>&1)"; rc=$?
check_msg "(f) malformed run-scoped baton: gate fails closed" 1 "$rc" "$out" "malformed baton"

# Missing jq must also fail closed when baton candidates exist.  Build a tiny
# PATH that has git/find but deliberately omits jq.
BOX="$TMP_DIR/f-missing-jq"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "implementing" "vadi" 6
NO_JQ_BIN="$TMP_DIR/no-jq-bin"
mkdir -p "$NO_JQ_BIN"
ln -s "$(command -v git)" "$NO_JQ_BIN/git"
ln -s "$(command -v find)" "$NO_JQ_BIN/find"
ln -s "$(command -v env)" "$NO_JQ_BIN/env"
ln -s "$(command -v bash)" "$NO_JQ_BIN/bash"
out="$(cd "$BOX" && PATH="$NO_JQ_BIN" DVANDVA_ROLE=vadi "$GATE" 2>&1)"; rc=$?
check_msg "(f) missing jq: gate fails closed" 1 "$rc" "$out" "jq is required"

# ===========================================================================
# (g) Team state (active_roles contains the role) → gate exits 0
# ===========================================================================
BOX="$TMP_DIR/g-team-vadi"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "parallel_implementing" "team" 8 '["vadi","prativadi"]'
out="$(cd "$BOX" && DVANDVA_ROLE=vadi "$GATE" 2>&1)"; rc=$?
check "(g) team state: vadi in active_roles → exits 0" 0 "$rc" "$out"

BOX="$TMP_DIR/g-team-prativadi"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "cross_review" "team" 9 '["vadi","prativadi"]'
out="$(cd "$BOX" && DVANDVA_ROLE=prativadi "$GATE" 2>&1)"; rc=$?
check "(g) team state: prativadi in active_roles → exits 0" 0 "$rc" "$out"

# A role NOT in active_roles should still be blocked if it's not the assignee
BOX="$TMP_DIR/g-run-scoped-only"
new_git_repo "$BOX"
mkdir -p "$BOX/.dvandva/runs/run-b"
make_gate_baton "$BOX/.dvandva/runs/run-b/baton.json" "cross_fixing" "team" 12 '["vadi","prativadi"]'
out="$(cd "$BOX" && DVANDVA_ROLE=vadi "$GATE" 2>&1)"; rc=$?
check "(g) run-scoped team baton: vadi in active_roles → exits 0" 0 "$rc" "$out"

BOX="$TMP_DIR/g-run-scoped-scalar"
new_git_repo "$BOX"
mkdir -p "$BOX/.dvandva/runs/run-c"
make_gate_baton "$BOX/.dvandva/runs/run-c/baton.json" "phase_fixing" "vadi" 13
out="$(cd "$BOX" && DVANDVA_ROLE=vadi "$GATE" 2>&1)"; rc=$?
check "(g) run-scoped scalar baton: matching assignee exits 0" 0 "$rc" "$out"
out="$(cd "$BOX" && DVANDVA_ROLE=prativadi "$GATE" 2>&1)"; rc=$?
check_msg "(g) run-scoped scalar baton: wrong role exits 1" 1 "$rc" "$out" "assignee=vadi"

# ===========================================================================
# (h) Installer: idempotency + foreign hooksPath refusal
# ===========================================================================
BOX="$TMP_DIR/h-installer"
new_git_repo "$BOX"

out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(h) fresh install: exits 0" 0 "$rc" "$out"
current="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
if [[ "$current" == ".githooks" ]]; then
  echo "PASS: (h) fresh install: core.hooksPath=.githooks"
else
  echo "FAIL: (h) fresh install: expected .githooks, got '$current'"
  failures=$((failures + 1))
fi

out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check_msg "(h) second install is idempotent: exits 0" 0 "$rc" "$out" "already installed"

# Set a foreign hooksPath and verify refusal
git -C "$BOX" config core.hooksPath ".custom-hooks"
out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check_msg "(h) foreign hooksPath refused without --force: exits 1" 1 "$rc" "$out" "already set to"

# --force overrides the foreign path
out="$(cd "$BOX" && "$INSTALLER" --force 2>&1)"; rc=$?
check "(h) --force overrides foreign hooksPath: exits 0" 0 "$rc" "$out"
current="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
if [[ "$current" == ".githooks" ]]; then
  echo "PASS: (h) --force: core.hooksPath now .githooks"
else
  echo "FAIL: (h) --force: expected .githooks, got '$current'"
  failures=$((failures + 1))
fi

# --uninstall clears the hooksPath
out="$(cd "$BOX" && "$INSTALLER" --uninstall 2>&1)"; rc=$?
check "(h) --uninstall: exits 0" 0 "$rc" "$out"
current="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
if [[ -z "$current" ]]; then
  echo "PASS: (h) --uninstall: core.hooksPath cleared"
else
  echo "FAIL: (h) --uninstall: core.hooksPath still set to '$current'"
  failures=$((failures + 1))
fi

# --uninstall again is safe (no-op)
out="$(cd "$BOX" && "$INSTALLER" --uninstall 2>&1)"; rc=$?
check "(h) second --uninstall: exits 0" 0 "$rc" "$out"

# Verify hook is NOT set globally (no --global change)
global_hooks="$(git config --global core.hooksPath 2>/dev/null || echo "")"
if [[ "$global_hooks" == ".githooks" ]]; then
  # If the test is run in a repo that already has .githooks globally, we
  # cannot distinguish; just warn instead of failing.
  echo "WARN: (h) global core.hooksPath is already .githooks — skipping global-isolation check"
else
  echo "PASS: (h) installer did not modify global core.hooksPath"
fi

# ===========================================================================
# (i) Drift lint flags unstamped commits (off-protocol drift)
# ===========================================================================
BOX="$TMP_DIR/i-drift"
new_git_repo "$BOX"

# Commit 1 (stamped): checkpoint 3
touch "$BOX/file1.txt"
git -C "$BOX" add file1.txt
git -C "$BOX" commit --quiet -m "$(printf 'feat: stamped commit\n\nDvandva-Checkpoint: 3')"

# Commit 2 (unstamped): off-protocol
touch "$BOX/file2.txt"
git -C "$BOX" add file2.txt
git -C "$BOX" commit --quiet -m "fix: off-protocol commit without trailer"

out="$(cd "$BOX" && "$DRIFT_LINT" 2>&1)"; rc=$?
check_msg "(i) drift lint exits 1 on unstamped commits" 1 "$rc" "$out" "DVANDVA_DRIFT warning"
check_msg "(i) drift lint names off-protocol commit" 1 "$rc" "$out" "off-protocol"

# --warn mode: advisory, exits 0
out="$(cd "$BOX" && "$DRIFT_LINT" --warn 2>&1)"; rc=$?
check "(i) drift lint --warn exits 0" 0 "$rc" "$out"
check_msg "(i) drift lint --warn still prints DVANDVA_DRIFT" 0 "$rc" "$out" "DVANDVA_DRIFT"

# Commit 3 (stamped): checkpoint 4 — should clear the drift
touch "$BOX/file3.txt"
git -C "$BOX" add file3.txt
git -C "$BOX" commit --quiet -m "$(printf 'feat: second stamped commit\n\nDvandva-Checkpoint: 4')"

out="$(cd "$BOX" && "$DRIFT_LINT" 2>&1)"; rc=$?
check "(i) drift lint exits 0 after stamped commit" 0 "$rc" "$out"

# Repo with no checkpointed commits at all → no drift to report
BOX2="$TMP_DIR/i-no-checkpoints"
new_git_repo "$BOX2"
touch "$BOX2/file.txt"
git -C "$BOX2" add file.txt
git -C "$BOX2" commit --quiet -m "plain commit without trailer"
out="$(cd "$BOX2" && "$DRIFT_LINT" 2>&1)"; rc=$?
check "(i) drift lint exits 0 when no checkpoints exist" 0 "$rc" "$out"

# ===========================================================================
# (j) --no-verify bypass: Git bypasses pre-commit, so the gate cannot block.
#     With DVANDVA_ROLE unset, prepare-commit-msg also cannot stamp the trailer.
#     This documents the limitation; drift-lint is the backstop.
# ===========================================================================
BOX="$TMP_DIR/j-no-verify-bypass"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "implementing" "vadi" 14
mkdir -p "$BOX/.githooks"
cat > "$BOX/.githooks/pre-commit" <<HOOK
#!/usr/bin/env bash
exec "${GATE}"
HOOK
cat > "$BOX/.githooks/prepare-commit-msg" <<HOOK
#!/usr/bin/env bash
exec "${PREPARE_HOOK}" "\$@"
HOOK
chmod +x "$BOX/.githooks/pre-commit" "$BOX/.githooks/prepare-commit-msg"
git -C "$BOX" config core.hooksPath ".githooks"

touch "$BOX/noverify.txt"
git -C "$BOX" add noverify.txt
out="$(cd "$BOX" && (unset DVANDVA_ROLE; git commit --no-verify -m "bypass without role") 2>&1)"; rc=$?
check "(j) --no-verify bypasses commit gate without DVANDVA_ROLE" 0 "$rc" "$out"
if [[ "$rc" -eq 0 ]]; then
  commit_body="$(git -C "$BOX" show -s --format="%B" HEAD)"
  if echo "$commit_body" | grep -qE "^Dvandva-Checkpoint:"; then
    echo "FAIL: (j) --no-verify commit unexpectedly has Dvandva-Checkpoint trailer"
    echo "  commit body: $commit_body"
    failures=$((failures + 1))
  else
    echo "PASS: (j) --no-verify commit has no Dvandva-Checkpoint trailer"
  fi
fi

out="$(cd "$BOX" && "$DRIFT_LINT" 2>&1)"; rc=$?
check_msg "(j) drift lint flags first active-baton bypass commit" 1 "$rc" "$out" "DVANDVA_DRIFT warning"
check_msg "(j) drift lint names first bypass commit" 1 "$rc" "$out" "bypass without role"

# ===========================================================================
# (k) --no-verify should not let prepare-commit-msg stamp an arbitrary
#     checkpoint when multiple run-scoped batons are active.
# ===========================================================================
BOX="$TMP_DIR/k-prepare-ambiguous"
new_git_repo "$BOX"
mkdir -p "$BOX/.dvandva/runs/run-a" "$BOX/.dvandva/runs/run-b"
make_gate_baton "$BOX/.dvandva/runs/run-a/baton.json" "implementing" "vadi" 21
make_gate_baton "$BOX/.dvandva/runs/run-b/baton.json" "spec_drafting" "vadi" 22
mkdir -p "$BOX/.githooks"
cat > "$BOX/.githooks/pre-commit" <<HOOK
#!/usr/bin/env bash
exec "${GATE}"
HOOK
cat > "$BOX/.githooks/prepare-commit-msg" <<HOOK
#!/usr/bin/env bash
exec "${PREPARE_HOOK}" "\$@"
HOOK
chmod +x "$BOX/.githooks/pre-commit" "$BOX/.githooks/prepare-commit-msg"
git -C "$BOX" config core.hooksPath ".githooks"

touch "$BOX/ambiguous.txt"
git -C "$BOX" add ambiguous.txt
out="$(DVANDVA_ROLE=vadi git -C "$BOX" commit --no-verify -m "ambiguous no-verify" 2>&1)"; rc=$?
check_msg "(k) prepare hook blocks ambiguous --no-verify commit" 1 "$rc" "$out" "ambiguous active runs"
latest_subject="$(git -C "$BOX" log -1 --format=%s)"
if [[ "$latest_subject" == "initial" ]]; then
  echo "PASS: (k) ambiguous --no-verify did not create a commit"
else
  echo "FAIL: (k) ambiguous --no-verify created a commit: $latest_subject"
  failures=$((failures + 1))
fi

# ===========================================================================
# (l) Drift lint handles an empty git repo with no commits.
# ===========================================================================
BOX="$TMP_DIR/l-empty-repo"
mkdir -p "$BOX"
git -C "$BOX" init --quiet
out="$(cd "$BOX" && "$DRIFT_LINT" 2>&1)"; rc=$?
check "(l) drift lint exits 0 in empty git repo" 0 "$rc" "$out"
check_msg "(l) empty git repo reports no checkpoint history" 0 "$rc" "$out" "no checkpointed commits"

# ===========================================================================
# Summary
# ===========================================================================
echo ""
if [[ "$failures" -eq 0 ]]; then
  echo "All tests passed."
  exit 0
else
  echo "$failures test(s) failed."
  exit 1
fi
