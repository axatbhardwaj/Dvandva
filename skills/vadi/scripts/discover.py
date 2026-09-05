#!/usr/bin/env python3
"""Read-only registry lookup. Selection and exact claims remain separate."""
import argparse
import json
import os
import re
import subprocess
import sys
import time


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
    return "babysitting" if value == "babysit" else value


def filter_candidates(result, args):
    candidates = result["candidates"]
    if not isinstance(candidates, list):
        raise ValueError("kernel candidates must be an array")
    selected = []
    match_bases = []
    for candidate in candidates:
        peer_key = "worker_harness" if args.role == "reviewer" else "reviewer_harness"
        if candidate[peer_key].casefold() != args.peer.casefold():
            continue
        refs = candidate["objective"]["refs"]
        workflows = [workflow(ref["value"]) for ref in refs if ref["kind"] == "workflow"]
        if len(workflows) > 1:
            raise ValueError("candidate has ambiguous workflow references")
        actual = workflows[0] if workflows else "implementation"
        if actual != workflow(args.workflow):
            continue
        match_basis = None
        if actual == "review":
            members = [ref["value"] for ref in refs if ref["kind"] == "review_member"]
            if (args.task_reference and candidate["task_reference"] != args.task_reference
                    and args.task_reference not in members):
                continue
            if len(members) != len({member.casefold() for member in members}):
                raise ValueError("candidate has duplicate review_member references")
            if members and not args.repository_id.casefold().startswith("github.com/"):
                raise ValueError("review members require a canonical GitHub repository")
            member_pattern = re.compile(
                rf"^https://{re.escape(args.repository_id)}/pull/[1-9][0-9]*$",
                re.IGNORECASE,
            )
            if any(not member_pattern.fullmatch(member) for member in members):
                raise ValueError("candidate has a non-canonical or cross-repository review member")
        else:
            members = []
        if args.task_reference:
            if candidate["task_reference"] == args.task_reference:
                match_basis = "task_reference"
            elif actual == "review" and args.task_reference in members:
                match_basis = "review_member"
            else:
                continue
        if args.objective and candidate["objective"]["summary"] != args.objective:
            continue
        selected.append(candidate)
        match_bases.append(match_basis)
    result["candidates"] = selected
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
