#!/usr/bin/env python3
"""Run a small independent discovery-quality corpus; no confidence/completeness claim.

One deterministic run per case is the CI default. Repeated process timings are
optional measurements on this environment, not portable performance claims.
"""

import argparse
from fractions import Fraction
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import time

from verify_discovery import digest, rows, selected_evidence, validate_run


ROOT = Path(__file__).resolve().parents[1]


def validate_corpus(corpus):
    if not isinstance(corpus, dict) or corpus.get("schema") != "ries-discovery-corpus-v1":
        raise ValueError("expected ries-discovery-corpus-v1")
    cases = corpus.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ValueError("corpus needs nonempty cases")
    names = set()
    for case in cases:
        name = case.get("name")
        if not isinstance(name, str) or not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,79}", name) or name in names:
            raise ValueError("case names must be unique safe artifact names")
        names.add(name)
        if case.get("kind") not in {"formula", "named_constant", "near_miss", "control"}:
            raise ValueError(f"{name}: unknown case kind")
        if not isinstance(case.get("target_literal"), str):
            raise ValueError(f"{name}: missing target literal")
        options = case.get("options", {})
        if set(options) - {"level", "max_matches", "preset"}:
            raise ValueError(f"{name}: unknown search option")
        for key, default, low, high in (("level", 3, 0, 5), ("max_matches", 8, 1, 1000)):
            value = options.get(key, default)
            if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
                raise ValueError(f"{name}: invalid {key}")
        if case["kind"] == "formula" and not case.get("expected_solved_postfix"):
            raise ValueError(f"{name}: formula needs independent expected spellings")
        if case["kind"] == "named_constant" and not case.get("expected_constant"):
            raise ValueError(f"{name}: named constant identity is required")
        if case["kind"] in {"formula", "named_constant"}:
            limit = case.get("required_top_k", 3)
            if isinstance(limit, bool) or not isinstance(limit, int) or not 1 <= limit <= options.get("max_matches", 8):
                raise ValueError(f"{name}: invalid required_top_k")
    return corpus


def expected_rank(case, run):
    if case["kind"] not in {"formula", "named_constant"}:
        return None
    expected = case.get("expected_solved_postfix", [])
    if case["kind"] == "named_constant":
        slots = [chr(128 + index) for index, (name, _) in enumerate(run["config"]["constants"])
                 if name == case["expected_constant"]]
        expected = slots
    for index, hypothesis in enumerate(run["hypotheses"]):
        tolerance = run["config"]["input_tolerance"]
        if (hypothesis["within_input_precision"] and
                abs(hypothesis["representative"]["error"]) <= tolerance and
                any(row.get("solve_for_x_postfix") in expected and abs(row["error"]) <= tolerance
                    for row in rows(hypothesis))):
            return index + 1
    return None


def measure(case, run):
    validate_run(run)
    issues = []
    if run["target_literal"] != case["target_literal"]:
        issues.append("original target literal was not preserved")
    for key, default in (("level", 3), ("max_matches", 8), ("preset", None)):
        if run["config"].get(key) != case.get("options", {}).get(key, default):
            issues.append(f"resolved {key} differs from the requested case")
    hypotheses = run["hypotheses"]
    keys = [hypothesis["key"] for hypothesis in hypotheses]
    displayed = [hypothesis["representative"].get("solve_for_x_postfix") or
                 (hypothesis["representative"]["lhs_postfix"], hypothesis["representative"]["rhs_postfix"])
                 for hypothesis in hypotheses]
    duplicates = len(keys) - len(set(keys))
    if duplicates:
        issues.append("duplicate hypothesis keys consume result slots")
    displayed_duplicates = len(displayed) - len(set(displayed))
    if displayed_duplicates:
        issues.append("duplicate solved presentations consume result slots")
    if len(hypotheses) > run["config"]["max_matches"]:
        issues.append("result exceeds requested hypothesis limit")
    compatible = sum(abs(hypothesis["representative"]["error"]) <= run["config"]["input_tolerance"]
                     for hypothesis in hypotheses)
    if any(hypothesis["within_input_precision"] !=
           (abs(hypothesis["representative"]["error"]) <= run["config"]["input_tolerance"])
           for hypothesis in hypotheses):
        issues.append("reported input-precision agreement disagrees with root errors")
    equation_ids = [(row["lhs_postfix"], row["rhs_postfix"]) for h in hypotheses for row in rows(h)]
    rank = expected_rank(case, run)
    known = case["kind"] in {"formula", "named_constant"}
    if known and (rank is None or rank > case.get("required_top_k", 3)):
        issues.append("required known answer is missing from the accepted top-k")
    observation = None
    if case["kind"] == "near_miss":
        reference = case["reference_solved_postfix"]
        ranks = [index + 1 for index, h in enumerate(hypotheses)
                 if any(row.get("solve_for_x_postfix") == reference for row in rows(h))]
        observation = {"reference_rank": ranks[0] if ranks else None,
                       "reference_value": case["reference_value"],
                       "distance_from_input": str(abs(Fraction(case["reference_value"]) -
                                                      Fraction(case["target_literal"]))),
                       "claim": "Reference proximity is observed; other returned formulas are not labeled false positives."}
    return {"passed": not issues, "issues": issues, "kind": case["kind"],
            "known_answer_rank": rank, "top1_recovered": rank == 1 if known else None,
            "top3_recovered": rank is not None and rank <= 3 if known else None,
            "topk_recovered": rank is not None and rank <= case.get("required_top_k", 3) if known else None,
            "unique_hypotheses": len(set(keys)), "returned_hypotheses": len(keys),
            "duplicate_hypothesis_count": duplicates,
            "duplicate_hypothesis_rate": duplicates / len(keys) if keys else 0.0,
            "duplicate_displayed_hypotheses": displayed_duplicates,
            "duplicate_displayed_rate": displayed_duplicates / len(displayed) if displayed else 0.0,
            "retained_equation_count": len(equation_ids),
            "distinct_equation_count": len(set(equation_ids)),
            "duplicate_equation_rate": (len(equation_ids) - len(set(equation_ids))) / len(equation_ids) if equation_ids else 0.0,
            "supporting_variant_count": sum(len(h["supporting_equations"]) for h in hypotheses),
            "precision_compatible_hypotheses": compatible, "near_miss_observation": observation}


def source_receipt(binary):
    git = lambda *args: subprocess.check_output(["git", *args], cwd=ROOT)
    names = git("ls-files", "--cached", "--others", "--exclude-standard", "-z").split(b"\0")
    hashes = {}
    for name in sorted(set(os.fsdecode(name) for name in names if name)):
        path = ROOT / name
        if path.is_file() and (name.startswith(("src/", "scripts/", "tools/", "tests/")) or name in {"Cargo.toml", "Cargo.lock"}):
            hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "commit": git("rev-parse", "HEAD").decode().strip(),
            "git_tree": git("rev-parse", "HEAD^{tree}").decode().strip(),
            "working_tree_clean": not bool(git("status", "--porcelain")),
            "working_diff_sha256": hashlib.sha256(git("diff", "HEAD")).hexdigest(),
            "source_sha256": hashes,
            "source_snapshot_scope": "Execution-time source snapshot including untracked inputs; binary hash identifies the executed artifact. Clean release provenance is a separate gate.",
            "platform": platform.uname()._asdict(), "python": platform.python_version(),
            "cpu_count": os.cpu_count(),
            "rustc": subprocess.check_output(["rustc", "--version", "--verbose"], text=True).strip()}


def qualify(binary, corpus, output, repeats=1, timeout=60):
    validate_corpus(corpus)
    output.mkdir(parents=True, exist_ok=True)
    report = {"schema": "ries-discovery-qualification-v1", "scope": corpus.get("scope"),
              "corpus": corpus, "corpus_sha256": digest(corpus), "repeats": repeats,
              "environment": source_receipt(binary), "runs": [], "passed": True}
    for case in corpus["cases"]:
        ordered_keys = None
        for repeat in range(repeats):
            stem = f"{case['name']}-{repeat}"
            options = case.get("options", {})
            command = [str(binary), "--discover", "--json", "--deterministic",
                       "-l", str(options.get("level", 3)), "-n", str(options.get("max_matches", 8))]
            if options.get("preset"):
                command.extend(["--preset", options["preset"]])
            command.extend(["--", case["target_literal"]])
            start = time.perf_counter()
            entry = {"case": case["name"], "kind": case["kind"], "repeat": repeat, "command": command}
            try:
                result = subprocess.run(command, capture_output=True, text=True, timeout=timeout)
                entry["wall_ms"] = (time.perf_counter() - start) * 1000
                (output / f"{stem}.json").write_text(result.stdout)
                (output / f"{stem}.stderr").write_text(result.stderr)
                if result.returncode:
                    raise ValueError(f"discovery process exited {result.returncode}; inspect {stem}.stderr")
                run = json.loads(result.stdout)
                entry.update(measure(case, run))
                entry["resolved_config"] = run["config"]
                entry["engine_stats"] = run["stats"]
                keys = [hypothesis["key"] for hypothesis in run["hypotheses"]]
                if ordered_keys is None:
                    ordered_keys = keys
                elif keys != ordered_keys:
                    entry["issues"].append("deterministic repeat changed ordered hypothesis keys")
                    entry["passed"] = False
                if run["hypotheses"]:
                    evidence = selected_evidence(run, 0)
                    (output / f"{stem}-selected-evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
                    entry["first_hypothesis_polynomial_evidence"] = evidence["evidence"]["status"]
            except (OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
                entry.update({"passed": False, "issues": [str(error)],
                              "wall_ms": (time.perf_counter() - start) * 1000})
                (output / f"{stem}-failure.json").write_text(json.dumps(entry, indent=2) + "\n")
            report["runs"].append(entry)
            report["passed"] &= entry["passed"]
    if hashlib.sha256(binary.read_bytes()).hexdigest() != report["environment"]["binary_sha256"]:
        report["passed"] = False
        report["binary_changed_during_run"] = True
    report["summaries"] = []
    for kind in ("formula", "named_constant", "near_miss", "control"):
        entries = [entry for entry in report["runs"] if entry["kind"] == kind]
        if entries:
            known = kind in {"formula", "named_constant"}
            report["summaries"].append({"kind": kind, "runs": len(entries),
                "known_answer_metrics_applicable": known,
                "passed_runs": sum(entry["passed"] for entry in entries),
                "top1_recovered_runs": sum(entry.get("top1_recovered") is True for entry in entries) if known else None,
                "top3_recovered_runs": sum(entry.get("top3_recovered") is True for entry in entries) if known else None,
                "topk_recovered_runs": sum(entry.get("topk_recovered") is True for entry in entries) if known else None,
                "median_wall_ms": statistics.median(entry["wall_ms"] for entry in entries)})
    families = {}
    for case in corpus["cases"]:
        if case.get("expected_solved_postfix"):
            families.setdefault(tuple(case["expected_solved_postfix"]), []).append(case)
    report["precision_series"] = []
    for expected, cases in families.items():
        if len(cases) > 1:
            names = {case["name"] for case in cases}
            report["precision_series"].append({"expected_solved_postfix": expected,
                "observations": [{"case": entry["case"],
                                  "input_tolerance": entry.get("resolved_config", {}).get("input_tolerance"),
                                  "known_answer_rank": entry.get("known_answer_rank"),
                                  "unique_hypotheses": entry.get("unique_hypotheses"),
                                  "precision_compatible_hypotheses": entry.get("precision_compatible_hypotheses")}
                                 for entry in report["runs"] if entry["case"] in names]})
    (output / "discovery-qualification.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/ries-rs")
    parser.add_argument("--corpus", type=Path, default=ROOT / "tests/corpus/discovery-quality.json")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    if args.repeats < 1 or not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("repeats and timeout must be positive")
    try:
        report = qualify(args.binary.resolve(), json.loads(args.corpus.read_text()), args.output,
                         args.repeats, args.timeout)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Discovery qualification failed: {error}\n")
    print(f"{len(report['runs'])} discovery runs; passed={report['passed']}; evidence: {args.output}")
    raise SystemExit(0 if report["passed"] else 1)


if __name__ == "__main__":
    main()
