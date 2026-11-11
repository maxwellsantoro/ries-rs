#!/usr/bin/env python3
"""Compare the two LHS policies in paired, deterministic release searches."""

import argparse
import json
import math
from pathlib import Path
import statistics
import subprocess
import tempfile

from capture_search_benchmark import REPO_DIR, collect_environment, run_command


POLICIES = ("preserve", "dedup")
METRICS = (
    "expressions_generated_lhs",
    "expressions_generated_rhs",
    "candidate_pairs_tested",
    "newton_calls",
    "generation_ms",
    "search_ms",
    "elapsed_ms",
    "peak_memory_bytes",
)


def positive_int(value):
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("expected a positive integer")
    return number


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, help="Target literal passed to ries-rs")
    parser.add_argument("--level", default="2", help="CLI search level (default 2)")
    parser.add_argument("--ranking", choices=("complexity", "parity"), default="complexity")
    parser.add_argument("--max-match-distance", help="Maximum absolute root error")
    parser.add_argument("--exclude", help="Symbol codes to exclude from both runs")
    parser.add_argument("--max-matches", type=positive_int, default=16)
    parser.add_argument("--repeats", type=positive_int, default=5, help="Measured pairs after warm-up")
    parser.add_argument("--output", type=Path, help="Write JSON to this path instead of stdout")
    return parser.parse_args()


def run_search(command, dedup):
    completed = run_command(command, cwd=REPO_DIR)
    result = json.loads(completed.stdout)
    matcher = result.get("matcher")
    if matcher != "serial" or result.get("lhs_value_dedup_applied") is not dedup:
        raise ValueError(
            f"Expected a materialized serial search with LHS deduplication={dedup}, "
            f"got matcher={matcher!r}, lhs_value_dedup_applied="
            f"{result.get('lhs_value_dedup_applied')!r}. Lower the level or exclude symbols."
        )
    return result


def summarize(samples):
    rows = samples[0]["results"]
    if any(sample["results"] != rows for sample in samples[1:]):
        raise ValueError("Ranked results changed between repeats of the same policy")

    medians = {}
    for metric in METRICS:
        values = [sample["search_stats"][metric] for sample in samples]
        medians[metric] = None if None in values else statistics.median(values)
    return medians


def compare_rows(preserved, deduplicated):
    def equations(rows):
        return [(row["lhs_postfix"], row["rhs_postfix"]) for row in rows]

    def exact_equations(rows):
        return set(equations(row for row in rows if row["exact"]))

    preserve_order = equations(preserved)
    dedup_order = equations(deduplicated)
    root_changed = None
    if preserved and deduplicated:
        a, b = preserved[0]["x_value"], deduplicated[0]["x_value"]
        root_changed = abs(a - b) > 8 * max(math.ulp(a), math.ulp(b))
    return {
        "rank_one_equation_changed": preserve_order[:1] != dedup_order[:1],
        "rank_one_root_changed_by_more_than_eight_ulps": root_changed,
        "ordered_equations_changed": preserve_order != dedup_order,
        "preserved_exact_equations_missing_from_dedup": sorted(
            exact_equations(preserved) - exact_equations(deduplicated)
        ),
    }


def capture(args, profile):
    command = [
        "cargo", "run", "--release", "--quiet", "--locked", "--",
        "--deterministic", "--report=false", "--json", f"--profile={profile}",
        f"--level={args.level}", f"--max-matches={args.max_matches}",
        f"--{args.ranking}-ranking",
    ]
    if args.max_match_distance is not None:
        command.append(f"--max-match-distance={args.max_match_distance}")
    if args.exclude is not None:
        command.append(f"--exclude={args.exclude}")
    target = ["--", args.target]
    commands = {
        "preserve": [*command, *target],
        "dedup": [*command, "--lhs-value-dedup", *target],
    }
    samples = {policy: [] for policy in POLICIES}
    for pair in range(args.repeats + 1):
        order = reversed(POLICIES) if pair % 2 else POLICIES
        for policy in order:
            result = run_search(commands[policy], dedup=policy == "dedup")
            if pair:
                samples[policy].append(result)

    rhs_counts = {
        result["search_stats"]["expressions_generated_rhs"]
        for runs in samples.values()
        for result in runs
    }
    if len(rhs_counts) != 1:
        raise ValueError("The RHS expression count changed between runs")

    return {
        "environment": collect_environment("LHS value deduplication comparison"),
        "profile_contents": "",
        "comparison": compare_rows(samples["preserve"][0]["results"], samples["dedup"][0]["results"]),
        "policies": {
            policy: {
                "command": commands[policy],
                "medians": summarize(samples[policy]),
                "samples": samples[policy],
            }
            for policy in POLICIES
        },
    }


def main():
    args = parse_args()
    try:
        with tempfile.TemporaryDirectory() as directory:
            profile = Path(directory) / "empty.ries"
            profile.touch()
            data = capture(args, profile)
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.stderr.strip() or str(error)) from error
    except ValueError as error:
        raise SystemExit(str(error)) from error

    output = json.dumps(data, indent=2) + "\n"
    if args.output is None:
        print(output, end="")
    else:
        args.output.write_text(output, encoding="utf-8")


if __name__ == "__main__":
    main()
