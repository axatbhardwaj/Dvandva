#!/usr/bin/env bash
# Smoke coverage for the Workflow template's JavaScript parsing and prompt quoting.

set -u -o pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/../.." && pwd -P)
TEMPLATE="$REPO_ROOT/plugins/dvandva/references/adversarial-loop.template.js"

if [[ ! -f "$TEMPLATE" ]]; then
  printf 'FAIL: expected template at %s\n' "$TEMPLATE" >&2
  exit 1
fi

TEST_TMP=$(mktemp -d "${TMPDIR:-/tmp}/adversarial-loop-template-test.XXXXXX")
trap 'rm -rf "$TEST_TMP"' EXIT
MODULE_COPY="$TEST_TMP/adversarial-loop.template.mjs"

# Workflow permits top-level return; Node's module parser does not. Convert only
# that runtime-specific statement so the rest of the template is syntax-checked.
sed 's/^return /export default /' "$TEMPLATE" > "$MODULE_COPY"
if ! node --input-type=module --check < "$MODULE_COPY"; then
  printf 'FAIL: template must parse as an ES module after top-level return conversion\n' >&2
  exit 1
fi

# Exclude template delimiters, then reject any unescaped backtick in a lane prompt.
# Escaped command markup (\\`) is intentional; raw backticks terminate the outer
# JavaScript template literal and silently truncate a Workflow prompt.
lane_prompt_bodies() {
  awk '
    /^const executeLane = s => `/{ in_prompt = 1; next }
    /^const stampIds =/{ in_prompt = 0 }
    /^const stampLane = \(\) => `/{ in_prompt = 1; next }
    /^\/\/ Claude attacks/{ in_prompt = 0 }
    /^  return `You are Opus, the adversarial ATTACK lane/{ in_prompt = 1; next }
    /^}$/ && in_prompt { in_prompt = 0; next }
    in_prompt {
      sub(/`$/, "")
      print
    }
  ' "$TEMPLATE"
}

execute_lane_prompt_body() {
  awk '
    /^const executeLane = s => `/{ in_prompt = 1; next }
    /^const stampIds =/{ in_prompt = 0 }
    in_prompt {
      sub(/`$/, "")
      print
    }
  ' "$TEMPLATE"
}

lane_prompt_text=$(lane_prompt_bodies)
execute_lane_prompt_text=$(execute_lane_prompt_body)

if ! grep -Fq "typeof args === 'string'" "$TEMPLATE"; then
  printf 'FAIL: template must defensively normalize string workflow args\n' >&2
  exit 1
fi

if ! grep -Fq 'Plan for this step:' <<< "$execute_lane_prompt_text"; then
  printf 'FAIL: execute lane must embed the Plan for this step block\n' >&2
  exit 1
fi

raw_backtick_lines=$(grep -Ec '(^|[^\\])`' <<< "$lane_prompt_text" || true)
if [[ "$raw_backtick_lines" -ne 0 ]]; then
  printf 'FAIL: found %s lane-prompt line(s) with raw backticks\n' "$raw_backtick_lines" >&2
  exit 1
fi

if ! grep -Fq 'failing test BEFORE the implementation' <<< "$execute_lane_prompt_text"; then
  printf 'FAIL: execute lane must require a failing test BEFORE the implementation\n' >&2
  exit 1
fi

if ! grep -Fq 'red (pre-fix) and green (post-fix) outputs included in the report' <<< "$execute_lane_prompt_text"; then
  printf 'FAIL: execute lane must require red and green verification outputs in the report\n' >&2
  exit 1
fi

if ! grep -Fq 'existing failing test' <<< "$execute_lane_prompt_text"; then
  printf 'FAIL: execute lane must allow an existing failing test to supply red evidence\n' >&2
  exit 1
fi

printf 'PASS: template parses as a module and lane prompts have no raw backticks\n'
