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
#  (i) drift lint flags unstamped commits and honors hook-adoption baseline
#      across a stamp -> no-verify -> stamp sandwich
#  (j) --no-verify bypass is visible: commit succeeds without trailer
#      and drift-lint catches the first active-baton bypass commit
#  (k) --no-verify cannot stamp an arbitrary checkpoint under multi-run ambiguity
#  (l) empty git repo has no drift
#  (m) hook adoption in an unborn repo is backfilled to the root commit
#      and still catches an unstamped root commit when an active baton exists
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GATE="$ROOT_DIR/scripts/dvandva-commit-gate.sh"
INSTALLER="$ROOT_DIR/scripts/install-dvandva-hooks.sh"
DRIFT_LINT="$ROOT_DIR/scripts/dvandva-drift-lint.sh"
PREPARE_HOOK="$ROOT_DIR/.githooks/prepare-commit-msg"

# Plugin-shipped sources (C1: delegating, plugin-shipped, reversible work-gate).
PLUGIN_DIR="$ROOT_DIR/plugins/dvandva"
PLUGIN_INSTALLER="$PLUGIN_DIR/scripts/install-dvandva-hooks.sh"
PLUGIN_PRECOMMIT="$PLUGIN_DIR/hooks/pre-commit"
PLUGIN_PREPARE="$PLUGIN_DIR/hooks/prepare-commit-msg"

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
# Helper: create a temp git repo that already owns its hooks via a foreign
# hooksPath (simulating Husky v9: core.hooksPath=.husky/_).  Each foreign hook
# appends a distinct marker line to <repo>/hook.log so delegation is provable.
# .dvandva/ and hook.log are gitignored so installs produce zero tracked diff.
# ---------------------------------------------------------------------------
new_husky_repo() {
  local dir="$1"
  mkdir -p "$dir/.husky/_"
  git -C "$dir" init --quiet
  git -C "$dir" config user.email "test@dvandva.test"
  git -C "$dir" config user.name "Dvandva Test"
  printf '.dvandva/\n/hook.log\n' > "$dir/.gitignore"
  local name
  for name in pre-commit commit-msg pre-push; do
    cat > "$dir/.husky/_/$name" <<EOF
#!/usr/bin/env bash
echo "HUSKY_${name}_FIRED args=[\$*]" >> "$dir/hook.log"
exit 0
EOF
    chmod +x "$dir/.husky/_/$name"
  done
  git -C "$dir" add .gitignore .husky
  git -C "$dir" commit --quiet -m "husky setup"
  git -C "$dir" config core.hooksPath ".husky/_"
}

# Count how many times a substring appears across a file (0 if file absent).
# grep -c prints the count to stdout even on 0 matches (exiting 1), so capture
# it and normalize — never chain `|| echo 0` (that would double-print "0").
count_in_file() {
  local needle="$1" file="$2" n
  [[ -f "$file" ]] || { echo 0; return; }
  n="$(grep -c -- "$needle" "$file" 2>/dev/null)" || n=0
  echo "$n"
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
# (h) Delegating installer in a default (unset hooksPath) repo:
#     records the default sentinel, materializes the gitignored hook dir,
#     sets core.hooksPath=.dvandva/githooks, is idempotent and reversible.
# ===========================================================================
BOX="$TMP_DIR/h-installer"
new_git_repo "$BOX"

out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(h) fresh install: exits 0" 0 "$rc" "$out"
current="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
if [[ "$current" == ".dvandva/githooks" ]]; then
  echo "PASS: (h) fresh install: core.hooksPath=.dvandva/githooks"
else
  echo "FAIL: (h) fresh install: expected .dvandva/githooks, got '$current'"
  failures=$((failures + 1))
fi
prior="$(git -C "$BOX" config --local dvandva.priorHooksPath 2>/dev/null || echo "")"
if [[ "$prior" == "__DVANDVA_DEFAULT__" ]]; then
  echo "PASS: (h) default-unset repo records __DVANDVA_DEFAULT__ sentinel"
else
  echo "FAIL: (h) expected prior sentinel __DVANDVA_DEFAULT__, got '$prior'"
  failures=$((failures + 1))
fi
# Materialized, executable hook dir (lib + wrappers + gate + drift-lint).
for f in pre-commit prepare-commit-msg dvandva-hook-lib.sh dvandva-commit-gate.sh dvandva-drift-lint.sh; do
  if [[ -f "$BOX/.dvandva/githooks/$f" ]]; then
    echo "PASS: (h) materialized .dvandva/githooks/$f"
  else
    echo "FAIL: (h) missing materialized .dvandva/githooks/$f"
    failures=$((failures + 1))
  fi
done
if [[ -x "$BOX/.dvandva/githooks/pre-commit" && -x "$BOX/.dvandva/githooks/prepare-commit-msg" ]]; then
  echo "PASS: (h) materialized wrappers are executable"
else
  echo "FAIL: (h) materialized wrappers are not executable"
  failures=$((failures + 1))
fi
adopted_at="$(git -C "$BOX" config --local dvandva.hooksAdoptedAt 2>/dev/null || echo "")"
head_sha="$(git -C "$BOX" rev-parse HEAD)"
if [[ "$adopted_at" == "$head_sha" ]]; then
  echo "PASS: (h) fresh install: dvandva.hooksAdoptedAt records HEAD"
else
  echo "FAIL: (h) fresh install: expected dvandva.hooksAdoptedAt=$head_sha, got '$adopted_at'"
  failures=$((failures + 1))
fi

# Functional selfcheck probe: active pre-commit/prepare wrappers self-identify.
out="$(cd "$BOX" && DVANDVA_HOOK_SELFCHECK=1 .dvandva/githooks/pre-commit 2>&1)"; rc=$?
check_msg "(h) selfcheck pre-commit prints DVANDVA_GATE_WIRED" 0 "$rc" "$out" "DVANDVA_GATE_WIRED"
out="$(cd "$BOX" && DVANDVA_HOOK_SELFCHECK=1 .dvandva/githooks/prepare-commit-msg 2>&1)"; rc=$?
check_msg "(h) selfcheck prepare-commit-msg prints DVANDVA_PREPARE_WIRED" 0 "$rc" "$out" "DVANDVA_PREPARE_WIRED"

# Idempotent: second install leaves prior + hooksPath unchanged.
out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(h) second install idempotent: exits 0" 0 "$rc" "$out"
prior2="$(git -C "$BOX" config --local dvandva.priorHooksPath 2>/dev/null || echo "")"
current2="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
if [[ "$prior2" == "__DVANDVA_DEFAULT__" && "$current2" == ".dvandva/githooks" ]]; then
  echo "PASS: (h) idempotent: prior + hooksPath unchanged on re-install"
else
  echo "FAIL: (h) idempotent: prior='$prior2' hooksPath='$current2'"
  failures=$((failures + 1))
fi

# --uninstall restores the default (unset) + removes dir + clears dvandva.* keys.
out="$(cd "$BOX" && "$INSTALLER" --uninstall 2>&1)"; rc=$?
check "(h) --uninstall: exits 0" 0 "$rc" "$out"
current="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
if [[ -z "$current" ]]; then
  echo "PASS: (h) --uninstall: core.hooksPath cleared (default restored)"
else
  echo "FAIL: (h) --uninstall: core.hooksPath still set to '$current'"
  failures=$((failures + 1))
fi
if [[ ! -d "$BOX/.dvandva/githooks" ]]; then
  echo "PASS: (h) --uninstall: hook dir removed"
else
  echo "FAIL: (h) --uninstall: hook dir remains"
  failures=$((failures + 1))
fi
leftover="$(git -C "$BOX" config --local --get-regexp '^dvandva\.(priorHooksPath|hooksAdopted)' 2>/dev/null || echo "")"
if [[ -z "$leftover" ]]; then
  echo "PASS: (h) --uninstall: dvandva.* keys cleared"
else
  echo "FAIL: (h) --uninstall: leftover keys: $leftover"
  failures=$((failures + 1))
fi

# --uninstall again is safe (no-op)
out="$(cd "$BOX" && "$INSTALLER" --uninstall 2>&1)"; rc=$?
check "(h) second --uninstall: exits 0" 0 "$rc" "$out"

# Verify hook is NOT set globally (no --global change)
global_hooks="$(git config --global core.hooksPath 2>/dev/null || echo "")"
if [[ -n "$global_hooks" ]]; then
  echo "WARN: (h) global core.hooksPath is set ($global_hooks) — skipping global-isolation check"
else
  echo "PASS: (h) installer did not modify global core.hooksPath"
fi

# ===========================================================================
# (i) Drift lint flags unstamped commits (off-protocol drift)
# ===========================================================================
BOX="$TMP_DIR/i-drift"
new_git_repo "$BOX"
out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(i) drift lint sandwich baseline install exits 0" 0 "$rc" "$out"

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

# Commit 3 (stamped): checkpoint 4 — must not hide the unstamped middle commit
touch "$BOX/file3.txt"
git -C "$BOX" add file3.txt
git -C "$BOX" commit --quiet -m "$(printf 'feat: second stamped commit\n\nDvandva-Checkpoint: 4')"

out="$(cd "$BOX" && "$DRIFT_LINT" 2>&1)"; rc=$?
check_msg "(i) drift lint still flags sandwich bypass after later stamp" 1 "$rc" "$out" "off-protocol"

# Repo with no checkpointed commits at all → no drift to report
BOX2="$TMP_DIR/i-no-checkpoints"
new_git_repo "$BOX2"
touch "$BOX2/file.txt"
git -C "$BOX2" add file.txt
git -C "$BOX2" commit --quiet -m "plain commit without trailer"
out="$(cd "$BOX2" && "$DRIFT_LINT" 2>&1)"; rc=$?
check "(i) drift lint exits 0 when no checkpoints exist" 0 "$rc" "$out"

# Active baton adopted after existing history → pre-adoption commits are not
# drift.  Only commits after the local hooks baseline are reportable.
BOX3="$TMP_DIR/i-adoption-baseline"
new_git_repo "$BOX3"
touch "$BOX3/pre-adoption.txt"
git -C "$BOX3" add pre-adoption.txt
git -C "$BOX3" commit --quiet -m "plain pre-adoption commit"
make_gate_baton "$BOX3/.dvandva/baton.json" "implementing" "vadi" 31
out="$(cd "$BOX3" && "$INSTALLER" 2>&1)"; rc=$?
check "(i) adoption baseline install exits 0" 0 "$rc" "$out"
out="$(cd "$BOX3" && "$DRIFT_LINT" 2>&1)"; rc=$?
check "(i) active baton drift lint ignores pre-adoption history" 0 "$rc" "$out"

touch "$BOX3/post-adoption.txt"
git -C "$BOX3" add post-adoption.txt
out="$(cd "$BOX3" && (unset DVANDVA_ROLE; git commit --no-verify -m "post-adoption bypass") 2>&1)"; rc=$?
check "(i) post-adoption no-verify commit succeeds for drift probe" 0 "$rc" "$out"
out="$(cd "$BOX3" && "$DRIFT_LINT" 2>&1)"; rc=$?
check_msg "(i) drift lint flags post-adoption bypass only" 1 "$rc" "$out" "post-adoption bypass"

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
# (m) Installing hooks in an unborn repo cannot know the future root commit yet,
#     but it must leave a pending marker that gets backfilled as soon as a root
#     commit exists.  Otherwise an adopted-from-empty run loses its adoption
#     floor until the installer happens to be run again.
# ===========================================================================
BOX="$TMP_DIR/m-empty-install-backfill"
mkdir -p "$BOX"
git -C "$BOX" init --quiet
git -C "$BOX" config user.email "test@dvandva.test"
git -C "$BOX" config user.name "Dvandva Test"

out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(m) empty repo install exits 0" 0 "$rc" "$out"
pending_baseline="$(git -C "$BOX" config --local dvandva.hooksAdoptedAt 2>/dev/null || echo "")"
if [[ "$pending_baseline" == "__DVANDVA_ROOT_PENDING__" ]]; then
  echo "PASS: (m) empty repo install records pending root baseline"
else
  echo "FAIL: (m) empty repo install expected pending root baseline, got '$pending_baseline'"
  failures=$((failures + 1))
fi

touch "$BOX/root.txt"
git -C "$BOX" add root.txt
git -C "$BOX" commit --quiet -m "$(printf 'feat: root checkpoint\n\nDvandva-Checkpoint: 1')"
root_sha="$(git -C "$BOX" rev-parse HEAD)"

out="$(cd "$BOX" && "$DRIFT_LINT" 2>&1)"; rc=$?
check "(m) drift lint backfills pending baseline" 0 "$rc" "$out"
backfilled_baseline="$(git -C "$BOX" config --local dvandva.hooksAdoptedAt 2>/dev/null || echo "")"
if [[ "$backfilled_baseline" == "$root_sha" ]]; then
  echo "PASS: (m) pending baseline backfilled to root commit"
else
  echo "FAIL: (m) expected dvandva.hooksAdoptedAt=$root_sha, got '$backfilled_baseline'"
  failures=$((failures + 1))
fi

BOX="$TMP_DIR/m-empty-install-unstamped-root"
mkdir -p "$BOX"
git -C "$BOX" init --quiet
git -C "$BOX" config user.email "test@dvandva.test"
git -C "$BOX" config user.name "Dvandva Test"
make_gate_baton "$BOX/.dvandva/baton.json" "implementing" "vadi" 1
out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(m) empty active repo install exits 0" 0 "$rc" "$out"

touch "$BOX/root-bypass.txt"
git -C "$BOX" add root-bypass.txt
out="$(cd "$BOX" && git commit --no-verify --quiet -m "root bypass without trailer" 2>&1)"; rc=$?
check "(m) unstamped root bypass commit succeeds for drift probe" 0 "$rc" "$out"
out="$(cd "$BOX" && "$DRIFT_LINT" 2>&1)"; rc=$?
check_msg "(m) drift lint flags unstamped root after pending baseline" 1 "$rc" "$out" "root bypass without trailer"
out="$(cd "$BOX" && "$DRIFT_LINT" 2>&1)"; rc=$?
check_msg "(m) persisted root baseline remains inclusive" 1 "$rc" "$out" "root bypass without trailer"

# ===========================================================================
# (n) Husky-owned repo: the delegating gate still ENFORCES. A foreign
#     hooksPath is wrapped (not refused); a wrong-role commit is blocked by
#     the gate BEFORE the prior chain runs (commit-msg marker does not fire).
# ===========================================================================
BOX="$TMP_DIR/n-husky-enforce"
new_husky_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "implementing" "vadi" 5
out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(n) install wraps foreign Husky hooksPath: exits 0" 0 "$rc" "$out"
recorded="$(git -C "$BOX" config --local dvandva.priorHooksPath 2>/dev/null || echo "")"
if [[ "$recorded" == ".husky/_" ]]; then
  echo "PASS: (n) prior Husky hooksPath recorded (.husky/_)"
else
  echo "FAIL: (n) expected prior .husky/_, got '$recorded'"
  failures=$((failures + 1))
fi
: > "$BOX/hook.log"
touch "$BOX/wrong.txt"; git -C "$BOX" add wrong.txt
out="$(cd "$BOX" && DVANDVA_ROLE=prativadi git commit -m "wrong role" 2>&1)"; rc=$?
check_msg "(n) wrong-role commit blocked by gate" 1 "$rc" "$out" "DVANDVA_GATE blocked"
if [[ "$(count_in_file 'HUSKY_commit-msg_FIRED' "$BOX/hook.log")" == "0" ]]; then
  echo "PASS: (n) blocked commit did not reach foreign commit-msg"
else
  echo "FAIL: (n) foreign commit-msg fired despite gate block"
  failures=$((failures + 1))
fi

# ===========================================================================
# (o) Husky-owned repo, allowed role: gate passes, then the prior chain is
#     delegated to (pre-commit + commit-msg markers fire), the checkpoint
#     trailer is stamped, pre-push is preserved, install is zero-tracked-diff
#     and idempotent, enumerated stubs preserve argv/exit/exec, and uninstall
#     restores Husky so all three foreign hooks fire again.
# ===========================================================================
BOX="$TMP_DIR/o-husky-wrap"
new_husky_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "implementing" "vadi" 8
out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(o) install in Husky repo: exits 0" 0 "$rc" "$out"
current="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
if [[ "$current" == ".dvandva/githooks" ]]; then
  echo "PASS: (o) core.hooksPath repointed to .dvandva/githooks"
else
  echo "FAIL: (o) expected .dvandva/githooks, got '$current'"
  failures=$((failures + 1))
fi
# Enumerated pass-through stubs for foreign (non-owned) hook names.
for stub in commit-msg pre-push; do
  if [[ -x "$BOX/.dvandva/githooks/$stub" ]]; then
    echo "PASS: (o) executable pass-through stub for $stub"
  else
    echo "FAIL: (o) missing/non-exec stub for $stub"
    failures=$((failures + 1))
  fi
done
# We OWN pre-commit/prepare-commit-msg — those are wrappers, not naive stubs.
# Zero tracked diff: only the gitignored dir + local git config changed.
status="$(git -C "$BOX" status --porcelain 2>/dev/null)"
if [[ -z "$status" ]]; then
  echo "PASS: (o) zero tracked diff after install"
else
  echo "FAIL: (o) tracked changes after install:"
  echo "$status"
  failures=$((failures + 1))
fi

# Commit as the allowed role: gate passes → delegate → stamp.
: > "$BOX/hook.log"
touch "$BOX/feat.txt"; git -C "$BOX" add feat.txt
out="$(cd "$BOX" && DVANDVA_ROLE=vadi git commit -m "vadi feature" 2>&1)"; rc=$?
check "(o) allowed-role commit succeeds" 0 "$rc" "$out"
if [[ "$(count_in_file 'HUSKY_pre-commit_FIRED' "$BOX/hook.log")" -ge 1 ]]; then
  echo "PASS: (o) gate delegated to Husky pre-commit"
else
  echo "FAIL: (o) Husky pre-commit marker did not fire"
  failures=$((failures + 1))
fi
if [[ "$(count_in_file 'HUSKY_commit-msg_FIRED' "$BOX/hook.log")" -ge 1 ]]; then
  echo "PASS: (o) foreign commit-msg fired via pass-through stub"
else
  echo "FAIL: (o) foreign commit-msg marker did not fire"
  failures=$((failures + 1))
fi
commit_body="$(git -C "$BOX" show -s --format='%B' HEAD)"
if echo "$commit_body" | grep -qE "^Dvandva-Checkpoint: 8$"; then
  echo "PASS: (o) checkpoint trailer stamped after delegation"
else
  echo "FAIL: (o) Dvandva-Checkpoint: 8 trailer not stamped"
  failures=$((failures + 1))
fi

# pre-push is preserved: a real push to a local bare remote fires its marker.
REMOTE="$TMP_DIR/o-remote.git"
git init --quiet --bare "$REMOTE"
git -C "$BOX" remote add origin "$REMOTE"
: > "$BOX/hook.log"
out="$(cd "$BOX" && DVANDVA_ROLE=vadi git push -q origin HEAD 2>&1)"; rc=$?
check "(o) push to bare remote succeeds" 0 "$rc" "$out"
if [[ "$(count_in_file 'HUSKY_pre-push_FIRED' "$BOX/hook.log")" -ge 1 ]]; then
  echo "PASS: (o) foreign pre-push fired via pass-through stub"
else
  echo "FAIL: (o) foreign pre-push marker did not fire"
  failures=$((failures + 1))
fi

# Idempotent re-install: prior unchanged, no recursion (single marker per hook).
out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(o) second install idempotent: exits 0" 0 "$rc" "$out"
recorded2="$(git -C "$BOX" config --local dvandva.priorHooksPath 2>/dev/null || echo "")"
if [[ "$recorded2" == ".husky/_" ]]; then
  echo "PASS: (o) idempotent: prior still .husky/_ (no self-wrap)"
else
  echo "FAIL: (o) idempotent: prior became '$recorded2'"
  failures=$((failures + 1))
fi
: > "$BOX/hook.log"
touch "$BOX/feat2.txt"; git -C "$BOX" add feat2.txt
out="$(cd "$BOX" && DVANDVA_ROLE=vadi git commit -m "vadi feature 2" 2>&1)"; rc=$?
check "(o) commit after re-install succeeds" 0 "$rc" "$out"
pre_count="$(count_in_file 'HUSKY_pre-commit_FIRED' "$BOX/hook.log")"
if [[ "$pre_count" == "1" ]]; then
  echo "PASS: (o) no recursion: Husky pre-commit fired exactly once"
else
  echo "FAIL: (o) recursion suspected: Husky pre-commit fired $pre_count time(s)"
  failures=$((failures + 1))
fi

# Enumerated stub preserves argv + exit status + exec bit (direct invocation
# against a prior hook that exits non-zero and echoes its arguments).
cat > "$BOX/.husky/_/commit-msg" <<'EOF'
#!/usr/bin/env bash
echo "ARGV=[$*]" > "$(git rev-parse --show-toplevel)/stub-probe.log"
exit 7
EOF
chmod +x "$BOX/.husky/_/commit-msg"
out="$(cd "$BOX" && .dvandva/githooks/commit-msg .git/COMMIT_EDITMSG extra 2>&1)"; rc=$?
if [[ "$rc" -eq 7 ]]; then
  echo "PASS: (o) stub propagates prior hook exit status (7)"
else
  echo "FAIL: (o) stub exit status expected 7, got $rc"
  failures=$((failures + 1))
fi
if grep -q 'ARGV=\[.git/COMMIT_EDITMSG extra\]' "$BOX/stub-probe.log" 2>/dev/null; then
  echo "PASS: (o) stub forwards argv unchanged"
else
  echo "FAIL: (o) stub did not forward argv: $(cat "$BOX/stub-probe.log" 2>/dev/null)"
  failures=$((failures + 1))
fi
# restore the simple marker commit-msg before uninstall checks
cat > "$BOX/.husky/_/commit-msg" <<EOF
#!/usr/bin/env bash
echo "HUSKY_commit-msg_FIRED args=[\$*]" >> "$BOX/hook.log"
exit 0
EOF
chmod +x "$BOX/.husky/_/commit-msg"

# Uninstall restores Husky ownership; all three foreign hooks fire again.
out="$(cd "$BOX" && "$INSTALLER" --uninstall 2>&1)"; rc=$?
check "(o) uninstall: exits 0" 0 "$rc" "$out"
current="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
if [[ "$current" == ".husky/_" ]]; then
  echo "PASS: (o) uninstall restored core.hooksPath=.husky/_"
else
  echo "FAIL: (o) uninstall expected .husky/_, got '$current'"
  failures=$((failures + 1))
fi
if [[ ! -d "$BOX/.dvandva/githooks" ]]; then
  echo "PASS: (o) uninstall removed the materialized hook dir"
else
  echo "FAIL: (o) uninstall left the hook dir behind"
  failures=$((failures + 1))
fi
: > "$BOX/hook.log"
touch "$BOX/after.txt"; git -C "$BOX" add after.txt
out="$(cd "$BOX" && git commit -m "post-uninstall" 2>&1)"; rc=$?
check "(o) post-uninstall commit succeeds" 0 "$rc" "$out"
out="$(cd "$BOX" && git push -q origin HEAD 2>&1)"; rc=$?
if [[ "$(count_in_file 'HUSKY_pre-commit_FIRED' "$BOX/hook.log")" -ge 1 \
   && "$(count_in_file 'HUSKY_commit-msg_FIRED' "$BOX/hook.log")" -ge 1 \
   && "$(count_in_file 'HUSKY_pre-push_FIRED' "$BOX/hook.log")" -ge 1 ]]; then
  echo "PASS: (o) all three foreign hooks fire again after uninstall"
else
  echo "FAIL: (o) some foreign hook did not fire after uninstall: $(cat "$BOX/hook.log" 2>/dev/null)"
  failures=$((failures + 1))
fi

# ===========================================================================
# (p) Absolute prior hooksPath round-trips through record → wrap → restore.
# ===========================================================================
BOX="$TMP_DIR/p-absolute-prior"
new_git_repo "$BOX"
ABS_HOOKS="$BOX/abs-hooks"
mkdir -p "$ABS_HOOKS"
cat > "$ABS_HOOKS/pre-commit" <<EOF
#!/usr/bin/env bash
echo "ABS_PRECOMMIT_FIRED" >> "$BOX/hook.log"
exit 0
EOF
chmod +x "$ABS_HOOKS/pre-commit"
git -C "$BOX" config core.hooksPath "$ABS_HOOKS"   # absolute path
out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(p) install over absolute prior: exits 0" 0 "$rc" "$out"
recorded="$(git -C "$BOX" config --local dvandva.priorHooksPath 2>/dev/null || echo "")"
if [[ "$recorded" == "$ABS_HOOKS" ]]; then
  echo "PASS: (p) absolute prior recorded verbatim"
else
  echo "FAIL: (p) expected '$ABS_HOOKS', got '$recorded'"
  failures=$((failures + 1))
fi
: > "$BOX/hook.log"
touch "$BOX/p.txt"; git -C "$BOX" add p.txt
out="$(cd "$BOX" && git commit -m "p commit" 2>&1)"; rc=$?
check "(p) commit succeeds (no active baton)" 0 "$rc" "$out"
if [[ "$(count_in_file 'ABS_PRECOMMIT_FIRED' "$BOX/hook.log")" -ge 1 ]]; then
  echo "PASS: (p) absolute prior pre-commit delegated to"
else
  echo "FAIL: (p) absolute prior pre-commit not fired"
  failures=$((failures + 1))
fi
out="$(cd "$BOX" && "$INSTALLER" --uninstall 2>&1)"; rc=$?
check "(p) uninstall: exits 0" 0 "$rc" "$out"
current="$(git -C "$BOX" config --local core.hooksPath 2>/dev/null || echo "")"
if [[ "$current" == "$ABS_HOOKS" ]]; then
  echo "PASS: (p) uninstall restored absolute prior hooksPath"
else
  echo "FAIL: (p) uninstall expected '$ABS_HOOKS', got '$current'"
  failures=$((failures + 1))
fi

# ===========================================================================
# (q) Layer-1: a fresh repo with ONLY the plugin (no prior hooks) materializes
#     the gate and enforces it — proves the work-gate ships in the plugin.
# ===========================================================================
BOX="$TMP_DIR/q-layer1"
new_git_repo "$BOX"
make_gate_baton "$BOX/.dvandva/baton.json" "implementing" "vadi" 3
out="$(cd "$BOX" && "$PLUGIN_INSTALLER" 2>&1)"; rc=$?
check "(q) plugin installer materializes in fresh repo: exits 0" 0 "$rc" "$out"
if [[ -x "$BOX/.dvandva/githooks/pre-commit" && -x "$BOX/.dvandva/githooks/dvandva-commit-gate.sh" ]]; then
  echo "PASS: (q) plugin install materialized wrapper + gate"
else
  echo "FAIL: (q) plugin install did not materialize the gate"
  failures=$((failures + 1))
fi
touch "$BOX/q.txt"; git -C "$BOX" add q.txt
out="$(cd "$BOX" && DVANDVA_ROLE=prativadi git commit -m "wrong role layer1" 2>&1)"; rc=$?
check_msg "(q) layer-1 gate blocks wrong role" 1 "$rc" "$out" "DVANDVA_GATE blocked"
out="$(cd "$BOX" && DVANDVA_ROLE=vadi git commit -m "right role layer1" 2>&1)"; rc=$?
check "(q) layer-1 gate allows correct role" 0 "$rc" "$out"

# ===========================================================================
# (r) Self-loop guard: if priorHooksPath ever points at our own hook dir, the
#     wrapper must NOT recurse into itself.
# ===========================================================================
BOX="$TMP_DIR/r-self-loop"
new_git_repo "$BOX"
out="$(cd "$BOX" && "$INSTALLER" 2>&1)"; rc=$?
check "(r) install: exits 0" 0 "$rc" "$out"
# Poison the recorded prior to point at our own dir.
git -C "$BOX" config --local dvandva.priorHooksPath ".dvandva/githooks"
touch "$BOX/r.txt"; git -C "$BOX" add r.txt
out="$(cd "$BOX" && timeout 20 env DVANDVA_ROLE=vadi git commit -m "self loop" 2>&1)"; rc=$?
if [[ "$rc" -ne 124 ]]; then
  echo "PASS: (r) self-loop prior did not cause infinite recursion (exit $rc)"
else
  echo "FAIL: (r) self-loop prior caused a hang (timeout)"
  failures=$((failures + 1))
fi

# ===========================================================================
# (s) Byte-identity: the repo-root dogfood mirror equals the plugin sources.
# ===========================================================================
if cmp -s "$INSTALLER" "$PLUGIN_INSTALLER"; then
  echo "PASS: (s) scripts/install-dvandva-hooks.sh ≡ plugin installer"
else
  echo "FAIL: (s) root installer drifted from plugin installer"
  failures=$((failures + 1))
fi
if cmp -s "$ROOT_DIR/.githooks/pre-commit" "$PLUGIN_PRECOMMIT"; then
  echo "PASS: (s) .githooks/pre-commit ≡ plugin pre-commit wrapper"
else
  echo "FAIL: (s) .githooks/pre-commit drifted from plugin wrapper"
  failures=$((failures + 1))
fi
if cmp -s "$ROOT_DIR/.githooks/prepare-commit-msg" "$PLUGIN_PREPARE"; then
  echo "PASS: (s) .githooks/prepare-commit-msg ≡ plugin prepare-commit-msg wrapper"
else
  echo "FAIL: (s) .githooks/prepare-commit-msg drifted from plugin wrapper"
  failures=$((failures + 1))
fi
# Gate + drift-lint also ship in the plugin (kills the repo-only Layer 1).
if cmp -s "$GATE" "$PLUGIN_DIR/scripts/dvandva-commit-gate.sh"; then
  echo "PASS: (s) dvandva-commit-gate.sh ships in the plugin (byte-identical)"
else
  echo "FAIL: (s) plugin dvandva-commit-gate.sh missing/drifted"
  failures=$((failures + 1))
fi
if cmp -s "$DRIFT_LINT" "$PLUGIN_DIR/scripts/dvandva-drift-lint.sh"; then
  echo "PASS: (s) dvandva-drift-lint.sh ships in the plugin (byte-identical)"
else
  echo "FAIL: (s) plugin dvandva-drift-lint.sh missing/drifted"
  failures=$((failures + 1))
fi

# ===========================================================================
# (t) GAP 3: Linked-worktree delegation.
#
# Install Dvandva inside a linked worktree; verify the gate fires there and
# that resolve_prior_hook uses --git-common-dir to reach the main .git/hooks
# directory so the prior pre-commit chain fires even though the prior hooks
# live in the main repository's default hooks directory.
#
# Setup:
#   - Main repo with a prior pre-commit hook at .git/hooks/pre-commit
#   - Linked worktree on a separate branch (git worktree add -b ...)
#   - Installer run inside the linked worktree
#
# Contracts verified:
#   1. Installer exits 0 inside a linked worktree.
#   2. --git-common-dir in the linked worktree points at the main .git dir.
#   3. Wrong-role commit is blocked by the gate (gate fires in linked wt).
#   4. Correct-role commit succeeds AND prior pre-commit fires (delegation
#      via resolve_prior_hook / --git-common-dir).
# ===========================================================================
BOX_T_MAIN="$TMP_DIR/t-linked-main"
BOX_T_LINKED="$TMP_DIR/t-linked-wt"
PRIOR_HOOK_LOG="$TMP_DIR/t-prior-hook.log"

new_git_repo "$BOX_T_MAIN"

# Plant a prior pre-commit hook in the main .git/hooks directory (default hooks dir).
mkdir -p "$BOX_T_MAIN/.git/hooks"
cat > "$BOX_T_MAIN/.git/hooks/pre-commit" <<PRIOR_HOOK
#!/usr/bin/env bash
echo "PRIOR_PRECOMMIT_FIRED" >> "$PRIOR_HOOK_LOG"
exit 0
PRIOR_HOOK
chmod +x "$BOX_T_MAIN/.git/hooks/pre-commit"

# Create a linked worktree on a new branch.
git -C "$BOX_T_MAIN" worktree add -b t-linked-branch "$BOX_T_LINKED" 2>/dev/null
linked_wt_ok=$?
if [[ "$linked_wt_ok" -ne 0 ]]; then
  echo "SKIP: (t) git worktree add failed (exit $linked_wt_ok) — linked-worktree fixture infeasible in this environment"
else
  # Verify --git-common-dir in the linked worktree resolves to the main .git.
  got_common_dir="$(git -C "$BOX_T_LINKED" rev-parse --git-common-dir 2>/dev/null)"
  if [[ "$got_common_dir" == "$BOX_T_MAIN/.git" ]]; then
    echo "PASS: (t) --git-common-dir in linked worktree → main .git"
  else
    echo "FAIL: (t) --git-common-dir expected $BOX_T_MAIN/.git, got '$got_common_dir'"
    failures=$((failures + 1))
  fi

  # Seed an active baton in the linked worktree (untracked; the gate reads from
  # REPO_ROOT which resolves to BOX_T_LINKED inside the linked wt context).
  make_gate_baton "$BOX_T_LINKED/.dvandva/baton.json" "implementing" "vadi" 20

  # Run the installer from inside the linked worktree.
  out="$(cd "$BOX_T_LINKED" && "$INSTALLER" 2>&1)"; rc=$?
  check "(t) installer in linked worktree: exits 0" 0 "$rc" "$out"

  # Wrong-role commit must be blocked by the gate inside the linked worktree.
  touch "$BOX_T_LINKED/t1.txt"; git -C "$BOX_T_LINKED" add t1.txt
  out="$(cd "$BOX_T_LINKED" && DVANDVA_ROLE=prativadi git commit -m "wrong role in linked wt" 2>&1)"; rc=$?
  check_msg "(t) gate blocks wrong role in linked worktree" 1 "$rc" "$out" "DVANDVA_GATE blocked"

  # Correct-role commit must succeed AND the prior pre-commit must fire
  # (delegation via resolve_prior_hook using --git-common-dir).
  : > "$PRIOR_HOOK_LOG"
  touch "$BOX_T_LINKED/t2.txt"; git -C "$BOX_T_LINKED" add t2.txt
  out="$(cd "$BOX_T_LINKED" && DVANDVA_ROLE=vadi git commit -m "vadi in linked wt" 2>&1)"; rc=$?
  check "(t) correct role allowed in linked worktree" 0 "$rc" "$out"

  if [[ "$(count_in_file 'PRIOR_PRECOMMIT_FIRED' "$PRIOR_HOOK_LOG")" -ge 1 ]]; then
    echo "PASS: (t) prior hook at main .git/hooks fires via --git-common-dir resolution"
  else
    echo "FAIL: (t) prior hook at main .git/hooks did not fire (--git-common-dir resolution broken)"
    failures=$((failures + 1))
  fi

  # Clean up linked worktree to avoid leaving dangling worktree metadata.
  git -C "$BOX_T_MAIN" worktree remove --force "$BOX_T_LINKED" 2>/dev/null || true
fi

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
