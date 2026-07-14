#!/usr/bin/env bash
# Pure, sourceable decision logic for the adversarial-loop Stop gate.
#
# Public API:
#   adversarial_loop_gate_predicate <resolved-repo-root> <session-id>
#
# The function always completes with shell status 0. Its one stdout line is
# tab-separated: "allow<TAB><reason>" or "block<TAB><reason>". Adapters own
# their event-specific output contract; gate.sh turns only block decisions into
# Claude Code Stop-hook JSON.

_adversarial_loop_predicate_result() {
  printf '%s\t%s\n' "$1" "$2"
}

_adversarial_loop_compare_timestamps() {
  local candidate=$1
  local current=$2
  local candidate_base=${candidate:0:19}
  local current_base=${current:0:19}
  local candidate_fraction=''
  local current_fraction=''
  local candidate_digit current_digit index max_length

  if [[ "$candidate_base" > "$current_base" ]]; then
    printf '1\n'
    return 0
  fi
  if [[ "$candidate_base" < "$current_base" ]]; then
    printf '%s\n' '-1'
    return 0
  fi

  if [[ "${candidate:19:1}" == . ]]; then
    candidate_fraction=${candidate:20}
    candidate_fraction=${candidate_fraction%Z}
  fi
  if [[ "${current:19:1}" == . ]]; then
    current_fraction=${current:20}
    current_fraction=${current_fraction%Z}
  fi

  max_length=${#candidate_fraction}
  if [[ ${#current_fraction} -gt $max_length ]]; then
    max_length=${#current_fraction}
  fi
  for ((index = 0; index < max_length; index++)); do
    candidate_digit=${candidate_fraction:index:1}
    current_digit=${current_fraction:index:1}
    candidate_digit=${candidate_digit:-0}
    current_digit=${current_digit:-0}
    if [[ "$candidate_digit" > "$current_digit" ]]; then
      printf '1\n'
      return 0
    fi
    if [[ "$candidate_digit" < "$current_digit" ]]; then
      printf '%s\n' '-1'
      return 0
    fi
  done

  printf '0\n'
}

_adversarial_loop_evidence_name_gt() {
  local candidate=$1
  local current=$2
  local candidate_number current_number

  if [[ "$candidate" =~ ^attempt-([0-9]+)\.json$ ]]; then
    candidate_number=${BASH_REMATCH[1]}
  else
    [[ "$candidate" > "$current" ]]
    return
  fi
  if [[ "$current" =~ ^attempt-([0-9]+)\.json$ ]]; then
    current_number=${BASH_REMATCH[1]}
  else
    [[ "$candidate" > "$current" ]]
    return
  fi

  while [[ ${#candidate_number} -gt 1 && "${candidate_number:0:1}" == 0 ]]; do
    candidate_number=${candidate_number:1}
  done
  while [[ ${#current_number} -gt 1 && "${current_number:0:1}" == 0 ]]; do
    current_number=${current_number:1}
  done

  if [[ ${#candidate_number} -ne ${#current_number} ]]; then
    [[ ${#candidate_number} -gt ${#current_number} ]]
    return
  fi
  if [[ "$candidate_number" != "$current_number" ]]; then
    [[ "$candidate_number" > "$current_number" ]]
    return
  fi

  [[ "$candidate" > "$current" ]]
}

adversarial_loop_gate_predicate() {
  local repo_root=${1-}
  local session_id=${2-}
  local state_dir goal_file status mode goal_id owner step_count
  local index id author author_agent_id revision step_status artifact_path artifact_digest
  local artifact_file computed_digest evidence_dir evidence_file evidence_name
  local evidence_goal_id evidence_step_id evidence_created_at latest_file
  local latest_created_at latest_name latest_revision latest_digest latest_verdict
  local latest_reviewer latest_reviewer_agent_id found_evidence evidence_timestamp_order
  local enumerated_step_count=0
  local -a step_ids step_authors step_author_agent_ids step_revisions step_statuses step_paths step_digests
  local -A seen_ids

  if [[ -z "$repo_root" || ! -d "$repo_root" ]]; then
    _adversarial_loop_predicate_result block 'resolved repo root is unavailable'
    return 0
  fi

  state_dir="$repo_root/.adversarial-loop"
  goal_file="$state_dir/goal.json"

  # The absent state is deliberately inert. A present-but-unreadable path is
  # not absent and falls through to fail-closed JSON validation.
  if [[ ! -e "$goal_file" && ! -L "$goal_file" ]]; then
    _adversarial_loop_predicate_result allow ''
    return 0
  fi

  if ! command -v jq >/dev/null 2>&1; then
    _adversarial_loop_predicate_result block 'jq is required to evaluate the active goal'
    return 0
  fi

  if ! jq -e 'type == "object" and (.status | type == "string")' "$goal_file" >/dev/null 2>&1; then
    _adversarial_loop_predicate_result block 'goal.json is malformed'
    return 0
  fi

  status=$(jq -r '.status' "$goal_file" 2>/dev/null)
  case "$status" in
    active | done) ;;
    abandoned)
      _adversarial_loop_predicate_result allow ''
      return 0
      ;;
    *)
      _adversarial_loop_predicate_result block "goal has invalid status: $status"
      return 0
      ;;
  esac

  if [[ -z "$session_id" ]]; then
    _adversarial_loop_predicate_result block 'enforced goal cannot be matched because session_id is missing'
    return 0
  fi

  if ! jq -e '
    .owner_session_id | (type == "string" and length > 0)
  ' "$goal_file" >/dev/null 2>&1; then
    _adversarial_loop_predicate_result block 'active goal owner_session_id is missing or invalid'
    return 0
  fi

  owner=$(jq -r '.owner_session_id' "$goal_file" 2>/dev/null)
  if [[ "$owner" != "$session_id" ]]; then
    _adversarial_loop_predicate_result allow ''
    return 0
  fi

  if ! jq -e '
    (.goal_id | (type == "string" and length > 0)) and
    (.acceptance | type == "string") and
    (.steps | type == "array")
  ' "$goal_file" >/dev/null 2>&1; then
    _adversarial_loop_predicate_result block 'active goal has missing or invalid required fields'
    return 0
  fi

  if ! jq -e '
    if has("mode") then
      (.mode == "cross-vendor" or .mode == "cross-context")
    else
      true
    end
  ' "$goal_file" >/dev/null 2>&1; then
    _adversarial_loop_predicate_result block 'active goal has invalid mode'
    return 0
  fi
  mode=$(jq -r 'if has("mode") then .mode else "cross-vendor" end' "$goal_file" 2>/dev/null)

  goal_id=$(jq -r '.goal_id' "$goal_file" 2>/dev/null)
  if [[ ! "$goal_id" =~ ^[a-z0-9][a-z0-9_-]{0,63}$ ]]; then
    _adversarial_loop_predicate_result block 'active goal has invalid goal id'
    return 0
  fi

  if ! command -v sha256sum >/dev/null 2>&1; then
    _adversarial_loop_predicate_result block 'sha256sum is required to evaluate the active goal'
    return 0
  fi

  step_count=$(jq -r '.steps | length' "$goal_file" 2>/dev/null)
  if [[ "$step_count" == 0 ]]; then
    _adversarial_loop_predicate_result block 'active goal steps empty'
    return 0
  fi

  # Step 4: validate the collection and each required step field before using
  # any value to form a path or decide whether the goal can pass.
  while IFS= read -r index; do
    if ! jq -e --argjson index "$index" '
      .steps[$index] |
      type == "object" and
      (.id | type == "string") and
      (.kind == "plan" or .kind == "execute") and
      (.author_family == "claude" or .author_family == "gpt") and
      ((has("author_agent_id") | not) or (.author_agent_id | type == "string")) and
      (.revision | (type == "number" and floor == . and . >= 1)) and
      (.status == "pending" or .status == "complete") and
      (.artifact_path | (type == "string" and length > 0)) and
      (
        (.status == "pending" and (.artifact_digest | (type == "string" and test("^([0-9a-f]{64})?$")))) or
        (.status == "complete" and (.artifact_digest | (type == "string" and test("^[0-9a-f]{64}$"))))
      )
    ' "$goal_file" >/dev/null 2>&1; then
      _adversarial_loop_predicate_result block "step at index $index has missing or invalid required fields"
      return 0
    fi

    id=$(jq -r --argjson index "$index" '.steps[$index].id' "$goal_file" 2>/dev/null)
    if [[ ! "$id" =~ ^[a-z0-9][a-z0-9_-]{0,63}$ ]]; then
      _adversarial_loop_predicate_result block "step $id has invalid id"
      return 0
    fi
    if [[ -n "${seen_ids[$id]+present}" ]]; then
      _adversarial_loop_predicate_result block "step $id has a duplicate id"
      return 0
    fi
    seen_ids["$id"]=1

    author=$(jq -r --argjson index "$index" '.steps[$index].author_family' "$goal_file" 2>/dev/null)
    author_agent_id=$(jq -r --argjson index "$index" '
      if (.steps[$index].author_agent_id? | type) == "string" then
        .steps[$index].author_agent_id
      else
        ""
      end
    ' "$goal_file" 2>/dev/null)
    if [[ "$mode" == cross-context && -z "$author_agent_id" ]]; then
      _adversarial_loop_predicate_result block "step $id author_agent_id is required in cross-context mode"
      return 0
    fi
    revision=$(jq -r --argjson index "$index" '.steps[$index].revision' "$goal_file" 2>/dev/null)
    step_status=$(jq -r --argjson index "$index" '.steps[$index].status' "$goal_file" 2>/dev/null)
    artifact_path=$(jq -r --argjson index "$index" '.steps[$index].artifact_path' "$goal_file" 2>/dev/null)
    artifact_digest=$(jq -r --argjson index "$index" '.steps[$index].artifact_digest' "$goal_file" 2>/dev/null)

    step_ids+=("$id")
    step_authors+=("$author")
    step_author_agent_ids+=("$author_agent_id")
    step_revisions+=("$revision")
    step_statuses+=("$step_status")
    step_paths+=("$artifact_path")
    step_digests+=("$artifact_digest")
    enumerated_step_count=$((enumerated_step_count + 1))
  done < <(jq -r '.steps | keys[]' "$goal_file" 2>/dev/null)

  if [[ "$enumerated_step_count" -ne "$step_count" ]]; then
    _adversarial_loop_predicate_result block 'active goal validated step count mismatch'
    return 0
  fi

  # Step 5: a completed step remains bound to the bytes of its recorded
  # artifact, rather than merely to a label in goal.json.
  for index in "${!step_ids[@]}"; do
    id=${step_ids[$index]}
    if [[ "${step_statuses[$index]}" != complete ]]; then
      continue
    fi

    artifact_path=${step_paths[$index]}
    if [[ "$artifact_path" == /* ]]; then
      artifact_file="$artifact_path"
    else
      artifact_file="$repo_root/$artifact_path"
    fi
    if ! computed_digest=$(sha256sum -- "$artifact_file" 2>/dev/null); then
      _adversarial_loop_predicate_result block "step $id artifact changed (artifact is unreadable)"
      return 0
    fi
    computed_digest=${computed_digest%%[[:space:]]*}
    if [[ "$computed_digest" != "${step_digests[$index]}" ]]; then
      _adversarial_loop_predicate_result block "step $id artifact changed"
      return 0
    fi
  done

  # Step 6: every step needs a latest, append-only evidence attempt. Earlier
  # failures remain on disk; ordering is explicit created_at then filename.
  for index in "${!step_ids[@]}"; do
    id=${step_ids[$index]}
    evidence_dir="$state_dir/evidence/$goal_id/$id"
    found_evidence=false
    latest_file=''
    latest_created_at=''
    latest_name=''

    if [[ -d "$evidence_dir" ]]; then
      while IFS= read -r -d '' evidence_file; do
        found_evidence=true
        evidence_name=${evidence_file##*/}
        if ! jq -e '
          type == "object" and
          (.goal_id | (type == "string" and length > 0)) and
          (.step_id | (type == "string" and length > 0)) and
          (.step_revision | (type == "number" and floor == . and . >= 1)) and
          (.artifact_digest | (type == "string" and test("^[0-9a-f]{64}$"))) and
          (.reviewer_family == "claude" or .reviewer_family == "gpt") and
          ((has("reviewer_agent_id") | not) or (.reviewer_agent_id | type == "string")) and
          (.reviewer_model | type == "string") and
          (.verdict == "pass" or .verdict == "fail") and
          (.findings | type == "array") and
          (.transcript_ref | type == "string") and
          (.created_at | (type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]+)?Z$")))
        ' "$evidence_file" >/dev/null 2>&1; then
          _adversarial_loop_predicate_result block "step $id has malformed evidence $evidence_name"
          return 0
        fi

        evidence_goal_id=$(jq -r '.goal_id' "$evidence_file" 2>/dev/null)
        evidence_step_id=$(jq -r '.step_id' "$evidence_file" 2>/dev/null)
        if [[ "$evidence_goal_id" != "$goal_id" || "$evidence_step_id" != "$id" ]]; then
          _adversarial_loop_predicate_result block "step $id evidence identity does not match its path"
          return 0
        fi

        evidence_created_at=$(jq -r '.created_at' "$evidence_file" 2>/dev/null)
        if [[ -z "$latest_file" ]]; then
          latest_file="$evidence_file"
          latest_created_at="$evidence_created_at"
          latest_name="$evidence_name"
          continue
        fi

        evidence_timestamp_order=$(_adversarial_loop_compare_timestamps "$evidence_created_at" "$latest_created_at")
        if [[ "$evidence_timestamp_order" -gt 0 ]] ||
          { [[ "$evidence_timestamp_order" -eq 0 ]] && _adversarial_loop_evidence_name_gt "$evidence_name" "$latest_name"; }; then
          latest_file="$evidence_file"
          latest_created_at="$evidence_created_at"
          latest_name="$evidence_name"
        fi
      done < <(find "$evidence_dir" -maxdepth 1 -type f -name '*.json' -print0 2>/dev/null)
    fi

    if [[ "$found_evidence" != true ]]; then
      _adversarial_loop_predicate_result block "step $id is missing evidence"
      return 0
    fi

    latest_revision=$(jq -r '.step_revision' "$latest_file" 2>/dev/null)
    latest_digest=$(jq -r '.artifact_digest' "$latest_file" 2>/dev/null)
    latest_verdict=$(jq -r '.verdict' "$latest_file" 2>/dev/null)
    latest_reviewer=$(jq -r '.reviewer_family' "$latest_file" 2>/dev/null)
    latest_reviewer_agent_id=$(jq -r '
      if (.reviewer_agent_id? | type) == "string" then
        .reviewer_agent_id
      else
        ""
      end
    ' "$latest_file" 2>/dev/null)

    if [[ "$latest_revision" != "${step_revisions[$index]}" ]]; then
      _adversarial_loop_predicate_result block "step $id latest evidence revision mismatch"
      return 0
    fi
    if [[ "$latest_digest" != "${step_digests[$index]}" ]]; then
      _adversarial_loop_predicate_result block "step $id latest evidence artifact digest mismatch"
      return 0
    fi
    if [[ "$latest_verdict" != pass ]]; then
      _adversarial_loop_predicate_result block "step $id latest evidence verdict is $latest_verdict"
      return 0
    fi
    if [[ "$mode" == cross-vendor ]]; then
      if [[ "$latest_reviewer" == "${step_authors[$index]}" ]]; then
        _adversarial_loop_predicate_result block "step $id latest evidence reviewer family matches author family"
        return 0
      fi
    else
      if [[ -z "$latest_reviewer_agent_id" ]]; then
        _adversarial_loop_predicate_result block "step $id latest evidence reviewer_agent_id is required in cross-context mode"
        return 0
      fi
      if [[ "$latest_reviewer_agent_id" == "${step_author_agent_ids[$index]}" ]]; then
        _adversarial_loop_predicate_result block "step $id latest evidence reviewer agent id matches author agent id"
        return 0
      fi
    fi
  done

  # Step 7 is deliberately after evidence validation, as specified: a pending
  # step cannot be used as a vacuous escape from cross-family review.
  for index in "${!step_ids[@]}"; do
    if [[ "${step_statuses[$index]}" != complete ]]; then
      _adversarial_loop_predicate_result block "step ${step_ids[$index]} not complete"
      return 0
    fi
  done

  _adversarial_loop_predicate_result allow ''
}
