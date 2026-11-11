#!/usr/bin/env python3
"""Independent exact-Fraction oracle for CLI decimal-to-f64 acceptance."""
import argparse
from decimal import Decimal
from fractions import Fraction
import json
import math
from pathlib import Path
import random
import subprocess


def expected(text):
    if len(text) > 4096:
        return False
    decimal = Decimal(text)
    target = float(text)
    if not math.isfinite(target) or (target == 0 and decimal != 0):
        return False
    if decimal == 0:
        return True
    quantum = Fraction(10) ** decimal.as_tuple().exponent
    return abs(Fraction(target) - Fraction(decimal)) <= quantum / 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    texts = ["1.000000000000000000000000000001", "1.000000000000000000000000000000", "1e-400", "-1e-400", "5e-324", "1e-323", "0.10000000000000000000", "1.0000000000000002", "1.0000000000000002220446049250313080847263336181640625", "0.000000000000000000000"]
    texts += ["9007199254740993", "9007199254740992", "0e-400", "-0", "1e400", "1e-100000", "0e100000", "1." + "0" * 4000, "1." + "0" * 4095]
    rng = random.Random(20261111)
    texts += [f'{rng.choice(["", "-"])}{rng.randrange(1, 10**20)}e{rng.randrange(-400, 301)}' for _ in range(100)]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    profile = args.output.parent / "decimal-empty.ries"
    profile.write_text("")
    rows = []
    for text in texts:
        command = [str(args.binary.resolve()), "-l0", "-Sx1", "-n1", "--json", "--report=false", "--match-all-digits", f"--profile={profile.resolve()}", "--", text]
        result = subprocess.run(command, capture_output=True, text=True, timeout=30)
        want = expected(text)
        accepted = result.returncode == 0
        passed = want == accepted and (bool(json.loads(result.stdout)) if accepted else not result.stdout)
        rows.append({"literal": text, "expected_acceptance": want, "actual_acceptance": accepted,
                     "passed": passed, "stderr": result.stderr, "returncode": result.returncode})
    report = {"schema": "ries-decimal-boundary-oracle-v1", "passed": all(r["passed"] for r in rows), "cases": rows}
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f'{len(rows)} exact decimal boundary cases; passed={report["passed"]}')
    raise SystemExit(0 if report["passed"] else 1)


if __name__ == "__main__":
    main()
