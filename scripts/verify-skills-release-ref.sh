#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"
remote="${2:-origin}"
event_sha="${GITHUB_SHA:-}"

invalid() {
  printf 'verify-skills-release-ref: release_ref_invalid reason=%s\n' "$1" >&2
  exit 1
}

if test "$#" -gt 2 || ! [[ "$tag" =~ ^skills-v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  invalid malformed_tag
fi
if test -z "$remote"; then
  invalid missing_remote_name
fi

capture_root="$(mktemp -d "${TMPDIR:-/tmp}/dvandva-release-ref.XXXXXX")"
cleanup() {
  rm -rf -- "$capture_root"
}
trap cleanup EXIT

ref="refs/tags/$tag"
set +e
git show-ref --verify --hash "$ref" >"$capture_root/local-object" 2>/dev/null
local_object_status=$?
git rev-parse --verify "$ref^{}" >"$capture_root/local-peeled" 2>/dev/null
local_peeled_status=$?
git ls-remote --tags "$remote" "$ref" "$ref^{}" \
  >"$capture_root/remote-refs" 2>/dev/null
remote_status=$?
set -e

test "$local_object_status" -eq 0 || invalid missing_local_object
test "$local_peeled_status" -eq 0 || invalid missing_local_peeled_commit
test "$remote_status" -eq 0 || invalid remote_lookup_failed

python3 - \
  "$tag" "$event_sha" \
  "$capture_root/local-object" \
  "$capture_root/local-peeled" \
  "$capture_root/remote-refs" <<'PY' || invalid identity_mismatch
import re
import sys
from pathlib import Path


tag, event_sha = sys.argv[1:3]
paths = [Path(value) for value in sys.argv[3:]]
hex_object = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})\Z")


def text(path):
    raw = path.read_bytes()
    if len(raw) > 8192 or b"\0" in raw:
        raise ValueError("invalid capture")
    return raw.decode("utf-8", errors="strict")


def one_object(path):
    lines = text(path).splitlines()
    if len(lines) != 1 or not hex_object.fullmatch(lines[0]):
        raise ValueError("invalid object")
    return lines[0]


local_object = one_object(paths[0])
local_peeled = one_object(paths[1])
if not hex_object.fullmatch(event_sha):
    raise ValueError("invalid event object")

base_ref = f"refs/tags/{tag}"
peeled_ref = f"{base_ref}^{{}}"
remote = {}
for line in text(paths[2]).splitlines():
    fields = line.split("\t")
    if len(fields) != 2 or fields[1] not in (base_ref, peeled_ref):
        raise ValueError("invalid remote row")
    object_id, name = fields
    if not hex_object.fullmatch(object_id) or name in remote:
        raise ValueError("ambiguous remote row")
    remote[name] = object_id

if remote.get(base_ref) != local_object:
    raise ValueError("remote tag object moved")
if local_object == local_peeled:
    if set(remote) != {base_ref}:
        raise ValueError("lightweight tag has unexpected peeled ref")
else:
    if set(remote) != {base_ref, peeled_ref}:
        raise ValueError("annotated tag is missing peeled ref")
    if remote[peeled_ref] != local_peeled:
        raise ValueError("remote peeled commit moved")
if event_sha != local_peeled:
    raise ValueError("event commit moved")
PY

printf 'verify-skills-release-ref: verified tag=%s commit=%s\n' \
  "$tag" "$event_sha"
