#!/usr/bin/env python3
"""Validate Freeflow checkpoint policy against a verified role snapshot."""

import json
from pathlib import Path
import re
import subprocess
import sys


def reject(message):
    print(json.dumps({"error": "invalid_checkpoint", "message": message}, separators=(",", ":")))
    raise SystemExit(1)


def git_type(worktree, value):
    result = subprocess.run(
        ["git", "-C", worktree, "cat-file", "-t", value],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.stdout.strip() if result.returncode == 0 else None


def metadata_marks_user_only(root):
    try:
        skill_root = Path(root).resolve(strict=True)
        if not skill_root.is_dir():
            return False
        skill_md = (skill_root / "SKILL.md").read_text(encoding="utf-8")
        openai_yaml = (skill_root / "agents" / "openai.yaml").read_text(encoding="utf-8")
    except (OSError, UnicodeError):
        return False
    if len(skill_md) > 65536 or len(openai_yaml) > 65536:
        return False
    return bool(
        re.search(r"^disable-model-invocation:\s*true\s*$", skill_md, re.MULTILINE)
        or re.search(r"^\s*allow_implicit_invocation:\s*false\s*$", openai_yaml, re.MULTILINE)
    )


def main():
    if len(sys.argv) != 3:
        raise SystemExit(2)
    try:
        snapshot = json.load(sys.stdin)
        with open(sys.argv[1], encoding="utf-8") as source:
            action = json.load(source)
        expected_revision = int(sys.argv[2])
        refs = snapshot["objective"]["refs"]
    except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError):
        return
    if snapshot.get("revision") != expected_revision:
        return
    if not isinstance(action, dict) or action.get("type") != "submit_checkpoint":
        return
    freeflow = any(
        isinstance(ref, dict)
        and str(ref.get("kind", "")).casefold() == "workflow"
        and str(ref.get("value", "")).casefold() == "freeflow"
        for ref in refs
    )
    if not freeflow:
        return
    delivery_kinds = [
        str(ref.get("value", "")).casefold()
        for ref in refs
        if isinstance(ref, dict)
        and str(ref.get("kind", "")).casefold() == "delivery_kind"
    ]
    if len(delivery_kinds) != 1 or delivery_kinds[0] not in {"code", "analysis"}:
        reject("Freeflow delivery_kind must be exactly one of code or analysis")
    required_skills = [
        str(ref.get("value", ""))
        for ref in refs
        if isinstance(ref, dict)
        and str(ref.get("kind", "")).casefold() == "required_user_skill"
    ]
    invoked_skills = {
        str(Path(str(ref.get("value", ""))).resolve())
        for ref in refs
        if isinstance(ref, dict)
        and str(ref.get("kind", "")).casefold() == "invoked_user_skill"
    }
    if len(set(required_skills)) != len(required_skills):
        reject("required user-only skill references must be unique")
    for root in required_skills:
        if not metadata_marks_user_only(root):
            reject("required user-only skill metadata is missing, unreadable, or model-invocable")
        if str(Path(root).resolve()) not in invoked_skills:
            reject("required user-only skill has not been explicitly invoked")
    checkpoint = action.get("checkpoint")
    checkpoint_kind = checkpoint.get("kind") if isinstance(checkpoint, dict) else None
    if delivery_kinds[0] == "code" and checkpoint_kind == "analysis":
        reject("code-carrying delivery requires a git checkpoint; naming a commit in an analysis artifact is insufficient")
    if delivery_kinds[0] == "analysis" and checkpoint_kind == "git":
        reject("analysis-only delivery requires an analysis checkpoint")
    if checkpoint_kind != "git":
        return
    workspace = snapshot.get("workspace")
    worktree = workspace.get("worktree") if isinstance(workspace, dict) else None
    identity = checkpoint.get("identity")
    deliverables = checkpoint.get("deliverables")
    if not isinstance(worktree, str) or not worktree:
        reject("git checkpoint requires a verified workspace")
    if not isinstance(identity, str) or not isinstance(deliverables, list):
        return
    artifacts = []
    for deliverable in deliverables:
        incoming = deliverable.get("artifacts") if isinstance(deliverable, dict) else None
        if isinstance(incoming, list):
            artifacts.extend(item for item in incoming if isinstance(item, dict))
    commits = [item.get("value") for item in artifacts if item.get("kind") == "commit"]
    if any(value != identity for value in commits):
        reject("git checkpoint commit artifacts must match checkpoint identity")
    objects = [(identity, "commit")] + [
        (item.get("value"), item.get("kind"))
        for item in artifacts
        if item.get("kind") in {"commit", "tree", "blob"}
    ]
    for value, expected_type in objects:
        if isinstance(value, str) and git_type(worktree, value) != expected_type:
            reject("git checkpoint object is unavailable or has the wrong type in the verified workspace")


if __name__ == "__main__":
    main()
