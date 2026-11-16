#!/usr/bin/env python3
"""Bind a selected discovery hypothesis to an independent polynomial root check.

Accept a full ries-discovery-v1 run plus a zero-based --hypothesis-index, or a
ries-discovery-selection-v1 browser wrapper. Preserve the complete run. A hash
detects accidental drift; it does not authenticate an executed search. Only the
existing certifier's bounded degree-two grammar can receive a root certificate.
"""

import argparse
from fractions import Fraction
import hashlib
import json
import math
from pathlib import Path
import re
import struct
import sys

from algebraic_certificate import bounded_q, certify, verify


DECIMAL = re.compile(r"[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?", re.ASCII)
CLAIM = ("An independently bounded equation root is not proof that the entered decimal "
         "equals that root or that a displayed rearrangement/intended mathematical constant "
         "satisfies the equation. The certificate proves only its explicit source equation. "
         "The run hash detects content drift, not authenticity or search completeness.")


def digest(data):
    return hashlib.sha256(json.dumps(data, sort_keys=True, separators=(",", ":"),
                                     ensure_ascii=True, allow_nan=False).encode()).hexdigest()


def finite(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def decimal_tolerance(literal):
    text = literal.strip()
    mantissa, *exponent = re.split("[eE]", text)
    places = len(mantissa.partition(".")[2])
    power = int(exponent[0]) if exponent else 0
    return min(float(f"5e{power - places - 1}"), sys.float_info.max)


def rows(hypothesis):
    return [hypothesis["representative"], *hypothesis["supporting_equations"]]


def structural_postfix_key(postfix, strip_outer_double_negation=False):
    """Mechanical syntax identity only; no numerical or domain-based equivalence."""
    if not isinstance(postfix, str) or not 0 < len(postfix) <= 4096:
        raise ValueError("structural identity needs bounded postfix text")
    if strip_outer_double_negation:
        while postfix.endswith("nn"):
            postfix = postfix[:-2]
    stack = []
    for symbol in postfix:
        code = ord(symbol)
        if symbol in "123456789pefgPzGx" or 128 <= code <= 143:
            stack.append(symbol)
        elif symbol in "nrqslESCTW" or 144 <= code <= 159:
            if not stack:
                raise ValueError("invalid unary postfix stack")
            name = symbol if code < 128 else f"#{code}"
            stack.append(f"{name}({stack.pop()})")
        elif symbol in "+-*/^vLA":
            if len(stack) < 2:
                raise ValueError("invalid binary postfix stack")
            right, left = stack.pop(), stack.pop()
            if symbol in "+*" and left > right:
                left, right = right, left
            stack.append(f"({left}{symbol}{right})")
        else:
            raise ValueError(f"unsupported structural postfix symbol {symbol!r}")
    if len(stack) != 1:
        raise ValueError("invalid postfix stack")
    return stack[0]


def candidate_identity(row):
    solved = row.get("solve_for_x_postfix")
    if solved:
        return ("solved", structural_postfix_key(solved, strip_outer_double_negation=True))
    return ("equation", structural_postfix_key(row["lhs_postfix"]),
            structural_postfix_key(row["rhs_postfix"]))


def validate_group(hypothesis):
    representative = hypothesis["representative"]
    identity = candidate_identity(representative)
    for supporting in hypothesis["supporting_equations"]:
        if candidate_identity(supporting) != identity:
            raise ValueError("supporting equation belongs to a different structural root candidate")
        a, b = representative["x_value"], supporting["x_value"]
        # A coarse sanity check catches stale, clearly different roots. Close
        # roots never establish identity: the independent structural test above
        # is required regardless of this distance.
        allowance = max(1e-6 * max(abs(a), abs(b), 1.0), 128 * max(math.ulp(a), math.ulp(b)))
        if abs(a - b) > allowance:
            raise ValueError("supporting equation has a clearly different numerical root")


def validate_run(run):
    """Check envelope/bindings, not the truth of arbitrary expression syntax."""
    if not isinstance(run, dict) or run.get("schema") != "ries-discovery-v1":
        raise ValueError("expected a ries-discovery-v1 run")
    literal = run.get("target_literal")
    if not isinstance(literal, str) or not 0 < len(literal.strip()) <= 4096 or not DECIMAL.fullmatch(literal.strip()):
        raise ValueError("run must preserve a finite decimal target literal")
    target = run.get("target")
    if not finite(target) or not math.isfinite(float(literal)):
        raise ValueError("target must be finite")
    if struct.pack("!d", target) != struct.pack("!d", float(literal)):
        raise ValueError("numeric target does not match the original literal")
    mantissa = re.split("[eE]", literal.strip())[0].lstrip("+-")
    if target == 0 and any(char not in "0." for char in mantissa):
        raise ValueError("nonzero decimal literal underflows to zero")
    config = run.get("config")
    if not isinstance(config, dict) or not finite(config.get("input_tolerance")):
        raise ValueError("resolved precision configuration is missing")
    if config["input_tolerance"] != decimal_tolerance(literal):
        raise ValueError("input tolerance does not match the literal's last place")
    if target != 0:
        text, *exponent = re.split("[eE]", literal.strip())
        power = (int(exponent[0]) if exponent else 0) - len(text.partition(".")[2]) - 1
        if abs(Fraction(literal.strip()) - Fraction(target)) > Fraction(f"5e{power}"):
            raise ValueError("numeric target loses more than the entered decimal precision")
    if not isinstance(run.get("stats"), dict) or not isinstance(run.get("engine_version"), str):
        raise ValueError("engine version and resolved statistics are required")
    hypotheses = run.get("hypotheses")
    if not isinstance(hypotheses, list) or len(hypotheses) > 1000:
        raise ValueError("hypotheses must be a list of at most 1000 entries")
    for hypothesis in hypotheses:
        if not isinstance(hypothesis, dict) or not isinstance(hypothesis.get("key"), str) or not hypothesis["key"]:
            raise ValueError("hypothesis key is missing")
        if not isinstance(hypothesis.get("supporting_equations"), list) or not isinstance(hypothesis.get("representative"), dict):
            raise ValueError("representative/supporting equations are missing")
        if not isinstance(hypothesis.get("within_input_precision"), bool) or not isinstance(hypothesis.get("validation"), dict):
            raise ValueError("hypothesis precision and validation availability are required")
        for row in rows(hypothesis):
            if not isinstance(row, dict) or not all(isinstance(row.get(key), str) and row[key]
                                                   for key in ("lhs_postfix", "rhs_postfix")):
                raise ValueError("equation postfix is missing")
            if not finite(row.get("x_value")) or not finite(row.get("error")):
                raise ValueError("equation root/error must be finite")
            if abs((row["x_value"] - target) - row["error"]) > 8 * math.ulp(target):
                raise ValueError("equation error does not agree with the target/root")
        validate_group(hypothesis)
    return run


def selected_evidence(run, index):
    validate_run(run)
    if isinstance(index, bool) or not isinstance(index, int) or not 0 <= index < len(run["hypotheses"]):
        raise ValueError("hypothesis index is outside the run (indices start at zero)")
    hypothesis = run["hypotheses"][index]
    attempts = []
    for equation_index, row in enumerate(rows(hypothesis)):
        result = certify(row, run["target_literal"].strip())
        if result["status"] == "independently_bounded":
            certificate = result["certificate"]
            if not verify(certificate):
                raise ValueError("independent certificate verification failed")
            evidence = {"status": "independently_bounded", "source_equation_index": equation_index,
                        "source_equation": {key: row[key] for key in ("lhs_postfix", "rhs_postfix", "x_value")},
                        "certificate": certificate, "prior_unavailable_attempts": attempts}
            break
        attempts.append({"source_equation_index": equation_index, "reason": result["reason"]})
    else:
        evidence = {"status": "unavailable", "reason": "No retained equation can be bounded by the supported polynomial grammar/arithmetic budget.",
                    "attempts": attempts}
    return {"schema": "ries-discovery-evidence-v1", "run": run, "run_sha256": digest(run),
            "target_literal": run["target_literal"], "hypothesis_index": index,
            "hypothesis_key": hypothesis["key"], "claim": CLAIM, "evidence": evidence}


def verify_evidence(report):
    if not isinstance(report, dict) or report.get("schema") != "ries-discovery-evidence-v1":
        raise ValueError("expected a ries-discovery-evidence-v1 report")
    run = validate_run(report.get("run"))
    if report.get("run_sha256") != digest(run) or report.get("target_literal") != run["target_literal"]:
        raise ValueError("report/run literal or content hash changed")
    expected = selected_evidence(run, report.get("hypothesis_index"))
    if report.get("hypothesis_key") != expected["hypothesis_key"] or report.get("evidence") != expected["evidence"]:
        raise ValueError("certificate, equation, selected hypothesis, or availability changed")
    return expected["evidence"]["status"]


def unpack_selection(data, index):
    if isinstance(data, dict) and data.get("schema") == "ries-discovery-selection-v1":
        if index is not None and index != data.get("hypothesis_index"):
            raise ValueError("explicit index differs from the selected browser export")
        return data.get("run"), data.get("hypothesis_index")
    return data, 0 if index is None else index


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("--hypothesis-index", type=int, help="Zero-based selection (default: first hypothesis)")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--verify", action="store_true", help="Recheck an existing evidence report and its run/certificate bindings")
    args = parser.parse_args()
    try:
        data = json.loads(args.input.read_text())
        if args.verify:
            if args.hypothesis_index is not None or args.output:
                parser.error("--verify does not accept selection/output options")
            status = verify_evidence(data)
            print(f"Report bindings checked; polynomial root evidence: {status}. {CLAIM}")
        else:
            run, index = unpack_selection(data, args.hypothesis_index)
            report = selected_evidence(run, index)
            text = json.dumps(report, indent=2, allow_nan=False) + "\n"
            if args.output:
                args.output.write_text(text)
                print(f"Polynomial root evidence: {report['evidence']['status']}; saved {args.output}")
            else:
                print(text, end="")
    except (OSError, ValueError, KeyError, TypeError, OverflowError) as error:
        parser.exit(1, f"Discovery evidence failed: {error}\n")


if __name__ == "__main__":
    main()
