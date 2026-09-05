#!/usr/bin/env python3
"""Validate Freeflow checkpoint policy against a verified role snapshot."""

from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True

from role_guard import load_action, ref_values, rejection
from skill_metadata import marks_user_only


reject = rejection("invalid_checkpoint")


def git_type(worktree, value):
    result = subprocess.run(
        ["git", "-C", worktree, "cat-file", "-t", value],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.stdout.strip() if result.returncode == 0 else None


def main():
    if len(sys.argv) != 3:
        raise SystemExit(2)
    context = load_action(sys.argv[1], sys.argv[2], "submit_checkpoint")
    if context is None:
        return
    snapshot, action = context
    freeflow = any(value.casefold() == "freeflow" for value in ref_values(snapshot, "workflow"))
    if not freeflow:
        return
    delivery_kinds = [value.casefold() for value in ref_values(snapshot, "delivery_kind")]
    if len(delivery_kinds) != 1 or delivery_kinds[0] not in {"code", "analysis"}:
        reject("Freeflow delivery_kind must be exactly one of code or analysis")
    required_skills = ref_values(snapshot, "required_user_skill")
    invoked_skills = {
        str(Path(value).resolve()) for value in ref_values(snapshot, "invoked_user_skill")
    }
    if len(set(required_skills)) != len(required_skills):
        reject("required user-only skill references must be unique")
    for root in required_skills:
        if not marks_user_only(root):
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
