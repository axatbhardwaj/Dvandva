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
    parallel_implementing|cross_review|cross_fixing)
      echo "team"
      ;;
    research_review|spec_review|deep_review|phase_review|counter_review)
      echo "prativadi"
      ;;
    human_question|human_decision|done)
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
  REQUIRED_KEYS+=(run_id original_ask research_ref run_explainer_ref active_roles agent_instances work_split subagent_tracks verification_matrix)
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
  if ! jq -e '
    (.active_roles | type) == "array" and
    all(.active_roles[]; . == "vadi" or . == "prativadi") and
    ((.active_roles | unique | length) == (.active_roles | length))
  ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_active_roles candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '
    def nonblank:
      (type == "string") and test("[^[:space:]]");
    def safe_id:
      (type == "string") and
      (length > 0) and
      test("^[A-Za-z0-9][A-Za-z0-9._-]*$") and
      (contains("..") | not);
    def safe_rel_path:
      (type == "string") and
      (length > 0) and
      (startswith("/") | not) and
      (contains("//") | not) and
      ((split("/") | all(. != "" and . != "." and . != "..")));
    def valid_model:
      . == "opus-class|gpt-5.5" or
      . == "sonnet-class|gpt-5.4" or
      . == "opus" or
      . == "sonnet" or
      . == "gpt-5.5" or
      . == "gpt-5.4";
    def valid_permission:
      . == "readonly" or
      . == "verify-only" or
      . == "edit-scoped" or
      . == "write-artifact-only";
    def generated_instance:
      (.agent_kind // "") == "generated" or has("parent_role") or has("permission_class") or has("model_class");
    (.agent_instances | type) == "array" and
    (([.agent_instances[]?.id] | length) == ([.agent_instances[]?.id] | unique | length)) and
    all(.agent_instances[]?;
      (.id | safe_id) and
      (
        if generated_instance then
          ((.parent_role == "vadi") or (.parent_role == "prativadi")) and
          (.spawned_by | nonblank) and
          ((.spawned_at_checkpoint | type) == "number") and
          (((.phase | type) == "string" or (.phase | type) == "number") and ((.phase | tostring | length) > 0)) and
          (.purpose | nonblank) and
          ((.agent_kind // "") == "generated") and
          ((.model_class | valid_model)) and
          ((.permission_class | valid_permission)) and
          ((.status // "") as $status | ["planned", "running", "closed", "rejected", "collapsed"] | index($status) != null) and
          ((.work_item_ids | type) == "array") and
          ((.read_paths | type) == "array") and all(.read_paths[]; safe_rel_path) and
          ((.write_paths | type) == "array") and all(.write_paths[]; safe_rel_path) and
          ((.depends_on | type) == "array") and
          ((.output_refs | type) == "array") and
          ((.evidence_refs | type) == "array") and
          ((.base_checkpoint | type) == "number") and
          (
            if .status == "closed" then
              (.closed_at | nonblank) and
              (.result | nonblank) and
              ((.work_item_ids | length) > 0) and
              ((.evidence_refs | length) > 0) and
              any(.evidence_refs[]; (type == "string") and startswith("closed:"))
            else
              true
            end
          )
        else
          true
        end
      )
    )
  ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_agent_instances candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '
    def generated_live:
      (.agent_kind // "") == "generated" and
      ((.status // "") != "rejected") and
      ((.status // "") != "collapsed");
    def path_overlap($left; $right):
      ($left == $right) or
      ($left | startswith($right + "/")) or
      ($right | startswith($left + "/"));
    def overlap($a; $b):
      any(($a.write_paths // [])[]; . as $path | any(($b.write_paths // [])[]; path_overlap($path; .)));
    def serialized($a; $b):
      (($a.conflict_group // "") != "") and
      (($a.conflict_group // "") == ($b.conflict_group // "")) and
      (((($a.depends_on // []) | index($b.id)) != null) or ((($b.depends_on // []) | index($a.id)) != null));
    [ .agent_instances[]? | select(generated_live) | select((.write_paths | length) > 0) ] as $instances |
    [
      range(0; ($instances | length)) as $i |
      range($i + 1; ($instances | length)) as $j |
      ($instances[$i]) as $a |
      ($instances[$j]) as $b |
      select(overlap($a; $b) and (serialized($a; $b) | not))
    ] | length == 0
  ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_agent_instances_write_paths candidate=$CANDIDATE_FILE" >&2
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
  if ! jq -e '
    ((.subagent_tracks | type) == "array") and
    (.subagent_tracks | length) > 0 and
    all(.subagent_tracks[];
      ((.id | type) == "string" and (.id | length) > 0) and
      ((.phase | type) == "string" or (.phase | type) == "number") and
      ((.phase | tostring | length) > 0) and
      ((.status | type) == "string" and (.status | length) > 0) and
      ((.track | type) == "string" and (.track | length) > 0) and
      ((.owner | type) == "string" and (.owner | length) > 0) and
      ((.parallelized | type) == "boolean") and
      ((.rationale | type) == "string" and (.rationale | length) > 0) and
      ((.inputs | type) == "array") and
      ((.outputs | type) == "array") and
      ((.evidence_refs | type) == "array") and
      ((.result | type) == "string" and (.result | length) > 0)
    )
  ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_subagent_tracks candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '
    . as $root |
    def static_owner:
      test("^dvandva-(researcher|architect|implementer|test-creator|cross-reviewer|adversarial-analyst|deep-reviewer|deslopper|sandbox-verifier|baton-auditor|security-auditor|integration-checker|debugger|doc-verifier|pattern-mapper)$");
    def legacy_owner:
      test("^(adversarial-analyst|quality-reviewer|sandbox-executor|architect|developer)$");
    def coordinator_owner:
      . == "vadi" or . == "prativadi" or . == "team" or . == "human";
    def closed_agent_instance($track):
      any($root.agent_instances[]?;
        (.id == $track.owner) and
        ((.agent_kind // "") == "generated") and
        (
          (($track.owner_role // "") == "") or
          (($track.owner_role // "") == (.parent_role // ""))
        ) and
        (.status == "closed") and
        ((.output_refs | length) > 0) and
        ((.evidence_refs | length) > 0) and
        any(.evidence_refs[]; (type == "string") and startswith("closed:"))
      );
    all(.subagent_tracks[];
      if ((.owner | coordinator_owner) or (.owner | static_owner) or (.owner | legacy_owner)) then
        if .parallelized then
          (((.outputs | length) > 0) or ((.evidence_refs | length) > 0))
        else
          true
        end
      else
        (closed_agent_instance(.) and (((.outputs | length) > 0) or ((.evidence_refs | length) > 0)))
      end
    )
  ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    if jq -e '
      def static_owner:
        test("^dvandva-(researcher|architect|implementer|test-creator|cross-reviewer|adversarial-analyst|deep-reviewer|deslopper|sandbox-verifier|baton-auditor|security-auditor|integration-checker|debugger|doc-verifier|pattern-mapper)$");
      def legacy_owner:
        test("^(adversarial-analyst|quality-reviewer|sandbox-executor|architect|developer)$");
      any(.subagent_tracks[];
        ((.owner | static_owner) | not) and
        ((.owner | legacy_owner) | not) and
        ((.owner == "vadi" or .owner == "prativadi" or .owner == "team" or .owner == "human") | not)
      )
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      echo "DVANDVA_WRITE bad_agent_instances candidate=$CANDIDATE_FILE" >&2
    else
      echo "DVANDVA_WRITE bad_subagent_tracks candidate=$CANDIDATE_FILE" >&2
    fi
    exit 23
  fi
  if [[ "$new_status" != "research_drafting" && "$new_status" != "human_question" && "$new_status" != "human_decision" ]] \
    && ! jq -e '(.research_ref | type) == "string" and (.research_ref | length) > 0' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_research_ref candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if [[ "$new_status" == "done" ]] \
    && ! jq -e '(.run_explainer_ref | type) == "string" and (.run_explainer_ref | test("^\\./superpowers/run-reports/[0-9]{4}-[0-9]{2}-[0-9]{2}-[A-Za-z0-9._-]+-explainer\\.html$"))' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_run_explainer_ref candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if [[ "$new_status" == "done" ]]; then
    explainer_ref="$(jq -r '.run_explainer_ref' "$CANDIDATE_FILE")"
    explainer_run_id="$(printf '%s\n' "$explainer_ref" | sed -E 's#^\./superpowers/run-reports/[0-9]{4}-[0-9]{2}-[0-9]{2}-(.*)-explainer\.html$#\1#')"
    if [[ "$explainer_run_id" != "$new_run_id" ]]; then
      echo "DVANDVA_WRITE bad_run_explainer_ref candidate=$CANDIDATE_FILE" >&2
      exit 23
    fi
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
new_phase="$(jq -r '.phase' "$CANDIDATE_FILE")"

case "$schema:$new_status" in
  dvandva.baton.v1:spec_drafting|dvandva.baton.v1:spec_review|dvandva.baton.v1:spec_revision|dvandva.baton.v1:human_question|dvandva.baton.v1:implementing|dvandva.baton.v1:phase_review|dvandva.baton.v1:phase_fixing|dvandva.baton.v1:review_of_review|dvandva.baton.v1:counter_review|dvandva.baton.v1:human_decision|dvandva.baton.v1:done) ;;
  dvandva.baton.v2:research_drafting|dvandva.baton.v2:research_review|dvandva.baton.v2:research_revision|dvandva.baton.v2:spec_drafting|dvandva.baton.v2:spec_review|dvandva.baton.v2:spec_revision|dvandva.baton.v2:human_question|dvandva.baton.v2:implementing|dvandva.baton.v2:parallel_implementing|dvandva.baton.v2:test_creation|dvandva.baton.v2:cross_review|dvandva.baton.v2:cross_fixing|dvandva.baton.v2:deep_review|dvandva.baton.v2:deslop|dvandva.baton.v2:phase_review|dvandva.baton.v2:phase_fixing|dvandva.baton.v2:review_of_review|dvandva.baton.v2:counter_review|dvandva.baton.v2:human_decision|dvandva.baton.v2:done) ;;
  *)
    echo "DVANDVA_WRITE bad_status status=$new_status candidate=$CANDIDATE_FILE" >&2
    exit 23
    ;;
esac

if [[ "$schema" == "dvandva.baton.v2" ]]; then
  case "$new_status" in
    research_drafting|research_review|research_revision)
      if ! jq -e '.phase == "research"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_phase_status status=$new_status candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
      ;;
    spec_drafting|spec_review|spec_revision)
      if ! jq -e '.phase == "spec"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_phase_status status=$new_status candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
      ;;
    implementing|parallel_implementing|test_creation|cross_review|cross_fixing|deep_review|deslop|phase_review|phase_fixing|review_of_review|counter_review|done)
      if ! jq -e '(.phase | type) == "number"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_phase_status status=$new_status candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
      ;;
  esac
fi

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
  case "$new_status" in
    parallel_implementing|cross_review|cross_fixing)
      if ! jq -e '
        (.assignee == "team") and
        ((.active_roles | sort) == ["prativadi", "vadi"])
      ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_active_roles status=$new_status candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
      ;;
    *)
      if ! jq -e '(.active_roles | length) == 0' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_active_roles status=$new_status candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
      ;;
  esac
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
  if ! cur="$(jq -r '[.schema // "", .status // "", (.checkpoint // -1 | tostring), (.master_plan_locked // false | tostring), .resume_assignee // "", .resume_status // "", .run_id // "", (.phase | tostring)] | join("\u001f")' "$BATON_FILE" 2>/dev/null)"; then
    echo "DVANDVA_WRITE current_baton_unparseable file=$BATON_FILE refusing_to_overwrite=true" >&2
    exit 25
  fi
  IFS=$'\x1f' read -r cur_schema cur_status cur_checkpoint cur_locked cur_resume_assignee cur_resume_status cur_run_id cur_phase <<< "$cur"

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
  #   1. checkpoint+1   2. same-status team-sync gate   3. from-human_question
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
    if [[ "$schema" == "dvandva.baton.v2" ]]; then
      case "$new_status" in
        parallel_implementing|cross_review|cross_fixing)
          if [[ "$new_phase" != "$cur_phase" ]]; then
            reason="same-status team sync cannot change phase current=$cur_phase candidate=$new_phase"
          elif jq -e '
            (.assignee == "team") and
            ((.active_roles | sort) == ["prativadi", "vadi"]) and
            ((.summary | type) == "string" and (.summary | test("[^[:space:]]"))) and
            ((.next_action | type) == "string" and (.next_action | test("[^[:space:]]")))
          ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
            legal=1
          else
            reason="same-status team sync requires team assignee, both active_roles, summary, and next_action"
          fi
          ;;
        *)
          reason="same-status rewrite (only v2 team sync checkpoints may keep status)"
          ;;
      esac
    else
      reason="same-status rewrite (one baton write per handoff)"
    fi
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
    case "$schema" in
      dvandva.baton.v1)
        case "${cur_status}:${new_status}" in
          spec_drafting:spec_review|spec_review:spec_revision|spec_review:implementing|spec_revision:spec_review) legal=1 ;;
          implementing:phase_review|phase_review:phase_fixing|phase_review:review_of_review|phase_review:implementing|phase_review:done|phase_fixing:phase_review) legal=1 ;;
          review_of_review:implementing|review_of_review:done|review_of_review:counter_review|counter_review:implementing|counter_review:done|counter_review:review_of_review) legal=1 ;;
          *) reason="no legal edge ${cur_status}->${new_status}" ;;
        esac
        ;;
      dvandva.baton.v2)
        case "${cur_status}:${new_status}" in
          research_drafting:research_review|research_review:research_revision|research_revision:research_review|research_review:spec_drafting) legal=1 ;;
          spec_drafting:spec_review|spec_review:spec_revision|spec_review:parallel_implementing|spec_revision:spec_review) legal=1 ;;
          parallel_implementing:test_creation|test_creation:cross_review|cross_review:cross_fixing|cross_fixing:test_creation|cross_review:deep_review) legal=1 ;;
          deep_review:phase_fixing|deep_review:deslop|phase_fixing:test_creation|deslop:phase_fixing|deslop:parallel_implementing|deslop:done) legal=1 ;;
          *) reason="no legal edge ${cur_status}->${new_status}" ;;
        esac
        ;;
    esac
  fi

  if [[ "$legal" -eq 1 && "$schema" == "dvandva.baton.v2" && "$new_status" == "parallel_implementing" ]]; then
    if ! jq -e '
      . as $root |
      [
        $root.work_split[]? |
        select((.phase | tostring) == ($root.phase | tostring)) |
        select((.chunk_type // .type // "implementation") == "implementation") |
        select(((.owner_role // .owner // "") == "vadi") or ((.owner_role // .owner // "") == "prativadi")) |
        select(((.cross_review_by // "") == "vadi") or ((.cross_review_by // "") == "prativadi")) |
        select((.cross_review_by // "") != (.owner_role // .owner // "")) |
        select((.paths | type) == "array" and (.paths | length) > 0)
      ] as $chunks |
      ($chunks | length) >= 5 and
      any($chunks[]; (.owner_role // .owner // "") == "vadi") and
      any($chunks[]; (.owner_role // .owner // "") == "prativadi")
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      legal=0
      reason="parallel_implementing requires at least five two-team implementation work_split chunks with reciprocal cross_review_by"
      echo "DVANDVA_WRITE bad_parallel_work_split candidate=$CANDIDATE_FILE" >&2
      exit 23
    fi
  fi

  if [[ "$legal" -eq 1 && "$schema" == "dvandva.baton.v2" && "$cur_status" == "parallel_implementing" && "$new_status" == "test_creation" ]]; then
    if ! jq -e '
      . as $root |
      [
        $root.subagent_tracks[]? |
        select((.phase | tostring) == ($root.phase | tostring)) |
        select(.track == "implementation-chunk") |
        select(.status == "completed") |
        select(.result == "passed" or .result == "approved") |
        select(((.owner_role // .role // "") == "vadi") or ((.owner_role // .role // "") == "prativadi")) |
        select(((.outputs | length) > 0) and ((.evidence_refs | length) > 0))
      ] as $tracks |
      ($tracks | length) >= 5 and
      any($tracks[]; (.owner_role // .role // "") == "vadi") and
      any($tracks[]; (.owner_role // .role // "") == "prativadi")
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      legal=0
      reason="parallel_implementing->test_creation requires completed implementation-chunk subagent_tracks for both roles"
    fi
  fi

  if [[ "$legal" -eq 1 && "$schema" == "dvandva.baton.v2" && "$cur_status" == "test_creation" && "$new_status" == "cross_review" ]]; then
    if ! jq -e '
      any(.subagent_tracks[];
        (
          .phase == "test_creation" and
          .track == "test-creation" and
          .owner == "dvandva-test-creator" and
          .status == "completed" and
          (.result == "passed" or .result == "approved") and
          ((.outputs | length) > 0) and
          ((.evidence_refs | length) > 0)
        )
      )
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      legal=0
      reason="test_creation->cross_review requires completed test-creation subagent_track from dvandva-test-creator"
    fi
  fi

  if [[ "$legal" -eq 1 && "$schema" == "dvandva.baton.v2" && "$cur_status" == "cross_review" && "$new_status" == "cross_fixing" ]]; then
    if ! jq -e '
      any(.subagent_tracks[];
        (
          .phase == "cross_review" and
          .track == "cross-review" and
          (((.owner_role // .role // "") == "vadi") or ((.owner_role // .role // "") == "prativadi")) and
          .status == "completed" and
          (.result != "passed" and .result != "approved") and
          ((.outputs | length) > 0) and
          ((.evidence_refs | length) > 0)
        )
      )
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      legal=0
      reason="cross_review->cross_fixing requires completed cross-review subagent_tracks with non-approval evidence"
    fi
  fi

  if [[ "$legal" -eq 1 && "$schema" == "dvandva.baton.v2" && "$cur_status" == "cross_review" && "$new_status" == "deep_review" ]]; then
    if ! jq -e '
      def done_cross($role):
        any(.subagent_tracks[];
          (
            .phase == "cross_review" and
            .track == "cross-review" and
            (.owner_role // .role // "") == $role and
            .status == "completed" and
            (.result == "passed" or .result == "approved") and
            ((.outputs | length) > 0) and
            ((.evidence_refs | length) > 0)
          )
        );
      done_cross("vadi") and done_cross("prativadi")
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      legal=0
      reason="cross_review->deep_review requires completed cross-review subagent_tracks for both roles with phase=\"cross_review\""
    fi
  fi

  if [[ "$legal" -eq 1 && "$schema" == "dvandva.baton.v2" && "$cur_status" == "deep_review" && "$new_status" == "deslop" ]]; then
    if ! jq -e '
      def done_angle($name):
        any(.subagent_tracks[];
          (
            .phase == "deep_review" and
            .track == $name and
            .status == "completed" and
            (.result == "passed" or .result == "approved") and
            ((.outputs | length) > 0) and
            ((.evidence_refs | length) > 0)
          )
        );
      done_angle("correctness-regression") and
      done_angle("test-evidence") and
      done_angle("protocol-handoff")
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      legal=0
      reason="deep_review->deslop requires three completed review-angle subagent_tracks"
    fi
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

echo "DVANDVA_WRITE ok status=$new_status assignee=$new_assignee phase=$new_phase checkpoint=$new_checkpoint"
exit 0
