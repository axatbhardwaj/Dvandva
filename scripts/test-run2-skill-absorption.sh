#!/usr/bin/env bash
# Focused Run 2 absorption checks for the source skills folded into Dvandva.
#
# Default scope checks every absorbed skill. During two-team implementation,
# use --scope vadi to verify only the vadi-owned testing/worktree chunks
# without blocking on prativadi-owned understanding edits.
set -u

SCOPE="all"
failures=0

usage() {
  cat >&2 <<'USAGE'
Usage: test-run2-skill-absorption.sh [--scope all|vadi|testing|understanding|worktree] [--self-test]

Scopes:
  all            Check every absorbed source skill. Default.
  vadi           Check vadi-owned Run2 chunks: testing + worktree.
  testing        Check dvandva:testing absorption only.
  understanding  Check dvandva:understanding absorption only.
  worktree       Check dvandva:worktree-setup absorption only.
  --self-test    Exercise this script's pass and fail paths with temp fixtures.
USAGE
}

fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

pass() {
  echo "PASS: $*"
}

resolve_root() {
  local candidate="$1"
  local resolved

  if [[ -z "$candidate" ]]; then
    echo "FAIL: absorption root is empty" >&2
    return 1
  fi

  if ! resolved="$(cd "$candidate" 2>/dev/null && pwd)"; then
    echo "FAIL: absorption root missing or not a directory: $candidate" >&2
    return 1
  fi

  printf '%s\n' "$resolved"
}

require_file() {
  local file="$1" label="$2"
  if [[ -f "$file" ]]; then
    pass "$label exists"
  else
    fail "$label missing at ${file#$ROOT_DIR/}"
  fi
}

require_text() {
  local file="$1" needle="$2" label="$3"
  if [[ ! -f "$file" ]]; then
    fail "$label cannot be checked; missing ${file#$ROOT_DIR/}"
    return
  fi

  if grep -Fq -- "$needle" "$file"; then
    pass "$label"
  else
    fail "$label; missing '$needle' in ${file#$ROOT_DIR/}"
  fi
}

check_testing() {
  local skill="$ROOT_DIR/plugins/dvandva/skills/testing/SKILL.md"
  local agent="$ROOT_DIR/plugins/dvandva/agents/test-creator.md"

  require_file "$skill" "testing skill"
  require_text "$skill" "BATON_STATE" "testing skill surfaces baton state"
  require_text "$skill" "detect context" "testing skill preserves Step 1 detect context"
  require_text "$skill" "coverage analysis" "testing skill preserves Step 2 coverage analysis"
  require_text "$skill" "red attack" "testing skill preserves Step 3 red attack"
  require_text "$skill" "green verification" "testing skill preserves Step 4 green verification"
  require_text "$skill" "sandbox validation" "testing skill preserves Step 5 sandbox validation"
  require_text "$skill" "blue test writing" "testing skill preserves Step 6 blue test writing"
  require_text "$skill" "quality review" "testing skill preserves Step 7 quality review"
  require_text "$skill" "final review" "testing skill preserves Step 8 final review"
  require_text "$skill" "results" "testing skill preserves Step 9 results"
  require_text "$skill" "Boundary" "testing skill covers boundary attacks"
  require_text "$skill" "State/Concurrency" "testing skill covers state and concurrency attacks"
  require_text "$skill" "Error Handling" "testing skill covers error-handling attacks"
  require_text "$skill" "Bypass Logic" "testing skill covers bypass-logic attacks"
  require_text "$skill" "False positives and design limitations" "testing skill filters false positives before tests"
  require_text "$skill" "/tmp" "testing skill keeps runtime probes ephemeral"
  require_text "$skill" "shell=True" "testing skill forbids shell=True probes"
  require_text "$skill" "Docker --network none" "testing skill documents offline sandbox preference"
  require_text "$skill" "UNVERIFIABLE" "testing skill records blocked probes as unverifiable"
  require_text "$skill" "tests only for confirmed issues" "testing skill writes tests only for confirmed gaps"
  require_text "$skill" ".testing-skill/" "testing skill explicitly rejects old state directory recreation"
  require_text "$skill" "subagent_tracks" "testing skill maps old state to subagent tracks"
  require_text "$skill" "verification_matrix" "testing skill maps coverage to verification matrix"
  require_text "$skill" "Do not implement production behavior" "testing skill keeps production fixes out of testing"

  require_file "$agent" "test creator agent"
  require_text "$agent" "red attack" "test creator agent understands red attack stage"
  require_text "$agent" "blue test writing" "test creator agent understands blue test writing stage"
  require_text "$agent" "False positives and design limitations" "test creator agent filters unconfirmed findings"
  require_text "$agent" "UNVERIFIABLE" "test creator agent records blocked probes"
}

check_understanding() {
  local skill="$ROOT_DIR/plugins/dvandva/skills/understanding/SKILL.md"

  require_file "$skill" "understanding skill"
  require_text "$skill" "BATON_STATE" "understanding skill surfaces baton state"
  require_text "$skill" "anti-lecture" "understanding skill enforces anti-lecture behavior"
  require_text "$skill" "learner speaks at least as much" "understanding skill requires learner participation"
  require_text "$skill" "makes sense" "understanding skill rejects makes-sense as mastery evidence"
  require_text "$skill" "mental model" "understanding skill includes mental-model pillar"
  require_text "$skill" "concrete trace" "understanding skill includes concrete-trace pillar"
  require_text "$skill" "transfer" "understanding skill includes transfer pillar"
  require_text "$skill" "whys-to-bedrock" "understanding skill preserves why-chain questioning"
  require_text "$skill" "explain-back" "understanding skill uses explain-back checks"
  require_text "$skill" "edge-case prediction" "understanding skill uses edge-case prediction"
  require_text "$skill" "counterfactual" "understanding skill uses counterfactual checks"
  require_text "$skill" "research_ref" "understanding skill grounds teaching in research refs"
  require_text "$skill" "plan_ref" "understanding skill grounds teaching in plan refs"
  require_text "$skill" "./superpowers/understanding/YYYY-MM-DD-<topic>.html" "understanding skill writes HTML checklist"
  require_text "$skill" "copy-as-prompt" "understanding HTML exposes copy-as-prompt export"
  require_text "$skill" "copy-as-JSON" "understanding HTML exposes copy-as-JSON export"
  require_text "$skill" "when not to use" "understanding skill states when not to use it"
}

check_worktree() {
  local skill="$ROOT_DIR/plugins/dvandva/skills/worktree-setup/SKILL.md"

  require_file "$skill" "worktree setup skill"
  require_text "$skill" "BATON_STATE" "worktree skill surfaces baton state"
  require_text "$skill" "superpowers:using-git-worktrees" "worktree skill invokes Superpowers worktree gate"
  require_text "$skill" "git status --short --branch" "worktree skill preserves status preflight"
  require_text "$skill" "git worktree list --porcelain" "worktree skill preserves worktree preflight"
  require_text "$skill" "git branch --list" "worktree skill preserves branch preflight"
  require_text "$skill" "/home/xzat/defi/monorepo" "worktree skill preserves DeFi default root"
  require_text "$skill" "EDEF" "worktree skill preserves EDEF key handling"
  require_text "$skill" "TDEF" "worktree skill preserves TDEF key handling"
  require_text "$skill" "STDEF" "worktree skill preserves STDEF key handling"
  require_text "$skill" "custom-key" "worktree skill requires Monday custom-key"
  require_text "$skill" "pulse ID" "worktree skill warns against pulse IDs as branch names"
  require_text "$skill" "bare number" "worktree skill warns against bare item numbers"
  require_text "$skill" "Monday has no git-branch field" "worktree skill avoids invented Monday git-branch field"
  require_text "$skill" "monorepo-edef-12" "worktree skill includes lowercase DeFi path example"
  require_text "$skill" ".env.local" "worktree skill preserves env copy list"
  require_text "$skill" "node_modules" "worktree skill excludes dependency directories while copying env"
  require_text "$skill" "bun install" "worktree skill preserves Bun install step"
  require_text "$skill" "\"configVersion\": 0" "worktree skill removes Bun setup-only lock noise"
  require_text "$skill" "timeout --kill-after=10s 180s bash -lc 'TURBO_UI=false bun run test'" "worktree skill preserves bounded baseline command"
  require_text "$skill" "turbo, vitest, or bun run test" "worktree skill checks leftover test processes"
  require_text "$skill" "BRANCH-NOTES.md" "worktree skill preserves branch notes"
  require_text "$skill" "~/ACTIVE-WORK.md" "worktree skill preserves active work index"
  require_text "$skill" "dark self-contained HTML" "worktree skill uses HTML review artifacts"
  require_text "$skill" "Never post GitHub review" "worktree skill preserves no-auto-post guardrail"
  require_text "$skill" "axatbhardwaj@outlook.com" "worktree skill preserves verified Outlook email"
  require_text "$skill" "axatbhardwaj@gmail.com" "worktree skill preserves verified Gmail email"
  require_text "$skill" "exact SHAs" "worktree skill reports exact SHAs"
}

make_fixture() {
  local root="$1"
  mkdir -p "$root/plugins/dvandva/skills/testing" \
    "$root/plugins/dvandva/skills/understanding" \
    "$root/plugins/dvandva/skills/worktree-setup" \
    "$root/plugins/dvandva/agents"

  cat > "$root/plugins/dvandva/skills/testing/SKILL.md" <<'FIXTURE'
BATON_STATE
detect context coverage analysis red attack green verification sandbox validation blue test writing quality review final review results
Boundary State/Concurrency Error Handling Bypass Logic
False positives and design limitations
/tmp shell=True Docker --network none UNVERIFIABLE
tests only for confirmed issues
.testing-skill/ subagent_tracks verification_matrix
Do not implement production behavior
FIXTURE

  cat > "$root/plugins/dvandva/agents/test-creator.md" <<'FIXTURE'
red attack
blue test writing
False positives and design limitations
UNVERIFIABLE
FIXTURE

  cat > "$root/plugins/dvandva/skills/understanding/SKILL.md" <<'FIXTURE'
BATON_STATE
anti-lecture learner speaks at least as much makes sense
mental model concrete trace transfer whys-to-bedrock explain-back edge-case prediction counterfactual
research_ref plan_ref
./superpowers/understanding/YYYY-MM-DD-<topic>.html
copy-as-prompt copy-as-JSON when not to use
FIXTURE

  cat > "$root/plugins/dvandva/skills/worktree-setup/SKILL.md" <<'FIXTURE'
BATON_STATE
superpowers:using-git-worktrees
git status --short --branch
git worktree list --porcelain
git branch --list
/home/xzat/defi/monorepo
EDEF TDEF STDEF custom-key pulse ID bare number Monday has no git-branch field monorepo-edef-12
.env.local node_modules bun install "configVersion": 0
timeout --kill-after=10s 180s bash -lc 'TURBO_UI=false bun run test'
turbo, vitest, or bun run test
BRANCH-NOTES.md ~/ACTIVE-WORK.md
dark self-contained HTML
Never post GitHub review
axatbhardwaj@outlook.com axatbhardwaj@gmail.com exact SHAs
FIXTURE
}

self_test() {
  local script="${BASH_SOURCE[0]}"
  local good bad
  good="$(mktemp -d)"
  bad="$(mktemp -d)"
  trap 'rm -rf "$good" "$bad"' RETURN

  make_fixture "$good"
  if DVANDVA_RUN2_ABSORPTION_ROOT="$good" bash "$script" --scope all >/dev/null; then
    pass "self-test valid fixture passes"
  else
    fail "self-test valid fixture should pass"
  fi

  make_fixture "$bad"
  sed -i 's/red attack//g' "$bad/plugins/dvandva/skills/testing/SKILL.md"
  local output rc
  output="$(DVANDVA_RUN2_ABSORPTION_ROOT="$bad" bash "$script" --scope testing 2>&1)"
  rc=$?
  if [[ "$rc" -ne 0 && "$output" == *"red attack"* ]]; then
    pass "self-test missing requirement fails"
  else
    fail "self-test missing requirement should fail and mention red attack"
  fi

  output="$(DVANDVA_RUN2_ABSORPTION_ROOT="$bad/does-not-exist" bash "$script" --scope testing 2>&1)"
  rc=$?
  if [[ "$rc" -ne 0 && "$output" == *"absorption root"* ]]; then
    pass "self-test invalid root fails early"
  else
    fail "self-test invalid root should fail early with root error"
  fi

  [[ "$failures" -eq 0 ]]
}

ROOT_DIR="$(resolve_root "${DVANDVA_RUN2_ABSORPTION_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}")" || exit 1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --scope)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      SCOPE="$2"
      shift 2
      ;;
    --self-test)
      self_test
      exit $?
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

case "$SCOPE" in
  all)
    check_testing
    check_understanding
    check_worktree
    ;;
  vadi)
    check_testing
    check_worktree
    ;;
  testing)
    check_testing
    ;;
  understanding)
    check_understanding
    ;;
  worktree)
    check_worktree
    ;;
  *)
    usage
    exit 2
    ;;
esac

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
