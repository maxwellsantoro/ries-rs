#!/usr/bin/env python3
"""Select CI lanes conservatively; release and non-PR runs always use all lanes.

No persistent test-result cache is involved. A pull request skips a lane only
when every changed path is understood. Git failures and unknown paths select
the full matrix. Use the same classifier for CI and coverage.
"""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess


LANES = ("rust", "python", "features", "wasm", "qualification", "web", "consumers", "audit")
JOB_LANES = {
    "fmt": "rust", "clippy": "rust", "check-features": "features",
    "test-ries-py-rust": "python", "test-ries-py-python": "python",
    "test": "rust", "test-highprec": "rust", "test-wasm": "wasm",
    "audit": "audit", "parity-check": "qualification", "test-web": "web",
    "test-installed-consumers": "consumers",
}


def classify(paths):
    selected = set()
    for path in paths:
        if path.startswith("tests/corpus/") and path not in {
            "tests/corpus/search-contract.json", "tests/corpus/discovery-quality.json",
        }:
            # A new fixture may be consumed by Rust or a binding, so its
            # dependencies need an explicit mapping before narrowing checks.
            return {lane: True for lane in LANES}
        suffix = Path(path).suffix.lower()
        # Preserve the cheap prose path without silently exempting future
        # executable tooling merely because it lives below docs/.
        if (suffix == ".md"
                or path.startswith(("docs/archive/", "docs/benchmarks/artifacts/"))
                or (path.startswith("docs/") and suffix in {".rst", ".txt", ".pdf"})
                or (path.startswith("docs/assets/") and suffix in {
                    ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".avif", ".ico",
                })):
            continue
        if (path.startswith("web/") and suffix in {
                ".html", ".js", ".css", ".json", ".woff", ".woff2", ".ttf", ".otf",
                ".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".map", ".txt",
        }) or path in {"playwright.config.ts", "tests/web-smoke.spec.ts"}:
            selected.add("web")
        elif path in {
            "package.json", "package-lock.json", "tests/wasm-package.mjs",
            "scripts/build_web_site.sh", "scripts/build_web_threads.sh",
        }:
            selected.update(("web", "consumers"))
        elif path.startswith("ries-py/") or path in {
            "scripts/test_ries_py_rust.sh", "scripts/test_ries_py_python.sh",
            "scripts/check_cas_exports.py",
        }:
            selected.update(("python", "features", "consumers"))
            if path.endswith(("Cargo.toml", "Cargo.lock")):
                selected.add("audit")
        elif path == "scripts/test_release_consumers.py":
            selected.add("consumers")
        elif path in {
            "tests/corpus/search-contract.json",
            "tests/corpus/discovery-quality.json",
            "scripts/qualify_discovery.py", "scripts/verify_discovery.py",
            "scripts/qualify_search.py", "scripts/compare_original.py",
            "scripts/algebraic_certificate.py", "scripts/check_decimal_boundary.py",
            "scripts/run_bounded.py", "tests/compare_with_original.sh", "tests/validate_parity.py",
        }:
            selected.add("qualification")
        elif path in {
            "scripts/check_release_integrity.py", "scripts/check_docs.py",
            "tests/test_ci_validation.py", "tests/test_docs_check.py", "tests/test_review_tools.py",
            "tests/test_discovery_tools.py",
        }:
            # Release integrity and Python tool tests run unconditionally.
            continue
        else:
            # Includes core Rust, CI configuration, dependency manifests, and
            # new files whose dependency surface has not yet been classified.
            return {lane: True for lane in LANES}
    return {lane: lane in selected for lane in LANES}


def changed_paths(base, head, cwd=None):
    for revision in (base, head):
        if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-fA-F]{40,64}", revision):
            raise ValueError("PR base and head must be full commit hashes")
    ancestor = subprocess.check_output(
        ["git", "merge-base", base, head], cwd=cwd, text=True,
    ).strip()
    # Treat renames as deletion + addition: moving source into docs must still
    # validate removal of the source. NUL separation preserves unusual names.
    output = subprocess.check_output(
        ["git", "diff", "--name-only", "--no-renames", "-z", ancestor, head, "--"], cwd=cwd,
    )
    return [os.fsdecode(path) for path in output.split(b"\0") if path]


def determine_scope(event_name, event_path):
    if event_name != "pull_request":
        return {lane: True for lane in LANES}, "Full validation for a non-PR event"
    try:
        event = json.loads(Path(event_path).read_text())
        pr = event["pull_request"]
        paths = changed_paths(pr["base"]["sha"], pr["head"]["sha"])
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        return {lane: True for lane in LANES}, f"Full validation: diff unavailable ({type(error).__name__})"
    return classify(paths), f"Classified {len(paths)} changed paths against the PR merge base"


def validate_results(needs):
    """Require selected lanes to succeed; skipping a selected job is a failure."""
    for name in ("scope", "release-integrity"):
        if needs.get(name, {}).get("result") != "success":
            raise ValueError(f"{name} did not succeed")
    lanes = needs["scope"].get("outputs", {})
    if any(lanes.get(lane) not in ("true", "false") for lane in LANES):
        raise ValueError("scope outputs are missing or invalid")
    for job, lane in JOB_LANES.items():
        result = needs.get(job, {}).get("result")
        allowed = ("success",) if lanes[lane] == "true" else ("success", "skipped")
        if result not in allowed:
            raise ValueError(f"{job}: unexpected {result!r} for selected={lanes[lane]}")


def validate_coverage_results(needs):
    """Make coverage's required status honor the same conservative skip rule."""
    if needs.get("scope", {}).get("result") != "success":
        raise ValueError("scope did not succeed")
    selected = needs["scope"].get("outputs", {}).get("rust")
    if selected not in ("true", "false"):
        raise ValueError("scope rust output is missing or invalid")
    result = needs.get("coverage", {}).get("result")
    allowed = ("success",) if selected == "true" else ("success", "skipped")
    if result not in allowed:
        raise ValueError(f"coverage: unexpected {result!r} for selected={selected}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--event-name", default=os.environ.get("GITHUB_EVENT_NAME", "local"))
    parser.add_argument("--event-path", default=os.environ.get("GITHUB_EVENT_PATH", ""))
    parser.add_argument("--github-output", default=os.environ.get("GITHUB_OUTPUT"))
    checks = parser.add_mutually_exclusive_group()
    checks.add_argument("--check-results", action="store_true",
                        help="Validate the aggregate job results from CI_NEEDS JSON")
    checks.add_argument("--check-coverage-results", action="store_true",
                        help="Validate coverage job results from CI_NEEDS JSON")
    args = parser.parse_args()
    if args.check_results:
        validate_results(json.loads(os.environ["CI_NEEDS"]))
        print("All selected CI jobs succeeded; unselected jobs may be skipped.")
        return
    if args.check_coverage_results:
        validate_coverage_results(json.loads(os.environ["CI_NEEDS"]))
        print("Coverage succeeded or was intentionally unselected.")
        return
    scope, reason = determine_scope(args.event_name, args.event_path)
    print(json.dumps({"reason": reason, "lanes": scope}, sort_keys=True))
    if args.github_output:
        with open(args.github_output, "a") as output:
            for lane, enabled in scope.items():
                output.write(f"{lane}={str(enabled).lower()}\n")


if __name__ == "__main__":
    main()
