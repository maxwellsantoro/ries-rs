#!/usr/bin/env python3
"""Fixed-corpus matcher differential, independent polynomial checks and resources.

Raw JSON and manifests are retained. A pass is evidence for this corpus only.
Timing includes process startup; peak RSS is the CLI's process measurement.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import statistics
import time

from algebraic_certificate import canonical, certify, equation_polynomial

ROOT = Path(__file__).resolve().parents[1]
MODES = {"serial": ["--deterministic"], "streaming": ["--streaming", "--deterministic"],
         "parallel": ["--parallel"], "turbo": ["--turbo"]}


def identity(row):
    return (row["lhs_postfix"], row["rhs_postfix"], row["complexity"])


def compare(reference, actual):
    expected = [identity(r) for r in reference]
    found = [identity(r) for r in actual]
    return {"ordered_equations_equal": expected == found,
            "missing_equations": sorted(set(expected) - set(found)),
            "extra_equations": sorted(set(found) - set(expected)),
            "classifications_equal": len(reference) == len(actual) and all(a.get("exact") == b.get("exact") for a, b in zip(reference, actual)),
            "roots_equal": len(reference) == len(actual) and all(
                a["x_value"] == b["x_value"] and a["error"] == b["error"]
                for a, b in zip(reference, actual))}


def qualify(binary, corpus, output, repeats=1):
    output.mkdir(parents=True, exist_ok=True)
    profile = output / "empty.ries"
    profile.write_text("")
    git = lambda *args: subprocess.check_output(["git", *args], cwd=ROOT)
    files = git("ls-files", "--cached", "--others", "--exclude-standard", "-z").decode().split("\0")
    source_hashes = {}
    for name in sorted(set(files)):
        if name and (name.startswith(("src/", "scripts/", "tools/", "ries-py/src/", "tests/", "web/", ".github/workflows/")) or
                     name in ("Cargo.toml", "Cargo.lock", "package.json", "package-lock.json", "tailwind.config.cjs", "ries-py/Cargo.toml", "ries-py/Cargo.lock", "ries-py/pyproject.toml")):
            path = ROOT / name
            if path.is_file():
                source_hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    report = {"schema": "ries-qualification-v1", "corpus": corpus,
              "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "commit": git("rev-parse", "HEAD").decode().strip(),
              "git_tree": git("rev-parse", "HEAD^{tree}").decode().strip(),
              "working_tree_clean": not bool(git("status", "--porcelain")),
              "working_diff_sha256": hashlib.sha256(git("diff", "HEAD")).hexdigest(),
              "source_sha256": source_hashes, "platform": platform.platform(), "repeats": repeats, "runs": [], "passed": True}
    for case in corpus["cases"]:
        reference = None
        for mode, options in MODES.items():
            if mode not in case.get("modes", MODES):
                continue
            for repeat in range(repeats):
                stem = f'{case["name"]}-{mode}-{repeat}'
                manifest = output / f"{stem}-manifest.json"
                command = [str(binary), case["target"], "--json", "--strict-config", "--report=false",
                           "-n8", f"--profile={profile.resolve()}", "--complexity-ranking",
                           *case["args"], *options, "--emit-manifest", str(manifest.resolve())]
                if "--parity-ranking" in case["args"]:
                    command.remove("--complexity-ranking")
                start = time.perf_counter()
                result = subprocess.run(command, capture_output=True, text=True, timeout=180)
                wall_ms = (time.perf_counter() - start) * 1000
                (output / f"{stem}.json").write_text(result.stdout)
                (output / f"{stem}.stderr").write_text(result.stderr)
                if result.returncode:
                    raise RuntimeError(f"{stem} failed: {result.stderr}")
                data = json.loads(result.stdout)
                rows = data["results"]
                if reference is None:
                    reference = rows
                comparison = compare(reference, rows)
                certificates = [certify(row, case["target"]) for row in rows]
                known_answer_found = None
                if "expected_polynomial" in case:
                    known_answer_found = any(
                        c["status"] == "independently_bounded" and
                        canonical(equation_polynomial(r["lhs_postfix"], r["rhs_postfix"])) == case["expected_polynomial"]
                        for r, c in zip(rows, certificates))
                first_ok = "expected_first" not in case or bool(rows and rows[0]["equation"] == case["expected_first"])
                grammar = next((arg[2:] for arg in case["args"] if arg.startswith("-S")), None)
                grammar_ok = grammar is None or all(set(r["lhs_postfix"] + r["rhs_postfix"]) <= set(grammar) for r in rows)
                manifest_data = json.loads(manifest.read_text())
                weights = manifest_data["config"]["replay"]["generation"]["symbol_table"]
                bounds_ok = all(sum(weights[str(ord(c))]["weight"] for c in row[f"{side}_postfix"]) <= data[f"max_{side}_complexity"] for row in rows for side in ("lhs", "rhs"))
                passed = (bounds_ok and comparison["ordered_equations_equal"] and comparison["roots_equal"] and comparison["classifications_equal"] and
                          known_answer_found is not False and first_ok and grammar_ok)
                report["passed"] &= passed
                stats = data["search_stats"]
                report["runs"].append({"case": case["name"], "requested_mode": mode,
                    "effective_matcher": data["matcher"], "repeat": repeat, "passed": passed,
                    "command": command,
                    "comparison": comparison, "known_answer_found": known_answer_found,
                    "first_result_valid": first_ok, "grammar_valid": grammar_ok, "complexity_bounds_valid": bounds_ok,
                    "wall_ms": wall_ms, "stats": stats, "certification": certificates})
    report["summaries"] = []
    for case in corpus["cases"]:
        for mode in case.get("modes", MODES):
            runs = [r for r in report["runs"] if r["case"] == case["name"] and r["requested_mode"] == mode]
            report["summaries"].append({"case": case["name"], "requested_mode": mode,
                "effective_matchers": sorted(set(r["effective_matcher"] for r in runs)),
                "median_wall_ms": statistics.median(r["wall_ms"] for r in runs),
                "median_peak_rss_bytes": statistics.median(r["stats"]["peak_memory_bytes"] for r in runs) if all(r["stats"]["peak_memory_bytes"] is not None for r in runs) else None,
                "missing_known_answer_runs": sum(r["known_answer_found"] is False for r in runs),
                "independently_bounded_results": sum(c["status"] == "independently_bounded" for r in runs for c in r["certification"]),
                "unavailable_certifications": sum(c["status"] == "unavailable" for r in runs for c in r["certification"])})
    (output / "qualification.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/ries-rs")
    parser.add_argument("--corpus", type=Path, default=ROOT / "tests/corpus/search-contract.json")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=1)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    report = qualify(args.binary.resolve(), json.loads(args.corpus.read_text()), args.output, args.repeats)
    print(f'{len(report["runs"])} measured runs; passed={report["passed"]}; evidence: {args.output}')
    raise SystemExit(0 if report["passed"] else 1)


if __name__ == "__main__":
    main()
