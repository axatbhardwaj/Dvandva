#!/usr/bin/env bash
# Static checks for Dvandva role preflight wording.  These protect the process
# contract that cannot be fully enforced by dvandva-write.sh.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VADI="$ROOT_DIR/plugins/dvandva/skills/vadi/SKILL.md"
PRATIVADI="$ROOT_DIR/plugins/dvandva/skills/prativadi/SKILL.md"
COMMAND_VADI="$ROOT_DIR/plugins/dvandva/commands/vadi.md"
COMMAND_PRATIVADI="$ROOT_DIR/plugins/dvandva/commands/prativadi.md"
STATE_REF="$ROOT_DIR/plugins/dvandva/references/state-transition-table.md"

failures=0

pass() {
  printf 'PASS: %s\n' "$*"
}

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  failures=$((failures + 1))
}

require_match() {
  local file="$1" regex="$2" message="$3"
  local text
  text="$(tr '\n' ' ' < "$file")"
  if printf '%s\n' "$text" | grep -Eiq -- "$regex"; then
    pass "$message"
  else
    fail "$message"
  fi
}

reject_match() {
  local file="$1" regex="$2" message="$3"
  local text
  text="$(tr '\n' ' ' < "$file")"
  if printf '%s\n' "$text" | grep -Eiq -- "$regex"; then
    fail "$message"
  else
    pass "$message"
  fi
}

for file in "$VADI" "$PRATIVADI"; do
  role="$(basename "$(dirname "$file")")"
  require_match "$file" \
    'Baton creation/resume discovery is mandatory before active work' \
    "$role skill makes baton discovery a hard preflight gate"
  require_match "$file" \
    'Resolve the active baton path before reading or writing' \
    "$role skill resolves baton before reads/writes"
  require_match "$file" \
    'before active non-wait work' \
    "$role skill runs hook preflight only after baton resolution"
  require_match "$file" \
    "export[[:space:]]+DVANDVA_ROLE=$role" \
    "$role skill exports DVANDVA_ROLE=$role"
  require_match "$file" \
    'asserts?[[:space:]]+[`]?DVANDVA_ROLE='"$role"'[`]?' \
    "$role skill asserts DVANDVA_ROLE=$role"
  require_match "$file" \
    'detects?[[:space:]]+Dvandva hook adoption|hook adoption status' \
    "$role skill detects hook adoption instead of forcing it"
  require_match "$file" \
    'dvandva\.priorHooksPath' \
    "$role skill records prior hooksPath as dvandva.priorHooksPath and restores on uninstall"
  require_match "$file" \
    'Checkpoint commits require Dvandva hook adoption' \
    "$role skill gates checkpoint commits on adopted hooks"
  require_match "$file" \
    'Final commits require Dvandva hook adoption' \
    "$role skill gates final commits on adopted hooks"
  reject_match "$file" \
    'bash[[:space:]]+scripts/install-dvandva-hooks\.sh' \
    "$role skill does not require target-repo scripts/install-dvandva-hooks.sh"
  require_match "$file" \
    'termination_review' \
    "$role skill documents the multipart termination review state"
  require_match "$file" \
    'both roles.*stop|stop.*both roles|shared termination|multipart termination' \
    "$role skill says terminal stop is a shared two-role decision"
  require_match "$file" \
    'run_explainer_reviews' \
    "$role skill surfaces and gates final run explainer reviews"
  require_match "$file" \
    'approval and explainer-review ownership|explainer-review and approval ownership' \
    "$role skill says helper-enforced ownership covers explainer reviews"
  require_match "$file" \
    'human_(decision|question).*(paired run pause|stop both roles together)|stop both roles together.*human_(decision|question)|paired run pause.*human_(decision|question)' \
    "$role skill says human_question and human_decision are paired run pauses that stop both roles together"
  require_match "$file" \
    'newer sibling.*human_(decision|question)|human_(decision|question).*(newer sibling|sibling run)' \
    "$role skill requires newer sibling human-intervention propagation"
  require_match "$file" \
    'sibling.{0,160}human_question.{0,160}question.{0,160}resume_assignee.{0,160}resume_status|human_question.{0,160}sibling.{0,160}question.{0,160}resume_assignee.{0,160}resume_status|question.{0,160}resume_assignee.{0,160}resume_status.{0,160}sibling.{0,160}human_question' \
    "$role skill preserves sibling human_question question and resume metadata"
  require_match "$file" \
    '--since-checkpoint' \
    "$role skill uses checkpoint-gated wait after handoff"
done

for file in "$COMMAND_VADI" "$COMMAND_PRATIVADI"; do
  command_role="$(basename "$file" .md)"
  require_match "$file" \
    'post-handshake "done"|post-handshake done' \
    "$command_role command distinguishes post-handshake done from final approval"
  require_match "$file" \
    'termination_review' \
    "$command_role command documents termination_review as active"
  require_match "$file" \
    'keep polling or stop together|both roles keep polling|both approve' \
    "$command_role command says roles stop together only after shared approval"
  require_match "$file" \
    'run_explainer_reviews' \
    "$command_role command requires both explainer reviews before done"
  reject_match "$file" \
    'Continue the walkaway run until the resolved Dvandva baton status is "done", "human_question", or "human_decision"' \
    "$command_role command rejects one-step terminal stop wording"
  require_match "$file" \
    'human_(decision|question).*(paired run pause|stop both roles together)|stop both roles together.*human_(decision|question)|paired run pause.*human_(decision|question)' \
    "$command_role command says human_question and human_decision are paired run pauses that stop both roles together"
  require_match "$file" \
    'newer sibling.*human_(decision|question)|human_(decision|question).*(newer sibling|sibling run)' \
    "$command_role command requires newer sibling human-intervention propagation"
  require_match "$file" \
    '--since-checkpoint' \
    "$command_role command uses checkpoint-gated wait after handoff"
done

require_match "$VADI" \
  'human_question.*resumable for discovery|resumable for discovery.*human_question' \
  "vadi skill treats human_question as resumable during discovery"
reject_match "$VADI" \
  'only terminal `done`/`human_decision`/`human_question` archives remain, auto-create' \
  "vadi skill does not classify human_question as only a terminal archive"
require_match "$VADI" \
  'Research fixbacks set .*`phase: "research"`.*`status: "research_review"`.*`assignee: "prativadi"`.*`review_target: "research"`' \
  "vadi skill emits helper-valid research phase_fixing fixbacks"
require_match "$VADI" \
  'Review fixbacks set .*`phase: "review"`.*`status: "deep_review"`.*`assignee: "prativadi"`.*`review_target: null`' \
  "vadi skill emits helper-valid review phase_fixing fixbacks"
reject_match "$VADI" \
  'Keep the current mode phase \(`<current N>`, `"spec"`, or `"review"`\)' \
  "vadi skill does not keep phase=spec for research_review fixbacks"

require_match "$PRATIVADI" \
  'mode-appropriate terminal artifact \(`run_explainer_ref`, `research_ref` plus conditional `plan_ref`, or `review_ref`\)' \
  "prativadi skill termination review reads the mode-appropriate artifact"
require_match "$PRATIVADI" \
  'Development runs require .*run_explainer_ref.*research runs require .*research_ref.*plan_ref.*review runs require .*review_ref' \
  "prativadi final ship rule is mode-conditional"
require_match "$PRATIVADI" \
  'run_explainer_reviews.*vadi.*prativadi|vadi.*prativadi.*run_explainer_reviews' \
  "prativadi final ship rule requires both explainer reviews"
require_match "$PRATIVADI" \
  'one-date run explainer.*YYYY-MM-DD-<run_id>-explainer\.html.*<run_id>-explainer\.html.*never add a second date prefix' \
  "prativadi final ship rule documents the one-date explainer convention"
reject_match "$PRATIVADI" \
  'A final dark self-contained run explainer exists at `\./superpowers/run-reports/YYYY-MM-DD-<run_id>-explainer\.html`' \
  "prativadi final ship rule is not development-artifact-only"

reject_match "$STATE_REF" \
  'role preflight exports and asserts `DVANDVA_ROLE=<role>`,[[:space:]]*`scripts/install-dvandva-hooks\.sh` sets and verifies `core\.hooksPath=\.githooks`' \
  "state reference no longer documents unconditional target-repo hook install"
require_match "$STATE_REF" \
  'Checkpoint commits require Dvandva hook adoption' \
  "state reference gates checkpoint commits on adopted hooks"
require_match "$STATE_REF" \
  'termination_review' \
  "state reference documents termination_review"
require_match "$STATE_REF" \
  'deslop.*termination_review|termination_review.*done' \
  "state reference routes final done through termination_review"
require_match "$STATE_REF" \
  'run_explainer_reviews.*vadi.*prativadi|vadi.*prativadi.*run_explainer_reviews' \
  "state reference requires both final explainer reviews"
require_match "$STATE_REF" \
  'approval and explainer-review ownership|explainer-review ownership|run_explainer_reviews.{0,120}DVANDVA_ROLE.{0,120}ownership|DVANDVA_ROLE.{0,120}run_explainer_reviews.{0,120}ownership' \
  "state reference documents DVANDVA_ROLE ownership for explainer reviews"
require_match "$STATE_REF" \
  'human_(decision|question).*(paired run pause|stop both roles together)|stop both roles together.*human_(decision|question)|paired run pause.*human_(decision|question)' \
  "state reference says human_question and human_decision are paired run pauses that stop both roles together"
require_match "$STATE_REF" \
  'newer sibling.*human_(decision|question)|human_(decision|question).*(newer sibling|sibling run)' \
  "state reference requires newer sibling human-intervention propagation"
require_match "$STATE_REF" \
  'sibling.{0,160}human_question.{0,160}question.{0,160}resume_assignee.{0,160}resume_status|human_question.{0,160}sibling.{0,160}question.{0,160}resume_assignee.{0,160}resume_status|question.{0,160}resume_assignee.{0,160}resume_status.{0,160}sibling.{0,160}human_question' \
  "state reference preserves sibling human_question question and resume metadata"
require_match "$STATE_REF" \
  'termination_review.*active|active.*termination_review' \
  "state reference preserves termination_review as active and non-terminal"
require_match "$STATE_REF" \
  '--since-checkpoint' \
  "state reference documents checkpoint-gated handoff waits"

# Static README coverage: reject stale Run-4 guidance that predates the
# delegating-wrapper coexistence model.
README="$ROOT_DIR/README.md"
reject_match "$README" \
  'bash[[:space:]]+scripts/install-dvandva-hooks\.sh' \
  "README does not document bash scripts/install-dvandva-hooks.sh as a user instruction"
reject_match "$README" \
  'core\.hooksPath=\.githooks' \
  "README does not document core.hooksPath=.githooks as the adoption target"

# README accuracy: there is NO repo-root scripts/dvandva-preflight.sh — the turn
# preflight ships per-role inside the plugin tree
# (plugins/dvandva/skills/<role>/scripts/dvandva-preflight.sh) and is invoked via
# the role skill.  Reject the stale root-script invocation and require the real
# per-role/skill-invoked path so the README cannot drift back to a path that
# does not exist in a plugin-installed target repo.
if [[ ! -f "$ROOT_DIR/scripts/dvandva-preflight.sh" ]]; then
  pass "no repo-root scripts/dvandva-preflight.sh exists (preflight ships per-role)"
else
  fail "unexpected repo-root scripts/dvandva-preflight.sh (preflight should ship per-role only)"
fi
reject_match "$README" \
  'bash[[:space:]]+scripts/dvandva-preflight\.sh' \
  "README does not point users at a nonexistent root scripts/dvandva-preflight.sh"
require_match "$README" \
  'plugins/dvandva/skills/<role>/scripts/dvandva-preflight\.sh|per-role.*dvandva-preflight\.sh|dvandva-preflight\.sh.*ships per-role' \
  "README documents the per-role/skill-invoked turn preflight path"
require_match "$README" \
  'termination_review' \
  "README documents multipart termination review"
require_match "$README" \
  'approval and explainer-review ownership|explainer-review ownership|run_explainer_reviews.{0,120}DVANDVA_ROLE.{0,120}ownership|DVANDVA_ROLE.{0,120}run_explainer_reviews.{0,120}ownership' \
  "README documents DVANDVA_ROLE ownership for explainer reviews"

for file in "$ROOT_DIR/product.md" "$ROOT_DIR/docs/protocol/local-baton-channel.md" "$ROOT_DIR/plugins/dvandva/references/local-baton-channel.md"; do
  label="${file#$ROOT_DIR/}"
  reject_match "$file" \
    'bash[[:space:]]+scripts/install-dvandva-hooks\.sh' \
    "$label does not document repo-root install-dvandva-hooks as the role preflight"
  reject_match "$file" \
    'core\.hooksPath=\.githooks' \
    "$label does not document core.hooksPath=.githooks as the adoption target"
  require_match "$file" \
    '\.dvandva/githooks' \
    "$label documents the .dvandva/githooks delegating wrapper"
  require_match "$file" \
    'run_explainer_reviews' \
    "$label documents final explainer review evidence"
  require_match "$file" \
    '--since-checkpoint' \
    "$label documents checkpoint-gated handoff waits"
done

if [[ "$failures" -gt 0 ]]; then
  exit 1
fi

exit 0
