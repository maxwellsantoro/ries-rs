#!/usr/bin/env python3
"""Regression checks for the parity and Criterion baseline gates."""
import os
import fnmatch
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parent.parent
PARITY = ROOT / "tests/compare_with_original.sh"
BASELINE = ROOT / "tests/validate_benchmark_baseline.sh"


class Checks(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.directory = Path(self.tmp.name)

    def executable(self, name, body):
        path = self.directory / name
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o755)
        return path

    def parity(self, original, rust):
        env = dict(os.environ, RIES_RS_BIN=str(rust))
        return subprocess.run([str(PARITY), "3.14", "2", "6", str(original)],
                              env=env, capture_output=True, text=True)

    def test_parity_requires_qualifying_equations_from_both(self):
        good = self.executable("good", "echo 'x = pi for x = T + 8e-11 {29}'\nexit 242")
        good_rust = self.executable("good-rust", "echo 'x = pi for x = T + 8e-11 {29}'")
        exact = self.executable("exact", "echo \"x = 2 ('exact' match) {28}\"")
        exact_note = self.executable("exact-note", "echo \"NOTE: 'exact' match may result from floating-point roundoff error.\"")
        bogus = self.executable("bogus", "echo 'unrelated output 8e-11'")
        distant = self.executable("distant", "echo 'x = pi for x = T + 0.1 {29}'")
        failed = self.executable("failed", "echo 'x = pi for x = T + 8e-11 {29}'\nexit 1")
        self.assertEqual(self.parity(good, good_rust).returncode, 0)
        self.assertEqual(self.parity(exact, exact).returncode, 0)
        for original, rust in ((bogus, good_rust), (good, bogus), (good, distant),
                               (good, exact_note), (failed, good_rust)):
            self.assertNotEqual(self.parity(original, rust).returncode, 0)

    def test_baseline_selection_and_missing_data(self):
        def run(name, run_id):
            return subprocess.run([str(BASELINE), name, run_id, str(self.directory)],
                                  capture_output=True, text=True).returncode
        self.assertNotEqual(run("main", "123"), 0)
        for benchmark in ("eval_simple", "search_levels", "lhs_generation"):
            path = self.directory / benchmark / "case" / "main"
            path.mkdir(parents=True)
            (path / "estimates.json").write_text("{}")
        self.assertEqual(run("main", "123"), 0)
        self.assertNotEqual(run("other", "123"), 0)
        self.assertNotEqual(run("../main", "123"), 0)
        self.assertNotEqual(run("main", "abc"), 0)

    def test_release_validation_uses_selected_tag_in_manual_recovery(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        step = workflow.split("      - name: Require tag to match package version\n", 1)[1]
        script = textwrap.dedent(step.split("        run: |\n", 1)[1]
                                 .split("\n  build-binaries:", 1)[0])
        (self.directory / "Cargo.toml").write_text('[package]\nversion = "3.0.0"\n')
        for tag, expected in (("v3.0.0", 0), ("v3.0.1", 1), ("main", 1)):
            with self.subTest(tag=tag):
                result = subprocess.run(
                    ["bash", "-eo", "pipefail", "-c", script], cwd=self.directory,
                    env=dict(os.environ, RELEASE_TAG=tag, GITHUB_REF_NAME="main"),
                    capture_output=True, text=True)
                self.assertEqual(result.returncode, expected, result.stdout + result.stderr)

    def test_release_recovery_keeps_all_checkouts_on_selected_source(self):
        # Missing one override can test or publish main while other jobs use the tag.
        for name, expected in (
            ("ci.yml", "ref: ${{ inputs.source_ref || github.sha }}"),
            ("release.yml", "ref: ${{ inputs.tag && format('refs/tags/{0}', inputs.tag) || github.sha }}"),
        ):
            lines = (ROOT / ".github/workflows" / name).read_text().splitlines()
            checkouts = [i for i, line in enumerate(lines) if "uses: actions/checkout@" in line]
            self.assertTrue(checkouts)
            for i in checkouts:
                with self.subTest(workflow=name, line=i + 1):
                    self.assertEqual(lines[i + 1].strip(), "with:")
                    self.assertEqual(lines[i + 2].strip(), expected)

    def test_python_39_release_wheel_uses_supported_linux_runner(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        job = workflow.split("  build-python:\n", 1)[1].split("  build-python-sdist:\n", 1)[0]
        self.assertIn("- os: ubuntu-24.04", job)
        self.assertNotIn("- os: ubuntu-latest", job)

    def test_github_release_excludes_ci_evidence_and_consumer_builds(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        job = workflow.split("  create-release:\n", 1)[1]
        selectors = [line.strip().split(": ", 1)[1] for line in job.splitlines()
                     if line.strip().startswith(("pattern: ", "name: python-sdist"))]
        artifact_names = ["ries-rs-linux-x86_64", "ries-rs-macos-x86_64",
                          "ries-rs-macos-aarch64", "ries-rs-windows-x86_64.exe",
                          "ries-rs-wasm", "python-wheel-ubuntu-24.04",
                          "python-wheel-macos-latest", "python-wheel-windows-latest",
                          "python-sdist", "search-qualification-commit", "installed-consumers-commit"]
        selected = [name for name in artifact_names
                    if any(fnmatch.fnmatchcase(name, pattern) for pattern in selectors)]
        self.assertEqual(selected, artifact_names[:9])
        self.assertNotIn("artifacts/**/*", job)


if __name__ == "__main__":
    unittest.main()
