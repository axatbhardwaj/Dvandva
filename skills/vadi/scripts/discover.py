#!/usr/bin/env python3
"""Read-only registry lookup. Selection and exact claims remain separate."""
import argparse
import json
import os
import subprocess
import sys
import time

sys.dont_write_bytecode = True

from role_guard import CandidateError, ref_values, validate_review_members


def kernel_json(binary, *args):
    result = subprocess.run([binary, *args], text=True, capture_output=True)
    if result.returncode:
        sys.stdout.write(result.stdout)
        sys.stderr.write(result.stderr)
        raise SystemExit(result.returncode)
    value = json.loads(result.stdout)
    if not isinstance(value, dict):
        raise ValueError("kernel response must be an object")
    return value


def workflow(value):
    # Persistent Review must not adopt legacy one-shot pr_review runs.
    value = value.casefold()
    return "babysitting" if value == "babysit" else value


def filter_candidates(result, args):
    candidates = result["candidates"]
    if not isinstance(candidates, list):
        raise ValueError("kernel candidates must be an array")
    selected = []
    match_bases = []
    invalid_candidates = []
    for candidate in candidates:
        try:
            peer_key = "worker_harness" if args.role == "reviewer" else "reviewer_harness"
            if candidate[peer_key].casefold() != args.peer.casefold():
                continue
            refs = candidate["objective"]["refs"]
            if not isinstance(refs, list):
                raise CandidateError("candidate objective references are not an array")
            workflows = [workflow(value) for value in ref_values(candidate, "workflow")]
            if len(workflows) > 1:
                raise ValueError("candidate has ambiguous workflow references")
            actual = workflows[0] if workflows else "implementation"
            if actual != workflow(args.workflow):
                continue
            match_basis = None
            if actual == "review":
                members = ref_values(candidate, "review_member")
                member_match = (
                    args.task_reference is not None
                    and any(member.casefold() == args.task_reference.casefold() for member in members)
                )
                if (args.task_reference and candidate["task_reference"] != args.task_reference
                        and not member_match):
                    continue
                if len(members) != len({member.casefold() for member in members}):
                    raise CandidateError("candidate has duplicate review_member references")
                if members:
                    validate_review_members(args.repository_id, members)
            else:
                members = []
                member_match = False
            if args.task_reference:
                if candidate["task_reference"] == args.task_reference:
                    match_basis = "task_reference"
                elif actual == "review" and member_match:
                    match_basis = "review_member"
                else:
                    continue
            if args.objective and candidate["objective"]["summary"] != args.objective:
                continue
            selected.append(candidate)
            match_bases.append(match_basis)
        except (AttributeError, CandidateError, KeyError, TypeError) as error:
            invalid_candidates.append({
                "run_id": candidate.get("run_id") if isinstance(candidate, dict) else None,
                "error": str(error),
            })
            continue
    result["candidates"] = selected
    if invalid_candidates:
        result["invalid_candidates"] = invalid_candidates
    if result["outcome"] != "corrupt":
        if not selected:
            result["outcome"] = "none"
        elif len(selected) > 1:
            result["outcome"] = "ambiguous"
        elif selected[0].get("migration"):
            result["outcome"] = "upgrade_required"
        elif selected[0]["claim_state"] == "busy":
            result["outcome"] = "busy"
        else:
            result["outcome"] = "match"
            if match_bases[0] is not None:
                result["match_basis"] = match_bases[0]
    result["read_only"] = True
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary")
    parser.add_argument("runs_dir")
    parser.add_argument("role", choices=["worker", "reviewer"])
    parser.add_argument("session")
    parser.add_argument("harness")
    parser.add_argument("peer")
    parser.add_argument("workspace")
    parser.add_argument(
        "--workflow", required=True,
        choices=["discovery", "implementation", "babysitting", "review",
                 "freeflow", "babysit", "pr_review"],
    )
    parser.add_argument("--task-reference")
    parser.add_argument("--objective", help="Optional exact canonical objective, never a fuzzy query")
    parser.add_argument("--wait", action="store_true")
    args = parser.parse_args()
    for value in [args.session, args.harness, args.peer, args.task_reference, args.objective]:
        if value is not None and (not value.strip() or value != value.strip()):
            parser.error("identity values must be nonblank and have no surrounding whitespace")
    if args.harness.casefold() == args.peer.casefold():
        parser.error("participant harnesses must be distinct")
    timeout = int(os.environ.get("DVANDVA_DISCOVER_TIMEOUT_MS", "60000"))
    interval = int(os.environ.get("DVANDVA_DISCOVER_INTERVAL_MS", "1000"))
    if not 1 <= timeout <= 60000 or not 1 <= interval <= 60000:
        parser.error("discovery timeout and interval must be between 1 and 60000 ms")
    identity = kernel_json(args.binary, "identify", "--workspace", args.workspace)
    args.repository_id = identity["repository_id"]
    deadline = time.monotonic() + timeout / 1000
    while True:
        result = kernel_json(args.binary, "discover", "--read-only", "--runs-dir", args.runs_dir,
                             "--repository-id", identity["repository_id"],
                             "--harness", args.harness, "--role", args.role,
                             "--session-id", args.session, "--stale-after-days", "14")
        result = filter_candidates(result, args)
        remaining = deadline - time.monotonic()
        if not args.wait or result["outcome"] != "none" or remaining <= 0:
            print(json.dumps(result, indent=2))
            return
        time.sleep(min(interval / 1000, remaining))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError) as error:
        print(json.dumps({"error": "invalid_discovery_response", "message": str(error)}))
        raise SystemExit(1)
