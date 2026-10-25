"""Policy regression tests for the Rust scheduler's CI supply-chain boundary."""

from __future__ import annotations

import re
import unittest
from pathlib import Path

SHA = re.compile(r"[0-9a-f]{40}\Z")
VERSION = re.compile(r"\d+\.\d+\.\d+\Z")


def policy_violations(workflow: str) -> list[str]:
    errors: list[str] = []
    action_refs = re.findall(r"(?m)^\s*-\s*uses:\s*([^\s#]+)", workflow)
    if not action_refs:
        errors.append("workflow must declare at least one action")
    for action_ref in action_refs:
        if action_ref.startswith("./"):
            continue
        _, separator, revision = action_ref.rpartition("@")
        if not separator or not SHA.fullmatch(revision):
            errors.append(f"external action is not pinned to a full commit SHA: {action_ref}")

    installs = re.findall(
        r"(?m)^\s*(?:-\s*)?run:\s*cargo install --locked cargo-audit --version (\S+)\s*$",
        workflow,
    )
    if len(installs) != 1 or not VERSION.fullmatch(installs[0]):
        errors.append("workflow must install exactly one semver-pinned cargo-audit with --locked")

    audits = re.findall(
        r"(?m)^\s*(?:-\s*)?run:\s*cargo audit --file Cargo\.lock\s*$",
        workflow,
    )
    if len(audits) != 1:
        errors.append("workflow must audit Cargo.lock exactly once with cargo audit --file Cargo.lock")
    return errors


class SupplyChainPolicyTests(unittest.TestCase):
    def test_current_repository_workflow_obeys_policy(self) -> None:
        workflow_path = Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml"
        errors = policy_violations(workflow_path.read_text(encoding="utf-8"))
        self.assertEqual(errors, [], "\n".join(errors))

    def test_accepts_full_action_shas_and_locked_audit(self) -> None:
        workflow = """jobs:
  ci:
    steps:
      - uses: actions/checkout@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa # v4
      - uses: dtolnay/rust-toolchain@bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb # stable
      - run: cargo install --locked cargo-audit --version 0.22.2
      - run: cargo audit --file Cargo.lock
"""
        self.assertEqual(policy_violations(workflow), [])

    def test_rejects_mutable_action_refs(self) -> None:
        workflow = """steps:
  - uses: actions/checkout@v4
  - uses: dtolnay/rust-toolchain@bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
  - run: cargo install --locked cargo-audit --version 0.22.2
  - run: cargo audit --file Cargo.lock
"""
        self.assertTrue(any("not pinned" in error for error in policy_violations(workflow)))

    def test_requires_a_manifest_audit_and_exact_tool_version(self) -> None:
        workflow = """steps:
  - uses: actions/checkout@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  - run: cargo install --locked cargo-audit --version latest
  - run: cargo audit
"""
        errors = policy_violations(workflow)
        self.assertTrue(any("semver-pinned cargo-audit" in error for error in errors))
        self.assertTrue(any("audit Cargo.lock" in error for error in errors))


if __name__ == "__main__":
    unittest.main(verbosity=2)
