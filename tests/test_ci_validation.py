#!/usr/bin/env python3
"""Regression checks for the parity and Criterion baseline gates."""
import os
from pathlib import Path
import subprocess
import tempfile
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


if __name__ == "__main__":
    unittest.main()
