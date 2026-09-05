#!/usr/bin/env python3
"""Gate persistent Review finalization on materialized current evidence."""

import hashlib
import json
from datetime import datetime
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

from role_guard import load_action, ref_values, rejection


DIGEST = re.compile(r"[0-9a-f]{64}")
MEMBER = re.compile(
    r"https://github\.com/([A-Z0-9](?:[A-Z0-9-]{0,37}[A-Z0-9])?)/([A-Z0-9_.-]{1,100})/pull/([1-9][0-9]*)",
    re.I,
)
FULL_REVISION = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", re.I)


reject = rejection("review_not_ready")


def load_context(action_path, expected_revision):
    context = load_action(action_path, expected_revision, "finalize")
    if context is None:
        return None
    snapshot, _action = context
    if "review" not in [value.casefold() for value in ref_values(snapshot, "workflow")]:
        return None
    members = ref_values(snapshot, "review_member")
    # All member-less Review runs predate persistent member scope and retain
    # their exact kernel-only finalization behavior, regardless of task shape.
    if not members:
        return None
    return snapshot, members


def checkpoint_digests(snapshot):
    checkpoint = snapshot.get("checkpoint")
    if not isinstance(checkpoint, dict) or checkpoint.get("kind") != "analysis":
        return []
    digests = []
    for deliverable in checkpoint.get("deliverables", []):
        if not isinstance(deliverable, dict):
            continue
        for artifact in deliverable.get("artifacts", []):
            if isinstance(artifact, dict) and artifact.get("kind") == "analysis_digest":
                value = artifact.get("value")
                if isinstance(value, str) and DIGEST.fullmatch(value):
                    digests.append(value)
    return digests


def member_number(url):
    match = MEMBER.fullmatch(url)
    return int(match.group(3)) if match else None


def validate_current_basis(record, member_id, member_numbers):
    author = record.get("author")
    actor = record.get("acting_reviewer")
    head = record.get("head")
    base = record.get("base")
    observed_at = record.get("observed_at")
    review_basis = record.get("review_basis")
    dependencies = record.get("dependencies")
    if not all(
        isinstance(value, str) and value.strip()
        for value in (author, actor, head, base, observed_at, review_basis)
    ):
        reject(f"{member_id} lacks full identity, revision, timestamp, or basis evidence")
    author = author.strip()
    actor = actor.strip()
    if actor.casefold() == author.casefold() or not FULL_REVISION.fullmatch(head) or not FULL_REVISION.fullmatch(base):
        reject(f"{member_id} identity or revision evidence is invalid")
    number = int(member_id.removeprefix("pr-"))
    if (
        not isinstance(dependencies, list)
        or any(type(dependency) is not int for dependency in dependencies)
        or len(dependencies) != len(set(dependencies))
        or number in dependencies
        or any(dependency not in member_numbers for dependency in dependencies)
    ):
        reject(f"{member_id} dependency relationship evidence is invalid")
    try:
        observed = datetime.fromisoformat(observed_at.replace("Z", "+00:00"))
    except ValueError:
        reject(f"{member_id} timestamp is invalid")
    if observed.tzinfo is None:
        reject(f"{member_id} timestamp is invalid")


def validate_artifact(record, member_url, member_id, member_numbers):
    if not isinstance(record, dict):
        reject(f"{member_id} analysis evidence is not an object")
    number = member_number(member_url)
    if number is None or member_id != f"pr-{number}" or record.get("url", "").casefold() != member_url.casefold():
        reject(f"{member_id} does not match its frozen Review member")
    disposition = record.get("disposition")
    if record.get("evidence_valid") is not True:
        reject(f"{member_id} evidence is not current")
    validate_current_basis(record, member_id, member_numbers)
    if disposition in {"closed", "merged"}:
        return
    if disposition != "open":
        reject(f"{member_id} has no verified open, closed, or merged disposition")
    exact_body = record.get("exact_body")
    body_digest = record.get("body_digest")
    actor = record["acting_reviewer"].strip()
    head = record.get("head")
    receipts = record.get("receipts")
    if record.get("checks") != "green":
        reject(f"{member_id} required checks are not green")
    if record.get("blocking_feedback") != []:
        reject(f"{member_id} has unresolved blocking feedback")
    if record.get("adjudicated_verdict") != "APPROVE":
        reject(f"{member_id} does not have a current APPROVE verdict")
    if not all(isinstance(value, str) and value for value in (exact_body, body_digest, actor, head)):
        reject(f"{member_id} is missing receipt identity evidence")
    if hashlib.sha256(exact_body.encode()).hexdigest() != body_digest:
        reject(f"{member_id} exact review body digest does not match")
    if not isinstance(receipts, list) or not any(
        isinstance(receipt, dict)
        and receipt.get("pr") == number
        and isinstance(receipt.get("actor"), str)
        and receipt["actor"].strip().casefold() == actor.casefold()
        and receipt.get("head") == head
        and receipt.get("state") == "APPROVE"
        and receipt.get("body_digest") == body_digest
        for receipt in receipts
    ):
        reject(f"{member_id} has no exact current APPROVE receipt")


def validate(snapshot, members, artifact_dir):
    checkpoint = snapshot.get("checkpoint")
    if not isinstance(checkpoint, dict) or checkpoint.get("kind") != "analysis":
        reject("persistent Review finalization requires a current analysis checkpoint")
    member_matches = [MEMBER.fullmatch(url) for url in members]
    parsed_members = [int(match.group(3)) if match else None for match in member_matches]
    if not members or any(number is None for number in parsed_members) or len(set(url.casefold() for url in members)) != len(members):
        reject("persistent Review has invalid or duplicate frozen members")
    repositories = {
        (match.group(1).casefold(), match.group(2).casefold())
        for match in member_matches if match
    }
    if len(repositories) != 1:
        reject("persistent Review members do not belong to one canonical repository")
    workspace = snapshot.get("workspace")
    workspace_repository = workspace.get("repository_id") if isinstance(workspace, dict) else None
    member_repository = "github.com/" + "/".join(next(iter(repositories)))
    if not isinstance(workspace_repository, str) or workspace_repository.casefold() != member_repository:
        reject("persistent Review members do not match the canonical workspace repository")
    deliverables = checkpoint.get("deliverables")
    if not isinstance(deliverables, list) or len(deliverables) != len(members):
        reject("persistent Review checkpoint does not cover every frozen member")
    member_by_id = {f"pr-{member_number(url)}": url for url in members}
    seen = set()
    root = Path(artifact_dir)
    for deliverable in deliverables:
        member_id = deliverable.get("id") if isinstance(deliverable, dict) else None
        artifacts = deliverable.get("artifacts") if isinstance(deliverable, dict) else None
        if member_id not in member_by_id or member_id in seen or not isinstance(artifacts, list) or len(artifacts) != 1:
            reject("persistent Review checkpoint member manifest is incomplete or duplicated")
        artifact = artifacts[0]
        digest = artifact.get("value") if isinstance(artifact, dict) and artifact.get("kind") == "analysis_digest" else None
        if not isinstance(digest, str) or not DIGEST.fullmatch(digest):
            reject(f"{member_id} has no valid analysis digest")
        try:
            materialized = json.loads((root / f"{digest}.json").read_text(encoding="utf-8"))
            if materialized.get("digest") != digest or not isinstance(materialized.get("contents"), str):
                raise ValueError
            record = json.loads(materialized["contents"])
        except (OSError, json.JSONDecodeError, TypeError, ValueError):
            reject(f"{member_id} analysis evidence could not be materialized")
        validate_artifact(record, member_by_id[member_id], member_id, set(parsed_members))
        seen.add(member_id)
    if seen != set(member_by_id):
        reject("persistent Review checkpoint does not cover every frozen member")


def main():
    if len(sys.argv) not in {4, 5} or sys.argv[1] not in {"list", "validate"}:
        raise SystemExit(2)
    context = load_context(sys.argv[2], sys.argv[3])
    if context is None:
        return
    snapshot, members = context
    if sys.argv[1] == "list":
        for digest in sorted(set(checkpoint_digests(snapshot))):
            print(digest)
        return
    if len(sys.argv) != 5:
        raise SystemExit(2)
    validate(snapshot, members, sys.argv[4])


if __name__ == "__main__":
    main()
