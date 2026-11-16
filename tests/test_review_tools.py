#!/usr/bin/env python3
"""Counterexamples for independent certificates, differentials and resource gates."""
import copy
from fractions import Fraction as Q
from pathlib import Path
import subprocess
import re
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "tools"))
from algebraic_certificate import canonical, certify, equation_polynomial, verify
from compare_original import parse_original
from qualify_search import compare
from replay_manifest import check_parity, build_command_args


class Certificates(unittest.TestCase):
    def test_irrational_root_is_bounded_but_decimal_is_not_claimed_exact(self):
        row = {"lhs_postfix": "xs", "rhs_postfix": "2", "x_value": 2**0.5}
        result = certify(row, "1.4142135623730951")
        self.assertEqual(result["status"], "independently_bounded")
        certificate = result["certificate"]
        self.assertTrue(verify(certificate))
        lo, hi = map(Q, certificate["interval"])
        self.assertLess(lo * lo, 2)
        self.assertGreater(hi * hi, 2)
        self.assertGreater(Q(certificate["max_distance_from_decimal"]), 0)

    def test_verifier_rejects_tampering(self):
        certificate = certify({"lhs_postfix": "xs", "rhs_postfix": "2", "x_value": 2**0.5}, "1.4142")["certificate"]
        for key, value in [("coefficients", ["-3", "0", "1"]), ("rhs_postfix", "3"),
                           ("interval", ["1", "1"]), ("interval", ["-2", "2"]),
                           ("interval", ["2", "1"]), ("max_distance_from_decimal", "0"),
                           ("target_literal", "1.5"), ("residual_at_numerical_root", "0"),
                           ("numerical_root", "2"), ("schema", "unknown")]:
            changed = copy.deepcopy(certificate)
            changed[key] = value
            self.assertFalse(verify(changed), (key, value))

    def test_supported_and_unavailable_grammars(self):
        exact = certify({"lhs_postfix": "x2/", "rhs_postfix": "3", "x_value": 6}, "6.000")
        self.assertTrue(verify(exact["certificate"]))
        for lhs, rhs in [("xr", "2"), ("xq", "2"), ("x", "p"), ("xss", "2"), ("xx-", "11-")]:
            self.assertEqual(certify({"lhs_postfix": lhs, "rhs_postfix": rhs, "x_value": 1.0}, "1")["status"], "unavailable")
        self.assertEqual(canonical(equation_polynomial("x2*", "3")), ["-3/2", "1"])

    def test_original_normalizes_spelling_and_checks_equation_lines(self):
        a = parse_original("    2 x = 3 ('exact' match) {47}\n")
        b = parse_original("    x+x = 3 ('exact' match) {49}\n")
        self.assertEqual(a["polynomial"], b["polynomial"])
        self.assertEqual(a["complexity"], 47)
        self.assertEqual(parse_original(" x^2 = 2 for x = T - 2.22045e-16 {37}")["polynomial"], ["-2", "0", "1"])
        with self.assertRaises(ValueError):
            parse_original("NOTE: 'exact' match may result from roundoff")

    def test_differential_reports_missing_equations_and_changed_roots(self):
        row = {"lhs_postfix": "x", "rhs_postfix": "2", "complexity": 28, "x_value": 2.0, "error": 0.0}
        self.assertEqual(len(compare([row], [])["missing_equations"]), 1)
        changed = dict(row, x_value=3.0)
        result = compare([row], [changed])
        self.assertTrue(result["ordered_equations_equal"])
        self.assertFalse(result["roots_equal"])
        changed = dict(row, exact=True)
        self.assertFalse(compare([dict(row, exact=False)], [changed])["classifications_equal"])


class ReplayAndBudgets(unittest.TestCase):
    def test_exact_replay_rejects_drift_and_preserves_signed_zero(self):
        row = {"lhs_postfix": "x", "rhs_postfix": "1", "complexity": 25,
               "is_exact": True, "x_value": 1.0, "error": 0.0}
        changed = dict(row, x_value=1.0 + 5e-13, error=5e-13)
        self.assertFalse(check_parity([row], [changed]))
        self.assertTrue(check_parity([row], [changed], comparison="tolerant", abs_tolerance=1e-12))
        self.assertFalse(check_parity([dict(row, error=-0.0)], [row]))
        with self.assertRaises(ValueError):
            check_parity([row], [changed], comparison="tolerant")
        literal = "1.000000000000000000000000000000"
        config = {"target": 1.0, "target_literal": literal, "replay": {"cli_args": [literal, "--match-all-digits"]}}
        self.assertEqual(build_command_args(config)[0], literal)
        config["replay"]["cli_args"][0] = "1"
        with self.assertRaises(ValueError):
            build_command_args(config)

    def test_repeated_squaring_declines_before_large_integer_allocation(self):
        for postfix in ["2" + "s" * 20, "2r" + "s" * 20]:
            result = certify({"lhs_postfix": "x", "rhs_postfix": postfix, "x_value": 1.0}, "1")
            self.assertEqual(result["status"], "unavailable")
            self.assertIn("budget", result["reason"])
        self.assertFalse(verify({"schema": "ries-polynomial-root-v1", "lhs_postfix": "x", "rhs_postfix": "2" + "s" * 200}))
        result = certify({"lhs_postfix": "x", "rhs_postfix": "2", "x_value": 2.0}, "1e1000000000")
        self.assertEqual(result["status"], "unavailable")


class ReleaseGates(unittest.TestCase):
    def test_every_release_build_depends_on_full_reusable_ci(self):
        ci = (ROOT / ".github/workflows/ci.yml").read_text()
        release = (ROOT / ".github/workflows/release.yml").read_text()
        self.assertIn("  workflow_call:", ci)
        self.assertIn("uses: ./.github/workflows/ci.yml", release)
        jobs = dict(re.findall(r"^  ([a-z][a-z-]*):\n(.*?)(?=^  [a-z][a-z-]*:|\Z)", release, re.M | re.S))
        self.assertIn("    needs: test-release\n", jobs["validate-tag"])
        for job in ("build-binaries", "build-wasm", "build-python", "build-python-sdist"):
            block = jobs[job]
            self.assertIn("    needs: validate-tag\n", block.split("    steps:", 1)[0])
        for job in ("publish-crate", "publish-python"):
            self.assertIn("needs: [build-binaries, build-wasm, build-python, build-python-sdist]", jobs[job])
        self.assertIn("npm run test:web:smoke:build", ci)
        self.assertIn("python3 scripts/qualify_search.py", ci)
        self.assertIn("python3 scripts/compare_original.py", ci)


@unittest.skipUnless(sys.platform.startswith("linux"), "RLIMIT_AS guarantees are Linux-only")
class Budgets(unittest.TestCase):
    def invoke(self, code, seconds=5):
        return subprocess.run([sys.executable, str(ROOT / "scripts/run_bounded.py"), "--memory-mib", "64",
                               "--seconds", str(seconds), "--", sys.executable, "-c", code], capture_output=True, text=True)

    def test_success_allocation_failure_and_timeout_are_distinct(self):
        self.assertEqual(self.invoke("print('complete')").returncode, 0)
        failure = self.invoke("x = bytearray(256 * 1024 * 1024)")
        self.assertNotEqual(failure.returncode, 0)
        self.assertIn("incomplete", failure.stderr)
        timeout = self.invoke("import time; time.sleep(10)", seconds=1)
        self.assertEqual(timeout.returncode, 124)
        self.assertIn("incomplete", timeout.stderr)


if __name__ == "__main__":
    unittest.main()
