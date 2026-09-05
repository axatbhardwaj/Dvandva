"""Shared fail-closed helpers for public role-facade policy guards."""

import json
import re
import sys


REVIEW_MEMBER = re.compile(
    r"https://github\.com/((?i:[A-Z0-9](?:[A-Z0-9-]{0,37}[A-Z0-9])?))/"
    r"((?i:[A-Z0-9_.-]{1,100}))/pull/([1-9][0-9]*)",
)


class CandidateError(ValueError):
    """A malformed registry candidate that discovery may safely skip."""


def parse_review_member(value):
    match = REVIEW_MEMBER.fullmatch(value) if isinstance(value, str) else None
    if match is None:
        return None
    return match.group(1).casefold(), match.group(2).casefold(), int(match.group(3))


def validate_review_members(repository_id, members):
    parsed = [parse_review_member(member) for member in members]
    if any(member is None for member in parsed):
        raise CandidateError("candidate has a non-canonical or cross-repository review member")
    repositories = {(owner, repository) for owner, repository, _ in parsed}
    if len(repositories) != 1:
        raise CandidateError("candidate has a non-canonical or cross-repository review member")
    expected = "github.com/" + "/".join(next(iter(repositories)))
    if not isinstance(repository_id, str) or repository_id.casefold() != expected:
        raise CandidateError("candidate has a non-canonical or cross-repository review member")
    return parsed


def rejection(error):
    def reject(message):
        print(json.dumps({"error": error, "message": message}, separators=(",", ":")))
        raise SystemExit(1)

    return reject


def load_action(action_path, expected_revision, action_type):
    try:
        snapshot = json.load(sys.stdin)
        with open(action_path, encoding="utf-8") as source:
            action = json.load(source)
        expected = int(expected_revision)
    except (OSError, json.JSONDecodeError, TypeError, ValueError):
        return None
    if (
        not isinstance(snapshot, dict)
        or snapshot.get("revision") != expected
        or not isinstance(action, dict)
        or action.get("type") != action_type
    ):
        return None
    return snapshot, action


def ref_values(snapshot, kind):
    objective = snapshot.get("objective")
    refs = objective.get("refs", []) if isinstance(objective, dict) else []
    return [
        str(ref.get("value", ""))
        for ref in refs
        if isinstance(ref, dict) and str(ref.get("kind", "")).casefold() == kind.casefold()
    ]


def validate_review_start(members):
    try:
        identity = json.load(sys.stdin)
        repository_id = identity.get("repository_id")
        parsed = [parse_review_member(member) for member in members]
    except (AttributeError, json.JSONDecodeError, TypeError):
        parsed = []
        repository_id = None
    if any(member is None for member in parsed) or len({member[:2] for member in parsed}) != 1:
        print("dvandva-role: Review members must belong to one canonical repository", file=sys.stderr)
        raise SystemExit(2)
    expected = "github.com/" + "/".join(next(iter({member[:2] for member in parsed})))
    if not isinstance(repository_id, str) or repository_id.casefold() != expected:
        print("dvandva-role: Review members must match the canonical workspace repository", file=sys.stderr)
        raise SystemExit(2)


if __name__ == "__main__":
    if len(sys.argv) < 3 or sys.argv[1] != "review-start":
        raise SystemExit(2)
    validate_review_start(sys.argv[2:])
