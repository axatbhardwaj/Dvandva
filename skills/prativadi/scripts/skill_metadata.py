"""Bounded, fail-closed parsing for user-only skill invocation metadata."""

import json
from pathlib import Path
import re


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
    value = value.strip()
    if not value or value[0] not in {"'", '"'}:
        comment = re.search(r"\s+#", value)
        return value[: comment.start()].rstrip() if comment else value
    quote = value[0]
    index = 1
    while index < len(value):
        if quote == '"' and value[index] == "\\":
            index += 2
            continue
        if quote == "'" and value[index : index + 2] == "''":
            index += 2
            continue
        if value[index] == quote:
            remainder = value[index + 1 :].strip()
            return value[: index + 1] if not remainder or remainder.startswith("#") else None
        index += 1
    return None


def parse_scalar(value):
    """Accept a deliberately small, complete YAML scalar subset."""
    if not value or value[0] in "[{}]|>" or value[-1:] in "]}":
        return None
    if value[0] == '"':
        try:
            parsed = json.loads(value)
        except json.JSONDecodeError:
            return None
        return parsed if isinstance(parsed, str) else None
    if value[0] == "'":
        if not re.fullmatch(r"'(?:[^']|'')*'", value):
            return None
        return value[1:-1].replace("''", "'")
    if re.search(r":(?:\s|$)", value) or value[0] in "-?:,!&*#%@`":
        return None
    if value.casefold() == "true":
        return True
    if value.casefold() == "false":
        return False
    return value


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
        scalar = parse_scalar(value)
        if scalar is None:
            return None
        parents = list(path[:-1])
        values[path] = scalar.casefold() if isinstance(scalar, str) else scalar
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
    return None if values is None else values.get(("disable-model-invocation",)) is True


def policy_disables_implicit_invocation(text):
    values = parse_mapping(text.splitlines())
    return None if values is None else values.get(("policy", "allow_implicit_invocation")) is False


def marks_user_only(root):
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
    openai_policy = policy_disables_implicit_invocation(openai_yaml) if isinstance(openai_yaml, str) else False
    if skill_policy is None or openai_policy is None:
        return False
    return skill_policy or openai_policy
