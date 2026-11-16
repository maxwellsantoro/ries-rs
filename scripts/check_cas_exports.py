#!/usr/bin/env python3
"""Execute CLI SymPy exports against an independent symbolic implementation.

Run with uv run --no-project --with sympy==1.14.0 python
scripts/check_cas_exports.py --binary target/debug/ries-rs.
"""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

import sympy


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    x = sympy.Symbol("x", real=True)
    cases = [
        ("-2", "x38nv", "2", sympy.Integer(-2), "real_root("),
        ("-0.5", "x38nv", "2", sympy.Rational(-1, 2), "real_root("),
        ("16", "x124/v", "2", sympy.Integer(16), "real_root("),
        ("0.5671432904097838", "xW1", "0", sympy.LambertW(1), "LambertW("),
    ]
    with tempfile.TemporaryDirectory(prefix="ries-cas-") as directory:
        profile = Path(directory) / "empty.profile"
        profile.touch()
        for target, symbols, level, expected, function in cases:
            command = [
                str(binary), "--profile", str(profile), "--report=false",
                "--deterministic", "--one-sided", "--format=sympy", "--json", "--max-matches=1",
                "--level=" + level, "--only-symbols=" + symbols, "--", target,
            ]
            output = json.loads(subprocess.check_output(command, text=True))
            result = output["results"][0]
            assert result["lhs"] == "x", result
            exported = result["rhs"]
            assert function in exported, exported
            value = sympy.sympify(exported, locals={"x": x})
            error = abs(sympy.N(value - expected, 50))
            assert error < sympy.Float("1e-14"), (target, exported, error)
            assert value.is_real is True, (target, exported, value)
    print(f"{len(cases)} CLI CAS exports preserve their independently evaluated real values")


if __name__ == "__main__":
    main()
