#!/usr/bin/env bash
# Emit bounded Dvandva baton state for routine transcript surfacing.
#
# This helper is bundled as a real executable inside each runtime skill:
#   plugins/dvandva/skills/vadi/scripts/dvandva-state.sh
#   plugins/dvandva/skills/prativadi/scripts/dvandva-state.sh
# The two copies must stay byte-identical.
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROLE="$(basename "$(dirname "$SCRIPT_DIR")")"
BATON_FILE=""
COMPACT=0

usage() {
  cat >&2 <<'USAGE'
Usage: dvandva-state.sh --compact --file <baton.json> [--role vadi|prativadi]

Emits BATON_STATE_COMPACT JSON: a bounded summary with refs, counts, current
role work, open findings, latest verification, and next_action. It does not
emit full dynamic arrays such as work_split, subagent_tracks, or
verification_matrix. Read the authoritative baton.json before state-changing
writes, approvals, human handoffs, or validation diagnosis.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --compact)
      COMPACT=1
      shift 1
      ;;
    --file)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      BATON_FILE="$2"
      shift 2
      ;;
    --role)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      ROLE="$2"
      shift 2
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

case "$ROLE" in
  vadi|prativadi|team|human) ;;
  *)
    echo "ERROR: --role must be vadi, prativadi, team, or human" >&2
    exit 2
    ;;
esac

if [[ "$COMPACT" -ne 1 || -z "$BATON_FILE" ]]; then
  usage
  exit 2
fi

if [[ ! -f "$BATON_FILE" ]]; then
  echo "ERROR: baton file not found: $BATON_FILE" >&2
  exit 21
fi

if ! jq -e . "$BATON_FILE" >/dev/null 2>&1; then
  echo "ERROR: baton JSON invalid: $BATON_FILE" >&2
  exit 22
fi

jq --arg baton_file "$BATON_FILE" --arg role "$ROLE" '
  def as_array:
    if type == "array" then .
    elif type == "object" then [to_entries[] | .value]
    else []
    end;

  def count_value($value):
    if ($value | type) == "array" or ($value | type) == "object" then
      ($value | length)
    else
      0
    end;

  def clean_refs:
    (
      if (.refs | type) == "object" then .refs else {} end
    ) + {
      research_ref: (.research_ref // null),
      plan_ref: (.plan_ref // null),
      run_explainer_ref: (.run_explainer_ref // null),
      review_ref: (.review_ref // null)
    }
    | with_entries(
        select(
          .value != null and
          .value != "" and
          .value != [] and
          .value != {}
        )
      );

  def is_open_finding:
    ((.status // "open") | tostring | ascii_downcase) as $status |
    ($status != "closed" and $status != "resolved" and $status != "completed" and $status != "approved");

  def compact_work($root; $role):
    ($root.work_split // [] | as_array) as $items |
    ($root.phase | tostring) as $current_phase |
    ($root.status // "" | tostring) as $current_status |
    [
      $items[] |
      select((.phase // "" | tostring) == $current_phase) |
      select(
        if $current_status == "parallel_implementing" then
          ((.chunk_type // .type // "implementation") == "implementation")
        else
          true
        end
      ) |
      select(((.owner_role // .owner // "") | tostring) == $role) |
      {
        id: (.id // null),
        phase: (.phase // null),
        chunk_type: (.chunk_type // .type // "implementation"),
        owner_role: (.owner_role // .owner // null),
        status: (.status // null),
        paths_count: count_value(.paths // []),
        write_paths_count: count_value(.write_paths // []),
        depends_on_count: count_value(.depends_on // [])
      }
    ];

  def compact_findings:
    [
      (.findings // [] | as_array)[] |
      select(is_open_finding) |
      {
        id: (.id // null),
        severity: (.severity // null),
        area: (.area // null),
        status: (.status // "open")
      }
    ];

  def latest_verification:
    if (.verification_latest | type) == "object" then
      .verification_latest
    elif (.verification | type) == "array" and (.verification | length) > 0 then
      (.verification[-1] | {
        command: (.command // null),
        result: (.result // null),
        notes: (.notes // null)
      })
    else
      {}
    end;

  {
    kind: "BATON_STATE_COMPACT",
    baton_file: $baton_file,
    role: $role,
    schema: (.schema // null),
    run_id: (.run_id // null),
    mode: (.mode // null),
    run_mode: (.run_mode // null),
    phase: (.phase // null),
    status: (.status // null),
    assignee: (.assignee // null),
    active_roles: (if (.active_roles | type) == "array" then .active_roles else [] end),
    checkpoint: (.checkpoint // null),
    refs: clean_refs,
    counts: {
      work_split: count_value(.work_split // []),
      subagent_tracks: count_value(.subagent_tracks // []),
      verification_matrix: count_value(.verification_matrix // []),
      findings: count_value(.findings // []),
      blockers: count_value(.blockers // []),
      changed_paths: count_value(.changed_paths // [])
    },
    current_role_work: compact_work(.; $role),
    open_findings: compact_findings,
    verification_latest: latest_verification,
    next_action: (.next_action // {})
  }
' "$BATON_FILE"
