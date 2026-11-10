#!/usr/bin/env python3
"""Require each solver to report an equation within the root-distance limit."""
import math
import re
import sys
from pathlib import Path

# Only equation result lines count; discard banners, statistics and symbol legends.
MATCH = re.compile(
    r"^\s*\S.*\s=\s+\S.*\s+for x = T\s*([+-])\s*"
    r"(\d+(?:\.\d*)?(?:[eE][+-]?\d+)?)\s*\{\d+\}\s*$"
)

EXACT = re.compile(r"^\s*\S.*\s=\s+\S.*\s+\('exact' match\)\s*\{\d+\}\s*$")


def qualifies(path: str, limit: float) -> bool:
    for line in Path(path).read_text().splitlines():
        if EXACT.match(line):
            return True
        match = MATCH.match(line)
        if match:
            delta = float(match.group(2))
            if math.isfinite(delta) and delta <= limit:
                return True
    return False


def main(limit: float, original: str, rust: str) -> int:
    for label, path in (("original RIES", original), ("ries-rs", rust)):
        if not qualifies(path, limit):
            print(f"{label}: no equation with |root - target| <= {limit:g}", file=sys.stderr)
            return 1
    print(f"Both solvers reported an equation with |root - target| <= {limit:g}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit("usage: validate_parity.py MAX_DISTANCE ORIGINAL_OUTPUT RUST_OUTPUT")
    sys.exit(main(float(sys.argv[1]), sys.argv[2], sys.argv[3]))
