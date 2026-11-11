#!/usr/bin/env python3
"""
RIES-RS Manifest Replay Tool
----------------------------
Replays a self-contained JSON manifest from `ries-rs --emit-manifest` and
compares the recorded settings and results with a new run.

Usage:
  python3 tools/replay_manifest.py <path/to/manifest.json> [--bin <path/to/ries-rs>]
"""

import argparse
import json
import math
import os
import shlex
import subprocess
import sys
import struct
import tempfile
from typing import Dict, Any, List


def parse_args():
    parser = argparse.ArgumentParser(description="Replay a RIES-RS manifest")
    parser.add_argument("manifest", help="Path to the manifest.json file")
    parser.add_argument(
        "--bin",
        default="cargo run --release --",
        help="Path or command to the ries-rs binary (default: 'cargo run --release --')"
    )
    parser.add_argument("--comparison", choices=("exact", "tolerant"), default="exact",
                        help="exact requires identical f64 bits (default)")
    parser.add_argument("--abs-tolerance", type=float,
                        help="explicit absolute tolerance, required with --comparison=tolerant")
    args = parser.parse_args()
    if args.comparison == "tolerant":
        if args.abs_tolerance is None or not math.isfinite(args.abs_tolerance) or args.abs_tolerance < 0:
            parser.error("tolerant comparison requires a finite nonnegative --abs-tolerance")
    elif args.abs_tolerance is not None:
        parser.error("--abs-tolerance requires --comparison=tolerant")
    return args


# Each token has exactly one option; values use --name=value. Never accept file
# paths, output switches, positional arguments after the target, or unknown flags.
VALUE_OPTIONS = set("""level max-matches report format top-k newton-iterations
max-match-distance exclude enable only-symbols op-limits S-RHS N-RHS E-RHS O-RHS
symbol-weights symbol-names preset stop-below min-match-distance precision
zero-threshold canon-reduction derivative-margin max-equate-value min-equate-value
max-memory min-memory memory-abort-threshold max-trig-cycles significance-loss-margin
trig-argument-scale user-constant define""".split())
FLAG_OPTIONS = set("""absolute solve no-solve-for-x algebraic constructible rational
integer ie re liouvillian-subexpressions parallel turbo deterministic streaming lhs-value-dedup
adaptive classic parity-ranking complexity-ranking no-stable stability-check
stability-thorough stop-at-exact one-sided no-refinement any-exponents
any-subexpressions any-trig-args canon-simplify explicit-multiply match-all-digits
no-canon-simplify numeric-anagram rational-exponents rational-trig-args strict-config allow-unverified""".split())


def build_command_args(config: Dict[str, Any]) -> List[str]:
    """Validate recorded arguments, or reconstruct a basic legacy invocation."""
    replay = config.get("replay")
    if replay is not None and not isinstance(replay, dict):
        raise ValueError("invalid replay settings")
    if replay is not None and "cli_args" in replay:
        recorded = replay.get("cli_args")
        if not isinstance(recorded, list) or not recorded:
            raise ValueError("manifest has no self-contained CLI replay arguments; profile, legacy, or older settings cannot be replayed safely")
        if not all(isinstance(token, str) for token in recorded):
            raise ValueError("invalid CLI replay arguments")
        try:
            target = float(recorded[0])
        except ValueError as exc:
            raise ValueError("invalid replay target") from exc
        if not math.isfinite(target) or struct.pack(">d", target) != struct.pack(">d", config["target"]):
            raise ValueError("replay target does not match manifest")
        if config.get("target_literal") is not None and recorded[0] != config["target_literal"]:
            raise ValueError("replay target spelling does not match original decimal literal")
        for token in recorded[1:]:
            if not token.startswith("--"):
                raise ValueError("invalid replay option")
            name, separator, _ = token[2:].partition("=")
            if not ((separator and name in VALUE_OPTIONS) or
                    (not separator and name in FLAG_OPTIONS)):
                raise ValueError(f"unsupported or unsafe replay option: {name}")
        return recorded

    if replay is None:
        print("Warning: legacy manifest lacks resolved replay settings; only recorded summary fields and results can be checked", file=sys.stderr)

    # Old manifests lack the detailed settings needed for custom runs. The
    # replayed configuration is checked below before comparing results.
    if config.get("user_constants"):
        raise ValueError("legacy manifest contains user constants without their weights")
    required = ("target", "level", "max_matches", "max_error", "ranking_mode", "deterministic")
    if any(key not in config for key in required):
        raise ValueError("legacy manifest lacks required search settings")
    result = [str(config["target"]), "--level=" + str(config["level"]),
              "--max-matches=" + str(config["max_matches"]),
              "--max-match-distance=" + str(config["max_error"])]
    ranking = config["ranking_mode"]
    if ranking not in ("parity", "complexity"):
        raise ValueError("unsupported legacy ranking mode")
    result.append("--" + ranking + "-ranking")
    if config["deterministic"]:
        result.append("--deterministic")
    if config.get("allowed_symbols") is not None:
        result.append("--only-symbols=" + "".join(config["allowed_symbols"]))
    if config.get("excluded_symbols"):
        result.append("--exclude=" + "".join(config["excluded_symbols"]))
    if replay is not None:
        output = replay.get("output", {})
        if not isinstance(output, dict):
            raise ValueError("invalid replay output settings")
        if isinstance(output.get("report"), bool):
            result.append("--report=" + str(output["report"]).lower())
        if type(output.get("top_k")) is int:
            result.append("--top-k=" + str(output["top_k"]))
    return result


def check_parity(original_results: List[Dict], replayed_results: List[Dict], *, comparison="exact", abs_tolerance=None) -> bool:
    """Exact f64 replay by default; tolerant comparison is explicit and reported."""
    if comparison not in ("exact", "tolerant"):
        raise ValueError("unknown comparison mode")
    if comparison == "tolerant" and (abs_tolerance is None or not math.isfinite(abs_tolerance) or abs_tolerance < 0):
        raise ValueError("tolerant comparison requires a finite nonnegative tolerance")
    if len(original_results) != len(replayed_results):
        print(f"Match count mismatch! Original: {len(original_results)}, Replayed: {len(replayed_results)}")
        return False

    success = True
    for i, (orig, repl) in enumerate(zip(original_results, replayed_results)):
        # Check symbolic identity
        if orig["lhs_postfix"] != repl["lhs_postfix"] or orig["rhs_postfix"] != repl["rhs_postfix"]:
            print(f"Index {i} symbolic mismatch.")
            print(f"   Original: {orig['lhs_postfix']} = {orig['rhs_postfix']}")
            print(f"   Replayed: {repl['lhs_postfix']} = {repl['rhs_postfix']}")
            success = False

        for key in ("complexity", "is_exact"):
            if key in orig and orig[key] != repl.get(key):
                print(f"Index {i} differs in {key}.")
                success = False
        for key in ("x_value", "error"):
            if key not in orig:
                continue
            same = (struct.pack(">d", orig[key]) == struct.pack(">d", repl[key])
                    if comparison == "exact" else math.isclose(orig[key], repl[key], rel_tol=0, abs_tol=abs_tolerance))
            if not same:
                print(f"Index {i} differs in {key}.")
                success = False

    return success


def main():
    # Paths and symbol descriptions can contain characters outside the console encoding.
    sys.stdout.reconfigure(errors="backslashreplace")
    args = parse_args()

    if not os.path.isfile(args.manifest):
        print(f"Error: Manifest file '{args.manifest}' not found.", file=sys.stderr)
        sys.exit(1)

    with open(args.manifest, encoding="utf-8") as f:
        try:
            manifest = json.load(f)
        except (json.JSONDecodeError, UnicodeDecodeError) as e:
            print(f"Error parsing manifest: {e}", file=sys.stderr)
            sys.exit(1)

    config = manifest.get("config") if isinstance(manifest, dict) else None
    if not isinstance(config, dict) or not {"target", "level"}.issubset(config):
        print("Invalid manifest: missing 'config' block.", file=sys.stderr)
        sys.exit(1)
    if not isinstance(manifest.get("results"), list):
        print("Invalid manifest: missing 'results' list.", file=sys.stderr)
        sys.exit(1)

    print(f"Replaying manifest generated on {manifest.get('timestamp', 'Unknown')}")
    print(f"Target: {config['target']}, Level: {config['level']}")
    print(f"Comparison: {args.comparison}" + (f", absolute tolerance={args.abs_tolerance:g}" if args.comparison == "tolerant" else ", identical f64 bit patterns"))
    if config.get("target_literal") is None:
        print("Original decimal spelling unavailable in this legacy manifest; only recorded numerical results can be compared.")

    # Create temporary file for the new manifest
    fd, temp_path = tempfile.mkstemp(suffix=".json")
    os.close(fd)
    profile_fd, profile_path = tempfile.mkstemp(suffix=".profile")
    os.close(profile_fd)

    try:
        try:
            cmd_args = build_command_args(config)
        except (ValueError, KeyError, TypeError) as e:
            print(f"Cannot replay manifest: {e}", file=sys.stderr)
            sys.exit(1)
        # The empty profile bypasses implicit home/current-directory profiles.
        base_cmd = [args.bin] if os.path.isfile(args.bin) else shlex.split(args.bin)
        full_cmd = base_cmd + cmd_args[1:] + ["--profile", profile_path,
                                             "--emit-manifest", temp_path, "--", cmd_args[0]]

        print(f"Executing: {' '.join(full_cmd)}")
        # Run the command and capture output so it doesn't pollute the terminal
        result = subprocess.run(
            full_cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            encoding="utf-8",
            errors="replace",
        )

        if result.returncode != 0:
            print("Execution failed!", file=sys.stderr)
            print(result.stderr, file=sys.stderr)
            sys.exit(1)

        # Parse the new manifest
        with open(temp_path, encoding="utf-8") as f:
            new_manifest = json.load(f)

        # Detect legacy settings that the compact summary cannot reproduce.
        keys = ("target", "target_literal", "level", "max_lhs_complexity", "max_rhs_complexity",
                "deterministic", "parallel", "max_error", "max_matches", "ranking_mode",
                "lhs_value_dedup", "matcher")
        for key in keys:
            if key in config and config[key] != new_manifest["config"].get(key):
                print(f"Cannot reproduce manifest setting: {key}", file=sys.stderr)
                sys.exit(1)

        if config.get("replay") is not None:
            expected = {k: v for k, v in config["replay"].items() if k != "cli_args"}
            actual = {k: v for k, v in new_manifest["config"]["replay"].items() if k != "cli_args"}
            if expected != actual:
                print("Cannot reproduce resolved replay settings", file=sys.stderr)
                sys.exit(1)

        is_parity = check_parity(
            manifest.get("results", []), 
            new_manifest.get("results", []),
            comparison=args.comparison, abs_tolerance=args.abs_tolerance,
        )

        if is_parity:
            print(f"Successfully reproduced {len(new_manifest['results'])} matches!")
            sys.exit(0)
        else:
            print("Reproduction failed. Outputs diverged.")
            sys.exit(1)

    finally:
        # Cleanup
        if os.path.exists(temp_path):
            os.remove(temp_path)
        os.remove(profile_path)


if __name__ == "__main__":
    main()
