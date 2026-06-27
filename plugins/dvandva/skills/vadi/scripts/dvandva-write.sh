#!/usr/bin/env bash
# Validated, atomic install of a Dvandva baton candidate, plus auto-snapshot.
#
# Usage: dvandva-write.sh <path-to-baton.json> <path-to-candidate.json>
#
# The active agent writes the complete next baton to a candidate file
# (canonical: .dvandva/baton.next.json), then runs this helper. The helper
# validates the candidate (schema, required keys, status enum, transition
# legality, checkpoint arithmetic), installs it atomically (tmp + same-dir
# mv), then snapshots via the sibling dvandva-snapshot.sh.
#
# This helper is bundled as a real executable inside each runtime skill:
#   plugins/dvandva/skills/vadi/scripts/dvandva-write.sh
#   plugins/dvandva/skills/prativadi/scripts/dvandva-write.sh
# The two copies must stay byte-identical so copy-installs and plugin installs
# keep the helper findable via ${CLAUDE_SKILL_DIR}/scripts/dvandva-write.sh.
# scripts/test-dvandva-write.sh fails if either runtime copy is missing or drifts.
#
# The transition whitelist below mirrors references/state-transition-table.md.
# scripts/test-dvandva-write.sh asserts every documented edge, so drift
# between this script and the table fails tests.
#
# Exit codes:
#   0  candidate validated, installed, snapshot written
#   2  usage error
#   21 candidate file missing
#   22 candidate is not valid JSON
#   23 candidate fails schema/required-keys/enum checks
#   24 illegal state transition (incl. checkpoint and question-field rules)
#   25 current baton exists but is unparseable (never overwritten)
#   26 install failed (cp/mv error; baton unchanged)
#   30 candidate installed but snapshot failed (baton IS updated)
set -u

if [[ $# -ne 2 ]]; then
  echo "Usage: dvandva-write.sh <path-to-baton.json> <path-to-candidate.json>" >&2
  exit 2
fi

BATON_FILE="$1"
CANDIDATE_FILE="$2"

is_safe_run_id() {
  local value="$1"
  [[ "$value" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] && [[ "$value" != *".."* ]]
}

v2_expected_assignee() {
  case "$1" in
    research_drafting|research_revision|spec_drafting|spec_revision|implementing|test_creation|deslop|phase_fixing|review_of_review)
      echo "vadi"
      ;;
    research_review|spec_review|deep_review|phase_review|counter_review)
      echo "prativadi"
      ;;
    human_question|human_decision)
      echo "human"
      ;;
    *)
      echo ""
      ;;
  esac
}

if [[ ! -f "$CANDIDATE_FILE" ]]; then
  echo "DVANDVA_WRITE missing candidate=$CANDIDATE_FILE" >&2
  exit 21
fi

if ! jq empty "$CANDIDATE_FILE" 2>/dev/null; then
  echo "DVANDVA_WRITE invalid_json candidate=$CANDIDATE_FILE" >&2
  exit 22
fi

schema="$(jq -r '.schema // ""' "$CANDIDATE_FILE")"
case "$schema" in
  dvandva.baton.v1|dvandva.baton.v2) ;;
  *)
    echo "DVANDVA_WRITE schema_mismatch candidate=$CANDIDATE_FILE want=dvandva.baton.v1|dvandva.baton.v2" >&2
    exit 23
    ;;
esac

REQUIRED_KEYS=(schema updated_at mode run_mode phase total_phases status assignee current_engine review_target plan_ref master_plan_locked question resume_assignee resume_status disagreement_round disagreement_cap turn_cap branch checkpoint allow_commit allow_push allow_pr vadi_final_approval prativadi_final_approval final_commit pushed_ref summary changed_paths verification findings narrow_fixups vadi_counter deferred blockers next_action)
if [[ "$schema" == "dvandva.baton.v2" ]]; then
  REQUIRED_KEYS+=(run_id original_ask research_ref work_split verification_matrix)
fi

for key in "${REQUIRED_KEYS[@]}"; do
  if ! jq -e "has(\"$key\")" "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE missing_key key=$key candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
done

new_status="$(jq -r '.status // ""' "$CANDIDATE_FILE")"
new_assignee="$(jq -r '.assignee // ""' "$CANDIDATE_FILE")"

if [[ "$schema" == "dvandva.baton.v2" ]]; then
  new_run_id="$(jq -r '.run_id // ""' "$CANDIDATE_FILE")"
  if ! jq -e '(.run_id | type) == "string" and (.run_id | length) > 0' "$CANDIDATE_FILE" >/dev/null 2>&1 || ! is_safe_run_id "$new_run_id"; then
    echo "DVANDVA_WRITE bad_run_id candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '(.original_ask | type) == "string" and (.original_ask | length) > 0' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_original_ask candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '((.work_split | type) == "array" or (.work_split | type) == "object") and (.work_split | length) > 0' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_work_split candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '((.verification_matrix | type) == "array" or (.verification_matrix | type) == "object") and (.verification_matrix | length) > 0' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_verification_matrix candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if [[ "$new_status" != "research_drafting" && "$new_status" != "human_question" && "$new_status" != "human_decision" ]] \
    && ! jq -e '(.research_ref | type) == "string" and (.research_ref | length) > 0' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_research_ref candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
fi

# Type gate before extraction: jq -r strips quotes, so a JSON string "5"
# would pass the integer regex below, and a string "08" would error out
# of bash arithmetic in a way [[ ]] treats as false, skipping the
# checkpoint+1 guard. Reject non-number checkpoints outright.
if ! jq -e '(.checkpoint | type) == "number"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
  echo "DVANDVA_WRITE bad_checkpoint_type candidate=$CANDIDATE_FILE" >&2
  exit 23
fi
new_checkpoint="$(jq -r '.checkpoint' "$CANDIDATE_FILE")"

case "$schema:$new_status" in
  dvandva.baton.v1:spec_drafting|dvandva.baton.v1:spec_review|dvandva.baton.v1:spec_revision|dvandva.baton.v1:human_question|dvandva.baton.v1:implementing|dvandva.baton.v1:phase_review|dvandva.baton.v1:phase_fixing|dvandva.baton.v1:review_of_review|dvandva.baton.v1:counter_review|dvandva.baton.v1:human_decision|dvandva.baton.v1:done) ;;
  dvandva.baton.v2:research_drafting|dvandva.baton.v2:research_review|dvandva.baton.v2:research_revision|dvandva.baton.v2:spec_drafting|dvandva.baton.v2:spec_review|dvandva.baton.v2:spec_revision|dvandva.baton.v2:human_question|dvandva.baton.v2:implementing|dvandva.baton.v2:test_creation|dvandva.baton.v2:deep_review|dvandva.baton.v2:deslop|dvandva.baton.v2:phase_review|dvandva.baton.v2:phase_fixing|dvandva.baton.v2:review_of_review|dvandva.baton.v2:counter_review|dvandva.baton.v2:human_decision|dvandva.baton.v2:done) ;;
  *)
    echo "DVANDVA_WRITE bad_status status=$new_status candidate=$CANDIDATE_FILE" >&2
    exit 23
    ;;
esac

if [[ -z "$new_assignee" || "$new_assignee" == "null" ]]; then
  echo "DVANDVA_WRITE bad_assignee candidate=$CANDIDATE_FILE" >&2
  exit 23
fi

if [[ "$schema" == "dvandva.baton.v2" ]]; then
  expected_assignee="$(v2_expected_assignee "$new_status")"
  if [[ -n "$expected_assignee" && "$new_assignee" != "$expected_assignee" ]]; then
    echo "DVANDVA_WRITE bad_assignee_owner status=$new_status want=$expected_assignee got=$new_assignee candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
fi

if ! [[ "$new_checkpoint" =~ ^[0-9]+$ ]]; then
  echo "DVANDVA_WRITE bad_checkpoint checkpoint=$new_checkpoint candidate=$CANDIDATE_FILE" >&2
  exit 23
fi

read -r cand_q_null cand_ra_null cand_rs_null <<< "$(jq -r '[(.question == null), (.resume_assignee == null), (.resume_status == null)] | map(tostring) | join(" ")' "$CANDIDATE_FILE")"

legal=0
reason=""

if [[ ! -f "$BATON_FILE" ]]; then
  # Scaffold: only the vadi may create the very first baton.
  if [[ "$schema" == "dvandva.baton.v1" && "$new_status" == "spec_drafting" && "$new_assignee" == "vadi" && "$new_checkpoint" -eq 0 ]]; then
    legal=1
  elif [[ "$schema" == "dvandva.baton.v2" && "$new_status" == "research_drafting" && "$new_assignee" == "vadi" && "$new_checkpoint" -eq 0 ]]; then
    legal=1
  else
    reason="scaffold requires v1 status=spec_drafting or v2 status=research_drafting with assignee=vadi checkpoint=0, got schema=$schema status=$new_status assignee=$new_assignee checkpoint=$new_checkpoint"
  fi
else
  # Defense-in-depth: a current baton with a non-number checkpoint is
  # corrupt state from outside this helper — refuse rather than risk
  # octal/coercion artifacts in the +1 arithmetic below.
  if ! jq -e '(.checkpoint | type) == "number"' "$BATON_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE current_baton_unparseable file=$BATON_FILE bad_checkpoint_type=true" >&2
    exit 25
  fi

  # Use a non-whitespace delimiter: bash collapses adjacent IFS whitespace,
  # which would shift run_id when resume fields are empty.
  if ! cur="$(jq -r '[.schema // "", .status // "", (.checkpoint // -1 | tostring), (.master_plan_locked // false | tostring), .resume_assignee // "", .resume_status // "", .run_id // ""] | join("\u001f")' "$BATON_FILE" 2>/dev/null)"; then
    echo "DVANDVA_WRITE current_baton_unparseable file=$BATON_FILE refusing_to_overwrite=true" >&2
    exit 25
  fi
  IFS=$'\x1f' read -r cur_schema cur_status cur_checkpoint cur_locked cur_resume_assignee cur_resume_status cur_run_id <<< "$cur"

  case "$cur_schema" in
    dvandva.baton.v1|dvandva.baton.v2) ;;
    *)
      echo "DVANDVA_WRITE current_baton_unparseable file=$BATON_FILE bad_schema=$cur_schema" >&2
      exit 25
      ;;
  esac

  if ! [[ "$cur_checkpoint" =~ ^-?[0-9]+$ ]]; then
    echo "DVANDVA_WRITE current_baton_unparseable file=$BATON_FILE bad_checkpoint=$cur_checkpoint" >&2
    exit 25
  fi

  if [[ "$cur_schema" == "dvandva.baton.v2" ]] && ! is_safe_run_id "$cur_run_id"; then
    echo "DVANDVA_WRITE current_baton_unparseable file=$BATON_FILE bad_run_id=$cur_run_id" >&2
    exit 25
  fi

  # Precedence is load-bearing — do not reorder:
  #   1. checkpoint+1   2. same-status ban   3. from-human_question
  #   4. to-human_decision (universal)   5. from-human_decision
  #   6. to-human_question (spec-only, unlocked, fields set)   7. edge whitelist
  # e.g. moving the same-status ban below the human branches would silently
  # legalize human_decision->human_decision rewrites.
  if [[ "$cur_schema" != "$schema" ]]; then
    reason="schema_change current=$cur_schema candidate=$schema"
  elif [[ "$schema" == "dvandva.baton.v2" && "$cur_run_id" != "$new_run_id" ]]; then
    reason="run_id_change current=$cur_run_id candidate=$new_run_id"
  elif [[ "$new_checkpoint" -ne $((cur_checkpoint + 1)) ]]; then
    reason="checkpoint must be $((cur_checkpoint + 1)), got $new_checkpoint"
  elif [[ "$new_status" == "$cur_status" ]]; then
    reason="same-status rewrite (one baton write per handoff)"
  elif [[ "$cur_status" == "human_question" ]]; then
    if [[ "$new_status" == "human_decision" ]]; then
      legal=1
    elif [[ "$new_status" == "$cur_resume_status" && "$new_assignee" == "$cur_resume_assignee" && "$cand_q_null" == "true" && "$cand_ra_null" == "true" && "$cand_rs_null" == "true" ]]; then
      legal=1
    else
      reason="human_question resume must restore status=$cur_resume_status assignee=$cur_resume_assignee and clear question/resume fields"
    fi
  elif [[ "$new_status" == "human_decision" ]]; then
    legal=1   # universal escalation
  elif [[ "$cur_status" == "human_decision" ]]; then
    legal=1   # human-authorized resume to any state
  elif [[ "$new_status" == "human_question" ]]; then
    if [[ "$cur_locked" == "true" ]]; then
      reason="human_question is only legal before master_plan_locked"
    elif [[ "$cur_status" != "spec_drafting" && "$cur_status" != "spec_review" && "$cur_status" != "spec_revision" && "$cur_status" != "research_drafting" && "$cur_status" != "research_review" && "$cur_status" != "research_revision" ]]; then
      reason="human_question only enters from spec or research states, not $cur_status"
    elif [[ "$cand_q_null" == "true" || "$cand_ra_null" == "true" || "$cand_rs_null" == "true" ]]; then
      reason="human_question requires non-null question, resume_assignee, resume_status"
    else
      legal=1
    fi
  else
    case "${cur_status}:${new_status}" in
      research_drafting:research_review|research_review:research_revision|research_revision:research_review|research_review:spec_drafting) legal=1 ;;
      spec_drafting:spec_review|spec_review:spec_revision|spec_review:implementing|spec_revision:spec_review) legal=1 ;;
      implementing:test_creation|test_creation:deep_review|deep_review:phase_fixing|deep_review:deslop|phase_fixing:test_creation|deslop:phase_fixing|deslop:implementing|deslop:done) legal=1 ;;
      implementing:phase_review|phase_review:phase_fixing|phase_review:review_of_review|phase_review:implementing|phase_review:done|phase_fixing:phase_review) legal=1 ;;
      review_of_review:implementing|review_of_review:done|review_of_review:counter_review|counter_review:implementing|counter_review:done|counter_review:review_of_review) legal=1 ;;
      *) reason="no legal edge ${cur_status}->${new_status}" ;;
    esac
  fi
fi

if [[ "$legal" -ne 1 ]]; then
  echo "DVANDVA_WRITE illegal_transition $reason" >&2
  exit 24
fi

BATON_DIR="$(dirname "$BATON_FILE")"
mkdir -p "$BATON_DIR"
# Sweep tmp files orphaned by a killed writer; inert to readers but clutter.
# Note: the glob would also hit a LIVE concurrent writer's tmp — acceptable
# because the protocol's assignee field makes writes single-owner by design.
rm -f "$BATON_DIR"/.baton.json.tmp.* 2>/dev/null
TMP_FILE="$BATON_DIR/.baton.json.tmp.$$"

if ! cp "$CANDIDATE_FILE" "$TMP_FILE"; then
  echo "DVANDVA_WRITE install_failed stage=cp" >&2
  rm -f "$TMP_FILE"
  exit 26
fi

if ! mv -f "$TMP_FILE" "$BATON_FILE"; then
  echo "DVANDVA_WRITE install_failed stage=mv" >&2
  rm -f "$TMP_FILE"
  exit 26
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if ! "$SCRIPT_DIR/dvandva-snapshot.sh" "$BATON_FILE"; then
  echo "DVANDVA_WRITE snapshot_failed file=$BATON_FILE baton_is_installed=true" >&2
  exit 30
fi

new_phase="$(jq -r '.phase' "$CANDIDATE_FILE")"
echo "DVANDVA_WRITE ok status=$new_status assignee=$new_assignee phase=$new_phase checkpoint=$new_checkpoint"
exit 0
