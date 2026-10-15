#!/usr/bin/env python3
"""Structural consistency checks for the proposed PR-021 roadmap packet."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
ROADMAP = (ROOT / "plan/product-roadmap.md").read_text(encoding="utf-8")
LEDGER = (ROOT / "plan/roadmap-pr-ledger.md").read_text(encoding="utf-8")

phases = [int(value) for value in re.findall(r"^### Phase ([1-8]):", ROADMAP, re.M)]
assert phases == list(range(1, 9)), f"phase order/count mismatch: {phases}"
print("PASS roadmap has the eight ordered phases")

matrix_text = ROADMAP.split("## Pairwise review matrix", 1)[1].split(
    "## Multi phase journey reviews", 1
)[0]
pairs = [tuple(map(int, match)) for match in re.findall(r"\| P([1-8]) \+ P([1-8]) \|", matrix_text)]
expected_pairs = [(left, right) for left in range(1, 9) for right in range(left + 1, 9)]
assert len(pairs) == 28 and sorted(pairs) == expected_pairs, f"pair matrix mismatch: {pairs}"
print("PASS pair matrix contains each of the 28 unique phase pairs")

journey_text = ROADMAP.split("## Multi phase journey reviews", 1)[1].split(
    "## Adversarial review protocol", 1
)[0]
journeys = [int(value) for value in re.findall(r"^(\d+)\. ", journey_text, re.M)]
assert journeys == list(range(1, 7)), f"combined journey count/order mismatch: {journeys}"
print("PASS roadmap contains six ordered combined journeys")

entries = list(re.finditer(r"^### PR-(\d+): (.+)$", LEDGER, re.M))
ids = [int(entry.group(1)) for entry in entries]
assert ids == list(range(21, 54)), f"canonical Task-ID sequence mismatch: {ids}"
print("PASS ledger has PR-021 through PR-053 in order (31 product tasks plus cross-cutting PR-053)")

branches: set[str] = set()
commits_checked = 0


def parse_dependencies(text: str) -> set[int]:
    dependencies: set[int] = set()
    for first, last in re.findall(r"PR-(\d+) through PR-(\d+)", text):
        start, end = int(first), int(last)
        assert start <= end, f"reversed dependency range: PR-{start:03} through PR-{end:03}"
        dependencies.update(range(start, end + 1))
    for suffix in re.findall(r"PR-\d+/(\d+(?:/\d+)*)", text):
        dependencies.update(map(int, suffix.split("/")))
    # Parse every reference, including compact historical forms such as PR-029/030.
    dependencies.update(map(int, re.findall(r"PR-(\d+)", text)))
    return dependencies


assert parse_dependencies("PR-029/030 and PR-032 through PR-035") == {29, 30, 32, 33, 34, 35}
print("PASS dependency parser expands direct, slash-separated, and range references")

for index, entry in enumerate(entries):
    task_id = int(entry.group(1))
    end = entries[index + 1].start() if index + 1 < len(entries) else len(LEDGER)
    body = LEDGER[entry.start() : end]
    branch_match = re.search(r"^- Branch: `([^`]+)`\.$", body, re.M)
    commit_match = re.search(r"^- Commits: (.+)$", body, re.M)
    dependency_match = re.search(r"^- Depends on (.+)$", body, re.M)
    assert branch_match and commit_match and dependency_match, f"missing planning fields in PR-{task_id:03}"
    branch = branch_match.group(1)
    assert branch not in branches, f"duplicate branch: {branch}"
    branches.add(branch)

    subjects = re.findall(r"`([^`]+)`", commit_match.group(1))
    assert len(subjects) >= 2, f"PR-{task_id:03} needs at least two planned commits"
    for subject in subjects:
        assert re.fullmatch(
            r"(?:feat|fix|docs|test|refactor|chore|perf|build|ci|revert)(?:\([a-z0-9-]+\))?: .+",
            subject,
        ), f"non-Conventional Commit subject in PR-{task_id:03}: {subject}"
        commits_checked += 1

    dep_text = dependency_match.group(1)
    deps = parse_dependencies(dep_text)
    for dep in deps:
        assert dep < task_id, f"dependency cycle/forward edge: PR-{task_id:03} -> PR-{dep:03}"
        assert dep <= 20 or dep in ids, f"unknown dependency PR-{dep:03} in PR-{task_id:03}"

print(f"PASS all {len(entries)} task entries have unique branches, dependency fields, and {commits_checked} Conventional Commit subjects")
print("PASS every planned dependency points to a completed or earlier Task-ID; no forward edge or cycle")

assert "Only two layers are reserved for the final phase: platform user accounts and authentication, and a private platform administration dashboard." in ROADMAP
assert ROADMAP.index("### Phase 7:") < ROADMAP.index("### Phase 8:")
assert "No login, platform-user table, owner session, admin route" in LEDGER
assert "## Phase 8: platform users and administration, last" in LEDGER
print("PASS private account/auth/admin implementation is confined to Phase 8")

assert "must not be edited" in ROADMAP
assert "do not edit frozen C-01" in LEDGER
assert "restore-before-delete proof" in LEDGER
assert "private overlay repository" in LEDGER
assert "private-only environment flag" in ROADMAP
print("PASS frozen contracts, restore gate, and external private overlay boundaries are explicit")

assert "### PR-053: install and route project-local skills" in LEDGER
assert LEDGER.index("### PR-052:") < LEDGER.index("### PR-053:")
assert "must complete before PR-022 starts" in LEDGER
print("PASS cross-cutting PR-053 follows the stable product-task IDs and gates the first product task")

assert "16 KiB" in LEDGER and "1 MiB" in LEDGER
assert "manifest_version" in LEDGER and "schema_version" in LEDGER
assert "form.submission.accepted" in LEDGER
assert "unknown" in LEDGER
print("PASS ledger names separate transport/schema versions, submission event, and unknown external-effect state")
