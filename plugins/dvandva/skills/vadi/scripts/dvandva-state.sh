#!/usr/bin/env bash
# Emit bounded Dvandva baton state for routine transcript surfacing.
#
# This helper is bundled as a real executable inside each runtime skill:
#   plugins/dvandva/skills/vadi/scripts/dvandva-state.sh
#   plugins/dvandva/skills/prativadi/scripts/dvandva-state.sh
# The two copies must stay byte-identical.
set -u

# --- Delegating shim -------------------------------------------------------
# Prefer a compiled dvandva binary (DVANDVA_BIN > co-located binary > PATH).
# When found, exec it with the subcommand derived from this shim's own
# basename, forwarding all args unchanged (DVANDVA_ROLE / DVANDVA_* selectors
# pass through the environment automatically; the binary derives role from
# --role > DVANDVA_ROLE > argv0). When no binary is found, fall through to
# the preserved shell implementation below (unchanged behavior).
__dvandva_shim_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
__dvandva_shim_name="$(basename "${BASH_SOURCE[0]}")"
__dvandva_shim_subcmd="${__dvandva_shim_name#dvandva-}"
__dvandva_shim_subcmd="${__dvandva_shim_subcmd%.sh}"
__dvandva_shim_bin=""
if [[ -n "${DVANDVA_BIN:-}" && -x "${DVANDVA_BIN:-}" ]]; then
  __dvandva_shim_bin="$DVANDVA_BIN"
elif [[ -x "$__dvandva_shim_dir/dvandva" ]]; then
  __dvandva_shim_bin="$__dvandva_shim_dir/dvandva"
elif command -v dvandva >/dev/null 2>&1; then
  __dvandva_shim_bin="$(command -v dvandva)"
fi
if [[ -n "$__dvandva_shim_bin" ]]; then
  # CR-1: for `state`, preserve the shim-derived role (parent-of-parent dir)
  # so a delegated invocation without an explicit --role still resolves the
  # correct role. An explicit --role in "$@" still wins in the binary, and a
  # caller-provided DVANDVA_ROLE is left untouched. Not applied to `resolve`,
  # which always receives an explicit --role.
  if [[ "$__dvandva_shim_subcmd" == "state" && -z "${DVANDVA_ROLE:-}" ]]; then
    __dvandva_shim_role="$(basename "$(dirname "$__dvandva_shim_dir")")"
    if [[ "$__dvandva_shim_role" == "vadi" || "$__dvandva_shim_role" == "prativadi" ]]; then
      export DVANDVA_ROLE="$__dvandva_shim_role"
    fi
    unset __dvandva_shim_role
  fi
  exec "$__dvandva_shim_bin" "$__dvandva_shim_subcmd" "$@"
fi
unset __dvandva_shim_dir __dvandva_shim_name __dvandva_shim_subcmd __dvandva_shim_bin
# --- End delegating shim; preserved shell fallback continues below --------

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

if ! jq -e 'type == "object"' "$BATON_FILE" >/dev/null 2>&1; then
  echo "ERROR: baton JSON root must be object: $BATON_FILE" >&2
  exit 22
fi

jq --arg baton_file "$BATON_FILE" --arg role "$ROLE" '
  def string_limit: 240;
  def action_limit: 500;
  def item_limit: 10;

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

  def bounded_scalar($max):
    if . == null then
      null
    elif type == "string" then
      if length > $max then .[0:$max] + "...[truncated]" else . end
    else
      (tostring | if length > $max then .[0:$max] + "...[truncated]" else . end)
    end;

  def pick_bounded($keys; $max):
    with_entries(
      select(.key as $key | ($keys | index($key)) != null)
      | .value |= bounded_scalar($max)
    );

  def compact_verification($item):
    if ($item | type) == "object" then
      $item
      | {
          command: (.command | bounded_scalar(string_limit)),
          result: (.result | bounded_scalar(80)),
          notes: (.notes | bounded_scalar(string_limit))
        }
      | with_entries(select(.value != null and .value != ""))
    elif $item == null then
      {}
    else
      {
        command: ($item | bounded_scalar(string_limit)),
        result: "legacy"
      }
    end;

  def cap_items($max):
    if length > $max then
      .[0:$max] + [{more_count: (length - $max)}]
    else
      .
    end;

  def clean_refs:
    (
      if (.refs | type) == "object" then
        (.refs | pick_bounded(["branch", "base", "plan", "plan_ref", "research_ref", "run_explainer_ref", "review_ref"]; string_limit))
      else
        {}
      end
    ) + {
      research_ref: (.research_ref | bounded_scalar(string_limit)),
      plan_ref: (.plan_ref | bounded_scalar(string_limit)),
      run_explainer_ref: (.run_explainer_ref | bounded_scalar(string_limit)),
      review_ref: (.review_ref | bounded_scalar(string_limit))
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
    if type == "object" then
      ((.status // "open") | tostring | ascii_downcase) as $status |
      ($status != "closed" and $status != "resolved" and $status != "completed" and $status != "approved")
    else
      true
    end;

  def compact_work($root; $role):
    ($root.work_split // [] | as_array) as $items |
    ($root.phase // "" | tostring) as $current_phase |
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
    ] | cap_items(item_limit);

  def compact_findings:
    [
      (.findings // [] | as_array)[] |
      select(is_open_finding) |
      if type == "object" then
        {
          id: (.id // null),
          severity: (.severity // null),
          area: (.area // null),
          status: (.status // "open"),
          summary: (.summary | bounded_scalar(string_limit))
        }
        | with_entries(select(.value != null and .value != ""))
      else
        {
          id: null,
          severity: null,
          area: null,
          status: "open",
          summary: (. | bounded_scalar(string_limit))
        }
      end
    ] | cap_items(item_limit);

  def latest_verification:
    if (.verification_latest | type) == "object" then
      compact_verification(.verification_latest)
    elif (.verification | type) == "array" and (.verification | length) > 0 then
      compact_verification(.verification[-1])
    else
      {}
    end;

  def compact_next_action:
    if (.next_action | type) == "object" then
      (.next_action | pick_bounded(["owner_role", "role", "assignee", "status", "prompt", "summary", "action", "command"]; action_limit))
    elif .next_action == null then
      {}
    else
      (.next_action | bounded_scalar(action_limit))
    end;
  def development_mode:
    ((.mode // "") == "development" or (.mode // "") == "feature-pr");
  def effective_profile:
    if development_mode then (.profile // "full") else (.profile // null) end;
  def effective_profile_floor:
    if development_mode then (.profile_floor // effective_profile) else (.profile_floor // null) end;

  {
    kind: "BATON_STATE_COMPACT",
    baton_file: $baton_file,
    role: $role,
    schema: (.schema // null),
    run_id: (.run_id // null),
    mode: (.mode // null),
    profile: effective_profile,
    profile_floor: effective_profile_floor,
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
    next_action: compact_next_action
  }
' "$BATON_FILE"
