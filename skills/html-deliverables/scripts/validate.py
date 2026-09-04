#!/usr/bin/env python3
"""Validate the static contract for a Dvandva HTML deliverable."""

import datetime as dt
import json
import re
import sys
from html.parser import HTMLParser
from pathlib import Path


TOKENS = {
    "ground": "#0b0f14", "panel": "#121821", "panel2": "#182130",
    "line": "#26303e", "ink": "#dce4ee", "dim": "#8a97a8",
    "faint": "#5c6774", "vadi": "#34d399", "prat": "#a78bfa",
    "team": "#5ca9ff", "human": "#e0a63d", "seal": "#46c26a",
    "stop": "#ff6a5e",
}


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate metadata key: {key}")
        result[key] = value
    return result


class ContractParser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.doctype = False
        self.title_depth = 0
        self.title = []
        self.meta_depth = 0
        self.meta_blocks = []
        self.figures = []
        self.figure = None
        self.foot_depth = 0
        self.foot_text = []

    def handle_decl(self, decl):
        self.doctype |= decl.lower() == "doctype html"

    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        classes = set(values.get("class", "").split())
        if tag == "title":
            self.title_depth += 1
        if tag == "script" and values.get("id") == "dvandva-artifact-meta":
            if values.get("type", "").lower() != "application/json":
                self.meta_blocks.append(None)
            else:
                self.meta_blocks.append([])
            self.meta_depth += 1
        if tag == "figure":
            self.figure = {"svg": False, "caption_depth": 0, "caption": []}
        elif self.figure is not None and tag == "svg":
            self.figure["svg"] = True
        elif self.figure is not None and tag == "figcaption":
            self.figure["caption_depth"] += 1
        if "foot" in classes:
            self.foot_depth += 1

    def handle_endtag(self, tag):
        if tag == "title" and self.title_depth:
            self.title_depth -= 1
        if tag == "script" and self.meta_depth:
            self.meta_depth -= 1
        if tag == "figcaption" and self.figure is not None:
            self.figure["caption_depth"] -= 1
        if tag == "figure" and self.figure is not None:
            self.figures.append(self.figure)
            self.figure = None
        if self.foot_depth and tag in {"p", "div", "footer"}:
            self.foot_depth -= 1

    def handle_data(self, data):
        if self.title_depth:
            self.title.append(data)
        if self.meta_depth and self.meta_blocks and self.meta_blocks[-1] is not None:
            self.meta_blocks[-1].append(data)
        if self.figure is not None and self.figure["caption_depth"]:
            self.figure["caption"].append(data)
        if self.foot_depth:
            self.foot_text.append(data)


def validate(path):
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        return [f"cannot read UTF-8 HTML: {error}"]

    parser = ContractParser()
    try:
        parser.feed(text)
    except Exception as error:
        return [f"cannot parse HTML: {error}"]

    errors = []
    if not parser.doctype:
        errors.append("missing HTML5 doctype")
    if not "".join(parser.title).strip():
        errors.append("missing non-empty title")
    if len(parser.meta_blocks) != 1 or parser.meta_blocks[0] is None:
        errors.append("expected one application/json metadata block")
    else:
        try:
            meta = json.loads("".join(parser.meta_blocks[0]), object_pairs_hook=unique_object)
            if type(meta) is not dict:
                raise ValueError("metadata must be a JSON object")
            for field in ("schema", "artifact_type", "title", "date", "basis"):
                if not isinstance(meta.get(field), str) or not meta[field].strip():
                    errors.append(f"metadata {field} must be a non-empty string")
            for field in ("schema", "title", "basis"):
                value = meta.get(field, "")
                if isinstance(value, str) and ("<!--" in value or "-->" in value):
                    errors.append(f"metadata {field} contains an unreplaced placeholder")
            kind_value = meta.get("artifact_type", "")
            kind = kind_value if isinstance(kind_value, str) else ""
            if not re.fullmatch(r"[a-z][a-z0-9_]*", kind):
                errors.append("metadata artifact_type must be a lowercase identifier")
            if meta.get("schema") != f"dvandva.artifact.{kind}.v1":
                errors.append("metadata schema must match artifact_type")
            meta_title = meta.get("title", "")
            if not isinstance(meta_title, str) or meta_title.strip() != "".join(parser.title).strip():
                errors.append("metadata title must match the HTML title")
            date_value = meta.get("date", "")
            try:
                if not isinstance(date_value, str) or not re.fullmatch(r"\d{4}-\d{2}-\d{2}", date_value):
                    raise ValueError
                dt.date.fromisoformat(date_value)
            except ValueError:
                errors.append("metadata date must use YYYY-MM-DD")
        except (json.JSONDecodeError, ValueError) as error:
            errors.append(f"invalid metadata JSON: {error}")

    if "color-scheme: dark;" not in text:
        errors.append("missing literal color-scheme: dark;")
    for name, value in TOKENS.items():
        if not re.search(rf"--{name}\s*:\s*{re.escape(value)}\s*;", text, re.I):
            errors.append(f"missing house token --{name}:{value}")
    if not re.search(r"figure\s*\{[^}]*overflow-x\s*:\s*auto", text, re.I | re.S):
        errors.append("figure must own horizontal overflow")
    if not re.search(r"svg\s*\{[^}]*min-width\s*:", text, re.I | re.S):
        errors.append("SVG needs a minimum width")
    if not re.search(r"@media\s*\(prefers-reduced-motion:\s*reduce\)", text, re.I):
        errors.append("missing reduced-motion media rule")
    if not parser.figures:
        errors.append("at least one figure is required")
    for figure in parser.figures:
        if not figure["svg"]:
            errors.append("every figure needs an inline SVG")
        if not "".join(figure["caption"]).strip():
            errors.append("every figure needs a non-empty figcaption")
    if not "".join(parser.foot_text).strip():
        errors.append("missing non-empty .foot stamp")
    return errors


def main():
    if len(sys.argv) != 2:
        print("usage: validate.py ARTIFACT.html", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    errors = validate(path)
    if errors:
        for error in errors:
            print(f"html-deliverable: {error}", file=sys.stderr)
        return 1
    print(f"html-deliverable: valid {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
