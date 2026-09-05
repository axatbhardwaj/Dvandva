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


def bounded_text(path, required):
    try:
        with path.open("rb") as source:
            raw = source.read(65537)
    except FileNotFoundError:
        return None if not required else False
    except OSError:
        return False
    if len(raw) > 65536:
        return False
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return False


def strip_yaml_comment(value):
    quote = None
    escaped = False
    for index, character in enumerate(value):
        if quote == '"' and character == "\\" and not escaped:
            escaped = True
            continue
        if character in {"'", '"'} and not escaped:
            if quote is None:
                quote = character
            elif quote == character:
                quote = None
        if character == "#" and quote is None and (index == 0 or value[index - 1].isspace()):
            return value[:index].rstrip()
        escaped = False
    return None if quote else value.strip()


def parse_mapping(lines):
    """Parse the nested scalar-map subset used by skill metadata."""
    values = {}
    seen = set()
    parents = []
    for raw in lines:
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        if "\t" in raw:
            return None
        indent = len(raw) - len(raw.lstrip(" "))
        if indent % 2 or indent // 2 > len(parents):
            return None
        match = re.fullmatch(r" *([A-Za-z0-9_-]+):\s*(.*)", raw)
        if not match:
            return None
        level = indent // 2
        path = tuple(parents[:level] + [match.group(1)])
        if path in seen:
            return None
        seen.add(path)
        value = strip_yaml_comment(match.group(2))
        if value is None:
            return None
        if not value:
            parents = list(path)
            continue
        if value[0] in "[{|>" or value[-1:] in "]}":
            return None
        if value[0] in {"'", '"'} and value[-1:] != value[0]:
            return None
        parents = list(path[:-1])
        values[path] = value.strip("'\"").casefold()
    return values


def frontmatter_disables_model_invocation(text):
    lines = text.splitlines()
    if not lines or lines[0] != "---":
        return False
    try:
        end = lines[1:].index("---") + 1
    except ValueError:
        return None
    values = parse_mapping(lines[1:end])
    return None if values is None else values.get(("disable-model-invocation",)) == "true"


def policy_disables_implicit_invocation(text):
    values = parse_mapping(text.splitlines())
    return None if values is None else values.get(("policy", "allow_implicit_invocation")) == "false"


def metadata_marks_user_only(root):
    try:
        skill_root = Path(root).resolve(strict=True)
        if not skill_root.is_dir():
            return False
    except OSError:
        return False
    skill_md = bounded_text(skill_root / "SKILL.md", required=True)
    openai_yaml = bounded_text(skill_root / "agents" / "openai.yaml", required=False)
    if not isinstance(skill_md, str) or openai_yaml is False:
        return False
    skill_policy = frontmatter_disables_model_invocation(skill_md)
    openai_policy = (
        policy_disables_implicit_invocation(openai_yaml)
        if isinstance(openai_yaml, str)
        else False
    )
    if skill_policy is None or openai_policy is None:
        return False
    return skill_policy or openai_policy


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
