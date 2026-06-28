#!/usr/bin/env bash
# Tests for scripts/lint-run3-dynamic-agents.sh.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_SCRIPT="$ROOT_DIR/scripts/lint-run3-dynamic-agents.sh"
TMP_DIR="$(mktemp -d)"
failures=0

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

pass() {
  echo "PASS: $1"
}

fail() {
  echo "FAIL: $1"
  failures=$((failures + 1))
}

make_case() {
  local name="$1"
  local box="$TMP_DIR/$name"

  mkdir -p \
    "$box/scripts" \
    "$box/docs/protocol" \
    "$box/docs/workflows" \
    "$box/plugins/dvandva/commands" \
    "$box/plugins/dvandva/references" \
    "$box/plugins/dvandva/skills/research"

  cp "$SOURCE_SCRIPT" "$box/scripts/lint-run3-dynamic-agents.sh"
  chmod +x "$box/scripts/lint-run3-dynamic-agents.sh"
  touch "$box/README.md" "$box/product.md"
  echo "$box"
}

write_contract_surface() {
  local box="$1"
  cat > "$box/plugins/dvandva/skills/research/SKILL.md" <<'SURFACE'
Dvandva uses agent_instances for Run 3 dynamic agent records.
The static roster is the seed roster for run-scoped dynamic agents.
Explicit closure is required; every generated handle must be explicitly closed before completion.
Dynamic write-path disjointness is required unless conflict_group serialization applies.
There is no daemon and no mailbox.
There is no hidden scheduler or hidden central process.
Claude Code maps `opus` to Opus-class and `sonnet` to Sonnet-class models.
Codex maps `opus` to `gpt-5.5` and `sonnet` to `gpt-5.4`.
generated agents must never own assignee, active_roles, or transitions.
SURFACE
}

run_expect() {
  local name="$1" expected_exit="$2" expected_text="$3"
  shift 3

  local output
  output="$("$@" 2>&1)"
  local actual_exit=$?

  if [[ "$actual_exit" -ne "$expected_exit" ]]; then
    fail "$name expected exit $expected_exit got $actual_exit"
    echo "$output"
    return
  fi

  if [[ "$output" != *"$expected_text"* ]]; then
    fail "$name missing expected output: $expected_text"
    echo "$output"
    return
  fi

  pass "$name"
}

BOX="$(make_case pass-surface)"
write_contract_surface "$BOX"
run_expect "run3 lint accepts complete contract surface" 0 "Run 3 dynamic-agent lint passed." \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-agent-instances)"
write_contract_surface "$BOX"
perl -0pi -e 's/agent_instances/agent registry/g' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing agent_instances" 1 "FAIL: surface names Run 3 agent_instances" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-seed-roster)"
write_contract_surface "$BOX"
perl -0pi -e 's/^The static roster is the seed roster for run-scoped dynamic agents\.\n//m' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing seed roster wording" 1 "FAIL: surface treats the roster as a seed/static roster" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-run-scoped-dynamic)"
write_contract_surface "$BOX"
perl -0pi -e 's/run-scoped dynamic agents/local generated workers/g' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing run-scoped dynamic wording" 1 "FAIL: surface documents run-scoped dynamic agents or instances" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-explicit-closure)"
write_contract_surface "$BOX"
perl -0pi -e 's/^Explicit closure.*\n//m' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing explicit closure" 1 "FAIL: surface requires explicit subagent handle closure" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-write-path-disjointness)"
write_contract_surface "$BOX"
perl -0pi -e 's/^Dynamic write-path.*\n//m' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing write-path disjointness" 1 "FAIL: surface documents write-path disjointness or conflict_group serialization" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-no-daemon)"
write_contract_surface "$BOX"
perl -0pi -e 's/no daemon/no background service/g' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing no-daemon guardrail" 1 "FAIL: surface rejects a runtime daemon" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-no-mailbox)"
write_contract_surface "$BOX"
perl -0pi -e 's/ and no mailbox//g' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing no-mailbox guardrail" 1 "FAIL: surface rejects a runtime mailbox" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-hidden-scheduler)"
write_contract_surface "$BOX"
perl -0pi -e 's/^There is no hidden scheduler.*\n//m' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing hidden-scheduler guardrail" 1 "FAIL: surface rejects a hidden scheduler or central owner" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-anthropic-models)"
write_contract_surface "$BOX"
perl -0pi -e 's/^Claude Code maps.*\n//m' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing Claude model mapping" 1 "FAIL: surface documents Anthropic opus/sonnet model-class mapping" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-codex-models)"
write_contract_surface "$BOX"
perl -0pi -e 's/^Codex maps.*\n//m' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing Codex model mapping" 1 "FAIL: surface documents Codex gpt-5.5/gpt-5.4 model-class mapping" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case missing-generated-agent-ownership)"
write_contract_surface "$BOX"
perl -0pi -e 's/^generated agents.*\n//m' "$BOX/plugins/dvandva/skills/research/SKILL.md"
run_expect "run3 lint rejects missing generated-agent ownership rule" 1 "FAIL: surface says generated agents do not own assignee, active_roles, or transitions" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

BOX="$(make_case script-is-not-surface)"
run_expect "run3 lint does not pass by scanning itself" 1 "Run 3 dynamic-agent lint failed" \
  "$BOX/scripts/lint-run3-dynamic-agents.sh"

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
