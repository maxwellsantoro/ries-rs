#!/usr/bin/env python3
"""Restricted historical differential: equation identity, cost and exact root bounds.

Both programs must discover the known polynomial for each fixture. Equivalent
spellings are normalized with exact rational polynomial arithmetic. Historical
ranking and search budgets differ, so expression/cost differences are reported,
not silently called full parity. Printed root distances are not the oracle.
"""
import argparse
import ast
from fractions import Fraction as Q
import json
import hashlib
from pathlib import Path
import re
import subprocess

from algebraic_certificate import add, canonical, certify, divide, equation_polynomial, mul, neg, trim

ROOT = Path(__file__).resolve().parents[1]


def infix_polynomial(expression):
    expression = re.sub(r"(?<=[0-9x)])\s*(?=[x(])", "*", expression)
    tree = ast.parse(expression.replace("^", "**"), mode="eval")

    def visit(node):
        if isinstance(node, ast.Expression):
            return visit(node.body)
        if isinstance(node, ast.Name) and node.id == "x":
            return [Q(0), Q(1)]
        if isinstance(node, ast.Constant) and type(node.value) is int and 1 <= node.value <= 9:
            return [Q(node.value)]
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
            return neg(visit(node.operand))
        if isinstance(node, ast.BinOp):
            a, b = visit(node.left), visit(node.right)
            if isinstance(node.op, ast.Add):
                return add(a, b)
            if isinstance(node.op, ast.Sub):
                return add(a, neg(b))
            if isinstance(node.op, ast.Mult):
                return mul(a, b)
            if isinstance(node.op, ast.Div):
                return divide(a, b)
            if isinstance(node.op, ast.Pow) and b == [Q(2)]:
                return mul(a, a)
        raise ValueError(f"unsupported historical syntax: {expression}")

    return trim(visit(tree))


def parse_original(text):
    pattern = r"^\s*(.+?)\s+=\s+(.+?)\s+(?:\('exact' match\)|for x = T\s*[+-]\s*\S+)\s*\{(\d+)\}\s*$"
    for line in text.splitlines():
        match = re.match(pattern, line)
        if match:
            lhs, rhs, cost = match.groups()
            p = add(infix_polynomial(lhs), neg(infix_polynomial(rhs)))
            return {"lhs": lhs, "rhs": rhs, "complexity": int(cost), "polynomial": canonical(p)}
    raise ValueError("original did not emit a supported equation")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--original", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/ries-rs")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    profile = args.output / "empty.ries"
    profile.write_text("")
    corpus = json.loads((ROOT / "tests/corpus/search-contract.json").read_text())
    report = {"schema": "ries-historical-differential-v1", "passed": True, "cases": [],
              "original_binary_sha256": hashlib.sha256(args.original.read_bytes()).hexdigest(),
              "rust_binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest()}
    for case in corpus["cases"]:
        if "expected_polynomial" not in case:
            continue
        target = case["target"]
        original = subprocess.run([str(args.original.resolve()), "-l0", "-Sx12345+-*ns", "--max-matches", "1",
                                   "--max-match-distance", "1e-8", target], capture_output=True, text=True, timeout=30)
        if original.returncode not in (0, 242):
            raise RuntimeError(original.stderr)
        (args.output / f'{case["name"]}-original.txt').write_text(original.stdout)
        old = parse_original(original.stdout)
        rust = subprocess.run([str(args.binary.resolve()), target, "-l0", "-Sx12345+-*ns", "--classic", "--strict-config",
                               "--json", "--report=false", "-n1", f"--profile={profile.resolve()}"],
                              capture_output=True, text=True, check=True, timeout=30)
        (args.output / f'{case["name"]}-rust.json').write_text(rust.stdout)
        row = json.loads(rust.stdout)["results"][0]
        poly = canonical(equation_polynomial(row["lhs_postfix"], row["rhs_postfix"]))
        certificate = certify(row, target)
        # Exact normalization establishes that the original equation has the
        # same root set as the independently bounded Rust equation.
        passed = (old["polynomial"] == poly == case["expected_polynomial"] and
                  certificate["status"] == "independently_bounded" and
                  Q(certificate["certificate"]["max_distance_from_decimal"]) <= Q("1e-8"))
        report["passed"] &= passed
        report["cases"].append({"name": case["name"], "target_literal": target,
            "expected_polynomial": case["expected_polynomial"], "original": old,
            "rust": {"lhs": row["lhs"], "rhs": row["rhs"], "complexity": row["complexity"], "polynomial": poly},
            "certification": certificate, "passed": passed,
            "cost_difference": row["complexity"] - old["complexity"],
            "scope": "Top-1 known-answer polynomial equivalence; historical generation and ranking may choose different spellings"})
    (args.output / "differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f'{len(report["cases"])} historical known-answer cases; passed={report["passed"]}')
    raise SystemExit(0 if report["passed"] else 1)


if __name__ == "__main__":
    main()
