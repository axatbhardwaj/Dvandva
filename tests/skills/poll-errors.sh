#!/usr/bin/env bash
# A failed kernel wait must retain its error, never masquerade as cancellation.
set -euo pipefail
export PYTHONOPTIMIZE=1
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
test_root="$(mktemp -d)"
trap 'rm -rf -- "$test_root"' EXIT
cargo build --quiet --locked --manifest-path "$repo_root/v4/Cargo.toml"
export XDG_DATA_HOME="$test_root/data" XDG_STATE_HOME="$test_root/state"
export POLL_TEST_KERNEL="$repo_root/v4/target/debug/dvandva-v4"
binary="$XDG_DATA_HOME/dvandva/bin/0.3.8/dvandva-kernel"
mkdir -p "$(dirname "$binary")"
# Only the wait is fault-injected. The real binary supplies the pinned probe.
cat >"$binary" <<'KERNEL'
#!/usr/bin/env bash
if test "${1:-}" = role && test "${2:-}" = wait; then
  case "$POLL_TEST_MODE" in
    error) printf '{"error":"claim_fenced"}\n'; exit 7 ;;
    empty) exit 0 ;;
    malformed) printf 'not JSON\n' ;;
    unknown) printf '{"wait_outcome":"surprise","revision":2}\n' ;;
    no_revision) printf '{"wait_outcome":"idle_timeout"}\n' ;;
    signal) exit 130 ;;
  esac
  exit 0
fi
exec "$POLL_TEST_KERNEL" "$@"
KERNEL
chmod +x "$binary"
for role in vadi prativadi; do
  facade="$repo_root/skills/$role/scripts/dvandva-role.sh"
  for mode in error empty malformed unknown no_revision signal; do
    status=0
    output="$(POLL_TEST_MODE="$mode" bash "$facade" poll session "$test_root/run" 0 1000 2>"$test_root/stderr")" || status=$?
    case "$mode" in
      error)
        test "$status" = 7
        test "$output" = '{"error":"claim_fenced"}' || {
          printf '%s discarded the kernel error payload\n' "$role" >&2; exit 1;
        } ;;
      signal) test "$status" = 130 ;;
      *)
        test "$status" != 0 || { printf '%s accepted %s response\n' "$role" "$mode" >&2; exit 1; }
        test "$output" = '{"error":"invalid_poll_response"}' ;;
    esac
  done
done
printf 'poll errors: ok\n'
