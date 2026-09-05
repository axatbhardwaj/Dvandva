#!/usr/bin/env python3
"""Deterministic external GitHub fixture for the persistent Review canary.

This models only GitHub observations and review writes. Baton/checkpoint
behavior remains exercised through the installed public role facade.
"""

from __future__ import annotations

import copy
import hashlib
import json
import sys
from pathlib import Path


REPOSITORY = "github.com/axatbhardwaj/Dvandva"
REVIEWER = "paired-reviewer"


def oid(character: str) -> str:
    return character * 40


def digest(body: str) -> str:
    return hashlib.sha256(body.encode()).hexdigest()


class GitHubFixture:
    def __init__(self) -> None:
        self.prs = {
            101: self.pr(101, "alice", oid("1"), oid("a"), [], "green"),
            102: self.pr(102, "bob", oid("2"), oid("a"), [], "green"),
            103: self.pr(103, "carol", oid("3"), oid("a"), [], "pending"),
            104: self.pr(104, "dave", oid("4"), oid("3"), [103], "green"),
            105: self.pr(105, "erin", oid("5"), oid("a"), [], "green"),
        }
        self.prs[102]["blocking_feedback"] = ["Bounds check is absent"]
        self.verdicts = {
            101: "APPROVE",
            102: "REQUEST_CHANGES",
            103: "APPROVE",
            104: "APPROVE",
            105: "APPROVE",
        }
        self.bodies = {
            number: f"Fixture verdict for PR {number}: {verdict}"
            for number, verdict in self.verdicts.items()
        }
        self.evidence_valid = {number: True for number in self.prs}
        self.receipts: list[dict[str, object]] = []
        self.events: list[dict[str, object]] = []
        self.write_count = 0

    @staticmethod
    def pr(
        number: int,
        author: str,
        head: str,
        base: str,
        dependencies: list[int],
        checks: str,
    ) -> dict[str, object]:
        return {
            "number": number,
            "url": f"https://{REPOSITORY}/pull/{number}",
            "author": author,
            "head": head,
            "base": base,
            "dependencies": dependencies,
            "checks": checks,
            "blocking_feedback": [],
            "disposition": "open",
        }

    def record(self, event: str, number: int, **details: object) -> None:
        self.events.append({"event": event, "pr": number, **details})

    def invalidate(self, number: int, reason: str) -> list[int]:
        affected = {number}
        changed = True
        while changed:
            changed = False
            for candidate, pr in self.prs.items():
                if candidate not in affected and any(
                    dependency in affected for dependency in pr["dependencies"]
                ):
                    affected.add(candidate)
                    changed = True
        for candidate in affected:
            self.evidence_valid[candidate] = False
        self.record("evidence_invalidated", number, reason=reason, affected=sorted(affected))
        return sorted(affected)

    def change_head(self, number: int, head: str) -> list[int]:
        self.prs[number]["head"] = head
        return self.invalidate(number, "head_drift")

    def change_base(self, number: int, base: str) -> list[int]:
        self.prs[number]["base"] = base
        return self.invalidate(number, "base_or_dependency_drift")

    def recheck(self, number: int) -> None:
        dependencies = self.prs[number]["dependencies"]
        assert all(self.evidence_valid[dependency] for dependency in dependencies)
        self.evidence_valid[number] = True
        self.record("evidence_rechecked", number)

    def find_exact(
        self, number: int, actor: str, head: str, state: str, body: str
    ) -> dict[str, object] | None:
        body_digest = digest(body)
        return next(
            (
                receipt
                for receipt in self.receipts
                if receipt["pr"] == number
                and str(receipt["actor"]).casefold() == actor.casefold()
                and receipt["head"] == head
                and receipt["state"] == state
                and receipt["body_digest"] == body_digest
            ),
            None,
        )

    def submit(
        self,
        number: int,
        actor: str,
        state: str,
        body: str,
        expected_head: str,
        expected_base: str,
    ) -> dict[str, object]:
        pr = self.prs[number]
        if actor.casefold() == str(pr["author"]).casefold():
            return {"outcome": "self_review"}
        if pr["head"] != expected_head:
            return {"outcome": "head_drift", "actual_head": pr["head"]}
        if pr["base"] != expected_base:
            return {"outcome": "base_drift", "actual_base": pr["base"]}
        existing = self.find_exact(number, actor, expected_head, state, body)
        if existing is not None:
            return {"outcome": "confirmed_existing", "receipt": existing}
        self.write_count += 1
        receipt: dict[str, object] = {
            "id": f"fixture-review-{self.write_count}",
            "actor": actor,
            "pr": number,
            "head": expected_head,
            "state": state,
            "body_digest": digest(body),
        }
        self.receipts.append(receipt)
        self.record("review_submitted", number, receipt_id=receipt["id"])
        return {"outcome": "submitted", "receipt": receipt}

    def artifact(self, number: int) -> dict[str, object]:
        pr = copy.deepcopy(self.prs[number])
        return {
            **pr,
            "acting_reviewer": REVIEWER,
            "adjudicated_verdict": self.verdicts[number],
            "exact_body": self.bodies[number],
            "body_digest": digest(self.bodies[number]),
            "receipts": [r for r in self.receipts if r["pr"] == number],
            "evidence_valid": self.evidence_valid[number],
        }

    def ready(self) -> bool:
        for number, pr in self.prs.items():
            if pr["disposition"] != "open":
                continue
            if (
                pr["checks"] != "green"
                or pr["blocking_feedback"]
                or self.verdicts[number] != "APPROVE"
                or not self.evidence_valid[number]
                or self.find_exact(
                    number, REVIEWER, str(pr["head"]), "APPROVE", self.bodies[number]
                ) is None
            ):
                return False
        return True


def write_artifacts(root: Path, phase: str, artifacts: dict[int, dict[str, object]]) -> None:
    directory = root / phase
    directory.mkdir(parents=True, exist_ok=True)
    for number, artifact in artifacts.items():
        (directory / f"pr-{number}.json").write_text(
            json.dumps(artifact, indent=2, sort_keys=True) + "\n"
        )


def run(root: Path) -> None:
    fixture = GitHubFixture()
    assert set(fixture.verdicts.values()) == {"APPROVE", "REQUEST_CHANGES"}
    assert not fixture.ready()
    initial_ready = fixture.ready()
    initial = {number: fixture.artifact(number) for number in fixture.prs}

    # An interruption immediately before a write has no receipt to reuse.
    fixture.record("interrupted_before_write", 101)
    assert fixture.find_exact(
        101, REVIEWER, str(fixture.prs[101]["head"]), "APPROVE", fixture.bodies[101]
    ) is None
    first = fixture.submit(
        101,
        REVIEWER,
        "APPROVE",
        fixture.bodies[101],
        str(fixture.prs[101]["head"]),
        str(fixture.prs[101]["base"]),
    )
    assert first["outcome"] == "submitted"

    # An interruption after a write is recovered by exact receipt lookup. The
    # retry must not create a duplicate formal review.
    requested = fixture.submit(
        102,
        REVIEWER,
        "REQUEST_CHANGES",
        fixture.bodies[102],
        str(fixture.prs[102]["head"]),
        str(fixture.prs[102]["base"]),
    )
    assert requested["outcome"] == "submitted"
    fixture.record("interrupted_after_write", 102)
    writes_after_interruption = fixture.write_count
    retry = fixture.submit(
        102,
        REVIEWER,
        "REQUEST_CHANGES",
        fixture.bodies[102],
        str(fixture.prs[102]["head"]),
        str(fixture.prs[102]["base"]),
    )
    assert retry["outcome"] == "confirmed_existing"
    assert fixture.write_count == writes_after_interruption
    assert not fixture.ready()
    requested_changes_ready = fixture.ready()

    # Identity and receipt matching is exact. Actor, head, state, and body all
    # participate; author self-review is rejected.
    assert fixture.find_exact(
        102, "someone-else", str(fixture.prs[102]["head"]),
        "REQUEST_CHANGES", fixture.bodies[102]
    ) is None
    assert fixture.find_exact(
        102, REVIEWER, str(fixture.prs[102]["head"]),
        "REQUEST_CHANGES", fixture.bodies[102] + " changed"
    ) is None
    assert fixture.submit(
        103, "carol", "APPROVE", fixture.bodies[103],
        str(fixture.prs[103]["head"]), str(fixture.prs[103]["base"])
    )["outcome"] == "self_review"

    # A head and then a base move immediately before submission each block the
    # stale write. Rechecking can retain evidence only at the observed revision.
    old_104_head = str(fixture.prs[104]["head"])
    old_104_base = str(fixture.prs[104]["base"])
    fixture.change_head(104, oid("6"))
    assert fixture.submit(
        104, REVIEWER, "APPROVE", fixture.bodies[104], old_104_head, old_104_base
    )["outcome"] == "head_drift"
    fixture.recheck(104)
    current_104_head = str(fixture.prs[104]["head"])
    fixture.change_base(104, oid("7"))
    assert fixture.submit(
        104, REVIEWER, "APPROVE", fixture.bodies[104],
        current_104_head, old_104_base
    )["outcome"] == "base_drift"
    fixture.recheck(104)

    # Pending CI becoming green does not conceal dependency drift. A changed
    # member invalidates itself and its dependent while unrelated checked
    # evidence stays valid.
    fixture.prs[103]["checks"] = "green"
    fixture.record("checks_changed", 103, before="pending", after="green")
    affected = fixture.change_head(103, oid("8"))
    assert affected == [103, 104]
    assert all(fixture.evidence_valid[n] for n in (101, 102, 105))
    assert not fixture.evidence_valid[103] and not fixture.evidence_valid[104]
    fixture.change_base(104, str(fixture.prs[103]["head"]))
    fixture.recheck(103)
    fixture.recheck(104)

    # New blocking feedback requires a complete author repair and fresh head
    # review before the requested-changes member can become APPROVE.
    fixture.prs[102]["blocking_feedback"].append("New regression found")
    fixture.invalidate(102, "new_blocking_feedback")
    fixture.change_head(102, oid("9"))
    fixture.prs[102]["blocking_feedback"] = []
    fixture.record("author_repair", 102)
    fixture.recheck(102)
    fixture.verdicts[102] = "APPROVE"
    fixture.bodies[102] = "Fixture verdict for PR 102 after repair: APPROVE"

    for number in (102, 103, 104):
        result = fixture.submit(
            number,
            REVIEWER,
            "APPROVE",
            fixture.bodies[number],
            str(fixture.prs[number]["head"]),
            str(fixture.prs[number]["base"]),
        )
        assert result["outcome"] == "submitted"
        assert fixture.find_exact(
            number,
            REVIEWER,
            str(fixture.prs[number]["head"]),
            "APPROVE",
            fixture.bodies[number],
        ) is not None

    # Terminal GitHub dispositions remain literal rather than fabricated green
    # readiness. Every still-open member satisfies the persistent Review gate.
    fixture.prs[101]["disposition"] = "merged"
    fixture.prs[105]["disposition"] = "closed"
    fixture.record("disposition_changed", 101, disposition="merged")
    fixture.record("disposition_changed", 105, disposition="closed")
    assert fixture.ready()
    for number, pr in fixture.prs.items():
        if pr["disposition"] != "open":
            continue
        assert pr["checks"] == "green"
        assert not pr["blocking_feedback"]
        assert fixture.verdicts[number] == "APPROVE"
        assert fixture.evidence_valid[number]
        assert fixture.find_exact(
            number,
            REVIEWER,
            str(pr["head"]),
            "APPROVE",
            fixture.bodies[number],
        ) is not None

    final = {number: fixture.artifact(number) for number in fixture.prs}
    write_artifacts(root, "initial", initial)
    write_artifacts(root, "final", final)
    summary = {
        "fixture": "deterministic GitHub Review lifecycle",
        "repository": REPOSITORY,
        "members": sorted(fixture.prs),
        "write_count": fixture.write_count,
        "receipt_count": len(fixture.receipts),
        "zero_duplicate_confirmed_retries": True,
        "initial_ready": initial_ready,
        "requested_changes_ready": requested_changes_ready,
        "final_ready": fixture.ready(),
        "events": fixture.events,
    }
    (root / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(
        "github fixture: ok; "
        f"writes={summary['write_count']}; receipts={summary['receipt_count']}; "
        f"events={len(summary['events'])}"
    )


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: github_review_lifecycle.py OUTPUT_DIR")
    run(Path(sys.argv[1]))
