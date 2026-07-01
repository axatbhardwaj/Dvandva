#!/usr/bin/env bash
# Tests for the Dvandva skill linter.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINTER="$ROOT_DIR/scripts/lint-skills.sh"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

failures=0

run_case() {
  local name="$1"
  local expected_exit="$2"
  shift 2

  local output
  output="$("$@" 2>&1)"
  local actual_exit=$?
  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    echo "FAIL: $name expected exit $expected_exit, got $actual_exit"
    echo "$output"
    failures=$((failures + 1))
    return 1
  fi
  echo "PASS: $name"
  return 0
}

run_case_contains() {
  local name="$1"
  local expected_exit="$2"
  local expected_text="$3"
  shift 3

  local output
  output="$("$@" 2>&1)"
  local actual_exit=$?
  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    echo "FAIL: $name expected exit $expected_exit, got $actual_exit"
    echo "$output"
    failures=$((failures + 1))
    return 1
  fi
  if [[ "$output" != *"$expected_text"* ]]; then
    echo "FAIL: $name missing expected output: $expected_text"
    echo "$output"
    failures=$((failures + 1))
    return 1
  fi
  echo "PASS: $name"
  return 0
}

cat > "$TMP_DIR/role-without-schema.md" <<'SKILL'
---
name: vadi
description: Use when testing the role-skill schema gate.
---

# Test Role

This role skill intentionally omits the baton schema.
SKILL

cat > "$TMP_DIR/non-role-invalid-frontmatter.md" <<'SKILL'
---
name: helper
---

# Invalid Helper

Missing description.
SKILL

cat > "$TMP_DIR/role-with-out-of-band-approval.md" <<'SKILL'
---
name: prativadi
description: Use when testing rejection of stale out-of-band final approval text.
---

# Test Role

- If `<current N> == total_phases`, set `prativadi_final_approval: true`; the vadi must review later.

```json
SKILL
cat "$ROOT_DIR/plugins/dvandva/references/baton-schema.json" >> "$TMP_DIR/role-with-out-of-band-approval.md"
cat >> "$TMP_DIR/role-with-out-of-band-approval.md" <<'SKILL'
```
SKILL

jq '. + {run_id: "v2-only"}' \
  "$ROOT_DIR/plugins/dvandva/references/baton-schema.json" \
  > "$TMP_DIR/v1-with-v2-key.json"
{
  cat <<'SKILL'
---
name: vadi
description: Use when testing rejection of v2-only keys in inline v1 baton schema.
---

# Test Role

```json
SKILL
  cat "$TMP_DIR/v1-with-v2-key.json"
  cat <<'SKILL'
```
SKILL
} > "$TMP_DIR/role-with-v2-only-key.md"

run_case "vadi role skill passes full lint" 0 \
  "$LINTER" "$ROOT_DIR/plugins/dvandva/skills/vadi/SKILL.md"

run_case "prativadi role skill passes full lint" 0 \
  "$LINTER" "$ROOT_DIR/plugins/dvandva/skills/prativadi/SKILL.md"

run_case "role skill without embedded schema fails" 1 \
  "$LINTER" "$TMP_DIR/role-without-schema.md"

run_case_contains "role inline v1 schema rejects v2-only key" 1 "unexpected key" \
  "$LINTER" "$TMP_DIR/role-with-v2-only-key.md"

run_case_contains "role skill rejects out-of-band final approval text" 1 "out-of-band final approval" \
  "$LINTER" "$TMP_DIR/role-with-out-of-band-approval.md"

run_case "non-role research skill passes without embedded schema" 0 \
  "$LINTER" "$ROOT_DIR/plugins/dvandva/skills/research/SKILL.md"

run_case "non-role testing skill passes without embedded schema" 0 \
  "$LINTER" "$ROOT_DIR/plugins/dvandva/skills/testing/SKILL.md"

run_case "non-role invalid frontmatter still fails" 1 \
  "$LINTER" "$TMP_DIR/non-role-invalid-frontmatter.md"

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
