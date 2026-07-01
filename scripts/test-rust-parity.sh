#!/usr/bin/env bash
# Differential parity harness: Rust `dvandva` read path vs the shell fallback.
#
# THE VERIFICATION GATE for the rust-migration run. It feeds ONE baton corpus
# through BOTH implementations of the same delegating shim and asserts they are
# equivalent per the plan's parity bar:
#
#   state   : exit codes identical; emitted JSON `jq -S` VALUE-EQUAL (key order
#             and whitespace are cosmetic, values are not).
#   resolve : exit codes identical; the leading token (RESOLVED/CREATE/ASK)
#             byte-identical; the RESOLVED/CREATE path byte-identical; the ASK
#             JSON array `jq -S` VALUE-EQUAL.
#
# Both behaviors come from the SAME shim (see plugins/dvandva/skills/*/scripts):
#   shell/fallback (vm-fallback)      : DVANDVA_BIN unset + no `dvandva` on PATH
#                                       + no co-located binary -> preserved shell
#                                       implementation runs (via jq).
#   rust/binary    (vm-shim-binary-path): DVANDVA_BIN=<release binary> -> the
#                                       shim exec's the compiled binary.
#
# The shell side is also the GROUND-TRUTH ORACLE: deterministic cases pin an
# expected exit + stdout on the shell side, so a mutual (both-wrong) regression
# is caught, not just a divergence.
#
# Hermetic: every scenario is built in a fresh temp dir; no dependence on the
# real .dvandva/. Requires `cargo build --release` first (rust/target/release).
#
# Idiom mirrors scripts/test-dvandva-{state,resolve}.sh: a `failures` counter,
# PASS:/FAIL: lines per case, and exit 1 if any case fails.
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT_DIR/rust/target/release/dvandva"
STATE_VADI="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-state.sh"
STATE_PRATIVADI="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-state.sh"
RESOLVE_VADI="$ROOT_DIR/plugins/dvandva/skills/vadi/scripts/dvandva-resolve.sh"
RESOLVE_PRATIVADI="$ROOT_DIR/plugins/dvandva/skills/prativadi/scripts/dvandva-resolve.sh"
REFERENCE_BATON="$ROOT_DIR/plugins/dvandva/references/baton-schema-v2.json"

TMP_DIR="$(mktemp -d)"
cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT

failures=0
cases=0

pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1"; failures=$((failures + 1)); }

# --- Startup guards --------------------------------------------------------
# The binary must exist, and NO global `dvandva` may be on PATH — a global
# binary would poison the "shell/fallback" side (the shim would delegate to it
# instead of running the shell implementation), silently turning every parity
# comparison into rust-vs-rust.
if [[ ! -x "$BIN" ]]; then
  echo "FATAL: release binary missing at $BIN"
  echo "       build it first: (cd $ROOT_DIR/rust && cargo build --release)"
  exit 2
fi
if command -v dvandva >/dev/null 2>&1; then
  echo "FATAL: a 'dvandva' binary is on PATH ($(command -v dvandva)); it would"
  echo "       poison the shell/fallback side of the differential harness."
  echo "       Remove it from PATH before running this harness."
  exit 2
fi

# Env scrub shared by both sides so selector state is hermetic regardless of the
# caller's environment. SEL[] holds the per-case selector assignments.
SEL=()
SCRUB=(-u DVANDVA_BATON_FILE -u DVANDVA_RUN_DIR -u DVANDVA_RUN_ID -u DVANDVA_ROLE)

# run_shell / run_rust: run an identical command with only the DVANDVA_BIN
# environment differing. run_shell forces the preserved shell fallback;
# run_rust forces delegation to the compiled binary.
# NOTE: GNU `env` requires all -u options BEFORE any NAME=VALUE assignment, so
# SCRUB (the -u flags) precedes the DVANDVA_BIN assignment on the rust side.
run_shell() { env -u DVANDVA_BIN "${SCRUB[@]}" "${SEL[@]}" "$@"; }
run_rust() { env "${SCRUB[@]}" DVANDVA_BIN="$BIN" "${SEL[@]}" "$@"; }

# --- Fixtures --------------------------------------------------------------
seed_baton() {
  # seed_baton <file> <run_id> <status> <assignee> <updated_at>
  local file="$1" run_id="$2" status="$3" assignee="$4" updated_at="$5"
  mkdir -p "$(dirname "$file")"
  cat >"$file" <<JSON
{
  "schema": "dvandva.baton.v2",
  "run_id": "$run_id",
  "status": "$status",
  "assignee": "$assignee",
  "phase": 1,
  "checkpoint": 3,
  "updated_at": "$updated_at"
}
JSON
}

seed_baton_no_checkpoint() {
  # A VALID baton missing `checkpoint` entirely (leniency probe).
  local file="$1" run_id="$2" status="$3" assignee="$4" updated_at="$5"
  mkdir -p "$(dirname "$file")"
  cat >"$file" <<JSON
{
  "schema": "dvandva.baton.v2",
  "run_id": "$run_id",
  "status": "$status",
  "assignee": "$assignee",
  "updated_at": "$updated_at"
}
JSON
}

# ===========================================================================
# RESOLVE comparator.
#
#   parity_resolve NAME EXP_EXIT EXP_STDOUT -- CMD...
#     EXP_EXIT   : expected exit on the SHELL oracle (>=0), or -1 to skip pin.
#     EXP_STDOUT : expected exact SHELL stdout line, or "" to skip the pin
#                  (used for non-deterministic ASK arrays).
#   Selector env is taken from the global SEL[] array; it is reset afterwards.
# ===========================================================================
parity_resolve() {
  local name="$1" exp_exit="$2" exp_stdout="$3"
  shift 3
  [[ "${1:-}" == "--" ]] && shift
  cases=$((cases + 1))
  local d="$TMP_DIR/r$cases"
  mkdir -p "$d"

  run_shell "$@" >"$d/sh.out" 2>"$d/sh.err"
  local sh_exit=$?
  run_rust "$@" >"$d/rs.out" 2>"$d/rs.err"
  local rs_exit=$?
  SEL=()

  local sh_line rs_line sh_tok rs_tok
  sh_line="$(cat "$d/sh.out")"
  rs_line="$(cat "$d/rs.out")"
  sh_tok="${sh_line%% *}"
  rs_tok="${rs_line%% *}"

  # Oracle pin: shell must match the known-good expected behavior.
  if [[ "$exp_exit" -ge 0 && "$sh_exit" -ne "$exp_exit" ]]; then
    fail "$name — shell oracle exit $sh_exit != expected $exp_exit (out:$sh_line err:$(cat "$d/sh.err"))"
    return
  fi
  if [[ -n "$exp_stdout" && "$sh_line" != "$exp_stdout" ]]; then
    fail "$name — shell oracle stdout mismatch; expected [$exp_stdout] got [$sh_line]"
    return
  fi

  # Parity bar: exit codes identical.
  if [[ "$sh_exit" -ne "$rs_exit" ]]; then
    fail "$name — EXIT divergence shell=$sh_exit rust=$rs_exit (shell:[$sh_line] rust:[$rs_line])"
    return
  fi

  # Parity bar: leading token byte-identical.
  if [[ "$sh_tok" != "$rs_tok" ]]; then
    fail "$name — TOKEN divergence shell=[$sh_tok] rust=[$rs_tok]"
    return
  fi

  case "$sh_tok" in
    RESOLVED | CREATE)
      # Path byte-identical -> whole line byte-identical.
      if [[ "$sh_line" != "$rs_line" ]]; then
        fail "$name — PATH divergence shell:[$sh_line] rust:[$rs_line]"
        return
      fi
      ;;
    ASK)
      # ASK JSON array value-equal via jq -S.
      local sh_json="${sh_line#ASK }" rs_json="${rs_line#ASK }"
      if ! diff <(jq -S . <<<"$sh_json" 2>/dev/null) <(jq -S . <<<"$rs_json" 2>/dev/null) >"$d/askdiff" 2>&1; then
        fail "$name — ASK array NOT value-equal"
        sed 's/^/    /' "$d/askdiff"
        return
      fi
      ;;
    *)
      # Usage / empty stdout (exit 2): exit + token equality already checked;
      # require stdout to match byte-for-byte (both empty).
      if [[ "$sh_line" != "$rs_line" ]]; then
        fail "$name — stdout divergence shell:[$sh_line] rust:[$rs_line]"
        return
      fi
      ;;
  esac

  pass "$name (exit=$sh_exit token=${sh_tok:-<none>})"
}

# ===========================================================================
# STATE comparator.
#   parity_state NAME EXP_EXIT SCRIPT --file <baton> --role <r>
#     On exit 0 the emitted JSON is compared jq -S value-equal.
#     On nonzero exit the exit code parity is the contract (no stdout JSON).
# ===========================================================================
parity_state() {
  local name="$1" exp_exit="$2"
  shift 2
  cases=$((cases + 1))
  local d="$TMP_DIR/s$cases"
  mkdir -p "$d"

  run_shell "$@" >"$d/sh.out" 2>"$d/sh.err"
  local sh_exit=$?
  run_rust "$@" >"$d/rs.out" 2>"$d/rs.err"
  local rs_exit=$?
  SEL=()

  if [[ "$exp_exit" -ge 0 && "$sh_exit" -ne "$exp_exit" ]]; then
    fail "$name — shell oracle exit $sh_exit != expected $exp_exit (err:$(cat "$d/sh.err"))"
    return
  fi
  if [[ "$sh_exit" -ne "$rs_exit" ]]; then
    fail "$name — EXIT divergence shell=$sh_exit rust=$rs_exit (shell-err:$(cat "$d/sh.err") rust-err:$(cat "$d/rs.err"))"
    return
  fi

  if [[ "$sh_exit" -eq 0 ]]; then
    if ! diff <(jq -S . "$d/sh.out" 2>/dev/null) <(jq -S . "$d/rs.out" 2>/dev/null) >"$d/statediff" 2>&1; then
      fail "$name — state JSON NOT value-equal (jq -S)"
      sed 's/^/    /' "$d/statediff"
      return
    fi
  fi

  pass "$name (exit=$sh_exit)"
}

echo "=== RESOLVE parity ==="

# R1: empty tree -> CREATE deterministic slug.
BOX="$TMP_DIR/box-empty"
mkdir -p "$BOX"
parity_resolve "R1 empty tree -> CREATE run" 0 "CREATE .dvandva/runs/run/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R2: exactly one resumable run -> RESOLVED.
BOX="$TMP_DIR/box-one"
seed_baton "$BOX/.dvandva/runs/alpha/baton.json" alpha implementing vadi "2026-06-29T10:00:00Z"
parity_resolve "R2 one resumable -> RESOLVED" 0 "RESOLVED .dvandva/runs/alpha/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R3: human_decision is resumable -> RESOLVED (NOT create).
BOX="$TMP_DIR/box-hdec"
seed_baton "$BOX/.dvandva/runs/decide/baton.json" decide human_decision human "2026-06-29T10:00:00Z"
parity_resolve "R3 human_decision resumable -> RESOLVED" 0 "RESOLVED .dvandva/runs/decide/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R4: human_question is resumable -> RESOLVED.
BOX="$TMP_DIR/box-hq"
seed_baton "$BOX/.dvandva/runs/askrun/baton.json" askrun human_question human "2026-06-29T10:00:00Z"
parity_resolve "R4 human_question resumable -> RESOLVED" 0 "RESOLVED .dvandva/runs/askrun/baton.json" \
  -- "$RESOLVE_PRATIVADI" --role prativadi --cwd "$BOX"

# R5: only a done archive -> CREATE (done is the only terminal status).
BOX="$TMP_DIR/box-done"
seed_baton "$BOX/.dvandva/runs/finished/baton.json" finished done human "2026-06-29T10:00:00Z"
parity_resolve "R5 only done -> CREATE" 0 "CREATE .dvandva/runs/run/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R6: done archive named 'run' -> deterministic non-colliding slug run-2.
BOX="$TMP_DIR/box-done-run"
seed_baton "$BOX/.dvandva/runs/run/baton.json" run done human "2026-06-29T10:00:00Z"
parity_resolve "R6 done named run -> CREATE run-2" 0 "CREATE .dvandva/runs/run-2/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R7: two resumable runs, no selector -> ASK exit 12 (array value-equal).
BOX="$TMP_DIR/box-two"
seed_baton "$BOX/.dvandva/runs/alpha/baton.json" alpha spec_review prativadi "2026-06-29T10:00:00Z"
seed_baton "$BOX/.dvandva/runs/beta/baton.json" beta implementing vadi "2026-06-29T11:00:00Z"
parity_resolve "R7 two resumable -> ASK(12)" 12 "" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R8: ASK ordering deterministic: updated_at desc (beta 11:00 before alpha 10:00).
# (Same tree as R7.) Verify both sides agree AND match the expected order.
sh_ask="$(run_shell "$RESOLVE_VADI" --role vadi --cwd "$BOX" 2>/dev/null)"
rs_ask="$(run_rust "$RESOLVE_VADI" --role vadi --cwd "$BOX" 2>/dev/null)"
SEL=()
cases=$((cases + 1))
sh_order="$(jq -r '[.[].run_id]|join(",")' <<<"${sh_ask#ASK }" 2>/dev/null)"
rs_order="$(jq -r '[.[].run_id]|join(",")' <<<"${rs_ask#ASK }" 2>/dev/null)"
if [[ "$sh_order" == "beta,alpha" && "$rs_order" == "beta,alpha" ]]; then
  pass "R8 ASK order updated_at desc (beta,alpha) — shell==rust==expected"
else
  fail "R8 ASK order — expected beta,alpha shell=[$sh_order] rust=[$rs_order]"
fi

# R9: updated_at tie -> run_id ascending tiebreak; shell==rust==expected.
BOX="$TMP_DIR/box-tie"
seed_baton "$BOX/.dvandva/runs/zeta/baton.json" zeta spec_review prativadi "2026-06-29T10:00:00Z"
seed_baton "$BOX/.dvandva/runs/gamma/baton.json" gamma implementing vadi "2026-06-29T10:00:00Z"
sh_tie="$(run_shell "$RESOLVE_VADI" --role vadi --cwd "$BOX" 2>/dev/null)"
rs_tie="$(run_rust "$RESOLVE_VADI" --role vadi --cwd "$BOX" 2>/dev/null)"
SEL=()
cases=$((cases + 1))
sh_to="$(jq -r '[.[].run_id]|join(",")' <<<"${sh_tie#ASK }" 2>/dev/null)"
rs_to="$(jq -r '[.[].run_id]|join(",")' <<<"${rs_tie#ASK }" 2>/dev/null)"
if [[ "$sh_to" == "gamma,zeta" && "$rs_to" == "gamma,zeta" ]]; then
  pass "R9 ASK tie -> run_id asc (gamma,zeta) — shell==rust==expected"
else
  fail "R9 ASK tie — expected gamma,zeta shell=[$sh_to] rust=[$rs_to]"
fi

# R10: corrupt baton during discovery -> fail-closed ASK exit 12, stderr names it.
BOX="$TMP_DIR/box-corrupt"
seed_baton "$BOX/.dvandva/runs/valid/baton.json" valid implementing vadi "2026-06-29T10:00:00Z"
mkdir -p "$BOX/.dvandva/runs/corrupt"
printf '{ not valid json\n' >"$BOX/.dvandva/runs/corrupt/baton.json"
parity_resolve "R10 corrupt in discovery -> ASK(12) fail-closed" 12 "ASK []" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"
# Both sides must NAME the corrupt path on stderr.
cases=$((cases + 1))
sh_e="$(run_shell "$RESOLVE_VADI" --role vadi --cwd "$BOX" 2>&1 >/dev/null)"
rs_e="$(run_rust "$RESOLVE_VADI" --role vadi --cwd "$BOX" 2>&1 >/dev/null)"
SEL=()
if [[ "$sh_e" == *"corrupt"* && "$rs_e" == *"corrupt"* ]]; then
  pass "R10b corrupt path named on stderr (both sides)"
else
  fail "R10b corrupt path not named — shell:[$sh_e] rust:[$rs_e]"
fi

# R11: LONE corrupt baton, no valid sibling -> ASK exit 12 (NOT CREATE).
BOX="$TMP_DIR/box-corrupt-only"
mkdir -p "$BOX/.dvandva/runs/broken"
printf '{ bad json entirely\n' >"$BOX/.dvandva/runs/broken/baton.json"
parity_resolve "R11 lone corrupt -> ASK(12) not CREATE" 12 "ASK []" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R12: corrupt LEGACY .dvandva/baton.json -> fail-closed ASK exit 12.
BOX="$TMP_DIR/box-corrupt-legacy"
mkdir -p "$BOX/.dvandva"
printf '{ bad\n' >"$BOX/.dvandva/baton.json"
parity_resolve "R12 corrupt legacy baton -> ASK(12)" 12 "ASK []" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R13: selector precedence — DVANDVA_BATON_FILE beats RUN_DIR beats RUN_ID.
BOX="$TMP_DIR/box-prec"
FILEWIN="$BOX/custom/win-baton.json"
seed_baton "$FILEWIN" winfile spec_review vadi "2026-06-29T10:00:00Z"
seed_baton "$BOX/.dvandva/runs/dirrun/baton.json" dirrun implementing vadi "2026-06-29T10:00:00Z"
SEL=(DVANDVA_BATON_FILE="$FILEWIN" DVANDVA_RUN_DIR="$BOX/.dvandva/runs/dirrun" DVANDVA_RUN_ID=idrun)
parity_resolve "R13 BATON_FILE beats RUN_DIR+RUN_ID" 0 "RESOLVED $FILEWIN" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R14: DVANDVA_RUN_DIR beats DVANDVA_RUN_ID (BATON_FILE absent).
SEL=(DVANDVA_RUN_DIR="$BOX/.dvandva/runs/dirrun" DVANDVA_RUN_ID=idrun)
parity_resolve "R14 RUN_DIR beats RUN_ID" 0 "RESOLVED $BOX/.dvandva/runs/dirrun/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R15: safe DVANDVA_RUN_ID -> RESOLVED .dvandva/runs/<id>/baton.json.
BOX="$TMP_DIR/box-runid"
mkdir -p "$BOX"
SEL=(DVANDVA_RUN_ID=alpha)
parity_resolve "R15 safe RUN_ID -> RESOLVED named path" 0 "RESOLVED .dvandva/runs/alpha/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R16: DVANDVA_RUN_DIR trailing slash normalized.
BOX="$TMP_DIR/box-slash"
seed_baton "$BOX/.dvandva/runs/gamma/baton.json" gamma implementing vadi "2026-06-29T10:00:00Z"
SEL=(DVANDVA_RUN_DIR="$BOX/.dvandva/runs/gamma/")
parity_resolve "R16 RUN_DIR trailing slash normalized" 0 "RESOLVED $BOX/.dvandva/runs/gamma/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R17: unsafe DVANDVA_RUN_ID values -> exit 2 (before any fs op).
BOX="$TMP_DIR/box-unsafe"
mkdir -p "$BOX"
for bad in "../x" "a/b" ".." "a..b" 'a\b' "" "   "; do
  SEL=(DVANDVA_RUN_ID="$bad")
  parity_resolve "R17 unsafe RUN_ID [$bad] -> exit 2" 2 "" \
    -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"
done
# R17b: unsafe RUN_ID rejected BEFORE touching fs (nonexistent --cwd still exit 2).
SEL=(DVANDVA_RUN_ID="../escape")
parity_resolve "R17b unsafe RUN_ID rejected before fs op" 2 "" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$TMP_DIR/does-not-exist"

# R18: legacy .dvandva/baton.json participates in discovery as one resumable run.
BOX="$TMP_DIR/box-legacy"
seed_baton "$BOX/.dvandva/baton.json" legacy-run implementing vadi "2026-06-29T10:00:00Z"
parity_resolve "R18 legacy baton -> RESOLVED legacy path" 0 "RESOLVED .dvandva/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R19 LENIENCY: a VALID baton whose status is an unknown/future token, single
# run -> RESOLVED (shell tolerates `.status // ""`; Rust must not fail-strict).
BOX="$TMP_DIR/box-future"
seed_baton "$BOX/.dvandva/runs/futurerun/baton.json" futurerun some_future_status_v9 vadi "2026-06-29T10:00:00Z"
parity_resolve "R19 LENIENCY future status single -> RESOLVED" 0 "RESOLVED .dvandva/runs/futurerun/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R20 LENIENCY: future-status AND missing-checkpoint runs surfaced in an ASK
# list -> both tolerated, array value-equal, deterministic order.
BOX="$TMP_DIR/box-future-multi"
seed_baton "$BOX/.dvandva/runs/aa/baton.json" aa future_token_x vadi "2026-06-29T11:00:00Z"
seed_baton_no_checkpoint "$BOX/.dvandva/runs/bb/baton.json" bb another_future_token human "2026-06-29T10:00:00Z"
parity_resolve "R20 LENIENCY future+no-checkpoint in ASK -> value-equal" 12 "" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R21: explicit DVANDVA_RUN_ID bypasses discovery even with a corrupt sibling.
BOX="$TMP_DIR/box-explicit-corrupt"
mkdir -p "$BOX/.dvandva/runs/corrupt"
printf '{ bad json\n' >"$BOX/.dvandva/runs/corrupt/baton.json"
SEL=(DVANDVA_RUN_ID=target)
parity_resolve "R21 explicit RUN_ID bypasses corrupt discovery -> RESOLVED" 0 "RESOLVED .dvandva/runs/target/baton.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R22: explicit DVANDVA_BATON_FILE (non-existent) -> RESOLVED unconditionally.
BOX="$TMP_DIR/box-explicit-missing"
mkdir -p "$BOX"
SEL=(DVANDVA_BATON_FILE="/tmp/no-such-dvandva-baton-parity.json")
parity_resolve "R22 explicit BATON_FILE (missing) -> RESOLVED unconditionally" 0 "RESOLVED /tmp/no-such-dvandva-baton-parity.json" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# R23: usage errors — missing role, unknown role -> exit 2.
BOX="$TMP_DIR/box-usage"
mkdir -p "$BOX"
parity_resolve "R23a missing --role -> exit 2" 2 "" \
  -- "$RESOLVE_VADI" --cwd "$BOX"
parity_resolve "R23b unknown role -> exit 2" 2 "" \
  -- "$RESOLVE_VADI" --role bystander --cwd "$BOX"

# ---------------------------------------------------------------------------
# REGRESSION cases from cross-review (resolve). The original 55 cases missed
# these. F2b/F3 are EXPECTED TO FAIL until prativadi's resolve.rs/state.rs fixes
# land; the assertions below are the CORRECT shell==rust parity bar (do NOT
# weaken them to make rust pass — they turn green when the bug is fixed).
# ---------------------------------------------------------------------------

# F2b REGRESSION (expect FAIL): a sibling baton with `run_id:false` and
# `status:false`. The shell coalesces `false` via `.x // ""` to the empty string
# (false is falsy for jq `//`), so run_id falls back to the DIRECTORY NAME ("aa")
# and status becomes "" (non-"done" -> resumable). With a second normal run this
# is two resumable runs -> ASK(12), ordered updated_at desc (bb 06-29 before
# aa 01-01). Rust currently LEAKS the literal string "false" for run_id/status,
# so the ASK array is not value-equal. Assert exit + leading token + ASK jq -S
# value-equal against the shell oracle.
BOX="$TMP_DIR/box-f2b-false-scalars"
mkdir -p "$BOX/.dvandva/runs/aa"
cat >"$BOX/.dvandva/runs/aa/baton.json" <<'JSON'
{"schema":"dvandva.baton.v2","run_id":false,"status":false,"updated_at":"2026-01-01T00:00:00Z","checkpoint":1}
JSON
seed_baton "$BOX/.dvandva/runs/bb/baton.json" bb-run implementing vadi "2026-06-29T10:00:00Z"
parity_resolve "F2b REGRESSION false run_id/status coalesce -> dir-name fallback + empty status in ASK(12)" 12 "" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# F3a REGRESSION (expect FAIL): a run dir whose `baton.json` is itself a
# DIRECTORY. It still matches the discovery glob, but jq cannot read it, so the
# shell FAILS CLOSED -> `ASK []` exit 12 (never a wrong CREATE/RESOLVED). Rust
# currently ignores the unreadable entry and emits CREATE exit 0. Assert parity
# to the fail-closed shell oracle.
BOX="$TMP_DIR/box-f3a-dir-baton"
mkdir -p "$BOX/.dvandva/runs/foo/baton.json"
parity_resolve "F3a REGRESSION baton.json is a DIRECTORY -> fail-closed ASK(12)" 12 "ASK []" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

# F3b REGRESSION (expect FAIL): a run dir whose `baton.json` is a BROKEN SYMLINK
# (target does not exist). The glob matches the dangling entry, jq cannot open
# the target, so the shell FAILS CLOSED -> `ASK []` exit 12. Rust currently
# treats it as absent and emits CREATE exit 0. Assert parity to the shell oracle.
BOX="$TMP_DIR/box-f3b-broken-symlink"
mkdir -p "$BOX/.dvandva/runs/bar"
ln -s /nonexistent-dvandva-target "$BOX/.dvandva/runs/bar/baton.json"
parity_resolve "F3b REGRESSION baton.json is a BROKEN SYMLINK -> fail-closed ASK(12)" 12 "ASK []" \
  -- "$RESOLVE_VADI" --role vadi --cwd "$BOX"

echo
echo "=== STATE parity ==="

# --- State fixtures --------------------------------------------------------
FULL="$TMP_DIR/full.json"
cat >"$FULL" <<'JSON'
{
  "schema": "dvandva.baton.v2",
  "run_id": "token-efficient-runs",
  "mode": "development",
  "profile": "standard",
  "profile_floor": "standard",
  "profile_decision": {"selected_profile": "standard", "floor": "standard"},
  "profile_history": [],
  "run_mode": "walkaway",
  "phase": 1,
  "status": "parallel_implementing",
  "assignee": "team",
  "active_roles": ["vadi", "prativadi"],
  "checkpoint": 42,
  "refs": {"branch": "token-efficient-runs", "base": "main", "plan": "superpowers/plans/token-efficient-runs.html"},
  "work_split": [
    {"id": "impl-tests", "phase": 1, "chunk_type": "implementation", "owner_role": "vadi", "status": "ready", "depends_on": ["contract"], "paths": ["scripts/test-dvandva-state.sh"], "cross_review_by": "prativadi", "notes": "must not leak into current_role_work"},
    {"id": "impl-core", "phase": 1, "chunk_type": "implementation", "owner_role": "vadi", "status": "pending", "depends_on": ["impl-tests"], "paths": ["a", "b"], "write_paths": ["a"], "notes": "verbose"},
    {"id": "review-core", "phase": 1, "chunk_type": "implementation", "owner_role": "prativadi", "status": "pending", "depends_on": ["impl-core"], "paths": ["c"]}
  ],
  "subagent_tracks": [
    {"id": "writer", "owner_role": "vadi", "status": "complete"},
    {"id": "reviewer", "owner_role": "prativadi", "status": "pending"}
  ],
  "verification_matrix": [
    {"id": "vm-red", "status": "red"},
    {"id": "vm-green", "status": "pending"}
  ],
  "findings": [
    {"id": "F-1", "status": "open", "severity": "medium", "summary": "open finding"},
    {"id": "F-2", "status": "resolved", "severity": "low", "summary": "closed finding"}
  ],
  "blockers": [{"id": "B-1", "status": "open", "summary": "helper missing"}],
  "changed_paths": ["scripts/test-dvandva-state.sh", "plugins/dvandva/skills/vadi/scripts/dvandva-state.sh"],
  "verification_latest": {"command": "scripts/test-dvandva-state.sh", "result": "red", "notes": "short note", "extra": "must not be surfaced"},
  "next_action": {"owner_role": "vadi", "prompt": "Implement the compact state helper.", "private": "drop"}
}
JSON

# S1/S2: full v2 baton, both roles (proves role-scoped current_role_work + that
# dynamic arrays work_split/subagent_tracks/verification_matrix are omitted).
parity_state "S1 full v2 baton (vadi)" 0 "$STATE_VADI" --compact --file "$FULL" --role vadi
parity_state "S2 full v2 baton (prativadi)" 0 "$STATE_PRATIVADI" --compact --file "$FULL" --role prativadi

# S3: missing optional refs / empty arrays.
MINIMAL="$TMP_DIR/minimal.json"
cat >"$MINIMAL" <<'JSON'
{
  "schema": "dvandva.baton.v2",
  "run_id": "minimal-run",
  "mode": "development",
  "run_mode": "supervised",
  "phase": 1,
  "status": "implementing",
  "assignee": "vadi",
  "active_roles": ["vadi"],
  "checkpoint": 3,
  "work_split": [],
  "subagent_tracks": [],
  "verification_matrix": [],
  "findings": [],
  "blockers": [],
  "changed_paths": []
}
JSON
parity_state "S3 missing optional refs" 0 "$STATE_VADI" --compact --file "$MINIMAL" --role vadi

# S4: oversize (>240) AND multibyte/UTF-8 summary + next_action (codepoint vs
# byte slicing is THE key state-truncation parity risk).
MB="$TMP_DIR/mb.json"
long_mb="$(printf 'a%.0s' $(seq 1 60))$(printf 'é%.0s' $(seq 1 100))$(printf 'あ%.0s' $(seq 1 100))"
jq -n --arg s "$long_mb" '{
  schema: "dvandva.baton.v2", run_id: "mb", mode: "development",
  status: "implementing", assignee: "vadi", phase: 1, checkpoint: 1,
  work_split: [], subagent_tracks: [], verification_matrix: [],
  findings: [{id: "F-1", status: "open", severity: "low", summary: $s}],
  blockers: [], changed_paths: [],
  next_action: $s
}' >"$MB"
parity_state "S4 oversize multibyte summary+next_action (codepoint slicing)" 0 "$STATE_VADI" --compact --file "$MB" --role vadi

# S5: findings as a top-level SCALAR string.
SCALAR="$TMP_DIR/scalar-findings.json"
cat >"$SCALAR" <<'JSON'
{
  "schema": "dvandva.baton.v2", "run_id": "scalar-findings", "mode": "development",
  "status": "implementing", "assignee": "vadi", "phase": 1, "checkpoint": 1,
  "work_split": [], "subagent_tracks": [], "verification_matrix": [],
  "findings": "just-a-scalar-string", "blockers": [], "changed_paths": []
}
JSON
parity_state "S5 findings as scalar string" 0 "$STATE_VADI" --compact --file "$SCALAR" --role vadi

# S6: findings as an ARRAY containing a bare string (legacy finding element).
ARRFIND="$TMP_DIR/array-string-findings.json"
cat >"$ARRFIND" <<'JSON'
{
  "schema": "dvandva.baton.v2", "run_id": "legacy-string-findings", "mode": "development",
  "profile": "standard", "profile_floor": "standard",
  "status": "cross_fixing", "assignee": "team", "phase": 1, "checkpoint": 12,
  "work_split": [], "subagent_tracks": [], "verification_matrix": [],
  "findings": ["legacy finding string"], "blockers": [], "changed_paths": []
}
JSON
parity_state "S6 findings array-with-string element" 0 "$STATE_VADI" --compact --file "$ARRFIND" --role vadi

# S7: legacy STRING verification array -> {command, result:"legacy"}.
LEGVER="$TMP_DIR/legacy-verification.json"
cat >"$LEGVER" <<'JSON'
{
  "schema": "dvandva.baton.v2", "run_id": "legacy-string-verification", "mode": "development",
  "profile": "fast", "profile_floor": "fast",
  "status": "implementing", "assignee": "vadi", "phase": 1, "checkpoint": 11,
  "work_split": [], "subagent_tracks": [], "verification_matrix": [],
  "verification": ["legacy verification string"],
  "findings": [], "blockers": [], "changed_paths": []
}
JSON
parity_state "S7 legacy string verification -> result:legacy" 0 "$STATE_VADI" --compact --file "$LEGVER" --role vadi

# S8: mode=feature-pr -> development_mode true, profile defaults to "full".
FEATUREPR="$TMP_DIR/feature-pr.json"
cat >"$FEATUREPR" <<'JSON'
{
  "schema": "dvandva.baton.v2", "run_id": "feature-pr-run", "mode": "feature-pr",
  "status": "implementing", "assignee": "vadi", "phase": 1, "checkpoint": 5,
  "work_split": [], "subagent_tracks": [], "verification_matrix": [],
  "findings": [], "blockers": [], "changed_paths": []
}
JSON
parity_state "S8 mode=feature-pr (profile defaults full)" 0 "$STATE_VADI" --compact --file "$FEATUREPR" --role vadi

# S9: non-object root JSON -> exit 22.
NONOBJ="$TMP_DIR/nonobj.json"
printf '[]\n' >"$NONOBJ"
parity_state "S9 non-object root -> exit 22" 22 "$STATE_VADI" --compact --file "$NONOBJ" --role vadi

# S10: missing file -> exit 21.
parity_state "S10 missing file -> exit 21" 21 "$STATE_VADI" --compact --file "$TMP_DIR/no-such-baton.json" --role vadi

# S11: invalid JSON -> exit 22.
INVALID="$TMP_DIR/invalid.json"
printf '{ "schema": "x",\n' >"$INVALID"
parity_state "S11 invalid JSON -> exit 22" 22 "$STATE_VADI" --compact --file "$INVALID" --role vadi

# S12 LENIENCY: valid baton with unknown/future status AND missing checkpoint ->
# exit 0, value-equal (status passthrough, checkpoint null). If Rust state used
# the strict enum here it would exit 22 — this case catches that.
LENIENT="$TMP_DIR/lenient-state.json"
cat >"$LENIENT" <<'JSON'
{
  "schema": "dvandva.baton.v2", "run_id": "lenient", "mode": "development",
  "status": "future_status_zz", "assignee": "vadi", "phase": 1,
  "work_split": [], "subagent_tracks": [], "verification_matrix": [],
  "findings": [], "blockers": [], "changed_paths": []
}
JSON
parity_state "S12 LENIENCY future status + no checkpoint -> exit 0 value-equal" 0 "$STATE_VADI" --compact --file "$LENIENT" --role vadi

# S13: phase-less work is retained (item.phase absent == root.phase absent).
PHASELESS="$TMP_DIR/phase-less.json"
cat >"$PHASELESS" <<'JSON'
{
  "schema": "dvandva.baton.v2", "run_id": "phase-less-run", "mode": "development",
  "run_mode": "walkaway", "status": "implementing", "assignee": "vadi", "checkpoint": 7,
  "work_split": [{"id": "phase-less-work", "chunk_type": "implementation", "owner_role": "vadi", "status": "ready", "paths": ["x"]}],
  "subagent_tracks": [], "verification_matrix": [], "findings": [], "blockers": [], "changed_paths": []
}
JSON
parity_state "S13 phase-less work retained" 0 "$STATE_VADI" --compact --file "$PHASELESS" --role vadi

# S14: large baton -> item cap (10 + more_count) and bounded ref/next_action.
LARGE="$TMP_DIR/large.json"
long="$(printf 'x%.0s' $(seq 1 1500))"
jq -n --arg long "$long" '{
  schema: "dvandva.baton.v2", run_id: "large-run", mode: "development",
  run_mode: "walkaway", phase: 1, status: "implementing", assignee: "vadi",
  active_roles: [], checkpoint: 9,
  refs: {huge: $long, branch: ("branch-" + $long), plan: "superpowers/plans/large-run.html"},
  research_ref: ("./superpowers/research/" + $long + ".html"),
  plan_ref: "./superpowers/plans/large-run.html",
  work_split: [range(0;15) as $i | {id: ("work-" + ($i|tostring)), phase: 1, chunk_type: "implementation", owner_role: "vadi", status: "ready", paths: ["a","b"], write_paths: ["a"], depends_on: ["root"]}],
  subagent_tracks: [], verification_matrix: [],
  findings: [range(0;15) as $i | {id: ("F-" + ($i|tostring)), severity: "low", status: "open", summary: $long}],
  blockers: [], changed_paths: [],
  verification_latest: {command: $long, result: "passed", notes: $long, extra: $long},
  next_action: $long
}' >"$LARGE"
parity_state "S14 large baton item-cap + bounded refs" 0 "$STATE_VADI" --compact --file "$LARGE" --role vadi

# S15: REAL reference snapshot (plugins/dvandva/references/baton-schema-v2.json).
if [[ -f "$REFERENCE_BATON" ]]; then
  parity_state "S15 reference baton snapshot (vadi)" 0 "$STATE_VADI" --compact --file "$REFERENCE_BATON" --role vadi
  parity_state "S16 reference baton snapshot (prativadi)" 0 "$STATE_PRATIVADI" --compact --file "$REFERENCE_BATON" --role prativadi
else
  fail "S15 reference baton missing at $REFERENCE_BATON"
fi

# S17: role=team / role=human accepted (state allows all four roles).
parity_state "S17 role=team accepted" 0 "$STATE_VADI" --compact --file "$FULL" --role team

# ---------------------------------------------------------------------------
# REGRESSION cases from cross-review (state). The original 55 cases missed these.
# F1/F2a are EXPECTED TO FAIL until prativadi's state.rs fix lands; the assertions
# are the CORRECT shell==rust parity bar (do NOT weaken them to make rust pass).
# ---------------------------------------------------------------------------

# F1 REGRESSION (expect FAIL): a work_split item whose PRIMARY alias keys are
# present-but-NULL (owner_role:null, chunk_type:null) with a non-null secondary
# alias (owner:"vadi", type:"implementation"). The shell uses jq `//`, which
# treats null as absent and FALLS THROUGH to the secondary alias — so the item
# is selected (owner "vadi" matches role, type is "implementation" under the
# parallel_implementing status filter) and surfaced in current_role_work with
# owner_role:"vadi" / chunk_type:"implementation". Rust currently keys on
# alias-PRESENCE (not null), so it drops the item -> current_role_work [].
# Assert rust state value-equal to the shell oracle.
F1_NULLALIAS="$TMP_DIR/f1-null-primary-alias.json"
cat >"$F1_NULLALIAS" <<'JSON'
{
  "schema": "dvandva.baton.v2", "run_id": "f1-null-alias", "mode": "development",
  "run_mode": "walkaway", "status": "parallel_implementing", "assignee": "team",
  "phase": 1, "checkpoint": 1, "active_roles": ["vadi", "prativadi"],
  "work_split": [{"id": "x", "owner_role": null, "owner": "vadi", "chunk_type": null, "type": "implementation", "phase": 1, "status": "planned"}],
  "subagent_tracks": [], "verification_matrix": [], "findings": [], "blockers": [], "changed_paths": []
}
JSON
parity_state "F1 REGRESSION work_split null primary alias -> // falls through (owner_role vadi / chunk_type implementation)" 0 "$STATE_VADI" --compact --file "$F1_NULLALIAS" --role vadi

# F2a REGRESSION (expect FAIL): top-level scalars set to boolean `false`
# (schema:false, profile:false, phase:false). The shell coalesces via `//`,
# which treats `false` as absent: schema -> null (`.schema // null`), profile ->
# "full" (`.profile // "full"` under development_mode), profile_floor -> "full",
# phase -> null (`.phase // null`). Rust currently LEAKS the literal `false`.
# Assert rust state value-equal to the shell oracle.
F2A_FALSE="$TMP_DIR/f2a-false-scalars.json"
cat >"$F2A_FALSE" <<'JSON'
{
  "schema": false, "run_id": "f2a-false-scalars", "mode": "development",
  "profile": false, "phase": false,
  "status": "implementing", "assignee": "vadi", "checkpoint": 1,
  "work_split": [], "subagent_tracks": [], "verification_matrix": [], "findings": [], "blockers": [], "changed_paths": []
}
JSON
parity_state "F2a REGRESSION false top-level scalars coalesce (schema null / profile full / phase null)" 0 "$STATE_VADI" --compact --file "$F2A_FALSE" --role vadi

echo
echo "=== SHIM-PATH parity (vm-shim-binary-path / vm-fallback) ==="

# V1 vm-fallback: with DVANDVA_BIN unset + no binary on PATH + no co-located
# binary, the shim runs the preserved SHELL implementation. (This is the shell
# side of every case above; here we assert it explicitly produces the shell
# projection.)
cases=$((cases + 1))
fb_out="$(env -u DVANDVA_BIN "$STATE_VADI" --compact --file "$FULL" --role vadi 2>/dev/null)"
if jq -e '.kind == "BATON_STATE_COMPACT"' <<<"$fb_out" >/dev/null 2>&1; then
  pass "V1 vm-fallback: shim without binary runs shell implementation"
else
  fail "V1 vm-fallback: shim did not produce shell projection (out: $fb_out)"
fi

# V2 vm-shim-binary-path: shim WITH DVANDVA_BIN must reproduce the binary run
# directly, BYTE-identical, for both state and resolve (proves the shim
# forwards subcommand + args + selectors without mangling).
cases=$((cases + 1))
shim_state="$(env DVANDVA_BIN="$BIN" "$STATE_VADI" --compact --file "$FULL" --role vadi 2>/dev/null)"
direct_state="$("$BIN" state --compact --file "$FULL" --role vadi 2>/dev/null)"
if [[ "$shim_state" == "$direct_state" ]]; then
  pass "V2a vm-shim-binary-path: state shim == binary direct (byte-identical)"
else
  fail "V2a vm-shim-binary-path: state shim != binary direct"
fi
cases=$((cases + 1))
BOXV="$TMP_DIR/box-v2-resolve"
seed_baton "$BOXV/.dvandva/runs/only/baton.json" only implementing vadi "2026-06-29T10:00:00Z"
shim_res="$(env DVANDVA_BIN="$BIN" "$RESOLVE_VADI" --role vadi --cwd "$BOXV" 2>/dev/null)"
direct_res="$("$BIN" resolve --role vadi --cwd "$BOXV" 2>/dev/null)"
if [[ "$shim_res" == "$direct_res" ]]; then
  pass "V2b vm-shim-binary-path: resolve shim == binary direct (byte-identical)"
else
  fail "V2b vm-shim-binary-path: resolve shim != binary direct (shim:[$shim_res] direct:[$direct_res])"
fi

# V3: BOTH role dirs (vadi + prativadi) delegate to the binary identically for
# identical inputs (role only affects resolve stderr, so stdout is identical).
cases=$((cases + 1))
v_res="$(env DVANDVA_BIN="$BIN" "$RESOLVE_VADI" --role vadi --cwd "$BOXV" 2>/dev/null)"
p_res="$(env DVANDVA_BIN="$BIN" "$RESOLVE_PRATIVADI" --role prativadi --cwd "$BOXV" 2>/dev/null)"
if [[ "$v_res" == "$p_res" && "$v_res" == "$direct_res" ]]; then
  pass "V3 both role dirs delegate to binary with identical stdout"
else
  fail "V3 role-dir delegation diverged (vadi:[$v_res] prativadi:[$p_res])"
fi

# V4: role derivation PRESERVED via a CO-LOCATED binary (the intended install
# layout: `dvandva` next to the shim under skills/<role>/scripts/). With no
# --role and no DVANDVA_BIN, the shim finds the co-located binary and exec's it;
# the binary derives role from argv0's parent-of-parent dir. This must match the
# shell fallback, which derives role from the same script directory.
FAKE="$TMP_DIR/fake-skills"
for role in vadi prativadi; do
  mkdir -p "$FAKE/skills/$role/scripts"
  cp "$ROOT_DIR/plugins/dvandva/skills/$role/scripts/dvandva-state.sh" "$FAKE/skills/$role/scripts/dvandva-state.sh"
  cp "$BIN" "$FAKE/skills/$role/scripts/dvandva"
done
for role in vadi prativadi; do
  cases=$((cases + 1))
  # Rust via co-located binary, NO --role: role derived from argv0 dir.
  colocated="$(env -u DVANDVA_BIN -u DVANDVA_ROLE "$FAKE/skills/$role/scripts/dvandva-state.sh" --compact --file "$FULL" 2>/dev/null)"
  # Shell fallback via the REAL shim (no co-located binary), NO --role: role
  # derived from the real script directory.
  shellrole="$(env -u DVANDVA_BIN -u DVANDVA_ROLE "$ROOT_DIR/plugins/dvandva/skills/$role/scripts/dvandva-state.sh" --compact --file "$FULL" 2>/dev/null)"
  co_role="$(jq -r '.role' <<<"$colocated" 2>/dev/null)"
  sh_role="$(jq -r '.role' <<<"$shellrole" 2>/dev/null)"
  if [[ "$co_role" == "$role" && "$sh_role" == "$role" ]] &&
    diff <(jq -S . <<<"$colocated" 2>/dev/null) <(jq -S . <<<"$shellrole" 2>/dev/null) >/dev/null 2>&1; then
    pass "V4 role derivation preserved (co-located binary, $role, no --role)"
  else
    fail "V4 role derivation diverged ($role): co-located role=[$co_role] shell role=[$sh_role]"
  fi
done

# CR-1 REGRESSION (should PASS — already fixed): `state` via the shim WITH
# DVANDVA_BIN set but NO --role and NO DVANDVA_ROLE. The shim must derive the
# role from its own parent-of-parent dir (skills/<role>/scripts) and EXPORT it
# before exec'ing the binary, so the delegated run emits the correct role —
# matching the shell fallback, which derives role from the same script dir.
# V4 covers the CO-LOCATED-binary path; the original 55 cases never exercised
# this DVANDVA_BIN path with a missing --role. Assert exit 0 + the LITERAL role
# value (not just shell==rust equality, which a mutual role=null bug would pass)
# + shell==rust value-equal, for both role dirs.
CR1_BATON="$TMP_DIR/cr1-role-derivation.json"
cat >"$CR1_BATON" <<'JSON'
{
  "schema": "dvandva.baton.v2", "run_id": "cr1-role", "mode": "development",
  "run_mode": "walkaway", "status": "implementing", "assignee": "vadi",
  "phase": 1, "checkpoint": 1, "active_roles": ["vadi"],
  "work_split": [], "subagent_tracks": [], "verification_matrix": [], "findings": [], "blockers": [], "changed_paths": []
}
JSON
for role in vadi prativadi; do
  cases=$((cases + 1))
  cr_script="$ROOT_DIR/plugins/dvandva/skills/$role/scripts/dvandva-state.sh"
  cr_d="$TMP_DIR/cr1-$role"
  mkdir -p "$cr_d"
  # Rust: DVANDVA_BIN set, no --role, no DVANDVA_ROLE -> shim derives+exports role.
  env -u DVANDVA_ROLE -u DVANDVA_BATON_FILE -u DVANDVA_RUN_DIR -u DVANDVA_RUN_ID \
    DVANDVA_BIN="$BIN" "$cr_script" --compact --file "$CR1_BATON" >"$cr_d/rs.out" 2>"$cr_d/rs.err"
  cr_rs_exit=$?
  # Shell fallback: no DVANDVA_BIN, no --role, no DVANDVA_ROLE -> derives role from dir.
  env -u DVANDVA_BIN -u DVANDVA_ROLE -u DVANDVA_BATON_FILE -u DVANDVA_RUN_DIR -u DVANDVA_RUN_ID \
    "$cr_script" --compact --file "$CR1_BATON" >"$cr_d/sh.out" 2>"$cr_d/sh.err"
  cr_sh_exit=$?
  cr_rs_role="$(jq -r '.role' "$cr_d/rs.out" 2>/dev/null)"
  cr_sh_role="$(jq -r '.role' "$cr_d/sh.out" 2>/dev/null)"
  if [[ "$cr_rs_exit" -eq 0 && "$cr_sh_exit" -eq 0 &&
    "$cr_rs_role" == "$role" && "$cr_sh_role" == "$role" ]] &&
    diff <(jq -S . "$cr_d/sh.out" 2>/dev/null) <(jq -S . "$cr_d/rs.out" 2>/dev/null) >/dev/null 2>&1; then
    pass "CR-1 state shim no --role derives+exports role=$role (DVANDVA_BIN path) — exit0 + role literal + value-equal"
  else
    fail "CR-1 state shim no --role ($role): rust-exit=$cr_rs_exit rust-role=[$cr_rs_role] shell-exit=$cr_sh_exit shell-role=[$cr_sh_role]"
  fi
done

echo
echo "=== SUMMARY ==="
echo "cases run: $cases  failures: $failures"
if [[ "$failures" -gt 0 ]]; then
  echo "RESULT: PARITY FAILURES DETECTED ($failures) — see FAIL lines above."
  exit 1
fi
echo "RESULT: ALL PARITY CASES PASSED"
exit 0
