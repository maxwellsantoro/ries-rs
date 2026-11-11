#!/usr/bin/env python3
"""Experimental exact rational certificates for degree <= 2 polynomial equations.

Consumes CLI JSON, preserving the supplied decimal literal separately from f64.
A certificate bounds a unique real root; it does not assert that the decimal
literal equals that root. No production evaluator or Newton routine is used.
"""
from __future__ import annotations

import argparse
from fractions import Fraction as Q
import json
import math
import re
from pathlib import Path


MAX_COEFFICIENT_BITS = 4096
MAX_RATIONAL_TEXT = 2048
MAX_DECIMAL_DIGITS = 1233  # floor(4096 * log10(2))


def checked(q):
    if max(abs(q.numerator).bit_length(), q.denominator.bit_length()) > MAX_COEFFICIENT_BITS:
        raise ValueError("coefficient arithmetic exceeds 4096-bit budget")
    return q


def bounded_q(value):
    if isinstance(value, str):
        if len(value) > MAX_RATIONAL_TEXT:
            raise ValueError("rational text exceeds arithmetic-size budget")
        if "/" in value:
            parts = value.split("/")
            if len(parts) != 2 or any(not re.fullmatch(r"[+-]?\d+", p) or len(p.lstrip("+-0")) > MAX_DECIMAL_DIGITS for p in parts):
                raise ValueError("rational text exceeds arithmetic-size budget")
        else:
            match = re.fullmatch(r"[+-]?(\d*(?:\.\d*)?)(?:[eE]([+-]?\d+))?", value)
            if not match or not any(c.isdigit() for c in match[1]):
                raise ValueError("invalid rational text")
            exponent = match[2] or "0"
            if len(exponent.lstrip("+-0")) > 4:
                raise ValueError("decimal exponent exceeds arithmetic-size budget")
            places = len(match[1].partition(".")[2])
            digits = len(match[1].replace(".", "").lstrip("0"))
            if digits + abs(int(exponent) - places) > MAX_DECIMAL_DIGITS:
                raise ValueError("decimal exceeds arithmetic-size budget")
    elif isinstance(value, int) and abs(value).bit_length() > MAX_COEFFICIENT_BITS:
        raise ValueError("integer exceeds arithmetic-size budget")
    return checked(Q(value))


def qmul(a, b):
    if not a or not b:
        return Q(0)
    if (abs(a.numerator).bit_length() + abs(b.numerator).bit_length() > MAX_COEFFICIENT_BITS or
            a.denominator.bit_length() + b.denominator.bit_length() > MAX_COEFFICIENT_BITS):
        raise ValueError("multiplication would exceed 4096-bit arithmetic budget")
    return checked(a * b)


def qadd(a, b):
    if not a:
        return checked(b)
    if not b:
        return checked(a)
    if a.denominator == b.denominator:
        estimate = max(abs(a.numerator).bit_length(), abs(b.numerator).bit_length()) + 1
        denominator = a.denominator.bit_length()
    else:
        estimate = max(abs(a.numerator).bit_length() + b.denominator.bit_length(),
                       abs(b.numerator).bit_length() + a.denominator.bit_length()) + 1
        denominator = a.denominator.bit_length() + b.denominator.bit_length()
    if max(estimate, denominator) > MAX_COEFFICIENT_BITS:
        raise ValueError("addition would exceed 4096-bit arithmetic budget")
    return checked(a + b)


def qsub(a, b):
    return qadd(a, -b)


def qdiv(a, b):
    if not b:
        raise ValueError("zero denominator")
    return qmul(a, checked(Q(b.denominator, b.numerator)))


def opposite(a, b):
    return (a < 0 < b) or (b < 0 < a)


def trim(p):
    p = list(map(bounded_q, p))
    while len(p) > 1 and p[-1] == 0:
        p.pop()
    if len(p) > 3:
        raise ValueError("polynomial degree exceeds two")
    return p


def add(a, b):
    return trim([qadd(a[i] if i < len(a) else Q(0), b[i] if i < len(b) else Q(0))
                 for i in range(max(len(a), len(b)))])


def neg(a):
    return [-v for v in a]


def mul(a, b):
    p = [Q(0)] * (len(a) + len(b) - 1)
    for i, x in enumerate(a):
        for j, y in enumerate(b):
            p[i + j] = qadd(p[i + j], qmul(x, y))
    return trim(p)


def divide(a, b):
    if len(b) != 1 or b[0] == 0:
        raise ValueError("division requires a nonzero constant denominator")
    return [qdiv(v, b[0]) for v in a]


def polynomial(postfix):
    if not isinstance(postfix, str) or not 0 < len(postfix) <= 256:
        raise ValueError("expected 1..256 postfix symbols")
    stack = []
    for c in postfix:
        if c in "123456789":
            stack.append([Q(c)])
        elif c == "x":
            stack.append([Q(0), Q(1)])
        elif c in "nsr":
            a = stack.pop()
            stack.append(neg(a) if c == "n" else mul(a, a) if c == "s" else divide([Q(1)], a))
        elif c in "+-*/":
            b, a = stack.pop(), stack.pop()
            stack.append({"+": add, "-": lambda a, b: add(a, neg(b)), "*": mul, "/": divide}[c](a, b))
        else:
            raise ValueError(f"unsupported symbol {c!r}")
    if len(stack) != 1:
        raise ValueError("invalid postfix stack")
    return trim(stack[0])


def equation_polynomial(lhs, rhs):
    try:
        return add(polynomial(lhs), neg(polynomial(rhs)))
    except IndexError as error:
        raise ValueError("invalid postfix stack") from error


def value(p, x):
    answer = Q(0)
    for coefficient in reversed(p):
        answer = qadd(qmul(answer, x), coefficient)
    return answer


def canonical(p):
    """Scale-independent equation identity, retaining polynomial degree."""
    p = trim(p)
    if p[-1] == 0:
        raise ValueError("identically zero equation")
    return [str(qdiv(v, p[-1])) for v in p]


def verify(certificate):
    """Recompute the claim using exact arithmetic; do not trust stored coefficients."""
    try:
        if certificate["schema"] != "ries-polynomial-root-v1":
            return False
        p = equation_polynomial(certificate["lhs_postfix"], certificate["rhs_postfix"])
        if len(p) < 2 or [str(c) for c in p] != certificate["coefficients"]:
            return False
        target = bounded_q(certificate["target_literal"])
        lo, hi = map(bounded_q, certificate["interval"])
        numerical_root = bounded_q(certificate["numerical_root"])
        if lo > hi or not lo <= numerical_root <= hi:
            return False
        if value(p, numerical_root) != bounded_q(certificate["residual_at_numerical_root"]):
            return False
        if lo == hi:
            valid = value(p, lo) == 0
        else:
            derivative = [qmul(Q(i), p[i]) for i in range(1, len(p))]
            # For degree <= 2 the derivative is linear. Equal nonzero signs at
            # both endpoints prove strict monotonicity throughout the interval.
            valid = (opposite(value(p, lo), value(p, hi)) and
                     not opposite(value(derivative, lo), value(derivative, hi)) and value(derivative, lo) != 0 and value(derivative, hi) != 0)
        return valid and bounded_q(certificate["max_distance_from_decimal"]) == max(abs(qsub(lo, target)), abs(qsub(hi, target)))
    except (KeyError, TypeError, ValueError, IndexError, ZeroDivisionError, OverflowError):
        return False


def certify(row, target_literal):
    try:
        target = bounded_q(target_literal)
        p = equation_polynomial(row["lhs_postfix"], row["rhs_postfix"])
        if len(p) < 2:
            raise ValueError("constant or identically zero equation")
        root = float(row["x_value"])
        if not math.isfinite(root):
            raise ValueError("non-finite numerical root")
        x = bounded_q(root)
        if value(p, x) == 0:
            lo = hi = x
        else:
            radius = Q(math.ulp(root))
            for _ in range(64):
                lo, hi = qsub(x, radius), qadd(x, radius)
                if opposite(value(p, lo), value(p, hi)):
                    break
                radius = qmul(radius, Q(2))
            else:
                raise ValueError("could not bracket the numerical root")
        certificate = {
            "schema": "ries-polynomial-root-v1",
            "lhs_postfix": row["lhs_postfix"], "rhs_postfix": row["rhs_postfix"],
            "coefficients": [str(c) for c in p], "target_literal": target_literal,
            "interval": [str(lo), str(hi)],
            "numerical_root": str(x), "residual_at_numerical_root": str(value(p, x)),
            "max_distance_from_decimal": str(max(abs(qsub(lo, target)), abs(qsub(hi, target)))),
        }
        if not verify(certificate):
            raise ValueError("root is not isolated by a monotone bracket")
        return {"status": "independently_bounded", "certificate": certificate}
    except (ValueError, KeyError, IndexError, ZeroDivisionError, OverflowError) as error:
        return {"status": "unavailable", "reason": str(error)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("--target-literal", help="Original decimal literal, required when producing certificates")
    parser.add_argument("--verify", action="store_true", help="Verify an existing certificate report")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    data = json.loads(args.input.read_text())
    if args.verify:
        certificates = [row["certificate"] for row in data["results"] if row["status"] == "independently_bounded"]
        if not certificates or not all(verify(c) for c in certificates):
            raise SystemExit("certificate verification failed or no bounded results")
        print(f"Verified {len(certificates)} exact rational root certificates")
        return
    if args.target_literal is None:
        parser.error("--target-literal is required")
    bounded_q(args.target_literal)  # Reject oversized arithmetic before conversion.
    report = {"schema": "ries-certification-report-v1", "target_literal": args.target_literal,
              "claim": "Unique equation root in a rational interval; no claim of decimal equality or formal proof",
              "results": [certify(row, args.target_literal) for row in data["results"]]}
    text = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.write_text(text)
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
