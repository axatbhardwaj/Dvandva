"""Shared fail-closed helpers for public role-facade policy guards."""

import json
import sys


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
