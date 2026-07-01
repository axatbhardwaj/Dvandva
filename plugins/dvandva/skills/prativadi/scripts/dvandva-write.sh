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
#   27 stale checkpoint (candidate is same or older than current baton)
#   28 lock unavailable: a non-directory squats the lock path (fail-closed; the
#      critical section never runs unlocked, so the write race cannot re-open)
#   29 lock ownership lost: this writer's fencing token was replaced by a peer
#      that stole the (age-timed-out) lock; install aborted, baton unchanged
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

run_explainer_ref_matches_run_id() {
  local ref="$1"
  local run_id="$2"
  local stem=""
  if [[ "$ref" =~ ^\./superpowers/run-reports/([A-Za-z0-9._-]+)-explainer\.html$ ]]; then
    stem="${BASH_REMATCH[1]}"
  else
    return 1
  fi

  if [[ "$run_id" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}- ]]; then
    [[ "$stem" == "$run_id" ]]
  elif [[ "$stem" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}-(.+)$ ]]; then
    [[ "${BASH_REMATCH[1]}" == "$run_id" ]]
  else
    return 1
  fi
}

named_run_dir_id() {
  local path="$1"
  if [[ "$path" =~ (^|/)\.dvandva/runs/([^/]+)/baton\.json$ ]]; then
    printf '%s\n' "${BASH_REMATCH[2]}"
  fi
}

v2_expected_assignee() {
  case "$1" in
    research_drafting|research_revision|spec_drafting|spec_revision|implementing|test_creation|deslop|phase_fixing|review_of_review)
      echo "vadi"
      ;;
    parallel_implementing|cross_review|cross_fixing|termination_review)
      echo "team"
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

canonical_mode() {
  case "$1" in
    development|feature-pr)
      printf '%s\n' "development"
      ;;
    research|review)
      printf '%s\n' "$1"
      ;;
    *)
      return 1
      ;;
  esac
}

profile_rank() {
  case "$1" in
    fast) echo 1 ;;
    standard) echo 2 ;;
    full) echo 3 ;;
    *) return 1 ;;
  esac
}

cross_review_cycle_checkpoint() {
  local current_file="$1"
  local current_checkpoint="$2"
  local history_dir
  local file
  local row
  local rows=""

  history_dir="$(dirname "$current_file")/history"

  if [[ -d "$history_dir" ]]; then
    while IFS= read -r -d '' file; do
      row="$(jq -r --argjson cur "$current_checkpoint" '
        select((.checkpoint | type) == "number" and .checkpoint <= $cur and (.status | type) == "string")
        | [.checkpoint, .status]
        | @tsv
      ' "$file" 2>/dev/null || true)"
      if [[ -n "$row" ]]; then
        rows+="$row"$'\n'
      fi
    done < <(find "$history_dir" -maxdepth 1 -type f -name '*.json' -print0 2>/dev/null)
  fi

  row="$(jq -r '
    select((.checkpoint | type) == "number" and (.status | type) == "string")
    | [.checkpoint, .status]
    | @tsv
  ' "$current_file" 2>/dev/null || true)"
  if [[ -n "$row" ]]; then
    rows+="$row"$'\n'
  fi

  if [[ -z "$rows" ]]; then
    printf '%s\n' "$current_checkpoint"
    return 0
  fi

  printf '%s' "$rows" | sort -n -k1,1 | awk -v cur="$current_checkpoint" '
    $1 <= cur {
      checkpoints[++n] = $1
      statuses[n] = $2
    }
    END {
      cycle = cur
      found = 0
      for (i = n; i >= 1; i--) {
        if (statuses[i] == "cross_review") {
          cycle = checkpoints[i]
          found = 1
        } else if (found) {
          break
        }
      }
      print cycle
    }
  '
}

deep_review_cycle_checkpoint() {
  local current_file="$1"
  local current_checkpoint="$2"
  local history_dir
  local file
  local row
  local rows=""

  history_dir="$(dirname "$current_file")/history"

  if [[ -d "$history_dir" ]]; then
    while IFS= read -r -d '' file; do
      row="$(jq -r --argjson cur "$current_checkpoint" '
        select((.checkpoint | type) == "number" and .checkpoint <= $cur and (.status | type) == "string")
        | [.checkpoint, .status]
        | @tsv
      ' "$file" 2>/dev/null || true)"
      if [[ -n "$row" ]]; then
        rows+="$row"$'\n'
      fi
    done < <(find "$history_dir" -maxdepth 1 -type f -name '*.json' -print0 2>/dev/null)
  fi

  row="$(jq -r '
    select((.checkpoint | type) == "number" and (.status | type) == "string")
    | [.checkpoint, .status]
    | @tsv
  ' "$current_file" 2>/dev/null || true)"
  if [[ -n "$row" ]]; then
    rows+="$row"$'\n'
  fi

  if [[ -z "$rows" ]]; then
    printf '%s\n' "$current_checkpoint"
    return 0
  fi

  printf '%s' "$rows" | sort -n -k1,1 | awk -v cur="$current_checkpoint" '
    $1 <= cur {
      checkpoints[++n] = $1
      statuses[n] = $2
    }
    END {
      cycle = cur
      found = 0
      for (i = n; i >= 1; i--) {
        if (statuses[i] == "deep_review") {
          cycle = checkpoints[i]
          found = 1
        } else if (found) {
          break
        }
      }
      print cycle
    }
  '
}

phase_review_cycle_checkpoint() {
  local current_file="$1"
  local current_checkpoint="$2"
  local current_phase="$3"
  local history_dir
  local file
  local row
  local rows=""

  history_dir="$(dirname "$current_file")/history"

  if [[ -d "$history_dir" ]]; then
    while IFS= read -r -d '' file; do
      row="$(jq -r --argjson cur "$current_checkpoint" '
        select((.checkpoint | type) == "number" and .checkpoint <= $cur and (.status | type) == "string")
        | [.checkpoint, .status, (.phase | tostring)]
        | @tsv
      ' "$file" 2>/dev/null || true)"
      if [[ -n "$row" ]]; then
        rows+="$row"$'\n'
      fi
    done < <(find "$history_dir" -maxdepth 1 -type f -name '*.json' -print0 2>/dev/null)
  fi

  row="$(jq -r '
    select((.checkpoint | type) == "number" and (.status | type) == "string")
    | [.checkpoint, .status, (.phase | tostring)]
    | @tsv
  ' "$current_file" 2>/dev/null || true)"
  if [[ -n "$row" ]]; then
    rows+="$row"$'\n'
  fi

  if [[ -z "$rows" ]]; then
    printf '%s\n' "$current_checkpoint"
    return 0
  fi

  printf '%s' "$rows" | sort -n -k1,1 | awk -v cur="$current_checkpoint" -v phase="$current_phase" '
    $1 <= cur {
      checkpoints[++n] = $1
      statuses[n] = $2
      phases[n] = $3
    }
    END {
      cycle = cur
      for (i = n; i >= 1; i--) {
        if (statuses[i] == "phase_review" && phases[i] == phase) {
          cycle = checkpoints[i]
          break
        }
      }
      print cycle
    }
  '
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

named_run_id="$(named_run_dir_id "$BATON_FILE")"
if [[ -n "$named_run_id" ]]; then
  candidate_named_run_id="$(jq -r 'if (has("run_id") and (.run_id | type) == "string") then .run_id else "" end' "$CANDIDATE_FILE")"
  if [[ "$schema" != "dvandva.baton.v2" || "$candidate_named_run_id" != "$named_run_id" ]]; then
    echo "DVANDVA_WRITE bad_run_id_dir baton=$BATON_FILE candidate_run_id=$candidate_named_run_id expected_run_id=$named_run_id schema=$schema" >&2
    exit 23
  fi
fi

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

if ! jq -e '
  .review_target == null or
  .review_target == "research" or
  .review_target == "spec" or
  .review_target == "implementation" or
  .review_target == "prativadi_fixups" or
  .review_target == "vadi_counter"
' "$CANDIDATE_FILE" >/dev/null 2>&1; then
  echo "DVANDVA_WRITE bad_review_target candidate=$CANDIDATE_FILE" >&2
  exit 23
fi

new_status="$(jq -r '.status // ""' "$CANDIDATE_FILE")"
new_assignee="$(jq -r '.assignee // ""' "$CANDIDATE_FILE")"
new_mode="$(jq -r '.mode // ""' "$CANDIDATE_FILE")"
new_effective_mode=""
new_effective_profile=""
new_profile_floor=""

if [[ "$schema" == "dvandva.baton.v2" ]]; then
  if ! new_effective_mode="$(canonical_mode "$new_mode")"; then
    echo "DVANDVA_WRITE bad_mode mode=$new_mode candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  new_run_id="$(jq -r '.run_id // ""' "$CANDIDATE_FILE")"
  if ! jq -e '(.run_id | type) == "string" and (.run_id | length) > 0' "$CANDIDATE_FILE" >/dev/null 2>&1 || ! is_safe_run_id "$new_run_id"; then
    echo "DVANDVA_WRITE bad_run_id candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '(.original_ask | type) == "string" and (.original_ask | length) > 0' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_original_ask candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if [[ "$new_effective_mode" == "development" ]]; then
    if ! jq -e '
      def profile_value: . == "fast" or . == "standard" or . == "full";
      def nonblank: (type == "string") and test("[^[:space:]]");
      (
        ((has("profile") | not) or .profile == null or (.profile | profile_value)) and
        ((has("profile_floor") | not) or .profile_floor == null or (.profile_floor | profile_value)) and
        ((has("profile_history") | not) or (.profile_history | type) == "array") and
        all((.profile_history // [])[]?;
          ((.from == null) or (.from | profile_value)) and
          (.to | profile_value) and
          (.floor | profile_value) and
          ((.checkpoint | type) == "number") and
          ((.actor_role == "vadi") or (.actor_role == "prativadi") or (.actor_role == "human") or (.actor_role == "team")) and
          (.reason | nonblank) and
          ((.evidence_refs | type) == "array")
        ) and
        (
          if has("profile_decision") and .profile_decision != null then
            (.profile_decision | type) == "object" and
            (.profile_decision.selected_profile | profile_value) and
            (.profile_decision.floor | profile_value) and
            (.profile_decision.reason | nonblank) and
            (.profile_decision.decided_by | nonblank) and
            ((.profile_decision.decided_at == null) or (.profile_decision.decided_at | type) == "string") and
            ((.profile_decision.risk_inputs | type) == "array") and
            ((.profile_decision.hard_triggers | type) == "array") and
            ((.profile_decision.allowlist_match | type) == "boolean") and
            ((.profile_decision.allowlist_refs | type) == "array") and
            ((.profile_decision.evidence_refs | type) == "array")
          else
            true
          end
        )
      )
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      echo "DVANDVA_WRITE bad_profile candidate=$CANDIDATE_FILE" >&2
      exit 23
    fi
    if [[ ! -f "$BATON_FILE" && "$new_status" != "human_decision" ]] && ! jq -e '
      (has("profile") and .profile != null) and
      (has("profile_floor") and .profile_floor != null) and
      (has("profile_decision") and (.profile_decision | type) == "object") and
      (has("profile_history") and (.profile_history | type) == "array")
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      echo "DVANDVA_WRITE bad_profile candidate=$CANDIDATE_FILE" >&2
      exit 23
    fi
    if jq -e 'has("profile") and .profile != null' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      new_effective_profile="$(jq -r '.profile' "$CANDIDATE_FILE")"
    elif [[ ! -f "$BATON_FILE" ]]; then
      new_effective_profile="standard"
    else
      new_effective_profile="full"
    fi
    if jq -e 'has("profile_floor") and .profile_floor != null' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      new_profile_floor="$(jq -r '.profile_floor' "$CANDIDATE_FILE")"
    else
      new_profile_floor="$new_effective_profile"
    fi
    if jq -e 'has("profile_decision") and .profile_decision != null' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      if ! jq -e --arg profile "$new_effective_profile" --arg floor "$new_profile_floor" '
        .profile_decision.selected_profile == $profile and
        .profile_decision.floor == $floor
      ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_profile candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
    fi
    if [[ "$(profile_rank "$new_effective_profile")" -lt "$(profile_rank "$new_profile_floor")" && "$new_status" != "human_decision" ]]; then
      echo "DVANDVA_WRITE bad_profile_downgrade candidate=$CANDIDATE_FILE" >&2
      exit 23
    fi
    if jq -e '
      def hard_path:
        . == ".dvandva" or
        startswith(".dvandva/") or
        startswith(".githooks/") or
        startswith(".dvandva/githooks/") or
        . == "product.md" or
        . == "plugins/dvandva/references/baton-schema-v2.json" or
        . == "plugins/dvandva/references/state-transition-table.md" or
        . == "plugins/dvandva/references/local-baton-channel.md" or
        . == "docs/protocol/local-baton-channel.md" or
        . == "templates/channel/baton.json" or
        test("^plugins/dvandva/skills/[^/]+/SKILL\\.md$") or
        test("^plugins/dvandva/commands/[^/]+\\.md$") or
        test("^plugins/dvandva/skills/[^/]+/scripts/dvandva-[^/]+\\.sh$") or
        test("^scripts/[^/]+\\.sh$") or
        test("^plugins/dvandva/scripts/.*\\.sh$") or
        test("(^|/)\\.env(\\..*)?$") or
        test("(^|/)(secret|secrets|credential|credentials)(/|$)") or
        test("(^|/)(api|apis|client|clients)(/|$)") or
        test("(^|/)(package-lock\\.json|package\\.json|pnpm-lock\\.yaml|yarn\\.lock|requirements\\.txt|pyproject\\.toml|Cargo\\.toml|Cargo\\.lock)$");
      [
        (.changed_paths // [])[]?,
        (.work_split // [] | if type == "array" then .[]? else .[]? end | (.paths // [])[]?),
        (.work_split // [] | if type == "array" then .[]? else .[]? end | (.read_paths // [])[]?),
        (.work_split // [] | if type == "array" then .[]? else .[]? end | (.write_paths // [])[]?),
        ((.agent_instances // [])[]? | (.read_paths // [])[]?),
        ((.agent_instances // [])[]? | (.write_paths // [])[]?)
      ] | any(type == "string" and hard_path)
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      if [[ "$new_effective_profile" != "full" || "$new_profile_floor" != "full" ]] || ! jq -e '
        ((has("profile_decision") | not) or .profile_decision == null or .profile_decision.floor == "full")
      ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_profile_floor candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
    fi
    if [[ "$new_effective_profile" == "fast" ]] && ! jq -e '
      def allow_path:
        . == "README.md" or
        startswith("docs/research/") or
        startswith("docs/case-studies/");
      (.profile_decision.allowlist_match == true) and
      ((.profile_decision.evidence_refs | type) == "array" and (.profile_decision.evidence_refs | length) > 0) and
      ([
        (.changed_paths // [])[]?,
        (.work_split // [] | if type == "array" then .[]? else .[]? end | (.paths // [])[]?),
        (.work_split // [] | if type == "array" then .[]? else .[]? end | (.read_paths // [])[]?),
        (.work_split // [] | if type == "array" then .[]? else .[]? end | (.write_paths // [])[]?),
        ((.agent_instances // [])[]? | (.read_paths // [])[]?),
        ((.agent_instances // [])[]? | (.write_paths // [])[]?)
      ] | all(type == "string" and allow_path))
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      echo "DVANDVA_WRITE bad_profile_floor candidate=$CANDIDATE_FILE" >&2
      exit 23
    fi
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
    def reserved_agent_id:
      test("^(dvandva-(researcher|architect|implementer|test-creator|cross-reviewer|adversarial-analyst|deep-reviewer|deslopper|sandbox-verifier|baton-auditor|security-auditor|integration-checker|debugger|doc-verifier|pattern-mapper)|adversarial-analyst|quality-reviewer|sandbox-executor|architect|developer|vadi|prativadi|team|human)$");
    (.agent_instances | type) == "array" and
    (([.agent_instances[]?.id] | length) == ([.agent_instances[]?.id] | unique | length)) and
    all(.agent_instances[]?;
      (.id | safe_id) and
      (
        if generated_instance then
          ((.id | reserved_agent_id) | not) and
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
    def live($agent):
      (($agent.status // "") == "planned") or (($agent.status // "") == "running");
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
      select((($a.base_checkpoint == $b.base_checkpoint) or (live($a) and live($b))) and overlap($a; $b) and (serialized($a; $b) | not))
    ] | length == 0
  ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_agent_instances_write_paths candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '((.work_split | type) == "array" or (.work_split | type) == "object") and (.work_split | length) > 0' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_work_split candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '
    def safe_rel_path:
      (type == "string") and
      (length > 0) and
      (startswith("/") | not) and
      (contains("//") | not) and
      ((split("/") | all(. != "" and . != "." and . != "..")));
    def work_items:
      if (.work_split | type) == "array" then
        .work_split[]?
      else
        .work_split[]?
      end;
    all(work_items;
      ((has("paths") | not) or ((.paths | type) == "array" and all(.paths[]; safe_rel_path))) and
      ((has("read_paths") | not) or ((.read_paths | type) == "array" and all(.read_paths[]; safe_rel_path))) and
      ((has("write_paths") | not) or ((.write_paths | type) == "array" and all(.write_paths[]; safe_rel_path))) and
      ((has("depends_on") | not) or ((.depends_on | type) == "array")) and
      ((has("conflict_group") | not) or ((.conflict_group | type) == "string"))
    )
  ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_work_split candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '
    . as $root |
    def allowed_anchor:
      . == "spec-approved" or
      . == "parallel_implementing" or
      . == "implementing" or
      . == "test_creation" or
      . == "cross_review" or
      . == "deep_review" or
      . == "phase_review" or
      . == "deslop";
    def work_items:
      if ($root.work_split | type) == "array" then
        $root.work_split[]? | select(type == "object")
      else
        $root.work_split | to_entries[]? | select(.value | type == "object") | (.value + {id: (.value.id // .key)})
      end;
    [work_items] as $items |
    ([$items[] | .id? | select(type == "string" and length > 0)] | unique) as $ids |
    all($items[];
      ((.depends_on // []) | type) == "array" and
      all((.depends_on // [])[];
        . as $dep |
        (type == "string") and ((($ids | index($dep)) != null) or ($dep | allowed_anchor))
      )
    ) and
    (
      [
        $items[] |
        {id: (.id // ""), deps: [(.depends_on // [])[] as $dep | select(($ids | index($dep)) != null) | $dep]} |
        select(.id | length > 0)
      ] as $nodes |
      def strip_ready:
        . as $nodes |
        [$nodes[] | select((.deps | length) == 0) | .id] as $ready |
        if ($ready | length) == 0 then
          $nodes
        else
          [
            $nodes[] |
            . as $node |
            select(($ready | index($node.id)) == null) |
            .deps = [.deps[] as $dep | select(($ready | index($dep)) == null) | $dep]
          ]
        end;
      ($nodes | until((strip_ready == .); strip_ready) | length) == 0
    )
  ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_depends_on candidate=$CANDIDATE_FILE" >&2
    exit 23
  fi
  if ! jq -e '
    . as $root |
    def work_items:
      if ($root.work_split | type) == "array" then
        $root.work_split[]?
      else
        $root.work_split[]?
      end;
    def terminal_status:
      . == "completed" or
      . == "approved" or
      . == "passed" or
      . == "closed" or
      . == "done" or
      . == "rejected" or
      . == "collapsed" or
      . == "skipped" or
      . == "cancelled";
    def chunk_kind($item):
      ($item.chunk_type // $item.type // (
        if $root.status == "parallel_implementing" then
          "implementation"
        elif $root.status == "cross_fixing" then
          "cross_fixing"
        else
          ""
        end
      ));
    def write_capable_chunk($item):
      (chunk_kind($item) == "implementation") or
      (chunk_kind($item) == "cross_fixing") or
      (chunk_kind($item) == "fix");
    def parallel_impl_chunk($item):
      (chunk_kind($item) == "implementation") and
      (($item.phase | tostring) == ($root.phase | tostring)) and
      (((($item.owner_role // $item.owner // "") == "vadi")) or ((($item.owner_role // $item.owner // "") == "prativadi"))) and
      (((($item.cross_review_by // "") == "vadi")) or ((($item.cross_review_by // "") == "prativadi"))) and
      (($item | has("write_paths")) or ((($item.paths // []) | length) > 0));
    def effective_write_paths($item):
      if write_capable_chunk($item) then
        ((($item.paths // []) + ($item.write_paths // [])) | unique)
      elif ($item | has("write_paths")) then
        ($item.write_paths // [])
      else
        []
      end;
    # work_split has no base_checkpoint wave model. Terminal chunks are
    # completed historical work and are excluded so later fix chunks can reuse
    # the same paths; live planned/running overlaps are still rejected below.
    def live_item($item):
      (($item.status // "") | terminal_status | not);
    def path_overlap($left; $right):
      ($left == $right) or
      ($left | startswith($right + "/")) or
      ($right | startswith($left + "/"));
    def overlap($a; $b):
      any(effective_write_paths($a)[]; . as $path | any(effective_write_paths($b)[]; path_overlap($path; .)));
    def serialized($a; $b):
      (($a.conflict_group // "") != "") and
      (($a.conflict_group // "") == ($b.conflict_group // "")) and
      (((($a.depends_on // []) | index($b.id)) != null) or ((($b.depends_on // []) | index($a.id)) != null));
    (
      if $root.status == "parallel_implementing" then
        all(work_items;
          if parallel_impl_chunk(.) then
            (effective_write_paths(.) | length) > 0
          else
            true
          end
        )
      else
        true
      end
    ) and
    ([ work_items | select(live_item(.)) | select((effective_write_paths(.) | length) > 0) ] as $writers |
      [
        range(0; ($writers | length)) as $i |
        range($i + 1; ($writers | length)) as $j |
        ($writers[$i]) as $a |
        ($writers[$j]) as $b |
        select(overlap($a; $b) and (serialized($a; $b) | not))
      ] | length == 0)
  ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
    echo "DVANDVA_WRITE bad_work_split_write_paths candidate=$CANDIDATE_FILE" >&2
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
  if [[ "$new_status" == "done" ]]; then
    case "$new_effective_mode" in
      development)
        if [[ "$new_effective_profile" == "full" ]]; then
          explainer_ref="$(jq -r 'if (.run_explainer_ref | type) == "string" then .run_explainer_ref else "" end' "$CANDIDATE_FILE")"
          if ! run_explainer_ref_matches_run_id "$explainer_ref" "$new_run_id"; then
            echo "DVANDVA_WRITE bad_run_explainer_ref candidate=$CANDIDATE_FILE" >&2
            exit 23
          fi
          if ! jq -e '
            .run_explainer_ref as $ref |
            def reviewed_by($role):
              any(.run_explainer_reviews[]?;
                .role == $role and
                .artifact_ref == $ref and
                .status == "completed" and
                .result == "approved" and
                ((.summary | type) == "string" and (.summary | test("[^[:space:]]"))) and
                ((.evidence_refs | type) == "array" and ((.evidence_refs | length) > 0))
              );
            ((.run_explainer_reviews | type) == "array") and
            reviewed_by("vadi") and
            reviewed_by("prativadi")
          ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
            echo "DVANDVA_WRITE bad_run_explainer_reviews candidate=$CANDIDATE_FILE" >&2
            exit 23
          fi
        else
          if ! jq -e '
            def good_result:
              . == "passed" or . == "approved";
            def matrix_items:
              if (.verification_matrix | type) == "array" then
                .verification_matrix[]?
              elif (.verification_matrix | type) == "object" then
                .verification_matrix[]?
              else
                empty
              end;
            def good_matrix:
              [matrix_items] as $items |
              ($items | length) > 0 and
              all($items[];
                (((.current // .result // "") | good_result)) and
                ((.evidence_refs | type) == "array" and (.evidence_refs | length) > 0)
              );
            def good_verification:
              any(.verification[]?;
                (((.result // "") | good_result)) and
                ((.command | type) == "string" and (.command | test("[^[:space:]]")))
              );
            def good_phase_review:
              any(.subagent_tracks[]?;
                (.phase | tostring) == "phase_review" and
                .track == "phase-review" and
                .status == "completed" and
                (((.result // "") | good_result)) and
                ((.owner_role // .role // .owner // "") == "prativadi") and
                ((.outputs | type) == "array" and (.outputs | length) > 0) and
                ((.evidence_refs | type) == "array" and (.evidence_refs | length) > 0)
              );
            (.profile_decision | type) == "object" and
            good_verification and
            good_matrix and
            good_phase_review
          ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
            echo "DVANDVA_WRITE bad_compact_terminal_evidence candidate=$CANDIDATE_FILE" >&2
            exit 23
          fi
        fi
        ;;
      research)
        if ! jq -e '
          (.research_outcome == null or .research_outcome == "exploratory" or .research_outcome == "seed_development") and
          (.research_ref | type) == "string" and (.research_ref | length) > 0 and
          (((.research_outcome // "exploratory") != "seed_development") or ((.plan_ref | type) == "string" and (.plan_ref | length) > 0))
        ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
          echo "DVANDVA_WRITE bad_research_done_ref candidate=$CANDIDATE_FILE" >&2
          exit 23
        fi
        ;;
      review)
        if ! jq -e '
          (.review_ref | type) == "string" and
          (.review_ref | test("^\\./superpowers/reviews/[A-Za-z0-9._/-]+\\.html$")) and
          ((.review_ref | test("(^|/)\\.\\.(/|$)|//")) | not)
        ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
          echo "DVANDVA_WRITE bad_review_ref candidate=$CANDIDATE_FILE" >&2
          exit 23
        fi
        ;;
    esac
  fi
fi

if [[ "$new_status" == "done" ]] \
  && ! jq -e '
    (.assignee == "human" or .assignee == "team" or .assignee == "vadi" or .assignee == "prativadi") and
    (.vadi_final_approval == true) and
    (.prativadi_final_approval == true)
  ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
  echo "DVANDVA_WRITE bad_done_state candidate=$CANDIDATE_FILE" >&2
  exit 23
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
new_vadi_final_approval="$(jq -r '.vadi_final_approval // false' "$CANDIDATE_FILE")"
new_prativadi_final_approval="$(jq -r '.prativadi_final_approval // false' "$CANDIDATE_FILE")"

case "$schema:$new_status" in
  dvandva.baton.v1:spec_drafting|dvandva.baton.v1:spec_review|dvandva.baton.v1:spec_revision|dvandva.baton.v1:human_question|dvandva.baton.v1:implementing|dvandva.baton.v1:phase_review|dvandva.baton.v1:phase_fixing|dvandva.baton.v1:review_of_review|dvandva.baton.v1:counter_review|dvandva.baton.v1:human_decision|dvandva.baton.v1:done) ;;
  dvandva.baton.v2:research_drafting|dvandva.baton.v2:research_review|dvandva.baton.v2:research_revision|dvandva.baton.v2:spec_drafting|dvandva.baton.v2:spec_review|dvandva.baton.v2:spec_revision|dvandva.baton.v2:human_question|dvandva.baton.v2:implementing|dvandva.baton.v2:parallel_implementing|dvandva.baton.v2:test_creation|dvandva.baton.v2:cross_review|dvandva.baton.v2:cross_fixing|dvandva.baton.v2:deep_review|dvandva.baton.v2:deslop|dvandva.baton.v2:termination_review|dvandva.baton.v2:phase_review|dvandva.baton.v2:phase_fixing|dvandva.baton.v2:review_of_review|dvandva.baton.v2:counter_review|dvandva.baton.v2:human_decision|dvandva.baton.v2:done) ;;
  *)
    echo "DVANDVA_WRITE bad_status status=$new_status candidate=$CANDIDATE_FILE" >&2
    exit 23
    ;;
esac

if [[ "$schema" == "dvandva.baton.v2" ]]; then
  case "$new_effective_mode:$new_status" in
    *:human_question|*:human_decision)
      ;;
    development:research_drafting|development:research_review|development:research_revision)
      if ! jq -e '.phase == "research"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_phase_status status=$new_status candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
      ;;
    development:spec_drafting|development:spec_review|development:spec_revision)
      if ! jq -e '.phase == "spec"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_phase_status status=$new_status candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
      ;;
    development:implementing|development:parallel_implementing|development:test_creation|development:cross_review|development:cross_fixing|development:deep_review|development:deslop|development:termination_review|development:phase_review|development:phase_fixing|development:review_of_review|development:counter_review|development:done)
      if ! jq -e '(.phase | type) == "number"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_phase_status status=$new_status candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
      ;;
    research:research_drafting|research:research_review|research:research_revision)
      if ! jq -e '.phase == "research"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_phase_status status=$new_status candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
      ;;
    research:*)
      if ! jq -e '.phase == "spec"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
        echo "DVANDVA_WRITE bad_phase_status status=$new_status candidate=$CANDIDATE_FILE" >&2
        exit 23
      fi
      ;;
    review:*)
      if ! jq -e '.phase == "review"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
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
    parallel_implementing|cross_review|cross_fixing|termination_review)
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

# --- Concurrency guard: PORTABLE atomic mutual exclusion ----------------------
# Two engines share one worktree and may run this helper at the same time. Without
# a lock both writers could read the same on-disk checkpoint=N, both build a
# checkpoint=N+1 candidate, both pass the (N+1 > N) guard, and both mv their
# candidate into place -- the later mv silently clobbers the earlier accepted
# write (a lost update / TOCTOU stale-write race). We therefore serialize the
# read-current-state -> validate-transition -> mv-install -> snapshot section.
#
# The lock primitive is mkdir: it is atomic and POSIX-portable (works on stock
# macOS, unlike flock, which we deliberately do NOT depend on). The race loser
# acquires the lock only after the winner releases it, re-reads the now-advanced
# checkpoint (N+1), finds its own N+1 candidate stale (N+1 <= N+1) and exits
# 27 stale_checkpoint -- fail-closed, with no lost update.
#
# Pure candidate-schema checks above ran outside the lock (they do not depend on
# current on-disk state); everything from here through the snapshot is inside it.
BATON_DIR="$(dirname "$BATON_FILE")"
mkdir -p "$BATON_DIR"
LOCK_DIR="$BATON_DIR/.baton.lock.d"
LOCK_ACQUIRED=0
# Bounded wait before a held lock is presumed abandoned by a crashed writer. A
# live writer holds the lock only for one validate+mv+snapshot (sub-second), so
# this generous default never trips for healthy writers; it only breaks a
# deadlock left by a SIGKILLed writer. Override via DVANDVA_LOCK_TIMEOUT (seconds).
LOCK_TIMEOUT="${DVANDVA_LOCK_TIMEOUT:-30}"
# Validate early: LOCK_TIMEOUT is used in bash arithmetic inside the lock loop
# ([[ "$age" -ge "$LOCK_TIMEOUT" ]]). Three hazard classes must be caught here:
#   (a) Non-numeric (e.g. "abc"): under set -u causes an unbound-variable crash
#       (rc=1, unstructured error output, not a clean bad_lock_timeout).
#   (b) Negative (e.g. "-5"): age(>=0) always satisfies the comparison → the loop
#       steals ANY held lock immediately, defeating the locking protocol.
#   (c) Zero or leading-zero forms (0, 00, 08, 09, ...): zero makes age(0) >= 0
#       always true — same instant-steal bypass as negative. Leading-zero forms
#       (08, 09) are invalid octal literals in bash arithmetic; [[ age -ge 08 ]]
#       prints "value too great for base" and returns false, so the steal never
#       fires and the acquire loop spins until an external timeout kills it.
# Fix: require CANONICAL POSITIVE DECIMAL — ^[1-9][0-9]*$ — which rejects all
# three hazard classes while accepting any legitimate positive integer. The default
# (30) matches because it is set before this guard and 30 satisfies the regex.
# Fail closed with exit 2 (same bucket as other usage/validation errors); do NOT
# clamp or guess silently.
if [[ ! "${LOCK_TIMEOUT}" =~ ^[1-9][0-9]*$ ]]; then
  echo "DVANDVA_WRITE bad_lock_timeout value=${LOCK_TIMEOUT}" >&2
  exit 2
fi

# Fencing token: a value unique to THIS invocation, written into the lock dir at
# acquire and re-verified immediately before the irreversible mv-install. If a
# slow (but still LIVE) writer's lock is age-stolen by a peer, the peer rewrites
# the token; the slow writer then detects the mismatch and aborts instead of
# clobbering the peer's already-installed checkpoint+1 (the two-writers-both-
# install bug). Portability: $$ differs across the two concurrent engine
# processes (PIDs are unique among live processes), which alone distinguishes
# racing writers. We also fold in ${BASHPID:-0} (guarded for bash 3.2 / stock
# macOS where it may be unset under set -u), a wall-clock stamp, two $RANDOM
# draws (a bash builtin, unlike GNU-only `date +%N`), and a /dev/urandom hexdump
# when present -- so even PID reuse by a later invocation (after a crash) yields
# a fresh, non-colliding token. No flock; mkdir + token files only.
lock_nonce="$(od -An -N16 -tx1 /dev/urandom 2>/dev/null | tr -d ' \n')"
LOCK_TOKEN="$$.${BASHPID:-0}.$(date +%s 2>/dev/null || echo 0).$RANDOM.$RANDOM.${lock_nonce:-0}"

release_lock() {
  if [[ "$LOCK_ACQUIRED" -eq 1 ]]; then
    rm -rf "$LOCK_DIR" 2>/dev/null
    LOCK_ACQUIRED=0
  fi
}

# True only if we acquired the lock AND our fencing token is still installed on
# disk. A peer that age-steals the lock renames our dir aside and writes its own
# token, so a stolen lock makes this return non-zero (token differs or absent).
holds_lock() {
  [[ "$LOCK_ACQUIRED" -eq 1 ]] || return 1
  local on_disk
  on_disk="$(cat "$LOCK_DIR/owner" 2>/dev/null)" || return 1
  [[ "$on_disk" == "$LOCK_TOKEN" ]]
}

cleanup_and_exit() {
  release_lock
  exit "$1"
}

# Release on every exit path (normal, error, or signal). release_lock is
# idempotent, so the EXIT trap firing after a signal handler is harmless.
trap 'release_lock' EXIT
trap 'cleanup_and_exit 130' INT
trap 'cleanup_and_exit 143' TERM

acquire_lock() {
  local now lock_started age first_seen
  first_seen="$(date +%s 2>/dev/null || echo 0)"
  while true; do
    if mkdir "$LOCK_DIR" 2>/dev/null; then
      LOCK_ACQUIRED=1
      # Record acquisition wall-clock so a later waiter can age out a crash.
      date +%s > "$LOCK_DIR/started_at" 2>/dev/null || true
      # Stamp our fencing token so a peer that age-steals this lock provably
      # replaces it, letting our pre-install check below detect the theft.
      printf '%s' "$LOCK_TOKEN" > "$LOCK_DIR/owner" 2>/dev/null || true
      return 0
    fi
    # mkdir failed. A NON-DIRECTORY squatting the lock path (corruption, a
    # leftover, or an attacker planting a file) can NEVER become a held lock, so
    # we must NOT fall through and run the critical section unlocked -- that is
    # exactly the race window. Fail closed: signal the caller to abort (rc 2).
    if [[ -e "$LOCK_DIR" && ! -d "$LOCK_DIR" ]]; then
      return 2
    fi
    # If the lock dir does not exist at all, the failure is environmental (e.g. a
    # read-only baton dir), not contention -- give up on locking and let the real
    # install fail with its own exit code rather than spin forever.
    if [[ ! -d "$LOCK_DIR" ]]; then
      return 1
    fi
    # Contention: another writer holds the lock. Age it out only if it looks
    # abandoned (started_at older than LOCK_TIMEOUT, or -- if started_at is
    # missing because the holder crashed mid-creation -- we have observed the
    # contention ourselves for longer than LOCK_TIMEOUT).
    now="$(date +%s 2>/dev/null || echo 0)"
    lock_started="$(cat "$LOCK_DIR/started_at" 2>/dev/null || echo "")"
    if [[ "$lock_started" =~ ^[0-9]+$ ]]; then
      age=$(( now - lock_started ))
    else
      age=$(( now - first_seen ))
    fi
    if [[ "$age" -ge "$LOCK_TIMEOUT" ]]; then
      # Steal atomically: rename the stale dir aside (directory rename is atomic,
      # so concurrent stealers cannot both win) then remove it, before retrying.
      stale_dir="$LOCK_DIR.stale.$$"
      if mv "$LOCK_DIR" "$stale_dir" 2>/dev/null; then
        rm -rf "$stale_dir" 2>/dev/null
      fi
      first_seen="$(date +%s 2>/dev/null || echo 0)"
      continue
    fi
    sleep 0.1
  done
}

# Lock acquisition is MANDATORY for the critical section. acquire_lock returns:
#   0 = lock held (token stamped)
#   1 = environmental: the lock dir could not be created and does NOT exist
#       (e.g. a genuinely unwritable/read-only baton dir). The dir being
#       unwritable means the real install below also cannot write, so there is
#       no race to lose -- we proceed unlocked and let cp/mv report exit 26.
#   2 = a non-directory squats the lock path. We can never hold a real lock, and
#       the baton dir may still be writable, so proceeding unlocked WOULD reopen
#       the write race. Fail closed.
acquire_lock
lock_rc=$?
if [[ "$lock_rc" -eq 2 ]]; then
  echo "DVANDVA_WRITE lock_unavailable path=$LOCK_DIR reason=non_directory_at_lock_path" >&2
  exit 28
fi

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
  if ! cur="$(jq -r '[.schema // "", .status // "", (.checkpoint // -1 | tostring), (.master_plan_locked // false | tostring), .resume_assignee // "", .resume_status // "", .run_id // "", (.phase | tostring), (.vadi_final_approval // false | tostring), (.prativadi_final_approval // false | tostring), .mode // ""] | join("\u001f")' "$BATON_FILE" 2>/dev/null)"; then
    echo "DVANDVA_WRITE current_baton_unparseable file=$BATON_FILE refusing_to_overwrite=true" >&2
    exit 25
  fi
  IFS=$'\x1f' read -r cur_schema cur_status cur_checkpoint cur_locked cur_resume_assignee cur_resume_status cur_run_id cur_phase cur_vadi_final_approval cur_prativadi_final_approval cur_mode <<< "$cur"

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
  cur_effective_mode=""
  cur_effective_profile=""
  cur_profile_floor=""
  if [[ "$cur_schema" == "dvandva.baton.v2" ]]; then
    if ! cur_effective_mode="$(canonical_mode "$cur_mode")"; then
      echo "DVANDVA_WRITE current_baton_unparseable file=$BATON_FILE bad_mode=$cur_mode" >&2
      exit 25
    fi
    if [[ "$cur_effective_mode" == "development" ]]; then
      if jq -e 'has("profile") and .profile != null' "$BATON_FILE" >/dev/null 2>&1; then
        cur_effective_profile="$(jq -r '.profile' "$BATON_FILE")"
      else
        cur_effective_profile="full"
      fi
      if jq -e 'has("profile_floor") and .profile_floor != null' "$BATON_FILE" >/dev/null 2>&1; then
        cur_profile_floor="$(jq -r '.profile_floor' "$BATON_FILE")"
      else
        cur_profile_floor="$cur_effective_profile"
      fi
    fi
  fi

  approval_reason=""
  if [[ "$schema" == "dvandva.baton.v2" && "$new_status" != "done" ]]; then
    writer_role="${DVANDVA_ROLE:-}"
    approval_reset_transition=0
    if [[ "$cur_status" == "termination_review" && "$new_status" == "phase_fixing" ]]; then
      approval_reset_transition=1
    fi
    if [[ "$approval_reset_transition" -eq 1 && ( "$new_vadi_final_approval" == "true" || "$new_prativadi_final_approval" == "true" ) ]]; then
      approval_reason="stale_approval: termination_review->phase_fixing must reset both final approvals"
    elif [[ "$new_status" != "termination_review" && "$new_vadi_final_approval" == "true" && "$cur_vadi_final_approval" != "true" ]]; then
      approval_reason="approval_out_of_band: vadi_final_approval can only be raised while entering termination_review"
    elif [[ "$new_status" != "termination_review" && "$new_prativadi_final_approval" == "true" && "$cur_prativadi_final_approval" != "true" ]]; then
      approval_reason="approval_out_of_band: prativadi_final_approval can only be raised while entering termination_review"
    elif [[ "$approval_reset_transition" -eq 0 && "$new_vadi_final_approval" != "$cur_vadi_final_approval" && "$writer_role" != "vadi" ]]; then
      approval_reason="final approval ownership requires DVANDVA_ROLE=vadi to change vadi_final_approval"
    elif [[ "$approval_reset_transition" -eq 0 && "$new_prativadi_final_approval" != "$cur_prativadi_final_approval" && "$writer_role" != "prativadi" ]]; then
      approval_reason="final approval ownership requires DVANDVA_ROLE=prativadi to change prativadi_final_approval"
    fi
  fi

  loop_reason=""
  if [[ "$schema" == "dvandva.baton.v2" && "$new_status" != "human_decision" ]]; then
    loop_edge=""
    case "${cur_status}:${new_status}" in
      deep_review:phase_fixing|cross_review:cross_fixing|termination_review:phase_fixing|phase_review:phase_fixing|review_of_review:counter_review|counter_review:review_of_review)
        loop_edge="${cur_status}:${new_status}"
        ;;
    esac
    if [[ "$new_phase" != "$cur_phase" ]] \
      && jq -e '((.loop_counts // {}) | type) == "object" and ((.loop_counts // {}) | length) > 0' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      loop_reason="bad_loop_counts phase_advanced current=$cur_phase candidate=$new_phase must_reset=true"
    elif [[ -n "$loop_edge" ]]; then
      cur_loop_count="$(jq -r --arg edge "$loop_edge" '(.loop_counts // {})[$edge] // 0' "$BATON_FILE" 2>/dev/null || printf 'invalid')"
      new_loop_count="$(jq -r --arg edge "$loop_edge" '(.loop_counts // {})[$edge] // 0' "$CANDIDATE_FILE" 2>/dev/null || printf 'invalid')"
      loop_cap="$(jq -r '.disagreement_cap // 0' "$CANDIDATE_FILE" 2>/dev/null || printf 'invalid')"
      if [[ ! "$cur_loop_count" =~ ^[0-9]+$ || ! "$new_loop_count" =~ ^[0-9]+$ || ! "$loop_cap" =~ ^[0-9]+$ || "$loop_cap" -eq 0 ]]; then
        loop_reason="bad_loop_counts edge=$loop_edge count=$new_loop_count"
      elif [[ "$cur_loop_count" -ge "$loop_cap" ]]; then
        loop_reason="loop_cap edge=$loop_edge count=$cur_loop_count cap=$loop_cap"
      elif [[ "$new_loop_count" -ne $((cur_loop_count + 1)) ]]; then
        loop_reason="bad_loop_counts edge=$loop_edge expected=$((cur_loop_count + 1)) got=$new_loop_count"
      fi
    fi
  fi

  review_ownership_reason=""
  if [[ "$schema" == "dvandva.baton.v2" ]]; then
    writer_role="${DVANDVA_ROLE:-}"
    if ! jq -e -n --arg role "$writer_role" --slurpfile current "$BATON_FILE" --slurpfile candidate "$CANDIDATE_FILE" '
      def reviews($doc):
        if (($doc.run_explainer_reviews? // []) | type) == "array" then
          ($doc.run_explainer_reviews? // [])
        else
          []
        end;
      def protected_reviews($doc):
        [ reviews($doc)[]? | select((.role // "") != $role) | tojson ] | sort;
      protected_reviews($current[0]) == protected_reviews($candidate[0])
    ' >/dev/null 2>&1; then
      review_ownership_reason="run explainer review ownership requires DVANDVA_ROLE=vadi/prativadi and only that role may change its own run_explainer_reviews entries"
    fi
  fi

  if [[ "$schema" == "dvandva.baton.v2" && "$new_effective_mode" == "development" && "$new_status" == "done" && "$new_effective_profile" != "full" ]]; then
    phase_review_required_checkpoint="$(phase_review_cycle_checkpoint "$BATON_FILE" "$cur_checkpoint" "$cur_phase")"
    if ! jq -e --argjson review_checkpoint "$phase_review_required_checkpoint" '
      def good_result:
        . == "passed" or . == "approved";
      any(.subagent_tracks[]?;
        (.phase | tostring) == "phase_review" and
        .track == "phase-review" and
        .review_checkpoint == $review_checkpoint and
        .status == "completed" and
        (((.result // "") | good_result)) and
        ((.owner_role // .role // .owner // "") == "prativadi") and
        ((.outputs | type) == "array" and (.outputs | length) > 0) and
        ((.evidence_refs | type) == "array" and (.evidence_refs | length) > 0)
      )
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      echo "DVANDVA_WRITE bad_compact_terminal_evidence candidate=$CANDIDATE_FILE" >&2
      exit 23
    fi
  fi

  if [[ "$schema" == "dvandva.baton.v2" && "$cur_effective_mode" == "development" && "$new_effective_mode" == "development" ]]; then
    if ! jq -e --slurpfile current "$BATON_FILE" '
      ($current[0].profile_history // []) as $old |
      (.profile_history // []) as $new |
      all($old[]?; . as $entry | any($new[]?; . == $entry))
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      echo "DVANDVA_WRITE bad_profile_history candidate=$CANDIDATE_FILE" >&2
      exit 23
    fi
  fi

  if [[ "$schema" == "dvandva.baton.v2" && "$cur_effective_mode" == "development" && "$new_effective_mode" == "development" && "$new_status" != "human_decision" ]]; then
    if ! jq -e --arg floor "$cur_profile_floor" --slurpfile current "$BATON_FILE" '
      def rank:
        if . == "fast" then 1
        elif . == "standard" then 2
        elif . == "full" then 3
        else 0
        end;
      def low_keys($items):
        [$items[]? | select((.floor | rank) < ($floor | rank)) | tojson] | unique;
      def low_count($items; $key):
        [$items[]? | select((.floor | rank) < ($floor | rank) and (tojson == $key))] | length;
      ($current[0].profile_history // []) as $old |
      (.profile_history // []) as $new |
      ((low_keys($old) + low_keys($new)) | unique) as $keys |
      all($keys[]?;
        . as $key |
        low_count($new; $key) == low_count($old; $key)
      )
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      echo "DVANDVA_WRITE bad_profile_downgrade candidate=$CANDIDATE_FILE" >&2
      exit 23
    fi
  fi

  if [[ "$schema" == "dvandva.baton.v2" && "$cur_effective_mode" == "development" && "$new_effective_mode" == "development" && "$new_status" != "human_decision" && ( "$new_effective_profile" != "$cur_effective_profile" || "$new_profile_floor" != "$cur_profile_floor" ) ]]; then
    if ! jq -e --arg from "$cur_effective_profile" --arg to "$new_effective_profile" --arg floor "$new_profile_floor" --argjson checkpoint "$new_checkpoint" '
      any(.profile_history[]?;
        .from == $from and
        .to == $to and
        .floor == $floor and
        .checkpoint == $checkpoint and
        ((.actor_role == "vadi") or (.actor_role == "prativadi") or (.actor_role == "human") or (.actor_role == "team")) and
        ((.reason | type) == "string" and (.reason | test("[^[:space:]]"))) and
        ((.evidence_refs | type) == "array" and (.evidence_refs | length) > 0)
      )
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      echo "DVANDVA_WRITE bad_profile_history candidate=$CANDIDATE_FILE" >&2
      exit 23
    fi
  fi

  # Precedence is load-bearing — do not reorder:
  #   1. stale checkpoint guard   2. checkpoint+1
  #   3. ownership gates   4. same-status team-sync gate
  #   5. from-human_question   6. to-human_decision (universal)
  #   7. from-human_decision
  #   8. to-human_question (spec-only, unlocked, fields set)   9. edge whitelist
  # e.g. moving the same-status ban below the human branches would silently
  # legalize human_decision->human_decision rewrites.
  if [[ "$cur_schema" != "$schema" ]]; then
    reason="schema_change current=$cur_schema candidate=$schema"
  elif [[ "$schema" == "dvandva.baton.v2" && "$cur_run_id" != "$new_run_id" ]]; then
    reason="run_id_change current=$cur_run_id candidate=$new_run_id"
  elif [[ "$schema" == "dvandva.baton.v2" && "$cur_effective_mode" != "$new_effective_mode" ]]; then
    reason="mode_change current=$cur_mode candidate=$new_mode"
  elif [[ "$schema" == "dvandva.baton.v2" && "$cur_effective_mode" == "development" && "$new_effective_mode" == "development" && "$new_status" != "human_decision" && "$(profile_rank "$new_effective_profile")" -lt "$(profile_rank "$cur_profile_floor")" ]]; then
    echo "DVANDVA_WRITE bad_profile_downgrade candidate=$CANDIDATE_FILE" >&2
    exit 23
  elif [[ "$schema" == "dvandva.baton.v2" && "$cur_effective_mode" == "development" && "$new_effective_mode" == "development" && "$new_status" != "human_decision" && "$(profile_rank "$new_profile_floor")" -lt "$(profile_rank "$cur_profile_floor")" ]]; then
    echo "DVANDVA_WRITE bad_profile_downgrade candidate=$CANDIDATE_FILE" >&2
    exit 23
  elif [[ "$new_checkpoint" -le "$cur_checkpoint" ]]; then
    echo "DVANDVA_WRITE stale_checkpoint current=$cur_checkpoint candidate=$new_checkpoint" >&2
    exit 27
  elif [[ "$new_checkpoint" -ne $((cur_checkpoint + 1)) ]]; then
    reason="checkpoint must be $((cur_checkpoint + 1)), got $new_checkpoint"
  elif [[ "$approval_reason" == approval_out_of_band* || "$approval_reason" == stale_approval* ]]; then
    echo "DVANDVA_WRITE $approval_reason" >&2
    exit 23
  elif [[ -n "$approval_reason" ]]; then
    reason="$approval_reason"
  elif [[ -n "$loop_reason" ]]; then
    echo "DVANDVA_WRITE $loop_reason" >&2
    exit 23
  elif [[ -n "$review_ownership_reason" && ( "$new_status" != "done" || ( "$cur_status" == "termination_review" && "$cur_vadi_final_approval" == "true" && "$cur_prativadi_final_approval" == "true" ) ) ]]; then
    reason="$review_ownership_reason"
  elif [[ "$new_status" == "$cur_status" ]]; then
    if [[ "$schema" == "dvandva.baton.v2" ]]; then
      case "$new_status" in
        parallel_implementing|cross_review|cross_fixing|termination_review)
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
    elif [[ "$cur_resume_status" == "done" || "$new_status" == "done" ]]; then
      reason="human_question cannot resume directly to done"
    elif [[ "$new_status" == "$cur_resume_status" && "$new_assignee" == "$cur_resume_assignee" && "$cand_q_null" == "true" && "$cand_ra_null" == "true" && "$cand_rs_null" == "true" ]]; then
      legal=1
    else
      reason="human_question resume must restore status=$cur_resume_status assignee=$cur_resume_assignee and clear question/resume fields"
    fi
  elif [[ "$schema" == "dvandva.baton.v2" && "$new_status" == "done" && "$cur_status" != "termination_review" ]]; then
    reason="done requires current status termination_review"
  elif [[ "$schema" == "dvandva.baton.v2" && "$new_status" == "done" && ( "$cur_vadi_final_approval" != "true" || "$cur_prativadi_final_approval" != "true" ) ]]; then
    reason="done requires current termination_review with both final approvals"
  elif [[ "$new_status" == "human_decision" ]]; then
    legal=1   # universal escalation
  elif [[ "$cur_status" == "human_decision" ]]; then
    legal=1   # human-authorized resume to any non-terminal protocol state
  elif [[ "$new_status" == "human_question" ]]; then
    if [[ "$cur_locked" == "true" ]]; then
      reason="human_question is only legal before master_plan_locked"
    elif [[ "$cur_status" != "spec_drafting" && "$cur_status" != "spec_review" && "$cur_status" != "spec_revision" && "$cur_status" != "research_drafting" && "$cur_status" != "research_review" && "$cur_status" != "research_revision" ]]; then
      reason="human_question only enters from spec or research states, not $cur_status"
    elif [[ "$cand_q_null" == "true" || "$cand_ra_null" == "true" || "$cand_rs_null" == "true" ]]; then
      reason="human_question requires non-null question, resume_assignee, resume_status"
    elif jq -e '.resume_status == "done"' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      reason="human_question cannot resume directly to done"
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
        case "$cur_effective_mode" in
          development)
            case "$new_effective_profile" in
              fast)
                case "${cur_status}:${new_status}" in
                  research_drafting:research_review|research_review:research_revision|research_revision:research_review|research_review:implementing) legal=1 ;;
                  implementing:phase_review|phase_review:phase_fixing|phase_fixing:phase_review|phase_review:termination_review|termination_review:phase_fixing|termination_review:done) legal=1 ;;
                  *) reason="no legal edge ${cur_status}->${new_status}" ;;
                esac
                ;;
              standard)
                case "${cur_status}:${new_status}" in
                  research_drafting:research_review|research_review:research_revision|research_revision:research_review|research_review:spec_drafting) legal=1 ;;
                  spec_drafting:spec_review|spec_review:spec_revision|spec_revision:spec_review|spec_review:implementing) legal=1 ;;
                  implementing:phase_review|phase_review:phase_fixing|phase_review:implementing|phase_fixing:phase_review|phase_review:termination_review|termination_review:phase_fixing|termination_review:done) legal=1 ;;
                  *) reason="no legal edge ${cur_status}->${new_status}" ;;
                esac
                ;;
              full)
                case "${cur_status}:${new_status}" in
                  research_drafting:research_review|research_review:research_revision|research_revision:research_review|research_review:spec_drafting) legal=1 ;;
                  spec_drafting:spec_review|spec_review:spec_revision|spec_review:parallel_implementing|spec_revision:spec_review) legal=1 ;;
                  parallel_implementing:test_creation|test_creation:cross_review|cross_review:cross_fixing|cross_fixing:test_creation|cross_review:deep_review) legal=1 ;;
                  deep_review:phase_fixing|deep_review:review_of_review|deep_review:deslop|review_of_review:counter_review|review_of_review:deslop|counter_review:review_of_review|counter_review:deslop|phase_fixing:test_creation|deslop:phase_fixing|deslop:parallel_implementing|deslop:termination_review|termination_review:phase_fixing|termination_review:done) legal=1 ;;
                  *) reason="no legal edge ${cur_status}->${new_status}" ;;
                esac
                ;;
            esac
            ;;
          research)
            case "${cur_status}:${new_status}" in
              research_drafting:research_review|research_review:research_revision|research_revision:research_review|research_review:spec_drafting) legal=1 ;;
              spec_drafting:spec_review|spec_review:spec_revision|spec_revision:spec_review) legal=1 ;;
              research_review:termination_review|spec_review:termination_review|termination_review:phase_fixing|phase_fixing:research_review|termination_review:done) legal=1 ;;
              *) reason="no legal edge ${cur_status}->${new_status}" ;;
            esac
            ;;
          review)
            case "${cur_status}:${new_status}" in
              research_drafting:research_review|research_review:research_revision|research_revision:research_review) legal=1 ;;
              research_review:deep_review|deep_review:deslop|deslop:termination_review|termination_review:phase_fixing|phase_fixing:deep_review|termination_review:done) legal=1 ;;
              *) reason="no legal edge ${cur_status}->${new_status}" ;;
            esac
            ;;
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
    cross_review_required_checkpoint="$(cross_review_cycle_checkpoint "$BATON_FILE" "$cur_checkpoint")"
    if ! jq -e --argjson review_checkpoint "$cross_review_required_checkpoint" '
      any(.subagent_tracks[];
        (
          .phase == "cross_review" and
          .track == "cross-review" and
          .review_checkpoint == $review_checkpoint and
          (((.owner_role // .role // "") == "vadi") or ((.owner_role // .role // "") == "prativadi")) and
          .status == "completed" and
          (.result != "passed" and .result != "approved") and
          ((.outputs | length) > 0) and
          ((.evidence_refs | length) > 0)
        )
      )
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      legal=0
      reason="cross_review->cross_fixing requires current-cycle completed cross-review subagent_tracks with non-approval evidence"
    fi
  fi

  if [[ "$legal" -eq 1 && "$schema" == "dvandva.baton.v2" && "$cur_status" == "cross_review" && "$new_status" == "deep_review" ]]; then
    cross_review_required_checkpoint="$(cross_review_cycle_checkpoint "$BATON_FILE" "$cur_checkpoint")"
    if ! jq -e --argjson review_checkpoint "$cross_review_required_checkpoint" '
      def done_cross($role):
        any(.subagent_tracks[];
          (
            .phase == "cross_review" and
            .track == "cross-review" and
            .review_checkpoint == $review_checkpoint and
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
      reason="cross_review->deep_review requires current-cycle completed cross-review subagent_tracks for both roles with phase=\"cross_review\""
    fi
  fi

  if [[ "$legal" -eq 1 && "$schema" == "dvandva.baton.v2" && "$new_status" == "review_of_review" ]]; then
    if ! jq -e '
      (.narrow_fixups | type) == "array" and
      ((.narrow_fixups | length) > 0) and
      all(.narrow_fixups[]; (type == "string") and test("\\S"))
    ' "$CANDIDATE_FILE" >/dev/null 2>&1; then
      legal=0
      reason="review_of_review requires non-empty narrow_fixups"
    fi
  fi

  if [[ "$legal" -eq 1 && "$schema" == "dvandva.baton.v2" && "$cur_status" == "deep_review" && ( "$new_status" == "deslop" || "$new_status" == "review_of_review" ) ]]; then
    deep_review_required_checkpoint="$(deep_review_cycle_checkpoint "$BATON_FILE" "$cur_checkpoint")"
    if ! jq -e --argjson review_checkpoint "$deep_review_required_checkpoint" '
      def done_angle($name):
        any(.subagent_tracks[];
          (
            .phase == "deep_review" and
            .track == $name and
            .review_checkpoint == $review_checkpoint and
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
      reason="deep_review->deslop requires current-cycle three completed review-angle subagent_tracks"
    fi
  fi
fi

if [[ "$legal" -ne 1 ]]; then
  echo "DVANDVA_WRITE illegal_transition $reason" >&2
  exit 24
fi

# Test-only deterministic interleaving seam. Unset in production (a single
# string test, zero cost). It NEVER reads or executes input -- it only touches
# and stats sentinel files -- so scripts/test-dvandva-write.sh can park a writer
# here (after it has read the current checkpoint and judged the transition legal,
# but before it installs) and deterministically let a peer steal the lock, in
# order to prove the fencing guarantee below. The worst a stray env value can do
# is make this one helper wait a few seconds, then continue.
if [[ -n "${DVANDVA_WRITE_BARRIER:-}" ]]; then
  : > "${DVANDVA_WRITE_BARRIER}.arrived" 2>/dev/null || true
  __barrier_waited=0
  while [[ ! -e "${DVANDVA_WRITE_BARRIER}.release" && "$__barrier_waited" -lt 200 ]]; do
    sleep 0.05
    __barrier_waited=$((__barrier_waited + 1))
  done
fi

# FENCING (mandatory before the irreversible install). We have already read the
# current checkpoint=N and judged our checkpoint=N+1 candidate legal. If, while
# we were inside the critical section, a peer judged our lock stale and stole it
# (age-based steal -- which fires when we are merely SLOW, not dead), the peer
# rewrote the owner token and is itself installing N+1. Installing now would
# clobber the peer's accepted write: two checkpoint+1 writers both "succeed". So
# re-verify we still own the lock; if the token differs or is gone, relinquish
# and abort fail-closed. (Skipped when LOCK_ACQUIRED=0, i.e. the rc-1 unlocked
# path on an unwritable dir, which fails at cp/mv with exit 26 anyway.)
if [[ "$LOCK_ACQUIRED" -eq 1 ]] && ! holds_lock; then
  echo "DVANDVA_WRITE lock_lost fencing_token_mismatch path=$LOCK_DIR refusing_to_install=true" >&2
  # The lock dir now belongs to the peer that stole it -- do NOT let our EXIT
  # trap remove it. Relinquish ownership before aborting.
  LOCK_ACQUIRED=0
  exit 29
fi

# BATON_DIR was computed and created above when the lock was acquired.
# Sweep tmp files orphaned by a killed writer; inert to readers but clutter.
# The concurrency lock above means no LIVE concurrent writer's tmp can exist in
# this window, so the glob only ever reaps abandoned tmp files.
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
