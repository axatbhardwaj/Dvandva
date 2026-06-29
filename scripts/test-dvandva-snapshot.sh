#!/usr/bin/env bash
# Tests for the bundled Dvandva snapshot helper.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-snapshot.sh"
PRATIVADI_SCRIPT="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-snapshot.sh"
TMP_DIR="$(mktemp -d)"

cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT

failures=0

write_baton() {
  local file="$1" assignee="$2" status="$3" checkpoint="$4" branch="${5:-main}"
  mkdir -p "$(dirname "$file")"
  cat > "$file" <<JSON
{
  "schema": "dvandva.baton.v1",
  "assignee": "$assignee",
  "status": "$status",
  "phase": 1,
  "checkpoint": $checkpoint,
  "branch": "$branch"
}
JSON
}

# Case 1: happy path — snapshot lands in .dvandva/history/
DVANDVA_DIR="$TMP_DIR/case1/.dvandva"
mkdir -p "$DVANDVA_DIR"
write_baton "$DVANDVA_DIR/baton.json" "vadi" "implementing" 3
"$SCRIPT" "$DVANDVA_DIR/baton.json"
EXPECTED_HISTORY="$DVANDVA_DIR/history/3-implementing-vadi.json"
if [[ -f "$EXPECTED_HISTORY" ]] && cmp -s "$DVANDVA_DIR/baton.json" "$EXPECTED_HISTORY"; then
  echo "PASS: history snapshot written, byte-identical to baton"
else
  echo "FAIL: history snapshot missing or mismatched at $EXPECTED_HISTORY"
  failures=$((failures + 1))
fi

# Case 2: terminal status also writes named archive
DVANDVA_DIR2="$TMP_DIR/case2/.dvandva"
mkdir -p "$DVANDVA_DIR2"
write_baton "$DVANDVA_DIR2/baton.json" "human" "done" 10 "feature-x"
"$SCRIPT" "$DVANDVA_DIR2/baton.json"
EXPECTED_ARCHIVE="$DVANDVA_DIR2/baton.feature-x-10-done.json"
if [[ -f "$EXPECTED_ARCHIVE" ]] && cmp -s "$DVANDVA_DIR2/baton.json" "$EXPECTED_ARCHIVE"; then
  echo "PASS: terminal status produces named archive"
else
  echo "FAIL: terminal archive missing or mismatched at $EXPECTED_ARCHIVE"
  failures=$((failures + 1))
fi

# Case 2a: termination_review is active, not terminal; history only, no archive.
DVANDVA_DIR2a="$TMP_DIR/case2a/.dvandva"
mkdir -p "$DVANDVA_DIR2a"
write_baton "$DVANDVA_DIR2a/baton.json" "team" "termination_review" 9 "feature-x"
"$SCRIPT" "$DVANDVA_DIR2a/baton.json"
EXPECTED_TERMINATION_HISTORY="$DVANDVA_DIR2a/history/9-termination_review-team.json"
UNEXPECTED_TERMINATION_ARCHIVE="$DVANDVA_DIR2a/baton.feature-x-9-termination_review.json"
if [[ -f "$EXPECTED_TERMINATION_HISTORY" ]] && [[ ! -e "$UNEXPECTED_TERMINATION_ARCHIVE" ]]; then
  echo "PASS: termination_review writes history only, no terminal archive"
else
  echo "FAIL: termination_review snapshot/archive classification wrong"
  failures=$((failures + 1))
fi

RUNS_ROOT="$TMP_DIR/case2-runs/.dvandva/runs"
ALPHA_DIR="$RUNS_ROOT/alpha"
BETA_DIR="$RUNS_ROOT/beta"
write_baton "$ALPHA_DIR/baton.json" "human" "done" 12 "alpha-branch"
write_baton "$BETA_DIR/baton.json" "human" "done" 13 "beta-branch"
"$SCRIPT" "$ALPHA_DIR/baton.json"
"$SCRIPT" "$BETA_DIR/baton.json"
if [[ -f "$ALPHA_DIR/history/12-done-human.json" \
  && -f "$ALPHA_DIR/baton.alpha-branch-12-done.json" \
  && -f "$BETA_DIR/history/13-done-human.json" \
  && -f "$BETA_DIR/baton.beta-branch-13-done.json" \
  && ! -e "$TMP_DIR/case2-runs/.dvandva/history" ]]; then
  echo "PASS: named-run snapshots and archives stay under each run parent"
else
  echo "FAIL: named-run snapshot isolation broken"
  failures=$((failures + 1))
fi

# Case 2b: branch with '/' is sanitized in archive filename
DVANDVA_DIR2b="$TMP_DIR/case2b/.dvandva"
mkdir -p "$DVANDVA_DIR2b"
write_baton "$DVANDVA_DIR2b/baton.json" "human" "done" 11 "feature/foo"
"$SCRIPT" "$DVANDVA_DIR2b/baton.json"
EXPECTED_SANITIZED="$DVANDVA_DIR2b/baton.feature-foo-11-done.json"
UNINTENDED_SUBPATH="$DVANDVA_DIR2b/baton.feature/foo-11-done.json"
if [[ -f "$EXPECTED_SANITIZED" ]] && [[ ! -e "$UNINTENDED_SUBPATH" ]]; then
  echo "PASS: branch with '/' sanitized to '-' in archive filename"
else
  echo "FAIL: branch sanitization broken (expected $EXPECTED_SANITIZED present, subpath absent)"
  failures=$((failures + 1))
fi

# Case 3: no-clobber on collision
DVANDVA_DIR3="$TMP_DIR/case3/.dvandva"
mkdir -p "$DVANDVA_DIR3"
write_baton "$DVANDVA_DIR3/baton.json" "vadi" "implementing" 4
"$SCRIPT" "$DVANDVA_DIR3/baton.json"
ORIGINAL_HISTORY="$DVANDVA_DIR3/history/4-implementing-vadi.json"
# Capture original bytes
ORIGINAL_HASH="$(sha256sum "$ORIGINAL_HISTORY" | awk '{print $1}')"
# Modify baton (different bytes, same checkpoint), re-run
echo '{"schema":"dvandva.baton.v1","assignee":"vadi","status":"implementing","phase":1,"checkpoint":4,"branch":"main","extra":"modified"}' > "$DVANDVA_DIR3/baton.json"
"$SCRIPT" "$DVANDVA_DIR3/baton.json"
POST_HASH="$(sha256sum "$ORIGINAL_HISTORY" | awk '{print $1}')"
DUP_COUNT=$(ls "$DVANDVA_DIR3/history/"4-implementing-vadi.dup-*.json 2>/dev/null | wc -l)
if [[ "$ORIGINAL_HASH" == "$POST_HASH" && "$DUP_COUNT" -ge 1 ]]; then
  echo "PASS: no-clobber preserved original and wrote dup file"
else
  echo "FAIL: no-clobber broken (original_hash_changed=$([ \"$ORIGINAL_HASH\" != \"$POST_HASH\" ] && echo yes || echo no), dup_count=$DUP_COUNT)"
  failures=$((failures + 1))
fi

# Case 4: byte-identical copies of the helper
DVANDVA_DIR4="$TMP_DIR/case4/.dvandva"
mkdir -p "$DVANDVA_DIR4"
write_baton "$DVANDVA_DIR4/baton.json" "vadi" "implementing" 5
touch "$DVANDVA_DIR4/history"
snapshot_failure_output="$("$SCRIPT" "$DVANDVA_DIR4/baton.json" 2>&1)"
snapshot_failure_exit=$?
if [[ "$snapshot_failure_exit" -eq 23 ]] && [[ "$snapshot_failure_output" == *"DVANDVA_SNAPSHOT write_failed"* ]]; then
  echo "PASS: snapshot write failure exits 23"
else
  echo "FAIL: snapshot write failure expected exit 23 with write_failed, got $snapshot_failure_exit"
  echo "$snapshot_failure_output"
  failures=$((failures + 1))
fi

# Case 5: byte-identical copies of the helper
if cmp -s "$SCRIPT" "$PRATIVADI_SCRIPT"; then
  echo "PASS: plugin snapshot helpers are byte-identical"
else
  echo "FAIL: plugin snapshot helpers drifted"
  failures=$((failures + 1))
fi

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi
exit 0
